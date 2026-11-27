//! Procedural articulated 3D mesh generator for **HYDRA: Project Incarnation**.
//!
//! Constructs one anatomically cohesive obsidian-and-amethyst seven-headed
//! dragon creature anchored inside a monumental gothic obsidian sanctuary:
//! - Sculpted armored thorax, scapulae, spine ridge, and 6 sternal rib-prongs
//!   physically cradling the faceted central Amethyst Heart.
//! - Clawed muscular forelimbs gripping a stepped octagonal obsidian dais.
//! - Articulated gothic wings with curved bone spars, thumb claws, and smooth
//!   skinned scalloped leather membranes.
//! - 6-bone coiled armored tail with dorsal crest fins.
//! - Seven 6-bone muscular necks blending smoothly from thorax to skull, with
//!   ventral throat scute grooves and dorsal ridge spines.
//! - Seven anatomically distinct 3D sculpted dragon skulls + hinged lower jaws,
//!   overhanging brow ridges, orbital sockets, 3D reptilian eyeballs, ivory
//!   fangs, and unique keratin/crystalline horn crowns per personality.
//! - Seven anatomical energy conduits radiating from the Amethyst Heart.
//! - Towering fluted gothic cathedral pillars and pointed sanctuary arches.

#[cfg(test)]
use crate::leviathan_3d_math::BONE_COUNT;
use crate::leviathan_3d_math::{
    cubic_bezier_tangent_v3, cubic_bezier_v3, head_jaw_bone, head_neck_bone, head_skull_bone,
    LeviathanSkeleton, Vec3, BONE_ARM_L_EL, BONE_ARM_L_PAW, BONE_ARM_L_SH, BONE_ARM_R_EL,
    BONE_ARM_R_PAW, BONE_ARM_R_SH, BONE_HEART, BONE_ROOT, BONE_TAIL_BASE, BONE_THORAX,
    BONE_WING_L_EL, BONE_WING_L_SH, BONE_WING_L_TIP0, BONE_WING_L_TIP1, BONE_WING_L_TIP2,
    BONE_WING_L_TIP3, BONE_WING_R_EL, BONE_WING_R_SH, BONE_WING_R_TIP0, BONE_WING_R_TIP1,
    BONE_WING_R_TIP2, BONE_WING_R_TIP3, NECK_BONES_PER_HEAD,
};
use std::f32::consts::{PI, TAU};

pub const MAT_OBSIDIAN_SCALES: u32 = 0;
pub const MAT_KERATIN_HORN: u32 = 1;
pub const MAT_VENTRAL_SCUTES: u32 = 2;
pub const MAT_WING_MEMBRANE: u32 = 3;
pub const MAT_AMETHYST_HEART: u32 = 4;
pub const MAT_REPTILIAN_EYE: u32 = 5;
pub const MAT_SANCTUARY_STONE: u32 = 6;
pub const MAT_IVORY_FANG: u32 = 7;
pub const MAT_ENERGY_CONDUIT: u32 = 8;
pub const MAT_SHIELD_BARRIER: u32 = 9;

/// Packed 80-byte aligned GPU vertex (`5 x 16 bytes`).
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Vertex3D {
    pub pos: [f32; 3],
    pub pad0: f32,
    pub normal: [f32; 3],
    pub pad1: f32,
    pub tangent: [f32; 4], // xyz + handedness w
    pub uv: [f32; 2],
    pub joints_u16_pad: [u32; 2], // packed: [u16 pair j0|j1<<16, j2|j3<<16] -> clean u32 in WGSL
    pub weights: [f32; 4],
    pub meta: [u32; 4], // [mat_id, head_idx (0..6 or 7=shared), flags, custom]
}

// Recompute compact cleanly: let's keep explicit 96-byte or 80-byte aligned struct matching WGSL @location(0..6):
// Actually in WGPU vertex buffers, VertexAttribute offsets don't require 16-byte struct alignment, only 4-byte multiple!
// Let's keep explicit unpacked [u32; 4] joints for crystal clarity & zero bitpacking bugs:
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct GpuVertex {
    pub pos: [f32; 3],     // offset 0  (12B) -> @location(0) vec3<f32>
    pub normal: [f32; 3],  // offset 12 (12B) -> @location(1) vec3<f32>
    pub uv: [f32; 2],      // offset 24 (8B)  -> @location(2) vec2<f32>
    pub tangent: [f32; 4], // offset 32 (16B) -> @location(3) vec4<f32>
    pub joints: [u32; 4],  // offset 48 (16B) -> @location(4) vec4<u32>
    pub weights: [f32; 4], // offset 64 (16B) -> @location(5) vec4<f32>
    pub meta: [u32; 4],    // offset 80 (16B) -> @location(6) vec4<u32>
}

pub const GPU_VERTEX_STRIDE: usize = 96;

#[derive(Clone, Debug)]
pub struct LeviathanMeshData {
    pub vertices: Vec<GpuVertex>,
    pub opaque_indices: Vec<u32>,
    pub shadow_indices: Vec<u32>,
    pub transparent_indices: Vec<u32>,
    /// Index subrange inside `opaque_indices` for creature (used for shadow pass).
    pub eye_head_offset_count: [(u32, u32); 7],
}

impl LeviathanMeshData {
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.vertices.len() * GPU_VERTEX_STRIDE);
        for v in &self.vertices {
            for f in v.pos {
                out.extend_from_slice(&safe_f(f).to_le_bytes());
            }
            for f in v.normal {
                out.extend_from_slice(&safe_f(f).to_le_bytes());
            }
            for f in v.uv {
                out.extend_from_slice(&safe_f(f).to_le_bytes());
            }
            for f in v.tangent {
                out.extend_from_slice(&safe_f(f).to_le_bytes());
            }
            for u in v.joints {
                out.extend_from_slice(&u.to_le_bytes());
            }
            for f in v.weights {
                out.extend_from_slice(&safe_f(f).to_le_bytes());
            }
            for u in v.meta {
                out.extend_from_slice(&u.to_le_bytes());
            }
        }
        out
    }

    pub fn u32_bytes(indices: &[u32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(indices.len() * 4);
        for &idx in indices {
            out.extend_from_slice(&idx.to_le_bytes());
        }
        out
    }
}

#[inline]
fn safe_f(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

struct MeshBuilder {
    vertices: Vec<GpuVertex>,
    opaque_indices: Vec<u32>,
    shadow_indices: Vec<u32>,
    transparent_indices: Vec<u32>,
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(28_000),
            opaque_indices: Vec::with_capacity(72_000),
            shadow_indices: Vec::with_capacity(64_000),
            transparent_indices: Vec::with_capacity(12_000),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_vertex(
        &mut self,
        pos: Vec3,
        normal: Vec3,
        tangent: [f32; 4],
        uv: [f32; 2],
        joints: [u32; 4],
        weights: [f32; 4],
        mat_id: u32,
        head_idx: u32,
        flags: u32,
    ) -> u32 {
        let n = normal.normalize_or(Vec3::Y);
        let idx = self.vertices.len() as u32;
        self.vertices.push(GpuVertex {
            pos: pos.to_array(),
            normal: n.to_array(),
            uv,
            tangent,
            joints,
            weights: normalize_weights(weights),
            meta: [mat_id, head_idx, flags, 0],
        });
        idx
    }

    fn push_tri(&mut self, a: u32, b: u32, c: u32, cast_shadow: bool, transparent: bool) {
        if transparent {
            self.transparent_indices.extend_from_slice(&[a, b, c]);
        } else {
            self.opaque_indices.extend_from_slice(&[a, b, c]);
            if cast_shadow {
                self.shadow_indices.extend_from_slice(&[a, b, c]);
            }
        }
    }

    /// Recompute smooth area-weighted normals & orthonormal tangents for a triangle range.
    fn compute_smooth_normals_and_tangents(&mut self, start_vert: usize, index_slice: &[u32]) {
        let count = self.vertices.len() - start_vert;
        if count == 0 {
            return;
        }
        let mut accum_n = vec![Vec3::ZERO; count];
        let mut accum_t = vec![Vec3::ZERO; count];

        for tri in index_slice.chunks_exact(3) {
            let (ia, ib, ic) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let pa = Vec3::new(
                self.vertices[ia].pos[0],
                self.vertices[ia].pos[1],
                self.vertices[ia].pos[2],
            );
            let pb = Vec3::new(
                self.vertices[ib].pos[0],
                self.vertices[ib].pos[1],
                self.vertices[ib].pos[2],
            );
            let pc = Vec3::new(
                self.vertices[ic].pos[0],
                self.vertices[ic].pos[1],
                self.vertices[ic].pos[2],
            );
            let e1 = pb - pa;
            let e2 = pc - pa;
            let n = e1.cross(e2);
            let du1 = self.vertices[ib].uv[0] - self.vertices[ia].uv[0];
            let dv1 = self.vertices[ib].uv[1] - self.vertices[ia].uv[1];
            let du2 = self.vertices[ic].uv[0] - self.vertices[ia].uv[0];
            let dv2 = self.vertices[ic].uv[1] - self.vertices[ia].uv[1];
            let det = du1 * dv2 - du2 * dv1;
            let t_dir = if det.abs() > 1e-7 {
                (e1 * dv2 - e2 * dv1) * (1.0 / det)
            } else {
                e1.normalize_or(Vec3::X)
            };
            for idx in [ia, ib, ic] {
                if idx >= start_vert {
                    let local = idx - start_vert;
                    accum_n[local] = accum_n[local] + n;
                    accum_t[local] = accum_t[local] + t_dir;
                }
            }
        }

        for local in 0..count {
            let vi = start_vert + local;
            let n = accum_n[local].normalize_or(Vec3::new(
                self.vertices[vi].normal[0],
                self.vertices[vi].normal[1],
                self.vertices[vi].normal[2],
            ));
            let t_raw = accum_t[local];
            let t_ortho = (t_raw - n * n.dot(t_raw)).normalize_or(if n.y.abs() < 0.9 {
                Vec3::Y.cross(n).normalize_or(Vec3::X)
            } else {
                Vec3::Z.cross(n).normalize_or(Vec3::X)
            });
            self.vertices[vi].normal = n.to_array();
            self.vertices[vi].tangent = [t_ortho.x, t_ortho.y, t_ortho.z, 1.0];
        }
    }
}

fn normalize_weights(w: [f32; 4]) -> [f32; 4] {
    let s = w[0].max(0.0) + w[1].max(0.0) + w[2].max(0.0) + w[3].max(0.0);
    if s > 1e-5 && s.is_finite() {
        [
            w[0].max(0.0) / s,
            w[1].max(0.0) / s,
            w[2].max(0.0) / s,
            w[3].max(0.0) / s,
        ]
    } else {
        [1.0, 0.0, 0.0, 0.0]
    }
}

/// Orthonormal right-handed frame `(n_rad, b_rad, t_dir)` with `n_rad × b_rad = t_dir`.
fn tube_frame(tangent: Vec3, hint_up: Vec3) -> (Vec3, Vec3) {
    let t = tangent.normalize_or(Vec3::Y);
    let up = if t.dot(hint_up).abs() > 0.94 {
        Vec3::X
    } else {
        hint_up
    };
    let n_rad = up.cross(t).normalize_or(Vec3::X);
    let b_rad = t.cross(n_rad).normalize_or(Vec3::Z);
    (n_rad, b_rad)
}

#[derive(Clone, Copy)]
struct RingSample {
    center: Vec3,
    tangent: Vec3,
    n_rad: Vec3,
    b_rad: Vec3,
    rx: f32,
    ry: f32,
    v_uv: f32,
    joints: [u32; 4],
    weights: [f32; 4],
}

/// Build an organic closed or open lofted tube with per-ring bone skinning,
/// custom radial profile shaping (dragon ventral flattening, dorsal ridge), and outward CCW winding.
fn add_sculpted_tube(
    mb: &mut MeshBuilder,
    rings: &[RingSample],
    radial_segs: usize,
    u_scale: f32,
    dorsal_ridge: f32,
    ventral_flatten: f32,
    mat_id: u32,
    head_idx: u32,
    flags: u32,
    cap_start: bool,
    cap_end: bool,
    cast_shadow: bool,
) {
    if rings.len() < 2 || radial_segs < 4 {
        return;
    }
    let start_vert = mb.vertices.len();
    let start_idx = mb.opaque_indices.len();
    let stride = radial_segs + 1;

    for ring in rings {
        for j in 0..=radial_segs {
            let u_frac = j as f32 / radial_segs as f32;
            let theta = u_frac * TAU;
            let (sin_t, cos_t) = theta.sin_cos();

            // Subtle anatomical shaping:
            // angle pi/2 (+b_rad = ventral/front) -> flatten for armored throat scutes;
            // angle 3pi/2 (-b_rad = dorsal/back) -> pinch into razor spine keel.
            let dorsal_factor = (-sin_t).max(0.0).powf(4.0) * dorsal_ridge;
            let ventral_factor = sin_t.max(0.0).powf(2.0) * ventral_flatten;
            let rx = ring.rx * (1.0 - dorsal_factor * 0.25);
            let ry = ring.ry * (1.0 + dorsal_factor - ventral_factor);

            let offset = ring.n_rad * (cos_t * rx) + ring.b_rad * (sin_t * ry);
            let pos = ring.center + offset;
            let approx_n = offset.normalize_or(ring.b_rad);
            let t_dir = (ring.b_rad * cos_t - ring.n_rad * sin_t).normalize_or(Vec3::X);

            // Detect ventral throat strip (front-facing b_rad side where sin_t > 0.58)
            let local_mat =
                if mat_id == MAT_OBSIDIAN_SCALES && sin_t > 0.62 && ventral_flatten > 0.05 {
                    MAT_VENTRAL_SCUTES
                } else {
                    mat_id
                };

            mb.push_vertex(
                pos,
                approx_n,
                [t_dir.x, t_dir.y, t_dir.z, 1.0],
                [u_frac * u_scale, ring.v_uv],
                ring.joints,
                ring.weights,
                local_mat,
                head_idx,
                flags,
            );
        }
    }

    for i in 0..rings.len() - 1 {
        let r0 = start_vert as u32 + (i * stride) as u32;
        let r1 = start_vert as u32 + ((i + 1) * stride) as u32;
        for j in 0..radial_segs as u32 {
            let v00 = r0 + j;
            let v01 = r0 + j + 1;
            let v10 = r1 + j;
            let v11 = r1 + j + 1;
            // With (n_rad, b_rad, tangent) right-handed (n_rad x b_rad = tangent),
            // (v00, v01, v10) and (v01, v11, v10) point STRICTLY outward!
            mb.push_tri(v00, v01, v10, cast_shadow, false);
            mb.push_tri(v01, v11, v10, cast_shadow, false);
        }
    }

    if cap_start {
        let first = rings[0];
        let c_idx = mb.push_vertex(
            first.center,
            -first.tangent,
            [first.n_rad.x, first.n_rad.y, first.n_rad.z, 1.0],
            [0.5 * u_scale, first.v_uv],
            first.joints,
            first.weights,
            mat_id,
            head_idx,
            flags,
        );
        let r0 = start_vert as u32;
        for j in 0..radial_segs as u32 {
            mb.push_tri(c_idx, r0 + j + 1, r0 + j, cast_shadow, false);
        }
    }

    if cap_end {
        let last = rings[rings.len() - 1];
        let c_idx = mb.push_vertex(
            last.center,
            last.tangent,
            [last.n_rad.x, last.n_rad.y, last.n_rad.z, 1.0],
            [0.5 * u_scale, last.v_uv],
            last.joints,
            last.weights,
            mat_id,
            head_idx,
            flags,
        );
        let rn = start_vert as u32 + ((rings.len() - 1) * stride) as u32;
        for j in 0..radial_segs as u32 {
            mb.push_tri(c_idx, rn + j, rn + j + 1, cast_shadow, false);
        }
    }

    let indices_added = mb.opaque_indices[start_idx..].to_vec();
    mb.compute_smooth_normals_and_tangents(start_vert, &indices_added);
}

/// Add a sculpted faceted pyramid / spine / claw / crystal prong along `a -> b`.
#[allow(clippy::too_many_arguments)]
fn add_curved_horn(
    mb: &mut MeshBuilder,
    pts: &[Vec3],
    r_base: f32,
    r_tip: f32,
    ring_segs: usize,
    keratin_ridges: bool,
    mat_id: u32,
    head_idx: u32,
    bone: u32,
) {
    if pts.len() < 2 {
        return;
    }
    let samples = 14usize;
    let mut rings = Vec::with_capacity(samples + 1);
    let n_pts = pts.len() - 1;
    let mut prev_n = Vec3::X;
    for s in 0..=samples {
        let u = s as f32 / samples as f32;
        let scaled = u * n_pts as f32;
        let seg = (scaled.floor() as usize).min(n_pts - 1);
        let local_t = scaled - seg as f32;
        let p0 = pts[(seg).saturating_sub(1)];
        let p1 = pts[seg];
        let p2 = pts[(seg + 1).min(n_pts)];
        let p3 = pts[(seg + 2).min(n_pts)];
        let center = catmull_rom(p0, p1, p2, p3, local_t);
        let center_next = catmull_rom(p0, p1, p2, p3, (local_t + 0.04).min(1.0));
        let tangent = (center_next - center).normalize_or(p2 - p1);
        let (mut n_rad, mut b_rad) = tube_frame(tangent, prev_n.cross(tangent));
        if s > 0 && n_rad.dot(prev_n) < 0.0 {
            n_rad = -n_rad;
            b_rad = -b_rad;
        }
        prev_n = n_rad;
        let ridge = if keratin_ridges {
            1.0 + 0.055 * (u * 30.0).sin() * (1.0 - u * 0.7)
        } else {
            1.0
        };
        let r = (r_base * (1.0 - u) + r_tip * u) * ridge;
        rings.push(RingSample {
            center,
            tangent,
            n_rad,
            b_rad,
            rx: r,
            ry: r * 1.12,
            v_uv: u * 4.0,
            joints: [bone, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        });
    }
    add_sculpted_tube(
        mb, &rings, ring_segs, 1.5, 0.14, 0.0, mat_id, head_idx, 0, false, true, true,
    );
}

#[inline]
fn catmull_rom(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;
    (p1 * 2.0
        + (p2 - p0) * t
        + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * t2
        + (-p0 + p1 * 3.0 - p2 * 3.0 + p3) * t3)
        * 0.5
}

/// Add a sculpted faceted crystal diamond/gemstone (zero smooth spheres).
#[allow(clippy::too_many_arguments)]
fn add_faceted_crystal(
    mb: &mut MeshBuilder,
    center: Vec3,
    rx: f32,
    ry: f32,
    rz: f32,
    sides: usize,
    bone: u32,
    mat_id: u32,
    head_idx: u32,
    flags: u32,
    cast_shadow: bool,
) {
    let crown_y = ry * 0.42;
    let table_r = 0.54;
    let push_flat_tri = |mb: &mut MeshBuilder, a: Vec3, b: Vec3, c: Vec3| {
        let n = (b - a).cross(c - a).normalize_or(Vec3::Y);
        let t = (b - a).normalize_or(Vec3::X);
        let va = mb.push_vertex(
            a,
            n,
            [t.x, t.y, t.z, 1.0],
            [a.x * 2.0, a.y * 2.0],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            head_idx,
            flags,
        );
        let vb = mb.push_vertex(
            b,
            n,
            [t.x, t.y, t.z, 1.0],
            [b.x * 2.0, b.y * 2.0],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            head_idx,
            flags,
        );
        let vc = mb.push_vertex(
            c,
            n,
            [t.x, t.y, t.z, 1.0],
            [c.x * 2.0, c.y * 2.0],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            head_idx,
            flags,
        );
        mb.push_tri(va, vb, vc, cast_shadow, false);
    };

    let apex_top = center + Vec3::new(0.0, ry, 0.0);
    let apex_bot = center + Vec3::new(0.0, -ry, 0.0);
    let ring_pt = |angle: f32, y_off: f32, scale: f32| {
        let (s, c) = angle.sin_cos();
        center + Vec3::new(c * rx * scale, y_off, s * rz * scale)
    };

    for i in 0..sides {
        let a0 = (i as f32 / sides as f32) * TAU;
        let a1 = ((i + 1) as f32 / sides as f32) * TAU;
        let g0 = ring_pt(a0, 0.0, 1.0);
        let g1 = ring_pt(a1, 0.0, 1.0);
        let u0 = ring_pt(a0, crown_y, table_r);
        let u1 = ring_pt(a1, crown_y, table_r);
        let l0 = ring_pt(a0, -crown_y, table_r);
        let l1 = ring_pt(a1, -crown_y, table_r);

        // Top star crown + bevel facets (CCW viewed from outside)
        push_flat_tri(mb, apex_top, u1, u0);
        push_flat_tri(mb, u0, u1, g1);
        push_flat_tri(mb, u0, g1, g0);
        // Pavilion facets
        push_flat_tri(mb, g0, g1, l1);
        push_flat_tri(mb, g0, l1, l0);
        push_flat_tri(mb, apex_bot, l0, l1);
    }
}

/// Add a 3D UV sphere (used specifically for deeply recessed glowing reptilian eyeballs).
#[allow(clippy::too_many_arguments)]
fn add_uv_sphere(
    mb: &mut MeshBuilder,
    center: Vec3,
    radius: f32,
    rings_n: usize,
    segs_n: usize,
    bone: u32,
    mat_id: u32,
    head_idx: u32,
    flags: u32,
) -> (u32, u32) {
    let start_idx = mb.opaque_indices.len() as u32;
    let start_vert = mb.vertices.len() as u32;
    for r in 0..=rings_n {
        let v = r as f32 / rings_n as f32;
        let phi = v * PI;
        let (sp, cp) = phi.sin_cos();
        for s in 0..=segs_n {
            let u = s as f32 / segs_n as f32;
            let theta = u * TAU;
            let (st, ct) = theta.sin_cos();
            // Orient sphere pole toward +Z so UV.xy directly faces the pupil aperture!
            let dir = Vec3::new(sp * st, cp, sp * ct);
            let pos = center + dir * radius;
            let t = Vec3::new(ct, 0.0, -st).normalize_or(Vec3::X);
            mb.push_vertex(
                pos,
                dir,
                [t.x, t.y, t.z, 1.0],
                [u, v],
                [bone, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
                mat_id,
                head_idx,
                flags,
            );
        }
    }
    let stride = (segs_n + 1) as u32;
    for r in 0..rings_n as u32 {
        for s in 0..segs_n as u32 {
            let a = start_vert + r * stride + s;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            mb.push_tri(a, b, c, false, false);
            mb.push_tri(b, d, c, false, false);
        }
    }
    let count = mb.opaque_indices.len() as u32 - start_idx;
    (start_idx, count)
}

/// Add a solid extruded octagonal prism or gothic pillar segment.
fn add_box_prism(
    mb: &mut MeshBuilder,
    center: Vec3,
    half: Vec3,
    bone: u32,
    mat_id: u32,
    cast_shadow: bool,
) {
    let corners = [
        Vec3::new(-half.x, -half.y, half.z),
        Vec3::new(half.x, -half.y, half.z),
        Vec3::new(half.x, half.y, half.z),
        Vec3::new(-half.x, half.y, half.z),
        Vec3::new(-half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, -half.z),
        Vec3::new(half.x, half.y, -half.z),
        Vec3::new(-half.x, half.y, -half.z),
    ];
    let faces = [
        ([0, 1, 2, 3], Vec3::Z, Vec3::X),
        ([5, 4, 7, 6], -Vec3::Z, -Vec3::X),
        ([4, 0, 3, 7], -Vec3::X, Vec3::Z),
        ([1, 5, 6, 2], Vec3::X, -Vec3::Z),
        ([3, 2, 6, 7], Vec3::Y, Vec3::X),
        ([4, 5, 1, 0], -Vec3::Y, Vec3::X),
    ];
    for (quad, n, t) in faces {
        let base = mb.vertices.len() as u32;
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for (k, &ci) in quad.iter().enumerate() {
            mb.push_vertex(
                center + corners[ci],
                n,
                [t.x, t.y, t.z, 1.0],
                uvs[k],
                [bone, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
                mat_id,
                7,
                0,
            );
        }
        mb.push_tri(base, base + 1, base + 2, cast_shadow, false);
        mb.push_tri(base, base + 2, base + 3, cast_shadow, false);
    }
}

/// Build the complete articulated 3D Leviathan + Gothic Obsidian Sanctuary mesh.
pub fn build_leviathan_mesh() -> LeviathanMeshData {
    let bind = LeviathanSkeleton::rest_bind_positions();
    let specs = LeviathanSkeleton::head_rest_specs();
    let mut mb = MeshBuilder::new();
    let mut eye_ranges = [(0u32, 0u32); 7];

    // ========================================================================
    // 1. Shared Muscular Obsidian Thorax, Scapulae, Sternum Ribs & Dorsal Spine
    // ========================================================================
    let thorax_rings_spec = [
        (Vec3::new(0.0, -1.78, -0.18), 0.24, 0.19, 0.0),
        (Vec3::new(0.0, -1.52, -0.06), 0.56, 0.42, 1.5),
        (Vec3::new(0.0, -1.24, 0.08), 0.88, 0.56, 3.2),
        (Vec3::new(0.0, -0.96, 0.16), 0.96, 0.58, 5.0),
        (Vec3::new(0.0, -0.68, 0.14), 0.78, 0.44, 6.8),
        (Vec3::new(0.0, -0.44, 0.08), 0.48, 0.28, 8.2),
    ];
    let thorax_rings: Vec<RingSample> = thorax_rings_spec
        .iter()
        .map(|&(c, rx, ry, v)| RingSample {
            center: c,
            tangent: Vec3::Y,
            n_rad: Vec3::X,
            b_rad: Vec3::Z,
            rx,
            ry,
            v_uv: v,
            joints: [BONE_THORAX as u32, BONE_ROOT as u32, 0, 0],
            weights: [0.85, 0.15, 0.0, 0.0],
        })
        .collect();
    add_sculpted_tube(
        &mut mb,
        &thorax_rings,
        24,
        8.0,
        0.16,
        0.14,
        MAT_OBSIDIAN_SCALES,
        7,
        0,
        true,
        true,
        true,
    );

    // 6 Sculpted Armored Sternal Rib-Prongs physically hugging the Amethyst Heart
    for side in [-1.0_f32, 1.0_f32] {
        for rib in 0..3 {
            let y_top = -0.82 - rib as f32 * 0.20;
            let pts = [
                Vec3::new(side * 0.56, y_top, 0.26),
                Vec3::new(side * 0.44, y_top - 0.05, 0.56),
                Vec3::new(side * 0.26, y_top - 0.10, 0.64),
            ];
            add_curved_horn(
                &mut mb,
                &pts,
                0.072,
                0.016,
                7,
                false,
                MAT_KERATIN_HORN,
                7,
                BONE_THORAX as u32,
            );
        }
    }

    // ========================================================================
    // 2. Physically Integrated Faceted 3D Amethyst Heart & 7 Anatomical Conduits
    // ========================================================================
    let heart_c = bind[BONE_HEART];
    // Outer crystalline faceted amethyst gemstone
    add_faceted_crystal(
        &mut mb,
        heart_c,
        0.30,
        0.38,
        0.24,
        8,
        BONE_HEART as u32,
        MAT_AMETHYST_HEART,
        7,
        0,
        true,
    );
    // Inner glowing star-core crystal inside the outer gem
    add_faceted_crystal(
        &mut mb,
        heart_c + Vec3::new(0.0, 0.0, 0.02),
        0.15,
        0.22,
        0.16,
        6,
        BONE_HEART as u32,
        MAT_ENERGY_CONDUIT,
        7,
        2,
        false,
    );

    // Seven glowing physical conduit veins arching from Heart into the 7 necks
    for h in 0..7 {
        let p0 = specs[h].0;
        let mid = Vec3::new(
            p0.x * 0.55,
            (heart_c.y + p0.y) * 0.52,
            (heart_c.z + p0.z) * 0.55 + 0.10,
        );
        let pts = [
            heart_c + Vec3::new((h as f32 - 3.0) * 0.04, 0.08, 0.02),
            mid,
            p0 + Vec3::new(0.0, 0.06, 0.16),
        ];
        add_curved_horn(
            &mut mb,
            &pts,
            0.026,
            0.018,
            6,
            false,
            MAT_ENERGY_CONDUIT,
            h as u32,
            head_neck_bone(h, 0) as u32,
        );
    }

    // ========================================================================
    // 3. Muscular Forelimbs & Talon Claws Gripping the Obsidian Dais
    // ========================================================================
    for (side, sh, el, paw) in [
        (-1.0_f32, BONE_ARM_L_SH, BONE_ARM_L_EL, BONE_ARM_L_PAW),
        (1.0_f32, BONE_ARM_R_SH, BONE_ARM_R_EL, BONE_ARM_R_PAW),
    ] {
        let arm_pts = [
            (bind[sh], 0.21, 0.18, sh, BONE_THORAX, 0.0),
            (bind[sh].lerp(bind[el], 0.55), 0.17, 0.15, sh, el, 2.0),
            (bind[el], 0.15, 0.14, el, paw, 4.0),
            (bind[el].lerp(bind[paw], 0.55), 0.13, 0.12, el, paw, 6.0),
            (
                bind[paw] + Vec3::new(0.0, 0.08, 0.0),
                0.14,
                0.13,
                paw,
                el,
                7.5,
            ),
        ];
        let mut arm_rings = Vec::with_capacity(arm_pts.len());
        for i in 0..arm_pts.len() {
            let (pos, rx, ry, b0, b1, v) = arm_pts[i];
            let tang = if i + 1 < arm_pts.len() {
                arm_pts[i + 1].0 - pos
            } else {
                pos - arm_pts[i - 1].0
            }
            .normalize_or(-Vec3::Y);
            let (nr, br) = tube_frame(tang, Vec3::Z);
            arm_rings.push(RingSample {
                center: pos,
                tangent: tang,
                n_rad: nr,
                b_rad: br,
                rx,
                ry,
                v_uv: v,
                joints: [b0 as u32, b1 as u32, 0, 0],
                weights: [0.75, 0.25, 0.0, 0.0],
            });
        }
        add_sculpted_tube(
            &mut mb,
            &arm_rings,
            12,
            4.0,
            0.1,
            0.05,
            MAT_OBSIDIAN_SCALES,
            7,
            0,
            true,
            true,
            true,
        );
        // Elbow spur + 3 wicked talon claws gripping the edge of the obsidian dais
        let elbow_spur = [
            bind[el],
            bind[el] + Vec3::new(side * 0.22, -0.02, -0.18),
            bind[el] + Vec3::new(side * 0.36, 0.06, -0.32),
        ];
        add_curved_horn(
            &mut mb,
            &elbow_spur,
            0.055,
            0.008,
            6,
            false,
            MAT_KERATIN_HORN,
            7,
            el as u32,
        );
        for c in [-1.0_f32, 0.0, 1.0] {
            let talon_base = bind[paw] + Vec3::new(c * 0.11, 0.02, 0.08);
            let talon_pts = [
                talon_base,
                talon_base + Vec3::new(c * 0.06, -0.04, 0.18),
                talon_base + Vec3::new(c * 0.08, -0.18, 0.24),
            ];
            add_curved_horn(
                &mut mb,
                &talon_pts,
                0.042,
                0.006,
                6,
                false,
                MAT_KERATIN_HORN,
                7,
                paw as u32,
            );
        }
    }

    // ========================================================================
    // 4. Sweeping Articulated 3D Gothic Wings (Bones, Thumb Claws & Scalloped Membrane)
    // ========================================================================
    for (side, sh, el, t0, t1, t2, t3) in [
        (
            -1.0_f32,
            BONE_WING_L_SH,
            BONE_WING_L_EL,
            BONE_WING_L_TIP0,
            BONE_WING_L_TIP1,
            BONE_WING_L_TIP2,
            BONE_WING_L_TIP3,
        ),
        (
            1.0_f32,
            BONE_WING_R_SH,
            BONE_WING_R_EL,
            BONE_WING_R_TIP0,
            BONE_WING_R_TIP1,
            BONE_WING_R_TIP2,
            BONE_WING_R_TIP3,
        ),
    ] {
        let p_sh = bind[sh];
        let p_el = bind[el];
        let p_hip = Vec3::new(side * 0.56, -1.48, -0.24);
        // Leading wing arm & phalangeal bone spars
        add_curved_horn(
            &mut mb,
            &[p_sh, p_sh.lerp(p_el, 0.5) + Vec3::new(0.0, 0.14, 0.0), p_el],
            0.095,
            0.072,
            8,
            false,
            MAT_OBSIDIAN_SCALES,
            7,
            el as u32,
        );
        // Elbow wing-claw spur
        add_curved_horn(
            &mut mb,
            &[
                p_el,
                p_el + Vec3::new(side * 0.08, 0.36, 0.14),
                p_el + Vec3::new(side * 0.14, 0.56, 0.18),
            ],
            0.055,
            0.008,
            6,
            true,
            MAT_KERATIN_HORN,
            7,
            el as u32,
        );
        let tips = [t0, t1, t2, t3];
        for (ti, &tb) in tips.iter().enumerate() {
            let mid = p_el.lerp(bind[tb], 0.52) + Vec3::new(0.0, 0.14 - ti as f32 * 0.05, 0.10);
            add_curved_horn(
                &mut mb,
                &[p_el, mid, bind[tb]],
                0.056 - ti as f32 * 0.008,
                0.012,
                6,
                false,
                MAT_KERATIN_HORN,
                7,
                tb as u32,
            );
        }

        // Smooth Skinned Gothic Wing Membrane (4 scalloped arch bays)
        let anchors = [
            (p_sh, p_hip, sh, BONE_THORAX),
            (p_el, bind[t3], el, t3),
            (p_el, bind[t2], el, t2),
            (p_el, bind[t1], el, t1),
            (p_el, bind[t0], el, t0),
        ];
        // Build quad strip mesh across the 4 wing panels connecting upper bone arch to scalloped lower edge
        let top_chain = [
            p_sh,
            p_el,
            p_el.lerp(bind[t0], 0.45),
            p_el.lerp(bind[t0], 0.78),
            bind[t0],
        ];
        let top_bones = [sh, el, t0, t0, t0];
        let bot_chain = [p_hip, bind[t3], bind[t2], bind[t1], bind[t0]];
        let bot_bones = [BONE_THORAX, t3, t2, t1, t0];
        let _ = anchors;

        let segs_u = 16usize;
        let segs_v = 6usize;
        let mem_start = mb.vertices.len();
        let idx_start = mb.transparent_indices.len();
        for iu in 0..=segs_u {
            let u = iu as f32 / segs_u as f32;
            let f_idx = (u * 4.0).min(3.999);
            let panel = f_idx.floor() as usize;
            let lt = f_idx - panel as f32;
            let top_p = top_chain[panel].lerp(top_chain[panel + 1], lt);
            let bot_base = bot_chain[panel].lerp(bot_chain[panel + 1], lt);
            // Gothic concave scalloped arch between each finger spar!
            let arch = (lt * PI).sin() * 0.24 * (1.0 - (panel as f32 / 4.0) * 0.35);
            let bot_p = bot_base + Vec3::new(0.0, arch, 0.06);
            let tb = top_bones[(panel + 1).min(4)] as u32;
            let bb = bot_bones[(panel + 1).min(4)] as u32;
            for iv in 0..=segs_v {
                let v = iv as f32 / segs_v as f32;
                let camber = (v * PI).sin() * 0.14;
                let pos = top_p.lerp(bot_p, v) + Vec3::new(0.0, 0.0, camber);
                mb.push_vertex(
                    pos,
                    Vec3::Z,
                    [side, 0.0, 0.0, 1.0],
                    [u * 4.0, v],
                    [tb, bb, BONE_THORAX as u32, 0],
                    [(1.0 - v) * 0.85, v * 0.85, 0.15, 0.0],
                    MAT_WING_MEMBRANE,
                    7,
                    if side > 0.0 { 1 } else { 0 },
                );
            }
        }
        let v_stride = (segs_v + 1) as u32;
        for iu in 0..segs_u as u32 {
            for iv in 0..segs_v as u32 {
                let a = mem_start as u32 + iu * v_stride + iv;
                let b = a + 1;
                let c = a + v_stride;
                let d = c + 1;
                mb.push_tri(a, c, b, false, true);
                mb.push_tri(b, c, d, false, true);
            }
        }
        let added = mb.transparent_indices[idx_start..].to_vec();
        mb.compute_smooth_normals_and_tangents(mem_start, &added);
    }

    // ========================================================================
    // 5. 6-Bone Coiling Armored Tail + Dorsal Crest Blades
    // ========================================================================
    let tail_samples = 24usize;
    let tail_pts: Vec<Vec3> = (0..6).map(|i| bind[BONE_TAIL_BASE + i]).collect();
    let mut tail_rings = Vec::with_capacity(tail_samples + 1);
    let mut prev_n = Vec3::Y;
    for s in 0..=tail_samples {
        let u = s as f32 / tail_samples as f32;
        let f = (u * 5.0).min(4.999);
        let seg = f.floor() as usize;
        let lt = f - seg as f32;
        let center = catmull_rom(
            tail_pts[seg.saturating_sub(1)],
            tail_pts[seg],
            tail_pts[(seg + 1).min(5)],
            tail_pts[(seg + 2).min(5)],
            lt,
        );
        let c_next = catmull_rom(
            tail_pts[seg.saturating_sub(1)],
            tail_pts[seg],
            tail_pts[(seg + 1).min(5)],
            tail_pts[(seg + 2).min(5)],
            (lt + 0.05).min(1.0),
        );
        let tangent = (c_next - center).normalize_or(Vec3::X);
        let (mut nr, mut br) = tube_frame(tangent, Vec3::Y);
        if s > 0 && nr.dot(prev_n) < 0.0 {
            nr = -nr;
            br = -br;
        }
        prev_n = nr;
        let b0 = BONE_TAIL_BASE + seg;
        let b1 = BONE_TAIL_BASE + (seg + 1).min(5);
        let r = 0.24 * (1.0 - u).powf(0.85) + 0.025;
        tail_rings.push(RingSample {
            center,
            tangent,
            n_rad: nr,
            b_rad: br,
            rx: r,
            ry: r * 1.12,
            v_uv: u * 16.0,
            joints: [b0 as u32, b1 as u32, 0, 0],
            weights: [1.0 - lt, lt, 0.0, 0.0],
        });
    }
    add_sculpted_tube(
        &mut mb,
        &tail_rings,
        14,
        5.0,
        0.22,
        0.08,
        MAT_OBSIDIAN_SCALES,
        7,
        0,
        false,
        true,
        true,
    );
    // Tail-spade diamond tip
    add_faceted_crystal(
        &mut mb,
        tail_pts[5] + Vec3::new(0.10, 0.08, 0.04),
        0.07,
        0.16,
        0.04,
        4,
        (BONE_TAIL_BASE + 5) as u32,
        MAT_KERATIN_HORN,
        7,
        0,
        true,
    );

    // ========================================================================
    // 6. Seven 6-Bone Muscular Articulated Necks with Dorsal Spines & Dorsal Ridges
    // ========================================================================
    for h in 0..7 {
        let (p0, c1, c2, skull) = specs[h];
        let thickness = if h == 3 { 1.24 } else { 1.0 };
        let r_root = 0.24 * thickness;
        let r_neck = 0.145 * thickness;
        let samples = 24usize;
        let mut neck_rings = Vec::with_capacity(samples + 1);
        let mut prev_n = Vec3::X;
        for s in 0..=samples {
            let u = s as f32 / samples as f32;
            let center = cubic_bezier_v3(p0, c1, c2, skull, u);
            let tangent = cubic_bezier_tangent_v3(p0, c1, c2, skull, u);
            // Make b_rad point toward +Z (front of dragon throat) so ventral plates sit cleanly on front!
            let (mut nr, mut br) = tube_frame(tangent, Vec3::Z);
            if s > 0 && nr.dot(prev_n) < 0.0 {
                nr = -nr;
                br = -br;
            }
            // Ensure br points toward +Z (front throat)
            if br.z < 0.0 {
                br = -br;
                nr = -nr;
            }
            prev_n = nr;

            let f = (u * (NECK_BONES_PER_HEAD - 1) as f32)
                .min((NECK_BONES_PER_HEAD - 1) as f32 - 0.001);
            let seg = f.floor() as usize;
            let lt = f - seg as f32;
            let b0 = head_neck_bone(h, seg);
            let b1 = head_neck_bone(h, (seg + 1).min(NECK_BONES_PER_HEAD - 1));
            let b_skull = if u > 0.85 { head_skull_bone(h) } else { b1 };
            let r = (r_root * (1.0 - u) + r_neck * u) * (1.0 + 0.08 * (u * PI * 2.0).sin());
            neck_rings.push(RingSample {
                center,
                tangent,
                n_rad: nr,
                b_rad: br,
                rx: r,
                ry: r * 1.15,
                v_uv: u * 26.0,
                joints: [b0 as u32, b_skull as u32, 0, 0],
                weights: [1.0 - lt, lt, 0.0, 0.0],
            });

            // Sculpted physical 3D dorsal ridge spines stepping down the back of each neck
            if s >= 2 && s <= samples - 2 && s % 2 == 0 {
                let spine_root = center - br * (r * 0.95);
                let spine_mid = spine_root - br * (0.09 * thickness) - tangent * 0.03;
                let spine_tip = spine_root - br * (0.20 * thickness) - tangent * 0.09;
                add_curved_horn(
                    &mut mb,
                    &[spine_root, spine_mid, spine_tip],
                    0.032 * thickness,
                    0.004,
                    5,
                    false,
                    MAT_KERATIN_HORN,
                    h as u32,
                    b0 as u32,
                );
            }
        }
        add_sculpted_tube(
            &mut mb,
            &neck_rings,
            16,
            8.0,
            0.22,
            0.12,
            MAT_OBSIDIAN_SCALES,
            h as u32,
            0,
            true,
            true,
            true,
        );
    }

    // ========================================================================
    // 7. Seven Anatomically Distinct Sculpted 3D Dragon Heads + Hinged Jaws + Personalities
    // ========================================================================
    for h in 0..7 {
        eye_ranges[h] = build_distinct_dragon_head(&mut mb, h, bind[head_skull_bone(h)]);
    }

    // ========================================================================
    // 8. Monumental Gothic Obsidian Sanctuary Architecture & Carved Dais
    // ========================================================================
    build_gothic_sanctuary(&mut mb, bind[BONE_ROOT]);

    LeviathanMeshData {
        vertices: mb.vertices,
        opaque_indices: mb.opaque_indices,
        shadow_indices: mb.shadow_indices,
        transparent_indices: mb.transparent_indices,
        eye_head_offset_count: eye_ranges,
    }
}

/// Sculpt one complete, personality-specific 3D Dragon Head, Hinged Lower Jaw,
/// Teeth/Fangs, Orbital Sockets, Living Eyeballs, and Custom Crown Horns.
fn build_distinct_dragon_head(mb: &mut MeshBuilder, head_idx: usize, skull_c: Vec3) -> (u32, u32) {
    let sk_bone = head_skull_bone(head_idx) as u32;
    let jaw_bone = head_jaw_bone(head_idx) as u32;
    let sc = if head_idx == 3 { 1.32 } else { 1.14 };

    // 7A. Sculpted Upper Cranium + Overhanging Brow Plates + Tapering Snout (`-Z -> +Z`)
    // Longitudinal anatomical profile along local Z (-0.36 back cranium to +0.66 snout tip)
    let snout_len = match head_idx {
        1 => 0.74 * sc, // FANG: fierce elongated predator viper snout
        5 => 0.62 * sc, // ORACLE: narrow regal sage snout
        3 => 0.65 * sc, // AEGIS: massive broad sovereign skull
        _ => 0.62 * sc,
    };
    let skull_w = match head_idx {
        3 => 1.24 * sc, // AEGIS widest armor
        1 => 0.98 * sc, // FANG sleek viper
        _ => 1.04 * sc,
    };

    let slices = 14usize;
    let mut upper_rings = Vec::with_capacity(slices + 1);
    for s in 0..=slices {
        let u = s as f32 / slices as f32;
        let z_loc = -0.34 * sc + u * (snout_len + 0.34 * sc);
        // Sculpted cranium dome -> sunken orbital pinch -> flaring muzzle
        let arch_y = if u < 0.35 {
            0.06 * (u / 0.35).sin() * sc
        } else {
            0.06 * sc - (u - 0.35) * 0.22 * sc
        };
        let rx = if u < 0.28 {
            (0.16 + 0.16 * (u / 0.28).sin()) * skull_w
        } else if u < 0.56 {
            (0.28 - (u - 0.28) * 0.28) * skull_w
        } else {
            (0.20 - (u - 0.56) * 0.30).max(0.065) * skull_w
        };
        let ry = if u < 0.36 {
            (0.16 + 0.12 * (u / 0.36).sin()) * sc
        } else {
            (0.24 - (u - 0.36) * 0.24).max(0.065) * sc
        };
        upper_rings.push(RingSample {
            center: skull_c + Vec3::new(0.0, arch_y, z_loc),
            tangent: Vec3::Z,
            n_rad: Vec3::X,
            b_rad: Vec3::Y,
            rx,
            ry,
            v_uv: u * 16.0,
            joints: [sk_bone, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        });
    }
    add_sculpted_tube(
        mb,
        &upper_rings,
        20,
        18.0,
        0.28,
        0.18,
        MAT_OBSIDIAN_SCALES,
        head_idx as u32,
        0,
        true,
        true,
        true,
    );

    // Sculpted Twin Nasal Ridge Spurs on front muzzle (adds fierce draconic snout silhouette)
    for side in [-1.0_f32, 1.0_f32] {
        let nasal_pts = [
            skull_c + Vec3::new(side * 0.055 * skull_w, 0.015 * sc, snout_len * 0.80),
            skull_c + Vec3::new(side * 0.070 * skull_w, 0.075 * sc, snout_len * 0.70),
        ];
        add_curved_horn(
            mb,
            &nasal_pts,
            0.022 * sc,
            0.005 * sc,
            5,
            false,
            MAT_KERATIN_HORN,
            head_idx as u32,
            sk_bone,
        );
    }

    // Overhanging Armored Supraorbital Brow Plates & Cheekbone Armor (deepens orbital eye sockets!)
    for side in [-1.0_f32, 1.0_f32] {
        let brow_pts = [
            skull_c + Vec3::new(side * 0.08 * skull_w, 0.20 * sc, 0.28 * sc),
            skull_c + Vec3::new(side * 0.26 * skull_w, 0.24 * sc, 0.08 * sc),
            skull_c + Vec3::new(side * 0.36 * skull_w, 0.20 * sc, -0.18 * sc),
        ];
        add_curved_horn(
            mb,
            &brow_pts,
            0.058 * sc,
            0.014 * sc,
            6,
            false,
            MAT_OBSIDIAN_SCALES,
            head_idx as u32,
            sk_bone,
        );

        // Jagged Swept Cheek Armor Spurs (characteristic silhouette of an ancient dragon!)
        let cheek_pts = [
            skull_c + Vec3::new(side * 0.22 * skull_w, -0.04 * sc, 0.04 * sc),
            skull_c + Vec3::new(side * 0.40 * skull_w, -0.05 * sc, -0.18 * sc),
            skull_c + Vec3::new(side * 0.54 * skull_w, 0.03 * sc, -0.40 * sc),
        ];
        add_curved_horn(
            mb,
            &cheek_pts,
            0.064 * sc,
            0.009 * sc,
            6,
            true,
            MAT_KERATIN_HORN,
            head_idx as u32,
            sk_bone,
        );
    }

    // 7B. Hinged Lower Jaw (`jaw_bone`) + Chin Armor + Curved Ivory Fangs
    let jaw_slices = 10usize;
    let mut jaw_rings = Vec::with_capacity(jaw_slices + 1);
    for s in 0..=jaw_slices {
        let u = s as f32 / jaw_slices as f32;
        let z_loc = -0.04 * sc + u * (snout_len * 0.96);
        let y_loc = -0.11 * sc - u * 0.055 * sc;
        let rx = (0.195 * (1.0 - u * 0.62)) * skull_w;
        let ry = (0.098 * (1.0 - u * 0.42)) * sc;
        jaw_rings.push(RingSample {
            center: skull_c + Vec3::new(0.0, y_loc, z_loc),
            tangent: Vec3::Z,
            n_rad: Vec3::X,
            b_rad: Vec3::Y,
            rx,
            ry,
            v_uv: u * 11.0,
            joints: [jaw_bone, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        });
    }
    add_sculpted_tube(
        mb,
        &jaw_rings,
        16,
        14.0,
        0.04,
        0.10,
        MAT_OBSIDIAN_SCALES,
        head_idx as u32,
        0,
        true,
        true,
        true,
    );

    // Armored Chin Spur under the mandible
    add_curved_horn(
        mb,
        &[
            skull_c + Vec3::new(0.0, -0.16 * sc, 0.30 * sc),
            skull_c + Vec3::new(0.0, -0.23 * sc, 0.14 * sc),
            skull_c + Vec3::new(0.0, -0.28 * sc, -0.02 * sc),
        ],
        0.048 * sc,
        0.008,
        5,
        false,
        MAT_KERATIN_HORN,
        head_idx as u32,
        jaw_bone,
    );

    // Upper & Lower Curved Ivory Fangs (extra fierce saber fangs on FANG=1)
    let fang_len = if head_idx == 1 { 0.19 * sc } else { 0.13 * sc };
    for side in [-1.0_f32, 1.0_f32] {
        // Upper primary canine
        add_curved_horn(
            mb,
            &[
                skull_c + Vec3::new(side * 0.10 * skull_w, -0.08 * sc, snout_len * 0.72),
                skull_c
                    + Vec3::new(
                        side * 0.095 * skull_w,
                        -0.08 * sc - fang_len * 0.55,
                        snout_len * 0.74,
                    ),
                skull_c
                    + Vec3::new(
                        side * 0.085 * skull_w,
                        -0.08 * sc - fang_len,
                        snout_len * 0.70,
                    ),
            ],
            0.026 * sc,
            0.003,
            5,
            false,
            MAT_IVORY_FANG,
            head_idx as u32,
            sk_bone,
        );
        // Secondary premolar teeth
        for k in 0..2 {
            let z_t = snout_len * (0.48 - k as f32 * 0.16);
            let x_t = side * (0.125 + k as f32 * 0.02) * skull_w;
            add_curved_horn(
                mb,
                &[
                    skull_c + Vec3::new(x_t, -0.07 * sc, z_t),
                    skull_c + Vec3::new(x_t, -0.125 * sc, z_t),
                ],
                0.017 * sc,
                0.002,
                5,
                false,
                MAT_IVORY_FANG,
                head_idx as u32,
                sk_bone,
            );
        }
        // Lower mandible fang
        add_curved_horn(
            mb,
            &[
                skull_c + Vec3::new(side * 0.075 * skull_w, -0.13 * sc, snout_len * 0.60),
                skull_c + Vec3::new(side * 0.070 * skull_w, -0.05 * sc, snout_len * 0.62),
            ],
            0.018 * sc,
            0.002,
            5,
            false,
            MAT_IVORY_FANG,
            head_idx as u32,
            jaw_bone,
        );
    }

    // 7C. Fierce Glowing 3D Reptilian Eyeballs clearly seated beneath overhanging brow armor!
    let eye_off = Vec3::new(0.215 * skull_w, 0.095 * sc, 0.145 * sc);
    let mut eye_total = (0u32, 0u32);
    for (ei, side) in [-1.0_f32, 1.0_f32].iter().enumerate() {
        let (off, cnt) = add_uv_sphere(
            mb,
            skull_c + Vec3::new(side * eye_off.x, eye_off.y, eye_off.z),
            0.074 * sc,
            10,
            14,
            sk_bone,
            MAT_REPTILIAN_EYE,
            head_idx as u32,
            if ei == 0 { 0 } else { 1 },
        );
        if ei == 0 {
            eye_total = (off, cnt);
        } else {
            eye_total.1 += cnt;
        }
    }

    // 7D. Unique Personality Horn Crowns & Mythic Cranial Structures
    for side in [-1.0_f32, 1.0_f32] {
        match head_idx {
            0 => {
                // HUNTER: sleek swept raptor sensor blade-horns + aerodynamic fin
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.14, 0.16, -0.06),
                        skull_c + Vec3::new(side * 0.36, 0.34, -0.32),
                        skull_c + Vec3::new(side * 0.56, 0.56, -0.58),
                    ],
                    0.065,
                    0.008,
                    7,
                    true,
                    MAT_KERATIN_HORN,
                    0,
                    sk_bone,
                );
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.20, 0.06, -0.10),
                        skull_c + Vec3::new(side * 0.44, 0.16, -0.36),
                    ],
                    0.036,
                    0.006,
                    6,
                    false,
                    MAT_KERATIN_HORN,
                    0,
                    sk_bone,
                );
            }
            1 => {
                // FANG: heavy muscular recurved predator war-horns + forward nasal spike
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.16, 0.16, -0.04),
                        skull_c + Vec3::new(side * 0.38, 0.24, -0.28),
                        skull_c + Vec3::new(side * 0.34, 0.64, -0.20),
                    ],
                    0.088,
                    0.010,
                    8,
                    true,
                    MAT_KERATIN_HORN,
                    1,
                    sk_bone,
                );
            }
            2 => {
                // COGNITION: tall aristocratic neural crown horns + faceted brow crystal
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.13, 0.20, -0.04),
                        skull_c + Vec3::new(side * 0.26, 0.54, -0.22),
                        skull_c + Vec3::new(side * 0.32, 0.84, -0.34),
                    ],
                    0.064,
                    0.008,
                    7,
                    true,
                    MAT_KERATIN_HORN,
                    2,
                    sk_bone,
                );
                add_faceted_crystal(
                    mb,
                    skull_c + Vec3::new(side * 0.24, 0.58, 0.04),
                    0.045,
                    0.11,
                    0.045,
                    6,
                    sk_bone,
                    MAT_ENERGY_CONDUIT,
                    2,
                    1,
                    false,
                );
            }
            3 => {
                // AEGIS: monumental sovereign elder-dragon crown horns + royal inner tine
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.20, 0.22, -0.06),
                        skull_c + Vec3::new(side * 0.44, 0.46, -0.28),
                        skull_c + Vec3::new(side * 0.62, 0.84, -0.44),
                    ],
                    0.105,
                    0.012,
                    8,
                    true,
                    MAT_KERATIN_HORN,
                    3,
                    sk_bone,
                );
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.12, 0.24, -0.02),
                        skull_c + Vec3::new(side * 0.22, 0.58, -0.10),
                        skull_c + Vec3::new(side * 0.20, 0.82, 0.02),
                    ],
                    0.056,
                    0.008,
                    6,
                    true,
                    MAT_KERATIN_HORN,
                    3,
                    sk_bone,
                );
            }
            4 => {
                // NEXUS: chiseled angular forward-poised obsidian lance-horns + conduit fins
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.15, 0.18, -0.05),
                        skull_c + Vec3::new(side * 0.30, 0.48, -0.18),
                        skull_c + Vec3::new(side * 0.36, 0.78, 0.04),
                    ],
                    0.070,
                    0.010,
                    6,
                    false,
                    MAT_KERATIN_HORN,
                    4,
                    sk_bone,
                );
            }
            5 => {
                // ORACLE: sweeping inward-curving lunar crescent horns forming a celestial halo
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.16, 0.18, -0.04),
                        skull_c + Vec3::new(side * 0.42, 0.44, -0.16),
                        skull_c + Vec3::new(side * 0.24, 0.78, -0.06),
                    ],
                    0.072,
                    0.009,
                    8,
                    true,
                    MAT_KERATIN_HORN,
                    5,
                    sk_bone,
                );
            }
            _ => {
                // GENESIS: wide-branching organic elder stag antlers with crystal nodes
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.15, 0.18, -0.04),
                        skull_c + Vec3::new(side * 0.38, 0.44, -0.22),
                        skull_c + Vec3::new(side * 0.54, 0.74, -0.34),
                    ],
                    0.068,
                    0.009,
                    7,
                    true,
                    MAT_KERATIN_HORN,
                    6,
                    sk_bone,
                );
                add_curved_horn(
                    mb,
                    &[
                        skull_c + Vec3::new(side * 0.36, 0.42, -0.20),
                        skull_c + Vec3::new(side * 0.26, 0.76, -0.04),
                    ],
                    0.042,
                    0.007,
                    6,
                    false,
                    MAT_KERATIN_HORN,
                    6,
                    sk_bone,
                );
                add_faceted_crystal(
                    mb,
                    skull_c + Vec3::new(side * 0.54, 0.76, -0.34),
                    0.045,
                    0.095,
                    0.045,
                    6,
                    sk_bone,
                    MAT_ENERGY_CONDUIT,
                    6,
                    1,
                    false,
                );
            }
        }
    }

    // Center sagittal crests / signature personality geometries:
    if head_idx == 1 {
        // FANG protruding hooked nasal horn
        add_curved_horn(
            mb,
            &[
                skull_c + Vec3::new(0.0, 0.04, 0.52 * sc),
                skull_c + Vec3::new(0.0, 0.16, 0.58 * sc),
            ],
            0.036,
            0.006,
            6,
            false,
            MAT_KERATIN_HORN,
            1,
            sk_bone,
        );
    } else if head_idx == 2 {
        // COGNITION glowing amethyst pineal gem on forehead
        add_faceted_crystal(
            mb,
            skull_c + Vec3::new(0.0, 0.22 * sc, 0.08 * sc),
            0.055,
            0.095,
            0.055,
            6,
            sk_bone,
            MAT_ENERGY_CONDUIT,
            2,
            1,
            false,
        );
    } else if head_idx == 3 {
        // AEGIS regal central brow crown spike + translucent hexagonal defensive dome (flag=3)
        add_curved_horn(
            mb,
            &[
                skull_c + Vec3::new(0.0, 0.20 * sc, 0.04 * sc),
                skull_c + Vec3::new(0.0, 0.46 * sc, 0.08 * sc),
            ],
            0.068,
            0.008,
            6,
            false,
            MAT_KERATIN_HORN,
            3,
            sk_bone,
        );
        // Translucent 3D Hexagonal Shield Ring framing AEGIS (visible strictly when active/alert/selected)
        add_3d_ring_torus(
            mb,
            skull_c + Vec3::new(0.0, 0.08 * sc, 0.10 * sc),
            Vec3::Z,
            0.84 * sc,
            0.012,
            6,
            sk_bone,
            MAT_SHIELD_BARRIER,
            3,
            3,
        );
    } else if head_idx == 5 {
        // ORACLE tilted 3D celestial astrolabe ring framing the Seer's head
        add_3d_ring_torus(
            mb,
            skull_c + Vec3::new(0.0, 0.16 * sc, -0.06 * sc),
            Vec3::new(0.18, 0.25, 0.95).normalize_or(Vec3::Z),
            0.64 * sc,
            0.018,
            32,
            sk_bone,
            MAT_ENERGY_CONDUIT,
            5,
            4,
        );
    }

    eye_total
}

/// Add a 3D ring/torus into the transparent pass (for AEGIS shield barrier & ORACLE astrolabe).
#[allow(clippy::too_many_arguments)]
fn add_3d_ring_torus(
    mb: &mut MeshBuilder,
    center: Vec3,
    axis: Vec3,
    major_r: f32,
    minor_r: f32,
    segs: usize,
    bone: u32,
    mat_id: u32,
    head_idx: u32,
    flags: u32,
) {
    let (u_ax, v_ax) = tube_frame(axis, Vec3::Y);
    let r_segs = 6usize;
    let start_vert = mb.vertices.len();
    for i in 0..=segs {
        let u = i as f32 / segs as f32;
        let theta = u * TAU;
        let (st, ct) = theta.sin_cos();
        let radial = u_ax * ct + v_ax * st;
        let ring_c = center + radial * major_r;
        let tang = (v_ax * ct - u_ax * st).normalize_or(Vec3::X);
        for j in 0..=r_segs {
            let v = j as f32 / r_segs as f32;
            let phi = v * TAU;
            let (sp, cp) = phi.sin_cos();
            let n = radial * cp + axis * sp;
            let pos = ring_c + n * minor_r;
            mb.push_vertex(
                pos,
                n,
                [tang.x, tang.y, tang.z, 1.0],
                [u * 4.0, v],
                [bone, 0, 0, 0],
                [1.0, 0.0, 0.0, 0.0],
                mat_id,
                head_idx,
                flags,
            );
        }
    }
    let stride = (r_segs + 1) as u32;
    for i in 0..segs as u32 {
        for j in 0..r_segs as u32 {
            let a = start_vert as u32 + i * stride + j;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            mb.push_tri(a, c, b, false, true);
            mb.push_tri(b, c, d, false, true);
        }
    }
}

/// Add a flat-topped octagonal carved stone plinth (for the Sanctuary Dais).
fn add_octagonal_plinth(
    mb: &mut MeshBuilder,
    center: Vec3,
    rx: f32,
    height: f32,
    rz: f32,
    bevel: f32,
    bone: u32,
    mat_id: u32,
) {
    let top_y = center.y + height * 0.5;
    let bot_y = center.y - height * 0.5;
    let top_center = mb.push_vertex(
        Vec3::new(center.x, top_y, center.z),
        Vec3::Y,
        [1.0, 0.0, 0.0, 1.0],
        [0.5, 0.5],
        [bone, 0, 0, 0],
        [1.0, 0.0, 0.0, 0.0],
        mat_id,
        7,
        0,
    );
    let sides = 8usize;
    for i in 0..sides {
        let a0 = (i as f32 / sides as f32) * TAU + PI / 8.0;
        let a1 = ((i + 1) as f32 / sides as f32) * TAU + PI / 8.0;
        let (s0, c0) = a0.sin_cos();
        let (s1, c1) = a1.sin_cos();
        let t0 = Vec3::new(
            center.x + c0 * rx * bevel,
            top_y,
            center.z + s0 * rz * bevel,
        );
        let t1 = Vec3::new(
            center.x + c1 * rx * bevel,
            top_y,
            center.z + s1 * rz * bevel,
        );
        let b0 = Vec3::new(center.x + c0 * rx, bot_y, center.z + s0 * rz);
        let b1 = Vec3::new(center.x + c1 * rx, bot_y, center.z + s1 * rz);

        let vt0 = mb.push_vertex(
            t0,
            Vec3::Y,
            [1.0, 0.0, 0.0, 1.0],
            [c0 * 0.5 + 0.5, s0 * 0.5 + 0.5],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            7,
            0,
        );
        let vt1 = mb.push_vertex(
            t1,
            Vec3::Y,
            [1.0, 0.0, 0.0, 1.0],
            [c1 * 0.5 + 0.5, s1 * 0.5 + 0.5],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            7,
            0,
        );
        mb.push_tri(top_center, vt1, vt0, false, false);

        let side_n = Vec3::new(c0 + c1, 0.35, s0 + s1).normalize_or(Vec3::Z);
        let side_t = (t1 - t0).normalize_or(Vec3::X);
        let st0 = mb.push_vertex(
            t0,
            side_n,
            [side_t.x, side_t.y, side_t.z, 1.0],
            [0.0, 0.0],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            7,
            0,
        );
        let st1 = mb.push_vertex(
            t1,
            side_n,
            [side_t.x, side_t.y, side_t.z, 1.0],
            [1.0, 0.0],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            7,
            0,
        );
        let sb0 = mb.push_vertex(
            b0,
            side_n,
            [side_t.x, side_t.y, side_t.z, 1.0],
            [0.0, 1.0],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            7,
            0,
        );
        let sb1 = mb.push_vertex(
            b1,
            side_n,
            [side_t.x, side_t.y, side_t.z, 1.0],
            [1.0, 1.0],
            [bone, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            mat_id,
            7,
            0,
        );
        mb.push_tri(st0, st1, sb1, false, false);
        mb.push_tri(st0, sb1, sb0, false, false);
    }
}

/// Build the carved Obsidian Dais and towering Gothic Cathedral Pillars in `-Z` Sanctuary.
fn build_gothic_sanctuary(mb: &mut MeshBuilder, root: Vec3) {
    // 2-tier carved octagonal obsidian altar dais anchoring Leviathan's forelimbs
    add_octagonal_plinth(
        mb,
        root + Vec3::new(0.0, -0.10, 0.30),
        2.35,
        0.24,
        1.45,
        0.86,
        BONE_ROOT as u32,
        MAT_SANCTUARY_STONE,
    );
    add_octagonal_plinth(
        mb,
        root + Vec3::new(0.0, -0.34, 0.25),
        3.45,
        0.26,
        2.05,
        0.88,
        BONE_ROOT as u32,
        MAT_SANCTUARY_STONE,
    );

    // Towering fluted gothic obsidian cathedral pillars receding in Z depth behind the dragon
    let pillars = [
        (Vec3::new(-3.4, 0.8, -3.4), 0.34, 4.4),
        (Vec3::new(3.4, 0.8, -3.4), 0.34, 4.4),
        (Vec3::new(-5.8, 1.0, -5.2), 0.42, 5.2),
        (Vec3::new(5.8, 1.0, -5.2), 0.42, 5.2),
        (Vec3::new(-1.9, 1.2, -6.5), 0.30, 5.4),
        (Vec3::new(1.9, 1.2, -6.5), 0.30, 5.4),
    ];
    for (p, r, h) in pillars {
        add_box_prism(
            mb,
            p,
            Vec3::new(r, h, r),
            BONE_ROOT as u32,
            MAT_SANCTUARY_STONE,
            false,
        );
        // Chamfered obsidian pillar capital
        add_box_prism(
            mb,
            p + Vec3::new(0.0, h * 0.72, 0.0),
            Vec3::new(r * 1.45, 0.22, r * 1.45),
            BONE_ROOT as u32,
            MAT_SANCTUARY_STONE,
            false,
        );
    }

    // Pointed Gothic Sanctuary Arch Ribs spanning high across the background vault
    for z in [-3.4_f32, -5.2_f32] {
        let span = if z > -4.0 { 3.4 } else { 5.8 };
        for side in [-1.0_f32, 1.0_f32] {
            add_curved_horn(
                mb,
                &[
                    Vec3::new(side * span, 2.8, z),
                    Vec3::new(side * span * 0.55, 4.3, z),
                    Vec3::new(0.0, 5.1, z),
                ],
                0.22,
                0.14,
                6,
                false,
                MAT_SANCTUARY_STONE,
                7,
                BONE_ROOT as u32,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_mesh_has_valid_indices_normalized_weights_and_bounded_size() {
        let mesh = build_leviathan_mesh();
        assert!(
            mesh.vertices.len() > 3_000,
            "too few vertices: {}",
            mesh.vertices.len()
        );
        assert!(mesh.opaque_indices.len() > 6_000);
        assert!(!mesh.transparent_indices.is_empty());
        assert_eq!(mesh.opaque_indices.len() % 3, 0);
        assert_eq!(mesh.transparent_indices.len() % 3, 0);
        assert_eq!(mesh.shadow_indices.len() % 3, 0);

        let v_count = mesh.vertices.len() as u32;
        for &idx in mesh
            .opaque_indices
            .iter()
            .chain(mesh.transparent_indices.iter())
            .chain(mesh.shadow_indices.iter())
        {
            assert!(
                idx < v_count,
                "out-of-bounds vertex index {idx} >= {v_count}"
            );
        }

        for (vi, v) in mesh.vertices.iter().enumerate() {
            for f in v
                .pos
                .iter()
                .chain(v.normal.iter())
                .chain(v.uv.iter())
                .chain(v.tangent.iter())
            {
                assert!(f.is_finite(), "non-finite attribute on vertex {vi}");
            }
            for &j in &v.joints {
                assert!((j as usize) < BONE_COUNT, "bone index {j} out of range");
            }
            let wsum: f32 = v.weights.iter().sum();
            assert!((wsum - 1.0).abs() < 1e-3, "unnormalized weights sum={wsum}");
        }

        let total_gpu_bytes = mesh.vertex_bytes().len()
            + mesh.opaque_indices.len() * 4
            + mesh.transparent_indices.len() * 4;
        // Strictly bounded well under 32 MiB VRAM budget!
        assert!(
            total_gpu_bytes < 16 * 1024 * 1024,
            "mesh memory unexpectedly large: {total_gpu_bytes}"
        );
    }
}
