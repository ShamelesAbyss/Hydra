//! Pure-Rust 3D math, camera choreography, and hierarchical 96-bone articulated
//! skeleton for **HYDRA: Project Incarnation**.
//!
//! Coordinate system: Right-handed world space (`+X` right, `+Y` up toward crowns,
//! `+Z` toward the viewer, `-Z` into the gothic obsidian sanctuary).
//! Projection uses WGPU clip-space depth `Z in [0, 1]`.
//!
//! Operational truthfulness:
//! - Breathing, heartbeat, staggered blinking, tail coil, and subtle head glance
//!   sway are explicitly decorative idle anatomy.
//! - Jaw aperture, shield posture, eye intensity, and cranial flare react strictly
//!   to real observed subsystem activity, alerts, and selection state.

use eframe::egui::{Pos2, Rect, Vec2 as EguiVec2};
use std::f32::consts::PI;

pub const BONE_COUNT: usize = 96;

pub const BONE_ROOT: usize = 0;
pub const BONE_THORAX: usize = 1;
pub const BONE_HEART: usize = 2;
pub const BONE_ARM_L_SH: usize = 3;
pub const BONE_ARM_L_EL: usize = 4;
pub const BONE_ARM_L_PAW: usize = 5;
pub const BONE_ARM_R_SH: usize = 6;
pub const BONE_ARM_R_EL: usize = 7;
pub const BONE_ARM_R_PAW: usize = 8;
pub const BONE_WING_L_SH: usize = 9;
pub const BONE_WING_L_EL: usize = 10;
pub const BONE_WING_L_TIP0: usize = 11;
pub const BONE_WING_L_TIP1: usize = 12;
pub const BONE_WING_L_TIP2: usize = 13;
pub const BONE_WING_L_TIP3: usize = 14;
pub const BONE_WING_R_SH: usize = 15;
pub const BONE_WING_R_EL: usize = 16;
pub const BONE_WING_R_TIP0: usize = 17;
pub const BONE_WING_R_TIP1: usize = 18;
pub const BONE_WING_R_TIP2: usize = 19;
pub const BONE_WING_R_TIP3: usize = 20;
pub const BONE_TAIL_BASE: usize = 21; // 21..=26 (6 tail bones)
pub const BONE_HEAD_BASE: usize = 27; // 27 + head * 8 + [0..5=neck, 6=skull, 7=jaw] -> 27..82
pub const NECK_BONES_PER_HEAD: usize = 6;
pub const BONES_PER_HEAD: usize = 8;

#[inline]
pub const fn head_neck_bone(head: usize, seg: usize) -> usize {
    BONE_HEAD_BASE + head * BONES_PER_HEAD + seg
}

#[inline]
pub const fn head_skull_bone(head: usize) -> usize {
    BONE_HEAD_BASE + head * BONES_PER_HEAD + 6
}

#[inline]
pub const fn head_jaw_bone(head: usize) -> usize {
    BONE_HEAD_BASE + head * BONES_PER_HEAD + 7
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[inline]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    #[inline]
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    #[inline]
    pub fn len_sq(self) -> f32 {
        self.dot(self)
    }

    #[inline]
    pub fn length(self) -> f32 {
        self.len_sq().sqrt()
    }

    #[inline]
    pub fn normalize_or(self, fallback: Self) -> Self {
        let l = self.length();
        if l > 1e-6 && l.is_finite() {
            self * (1.0 / l)
        } else {
            fallback
        }
    }

    #[inline]
    pub fn lerp(self, o: Self, t: f32) -> Self {
        self + (o - self) * t
    }

    #[inline]
    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, r: Self) -> Self {
        Self::new(self.x + r.x, self.y + r.y, self.z + r.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, r: Self) -> Self {
        Self::new(self.x - r.x, self.y - r.y, self.z - r.z)
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vec4 {
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    #[inline]
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let half = angle * 0.5;
        let (s, c) = half.sin_cos();
        let a = axis.normalize_or(Vec3::Y);
        Self {
            x: a.x * s,
            y: a.y * s,
            z: a.z * s,
            w: c,
        }
    }

    /// Intrinsic YXZ (yaw, pitch, roll) orientation.
    #[inline]
    pub fn from_euler_yxz(yaw: f32, pitch: f32, roll: f32) -> Self {
        let qy = Self::from_axis_angle(Vec3::Y, yaw);
        let qx = Self::from_axis_angle(Vec3::X, pitch);
        let qz = Self::from_axis_angle(Vec3::Z, roll);
        (qy * qx * qz).normalize()
    }

    #[inline]
    pub fn normalize(self) -> Self {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if l > 1e-6 && l.is_finite() {
            let inv = 1.0 / l;
            Self {
                x: self.x * inv,
                y: self.y * inv,
                z: self.z * inv,
                w: self.w * inv,
            }
        } else {
            Self::IDENTITY
        }
    }

    /// Shortest-arc quaternion aligning `from` unit direction onto `to` unit direction.
    pub fn from_rotation_arc(from: Vec3, to: Vec3) -> Self {
        let f = from.normalize_or(Vec3::Y);
        let t = to.normalize_or(Vec3::Y);
        let d = f.dot(t);
        if d > 0.9999 {
            return Self::IDENTITY;
        }
        if d < -0.9999 {
            let ortho = if f.x.abs() < 0.8 { Vec3::X } else { Vec3::Z };
            let axis = f.cross(ortho).normalize_or(Vec3::X);
            return Self::from_axis_angle(axis, PI);
        }
        let c = f.cross(t);
        Self {
            x: c.x,
            y: c.y,
            z: c.z,
            w: 1.0 + d,
        }
        .normalize()
    }

    #[inline]
    pub fn rotate_vec3(self, v: Vec3) -> Vec3 {
        let qv = Vec3::new(self.x, self.y, self.z);
        let t = qv.cross(v) * 2.0;
        v + t * self.w + qv.cross(t)
    }
}

impl std::ops::Mul for Quat {
    type Output = Self;
    #[inline]
    fn mul(self, r: Self) -> Self {
        Self {
            x: self.w * r.x + self.x * r.w + self.y * r.z - self.z * r.y,
            y: self.w * r.y - self.x * r.z + self.y * r.w + self.z * r.x,
            z: self.w * r.z + self.x * r.y - self.y * r.x + self.z * r.w,
            w: self.w * r.w - self.x * r.x - self.y * r.y - self.z * r.z,
        }
    }
}

/// Column-major 4x4 matrix matching WGSL `mat4x4<f32>`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    #[inline]
    pub fn from_translation(t: Vec3) -> Self {
        Self {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [t.x, t.y, t.z, 1.0],
            ],
        }
    }

    pub fn from_trs(t: Vec3, q: Quat, s: Vec3) -> Self {
        let q = q.normalize();
        let x2 = q.x + q.x;
        let y2 = q.y + q.y;
        let z2 = q.z + q.z;
        let xx = q.x * x2;
        let xy = q.x * y2;
        let xz = q.x * z2;
        let yy = q.y * y2;
        let yz = q.y * z2;
        let zz = q.z * z2;
        let wx = q.w * x2;
        let wy = q.w * y2;
        let wz = q.w * z2;

        Self {
            cols: [
                [
                    (1.0 - (yy + zz)) * s.x,
                    (xy + wz) * s.x,
                    (xz - wy) * s.x,
                    0.0,
                ],
                [
                    (xy - wz) * s.y,
                    (1.0 - (xx + zz)) * s.y,
                    (yz + wx) * s.y,
                    0.0,
                ],
                [
                    (xz + wy) * s.z,
                    (yz - wx) * s.z,
                    (1.0 - (xx + yy)) * s.z,
                    0.0,
                ],
                [t.x, t.y, t.z, 1.0],
            ],
        }
    }

    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let f = (target - eye).normalize_or(Vec3::new(0.0, 0.0, -1.0));
        let s = f.cross(up).normalize_or(Vec3::X);
        let u = s.cross(f);
        Self {
            cols: [
                [s.x, u.x, -f.x, 0.0],
                [s.y, u.y, -f.y, 0.0],
                [s.z, u.z, -f.z, 0.0],
                [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
            ],
        }
    }

    /// Right-handed perspective matrix for WGPU `Z in [0, 1]`.
    pub fn perspective_rh(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Self {
        let f = 1.0 / (fov_y_rad.clamp(0.1, 2.8) * 0.5).tan();
        let nf = 1.0 / (near - far);
        Self {
            cols: [
                [f / aspect.max(0.1), 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, far * nf, -1.0],
                [0.0, 0.0, near * far * nf, 0.0],
            ],
        }
    }

    /// Right-handed orthographic projection for WGPU `Z in [0, 1]` shadow map.
    pub fn ortho_rh(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        let rml = right - left;
        let tmb = top - bottom;
        let nmf = near - far;
        Self {
            cols: [
                [2.0 / rml, 0.0, 0.0, 0.0],
                [0.0, 2.0 / tmb, 0.0, 0.0],
                [0.0, 0.0, 1.0 / nmf, 0.0],
                [
                    -(right + left) / rml,
                    -(top + bottom) / tmb,
                    near / nmf,
                    1.0,
                ],
            ],
        }
    }

    #[inline]
    pub fn mul_mat4(self, rhs: Self) -> Self {
        let mut out = [[0.0f32; 4]; 4];
        for c in 0..4 {
            for r in 0..4 {
                out[c][r] = self.cols[0][r] * rhs.cols[c][0]
                    + self.cols[1][r] * rhs.cols[c][1]
                    + self.cols[2][r] * rhs.cols[c][2]
                    + self.cols[3][r] * rhs.cols[c][3];
            }
        }
        Self { cols: out }
    }

    #[inline]
    pub fn mul_vec4(self, v: Vec4) -> Vec4 {
        Vec4::new(
            self.cols[0][0] * v.x
                + self.cols[1][0] * v.y
                + self.cols[2][0] * v.z
                + self.cols[3][0] * v.w,
            self.cols[0][1] * v.x
                + self.cols[1][1] * v.y
                + self.cols[2][1] * v.z
                + self.cols[3][1] * v.w,
            self.cols[0][2] * v.x
                + self.cols[1][2] * v.y
                + self.cols[2][2] * v.z
                + self.cols[3][2] * v.w,
            self.cols[0][3] * v.x
                + self.cols[1][3] * v.y
                + self.cols[2][3] * v.z
                + self.cols[3][3] * v.w,
        )
    }

    pub fn write_le_bytes(&self, dst: &mut [u8]) {
        let mut i = 0;
        for col in &self.cols {
            for &val in col {
                let s = if val.is_finite() { val } else { 0.0 };
                dst[i..i + 4].copy_from_slice(&s.to_le_bytes());
                i += 4;
            }
        }
    }
}

#[inline]
pub fn cubic_bezier_v3(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t)
}

#[inline]
pub fn cubic_bezier_tangent_v3(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    ((p1 - p0) * (3.0 * u * u) + (p2 - p1) * (6.0 * u * t) + (p3 - p2) * (3.0 * t * t))
        .normalize_or(Vec3::Y)
}

// ============================================================================
// Camera State & Smooth Focus Choreography
// ============================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct LeviathanCamera {
    pub eye: Vec3,
    pub target: Vec3,
    pub fov_y_rad: f32,
}

impl Default for LeviathanCamera {
    fn default() -> Self {
        Self::overview(1.65)
    }
}

impl LeviathanCamera {
    /// Default Sovereign Overview framing all seven heads, wings, claws, Heart, and dais.
    pub fn overview(aspect: f32) -> Self {
        // Widen FOV gently on compact/narrow screens so outer heads stay cleanly framed.
        let fov_deg = if aspect < 1.55 {
            48.0 + ((1.55 - aspect) * 20.0).clamp(0.0, 14.0)
        } else {
            48.0
        };
        Self {
            eye: Vec3::new(0.0, 0.34, 7.85),
            target: Vec3::new(0.0, 0.05, 0.05),
            fov_y_rad: fov_deg.to_radians(),
        }
    }

    /// Heroic 3/4 anatomical close-up on head `0..=6` showcasing snout profile, fangs,
    /// eye socket, crown horns, and overlapping neck scales with clean headroom.
    pub fn focused_for_head(head_idx: usize, head_world: Vec3, aspect: f32) -> Self {
        let side_x = if head_idx < 3 {
            1.36
        } else if head_idx > 3 {
            -1.36
        } else {
            1.08
        };
        let eye_offset = Vec3::new(side_x, 0.34, 2.86);
        let target = head_world + Vec3::new(0.0, 0.06, 0.08);
        let fov_deg: f32 = if aspect < 1.4 { 47.0 } else { 43.0 };
        Self {
            eye: head_world + eye_offset,
            target,
            fov_y_rad: fov_deg.to_radians(),
        }
    }

    /// Advance camera smoothly toward overview or focused head target.
    pub fn update(
        &mut self,
        focus_head: Option<usize>,
        skeleton: &LeviathanSkeleton,
        aspect: f32,
        dt: f32,
        motion: bool,
    ) {
        let desired = match focus_head.filter(|&i| i < 7) {
            Some(idx) => {
                let hp = skeleton.joint_pos[head_skull_bone(idx)];
                Self::focused_for_head(idx, hp, aspect)
            }
            None => Self::overview(aspect),
        };
        if !motion {
            *self = desired;
            return;
        }
        let alpha = 1.0 - (-dt.clamp(0.001, 0.1) * 7.5).exp();
        self.eye = self.eye.lerp(desired.eye, alpha);
        self.target = self.target.lerp(desired.target, alpha);
        self.fov_y_rad += (desired.fov_y_rad - self.fov_y_rad) * alpha;
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, Vec3::Y)
    }

    pub fn proj_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y_rad, aspect, 0.2, 65.0)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.proj_matrix(aspect).mul_mat4(self.view_matrix())
    }

    /// Project a 3D world point to screen `Pos2` and normalized `[0..1]` UV inside `rect`.
    pub fn project_point(&self, world: Vec3, rect: Rect) -> Option<(Pos2, [f32; 2], f32)> {
        let aspect = (rect.width() / rect.height().max(1.0)).max(0.25);
        let clip = self
            .view_proj(aspect)
            .mul_vec4(Vec4::new(world.x, world.y, world.z, 1.0));
        if clip.w <= 1e-4 || !clip.w.is_finite() {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;
        let ndc_z = clip.z / clip.w;
        let u = ndc_x * 0.5 + 0.5;
        let v = 0.5 - ndc_y * 0.5;
        let pos = Pos2::new(
            rect.left() + u * rect.width(),
            rect.top() + v * rect.height(),
        );
        Some((pos, [u, v], ndc_z))
    }
}

// ============================================================================
// Hierarchical Articulated 96-Bone Skeleton
// ============================================================================

#[derive(Clone, Debug)]
pub struct SkeletonParams {
    pub time: f32,
    pub motion: bool,
    pub breath: f32,
    pub heart_pulse: f32,
    pub pointer_world: Option<Vec3>,
    pub head_activity: [f32; 7],
    pub head_warning: [bool; 7],
    pub head_emphasis: [f32; 7],
    pub head_eye_open: [f32; 7],
    pub selected: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct LeviathanSkeleton {
    pub bind_pos: [Vec3; BONE_COUNT],
    pub joint_pos: [Vec3; BONE_COUNT],
    pub skin_mats: [Mat4; BONE_COUNT],
    pub head_look: [[f32; 2]; 7],
}

impl LeviathanSkeleton {
    /// Canonical rest pose in right-handed 3D world coordinates.
    pub fn rest_bind_positions() -> [Vec3; BONE_COUNT] {
        let mut p = [Vec3::ZERO; BONE_COUNT];
        p[BONE_ROOT] = Vec3::new(0.0, -2.05, 0.0);
        p[BONE_THORAX] = Vec3::new(0.0, -1.08, 0.08);
        p[BONE_HEART] = Vec3::new(0.0, -1.05, 0.54);

        // Forelimbs gripping obsidian dais
        p[BONE_ARM_L_SH] = Vec3::new(-0.92, -1.08, 0.22);
        p[BONE_ARM_L_EL] = Vec3::new(-1.54, -1.52, 0.48);
        p[BONE_ARM_L_PAW] = Vec3::new(-1.40, -1.98, 0.72);
        p[BONE_ARM_R_SH] = Vec3::new(0.92, -1.08, 0.22);
        p[BONE_ARM_R_EL] = Vec3::new(1.54, -1.52, 0.48);
        p[BONE_ARM_R_PAW] = Vec3::new(1.40, -1.98, 0.72);

        // Articulated sweeping gothic wings soaring behind all seven necks
        p[BONE_WING_L_SH] = Vec3::new(-0.62, -0.78, -0.26);
        p[BONE_WING_L_EL] = Vec3::new(-2.72, 1.24, -0.82);
        p[BONE_WING_L_TIP0] = Vec3::new(-5.35, 1.88, -1.42);
        p[BONE_WING_L_TIP1] = Vec3::new(-5.05, 0.45, -1.24);
        p[BONE_WING_L_TIP2] = Vec3::new(-4.02, -0.58, -1.02);
        p[BONE_WING_L_TIP3] = Vec3::new(-2.65, -1.16, -0.80);

        p[BONE_WING_R_SH] = Vec3::new(0.62, -0.78, -0.26);
        p[BONE_WING_R_EL] = Vec3::new(2.72, 1.24, -0.82);
        p[BONE_WING_R_TIP0] = Vec3::new(5.35, 1.88, -1.42);
        p[BONE_WING_R_TIP1] = Vec3::new(5.05, 0.45, -1.24);
        p[BONE_WING_R_TIP2] = Vec3::new(4.02, -0.58, -1.02);
        p[BONE_WING_R_TIP3] = Vec3::new(2.65, -1.16, -0.80);

        // 6-bone coiling armored tail on right flank (clear of Heart caption & memorial)
        let tail_pts = [
            Vec3::new(0.46, -1.42, -0.32),
            Vec3::new(1.25, -1.72, -0.44),
            Vec3::new(2.15, -1.74, -0.22),
            Vec3::new(2.96, -1.48, 0.12),
            Vec3::new(3.58, -1.06, 0.42),
            Vec3::new(4.02, -0.52, 0.62),
        ];
        for (i, &tp) in tail_pts.iter().enumerate() {
            p[BONE_TAIL_BASE + i] = tp;
        }

        // Seven sculpted dragon heads + 6-segment neck curves
        let targets = Self::head_rest_specs();
        for h in 0..7 {
            let (p0, c1, c2, skull) = targets[h];
            for seg in 0..NECK_BONES_PER_HEAD {
                let t = seg as f32 / (NECK_BONES_PER_HEAD as f32);
                p[head_neck_bone(h, seg)] = cubic_bezier_v3(p0, c1, c2, skull, t);
            }
            p[head_skull_bone(h)] = skull;
            // TMJ lower-jaw hinge just below/back from the cranium center
            p[head_jaw_bone(h)] = skull + Vec3::new(0.0, -0.10, 0.12);
        }

        p
    }

    #[inline]
    pub fn head_rest_specs() -> [(Vec3, Vec3, Vec3, Vec3); 7] {
        [
            // 0: HUNTER (Watcher — outermost left, agile crane)
            (
                Vec3::new(-0.62, -0.60, 0.04),
                Vec3::new(-1.55, -0.25, 0.12),
                Vec3::new(-2.95, 0.12, 0.16),
                Vec3::new(-3.62, 0.56, 0.10),
            ),
            // 1: FANG (Predator — muscular coiled strike posture)
            (
                Vec3::new(-0.44, -0.50, 0.14),
                Vec3::new(-1.12, 0.05, 0.30),
                Vec3::new(-1.95, 0.72, 0.42),
                Vec3::new(-2.42, 1.32, 0.32),
            ),
            // 2: COGNITION (Mind — lofty philosophical arch)
            (
                Vec3::new(-0.24, -0.40, 0.22),
                Vec3::new(-0.66, 0.38, 0.40),
                Vec3::new(-1.02, 1.18, 0.58),
                Vec3::new(-1.24, 1.84, 0.52),
            ),
            // 3: AEGIS (Guardian — sovereign central apex, commanding foreground)
            (
                Vec3::new(0.00, -0.34, 0.28),
                Vec3::new(0.00, 0.45, 0.54),
                Vec3::new(0.00, 1.35, 0.80),
                Vec3::new(0.00, 2.08, 0.76),
            ),
            // 4: NEXUS (Executor — poised forward precision)
            (
                Vec3::new(0.24, -0.40, 0.22),
                Vec3::new(0.66, 0.38, 0.40),
                Vec3::new(1.02, 1.18, 0.58),
                Vec3::new(1.24, 1.84, 0.52),
            ),
            // 5: ORACLE (Seer — serene contemplative arc)
            (
                Vec3::new(0.44, -0.50, 0.14),
                Vec3::new(1.12, 0.05, 0.30),
                Vec3::new(1.95, 0.72, 0.42),
                Vec3::new(2.42, 1.32, 0.32),
            ),
            // 6: GENESIS (Architect — wide noble sweep)
            (
                Vec3::new(0.62, -0.60, 0.04),
                Vec3::new(1.55, -0.25, 0.12),
                Vec3::new(2.95, 0.12, 0.16),
                Vec3::new(3.62, 0.56, 0.10),
            ),
        ]
    }

    pub fn evaluate(params: &SkeletonParams) -> Self {
        let bind_pos = Self::rest_bind_positions();
        let mut joint_pos = bind_pos;
        let mut skin_mats = [Mat4::IDENTITY; BONE_COUNT];
        let mut head_look = [[0.0f32; 2]; 7];

        let t = if params.motion && params.time.is_finite() {
            params.time.rem_euclid(3600.0)
        } else {
            0.0
        };
        let b = if params.breath.is_finite() {
            params.breath.clamp(0.0, 1.0)
        } else {
            0.45
        };
        let pulse = if params.heart_pulse.is_finite() {
            params.heart_pulse.clamp(0.0, 1.0)
        } else {
            0.32
        };

        let breath_off = (b - 0.45) * 0.055;
        let thorax_scale = Vec3::new(
            1.0 + (b - 0.45) * 0.045,
            1.0 + (b - 0.45) * 0.032,
            1.0 + (b - 0.45) * 0.055,
        );
        skin_mats[BONE_THORAX] = Mat4::from_trs(
            bind_pos[BONE_THORAX] + Vec3::new(0.0, breath_off * 0.4, 0.0),
            Quat::IDENTITY,
            thorax_scale,
        )
        .mul_mat4(Mat4::from_translation(-bind_pos[BONE_THORAX]));

        // Beating Amethyst Heart
        let heart_scale = Vec3::ONE * (1.0 + (pulse - 0.32) * 0.095);
        joint_pos[BONE_HEART] = bind_pos[BONE_HEART] + Vec3::new(0.0, breath_off * 0.35, 0.02);
        skin_mats[BONE_HEART] = Mat4::from_trs(joint_pos[BONE_HEART], Quat::IDENTITY, heart_scale)
            .mul_mat4(Mat4::from_translation(-bind_pos[BONE_HEART]));

        // Forelimbs breathe slightly at shoulders while claws remain planted on dais
        for &(sh, el) in &[
            (BONE_ARM_L_SH, BONE_ARM_L_EL),
            (BONE_ARM_R_SH, BONE_ARM_R_EL),
        ] {
            joint_pos[sh] = bind_pos[sh] + Vec3::new(0.0, breath_off * 0.5, 0.0);
            joint_pos[el] = bind_pos[el] + Vec3::new(0.0, breath_off * 0.25, 0.0);
            skin_mats[sh] = Mat4::from_translation(joint_pos[sh] - bind_pos[sh]);
            skin_mats[el] = Mat4::from_translation(joint_pos[el] - bind_pos[el]);
        }

        // Wings lift organically with breath + faint secondary tip lag
        let wing_lift = (b - 0.45) * 0.24;
        let wing_lag = if params.motion {
            (t * 1.18 - 0.45).sin() * 0.14
        } else {
            0.0
        };
        for &(sh, el, t0, t1, t2, t3, side) in &[
            (
                BONE_WING_L_SH,
                BONE_WING_L_EL,
                BONE_WING_L_TIP0,
                BONE_WING_L_TIP1,
                BONE_WING_L_TIP2,
                BONE_WING_L_TIP3,
                -1.0_f32,
            ),
            (
                BONE_WING_R_SH,
                BONE_WING_R_EL,
                BONE_WING_R_TIP0,
                BONE_WING_R_TIP1,
                BONE_WING_R_TIP2,
                BONE_WING_R_TIP3,
                1.0_f32,
            ),
        ] {
            let sh_pos = bind_pos[sh] + Vec3::new(0.0, breath_off * 0.7, 0.0);
            let el_pos = bind_pos[el] + Vec3::new(side * 0.06, wing_lift * 0.9, wing_lift * 0.35);
            let t0_pos =
                bind_pos[t0] + Vec3::new(side * 0.12, wing_lift * 1.4 + wing_lag, wing_lift * 0.6);
            let t1_pos =
                bind_pos[t1] + Vec3::new(side * 0.09, wing_lift * 0.95 + wing_lag * 0.8, 0.0);
            let t2_pos = bind_pos[t2] + Vec3::new(side * 0.05, wing_lift * 0.45, 0.0);
            let t3_pos = bind_pos[t3] + Vec3::new(side * 0.02, wing_lift * 0.2, 0.0);
            let el_rot = Quat::from_axis_angle(Vec3::Z, -side * wing_lift * 0.22);
            joint_pos[sh] = sh_pos;
            joint_pos[el] = el_pos;
            joint_pos[t0] = t0_pos;
            joint_pos[t1] = t1_pos;
            joint_pos[t2] = t2_pos;
            joint_pos[t3] = t3_pos;
            skin_mats[sh] = Mat4::from_translation(sh_pos - bind_pos[sh]);
            skin_mats[el] = Mat4::from_trs(el_pos, el_rot, Vec3::ONE)
                .mul_mat4(Mat4::from_translation(-bind_pos[el]));
            for (bone, pos) in [(t0, t0_pos), (t1, t1_pos), (t2, t2_pos), (t3, t3_pos)] {
                skin_mats[bone] = Mat4::from_trs(pos, el_rot, Vec3::ONE)
                    .mul_mat4(Mat4::from_translation(-bind_pos[bone]));
            }
        }

        // Tail undulating gracefully along right flank
        for i in 0..6 {
            let bone = BONE_TAIL_BASE + i;
            let amp = (i as f32 / 5.0).powf(1.3);
            let wave_x = if params.motion {
                (t * 0.82 - i as f32 * 0.55).sin() * 0.11 * amp
            } else {
                0.0
            };
            let wave_y = if params.motion {
                (t * 1.05 - i as f32 * 0.48).cos() * 0.08 * amp
            } else {
                0.0
            };
            let pos = bind_pos[bone] + Vec3::new(wave_x, wave_y, wave_x * 0.5);
            joint_pos[bone] = pos;
            skin_mats[bone] = Mat4::from_translation(pos - bind_pos[bone]);
        }

        // Seven individually articulated necks, skulls, and hinged jaws
        let specs = Self::head_rest_specs();
        // Distinct behavioral cadence multipliers per personality
        let tempos = [0.68_f32, 0.84, 0.44, 0.36, 0.56, 0.40, 0.52];
        let reactivity = [0.95_f32, 0.88, 0.72, 0.58, 0.82, 0.65, 0.78];

        for h in 0..7 {
            let (p0_rest, c1_rest, c2_rest, skull_rest) = specs[h];
            let spread = (h as f32 - 3.0) / 3.0;
            let phase = h as f32 * 1.618 + 0.35;
            let tempo = tempos[h];
            let act = params.head_activity[h].clamp(0.0, 1.0);
            let warn = if params.head_warning[h] { 1.0 } else { 0.0 };
            let emph = params.head_emphasis[h].clamp(0.0, 1.0);

            // Subtle idle organic head float + natural neighboring glances
            let sway_x = if params.motion {
                (t * tempo + phase).sin() * 0.11 + (t * tempo * 1.9 + phase * 1.3).sin() * 0.03
            } else {
                0.0
            };
            let sway_y = if params.motion {
                (t * tempo * 1.23 + phase).cos() * 0.075 - (b - 0.45) * 0.09
            } else {
                0.0
            };
            let sway_z = if params.motion {
                (t * tempo * 0.85 + phase * 0.7).sin() * 0.07
            } else {
                0.0
            };

            // Selection acknowledgement nod / poised lean forward
            let ack_forward = emph * 0.14 + act * 0.10;
            let p0 = p0_rest + Vec3::new(0.0, breath_off * 0.6, 0.0);
            let c1 = c1_rest + Vec3::new(sway_x * 0.42, sway_y * 0.35, sway_z * 0.45);
            let c2 = c2_rest
                + Vec3::new(
                    sway_x * 0.78,
                    sway_y * 0.72,
                    sway_z * 0.8 + ack_forward * 0.5,
                );
            let skull_live = skull_rest + Vec3::new(sway_x, sway_y, sway_z + ack_forward);

            // Gaze & head orientation toward cursor (when in canvas) or contemplative neighbor scan
            let (gaze_x, gaze_y) = if let Some(ptr) = params.pointer_world {
                let d = ptr - skull_live;
                let nx = (d.x / 3.8).clamp(-1.0, 1.0) * reactivity[h];
                let ny = (d.y / 2.6).clamp(-1.0, 1.0) * reactivity[h] * 0.75;
                (nx, ny)
            } else if params.motion {
                // Slow organic sentinel scanning + subtle glance toward sovereign center / neighbors
                let gx = (t * (0.27 + h as f32 * 0.05) + phase).sin() * 0.45;
                let gy = (t * (0.21 + h as f32 * 0.04) + phase * 1.4).cos() * 0.24;
                (gx, gy)
            } else {
                (0.0, 0.0)
            };
            head_look[h] = [gaze_x, gaze_y];

            // Pitch skull snout downward (+pitch around X) so eyes, brows, nasal crest,
            // crown horns, and curved fangs command both Overview and 3/4 Focus cameras!
            let base_yaw = -spread * 0.22;
            let base_pitch = 0.30 + spread.abs() * 0.04;
            let live_yaw = base_yaw
                + gaze_x * 0.24
                + if params.motion {
                    (t * tempo * 0.7 + phase).sin() * 0.05
                } else {
                    0.0
                };
            let live_pitch = base_pitch - gaze_y * 0.18 + ack_forward * 0.06;
            let live_roll = -spread * 0.08 - gaze_x * 0.08;
            let skull_rot = Quat::from_euler_yxz(live_yaw, live_pitch, live_roll);

            // Compute live neck joints & local ribbon frames
            for seg in 0..NECK_BONES_PER_HEAD {
                let bone = head_neck_bone(h, seg);
                let u = seg as f32 / (NECK_BONES_PER_HEAD as f32);
                let pos_now = cubic_bezier_v3(p0, c1, c2, skull_live, u);
                let dir_rest = cubic_bezier_tangent_v3(p0_rest, c1_rest, c2_rest, skull_rest, u);
                let dir_now = cubic_bezier_tangent_v3(p0, c1, c2, skull_live, u);
                let delta_q = Quat::from_rotation_arc(dir_rest, dir_now);
                joint_pos[bone] = pos_now;
                skin_mats[bone] = Mat4::from_trs(pos_now, delta_q, Vec3::ONE)
                    .mul_mat4(Mat4::from_translation(-bind_pos[bone]));
            }

            let sk_bone = head_skull_bone(h);
            joint_pos[sk_bone] = skull_live;
            skin_mats[sk_bone] = Mat4::from_trs(skull_live, skull_rot, Vec3::ONE)
                .mul_mat4(Mat4::from_translation(-bind_pos[sk_bone]));

            // Hinged Lower Jaw (naturally slightly parted so curved ivory fangs glint even at rest;
            // flares wide strictly on real subsystem activity or alerts!)
            let jaw_bone = head_jaw_bone(h);
            let rest_hinge_offset = bind_pos[jaw_bone] - bind_pos[sk_bone];
            let jaw_world = skull_live + skull_rot.rotate_vec3(rest_hinge_offset);
            let idle_breath_jaw = if params.motion {
                0.08 + (b - 0.45).max(0.0) * 0.04
            } else {
                0.08
            };
            let fang_snarl = if h == 1 { 0.07 } else { 0.0 };
            let jaw_open_rad =
                (idle_breath_jaw + fang_snarl + act * 0.24 + warn * 0.20).clamp(0.0, 0.50);
            // Positive pitch rotates forward snout down (opening the mouth!)
            let jaw_rot = skull_rot * Quat::from_axis_angle(Vec3::X, jaw_open_rad);
            joint_pos[jaw_bone] = jaw_world;
            skin_mats[jaw_bone] = Mat4::from_trs(jaw_world, jaw_rot, Vec3::ONE)
                .mul_mat4(Mat4::from_translation(-bind_pos[jaw_bone]));
        }

        Self {
            bind_pos,
            joint_pos,
            skin_mats,
            head_look,
        }
    }
}

/// Compute accurate 3D-projected hit rectangles and floating badge anchors for all 7 heads.
pub fn projected_head_regions(
    rect: Rect,
    skeleton: &LeviathanSkeleton,
    camera: &LeviathanCamera,
    focus_head: Option<usize>,
) -> ([Rect; 7], [Pos2; 7]) {
    let min_dim = rect.width().min(rect.height()).max(1.0);
    let mut hits = [Rect::NOTHING; 7];
    let mut anchors = [Pos2::ZERO; 7];

    for i in 0..7 {
        let head_pos = skeleton.joint_pos[head_skull_bone(i)];
        let screen_pos = camera
            .project_point(head_pos, rect)
            .map(|(p, _, _)| p)
            .unwrap_or_else(|| rect.center());

        let base_r = match focus_head {
            Some(sel) if sel == i => (min_dim * 0.16).clamp(80.0, 210.0),
            _ => {
                let s = if i == 3 { 0.072 } else { 0.062 };
                (min_dim * s).clamp(44.0, 115.0)
            }
        };
        let box_rect =
            Rect::from_center_size(screen_pos, EguiVec2::new(base_r * 1.9, base_r * 2.1))
                .intersect(rect);
        hits[i] = box_rect;

        let crown_world = head_pos + Vec3::new(0.0, if i == 3 { 1.12 } else { 0.96 }, 0.0);
        let crown_pos = camera
            .project_point(crown_world, rect)
            .map(|(p, _, _)| p)
            .unwrap_or_else(|| screen_pos - EguiVec2::new(0.0, base_r * 1.25));
        anchors[i] = Pos2::new(
            crown_pos.x.clamp(rect.left() + 62.0, rect.right() - 62.0),
            crown_pos.y.clamp(rect.top() + 22.0, rect.bottom() - 40.0),
        );
    }

    (hits, anchors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mat4_camera_and_projection_are_finite_and_in_wgpu_depth_range() {
        let cam = LeviathanCamera::overview(16.0 / 9.0);
        let rect = Rect::from_min_size(Pos2::ZERO, EguiVec2::new(1600.0, 900.0));
        let specs = LeviathanSkeleton::head_rest_specs();
        for (i, (_, _, _, skull)) in specs.iter().enumerate() {
            let (pos, uv, z) = cam
                .project_point(*skull, rect)
                .expect("head visible in overview camera");
            assert!(pos.x.is_finite() && pos.y.is_finite());
            assert!(
                (0.05..=0.95).contains(&uv[0]) && (0.05..=0.95).contains(&uv[1]),
                "head {i} projected outside center margin: {uv:?}"
            );
            assert!(
                (0.0..=1.0).contains(&z),
                "head {i} WGPU NDC z out of [0,1]: {z}"
            );
        }
    }

    #[test]
    fn skeleton_reduced_motion_is_time_invariant_and_finite() {
        let base = SkeletonParams {
            time: 0.0,
            motion: false,
            breath: 0.45,
            heart_pulse: 0.32,
            pointer_world: None,
            head_activity: [0.0; 7],
            head_warning: [false; 7],
            head_emphasis: [0.0; 7],
            head_eye_open: [1.0; 7],
            selected: None,
        };
        let later = SkeletonParams {
            time: 1234.5,
            ..base.clone()
        };
        let s0 = LeviathanSkeleton::evaluate(&base);
        let s1 = LeviathanSkeleton::evaluate(&later);
        assert_eq!(s0.joint_pos, s1.joint_pos);
        assert_eq!(s0.skin_mats, s1.skin_mats);
        for m in &s0.skin_mats {
            for col in &m.cols {
                for &v in col {
                    assert!(v.is_finite());
                }
            }
        }
    }

    #[test]
    fn overview_projected_head_centers_are_well_separated() {
        let cam = LeviathanCamera::overview(16.0 / 10.0);
        let skel = LeviathanSkeleton::evaluate(&SkeletonParams {
            time: 1.5,
            motion: true,
            breath: 0.6,
            heart_pulse: 0.7,
            pointer_world: None,
            head_activity: [0.2; 7],
            head_warning: [false; 7],
            head_emphasis: [0.0; 7],
            head_eye_open: [1.0; 7],
            selected: None,
        });
        let rect = Rect::from_min_size(Pos2::ZERO, EguiVec2::new(1024.0, 560.0));
        let (hits, anchors) = projected_head_regions(rect, &skel, &cam, None);
        for i in 0..7 {
            assert!(rect.contains_rect(hits[i]));
            for j in i + 1..7 {
                assert!(
                    anchors[i].distance(anchors[j]) > 50.0,
                    "anchors {i} and {j} too close: {:?} vs {:?}",
                    anchors[i],
                    anchors[j]
                );
            }
        }
    }
}
