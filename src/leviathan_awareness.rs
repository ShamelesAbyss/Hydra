//! Procedural target-aware creature consciousness, personality-paced behavioral
//! attention, and coordinated whole-body compensation for **HYDRA: Project Synapse**.
//!
//! Operational truthfulness:
//! - Idle eye tracking, neighbor glances, and postural weight shifts remain strictly
//!   decorative life.
//! - Jaw flares, focused surge posture, and alert orientation are driven exclusively
//!   by verified UI interaction (open/moved floating panels) or genuine subsystem
//!   activity/warnings.

use crate::leviathan_3d_math::{AwarenessPose, BodyCompensation, HeadAwarenessOutput, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttentionTarget {
    Panel(Vec3),
    SubsystemEvent(Vec3),
    OperatorCamera(Vec3),
    NeighborHead(usize),
    AmethystHeart,
    SanctuaryPoint(Vec3),
    Rest,
}

#[derive(Clone, Copy, Debug)]
pub struct HeadPersonality {
    pub name: &'static str,
    /// Critically damped tracking frequency (higher = snappier predator turn).
    pub stiffness: f32,
    pub min_dwell_s: f32,
    pub max_dwell_s: f32,
    pub camera_affinity: f32,
    pub neighbor_affinity: f32,
    pub pull_gain: f32,
    pub tilt_bias: f32,
    pub jaw_readiness: f32,
}

pub const HEAD_PERSONALITIES: [HeadPersonality; 7] = [
    // 0: HUNTER — The Watcher (agile, curious, rapid scan dwell)
    HeadPersonality {
        name: "HUNTER",
        stiffness: 7.4,
        min_dwell_s: 2.6,
        max_dwell_s: 4.4,
        camera_affinity: 0.78,
        neighbor_affinity: 0.55,
        pull_gain: 0.24,
        tilt_bias: 0.04,
        jaw_readiness: 0.02,
    },
    // 1: FANG — The Predator (decisive, energetic, coiled strike poise)
    HeadPersonality {
        name: "FANG",
        stiffness: 8.2,
        min_dwell_s: 2.9,
        max_dwell_s: 5.0,
        camera_affinity: 0.85,
        neighbor_affinity: 0.42,
        pull_gain: 0.28,
        tilt_bias: -0.05,
        jaw_readiness: 0.05,
    },
    // 2: COGNITION — The Mind (contemplative, deliberate head tilt)
    HeadPersonality {
        name: "COGNITION",
        stiffness: 5.2,
        min_dwell_s: 4.0,
        max_dwell_s: 6.4,
        camera_affinity: 0.62,
        neighbor_affinity: 0.72,
        pull_gain: 0.19,
        tilt_bias: 0.09,
        jaw_readiness: 0.0,
    },
    // 3: AEGIS — The Guardian (steady, imposing sovereign sentinel)
    HeadPersonality {
        name: "AEGIS",
        stiffness: 4.8,
        min_dwell_s: 4.4,
        max_dwell_s: 6.8,
        camera_affinity: 0.90,
        neighbor_affinity: 0.35,
        pull_gain: 0.16,
        tilt_bias: 0.0,
        jaw_readiness: 0.01,
    },
    // 4: NEXUS — The Executor (economical, crisp, disciplined lock)
    HeadPersonality {
        name: "NEXUS",
        stiffness: 6.8,
        min_dwell_s: 3.4,
        max_dwell_s: 5.4,
        camera_affinity: 0.68,
        neighbor_affinity: 0.48,
        pull_gain: 0.20,
        tilt_bias: -0.02,
        jaw_readiness: 0.01,
    },
    // 5: ORACLE — The Seer (patient, graceful, ancient cosmic cadence)
    HeadPersonality {
        name: "ORACLE",
        stiffness: 4.2,
        min_dwell_s: 4.8,
        max_dwell_s: 7.4,
        camera_affinity: 0.55,
        neighbor_affinity: 0.66,
        pull_gain: 0.18,
        tilt_bias: 0.07,
        jaw_readiness: 0.0,
    },
    // 6: GENESIS — The Architect (exploratory, dynamic creative curiosity)
    HeadPersonality {
        name: "GENESIS",
        stiffness: 6.2,
        min_dwell_s: 3.0,
        max_dwell_s: 5.2,
        camera_affinity: 0.72,
        neighbor_affinity: 0.64,
        pull_gain: 0.25,
        tilt_bias: -0.06,
        jaw_readiness: 0.02,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AwarenessConfig {
    pub attention_intensity: f32,
    pub body_responsiveness: f32,
    pub animation_intensity: f32,
    pub reduced_motion: bool,
}

impl Default for AwarenessConfig {
    fn default() -> Self {
        Self {
            attention_intensity: 1.0,
            body_responsiveness: 1.0,
            animation_intensity: 1.0,
            reduced_motion: false,
        }
    }
}

#[derive(Clone, Debug)]
struct HeadSpringState {
    target_kind: AttentionTarget,
    dwell_remaining_s: f32,
    cycle_counter: u32,
    look_pos: Vec3,
    look_vel: Vec3,
    pull_pos: Vec3,
    pull_vel: Vec3,
}

impl HeadSpringState {
    fn new(idx: usize) -> Self {
        Self {
            target_kind: AttentionTarget::Rest,
            // Stagger initial dwell timers across all 7 heads so they never flip in lockstep.
            dwell_remaining_s: 1.4 + (idx as f32 * 0.73) % 3.2,
            cycle_counter: idx as u32 * 7 + 1,
            look_pos: Vec3::new(0.0, 0.8, 3.5),
            look_vel: Vec3::ZERO,
            pull_pos: Vec3::ZERO,
            pull_vel: Vec3::ZERO,
        }
    }
}

/// Critically damped spring step (unconditionally stable across frame intervals).
#[inline]
fn damp_vec3(pos: &mut Vec3, vel: &mut Vec3, target: Vec3, omega: f32, dt: f32) {
    let dt = dt.clamp(0.0005, 0.08);
    let omega = omega.clamp(0.5, 24.0);
    let d = *pos - target;
    let e = (-omega * dt).exp();
    let t = d + (*vel + d * omega) * dt;
    *pos = target + t * e;
    *vel = (*vel - (*vel + d * omega) * (omega * dt)) * e;
}

#[inline]
fn damp_f32(pos: &mut f32, vel: &mut f32, target: f32, omega: f32, dt: f32) {
    let dt = dt.clamp(0.0005, 0.08);
    let omega = omega.clamp(0.5, 24.0);
    let d = *pos - target;
    let e = (-omega * dt).exp();
    let t = d + (*vel + d * omega) * dt;
    *pos = target + t * e;
    *vel = (*vel - (*vel + d * omega) * (omega * dt)) * e;
}

pub struct AwarenessEngine {
    heads: [HeadSpringState; 7],
    thorax_yaw: f32,
    thorax_yaw_vel: f32,
    thorax_pitch: f32,
    thorax_pitch_vel: f32,
    tail_yaw: f32,
    tail_yaw_vel: f32,
    wing_l: f32,
    wing_l_vel: f32,
    wing_r: f32,
    wing_r_vel: f32,
}

impl Default for AwarenessEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AwarenessEngine {
    pub fn new() -> Self {
        Self {
            heads: std::array::from_fn(HeadSpringState::new),
            thorax_yaw: 0.0,
            thorax_yaw_vel: 0.0,
            thorax_pitch: 0.0,
            thorax_pitch_vel: 0.0,
            tail_yaw: 0.0,
            tail_yaw_vel: 0.0,
            wing_l: 0.0,
            wing_l_vel: 0.0,
            wing_r: 0.0,
            wing_r_vel: 0.0,
        }
    }

    pub fn current_target(&self, head: usize) -> Option<AttentionTarget> {
        self.heads.get(head).map(|h| h.target_kind)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        dt: f32,
        time: f32,
        camera_eye: Vec3,
        selected_head: Option<usize>,
        open_panels: &[Option<Vec3>; 7],
        head_rest_pos: &[Vec3; 7],
        head_activity: &[f32; 7],
        head_warning: &[bool; 7],
        cfg: &AwarenessConfig,
    ) -> AwarenessPose {
        let dt = dt.clamp(0.001, 0.08);
        let att_scale = if cfg.attention_intensity.is_finite() {
            cfg.attention_intensity.clamp(0.0, 1.5)
        } else {
            1.0
        };
        let body_scale = if cfg.body_responsiveness.is_finite() {
            cfg.body_responsiveness.clamp(0.0, 1.5)
        } else {
            1.0
        };
        let anim_scale = if cfg.animation_intensity.is_finite() {
            cfg.animation_intensity.clamp(0.0, 1.5)
        } else {
            1.0
        };

        let mut pose = AwarenessPose::default();
        let heart_world = Vec3::new(0.0, -1.05, 0.54);
        let sanctuary_points = [
            Vec3::new(-4.6, 0.4, -2.5),
            Vec3::new(0.0, 3.2, -1.8),
            Vec3::new(4.6, 0.4, -2.5),
            Vec3::new(0.0, -1.4, 3.6),
        ];

        let mut aggregate_pull_x = 0.0_f32;
        let mut active_pulls = 0.0_f32;

        for h in 0..7 {
            let pers = HEAD_PERSONALITIES[h];
            let rest_p = head_rest_pos[h];
            let rest_look = rest_p + Vec3::new((h as f32 - 3.0) * 0.25, -0.35, 3.4);
            let event_active = head_warning[h] || head_activity[h] > 0.15;
            let state = &mut self.heads[h];

            // Priority selection with anti-thrashing dwell guard
            if let Some(panel_pos) = open_panels[h] {
                state.target_kind = if event_active {
                    AttentionTarget::SubsystemEvent(panel_pos)
                } else {
                    AttentionTarget::Panel(panel_pos)
                };
                state.dwell_remaining_s = pers.min_dwell_s * 0.75;
            } else if event_active {
                let ev_pos = rest_look.lerp(heart_world + Vec3::new(0.0, 0.6, 1.2), 0.45);
                state.target_kind = AttentionTarget::SubsystemEvent(ev_pos);
                state.dwell_remaining_s = pers.min_dwell_s;
            } else if selected_head == Some(h) {
                state.target_kind = AttentionTarget::OperatorCamera(camera_eye);
                state.dwell_remaining_s = pers.min_dwell_s * 0.6;
            } else {
                state.dwell_remaining_s -= dt;
                // Release panel/event locks cleanly back to sentinel behavior when closed
                let stale_forced = matches!(
                    state.target_kind,
                    AttentionTarget::Panel(_) | AttentionTarget::SubsystemEvent(_)
                );
                if state.dwell_remaining_s <= 0.0 || stale_forced {
                    state.cycle_counter = state.cycle_counter.wrapping_add(1);
                    let selector = ((state.cycle_counter * 13 + h as u32 * 5) % 100) as f32 / 100.0;
                    state.target_kind = if selector < pers.camera_affinity * 0.42
                        && camera_eye.z > -3.5
                    {
                        AttentionTarget::OperatorCamera(camera_eye)
                    } else if selector < pers.camera_affinity * 0.42 + pers.neighbor_affinity * 0.34
                    {
                        let n = if h == 0 {
                            1
                        } else if h == 6 {
                            5
                        } else if state.cycle_counter % 2 == 0 {
                            h - 1
                        } else {
                            h + 1
                        };
                        AttentionTarget::NeighborHead(n)
                    } else if selector < 0.76 {
                        AttentionTarget::AmethystHeart
                    } else if selector < 0.90 {
                        AttentionTarget::SanctuaryPoint(
                            sanctuary_points[(state.cycle_counter as usize + h) % 4],
                        )
                    } else {
                        AttentionTarget::Rest
                    };
                    let span = (pers.max_dwell_s - pers.min_dwell_s).max(0.2);
                    let frac = ((state.cycle_counter * 29) % 17) as f32 / 16.0;
                    state.dwell_remaining_s = pers.min_dwell_s + span * frac;
                } else if matches!(state.target_kind, AttentionTarget::OperatorCamera(_)) {
                    state.target_kind = AttentionTarget::OperatorCamera(camera_eye);
                }
            }

            let raw_target = match state.target_kind {
                AttentionTarget::Panel(p) | AttentionTarget::SubsystemEvent(p) => p,
                AttentionTarget::OperatorCamera(cam) => cam,
                AttentionTarget::NeighborHead(n) => head_rest_pos[n.min(6)],
                AttentionTarget::AmethystHeart => heart_world,
                AttentionTarget::SanctuaryPoint(pt) => pt,
                AttentionTarget::Rest => rest_look,
            };

            // Restrain neck displacement cleanly toward active panels / camera
            let pull_target = match state.target_kind {
                AttentionTarget::Panel(p) | AttentionTarget::SubsystemEvent(p) => {
                    let delta = p - rest_p;
                    delta.normalize_or(Vec3::Z)
                        * (delta.length() * 0.16).clamp(0.08, pers.pull_gain)
                        * att_scale
                }
                AttentionTarget::OperatorCamera(_) if selected_head == Some(h) => {
                    Vec3::new(
                        (camera_eye.x - rest_p.x).clamp(-1.0, 1.0) * 0.10,
                        0.04,
                        0.14,
                    ) * att_scale
                }
                _ => Vec3::ZERO,
            };

            if cfg.reduced_motion {
                state.look_pos = raw_target;
                state.look_vel = Vec3::ZERO;
                state.pull_pos = pull_target * 0.65;
                state.pull_vel = Vec3::ZERO;
            } else {
                let omega = pers.stiffness * (0.65 + 0.35 * att_scale);
                damp_vec3(
                    &mut state.look_pos,
                    &mut state.look_vel,
                    raw_target,
                    omega,
                    dt,
                );
                damp_vec3(
                    &mut state.pull_pos,
                    &mut state.pull_vel,
                    pull_target,
                    omega * 0.72,
                    dt,
                );
            }

            let roll_wave = if cfg.reduced_motion {
                0.0
            } else {
                pers.tilt_bias * (0.65 + 0.35 * (time * 0.45 + h as f32).sin()) * anim_scale
            };

            let jaw_extra = if matches!(state.target_kind, AttentionTarget::Panel(_))
                || selected_head == Some(h)
                || event_active
            {
                pers.jaw_readiness
            } else {
                0.0
            };

            if matches!(
                state.target_kind,
                AttentionTarget::Panel(_) | AttentionTarget::SubsystemEvent(_)
            ) || selected_head == Some(h)
            {
                aggregate_pull_x += (state.look_pos.x - rest_p.x).clamp(-2.5, 2.5);
                active_pulls += 1.0;
            }

            pose.heads[h] = HeadAwarenessOutput {
                look_target: Some(rest_look.lerp(state.look_pos, att_scale.clamp(0.0, 1.0))),
                neck_pull: state.pull_pos * anim_scale.clamp(0.25, 1.35),
                gaze_bias: [0.0, 0.0],
                jaw_extra,
                pitch_extra: 0.0,
                roll_extra: roll_wave,
            };
        }

        // Restrained, physically coherent whole-body awareness compensation:
        // subtly acknowledges orbiting camera azimuth without turning the creature flatly toward camera.
        let cam_azimuth = camera_eye.x.atan2(camera_eye.z.max(1.2));
        let mean_pull_x = if active_pulls > 0.0 {
            aggregate_pull_x / active_pulls
        } else {
            0.0
        };
        let target_thorax_yaw =
            ((cam_azimuth * 0.16) + (mean_pull_x * 0.06)).clamp(-0.26, 0.26) * body_scale;
        let target_thorax_pitch = ((camera_eye.y - 0.34) * 0.025).clamp(-0.07, 0.07) * body_scale;
        let target_tail_yaw = (-target_thorax_yaw * 1.15).clamp(-0.32, 0.32);
        let target_wing_l = (-target_thorax_yaw * 0.42).clamp(-0.16, 0.16);
        let target_wing_r = (target_thorax_yaw * 0.42).clamp(-0.16, 0.16);

        if cfg.reduced_motion {
            self.thorax_yaw = target_thorax_yaw * 0.5;
            self.thorax_pitch = target_thorax_pitch * 0.5;
            self.tail_yaw = target_tail_yaw * 0.5;
            self.wing_l = target_wing_l * 0.5;
            self.wing_r = target_wing_r * 0.5;
        } else {
            let body_omega = 3.6;
            damp_f32(
                &mut self.thorax_yaw,
                &mut self.thorax_yaw_vel,
                target_thorax_yaw,
                body_omega,
                dt,
            );
            damp_f32(
                &mut self.thorax_pitch,
                &mut self.thorax_pitch_vel,
                target_thorax_pitch,
                body_omega,
                dt,
            );
            damp_f32(
                &mut self.tail_yaw,
                &mut self.tail_yaw_vel,
                target_tail_yaw,
                2.9,
                dt,
            );
            damp_f32(
                &mut self.wing_l,
                &mut self.wing_l_vel,
                target_wing_l,
                3.2,
                dt,
            );
            damp_f32(
                &mut self.wing_r,
                &mut self.wing_r_vel,
                target_wing_r,
                3.2,
                dt,
            );
        }

        pose.body = BodyCompensation {
            thorax_yaw: self.thorax_yaw,
            thorax_pitch: self.thorax_pitch,
            thorax_roll: (-self.thorax_yaw * 0.14).clamp(-0.06, 0.06),
            thorax_shift: Vec3::new(self.thorax_yaw * 0.14, 0.0, self.thorax_pitch.abs() * 0.04),
            wing_asym_l: self.wing_l,
            wing_asym_r: self.wing_r,
            tail_counter_yaw: self.tail_yaw,
        };

        pose
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leviathan_3d_math::LeviathanSkeleton;

    fn rest_heads() -> [Vec3; 7] {
        let specs = LeviathanSkeleton::head_rest_specs();
        std::array::from_fn(|i| specs[i].3)
    }

    #[test]
    fn personalities_have_distinct_timing_and_weights() {
        for i in 0..7 {
            assert!(!HEAD_PERSONALITIES[i].name.is_empty());
            assert!(HEAD_PERSONALITIES[i].min_dwell_s < HEAD_PERSONALITIES[i].max_dwell_s);
        }
        assert_ne!(
            HEAD_PERSONALITIES[1].stiffness,
            HEAD_PERSONALITIES[5].stiffness
        );
    }

    #[test]
    fn dwell_hysteresis_prevents_rapid_target_thrashing() {
        let mut engine = AwarenessEngine::new();
        let rest = rest_heads();
        let cam = Vec3::new(0.0, 0.34, 7.85);
        let cfg = AwarenessConfig::default();

        // Advance one small step to settle initial target
        engine.step(
            0.016,
            0.1,
            cam,
            None,
            &[None; 7],
            &rest,
            &[0.0; 7],
            &[false; 7],
            &cfg,
        );
        let initial: Vec<_> = (0..7).map(|i| engine.current_target(i)).collect();

        // 10 rapid sub-second ticks (0.2s total) should preserve unselected idle targets cleanly
        for step in 0..10 {
            engine.step(
                0.02,
                0.2 + step as f32 * 0.02,
                cam,
                None,
                &[None; 7],
                &rest,
                &[0.0; 7],
                &[false; 7],
                &cfg,
            );
        }
        let after: Vec<_> = (0..7).map(|i| engine.current_target(i)).collect();
        assert_eq!(initial, after, "heads switched idle targets too rapidly");
    }

    #[test]
    fn critically_damped_tracking_converges_smoothly_to_moved_panel() {
        let mut engine = AwarenessEngine::new();
        let rest = rest_heads();
        let cam = Vec3::new(0.0, 0.34, 7.85);
        let cfg = AwarenessConfig::default();
        let mut panels = [None; 7];
        let panel_a = Vec3::new(-1.2, 1.4, 2.0);
        panels[1] = Some(panel_a);

        let mut pose = AwarenessPose::default();
        for k in 0..90 {
            pose = engine.step(
                0.016,
                k as f32 * 0.016,
                cam,
                Some(1),
                &panels,
                &rest,
                &[0.0; 7],
                &[false; 7],
                &cfg,
            );
        }
        let d_a = (pose.heads[1].look_target.unwrap() - panel_a).length();
        assert!(
            d_a < 0.08,
            "head 1 did not converge toward panel_a: d={d_a}"
        );

        // Reposition panel interactively and verify smooth convergence to new target
        let panel_b = Vec3::new(-3.4, 0.8, 1.8);
        panels[1] = Some(panel_b);
        let first_step = engine.step(
            0.016,
            1.5,
            cam,
            Some(1),
            &panels,
            &rest,
            &[0.0; 7],
            &[false; 7],
            &cfg,
        );
        // Should move smoothly rather than snapping instantaneously in 16 ms
        assert!((first_step.heads[1].look_target.unwrap() - panel_b).length() > 1.0);
        for k in 0..90 {
            pose = engine.step(
                0.016,
                1.52 + k as f32 * 0.016,
                cam,
                Some(1),
                &panels,
                &rest,
                &[0.0; 7],
                &[false; 7],
                &cfg,
            );
        }
        let d_b = (pose.heads[1].look_target.unwrap() - panel_b).length();
        assert!(
            d_b < 0.08,
            "head 1 did not converge toward moved panel_b: d={d_b}"
        );
    }

    #[test]
    fn full_orbit_keeps_thorax_rotation_bounded_and_finite() {
        let mut engine = AwarenessEngine::new();
        let rest = rest_heads();
        let cfg = AwarenessConfig::default();
        for deg in (0..360).step_by(15) {
            let rad = (deg as f32).to_radians();
            let cam = Vec3::new(rad.sin() * 7.8, 0.5, rad.cos() * 7.8);
            let pose = engine.step(
                0.03,
                deg as f32 * 0.02,
                cam,
                None,
                &[None; 7],
                &rest,
                &[0.0; 7],
                &[false; 7],
                &cfg,
            );
            assert!(pose.body.thorax_yaw.is_finite());
            assert!(
                pose.body.thorax_yaw.abs() <= 0.30,
                "thorax rotated excessively: {}",
                pose.body.thorax_yaw
            );
            assert!(pose.body.tail_counter_yaw.is_finite());
        }
    }
}
