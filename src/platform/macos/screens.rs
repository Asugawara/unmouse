//! NSScreen からモニターの配置を作る。座標は CG のグローバル座標（左上原点）。

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_core_graphics::CGEvent;

use crate::geometry::{Point, Rect};
use crate::monitors::Monitor;

/// `screens()[0]` の高さ。NS と CG の座標変換の基準になる（`mainScreen` ではない）。
pub fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .iter()
        .next()
        .map(|s| s.frame().size.height)
        .unwrap_or(0.0)
}

pub fn monitors(mtm: MainThreadMarker) -> Vec<Monitor> {
    let screens = NSScreen::screens(mtm);
    let primary_h = primary_height(mtm);
    screens
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let f = s.frame();
            let ns = Rect::new(f.origin.x, f.origin.y, f.size.width, f.size.height);
            Monitor {
                id: i as u32,
                frame: crate::geometry::ns_to_cg(ns, primary_h),
                scale: s.backingScaleFactor(),
            }
        })
        .collect()
}

/// 今のカーソル位置（CG のグローバル座標）。
pub fn cursor() -> Point {
    let p = CGEvent::location(CGEvent::new(None).as_deref());
    Point::new(p.x, p.y)
}
