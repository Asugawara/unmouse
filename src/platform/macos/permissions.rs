//! Accessibility 権限と Secure Input の確認。

use std::ffi::c_void;

use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: &'static CFString;
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
}

/// Accessibility が許可されているか。`prompt` ならシステムの許可ダイアログを出す。
pub fn accessibility_trusted(prompt: bool) -> bool {
    if !prompt {
        return unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) };
    }
    let value = if prompt { CFBoolean::new(true) } else { CFBoolean::new(false) };
    let key = unsafe { kAXTrustedCheckOptionPrompt };
    let dict = CFDictionary::from_slices(&[key], &[value]);
    unsafe { AXIsProcessTrustedWithOptions(&*dict as *const _ as *const c_void) }
}

pub fn secure_input_enabled() -> bool {
    unsafe { IsSecureEventInputEnabled() }
}
