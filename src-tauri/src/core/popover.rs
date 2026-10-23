use serde::Serialize;

pub const POPOVER_WIDTH: f64 = 380.0;
pub const POPOVER_HEIGHT: f64 = 600.0;
pub const POPOVER_MIN_HEIGHT: f64 = 200.0;
const ARROW_MARGIN: f64 = 26.0;

pub fn popover_height(requested: Option<f64>) -> f64 {
    requested
        .filter(|height| height.is_finite())
        .map_or(POPOVER_HEIGHT, |height| {
            height.ceil().clamp(POPOVER_MIN_HEIGHT, POPOVER_HEIGHT)
        })
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverPlacement {
    pub revision: u64,
    pub arrow_x: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub struct Layout {
    pub frame: Bounds,
    pub arrow_x: Option<f64>,
}

pub fn layout(
    work: Bounds,
    anchor: Option<Bounds>,
    scale: f64,
    macos: bool,
    height: f64,
) -> Layout {
    let anchor = anchor.filter(|rect| rect.width > 0.0 && rect.height > 0.0);
    let width = (POPOVER_WIDTH * scale).round().min(work.width);
    let height = (height * scale).round().min(work.height);
    let (x, y) = match anchor {
        Some(tray) => {
            let above = tray.y - height;
            let y = if macos || above < work.y {
                tray.y + tray.height
            } else {
                above
            };
            (tray.x + tray.width / 2.0 - width / 2.0, y)
        }
        None => (
            work.x + (work.width - width) / 2.0,
            work.y + (work.height - height) / 2.0,
        ),
    };
    let x = x.clamp(work.x, work.x + work.width - width).round();
    let y = y.clamp(work.y, work.y + work.height - height).round();
    let arrow_x = anchor.filter(|_| macos).and_then(|tray| {
        let offset = (tray.x + tray.width / 2.0 - x) / scale;
        (offset >= ARROW_MARGIN && offset <= width / scale - ARROW_MARGIN).then_some(offset)
    });
    Layout {
        frame: Bounds {
            x,
            y,
            width,
            height,
        },
        arrow_x,
    }
}

/// AppKit frames grow upwards from the bottom of the main screen.
pub fn cocoa_layout(work: Bounds, anchor: Option<Bounds>, height: f64) -> Layout {
    let flip = |rect: Bounds| Bounds {
        y: -rect.y - rect.height,
        ..rect
    };
    let placed = layout(flip(work), anchor.map(flip), 1.0, true, height);
    Layout {
        frame: flip(placed.frame),
        arrow_x: placed.arrow_x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn cocoa_coordinates_keep_content_size_across_retina_and_external_screens() {
        let tray = |x: f64| bounds(x, 875.0, 24.0, 25.0);
        for (work, tray, expected_x, expected_y) in [
            (bounds(0.0, 0.0, 1440.0, 875.0), tray(1368.0), 1060.0, 275.0),
            (
                bounds(1440.0, -180.0, 1920.0, 1055.0),
                tray(3288.0),
                2980.0,
                275.0,
            ),
            (
                bounds(-1920.0, -180.0, 1920.0, 1055.0),
                tray(-1872.0),
                -1920.0,
                275.0,
            ),
            (
                bounds(0.0, 900.0, 1920.0, 1055.0),
                bounds(1848.0, 1955.0, 24.0, 25.0),
                1540.0,
                1355.0,
            ),
        ] {
            let placed = cocoa_layout(work, Some(tray), POPOVER_HEIGHT);
            assert_eq!(placed.frame.width, POPOVER_WIDTH);
            assert_eq!(placed.frame.height, POPOVER_HEIGHT);
            assert_eq!(placed.frame.x, expected_x);
            assert_eq!(placed.frame.y, expected_y);
            assert_eq!(placed.frame.y + placed.frame.height, tray.y);
            assert_eq!(
                placed.frame.x + placed.arrow_x.unwrap(),
                tray.x + tray.width / 2.0
            );
        }
    }

    #[test]
    fn cocoa_small_work_area_and_missing_tray_stay_within_screen() {
        let work = bounds(-360.0, 900.0, 360.0, 440.0);
        let placed = cocoa_layout(work, None, POPOVER_HEIGHT);
        assert_eq!(placed.frame, work);
        assert_eq!(placed.arrow_x, None);
    }

    #[test]
    fn retina_arrow_tracks_tray_after_right_edge_clamping() {
        let work = bounds(0.0, 50.0, 2880.0, 1750.0);
        let tray = bounds(2736.0, 0.0, 48.0, 50.0);
        let placed = layout(work, Some(tray), 2.0, true, POPOVER_HEIGHT);
        assert_eq!(placed.frame.x, 2120.0);
        assert_eq!(placed.frame.y, 50.0);
        assert_eq!(placed.arrow_x, Some(320.0));
        assert_eq!(placed.frame.x + placed.arrow_x.unwrap() * 2.0, 2760.0);
    }

    #[test]
    fn left_monitor_coordinates_keep_arrow_aligned() {
        let work = bounds(-1920.0, 25.0, 1920.0, 1055.0);
        let tray = bounds(-1872.0, 0.0, 24.0, 25.0);
        let placed = layout(work, Some(tray), 1.0, true, POPOVER_HEIGHT);
        assert_eq!(placed.frame.x, -1920.0);
        assert_eq!(placed.arrow_x, Some(60.0));
    }

    #[test]
    fn unavailable_or_outside_anchor_has_no_misleading_arrow() {
        let work = bounds(0.0, 25.0, 1280.0, 775.0);
        let centered = layout(work, None, 1.0, true, POPOVER_HEIGHT);
        assert_eq!(centered.frame.x, 450.0);
        assert_eq!(centered.arrow_x, None);
        let hidden = bounds(0.0, 800.0, 34.0, 0.0);
        let fallback = layout(work, Some(hidden), 1.0, true, POPOVER_HEIGHT);
        assert_eq!(fallback.frame, centered.frame);
        assert_eq!(fallback.arrow_x, None);
        let edge = bounds(1260.0, 0.0, 20.0, 25.0);
        assert_eq!(
            layout(work, Some(edge), 1.0, true, POPOVER_HEIGHT).arrow_x,
            None
        );
    }

    #[test]
    fn popover_height_follows_content_within_bounds() {
        assert_eq!(popover_height(None), POPOVER_HEIGHT);
        assert_eq!(popover_height(Some(f64::NAN)), POPOVER_HEIGHT);
        assert_eq!(popover_height(Some(311.2)), 312.0);
        assert_eq!(popover_height(Some(40.0)), POPOVER_MIN_HEIGHT);
        assert_eq!(popover_height(Some(2000.0)), POPOVER_HEIGHT);
    }

    #[test]
    fn short_popover_stays_attached_to_the_tray() {
        let work = bounds(0.0, 0.0, 1440.0, 875.0);
        let tray = bounds(1368.0, 875.0, 24.0, 25.0);
        let placed = cocoa_layout(work, Some(tray), 300.0);
        assert_eq!(placed.frame.height, 300.0);
        assert_eq!(placed.frame.y + placed.frame.height, tray.y);
        let work = bounds(0.0, 0.0, 1920.0 * 1.5, 1040.0 * 1.5);
        let tray = bounds(1800.0 * 1.5, 1040.0 * 1.5, 24.0 * 1.5, 40.0 * 1.5);
        let placed = layout(work, Some(tray), 1.5, false, 300.0);
        assert_eq!(placed.frame.height, 450.0);
        assert_eq!(placed.frame.y + placed.frame.height, tray.y);
    }

    #[test]
    fn windows_taskbar_placement_respects_each_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let work = bounds(-1920.0 * scale, 0.0, 1920.0 * scale, 1040.0 * scale);
            let tray = bounds(-80.0 * scale, 1040.0 * scale, 24.0 * scale, 40.0 * scale);
            let placed = layout(work, Some(tray), scale, false, POPOVER_HEIGHT);
            assert_eq!(placed.frame.width, (POPOVER_WIDTH * scale).round());
            assert_eq!(placed.frame.height, (POPOVER_HEIGHT * scale).round());
            assert_eq!(placed.frame.x + placed.frame.width, 0.0);
            assert_eq!(placed.frame.y + placed.frame.height, tray.y);
            assert_eq!(placed.arrow_x, None);
        }
    }

    #[test]
    fn top_taskbar_places_window_below_and_small_displays_fit() {
        let work = bounds(0.0, 40.0, 360.0, 440.0);
        let tray = bounds(300.0, 0.0, 24.0, 40.0);
        let placed = layout(work, Some(tray), 1.5, false, POPOVER_HEIGHT);
        assert_eq!(placed.frame, work);
    }
}
