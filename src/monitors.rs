//! モニターの配置と、操作の対象になる範囲（キャンバス）。

use serde::Deserialize;

use crate::geometry::{Point, Rect};

/// 操作の対象にするモニターの範囲。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// 全モニターを1枚の画面とみなす。
    #[default]
    AllMonitors,
    /// カーソルのあるモニターだけで分ける。
    CursorMonitor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub id: u32,
    pub frame: Rect,
    pub scale: f64,
}

/// 2つの矩形の重なり。面積がないときは `None`。
pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
}

/// 矩形の外接矩形。空なら原点の 0 サイズ。
pub fn bounding(rects: impl IntoIterator<Item = Rect>) -> Rect {
    let mut rects = rects.into_iter();
    let Some(first) = rects.next() else {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.right(), first.bottom());
    for r in rects {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.right());
        y1 = y1.max(r.bottom());
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// 対象になるモニターの番号。`CursorMonitor` でカーソルがどのモニターにもなければ先頭を使う。
pub fn active_indices(monitors: &[Monitor], scope: Scope, cursor: Point) -> Vec<usize> {
    match scope {
        Scope::AllMonitors => (0..monitors.len()).collect(),
        Scope::CursorMonitor => monitors
            .iter()
            .position(|m| m.frame.contains(cursor))
            .or((!monitors.is_empty()).then_some(0))
            .into_iter()
            .collect(),
    }
}

/// 操作の対象全体を囲む矩形。
pub fn canvas(monitors: &[Monitor], scope: Scope, cursor: Point) -> Rect {
    bounding(
        active_indices(monitors, scope, cursor)
            .into_iter()
            .map(|i| monitors[i].frame),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::testing::*;

    #[test]
    fn canvas_covers_all_monitors() {
        let all = [mon(MB), mon(EXT_R)];
        assert!(rect_approx(
            canvas(&all, Scope::AllMonitors, Point::new(0.0, 0.0)),
            Rect::new(0.0, -458.0, 4072.0, 1440.0)
        ));
        let top = [mon(MB), mon(EXT_TOP)];
        assert!(rect_approx(
            canvas(&top, Scope::AllMonitors, Point::new(0.0, 0.0)),
            Rect::new(-300.0, -1440.0, 2560.0, 2422.0)
        ));
    }

    #[test]
    fn cursor_monitor_scope() {
        let all = [mon(MB), mon(EXT_R)];
        assert!(rect_approx(
            canvas(&all, Scope::CursorMonitor, Point::new(2000.0, 0.0)),
            EXT_R
        ));
        // カーソルがどのモニターにもなければ先頭（MB）を使う
        assert!(rect_approx(
            canvas(&all, Scope::CursorMonitor, Point::new(-5000.0, 0.0)),
            MB
        ));
    }

    #[test]
    fn intersect_area_and_touching() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(rect_approx(
            intersect(a, Rect::new(5.0, 5.0, 10.0, 10.0)).unwrap(),
            Rect::new(5.0, 5.0, 5.0, 5.0)
        ));
        assert!(intersect(a, Rect::new(10.0, 0.0, 5.0, 5.0)).is_none());
    }

    #[test]
    fn intersect_disjoint_corner_and_contained() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        // 離れている
        assert!(intersect(a, Rect::new(20.0, 20.0, 5.0, 5.0)).is_none());
        // 角だけ接する、下辺だけ接する
        assert!(intersect(a, Rect::new(10.0, 10.0, 5.0, 5.0)).is_none());
        assert!(intersect(a, Rect::new(0.0, 10.0, 10.0, 5.0)).is_none());
        // 完全に含まれる（順序を入れ替えても同じ）
        let inner = Rect::new(2.0, 3.0, 4.0, 5.0);
        assert!(rect_approx(intersect(a, inner).unwrap(), inner));
        assert!(rect_approx(intersect(inner, a).unwrap(), inner));
        // EXT_TOP は MB の真上で接するだけ
        assert!(intersect(EXT_TOP, MB).is_none());
    }

    #[test]
    fn bounding_empty_single_and_negative() {
        assert!(rect_approx(bounding([]), Rect::new(0.0, 0.0, 0.0, 0.0)));
        assert!(rect_approx(bounding([EXT_R]), EXT_R));
        assert!(rect_approx(
            bounding([MB, EXT_R, C3]),
            Rect::new(0.0, -458.0, 6632.0, 1440.0)
        ));
    }

    #[test]
    fn active_indices_by_scope() {
        let ms = [mon(MB), mon(EXT_R)];
        let origin = Point::new(0.0, 0.0);
        assert_eq!(active_indices(&ms, Scope::AllMonitors, origin), vec![0, 1]);
        assert_eq!(active_indices(&ms, Scope::CursorMonitor, origin), vec![0]);
        assert_eq!(
            active_indices(&ms, Scope::CursorMonitor, Point::new(2000.0, 0.0)),
            vec![1]
        );
        // 境目の x=1512 は右のモニターに属する（半開区間）
        assert_eq!(
            active_indices(&ms, Scope::CursorMonitor, Point::new(1512.0, 0.0)),
            vec![1]
        );
        // どのモニターにもなければ先頭
        assert_eq!(
            active_indices(&ms, Scope::CursorMonitor, Point::new(-5000.0, 0.0)),
            vec![0]
        );
    }

    #[test]
    fn no_monitors_gives_empty_canvas() {
        let origin = Point::new(0.0, 0.0);
        assert!(active_indices(&[], Scope::AllMonitors, origin).is_empty());
        assert!(active_indices(&[], Scope::CursorMonitor, origin).is_empty());
        assert!(rect_approx(
            canvas(&[], Scope::CursorMonitor, origin),
            Rect::new(0.0, 0.0, 0.0, 0.0)
        ));
    }
}
