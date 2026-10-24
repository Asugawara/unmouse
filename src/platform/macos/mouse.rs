//! CGEvent でマウスを操作する。

use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventSource, CGEventSourceStateID, CGEventTapLocation, CGEventType,
    CGMouseButton,
};
use objc2_core_foundation::CGPoint;

use crate::controller::{Button, MouseOp};
use crate::geometry::Point;

pub struct Mouse {
    source: objc2_core_foundation::CFRetained<CGEventSource>,
}

fn pt(p: Point) -> CGPoint {
    CGPoint { x: p.x, y: p.y }
}

impl Mouse {
    pub fn new() -> Option<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)?;
        CGEventSource::set_local_events_suppression_interval(Some(&source), 0.0);
        Some(Self { source })
    }

    fn post(&self, kind: CGEventType, at: Point, button: CGMouseButton, click_state: i64) {
        if let Some(event) = CGEvent::new_mouse_event(Some(&self.source), kind, pt(at), button) {
            if click_state > 0 {
                CGEvent::set_integer_value_field(
                    Some(&event),
                    CGEventField::MouseEventClickState,
                    click_state,
                );
            }
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
    }

    /// カーソルを動かす。ドラッグ中は LeftMouseDragged で送る。
    pub fn move_to(&self, to: Point, dragging: bool) {
        let kind = if dragging {
            CGEventType::LeftMouseDragged
        } else {
            CGEventType::MouseMoved
        };
        self.post(kind, to, CGMouseButton::Left, 0);
    }

    pub fn apply(&self, op: MouseOp, at: Point) {
        match op {
            MouseOp::Click { button, count } => {
                let (down, up, b) = kinds(button);
                self.move_to(at, false);
                for n in 1..=count.max(1) {
                    self.post(down, at, b, n as i64);
                    self.post(up, at, b, n as i64);
                }
            }
            MouseOp::Down(button) => {
                let (down, _, b) = kinds(button);
                self.post(down, at, b, 1);
            }
            MouseOp::Up(button) => {
                let (_, up, b) = kinds(button);
                self.post(up, at, b, 1);
            }
        }
    }
}

fn kinds(button: Button) -> (CGEventType, CGEventType, CGMouseButton) {
    match button {
        Button::Left => (
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGMouseButton::Left,
        ),
        Button::Right => (
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGMouseButton::Right,
        ),
        Button::Middle => (
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
            CGMouseButton::Center,
        ),
    }
}
