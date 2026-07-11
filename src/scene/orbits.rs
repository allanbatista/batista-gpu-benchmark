//! Deterministic motion core: seeded parameter derivation (splitmix64) and pure
//! f64 orbit functions of the internal benchmark clock. Nothing here reads wall
//! time, diagnostics or any other frame-variant source (spec §2/§5).

use bevy::math::DVec3;
use bevy::prelude::*;
use std::f64::consts::TAU;

pub const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

/// Internal benchmark clock. Advanced by real per-frame deltas; reset at the
/// start of each run so every run replays the identical trajectory f(t).
#[derive(Resource, Default)]
pub struct BenchClock {
    pub t: f64,
}

pub fn tick_clock(mut clock: ResMut<BenchClock>, time: Res<Time<Real>>) {
    clock.t += time.delta_secs_f64();
}

// ---------------------------------------------------------------------------
// Seeded derivation
// ---------------------------------------------------------------------------

pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.next_f64() * (hi - lo)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbitKind {
    Horizontal,
    Vertical,
    Diagonal,
    Elliptical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightParams {
    pub kind: OrbitKind,
    pub spot: bool,
    pub radius: f64,
    pub height: f64,
    /// Signed angular speed (rad/s); sign encodes orbit direction.
    pub speed: f64,
    pub phase: f64,
    pub incline: f64,
    pub ellipse: f64,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub light_radius: f32,
    pub shadows: bool,
}

const PALETTE: [[f32; 3]; 8] = [
    [1.00, 0.55, 0.20],
    [0.25, 0.55, 1.00],
    [1.00, 0.25, 0.30],
    [0.30, 1.00, 0.55],
    [0.95, 0.90, 0.75],
    [0.80, 0.35, 1.00],
    [0.25, 0.95, 0.95],
    [1.00, 0.85, 0.25],
];

pub struct LightDeriveInput {
    pub seed: u32,
    pub count: u32,
    pub shadow_casters: u32,
    pub point_shadows: bool,
    pub spot_shadows: bool,
    pub shadows_enabled: bool,
    pub intensity_mul: f32,
    pub speed_mul: f64,
}

/// Derives all light parameters in a FIXED order (index order, unconditional
/// rng draws) so a given seed always yields the same rig.
pub fn derive_lights(input: &LightDeriveInput) -> Vec<LightParams> {
    let mut rng = SplitMix64((input.seed as u64) ^ 0xB5E7_1AB5_7ACE_D00D);
    (0..input.count)
        .map(|i| {
            let kind = match i % 4 {
                0 => OrbitKind::Horizontal,
                1 => OrbitKind::Vertical,
                2 => OrbitKind::Diagonal,
                _ => OrbitKind::Elliptical,
            };
            let spot = i % 3 == 2;
            let direction = if i % 2 == 0 { 1.0 } else { -1.0 };
            // rng draws below are unconditional and in fixed order — do not reorder.
            let radius = rng.range(2.4, 4.2);
            let height = rng.range(0.7, 2.8);
            let speed = direction * rng.range(0.25, 0.85) * input.speed_mul;
            let phase = (i as f64 * GOLDEN_ANGLE + rng.range(0.0, 0.25)).rem_euclid(TAU);
            let incline = rng.range(0.35, 0.95);
            let ellipse = rng.range(0.55, 0.85);
            let intensity_jitter = rng.range(0.8, 1.2) as f32;
            let light_radius = rng.range(0.02, 0.12) as f32;

            let base_intensity = if spot { 620_000.0 } else { 250_000.0 };
            let type_shadows_on = if spot { input.spot_shadows } else { input.point_shadows };
            LightParams {
                kind,
                spot,
                radius,
                height,
                speed,
                phase,
                incline,
                ellipse,
                color: PALETTE[i as usize % PALETTE.len()],
                intensity: base_intensity * input.intensity_mul * intensity_jitter,
                range: 16.0,
                light_radius,
                shadows: input.shadows_enabled && type_shadows_on && i < input.shadow_casters,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Pure trajectories
// ---------------------------------------------------------------------------

/// Light position at benchmark time `t`. Pure f64; identical across runs/machines.
pub fn light_position(p: &LightParams, t: f64) -> DVec3 {
    let a = (p.phase + p.speed * t).rem_euclid(TAU);
    let (sin, cos) = a.sin_cos();
    let pos = match p.kind {
        OrbitKind::Horizontal => DVec3::new(p.radius * cos, p.height, p.radius * sin),
        OrbitKind::Vertical => {
            // Circle in a vertical plane whose azimuth is fixed by the phase.
            let (paz_sin, paz_cos) = p.phase.sin_cos();
            DVec3::new(
                p.radius * cos * paz_cos,
                p.height + p.radius * 0.55 * sin,
                p.radius * cos * paz_sin,
            )
        }
        OrbitKind::Diagonal => {
            let (inc_sin, inc_cos) = p.incline.sin_cos();
            DVec3::new(p.radius * cos, p.height - p.radius * sin * inc_sin * 0.6, p.radius * sin * inc_cos)
        }
        OrbitKind::Elliptical => DVec3::new(p.radius * cos, p.height, p.radius * p.ellipse * sin),
    };
    DVec3::new(pos.x, pos.y.max(0.15), pos.z)
}

/// The environment room is a 9 m-radius, 6 m-tall cylinder; the camera must
/// never leave it (margin keeps the near plane off the walls).
pub const CAM_MAX_XZ: f64 = 8.3;
pub const CAM_MIN_Y: f64 = 0.3;
pub const CAM_MAX_Y: f64 = 5.6;

/// Clamps an eye position to the inside of the room (pure — keeps trajectories
/// deterministic).
pub fn clamp_to_room(eye: DVec3) -> DVec3 {
    let xz_len = (eye.x * eye.x + eye.z * eye.z).sqrt();
    let (x, z) = if xz_len > CAM_MAX_XZ {
        let s = CAM_MAX_XZ / xz_len;
        (eye.x * s, eye.z * s)
    } else {
        (eye.x, eye.z)
    };
    DVec3::new(x, eye.y.clamp(CAM_MIN_Y, CAM_MAX_Y), z)
}

/// Camera position/target at benchmark time `t`, framed around the model bounds.
pub fn camera_pose(t: f64, center: DVec3, radius: f64, dist_mul: f64, speed_mul: f64) -> (DVec3, DVec3) {
    let dist = (radius * 3.0).max(1.5) * dist_mul;
    let angle = (0.22 * speed_mul * t).rem_euclid(TAU);
    let height = center.y + radius * (0.8 + 0.3 * (t * 0.13 * speed_mul).sin());
    let eye = DVec3::new(dist * angle.cos(), height, dist * angle.sin());
    (clamp_to_room(eye), center)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(seed: u32) -> LightDeriveInput {
        LightDeriveInput {
            seed,
            count: 16,
            shadow_casters: 4,
            point_shadows: true,
            spot_shadows: true,
            shadows_enabled: true,
            intensity_mul: 1.0,
            speed_mul: 1.0,
        }
    }

    #[test]
    fn same_seed_same_rig() {
        assert_eq!(derive_lights(&input(42)), derive_lights(&input(42)));
    }

    #[test]
    fn different_seed_different_rig() {
        assert_ne!(derive_lights(&input(42)), derive_lights(&input(43)));
    }

    #[test]
    fn trajectories_are_pure_functions_of_time() {
        let rig = derive_lights(&input(42));
        for p in &rig {
            for t in [0.0, 1.5, 33.7, 61.2] {
                assert_eq!(light_position(p, t), light_position(p, t));
                // above the floor, inside the arena
                let pos = light_position(p, t);
                assert!(pos.y >= 0.15, "light below floor: {pos:?}");
                assert!(pos.length() < 20.0);
            }
        }
        let (eye_a, tgt_a) = camera_pose(12.34, DVec3::new(0.0, 0.9, 0.0), 1.0, 1.0, 1.0);
        let (eye_b, tgt_b) = camera_pose(12.34, DVec3::new(0.0, 0.9, 0.0), 1.0, 1.0, 1.0);
        assert_eq!(eye_a, eye_b);
        assert_eq!(tgt_a, tgt_b);
    }

    #[test]
    fn camera_never_leaves_the_room() {
        // zoomed way out at every angle/height the sliders allow
        for t in 0..200 {
            let t = t as f64 * 0.5;
            let (eye, _) = camera_pose(t, DVec3::new(0.0, 0.9, 0.0), 1.2, 3.0, 1.0);
            assert!((eye.x * eye.x + eye.z * eye.z).sqrt() <= CAM_MAX_XZ + 1e-9, "left the room at t={t}: {eye:?}");
            assert!(eye.y >= CAM_MIN_Y && eye.y <= CAM_MAX_Y);
        }
        let clamped = clamp_to_room(DVec3::new(50.0, 20.0, -50.0));
        assert!((clamped.x * clamped.x + clamped.z * clamped.z).sqrt() <= CAM_MAX_XZ + 1e-9);
        assert_eq!(clamped.y, CAM_MAX_Y);
    }

    #[test]
    fn shadow_casters_respect_caps_and_toggles() {
        let rig = derive_lights(&input(42));
        assert_eq!(rig.iter().filter(|p| p.shadows).count(), 4);
        let mut no_spot = input(42);
        no_spot.spot_shadows = false;
        let rig = derive_lights(&no_spot);
        assert!(rig.iter().filter(|p| p.shadows).all(|p| !p.spot));
    }
}
