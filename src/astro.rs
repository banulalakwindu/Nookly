//! The sun and the moon, worked out on the PC (no internet): when the sun
//! rises and sets where you are, and the moon's phase.

use std::f64::consts::{PI, TAU};

/// Sunrise and sunset today, in minutes after local midnight; None when the
/// sun doesn't rise or doesn't set (far north or south).
/// `doy`: day of the year (1..366); `tz_min`: your offset from UTC.
pub fn sun_times(lat: f64, lon: f64, doy: u32, tz_min: f64) -> Option<(f64, f64)> {
    let g = TAU / 365.0 * (doy as f64 - 1.0);
    let eq = 229.18 * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos() - 0.040849 * (2.0 * g).sin());
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos() + 0.000907 * (2.0 * g).sin() - 0.002697 * (3.0 * g).cos() + 0.00148 * (3.0 * g).sin();
    let la = lat.to_radians();
    let c = (90.833f64.to_radians().cos() / (la.cos() * decl.cos())) - la.tan() * decl.tan();
    if !(-1.0..=1.0).contains(&c) {
        return None;
    }
    let ha = c.acos().to_degrees();
    let rise = 720.0 - 4.0 * (lon + ha) - eq + tz_min;
    let set = 720.0 - 4.0 * (lon - ha) - eq + tz_min;
    Some((rise.rem_euclid(1440.0), set.rem_euclid(1440.0)))
}

/// The moon's age now (days since the new moon, 0..29.53).
pub const MONTH: f64 = 29.530_588_853;

pub fn moon_age(unix: f64) -> f64 {
    let jd = unix / 86400.0 + 2_440_587.5;
    ((jd - 2_451_550.1) / MONTH).rem_euclid(1.0) * MONTH
}

/// How much of the moon is lit (0..1) at `age`.
pub fn moon_lit(age: f64) -> f64 {
    (1.0 - (TAU * age / MONTH).cos()) / 2.0
}

/// The phase's name.
pub fn moon_name(age: f64) -> &'static str {
    const NAMES: [&str; 8] = ["New moon", "Waxing crescent", "First quarter", "Waxing gibbous", "Full moon", "Waning gibbous", "Last quarter", "Waning crescent"];
    NAMES[((age / MONTH * 8.0 + 0.5).floor() as usize) % 8]
}

/// Days until the next full moon and the next new moon.
pub fn moon_next(age: f64) -> (f64, f64) {
    let full = (MONTH / 2.0 - age).rem_euclid(MONTH);
    let new = (MONTH - age).rem_euclid(MONTH);
    (full, new)
}

/// The lit part of the moon as a shape: points of its outline, for a moon
/// of radius `r` at (cx, cy) (lit on the right while waxing).
pub fn moon_lit_shape(cx: f32, cy: f32, r: f32, age: f64) -> Vec<(f32, f32)> {
    let p = age / MONTH;
    let k = (TAU * p).cos() as f32;
    let waxing = p < 0.5;
    let n = 24;
    let mut pts = Vec::with_capacity(2 * n + 2);
    // Down the limb (the lit edge), then back up along the terminator.
    for i in 0..=n {
        let a = -PI / 2.0 + PI * i as f64 / n as f64;
        let (y, l) = (r * a.sin() as f32, r * a.cos() as f32);
        pts.push((cx + if waxing { l } else { -l }, cy + y));
    }
    for i in (0..=n).rev() {
        let a = -PI / 2.0 + PI * i as f64 / n as f64;
        let (y, l) = (r * a.sin() as f32, r * a.cos() as f32);
        pts.push((cx + if waxing { l * k } else { -l * k }, cy + y));
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_in_colombo() {
        // Colombo, 18 Nov: sunrise about 6:00, sunset about 17:50 (UTC+5:30).
        let (rise, set) = sun_times(6.93, 79.85, 322, 330.0).unwrap();
        assert!((rise - 360.0).abs() < 20.0, "{rise}");
        assert!((set - 1070.0).abs() < 20.0, "{set}");
    }

    #[test]
    fn moon_phases() {
        // 2024-01-11 11:57 UTC was a new moon; 2024-01-25 17:54 UTC a full moon.
        let new = moon_age(1_704_974_220.0);
        assert!(new < 1.0 || new > MONTH - 1.0, "{new}");
        let full = moon_age(1_706_205_240.0);
        assert!((full - MONTH / 2.0).abs() < 1.0, "{full}");
        assert_eq!(moon_name(full), "Full moon");
        assert!(moon_lit(full) > 0.97);
    }
}
