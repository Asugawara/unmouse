//! モニターごとの描画内容。座標はモニターの左上を原点にする。

use crate::config::{Config, LabelLayout, Style};
use crate::geometry::{Axis, Point, Rect};
use crate::nav::{LabelCell, Nav};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    /// 次に打つ文字（`next` のときは太字で濃い青）。コーナー表示では、その角の1文字。
    pub strong: char,
    /// 残りの文字（1か所にまとめる表示のときだけ）。
    pub rest: String,
    pub origin: Point,
    pub anchor: Anchor,
    /// 次に打つ文字かどうか。
    pub next: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeKey {
    pub key: char,
    pub center: Point,
    /// `true` は次に打つ方（濃い）、`false` はその次の方（薄い）。
    pub next: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub from: Point,
    pub to: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub monitor: usize,
    /// 範囲の外（暗くする矩形）。
    pub dims: Vec<Rect>,
    /// セルの境界（16% の薄い線）。
    pub faint_lines: Vec<Line>,
    /// 次の分割の境界（55% の線）。
    pub strong_lines: Vec<Line>,
    pub label_size: f64,
    pub labels: Vec<Label>,
    pub edge_keys: Vec<EdgeKey>,
}

/// モニターごとに1つずつ作る。
pub fn scenes(nav: &Nav, config: &Config) -> Vec<Scene> {
    let style = &config.style;
    let depth = nav.depth();
    let leaf = nav.leaf_depth();
    let current = nav.current();
    let typed = nav.typed();
    let labels = nav.labels();
    let corners = style.label_layout == LabelLayout::Corners;
    let (cell_w, _) = nav.cell_size();
    let label_size = fit_label_size(style, cell_w, leaf);
    let determined = depth >= leaf;
    let edge = if determined {
        edge_keys(nav, current, style.edge_key_size)
    } else {
        Vec::new()
    };

    nav.active()
        .into_iter()
        .map(|(monitor, frame)| Scene {
            monitor,
            dims: dims(frame, current),
            faint_lines: faint_lines(nav, frame),
            strong_lines: strong_lines(nav, frame, current),
            label_size,
            labels: labels_for(&labels, monitor, frame, typed, corners, style.uppercase),
            edge_keys: edge
                .iter()
                .filter(|(_, center, _)| frame.contains(*center))
                .map(|&(key, center, next)| EdgeKey {
                    key,
                    center: local(center, frame),
                    next,
                })
                .collect(),
        })
        .collect()
}

/// 文字幅の見積もり（文字数 × 0.6 × 大きさ + 4）がセルの幅に収まるように、必要なら縮める。
fn fit_label_size(style: &Style, cell_w: f64, chars: usize) -> f64 {
    if chars == 0 {
        return style.label_size;
    }
    let per_char = 0.6 * chars as f64;
    if per_char * style.label_size + 4.0 <= cell_w - 4.0 {
        style.label_size
    } else {
        ((cell_w - 8.0) / per_char).max(style.min_label_size)
    }
}

fn local(p: Point, frame: Rect) -> Point {
    Point::new(p.x - frame.x, p.y - frame.y)
}

fn cased(c: char, upper: bool) -> char {
    if upper { c.to_ascii_uppercase() } else { c }
}

fn corner(cell: Rect, index: usize) -> (Anchor, Point) {
    match index {
        0 => (Anchor::TopLeft, Point::new(cell.x + 2.0, cell.y + 2.0)),
        1 => (Anchor::TopRight, Point::new(cell.right() - 2.0, cell.y + 2.0)),
        2 => (Anchor::BottomLeft, Point::new(cell.x + 2.0, cell.bottom() - 2.0)),
        _ => (
            Anchor::BottomRight,
            Point::new(cell.right() - 2.0, cell.bottom() - 2.0),
        ),
    }
}

fn labels_for(
    cells: &[LabelCell],
    monitor: usize,
    frame: Rect,
    typed: &str,
    corners: bool,
    upper: bool,
) -> Vec<Label> {
    let n = typed.chars().count();
    let mut out = Vec::new();
    for cell in cells.iter().filter(|c| {
        c.monitor == monitor && c.label.starts_with(typed) && c.label.chars().count() > n
    }) {
        let piece = cell.piece;
        let chars: Vec<char> = cell.label.chars().collect();
        if corners && chars.len() == 4 {
            for (index, &c) in chars.iter().enumerate().skip(n) {
                let (anchor, at) = corner(piece, index);
                out.push(Label {
                    strong: cased(c, upper),
                    rest: String::new(),
                    origin: local(at, frame),
                    anchor,
                    next: index == n,
                });
            }
        } else {
            out.push(Label {
                strong: cased(chars[n], upper),
                rest: chars[n + 1..].iter().map(|&c| cased(c, upper)).collect(),
                origin: local(Point::new(piece.x + 2.0, piece.y + 2.0), frame),
                anchor: Anchor::TopLeft,
                next: true,
            });
        }
    }
    out
}

/// 範囲の外側を4つの矩形で覆う（上、下、左、右）。モニターの外にある範囲なら全体。
fn dims(frame: Rect, region: Rect) -> Vec<Rect> {
    let x0 = (region.x - frame.x).clamp(0.0, frame.w);
    let x1 = (region.right() - frame.x).clamp(0.0, frame.w);
    let y0 = (region.y - frame.y).clamp(0.0, frame.h);
    let y1 = (region.bottom() - frame.y).clamp(0.0, frame.h);
    [
        Rect::new(0.0, 0.0, frame.w, y0),
        Rect::new(0.0, y1, frame.w, frame.h - y1),
        Rect::new(0.0, y0, x0, y1 - y0),
        Rect::new(x1, y0, frame.w - x1, y1 - y0),
    ]
    .into_iter()
    .filter(|r| r.w > 0.0 && r.h > 0.0)
    .collect()
}

/// 葉のセルの境界のうち、モニターの内側にあるもの。
fn faint_lines(nav: &Nav, frame: Rect) -> Vec<Line> {
    let (cols, rows) = nav.grid_size();
    let canvas = nav.canvas();
    let mut lines = Vec::new();
    for k in 1..cols {
        let x = canvas.x + canvas.w * k as f64 / cols as f64;
        if x > frame.x && x < frame.right() {
            let lx = x - frame.x;
            lines.push(Line {
                from: Point::new(lx, 0.0),
                to: Point::new(lx, frame.h),
            });
        }
    }
    for k in 1..rows {
        let y = canvas.y + canvas.h * k as f64 / rows as f64;
        if y > frame.y && y < frame.bottom() {
            let ly = y - frame.y;
            lines.push(Line {
                from: Point::new(0.0, ly),
                to: Point::new(frame.w, ly),
            });
        }
    }
    lines
}

/// 次に分ける境界（範囲の内側）を、モニターの中で切って返す。
fn strong_lines(nav: &Nav, frame: Rect, current: Rect) -> Vec<Line> {
    let axis = nav.axis_at(nav.depth());
    let mut lines = Vec::new();
    for piece in current.split4(axis).iter().skip(1) {
        match axis {
            Axis::Cols => {
                let x = piece.x;
                let top = current.y.max(frame.y);
                let bottom = current.bottom().min(frame.bottom());
                if x > frame.x && x < frame.right() && bottom > top {
                    lines.push(Line {
                        from: Point::new(x - frame.x, top - frame.y),
                        to: Point::new(x - frame.x, bottom - frame.y),
                    });
                }
            }
            Axis::Rows => {
                let y = piece.y;
                let left = current.x.max(frame.x);
                let right = current.right().min(frame.right());
                if y > frame.y && y < frame.bottom() && right > left {
                    lines.push(Line {
                        from: Point::new(left - frame.x, y - frame.y),
                        to: Point::new(right - frame.x, y - frame.y),
                    });
                }
            }
        }
    }
    lines
}

/// 確定したセルの外側に出すキー。次に打つ向きは左から、その次の向きは上から並べる。
fn edge_keys(nav: &Nav, current: Rect, edge_size: f64) -> Vec<(char, Point, bool)> {
    let keys = nav.keys();
    let depth = nav.depth();
    let gap = edge_size / 2.0 + 2.0;
    let mut out = Vec::new();
    for (axis, next) in [(nav.axis_at(depth), true), (nav.axis_at(depth + 1), false)] {
        for (index, piece) in current.split4(axis).iter().enumerate() {
            let center = match axis {
                Axis::Cols => Point::new(piece.center().x, current.y - gap),
                Axis::Rows => Point::new(current.x - gap, piece.center().y),
            };
            out.push((keys[index], center, next));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{Controller, Effect};
    use crate::geometry::testing::*;
    use crate::monitors::Scope;

    fn nav_of(frames: &[Rect]) -> Nav {
        let monitors: Vec<_> = frames.iter().map(|&f| mon(f)).collect();
        Nav::new(&monitors, Scope::AllMonitors, Point::new(0.0, 0.0), MAX_CELL)
    }

    fn type_str(nav: &mut Nav, s: &str) {
        for c in s.chars() {
            assert!(nav.push(c));
        }
    }

    fn one(nav: &Nav, config: &Config) -> Scene {
        let mut scenes = scenes(nav, config);
        assert_eq!(scenes.len(), 1);
        scenes.remove(0)
    }

    #[test]
    fn single_monitor_at_start() {
        let config = Config::default();
        let scene = one(&nav_of(&[M]), &config);
        assert_eq!(scene.labels.len(), 256);
        let first = &scene.labels[0];
        assert_eq!(first.strong, 'j');
        assert_eq!(first.rest, "jjj");
        assert!(point_approx(first.origin, Point::new(2.0, 2.0)));
        assert!(scene.dims.is_empty());
        assert_eq!(scene.faint_lines.len(), 30);
        let xs: Vec<f64> = scene.strong_lines.iter().map(|l| l.from.x).collect();
        assert_eq!(xs, vec![360.0, 720.0, 1080.0]);
    }

    #[test]
    fn after_ll_dims_and_lines() {
        let config = Config::default();
        let mut nav = nav_of(&[M]);
        type_str(&mut nav, "ll");
        let scene = one(&nav, &config);
        assert_eq!(scene.dims.len(), 4);
        let expected = [
            Rect::new(0.0, 0.0, 1440.0, 450.0),
            Rect::new(0.0, 675.0, 1440.0, 225.0),
            Rect::new(0.0, 450.0, 720.0, 225.0),
            Rect::new(1080.0, 450.0, 360.0, 225.0),
        ];
        for (got, want) in scene.dims.iter().zip(expected) {
            assert!(rect_approx(*got, want));
        }
        assert_eq!(scene.labels.len(), 16);
        assert!(scene.labels.iter().all(|l| l.rest.chars().count() == 1));
        assert!(scene.labels.iter().all(|l| l.next));
        let xs: Vec<f64> = scene.strong_lines.iter().map(|l| l.from.x).collect();
        assert_eq!(xs, vec![810.0, 900.0, 990.0]);
    }

    #[test]
    fn determined_cell_shows_edge_keys() {
        let config = Config::default();
        let mut nav = nav_of(&[M]);
        type_str(&mut nav, "llk;");
        let scene = one(&nav, &config);
        assert!(scene.labels.is_empty());
        assert_eq!(scene.edge_keys.len(), 8);

        let above: Vec<&EdgeKey> = scene.edge_keys.iter().filter(|k| k.next).collect();
        assert_eq!(above.iter().map(|k| k.key).collect::<String>(), "jkl;");
        assert!(above.iter().all(|k| k.center.y < 618.75));
        assert!(above.windows(2).all(|w| w[0].center.x < w[1].center.x));

        let left: Vec<&EdgeKey> = scene.edge_keys.iter().filter(|k| !k.next).collect();
        assert!(left.iter().all(|k| k.center.x < 810.0));
        assert!(left.windows(2).all(|w| w[0].center.y < w[1].center.y));
    }

    #[test]
    fn edge_keys_swap_after_one_more_key() {
        let config = Config::default();
        let mut nav = nav_of(&[M]);
        type_str(&mut nav, "llk;;");
        let scene = one(&nav, &config);
        let next_left: Vec<&EdgeKey> = scene.edge_keys.iter().filter(|k| k.next).collect();
        assert!(next_left.iter().all(|k| k.center.x < 877.5));
        let after_above: Vec<&EdgeKey> = scene.edge_keys.iter().filter(|k| !k.next).collect();
        assert!(after_above.iter().all(|k| k.center.y < 618.75));
    }

    #[test]
    fn label_size_fits_cell() {
        let mut config = Config::default();
        config.style.label_size = 14.0;
        assert_eq!(one(&nav_of(&[M]), &config).label_size, 14.0);

        config.style.label_size = 40.0;
        let shrunk = one(&nav_of(&[M]), &config).label_size;
        assert!(approx(shrunk, 82.0 / 2.4));
    }

    #[test]
    fn corners_layout() {
        let mut config = Config::default();
        config.style.label_layout = LabelLayout::Corners;
        let nav = nav_of(&[M]);
        let scene = one(&nav, &config);
        assert_eq!(scene.labels.len(), 1024);
        assert_eq!(scene.labels.iter().filter(|l| l.next).count(), 256);

        // セル llk; の4隅（セル x 810..900, y 618.75..675）
        let corners: Vec<&Label> = scene
            .labels
            .iter()
            .filter(|l| approx(l.origin.x, 812.0) || approx(l.origin.x, 898.0))
            .filter(|l| l.origin.y > 618.0 && l.origin.y < 675.0)
            .filter(|l| l.origin.x > 810.0 && l.origin.x < 900.0)
            .collect();
        assert_eq!(corners.len(), 4);
        let anchors: Vec<Anchor> = corners.iter().map(|l| l.anchor).collect();
        assert!(anchors.contains(&Anchor::TopLeft));
        assert!(anchors.contains(&Anchor::TopRight));
        assert!(anchors.contains(&Anchor::BottomLeft));
        assert!(anchors.contains(&Anchor::BottomRight));
        assert!(point_approx(
            corners.iter().find(|l| l.anchor == Anchor::BottomRight).unwrap().origin,
            Point::new(898.0, 673.0)
        ));

        let mut after_ll = nav_of(&[M]);
        type_str(&mut after_ll, "ll");
        let scene = one(&after_ll, &config);
        assert_eq!(scene.labels.len(), 32);
        assert_eq!(scene.labels.iter().filter(|l| l.next).count(), 16);
        assert!(scene
            .labels
            .iter()
            .filter(|l| l.next)
            .all(|l| l.anchor == Anchor::BottomLeft));
    }

    #[test]
    fn corners_falls_back_for_five_chars() {
        let mut config = Config::default();
        config.style.label_layout = LabelLayout::Corners;
        let all = scenes(&nav_of(&[MB, EXT_R]), &config);
        assert!(all
            .iter()
            .flat_map(|s| &s.labels)
            .all(|l| l.anchor == Anchor::TopLeft));
    }

    #[test]
    fn mixed_monitors_at_start() {
        let config = Config::default();
        let nav = nav_of(&[MB, EXT_R]);
        let scenes = scenes(&nav, &config);
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].labels.len(), 264);
        assert_eq!(scenes[1].labels.len(), 645);
        // 各シーンの座標はモニターの左上が原点
        assert!(scenes[0].labels.iter().all(|l| l.origin.x >= 0.0 && l.origin.y >= 0.0));
        assert!(scenes[1].labels.iter().all(|l| l.origin.x >= 0.0 && l.origin.y >= 0.0));
        assert!(scenes[1].labels.iter().all(|l| l.origin.x < 2560.0));
    }

    #[test]
    fn mixed_monitors_after_k_and_l() {
        let config = Config::default();
        let mut nav = nav_of(&[MB, EXT_R]);
        type_str(&mut nav, "k");
        let after_k = scenes(&nav, &config);
        assert_eq!(after_k[0].labels.len(), 88);
        assert_eq!(after_k[1].labels.len(), 133);
        assert_eq!(after_k[0].dims.len(), 1);
        assert!(rect_approx(after_k[0].dims[0], Rect::new(0.0, 0.0, 1018.0, 982.0)));

        let mut nav = nav_of(&[MB, EXT_R]);
        type_str(&mut nav, "l");
        let after_l = scenes(&nav, &config);
        assert_eq!(after_l[0].dims.len(), 1);
        assert!(rect_approx(after_l[0].dims[0], Rect::new(0.0, 0.0, 1512.0, 982.0)));
        assert!(after_l[0].labels.is_empty());
    }

    #[test]
    fn uppercase_labels() {
        let mut config = Config::default();
        config.style.uppercase = true;
        let scene = one(&nav_of(&[M]), &config);
        assert_eq!(scene.labels[0].strong, 'J');
        assert_eq!(scene.labels[0].rest, "JJJ");
    }

    #[test]
    fn controller_uses_same_scene() {
        // 起動直後の表示が Controller の Show と同じ内容になること
        let config = Config::default();
        let mut controller = Controller::new(config.clone()).unwrap();
        let effects = controller.activate(|| (vec![mon(M)], Point::new(0.0, 0.0)));
        assert_eq!(effects, vec![Effect::Show(scenes(&nav_of(&[M]), &config))]);
    }
}
