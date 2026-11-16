//! Rectangles: physical-pixel `Rect` for Win32, CSS-pixel `RectF` from the frontend.

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::RECT;

/// A rectangle in physical pixels, with exclusive right and bottom like `RECT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub const fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// True when `other` lies entirely inside `self`.
    pub const fn contains_rect(&self, other: &Rect) -> bool {
        other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    pub const fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(
            self.left + dx,
            self.top + dy,
            self.right + dx,
            self.bottom + dy,
        )
    }

    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let r = Rect::new(
            self.left.max(other.left),
            self.top.max(other.top),
            self.right.min(other.right),
            self.bottom.min(other.bottom),
        );
        (!r.is_empty()).then_some(r)
    }
}

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Rect::new(r.left, r.top, r.right, r.bottom)
    }
}

impl From<Rect> for RECT {
    fn from(r: Rect) -> Self {
        RECT {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

/// A rectangle in CSS pixels, relative to the dock window's client area.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct RectF {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RectF {
    /// Convert to physical pixels, growing the rectangle by `margin` CSS pixels
    /// on every side and rounding outward so nothing is clipped.
    pub fn to_physical(self, scale: f64, margin: f64) -> Rect {
        Rect::new(
            ((self.x - margin) * scale).floor() as i32,
            ((self.y - margin) * scale).floor() as i32,
            ((self.x + self.width + margin) * scale).ceil() as i32,
            ((self.y + self.height + margin) * scale).ceil() as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_containment() {
        let outer = Rect::new(0, 0, 2560, 1600);
        let inner = Rect::new(0, 1492, 2560, 1600);
        assert_eq!(inner.width(), 2560);
        assert_eq!(inner.height(), 108);
        assert!(outer.contains_rect(&inner));
        assert!(!inner.contains_rect(&outer));
    }

    #[test]
    fn intersect_clips_or_returns_none() {
        let a = Rect::new(0, 0, 100, 100);
        assert_eq!(
            a.intersect(&Rect::new(50, 50, 200, 200)),
            Some(Rect::new(50, 50, 100, 100))
        );
        assert_eq!(a.intersect(&Rect::new(100, 0, 200, 100)), None);
    }

    #[test]
    fn css_rect_scales_and_rounds_outward() {
        let r = RectF {
            x: 10.5,
            y: 20.0,
            width: 100.0,
            height: 52.0,
        };
        // 150% scaling, no margin: 15.75 floors to 15, 165.75 ceils to 166.
        assert_eq!(r.to_physical(1.5, 0.0), Rect::new(15, 30, 166, 108));
        // A 4px margin grows each side by 6 physical pixels at 150%.
        assert_eq!(r.to_physical(1.5, 4.0), Rect::new(9, 24, 172, 114));
    }
}
