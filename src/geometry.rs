//! 座標と矩形。CG のグローバル座標（左上原点、y は下向き）で扱う。

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 分ける向き。`Cols` は縦帯（x を4等分）、`Rows` は横帯（y を4等分）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Cols,
    Rows,
}

impl Axis {
    pub fn other(self) -> Self {
        match self {
            Axis::Cols => Axis::Rows,
            Axis::Rows => Axis::Cols,
        }
    }
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn area(&self) -> f64 {
        self.w * self.h
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    /// 向き `axis` に4等分した矩形を、左から（上から）順に返す。
    pub fn split4(&self, axis: Axis) -> [Rect; 4] {
        match axis {
            Axis::Cols => {
                let w = self.w / 4.0;
                std::array::from_fn(|i| Rect::new(self.x + w * i as f64, self.y, w, self.h))
            }
            Axis::Rows => {
                let h = self.h / 4.0;
                std::array::from_fn(|i| Rect::new(self.x, self.y + h * i as f64, self.w, h))
            }
        }
    }

    /// 点 `p` が `split4(axis)` の何番目に入るか（端は内側に寄せる）。
    pub fn piece_index(&self, axis: Axis, p: Point) -> usize {
        let (offset, size) = match axis {
            Axis::Cols => (p.x - self.x, self.w),
            Axis::Rows => (p.y - self.y, self.h),
        };
        (offset / (size / 4.0)).floor().clamp(0.0, 3.0) as usize
    }
}

/// CG 座標（左上原点）と AppKit 座標（左下原点）の変換。どちらも同じ式で、互いの逆になる。
/// `primary_h` はプライマリモニターの高さ。
pub fn cg_to_ns(r: Rect, primary_h: f64) -> Rect {
    Rect::new(r.x, primary_h - (r.y + r.h), r.w, r.h)
}

pub fn ns_to_cg(r: Rect, primary_h: f64) -> Rect {
    cg_to_ns(r, primary_h)
}

#[cfg(test)]
pub(crate) mod testing {
    use crate::geometry::{Point, Rect};
    use crate::monitors::Monitor;

    pub const M: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);
    pub const MB: Rect = Rect::new(0.0, 0.0, 1512.0, 982.0);
    pub const EXT_R: Rect = Rect::new(1512.0, -458.0, 2560.0, 1440.0);
    pub const EXT_TOP: Rect = Rect::new(-300.0, -1440.0, 2560.0, 1440.0);
    pub const C3: Rect = Rect::new(4072.0, -458.0, 2560.0, 1440.0);
    pub const BIG: Rect = Rect::new(0.0, 0.0, 2560.0, 1440.0);
    pub const MAX_CELL: (f64, f64) = (180.0, 100.0);

    pub fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    pub fn rect_approx(a: Rect, b: Rect) -> bool {
        approx(a.x, b.x) && approx(a.y, b.y) && approx(a.w, b.w) && approx(a.h, b.h)
    }

    pub fn point_approx(a: Point, b: Point) -> bool {
        approx(a.x, b.x) && approx(a.y, b.y)
    }

    pub fn mon(frame: Rect) -> Monitor {
        Monitor {
            id: 0,
            frame,
            scale: 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn split4_cols_and_rows() {
        let r = Rect::new(0.0, 0.0, 400.0, 200.0);
        let cols = r.split4(Axis::Cols);
        let xs: Vec<f64> = cols.iter().map(|c| c.x).collect();
        assert_eq!(xs, vec![0.0, 100.0, 200.0, 300.0]);
        assert!(cols.iter().all(|c| approx(c.w, 100.0) && approx(c.h, 200.0)));

        let rows = r.split4(Axis::Rows);
        let ys: Vec<f64> = rows.iter().map(|c| c.y).collect();
        assert_eq!(ys, vec![0.0, 50.0, 100.0, 150.0]);
        assert!(rows.iter().all(|c| approx(c.h, 50.0) && approx(c.w, 400.0)));
    }

    #[test]
    fn piece_index_matches_split() {
        let r = Rect::new(0.0, 0.0, 400.0, 200.0);
        assert_eq!(r.piece_index(Axis::Cols, Point::new(0.0, 0.0)), 0);
        assert_eq!(r.piece_index(Axis::Cols, Point::new(250.0, 0.0)), 2);
        assert_eq!(r.piece_index(Axis::Cols, Point::new(400.0, 0.0)), 3);
        assert_eq!(r.piece_index(Axis::Rows, Point::new(0.0, 199.0)), 3);
    }

    #[test]
    fn cg_and_ns_conversion() {
        assert!(rect_approx(
            cg_to_ns(EXT_R, 982.0),
            Rect::new(1512.0, 0.0, 2560.0, 1440.0)
        ));
        assert!(rect_approx(
            cg_to_ns(EXT_TOP, 982.0),
            Rect::new(-300.0, 982.0, 2560.0, 1440.0)
        ));
        assert!(rect_approx(ns_to_cg(cg_to_ns(EXT_R, 982.0), 982.0), EXT_R));
        assert!(rect_approx(
            ns_to_cg(cg_to_ns(EXT_TOP, 982.0), 982.0),
            EXT_TOP
        ));
    }

    #[test]
    fn contains_is_half_open() {
        assert!(M.contains(Point::new(0.0, 0.0)));
        assert!(!M.contains(Point::new(1440.0, 0.0)));
        assert!(!M.contains(Point::new(-1.0, 0.0)));
    }
}
