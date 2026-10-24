//! CGEventTap でキー入力を受ける。

use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use crate::keys::Mods;

/// keyDown を処理する側。`true` なら飲み込む。
pub type Handler = Box<dyn FnMut(u16, Mods) -> bool>;

struct State {
    handler: Handler,
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
        if (state.handler)(code, mods) {
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
    pub fn new(handler: Handler) -> Option<Self> {
        let mask: u64 = (1 << CGEventType::KeyDown.0)
            | (1 << CGEventType::KeyUp.0)
            | (1 << CGEventType::FlagsChanged.0);
        let mut state = Box::new(State {
            handler,
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
