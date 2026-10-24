//! NSApplication の起動と、Controller・Overlay・Mouse の結びつけ。

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDidChangeScreenParametersNotification,
};
use objc2_core_foundation::{CFRunLoop, kCFRunLoopDefaultMode};
use objc2_foundation::{NSNotificationCenter, NSObject};

use super::mouse::Mouse;
use super::overlay::Overlay;
use super::screens;
use super::tap::{DoubleTap, Tap};
use crate::config::Config;
use crate::controller::{Controller, Effect};
use crate::geometry::Point;
use crate::keys::Mods;
use crate::monitors::Monitor;
use crate::trigger::{DoubleTapDetector, Trigger};

/// 1回のキー処理の結果。
pub struct KeyOutcome {
    pub consume: bool,
    /// 今回の処理でカーソルを置いた（クリックした）位置。
    pub target: Option<Point>,
    /// キーを受けてから描画が終わるまで。
    pub elapsed: Duration,
}

pub struct Runtime {
    mtm: MainThreadMarker,
    config: Config,
    controller: Controller,
    overlay: Overlay,
    mouse: Mouse,
    /// 起動時に Nav へ渡したモニター。Show でパネルの位置を決めるのに使う。
    monitors: Vec<Monitor>,
}

impl Runtime {
    pub fn new(mtm: MainThreadMarker, config: Config) -> Result<Self> {
        Ok(Self {
            mtm,
            controller: Controller::new(config.clone())?,
            config,
            overlay: Overlay::new(mtm),
            mouse: Mouse::new().ok_or_else(|| anyhow!("CGEventSource を作れません"))?,
            monitors: Vec::new(),
        })
    }

    pub fn is_active(&self) -> bool {
        self.controller.is_active()
    }

    pub fn activate(&mut self) -> KeyOutcome {
        let start = Instant::now();
        let mtm = self.mtm;
        let mut snapshot = Vec::new();
        let effects = self.controller.activate(|| {
            snapshot = screens::monitors(mtm);
            (snapshot.clone(), screens::cursor())
        });
        self.monitors = snapshot;
        let target = self.apply(effects);
        KeyOutcome {
            consume: true,
            target,
            elapsed: start.elapsed(),
        }
    }

    /// 起動の合図（修飾キーの2回タップ）。待機中なら起動し、起動中なら閉じる（打ちかけならその範囲の中心へ動かす）。
    pub fn trigger(&mut self) {
        let mtm = self.mtm;
        let mut snapshot = None;
        let effects = self.controller.on_trigger(|| {
            let m = screens::monitors(mtm);
            snapshot = Some(m.clone());
            (m, screens::cursor())
        });
        if let Some(m) = snapshot {
            self.monitors = m;
        }
        self.apply(effects);
    }

    pub fn cancel_external(&mut self) {
        let effects = self.controller.cancel_external();
        self.apply(effects);
    }

    pub fn key_down(&mut self, keycode: u16, mods: Mods) -> KeyOutcome {
        let start = Instant::now();
        let mtm = self.mtm;
        let mut snapshot = None;
        let result = self.controller.on_key_down(keycode, mods, || {
            let m = screens::monitors(mtm);
            snapshot = Some(m.clone());
            (m, screens::cursor())
        });
        if let Some(m) = snapshot {
            self.monitors = m;
        }
        let target = self.apply(result.effects);
        KeyOutcome {
            consume: result.consume,
            target,
            elapsed: start.elapsed(),
        }
    }

    /// Effect を実行する。カーソルを置いた位置があれば返す。
    fn apply(&mut self, effects: Vec<Effect>) -> Option<Point> {
        let mut target = None;
        for effect in effects {
            match effect {
                Effect::Show(scenes) => {
                    let primary_h = screens::primary_height(self.mtm);
                    self.overlay
                        .show(&self.monitors, primary_h, &scenes, &self.config);
                }
                Effect::Update(scenes) => self.overlay.update(&scenes),
                Effect::Hide => self.overlay.hide(),
                Effect::MoveCursor { to, dragging } => {
                    self.mouse.move_to(to, dragging);
                    target = Some(to);
                }
                Effect::Mouse(op, at) => {
                    self.mouse.apply(op, at);
                    target = Some(at);
                }
            }
        }
        target
    }
}

pub fn init_app(mtm: MainThreadMarker) -> Retained<NSApplication> {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app
}

/// 描画やイベントが進むように、run loop を少し回す。
pub fn pump(duration: Duration) {
    CFRunLoop::run_in_mode(unsafe { kCFRunLoopDefaultMode }, duration.as_secs_f64(), false);
}

struct ObserverIvars {
    on_change: Box<dyn Fn()>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "UnmouseScreenObserver"]
    #[ivars = ObserverIvars]
    struct ScreenObserver;

    impl ScreenObserver {
        #[unsafe(method(screensChanged:))]
        fn screens_changed(&self, _note: &AnyObject) {
            (self.ivars().on_change)();
        }
    }
);

/// 常駐して、ホットキーで起動する。戻らない。
pub fn run(mtm: MainThreadMarker, config: Config, verbose: bool) -> Result<()> {
    let app = init_app(mtm);
    let trigger = config.trigger()?;
    let double_tap_ms = config.double_tap_ms;
    let hotkey_text = config.hotkey.clone();
    let runtime = Rc::new(RefCell::new(Runtime::new(mtm, config)?));

    let handler_runtime = Rc::clone(&runtime);
    let double_tap = match trigger {
        Trigger::DoubleTap(modifier) => {
            let trigger_runtime = Rc::clone(&runtime);
            Some(DoubleTap {
                detector: DoubleTapDetector::new(modifier, double_tap_ms),
                on_trigger: Box::new(move || trigger_runtime.borrow_mut().trigger()),
            })
        }
        Trigger::Chord(_) => None,
    };
    let tap = Tap::new(
        Box::new(move |code, mods| handler_runtime.borrow_mut().key_down(code, mods).consume),
        double_tap,
        verbose,
    );
    eprintln!("event tap: {}", if tap.is_some() { "ok" } else { "failed" });
    let tap = tap.context(
        "event tap を作れません（Accessibility と入力監視の許可が、この実行ファイルを起動したアプリに必要です）",
    )?;
    match trigger {
        Trigger::DoubleTap(m) => eprintln!("trigger: double {} ({double_tap_ms}ms)", m.name()),
        Trigger::Chord(_) => eprintln!("trigger: chord {hotkey_text}"),
    }

    // モニター構成が変わったら、表示中の操作を取り消す。
    let observer_runtime = Rc::clone(&runtime);
    let observer: Retained<ScreenObserver> = {
        let this = mtm.alloc::<ScreenObserver>().set_ivars(ObserverIvars {
            on_change: Box::new(move || {
                if let Ok(mut r) = observer_runtime.try_borrow_mut() {
                    r.cancel_external();
                }
            }),
        });
        unsafe { msg_send![super(this), init] }
    };
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &observer,
            sel!(screensChanged:),
            Some(NSApplicationDidChangeScreenParametersNotification),
            None,
        );
    }

    app.run();
    drop(tap);
    drop(observer);
    Ok(())
}
