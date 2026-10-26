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

/// `--verbose` の keyDown の 1 行。通した keyDown はキーの内容を出さない（パスワードの入力がログに残るため）。
fn key_down_line(t: u64, code: u16, flags: u64, consumed: bool) -> String {
    if consumed {
        format!("t={t} keyDown code=0x{code:02x} flags=0x{flags:x} fired=true")
    } else {
        format!("t={t} keyDown passthrough")
    }
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

/// マウスの操作にあたるイベント。来たら2回タップの途中を捨てるが、飲み込まない。
const MOUSE_KINDS: [CGEventType; 11] = [
    CGEventType::MouseMoved,
    CGEventType::LeftMouseDown,
    CGEventType::LeftMouseUp,
    CGEventType::RightMouseDown,
    CGEventType::RightMouseUp,
    CGEventType::OtherMouseDown,
    CGEventType::OtherMouseUp,
    CGEventType::LeftMouseDragged,
    CGEventType::RightMouseDragged,
    CGEventType::OtherMouseDragged,
    CGEventType::ScrollWheel,
];

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
    if MOUSE_KINDS.contains(&kind) {
        // マウスを動かした・押した・スクロールした。飲み込まない。
        // ログにも出さない（多すぎるため）。
        if let Some(dt) = state.double_tap.as_mut() {
            dt.detector.on_other_key();
        }
        return ev;
    }
    let event_ref = unsafe { event.as_ref() };
    let code = CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventKeycode) as u16;
    if kind == CGEventType::FlagsChanged {
        // 飲み込まない（Cmd を使うショートカットを壊さないため）。
        let flags = CGEvent::flags(Some(event_ref)).0;
        let t = now_ms();
        let fired = state.flags_changed(code, flags, t);
        if state.verbose {
            eprintln!("t={t} flagsChanged code=0x{code:02x} flags=0x{flags:x} fired={fired}");
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
            let flags = CGEvent::flags(Some(event_ref)).0;
            eprintln!("{}", key_down_line(now_ms(), code, flags, consumed));
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
            | (1 << CGEventType::FlagsChanged.0)
            | MOUSE_KINDS.iter().fold(0u64, |m, k| m | (1u64 << k.0));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_down_line_shows_the_key_only_when_consumed() {
        assert_eq!(
            key_down_line(12, 0x25, 0x100, true),
            "t=12 keyDown code=0x25 flags=0x100 fired=true"
        );
    }

    #[test]
    fn key_down_line_hides_the_key_when_passed_through() {
        let line = key_down_line(34, 0x25, 0x100, false);
        assert_eq!(line, "t=34 keyDown passthrough");
        assert!(!line.contains("code="));
        assert!(!line.contains("flags="));
    }

    #[test]
    fn mods_from_flags_maps_each_modifier() {
        assert_eq!(mods_from_flags(CGEventFlags(0)), Mods::NONE);
        assert_eq!(mods_from_flags(CGEventFlags::MaskControl), Mods::CTRL);
        assert_eq!(mods_from_flags(CGEventFlags::MaskAlternate), Mods::ALT);
        assert_eq!(mods_from_flags(CGEventFlags::MaskShift), Mods::SHIFT);
        assert_eq!(mods_from_flags(CGEventFlags::MaskCommand), Mods::CMD);
    }

    #[test]
    fn mods_from_flags_combines_and_ignores_other_bits() {
        let ctrl_alt = CGEventFlags(CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0);
        assert_eq!(mods_from_flags(ctrl_alt), Mods::CTRL | Mods::ALT);
        let all = CGEventFlags(
            CGEventFlags::MaskControl.0
                | CGEventFlags::MaskAlternate.0
                | CGEventFlags::MaskShift.0
                | CGEventFlags::MaskCommand.0,
        );
        assert_eq!(
            mods_from_flags(all),
            Mods::CTRL | Mods::ALT | Mods::SHIFT | Mods::CMD
        );
        // CapsLock・NumericPad・左右別のビットは修飾キーの組に入らない
        let noise = CGEventFlags(
            CGEventFlags::MaskAlphaShift.0 | CGEventFlags::MaskNumericPad.0 | 0x10,
        );
        assert_eq!(mods_from_flags(noise), Mods::NONE);
        let cmd_with_side = CGEventFlags(CGEventFlags::MaskCommand.0 | 0x10);
        assert_eq!(mods_from_flags(cmd_with_side), Mods::CMD);
    }
}
