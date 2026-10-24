//! CGEventTap でキー入力を受ける。

use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::time::Instant;

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use crate::keys::Mods;
use crate::trigger::{DoubleTapDetector, Modifier};

/// keyDown を処理する側。`true` なら飲み込む。
pub type Handler = Box<dyn FnMut(u16, Mods) -> bool>;

/// 修飾キーの2回タップを受けたときに呼ぶ。
pub type TriggerHandler = Box<dyn FnMut()>;

/// 修飾キーの2回タップによる起動。
pub struct DoubleTap {
    pub detector: DoubleTapDetector,
    pub on_trigger: TriggerHandler,
}

struct State {
    handler: Handler,
    double_tap: Option<DoubleTap>,
    verbose: bool,
    /// 飲み込んだ keyDown のキーコード。対応する keyUp も飲み込む。
    swallowed: HashSet<u16>,
    port: Option<CFRetained<CFMachPort>>,
}

pub struct Tap {
    _port: CFRetained<CFMachPort>,
    _state: Box<State>,
}

pub fn mods_from_flags(flags: CGEventFlags) -> Mods {
    let mut m = Mods::NONE;
    if flags.contains(CGEventFlags::MaskControl) {
        m = m | Mods::CTRL;
    }
    if flags.contains(CGEventFlags::MaskAlternate) {
        m = m | Mods::ALT;
    }
    if flags.contains(CGEventFlags::MaskShift) {
        m = m | Mods::SHIFT;
    }
    if flags.contains(CGEventFlags::MaskCommand) {
        m = m | Mods::CMD;
    }
    m
}

/// 起動からの経過時間（ms）。CGEvent の timestamp は Apple Silicon で
/// mach_absolute_time の単位になり ns と一致しないため、受けた時刻をこちらで測る。
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

impl State {
    /// flagsChanged を検出器に渡す。2回タップが完成したら起動して `true`。
    fn flags_changed(&mut self, code: u16, flags: u64, t_ms: u64) -> bool {
        let Some(dt) = self.double_tap.as_mut() else {
            return false;
        };
        let pressed = Modifier::from_keycode(code).is_none_or(|m| m.is_down(flags));
        if dt.detector.on_modifier(code, pressed, t_ms) {
            (dt.on_trigger)();
            return true;
        }
        false
    }
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    let state = unsafe { &mut *(user_info as *mut State) };
    let ev = event.as_ptr();
    if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
        if let Some(port) = &state.port {
            CGEvent::tap_enable(port, true);
        }
        return ev;
    }
    let event_ref = unsafe { event.as_ref() };
    let code = CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventKeycode) as u16;
    if kind == CGEventType::FlagsChanged {
        // 飲み込まない（Cmd を使うショートカットを壊さないため）。
        let flags = CGEvent::flags(Some(event_ref)).0;
        let fired = state.flags_changed(code, flags, now_ms());
        if state.verbose {
            eprintln!("flagsChanged code=0x{code:02x} flags=0x{flags:x} fired={fired}");
        }
        return ev;
    }
    if kind == CGEventType::KeyDown {
        let repeat =
            CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventAutorepeat) != 0;
        let mods = mods_from_flags(CGEvent::flags(Some(event_ref)));
        if repeat {
            // 起動中のオートリピートは捨てる。
            return if state.swallowed.contains(&code) {
                std::ptr::null_mut()
            } else {
                ev
            };
        }
        let consumed = (state.handler)(code, mods);
        if let Some(dt) = state.double_tap.as_mut() {
            dt.detector.on_other_key();
        }
        if state.verbose {
            eprintln!(
                "keyDown code=0x{code:02x} flags=0x{:x} fired={consumed}",
                CGEvent::flags(Some(event_ref)).0
            );
        }
        if consumed {
            state.swallowed.insert(code);
            return std::ptr::null_mut();
        }
        ev
    } else if kind == CGEventType::KeyUp {
        if state.swallowed.remove(&code) {
            std::ptr::null_mut()
        } else {
            ev
        }
    } else {
        ev
    }
}

impl Tap {
    /// tap を作ってメイン run loop に載せる。作れなければ（権限なし等）`None`。
    pub fn new(handler: Handler, double_tap: Option<DoubleTap>, verbose: bool) -> Option<Self> {
        let mask: u64 = (1 << CGEventType::KeyDown.0)
            | (1 << CGEventType::KeyUp.0)
            | (1 << CGEventType::FlagsChanged.0);
        let mut state = Box::new(State {
            handler,
            double_tap,
            verbose,
            swallowed: HashSet::new(),
            port: None,
        });
        let info = &mut *state as *mut State as *mut c_void;
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(callback),
                info,
            )
        }?;
        state.port = Some(port.clone());
        let source = CFMachPort::new_run_loop_source(None, Some(&port), 0)?;
        CFRunLoop::main()?.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        CGEvent::tap_enable(&port, true);
        Some(Self {
            _port: port,
            _state: state,
        })
    }
}

/// tap が作れるかだけを確かめる（listen only で作ってすぐ捨てる）。
pub fn can_create() -> bool {
    unsafe extern "C-unwind" fn noop(
        _: CGEventTapProxy,
        _: CGEventType,
        e: NonNull<CGEvent>,
        _: *mut c_void,
    ) -> *mut CGEvent {
        e.as_ptr()
    }
    unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            1 << CGEventType::KeyDown.0,
            Some(noop),
            std::ptr::null_mut(),
        )
    }
    .is_some()
}
