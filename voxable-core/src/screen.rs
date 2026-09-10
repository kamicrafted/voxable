//! Window placement math, in logical points.
//!
//! Everything here is one coordinate space: **logical points**, the space macOS
//! reports window frames in. Mixing spaces is what stranded the Flow Bar off-screen —
//! a position computed in physical pixels lands at those numbers as points, so on a
//! 2x display it ends up twice as far right and down as intended.

/// A display's usable area, in logical points. `x`/`y` may be negative when a
/// monitor sits left of or above the primary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Area of the overlap between two rects, 0.0 when they do not intersect.
    fn overlap_area(&self, other: &Rect) -> f64 {
        let w = (self.right().min(other.right()) - self.x.max(other.x)).max(0.0);
        let h = (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0.0);
        w * h
    }
}

/// Minimum visible area, in square points, for a placement to count as usable.
/// Roughly a quarter of the 340x96 Flow Bar — enough to see and grab.
const MIN_VISIBLE_AREA: f64 = 8_000.0;

/// Is `window` visible enough on at least one of `monitors` to be usable?
///
/// Checks total overlap rather than the top-left corner: a pill hanging off the
/// bottom edge has an on-screen corner but nothing you can grab.
pub fn is_usable_position(window: &Rect, monitors: &[Rect]) -> bool {
    if monitors.is_empty() {
        return false;
    }
    let visible: f64 = monitors.iter().map(|m| window.overlap_area(m)).sum();
    visible >= MIN_VISIBLE_AREA
}

/// Bottom-center of `monitor`, `margin` points above its bottom edge.
pub fn bottom_center(monitor: &Rect, width: f64, height: f64, margin: f64) -> (f64, f64) {
    let x = monitor.x + (monitor.width - width) / 2.0;
    let y = monitor.y + monitor.height - height - margin;
    (x, y)
}

/// Where to put the window: the saved position when it is still usable, otherwise
/// bottom-center of `primary`.
///
/// Returns `(x, y)` in logical points, and whether the saved position was rejected
/// so the caller can log it.
pub fn resolve_position(
    saved: Option<(f64, f64)>,
    monitors: &[Rect],
    primary: &Rect,
    width: f64,
    height: f64,
    margin: f64,
) -> ((f64, f64), bool) {
    if let Some((x, y)) = saved {
        let candidate = Rect::new(x, y, width, height);
        if is_usable_position(&candidate, monitors) {
            return ((x, y), false);
        }
        return (bottom_center(primary, width, height, margin), true);
    }
    (bottom_center(primary, width, height, margin), false)
}

/// Nudge a window fully onto `monitors` if it hangs off an edge, keeping it as close
/// to where it was as possible.
///
/// The Flow Bar needs this because it grows to the right when it expands from the
/// collapsed dot: a dot parked near the right edge would otherwise expand off-screen.
/// Only the monitor the window most overlaps is considered, so a window on a secondary
/// display is clamped to that display rather than yanked to the primary.
pub fn clamp_to_monitor(window: &Rect, monitors: &[Rect]) -> (f64, f64) {
    let Some(best) = monitors
        .iter()
        .max_by(|a, b| {
            window
                .overlap_area(a)
                .partial_cmp(&window.overlap_area(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
    else {
        return (window.x, window.y);
    };

    // A window larger than the display pins to its origin rather than going negative.
    let max_x = (best.right() - window.width).max(best.x);
    let max_y = (best.bottom() - window.height).max(best.y);
    (
        window.x.clamp(best.x, max_x),
        window.y.clamp(best.y, max_y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 340.0;
    const H: f64 = 96.0;
    const MARGIN: f64 = 40.0;

    /// Dave's machine: 1512x982 built-in primary, 5120x1440 Dell arranged above.
    fn dual() -> (Vec<Rect>, Rect) {
        let primary = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let dell = Rect::new(0.0, -1440.0, 5120.0, 1440.0);
        (vec![primary, dell], primary)
    }

    #[test]
    fn bottom_center_of_primary_is_on_screen() {
        let (monitors, primary) = dual();
        let (x, y) = bottom_center(&primary, W, H, MARGIN);
        assert_eq!((x, y), (586.0, 846.0));
        assert!(is_usable_position(&Rect::new(x, y, W, H), &monitors));
    }

    #[test]
    fn position_below_every_display_is_rejected() {
        // The stranded save: y=1720 sits 738pt below the primary's bottom edge,
        // and the Dell is above, so nothing is there.
        let (monitors, primary) = dual();
        let (pos, fell_back) = resolve_position(Some((104.0, 1720.0)), &monitors, &primary, W, H, MARGIN);
        assert!(fell_back);
        assert_eq!(pos, (586.0, 846.0));
    }

    #[test]
    fn position_on_a_secondary_display_is_kept() {
        let (monitors, primary) = dual();
        let on_dell = (2000.0, -700.0);
        let (pos, fell_back) = resolve_position(Some(on_dell), &monitors, &primary, W, H, MARGIN);
        assert!(!fell_back);
        assert_eq!(pos, on_dell);
    }

    #[test]
    fn mostly_visible_at_an_edge_is_kept() {
        let (monitors, primary) = dual();
        // Hanging 40pt off the right edge leaves 300x96 = 28,800pt visible,
        // so x = 1512 - 300 = 1212.
        let (pos, fell_back) = resolve_position(Some((1212.0, 800.0)), &monitors, &primary, W, H, MARGIN);
        assert!(!fell_back);
        assert_eq!(pos, (1212.0, 800.0));
    }

    #[test]
    fn barely_visible_sliver_is_rejected() {
        let (monitors, primary) = dual();
        // 12pt of width on screen: 12x96 = 1,152pt, under the threshold.
        let (_, fell_back) = resolve_position(Some((1500.0, 800.0)), &monitors, &primary, W, H, MARGIN);
        assert!(fell_back);
    }

    #[test]
    fn no_saved_position_uses_the_default() {
        let (monitors, primary) = dual();
        let (pos, fell_back) = resolve_position(None, &monitors, &primary, W, H, MARGIN);
        assert!(!fell_back);
        assert_eq!(pos, (586.0, 846.0));
        assert!(is_usable_position(&Rect::new(pos.0, pos.1, W, H), &monitors));
    }

    #[test]
    fn negative_origin_display_is_handled() {
        // Monitor left of and above the primary; a position inside it is valid
        // even though both coordinates are negative.
        let primary = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let left = Rect::new(-2560.0, -300.0, 2560.0, 1440.0);
        let monitors = vec![primary, left];
        let (pos, fell_back) = resolve_position(Some((-1200.0, -100.0)), &monitors, &primary, W, H, MARGIN);
        assert!(!fell_back);
        assert_eq!(pos, (-1200.0, -100.0));
    }

    #[test]
    fn a_window_already_inside_is_left_alone() {
        let (monitors, _) = dual();
        let w = Rect::new(600.0, 800.0, W, H);
        assert_eq!(clamp_to_monitor(&w, &monitors), (600.0, 800.0));
    }

    #[test]
    fn expanding_near_the_right_edge_nudges_left() {
        // The dot sits at x=1490; expanded to 296 wide it would reach 1786 on a
        // 1512-wide display, so it slides back to 1512 - 296.
        let (monitors, _) = dual();
        let w = Rect::new(1490.0, 800.0, 296.0, 60.0);
        assert_eq!(clamp_to_monitor(&w, &monitors), (1216.0, 800.0));
    }

    #[test]
    fn clamping_stays_on_the_display_the_window_is_on() {
        // On the Dell (above the primary), clamping must not pull it to the primary.
        let (monitors, _) = dual();
        let w = Rect::new(5000.0, -700.0, 296.0, 60.0);
        let (x, y) = clamp_to_monitor(&w, &monitors);
        assert_eq!((x, y), (4824.0, -700.0));
    }

    #[test]
    fn a_window_wider_than_the_display_pins_to_the_origin() {
        let primary = Rect::new(0.0, 0.0, 200.0, 200.0);
        let w = Rect::new(50.0, 50.0, 400.0, 60.0);
        assert_eq!(clamp_to_monitor(&w, &[primary]), (0.0, 50.0));
    }

    #[test]
    fn no_monitors_leaves_the_position_unchanged() {
        let w = Rect::new(10.0, 20.0, W, H);
        assert_eq!(clamp_to_monitor(&w, &[]), (10.0, 20.0));
    }

    #[test]
    fn empty_monitor_list_is_not_usable() {
        assert!(!is_usable_position(&Rect::new(0.0, 0.0, W, H), &[]));
    }
}
