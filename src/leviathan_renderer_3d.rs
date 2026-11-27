//! Native Multi-Pass 3D WGPU PBR + Shadow + Bloom Renderer for **HYDRA: Project Incarnation**.
//!
//! Architecture:
//! - Pass 1 (`shadow_pipeline`): GPU-skinned vertex shader renders Leviathan depth into a
//!   `Depth32Float` shadow map (`1024^2` on High, `2048^2` on Ultra/Cinematic).
//! - Pass 2 (`hdr_target` `Rgba16Float` + `Depth32Float`):
//!   - Atmospheric Gothic Sanctuary backdrop (volumetric amethyst god-rays, mist, drifting stars).
//!   - GPU-skinned Opaque 3D Leviathan & Gothic Architecture with Cook-Torrance GGX PBR BRDF,
//!     3x3 PCF soft shadows, tangent-space normal-mapped two-row imbricated obsidian dragon scales,
//!     deep amethyst subsurface bioluminescence, faceted Amethyst Heart refraction & caustics,
//!     and 3D reptilian eyes with gaze tracking, slit pupils, wet glints, and blinking eyelids.
//!   - Depth-tested Translucent Pass (`wing membranes`, `AEGIS 3D shield dome`, `ORACLE astrolabe`,
//!     and 3D atmospheric amethyst dust motes).
//! - Pass 3 (`bloom_extract_h` + `bloom_v` at half HDR resolution on Medium..Cinematic): controlled
//!   high-threshold bloom (`L > 1.05`) so eyes, Heart core, and real event conduits shine cleanly.
//! - Pass 4 (`composite` inside egui's `RenderPass`): ACES Filmic tonemapping, vignette, dither,
//!   and sRGB conversion directly into the native command canvas.
//!
//! VRAM is strictly bounded by clamping render resolution to `<= 2560x1440` and reusing persistent
//! offscreen targets across frames without per-frame heap/texture churn.

use crate::leviathan_3d_math::{LeviathanCamera, LeviathanSkeleton, Mat4, Vec3, BONE_COUNT};
use crate::leviathan_mesh::{build_leviathan_mesh, GPU_VERTEX_STRIDE};
use eframe::egui::{self, Color32, Rect};
use eframe::egui_wgpu::{self, wgpu};
use std::borrow::Cow;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

// 2 mat4 (128B) + 5 vec4 (80B) + 21 head vec4s (336B) + 96 mat4 bones (6144B) = 6688B.
pub const SCENE_UNIFORM_BYTES: usize = 64 * 2 + 16 * 5 + 16 * 21 + 64 * BONE_COUNT;

const SHADOW_WGSL: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    heart_params: vec4<f32>,
    env_params: vec4<f32>,
    post_params: vec4<f32>,
    res_params: vec4<f32>,
    head_col: array<vec4<f32>, 7>,
    head_state: array<vec4<f32>, 7>,
    head_look: array<vec4<f32>, 7>,
    bones: array<mat4x4<f32>, 96>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
    @location(6) v_meta: vec4<u32>,
};

@vertex
fn vs_shadow(v: VsIn) -> @builtin(position) vec4<f32> {
    let j0 = min(v.joints.x, 95u);
    let j1 = min(v.joints.y, 95u);
    let j2 = min(v.joints.z, 95u);
    let j3 = min(v.joints.w, 95u);
    let p4 = vec4<f32>(v.pos, 1.0);
    let skinned = (u.bones[j0] * p4) * v.weights.x
        + (u.bones[j1] * p4) * v.weights.y
        + (u.bones[j2] * p4) * v.weights.z
        + (u.bones[j3] * p4) * v.weights.w;
    return u.light_view_proj * skinned;
}
"#;

const SCENE_WGSL: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,        // xyz=eye, w=time*motion
    heart_params: vec4<f32>,   // xyz=heart_world, w=heart_pulse
    env_params: vec4<f32>,     // x=offering_glow, y=breath, z=quality(0..4), w=motion(0|1)
    post_params: vec4<f32>,    // x=is_srgb, y=intro_fade, z=suspended, w=shadow_enabled
    res_params: vec4<f32>,     // x=hdr_w, y=hdr_h, z=bloom_strength, w=aspect
    head_col: array<vec4<f32>, 7>,   // rgb=accent, w=emphasis
    head_state: array<vec4<f32>, 7>, // x=activity, y=warning, z=eye_open, w=observation
    head_look: array<vec4<f32>, 7>,  // xy=gaze, zw=head_uv
    bones: array<mat4x4<f32>, 96>,
};

@group(0) @binding(0)
var<uniform> u: Uniforms;
@group(0) @binding(1)
var shadow_tex: texture_depth_2d;
@group(0) @binding(2)
var shadow_samp: sampler_comparison;

// ----------------------------------------------------------------------------
// Atmospheric Sanctuary Skybox Pass (Full-screen background behind 3D world)
// ----------------------------------------------------------------------------

struct SkyOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) idx: u32) -> SkyOut {
    var out: SkyOut;
    let x = f32((idx << 1u) & 2u);
    let y = f32(idx & 2u);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.9999, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

fn hash21(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let d = dot(q, q + 45.32);
    return fract(d * (q.x + q.y));
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let s = f * f * (3.0 - 2.0 * f);
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

fn fbm(p: vec2<f32>, oct: i32) -> f32 {
    var v = 0.0;
    var amp = 0.52;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        if (i >= oct) {
            break;
        }
        v = v + amp * value_noise(q);
        q = q * 2.08 + vec2<f32>(11.3, 7.1);
        amp = amp * 0.48;
    }
    return v;
}

@fragment
fn fs_sky(in: SkyOut) -> @location(0) vec4<f32> {
    let aspect = max(u.res_params.w, 0.25);
    let p = vec2<f32>((in.uv.x - 0.5) * aspect, in.uv.y - 0.5);
    let parallax = vec2<f32>(u.cam_pos.x, u.cam_pos.y) * 0.035;
    let t = u.cam_pos.w;
    let pulse = clamp(u.heart_params.w, 0.0, 1.0);
    let q_lvl = u.env_params.z;

    let d_center = length(vec2<f32>(p.x * 0.85, p.y * 1.15));
    var col = mix(
        vec3<f32>(0.020, 0.016, 0.046),
        vec3<f32>(0.006, 0.008, 0.020),
        clamp(d_center * 1.15, 0.0, 1.0)
    );

    if (q_lvl >= 1.0) {
        let oct = select(2, 3, q_lvl >= 2.0);
        let drift = vec2<f32>(t * 0.011, -t * 0.016);
        let n1 = fbm((p - parallax) * 2.3 + drift, oct);
        let n2 = fbm((p - parallax * 1.4) * 4.2 - drift * 1.3 + vec2<f32>(3.1, 1.7), oct);
        let mist = smoothstep(0.28, 0.86, n1 * 0.60 + n2 * 0.44) * exp(-d_center * d_center * 0.90);
        let mist_col = mix(vec3<f32>(0.24, 0.08, 0.44), vec3<f32>(0.05, 0.14, 0.26), smoothstep(0.2, 0.8, n2));
        col = col + mist_col * mist * (0.20 + 0.04 * q_lvl);

        // Cathedral god-rays radiating from upper amethyst sanctuary vault
        let origin = vec2<f32>(0.0, 0.14) - parallax;
        let dp = p - origin;
        let ang = atan2(dp.y, dp.x);
        let ray = pow(max(0.0, cos(ang * 7.0 + 1.5708)), 10.0) * exp(-length(dp) * 2.2);
        col = col + vec3<f32>(0.40, 0.16, 0.72) * ray * (0.09 + 0.06 * pulse);
    }

    // Subtle distant celestial dust stars with camera parallax
    if (q_lvl >= 2.0) {
        let star_p = (p - parallax * 0.8) * 18.0;
        let cell = floor(star_p);
        let f = fract(star_p) - 0.5;
        let rnd = hash21(cell);
        if (rnd > 0.86) {
            let off = vec2<f32>(hash21(cell + 13.7) - 0.5, hash21(cell + 37.1) - 0.5) * 0.65;
            let twinkle = 0.55 + 0.45 * sin(t * 2.1 + rnd * 35.0);
            let s = exp(-dot(f - off, f - off) * 110.0) * twinkle * smoothstep(1.2, 0.2, d_center);
            col = col + mix(vec3<f32>(0.76, 0.58, 1.0), vec3<f32>(0.48, 0.86, 1.0), fract(rnd * 9.3)) * s * 0.34;
        }
    }

    return vec4<f32>(col, 1.0);
}

// ----------------------------------------------------------------------------
// 3D Skinned Mesh Vertex & PBR Fragment Shader
// ----------------------------------------------------------------------------

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
    @location(4) joints: vec4<u32>,
    @location(5) weights: vec4<f32>,
    @location(6) v_meta: vec4<u32>,
};

struct MeshOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) world_n: vec3<f32>,
    @location(2) world_t: vec3<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) shadow_pos: vec4<f32>,
    @location(5) local_pos: vec3<f32>,
    @location(6) @interpolate(flat) v_meta: vec4<u32>,
};

fn skin_vec3(v: vec3<f32>, w: vec4<f32>, j: vec4<u32>, is_point: f32) -> vec3<f32> {
    let j0 = min(j.x, 95u);
    let j1 = min(j.y, 95u);
    let j2 = min(j.z, 95u);
    let j3 = min(j.w, 95u);
    let v4 = vec4<f32>(v, is_point);
    let out4 = (u.bones[j0] * v4) * w.x
        + (u.bones[j1] * v4) * w.y
        + (u.bones[j2] * v4) * w.z
        + (u.bones[j3] * v4) * w.w;
    return out4.xyz;
}

@vertex
fn vs_mesh(v: VsIn) -> MeshOut {
    var out: MeshOut;
    let wp = skin_vec3(v.pos, v.weights, v.joints, 1.0);
    let wn = normalize(skin_vec3(v.normal, v.weights, v.joints, 0.0));
    let wt = normalize(skin_vec3(v.tangent.xyz, v.weights, v.joints, 0.0));

    out.clip_pos = u.view_proj * vec4<f32>(wp, 1.0);
    out.world_pos = wp;
    out.world_n = wn;
    out.world_t = wt;
    out.uv = v.uv;
    out.shadow_pos = u.light_view_proj * vec4<f32>(wp, 1.0);
    out.local_pos = v.pos;
    out.v_meta = v.v_meta;
    return out;
}

// Two-row staggered overlapping obsidian dragon scales (zero checkerboard holes!)
fn dragon_scales(su: f32, sv: f32) -> vec4<f32> {
    let row_a = floor(sv);
    let gu_a = fract(su + stagger_val(row_a)) - 0.5;
    let gv_a = fract(sv) - 0.5;

    let sv_b = sv - 0.5;
    let row_b = floor(sv_b);
    let gu_b = fract(su + stagger_val(row_b)) - 0.5;
    let gv_b = fract(sv_b) - 0.5;

    let sx_a = gu_a * 1.92 * (1.0 + max(gv_a, 0.0) * 0.80);
    let sy_a = (gv_a - 0.02) * 1.36;
    let d_a = length(vec2<f32>(sx_a, sy_a)) - 0.94;
    let mask_a = 1.0 - smoothstep(-0.08, 0.06, d_a);

    let sx_b = gu_b * 1.92 * (1.0 + max(gv_b, 0.0) * 0.80);
    let sy_b = (gv_b - 0.02) * 1.36;
    let d_b = length(vec2<f32>(sx_b, sy_b)) - 0.94;

    let use_a = select(mask_a, 1.0, gv_a < 0.02 && mask_a > 0.5);
    let sx = mix(sx_b, sx_a, use_a);
    let sy = mix(sy_b, sy_a, use_a);
    let gu = mix(gu_b, gu_a, use_a);
    let gv = mix(gv_b, gv_a, use_a);
    let d_front = mix(d_b, d_a, use_a);

    let r2 = clamp(sx * sx + sy * sy, 0.0, 1.0);
    let dome = sqrt(max(0.0, 1.0 - r2));
    let keel = exp(-abs(gu) * 11.0) * smoothstep(-0.45, 0.40, gv) * 0.24;
    let lip_shadow = mix(smoothstep(0.0, 0.22, max(d_a, 0.0)), 1.0, use_a);
    let height = clamp((0.32 + dome * 0.60 + keel) * (0.44 + 0.56 * lip_shadow), 0.14, 1.0);

    let gx = -sx * 1.15 - sign(gu) * exp(-abs(gu) * 11.0) * 0.45;
    let gy = -sy * 1.05 + smoothstep(0.25, 0.58, gv) * 1.10;

    let thin_seam = exp(-abs(d_front + 0.02) * 28.0) * (1.0 - smoothstep(0.26, 0.52, dome));
    let vein_select = smoothstep(0.55, 0.85, value_noise(vec2<f32>(su * 0.28, sv * 0.22)));
    return vec4<f32>(height, gx, gy, clamp(thin_seam * vein_select, 0.0, 1.0));
}

fn stagger_val(row: f32) -> f32 {
    return (row - floor(row * 0.5) * 2.0) * 0.5;
}

// Cook-Torrance GGX PBR Microfacet Distribution, Geometry & Fresnel
fn pbr_ggx(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, albedo: vec3<f32>, metallic: f32, rough: f32) -> vec3<f32> {
    let h = normalize(v + l);
    let ndl = max(dot(n, l), 0.0);
    let ndv = max(dot(n, v), 0.001);
    let ndh = max(dot(n, h), 0.0);
    let hdv = max(dot(h, v), 0.0);

    let a = max(rough * rough, 0.02);
    let a2 = a * a;
    let denom = ndh * ndh * (a2 - 1.0) + 1.0;
    let d_ggx = a2 / max(3.14159265 * denom * denom, 1e-5);

    let k = ((rough + 1.0) * (rough + 1.0)) * 0.125;
    let g_v = ndv / (ndv * (1.0 - k) + k);
    let g_l = ndl / (ndl * (1.0 - k) + k);
    let g_smith = g_v * g_l;

    let f0 = mix(vec3<f32>(0.055, 0.050, 0.075), albedo, metallic);
    let fres = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - hdv, 5.0);

    let spec = (d_ggx * g_smith * fres) / max(4.0 * ndv * ndl, 1e-4);
    let kd = (vec3<f32>(1.0) - fres) * (1.0 - metallic);
    let diff = kd * albedo / 3.14159265;
    return (diff + spec) * ndl;
}

// 3x3 PCF shadow evaluation called in UNIFORM control flow for strict WGSL compliance!
fn sample_shadow(shadow_pos: vec4<f32>, ndl_key: f32) -> f32 {
    let proj = shadow_pos.xyz / max(shadow_pos.w, 1e-5);
    let suv = vec2<f32>(proj.x * 0.5 + 0.5, 0.5 - proj.y * 0.5);
    let in_bounds = suv.x >= 0.0 && suv.x <= 1.0 && suv.y >= 0.0 && suv.y <= 1.0 && proj.z >= 0.0 && proj.z <= 1.0;
    let clamped_uv = clamp(suv, vec2<f32>(0.002), vec2<f32>(0.998));
    let bias = max(0.0022 * (1.0 - ndl_key), 0.0008);
    let cmp_z = clamp(proj.z - bias, 0.0, 1.0);

    let texel = 1.0 / 1536.0;
    var vis = 0.0;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let off = vec2<f32>(f32(dx), f32(dy)) * texel;
            vis = vis + textureSampleCompare(shadow_tex, shadow_samp, clamped_uv + off, cmp_z);
        }
    }
    let pcf = vis / 9.0;
    let sh_vis = select(1.0, pcf, in_bounds && u.post_params.w > 0.5);
    return mix(0.28, 1.0, sh_vis);
}

@fragment
fn fs_opaque(in: MeshOut) -> @location(0) vec4<f32> {
    let mat_id = in.v_meta.x;
    let head_idx = min(in.v_meta.y, 7u);
    let flags = in.v_meta.z;
    let q_lvl = u.env_params.z;
    let t_anim = u.cam_pos.w;
    let pulse = clamp(u.heart_params.w, 0.0, 1.0);
    let offering = clamp(u.env_params.x, 0.0, 1.0);

    let v = normalize(u.cam_pos.xyz - in.world_pos);
    let n_base = normalize(in.world_n);
    let t_vec = normalize(in.world_t - n_base * dot(n_base, in.world_t));
    let b_vec = normalize(cross(n_base, t_vec));

    var head_accent = vec3<f32>(0.68, 0.34, 1.0);
    var head_emph = 0.0;
    var head_act = 0.0;
    var head_warn = 0.0;
    var head_eye_open = 1.0;
    var head_obs = 0.0;
    var head_gaze = vec2<f32>(0.0, 0.0);
    if (head_idx < 7u) {
        let hc = u.head_col[head_idx];
        let hs = u.head_state[head_idx];
        let hl = u.head_look[head_idx];
        head_warn = clamp(hs.y, 0.0, 1.0);
        head_accent = mix(hc.rgb, vec3<f32>(0.98, 0.65, 0.22), head_warn * 0.72);
        head_emph = clamp(hc.w, 0.0, 1.0);
        head_act = clamp(hs.x, 0.0, 1.0);
        head_eye_open = clamp(hs.z, 0.05, 1.0);
        head_obs = clamp(hs.w, 0.0, 1.0);
        head_gaze = hl.xy;
    }

    // Special Material 5: Living 3D Reptilian Eyeballs (radial fibrous iris, vertical slit pupil, eyelids)
    if (mat_id == 5u && head_idx < 7u) {
        let sk_m = u.bones[27u + head_idx * 8u + 6u];
        let sk_r = normalize(sk_m[0].xyz);
        let sk_u = normalize(sk_m[1].xyz);
        let look_off = sk_r * (head_gaze.x * 0.26) + sk_u * (head_gaze.y * 0.18);
        let iris_vec = normalize(n_base - look_off * 0.45);
        let lx = dot(n_base, sk_r) - head_gaze.x * 0.26;
        let ly = dot(n_base, sk_u) - head_gaze.y * 0.18;
        let r_iris = length(vec2<f32>(lx, ly)) * 1.48;
        let ang = atan2(ly, lx);

        // Eyelid blink curtain closing from top/bottom in skull local space
        let lid_dist = abs(dot(n_base, sk_u));
        if (lid_dist > head_eye_open * 0.86) {
            return vec4<f32>(0.02, 0.015, 0.04, 1.0);
        }

        let fibers = 0.80 + 0.20 * sin(ang * 16.0 + r_iris * 9.0);
        let limbal = 1.0 - smoothstep(0.62, 0.98, r_iris);
        let iris_col = mix(head_accent * 2.1, vec3<f32>(1.0, 0.98, 0.94), exp(-r_iris * 2.5) * 0.58);
        var eye_out = iris_col * fibers * limbal * (1.15 + 0.65 * head_emph + 0.95 * head_act + 0.70 * head_warn);

        // Razor-sharp vertical reptilian slit pupil
        let slit_w = 0.11 + 0.06 * head_emph;
        let in_slit = (1.0 - smoothstep(slit_w * 0.6, slit_w * 1.4, abs(lx)))
            * (1.0 - smoothstep(0.32, 0.54, abs(ly)));
        eye_out = mix(eye_out, vec3<f32>(0.008, 0.006, 0.016), in_slit);

        // Wet corneal specular pinpoint glint
        let glint_dir = normalize(v + normalize(vec3<f32>(-0.45, 0.65, 0.60)));
        let glint = pow(max(dot(iris_vec, glint_dir), 0.0), 180.0) * 2.6;
        eye_out = eye_out + vec3<f32>(1.0, 0.98, 1.0) * glint;
        return vec4<f32>(eye_out, 1.0);
    }

    // Material Setup & Procedural Surface Micro-Normal Detail
    var n = n_base;
    var albedo = vec3<f32>(0.04, 0.036, 0.068);
    var metallic = 0.18;
    var rough = 0.26;
    var ao = 1.0;
    var emissive = vec3<f32>(0.0);

    if (mat_id == 0u) {
        // Volcanic Obsidian Dragon Scales (crisp tangent-space relief & silver-violet sheen)
        let sc_scale = mix(1.15, 1.48, clamp(q_lvl / 3.0, 0.0, 1.0));
        let sc = dragon_scales(in.uv.x * sc_scale, in.uv.y * sc_scale);
        if (q_lvl >= 1.0) {
            n = normalize(n + (t_vec * sc.y + b_vec * sc.z) * 0.42);
        }
        ao = clamp(0.42 + 0.58 * sc.x, 0.28, 1.0);
        albedo = mix(vec3<f32>(0.038, 0.034, 0.068), vec3<f32>(0.105, 0.095, 0.160), sc.x);
        metallic = 0.28;
        rough = mix(0.32, 0.15, sc.x);

        // Amethyst bioluminescence beneath selected obsidian scale seams
        let bio_tint = mix(vec3<f32>(0.62, 0.20, 1.0), head_accent, select(0.15, 0.55, head_idx < 7u));
        let base_seam = sc.w * (0.24 + 0.20 * pulse + 0.36 * offering);
        let travel_wave = exp(-pow((fract(in.uv.y * 0.045 - t_anim * 0.72) - 0.5) * 6.5, 2.0));
        let active_seam = sc.w * travel_wave * (head_act * 2.2 + head_obs * 1.1 + head_warn * 1.4);
        emissive = bio_tint * (base_seam + active_seam);
    } else if (mat_id == 2u) {
        // Armored Ventral Throat Scutes
        let pv = fract(in.uv.y * 1.2) - 0.5;
        let lip = smoothstep(-0.48, -0.15, pv) * (1.0 - smoothstep(0.32, 0.50, pv));
        ao = 0.48 + 0.52 * lip;
        albedo = vec3<f32>(0.072, 0.064, 0.120);
        rough = 0.26;
        let seam = exp(-abs(pv - 0.38) * 24.0);
        emissive = head_accent * seam * (0.04 * pulse + head_act * 0.9 + head_obs * 0.45);
    } else if (mat_id == 1u) {
        // Keratin Dragon Horns, Ribs & Claws
        let tip_fade = smoothstep(0.0, 3.8, in.uv.y);
        albedo = mix(vec3<f32>(0.09, 0.08, 0.14), vec3<f32>(0.42, 0.36, 0.48), tip_fade * 0.70);
        metallic = 0.12;
        rough = mix(0.24, 0.14, tip_fade);
        emissive = head_accent * (1.0 - tip_fade) * (0.05 + 0.20 * head_act + 0.16 * head_emph);
    } else if (mat_id == 7u) {
        // Gleaming Ivory Fangs & Teeth
        albedo = vec3<f32>(0.90, 0.86, 0.76);
        metallic = 0.04;
        rough = 0.14;
        emissive = vec3<f32>(0.10, 0.09, 0.07);
    } else if (mat_id == 4u) {
        // Central Faceted Amethyst Heart Crystal (rich deep royal-violet jewel facets!)
        let fres = pow(1.0 - max(dot(n, v), 0.0), 2.8);
        let caustic = 0.5 + 0.5 * sin(in.local_pos.x * 20.0 + in.local_pos.y * 16.0 + t_anim * 1.4);
        let core_dist = length(in.world_pos - u.heart_params.xyz);
        let inner_fire = exp(-core_dist * 4.5) * (0.55 + 0.45 * pulse + 0.45 * offering);
        albedo = mix(vec3<f32>(0.14, 0.02, 0.34), vec3<f32>(0.58, 0.14, 0.96), 0.30 + 0.45 * caustic);
        metallic = 0.10;
        rough = 0.08;
        emissive = mix(vec3<f32>(0.34, 0.06, 0.72), vec3<f32>(0.78, 0.32, 1.0), fres * 0.55 + pulse * 0.25)
            * (0.48 + 0.38 * pulse + 0.45 * offering + inner_fire * 0.35);
        if (offering > 0.05) {
            emissive = emissive + vec3<f32>(0.96, 0.74, 0.32) * fres * offering * 0.55;
        }
    } else if (mat_id == 8u) {
        // Integrated Energy Conduits & Neural Crown Shards
        let pulse_pip = exp(-pow((fract(in.uv.y * 0.35 - t_anim * 0.75) - 0.5) * 5.0, 2.0));
        let cond_amp = select(0.55 + 0.40 * pulse, 0.22 + 1.2 * head_act + 0.95 * offering + 0.75 * head_emph, head_idx < 7u);
        let conduit_col = select(vec3<f32>(0.76, 0.40, 1.0), head_accent, head_idx < 7u);
        emissive = conduit_col * (0.35 + pulse_pip * 0.95) * cond_amp;
        albedo = conduit_col * 0.30;
        rough = 0.14;
    } else if (mat_id == 6u) {
        // Monumental Gothic Obsidian Sanctuary Stone & Carved Dais
        let tex_n = value_noise(in.world_pos.xy * 4.5 + in.world_pos.zz * 2.2);
        albedo = vec3<f32>(0.028, 0.024, 0.052) * (0.75 + 0.50 * tex_n);
        metallic = 0.14;
        rough = 0.44;
        ao = smoothstep(-3.6, 2.5, in.world_pos.y) * 0.65 + 0.35;
    }

    // Scene Lighting: Key Moonlit Light (shadow-mapped) + Rim + Front Sanctuary Fill + Central Heart
    let l_key = normalize(vec3<f32>(-0.50, 0.72, 0.48));
    let key_col = vec3<f32>(0.88, 0.92, 1.0) * 2.4;
    let ndl_key = max(dot(n, l_key), 0.0);
    let shadow = sample_shadow(in.shadow_pos, ndl_key);

    let l_rim = normalize(vec3<f32>(0.68, 0.42, -0.58));
    let rim_col = vec3<f32>(0.58, 0.30, 1.0) * 1.75;

    // Cool silver-violet front sanctuary bounce light revealing carved dragon skulls & chest armor
    let l_fill = normalize(vec3<f32>(0.18, 0.25, 0.95));
    let fill_col = vec3<f32>(0.38, 0.36, 0.64) * 0.72;

    let to_heart = u.heart_params.xyz - in.world_pos;
    let h_dist = max(length(to_heart), 0.12);
    let l_heart = to_heart / h_dist;
    let h_atten = 1.0 / (1.0 + 0.26 * h_dist * h_dist);
    let heart_lcol = mix(
        vec3<f32>(0.62, 0.18, 1.0),
        vec3<f32>(0.90, 0.54, 1.0),
        pulse * 0.42 + offering * 0.48
    ) * (2.6 + 1.3 * pulse + 1.5 * offering);

    let direct_key = pbr_ggx(n, v, l_key, albedo, metallic, rough) * key_col * shadow;
    let direct_rim = pbr_ggx(n, v, l_rim, albedo, metallic, rough) * rim_col;
    let direct_fill = pbr_ggx(n, v, l_fill, albedo, metallic, rough) * fill_col;
    let direct_heart = pbr_ggx(n, v, l_heart, albedo, metallic, rough) * heart_lcol * h_atten;

    // Grazing Fresnel obsidian silhouette rim
    let ndv = max(dot(n, v), 0.001);
    let silhouette = pow(1.0 - ndv, 3.8) * mix(vec3<f32>(0.40, 0.24, 0.78), head_accent, 0.40)
        * (0.40 + 0.60 * shadow);

    let ambient = albedo * vec3<f32>(0.24, 0.21, 0.40) * ao;
    let total = ambient + (direct_key + direct_rim + direct_fill + direct_heart) * ao + silhouette + emissive;

    // Gentle distance fog blending deeper sanctuary pillars seamlessly into obsidian mist
    let cam_dist = length(u.cam_pos.xyz - in.world_pos);
    let fog = smoothstep(8.5, 19.5, cam_dist);
    let fog_col = vec3<f32>(0.016, 0.012, 0.036);
    return vec4<f32>(mix(total, fog_col, fog * 0.72), 1.0);
}

@fragment
fn fs_transparent(in: MeshOut) -> @location(0) vec4<f32> {
    let mat_id = in.v_meta.x;
    let pulse = clamp(u.heart_params.w, 0.0, 1.0);
    let offering = clamp(u.env_params.x, 0.0, 1.0);
    let t_anim = u.cam_pos.w;

    if (mat_id == 9u) {
        // AEGIS 3D Hexagonal Shield Ring (sleek arcane barrier during activity/warnings/selection)
        let hs = u.head_state[3];
        let hc = u.head_col[3];
        let vis = clamp(hs.x * 1.4 + hs.y * 1.2 + hc.w * 0.85, 0.0, 1.0);
        if (vis < 0.04) {
            discard;
        }
        let col = mix(hc.rgb, vec3<f32>(0.98, 0.65, 0.22), hs.y * 0.75);
        let shimmer = 0.65 + 0.35 * sin(in.uv.x * 12.0 + t_anim * 2.4);
        return vec4<f32>(col * 1.15 * shimmer, 0.28 * vis);
    }

    if (mat_id == 8u) {
        // ORACLE celestial astrolabe ring
        let hs = u.head_state[5];
        let hc = u.head_col[5];
        let shimmer = 0.55 + 0.45 * sin(in.uv.x * 8.0 - t_anim * 1.6);
        let str = 0.35 + hs.x * 0.9 + hc.w * 0.7;
        return vec4<f32>(hc.rgb * 1.35 * shimmer * str, 0.58);
    }

    // Translucent Backlit Leather Gothic Dragon Wing Membrane with radiant veins
    let n = normalize(in.world_n);
    let v = normalize(u.cam_pos.xyz - in.world_pos);
    let h_dist = length(in.world_pos - u.heart_params.xyz);
    let sss = exp(-h_dist * 0.34) * (0.55 + 0.35 * pulse + 0.45 * offering);
    let rib_vein = pow(max(0.0, cos(in.uv.x * 3.14159 * 2.0)), 14.0) * 0.45;
    let micro_vein = smoothstep(0.56, 0.94, value_noise(in.uv * 4.5)) * 0.32;
    let rim = pow(1.0 - abs(dot(n, v)), 2.2);
    let mem = vec3<f32>(0.028, 0.018, 0.068)
        + vec3<f32>(0.36, 0.12, 0.72) * (sss * 0.62 + (rib_vein + micro_vein) * (0.45 + sss))
        + vec3<f32>(0.48, 0.24, 0.88) * rim * 0.35;
    return vec4<f32>(mem, 0.90);
}

// ----------------------------------------------------------------------------
// Pass 3: Controlled HDR Bloom Extract + Separable Blur + Final ACES Composite
// ----------------------------------------------------------------------------

@group(0) @binding(0)
var src_tex: texture_2d<f32>;
@group(0) @binding(1)
var src_samp: sampler;
@group(0) @binding(2)
var bloom_tex: texture_2d<f32>;
@group(0) @binding(3)
var<uniform> post_u: Uniforms;

@vertex
fn vs_post(@builtin(vertex_index) idx: u32) -> SkyOut {
    var out: SkyOut;
    let x = f32((idx << 1u) & 2u);
    let y = f32(idx & 2u);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_bloom_h(in: SkyOut) -> @location(0) vec4<f32> {
    let texel_x = 1.0 / max(post_u.res_params.x * 0.5, 64.0);
    let offsets = array<f32, 5>(-2.0, -1.0, 0.0, 1.0, 2.0);
    let weights = array<f32, 5>(0.12, 0.22, 0.32, 0.22, 0.12);
    var acc = vec3<f32>(0.0);
    for (var i = 0; i < 5; i = i + 1) {
        let s = textureSample(src_tex, src_samp, in.uv + vec2<f32>(offsets[i] * texel_x * 1.6, 0.0)).rgb;
        // High-threshold bloom so crisp obsidian scales never wash out
        let bright = max(s - vec3<f32>(1.02), vec3<f32>(0.0));
        acc = acc + bright * weights[i];
    }
    return vec4<f32>(acc, 1.0);
}

@fragment
fn fs_bloom_v(in: SkyOut) -> @location(0) vec4<f32> {
    let texel_y = 1.0 / max(post_u.res_params.y * 0.5, 64.0);
    let offsets = array<f32, 5>(-2.0, -1.0, 0.0, 1.0, 2.0);
    let weights = array<f32, 5>(0.12, 0.22, 0.32, 0.22, 0.12);
    var acc = vec3<f32>(0.0);
    for (var i = 0; i < 5; i = i + 1) {
        let s = textureSample(src_tex, src_samp, in.uv + vec2<f32>(0.0, offsets[i] * texel_y * 1.6)).rgb;
        acc = acc + s * weights[i];
    }
    return vec4<f32>(acc, 1.0);
}

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_composite(in: SkyOut) -> @location(0) vec4<f32> {
    var hdr = textureSample(src_tex, src_samp, in.uv).rgb;
    let bloom = textureSample(bloom_tex, src_samp, in.uv).rgb;
    let pulse = clamp(post_u.heart_params.w, 0.0, 1.0);
    hdr = hdr + bloom * post_u.res_params.z;

    // Soft moonlit memorial glow beneath central Amethyst Heart
    let dm = vec2<f32>((in.uv.x - 0.5) * post_u.res_params.w * 0.65, in.uv.y - 0.92);
    hdr = hdr + vec3<f32>(0.42, 0.24, 0.68) * exp(-dot(dm, dm) * 28.0) * (0.09 + 0.03 * pulse);

    // Suspended safety edge warning when EMERGENCY SUSPEND is active
    let edge = max(abs(in.uv.x - 0.5) * 2.0, abs(in.uv.y - 0.5) * 2.0);
    if (post_u.post_params.z > 0.5) {
        hdr = hdr + vec3<f32>(0.60, 0.08, 0.14) * smoothstep(0.65, 1.0, edge) * 0.34;
    }

    let vig = 1.0 - smoothstep(0.72, 1.08, edge) * 0.48;
    hdr = hdr * vig * clamp(post_u.post_params.y, 0.0, 1.0);

    var col = aces_tonemap(hdr);
    let dither = (hash21(in.pos.xy) - 0.5) * (1.5 / 255.0);
    col = clamp(col + vec3<f32>(dither), vec3<f32>(0.0), vec3<f32>(1.0));
    if (post_u.post_params.x > 0.5) {
        col = pow(col, vec3<f32>(2.2));
    }
    return vec4<f32>(col, 1.0);
}
"#;

#[derive(Clone, Debug)]
pub struct Scene3DParams {
    pub time: f32,
    pub motion: bool,
    pub quality: u8, // 0=Low, 1=Medium, 2=High, 3=Ultra, 4=Cinematic
    pub heart_pulse: f32,
    pub offering_glow: f32,
    pub breath: f32,
    pub suspended_alert: bool,
    pub intro_fade: f32,
    pub camera: LeviathanCamera,
    pub skeleton: LeviathanSkeleton,
    pub head_colors: [Color32; 7],
    pub head_activity: [f32; 7],
    pub head_warning: [bool; 7],
    pub head_emphasis: [f32; 7],
    pub head_eye_open: [f32; 7],
    pub head_observation: [f32; 7],
    pub aspect: f32,
}

impl Scene3DParams {
    pub fn pack_uniforms(&self, hdr_w: u32, hdr_h: u32, is_srgb: bool) -> Vec<u8> {
        let mut out = vec![0u8; SCENE_UNIFORM_BYTES];
        let mut off = 0usize;
        let write_f32 = |buf: &mut [u8], pos: &mut usize, val: f32| {
            let v = if val.is_finite() { val } else { 0.0 };
            buf[*pos..*pos + 4].copy_from_slice(&v.to_le_bytes());
            *pos += 4;
        };
        let write_mat4 = |buf: &mut [u8], pos: &mut usize, m: &Mat4| {
            m.write_le_bytes(&mut buf[*pos..*pos + 64]);
            *pos += 64;
        };

        let aspect = if self.aspect.is_finite() {
            self.aspect.clamp(0.25, 8.0)
        } else {
            1.6
        };
        let view_proj = self.camera.view_proj(aspect);

        // Directional upper-left moonlit shadow camera covering entire Leviathan volume
        let light_eye = Vec3::new(-4.8, 6.8, 4.6);
        let light_target = Vec3::new(0.0, 0.2, 0.0);
        let light_view = Mat4::look_at_rh(light_eye, light_target, Vec3::Y);
        let light_proj = Mat4::ortho_rh(-6.4, 6.4, -4.6, 4.8, 1.0, 18.0);
        let light_vp = light_proj.mul_mat4(light_view);

        write_mat4(&mut out, &mut off, &view_proj);
        write_mat4(&mut out, &mut off, &light_vp);

        // cam_pos (16B)
        let t_anim = if self.motion && self.time.is_finite() {
            self.time
        } else {
            0.0
        };
        for f in [
            self.camera.eye.x,
            self.camera.eye.y,
            self.camera.eye.z,
            t_anim,
        ] {
            write_f32(&mut out, &mut off, f);
        }

        // heart_params (16B)
        let hw = self.skeleton.joint_pos[crate::leviathan_3d_math::BONE_HEART];
        for f in [hw.x, hw.y, hw.z, self.heart_pulse.clamp(0.0, 1.0)] {
            write_f32(&mut out, &mut off, f);
        }

        // env_params (16B)
        let q = self.quality.min(4) as f32;
        for f in [
            self.offering_glow.clamp(0.0, 1.0),
            self.breath.clamp(0.0, 1.0),
            q,
            if self.motion { 1.0 } else { 0.0 },
        ] {
            write_f32(&mut out, &mut off, f);
        }

        // post_params (16B)
        let shadow_on = if self.quality >= 2 { 1.0 } else { 0.0 };
        for f in [
            if is_srgb { 1.0 } else { 0.0 },
            self.intro_fade.clamp(0.0, 1.0),
            if self.suspended_alert { 1.0 } else { 0.0 },
            shadow_on,
        ] {
            write_f32(&mut out, &mut off, f);
        }

        // res_params (16B)
        let bloom_strength = match self.quality {
            0 => 0.0,
            1 => 0.28,
            2 => 0.36,
            3 => 0.42,
            _ => 0.46,
        };
        for f in [hdr_w as f32, hdr_h as f32, bloom_strength, aspect] {
            write_f32(&mut out, &mut off, f);
        }

        // head_col (7 x 16B)
        for i in 0..7 {
            let c = self.head_colors[i];
            for f in [
                c.r() as f32 / 255.0,
                c.g() as f32 / 255.0,
                c.b() as f32 / 255.0,
                self.head_emphasis[i].clamp(0.0, 1.0),
            ] {
                write_f32(&mut out, &mut off, f);
            }
        }

        // head_state (7 x 16B)
        for i in 0..7 {
            for f in [
                self.head_activity[i].clamp(0.0, 1.0),
                if self.head_warning[i] { 1.0 } else { 0.0 },
                self.head_eye_open[i].clamp(0.05, 1.0),
                self.head_observation[i].clamp(0.0, 1.0),
            ] {
                write_f32(&mut out, &mut off, f);
            }
        }

        // head_look (7 x 16B)
        for i in 0..7 {
            let l = self.skeleton.head_look[i];
            for f in [l[0].clamp(-1.0, 1.0), l[1].clamp(-1.0, 1.0), 0.0, 0.0] {
                write_f32(&mut out, &mut off, f);
            }
        }

        // bones (96 x 64B)
        for i in 0..BONE_COUNT {
            write_mat4(&mut out, &mut off, &self.skeleton.skin_mats[i]);
        }

        debug_assert_eq!(off, SCENE_UNIFORM_BYTES);
        out
    }
}

#[allow(dead_code)]
struct Targets3D {
    hdr_w: u32,
    hdr_h: u32,
    shadow_size: u32,
    hdr_tex: wgpu::Texture,
    hdr_view: wgpu::TextureView,
    depth_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    shadow_tex: wgpu::Texture,
    shadow_view: wgpu::TextureView,
    bloom_a_tex: wgpu::Texture,
    bloom_a_view: wgpu::TextureView,
    bloom_b_tex: wgpu::Texture,
    bloom_b_view: wgpu::TextureView,
    scene_bg: wgpu::BindGroup,
    bloom_h_bg: wgpu::BindGroup,
    bloom_v_bg: wgpu::BindGroup,
    composite_bg: wgpu::BindGroup,
}

struct GpuTimestampState {
    query_set: wgpu::QuerySet,
    resolve_buf: wgpu::Buffer,
    read_buf: wgpu::Buffer,
    timestamp_period_ns: f32,
    pending_read: bool,
    mapped_ready: Arc<AtomicBool>,
    last_pass_ms: Arc<AtomicU64>,
}

pub struct Leviathan3DResources {
    vbuf: wgpu::Buffer,
    opaque_ibuf: wgpu::Buffer,
    opaque_count: u32,
    shadow_ibuf: wgpu::Buffer,
    shadow_count: u32,
    transp_ibuf: wgpu::Buffer,
    transp_count: u32,
    ubuf: wgpu::Buffer,
    linear_samp: wgpu::Sampler,
    shadow_samp: wgpu::Sampler,
    #[allow(dead_code)]
    shadow_bgl: wgpu::BindGroupLayout,
    shadow_bg: wgpu::BindGroup,
    scene_bgl: wgpu::BindGroupLayout,
    post_bgl: wgpu::BindGroupLayout,
    shadow_pipe: wgpu::RenderPipeline,
    sky_pipe: wgpu::RenderPipeline,
    opaque_pipe: wgpu::RenderPipeline,
    transp_pipe: wgpu::RenderPipeline,
    bloom_h_pipe: wgpu::RenderPipeline,
    bloom_v_pipe: wgpu::RenderPipeline,
    composite_pipe: wgpu::RenderPipeline,
    targets: Option<Targets3D>,
    timestamps: Option<GpuTimestampState>,
    is_srgb: bool,
}

/// Resolution scaling capped cleanly to `<= 2560x1440` for bounded VRAM on RTX 4070 and weaker GPUs.
pub fn compute_hdr_extent(rect_px: [u32; 2], quality: u8) -> (u32, u32, u32) {
    let scale = match quality {
        0 => 0.65_f32,
        1 => 0.80_f32,
        2 => 1.00_f32,
        3 => 1.00_f32,
        _ => 1.15_f32,
    };
    let rw = ((rect_px[0].max(64) as f32) * scale).round() as u32;
    let rh = ((rect_px[1].max(64) as f32) * scale).round() as u32;
    let hdr_w = rw.clamp(64, 2560);
    let hdr_h = rh.clamp(64, 1440);
    let shadow_size = match quality {
        0 | 1 => 512,
        2 => 1024,
        _ => 1536,
    };
    (hdr_w, hdr_h, shadow_size)
}

fn create_targets(
    device: &wgpu::Device,
    res: &Leviathan3DResources,
    hdr_w: u32,
    hdr_h: u32,
    shadow_size: u32,
) -> Targets3D {
    let hdr_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("leviathan_hdr_color"),
        size: wgpu::Extent3d {
            width: hdr_w,
            height: hdr_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let hdr_view = hdr_tex.create_view(&Default::default());

    let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("leviathan_hdr_depth"),
        size: wgpu::Extent3d {
            width: hdr_w,
            height: hdr_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_tex.create_view(&Default::default());

    let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("leviathan_shadow_depth"),
        size: wgpu::Extent3d {
            width: shadow_size,
            height: shadow_size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_view = shadow_tex.create_view(&Default::default());

    let bw = (hdr_w / 2).max(32);
    let bh = (hdr_h / 2).max(32);
    let make_bloom = |label| {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: bw,
                height: bh,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        (tex, view)
    };
    let (bloom_a_tex, bloom_a_view) = make_bloom("leviathan_bloom_a");
    let (bloom_b_tex, bloom_b_view) = make_bloom("leviathan_bloom_b");

    let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("leviathan_scene_bg"),
        layout: &res.scene_bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: res.ubuf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&res.shadow_samp),
            },
        ],
    });
    let make_post_bg = |label, src: &wgpu::TextureView, blm: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &res.post_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&res.linear_samp),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(blm),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: res.ubuf.as_entire_binding(),
                },
            ],
        })
    };
    let bloom_h_bg = make_post_bg("leviathan_bloom_h_bg", &hdr_view, &hdr_view);
    let bloom_v_bg = make_post_bg("leviathan_bloom_v_bg", &bloom_a_view, &bloom_a_view);
    let composite_bg = make_post_bg("leviathan_composite_bg", &hdr_view, &bloom_b_view);

    Targets3D {
        hdr_w,
        hdr_h,
        shadow_size,
        hdr_tex,
        hdr_view,
        depth_tex,
        depth_view,
        shadow_tex,
        shadow_view,
        bloom_a_tex,
        bloom_a_view,
        bloom_b_tex,
        bloom_b_view,
        scene_bg,
        bloom_h_bg,
        bloom_v_bg,
        composite_bg,
    }
}

pub struct GpuTimingHandle(Arc<AtomicU64>);

impl GpuTimingHandle {
    pub fn latest_ms(&self) -> Option<f64> {
        let bits = self.0.load(Ordering::Relaxed);
        if bits == 0 {
            None
        } else {
            let v = f64::from_bits(bits);
            (v.is_finite() && v > 0.0).then_some(v)
        }
    }
}

/// Initialize the procedural 3D PBR + Shadow + Bloom WGPU pipeline in egui_wgpu.
pub fn register_wgpu_3d_renderer(cc: &eframe::CreationContext<'_>) -> Option<GpuTimingHandle> {
    let render_state = cc.wgpu_render_state.as_ref()?;
    let device = &render_state.device;
    let queue = &render_state.queue;
    let target_format = render_state.target_format;
    let is_srgb = target_format.is_srgb();

    let mesh = build_leviathan_mesh();
    let vbytes = mesh.vertex_bytes();
    let obytes = crate::leviathan_mesh::LeviathanMeshData::u32_bytes(&mesh.opaque_indices);
    let sbytes = crate::leviathan_mesh::LeviathanMeshData::u32_bytes(&mesh.shadow_indices);
    let tbytes = crate::leviathan_mesh::LeviathanMeshData::u32_bytes(&mesh.transparent_indices);

    let create_init_buf = |label, data: &[u8], usage: wgpu::BufferUsages| {
        let size = (data.len() as u64).max(4);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !data.is_empty() {
            queue.write_buffer(&buf, 0, data);
        }
        buf
    };

    let vbuf = create_init_buf("leviathan_vbuf", &vbytes, wgpu::BufferUsages::VERTEX);
    let opaque_ibuf = create_init_buf("leviathan_o_ibuf", &obytes, wgpu::BufferUsages::INDEX);
    let shadow_ibuf = create_init_buf("leviathan_s_ibuf", &sbytes, wgpu::BufferUsages::INDEX);
    let transp_ibuf = create_init_buf("leviathan_t_ibuf", &tbytes, wgpu::BufferUsages::INDEX);

    let ubuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("leviathan_3d_uniforms"),
        size: SCENE_UNIFORM_BYTES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let linear_samp = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("leviathan_linear_samp"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let shadow_samp = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("leviathan_shadow_cmp_samp"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });

    let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("leviathan_shadow_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(SCENE_UNIFORM_BYTES as u64),
            },
            count: None,
        }],
    });
    let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("leviathan_shadow_bg"),
        layout: &shadow_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: ubuf.as_entire_binding(),
        }],
    });

    let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("leviathan_scene_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(SCENE_UNIFORM_BYTES as u64),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    });

    let post_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("leviathan_post_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(SCENE_UNIFORM_BYTES as u64),
                },
                count: None,
            },
        ],
    });

    let shadow_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("leviathan_shadow_shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADOW_WGSL)),
    });
    let scene_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("leviathan_scene_shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SCENE_WGSL)),
    });

    let vbuf_layout = [wgpu::VertexBufferLayout {
        array_stride: GPU_VERTEX_STRIDE as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 32,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32x4,
                offset: 48,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 64,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32x4,
                offset: 80,
                shader_location: 6,
            },
        ],
    }];

    let shadow_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("leviathan_shadow_pl"),
        bind_group_layouts: &[Some(&shadow_bgl)],
        immediate_size: 0,
    });
    let shadow_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("leviathan_shadow_pipe"),
        layout: Some(&shadow_pl),
        vertex: wgpu::VertexState {
            module: &shadow_mod,
            entry_point: Some("vs_shadow"),
            buffers: &vbuf_layout,
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 1.5,
                clamp: 0.0,
            },
        }),
        multisample: Default::default(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    });

    let scene_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("leviathan_scene_pl"),
        bind_group_layouts: &[Some(&scene_bgl)],
        immediate_size: 0,
    });

    let sky_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("leviathan_sky_pipe"),
        layout: Some(&scene_pl),
        vertex: wgpu::VertexState {
            module: &scene_mod,
            entry_point: Some("vs_sky"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &scene_mod,
            entry_point: Some("fs_sky"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });

    let opaque_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("leviathan_opaque_pipe"),
        layout: Some(&scene_pl),
        vertex: wgpu::VertexState {
            module: &scene_mod,
            entry_point: Some("vs_mesh"),
            buffers: &vbuf_layout,
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &scene_mod,
            entry_point: Some("fs_opaque"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });

    let transp_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("leviathan_transp_pipe"),
        layout: Some(&scene_pl),
        vertex: wgpu::VertexState {
            module: &scene_mod,
            entry_point: Some("vs_mesh"),
            buffers: &vbuf_layout,
            compilation_options: Default::default(),
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &scene_mod,
            entry_point: Some("fs_transparent"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    });

    let post_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("leviathan_post_pl"),
        bind_group_layouts: &[Some(&post_bgl)],
        immediate_size: 0,
    });
    let make_post_pipe = |label, fs_entry, format| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&post_pl),
            vertex: wgpu::VertexState {
                module: &scene_mod,
                entry_point: Some("vs_post"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene_mod,
                entry_point: Some(fs_entry),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    };

    let bloom_h_pipe = make_post_pipe(
        "leviathan_bloom_h_pipe",
        "fs_bloom_h",
        wgpu::TextureFormat::Rgba16Float,
    );
    let bloom_v_pipe = make_post_pipe(
        "leviathan_bloom_v_pipe",
        "fs_bloom_v",
        wgpu::TextureFormat::Rgba16Float,
    );
    let composite_pipe = make_post_pipe("leviathan_composite_pipe", "fs_composite", target_format);

    let last_pass_ms = Arc::new(AtomicU64::new(0));
    let timestamps = if device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("leviathan_gpu_timers"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("leviathan_ts_resolve"),
            size: 16,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("leviathan_ts_read"),
            size: 16,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Some(GpuTimestampState {
            query_set,
            resolve_buf,
            read_buf,
            timestamp_period_ns: queue.get_timestamp_period(),
            pending_read: false,
            mapped_ready: Arc::new(AtomicBool::new(false)),
            last_pass_ms: last_pass_ms.clone(),
        })
    } else {
        None
    };

    render_state
        .renderer
        .write()
        .callback_resources
        .insert(Leviathan3DResources {
            vbuf,
            opaque_ibuf,
            opaque_count: mesh.opaque_indices.len() as u32,
            shadow_ibuf,
            shadow_count: mesh.shadow_indices.len() as u32,
            transp_ibuf,
            transp_count: mesh.transparent_indices.len() as u32,
            ubuf,
            linear_samp,
            shadow_samp,
            shadow_bgl,
            shadow_bg,
            scene_bgl,
            post_bgl,
            shadow_pipe,
            sky_pipe,
            opaque_pipe,
            transp_pipe,
            bloom_h_pipe,
            bloom_v_pipe,
            composite_pipe,
            targets: None,
            timestamps,
            is_srgb,
        });

    Some(GpuTimingHandle(last_pass_ms))
}

struct Scene3DCallback {
    rect: Rect,
    params: Scene3DParams,
}

impl egui_wgpu::CallbackTrait for Scene3DCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(res) = callback_resources.get_mut::<Leviathan3DResources>() else {
            return Vec::new();
        };

        // Read back previous frame's GPU timestamp delta if mapped and ready
        if let Some(ts) = res.timestamps.as_mut() {
            if ts.pending_read {
                let _ = device.poll(wgpu::PollType::Poll);
                if ts.mapped_ready.load(Ordering::Acquire) {
                    let slice = ts.read_buf.slice(..);
                    let data = slice.get_mapped_range();
                    if data.len() == 16 {
                        let t0 = u64::from_le_bytes(data[0..8].try_into().unwrap_or([0; 8]));
                        let t1 = u64::from_le_bytes(data[8..16].try_into().unwrap_or([0; 8]));
                        if t1 > t0 {
                            let ms =
                                ((t1 - t0) as f64 * ts.timestamp_period_ns as f64) / 1_000_000.0;
                            if ms.is_finite() && ms < 1000.0 {
                                ts.last_pass_ms.store(ms.to_bits(), Ordering::Relaxed);
                            }
                        }
                    }
                    drop(data);
                    ts.read_buf.unmap();
                    ts.mapped_ready.store(false, Ordering::Release);
                    ts.pending_read = false;
                }
            }
        }

        let ppp = screen_descriptor.pixels_per_point.max(0.5);
        let rect_px = [
            ((self.rect.width() * ppp).round() as u32).max(64),
            ((self.rect.height() * ppp).round() as u32).max(64),
        ];
        let (hdr_w, hdr_h, shadow_size) = compute_hdr_extent(rect_px, self.params.quality);
        let need_rebuild = res
            .targets
            .as_ref()
            .is_none_or(|t| t.hdr_w != hdr_w || t.hdr_h != hdr_h || t.shadow_size != shadow_size);
        if need_rebuild {
            let new_targets = create_targets(device, res, hdr_w, hdr_h, shadow_size);
            res.targets = Some(new_targets);
        }

        let uniform_bytes = self.params.pack_uniforms(hdr_w, hdr_h, res.is_srgb);
        queue.write_buffer(&res.ubuf, 0, &uniform_bytes);

        let Some(targets) = res.targets.as_ref() else {
            return Vec::new();
        };

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("leviathan_3d_encoder"),
        });

        let use_ts = res.timestamps.as_ref().is_some_and(|t| !t.pending_read);
        let run_shadow = self.params.quality >= 2;
        let run_bloom = self.params.quality >= 1;

        // Pass 1: Directional Shadow Map
        if run_shadow {
            let shadow_ts = res.timestamps.as_ref().filter(|_| use_ts).map(|ts| {
                wgpu::RenderPassTimestampWrites {
                    query_set: &ts.query_set,
                    beginning_of_pass_write_index: Some(0),
                    end_of_pass_write_index: None,
                }
            });
            let mut sp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("leviathan_shadow_pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: shadow_ts,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            sp.set_pipeline(&res.shadow_pipe);
            sp.set_bind_group(0, &res.shadow_bg, &[]);
            sp.set_vertex_buffer(0, res.vbuf.slice(..));
            sp.set_index_buffer(res.shadow_ibuf.slice(..), wgpu::IndexFormat::Uint32);
            sp.draw_indexed(0..res.shadow_count, 0, 0..1);
        }

        // Pass 2: HDR 3D Skybox + Skinned Opaque Dragon + Skinned Transparent Pass
        {
            let scene_ts = res.timestamps.as_ref().filter(|_| use_ts).and_then(|ts| {
                let begin = if !run_shadow { Some(0) } else { None };
                let end = if !run_bloom { Some(1) } else { None };
                (begin.is_some() || end.is_some()).then_some(wgpu::RenderPassTimestampWrites {
                    query_set: &ts.query_set,
                    beginning_of_pass_write_index: begin,
                    end_of_pass_write_index: end,
                })
            });
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("leviathan_hdr_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.hdr_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.008,
                            g: 0.006,
                            b: 0.018,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: scene_ts,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            rp.set_bind_group(0, &targets.scene_bg, &[]);
            rp.set_pipeline(&res.sky_pipe);
            rp.draw(0..3, 0..1);

            rp.set_pipeline(&res.opaque_pipe);
            rp.set_vertex_buffer(0, res.vbuf.slice(..));
            rp.set_index_buffer(res.opaque_ibuf.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..res.opaque_count, 0, 0..1);

            rp.set_pipeline(&res.transp_pipe);
            rp.set_vertex_buffer(0, res.vbuf.slice(..));
            rp.set_index_buffer(res.transp_ibuf.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..res.transp_count, 0, 0..1);
        }

        // Pass 3: Controlled HDR Bloom (Medium..Cinematic)
        if run_bloom {
            {
                let mut bh = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("leviathan_bloom_h_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.bloom_a_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                bh.set_pipeline(&res.bloom_h_pipe);
                bh.set_bind_group(0, &targets.bloom_h_bg, &[]);
                bh.draw(0..3, 0..1);
            }
            {
                let bloom_end_ts = res.timestamps.as_ref().filter(|_| use_ts).map(|ts| {
                    wgpu::RenderPassTimestampWrites {
                        query_set: &ts.query_set,
                        beginning_of_pass_write_index: None,
                        end_of_pass_write_index: Some(1),
                    }
                });
                let mut bv = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("leviathan_bloom_v_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.bloom_b_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: bloom_end_ts,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                bv.set_pipeline(&res.bloom_v_pipe);
                bv.set_bind_group(0, &targets.bloom_v_bg, &[]);
                bv.draw(0..3, 0..1);
            }
        }

        if let Some(ts) = res.timestamps.as_mut().filter(|_| use_ts) {
            enc.resolve_query_set(&ts.query_set, 0..2, &ts.resolve_buf, 0);
            enc.copy_buffer_to_buffer(&ts.resolve_buf, 0, &ts.read_buf, 0, 16);
            queue.submit([enc.finish()]);
            let flag = ts.mapped_ready.clone();
            ts.mapped_ready.store(false, Ordering::Release);
            ts.read_buf
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |res| {
                    if res.is_ok() {
                        flag.store(true, Ordering::Release);
                    }
                });
            ts.pending_read = true;
            return Vec::new();
        }

        vec![enc.finish()]
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(res) = callback_resources.get::<Leviathan3DResources>() {
            if let Some(targets) = res.targets.as_ref() {
                render_pass.set_pipeline(&res.composite_pipe);
                render_pass.set_bind_group(0, &targets.composite_bg, &[]);
                render_pass.draw(0..3, 0..1);
            }
        }
    }
}

/// Emit a 3D WGPU PBR + shadow + bloom paint callback strictly inside `rect`.
pub fn paint_wgpu_3d_scene(painter: &egui::Painter, rect: Rect, params: Scene3DParams) {
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let callback = egui_wgpu::Callback::new_paint_callback(rect, Scene3DCallback { rect, params });
    painter.add(callback);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leviathan_3d_math::SkeletonParams;

    #[test]
    fn uniform_buffer_size_is_aligned_and_sanitizes_nan() {
        assert_eq!(SCENE_UNIFORM_BYTES, 6688);
        assert_eq!(SCENE_UNIFORM_BYTES % 16, 0);

        let skel = LeviathanSkeleton::evaluate(&SkeletonParams {
            time: f32::NAN,
            motion: false,
            breath: f32::INFINITY,
            heart_pulse: f32::NEG_INFINITY,
            pointer_world: None,
            head_activity: [0.0; 7],
            head_warning: [false; 7],
            head_emphasis: [0.0; 7],
            head_eye_open: [1.0; 7],
            selected: None,
            awareness: None,
        });
        let params = Scene3DParams {
            time: f32::NAN,
            motion: true,
            quality: 99,
            heart_pulse: f32::NAN,
            offering_glow: f32::INFINITY,
            breath: f32::NEG_INFINITY,
            suspended_alert: false,
            intro_fade: f32::NAN,
            camera: LeviathanCamera::overview(f32::NAN),
            skeleton: skel,
            head_colors: [Color32::LIGHT_BLUE; 7],
            head_activity: [f32::NAN; 7],
            head_warning: [false; 7],
            head_emphasis: [0.0; 7],
            head_eye_open: [f32::NAN; 7],
            head_observation: [f32::INFINITY; 7],
            aspect: f32::NAN,
        };
        let bytes = params.pack_uniforms(1600, 900, false);
        assert_eq!(bytes.len(), SCENE_UNIFORM_BYTES);
        for chunk in bytes.chunks_exact(4) {
            let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            assert!(v.is_finite(), "non-finite packed uniform float: {v}");
        }
    }

    #[test]
    fn hdr_resolution_is_bounded_and_scales_cleanly_with_presets() {
        let low = compute_hdr_extent([2560, 1440], 0);
        let high = compute_hdr_extent([2560, 1440], 2);
        let cine = compute_hdr_extent([3840, 2160], 4);
        assert!(low.0 < high.0 && low.1 < high.1);
        assert!(cine.0 <= 2560 && cine.1 <= 1440);
        assert!(high.2 >= 1024);
    }

    #[test]
    fn wgsl_3d_shaders_contain_all_pipeline_entry_points() {
        assert!(SHADOW_WGSL.contains("fn vs_shadow"));
        assert!(SCENE_WGSL.contains("fn vs_sky"));
        assert!(SCENE_WGSL.contains("fn fs_sky"));
        assert!(SCENE_WGSL.contains("fn vs_mesh"));
        assert!(SCENE_WGSL.contains("fn fs_opaque"));
        assert!(SCENE_WGSL.contains("fn fs_transparent"));
        assert!(SCENE_WGSL.contains("fn vs_post"));
        assert!(SCENE_WGSL.contains("fn fs_bloom_h"));
        assert!(SCENE_WGSL.contains("fn fs_bloom_v"));
        assert!(SCENE_WGSL.contains("fn fs_composite"));
        assert!(SCENE_WGSL.contains("fn pbr_ggx"));
        assert!(SCENE_WGSL.contains("fn sample_shadow"));
        assert!(SCENE_WGSL.contains("fn dragon_scales"));
    }
}
