//! 起動の合図。ホットキー（`ctrl+alt+j`）か、修飾キーの2回タップ（`double:right_cmd`）。

use anyhow::{Result, bail};

use crate::keys::{Mods, parse_hotkey};

/// キーコードと修飾キーの組。
pub type Hotkey = (u16, Mods);

/// 左右を区別する修飾キー。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    RightCmd,
    LeftCmd,
    RightOption,
    LeftOption,
    RightShift,
    LeftShift,
    RightCtrl,
    LeftCtrl,
}

impl Modifier {
    const ALL: [Modifier; 8] = [
        Modifier::RightCmd,
        Modifier::LeftCmd,
        Modifier::RightOption,
        Modifier::LeftOption,
        Modifier::RightShift,
        Modifier::LeftShift,
        Modifier::RightCtrl,
        Modifier::LeftCtrl,
    ];

    pub fn parse(name: &str) -> Result<Self> {
        let m = match name.trim().to_ascii_lowercase().as_str() {
            "right_cmd" => Modifier::RightCmd,
            "left_cmd" => Modifier::LeftCmd,
            "right_option" => Modifier::RightOption,
            "left_option" => Modifier::LeftOption,
            "right_shift" => Modifier::RightShift,
            "left_shift" => Modifier::LeftShift,
            "right_ctrl" => Modifier::RightCtrl,
            "left_ctrl" => Modifier::LeftCtrl,
            other => bail!("未知の修飾キー: {other:?}（right_cmd, left_cmd, right_option, left_option, right_shift, left_shift, right_ctrl, left_ctrl）"),
        };
        Ok(m)
    }

    pub fn name(self) -> &'static str {
        match self {
            Modifier::RightCmd => "right_cmd",
            Modifier::LeftCmd => "left_cmd",
            Modifier::RightOption => "right_option",
            Modifier::LeftOption => "left_option",
            Modifier::RightShift => "right_shift",
            Modifier::LeftShift => "left_shift",
            Modifier::RightCtrl => "right_ctrl",
            Modifier::LeftCtrl => "left_ctrl",
        }
    }

    /// kVK のキーコード。
    pub fn keycode(self) -> u16 {
        match self {
            Modifier::RightCmd => 0x36,
            Modifier::LeftCmd => 0x37,
            Modifier::RightOption => 0x3D,
            Modifier::LeftOption => 0x3A,
            Modifier::RightShift => 0x3C,
            Modifier::LeftShift => 0x38,
            Modifier::RightCtrl => 0x3E,
            Modifier::LeftCtrl => 0x3B,
        }
    }

    /// CGEvent の flags にある、左右別の（device-dependent）ビット。
    pub fn device_mask(self) -> u64 {
        match self {
            Modifier::RightCmd => 0x10,
            Modifier::LeftCmd => 0x08,
            Modifier::RightOption => 0x40,
            Modifier::LeftOption => 0x20,
            Modifier::RightShift => 0x04,
            Modifier::LeftShift => 0x02,
            Modifier::RightCtrl => 0x2000,
            Modifier::LeftCtrl => 0x01,
        }
    }

    pub fn from_keycode(code: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.keycode() == code)
    }

    /// flagsChanged の flags から、このキーが押されているか。
    pub fn is_down(self, flags: u64) -> bool {
        flags & self.device_mask() != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Chord(Hotkey),
    DoubleTap(Modifier),
}

impl Trigger {
    /// `"double:right_cmd"` か `"ctrl+alt+j"` を読む。
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().strip_prefix("double:") {
            Some(name) => Ok(Trigger::DoubleTap(Modifier::parse(name)?)),
            None => Ok(Trigger::Chord(parse_hotkey(s)?)),
        }
    }
}

/// 修飾キーの2回タップを見つける。
#[derive(Debug, Clone)]
pub struct DoubleTapDetector {
    modifier: Modifier,
    double_tap_ms: u64,
    /// 押している最中なら、押した時刻。
    down: Option<u64>,
    /// 1回目のタップを離した時刻。
    first_released: Option<u64>,
    /// 今押しているのが2回目か。
    second: bool,
}

impl DoubleTapDetector {
    pub fn new(modifier: Modifier, double_tap_ms: u64) -> Self {
        Self {
            modifier,
            double_tap_ms,
            down: None,
            first_released: None,
            second: false,
        }
    }

    fn reset(&mut self) {
        self.down = None;
        self.first_released = None;
        self.second = false;
    }

    /// 修飾キーの変化を渡す。2回目のタップを離した瞬間に `true`。
    pub fn on_modifier(&mut self, keycode: u16, pressed: bool, t_ms: u64) -> bool {
        if keycode != self.modifier.keycode() {
            self.reset();
            return false;
        }
        if pressed {
            if self.down.is_some() {
                return false;
            }
            self.second = self
                .first_released
                .is_some_and(|r| t_ms.saturating_sub(r) <= self.double_tap_ms);
            if !self.second {
                self.first_released = None;
            }
            self.down = Some(t_ms);
            return false;
        }
        let Some(pressed_at) = self.down.take() else {
            return false;
        };
        if t_ms.saturating_sub(pressed_at) > self.double_tap_ms {
            self.reset();
            return false;
        }
        if self.second {
            self.reset();
            return true;
        }
        self.first_released = Some(t_ms);
        false
    }

    /// 修飾キー以外が押された。途中の状態を捨てる。
    pub fn on_other_key(&mut self) {
        self.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIGHT_CMD: u16 = 0x36;
    const LEFT_CMD: u16 = 0x37;

    fn detector() -> DoubleTapDetector {
        DoubleTapDetector::new(Modifier::RightCmd, 300)
    }

    fn tap(d: &mut DoubleTapDetector, code: u16, down: u64, up: u64) -> bool {
        let first = d.on_modifier(code, true, down);
        assert!(!first, "押した時点では発火しない");
        d.on_modifier(code, false, up)
    }

    #[test]
    fn two_quick_taps_fire_on_second_release() {
        let mut d = detector();
        assert!(!d.on_modifier(RIGHT_CMD, true, 0));
        assert!(!d.on_modifier(RIGHT_CMD, false, 50));
        assert!(!d.on_modifier(RIGHT_CMD, true, 150));
        assert!(d.on_modifier(RIGHT_CMD, false, 200));
    }

    #[test]
    fn slow_second_tap_does_not_fire() {
        let mut d = detector();
        assert!(!tap(&mut d, RIGHT_CMD, 0, 50));
        assert!(!d.on_modifier(RIGHT_CMD, true, 400));
        assert!(!d.on_modifier(RIGHT_CMD, false, 450));
    }

    #[test]
    fn other_key_between_taps_cancels() {
        let mut d = detector();
        assert!(!tap(&mut d, RIGHT_CMD, 0, 50));
        d.on_other_key();
        assert!(!tap(&mut d, RIGHT_CMD, 100, 150));
    }

    #[test]
    fn other_modifier_does_not_fire_and_cancels() {
        let mut d = detector();
        assert!(!tap(&mut d, LEFT_CMD, 0, 50));
        assert!(!tap(&mut d, LEFT_CMD, 100, 150));

        assert!(!tap(&mut d, RIGHT_CMD, 200, 230));
        assert!(!tap(&mut d, LEFT_CMD, 240, 250));
        assert!(!tap(&mut d, RIGHT_CMD, 260, 280));
    }

    #[test]
    fn long_press_is_not_a_tap() {
        let mut d = detector();
        assert!(!tap(&mut d, RIGHT_CMD, 0, 500));
        assert!(!tap(&mut d, RIGHT_CMD, 520, 560));
    }

    #[test]
    fn long_second_press_does_not_fire() {
        let mut d = detector();
        assert!(!tap(&mut d, RIGHT_CMD, 0, 50));
        assert!(!tap(&mut d, RIGHT_CMD, 100, 500));
    }

    #[test]
    fn third_tap_alone_does_not_fire() {
        let mut d = detector();
        assert!(!tap(&mut d, RIGHT_CMD, 0, 50));
        assert!(tap(&mut d, RIGHT_CMD, 100, 150));
        assert!(!tap(&mut d, RIGHT_CMD, 200, 250));
        assert!(tap(&mut d, RIGHT_CMD, 300, 350));
    }

    #[test]
    fn gap_at_the_limit_still_counts() {
        let mut d = detector();
        assert!(!tap(&mut d, RIGHT_CMD, 0, 50));
        assert!(tap(&mut d, RIGHT_CMD, 350, 400));
    }

    #[test]
    fn release_without_press_is_ignored() {
        let mut d = detector();
        assert!(!d.on_modifier(RIGHT_CMD, false, 10));
        assert!(!tap(&mut d, RIGHT_CMD, 100, 150));
    }

    #[test]
    fn parse_trigger() {
        assert_eq!(
            Trigger::parse("double:right_cmd").unwrap(),
            Trigger::DoubleTap(Modifier::RightCmd)
        );
        assert_eq!(Modifier::RightCmd.keycode(), 0x36);
        assert!(Trigger::parse("double:foo").is_err());
        assert_eq!(
            Trigger::parse("ctrl+alt+j").unwrap(),
            Trigger::Chord((0x26, Mods::CTRL | Mods::ALT))
        );
        assert!(Trigger::parse("ctrl+foo").is_err());
    }

    #[test]
    fn keycodes_and_masks() {
        let table = [
            ("right_cmd", 0x36, 0x10),
            ("left_cmd", 0x37, 0x08),
            ("right_option", 0x3D, 0x40),
            ("left_option", 0x3A, 0x20),
            ("right_shift", 0x3C, 0x04),
            ("left_shift", 0x38, 0x02),
            ("right_ctrl", 0x3E, 0x2000),
            ("left_ctrl", 0x3B, 0x01),
        ];
        for (name, code, mask) in table {
            let m = Modifier::parse(name).unwrap();
            assert_eq!((m.name(), m.keycode(), m.device_mask()), (name, code, mask));
            assert_eq!(Modifier::from_keycode(code), Some(m));
        }
        assert_eq!(Modifier::from_keycode(0x39), None);
    }

    #[test]
    fn is_down_uses_side_bit() {
        // 右 Cmd を押した flags（共通の Cmd ビット + 右のビット）。
        let flags = 0x10_0000 | 0x10;
        assert!(Modifier::RightCmd.is_down(flags));
        assert!(!Modifier::LeftCmd.is_down(flags));
        assert!(!Modifier::RightCmd.is_down(0));
    }
}
