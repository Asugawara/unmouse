//! 文字の並びでセルを選ぶ。全モニターを囲む矩形を、列と行を交互に4等分して分ける。

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

#[derive(Debug, Clone)]
pub struct Nav {
    monitors: Vec<Monitor>,
    active: Vec<usize>,
    canvas: Rect,
    axes: Vec<Axis>,
    keys: [char; 4],
    typed: String,
    /// 入力1文字ごとの範囲。`regions.len()` が入力済みの文字数。
    regions: Vec<Rect>,
}

impl Nav {
    pub fn new(monitors: &[Monitor], scope: Scope, cursor: Point, max_cell: (f64, f64)) -> Self {
        let active = active_indices(monitors, scope, cursor);
        let canvas = bounding(active.iter().map(|&i| monitors[i].frame));
        Self {
            monitors: monitors.to_vec(),
            active,
            canvas,
            axes: plan_axes(canvas, max_cell),
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

    pub fn canvas(&self) -> Rect {
        self.canvas
    }

    pub fn axes(&self) -> Vec<Axis> {
        self.axes.clone()
    }

    /// ラベルの文字数（葉までの深さ）。
    pub fn leaf_depth(&self) -> usize {
        self.axes.len()
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

    /// 深さ `depth` で分けるときの向き。葉より深いときは、最後の向きから交互に続ける。
    pub fn axis_at(&self, depth: usize) -> Axis {
        if let Some(&axis) = self.axes.get(depth) {
            return axis;
        }
        let Some(&last) = self.axes.last() else {
            return Axis::Cols;
        };
        if (depth - self.axes.len()).is_multiple_of(2) {
            last.other()
        } else {
            last
        }
    }

    /// 葉の列数と行数。
    pub fn grid_size(&self) -> (usize, usize) {
        let cols = self.axes.iter().filter(|a| **a == Axis::Cols).count();
        let rows = self.axes.len() - cols;
        (4usize.pow(cols as u32), 4usize.pow(rows as u32))
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
        self.regions.last().copied().unwrap_or(self.canvas)
    }

    /// 入力された文字が指す矩形（モニターで切らない）。何も入力していなければ `None`。
    pub fn region(&self) -> Option<Rect> {
        self.regions.last().copied()
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
        self.collect_labels(self.canvas, &mut path, 0, &mut out);
        out
    }

    /// キーを1つ入力する。範囲が対象のモニターと重ならないとき、または分けた辺が 1pt 未満のときは拒否する。
    pub fn push(&mut self, c: char) -> bool {
        let Some(index) = self.keys.iter().position(|&k| k == c) else {
            return false;
        };
        let axis = self.axis_at(self.depth());
        let piece = self.current().split4(axis)[index];
        let side = match axis {
            Axis::Cols => piece.w,
            Axis::Rows => piece.h,
        };
        if side < 1.0 || self.best_piece(piece).is_none() {
            return false;
        }
        self.typed.push(c);
        self.regions.push(piece);
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

    /// 点を指すラベル（葉の深さの文字列）。点がどのモニターの中にもなければ `None`。
    pub fn path_to(&self, p: Point) -> Option<String> {
        if !self.active.iter().any(|&i| self.monitors[i].frame.contains(p)) {
            return None;
        }
        let mut rect = self.canvas;
        let mut path = String::new();
        for &axis in &self.axes {
            let index = rect.piece_index(axis, p);
            path.push(self.keys[index]);
            rect = rect.split4(axis)[index];
        }
        Some(path)
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

    fn collect_labels(&self, rect: Rect, path: &mut String, depth: usize, out: &mut Vec<LabelCell>) {
        if depth == self.axes.len() {
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
        for (index, child) in rect.split4(self.axes[depth]).into_iter().enumerate() {
            if self.best_piece(child).is_none() {
                continue;
            }
            path.push(self.keys[index]);
            self.collect_labels(child, path, depth + 1, out);
            path.pop();
        }
    }
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
}
