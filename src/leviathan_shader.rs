//! Optional GPU-accelerated WGSL atmospheric backdrop for the Leviathan canvas.
//!
//! Renders volumetric abyssal lighting, Amethyst Heart bloom, subtle sanctuary
//! geometry, and per-head reactive light halos inside the root canvas rect.
//! Never fabricates trading activity: operational intensity is driven strictly
//! by caller-supplied evidence, and reduced motion freezes all time drift.

use eframe::egui::{self, Color32, Pos2, Rect};
use eframe::egui_wgpu::{self, wgpu};
use std::borrow::Cow;
use std::num::NonZeroU64;

const UNIFORM_FLOATS: usize = (3 + 7 + 7) * 4;
const UNIFORM_BYTES: usize = UNIFORM_FLOATS * 4;

const SHADER_WGSL: &str = r#"
struct Uniforms {
    params: vec4<f32>,          // x=time, y=motion(0|1), z=quality(0|1|2), w=is_srgb(0|1)
    heart: vec4<f32>,           // xy=heart_uv, z=heart_pulse(0..1), w=heart_alert(0|1)
    canvas: vec4<f32>,          // x=aspect, y=active_total, z=selected_idx(-1..6), w=hovered_idx(-1..6)
    head_pos: array<vec4<f32>, 7>, // xy=uv, z=activity(0..1), w=warning(0|1)
    head_col: array<vec4<f32>, 7>, // rgb=accent(0..1), w=emphasis(0..1)
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

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.55;
    var q = p;
    for (var i = 0; i < 3; i = i + 1) {
        v = v + amp * value_noise(q);
        q = q * 2.07 + vec2<f32>(13.1, 7.7);
        amp = amp * 0.48;
    }
    return v;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let aspect = max(u.canvas.x, 0.25);
    let t = u.params.x * u.params.y;
    let quality = u.params.z;

    // Aspect-corrected coordinates relative to the Amethyst Heart.
    let heart_uv = u.heart.xy;
    let dh = vec2<f32>((uv.x - heart_uv.x) * aspect, uv.y - heart_uv.y);
    let r_heart = length(dh);

    // Base abyssal obsidian-to-indigo vault gradient.
    let center_d = length(vec2<f32>((uv.x - 0.5) * aspect, (uv.y - 0.52) * 1.15));
    var col = mix(
        vec3<f32>(0.036, 0.032, 0.072),
        vec3<f32>(0.015, 0.020, 0.042),
        clamp(center_d * 0.92, 0.0, 1.0)
    );

    // Subtle abyssal nebula clouds (medium/high quality).
    if (quality >= 0.5) {
        let drift = vec2<f32>(t * 0.014, -t * 0.020);
        let p1 = vec2<f32>(uv.x * aspect * 2.2, uv.y * 2.2) + drift;
        let n1 = fbm(p1);
        let n2 = fbm(p1 * 1.9 - drift * 1.5 + vec2<f32>(3.4, 1.9));
        let nebula = smoothstep(0.28, 0.86, n1 * 0.62 + n2 * 0.48);
        let nebula_mask = exp(-center_d * center_d * 0.88);
        let amethyst_mist = vec3<f32>(0.26, 0.11, 0.46);
        let deep_cyan_mist = vec3<f32>(0.05, 0.16, 0.28);
        let mist_tint = mix(amethyst_mist, deep_cyan_mist, smoothstep(0.18, 0.82, n2));
        col = col + mist_tint * nebula * nebula_mask * (0.20 + 0.07 * quality);
    }

    // Living Amethyst Heart volumetric core bloom + subtle heartbeat ripple.
    let pulse = clamp(u.heart.z, 0.0, 1.0);
    let heart_core = exp(-r_heart * r_heart * (8.2 - 1.8 * pulse));
    let heart_wide = exp(-r_heart * 2.15);
    let heart_tint = mix(
        vec3<f32>(0.52, 0.24, 0.84),
        vec3<f32>(0.80, 0.54, 0.98),
        pulse * 0.68
    );
    col = col + heart_tint * (heart_core * (0.30 + 0.15 * pulse) + heart_wide * 0.12);

    // Subtle sacred telemetry rings & radial rays centered on the sanctuary.
    if (quality >= 0.5) {
        let angle = atan2(dh.y, dh.x);
        let radial_beam = pow(max(0.0, cos(angle * 7.0 + 1.5708)), 9.0);
        let beam_fade = exp(-r_heart * 2.2) * smoothstep(0.05, 0.18, r_heart);
        col = col + vec3<f32>(0.42, 0.24, 0.72) * radial_beam * beam_fade * (0.065 + 0.04 * pulse);

        // Expanding subtle amethyst resonance ring on heartbeat / active work.
        let ring_r = fract(t * 0.14) * 0.98 + 0.10;
        let ring_band = exp(-pow((r_heart - ring_r) * 32.0, 2.0));
        let ring_strength = (0.022 + 0.045 * clamp(u.canvas.y, 0.0, 1.0)) * u.params.y * (1.0 - ring_r * 0.82);
        col = col + vec3<f32>(0.64, 0.40, 0.95) * ring_band * ring_strength;

        // Ultra-fine perspective sanctuary grid near the lower altar.
        let altar = smoothstep(0.50, 0.95, uv.y) * (1.0 - smoothstep(0.92, 1.0, uv.y));
        let gx = abs(fract(uv.x * 26.0) - 0.5);
        let gy = abs(fract(uv.y * 16.0) - 0.5);
        let grid_line = smoothstep(0.475, 0.498, max(gx, gy));
        col = col + vec3<f32>(0.36, 0.25, 0.62) * grid_line * altar * 0.075;
    }

    // Seven head reactive volumetric halos (truthful activity & alert driven).
    for (var i = 0; i < 7; i = i + 1) {
        let hp = u.head_pos[i];
        let hc = u.head_col[i];
        let d = vec2<f32>((uv.x - hp.x) * aspect, uv.y - hp.y);
        let dist2 = dot(d, d);
        let activity = clamp(hp.z, 0.0, 1.0);
        let warning = clamp(hp.w, 0.0, 1.0);
        let emphasis = clamp(hc.w, 0.0, 1.0);
        let strength = 0.095 + emphasis * 0.18 + activity * 0.28 + warning * 0.20;
        let radius_k = mix(22.0, 11.5, clamp(emphasis + activity * 0.8 + warning * 0.6, 0.0, 1.0));
        let halo = exp(-dist2 * radius_k);
        let warn_tint = vec3<f32>(0.96, 0.66, 0.26);
        let tint = mix(hc.rgb, warn_tint, warning * 0.65);
        col = col + tint * halo * strength;
    }

    // Subtle memorial moonlit glow beneath the heart.
    let dm = vec2<f32>((uv.x - 0.5) * aspect * 0.65, uv.y - 0.93);
    let mem_glow = exp(-dot(dm, dm) * 28.0);
    col = col + vec3<f32>(0.52, 0.36, 0.74) * mem_glow * (0.09 + 0.025 * pulse);

    // Cinematic edge vignette so canvas dissolves seamlessly into obsidian frame.
    let edge = max(abs(uv.x - 0.5) * 2.0, abs(uv.y - 0.5) * 2.0);
    let vig = 1.0 - smoothstep(0.68, 1.06, edge) * 0.55;
    col = col * vig;

    // Gentle anti-banding dither (order 1/255).
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
    pub heart_alert: bool,
    pub aspect: f32,
    pub active_total: f32,
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    pub head_uvs: [[f32; 2]; 7],
    pub head_colors: [Color32; 7],
    pub head_activity: [f32; 7],
    pub head_warning: [bool; 7],
    pub head_emphasis: [f32; 7],
}

impl AtmosphereParams {
    pub fn pack_uniforms(&self, is_srgb: bool) -> [u8; UNIFORM_BYTES] {
        let mut floats = [0.0f32; UNIFORM_FLOATS];
        let safe = |v: f32| if v.is_finite() { v } else { 0.0 };
        floats[0] = safe(self.time);
        floats[1] = if self.motion { 1.0 } else { 0.0 };
        floats[2] = self.quality.min(2) as f32;
        floats[3] = if is_srgb { 1.0 } else { 0.0 };

        floats[4] = safe(self.heart_uv[0]).clamp(0.0, 1.0);
        floats[5] = safe(self.heart_uv[1]).clamp(0.0, 1.0);
        floats[6] = safe(self.heart_pulse).clamp(0.0, 1.0);
        floats[7] = if self.heart_alert { 1.0 } else { 0.0 };

        floats[8] = safe(self.aspect).clamp(0.25, 8.0);
        floats[9] = safe(self.active_total).clamp(0.0, 1.0);
        floats[10] = self.selected.map(|i| i as f32).unwrap_or(-1.0);
        floats[11] = self.hovered.map(|i| i as f32).unwrap_or(-1.0);

        for i in 0..7 {
            let base = 12 + i * 4;
            floats[base] = safe(self.head_uvs[i][0]).clamp(-0.2, 1.2);
            floats[base + 1] = safe(self.head_uvs[i][1]).clamp(-0.2, 1.2);
            floats[base + 2] = safe(self.head_activity[i]).clamp(0.0, 1.0);
            floats[base + 3] = if self.head_warning[i] { 1.0 } else { 0.0 };
        }
        for i in 0..7 {
            let base = 40 + i * 4;
            let c = self.head_colors[i];
            floats[base] = c.r() as f32 / 255.0;
            floats[base + 1] = c.g() as f32 / 255.0;
            floats[base + 2] = c.b() as f32 / 255.0;
            floats[base + 3] = safe(self.head_emphasis[i]).clamp(0.0, 1.0);
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

/// Register the volumetric WGPU backdrop pipeline into egui_wgpu's callback
/// resources during app creation. Returns true if WGPU shader initialization
/// succeeded.
pub fn register_wgpu_atmosphere(cc: &eframe::CreationContext<'_>) -> bool {
    let Some(render_state) = cc.wgpu_render_state.as_ref() else {
        return false;
    };
    let device = &render_state.device;
    let target_format = render_state.target_format;
    let is_srgb = target_format.is_srgb();

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("hydra_leviathan_atmosphere_shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hydra_leviathan_atmosphere_uniforms"),
        size: UNIFORM_BYTES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("hydra_leviathan_atmosphere_bgl"),
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
        label: Some("hydra_leviathan_atmosphere_bg"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("hydra_leviathan_atmosphere_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("hydra_leviathan_atmosphere_pipeline"),
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
        assert_eq!(UNIFORM_BYTES % 16, 0);
        let params = AtmosphereParams {
            time: f32::NAN,
            motion: false,
            quality: 99,
            heart_uv: [f32::INFINITY, -5.0],
            heart_pulse: f32::NEG_INFINITY,
            heart_alert: true,
            aspect: f32::NAN,
            active_total: 2.5,
            selected: Some(3),
            hovered: None,
            head_uvs: [[0.5, 0.5]; 7],
            head_colors: [Color32::LIGHT_BLUE; 7],
            head_activity: [f32::NAN; 7],
            head_warning: [false; 7],
            head_emphasis: [0.5; 7],
        };
        let bytes = params.pack_uniforms(false);
        assert_eq!(bytes.len(), UNIFORM_BYTES);
        for chunk in bytes.chunks_exact(4) {
            let v = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            assert!(v.is_finite(), "packed non-finite float {v}");
        }
        // Quality is clamped to 2.0
        let q = f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(q, 2.0);
    }

    #[test]
    fn wgsl_source_contains_required_entry_points() {
        assert!(SHADER_WGSL.contains("fn vs_main"));
        assert!(SHADER_WGSL.contains("fn fs_main"));
        assert!(SHADER_WGSL.contains("var<uniform> u: Uniforms"));
    }
}
