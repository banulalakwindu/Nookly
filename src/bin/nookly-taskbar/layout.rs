//! Fitting the bar to the screen: how many app tiles show and whether the
//! rest go into an overflow tile. Pure, so it's tested without a window.

/// What fits: `shown` app tiles, then an overflow tile if `overflow`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub shown: usize,
    pub overflow: bool,
}

/// `n` apps, at most `max` of them shown (0 = as many as fit), in `room`
/// DIPs, each tile `tile` wide with `gap` between tiles. The overflow tile
/// is the same size as an app tile. With any room at all, at least one app
/// shows next to the overflow tile.
pub fn fit_apps(n: usize, max: usize, room: f32, tile: f32, gap: f32) -> Fit {
    if n == 0 {
        return Fit { shown: 0, overflow: false };
    }
    let slots = (((room + gap) / (tile + gap)).floor().max(0.0)) as usize;
    let cap = if max == 0 { usize::MAX } else { max };
    if n <= cap && n <= slots {
        return Fit { shown: n, overflow: false };
    }
    // The overflow tile takes a slot of its own.
    let shown = cap.min(slots.saturating_sub(1)).min(n);
    Fit { shown: shown.max(if slots >= 2 { 1 } else { 0 }), overflow: true }
}

/// Width of the app section for a fit.
pub fn apps_width(f: Fit, tile: f32, gap: f32) -> f32 {
    let k = f.shown + f.overflow as usize;
    if k == 0 { 0.0 } else { k as f32 * tile + (k - 1) as f32 * gap }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE: f32 = 40.0;
    const GAP: f32 = 2.0;

    #[test]
    fn everything_fits() {
        assert_eq!(fit_apps(5, 0, 1000.0, TILE, GAP), Fit { shown: 5, overflow: false });
        assert_eq!(fit_apps(0, 0, 1000.0, TILE, GAP), Fit { shown: 0, overflow: false });
    }

    #[test]
    fn max_apps_limits_and_overflows() {
        assert_eq!(fit_apps(12, 8, 2000.0, TILE, GAP), Fit { shown: 8, overflow: true });
        assert_eq!(fit_apps(8, 8, 2000.0, TILE, GAP), Fit { shown: 8, overflow: false });
    }

    #[test]
    fn screen_width_limits() {
        // 1280 px at 150%: 853 DIPs, about 500 left for apps -> 11 slots.
        let f = fit_apps(40, 0, 500.0, TILE, GAP);
        assert_eq!(f, Fit { shown: 10, overflow: true });
        assert!(apps_width(f, TILE, GAP) <= 500.0);
        // 4K at 100%: plenty of room.
        assert_eq!(fit_apps(40, 0, 3000.0, TILE, GAP), Fit { shown: 40, overflow: false });
    }

    #[test]
    fn tiny_room_keeps_one_app_and_overflow() {
        let f = fit_apps(10, 0, 82.0, TILE, GAP);
        assert_eq!(f, Fit { shown: 1, overflow: true });
        assert!(apps_width(f, TILE, GAP) <= 82.0);
        // No room for two tiles: only the overflow tile.
        assert_eq!(fit_apps(10, 0, 50.0, TILE, GAP), Fit { shown: 0, overflow: true });
    }

    #[test]
    fn never_wider_than_room() {
        for n in 0..50 {
            for max in [0, 1, 3, 8, 20] {
                for room in [40.0, 90.0, 300.0, 777.0, 1500.0] {
                    let f = fit_apps(n, max, room, TILE, GAP);
                    assert!(apps_width(f, TILE, GAP) <= room.max(TILE), "n={n} max={max} room={room} {f:?}");
                    assert!(f.shown <= n);
                    if max > 0 {
                        assert!(f.shown <= max);
                    }
                }
            }
        }
    }
}
