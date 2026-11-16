//! Where a wallpaper image lands on a monitor, for each Windows fit mode.
//!
//! Pure integer math with no Windows calls, so it can be tested exhaustively.
//! All rectangles are in physical pixels. "Monitor-relative" means the
//! monitor's top-left corner is (0, 0).

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;

/// The wallpaper position setting (Settings > Personalisation > Background).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    Center,
    Tile,
    Stretch,
    Fit,
    Fill,
    Span,
}

/// How the image is drawn on one monitor: scaled to `width` x `height`, with
/// its top-left corner at (`x`, `y`) in monitor-relative coordinates. Negative
/// coordinates mean the image is cropped. With `tile`, the scaled image
/// repeats in both directions from that corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub tile: bool,
}

/// One copy operation: take `src` (a rectangle of the scaled image) and put
/// its top-left corner at (`dst_x`, `dst_y`) in the target rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blit {
    pub src: Rect,
    pub dst_x: i32,
    pub dst_y: i32,
}

fn scaled(size: u32, factor: f64) -> u32 {
    ((f64::from(size) * factor).round() as u32).max(1)
}

/// Centre a span of `inner` pixels inside `outer` pixels. Rounds toward the
/// start, as Windows does, so odd differences put the extra pixel at the end.
fn centred(outer: i32, inner: i32) -> i32 {
    (outer - inner).div_euclid(2)
}

/// Place an `image` (width, height) on `monitor`. `virtual_desktop` is the
/// bounding rectangle of all monitors, which Span and Tile are anchored to.
/// Returns `None` for an empty image or monitor.
pub fn place(fit: Fit, image: (u32, u32), monitor: Rect, virtual_desktop: Rect) -> Option<Placement> {
    let (iw, ih) = image;
    let (mw, mh) = (monitor.width(), monitor.height());
    if iw == 0 || ih == 0 || mw <= 0 || mh <= 0 {
        return None;
    }
    let cover = |w: i32, h: i32| (f64::from(w) / f64::from(iw)).max(f64::from(h) / f64::from(ih));
    let contain = |w: i32, h: i32| (f64::from(w) / f64::from(iw)).min(f64::from(h) / f64::from(ih));

    let placement = match fit {
        Fit::Fill => {
            let factor = cover(mw, mh);
            // Never fall a pixel short of the monitor because of rounding.
            let (w, h) = (scaled(iw, factor).max(mw as u32), scaled(ih, factor).max(mh as u32));
            Placement { width: w, height: h, x: centred(mw, w as i32), y: centred(mh, h as i32), tile: false }
        }
        Fit::Fit => {
            let factor = contain(mw, mh);
            let (w, h) = (scaled(iw, factor).min(mw as u32), scaled(ih, factor).min(mh as u32));
            Placement { width: w, height: h, x: centred(mw, w as i32), y: centred(mh, h as i32), tile: false }
        }
        Fit::Stretch => Placement { width: mw as u32, height: mh as u32, x: 0, y: 0, tile: false },
        Fit::Center => Placement {
            width: iw,
            height: ih,
            x: centred(mw, iw as i32),
            y: centred(mh, ih as i32),
            tile: false,
        },
        Fit::Tile => Placement {
            width: iw,
            height: ih,
            // The tile grid starts at the virtual desktop's corner, so tiles
            // line up across monitors.
            x: -(monitor.left - virtual_desktop.left).rem_euclid(iw as i32),
            y: -(monitor.top - virtual_desktop.top).rem_euclid(ih as i32),
            tile: true,
        },
        Fit::Span => {
            let (vw, vh) = (virtual_desktop.width().max(1), virtual_desktop.height().max(1));
            let factor = cover(vw, vh);
            let (w, h) = (scaled(iw, factor).max(vw as u32), scaled(ih, factor).max(vh as u32));
            Placement {
                width: w,
                height: h,
                x: centred(vw, w as i32) - (monitor.left - virtual_desktop.left),
                y: centred(vh, h as i32) - (monitor.top - virtual_desktop.top),
                tile: false,
            }
        }
    };
    Some(placement)
}

/// The copy operations that paint `target` (monitor-relative) from a placed
/// image. Areas of `target` no blit covers show the background colour.
pub fn blits(placement: &Placement, target: Rect) -> Vec<Blit> {
    let (w, h) = (placement.width as i32, placement.height as i32);
    if w <= 0 || h <= 0 || target.is_empty() {
        return Vec::new();
    }

    let one = |image_x: i32, image_y: i32| -> Option<Blit> {
        let image = Rect::new(image_x, image_y, image_x + w, image_y + h);
        let visible = image.intersect(&target)?;
        Some(Blit {
            src: visible.offset(-image_x, -image_y),
            dst_x: visible.left - target.left,
            dst_y: visible.top - target.top,
        })
    };

    if !placement.tile {
        return one(placement.x, placement.y).into_iter().collect();
    }

    // First tile column/row at or before the target's edge.
    let first_x = placement.x + (target.left - placement.x).div_euclid(w) * w;
    let first_y = placement.y + (target.top - placement.y).div_euclid(h) * h;
    let mut out = Vec::new();
    let mut y = first_y;
    while y < target.bottom {
        let mut x = first_x;
        while x < target.right {
            out.extend(one(x, y));
            x += w;
        }
        y += h;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect::new(0, 0, 2560, 1600);

    fn place_single(fit: Fit, image: (u32, u32)) -> Placement {
        place(fit, image, MONITOR, MONITOR).unwrap()
    }

    #[test]
    fn fill_crops_the_longer_axis_and_centres() {
        // 16:9 image on a 16:10 monitor: height matches, sides are cropped.
        let p = place_single(Fit::Fill, (3840, 2160));
        assert_eq!((p.width, p.height), (2844, 1600));
        assert_eq!((p.x, p.y), (-142, 0));
        assert!(!p.tile);

        // Portrait image: width matches, top and bottom are cropped.
        let p = place_single(Fit::Fill, (1000, 2000));
        assert_eq!((p.width, p.height), (2560, 5120));
        assert_eq!((p.x, p.y), (0, -1760));
    }

    #[test]
    fn fill_always_covers_the_whole_monitor() {
        for image in [(1, 1), (7, 3), (1921, 1079), (3000, 1875), (2560, 1600), (5000, 100)] {
            let p = place_single(Fit::Fill, image);
            assert!(p.x <= 0 && p.y <= 0, "{image:?}");
            assert!(p.x + p.width as i32 >= 2560, "{image:?}");
            assert!(p.y + p.height as i32 >= 1600, "{image:?}");
        }
    }

    #[test]
    fn fit_letterboxes_and_centres() {
        // 16:9 on 16:10: full width, bars above and below.
        let p = place_single(Fit::Fit, (1920, 1080));
        assert_eq!((p.width, p.height), (2560, 1440));
        assert_eq!((p.x, p.y), (0, 80));

        // Portrait: full height, bars left and right.
        let p = place_single(Fit::Fit, (1000, 2000));
        assert_eq!((p.width, p.height), (800, 1600));
        assert_eq!((p.x, p.y), (880, 0));
    }

    #[test]
    fn stretch_ignores_aspect_ratio() {
        let p = place_single(Fit::Stretch, (100, 900));
        assert_eq!(p, Placement { width: 2560, height: 1600, x: 0, y: 0, tile: false });
    }

    #[test]
    fn center_keeps_the_image_size() {
        // Smaller than the monitor: surrounded by background.
        let p = place_single(Fit::Center, (1000, 600));
        assert_eq!((p.width, p.height, p.x, p.y), (1000, 600, 780, 500));

        // Larger than the monitor: cropped equally on both sides.
        let p = place_single(Fit::Center, (4000, 2000));
        assert_eq!((p.x, p.y), (-720, -200));
    }

    #[test]
    fn tile_starts_at_the_virtual_desktop_corner() {
        let p = place_single(Fit::Tile, (300, 200));
        assert_eq!((p.width, p.height, p.x, p.y, p.tile), (300, 200, 0, 0, true));

        // A second monitor to the right of a 1920-wide one: 1920 mod 300 = 120
        // pixels of a tile are already used up by the first monitor.
        let right = Rect::new(1920, 0, 4480, 1600);
        let desktop = Rect::new(0, 0, 4480, 1600);
        let p = place(Fit::Tile, (300, 200), right, desktop).unwrap();
        assert_eq!((p.x, p.y), (-120, 0));
    }

    #[test]
    fn span_covers_the_whole_desktop() {
        // Two 1920x1080 monitors side by side, image twice as wide as tall.
        let left = Rect::new(0, 0, 1920, 1080);
        let right = Rect::new(1920, 0, 3840, 1080);
        let desktop = Rect::new(0, 0, 3840, 1080);
        let on_left = place(Fit::Span, (4000, 2000), left, desktop).unwrap();
        let on_right = place(Fit::Span, (4000, 2000), right, desktop).unwrap();
        assert_eq!((on_left.width, on_left.height), (3840, 1920));
        assert_eq!((on_left.x, on_left.y), (0, -420));
        // The right monitor shows the right half of the same scaled image.
        assert_eq!((on_right.x, on_right.y), (-1920, -420));

        // On a single monitor Span is the same as Fill.
        assert_eq!(place_single(Fit::Span, (3840, 2160)), place_single(Fit::Fill, (3840, 2160)));
    }

    #[test]
    fn empty_inputs_have_no_placement() {
        assert_eq!(place(Fit::Fill, (0, 100), MONITOR, MONITOR), None);
        assert_eq!(place(Fit::Fill, (100, 100), Rect::new(0, 0, 0, 0), MONITOR), None);
    }

    #[test]
    fn single_blit_for_a_strip_of_a_filled_monitor() {
        let p = place_single(Fit::Fill, (3840, 2160)); // 2844x1600 at (-142, 0)
        // The dock's strip: the bottom 102 pixels.
        let strip = Rect::new(0, 1498, 2560, 1600);
        assert_eq!(
            blits(&p, strip),
            vec![Blit { src: Rect::new(142, 1498, 2702, 1600), dst_x: 0, dst_y: 0 }]
        );
    }

    #[test]
    fn letterbox_bars_are_left_to_the_background() {
        let p = place_single(Fit::Fit, (1920, 1080)); // 2560x1440 at (0, 80)
        // Whole monitor: the image starts 80 pixels down.
        assert_eq!(
            blits(&p, MONITOR),
            vec![Blit { src: Rect::new(0, 0, 2560, 1440), dst_x: 0, dst_y: 80 }]
        );
        // The bottom 60 pixels are entirely inside the lower bar.
        assert_eq!(blits(&p, Rect::new(0, 1540, 2560, 1600)), vec![]);
        // A strip straddling the bar: only its top 20 pixels have image.
        assert_eq!(
            blits(&p, Rect::new(0, 1500, 2560, 1600)),
            vec![Blit { src: Rect::new(0, 1420, 2560, 1440), dst_x: 0, dst_y: 0 }]
        );
    }

    #[test]
    fn tiles_cover_the_target_exactly_once() {
        let p = Placement { width: 300, height: 200, x: -120, y: 0, tile: true };
        let target = Rect::new(100, 150, 700, 450);
        let pieces = blits(&p, target);

        // Every target pixel is covered by exactly one blit.
        let area: i32 = pieces.iter().map(|b| b.src.width() * b.src.height()).sum();
        assert_eq!(area, target.width() * target.height());
        for b in &pieces {
            assert!(b.src.left >= 0 && b.src.top >= 0);
            assert!(b.src.right <= 300 && b.src.bottom <= 200);
            assert!(b.dst_x >= 0 && b.dst_y >= 0);
            assert!(b.dst_x + b.src.width() <= target.width());
            assert!(b.dst_y + b.src.height() <= target.height());
        }
        // First piece: target.left = 100 falls 220px into the tile that starts at -120.
        assert_eq!(pieces[0], Blit { src: Rect::new(220, 150, 300, 200), dst_x: 0, dst_y: 0 });
    }
}
