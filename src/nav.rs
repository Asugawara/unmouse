//! 文字の並びでセルを選ぶ。全モニターを囲む矩形を、列と行を交互に分けていく。
//! 既定の `Half` では、最後の段だけセルを半分にして、5文字と6文字のラベルを混ぜる。

use crate::config::Subdivide;
use crate::geometry::{Axis, Point, Rect};
use crate::monitors::{Monitor, Scope, active_indices, bounding, intersect};

/// 既定のキー（上から順に j k l ;）。
pub const DEFAULT_KEYS: [char; 4] = ['j', 'k', 'l', ';'];

#[derive(Debug, Clone, PartialEq)]
pub struct LabelCell {
    pub label: String,
    /// セル全体（モニターで切らない）。
    pub cell: Rect,
    /// 重なる面積が最大のモニター（`monitors` への添字）。
    pub monitor: usize,
    /// セルとそのモニターの重なり。
    pub piece: Rect,
}

/// 矩形を次にどう分けるか。
#[derive(Debug, Clone, Copy, PartialEq)]
enum Node {
    /// `axes[depth]` の向きに4等分する。
    Quad(usize),
    /// 最後の向き `A` に4等分した先頭の帯を `O` の向きに半分にした2つが葉。
    /// 残りの3帯を `O` の向きに半分にした2つは、`Tri`。
    Special,
    /// `A` の向きに3等分した3つが葉（4つ目のキーは使わない）。
    Tri,
    /// 葉。決まった後の細分は、この向きに4等分し、次からは交互にする。
    Refine(Axis),
}

#[derive(Debug, Clone)]
pub struct Nav {
    monitors: Vec<Monitor>,
    active: Vec<usize>,
    canvas: Rect,
    /// `plan_axes` で決まった向き。
    base_axes: Vec<Axis>,
    /// `Quarter` では `base_axes` に1つ足した向き。
    axes: Vec<Axis>,
    half: bool,
    keys: [char; 4],
    typed: String,
    /// 入力1文字ごとの範囲と、その範囲の分け方。`regions.len()` が入力済みの文字数。
    regions: Vec<(Rect, Node)>,
}

impl Nav {
    pub fn new(monitors: &[Monitor], scope: Scope, cursor: Point, max_cell: (f64, f64)) -> Self {
        let active = active_indices(monitors, scope, cursor);
        let canvas = bounding(active.iter().map(|&i| monitors[i].frame));
        let base_axes = plan_axes(canvas, max_cell);
        Self {
            monitors: monitors.to_vec(),
            active,
            canvas,
            axes: base_axes.clone(),
            base_axes,
            half: false,
            keys: DEFAULT_KEYS,
            typed: String::new(),
            regions: Vec::new(),
        }
    }

    /// 既定以外のキーを使う（上の列から順に、列と行のキーになる）。
    pub fn with_keys(mut self, keys: [char; 4]) -> Self {
        self.keys = keys;
        self
    }

    /// セルの分け方を選ぶ。`Off` は一様、`Half` は最後の段を半分、`Quarter` は4等分の段を1つ足す。
    pub fn with_subdivide(mut self, s: Subdivide) -> Self {
        self.axes = self.base_axes.clone();
        self.half = s == Subdivide::Half;
        if s == Subdivide::Quarter {
            let next = match self.axes.last() {
                Some(last) => last.other(),
                None => Axis::Cols,
            };
            self.axes.push(next);
        }
        self
    }

    pub fn canvas(&self) -> Rect {
        self.canvas
    }

    pub fn axes(&self) -> Vec<Axis> {
        self.axes.clone()
    }

    /// ラベルの最短と最長の文字数。
    pub fn label_len_range(&self) -> (usize, usize) {
        let n = self.axes.len();
        if self.half && n >= 1 {
            (n, n + 1)
        } else {
            (n, n)
        }
    }

    pub fn keys(&self) -> [char; 4] {
        self.keys
    }

    pub fn typed(&self) -> &str {
        &self.typed
    }

    pub fn depth(&self) -> usize {
        self.regions.len()
    }

    /// 入力済みの文字で葉に届いたか（届いた後の細分の入力も含む）。
    pub fn is_determined(&self) -> bool {
        matches!(self.current_node().1, Node::Refine(_))
    }

    /// 葉に届いた後、次に4等分する向き。葉に届いていなければ `None`。
    pub fn refine_axis(&self) -> Option<Axis> {
        match self.current_node().1 {
            Node::Refine(axis) => Some(axis),
            _ => None,
        }
    }

    /// 今の範囲を次に分ける子（キーと矩形）。葉に届いた後は細分の4つ。
    pub fn children(&self) -> Vec<(char, Rect)> {
        let (rect, node) = self.current_node();
        self.kids(rect, node)
            .into_iter()
            .map(|(index, child, _)| (self.keys[index], child))
            .collect()
    }

    /// 葉のセルの列数と行数（`Half` では最後の向きの反対側が2倍）。
    pub fn grid_size(&self) -> (usize, usize) {
        let cols = self.axes.iter().filter(|a| **a == Axis::Cols).count();
        let rows = self.axes.len() - cols;
        let (mut c, mut r) = (4usize.pow(cols as u32), 4usize.pow(rows as u32));
        if self.half
            && let Some(last) = self.axes.last()
        {
            match last.other() {
                Axis::Cols => c *= 2,
                Axis::Rows => r *= 2,
            }
        }
        (c, r)
    }

    /// 葉のセルの大きさ（全モニターで同じ）。
    pub fn cell_size(&self) -> (f64, f64) {
        let (cols, rows) = self.grid_size();
        (self.canvas.w / cols as f64, self.canvas.h / rows as f64)
    }

    /// 対象のモニターの番号と枠。
    pub fn active(&self) -> Vec<(usize, Rect)> {
        self.active
            .iter()
            .map(|&i| (i, self.monitors[i].frame))
            .collect()
    }

    /// 今の範囲。何も入力していなければキャンバス全体。
    pub fn current(&self) -> Rect {
        self.regions.last().map(|r| r.0).unwrap_or(self.canvas)
    }

    /// 入力された文字が指す矩形（モニターで切らない）。何も入力していなければ `None`。
    pub fn region(&self) -> Option<Rect> {
        self.regions.last().map(|r| r.0)
    }

    /// 範囲と各モニターの重なりのうち最大のものの中心。
    pub fn target(&self) -> Option<Point> {
        let region = self.region()?;
        self.best_piece(region).map(|(_, piece)| piece.center())
    }

    /// 葉のすべてのセル。どのモニターとも重ならないセルは含めない。入力による絞り込みはしない。
    pub fn labels(&self) -> Vec<LabelCell> {
        let mut out = Vec::new();
        let mut path = String::new();
        self.collect_labels(self.canvas, self.node_at(0, self.canvas), &mut path, &mut out);
        out
    }

    /// キーを1つ入力する。範囲が対象のモニターと重ならないとき、または分けた辺が 1pt 未満のときは拒否する。
    pub fn push(&mut self, c: char) -> bool {
        let Some(index) = self.keys.iter().position(|&k| k == c) else {
            return false;
        };
        let (rect, node) = self.current_node();
        let Some((_, piece, next)) = self
            .kids(rect, node)
            .into_iter()
            .find(|(i, _, _)| *i == index)
        else {
            return false;
        };
        if piece.w < 1.0 || piece.h < 1.0 || self.best_piece(piece).is_none() {
            return false;
        }
        self.typed.push(c);
        self.regions.push((piece, next));
        true
    }

    /// 最後の1文字を取り消す。
    pub fn back(&mut self) -> bool {
        if self.regions.pop().is_some() {
            self.typed.pop();
            true
        } else {
            false
        }
    }

    /// 点を指すラベル（葉までの文字列）。点がどのモニターの中にもなければ `None`。
    pub fn path_to(&self, p: Point) -> Option<String> {
        if !self.active.iter().any(|&i| self.monitors[i].frame.contains(p)) {
            return None;
        }
        let mut rect = self.canvas;
        let mut node = self.node_at(0, rect);
        let mut path = String::new();
        while !matches!(node, Node::Refine(_)) {
            let kids = self.kids(rect, node);
            let (index, child, next) = kids
                .iter()
                .find(|(_, r, _)| r.contains(p))
                .or_else(|| {
                    kids.iter().min_by(|a, b| {
                        distance(a.1, p)
                            .partial_cmp(&distance(b.1, p))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                })
                .copied()?;
            path.push(self.keys[index]);
            rect = child;
            node = next;
        }
        Some(path)
    }

    fn current_node(&self) -> (Rect, Node) {
        self.regions
            .last()
            .copied()
            .unwrap_or_else(|| (self.canvas, self.node_at(0, self.canvas)))
    }

    /// 深さ `depth` に着いた矩形 `rect` の分け方。
    fn node_at(&self, depth: usize, rect: Rect) -> Node {
        let n = self.axes.len();
        if n == 0 {
            return Node::Refine(long_side(rect));
        }
        if depth >= n {
            Node::Refine(long_side(rect))
        } else if self.half && depth == n - 1 {
            Node::Special
        } else {
            Node::Quad(depth)
        }
    }

    /// `node` で分けた子（キーの番号、矩形、子の分け方）。
    fn kids(&self, rect: Rect, node: Node) -> Vec<(usize, Rect, Node)> {
        let last = self.axes.last().copied().unwrap_or(Axis::Cols);
        match node {
            Node::Quad(depth) => rect
                .split4(self.axes[depth])
                .into_iter()
                .enumerate()
                .map(|(i, c)| (i, c, self.node_at(depth + 1, c)))
                .collect(),
            Node::Special => {
                let other = last.other();
                let first = rect.split4(last)[0];
                let rest = match last {
                    Axis::Cols => Rect::new(rect.x + rect.w / 4.0, rect.y, rect.w * 0.75, rect.h),
                    Axis::Rows => Rect::new(rect.x, rect.y + rect.h / 4.0, rect.w, rect.h * 0.75),
                };
                let [a0, a1] = split2(first, other);
                let [b0, b1] = split2(rest, other);
                vec![
                    (0, a0, Node::Refine(long_side(a0))),
                    (1, a1, Node::Refine(long_side(a1))),
                    (2, b0, Node::Tri),
                    (3, b1, Node::Tri),
                ]
            }
            Node::Tri => split3(rect, last)
                .into_iter()
                .enumerate()
                .map(|(i, c)| (i, c, Node::Refine(long_side(c))))
                .collect(),
            Node::Refine(axis) => rect
                .split4(axis)
                .into_iter()
                .enumerate()
                .map(|(i, c)| (i, c, Node::Refine(axis.other())))
                .collect(),
        }
    }

    /// 矩形と重なる対象モニターのうち、重なりが最大のもの（モニターの番号と重なり）。
    fn best_piece(&self, rect: Rect) -> Option<(usize, Rect)> {
        let mut best: Option<(usize, Rect)> = None;
        for &i in &self.active {
            if let Some(piece) = intersect(rect, self.monitors[i].frame) {
                let better = match best {
                    None => true,
                    Some((_, current)) => piece.area() > current.area(),
                };
                if better {
                    best = Some((i, piece));
                }
            }
        }
        best
    }

    fn collect_labels(&self, rect: Rect, node: Node, path: &mut String, out: &mut Vec<LabelCell>) {
        if matches!(node, Node::Refine(_)) {
            if let Some((monitor, piece)) = self.best_piece(rect) {
                out.push(LabelCell {
                    label: path.clone(),
                    cell: rect,
                    monitor,
                    piece,
                });
            }
            return;
        }
        for (index, child, next) in self.kids(rect, node) {
            if self.best_piece(child).is_none() {
                continue;
            }
            path.push(self.keys[index]);
            self.collect_labels(child, next, path, out);
            path.pop();
        }
    }
}

/// 長い辺の向きに分ける（正方形と横長は列、縦長は行）。
fn long_side(r: Rect) -> Axis {
    if r.w >= r.h { Axis::Cols } else { Axis::Rows }
}

/// 向き `axis` に2等分する（`Cols` は左右、`Rows` は上下）。
fn split2(r: Rect, axis: Axis) -> [Rect; 2] {
    match axis {
        Axis::Cols => [
            Rect::new(r.x, r.y, r.w / 2.0, r.h),
            Rect::new(r.x + r.w / 2.0, r.y, r.w / 2.0, r.h),
        ],
        Axis::Rows => [
            Rect::new(r.x, r.y, r.w, r.h / 2.0),
            Rect::new(r.x, r.y + r.h / 2.0, r.w, r.h / 2.0),
        ],
    }
}

/// 向き `axis` に3等分する。
fn split3(r: Rect, axis: Axis) -> [Rect; 3] {
    match axis {
        Axis::Cols => std::array::from_fn(|i| {
            Rect::new(r.x + r.w / 3.0 * i as f64, r.y, r.w / 3.0, r.h)
        }),
        Axis::Rows => std::array::from_fn(|i| {
            Rect::new(r.x, r.y + r.h / 3.0 * i as f64, r.w, r.h / 3.0)
        }),
    }
}

/// 点から矩形までの距離（中なら 0）。
fn distance(r: Rect, p: Point) -> f64 {
    let dx = (r.x - p.x).max(0.0).max(p.x - r.right());
    let dy = (r.y - p.y).max(0.0).max(p.y - r.bottom());
    dx.hypot(dy)
}

/// 列と行を交互に分ける向きを決める。片方の向きが `max_cell` 以下になったら、もう一方だけを分ける。
fn plan_axes(canvas: Rect, max_cell: (f64, f64)) -> Vec<Axis> {
    let (mut w, mut h) = (canvas.w, canvas.h);
    let mut want = Axis::Cols;
    let mut axes = Vec::new();
    while w > max_cell.0 || h > max_cell.1 {
        let axis = match want {
            Axis::Cols if w > max_cell.0 => Axis::Cols,
            Axis::Rows if h > max_cell.1 => Axis::Rows,
            _ => want.other(),
        };
        match axis {
            Axis::Cols => w /= 4.0,
            Axis::Rows => h /= 4.0,
        }
        axes.push(axis);
        want = axis.other();
    }
    axes
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::testing::*;

    fn nav_of(monitors: &[Monitor]) -> Nav {
        Nav::new(monitors, Scope::AllMonitors, Point::new(0.0, 0.0), MAX_CELL)
    }

    fn find<'a>(labels: &'a [LabelCell], label: &str) -> &'a LabelCell {
        labels.iter().find(|l| l.label == label).unwrap()
    }

    fn type_str(nav: &mut Nav, s: &str) {
        for c in s.chars() {
            assert!(nav.push(c), "push {c} should succeed");
        }
    }

    #[test]
    fn axes_pattern() {
        use Axis::{Cols as C, Rows as R};
        assert_eq!(nav_of(&[mon(M)]).axes(), vec![C, R, C, R]);
        assert_eq!(nav_of(&[mon(BIG)]).axes(), vec![C, R, C, R]);
        assert_eq!(nav_of(&[mon(MB), mon(EXT_R)]).axes(), vec![C, R, C, R, C]);
        assert_eq!(nav_of(&[mon(MB), mon(EXT_TOP)]).axes(), vec![C, R, C, R, R]);
        assert_eq!(
            nav_of(&[mon(MB), mon(EXT_R), mon(C3)]).axes(),
            vec![C, R, C, R, C]
        );
    }

    fn half_of(monitors: &[Monitor]) -> Nav {
        nav_of(monitors).with_subdivide(Subdivide::Half)
    }

    fn assert_prefix_free(labels: &[LabelCell]) {
        let mut names: Vec<&str> = labels.iter().map(|l| l.label.as_str()).collect();
        names.sort();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n, "duplicate labels");
        let sorted: Vec<&str> = {
            let mut v: Vec<&str> = labels.iter().map(|l| l.label.as_str()).collect();
            v.sort();
            v
        };
        for w in sorted.windows(2) {
            assert!(!w[1].starts_with(w[0]), "{} is a prefix of {}", w[0], w[1]);
        }
    }

    #[test]
    fn quarter_adds_one_axis() {
        use Axis::{Cols as C, Rows as R};
        let nav = nav_of(&[mon(M)]).with_subdivide(Subdivide::Quarter);
        assert_eq!(nav.axes(), vec![C, R, C, R, C]);
        let labels = nav.labels();
        assert_eq!(labels.len(), 1024);
        assert!(labels.iter().all(|l| l.label.chars().count() == 5));
        assert!(rect_approx(
            find(&labels, "jjjjj").cell,
            Rect::new(0.0, 0.0, 22.5, 56.25)
        ));
    }

    #[test]
    fn quarter_across_monitors_is_six() {
        use Axis::{Cols as C, Rows as R};
        let nav = nav_of(&[mon(MB), mon(EXT_R)]).with_subdivide(Subdivide::Quarter);
        assert_eq!(nav.axes(), vec![C, R, C, R, C, R]);
        let labels = nav.labels();
        assert!(labels.iter().all(|l| l.label.chars().count() == 6));
        // 2台の外接矩形は 4072×1440（既存テストの 63.625×90 から逆算）。64列×64行
        assert!(labels.iter().all(|l| approx(l.cell.w, 63.625) && approx(l.cell.h, 22.5)));
    }

    #[test]
    fn off_keeps_axes() {
        let base = nav_of(&[mon(MB), mon(EXT_R)]).axes();
        assert_eq!(
            nav_of(&[mon(MB), mon(EXT_R)]).with_subdivide(Subdivide::Off).axes(),
            base
        );
    }

    #[test]
    fn half_labels_single_monitor() {
        let nav = half_of(&[mon(M)]);
        let labels = nav.labels();
        assert_eq!(labels.len(), 512);
        assert_prefix_free(&labels);
        assert_eq!(labels.iter().filter(|l| l.label.chars().count() == 4).count(), 128);
        assert_eq!(labels.iter().filter(|l| l.label.chars().count() == 5).count(), 384);
        assert!(labels.iter().all(|l| approx(l.cell.w, 45.0) && approx(l.cell.h, 56.25)));
        assert_eq!(nav.label_len_range(), (4, 5));
        assert_eq!(nav.grid_size(), (32, 16));
        for (label, want) in [
            ("jjjj", Rect::new(0.0, 0.0, 45.0, 56.25)),
            ("jjjk", Rect::new(45.0, 0.0, 45.0, 56.25)),
            ("jjjlj", Rect::new(0.0, 56.25, 45.0, 56.25)),
            ("jjjll", Rect::new(0.0, 168.75, 45.0, 56.25)),
            ("jjj;j", Rect::new(45.0, 56.25, 45.0, 56.25)),
            ("jjj;l", Rect::new(45.0, 168.75, 45.0, 56.25)),
        ] {
            assert!(rect_approx(find(&labels, label).cell, want), "{label}");
        }
    }

    #[test]
    fn half_path_to_round_trips_every_label() {
        for frames in [vec![mon(M)], vec![mon(MB), mon(EXT_R)]] {
            let nav = half_of(&frames);
            for l in nav.labels() {
                assert_eq!(
                    nav.path_to(l.piece.center()).as_deref(),
                    Some(l.label.as_str()),
                    "{}",
                    l.label
                );
            }
        }
    }

    #[test]
    fn half_path_to_and_target() {
        let nav = half_of(&[mon(M)]);
        assert_eq!(nav.path_to(Point::new(892.8, 630.0)).as_deref(), Some("llk;l"));
        let mut nav = half_of(&[mon(M)]);
        type_str(&mut nav, "llk;l");
        assert!(nav.is_determined());
        assert!(point_approx(nav.target().unwrap(), Point::new(877.5, 646.875)));
    }

    #[test]
    fn half_progress_through_special_step() {
        let mut nav = half_of(&[mon(M)]);
        type_str(&mut nav, "llk");
        assert!(!nav.is_determined());
        assert!(rect_approx(nav.region().unwrap(), Rect::new(810.0, 450.0, 90.0, 225.0)));
        assert!(nav.push('j'));
        assert!(nav.is_determined());
        assert!(point_approx(nav.target().unwrap(), Point::new(832.5, 478.125)));
        assert!(nav.back());

        type_str(&mut nav, ";");
        assert!(!nav.is_determined());
        assert!(rect_approx(
            nav.region().unwrap(),
            Rect::new(855.0, 506.25, 45.0, 168.75)
        ));
        assert!(!nav.push(';'));
        assert_eq!(nav.typed(), "llk;");
        assert_eq!(nav.children().len(), 3);
    }

    #[test]
    fn half_across_monitors() {
        let nav = half_of(&[mon(MB), mon(EXT_R)]);
        assert_eq!(nav.label_len_range(), (5, 6));
        let labels = nav.labels();
        assert!(labels.iter().all(|l| matches!(l.label.chars().count(), 5 | 6)));
        assert!(labels.iter().any(|l| l.label.chars().count() == 5));
        assert!(labels.iter().any(|l| l.label.chars().count() == 6));
        assert_prefix_free(&labels);
        assert!(labels.iter().all(|l| approx(l.cell.w, 63.625) && approx(l.cell.h, 45.0)));
    }

    #[test]
    fn determined_cell_refines_by_long_side() {
        let mut nav = nav_of(&[mon(M)]);
        type_str(&mut nav, "llk;");
        assert!(nav.is_determined());
        assert_eq!(nav.refine_axis(), Some(Axis::Cols));
        let mut nav = nav_of(&[mon(MB), mon(EXT_R)]);
        type_str(&mut nav, "kk;l;");
        assert_eq!(nav.refine_axis(), Some(Axis::Rows));
    }

    #[test]
    fn labels_single_monitor() {
        let nav = nav_of(&[mon(M)]);
        let labels = nav.labels();
        assert_eq!(labels.len(), 256);
        assert!(labels.iter().all(|l| l.label.chars().count() == 4));
        let mut names: Vec<&str> = labels.iter().map(|l| l.label.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 256);

        assert!(rect_approx(
            find(&labels, "jjjj").cell,
            Rect::new(0.0, 0.0, 90.0, 56.25)
        ));
        assert!(rect_approx(
            find(&labels, ";;;;").cell,
            Rect::new(1350.0, 843.75, 90.0, 56.25)
        ));
        assert!(rect_approx(
            find(&labels, "llk;").cell,
            Rect::new(810.0, 618.75, 90.0, 56.25)
        ));
    }

    #[test]
    fn labels_across_monitors() {
        let nav = nav_of(&[mon(MB), mon(EXT_R)]);
        let labels = nav.labels();
        assert_eq!(labels.len(), 909);
        assert!(labels.iter().all(|l| l.label.chars().count() == 5));
        assert!(labels.iter().all(|l| approx(l.cell.w, 63.625) && approx(l.cell.h, 90.0)));
        assert_eq!(labels.iter().filter(|l| l.monitor == 0).count(), 264);
        assert_eq!(labels.iter().filter(|l| l.monitor == 1).count(), 645);

        // 列23・行5 は MB と EXT_R にまたがる。面積の大きい MB に出る
        let boundary = labels
            .iter()
            .find(|l| approx(l.cell.x, 1463.375) && approx(l.cell.y, -8.0))
            .unwrap();
        assert_eq!(boundary.monitor, 0);
        assert!(rect_approx(
            boundary.piece,
            Rect::new(1463.375, 0.0, 48.625, 82.0)
        ));
    }

    #[test]
    fn path_to_across_monitors() {
        let nav = nav_of(&[mon(MB), mon(EXT_R)]);
        assert_eq!(nav.path_to(Point::new(2000.0, 100.0)).as_deref(), Some("kk;l;"));

        let mut typed = nav_of(&[mon(MB), mon(EXT_R)]);
        type_str(&mut typed, "kk;l;");
        let target = typed.target().unwrap();
        assert!(point_approx(target, Point::new(2004.1875, 127.0)));
    }

    #[test]
    fn push_across_monitors_rejects_empty_region() {
        let mut nav = nav_of(&[mon(MB), mon(EXT_R)]);
        assert!(nav.push('j'));
        assert!(!nav.push('j'));
        assert!(rect_approx(
            nav.region().unwrap(),
            Rect::new(0.0, -458.0, 1018.0, 1440.0)
        ));
    }

    #[test]
    fn push_l_on_single_monitor() {
        let mut nav = nav_of(&[mon(M)]);
        assert!(nav.push('l'));
        assert!(rect_approx(nav.region().unwrap(), Rect::new(720.0, 0.0, 360.0, 900.0)));
        assert!(point_approx(nav.target().unwrap(), Point::new(900.0, 450.0)));
    }

    #[test]
    fn llk_then_semicolon_subdivides() {
        let mut nav = nav_of(&[mon(M)]);
        type_str(&mut nav, "llk;");
        assert!(point_approx(nav.target().unwrap(), Point::new(855.0, 646.875)));
        assert!(nav.push(';'));
        assert!(rect_approx(
            nav.region().unwrap(),
            Rect::new(877.5, 618.75, 22.5, 56.25)
        ));
        assert!(point_approx(nav.target().unwrap(), Point::new(888.75, 646.875)));
    }

    #[test]
    fn back_and_invalid_keys() {
        let mut nav = nav_of(&[mon(M)]);
        assert!(!nav.back());
        assert!(!nav.push('x'));
        assert!(nav.push('l'));
        assert!(nav.push('l'));
        assert_eq!(nav.typed(), "ll");
        assert!(nav.back());
        assert_eq!(nav.typed(), "l");
        assert!(nav.back());
        assert_eq!(nav.typed(), "");
        assert!(nav.region().is_none());
        assert!(!nav.back());
    }

    #[test]
    fn repeated_j_stops_before_forty() {
        let mut nav = nav_of(&[mon(M)]);
        let mut accepted = 0;
        while nav.push('j') {
            accepted += 1;
            assert!(accepted < 40, "push never rejected");
        }
        assert!(accepted < 40);
    }

    #[test]
    fn path_to_single_monitor() {
        let nav = nav_of(&[mon(M)]);
        assert_eq!(
            nav.path_to(Point::new(892.8, 630.0)).as_deref(),
            Some("llk;")
        );
        assert_eq!(nav.path_to(Point::new(2000.0, 100.0)), None);
    }

    fn all_modes() -> [Subdivide; 3] {
        [Subdivide::Off, Subdivide::Half, Subdivide::Quarter]
    }

    #[test]
    fn labels_are_unique_and_prefix_free_in_every_mode() {
        for frames in [
            vec![mon(M)],
            vec![mon(MB), mon(EXT_R)],
            vec![mon(MB), mon(EXT_TOP)],
            vec![mon(MB), mon(EXT_R), mon(C3)],
        ] {
            for mode in all_modes() {
                let nav = nav_of(&frames).with_subdivide(mode);
                assert_prefix_free(&nav.labels());
            }
        }
    }

    #[test]
    fn every_label_cell_overlaps_its_monitor() {
        for frames in [vec![mon(MB), mon(EXT_R)], vec![mon(MB), mon(EXT_TOP)]] {
            for mode in all_modes() {
                let nav = nav_of(&frames).with_subdivide(mode);
                for l in nav.labels() {
                    let frame = frames[l.monitor].frame;
                    assert!(l.piece.area() > 0.0, "{}", l.label);
                    assert!(intersect(l.cell, frame).is_some(), "{}", l.label);
                    // piece はセルとモニターの重なりそのもの
                    assert!(rect_approx(l.piece, intersect(l.cell, frame).unwrap()), "{}", l.label);
                    // 他のモニターとの重なりのほうが大きいことはない
                    for other in &frames {
                        if let Some(o) = intersect(l.cell, other.frame) {
                            assert!(o.area() <= l.piece.area() + 1e-6, "{}", l.label);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn typing_any_label_reaches_exactly_its_cell() {
        for frames in [
            vec![mon(M)],
            vec![mon(MB), mon(EXT_R)],
            vec![mon(MB), mon(EXT_TOP)],
        ] {
            for mode in all_modes() {
                let base = nav_of(&frames).with_subdivide(mode);
                for l in base.labels() {
                    let mut nav = base.clone();
                    for c in l.label.chars() {
                        assert!(
                            !nav.is_determined(),
                            "{mode:?} {}: 最後の文字の前に決まっている",
                            l.label
                        );
                        assert!(nav.push(c), "{mode:?} {}: {c} を入力できない", l.label);
                    }
                    assert!(nav.is_determined(), "{mode:?} {}", l.label);
                    assert!(rect_approx(nav.region().unwrap(), l.cell), "{mode:?} {}", l.label);
                    assert!(
                        point_approx(nav.target().unwrap(), l.piece.center()),
                        "{mode:?} {}",
                        l.label
                    );
                }
            }
        }
    }

    #[test]
    fn path_to_round_trips_in_every_mode() {
        for frames in [vec![mon(M)], vec![mon(MB), mon(EXT_R)], vec![mon(MB), mon(EXT_TOP)]] {
            for mode in all_modes() {
                let nav = nav_of(&frames).with_subdivide(mode);
                for l in nav.labels() {
                    assert_eq!(
                        nav.path_to(l.piece.center()).as_deref(),
                        Some(l.label.as_str()),
                        "{mode:?} {}",
                        l.label
                    );
                }
            }
        }
    }

    #[test]
    fn path_to_outside_every_monitor_is_none() {
        // MB と EXT_TOP の外接矩形の中だが、どのモニターにも入らない点
        let nav = nav_of(&[mon(MB), mon(EXT_TOP)]);
        assert!(nav.canvas().contains(Point::new(-100.0, 500.0)));
        assert_eq!(nav.path_to(Point::new(-100.0, 500.0)), None);
        // 半開区間: モニターの右端・下端は外
        let nav = nav_of(&[mon(M)]);
        assert_eq!(nav.path_to(Point::new(1440.0, 10.0)), None);
        assert_eq!(nav.path_to(Point::new(10.0, 900.0)), None);
        assert!(nav.path_to(Point::new(1439.9, 899.9)).is_some());
    }

    #[test]
    fn small_screen_has_no_axes_and_is_already_determined() {
        let small = Rect::new(0.0, 0.0, 150.0, 80.0);
        for mode in [Subdivide::Off, Subdivide::Half] {
            let nav = nav_of(&[mon(small)]).with_subdivide(mode);
            assert!(nav.axes().is_empty(), "{mode:?}");
            assert_eq!(nav.label_len_range(), (0, 0), "{mode:?}");
            assert_eq!(nav.grid_size(), (1, 1), "{mode:?}");
            assert!(nav.is_determined(), "{mode:?}");
            // 何も入力していないので target はない
            assert!(nav.target().is_none());
            // 決まったあとの細分は長い辺（横長なので列）から
            assert_eq!(nav.refine_axis(), Some(Axis::Cols));
            assert_eq!(nav.children().len(), 4);
        }
    }

    #[test]
    fn small_tall_screen_refines_by_rows() {
        // max_cell (180,100) 以下の縦長の画面。葉の長い辺は縦なので、細分は行から
        let tall = Rect::new(0.0, 0.0, 80.0, 90.0);
        let nav = nav_of(&[mon(tall)]);
        assert!(nav.axes().is_empty());
        assert_eq!(nav.refine_axis(), Some(Axis::Rows));
    }

    #[test]
    fn small_screen_refines_by_typing() {
        let small = Rect::new(0.0, 0.0, 160.0, 80.0);
        let mut nav = nav_of(&[mon(small)]);
        assert!(nav.push('l'));
        assert!(rect_approx(nav.region().unwrap(), Rect::new(80.0, 0.0, 40.0, 80.0)));
        // 次は行
        assert_eq!(nav.refine_axis(), Some(Axis::Rows));
        assert!(nav.push(';'));
        assert!(rect_approx(nav.region().unwrap(), Rect::new(80.0, 60.0, 40.0, 20.0)));
    }

    #[test]
    fn determined_cell_keeps_refining_and_alternates() {
        let mut nav = nav_of(&[mon(M)]);
        type_str(&mut nav, "llk;");
        let cell = nav.region().unwrap();
        // 決まった直後の子は、長い辺（横長なので列）の4等分
        let kids = nav.children();
        assert_eq!(kids.iter().map(|k| k.0).collect::<String>(), "jkl;");
        for ((_, rect), want) in kids.iter().zip(cell.split4(Axis::Cols)) {
            assert!(rect_approx(*rect, want));
        }
        // 1つ入力すると向きが入れ替わる
        assert!(nav.push('l'));
        assert_eq!(nav.refine_axis(), Some(Axis::Rows));
        let region = nav.region().unwrap();
        for ((_, rect), want) in nav.children().iter().zip(region.split4(Axis::Rows)) {
            assert!(rect_approx(*rect, want));
        }
        assert!(nav.push('j'));
        assert_eq!(nav.refine_axis(), Some(Axis::Cols));
        assert!(nav.is_determined());
        // 戻ると向きも戻る
        assert!(nav.back());
        assert_eq!(nav.refine_axis(), Some(Axis::Rows));
        assert!(nav.back());
        assert_eq!(nav.refine_axis(), Some(Axis::Cols));
        assert_eq!(nav.typed(), "llk;");
        // 葉の手前まで戻ると決まっていない
        assert!(nav.back());
        assert!(!nav.is_determined());
        assert_eq!(nav.refine_axis(), None);
    }

    #[test]
    fn children_at_start_split_the_canvas_by_the_first_axis() {
        let nav = nav_of(&[mon(M)]);
        let kids = nav.children();
        assert_eq!(kids.iter().map(|k| k.0).collect::<String>(), "jkl;");
        for ((_, rect), want) in kids.iter().zip(nav.canvas().split4(nav.axes()[0])) {
            assert!(rect_approx(*rect, want));
        }
        // 次の段は行
        let mut nav = nav_of(&[mon(M)]);
        assert!(nav.push('l'));
        let region = nav.region().unwrap();
        for ((_, rect), want) in nav.children().iter().zip(region.split4(Axis::Rows)) {
            assert!(rect_approx(*rect, want));
        }
    }

    #[test]
    fn custom_keys_label_and_select_cells() {
        let nav = nav_of(&[mon(M)]).with_keys(['a', 'b', 'c', 'd']);
        assert_eq!(nav.keys(), ['a', 'b', 'c', 'd']);
        let labels = nav.labels();
        assert!(labels.iter().all(|l| l.label.chars().all(|c| "abcd".contains(c))));
        // 既定のキーでの jjjj / llk; に当たるセルは、aaaa / ccbd
        assert!(rect_approx(find(&labels, "aaaa").cell, Rect::new(0.0, 0.0, 90.0, 56.25)));
        assert!(rect_approx(find(&labels, "ccbd").cell, Rect::new(810.0, 618.75, 90.0, 56.25)));
        assert_eq!(nav.path_to(Point::new(892.8, 630.0)).as_deref(), Some("ccbd"));

        let mut nav = nav;
        assert!(!nav.push('j'), "既定のキーは使えない");
        assert!(nav.push('c'));
        assert!(rect_approx(nav.region().unwrap(), Rect::new(720.0, 0.0, 360.0, 900.0)));
    }

    #[test]
    fn cursor_monitor_scope_limits_labels_and_path() {
        let monitors = [mon(MB), mon(EXT_R)];
        let nav = Nav::new(&monitors, Scope::CursorMonitor, Point::new(2000.0, 0.0), MAX_CELL);
        assert!(rect_approx(nav.canvas(), EXT_R));
        assert_eq!(nav.active().len(), 1);
        assert_eq!(nav.active()[0].0, 1);
        assert_eq!(nav.axes(), vec![Axis::Cols, Axis::Rows, Axis::Cols, Axis::Rows]);
        let labels = nav.labels();
        assert_eq!(labels.len(), 256);
        assert!(labels.iter().all(|l| l.monitor == 1));
        // 対象外のモニターの上の点は指せない
        assert_eq!(nav.path_to(Point::new(100.0, 100.0)), None);
        assert!(nav.path_to(Point::new(2000.0, 100.0)).is_some());
    }

    #[test]
    fn cursor_monitor_scope_falls_back_to_first_monitor() {
        let monitors = [mon(MB), mon(EXT_R)];
        let nav = Nav::new(&monitors, Scope::CursorMonitor, Point::new(-9999.0, 0.0), MAX_CELL);
        assert!(rect_approx(nav.canvas(), MB));
        assert_eq!(nav.active()[0].0, 0);
    }

    #[test]
    fn all_monitors_active_lists_every_monitor_with_its_frame() {
        let nav = nav_of(&[mon(MB), mon(EXT_R)]);
        let active = nav.active();
        assert_eq!(active.len(), 2);
        assert_eq!((active[0].0, active[1].0), (0, 1));
        assert!(rect_approx(active[0].1, MB) && rect_approx(active[1].1, EXT_R));
    }

    #[test]
    fn cell_size_is_canvas_over_grid() {
        let nav = nav_of(&[mon(M)]);
        assert_eq!(nav.grid_size(), (16, 16));
        assert_eq!(nav.cell_size(), (90.0, 56.25));
        let nav = nav_of(&[mon(M)]).with_subdivide(Subdivide::Quarter);
        assert_eq!(nav.grid_size(), (64, 16));
        assert_eq!(nav.cell_size(), (22.5, 56.25));
        let nav = nav_of(&[mon(MB), mon(EXT_R)]);
        assert_eq!(nav.grid_size(), (64, 16));
        let (w, h) = nav.cell_size();
        assert!(approx(w, 63.625) && approx(h, 90.0));
        let nav = half_of(&[mon(MB), mon(EXT_R)]);
        assert_eq!(nav.grid_size(), (64, 32));
    }

    #[test]
    fn half_back_unwinds_every_step() {
        let mut nav = half_of(&[mon(M)]);
        type_str(&mut nav, "llk;l");
        assert!(nav.is_determined());
        for want in ["llk;", "llk", "ll", "l", ""] {
            assert!(nav.back());
            assert_eq!(nav.typed(), want);
            assert!(!nav.is_determined());
        }
        assert!(!nav.back());
        assert!(nav.region().is_none());
    }

    #[test]
    fn half_tri_node_splits_into_three_equal_rows() {
        let mut nav = half_of(&[mon(M)]);
        type_str(&mut nav, "llk;");
        let region = nav.region().unwrap();
        let kids = nav.children();
        assert_eq!(kids.iter().map(|k| k.0).collect::<String>(), "jkl");
        for (i, (_, rect)) in kids.iter().enumerate() {
            assert!(approx(rect.h, region.h / 3.0) && approx(rect.w, region.w));
            assert!(approx(rect.y, region.y + region.h / 3.0 * i as f64));
        }
        // 3つとも葉
        for c in ['j', 'k', 'l'] {
            let mut n = nav.clone();
            assert!(n.push(c));
            assert!(n.is_determined());
        }
    }

    #[test]
    fn half_special_node_children_match_rule() {
        // llk の矩形 (810,450,90,225) を、j: 上の帯の左半分、k: 上の帯の右半分、
        // l/;: 残り3帯の左/右半分
        let mut nav = half_of(&[mon(M)]);
        type_str(&mut nav, "llk");
        let kids = nav.children();
        let want = [
            ('j', Rect::new(810.0, 450.0, 45.0, 56.25)),
            ('k', Rect::new(855.0, 450.0, 45.0, 56.25)),
            ('l', Rect::new(810.0, 506.25, 45.0, 168.75)),
            (';', Rect::new(855.0, 506.25, 45.0, 168.75)),
        ];
        assert_eq!(kids.len(), 4);
        for ((key, rect), (wkey, wrect)) in kids.iter().zip(want) {
            assert_eq!(*key, wkey);
            assert!(rect_approx(*rect, wrect), "{key}");
        }
    }

    #[test]
    fn quarter_on_small_screen_adds_one_axis() {
        let small = Rect::new(0.0, 0.0, 150.0, 80.0);
        let quarter = nav_of(&[mon(small)]).with_subdivide(Subdivide::Quarter);
        assert_eq!(quarter.axes(), vec![Axis::Cols]);
        assert_eq!(quarter.label_len_range(), (1, 1));
        assert_eq!(quarter.labels().len(), 4);
    }

    #[test]
    fn half_repeated_push_stops() {
        let mut nav = half_of(&[mon(M)]);
        let mut accepted = 0;
        while nav.push('j') {
            accepted += 1;
            assert!(accepted < 40, "push never rejected");
        }
    }

    #[test]
    fn label_len_range_per_mode() {
        let one = [mon(M)];
        let two = [mon(MB), mon(EXT_R)];
        assert_eq!(nav_of(&one).label_len_range(), (4, 4));
        assert_eq!(nav_of(&one).with_subdivide(Subdivide::Off).label_len_range(), (4, 4));
        assert_eq!(nav_of(&one).with_subdivide(Subdivide::Half).label_len_range(), (4, 5));
        assert_eq!(nav_of(&one).with_subdivide(Subdivide::Quarter).label_len_range(), (5, 5));
        assert_eq!(nav_of(&two).label_len_range(), (5, 5));
        assert_eq!(nav_of(&two).with_subdivide(Subdivide::Half).label_len_range(), (5, 6));
        assert_eq!(nav_of(&two).with_subdivide(Subdivide::Quarter).label_len_range(), (6, 6));
    }

    #[test]
    fn with_subdivide_can_be_switched_back_to_off() {
        let nav = nav_of(&[mon(M)]).with_subdivide(Subdivide::Quarter).with_subdivide(Subdivide::Off);
        assert_eq!(nav.axes().len(), 4);
        assert_eq!(nav.labels().len(), 256);
        let nav = nav_of(&[mon(M)]).with_subdivide(Subdivide::Half).with_subdivide(Subdivide::Off);
        assert_eq!(nav.label_len_range(), (4, 4));
    }

    #[test]
    fn push_rejects_cells_thinner_than_one_point_in_every_mode() {
        for mode in all_modes() {
            let mut nav = nav_of(&[mon(M)]).with_subdivide(mode);
            let mut accepted = 0;
            while nav.push('j') {
                accepted += 1;
                assert!(accepted < 40, "{mode:?}");
            }
            let r = nav.region().unwrap();
            assert!(r.w >= 1.0 && r.h >= 1.0, "{mode:?}: 受け付けた領域が 1pt 未満 {r:?}");
        }
    }
}
