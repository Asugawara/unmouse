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
    /// セルの分け方。
    pub subdivide: Subdivide,
    pub scope: Scope,
    /// true なら、セルが決まった後も表示を残して細分できる。false なら決まった時点で閉じる。
    pub refine: bool,
}

/// セルの分け方。`Off` は一様（2台で 512 セル）、`Half` は最後の段だけ半分（1024 セル、5文字と6文字）、
/// `Quarter` は4等分の段を1つ足す（2048 セル、6文字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subdivide {
    Off,
    #[default]
    Half,
    Quarter,
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
    /// 次に打つ文字の色。`grid.keys` と同じ順の4つ。
    pub key_colors: Vec<String>,
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
            subdivide: Subdivide::default(),
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
            key_colors: ["#0050ff", "#009900", "#ff0000", "#cc00cc"].map(String::from).to_vec(),
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

    /// キー `c`（大文字可）の色。`grid.keys` の同じ位置の `style.key_colors` を読む。
    /// `grid.keys` にないキーなら `None`。
    pub fn key_rgb(&self, c: char) -> Option<(u8, u8, u8)> {
        let c = c.to_ascii_lowercase().to_string();
        let idx = self
            .grid
            .keys
            .iter()
            .position(|k| k.to_ascii_lowercase() == c)?;
        parse_hex(self.style.key_colors.get(idx)?).ok()
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
        ensure!(
            self.style.key_colors.len() == 4,
            "style.key_colors は4個で指定してください"
        );
        for color in &self.style.key_colors {
            parse_hex(color).context("style.key_colors")?;
        }

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
        .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))
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
    fn key_colors_default_and_lookup() {
        let c = Config::default();
        assert_eq!(c.key_rgb('j'), Some((0, 80, 255)));
        assert_eq!(c.key_rgb(';'), Some((204, 0, 204)));
        assert_eq!(c.key_rgb('L'), Some((255, 0, 0)));
        assert_eq!(c.key_rgb('L'), c.key_rgb('l'));
        assert_eq!(c.key_rgb('x'), None);
    }

    #[test]
    fn key_colors_must_be_four_valid_colors() {
        assert!(Config::from_toml("[style]\nkey_colors = [\"#1d4ed8\", \"#15803d\", \"#c2410c\"]").is_err());
        assert!(Config::from_toml("[style]\nkey_colors = [\"#12345\", \"#15803d\", \"#c2410c\", \"#7e22ce\"]").is_err());
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
    fn subdivide_default_is_half() {
        assert_eq!(Config::default().grid.subdivide, Subdivide::Half);
    }

    #[test]
    fn subdivide_parses() {
        let parse = |text: &str| Config::from_toml(text).unwrap().grid.subdivide;
        assert_eq!(parse("[grid]\nsubdivide = \"off\""), Subdivide::Off);
        assert_eq!(parse("[grid]\nsubdivide = \"half\""), Subdivide::Half);
        assert_eq!(parse("[grid]\nsubdivide = \"quarter\""), Subdivide::Quarter);
    }

    #[test]
    fn subdivide_unknown_value_is_error() {
        assert!(Config::from_toml("[grid]\nsubdivide = \"foo\"").is_err());
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

    #[test]
    fn label_layout_unknown_value_is_error() {
        assert!(Config::from_toml("[style]\nlabel_layout = \"foo\"").is_err());
    }

    #[test]
    fn scope_parses_and_defaults_to_all_monitors() {
        let parse = |text: &str| Config::from_toml(text).unwrap().grid.scope;
        assert_eq!(parse(""), Scope::AllMonitors);
        assert_eq!(parse("[grid]\nscope = \"all_monitors\""), Scope::AllMonitors);
        assert_eq!(parse("[grid]\nscope = \"cursor_monitor\""), Scope::CursorMonitor);
        assert!(Config::from_toml("[grid]\nscope = \"foo\"").is_err());
    }

    #[test]
    fn unknown_fields_in_sections_are_errors() {
        assert!(Config::from_toml("[grid]\nbogus = 1").is_err());
        assert!(Config::from_toml("[actions]\nbogus = \"x\"").is_err());
        assert!(Config::from_toml("[style]\nbogus = 1").is_err());
        // 以前の設定名（extra_chars）は使えない
        assert!(Config::from_toml("[grid]\nextra_chars = 1").is_err());
    }

    #[test]
    fn wrong_types_and_broken_toml_are_errors() {
        assert!(Config::from_toml("double_tap_ms = \"fast\"").is_err());
        assert!(Config::from_toml("[grid]\nrefine = \"yes\"").is_err());
        assert!(Config::from_toml("[style]\nuppercase = 1").is_err());
        assert!(Config::from_toml("[grid\nrefine = true").is_err());
    }

    #[test]
    fn invalid_hotkey_is_error() {
        assert!(Config::from_toml("hotkey = \"ctrl+foo\"").is_err());
        assert!(Config::from_toml("hotkey = \"double:\"").is_err());
        assert!(Config::from_toml("hotkey = \"\"").is_err());
        let chord = Config::from_toml("hotkey = \"ctrl+alt+j\"").unwrap();
        assert_eq!(
            chord.trigger().unwrap(),
            Trigger::Chord((0x26, crate::keys::Mods::CTRL | crate::keys::Mods::ALT))
        );
    }

    #[test]
    fn max_cell_must_be_positive() {
        assert!(Config::from_toml("[grid]\nmax_cell = [0.0, 100.0]").is_err());
        assert!(Config::from_toml("[grid]\nmax_cell = [180.0, 0.0]").is_err());
        assert!(Config::from_toml("[grid]\nmax_cell = [-1.0, 100.0]").is_err());
        assert!(Config::from_toml("[grid]\nmax_cell = [180.0]").is_err());
        let c = Config::from_toml("[grid]\nmax_cell = [200.0, 120.0]").unwrap();
        assert_eq!(c.max_cell(), (200.0, 120.0));
        assert_eq!(Config::default().max_cell(), (180.0, 100.0));
    }

    #[test]
    fn opacity_ranges_are_zero_to_one_inclusive() {
        for name in ["dim_opacity", "label_bg_opacity", "rest_opacity"] {
            assert!(Config::from_toml(&format!("[style]\n{name} = 0.0")).is_ok(), "{name}");
            assert!(Config::from_toml(&format!("[style]\n{name} = 1.0")).is_ok(), "{name}");
            assert!(Config::from_toml(&format!("[style]\n{name} = -0.1")).is_err(), "{name}");
            assert!(Config::from_toml(&format!("[style]\n{name} = 1.1")).is_err(), "{name}");
        }
    }

    #[test]
    fn size_ranges_are_six_to_forty_eight_inclusive() {
        // label_size は min_label_size 以上である必要があるので、下限は min も 6 にして確かめる
        assert!(Config::from_toml("[style]\nlabel_size = 6\nmin_label_size = 6").is_ok());
        assert!(Config::from_toml("[style]\nlabel_size = 48").is_ok());
        assert!(Config::from_toml("[style]\nedge_key_size = 6").is_ok());
        assert!(Config::from_toml("[style]\nedge_key_size = 48").is_ok());
        assert!(Config::from_toml("[style]\nedge_key_size = 5.9").is_err());
        assert!(Config::from_toml("[style]\nedge_key_size = 49").is_err());
        assert!(Config::from_toml("[style]\nmin_label_size = 5.9").is_err());
        assert!(Config::from_toml("[style]\nmin_label_size = 49").is_err());
        // min_label_size == label_size は許される
        assert!(Config::from_toml("[style]\nlabel_size = 10\nmin_label_size = 10").is_ok());
        assert!(Config::from_toml("[style]\nlabel_size = 10\nmin_label_size = 10.5").is_err());
    }

    #[test]
    fn invalid_accent_is_error() {
        assert!(Config::from_toml("[style]\naccent = \"red\"").is_err());
        assert!(Config::from_toml("[style]\naccent = \"1d4ed8\"").is_err());
        assert!(Config::from_toml("[style]\naccent = \"#1d4ed\"").is_err());
        assert!(Config::from_toml("[style]\naccent = \"#1d4ed8f\"").is_err());
        assert!(Config::from_toml("[style]\naccent = \"#gggggg\"").is_err());
    }

    #[test]
    fn parse_hex_values() {
        assert_eq!(parse_hex("#000000").unwrap(), (0, 0, 0));
        assert_eq!(parse_hex("#ffffff").unwrap(), (255, 255, 255));
        assert_eq!(parse_hex("#FFFFFF").unwrap(), (255, 255, 255));
        assert_eq!(parse_hex("#0a0B0c").unwrap(), (10, 11, 12));
        for bad in ["", "#", "#fff", "#1234567", "#12345g", "1d4ed8", " #1d4ed8"] {
            assert!(parse_hex(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn parse_hex_rejects_sign_characters() {
        // #rrggbb は16進数の6桁だけ。符号は色の値ではない
        assert!(parse_hex("#+12345").is_err());
        assert!(parse_hex("#12+345").is_err());
    }

    #[test]
    fn parse_hex_multibyte_is_error_not_panic() {
        // バイト数は6だが、2バイト文字が境目をまたぐ
        assert!(parse_hex("#aéabc").is_err());
        assert!(parse_hex("#ééé").is_err());
    }

    #[test]
    fn key_colors_follow_custom_grid_keys() {
        let c = Config::from_toml(
            "[grid]\nkeys = [\"u\", \"i\", \"o\", \"p\"]\n[style]\nkey_colors = [\"#010203\", \"#040506\", \"#070809\", \"#0a0b0c\"]",
        )
        .unwrap();
        assert_eq!(c.key_rgb('u'), Some((1, 2, 3)));
        assert_eq!(c.key_rgb('I'), Some((4, 5, 6)));
        assert_eq!(c.key_rgb('p'), Some((10, 11, 12)));
        // 既定のキーはもう grid.keys にない
        assert_eq!(c.key_rgb('j'), None);
    }

    #[test]
    fn key_chars_returns_the_four_keys_in_order() {
        assert_eq!(Config::default().key_chars().unwrap(), ['j', 'k', 'l', ';']);
        let mut c = Config::default();
        c.grid.keys = ["jj", "k", "l", ";"].map(String::from).to_vec();
        assert!(c.key_chars().is_err());
        c.grid.keys = ["", "k", "l", ";"].map(String::from).to_vec();
        assert!(c.key_chars().is_err());
        c.grid.keys = ["j", "k", "l"].map(String::from).to_vec();
        assert!(c.key_chars().is_err());
        c.grid.keys = ["j", "k", "l", ";", "a"].map(String::from).to_vec();
        assert!(c.key_chars().is_err());
    }

    #[test]
    fn key_colors_wrong_count_or_format_is_error() {
        assert!(Config::from_toml(
            "[style]\nkey_colors = [\"#000000\", \"#000000\", \"#000000\", \"#000000\", \"#000000\"]"
        )
        .is_err());
        assert!(Config::from_toml("[style]\nkey_colors = []").is_err());
        assert!(Config::from_toml(
            "[style]\nkey_colors = [\"red\", \"#000000\", \"#000000\", \"#000000\"]"
        )
        .is_err());
    }

    #[test]
    fn double_tap_ms_is_checked_even_with_chord_hotkey() {
        assert!(Config::from_toml("hotkey = \"ctrl+alt+j\"\ndouble_tap_ms = 50").is_err());
    }

    #[test]
    fn load_reads_xdg_then_home_then_default() {
        use std::ffi::OsString;

        // 環境変数を触るのはこのテストだけ。途中で失敗しても戻す。
        struct Restore(Option<OsString>, Option<OsString>);
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    match &self.0 {
                        Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
                        None => std::env::remove_var("XDG_CONFIG_HOME"),
                    }
                    match &self.1 {
                        Some(v) => std::env::set_var("HOME", v),
                        None => std::env::remove_var("HOME"),
                    }
                }
            }
        }
        let _restore = Restore(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        );

        let root = std::env::temp_dir().join(format!("unmouse-config-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let xdg = root.join("xdg");
        let home = root.join("home");
        fs::create_dir_all(xdg.join("unmouse")).unwrap();
        fs::create_dir_all(home.join(".config/unmouse")).unwrap();
        let xdg_file = xdg.join("unmouse/config.toml");
        let home_file = home.join(".config/unmouse/config.toml");

        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &xdg);
            std::env::set_var("HOME", &home);
        }

        // どちらもなければ既定値
        assert_eq!(Config::load().unwrap(), Config::default());

        // HOME 側だけ
        fs::write(&home_file, "double_tap_ms = 123").unwrap();
        assert_eq!(Config::load().unwrap().double_tap_ms, 123);

        // XDG があれば優先する
        fs::write(&xdg_file, "double_tap_ms = 456").unwrap();
        assert_eq!(Config::load().unwrap().double_tap_ms, 456);

        // XDG_CONFIG_HOME が空文字なら HOME 側を見る
        unsafe { std::env::set_var("XDG_CONFIG_HOME", "") };
        assert_eq!(Config::load().unwrap().double_tap_ms, 123);
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &xdg) };

        // 不正な設定はエラーで、どのファイルかが分かる
        fs::write(&xdg_file, "double_tap_ms = 5").unwrap();
        let err = format!("{:#}", Config::load().unwrap_err());
        assert!(err.contains(xdg_file.to_str().unwrap()), "{err}");

        let _ = fs::remove_dir_all(&root);
    }
}
