//! 設定ファイル（TOML）。書式は `config.example.toml` のとおり。

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use crate::keys::KeyMap;
use crate::monitors::Scope;
use crate::trigger::Trigger;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub hotkey: String,
    pub double_tap_ms: u64,
    pub grid: Grid,
    pub actions: Actions,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Grid {
    pub keys: Vec<String>,
    pub max_cell: [f64; 2],
    pub scope: Scope,
    /// true なら、セルが決まった後も表示を残して細分できる。false なら決まった時点で閉じる。
    pub refine: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Actions {
    pub left_click: String,
    pub right_click: String,
    pub double_click: String,
    pub middle_click: String,
    pub move_only: String,
    pub drag: String,
    pub back: Vec<String>,
    pub cancel: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelLayout {
    #[default]
    Center,
    Inline,
    Corners,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Style {
    pub accent: String,
    pub dim_opacity: f64,
    pub font: String,
    pub label_size: f64,
    pub min_label_size: f64,
    pub edge_key_size: f64,
    pub label_layout: LabelLayout,
    pub label_bg_opacity: f64,
    pub rest_opacity: f64,
    pub uppercase: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "double:right_cmd".to_string(),
            double_tap_ms: 300,
            grid: Grid::default(),
            actions: Actions::default(),
            style: Style::default(),
        }
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            keys: ["j", "k", "l", ";"].map(String::from).to_vec(),
            max_cell: [180.0, 100.0],
            scope: Scope::AllMonitors,
            refine: false,
        }
    }
}

impl Default for Actions {
    fn default() -> Self {
        Self {
            left_click: "space".to_string(),
            right_click: "d".to_string(),
            double_click: "s".to_string(),
            middle_click: "a".to_string(),
            move_only: "f".to_string(),
            drag: "g".to_string(),
            back: vec!["h".to_string(), "backspace".to_string()],
            cancel: "escape".to_string(),
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
            accent: "#1d4ed8".to_string(),
            dim_opacity: 0.30,
            font: String::new(),
            label_size: 11.0,
            min_label_size: 8.0,
            edge_key_size: 11.0,
            label_layout: LabelLayout::Center,
            label_bg_opacity: 0.72,
            rest_opacity: 0.42,
            uppercase: false,
        }
    }
}

impl Config {
    /// TOML 文字列から読み、検証する。空の文字列は既定値になる。
    pub fn from_toml(text: &str) -> Result<Self> {
        let config: Config = toml::from_str(text).context("設定の書式が不正です")?;
        config.validate()?;
        Ok(config)
    }

    /// `$XDG_CONFIG_HOME/unmouse/config.toml`、なければ `~/.config/unmouse/config.toml` を読む。
    /// どちらもなければ既定値。
    pub fn load() -> Result<Self> {
        for path in candidate_paths() {
            if path.is_file() {
                let text = fs::read_to_string(&path)
                    .with_context(|| format!("{} を読めません", path.display()))?;
                return Self::from_toml(&text)
                    .with_context(|| format!("{} が不正です", path.display()));
            }
        }
        Ok(Self::default())
    }

    /// `grid.keys` の4文字。
    pub fn key_chars(&self) -> Result<[char; 4]> {
        ensure!(self.grid.keys.len() == 4, "grid.keys は4個で指定してください");
        let mut out = ['\0'; 4];
        for (slot, key) in out.iter_mut().zip(&self.grid.keys) {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => *slot = c,
                _ => bail!("grid.keys の {key:?} は1文字にしてください"),
            }
        }
        Ok(out)
    }

    /// `hotkey` を読んだ起動の合図。
    pub fn trigger(&self) -> Result<Trigger> {
        Trigger::parse(&self.hotkey).context("hotkey")
    }

    pub fn max_cell(&self) -> (f64, f64) {
        (self.grid.max_cell[0], self.grid.max_cell[1])
    }

    /// `style.accent` を RGB にする。
    pub fn accent_rgb(&self) -> Result<(u8, u8, u8)> {
        parse_hex(&self.style.accent)
    }

    fn validate(&self) -> Result<()> {
        self.trigger()?;
        ensure!(
            (100..=1000).contains(&self.double_tap_ms),
            "double_tap_ms は 100 以上 1000 以下にしてください（指定: {}）",
            self.double_tap_ms
        );
        KeyMap::from_config(self)?;
        self.accent_rgb()?;

        let style = &self.style;
        check_range("style.label_size", style.label_size, 6.0, 48.0)?;
        check_range("style.edge_key_size", style.edge_key_size, 6.0, 48.0)?;
        check_range("style.min_label_size", style.min_label_size, 6.0, 48.0)?;
        ensure!(
            style.min_label_size <= style.label_size,
            "style.min_label_size は style.label_size 以下にしてください"
        );
        check_range("style.dim_opacity", style.dim_opacity, 0.0, 1.0)?;
        check_range("style.label_bg_opacity", style.label_bg_opacity, 0.0, 1.0)?;
        check_range("style.rest_opacity", style.rest_opacity, 0.0, 1.0)?;

        ensure!(
            self.grid.max_cell.iter().all(|&v| v > 0.0),
            "grid.max_cell は正の値にしてください"
        );
        Ok(())
    }
}

fn check_range(name: &str, value: f64, min: f64, max: f64) -> Result<()> {
    ensure!(
        (min..=max).contains(&value),
        "{name} は {min} 以上 {max} 以下にしてください（指定: {value}）"
    );
    Ok(())
}

/// `#rrggbb` を RGB にする。
pub fn parse_hex(s: &str) -> Result<(u8, u8, u8)> {
    let hex = s
        .strip_prefix('#')
        .filter(|h| h.len() == 6)
        .with_context(|| format!("色は #rrggbb の形で指定してください: {s:?}"))?;
    let part = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16)
            .with_context(|| format!("色の値が不正です: {s:?}"))
    };
    Ok((part(0..2)?, part(2..4)?, part(4..6)?))
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        paths.push(PathBuf::from(dir).join("unmouse/config.toml"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".config/unmouse/config.toml"));
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_default() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn example_file_is_default() {
        let text = include_str!("../config.example.toml");
        assert_eq!(Config::from_toml(text).unwrap(), Config::default());
    }

    #[test]
    fn refine_defaults_to_false() {
        assert!(!Config::default().grid.refine);
    }

    #[test]
    fn refine_can_be_enabled() {
        let c = Config::from_toml("[grid]\nrefine = true").unwrap();
        assert!(c.grid.refine);
        let d = Config::default();
        assert_eq!(c.grid.keys, d.grid.keys);
        assert_eq!(c.grid.max_cell, d.grid.max_cell);
        assert_eq!(c.grid.scope, d.grid.scope);
        assert_eq!(c.hotkey, d.hotkey);
        assert_eq!(c.actions, d.actions);
    }

    #[test]
    fn hotkey_only_override_keeps_defaults() {
        let c = Config::from_toml("hotkey = \"ctrl+alt+k\"").unwrap();
        assert_eq!(c.hotkey, "ctrl+alt+k");
        assert_eq!(c.double_tap_ms, 300);
        let d = Config::default();
        assert_eq!(c.grid, d.grid);
        assert_eq!(c.actions, d.actions);
        assert_eq!(c.style, d.style);
    }

    #[test]
    fn default_trigger_is_double_right_cmd() {
        let c = Config::default();
        assert_eq!(c.hotkey, "double:right_cmd");
        assert_eq!(
            c.trigger().unwrap(),
            Trigger::DoubleTap(crate::trigger::Modifier::RightCmd)
        );
        assert!(Config::from_toml("hotkey = \"double:foo\"").is_err());
    }

    #[test]
    fn double_tap_ms_range() {
        assert!(Config::from_toml("double_tap_ms = 99").is_err());
        assert!(Config::from_toml("double_tap_ms = 1001").is_err());
        assert_eq!(Config::from_toml("double_tap_ms = 100").unwrap().double_tap_ms, 100);
        assert_eq!(Config::from_toml("double_tap_ms = 1000").unwrap().double_tap_ms, 1000);
    }

    #[test]
    fn key_count_and_duplicates_are_errors() {
        assert!(Config::from_toml("[grid]\nkeys = [\"j\", \"k\", \"l\"]").is_err());
        assert!(Config::from_toml("[grid]\nkeys = [\"j\", \"k\", \"l\", \"j\"]").is_err());
    }

    #[test]
    fn action_overlaps_are_errors() {
        // 操作キーが keys と重なる
        assert!(Config::from_toml("[actions]\ndrag = \"k\"").is_err());
        // 操作キー同士が重なる
        assert!(Config::from_toml("[actions]\nright_click = \"space\"").is_err());
    }

    #[test]
    fn accent_color_parses() {
        let c = Config::from_toml("[style]\naccent = \"#1d4ed8\"").unwrap();
        assert_eq!(c.accent_rgb().unwrap(), (29, 78, 216));
        assert!(parse_hex("1d4ed8").is_err());
    }

    #[test]
    fn label_size_range_and_min() {
        assert!(Config::from_toml("[style]\nlabel_size = 5").is_err());
        assert!(Config::from_toml("[style]\nlabel_size = 49").is_err());
        // min_label_size（既定 8）が label_size（7）より大きい
        assert!(Config::from_toml("[style]\nlabel_size = 7").is_err());
        assert!(Config::from_toml("[style]\nlabel_size = 9").is_ok());
    }

    #[test]
    fn unknown_fields_are_errors() {
        assert!(Config::from_toml("bogus = 1").is_err());
    }

    #[test]
    fn label_layout_parses() {
        let parse = |text: &str| Config::from_toml(text).unwrap().style.label_layout;
        assert_eq!(parse("[style]\nlabel_layout = \"inline\""), LabelLayout::Inline);
        assert_eq!(parse("[style]\nlabel_layout = \"center\""), LabelLayout::Center);
        assert_eq!(parse("[style]\nlabel_layout = \"corners\""), LabelLayout::Corners);
        assert_eq!(parse(""), LabelLayout::Center);
    }
}
