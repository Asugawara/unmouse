//! 画面に出さずにビットマップへ描く。README の画像を作るために、AppKit の描画をそのまま使う。

use objc2::rc::Retained;
use objc2_app_kit::NSGraphicsContext;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextGetData, CGColorSpace, CGContext, CGImageAlphaInfo,
    kCGColorSpaceSRGB,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// 幅 `w`、高さ `h` ポイントの描画面。`scale` 倍のピクセルで持つ。
/// 座標は左上原点で y は下向き（`NSView` の `isFlipped` と同じ）。
pub struct Canvas {
    cg: CFRetained<CGContext>,
    ns: Retained<NSGraphicsContext>,
    width: usize,
    height: usize,
}

impl Canvas {
    pub fn new(w: f64, h: f64, scale: f64) -> Self {
        let width = (w * scale).round() as usize;
        let height = (h * scale).round() as usize;
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
            .expect("sRGB color space");
        let cg = unsafe {
            CGBitmapContextCreate(
                std::ptr::null_mut(),
                width,
                height,
                8,
                width * 4,
                Some(&space),
                CGImageAlphaInfo::PremultipliedLast.0,
            )
        }
        .expect("bitmap context");
        // 左上原点・y 下向きにする。
        CGContext::translate_ctm(Some(&cg), 0.0, height as f64);
        CGContext::scale_ctm(Some(&cg), scale, -scale);
        CGContext::set_allows_font_smoothing(Some(&cg), false);
        let ns = NSGraphicsContext::graphicsContextWithCGContext_flipped(&cg, true);
        Self { cg, ns, width, height }
    }

    /// ピクセル単位の大きさ。
    pub fn pixels(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    /// `f` の間、この面を AppKit の現在の描画先にする。
    pub fn draw(&self, f: impl FnOnce()) {
        let previous = NSGraphicsContext::currentContext();
        NSGraphicsContext::setCurrentContext(Some(&self.ns));
        f();
        NSGraphicsContext::setCurrentContext(previous.as_deref());
    }

    /// 矩形 `(x, y, w, h)` の左上を原点にして、その外へはみ出さないように `f` を描く。
    pub fn draw_in(&self, rect: (f64, f64, f64, f64), f: impl FnOnce()) {
        let (x, y, w, h) = rect;
        CGContext::save_g_state(Some(&self.cg));
        CGContext::clip_to_rect(
            Some(&self.cg),
            NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)),
        );
        CGContext::translate_ctm(Some(&self.cg), x, y);
        self.draw(f);
        CGContext::restore_g_state(Some(&self.cg));
    }

    /// 全ピクセルを RGBA（ストレート）で。上の行から。
    pub fn rgba(&self) -> Vec<u8> {
        let len = self.width * self.height * 4;
        let data = CGBitmapContextGetData(Some(&self.cg)) as *const u8;
        assert!(!data.is_null());
        let mut out = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
        for px in out.as_chunks_mut::<4>().0 {
            let a = px[3] as u32;
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        out
    }
}
