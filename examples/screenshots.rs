//! README 用の画像を作る。実機の画面・設定・カーソルは一切読まず、作り物のデスクトップの上に
//! 本物のオーバーレイ描画（`draw_scene`）を重ねる。
//!
//! `cargo run --release --example screenshots -- --out docs/images`

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("macOS only");
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    mac::run()
}

#[cfg(target_os = "macos")]
mod mac {
    use std::fs::{self, File};
    use std::io::BufWriter;
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result, bail};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::AllocAnyThread;
    use objc2_app_kit::{
        NSAttributedStringNSStringDrawing, NSBezierPath, NSColor, NSFont, NSFontAttributeName,
        NSFontWeightBold, NSFontWeightMedium, NSFontWeightRegular,
        NSForegroundColorAttributeName, NSGradient, NSGradientDrawingOptions, NSGraphicsContext,
    };
    use objc2_foundation::{
        NSAttributedString, NSDictionary, NSMutableDictionary, NSPoint, NSRect, NSSize, NSString,
    };
    use unmouse::config::Config;
    use unmouse::controller::{Controller, Effect};
    use unmouse::geometry::{Point, Rect};
    use unmouse::keys::{Mods, parse_key};
    use unmouse::monitors::Monitor;
    use unmouse::nav::Nav;
    use unmouse::platform::macos::overlay::draw_scene;
    use unmouse::platform::macos::snapshot::Canvas;
    use unmouse::scene::Scene;

    const MONITOR_W: f64 = 1440.0;
    const MONITOR_H: f64 = 900.0;
    /// 最初のポインタの位置。
    const START: Point = Point::new(330.0, 250.0);
    /// ダイアログの `Save` ボタンの大きさ。
    const SAVE_W: f64 = 112.0;
    const SAVE_H: f64 = 36.0;

    pub fn run() -> Result<()> {
        let mut out = PathBuf::from("docs/images");
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--out" => out = args.next().context("--out にはディレクトリが要ります")?.into(),
                other => bail!("知らない引数: {other}"),
            }
        }
        fs::create_dir_all(&out).with_context(|| format!("{} を作れません", out.display()))?;

        let one = [monitor(0, 0.0)];
        let two = [monitor(0, 0.0), monitor(1, MONITOR_W)];

        // `Save` ボタンの中心がちょうどセルの中心になるように、ダイアログの位置を決める。
        let config = Config::default();
        let probe = Point::new(870.0, 610.0);
        let path = nav(&config, &one, probe).path_to(probe).context("path")?;
        let mut driver = Driver::new(&config, &one)?;
        driver.activate();
        for c in path.chars() {
            driver.key(c)?;
        }
        let target = driver.cursor;
        anyhow::ensure!(
            nav(&config, &one, target).path_to(target).as_deref() == Some(path.as_str()),
            "セルの中心が同じセルを指さない"
        );
        let dialog = Dialog::centered_on(target);

        // hero: 起動した直後。
        let stage = Stage::single(&one[0], 2.0);
        let mut d = Driver::new(&config, &one)?;
        d.activate();
        let canvas = stage.frame(&Frame::new(&config, d.scenes.as_deref(), Some(START), dialog));
        write_png(&out.join("hero.png"), &canvas)?;

        // narrow: 2文字打った後。
        for c in path.chars().take(2) {
            d.key(c)?;
        }
        let canvas = stage.frame(&Frame::new(&config, d.scenes.as_deref(), Some(START), dialog));
        write_png(&out.join("narrow.png"), &canvas)?;

        // refine: ラベルが決まった後の細分。
        let mut refine = Config::default();
        refine.grid.refine = true;
        let mut d = Driver::new(&refine, &one)?;
        d.activate();
        for c in path.chars() {
            d.key(c)?;
        }
        let canvas = stage.frame(&Frame::new(&refine, d.scenes.as_deref(), Some(d.cursor), dialog));
        write_png(&out.join("refine.png"), &canvas)?;

        // monitors: 2枚を1枚のキャンバスとして。
        let stage2 = Stage::pair(&two, 1.0);
        let mut d = Driver::new(&config, &two)?;
        d.activate();
        let mut f = Frame::new(&config, d.scenes.as_deref(), Some(Point::new(1900.0, 420.0)), dialog);
        f.second_screen = true;
        write_png(&out.join("monitors.png"), &stage2.frame(&f))?;

        demo(&out.join("demo.png"), &config, &one, &path, dialog)?;
        Ok(())
    }

    fn monitor(id: u32, x: f64) -> Monitor {
        Monitor { id, frame: Rect::new(x, 0.0, MONITOR_W, MONITOR_H), scale: 2.0 }
    }

    fn nav(config: &Config, monitors: &[Monitor], cursor: Point) -> Nav {
        Nav::new(monitors, config.grid.scope, cursor, config.max_cell())
            .with_keys(config.key_chars().expect("keys"))
            .with_subdivide(config.grid.subdivide)
    }

    /// `Controller` を実際のキー入力と同じ経路で動かして、表示中の Scene とポインタ位置を追う。
    struct Driver {
        controller: Controller,
        monitors: Vec<Monitor>,
        scenes: Option<Vec<Scene>>,
        cursor: Point,
    }

    impl Driver {
        fn new(config: &Config, monitors: &[Monitor]) -> Result<Self> {
            Ok(Self {
                controller: Controller::new(config.clone())?,
                monitors: monitors.to_vec(),
                scenes: None,
                cursor: START,
            })
        }

        fn activate(&mut self) {
            let (monitors, cursor) = (self.monitors.clone(), self.cursor);
            let effects = self.controller.activate(|| (monitors, cursor));
            self.apply(effects);
        }

        fn key(&mut self, c: char) -> Result<()> {
            let (monitors, cursor) = (self.monitors.clone(), self.cursor);
            let result = self.controller.on_key_down(parse_key(&c.to_string())?, Mods::NONE, || {
                (monitors, cursor)
            });
            self.apply(result.effects);
            Ok(())
        }

        fn apply(&mut self, effects: Vec<Effect>) {
            for effect in effects {
                match effect {
                    Effect::Show(scenes) | Effect::Update(scenes) => self.scenes = Some(scenes),
                    Effect::Hide => self.scenes = None,
                    Effect::MoveCursor { to, .. } => self.cursor = to,
                    Effect::Mouse(..) => {}
                }
            }
        }
    }

    // ---- 画面の並べ方 ----

    /// モニターを画像のどこに置くか。
    struct Stage {
        w: f64,
        h: f64,
        scale: f64,
        /// モニターごとの、画像の中での画面の位置。
        screens: Vec<Rect>,
        /// 画面の周りの枠を描くか。
        bezel: bool,
    }

    impl Stage {
        fn single(m: &Monitor, scale: f64) -> Self {
            Self {
                w: m.frame.w,
                h: m.frame.h,
                scale,
                screens: vec![Rect::new(0.0, 0.0, m.frame.w, m.frame.h)],
                bezel: false,
            }
        }

        fn pair(ms: &[Monitor], scale: f64) -> Self {
            let (margin, bezel, gap) = (28.0, 8.0, 6.0);
            let mut x = margin + bezel;
            let mut screens = Vec::new();
            for m in ms {
                screens.push(Rect::new(x, margin + bezel, m.frame.w, m.frame.h));
                x += m.frame.w + bezel * 2.0 + gap;
            }
            Self {
                w: x - bezel - gap + margin - bezel + bezel,
                h: ms[0].frame.h + (margin + bezel) * 2.0,
                scale,
                screens,
                bezel: true,
            }
        }

        fn frame(&self, f: &Frame) -> Canvas {
            let canvas = Canvas::new(self.w, self.h, self.scale);
            canvas.draw(|| {
                if self.bezel {
                    fill_rect(Rect::new(0.0, 0.0, self.w, self.h), rgb(0.90, 0.91, 0.94));
                    for s in &self.screens {
                        let b = Rect::new(s.x - 8.0, s.y - 8.0, s.w + 16.0, s.h + 16.0);
                        fill_round(b, 12.0, rgb(0.07, 0.07, 0.09));
                    }
                }
            });
            for (i, screen) in self.screens.iter().enumerate() {
                let rect = (screen.x, screen.y, screen.w, screen.h);
                canvas.draw_in(rect, || {
                    desktop(screen.w, screen.h, i, f);
                });
                if let Some(scenes) = f.scenes
                    && let Some(scene) = scenes.iter().find(|s| s.monitor == i)
                {
                    canvas.draw_in(rect, || draw_scene(scene, f.config));
                }
            }
            // ポインタと HUD は、オーバーレイの上。
            canvas.draw(|| {
                if let Some(p) = f.cursor {
                    let origin = |i: usize| i as f64 * MONITOR_W;
                    let i = if p.x >= MONITOR_W && self.screens.len() > 1 { 1 } else { 0 };
                    let s = self.screens[i];
                    pointer(s.x + p.x - origin(i), s.y + p.y);
                }
            });
            if let Some(caps) = f.hud {
                let s = self.screens[0];
                canvas.draw_in((s.x, s.y, s.w, s.h), || hud(s.w, s.h, caps));
            }
            canvas
        }
    }

    /// 1枚の絵に要るもの。
    struct Frame<'a> {
        config: &'a Config,
        scenes: Option<&'a [Scene]>,
        cursor: Option<Point>,
        dialog: Dialog,
        hud: Option<&'a [Cap]>,
        /// `Save` ボタンをポインタが乗った見た目にする。
        hover: bool,
        /// 2枚目のモニターに別のデスクトップを描く（`monitors.png`）。
        second_screen: bool,
    }

    impl<'a> Frame<'a> {
        fn new(
            config: &'a Config,
            scenes: Option<&'a [Scene]>,
            cursor: Option<Point>,
            dialog: Dialog,
        ) -> Self {
            Self {
                config,
                scenes,
                cursor,
                dialog,
                hud: None,
                hover: false,
                second_screen: false,
            }
        }
    }

    // ---- 描画の部品 ----

    fn rgb(r: f64, g: f64, b: f64) -> Retained<NSColor> {
        rgba(r, g, b, 1.0)
    }

    fn rgba(r: f64, g: f64, b: f64, a: f64) -> Retained<NSColor> {
        NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
    }

    fn ns(r: Rect) -> NSRect {
        NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
    }

    fn fill_rect(r: Rect, color: Retained<NSColor>) {
        color.setFill();
        NSBezierPath::fillRect(ns(r));
    }

    fn fill_round(r: Rect, radius: f64, color: Retained<NSColor>) {
        color.setFill();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(ns(r), radius, radius).fill();
    }

    fn stroke_round(r: Rect, radius: f64, width: f64, color: Retained<NSColor>) {
        color.setStroke();
        let path = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(ns(r), radius, radius);
        path.setLineWidth(width);
        path.stroke();
    }

    fn fill_circle(cx: f64, cy: f64, radius: f64, color: Retained<NSColor>) {
        color.setFill();
        let r = Rect::new(cx - radius, cy - radius, radius * 2.0, radius * 2.0);
        NSBezierPath::bezierPathWithOvalInRect(ns(r)).fill();
    }

    fn gradient(r: Rect, from: Retained<NSColor>, to: Retained<NSColor>, angle: f64) {
        if let Some(g) = NSGradient::initWithStartingColor_endingColor(NSGradient::alloc(), &from, &to) {
            g.drawInRect_angle(ns(r), angle);
        }
    }

    fn radial(cx: f64, cy: f64, radius: f64, from: Retained<NSColor>, to: Retained<NSColor>) {
        if let Some(g) = NSGradient::initWithStartingColor_endingColor(NSGradient::alloc(), &from, &to) {
            let c = NSPoint::new(cx, cy);
            g.drawFromCenter_radius_toCenter_radius_options(
                c,
                0.0,
                c,
                radius,
                NSGradientDrawingOptions::empty(),
            );
        }
    }

    #[derive(Clone, Copy)]
    enum Face {
        Ui(f64),
        Mono(f64),
    }

    fn font(face: Face, bold: bool, medium: bool) -> Retained<NSFont> {
        let weight = unsafe {
            if bold {
                NSFontWeightBold
            } else if medium {
                NSFontWeightMedium
            } else {
                NSFontWeightRegular
            }
        };
        match face {
            Face::Ui(size) => NSFont::systemFontOfSize_weight(size, weight),
            Face::Mono(size) => NSFont::monospacedSystemFontOfSize_weight(size, weight),
        }
    }

    fn attributed(text: &str, font: &NSFont, color: &NSColor) -> Retained<NSAttributedString> {
        let attrs: Retained<NSMutableDictionary<NSString, AnyObject>> = NSMutableDictionary::new();
        unsafe {
            attrs.setObject_forKey(font, ProtocolObject::from_ref(NSFontAttributeName));
            attrs.setObject_forKey(color, ProtocolObject::from_ref(NSForegroundColorAttributeName));
        }
        let dict: &NSDictionary<NSString, AnyObject> = &attrs;
        unsafe {
            NSAttributedString::initWithString_attributes(
                NSAttributedString::alloc(),
                &NSString::from_str(text),
                Some(dict),
            )
        }
    }

    /// `(x, y)` を左上にして描き、描いた幅を返す。
    fn text(s: &str, x: f64, y: f64, f: Face, bold: bool, color: Retained<NSColor>) -> f64 {
        let a = attributed(s, &font(f, bold, false), &color);
        a.drawAtPoint(NSPoint::new(x, y));
        a.size().width
    }

    fn text_width(s: &str, f: Face, bold: bool) -> f64 {
        attributed(s, &font(f, bold, false), &rgb(0.0, 0.0, 0.0)).size().width
    }

    /// 矩形の中央に描く。
    fn text_centered(s: &str, r: Rect, f: Face, bold: bool, color: Retained<NSColor>) {
        let a = attributed(s, &font(f, bold, false), &color);
        let size = a.size();
        a.drawAtPoint(NSPoint::new(
            r.x + (r.w - size.width) / 2.0,
            r.y + (r.h - size.height) / 2.0,
        ));
    }

    fn pointer(x: f64, y: f64) {
        let k = 1.3;
        let pts = [
            (0.0, 0.0),
            (0.0, 17.0),
            (4.2, 13.2),
            (6.9, 19.6),
            (9.7, 18.4),
            (7.0, 12.2),
            (12.4, 12.2),
        ];
        let path = NSBezierPath::bezierPath();
        for (i, (px, py)) in pts.iter().enumerate() {
            let p = NSPoint::new(x + px * k, y + py * k);
            if i == 0 {
                path.moveToPoint(p);
            } else {
                path.lineToPoint(p);
            }
        }
        path.closePath();
        path.setLineJoinStyle(objc2_app_kit::NSLineJoinStyle::Round);
        rgb(1.0, 1.0, 1.0).setStroke();
        path.setLineWidth(2.6);
        path.stroke();
        rgb(0.0, 0.0, 0.0).setFill();
        path.fill();
    }

    // ---- 作り物のデスクトップ ----

    /// ダイアログの位置。`Save` ボタンの左上を持つ。
    #[derive(Clone, Copy)]
    struct Dialog {
        save: Rect,
    }

    impl Dialog {
        fn centered_on(p: Point) -> Self {
            Self { save: Rect::new(p.x - SAVE_W / 2.0, p.y - SAVE_H / 2.0, SAVE_W, SAVE_H) }
        }

        /// ダイアログ全体（`Save` ボタンが右下に来る）。
        fn frame(&self) -> Rect {
            Rect::new(self.save.right() + 24.0 - 460.0, self.save.bottom() + 22.0 - 214.0, 460.0, 214.0)
        }
    }

    fn desktop(w: f64, h: f64, screen: usize, f: &Frame) {
        let full = Rect::new(0.0, 0.0, w, h);
        if screen == 0 {
            gradient(full, rgb(0.33, 0.38, 0.72), rgb(0.60, 0.78, 0.86), 90.0);
            radial(w * 0.82, h * 0.18, 520.0, rgba(0.98, 0.72, 0.78, 0.55), rgba(0.98, 0.72, 0.78, 0.0));
        } else {
            gradient(full, rgb(0.20, 0.45, 0.55), rgb(0.78, 0.86, 0.70), 90.0);
            radial(w * 0.2, h * 0.8, 520.0, rgba(0.95, 0.85, 0.55, 0.5), rgba(0.95, 0.85, 0.55, 0.0));
        }
        menu_bar(w, screen);
        if screen == 0 {
            notes_window(Rect::new(90.0, 80.0, 640.0, 470.0));
            code_window(Rect::new(560.0, 150.0, 780.0, 520.0));
            save_dialog(&f.dialog, f.hover);
        } else if f.second_screen {
            log_window(Rect::new(110.0, 120.0, 620.0, 400.0));
            chart_window(Rect::new(640.0, 300.0, 640.0, 440.0));
        }
    }

    fn menu_bar(w: f64, screen: usize) {
        fill_rect(Rect::new(0.0, 0.0, w, 28.0), rgba(1.0, 1.0, 1.0, 0.55));
        let ink = rgba(0.1, 0.1, 0.14, 0.9);
        let mut x = 18.0;
        for (i, item) in ["Editor", "File", "Edit", "View", "Window", "Help"].iter().enumerate() {
            x += text(item, x, 6.0, Face::Ui(13.0), i == 0, ink.clone()) + 20.0;
        }
        let clock = if screen == 0 { "Mon 9:41" } else { "Mon 9:41" };
        let cw = text_width(clock, Face::Ui(13.0), false);
        text(clock, w - cw - 18.0, 6.0, Face::Ui(13.0), false, ink.clone());
        fill_circle(w - cw - 44.0, 14.0, 5.0, rgba(0.1, 0.1, 0.14, 0.55));
        fill_circle(w - cw - 66.0, 14.0, 5.0, rgba(0.1, 0.1, 0.14, 0.55));
    }

    /// 影・角丸・タイトルバー・3つの丸を描いて、本文の矩形を返す。本文は `body` で塗る。
    fn window(r: Rect, title: &str, bar: Retained<NSColor>, title_color: Retained<NSColor>, body: impl FnOnce(Rect)) {
        for (grow, alpha) in [(14.0, 0.05), (9.0, 0.06), (5.0, 0.08), (2.0, 0.10)] {
            fill_round(
                Rect::new(r.x - grow, r.y - grow + 8.0, r.w + grow * 2.0, r.h + grow * 2.0),
                10.0 + grow,
                rgba(0.0, 0.0, 0.0, alpha),
            );
        }
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(ns(r), 10.0, 10.0).addClip();
        let bar_h = 38.0;
        fill_rect(Rect::new(r.x, r.y, r.w, bar_h), bar);
        let body_rect = Rect::new(r.x, r.y + bar_h, r.w, r.h - bar_h);
        body(body_rect);
        NSGraphicsContext::restoreGraphicsState_class();
        for (i, c) in [(1.0, 0.37, 0.34), (1.0, 0.74, 0.18), (0.16, 0.79, 0.25)].iter().enumerate() {
            fill_circle(r.x + 20.0 + i as f64 * 22.0, r.y + bar_h / 2.0, 6.5, rgb(c.0, c.1, c.2));
        }
        let tw = text_width(title, Face::Ui(13.0), true);
        text(title, r.x + (r.w - tw) / 2.0, r.y + 11.0, Face::Ui(13.0), true, title_color);
        stroke_round(r, 10.0, 1.0, rgba(0.0, 0.0, 0.0, 0.22));
    }

    fn notes_window(r: Rect) {
        window(r, "notes.md", rgb(0.90, 0.90, 0.92), rgba(0.15, 0.15, 0.2, 0.8), |b| {
            fill_rect(b, rgb(0.99, 0.99, 0.99));
            let ink = rgb(0.16, 0.17, 0.2);
            let soft = rgb(0.38, 0.40, 0.45);
            let (x, mut y) = (b.x + 32.0, b.y + 26.0);
            text("Weekly notes", x, y, Face::Ui(24.0), true, ink.clone());
            y += 48.0;
            let rows: [(&str, bool); 9] = [
                ("Monday", true),
                ("- Review the draft and send comments", false),
                ("- Book a room for Thursday", false),
                ("Tuesday", true),
                ("- Write the summary for the team", false),
                ("- Tidy up the shared folder", false),
                ("Wednesday", true),
                ("- Plan the next release", false),
                ("- Back up the project files", false),
            ];
            for (line, head) in rows {
                if head {
                    y += 6.0;
                    text(line, x, y, Face::Ui(15.0), true, ink.clone());
                } else {
                    text(line, x + 8.0, y, Face::Ui(15.0), false, soft.clone());
                }
                y += 26.0;
            }
        });
    }

    #[derive(Clone, Copy)]
    enum Tok {
        Plain,
        Key,
        Func,
        Str,
        Note,
    }

    fn tok_color(t: Tok) -> Retained<NSColor> {
        match t {
            Tok::Plain => rgb(0.84, 0.86, 0.90),
            Tok::Key => rgb(0.78, 0.56, 0.90),
            Tok::Func => rgb(0.45, 0.72, 0.98),
            Tok::Str => rgb(0.62, 0.84, 0.48),
            Tok::Note => rgb(0.45, 0.50, 0.58),
        }
    }

    fn code_window(r: Rect) {
        window(r, "main.rs", rgb(0.17, 0.18, 0.22), rgba(0.85, 0.87, 0.92, 0.85), |b| {
            fill_rect(b, rgb(0.12, 0.13, 0.16));
            use Tok::*;
            let lines: [&[(&str, Tok)]; 16] = [
                &[("// print what is left to do", Note)],
                &[("fn ", Key), ("main", Func), ("() {", Plain)],
                &[("    let ", Key), ("items = ", Plain), ("load", Func), ("(", Plain), ("\"items.txt\"", Str), (");", Plain)],
                &[("    for ", Key), ("item ", Plain), ("in ", Key), ("&items {", Plain)],
                &[("        if ", Key), ("item.done {", Plain)],
                &[("            continue", Key), (";", Plain)],
                &[("        }", Plain)],
                &[("        println!", Func), ("(", Plain), ("\"{}\"", Str), (", item.title);", Plain)],
                &[("    }", Plain)],
                &[("    save", Func), ("(&items);", Plain)],
                &[("}", Plain)],
                &[("", Plain)],
                &[("fn ", Key), ("load", Func), ("(name: &", Plain), ("str", Key), (") -> ", Plain), ("Vec", Func), ("<Item> {", Plain)],
                &[("    std::fs::read_to_string(name)", Plain)],
                &[("        .unwrap_or_default()", Plain)],
                &[("        .lines().map(Item::parse).collect()", Plain)],
            ];
            let mut y = b.y + 22.0;
            for (n, line) in lines.iter().enumerate() {
                text(&format!("{:>2}", n + 1), b.x + 16.0, y, Face::Mono(13.0), false, tok_color(Tok::Note));
                let mut x = b.x + 56.0;
                for (s, t) in line.iter() {
                    x += text(s, x, y, Face::Mono(13.0), false, tok_color(*t));
                }
                y += 21.0;
            }
        });
    }

    fn save_dialog(d: &Dialog, hover: bool) {
        let r = d.frame();
        window(r, "Untitled", rgb(0.93, 0.93, 0.95), rgba(0.15, 0.15, 0.2, 0.8), |b| {
            fill_rect(b, rgb(0.97, 0.97, 0.98));
            let ink = rgb(0.16, 0.17, 0.2);
            text("Save changes to notes.md?", b.x + 28.0, b.y + 26.0, Face::Ui(16.0), true, ink.clone());
            text(
                "Your edits will be lost if you close without saving.",
                b.x + 28.0,
                b.y + 54.0,
                Face::Ui(13.0),
                false,
                rgb(0.38, 0.40, 0.45),
            );
        });
        // 左から Don't Save / Cancel / Save。Save が右端。
        let s = d.save;
        let cancel = Rect::new(s.x - 12.0 - 90.0, s.y, 90.0, s.h);
        let dont = Rect::new(cancel.x - 12.0 - 120.0, s.y, 120.0, s.h);
        for (rect, label) in [(dont, "Don't Save"), (cancel, "Cancel")] {
            fill_round(rect, 8.0, rgb(1.0, 1.0, 1.0));
            stroke_round(rect, 8.0, 1.0, rgba(0.0, 0.0, 0.0, 0.22));
            text_centered(label, rect, Face::Ui(14.0), false, rgb(0.16, 0.17, 0.2));
        }
        let (top, bottom) = if hover {
            (rgb(0.28, 0.52, 0.98), rgb(0.12, 0.38, 0.88))
        } else {
            (rgb(0.22, 0.46, 0.95), rgb(0.10, 0.34, 0.82))
        };
        NSGraphicsContext::saveGraphicsState_class();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(ns(s), 8.0, 8.0).addClip();
        gradient(s, top, bottom, 90.0);
        NSGraphicsContext::restoreGraphicsState_class();
        text_centered("Save", s, Face::Ui(14.0), true, rgb(1.0, 1.0, 1.0));
    }

    fn log_window(r: Rect) {
        window(r, "output.log", rgb(0.90, 0.90, 0.92), rgba(0.15, 0.15, 0.2, 0.8), |b| {
            fill_rect(b, rgb(0.10, 0.11, 0.13));
            let rows = [
                ("09:38:02", "build started", Tok::Plain),
                ("09:38:09", "compiled 42 files", Tok::Str),
                ("09:38:10", "running 118 tests", Tok::Plain),
                ("09:38:14", "118 passed, 0 failed", Tok::Str),
                ("09:38:15", "packaging release", Tok::Plain),
                ("09:38:21", "done in 19s", Tok::Func),
            ];
            let mut y = b.y + 22.0;
            for (t, msg, tok) in rows {
                text(t, b.x + 20.0, y, Face::Mono(13.0), false, tok_color(Tok::Note));
                text(msg, b.x + 110.0, y, Face::Mono(13.0), false, tok_color(tok));
                y += 24.0;
            }
        });
    }

    fn chart_window(r: Rect) {
        window(r, "stats", rgb(0.93, 0.93, 0.95), rgba(0.15, 0.15, 0.2, 0.8), |b| {
            fill_rect(b, rgb(0.99, 0.99, 0.99));
            text("Items per day", b.x + 28.0, b.y + 24.0, Face::Ui(16.0), true, rgb(0.16, 0.17, 0.2));
            let heights = [0.35, 0.55, 0.45, 0.8, 0.62, 0.9, 0.5];
            let base = b.bottom() - 44.0;
            let top = b.y + 76.0;
            for (i, v) in heights.iter().enumerate() {
                let bar_h = (base - top) * v;
                fill_round(
                    Rect::new(b.x + 40.0 + i as f64 * 80.0, base - bar_h, 48.0, bar_h),
                    6.0,
                    rgb(0.30 + 0.05 * i as f64, 0.52, 0.90 - 0.04 * i as f64),
                );
            }
            fill_rect(Rect::new(b.x + 28.0, base + 1.0, b.w - 56.0, 1.5), rgba(0.0, 0.0, 0.0, 0.2));
        });
    }

    // ---- キーキャップの HUD ----

    #[derive(Clone)]
    struct Cap {
        /// キーに書く文字。
        label: String,
        /// 下に小さく添える文字。
        note: Option<String>,
        /// キーの後ろに付ける回数など。
        suffix: Option<String>,
    }

    impl Cap {
        fn key(c: char) -> Self {
            Self { label: c.to_string(), note: None, suffix: None }
        }

        fn double_tap() -> Self {
            Self {
                label: "\u{2318}".to_string(),
                note: Some("right".to_string()),
                suffix: Some("\u{00d7}2".to_string()),
            }
        }

        fn width(&self) -> f64 {
            if self.note.is_some() { 64.0 } else { 46.0 }
        }
    }

    fn hud(w: f64, h: f64, caps: &[Cap]) {
        if caps.is_empty() {
            return;
        }
        let (pad, gap, cap_h) = (14.0, 10.0, 46.0);
        let suffix_w = |c: &Cap| c.suffix.as_ref().map_or(0.0, |s| text_width(s, Face::Ui(18.0), true) + 8.0);
        let inner: f64 =
            caps.iter().map(|c| c.width() + suffix_w(c)).sum::<f64>() + gap * (caps.len() as f64 - 1.0);
        let panel = Rect::new(w - 32.0 - inner - pad * 2.0, h - 32.0 - cap_h - pad * 2.0, inner + pad * 2.0, cap_h + pad * 2.0);
        fill_round(panel, 18.0, rgba(0.05, 0.05, 0.08, 0.78));
        let mut x = panel.x + pad;
        let y = panel.y + pad;
        for c in caps {
            let r = Rect::new(x, y, c.width(), cap_h);
            fill_round(Rect::new(r.x, r.y + 3.0, r.w, r.h), 9.0, rgb(0.55, 0.57, 0.62));
            fill_round(Rect::new(r.x, r.y, r.w, r.h - 3.0), 9.0, rgb(0.97, 0.97, 0.98));
            let ink = rgb(0.14, 0.15, 0.18);
            match &c.note {
                Some(note) => {
                    text_centered(&c.label, Rect::new(r.x, r.y + 2.0, r.w, 24.0), Face::Ui(20.0), false, ink.clone());
                    text_centered(note, Rect::new(r.x, r.y + 24.0, r.w, 14.0), Face::Ui(11.0), false, rgb(0.4, 0.42, 0.47));
                }
                None => text_centered(&c.label, Rect::new(r.x, r.y, r.w, r.h - 3.0), Face::Mono(22.0), true, ink),
            }
            x += c.width();
            if let Some(s) = &c.suffix {
                text(s, x + 8.0, y + 12.0, Face::Ui(18.0), true, rgb(0.92, 0.93, 0.96));
                x += suffix_w(c);
            }
            x += gap;
        }
    }

    // ---- デモ（APNG） ----

    fn demo(path: &Path, config: &Config, monitors: &[Monitor], keys: &str, dialog: Dialog) -> Result<()> {
        let stage = Stage::single(&monitors[0], 1.0);
        let mut d = Driver::new(config, monitors)?;
        let mut frames: Vec<(Canvas, u32)> = Vec::new();
        let mut caps: Vec<Cap> = Vec::new();

        // 起動前: ポインタだけ。
        let shot = |d: &Driver, caps: &[Cap], cursor: Point, hover: bool, ms: u32, frames: &mut Vec<(Canvas, u32)>| {
            let mut f = Frame::new(config, d.scenes.as_deref(), Some(cursor), dialog);
            f.hud = Some(caps);
            f.hover = hover;
            frames.push((stage.frame(&f), ms));
        };
        shot(&d, &caps, START, false, 1400, &mut frames);

        // 右 Command を2回。
        d.activate();
        caps.push(Cap::double_tap());
        shot(&d, &caps, START, false, 900, &mut frames);

        let chars: Vec<char> = keys.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            d.key(*c)?;
            caps.push(Cap::key(*c));
            let last = i + 1 == chars.len();
            if last {
                anyhow::ensure!(d.scenes.is_none(), "最後のキーで閉じるはず");
                shot(&d, &caps, d.cursor, true, 2200, &mut frames);
            } else {
                shot(&d, &caps, START, false, 700, &mut frames);
            }
        }
        write_apng(path, &frames)
    }

    // ---- PNG ----

    /// 不透明な画像なので、アルファは書かない（RGB）。
    fn rgb_bytes(canvas: &Canvas) -> Vec<u8> {
        canvas.rgba().chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
    }

    fn encoder(path: &Path, canvas: &Canvas) -> Result<png::Encoder<'static, BufWriter<File>>> {
        let (w, h) = canvas.pixels();
        let file = File::create(path).with_context(|| format!("{} を作れません", path.display()))?;
        let mut e = png::Encoder::new(BufWriter::new(file), w, h);
        e.set_color(png::ColorType::Rgb);
        e.set_depth(png::BitDepth::Eight);
        e.set_compression(png::Compression::High);
        e.set_filter(png::Filter::Adaptive);
        Ok(e)
    }

    fn write_png(path: &Path, canvas: &Canvas) -> Result<()> {
        let mut writer = encoder(path, canvas)?.write_header()?;
        writer.write_image_data(&rgb_bytes(canvas))?;
        writer.finish()?;
        println!("{}", path.display());
        Ok(())
    }

    fn write_apng(path: &Path, frames: &[(Canvas, u32)]) -> Result<()> {
        let mut e = encoder(path, &frames[0].0)?;
        e.set_animated(frames.len() as u32, 0)?;
        let mut writer = e.write_header()?;
        for (canvas, ms) in frames {
            writer.set_frame_delay(*ms as u16, 1000)?;
            writer.write_image_data(&rgb_bytes(canvas))?;
        }
        writer.finish()?;
        println!("{}", path.display());
        Ok(())
    }
}
