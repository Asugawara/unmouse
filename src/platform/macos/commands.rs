//! CLI のサブコマンド。

use std::time::Duration;

use anyhow::{Context, Result, bail};
use objc2::MainThreadMarker;

use super::app::{self, Runtime};
use super::{overlay, permissions, screens, tap};
use crate::config::Config;
use crate::controller::Controller;
use crate::geometry::Point;
use crate::keys::{Action, Command, KeyMap, Mods, parse_key};
use crate::monitors::{Monitor, Scope};
use crate::nav::Nav;

fn main_thread() -> Result<MainThreadMarker> {
    MainThreadMarker::new().context("メインスレッドで実行してください")
}

fn nav_for(config: &Config, monitors: &[Monitor], cursor: Point) -> Result<Nav> {
    Ok(Nav::new(monitors, config.grid.scope, cursor, config.max_cell()).with_keys(config.key_chars()?))
}

pub fn run() -> Result<()> {
    let mtm = main_thread()?;
    let config = Config::load()?;
    if !permissions::accessibility_trusted(true) {
        bail!("Accessibility が許可されていません。システム設定で、このバイナリを起動した端末に許可を与えてください");
    }
    app::run(mtm, config)
}

pub fn doctor() -> Result<()> {
    let mtm = main_thread()?;
    println!("accessibility: {}", if permissions::accessibility_trusted(false) { "ok" } else { "NOT TRUSTED" });
    println!("event tap: {}", if tap::can_create() { "ok" } else { "cannot create" });
    println!(
        "secure input: {}",
        if permissions::secure_input_enabled() { "ENABLED (hotkey will not work)" } else { "off" }
    );
    match Config::load() {
        Ok(c) => println!(
            "config: ok (hotkey {}, keys {:?}, max_cell {:?}, scope {:?})",
            c.hotkey, c.grid.keys, c.grid.max_cell, c.grid.scope
        ),
        Err(e) => println!("config: ERROR {e:#}"),
    }
    let monitors = screens::monitors(mtm);
    println!("monitors: {}", monitors.len());
    for m in &monitors {
        println!("  #{} {}", m.id, fmt_rect(m));
    }
    Ok(())
}

fn fmt_rect(m: &Monitor) -> String {
    format!(
        "x={} y={} {}x{} scale={}",
        m.frame.x, m.frame.y, m.frame.w, m.frame.h, m.scale
    )
}

pub fn layout() -> Result<()> {
    let mtm = main_thread()?;
    let config = Config::load()?;
    let monitors = screens::monitors(mtm);
    let cursor = screens::cursor();
    let nav = nav_for(&config, &monitors, cursor)?;
    let (cw, ch) = nav.cell_size();
    println!("scope: {:?}", config.grid.scope);
    println!("canvas: {:?}", nav.canvas());
    println!("label chars: {}", nav.leaf_depth());
    println!("axes: {:?}", nav.axes());
    println!("cell: {cw:.1}x{ch:.1}pt");
    let labels = nav.labels();
    for (i, m) in monitors.iter().enumerate() {
        let n = labels.iter().filter(|l| l.monitor == i).count();
        println!("monitor #{}: {} labels  ({})", m.id, n, fmt_rect(m));
    }
    Ok(())
}

pub fn cursor() -> Result<()> {
    let p = screens::cursor();
    println!("cursor {:.1} {:.1}", p.x, p.y);
    Ok(())
}

pub fn path(x: f64, y: f64) -> Result<()> {
    let mtm = main_thread()?;
    let config = Config::load()?;
    let monitors = screens::monitors(mtm);
    let nav = nav_for(&config, &monitors, screens::cursor())?;
    match nav.path_to(Point::new(x, y)) {
        Some(label) => println!("path {x} {y} {label}"),
        None => bail!("({x}, {y}) はどのモニターの中にもありません"),
    }
    Ok(())
}

/// tap なしで起動し、キーを順に流す。
pub fn demo(keys: &str, delay_ms: u64, allow_click: bool) -> Result<()> {
    let mtm = main_thread()?;
    let config = Config::load()?;
    let keymap = KeyMap::from_config(&config)?;
    let mut codes = Vec::new();
    for k in keys.split_whitespace() {
        let code = parse_key(k)?;
        if !allow_click
            && let Some(Command::Act(a)) = keymap.lookup(code)
            && a != Action::MoveOnly
        {
            bail!("{k:?} はクリックします。確認だけなら f を使うか、--allow-click を付けてください");
        }
        codes.push((k, code));
    }
    // 確認用のコントローラ（設定が正しいか先に検証する）。
    Controller::new(config.clone())?;

    app::init_app(mtm);
    let mut runtime = Runtime::new(mtm, config)?;
    let delay = Duration::from_millis(delay_ms);

    let out = runtime.activate();
    println!("step activate  {:.2}ms (draw {:.2}ms)", ms(out.elapsed), ms(overlay::last_draw()));
    app::pump(delay);
    println!("       first paint {:.2}ms", ms(overlay::last_draw()));

    let mut target = None;
    for (name, code) in codes {
        let out = runtime.key_down(code, Mods::NONE);
        println!("step key {name:<5} {:.2}ms (draw {:.2}ms)", ms(out.elapsed), ms(overlay::last_draw()));
        if out.target.is_some() {
            target = out.target;
        }
        app::pump(delay);
    }
    if runtime.is_active() {
        runtime.cancel_external();
    }
    match target {
        Some(p) => println!("target {:.1} {:.1}", p.x, p.y),
        None => println!("target none"),
    }
    // 投げたイベントが届くまで少し待つ。
    app::pump(Duration::from_millis(150));
    let _ = Scope::AllMonitors;
    Ok(())
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
