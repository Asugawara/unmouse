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
    Center,
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
    let current = nav.current();
    let typed = nav.typed();
    let labels = nav.labels();
    let (cell_w, _) = nav.cell_size();
    let label_size = fit_label_size(style, cell_w, nav.label_len_range().1);
    let edge = if nav.is_determined() {
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
            labels: labels_for(&labels, monitor, frame, typed, style.label_layout, style.uppercase),
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

/// 文字幅の見積もり（(文字数 − 1 + next_scale) × 0.6 × 大きさ + 4）がセルの幅に収まるように、必要なら縮める。
/// 次に打つ1文字だけ `next_scale` 倍になるので、その分を足している。
fn fit_label_size(style: &Style, cell_w: f64, chars: usize) -> f64 {
    if chars == 0 {
        return style.label_size;
    }
    let per_char = 0.6 * (chars as f64 - 1.0 + style.next_scale);
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
    layout: LabelLayout,
    upper: bool,
) -> Vec<Label> {
    let n = typed.chars().count();
    let mut out = Vec::new();
    for cell in cells.iter().filter(|c| {
        c.monitor == monitor && c.label.starts_with(typed) && c.label.chars().count() > n
    }) {
        let piece = cell.piece;
        let chars: Vec<char> = cell.label.chars().collect();
        if layout == LabelLayout::Corners && chars.len() == 4 {
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
            // inline はセルの左上、それ以外（center と 4文字でない corners）は中央
            let (at, anchor) = if layout == LabelLayout::Inline {
                (Point::new(piece.x + 2.0, piece.y + 2.0), Anchor::TopLeft)
            } else {
                (piece.center(), Anchor::Center)
            };
            out.push(Label {
                strong: cased(chars[n], upper),
                rest: chars[n + 1..].iter().map(|&c| cased(c, upper)).collect(),
                origin: local(at, frame),
                anchor,
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

/// 次に分ける境界（範囲の内側）を、モニターの中で切って返す。同じ直線上でつながる線分は1本にまとめる。
fn strong_lines(nav: &Nav, frame: Rect, current: Rect) -> Vec<Line> {
    const EPS: f64 = 1e-6;
    // (座標, 始点, 終点)
    let mut vertical: Vec<(f64, f64, f64)> = Vec::new();
    let mut horizontal: Vec<(f64, f64, f64)> = Vec::new();
    for (_, c) in nav.children() {
        for x in [c.x, c.right()] {
            if x > current.x + EPS && x < current.right() - EPS {
                vertical.push((x, c.y, c.bottom()));
            }
        }
        for y in [c.y, c.bottom()] {
            if y > current.y + EPS && y < current.bottom() - EPS {
                horizontal.push((y, c.x, c.right()));
            }
        }
    }
    let mut lines = Vec::new();
    for (at, from, to) in merge_segments(vertical, EPS) {
        let (top, bottom) = (from.max(frame.y), to.min(frame.bottom()));
        if at > frame.x && at < frame.right() && bottom > top {
            lines.push(Line {
                from: Point::new(at - frame.x, top - frame.y),
                to: Point::new(at - frame.x, bottom - frame.y),
            });
        }
    }
    for (at, from, to) in merge_segments(horizontal, EPS) {
        let (left, right) = (from.max(frame.x), to.min(frame.right()));
        if at > frame.y && at < frame.bottom() && right > left {
            lines.push(Line {
                from: Point::new(left - frame.x, at - frame.y),
                to: Point::new(right - frame.x, at - frame.y),
            });
        }
    }
    lines
}

/// 同じ座標で重なる・つながる線分を1本にまとめる。座標、始点の順に並べて返す。
fn merge_segments(mut segments: Vec<(f64, f64, f64)>, eps: f64) -> Vec<(f64, f64, f64)> {
    segments.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(f64, f64, f64)> = Vec::new();
    for (at, from, to) in segments {
        match out.last_mut() {
            Some(last) if (last.0 - at).abs() < eps && from <= last.2 + eps => {
                last.2 = last.2.max(to);
            }
            _ => out.push((at, from, to)),
        }
    }
    out
}

/// 確定したセルの外側に出すキー。次に打つ向きは左から、その次の向きは上から並べる。
fn edge_keys(nav: &Nav, current: Rect, edge_size: f64) -> Vec<(char, Point, bool)> {
    let Some(axis) = nav.refine_axis() else {
        return Vec::new();
    };
    let keys = nav.keys();
    let gap = edge_size / 2.0 + 2.0;
    let mut out = Vec::new();
    for (axis, next) in [(axis, true), (axis.other(), false)] {
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
    use crate::config::Subdivide;
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
        assert!(point_approx(first.origin, Point::new(45.0, 28.125)));
        assert_eq!(first.anchor, Anchor::Center);
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
        config.style.next_scale = 1.0;
        config.style.label_size = 14.0;
        assert_eq!(one(&nav_of(&[M]), &config).label_size, 14.0);

        config.style.label_size = 40.0;
        let shrunk = one(&nav_of(&[M]), &config).label_size;
        assert!(approx(shrunk, 82.0 / 2.4));
    }

    #[test]
    fn label_size_fits_cell_with_next_scale() {
        let mut config = Config::default();
        assert_eq!(config.style.next_scale, 1.5);
        config.style.label_size = 14.0;
        assert_eq!(one(&nav_of(&[M]), &config).label_size, 14.0);

        // 4 文字（うち次の1文字は 1.5 倍）: (90 - 8) / (0.6 * 4.5) ≈ 30.37 に縮む
        config.style.label_size = 40.0;
        let shrunk = one(&nav_of(&[M]), &config).label_size;
        assert!(approx(shrunk, 82.0 / 2.7));
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
            .all(|l| l.anchor == Anchor::Center));
    }

    #[test]
    fn inline_layout_puts_label_at_top_left() {
        let mut config = Config::default();
        config.style.label_layout = LabelLayout::Inline;
        let scene = one(&nav_of(&[M]), &config);
        assert_eq!(scene.labels.len(), 256);
        let first = &scene.labels[0];
        assert!(point_approx(first.origin, Point::new(2.0, 2.0)));
        assert_eq!(first.anchor, Anchor::TopLeft);
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
        let mut config = Config::default();
        config.grid.subdivide = Subdivide::Off;
        let mut controller = Controller::new(config.clone()).unwrap();
        let effects = controller.activate(|| (vec![mon(M)], Point::new(0.0, 0.0)));
        assert_eq!(effects, vec![Effect::Show(scenes(&nav_of(&[M]), &config))]);
    }

    fn half_nav() -> Nav {
        nav_of(&[M]).with_subdivide(Subdivide::Half)
    }

    fn is_vertical(l: &Line) -> bool {
        approx(l.from.x, l.to.x)
    }

    #[test]
    fn half_at_start() {
        let config = Config::default();
        let scene = one(&half_nav(), &config);
        assert_eq!(scene.labels.len(), 512);
        assert_eq!(scene.faint_lines.iter().filter(|l| is_vertical(l)).count(), 31);
        assert_eq!(scene.faint_lines.iter().filter(|l| !is_vertical(l)).count(), 15);
        let xs: Vec<f64> = scene.strong_lines.iter().map(|l| l.from.x).collect();
        assert_eq!(xs, vec![360.0, 720.0, 1080.0]);
    }

    #[test]
    fn half_after_llk() {
        let config = Config::default();
        let mut nav = half_nav();
        type_str(&mut nav, "llk");
        let scene = one(&nav, &config);
        assert_eq!(scene.labels.len(), 8);
        assert!(scene.labels.iter().all(|l| l.next));
        let expected = [
            Rect::new(0.0, 0.0, 1440.0, 450.0),
            Rect::new(0.0, 675.0, 1440.0, 225.0),
            Rect::new(0.0, 450.0, 810.0, 225.0),
            Rect::new(900.0, 450.0, 540.0, 225.0),
        ];
        assert_eq!(scene.dims.len(), 4);
        for (got, want) in scene.dims.iter().zip(expected) {
            assert!(rect_approx(*got, want));
        }
        assert_eq!(scene.strong_lines.len(), 2);
        let v = scene.strong_lines.iter().find(|l| is_vertical(l)).unwrap();
        assert!(approx(v.from.x, 855.0) && approx(v.from.y, 450.0) && approx(v.to.y, 675.0));
        let h = scene.strong_lines.iter().find(|l| !is_vertical(l)).unwrap();
        assert!(approx(h.from.y, 506.25) && approx(h.from.x, 810.0) && approx(h.to.x, 900.0));
    }

    #[test]
    fn half_after_llk_semicolon() {
        let config = Config::default();
        let mut nav = half_nav();
        type_str(&mut nav, "llk;");
        let scene = one(&nav, &config);
        assert_eq!(scene.strong_lines.len(), 2);
        let mut ys: Vec<f64> = scene.strong_lines.iter().map(|l| l.from.y).collect();
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(approx(ys[0], 562.5) && approx(ys[1], 618.75));
        assert!(scene
            .strong_lines
            .iter()
            .all(|l| approx(l.from.x, 855.0) && approx(l.to.x, 900.0)));
    }

    #[test]
    fn label_size_uses_the_longest_label() {
        // 2台の half は 5 文字と 6 文字。セルの幅は 63.625、6 文字で見積もる
        let mut config = Config::default();
        config.style.next_scale = 1.0;
        config.style.label_size = 40.0;
        let nav = nav_of(&[MB, EXT_R]).with_subdivide(Subdivide::Half);
        let size = scenes(&nav, &config)[0].label_size;
        assert!(approx(size, (63.625 - 8.0) / (0.6 * 6.0)), "{size}");
        // 一様（5 文字）ならもう少し大きい
        let off = scenes(&nav_of(&[MB, EXT_R]), &config)[0].label_size;
        assert!(approx(off, (63.625 - 8.0) / (0.6 * 5.0)), "{off}");
        assert!(off > size);
    }

    #[test]
    fn label_size_keeps_setting_when_it_fits_and_never_goes_below_min() {
        // 収まるなら設定のまま
        let config = Config::default();
        let nav = nav_of(&[MB, EXT_R]).with_subdivide(Subdivide::Half);
        assert_eq!(scenes(&nav, &config)[0].label_size, 11.0);
        // 6 文字 × 0.6 × 大きさ + 4 <= セル幅 - 4 の境目: 大きさ (63.625-8)/3.6
        let mut edge = Config::default();
        edge.style.next_scale = 1.0;
        edge.style.label_size = (63.625 - 8.0) / 3.6;
        assert!(approx(scenes(&nav, &edge)[0].label_size, edge.style.label_size));
        // 細いセルでも min_label_size を下回らない（22.5 幅 × 5 文字は 4.83 になるところ）
        let mut tiny = Config::default();
        tiny.style.label_size = 20.0;
        tiny.style.min_label_size = 8.0;
        let quarter = nav_of(&[M]).with_subdivide(Subdivide::Quarter);
        assert_eq!(scenes(&quarter, &tiny)[0].label_size, 8.0);
        tiny.style.min_label_size = 9.5;
        assert_eq!(scenes(&quarter, &tiny)[0].label_size, 9.5);
    }

    #[test]
    fn small_screen_scene_has_no_labels() {
        let config = Config::default();
        let small = Rect::new(0.0, 0.0, 150.0, 80.0);
        let scene = one(&nav_of(&[small]), &config);
        assert!(scene.labels.is_empty());
        assert_eq!(scene.label_size, config.style.label_size);
        assert!(scene.dims.is_empty());
    }

    #[test]
    fn cursor_monitor_scope_gives_one_scene_in_local_coordinates() {
        let monitors = [mon(MB), mon(EXT_R)];
        let nav = Nav::new(&monitors, Scope::CursorMonitor, Point::new(2000.0, 0.0), MAX_CELL);
        let all = scenes(&nav, &Config::default());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].monitor, 1);
        assert_eq!(all[0].labels.len(), 256);
        // 左上のセルの中心はモニターの左上から (2560/16/2, 1440/16/2)
        assert!(point_approx(all[0].labels[0].origin, Point::new(80.0, 45.0)));
    }

    #[test]
    fn inline_layout_after_typing_keeps_remaining_chars() {
        let mut config = Config::default();
        config.style.label_layout = LabelLayout::Inline;
        let mut nav = nav_of(&[M]);
        type_str(&mut nav, "ll");
        let scene = one(&nav, &config);
        assert_eq!(scene.labels.len(), 16);
        assert!(scene.labels.iter().all(|l| l.anchor == Anchor::TopLeft && l.next));
        // ll の範囲 (720,450,360,225) の先頭セルは (720,450,90,56.25)。残りは 2 文字
        let first = &scene.labels[0];
        assert_eq!(first.strong, 'j');
        assert_eq!(first.rest, "j");
        assert!(point_approx(first.origin, Point::new(722.0, 452.0)));
    }

    #[test]
    fn corners_layout_uppercase_and_next_marks() {
        let mut config = Config::default();
        config.style.label_layout = LabelLayout::Corners;
        config.style.uppercase = true;
        let mut nav = nav_of(&[M]);
        type_str(&mut nav, "llk");
        let scene = one(&nav, &config);
        // 残り 1 文字のセルが 4 つ。各セルの角は 1 つだけ
        assert_eq!(scene.labels.len(), 4);
        assert!(scene.labels.iter().all(|l| l.next && l.rest.is_empty()));
        assert!(scene.labels.iter().all(|l| l.anchor == Anchor::BottomRight));
        assert_eq!(
            scene.labels.iter().map(|l| l.strong).collect::<String>(),
            "JKL;"
        );
    }

    #[test]
    fn corners_layout_in_half_mode_uses_centered_labels() {
        // 5文字と6文字のラベルは 4 隅に置けないので、中央に出す
        let mut config = Config::default();
        config.style.label_layout = LabelLayout::Corners;
        let nav = nav_of(&[MB, EXT_R]).with_subdivide(Subdivide::Half);
        let all = scenes(&nav, &config);
        assert!(all.iter().flat_map(|s| &s.labels).all(|l| l.anchor == Anchor::Center));
        assert!(all.iter().all(|s| !s.labels.is_empty()));
    }

    #[test]
    fn faint_lines_stay_inside_each_monitor() {
        let nav = nav_of(&[MB, EXT_R]);
        let all = scenes(&nav, &Config::default());
        let frames = [MB, EXT_R];
        for (scene, frame) in all.iter().zip(frames) {
            assert!(!scene.faint_lines.is_empty());
            for l in &scene.faint_lines {
                for p in [l.from, l.to] {
                    assert!(p.x >= 0.0 && p.x <= frame.w + 1e-6, "{p:?}");
                    assert!(p.y >= 0.0 && p.y <= frame.h + 1e-6, "{p:?}");
                }
                // 縦線は上端から下端まで、横線は左端から右端まで
                if is_vertical(l) {
                    assert!(approx(l.from.y, 0.0) && approx(l.to.y, frame.h));
                } else {
                    assert!(approx(l.from.x, 0.0) && approx(l.to.x, frame.w));
                }
            }
        }
        // 64 列 × 16 行のうち、MB の幅 1512 に入る縦線は 23 本（63.625 刻み）
        let verticals = all[0].faint_lines.iter().filter(|l| is_vertical(l)).count();
        assert_eq!(verticals, 23);
    }

    #[test]
    fn strong_lines_are_clipped_per_monitor() {
        // 起動直後: 外接矩形 (0,-458,4072,1440) を列に 4 等分する境界 x=1018,2036,3054
        let all = scenes(&nav_of(&[MB, EXT_R]), &Config::default());
        let in_mb: Vec<&Line> = all[0].strong_lines.iter().collect();
        assert_eq!(in_mb.len(), 1);
        assert!(approx(in_mb[0].from.x, 1018.0) && approx(in_mb[0].to.x, 1018.0));
        // MB は y 0..982 だけ（キャンバスの上の 458 は MB の外）
        assert!(approx(in_mb[0].from.y, 0.0) && approx(in_mb[0].to.y, 982.0));

        let in_ext: Vec<&Line> = all[1].strong_lines.iter().collect();
        assert_eq!(in_ext.len(), 2);
        let mut xs: Vec<f64> = in_ext.iter().map(|l| l.from.x).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(approx(xs[0], 2036.0 - 1512.0) && approx(xs[1], 3054.0 - 1512.0));
        assert!(in_ext.iter().all(|l| approx(l.from.y, 0.0) && approx(l.to.y, 1440.0)));
    }

    #[test]
    fn dims_cover_the_monitor_minus_the_region() {
        let config = Config::default();
        for typed in ["", "l", "ll", "llk", "llk;"] {
            let mut nav = nav_of(&[M]);
            type_str(&mut nav, typed);
            let scene = one(&nav, &config);
            let dimmed: f64 = scene.dims.iter().map(|r| r.area()).sum();
            assert!(approx(dimmed + nav.current().area(), M.area()), "{typed:?}");
            // 暗くする矩形どうしは重ならない
            for (i, a) in scene.dims.iter().enumerate() {
                for b in &scene.dims[i + 1..] {
                    assert!(crate::monitors::intersect(*a, *b).is_none(), "{typed:?}");
                }
            }
        }
    }

    #[test]
    fn dims_for_monitor_outside_the_region_cover_it_entirely() {
        let config = Config::default();
        let mut nav = nav_of(&[MB, EXT_R]);
        type_str(&mut nav, "l");
        let all = scenes(&nav, &config);
        // l = x 2036..3054 は EXT_R の中だけ。MB は全体を暗くする
        assert_eq!(all[0].dims.len(), 1);
        assert!(rect_approx(all[0].dims[0], Rect::new(0.0, 0.0, 1512.0, 982.0)));
        assert!(all[1].dims.len() > 1);
    }

    #[test]
    fn edge_keys_are_placed_in_the_monitor_that_contains_them() {
        let config = Config::default();
        let mut nav = nav_of(&[MB, EXT_R]);
        type_str(&mut nav, "kk;l;");
        assert!(nav.is_determined());
        let all = scenes(&nav, &config);
        assert!(all[0].edge_keys.is_empty());
        assert_eq!(all[1].edge_keys.len(), 8);
        // 座標はモニターの左上が原点。セルの 7.5pt 外側（キーの大きさ 11 の半分 + 2）に出る
        let region = nav.region().unwrap();
        let next: Vec<&EdgeKey> = all[1].edge_keys.iter().filter(|k| k.next).collect();
        assert_eq!(next.iter().map(|k| k.key).collect::<String>(), "jkl;");
        // 葉は縦長 (63.625 x 90) なので、次に打つ 4 つは行 = セルの左に縦に並ぶ
        for k in &next {
            assert!(approx(k.center.x, region.x - 7.5 - EXT_R.x), "{k:?}");
        }
        for (i, k) in next.iter().enumerate() {
            let want = region.y + region.h / 4.0 * (i as f64 + 0.5) - EXT_R.y;
            assert!(approx(k.center.y, want), "{k:?}");
        }
        let after: Vec<&EdgeKey> = all[1].edge_keys.iter().filter(|k| !k.next).collect();
        for k in &after {
            assert!(approx(k.center.y, region.y - 7.5 - EXT_R.y), "{k:?}");
        }
    }

    #[test]
    fn edge_key_size_changes_the_gap() {
        let mut config = Config::default();
        config.style.edge_key_size = 20.0;
        let mut nav = nav_of(&[M]);
        type_str(&mut nav, "llk;");
        let scene = one(&nav, &config);
        let above = scene.edge_keys.iter().find(|k| k.next).unwrap();
        // Off の葉 (810,618.75,90,56.25) は横長なので、次に打つ 4 つは上に並ぶ。隙間は 20/2 + 2
        assert!(approx(above.center.y, 618.75 - 12.0));
    }

    #[test]
    fn no_edge_keys_until_determined() {
        let mut nav = nav_of(&[M]);
        for c in "llk".chars() {
            assert!(nav.push(c));
            assert!(one(&nav, &Config::default()).edge_keys.is_empty());
        }
    }

    #[test]
    fn merge_segments_joins_touching_and_overlapping_only() {
        let merged = merge_segments(
            vec![
                (10.0, 50.0, 80.0),
                (10.0, 0.0, 30.0),
                (10.0, 30.0, 50.0), // つながる
                (10.0, 90.0, 100.0), // 隙間があるので別
                (20.0, 0.0, 100.0),  // 座標が違うので別
                (10.0, 95.0, 120.0), // 重なる
            ],
            1e-6,
        );
        assert_eq!(
            merged,
            vec![(10.0, 0.0, 80.0), (10.0, 90.0, 120.0), (20.0, 0.0, 100.0)]
        );
        assert!(merge_segments(Vec::new(), 1e-6).is_empty());
    }

    #[test]
    fn fit_label_size_zero_chars_keeps_setting() {
        let style = Style::default();
        assert_eq!(fit_label_size(&style, 10.0, 0), style.label_size);
    }
}
