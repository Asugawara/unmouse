//! モニターごとの NSPanel に Scene を描く。

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSBackingStoreType, NSBezierPath, NSColor, NSFont,
    NSFontAttributeName, NSFontWeightBold, NSFontWeightRegular, NSForegroundColorAttributeName,
    NSPanel, NSScreenSaverWindowLevel, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{
    NSAttributedString, NSDictionary, NSMutableDictionary, NSPoint, NSRect, NSSize, NSString,
};

use crate::config::Config;
use crate::geometry::{Rect, cg_to_ns};
use crate::monitors::Monitor;
use crate::scene::{Anchor, Scene};

static LAST_DRAW_NANOS: AtomicU64 = AtomicU64::new(0);

/// 直近の `drawRect` の所要時間。
pub fn last_draw() -> Duration {
    Duration::from_nanos(LAST_DRAW_NANOS.load(Ordering::Relaxed))
}

struct Ivars {
    scene: RefCell<Option<Scene>>,
    config: Config,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "UnmouseSceneView"]
    #[ivars = Ivars]
    struct SceneView;

    impl SceneView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let ivars = self.ivars();
            if let Some(scene) = ivars.scene.borrow().as_ref() {
                let start = Instant::now();
                draw(scene, &ivars.config);
                LAST_DRAW_NANOS.store(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }
        }
    }
);

impl SceneView {
    fn new(mtm: MainThreadMarker, frame: NSRect, config: Config) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(Ivars {
            scene: RefCell::new(None),
            config,
        });
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

define_class!(
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "UnmouseOverlayPanel"]
    struct OverlayPanel;

    impl OverlayPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            false
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool {
            false
        }
    }
);

struct Surface {
    monitor: usize,
    panel: Retained<OverlayPanel>,
    view: Retained<SceneView>,
}

pub struct Overlay {
    mtm: MainThreadMarker,
    primary_h: f64,
    surfaces: Vec<Surface>,
}

impl Overlay {
    pub fn new(mtm: MainThreadMarker) -> Self {
        Self {
            mtm,
            primary_h: 0.0,
            surfaces: Vec::new(),
        }
    }

    /// Scene のあるモニターごとにパネルを1枚作って出す。
    pub fn show(&mut self, monitors: &[Monitor], primary_h: f64, scenes: &[Scene], config: &Config) {
        self.hide();
        self.primary_h = primary_h;
        for scene in scenes {
            let Some(monitor) = monitors.get(scene.monitor) else {
                continue;
            };
            let ns = cg_to_ns(monitor.frame, primary_h);
            let frame = NSRect::new(NSPoint::new(ns.x, ns.y), NSSize::new(ns.w, ns.h));
            let panel = make_panel(self.mtm, frame);
            let view = SceneView::new(
                self.mtm,
                NSRect::new(NSPoint::new(0.0, 0.0), frame.size),
                config.clone(),
            );
            panel.setContentView(Some(&view));
            panel.orderFrontRegardless();
            self.surfaces.push(Surface {
                monitor: scene.monitor,
                panel,
                view,
            });
        }
        self.update(scenes);
    }

    pub fn update(&mut self, scenes: &[Scene]) {
        for surface in &self.surfaces {
            let scene = scenes.iter().find(|s| s.monitor == surface.monitor).cloned();
            *surface.view.ivars().scene.borrow_mut() = scene;
            surface.view.setNeedsDisplay(true);
            // 次の run loop を待たずに描く。
            surface.view.display();
            surface.panel.display();
        }
    }

    pub fn hide(&mut self) {
        for surface in self.surfaces.drain(..) {
            surface.panel.orderOut(None);
            surface.panel.close();
        }
    }

    pub fn is_shown(&self) -> bool {
        !self.surfaces.is_empty()
    }
}

fn make_panel(mtm: MainThreadMarker, frame: NSRect) -> Retained<OverlayPanel> {
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let panel: Retained<OverlayPanel> = unsafe {
        msg_send![
            mtm.alloc::<OverlayPanel>(),
            initWithContentRect: frame,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    unsafe { panel.setReleasedWhenClosed(false) };
    // 最大のレベル（CGShieldingWindowLevel 相当より上）。
    panel.setLevel(NSScreenSaverWindowLevel.max(i32::MAX as isize - 1));
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    panel.setIgnoresMouseEvents(true);
    panel.setOpaque(false);
    panel.setHasShadow(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHidesOnDeactivate(false);
    panel
}

fn rgba(r: f64, g: f64, b: f64, a: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
}

fn nsrect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}

fn font(name: &str, size: f64, bold: bool) -> Retained<NSFont> {
    if !name.is_empty()
        && let Some(f) = NSFont::fontWithName_size(&NSString::from_str(name), size)
    {
        return f;
    }
    let weight = unsafe { if bold { NSFontWeightBold } else { NSFontWeightRegular } };
    NSFont::monospacedSystemFontOfSize_weight(size, weight)
}

fn attributed(text: &str, font: &NSFont, color: &NSColor) -> Retained<NSAttributedString> {
    let attrs: Retained<NSMutableDictionary<NSString, AnyObject>> = NSMutableDictionary::new();
    unsafe {
        attrs.setObject_forKey(font, objc2::runtime::ProtocolObject::from_ref(NSFontAttributeName));
        attrs.setObject_forKey(
            color,
            objc2::runtime::ProtocolObject::from_ref(NSForegroundColorAttributeName),
        );
    }
    let dict: &NSDictionary<NSString, AnyObject> = &attrs;
    unsafe { NSAttributedString::initWithString_attributes(NSAttributedString::alloc(), &NSString::from_str(text), Some(dict)) }
}

fn draw(scene: &Scene, config: &Config) {
    let style = &config.style;
    let (ar, ag, ab) = config.accent_rgb().unwrap_or((0x1d, 0x4e, 0xd8));
    let (ar, ag, ab) = (ar as f64 / 255.0, ag as f64 / 255.0, ab as f64 / 255.0);

    rgba(0.0, 0.0, 0.0, style.dim_opacity).setFill();
    for r in &scene.dims {
        NSBezierPath::fillRect(nsrect(*r));
    }

    rgba(0.0, 0.0, 0.0, 0.16).setStroke();
    for l in &scene.faint_lines {
        NSBezierPath::strokeLineFromPoint_toPoint(
            NSPoint::new(l.from.x, l.from.y),
            NSPoint::new(l.to.x, l.to.y),
        );
    }
    rgba(ar, ag, ab, 0.55).setStroke();
    for l in &scene.strong_lines {
        NSBezierPath::strokeLineFromPoint_toPoint(
            NSPoint::new(l.from.x, l.from.y),
            NSPoint::new(l.to.x, l.to.y),
        );
    }

    let strong_font = font(&style.font, scene.label_size, true);
    let rest_font = font(&style.font, scene.label_size, false);
    let strong_color = rgba(ar * 0.6, ag * 0.6, ab * 0.9, 1.0);
    let ink = rgba(0.0, 0.0, 0.0, 1.0);
    let rest_color = rgba(0.0, 0.0, 0.0, style.rest_opacity);
    let bg = rgba(1.0, 1.0, 1.0, style.label_bg_opacity);
    let pad = 1.5;

    for label in &scene.labels {
        let first = attributed(
            &label.strong.to_string(),
            &strong_font,
            if label.next { &strong_color } else { &ink },
        );
        let first_size = first.size();
        let rest = (!label.rest.is_empty()).then(|| attributed(&label.rest, &rest_font, &rest_color));
        let rest_w = rest.as_ref().map_or(0.0, |r| r.size().width);
        let (w, h) = (first_size.width + rest_w + pad * 2.0, first_size.height + pad * 2.0);
        let (x, y) = match label.anchor {
            Anchor::TopLeft => (label.origin.x, label.origin.y),
            Anchor::TopRight => (label.origin.x - w, label.origin.y),
            Anchor::BottomLeft => (label.origin.x, label.origin.y - h),
            Anchor::BottomRight => (label.origin.x - w, label.origin.y - h),
        };
        bg.setFill();
        NSBezierPath::fillRect(NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)));
        first.drawAtPoint(NSPoint::new(x + pad, y + pad));
        if let Some(rest) = rest {
            rest.drawAtPoint(NSPoint::new(x + pad + first_size.width, y + pad));
        }
    }

    let edge_strong = font(&style.font, style.edge_key_size, true);
    for key in &scene.edge_keys {
        let color = if key.next {
            rgba(ar * 0.6, ag * 0.6, ab * 0.9, 1.0)
        } else {
            rgba(ar * 0.6, ag * 0.6, ab * 0.9, 0.42)
        };
        let text = attributed(&key.key.to_string(), &edge_strong, &color);
        let size = text.size();
        text.drawAtPoint(NSPoint::new(
            key.center.x - size.width / 2.0,
            key.center.y - size.height / 2.0,
        ));
    }

    if let Some(m) = scene.marker {
        let r = 6.0;
        let circle = NSRect::new(NSPoint::new(m.x - r, m.y - r), NSSize::new(r * 2.0, r * 2.0));
        rgba(1.0, 0.1, 0.1, 0.9).setFill();
        NSBezierPath::bezierPathWithOvalInRect(circle).fill();
        rgba(1.0, 1.0, 1.0, 1.0).setStroke();
        NSBezierPath::bezierPathWithOvalInRect(circle).stroke();
    }
}
