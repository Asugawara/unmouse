//! キー入力を受けて、表示の更新とマウス操作の指示（Effect）を返す。実際の操作は呼び出し側が行う。

use anyhow::Result;

use crate::config::Config;
use crate::geometry::Point;
use crate::keys::{Action, Command, KeyMap, Mods};
use crate::monitors::Monitor;
use crate::nav::Nav;
use crate::scene::{Scene, scenes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseOp {
    Click { button: Button, count: u32 },
    Down(Button),
    Up(Button),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Show(Vec<Scene>),
    Update(Vec<Scene>),
    Hide,
    MoveCursor { to: Point, dragging: bool },
    Mouse(MouseOp, Point),
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyResult {
    pub consume: bool,
    pub effects: Vec<Effect>,
}

#[derive(Debug)]
struct Session {
    /// 入力前の状態。ドラッグの開始後に入力をやり直すために使う。
    start: Nav,
    nav: Nav,
    /// ドラッグ中なら、ボタンを押した位置。
    drag: Option<Point>,
}

#[derive(Debug)]
pub struct Controller {
    config: Config,
    keys: KeyMap,
    chars: [char; 4],
    session: Option<Session>,
}

impl Controller {
    pub fn new(config: Config) -> Result<Self> {
        let keys = KeyMap::from_config(&config)?;
        let chars = config.key_chars()?;
        Ok(Self {
            config,
            keys,
            chars,
            session: None,
        })
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    /// 起動する。`env` はモニターの配置とカーソル位置。
    pub fn activate(&mut self, env: impl FnOnce() -> (Vec<Monitor>, Point)) -> Vec<Effect> {
        let (monitors, cursor) = env();
        let nav = Nav::new(
            &monitors,
            self.config.grid.scope,
            cursor,
            self.config.max_cell(),
        )
        .with_keys(self.chars)
        .with_subdivide(self.config.grid.subdivide);
        let effects = vec![Effect::Show(scenes(&nav, &self.config))];
        self.session = Some(Session {
            start: nav.clone(),
            nav,
            drag: None,
        });
        effects
    }

    /// 外から取り消す。起動していなければ何もしない。
    pub fn cancel_external(&mut self) -> Vec<Effect> {
        match self.session.take() {
            None => Vec::new(),
            Some(Session { drag: Some(from), .. }) => vec![
                Effect::Hide,
                Effect::Mouse(MouseOp::Up(Button::Left), from),
            ],
            Some(_) => vec![Effect::Hide],
        }
    }

    /// 起動の合図（修飾キーの2回タップなど）を受けた。待機中なら起動し、起動中なら取り消す。
    pub fn on_trigger(&mut self, env: impl FnOnce() -> (Vec<Monitor>, Point)) -> Vec<Effect> {
        if self.is_active() {
            self.cancel_external()
        } else {
            self.activate(env)
        }
    }

    pub fn on_key_down(
        &mut self,
        keycode: u16,
        mods: Mods,
        env: impl FnOnce() -> (Vec<Monitor>, Point),
    ) -> KeyResult {
        if self.keys.is_hotkey(keycode, mods) {
            return KeyResult {
                consume: true,
                effects: self.on_trigger(env),
            };
        }
        if !self.is_active() {
            return KeyResult {
                consume: false,
                effects: Vec::new(),
            };
        }
        let effects = match self.keys.lookup(keycode) {
            Some(command) => self.handle(command),
            None => Vec::new(),
        };
        KeyResult {
            consume: true,
            effects,
        }
    }

    fn handle(&mut self, command: Command) -> Vec<Effect> {
        match command {
            Command::Cancel => self.cancel_external(),
            Command::Act(action) => self.act(action),
            Command::Label(c) => self.step(|nav| nav.push(c)),
            Command::Back => self.step(|nav| nav.back()),
        }
    }

    /// 入力を変えて、変わったら表示を更新する。
    fn step(&mut self, change: impl FnOnce(&mut Nav) -> bool) -> Vec<Effect> {
        let Some(session) = self.session.as_mut() else {
            return Vec::new();
        };
        if !change(&mut session.nav) {
            return Vec::new();
        }
        let decided = session.nav.is_determined();
        let Some(to) = session.nav.target().filter(|_| decided) else {
            return vec![Effect::Update(scenes(&session.nav, &self.config))];
        };
        let dragging = session.drag.is_some();
        if self.config.grid.refine {
            return vec![
                Effect::Update(scenes(&session.nav, &self.config)),
                Effect::MoveCursor { to, dragging },
            ];
        }
        // refine = false: セルが決まった時点で閉じて待機に戻る。
        self.session = None;
        if dragging {
            vec![
                Effect::Hide,
                Effect::MoveCursor { to, dragging: true },
                Effect::Mouse(MouseOp::Up(Button::Left), to),
            ]
        } else {
            vec![Effect::Hide, Effect::MoveCursor { to, dragging: false }]
        }
    }

    fn act(&mut self, action: Action) -> Vec<Effect> {
        let Some(session) = self.session.as_mut() else {
            return Vec::new();
        };
        if let Some(from) = session.drag {
            return match action {
                Action::LeftClick | Action::Drag => self.release(from),
                _ => Vec::new(),
            };
        }
        let Some(target) = session.nav.target() else {
            return Vec::new();
        };
        match action {
            Action::Drag => {
                session.nav = session.start.clone();
                session.drag = Some(target);
                let update = Effect::Update(scenes(&session.nav, &self.config));
                vec![
                    Effect::MoveCursor {
                        to: target,
                        dragging: false,
                    },
                    Effect::Mouse(MouseOp::Down(Button::Left), target),
                    update,
                ]
            }
            Action::MoveOnly => {
                self.session = None;
                vec![
                    Effect::Hide,
                    Effect::MoveCursor {
                        to: target,
                        dragging: false,
                    },
                ]
            }
            Action::LeftClick => self.click(
                MouseOp::Click {
                    button: Button::Left,
                    count: 1,
                },
                target,
            ),
            Action::RightClick => self.click(
                MouseOp::Click {
                    button: Button::Right,
                    count: 1,
                },
                target,
            ),
            Action::DoubleClick => self.click(
                MouseOp::Click {
                    button: Button::Left,
                    count: 2,
                },
                target,
            ),
            Action::MiddleClick => self.click(
                MouseOp::Click {
                    button: Button::Middle,
                    count: 1,
                },
                target,
            ),
        }
    }

    fn click(&mut self, op: MouseOp, target: Point) -> Vec<Effect> {
        self.session = None;
        vec![Effect::Hide, Effect::Mouse(op, target)]
    }

    /// ドラッグを終える。入力がなければ、ボタンを押した位置で離す。
    fn release(&mut self, from: Point) -> Vec<Effect> {
        let target = self
            .session
            .as_ref()
            .and_then(|s| s.nav.target())
            .unwrap_or(from);
        self.session = None;
        vec![
            Effect::Hide,
            Effect::MoveCursor {
                to: target,
                dragging: true,
            },
            Effect::Mouse(MouseOp::Up(Button::Left), target),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Subdivide;
    use crate::geometry::testing::*;

    const J: u16 = 0x26;
    const K: u16 = 0x28;
    const L: u16 = 0x25;
    const SEMI: u16 = 0x29;
    const SPACE: u16 = 0x31;
    const D: u16 = 0x02;
    const S: u16 = 0x01;
    const A: u16 = 0x00;
    const F: u16 = 0x03;
    const G: u16 = 0x05;
    const H: u16 = 0x04;
    const BACKSPACE: u16 = 0x33;
    const ESC: u16 = 0x35;
    const UNASSIGNED: u16 = 0x0C;
    const HOTKEY_KEY: u16 = 0x26;

    fn controller() -> Controller {
        let mut config = Config {
            hotkey: "ctrl+alt+j".to_string(),
            ..Config::default()
        };
        config.grid.refine = true;
        config.grid.subdivide = Subdivide::Off;
        Controller::new(config).unwrap()
    }

    fn controller_no_refine() -> Controller {
        let mut config = Config {
            hotkey: "ctrl+alt+j".to_string(),
            ..Config::default()
        };
        config.grid.subdivide = Subdivide::Off;
        Controller::new(config).unwrap()
    }

    fn started_no_refine() -> Controller {
        let mut c = controller_no_refine();
        hotkey(&mut c);
        c
    }

    fn env() -> (Vec<Monitor>, Point) {
        (vec![mon(M)], Point::new(100.0, 100.0))
    }

    fn key(c: &mut Controller, code: u16) -> KeyResult {
        c.on_key_down(code, Mods::NONE, env)
    }

    fn hotkey(c: &mut Controller) -> KeyResult {
        c.on_key_down(HOTKEY_KEY, Mods::CTRL | Mods::ALT, env)
    }

    fn type_keys(c: &mut Controller, codes: &[u16]) {
        for &code in codes {
            key(c, code);
        }
    }

    fn started() -> Controller {
        let mut c = controller();
        hotkey(&mut c);
        c
    }

    #[test]
    fn idle_hotkey_shows_one_scene() {
        let mut c = controller();
        let result = hotkey(&mut c);
        assert!(result.consume);
        assert!(matches!(
            result.effects.as_slice(),
            [Effect::Show(scenes)] if scenes.len() == 1
        ));
        assert!(c.is_active());
    }

    #[test]
    fn trigger_toggles_between_show_and_hide() {
        let mut c = Controller::new(Config::default()).unwrap();
        assert!(matches!(
            c.on_trigger(env).as_slice(),
            [Effect::Show(scenes)] if scenes.len() == 1
        ));
        assert!(c.is_active());
        assert_eq!(c.on_trigger(env), vec![Effect::Hide]);
        assert!(!c.is_active());
    }

    #[test]
    fn double_tap_config_ignores_chord_keys() {
        let mut c = Controller::new(Config::default()).unwrap();
        let result = hotkey(&mut c);
        assert!(!result.consume);
        assert!(!c.is_active());
    }

    #[test]
    fn idle_plain_key_is_not_consumed() {
        let mut c = controller();
        let result = key(&mut c, J);
        assert!(!result.consume);
        assert!(result.effects.is_empty());
    }

    #[test]
    fn label_key_updates_without_moving_cursor() {
        let mut c = started();
        let result = key(&mut c, L);
        assert!(result.consume);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
    }

    #[test]
    fn completing_label_moves_cursor() {
        let mut c = started();
        type_keys(&mut c, &[L, L]);
        let result = key(&mut c, K);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
        let result = key(&mut c, SEMI);
        assert!(matches!(
            result.effects.as_slice(),
            [
                Effect::Update(_),
                Effect::MoveCursor { to, dragging: false },
            ] if *to == Point::new(855.0, 646.875)
        ));
    }

    #[test]
    fn completing_label_closes_and_moves() {
        let mut c = started_no_refine();
        type_keys(&mut c, &[L, L]);
        let result = key(&mut c, K);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
        let result = key(&mut c, SEMI);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: Point::new(855.0, 646.875),
                    dragging: false,
                },
            ]
        );
        assert!(!c.is_active());

        let result = key(&mut c, SPACE);
        assert!(!result.consume);
        assert!(result.effects.is_empty());
    }

    #[test]
    fn partial_input_actions_still_work() {
        let mut c = started_no_refine();
        type_keys(&mut c, &[L, L, K]);
        assert_eq!(
            key(&mut c, SPACE).effects,
            vec![
                Effect::Hide,
                Effect::Mouse(
                    MouseOp::Click {
                        button: Button::Left,
                        count: 1
                    },
                    Point::new(855.0, 562.5)
                ),
            ]
        );
    }

    #[test]
    fn drag_without_refine_releases_on_completion() {
        let mut c = started_no_refine();
        type_keys(&mut c, &[L, L, K]);
        let result = key(&mut c, G);
        assert!(matches!(
            result.effects.as_slice(),
            [
                Effect::MoveCursor { to, dragging: false },
                Effect::Mouse(MouseOp::Down(Button::Left), down),
                Effect::Update(_),
            ] if *to == Point::new(855.0, 562.5) && *down == Point::new(855.0, 562.5)
        ));
        type_keys(&mut c, &[J, J, J]);
        let result = key(&mut c, J);
        let end = Point::new(45.0, 28.125);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: end,
                    dragging: true
                },
                Effect::Mouse(MouseOp::Up(Button::Left), end),
            ]
        );
        assert!(!c.is_active());
    }

    #[test]
    fn refining_moves_cursor_each_key() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        let result = key(&mut c, SEMI);
        assert!(matches!(
            result.effects.as_slice(),
            [
                Effect::Update(_),
                Effect::MoveCursor { to, dragging: false },
            ] if *to == Point::new(888.75, 646.875)
        ));
    }

    #[test]
    fn back_to_undetermined_does_not_move() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        let result = key(&mut c, H);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));

        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI, SEMI]);
        let result = key(&mut c, H);
        assert!(matches!(
            result.effects.as_slice(),
            [
                Effect::Update(_),
                Effect::MoveCursor { to, dragging: false },
            ] if *to == Point::new(855.0, 646.875)
        ));
    }

    #[test]
    fn drag_target_moves_with_dragging() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI, G]);
        type_keys(&mut c, &[J, J, J]);
        let result = key(&mut c, J);
        assert!(matches!(
            result.effects.as_slice(),
            [
                Effect::Update(_),
                Effect::MoveCursor { to, dragging: true },
            ] if *to == Point::new(45.0, 28.125)
        ));
    }

    #[test]
    fn space_after_llk_semicolon_clicks_target() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        let result = key(&mut c, SPACE);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::Mouse(
                    MouseOp::Click {
                        button: Button::Left,
                        count: 1
                    },
                    Point::new(855.0, 646.875)
                ),
            ]
        );
        assert!(!c.is_active());
    }

    #[test]
    fn space_clicks_at_finer_and_coarser_targets() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI, SEMI]);
        let result = key(&mut c, SPACE);
        assert!(result.effects.contains(&Effect::Mouse(
            MouseOp::Click {
                button: Button::Left,
                count: 1
            },
            Point::new(888.75, 646.875)
        )));

        let mut c = started();
        type_keys(&mut c, &[L, L]);
        let result = key(&mut c, SPACE);
        assert!(result.effects.contains(&Effect::Mouse(
            MouseOp::Click {
                button: Button::Left,
                count: 1
            },
            Point::new(900.0, 562.5)
        )));
    }

    #[test]
    fn space_at_start_is_consumed_without_effect() {
        let mut c = started();
        let result = key(&mut c, SPACE);
        assert!(result.consume);
        assert!(result.effects.is_empty());
        assert!(c.is_active());
    }

    #[test]
    fn other_actions_after_llk_semicolon() {
        let expect = |code: u16, op: MouseOp| {
            let mut c = started();
            type_keys(&mut c, &[L, L, K, SEMI]);
            let result = key(&mut c, code);
            assert!(result.effects.contains(&Effect::Mouse(
                op,
                Point::new(855.0, 646.875)
            )));
        };
        expect(
            D,
            MouseOp::Click {
                button: Button::Right,
                count: 1,
            },
        );
        expect(
            S,
            MouseOp::Click {
                button: Button::Left,
                count: 2,
            },
        );
        expect(
            A,
            MouseOp::Click {
                button: Button::Middle,
                count: 1,
            },
        );

        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        assert_eq!(
            key(&mut c, F).effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: Point::new(855.0, 646.875),
                    dragging: false,
                },
            ]
        );
    }

    #[test]
    fn back_returns_to_previous_input() {
        let mut c = started();
        type_keys(&mut c, &[L, L]);
        let result = key(&mut c, H);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "l");
        key(&mut c, BACKSPACE);
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "");
    }

    #[test]
    fn back_at_start_is_consumed_without_effect() {
        let mut c = started();
        let result = key(&mut c, H);
        assert!(result.consume);
        assert!(result.effects.is_empty());
    }

    #[test]
    fn cancel_and_hotkey_hide() {
        let mut c = started();
        assert_eq!(key(&mut c, ESC).effects, vec![Effect::Hide]);
        assert!(!c.is_active());

        let mut c = started();
        assert_eq!(hotkey(&mut c).effects, vec![Effect::Hide]);
        assert!(!c.is_active());
    }

    #[test]
    fn unassigned_key_is_consumed_without_effect() {
        let mut c = started();
        let result = key(&mut c, UNASSIGNED);
        assert!(result.consume);
        assert!(result.effects.is_empty());
    }

    #[test]
    fn drag_then_release() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        let start = Point::new(855.0, 646.875);
        let result = key(&mut c, G);
        assert_eq!(result.effects.len(), 3);
        assert_eq!(
            result.effects[..2],
            [
                Effect::MoveCursor {
                    to: start,
                    dragging: false
                },
                Effect::Mouse(MouseOp::Down(Button::Left), start),
            ]
        );
        assert!(matches!(result.effects[2], Effect::Update(_)));
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "");

        type_keys(&mut c, &[J, J, J, J]);
        let result = key(&mut c, SPACE);
        let end = Point::new(45.0, 28.125);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: end,
                    dragging: true
                },
                Effect::Mouse(MouseOp::Up(Button::Left), end),
            ]
        );
        assert!(!c.is_active());
    }

    #[test]
    fn cancel_while_dragging_releases_button() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        key(&mut c, G);
        let result = key(&mut c, ESC);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::Mouse(MouseOp::Up(Button::Left), Point::new(855.0, 646.875)),
            ]
        );
    }

    #[test]
    fn drag_release_with_no_input_uses_start_point() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        key(&mut c, G);
        let result = key(&mut c, G);
        let start = Point::new(855.0, 646.875);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: start,
                    dragging: true
                },
                Effect::Mouse(MouseOp::Up(Button::Left), start),
            ]
        );
    }

    fn half_started(refine: bool) -> Controller {
        let mut config = Config {
            hotkey: "ctrl+alt+j".to_string(),
            ..Config::default()
        };
        config.grid.refine = refine;
        config.grid.subdivide = Subdivide::Half;
        let mut c = Controller::new(config).unwrap();
        hotkey(&mut c);
        c
    }

    #[test]
    fn half_five_char_label_closes_and_moves() {
        let mut c = half_started(false);
        type_keys(&mut c, &[L, L, K, SEMI]);
        // 5文字目の前は閉じない
        assert!(c.is_active());
        let result = key(&mut c, L);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: Point::new(877.5, 646.875),
                    dragging: false,
                },
            ]
        );
        assert!(!c.is_active());
    }

    #[test]
    fn half_four_char_label_closes_and_moves() {
        let mut c = half_started(false);
        type_keys(&mut c, &[L, L, K]);
        let result = key(&mut c, J);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: Point::new(832.5, 478.125),
                    dragging: false,
                },
            ]
        );
    }

    #[test]
    fn half_semicolon_step_only_updates_and_space_clicks() {
        let mut c = half_started(false);
        type_keys(&mut c, &[L, L, K]);
        let result = key(&mut c, SEMI);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
        let result = key(&mut c, SPACE);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::Mouse(
                    MouseOp::Click {
                        button: Button::Left,
                        count: 1
                    },
                    Point::new(877.5, 590.625)
                ),
            ]
        );
    }

    #[test]
    fn half_refine_keeps_session_after_four_chars() {
        let mut c = half_started(true);
        type_keys(&mut c, &[L, L, K]);
        let result = key(&mut c, J);
        assert!(c.is_active());
        match result.effects.as_slice() {
            [Effect::Update(scenes), Effect::MoveCursor { to, dragging: false }] => {
                assert_eq!(*to, Point::new(832.5, 478.125));
                let keys = &scenes[0].edge_keys;
                assert_eq!(keys.len(), 8);
                // 葉 45x56.25 は縦が長いので、次に打つ4つは行（セルの左）に並ぶ
                let next: Vec<_> = keys.iter().filter(|k| k.next).collect();
                assert_eq!(next.len(), 4);
                assert!(next.iter().all(|k| k.center.x < 810.0));
            }
            other => panic!("unexpected effects: {other:?}"),
        }
    }

    fn click(button: Button, count: u32, at: Point) -> Effect {
        Effect::Mouse(MouseOp::Click { button, count }, at)
    }

    #[test]
    fn new_rejects_invalid_key_config() {
        let mut config = Config::default();
        config.grid.keys = ["j", "j", "l", ";"].map(String::from).to_vec();
        assert!(Controller::new(config).is_err());
        let mut config = Config::default();
        config.actions.drag = "j".to_string();
        assert!(Controller::new(config).is_err());
        let mut config = Config::default();
        config.grid.keys.pop();
        assert!(Controller::new(config).is_err());
    }

    #[test]
    fn cancel_external_when_idle_does_nothing() {
        let mut c = controller();
        assert!(c.cancel_external().is_empty());
        assert!(!c.is_active());
    }

    #[test]
    fn idle_keys_other_than_the_trigger_are_not_consumed() {
        let mut c = controller();
        for code in [SPACE, ESC, H, BACKSPACE, SEMI, G] {
            let result = key(&mut c, code);
            assert!(!result.consume, "{code:#x}");
            assert!(result.effects.is_empty());
        }
        // 修飾キーの組が違うホットキーは起動しない
        let result = c.on_key_down(HOTKEY_KEY, Mods::CTRL, env);
        assert!(!result.consume);
        assert!(!c.is_active());
        let result = c.on_key_down(HOTKEY_KEY, Mods::CTRL | Mods::ALT | Mods::SHIFT, env);
        assert!(!result.consume);
        assert!(!c.is_active());
    }

    #[test]
    fn active_keys_ignore_modifiers() {
        let mut c = started();
        let result = c.on_key_down(L, Mods::SHIFT, env);
        assert!(result.consume);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "l");
        // ホットキーと同じキーでも、修飾キーの組が違えばラベルのキー
        let result = c.on_key_down(HOTKEY_KEY, Mods::CMD, env);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "lj");
    }

    #[test]
    fn rejected_label_key_is_consumed_without_effect() {
        // 2台の画面で、どのモニターとも重ならない領域になる入力は拒否される
        let mut c = Controller::new(Config {
            hotkey: "ctrl+alt+j".to_string(),
            ..Config::default()
        })
        .unwrap();
        let two = || (vec![mon(MB), mon(EXT_R)], Point::new(0.0, 0.0));
        c.on_key_down(HOTKEY_KEY, Mods::CTRL | Mods::ALT, two);
        assert!(matches!(
            c.on_key_down(J, Mods::NONE, two).effects.as_slice(),
            [Effect::Update(_)]
        ));
        let result = c.on_key_down(J, Mods::NONE, two);
        assert!(result.consume);
        assert!(result.effects.is_empty());
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "j");
    }

    #[test]
    fn half_semicolon_after_semicolon_is_rejected_without_effect() {
        let mut c = half_started(false);
        type_keys(&mut c, &[L, L, K, SEMI]);
        let result = key(&mut c, SEMI);
        assert!(result.consume);
        assert!(result.effects.is_empty());
        assert!(c.is_active());
        // l は葉なので閉じて動かす
        assert!(matches!(
            key(&mut c, L).effects.as_slice(),
            [Effect::Hide, Effect::MoveCursor { .. }]
        ));
    }

    #[test]
    fn can_start_again_after_a_click_with_fresh_input() {
        let mut c = started();
        type_keys(&mut c, &[L, L]);
        key(&mut c, SPACE);
        assert!(!c.is_active());
        let result = hotkey(&mut c);
        assert!(matches!(result.effects.as_slice(), [Effect::Show(_)]));
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "");
        // 1つ入力して取り消し、もう一度起動しても最初から
        key(&mut c, L);
        key(&mut c, ESC);
        hotkey(&mut c);
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "");
    }

    #[test]
    fn drag_at_start_without_input_does_nothing() {
        let mut c = started();
        let result = key(&mut c, G);
        assert!(result.consume);
        assert!(result.effects.is_empty());
        // ドラッグは始まっていないので、続けて入力して普通にクリックできる
        type_keys(&mut c, &[L, L]);
        assert!(key(&mut c, SPACE).effects.contains(&click(
            Button::Left,
            1,
            Point::new(900.0, 562.5)
        )));
    }

    #[test]
    fn other_actions_are_ignored_while_dragging() {
        for code in [D, S, A, F] {
            let mut c = started();
            type_keys(&mut c, &[L, L, K, SEMI]);
            key(&mut c, G);
            let result = key(&mut c, code);
            assert!(result.consume, "{code:#x}");
            assert!(result.effects.is_empty(), "{code:#x}");
            assert!(c.is_active(), "{code:#x}");
            // まだドラッグ中なので、あとで離せる
            let end = key(&mut c, SPACE);
            assert!(end.effects.contains(&Effect::Mouse(
                MouseOp::Up(Button::Left),
                Point::new(855.0, 646.875)
            )));
        }
    }

    #[test]
    fn drag_key_releases_at_the_typed_target() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        key(&mut c, G);
        type_keys(&mut c, &[J, J]);
        // jj の範囲 (0,0,360,225) → 中心 (180,112.5)
        let end = Point::new(180.0, 112.5);
        assert_eq!(
            key(&mut c, G).effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: end,
                    dragging: true
                },
                Effect::Mouse(MouseOp::Up(Button::Left), end),
            ]
        );
        assert!(!c.is_active());
    }

    #[test]
    fn trigger_while_dragging_releases_button() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        key(&mut c, G);
        assert_eq!(
            hotkey(&mut c).effects,
            vec![
                Effect::Hide,
                Effect::Mouse(MouseOp::Up(Button::Left), Point::new(855.0, 646.875)),
            ]
        );
        assert!(!c.is_active());
    }

    #[test]
    fn back_while_dragging_edits_the_new_input() {
        let mut c = started();
        type_keys(&mut c, &[L, L, K, SEMI]);
        key(&mut c, G);
        type_keys(&mut c, &[J, K]);
        key(&mut c, H);
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "j");
        key(&mut c, BACKSPACE);
        let result = key(&mut c, H);
        assert!(result.effects.is_empty(), "入力が空なら戻る先がない");
        assert!(c.is_active());
    }

    #[test]
    fn drag_without_refine_keeps_going_on_partial_input() {
        let mut c = started_no_refine();
        type_keys(&mut c, &[L, L, K]);
        key(&mut c, G);
        // 途中の入力では離さない
        let result = key(&mut c, K);
        assert!(matches!(result.effects.as_slice(), [Effect::Update(_)]));
        assert!(c.is_active());
    }

    #[test]
    fn half_drag_releases_on_a_five_char_leaf() {
        let mut c = half_started(false);
        type_keys(&mut c, &[L, L, K, SEMI]);
        key(&mut c, G);
        type_keys(&mut c, &[J, J, J, L]);
        assert!(c.is_active());
        // jjjl は 3 セルの段。5 文字目 j が葉 (0,56.25,45,56.25) の中心
        let result = key(&mut c, J);
        let end = Point::new(22.5, 84.375);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: end,
                    dragging: true
                },
                Effect::Mouse(MouseOp::Up(Button::Left), end),
            ]
        );
    }

    #[test]
    fn all_click_actions_work_with_partial_input_without_refine() {
        let at = Point::new(855.0, 562.5);
        for (code, want) in [
            (SPACE, click(Button::Left, 1, at)),
            (D, click(Button::Right, 1, at)),
            (S, click(Button::Left, 2, at)),
            (A, click(Button::Middle, 1, at)),
        ] {
            let mut c = started_no_refine();
            type_keys(&mut c, &[L, L, K]);
            assert_eq!(key(&mut c, code).effects, vec![Effect::Hide, want], "{code:#x}");
            assert!(!c.is_active());
        }
        let mut c = started_no_refine();
        type_keys(&mut c, &[L, L, K]);
        assert_eq!(
            key(&mut c, F).effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: at,
                    dragging: false
                }
            ]
        );
    }

    #[test]
    fn actions_at_start_do_nothing_for_every_action() {
        for code in [SPACE, D, S, A, F, G] {
            let mut c = started();
            let result = key(&mut c, code);
            assert!(result.consume, "{code:#x}");
            assert!(result.effects.is_empty(), "{code:#x}");
            assert!(c.is_active());
        }
    }

    #[test]
    fn custom_keys_and_actions_drive_the_controller() {
        let config = Config::from_toml(
            "hotkey = \"ctrl+alt+j\"\n[grid]\nkeys = [\"u\", \"i\", \"o\", \"p\"]\nsubdivide = \"off\"\n[actions]\nleft_click = \"return\"",
        )
        .unwrap();
        let mut c = Controller::new(config).unwrap();
        hotkey(&mut c);
        // o o = 既定の l l
        type_keys(&mut c, &[0x1F, 0x1F]);
        // 既定のラベルキーはもう効かない
        let result = key(&mut c, L);
        assert!(result.consume);
        assert!(result.effects.is_empty());
        assert_eq!(c.session.as_ref().unwrap().nav.typed(), "oo");
        assert_eq!(
            key(&mut c, 0x24).effects,
            vec![Effect::Hide, click(Button::Left, 1, Point::new(900.0, 562.5))]
        );
    }

    #[test]
    fn cursor_monitor_scope_shows_only_the_cursor_monitor() {
        let mut config = Config::default();
        config.grid.scope = crate::monitors::Scope::CursorMonitor;
        let mut c = Controller::new(config).unwrap();
        let effects = c.activate(|| (vec![mon(MB), mon(EXT_R)], Point::new(2000.0, 0.0)));
        match effects.as_slice() {
            [Effect::Show(scenes)] => {
                assert_eq!(scenes.len(), 1);
                assert_eq!(scenes[0].monitor, 1);
            }
            other => panic!("unexpected effects: {other:?}"),
        }
    }

    #[test]
    fn boundary_cell_targets_the_monitor_with_the_largest_overlap() {
        // 2台の境目をまたぐセル（列23・行5、x 1463.375〜1527）は MB の側が大きい。
        // 目標はその重なり (1463.375,0,48.625,82) の中心 (1487.6875,41)
        let mut config = Config {
            hotkey: "ctrl+alt+j".to_string(),
            ..Config::default()
        };
        config.grid.subdivide = Subdivide::Off;
        let mut c = Controller::new(config).unwrap();
        let two = || (vec![mon(MB), mon(EXT_R)], Point::new(0.0, 0.0));
        c.on_key_down(HOTKEY_KEY, Mods::CTRL | Mods::ALT, two);
        let label = c
            .session
            .as_ref()
            .unwrap()
            .nav
            .path_to(Point::new(1470.0, 40.0))
            .unwrap();
        assert_eq!(label.chars().count(), 5);
        let codes: Vec<u16> = label
            .chars()
            .map(|ch| crate::keys::char_keycode(ch).unwrap())
            .collect();
        for &code in &codes[..4] {
            c.on_key_down(code, Mods::NONE, two);
        }
        assert!(c.is_active());
        let result = c.on_key_down(codes[4], Mods::NONE, two);
        assert_eq!(
            result.effects,
            vec![
                Effect::Hide,
                Effect::MoveCursor {
                    to: Point::new(1487.6875, 41.0),
                    dragging: false
                },
            ]
        );
    }
}
