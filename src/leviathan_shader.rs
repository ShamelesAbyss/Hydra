//! GPU-accelerated 2.5D PBR WGSL renderer for the Reborn Seven-Headed Leviathan.
//!
//! Renders the entire living seven-headed obsidian-and-amethyst dragon sanctuary
//! directly on the GPU with:
//! - Anatomically sculpted smooth-blended (`smin`) dragon skulls, brow ridges,
//!   snouts, jaws, ivory-obsidian fangs, and seven distinct curved horn phenotypes.
//! - Imbricated overlapping obsidian dragon scales along every articulated neck
//!   with analytical 3D cylindrical normals, GGX/Blinn-Phong PBR specular highlights,
//!   crevice ambient occlusion, and deep amethyst bioluminescence glowing beneath
//!   selected scale seams.
//! - Independent per-head cursor/attention eye tracking, organic blinking eyelids,
//!   breathing wing unfurl, and dynamic point-light illumination cast upward from
//!   the beating central Amethyst Heart onto all seven necks and jaws.
//! - Adaptive LOD tiers (`Low`, `Medium`, `High`, `Ultra`) and ACES filmic
//!   tonemapping so RTX 4070-class GPUs run smoothly at high refresh rates while
//!   weaker hardware remains responsive.
//!
//! Operational truthfulness is strictly preserved: idle breathing, blinking, and
//! subtle heartbeat are decorative organism life; active energy conduits, jaw/eye
//! flares, and warning barriers strictly reflect real observed subsystem state.

use eframe::egui::{self, Color32, Pos2, Rect};
use eframe::egui_wgpu::{self, wgpu};
use std::borrow::Cow;
use std::num::NonZeroU64;

// 4 global vec4s + 4 arrays of 7 vec4s = 32 vec4s = 128 f32 = 512 bytes (16-byte aligned).
const UNIFORM_FLOATS: usize = (4 + 7 * 4) * 4;
const UNIFORM_BYTES: usize = UNIFORM_FLOATS * 4;

const SHADER_WGSL: &str = r#"
struct Uniforms {
    params: vec4<f32>,             // x=time, y=motion(0|1), z=quality(0..3), w=is_srgb(0|1)
    heart: vec4<f32>,              // xy=heart_uv, z=heart_pulse(0..1), w=offering_glow(0..1)
    canvas: vec4<f32>,             // x=aspect, y=active_total(0..1), z=breath(0..1), w=suspended_alert(0|1)
    selection: vec4<f32>,          // x=selected(-1..6), y=hovered(-1..6), z=intro_fade(0..1), w=creature_scale
    head_pos: array<vec4<f32>, 7>, // xy=head_uv, z=activity(0..1), w=warning(0|1)
    head_col: array<vec4<f32>, 7>, // rgb=accent(0..1), w=emphasis(0..1)
    head_anim: array<vec4<f32>, 7>,// x=look_x(-1..1), y=look_y(-1..1), z=eye_open(0..1), w=observation(0..1)
    neck_ctrl: array<vec4<f32>, 7>,// xy=ctrl1_uv, zw=ctrl2_uv
};

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VsOut {
    var out: VsOut;
    let x = f32((idx << 1u) & 2u);
    let y = f32(idx & 2u);
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

// ============================================================================
// Math, Noise, SDF Primitives & Filmic Tonemapping
// ============================================================================

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

fn fbm(p: vec2<f32>, octaves: i32) -> f32 {
    var v = 0.0;
    var amp = 0.52;
    var q = p;
    for (var i = 0; i < 4; i = i + 1) {
        if (i >= octaves) {
            break;
        }
        v = v + amp * value_noise(q);
        q = q * 2.06 + vec2<f32>(13.7, 7.3);
        amp = amp * 0.48;
    }
    return v;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / max(k, 0.0001), 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn sd_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    // Returns vec2(distance, t along segment [0..1])
    let pa = p - a;
    let ba = b - a;
    let denom = max(dot(ba, ba), 1e-6);
    let h = clamp(dot(pa, ba) / denom, 0.0, 1.0);
    return vec2<f32>(length(pa - ba * h), h);
}

fn sd_capsule_taper(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r1: f32, r2: f32) -> vec3<f32> {
    // Returns vec3(signed_dist, t, signed_lateral_norm)
    let pa = p - a;
    let ba = b - a;
    let len_ba = max(length(ba), 1e-5);
    let dir = ba / len_ba;
    let perp = vec2<f32>(-dir.y, dir.x);
    let h = clamp(dot(pa, dir) / len_ba, 0.0, 1.0);
    let closest = a + ba * h;
    let diff = p - closest;
    let dist = length(diff);
    let r = mix(r1, r2, h);
    let lat = clamp(dot(diff, perp) / max(r, 1e-5), -1.0, 1.0);
    return vec3<f32>(dist - r, h, lat);
}

fn sd_ellipse(p: vec2<f32>, r: vec2<f32>) -> f32 {
    let q = p / max(r, vec2<f32>(1e-4));
    let k0 = length(q);
    let k1 = length(q / max(r, vec2<f32>(1e-4)));
    return k0 * (k0 - 1.0) / max(k1, 1e-4);
}

fn rot2(p: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

fn cubic_bezier(p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32>, t: f32) -> vec2<f32> {
    let mt = 1.0 - t;
    return mt * mt * mt * p0
        + 3.0 * mt * mt * t * p1
        + 3.0 * mt * t * t * p2
        + t * t * t * p3;
}

fn uv_to_world(uv: vec2<f32>, aspect: f32) -> vec2<f32> {
    return vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5);
}

// ============================================================================
// Procedural True-Imbricated Overlapping Obsidian Dragon Scales
// Evaluates 2 staggered overlapping rows (zero checkerboard gaps!)
// Returns vec4(height, grad_u, grad_v, selective_bioluminescence_seam)
// ============================================================================

fn dragon_scales(su: f32, sv: f32) -> vec4<f32> {
    // Evaluate row A (front overlapping row) and row B (shifted row underneath)
    // so every single texel sits on a real curved overlapping dragon scale!
    let row_a = floor(sv);
    let stag_a = (row_a - floor(row_a * 0.5) * 2.0) * 0.5;
    let gu_a = fract(su + stag_a) - 0.5;
    let gv_a = fract(sv) - 0.5;

    let sv_b = sv - 0.5;
    let row_b = floor(sv_b);
    let stag_b = (row_b - floor(row_b * 0.5) * 2.0) * 0.5;
    let gu_b = fract(su + stag_b) - 0.5;
    let gv_b = fract(sv_b) - 0.5;

    // Front-most scale (smaller row index = higher up the neck/body overlapping downward)
    let taper_a = 1.0 + max(gv_a, 0.0) * 0.80;
    let sx_a = gu_a * 1.90 * taper_a;
    let sy_a = (gv_a - 0.02) * 1.35;
    let d_a = length(vec2<f32>(sx_a, sy_a)) - 0.94;
    let mask_a = 1.0 - smoothstep(-0.08, 0.06, d_a);

    let taper_b = 1.0 + max(gv_b, 0.0) * 0.80;
    let sx_b = gu_b * 1.90 * taper_b;
    let sy_b = (gv_b - 0.02) * 1.35;
    let d_b = length(vec2<f32>(sx_b, sy_b)) - 0.94;

    // Choose front overlapping scale smoothly (no black checkerboard holes!)
    let use_a = select(mask_a, 1.0, gv_a < 0.02 && mask_a > 0.5);
    let sx = mix(sx_b, sx_a, use_a);
    let sy = mix(sy_b, sy_a, use_a);
    let gu = mix(gu_b, gu_a, use_a);
    let gv = mix(gv_b, gv_a, use_a);
    let d_front = mix(d_b, d_a, use_a);

    let r2 = clamp(sx * sx + sy * sy, 0.0, 1.0);
    let dome = sqrt(max(0.0, 1.0 - r2));
    let keel = exp(-abs(gu) * 11.0) * smoothstep(-0.45, 0.40, gv) * 0.24;
    // Cast lip shadow from scale A onto scale B directly below its edge
    let lip_shadow = mix(smoothstep(0.0, 0.22, max(d_a, 0.0)), 1.0, use_a);
    let height = clamp((0.30 + dome * 0.62 + keel) * (0.45 + 0.55 * lip_shadow), 0.12, 1.0);

    let gx = -sx * 1.15 - sign(gu) * exp(-abs(gu) * 11.0) * 0.45;
    let gy = -sy * 1.05 + smoothstep(0.25, 0.58, gv) * 1.1;

    // Razor-thin recessed seam strictly at scale overlap border, gated by organic
    // low-frequency vein noise so amethyst fire glows ONLY beneath SELECTED scales!
    let thin_seam = exp(-abs(d_front + 0.02) * 28.0) * (1.0 - smoothstep(0.28, 0.55, dome));
    let vein_select = smoothstep(0.56, 0.86, value_noise(vec2<f32>(su * 0.28, sv * 0.22)));
    return vec4<f32>(height, gx, gy, clamp(thin_seam * vein_select, 0.0, 1.0));
}

// ============================================================================
// Physically Based Volcanic Obsidian + Amethyst Subsurface Lighting Model
// ============================================================================

fn pbr_obsidian(
    world_p: vec2<f32>,
    normal: vec3<f32>,
    ao: f32,
    roughness: f32,
    base_tint: vec3<f32>,
    heart_w: vec2<f32>,
    heart_pulse: f32,
    offering_glow: f32,
    accent: vec3<f32>,
    sub_glow: f32,
) -> vec3<f32> {
    let n = normalize(normal);
    let view = vec3<f32>(0.0, 0.0, 1.0);

    // Key light: crisp moonlit silver-blue from upper-left sanctuary vault
    let l_key = normalize(vec3<f32>(-0.52, -0.66, 0.54));
    let key_col = vec3<f32>(0.78, 0.84, 1.0);

    // Secondary rim light: deep amethyst-violet silhouette rim from upper-right
    let l_rim = normalize(vec3<f32>(0.68, -0.40, 0.34));
    let rim_col = vec3<f32>(0.50, 0.26, 0.90);

    // Dynamic Point Light: Central Amethyst Heart casting rich royal violet upward
    let to_heart = vec3<f32>(heart_w - world_p, 0.22);
    let h_dist = max(length(to_heart), 0.05);
    let l_heart = to_heart / h_dist;
    let h_atten = 1.0 / (1.0 + 7.5 * h_dist * h_dist);
    let heart_col = mix(
        vec3<f32>(0.58, 0.16, 0.98),
        vec3<f32>(0.90, 0.56, 1.0),
        heart_pulse * 0.42 + offering_glow * 0.48
    ) * (1.15 + 0.50 * heart_pulse + 0.65 * offering_glow);

    let ndl_key = max(dot(n, l_key), 0.0);
    let ndl_rim = max(dot(n, l_rim), 0.0);
    let ndl_heart = max(dot(n, l_heart), 0.0);
    let ndv = max(dot(n, view), 0.001);

    // Fresnel (Schlick, dark volcanic glass F0 ~ 0.065)
    let f0 = vec3<f32>(0.065, 0.060, 0.088);
    let fresnel = f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - ndv, 4.4);

    // Razor-crisp glossy specular highlights (volcanic obsidian sheen)
    let shininess = mix(128.0, 18.0, clamp(roughness, 0.05, 0.95));
    let h_key = normalize(l_key + view);
    let h_rim = normalize(l_rim + view);
    let h_heart = normalize(l_heart + view);
    let spec_key = pow(max(dot(n, h_key), 0.0), shininess) * 0.92;
    let spec_rim = pow(max(dot(n, h_rim), 0.0), shininess * 0.65) * 0.52;
    let spec_heart = pow(max(dot(n, h_heart), 0.0), shininess * 0.55) * 0.95;

    // True dark volcanic obsidian: deep charcoal-indigo diffuse so speculars & glow POP
    let ambient = base_tint * vec3<f32>(0.018, 0.016, 0.036) * ao;
    let diffuse = base_tint * (
        key_col * ndl_key * 0.095
        + rim_col * ndl_rim * 0.070
        + heart_col * ndl_heart * h_atten * 0.20
    ) * ao;

    let specular = (
        key_col * spec_key
        + rim_col * spec_rim
        + heart_col * spec_heart * h_atten
    ) * (0.25 + 0.75 * ao);

    // Sculptural grazing rim lighting carving ancient silhouettes out of the void
    let silhouette_rim = fresnel * mix(vec3<f32>(0.34, 0.22, 0.66), accent, 0.34) * (0.35 + 0.65 * ndl_key);

    // Deep amethyst bioluminescence glowing strictly from beneath selected scale seams
    let bio_col = mix(vec3<f32>(0.64, 0.18, 1.0), accent, 0.36);
    let bioluminescence = bio_col * sub_glow;

    return ambient + diffuse + specular + silhouette_rim + bioluminescence;
}

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// ============================================================================
// Layer 1: Massive Sweeping Gothic Membranous Dragon Wings
// ============================================================================

fn render_wings(
    p: vec2<f32>,
    heart_w: vec2<f32>,
    sc: f32,
    breath: f32,
    pulse: f32,
    offering: f32,
    quality: f32,
    bg: vec3<f32>,
) -> vec3<f32> {
    var col = bg;
    let lift = (breath - 0.45) * 0.026 * sc;

    for (var s_idx = 0; s_idx < 2; s_idx = s_idx + 1) {
        let side = select(-1.0, 1.0, s_idx == 1);
        let root = heart_w + vec2<f32>(side * 0.06 * sc, -0.02 * sc);
        let elbow = heart_w + vec2<f32>(side * 0.42 * sc, -0.35 * sc - lift);
        let claw = elbow + vec2<f32>(side * 0.03 * sc, -0.065 * sc);
        let tip0 = heart_w + vec2<f32>(side * 0.86 * sc, -0.26 * sc - lift * 1.45);
        let tip1 = heart_w + vec2<f32>(side * 0.78 * sc, -0.03 * sc - lift * 0.90);
        let tip2 = heart_w + vec2<f32>(side * 0.60 * sc, 0.09 * sc - lift * 0.40);
        let tip3 = heart_w + vec2<f32>(side * 0.38 * sc, 0.12 * sc - lift * 0.15);
        let hip = heart_w + vec2<f32>(side * 0.13 * sc, 0.08 * sc);

        let w_min_x = min(root.x, tip0.x) - 0.06 * sc;
        let w_max_x = max(root.x, tip0.x) + 0.06 * sc;
        if (p.x < w_min_x || p.x > w_max_x || p.y < claw.y - 0.06 * sc || p.y > tip3.y + 0.08 * sc) {
            continue;
        }

        // Normalized span u in [0..1] along the soaring wing arch
        let span_x = clamp((p.x - root.x) * side / (0.80 * sc), 0.0, 1.0);
        // Smooth organic arched leading edge passing cleanly through root -> elbow -> tip0
        let upper_y = mix(root.y, elbow.y, smoothstep(0.0, 0.45, span_x))
            + (tip0.y - elbow.y) * smoothstep(0.42, 1.0, span_x)
            - sin(span_x * 3.14159) * 0.055 * sc;
        // Scalloped gothic dragon trailing edge anchored cleanly to the finger tips
        let arch_t = fract(span_x * 3.0);
        let gothic_arch = sin(arch_t * 3.14159) * 0.044 * sc * smoothstep(0.04, 0.96, span_x);
        let env_low = mix(hip.y + 0.02 * sc, tip0.y + 0.015 * sc, pow(span_x, 1.48))
            + sin(span_x * 3.14159) * 0.085 * sc;
        let lower_y = env_low - gothic_arch;

        let in_span = smoothstep(0.0, 0.035, span_x) * (1.0 - smoothstep(0.96, 1.0, span_x));
        let in_mem = smoothstep(upper_y - 0.004 * sc, upper_y + 0.010 * sc, p.y)
            * (1.0 - smoothstep(lower_y - 0.010 * sc, lower_y + 0.005 * sc, p.y))
            * in_span;

        if (in_mem > 0.001) {
            // Backlit dark leathery obsidian membrane with translucent amethyst radiance
            let d_heart = length(p - heart_w);
            let sss = exp(-d_heart * 1.65 / max(sc, 0.25)) * (0.48 + 0.30 * pulse + 0.35 * offering);
            let edge_rim = exp(-abs(p.y - lower_y) * 44.0 / max(sc, 0.25)) * 0.38;
            let upper_rim = exp(-abs(p.y - upper_y) * 48.0 / max(sc, 0.25)) * 0.28;
            var vein = 0.0;
            if (quality >= 1.0) {
                let vn = value_noise(p * 22.0 / max(sc, 0.25) + vec2<f32>(side * 2.1, 0.0));
                vein = pow(smoothstep(0.54, 0.94, vn), 2.0) * 0.34;
            }
            let mem_col = vec3<f32>(0.015, 0.011, 0.034)
                + vec3<f32>(0.25, 0.07, 0.50) * (sss * 0.65 + vein * (0.28 + sss))
                + vec3<f32>(0.32, 0.16, 0.64) * (edge_rim + upper_rim) * (0.48 + 0.32 * pulse);
            col = mix(col, mem_col, in_mem * 0.90);
        }

        // Articulated obsidian wing bones cleanly terminating at the membrane arches
        let spar_t1 = vec2<f32>(root.x + side * 0.75 * sc, mix(hip.y, tip0.y, 0.72) + 0.055 * sc);
        let spar_t2 = vec2<f32>(root.x + side * 0.56 * sc, mix(hip.y, tip0.y, 0.42) + 0.075 * sc);
        let spar_t3 = vec2<f32>(root.x + side * 0.34 * sc, mix(hip.y, tip0.y, 0.18) + 0.068 * sc);

        let b0 = sd_capsule_taper(p, root, elbow, 0.0145 * sc, 0.0105 * sc);
        let bc = sd_capsule_taper(p, elbow, claw, 0.0085 * sc, 0.0018 * sc);
        let b1 = sd_capsule_taper(p, elbow, tip0, 0.010 * sc, 0.0022 * sc);
        let b2 = sd_capsule_taper(p, elbow, spar_t1, 0.0078 * sc, 0.0020 * sc);
        let b3 = sd_capsule_taper(p, elbow, spar_t2, 0.0068 * sc, 0.0018 * sc);
        let b4 = sd_capsule_taper(p, elbow, spar_t3, 0.0058 * sc, 0.0016 * sc);

        let d_bone = min(min(min(b0.x, bc.x), min(b1.x, b2.x)), min(b3.x, b4.x));
        let bone_mask = 1.0 - smoothstep(-0.0018 * sc, 0.0026 * sc, d_bone);
        if (bone_mask > 0.001) {
            let b_lat = select(b0.z, b1.z, b1.x < b0.x);
            let n_bone = normalize(vec3<f32>(side * 0.35, b_lat * 0.72 - 0.18, sqrt(max(0.06, 1.0 - b_lat * b_lat))));
            let bone_col = pbr_obsidian(
                p,
                n_bone,
                0.85,
                0.24,
                vec3<f32>(0.76, 0.74, 0.92),
                heart_w,
                pulse,
                offering,
                vec3<f32>(0.68, 0.34, 0.98),
                0.02,
            );
            col = mix(col, bone_col, bone_mask);
        }
    }
    return col;
}

// ============================================================================
// Layer 2: Muscular Obsidian Torso, Thoracic Rib-Cradle & Flanking Tail
// ============================================================================

fn render_torso_and_tail(
    p: vec2<f32>,
    heart_w: vec2<f32>,
    sc: f32,
    t_anim: f32,
    breath: f32,
    pulse: f32,
    offering: f32,
    quality: f32,
    bg: vec3<f32>,
) -> vec3<f32> {
    var col = bg;
    let chest_expand = 1.0 + (breath - 0.45) * 0.042;

    // Coiled armored tail tucked gracefully along lower-right flank —
    // completely clear of the Heart caption and Ashlan's memorial below!
    let tail_sway = sin(t_anim * 0.85) * 0.015 * sc;
    let t0 = heart_w + vec2<f32>(0.13 * sc, 0.035 * sc);
    let t1 = heart_w + vec2<f32>(0.34 * sc, 0.075 * sc + tail_sway * 0.5);
    let t2 = heart_w + vec2<f32>(0.53 * sc, 0.025 * sc - tail_sway);
    let t3 = heart_w + vec2<f32>(0.66 * sc, -0.065 * sc + tail_sway * 0.8);
    let ts0 = sd_capsule_taper(p, t0, t1, 0.024 * sc, 0.016 * sc);
    let ts1 = sd_capsule_taper(p, t1, t2, 0.016 * sc, 0.010 * sc);
    let ts2 = sd_capsule_taper(p, t2, t3, 0.010 * sc, 0.002 * sc);
    let d_tail = smin(smin(ts0.x, ts1.x, 0.014 * sc), ts2.x, 0.010 * sc);
    let tail_mask = 1.0 - smoothstep(-0.0018 * sc, 0.0026 * sc, d_tail);
    if (tail_mask > 0.001) {
        let sc_pat = dragon_scales(p.x * 38.0 / max(sc, 0.25), p.y * 38.0 / max(sc, 0.25));
        let n_tail = normalize(vec3<f32>(sc_pat.y * 0.30, sc_pat.z * 0.30 - 0.22, 0.92));
        let tail_col = pbr_obsidian(
            p,
            n_tail,
            0.48 + 0.52 * sc_pat.x,
            0.24,
            vec3<f32>(0.72, 0.70, 0.88),
            heart_w,
            pulse,
            offering,
            vec3<f32>(0.62, 0.28, 0.96),
            sc_pat.w * 0.35,
        );
        col = mix(col, tail_col, tail_mask);
    }

    // Muscular armored obsidian breastplate & shoulder pauldrons cradling the Heart
    let q = p - (heart_w + vec2<f32>(0.0, 0.006 * sc));
    if (abs(q.x) < 0.28 * sc && abs(q.y) < 0.19 * sc) {
        let d_chest = sd_ellipse(q, vec2<f32>(0.162 * sc * chest_expand, 0.095 * sc * chest_expand));
        let d_sh_l = sd_ellipse(q - vec2<f32>(-0.135 * sc, -0.025 * sc), vec2<f32>(0.076 * sc, 0.056 * sc));
        let d_sh_r = sd_ellipse(q - vec2<f32>(0.135 * sc, -0.025 * sc), vec2<f32>(0.076 * sc, 0.056 * sc));
        let d_body = smin(d_chest, min(d_sh_l, d_sh_r), 0.045 * sc);

        // Deep dramatic drop shadow behind torso
        let shadow = exp(-max(d_body, 0.0) * 26.0 / max(sc, 0.25)) * 0.62;
        col = col * (1.0 - shadow);

        let body_mask = 1.0 - smoothstep(-0.002 * sc, 0.0028 * sc, d_body);
        if (body_mask > 0.001) {
            let nx = clamp(q.x / (0.18 * sc), -0.92, 0.92);
            let ny = clamp(q.y / (0.11 * sc), -0.92, 0.92);
            let nz = sqrt(max(0.08, 1.0 - nx * nx * 0.65 - ny * ny * 0.65));
            var n_body = vec3<f32>(nx * 0.74, ny * 0.74, nz);
            var ao = 0.72;
            var sub_g = 0.04;
            if (quality >= 1.0) {
                let sp = dragon_scales(q.x * 34.0 / max(sc, 0.25), q.y * 36.0 / max(sc, 0.25));
                n_body = normalize(n_body + vec3<f32>(sp.y, sp.z, 0.0) * 0.28);
                ao = 0.44 + 0.56 * sp.x;
                sub_g = sp.w * (0.28 + 0.35 * pulse + 0.45 * offering);
            }
            // Sculpted obsidian thoracic rib plates arching around the Heart socket
            let rib_wave = sin(abs(q.y) * 56.0 / max(sc, 0.25) - abs(q.x) * 18.0 / max(sc, 0.25));
            let rib_zone = smoothstep(0.055 * sc, 0.13 * sc, length(q)) * (1.0 - smoothstep(0.14 * sc, 0.21 * sc, length(q)));
            n_body = normalize(n_body + vec3<f32>(0.0, rib_wave * rib_zone * 0.38, 0.0));

            let body_col = pbr_obsidian(
                p,
                n_body,
                ao,
                0.20,
                vec3<f32>(0.76, 0.74, 0.92),
                heart_w,
                pulse,
                offering,
                vec3<f32>(0.68, 0.26, 1.0),
                sub_g,
            );
            col = mix(col, body_col, body_mask);
        }
    }
    return col;
}

// ============================================================================
// Layer 3: Articulated Muscular Dragon Neck with True Overlapping Obsidian Scales
// ============================================================================

fn render_neck(
    p: vec2<f32>,
    idx: i32,
    heart_w: vec2<f32>,
    sc: f32,
    aspect: f32,
    t_anim: f32,
    pulse: f32,
    offering: f32,
    quality: f32,
    bg: vec3<f32>,
) -> vec3<f32> {
    let hp = u.head_pos[idx];
    let hc = u.head_col[idx];
    let ha = u.head_anim[idx];
    let nc = u.neck_ctrl[idx];

    let head_w = uv_to_world(hp.xy, aspect);
    let spread = (f32(idx) - 3.0) / 3.0;
    let base_w = heart_w + vec2<f32>(spread * 0.072 * sc, -0.025 * sc);
    let c1_w = uv_to_world(nc.xy, aspect);
    let c2_w = uv_to_world(nc.zw, aspect);

    let pad = 0.075 * sc;
    let min_x = min(min(base_w.x, head_w.x), min(c1_w.x, c2_w.x)) - pad;
    let max_x = max(max(base_w.x, head_w.x), max(c1_w.x, c2_w.x)) + pad;
    let min_y = min(min(base_w.y, head_w.y), min(c1_w.y, c2_w.y)) - pad;
    let max_y = max(max(base_w.y, head_w.y), max(c1_w.y, c2_w.y)) + pad;
    if (p.x < min_x || p.x > max_x || p.y < min_y || p.y > max_y) {
        return bg;
    }

    let thickness_scale = select(1.0, 1.20, idx == 3);
    let r_base = 0.036 * sc * thickness_scale;
    let r_tip = 0.018 * sc * thickness_scale;

    var best_d = 1e6;
    var best_t = 0.0;
    var best_lat = 0.0;
    var best_dir = vec2<f32>(0.0, -1.0);

    var prev = base_w;
    let segs = 7;
    for (var s = 1; s <= segs; s = s + 1) {
        let t0 = f32(s - 1) / f32(segs);
        let t1 = f32(s) / f32(segs);
        let curr = cubic_bezier(base_w, c1_w, c2_w, head_w, t1);
        let r0 = mix(r_base, r_tip, t0) * (1.0 + 0.07 * sin(t0 * 6.28318));
        let r1 = mix(r_base, r_tip, t1) * (1.0 + 0.07 * sin(t1 * 6.28318));
        let res = sd_capsule_taper(p, prev, curr, r0, r1);
        if (res.x < best_d) {
            best_d = res.x;
            best_t = mix(t0, t1, res.y);
            best_lat = res.z;
            let seg_v = curr - prev;
            best_dir = seg_v / max(length(seg_v), 1e-5);
        }
        prev = curr;
    }

    var col = bg;

    // Deep atmospheric cast shadow behind neck onto wings & deeper necks
    let shadow_d = max(best_d, 0.0);
    let shadow = exp(-shadow_d * 34.0 / max(sc, 0.25)) * 0.62;
    col = col * (1.0 - shadow);

    let activity = clamp(hp.z, 0.0, 1.0);
    let warning = clamp(hp.w, 0.0, 1.0);
    let obs = clamp(ha.w, 0.0, 1.0);
    let warn_col = vec3<f32>(0.96, 0.64, 0.20);
    let accent = mix(hc.rgb, warn_col, warning * 0.72);

    // Swept dorsal ridge crest spines along outer silhouette of curved neck
    let spine_side = select(-1.0, 1.0, idx >= 3);
    let spine_phase = fract(best_t * 14.0);
    let spine_tooth = pow(1.0 - smoothstep(0.0, 0.72, spine_phase), 1.4) * smoothstep(0.06, 0.90, best_t);
    let spine_reach = 0.013 * sc * spine_tooth * select(0.0, 1.0, best_lat * spine_side > 0.72);
    let d_with_spines = best_d - spine_reach;

    let neck_mask = 1.0 - smoothstep(-0.0018 * sc, 0.0024 * sc, d_with_spines);
    if (neck_mask <= 0.001) {
        return col;
    }

    let perp = vec2<f32>(-best_dir.y, best_dir.x);
    let lat_c = clamp(best_lat, -0.98, 0.98);
    let nz = sqrt(max(0.04, 1.0 - lat_c * lat_c));
    var n_cyl = vec3<f32>(perp * lat_c, nz);

    // True imbricated obsidian dragon scales wrapped around the cylindrical neck
    let theta = asin(lat_c) * 1.95;
    let scale_rows = mix(16.0, 24.0, clamp(quality / 2.0, 0.0, 1.0));
    let sc_pat = dragon_scales(theta * 1.35 + f32(idx) * 2.3, best_t * scale_rows);

    // Armored dark ventral throat scute plates down center belly (|lat| < 0.34)
    let throat_mask = 1.0 - smoothstep(0.22, 0.40, abs(lat_c));
    let plate_y = fract(best_t * 18.0) - 0.5;
    let plate_lip = smoothstep(-0.48, -0.14, plate_y) * (1.0 - smoothstep(0.32, 0.50, plate_y));
    let plate_grad_y = -plate_y * 1.15 + smoothstep(0.28, 0.48, plate_y) * 1.4;

    let detail_strength = select(0.18, 0.36, quality >= 1.0);
    let local_gx = mix(sc_pat.y, 0.0, throat_mask * 0.78);
    let local_gy = mix(sc_pat.z, plate_grad_y, throat_mask * 0.78);
    let tangent_perturb = perp * local_gx + best_dir * local_gy;
    n_cyl = normalize(n_cyl + vec3<f32>(tangent_perturb * detail_strength, 0.0));

    let ao = mix(0.36 + 0.64 * sc_pat.x, 0.44 + 0.56 * plate_lip, throat_mask * 0.72)
        * smoothstep(-0.02, 0.14, best_t);

    // Selective amethyst bioluminescence glowing out from BENEATH selected scales
    // (subtle volcanic pulse at rest; bright flowing conduit wave ONLY on real activity/obs/Offering)
    let heart_wave = exp(-best_t * 3.0) * (0.20 + 0.32 * pulse);
    let active_travel = exp(-pow((fract(best_t - t_anim * 0.75) - 0.5) * 6.5, 2.0)) * activity * 1.55;
    let obs_pip = exp(-pow((fract(best_t - t_anim * 0.45) - 0.5) * 8.0, 2.0)) * obs * 0.85;
    let offering_conduit = select(0.0, offering * 0.95 * exp(-best_t * 1.1), idx == 3 || idx == 4);
    let vein_glow = sc_pat.w * (1.0 - throat_mask * 0.65)
        * (0.32 + heart_wave + active_travel + warning * 0.85 + offering_conduit);
    let throat_seam = throat_mask * exp(-abs(plate_y - 0.38) * 26.0)
        * (0.06 * pulse + active_travel * 0.55 + obs_pip * 0.45 + offering_conduit * 0.50);
    let sub_glow = vein_glow + throat_seam;

    // Atmospheric depth separation so central apex AEGIS commands the foreground
    let depth_tint = mix(vec3<f32>(0.82, 0.80, 0.96), vec3<f32>(0.58, 0.56, 0.74), abs(spread) * 0.48);
    let neck_col = pbr_obsidian(
        p,
        n_cyl,
        ao,
        mix(0.18, 0.28, throat_mask),
        depth_tint,
        heart_w,
        pulse,
        offering,
        accent,
        sub_glow,
    );

    return mix(col, neck_col, neck_mask);
}

// ============================================================================
// Layer 4: Fierce Anatomically Sculpted Ancient Dragon Head (7 Phenotypes)
// ============================================================================

fn render_dragon_head(
    p: vec2<f32>,
    idx: i32,
    heart_w: vec2<f32>,
    sc: f32,
    aspect: f32,
    t_anim: f32,
    pulse: f32,
    offering: f32,
    quality: f32,
    bg: vec3<f32>,
) -> vec3<f32> {
    let hp = u.head_pos[idx];
    let hc = u.head_col[idx];
    let ha = u.head_anim[idx];

    let head_w = uv_to_world(hp.xy, aspect);
    let head_scale = sc * select(0.072, 0.086, idx == 3);

    let rel_raw = p - head_w;
    if (dot(rel_raw, rel_raw) > (head_scale * 2.75) * (head_scale * 2.75)) {
        return bg;
    }

    let look = ha.xy;
    let eye_open = clamp(ha.z, 0.06, 1.0);
    let activity = clamp(hp.z, 0.0, 1.0);
    let warning = clamp(hp.w, 0.0, 1.0);
    let emphasis = clamp(hc.w, 0.0, 1.0);

    let warn_col = vec3<f32>(0.98, 0.65, 0.22);
    let accent = mix(hc.rgb, warn_col, warning * 0.72);

    // Regal head cant + subtle living gaze tilt
    let spread = (f32(idx) - 3.0) / 3.0;
    let tilt = spread * 0.18 + look.x * 0.13;
    let q = rot2(rel_raw / max(head_scale, 1e-4), -tilt);
    let sym = vec2<f32>(abs(q.x), q.y);

    var col = bg;

    // ------------------------------------------------------------------------
    // 4A. Seven Distinct Ancient Curved Keratin Dragon Horn Crowns
    // ------------------------------------------------------------------------
    var h0 = vec2<f32>(0.36, -0.44);
    var h1 = vec2<f32>(0.84, -1.04);
    var h2 = vec2<f32>(1.24, -1.64);
    var r_h0 = 0.17;
    var r_h2 = 0.022;

    if (idx == 0) {
        // HUNTER: sleek swept raptor / hydro-blade horns
        h0 = vec2<f32>(0.34, -0.42);
        h1 = vec2<f32>(0.98, -0.82);
        h2 = vec2<f32>(1.58, -1.26);
        r_h0 = 0.14;
    } else if (idx == 1) {
        // FANG: heavy muscular recurved predator war-horns
        h0 = vec2<f32>(0.42, -0.40);
        h1 = vec2<f32>(1.08, -0.68);
        h2 = vec2<f32>(1.14, -1.56);
        r_h0 = 0.21;
    } else if (idx == 2) {
        // COGNITION: tall aristocratic curved neural crown horns
        h0 = vec2<f32>(0.32, -0.52);
        h1 = vec2<f32>(0.70, -1.14);
        h2 = vec2<f32>(0.88, -1.80);
        r_h0 = 0.15;
    } else if (idx == 3) {
        // AEGIS: monumental sovereign elder-dragon crown horns
        h0 = vec2<f32>(0.46, -0.46);
        h1 = vec2<f32>(1.06, -0.96);
        h2 = vec2<f32>(1.48, -1.70);
        r_h0 = 0.22;
    } else if (idx == 4) {
        // NEXUS: sleek forward-swept obsidian lance horns
        h0 = vec2<f32>(0.38, -0.46);
        h1 = vec2<f32>(0.88, -1.06);
        h2 = vec2<f32>(1.14, -1.72);
        r_h0 = 0.16;
    } else if (idx == 5) {
        // ORACLE: sweeping inward-curving lunar crescent horns
        h0 = vec2<f32>(0.44, -0.44);
        h1 = vec2<f32>(1.08, -0.98);
        h2 = vec2<f32>(0.74, -1.74);
        r_h0 = 0.17;
    } else {
        // GENESIS: wide-branching organic elder antler-horns
        h0 = vec2<f32>(0.38, -0.46);
        h1 = vec2<f32>(0.98, -0.94);
        h2 = vec2<f32>(1.44, -1.60);
        r_h0 = 0.16;
    }

    let hs0 = sd_capsule_taper(sym, h0, h1, r_h0, r_h0 * 0.56);
    let hs1 = sd_capsule_taper(sym, h1, h2, r_h0 * 0.56, r_h2);
    var d_horn = smin(hs0.x, hs1.x, 0.12);
    var horn_t = select(0.5 + 0.5 * hs1.y, 0.5 * hs0.y, hs0.x < hs1.x);
    var horn_lat = select(hs1.z, hs0.z, hs0.x < hs1.x);

    // Swept jagged dragon cheek spines & lower jaw spurs (unmistakable ancient dragon silhouette!)
    let cheek_h1 = sd_capsule_taper(sym, vec2<f32>(0.58, -0.04), vec2<f32>(1.24, -0.34), 0.12, 0.018);
    if (cheek_h1.x < d_horn) {
        d_horn = smin(d_horn, cheek_h1.x, 0.08);
        horn_t = cheek_h1.y;
        horn_lat = cheek_h1.z;
    }
    let cheek_h2 = sd_capsule_taper(sym, vec2<f32>(0.48, 0.24), vec2<f32>(0.96, 0.16), 0.09, 0.015);
    if (cheek_h2.x < d_horn) {
        d_horn = smin(d_horn, cheek_h2.x, 0.07);
        horn_t = cheek_h2.y;
        horn_lat = cheek_h2.z;
    }
    if (idx == 3 || idx == 6 || idx == 2) {
        let tine_end = select(vec2<f32>(0.54, -1.60), vec2<f32>(0.40, -1.72), idx == 6);
        let tine = sd_capsule_taper(sym, vec2<f32>(0.24, -0.60), tine_end, 0.105, 0.018);
        if (tine.x < d_horn) {
            d_horn = smin(d_horn, tine.x, 0.08);
            horn_t = tine.y;
            horn_lat = tine.z;
        }
    }

    // ------------------------------------------------------------------------
    // 4B. Fierce Angular Sculpted Elder-Dragon Skull (Elongated Snout & Heavy Brow)
    // ------------------------------------------------------------------------
    let jaw_open = 0.05 + activity * 0.18 + warning * 0.14;
    let d_cranium = sd_ellipse(q - vec2<f32>(0.0, -0.30), vec2<f32>(0.58, 0.48));
    let d_cheek = sd_capsule_taper(sym, vec2<f32>(0.34, -0.22), vec2<f32>(0.68, 0.18), 0.20, 0.12).x;
    let d_brow = sd_capsule_taper(sym, vec2<f32>(0.10, -0.18), vec2<f32>(0.64, -0.32), 0.14, 0.095).x;
    let d_snout = sd_capsule_taper(q, vec2<f32>(0.0, -0.14), vec2<f32>(0.0, 1.06), 0.36, 0.17).x;
    let d_muzzle = sd_ellipse(q - vec2<f32>(0.0, 1.02), vec2<f32>(0.25, 0.19));
    let d_chin = sd_capsule_taper(q, vec2<f32>(0.0, 0.62), vec2<f32>(0.0, 1.12 + jaw_open * 0.34), 0.27, 0.14).x;

    var d_skull = smin(d_cranium, d_cheek, 0.20);
    d_skull = smin(d_skull, d_brow, 0.12);
    d_skull = smin(d_skull, d_snout, 0.18);
    d_skull = smin(d_skull, d_muzzle, 0.14);
    d_skull = smin(d_skull, d_chin, 0.12);

    // Subtle AEGIS sovereign hexagonal shield aura when active/selected/alerting
    if (idx == 3 && (activity > 0.05 || warning > 0.5 || emphasis > 0.25)) {
        let hex_r = length(q);
        let hex_a = atan2(q.y, q.x);
        let hex_d = hex_r * cos(hex_a - floor((hex_a + 0.5236) / 1.0472) * 1.0472);
        let ring = exp(-pow((hex_d - 1.62) * 18.0, 2.0));
        let shield_str = (0.18 + emphasis * 0.26 + activity * 0.36 + warning * 0.38);
        col = col + accent * ring * shield_str;
    }

    // Subtle ORACLE lunar astrolabe ring behind skull
    if (idx == 5 && quality >= 1.0) {
        let orb = exp(-pow((length(q - vec2<f32>(0.0, -0.25)) - 1.38) * 26.0, 2.0));
        col = col + accent * orb * (0.10 + 0.18 * emphasis + 0.26 * activity);
    }

    // Deep dramatic shadow behind dragon head & horns
    let d_all = min(d_skull, d_horn);
    let head_shadow = exp(-max(d_all, 0.0) * 5.0) * 0.66;
    col = col * (1.0 - head_shadow);

    // Render sculpted keratin horns with transverse growth rings & bone-tip sheen
    let horn_mask = 1.0 - smoothstep(-0.02, 0.032, d_horn);
    if (horn_mask > 0.001) {
        let ring_ridge = sin(horn_t * 38.0) * 0.34;
        let nz_h = sqrt(max(0.06, 1.0 - horn_lat * horn_lat));
        let n_horn = normalize(vec3<f32>(sign(q.x) * horn_lat * 0.80, -0.34 + ring_ridge, nz_h));
        let horn_tint = mix(vec3<f32>(0.55, 0.52, 0.68), vec3<f32>(0.96, 0.92, 0.88), pow(horn_t, 1.6) * 0.72);
        let horn_col = pbr_obsidian(
            p,
            n_horn,
            0.52 + 0.48 * horn_t,
            0.18,
            horn_tint,
            heart_w,
            pulse,
            offering,
            accent,
            (1.0 - horn_t) * 0.10 * (0.4 + activity + emphasis),
        );
        col = mix(col, horn_col, horn_mask);
    }

    let skull_mask = 1.0 - smoothstep(-0.02, 0.03, d_skull);
    if (skull_mask <= 0.001) {
        return col;
    }

    // Chiseled 3D dragon bone-plate normals: prominent nasal crest, swept brow armor, sculpted cheekbones
    let nasal_ridge = exp(-abs(q.x) * 8.5) * smoothstep(-0.35, 1.0, q.y) * 0.56;
    let brow_plate = exp(-pow(d_brow * 6.5, 2.0)) * 0.58;
    let cheek_crest = exp(-pow(d_cheek * 6.0, 2.0)) * 0.35;
    let nx_s = clamp(q.x * 1.15 - sign(q.x) * nasal_ridge * 0.65 + sign(q.x) * cheek_crest * 0.25, -0.94, 0.94);
    let ny_s = clamp((q.y - 0.16) * 0.65 - brow_plate * 0.62, -0.92, 0.92);
    let nz_s = sqrt(max(0.08, 1.0 - nx_s * nx_s * 0.62 - ny_s * ny_s * 0.52));
    var n_skull = normalize(vec3<f32>(nx_s, ny_s, nz_s));

    // Fine symmetrical obsidian cranial micro-scales (smooth sculpted volcanic armor!)
    let cr_scales = dragon_scales(abs(q.x) * 8.2, q.y * 9.0);
    if (quality >= 1.0) {
        n_skull = normalize(n_skull + vec3<f32>(sign(q.x) * cr_scales.y, cr_scales.z, 0.0) * 0.12);
    }
    var ao = clamp(0.54 + 0.46 * cr_scales.x, 0.32, 1.0) * smoothstep(0.02, -0.24, d_skull);

    // Deep recessed predator orbital sockets underneath overhanging brow plates
    let eye_center = vec2<f32>(0.32, 0.02);
    let eye_socket_p = rot2(sym - eye_center, 0.42);
    let d_socket = sd_ellipse(eye_socket_p, vec2<f32>(0.22, 0.115));
    let socket_dark = 1.0 - smoothstep(-0.04, 0.10, d_socket);
    ao = ao * (1.0 - socket_dark * 0.80);

    // Flared dragon nostrils on the tapering snout tip
    let d_nostril = sd_ellipse(rot2(sym - vec2<f32>(0.115, 0.96), -0.35), vec2<f32>(0.036, 0.068));
    let nostril_mask = 1.0 - smoothstep(-0.010, 0.020, d_nostril);
    ao = ao * (1.0 - nostril_mask * 0.88);

    // Selective cranial crown bioluminescence + subtle nasal crest vein
    let crown_vein = cr_scales.w * smoothstep(0.65, -0.45, q.y)
        * (0.34 + 0.28 * pulse + 0.70 * activity + 0.38 * emphasis + 0.58 * warning);
    let brow_crest_glow = exp(-abs(q.x) * 26.0) * smoothstep(0.40, -0.40, q.y) * smoothstep(-0.62, -0.25, q.y)
        * (0.16 + 0.20 * pulse + 0.48 * activity + 0.32 * emphasis);

    var skull_col = pbr_obsidian(
        p,
        n_skull,
        ao,
        0.30,
        vec3<f32>(0.80, 0.78, 0.96),
        heart_w,
        pulse,
        offering,
        accent,
        crown_vein + brow_crest_glow,
    );

    // Subtle internal glowing mist inside nostrils when active
    skull_col = skull_col + accent * nostril_mask * (0.10 + 0.62 * activity + 0.50 * warning);

    // ------------------------------------------------------------------------
    // 4C. Fierce Dragon Maw & Gleaming Curved Ivory Fangs
    // ------------------------------------------------------------------------
    let mouth_y = 1.04 + jaw_open * 0.12;
    let d_mouth = sd_ellipse(q - vec2<f32>(0.0, mouth_y), vec2<f32>(0.22, 0.048 + jaw_open * 0.24));
    let mouth_mask = 1.0 - smoothstep(-0.012, 0.022, d_mouth);
    if (mouth_mask > 0.001) {
        let throat_fire = mix(vec3<f32>(0.025, 0.010, 0.055), accent * 1.25, 0.14 + activity * 0.78 + warning * 0.68);
        skull_col = mix(skull_col, throat_fire, mouth_mask * 0.94);
    }

    // Razor-sharp curved ivory-obsidian fangs (extra fierce on FANG)
    let fang_len = select(0.18, 0.27, idx == 1);
    let d_fang1 = sd_capsule_taper(sym, vec2<f32>(0.155, 0.98), vec2<f32>(0.135, 0.98 + fang_len), 0.032, 0.004).x;
    let d_fang2 = sd_capsule_taper(sym, vec2<f32>(0.225, 0.90), vec2<f32>(0.210, 0.90 + fang_len * 0.68), 0.024, 0.003).x;
    let d_fang = min(d_fang1, d_fang2);
    let fang_mask = 1.0 - smoothstep(-0.007, 0.014, d_fang);
    if (fang_mask > 0.001) {
        let ivory = vec3<f32>(0.96, 0.94, 0.90) * (0.58 + 0.42 * smoothstep(0.95, 1.18, sym.y));
        skull_col = mix(skull_col, ivory, fang_mask);
    }

    // ------------------------------------------------------------------------
    // 4D. Fierce Slanted Living Reptilian Eyes with Independent Tracking
    // ------------------------------------------------------------------------
    let eye_rot = rot2(sym - eye_center, 0.42);
    let eye_rx = 0.165;
    let eye_ry = 0.068 * eye_open;
    let d_eye = sd_ellipse(eye_rot, vec2<f32>(eye_rx, max(eye_ry, 0.007)));
    let eye_mask = 1.0 - smoothstep(-0.010, 0.015, d_eye);
    if (eye_mask > 0.001) {
        let signed_eye_local = q - vec2<f32>(sign(q.x) * eye_center.x, eye_center.y);
        let pupil_shift = vec2<f32>(look.x * 0.044, look.y * 0.018);
        let ep = signed_eye_local - pupil_shift;

        let r_iris = length(ep / vec2<f32>(0.140, 0.075));
        let ang_iris = atan2(ep.y, ep.x);
        let fibers = 0.82 + 0.18 * sin(ang_iris * 16.0 + r_iris * 9.0);
        let limbal = 1.0 - smoothstep(0.65, 1.02, r_iris);
        let iris_core = mix(accent * 1.55, vec3<f32>(1.0, 0.98, 0.95), exp(-r_iris * 2.4) * 0.58);
        var eye_col = iris_core * fibers * limbal * (1.10 + 0.55 * emphasis + 0.85 * activity + 0.65 * warning);

        // Razor-sharp vertical reptilian slit pupil
        let slit_w = 0.015 + 0.011 * emphasis;
        let d_slit = sd_ellipse(ep, vec2<f32>(slit_w, 0.068));
        let slit_mask = 1.0 - smoothstep(-0.005, 0.009, d_slit);
        eye_col = mix(eye_col, vec3<f32>(0.010, 0.008, 0.020), slit_mask);

        // Wet corneal pinpoint highlight
        let glint = exp(-dot(ep - vec2<f32>(-0.034, -0.018), ep - vec2<f32>(-0.034, -0.018)) * 2400.0);
        eye_col = eye_col + vec3<f32>(1.0, 0.98, 1.0) * glint * 1.75;

        // Menacing overhanging brow eyelid shadow
        let lid_shadow = smoothstep(-eye_ry * 0.95, -eye_ry * 0.08, eye_rot.y);
        eye_col = eye_col * (0.42 + 0.58 * lid_shadow);

        skull_col = mix(skull_col, eye_col, eye_mask);
    }

    let eye_spill = exp(-max(d_eye, 0.0) * 15.0) * eye_open * (0.20 + 0.28 * activity + 0.22 * emphasis);
    skull_col = skull_col + accent * eye_spill;

    return mix(col, skull_col, skull_mask);
}

// ============================================================================
// Layer 5: Central Deep-Royal Faceted Amethyst Heart in Obsidian Setting
// ============================================================================

fn render_amethyst_heart(
    p: vec2<f32>,
    heart_w: vec2<f32>,
    sc: f32,
    t_anim: f32,
    pulse: f32,
    offering: f32,
    bg: vec3<f32>,
) -> vec3<f32> {
    let q = (p - heart_w) / max(sc * 0.062, 1e-4);
    let r = length(q);
    if (r > 2.9) {
        return bg;
    }

    var col = bg;
    let beat_scale = 1.0 + (pulse - 0.32) * 0.080;
    let qs = q / beat_scale;

    let a = atan2(qs.y, qs.x);
    let hex_r = length(qs) * cos(a - floor((a + 0.5236) / 1.0472) * 1.0472);
    let d_gem = hex_r - 0.80;

    // Deep royal-violet volumetric radiance halo (rich amethyst, not washed-out white!)
    let halo_col = mix(
        vec3<f32>(0.46, 0.12, 0.92),
        vec3<f32>(0.84, 0.44, 1.0),
        pulse * 0.45 + offering * 0.50
    );
    col = col + halo_col * exp(-r * r * 2.1) * (0.28 + 0.22 * pulse + 0.34 * offering);

    // Dark obsidian bezel rim cradling the gem
    let bezel_mask = (1.0 - smoothstep(-0.02, 0.04, hex_r - 0.94)) * smoothstep(-0.04, 0.02, d_gem);
    if (bezel_mask > 0.001) {
        col = mix(col, vec3<f32>(0.035, 0.028, 0.068), bezel_mask * 0.92);
    }

    let gem_mask = 1.0 - smoothstep(-0.025, 0.035, d_gem);
    if (gem_mask > 0.001) {
        let sector = floor((a + 3.14159265) / 1.0471975);
        let facet_ang = (sector + 0.5) * 1.0471975 - 3.14159265;
        let table_mask = 1.0 - smoothstep(0.36, 0.44, hex_r);
        let n_facet = normalize(mix(
            vec3<f32>(cos(facet_ang) * 0.68, sin(facet_ang) * 0.68, 0.73),
            vec3<f32>(0.0, -0.10, 0.995),
            table_mask
        ));

        let l_key = normalize(vec3<f32>(-0.48, -0.66, 0.58));
        let ndl = max(dot(n_facet, l_key), 0.0);
        let spec = pow(max(dot(n_facet, normalize(l_key + vec3<f32>(0.0, 0.0, 1.0))), 0.0), 64.0);

        // Distinct dark-to-luminous royal amethyst crystal facets
        let caustic = 0.5 + 0.5 * sin(a * 6.0 + t_anim * 1.15 + hex_r * 10.0);
        let core_star = exp(-dot(qs, qs) * 14.0) * (0.85 + 0.55 * pulse);
        let facet_shade = 0.28 + 0.72 * ndl;
        let gem_base = mix(
            vec3<f32>(0.16, 0.03, 0.36),
            vec3<f32>(0.64, 0.22, 0.98),
            0.22 + 0.44 * facet_shade + 0.20 * table_mask + 0.14 * caustic
        );
        var gem_col = gem_base * (0.78 + 0.42 * pulse + 0.45 * offering)
            + vec3<f32>(0.96, 0.80, 1.0) * (core_star * 0.85 + spec * 1.45);

        // Crisp faceted crystal edge highlights
        let radial_rib = exp(-abs(sin((a + 0.5236) * 3.0)) * 22.0) * (1.0 - table_mask) * 0.32;
        let facet_edge = exp(-abs(hex_r - 0.40) * 34.0) + exp(-abs(d_gem) * 30.0) + radial_rib;
        gem_col = gem_col + vec3<f32>(0.88, 0.66, 1.0) * facet_edge * 0.45;

        if (offering > 0.05) {
            gem_col = mix(gem_col, vec3<f32>(1.0, 0.82, 0.48) * 1.25, offering * 0.28 * facet_edge);
        }

        col = mix(col, gem_col, gem_mask);
    }

    return col;
}

// ============================================================================
// Main Fragment Shader — Unified Living Sanctuary Pipeline
// ============================================================================

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let aspect = max(u.canvas.x, 0.25);
    let t_anim = u.params.x * u.params.y;
    let quality = u.params.z; // 0=Low, 1=Medium, 2=High, 3=Ultra
    let pulse = clamp(u.heart.z, 0.0, 1.0);
    let offering = clamp(u.heart.w, 0.0, 1.0);
    let breath = clamp(u.canvas.z, 0.0, 1.0);
    let sc = max(u.selection.w, 0.25);

    let p = uv_to_world(uv, aspect);
    let heart_w = uv_to_world(u.heart.xy, aspect);
    let dh = p - heart_w;
    let r_heart = length(dh);

    // ------------------------------------------------------------------------
    // Layer 0: Abyssal Obsidian Sanctuary, Volumetric Lighting & Drifting Motes
    // ------------------------------------------------------------------------
    let center_d = length(vec2<f32>(p.x * 0.92, (p.y - 0.02) * 1.12));
    var col = mix(
        vec3<f32>(0.028, 0.024, 0.058),
        vec3<f32>(0.010, 0.013, 0.028),
        clamp(center_d * 1.05, 0.0, 1.0)
    );

    if (quality >= 1.0) {
        let oct = select(2, 3, quality >= 2.0);
        let drift = vec2<f32>(t_anim * 0.012, -t_anim * 0.018);
        let n1 = fbm(p * 2.4 + drift, oct);
        let n2 = fbm(p * 4.4 - drift * 1.4 + vec2<f32>(3.1, 1.7), oct);
        let mist = smoothstep(0.28, 0.88, n1 * 0.62 + n2 * 0.45) * exp(-center_d * center_d * 0.95);
        let mist_col = mix(vec3<f32>(0.22, 0.09, 0.40), vec3<f32>(0.04, 0.14, 0.24), smoothstep(0.2, 0.8, n2));
        col = col + mist_col * mist * (0.22 + 0.06 * quality);

        // Subtle volumetric god-rays behind the Leviathan
        let ang = atan2(dh.y, dh.x);
        let ray = pow(max(0.0, cos(ang * 7.0 + 1.5708)), 10.0) * exp(-r_heart * 2.1);
        col = col + vec3<f32>(0.36, 0.18, 0.68) * ray * (0.08 + 0.045 * pulse);
    }

    // Subtle atmospheric halos behind active or hovered heads
    for (var i = 0; i < 7; i = i + 1) {
        let hp = u.head_pos[i];
        let hc = u.head_col[i];
        let d = p - uv_to_world(hp.xy, aspect);
        let act = clamp(hp.z, 0.0, 1.0);
        let wrn = clamp(hp.w, 0.0, 1.0);
        let emp = clamp(hc.w, 0.0, 1.0);
        let halo_str = 0.045 + emp * 0.14 + act * 0.26 + wrn * 0.20;
        let tint = mix(hc.rgb, vec3<f32>(0.96, 0.64, 0.20), wrn * 0.65);
        col = col + tint * exp(-dot(d, d) * 14.0 / max(sc * sc, 0.12)) * halo_str;
    }

    // ------------------------------------------------------------------------
    // Layers 1..5: The Reborn Seven-Headed Dragon Leviathan (True Depth Order)
    // ------------------------------------------------------------------------
    col = render_wings(p, heart_w, sc, breath, pulse, offering, quality, col);
    col = render_torso_and_tail(p, heart_w, sc, t_anim, breath, pulse, offering, quality, col);

    // Depth-sorted necks & heads (explicitly unrolled for maximum WGSL portability
    // and constant-folded GPU execution: outer pairs -> mid pairs -> sovereign apex 3)
    col = render_neck(p, 0, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_dragon_head(p, 0, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_neck(p, 6, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_dragon_head(p, 6, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_neck(p, 1, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_dragon_head(p, 1, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_neck(p, 5, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_dragon_head(p, 5, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_neck(p, 2, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_dragon_head(p, 2, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_neck(p, 4, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_dragon_head(p, 4, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_neck(p, 3, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);
    col = render_dragon_head(p, 3, heart_w, sc, aspect, t_anim, pulse, offering, quality, col);

    col = render_amethyst_heart(p, heart_w, sc, t_anim, pulse, offering, col);

    // ------------------------------------------------------------------------
    // Layer 6: Subtle Floating Amethyst Embers / Atmospheric Particles
    // ------------------------------------------------------------------------
    if (quality >= 2.0) {
        let grid_scale = select(14.0, 19.0, quality >= 2.5);
        let mp = p * grid_scale + vec2<f32>(sin(t_anim * 0.22) * 0.35, t_anim * 0.45);
        let cell = floor(mp);
        let f = fract(mp) - 0.5;
        let rnd = hash21(cell);
        if (rnd > 0.84) {
            let off = vec2<f32>(hash21(cell + 17.3) - 0.5, hash21(cell + 43.1) - 0.5) * 0.62;
            let md = length(f - off);
            let twinkle = 0.55 + 0.45 * sin(t_anim * 2.2 + rnd * 40.0);
            let mote = exp(-md * md * 95.0) * twinkle * smoothstep(1.15, 0.25, center_d);
            let mote_col = mix(vec3<f32>(0.76, 0.48, 1.0), vec3<f32>(0.42, 0.84, 1.0), fract(rnd * 7.1));
            col = col + mote_col * mote * 0.32;
        }
    }

    // Subtle moonlit memorial glow beneath the Heart for Ashlan's inscription
    let dm = vec2<f32>((uv.x - 0.5) * aspect * 0.65, uv.y - 0.93);
    col = col + vec3<f32>(0.48, 0.30, 0.72) * exp(-dot(dm, dm) * 28.0) * (0.085 + 0.025 * pulse);

    // Suspended safety tint vignette when EMERGENCY SUSPEND is active
    if (u.canvas.w > 0.5) {
        let edge_s = max(abs(uv.x - 0.5) * 2.0, abs(uv.y - 0.5) * 2.0);
        col = col + vec3<f32>(0.55, 0.08, 0.14) * smoothstep(0.65, 1.0, edge_s) * 0.28;
    }

    // Cinematic edge vignette so sanctuary melts smoothly into obsidian command rails
    let edge = max(abs(uv.x - 0.5) * 2.0, abs(uv.y - 0.5) * 2.0);
    let vig = 1.0 - smoothstep(0.70, 1.06, edge) * 0.52;
    col = col * vig * clamp(u.selection.z, 0.0, 1.0);

    // ACES Filmic tonemapping + subtle anti-banding dither
    col = aces_tonemap(col);
    let dither = (hash21(in.pos.xy) - 0.5) * (1.5 / 255.0);
    col = clamp(col + vec3<f32>(dither), vec3<f32>(0.0), vec3<f32>(1.0));

    if (u.params.w > 0.5) {
        col = pow(col, vec3<f32>(2.2));
    }
    return vec4<f32>(col, 1.0);
}
"#;

#[derive(Clone, Debug)]
pub struct AtmosphereParams {
    pub time: f32,
    pub motion: bool,
    pub quality: u8,
    pub heart_uv: [f32; 2],
    pub heart_pulse: f32,
    pub offering_glow: f32,
    pub aspect: f32,
    pub active_total: f32,
    pub breath: f32,
    pub suspended_alert: bool,
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    pub intro_fade: f32,
    pub creature_scale: f32,
    pub head_uvs: [[f32; 2]; 7],
    pub head_colors: [Color32; 7],
    pub head_activity: [f32; 7],
    pub head_warning: [bool; 7],
    pub head_emphasis: [f32; 7],
    pub head_look: [[f32; 2]; 7],
    pub head_eye_open: [f32; 7],
    pub head_observation: [f32; 7],
    pub neck_ctrl1_uv: [[f32; 2]; 7],
    pub neck_ctrl2_uv: [[f32; 2]; 7],
}

impl AtmosphereParams {
    pub fn pack_uniforms(&self, is_srgb: bool) -> [u8; UNIFORM_BYTES] {
        let mut floats = [0.0f32; UNIFORM_FLOATS];
        let safe = |v: f32| if v.is_finite() { v } else { 0.0 };

        // params (0..4)
        floats[0] = safe(self.time);
        floats[1] = if self.motion { 1.0 } else { 0.0 };
        floats[2] = self.quality.min(3) as f32;
        floats[3] = if is_srgb { 1.0 } else { 0.0 };

        // heart (4..8)
        floats[4] = safe(self.heart_uv[0]).clamp(0.0, 1.0);
        floats[5] = safe(self.heart_uv[1]).clamp(0.0, 1.0);
        floats[6] = safe(self.heart_pulse).clamp(0.0, 1.0);
        floats[7] = safe(self.offering_glow).clamp(0.0, 1.0);

        // canvas (8..12)
        floats[8] = safe(self.aspect).clamp(0.25, 8.0);
        floats[9] = safe(self.active_total).clamp(0.0, 1.0);
        floats[10] = safe(self.breath).clamp(0.0, 1.0);
        floats[11] = if self.suspended_alert { 1.0 } else { 0.0 };

        // selection (12..16)
        floats[12] = self.selected.map(|i| i as f32).unwrap_or(-1.0);
        floats[13] = self.hovered.map(|i| i as f32).unwrap_or(-1.0);
        floats[14] = safe(self.intro_fade).clamp(0.0, 1.0);
        floats[15] = safe(self.creature_scale).clamp(0.25, 4.0);

        // head_pos (16..44)
        for i in 0..7 {
            let base = 16 + i * 4;
            floats[base] = safe(self.head_uvs[i][0]).clamp(-0.3, 1.3);
            floats[base + 1] = safe(self.head_uvs[i][1]).clamp(-0.3, 1.3);
            floats[base + 2] = safe(self.head_activity[i]).clamp(0.0, 1.0);
            floats[base + 3] = if self.head_warning[i] { 1.0 } else { 0.0 };
        }
        // head_col (44..72)
        for i in 0..7 {
            let base = 44 + i * 4;
            let c = self.head_colors[i];
            floats[base] = c.r() as f32 / 255.0;
            floats[base + 1] = c.g() as f32 / 255.0;
            floats[base + 2] = c.b() as f32 / 255.0;
            floats[base + 3] = safe(self.head_emphasis[i]).clamp(0.0, 1.0);
        }
        // head_anim (72..100)
        for i in 0..7 {
            let base = 72 + i * 4;
            floats[base] = safe(self.head_look[i][0]).clamp(-1.0, 1.0);
            floats[base + 1] = safe(self.head_look[i][1]).clamp(-1.0, 1.0);
            floats[base + 2] = safe(self.head_eye_open[i]).clamp(0.06, 1.0);
            floats[base + 3] = safe(self.head_observation[i]).clamp(0.0, 1.0);
        }
        // neck_ctrl (100..128)
        for i in 0..7 {
            let base = 100 + i * 4;
            floats[base] = safe(self.neck_ctrl1_uv[i][0]).clamp(-0.3, 1.3);
            floats[base + 1] = safe(self.neck_ctrl1_uv[i][1]).clamp(-0.3, 1.3);
            floats[base + 2] = safe(self.neck_ctrl2_uv[i][0]).clamp(-0.3, 1.3);
            floats[base + 3] = safe(self.neck_ctrl2_uv[i][1]).clamp(-0.3, 1.3);
        }

        let mut bytes = [0u8; UNIFORM_BYTES];
        for (idx, val) in floats.iter().enumerate() {
            let b = val.to_le_bytes();
            bytes[idx * 4..idx * 4 + 4].copy_from_slice(&b);
        }
        bytes
    }
}

struct AtmosphereResources {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    is_srgb: bool,
}

/// Register the PBR Leviathan WGPU pipeline into egui_wgpu's callback resources
/// during app creation. Returns true if WGPU shader initialization succeeded.
pub fn register_wgpu_atmosphere(cc: &eframe::CreationContext<'_>) -> bool {
    let Some(render_state) = cc.wgpu_render_state.as_ref() else {
        return false;
    };
    let device = &render_state.device;
    let target_format = render_state.target_format;
    let is_srgb = target_format.is_srgb();

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hydra_leviathan_pbr_shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hydra_leviathan_pbr_uniforms"),
        size: UNIFORM_BYTES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hydra_leviathan_pbr_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(UNIFORM_BYTES as u64),
            },
            count: None,
        }],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hydra_leviathan_pbr_bg"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hydra_leviathan_pbr_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hydra_leviathan_pbr_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: target_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    });

    render_state
        .renderer
        .write()
        .callback_resources
        .insert(AtmosphereResources {
            pipeline,
            bind_group,
            uniform_buffer,
            is_srgb,
        });
    true
}

struct AtmosphereCallback {
    params: AtmosphereParams,
}

impl egui_wgpu::CallbackTrait for AtmosphereCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(res) = callback_resources.get::<AtmosphereResources>() {
            let bytes = self.params.pack_uniforms(res.is_srgb);
            queue.write_buffer(&res.uniform_buffer, 0, &bytes);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(res) = callback_resources.get::<AtmosphereResources>() {
            render_pass.set_pipeline(&res.pipeline);
            render_pass.set_bind_group(0, &res.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

/// Emit a paint callback bounded strictly to `rect` on the WGPU render pass.
pub fn paint_wgpu_atmosphere(painter: &egui::Painter, rect: Rect, params: AtmosphereParams) {
    if rect.width() < 2.0 || rect.height() < 2.0 {
        return;
    }
    let callback = egui_wgpu::Callback::new_paint_callback(rect, AtmosphereCallback { params });
    painter.add(callback);
}

/// Convert a screen position inside `rect` into normalized `[0..1]` UV coordinates.
pub fn pos_to_uv(rect: Rect, pos: Pos2) -> [f32; 2] {
    let w = rect.width().max(1.0);
    let h = rect.height().max(1.0);
    [(pos.x - rect.left()) / w, (pos.y - rect.top()) / h]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_packing_is_aligned_and_sanitizes_nonfinite_inputs() {
        assert_eq!(UNIFORM_BYTES, 512);
        assert_eq!(UNIFORM_BYTES % 16, 0);
        let params = AtmosphereParams {
            time: f32::NAN,
            motion: false,
            quality: 99,
            heart_uv: [f32::INFINITY, -5.0],
            heart_pulse: f32::NEG_INFINITY,
            offering_glow: f32::NAN,
            aspect: f32::NAN,
            active_total: 2.5,
            breath: f32::INFINITY,
            suspended_alert: true,
            selected: Some(3),
            hovered: None,
            intro_fade: f32::NAN,
            creature_scale: f32::NEG_INFINITY,
            head_uvs: [[0.5, 0.5]; 7],
            head_colors: [Color32::LIGHT_BLUE; 7],
            head_activity: [f32::NAN; 7],
            head_warning: [false; 7],
            head_emphasis: [0.5; 7],
            head_look: [[f32::NAN, f32::INFINITY]; 7],
            head_eye_open: [f32::NAN; 7],
            head_observation: [f32::INFINITY; 7],
            neck_ctrl1_uv: [[0.5, 0.6]; 7],
            neck_ctrl2_uv: [[0.5, 0.55]; 7],
        };
        let bytes = params.pack_uniforms(false);
        assert_eq!(bytes.len(), UNIFORM_BYTES);
        for chunk in bytes.chunks_exact(4) {
            let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            assert!(v.is_finite(), "packed non-finite float {v}");
        }
        // Quality is clamped to 3.0 (Ultra)
        let q = f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(q, 3.0);
    }

    #[test]
    fn wgsl_source_contains_pbr_leviathan_pipeline_functions() {
        assert!(SHADER_WGSL.contains("fn vs_main"));
        assert!(SHADER_WGSL.contains("fn fs_main"));
        assert!(SHADER_WGSL.contains("fn dragon_scales"));
        assert!(SHADER_WGSL.contains("fn pbr_obsidian"));
        assert!(SHADER_WGSL.contains("fn render_dragon_head"));
        assert!(SHADER_WGSL.contains("fn render_amethyst_heart"));
        assert!(SHADER_WGSL.contains("fn aces_tonemap"));
    }
}
