//! キーコード（macOS の kVK_ANSI_* 値）と操作の対応。判定は物理キーコードで行うので、JIS 配列でも同じ位置で動く。

use std::collections::HashMap;
use std::ops::BitOr;

use anyhow::{Context, Result, bail, ensure};

use crate::config::Config;
use crate::trigger::Trigger;

/// 修飾キーの集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const CTRL: Mods = Mods(1);
    pub const ALT: Mods = Mods(2);
    pub const SHIFT: Mods = Mods(4);
    pub const CMD: Mods = Mods(8);
}

impl BitOr for Mods {
    type Output = Mods;

    fn bitor(self, rhs: Mods) -> Mods {
        Mods(self.0 | rhs.0)
    }
}

/// 1打で実行する操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    LeftClick,
    RightClick,
    DoubleClick,
    MiddleClick,
    MoveOnly,
    Drag,
}

/// キーに割り当てた意味。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Label(char),
    Act(Action),
    Back,
    Cancel,
}

/// 文字に対応する kVK_ANSI_* のキーコード。
pub fn char_keycode(c: char) -> Option<u16> {
    let code = match c.to_ascii_lowercase() {
        'a' => 0x00,
        's' => 0x01,
        'd' => 0x02,
        'f' => 0x03,
        'h' => 0x04,
        'g' => 0x05,
        'z' => 0x06,
        'x' => 0x07,
        'c' => 0x08,
        'v' => 0x09,
        'b' => 0x0B,
        'q' => 0x0C,
        'w' => 0x0D,
        'e' => 0x0E,
        'r' => 0x0F,
        'y' => 0x10,
        't' => 0x11,
        '1' => 0x12,
        '2' => 0x13,
        '3' => 0x14,
        '4' => 0x15,
        '6' => 0x16,
        '5' => 0x17,
        '=' => 0x18,
        '9' => 0x19,
        '7' => 0x1A,
        '-' => 0x1B,
        '8' => 0x1C,
        '0' => 0x1D,
        ']' => 0x1E,
        'o' => 0x1F,
        'u' => 0x20,
        '[' => 0x21,
        'i' => 0x22,
        'p' => 0x23,
        'l' => 0x25,
        'j' => 0x26,
        '\'' => 0x27,
        'k' => 0x28,
        ';' => 0x29,
        '\\' => 0x2A,
        ',' => 0x2B,
        '/' => 0x2C,
        'n' => 0x2D,
        'm' => 0x2E,
        '.' => 0x2F,
        '`' => 0x32,
        _ => return None,
    };
    Some(code)
}

/// 名前（`space` など）または1文字からキーコードを得る。
pub fn parse_key(s: &str) -> Result<u16> {
    let name = s.trim().to_ascii_lowercase();
    let code = match name.as_str() {
        "space" => 0x31,
        "escape" | "esc" => 0x35,
        "backspace" => 0x33,
        "return" | "enter" => 0x24,
        "tab" => 0x30,
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => {
                    char_keycode(c).with_context(|| format!("未対応のキー: {s:?}"))?
                }
                _ => bail!("未知のキー: {s:?}"),
            }
        }
    };
    Ok(code)
}

/// `ctrl+alt+j` の形のホットキーを、キーコードと修飾キーに分ける。
/// `option` は `alt`、`command` は `cmd` の別名。
pub fn parse_hotkey(s: &str) -> Result<(u16, Mods)> {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (key, modifiers) = parts
        .split_last()
        .with_context(|| format!("ホットキーが空です: {s:?}"))?;
    let mut mods = Mods::NONE;
    for part in modifiers {
        let flag = match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Mods::CTRL,
            "alt" | "option" | "opt" => Mods::ALT,
            "shift" => Mods::SHIFT,
            "cmd" | "command" => Mods::CMD,
            other => bail!("未知の修飾キー: {other:?}"),
        };
        mods = mods | flag;
    }
    Ok((parse_key(key)?, mods))
}

fn insert_unique(
    map: &mut HashMap<u16, Command>,
    code: u16,
    command: Command,
    name: &str,
) -> Result<()> {
    ensure!(!map.contains_key(&code), "{name} が他のキーと重なっています");
    map.insert(code, command);
    Ok(())
}

/// 設定から作る、キーコードの割り当て表。
#[derive(Debug, Clone)]
pub struct KeyMap {
    /// ホットキー。修飾キーの2回タップのときは `None`。
    hotkey: Option<(u16, Mods)>,
    commands: HashMap<u16, Command>,
}

impl KeyMap {
    pub fn from_config(config: &Config) -> Result<Self> {
        let hotkey = match config.trigger()? {
            Trigger::Chord(h) => Some(h),
            Trigger::DoubleTap(_) => None,
        };
        let mut commands = HashMap::new();

        for ch in config.key_chars()? {
            let code = char_keycode(ch).with_context(|| format!("grid.keys: 未対応の文字 {ch:?}"))?;
            insert_unique(&mut commands, code, Command::Label(ch), &format!("grid.keys の {ch:?}"))?;
        }

        let actions = &config.actions;
        let named = [
            ("actions.left_click", &actions.left_click, Action::LeftClick),
            ("actions.right_click", &actions.right_click, Action::RightClick),
            ("actions.double_click", &actions.double_click, Action::DoubleClick),
            ("actions.middle_click", &actions.middle_click, Action::MiddleClick),
            ("actions.move_only", &actions.move_only, Action::MoveOnly),
            ("actions.drag", &actions.drag, Action::Drag),
        ];
        for (name, key, action) in named {
            let code = parse_key(key).with_context(|| name)?;
            insert_unique(&mut commands, code, Command::Act(action), name)?;
        }
        for key in &actions.back {
            let code = parse_key(key).context("actions.back")?;
            insert_unique(&mut commands, code, Command::Back, "actions.back")?;
        }
        let cancel = parse_key(&actions.cancel).context("actions.cancel")?;
        insert_unique(&mut commands, cancel, Command::Cancel, "actions.cancel")?;

        Ok(Self { hotkey, commands })
    }

    pub fn is_hotkey(&self, keycode: u16, mods: Mods) -> bool {
        self.hotkey == Some((keycode, mods))
    }

    /// 起動中は修飾キーを見ない。
    pub fn lookup(&self, keycode: u16) -> Option<Command> {
        self.commands.get(&keycode).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const J: u16 = 0x26;
    const SEMI: u16 = 0x29;
    const SPACE: u16 = 0x31;
    const D: u16 = 0x02;
    const H: u16 = 0x04;
    const BACKSPACE: u16 = 0x33;
    const ESC: u16 = 0x35;

    fn map() -> KeyMap {
        let config = Config {
            hotkey: "ctrl+alt+j".to_string(),
            ..Config::default()
        };
        KeyMap::from_config(&config).unwrap()
    }

    #[test]
    fn char_codes() {
        assert_eq!(char_keycode('a'), Some(0x00));
        assert_eq!(char_keycode('j'), Some(J));
        assert_eq!(char_keycode(';'), Some(SEMI));
        assert_eq!(char_keycode(','), Some(0x2B));
        assert_eq!(char_keycode('.'), Some(0x2F));
        assert_eq!(char_keycode('/'), Some(0x2C));
        assert_eq!(char_keycode('é'), None);
    }

    #[test]
    fn named_codes() {
        assert_eq!(parse_key("space").unwrap(), SPACE);
        assert_eq!(parse_key("escape").unwrap(), ESC);
        assert_eq!(parse_key("backspace").unwrap(), BACKSPACE);
        assert_eq!(parse_key("return").unwrap(), 0x24);
        assert!(parse_key("foo").is_err());
    }

    #[test]
    fn lookup_defaults() {
        let m = map();
        assert_eq!(m.lookup(J), Some(Command::Label('j')));
        assert_eq!(m.lookup(SEMI), Some(Command::Label(';')));
        assert_eq!(m.lookup(SPACE), Some(Command::Act(Action::LeftClick)));
        assert_eq!(m.lookup(D), Some(Command::Act(Action::RightClick)));
        assert_eq!(m.lookup(H), Some(Command::Back));
        assert_eq!(m.lookup(BACKSPACE), Some(Command::Back));
        assert_eq!(m.lookup(ESC), Some(Command::Cancel));
        assert_eq!(m.lookup(0x0C), None);
    }

    #[test]
    fn hotkey_parsing() {
        assert_eq!(
            parse_hotkey("ctrl+alt+j").unwrap(),
            (J, Mods::CTRL | Mods::ALT)
        );
        assert_eq!(
            parse_hotkey("option+command+j").unwrap(),
            (J, Mods::ALT | Mods::CMD)
        );
        assert!(parse_hotkey("ctrl+foo").is_err());
        assert!(parse_hotkey("hyper+j").is_err());
        assert!(map().is_hotkey(J, Mods::CTRL | Mods::ALT));
        assert!(!map().is_hotkey(J, Mods::NONE));
    }

    #[test]
    fn double_tap_trigger_has_no_chord() {
        let m = KeyMap::from_config(&Config::default()).unwrap();
        assert!(!m.is_hotkey(J, Mods::CTRL | Mods::ALT));
    }

    #[test]
    fn duplicate_or_overlapping_keys_are_errors() {
        let dup: Config = toml::from_str("[grid]\nkeys = [\"j\", \"j\", \"l\", \";\"]").unwrap();
        assert!(KeyMap::from_config(&dup).is_err());
        let overlap: Config = toml::from_str("[actions]\ndrag = \"j\"").unwrap();
        assert!(KeyMap::from_config(&overlap).is_err());
    }

    #[test]
    fn every_ansi_char_matches_kvk_table() {
        // HIToolbox Events.h の kVK_ANSI_*
        let table: [(char, u16); 47] = [
            ('a', 0x00), ('s', 0x01), ('d', 0x02), ('f', 0x03), ('h', 0x04), ('g', 0x05),
            ('z', 0x06), ('x', 0x07), ('c', 0x08), ('v', 0x09), ('b', 0x0B), ('q', 0x0C),
            ('w', 0x0D), ('e', 0x0E), ('r', 0x0F), ('y', 0x10), ('t', 0x11), ('1', 0x12),
            ('2', 0x13), ('3', 0x14), ('4', 0x15), ('6', 0x16), ('5', 0x17), ('=', 0x18),
            ('9', 0x19), ('7', 0x1A), ('-', 0x1B), ('8', 0x1C), ('0', 0x1D), (']', 0x1E),
            ('o', 0x1F), ('u', 0x20), ('[', 0x21), ('i', 0x22), ('p', 0x23), ('l', 0x25),
            ('j', 0x26), ('\'', 0x27), ('k', 0x28), (';', 0x29), ('\\', 0x2A), (',', 0x2B),
            ('/', 0x2C), ('n', 0x2D), ('m', 0x2E), ('.', 0x2F), ('`', 0x32),
        ];
        for (c, code) in table {
            assert_eq!(char_keycode(c), Some(code), "{c:?}");
        }
        let mut codes: Vec<u16> = table.iter().map(|&(_, code)| code).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), table.len(), "キーコードが重なっている");
        // 大文字も同じキー
        assert_eq!(char_keycode('J'), Some(J));
        assert_eq!(char_keycode('Z'), char_keycode('z'));
        // 割り当てのない文字
        assert_eq!(char_keycode(' '), None);
        assert_eq!(char_keycode('あ'), None);
    }

    #[test]
    fn parse_key_aliases_case_and_whitespace() {
        assert_eq!(parse_key("esc").unwrap(), ESC);
        assert_eq!(parse_key("enter").unwrap(), 0x24);
        assert_eq!(parse_key("tab").unwrap(), 0x30);
        assert_eq!(parse_key("SPACE").unwrap(), SPACE);
        assert_eq!(parse_key("  Escape ").unwrap(), ESC);
        assert_eq!(parse_key("J").unwrap(), J);
        assert_eq!(parse_key(";").unwrap(), SEMI);
        assert!(parse_key("").is_err());
        assert!(parse_key("jj").is_err());
        assert!(parse_key("é").is_err());
    }

    #[test]
    fn parse_hotkey_forms() {
        // 修飾キーなし
        assert_eq!(parse_hotkey("j").unwrap(), (J, Mods::NONE));
        // 別名、大文字、空白
        assert_eq!(
            parse_hotkey("Control + Opt + Shift + Command + space").unwrap(),
            (SPACE, Mods::CTRL | Mods::ALT | Mods::SHIFT | Mods::CMD)
        );
        assert_eq!(parse_hotkey("alt+cmd+j").unwrap(), (J, Mods::ALT | Mods::CMD));
        // 修飾キーの重複は同じ
        assert_eq!(parse_hotkey("ctrl+ctrl+j").unwrap(), (J, Mods::CTRL));
        // キーが欠けている・空
        assert!(parse_hotkey("").is_err());
        assert!(parse_hotkey("ctrl+").is_err());
        assert!(parse_hotkey("ctrl+alt").is_err(), "alt はキーとして未知");
        // 修飾キーの位置にキーがある
        assert!(parse_hotkey("j+ctrl").is_err());
    }

    #[test]
    fn mods_are_distinct_bits() {
        let all = [Mods::CTRL, Mods::ALT, Mods::SHIFT, Mods::CMD];
        for (i, a) in all.iter().enumerate() {
            assert_ne!(*a, Mods::NONE);
            for b in &all[i + 1..] {
                assert_ne!(a, b);
                assert_eq!(*a | *b, *b | *a);
            }
        }
        assert_eq!(Mods::CTRL | Mods::CTRL, Mods::CTRL);
        assert_eq!(Mods::NONE | Mods::ALT, Mods::ALT);
        assert_eq!(Mods::default(), Mods::NONE);
    }

    #[test]
    fn hotkey_needs_exact_modifiers() {
        let m = map();
        assert!(!m.is_hotkey(J, Mods::CTRL), "修飾キーが足りない");
        assert!(!m.is_hotkey(J, Mods::CTRL | Mods::ALT | Mods::SHIFT), "修飾キーが多い");
        assert!(!m.is_hotkey(K_CODE, Mods::CTRL | Mods::ALT), "キーが違う");
    }

    const K_CODE: u16 = 0x28;

    #[test]
    fn lookup_covers_all_actions_and_label_keys() {
        let m = map();
        for (code, action) in [
            (SPACE, Action::LeftClick),
            (D, Action::RightClick),
            (0x01, Action::DoubleClick),
            (0x00, Action::MiddleClick),
            (0x03, Action::MoveOnly),
            (0x05, Action::Drag),
        ] {
            assert_eq!(m.lookup(code), Some(Command::Act(action)), "{code:#x}");
        }
        // 4つのラベルキーは j k l ; の順
        for (code, c) in [(J, 'j'), (K_CODE, 'k'), (0x25, 'l'), (SEMI, ';')] {
            assert_eq!(m.lookup(code), Some(Command::Label(c)));
        }
    }

    #[test]
    fn custom_label_keys_and_actions_are_used() {
        let config: Config = toml::from_str(
            "[grid]\nkeys = [\"u\", \"i\", \"o\", \"p\"]\n[actions]\nleft_click = \"return\"\ncancel = \"q\"\nback = [\"backspace\"]",
        )
        .unwrap();
        let m = KeyMap::from_config(&config).unwrap();
        assert_eq!(m.lookup(0x20), Some(Command::Label('u')));
        assert_eq!(m.lookup(0x23), Some(Command::Label('p')));
        assert_eq!(m.lookup(0x24), Some(Command::Act(Action::LeftClick)));
        assert_eq!(m.lookup(0x0C), Some(Command::Cancel));
        assert_eq!(m.lookup(BACKSPACE), Some(Command::Back));
        // 元の割り当ては外れる
        assert_eq!(m.lookup(J), None);
        assert_eq!(m.lookup(SPACE), None);
        assert_eq!(m.lookup(ESC), None);
        assert_eq!(m.lookup(H), None);
    }

    #[test]
    fn invalid_keymap_inputs_are_errors() {
        let bad = |text: &str| {
            let config: Config = toml::from_str(text).unwrap();
            KeyMap::from_config(&config).is_err()
        };
        // 未知のキー名
        assert!(bad("[actions]\ndrag = \"foo\""));
        assert!(bad("[actions]\ncancel = \"nope\""));
        assert!(bad("[actions]\nback = [\"h\", \"nope\"]"));
        // back の中の重複、back と他の操作の重複
        assert!(bad("[actions]\nback = [\"h\", \"h\"]"));
        assert!(bad("[actions]\nback = [\"space\"]"));
        // cancel の重複
        assert!(bad("[actions]\ncancel = \"space\""));
        assert!(bad("[actions]\ncancel = \"j\""));
        // grid.keys: 個数、複数文字、キーのない文字
        assert!(bad("[grid]\nkeys = [\"j\", \"k\", \"l\"]"));
        assert!(bad("[grid]\nkeys = [\"jj\", \"k\", \"l\", \";\"]"));
        assert!(bad("[grid]\nkeys = [\"é\", \"k\", \"l\", \";\"]"));
        assert!(bad("[grid]\nkeys = [\"\", \"k\", \"l\", \";\"]"));
        // 大文字と小文字は同じキーなので重複
        assert!(bad("[grid]\nkeys = [\"j\", \"J\", \"l\", \";\"]"));
    }

    #[test]
    fn invalid_hotkey_in_config_is_error() {
        let config = Config {
            hotkey: "ctrl+foo".to_string(),
            ..Config::default()
        };
        assert!(KeyMap::from_config(&config).is_err());
    }
}
