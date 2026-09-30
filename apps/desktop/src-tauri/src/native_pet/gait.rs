//! Shared art-space stride. Rendering advances from actual grounded distance, not a separate clock.
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gait {
    frame_ms: f64,
    frames: usize,
    stride: f64,
}
fn get(mode: &str) -> Option<&'static Gait> {
    static DATA: OnceLock<HashMap<String, Gait>> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../../src/pet-gaits.json"))
            .expect("validated gait data")
    })
    .get(mode)
}
pub fn speed(mode: &str, size: f64, multiplier: f64) -> f64 {
    get(mode).map_or(0., |g| {
        g.stride * (size / 260.) / (g.frames as f64 * g.frame_ms / 1000.) * multiplier
    })
}
pub fn elapsed(mode: &str, distance: f64, size: f64) -> f64 {
    let speed = speed(mode, size, 1.);
    if speed > 0. {
        distance.max(0.) / speed
    } else {
        0.
    }
}
pub fn frame(mode: &str, seconds: f64, reduced: bool) -> usize {
    get(mode).map_or(0, |g| {
        if reduced {
            if mode == "run" {
                3
            } else {
                0
            }
        } else {
            (seconds.max(0.) * 1000. / g.frame_ms).floor() as usize % g.frames
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn size_and_speed_preserve_one_cycle_stride() {
        for mode in ["walk", "run"] {
            for size in [110., 180., 245.] {
                for multiplier in [0.5, 1., 2.] {
                    let g = get(mode).unwrap();
                    let cycle = g.frames as f64 * g.frame_ms / 1000.;
                    let distance = speed(mode, size, multiplier) * cycle / multiplier;
                    assert!((distance - g.stride * size / 260.).abs() < 1e-8);
                    assert!((elapsed(mode, distance, size) - cycle).abs() < 1e-8);
                }
            }
        }
    }
    #[test]
    fn stopped_pet_does_not_keep_stepping() {
        let t = elapsed("walk", 30., 180.);
        assert_eq!(
            frame("walk", t, false),
            frame("walk", elapsed("walk", 30., 180.), false)
        );
        assert_eq!(elapsed("walk", 0., 180.), 0.);
        assert_eq!(speed("idle", 180., 1.), 0.);
    }
}
