//! Original procedural seven-headed Leviathan command sanctuary.
//!
//! Idle breathing, blinking, wing lift, tail undulation, and steady Amethyst
//! Heartbeat are decorative anatomy; operational energy pathways, shield
//! manifestation, and alert flares strictly reflect real observed state.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2,
};
use std::f32::consts::{PI, TAU};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum SceneQuality {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct HeadPresentation {
    pub name: &'static str,
    pub status: &'static str,
    pub accent: Color32,
    pub activity: f32,
    /// Genuine local-history change, distinct from operational research activity.
    pub observation: f32,
    pub warning: bool,
}

pub struct SceneInput<'a> {
    pub heads: &'a [HeadPresentation; 7],
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    pub time: f32,
    pub motion: bool,
    pub quality: SceneQuality,
    pub heart_status: &'a str,
    pub wgpu_atmosphere: bool,
    /// Genuine Offering / Treasury verification illumination (`0.0..=1.0`).
    /// Never positive unless real local wallet validation or venue verification completed.
    pub offering_glow: f32,
    pub operational_mode: crate::treasury::OperationalMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadPose {
    pub center: Pos2,
    pub angle: f32,
    pub neck: [Pos2; 4],
}

/// Smooth physiological breathing wave (`0.0..=1.0`), frozen when reduced motion is enabled.
pub fn breath_cycle(time: f32, motion: bool) -> f32 {
    if !motion || !time.is_finite() {
        return 0.45;
    }
    let t = time.rem_euclid(3600.0);
    let raw = (t * 1.18).sin() * 0.5 + 0.5;
    raw * raw * (3.0 - 2.0 * raw)
}

/// Dual-peak cardiac *lub-dub* pulse (`0.0..=1.0`) for the central Amethyst Heart.
pub fn heart_pulse(time: f32, motion: bool) -> f32 {
    if !motion || !time.is_finite() {
        return 0.32;
    }
    let period = 1.48;
    let p = (time.rem_euclid(3600.0) % period) / period;
    let lub = (-((p - 0.14) / 0.055).powi(2)).exp();
    let dub = 0.58 * (-((p - 0.30) / 0.068).powi(2)).exp();
    (0.18 + 0.82 * (lub + dub).min(1.0)).clamp(0.0, 1.0)
}

/// Smooth per-head organic blink openness (`0.08..=1.0`), staggered across heads.
pub fn eye_openness(index: usize, time: f32, motion: bool) -> f32 {
    if !motion || !time.is_finite() {
        return 1.0;
    }
    let period = 5.4 + (index as f32) * 0.77;
    let phase = (time.rem_euclid(3600.0) + (index as f32) * 1.37) % period;
    let duration = 0.24;
    if phase < duration {
        let u = (phase / duration) * PI;
        let close = u.sin().powf(1.35);
        (1.0 - 0.92 * close).clamp(0.08, 1.0)
    } else {
        1.0
    }
}

/// Logical creature geometry, independent of platform DPI. Idle motion is decorative.
pub fn head_pose(index: usize, time: f32, motion: bool) -> HeadPose {
    let i = index.min(6);
    let centers = [
        (125.0, 308.0),
        (278.0, 234.0),
        (438.0, 198.0),
        (600.0, 174.0),
        (762.0, 198.0),
        (922.0, 234.0),
        (1075.0, 308.0),
    ];
    let t = if motion && time.is_finite() {
        time.rem_euclid(3600.0)
    } else {
        0.0
    };
    let phase = i as f32 * 1.618;
    let b = if motion {
        (breath_cycle(t, true) - 0.45) * 4.2
    } else {
        0.0
    };
    let sway = if motion {
        (t * 0.52 + phase).sin() * 5.2 + (t * 1.11 + phase * 1.7).sin() * 1.5
    } else {
        0.0
    };
    let rise = if motion {
        (t * 0.68 + phase * 0.9).sin() * 3.2 - b
    } else {
        0.0
    };
    let neck_ripple_1 = if motion {
        (t * 0.52 + phase - 0.75).sin() * 5.8
    } else {
        0.0
    };
    let neck_ripple_2 = if motion {
        (t * 0.52 + phase - 0.35).sin() * 4.4
    } else {
        0.0
    };
    let (x, y) = centers[i];
    let center = Pos2::new(x + sway, y + rise);
    let spread = i as f32 - 3.0;
    let angle = spread * 0.115
        + if motion {
            (t * 0.42 + phase).sin() * 0.026
        } else {
            0.0
        };
    HeadPose {
        center,
        angle,
        neck: [
            Pos2::new(600.0 + spread * 29.0, 615.0),
            Pos2::new(600.0 + spread * 82.0 + neck_ripple_1, 472.0 - b * 0.6),
            Pos2::new(x - spread * 14.0 + neck_ripple_2, y + 178.0),
            Pos2::new(center.x, center.y + 46.0),
        ],
    }
}

#[derive(Clone, Copy)]
struct Canvas {
    origin: Pos2,
    scale: f32,
    scale_x: f32,
}

impl Canvas {
    fn new(rect: Rect) -> Self {
        let scale = (rect.width() / 1200.0)
            .min(rect.height() / 820.0)
            .max(0.0001);
        let scale_x = (rect.width() / 1200.0).min(scale * 1.35);
        Self {
            origin: Pos2::new(
                rect.center().x - 600.0 * scale_x,
                rect.center().y - 410.0 * scale,
            ),
            scale,
            scale_x,
        }
    }
    fn p(self, p: Pos2) -> Pos2 {
        self.origin + Vec2::new(p.x * self.scale_x, p.y * self.scale)
    }
    fn xy(self, x: f32, y: f32) -> Pos2 {
        self.p(Pos2::new(x, y))
    }
}

pub fn head_hit_regions(rect: Rect, time: f32, motion: bool) -> [Rect; 7] {
    let c = Canvas::new(rect);
    std::array::from_fn(|i| {
        let p = head_pose(i, time, motion);
        Rect::from_center_size(
            c.p(p.center + Vec2::new(0.0, -10.0)),
            Vec2::new(115.0 * c.scale_x, 165.0 * c.scale),
        )
    })
}

fn cubic(p: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    Pos2::new(
        u * u * u * p[0].x
            + 3.0 * u * u * t * p[1].x
            + 3.0 * u * t * t * p[2].x
            + t * t * t * p[3].x,
        u * u * u * p[0].y
            + 3.0 * u * u * t * p[1].y
            + 3.0 * u * t * t * p[2].y
            + t * t * t * p[3].y,
    )
}

fn fade(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
    )
}

fn polygon(p: &egui::Painter, c: Canvas, points: &[Pos2], fill: Color32, stroke: Stroke) {
    p.add(Shape::convex_polygon(
        points.iter().map(|v| c.p(*v)).collect(),
        fill,
        Stroke::new(stroke.width * c.scale, stroke.color),
    ));
}

fn line(p: &egui::Painter, c: Canvas, points: &[Pos2], color: Color32, width: f32) {
    p.add(Shape::line(
        points.iter().map(|v| c.p(*v)).collect(),
        Stroke::new(width * c.scale, color),
    ));
}

fn glow_ex(
    p: &egui::Painter,
    c: Canvas,
    at: Pos2,
    r: f32,
    color: Color32,
    strength: f32,
    quality: SceneQuality,
) {
    let layers = match quality {
        SceneQuality::Low => 1,
        SceneQuality::Medium => 4,
        SceneQuality::High => 6,
    };
    let s = strength.clamp(0.0, 2.5);
    for layer in (1..=layers).rev() {
        let frac = layer as f32 / layers as f32;
        let radius = (r * (0.45 + frac * 1.15) + layer as f32 * 4.0) * c.scale;
        let alpha = ((26.0 * s) / (layer as f32 * 1.35)).clamp(1.0, 120.0) as u8;
        p.circle_filled(c.p(at), radius, fade(color, alpha));
    }
}

#[allow(clippy::too_many_arguments)]
fn arc(
    p: &egui::Painter,
    c: Canvas,
    center: Pos2,
    rx: f32,
    ry: f32,
    start: f32,
    end: f32,
    color: Color32,
    width: f32,
) {
    let steps = 40;
    let pts: Vec<_> = (0..=steps)
        .map(|i| {
            let a = start + (end - start) * i as f32 / steps as f32;
            center + Vec2::new(a.cos() * rx, a.sin() * ry)
        })
        .collect();
    line(p, c, &pts, color, width);
}

fn metal(accent: Color32) -> Color32 {
    blend(Color32::from_rgb(13, 18, 29), accent, 0.14)
}

fn obsidian_dark(accent: Color32) -> Color32 {
    blend(Color32::from_rgb(7, 10, 19), accent, 0.08)
}

fn secondary(index: usize) -> Color32 {
    match index {
        0 => Color32::from_rgb(138, 240, 255),
        1 => Color32::from_rgb(245, 191, 84),
        2 => Color32::from_rgb(224, 192, 255),
        3 => Color32::from_rgb(198, 228, 219),
        4 => Color32::from_rgb(206, 227, 250),
        5 => Color32::from_rgb(186, 152, 248),
        _ => Color32::from_rgb(252, 224, 255),
    }
}

fn safe_activity(head: &HeadPresentation) -> f32 {
    if head.activity.is_finite() {
        head.activity.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn atmosphere(
    p: &egui::Painter,
    rect: Rect,
    c: Canvas,
    poses: &[HeadPose; 7],
    input: &SceneInput<'_>,
) {
    let purple = Color32::from_rgb(142, 86, 222);
    let moonlit = Color32::from_rgb(198, 184, 235);
    let pulse = heart_pulse(input.time, input.motion);
    let t = if input.motion && input.time.is_finite() {
        input.time.rem_euclid(3600.0)
    } else {
        0.0
    };

    #[cfg(feature = "desktop-wgpu")]
    let used_shader = if input.wgpu_atmosphere {
        let active_total = input
            .heads
            .iter()
            .map(safe_activity)
            .fold(0.0_f32, f32::max);
        let heart_alert = input.heads.iter().any(|h| h.warning);
        let head_uvs =
            std::array::from_fn(|i| crate::leviathan_shader::pos_to_uv(rect, c.p(poses[i].center)));
        let head_colors = std::array::from_fn(|i| input.heads[i].accent);
        let head_activity = std::array::from_fn(|i| safe_activity(&input.heads[i]));
        let head_warning = std::array::from_fn(|i| input.heads[i].warning);
        let head_emphasis = std::array::from_fn(|i| {
            let mut e = 0.0_f32;
            if input.selected == Some(i) {
                e += 0.72;
            }
            if input.hovered == Some(i) {
                e += 0.48;
            }
            e.clamp(0.0, 1.0)
        });
        let params = crate::leviathan_shader::AtmosphereParams {
            time: t,
            motion: input.motion,
            quality: match input.quality {
                SceneQuality::Low => 0,
                SceneQuality::Medium => 1,
                SceneQuality::High => 2,
            },
            heart_uv: crate::leviathan_shader::pos_to_uv(rect, c.xy(600.0, 622.0)),
            heart_pulse: pulse,
            heart_alert,
            aspect: rect.width() / rect.height().max(1.0),
            active_total,
            selected: input.selected,
            hovered: input.hovered,
            head_uvs,
            head_colors,
            head_activity,
            head_warning,
            head_emphasis,
        };
        crate::leviathan_shader::paint_wgpu_atmosphere(p, rect, params);
        true
    } else {
        false
    };
    #[cfg(not(feature = "desktop-wgpu"))]
    let used_shader = false;

    if !used_shader {
        // Layered Glow / fallback abyssal sanctuary backdrop.
        p.rect_filled(rect, 0.0, Color32::from_rgb(5, 7, 14));
        if input.quality != SceneQuality::Low {
            for j in (1..=9).rev() {
                let rad = (115.0 + j as f32 * 48.0) * c.scale;
                let a = (11 - j).max(2) as u8;
                p.circle_filled(
                    c.xy(600.0, 485.0),
                    rad,
                    Color32::from_rgba_unmultiplied(64, 28, 108, a),
                );
            }
            // Soft volumetric per-head sanctuary halos.
            for (i, head) in input.heads.iter().enumerate() {
                let act = safe_activity(head);
                let emph = if input.selected == Some(i) {
                    0.85
                } else if input.hovered == Some(i) {
                    0.55
                } else {
                    0.18
                };
                let tint = if head.warning {
                    Color32::from_rgb(245, 168, 72)
                } else {
                    head.accent
                };
                glow_ex(
                    p,
                    c,
                    poses[i].center,
                    48.0 + act * 22.0,
                    tint,
                    emph + act * 0.9,
                    input.quality,
                );
            }
        }
    }

    // Sacred cybernetic astrolabe rings & radial sanctuary markings.
    let ring_center = Pos2::new(600.0, 420.0);
    for (idx, r) in [210.0_f32, 292.0, 378.0, 456.0].iter().enumerate() {
        let alpha = if idx == 1 { 34 } else { 22 };
        arc(
            p,
            c,
            ring_center,
            *r,
            *r * 0.88,
            PI * 1.03,
            PI * 1.97,
            fade(purple, alpha),
            if idx == 1 { 1.0 } else { 0.7 },
        );
    }
    if input.quality != SceneQuality::Low {
        let rot = if input.motion { t * 0.035 } else { 0.0 };
        for k in 0..24 {
            let a = PI * 1.04 + (k as f32 / 23.0) * PI * 0.92;
            let v = Vec2::new(a.cos(), a.sin() * 0.88);
            let tick = if k % 4 == 0 { 11.0 } else { 5.0 };
            line(
                p,
                c,
                &[ring_center + v * 292.0, ring_center + v * (292.0 + tick)],
                fade(moonlit, if k % 4 == 0 { 48 } else { 24 }),
                0.8,
            );
        }
        // Subtle drifting celestial dust & gently ascending amethyst sanctuary motes.
        let count = if input.quality == SceneQuality::High {
            72
        } else {
            36
        };
        for i in 0..count {
            let base_x = 34.0 + ((i * 197 + 53) % 1132) as f32;
            let base_y = 48.0 + ((i * 149 + 19) % 690) as f32;
            let is_ember = i % 5 == 0;
            let drift_y = if input.motion && is_ember {
                ((t * (5.5 + (i % 4) as f32 * 2.2) + i as f32 * 17.0) % 680.0) - 340.0
            } else {
                0.0
            };
            let drift_x = if input.motion && is_ember {
                (t * 0.45 + i as f32 * 1.3 + rot).sin() * 9.0
            } else {
                0.0
            };
            let y = if is_ember {
                720.0 - ((720.0 - (base_y - drift_y)).rem_euclid(670.0))
            } else {
                base_y
            };
            let x = (base_x + drift_x).clamp(24.0, 1176.0);
            let twinkle = if input.motion {
                0.6 + 0.4 * (t * (0.9 + (i % 7) as f32 * 0.25) + i as f32).sin()
            } else {
                0.85
            };
            let (col, rad, alpha) = if is_ember {
                (
                    blend(purple, moonlit, 0.42),
                    1.35 + (i % 3) as f32 * 0.3,
                    (78.0 * twinkle) as u8,
                )
            } else if i % 9 == 0 {
                (moonlit, 1.1, (95.0 * twinkle) as u8)
            } else {
                (moonlit, 0.65, (34.0 * twinkle) as u8)
            };
            p.circle_filled(c.xy(x, y), rad * c.scale, fade(col, alpha));
        }
    }

    // Perimeter sanctuary telemetry rails (left & right).
    for side in [-1.0_f32, 1.0] {
        let x_outer = 600.0 + side * 562.0;
        let x_inner = 600.0 + side * 532.0;
        line(
            p,
            c,
            &[
                Pos2::new(x_outer, 700.0),
                Pos2::new(x_outer, 165.0),
                Pos2::new(x_inner, 118.0),
            ],
            fade(purple, 42),
            1.1,
        );
        for node_y in [240.0, 420.0, 600.0] {
            p.circle_filled(c.xy(x_outer, node_y), 2.2 * c.scale, fade(moonlit, 75));
        }
    }

    // Sanctuary plinth / altar line beneath the heart and above the memorial.
    let altar_alpha = (58.0 + pulse * 38.0) as u8;
    line(
        p,
        c,
        &[
            Pos2::new(260.0, 742.0),
            Pos2::new(456.0, 742.0),
            Pos2::new(488.0, 756.0),
            Pos2::new(712.0, 756.0),
            Pos2::new(744.0, 742.0),
            Pos2::new(940.0, 742.0),
        ],
        fade(purple, altar_alpha),
        1.15,
    );
}

fn body(p: &egui::Painter, c: Canvas, input: &SceneInput<'_>) {
    let edge = Color32::from_rgb(108, 86, 148);
    let amethyst = Color32::from_rgb(164, 104, 240);
    let moonlit = Color32::from_rgb(184, 174, 214);
    let breath = breath_cycle(input.time, input.motion);
    let pulse = heart_pulse(input.time, input.motion);
    let lift = (breath - 0.45) * 6.5;
    let expand = (breath - 0.45) * 5.0;
    let tail_wave = if input.motion && input.time.is_finite() {
        (input.time * 0.85).sin() * 8.0
    } else {
        0.0
    };

    // 1. Colossal sweeping multi-tiered cybernetic dragon wings framing the sanctuary flanks.
    for side in [-1.0_f32, 1.0] {
        let sx = |v: f32| 600.0 + side * v;
        let outer_tip_x = 518.0 + expand * 1.8;
        let outer_tip_y = 374.0 - lift * 1.9;
        let mid_pinion_x = 456.0 + expand * 1.3;
        let mid_pinion_y = 492.0 - lift * 1.2;
        let low_pinion_x = 368.0 + expand * 0.9;
        let low_pinion_y = 582.0 - lift * 0.7;

        // Primary grand sweeping wing membrane (translucent deep indigo-amethyst glass).
        polygon(
            p,
            c,
            &[
                Pos2::new(sx(72.0), 558.0 - lift * 0.4),
                Pos2::new(sx(272.0 + expand), 446.0 - lift * 1.3),
                Pos2::new(sx(outer_tip_x), outer_tip_y),
                Pos2::new(sx(mid_pinion_x), mid_pinion_y),
                Pos2::new(sx(326.0 + expand * 0.7), 548.0 - lift * 0.6),
                Pos2::new(sx(192.0), 598.0 - lift * 0.3),
                Pos2::new(sx(82.0), 618.0),
            ],
            Color32::from_rgba_unmultiplied(24, 17, 46, 175),
            Stroke::new(1.15, fade(blend(edge, amethyst, 0.35), 145)),
        );
        // Secondary lower swept wing membrane pinion.
        polygon(
            p,
            c,
            &[
                Pos2::new(sx(82.0), 598.0 - lift * 0.3),
                Pos2::new(sx(mid_pinion_x), mid_pinion_y + 4.0),
                Pos2::new(sx(low_pinion_x), low_pinion_y),
                Pos2::new(sx(264.0 + expand * 0.5), 626.0 - lift * 0.3),
                Pos2::new(sx(128.0), 662.0),
            ],
            Color32::from_rgba_unmultiplied(16, 16, 36, 190),
            Stroke::new(1.0, fade(edge, 125)),
        );
        // Sculpted beveled obsidian-platinum leading wing bone & curved wing talon.
        polygon(
            p,
            c,
            &[
                Pos2::new(sx(68.0), 554.0 - lift * 0.4),
                Pos2::new(sx(266.0 + expand), 436.0 - lift * 1.3),
                Pos2::new(sx(outer_tip_x + 12.0), outer_tip_y - 10.0),
                Pos2::new(sx(276.0 + expand), 456.0 - lift * 1.3),
                Pos2::new(sx(84.0), 574.0 - lift * 0.4),
            ],
            Color32::from_rgb(30, 27, 50),
            Stroke::new(1.15, fade(moonlit, 150)),
        );
        // Gleaming wing-joint talon spur.
        polygon(
            p,
            c,
            &[
                Pos2::new(sx(258.0 + expand), 442.0 - lift * 1.3),
                Pos2::new(sx(276.0 + expand), 410.0 - lift * 1.5),
                Pos2::new(sx(286.0 + expand), 448.0 - lift * 1.3),
            ],
            Color32::from_rgb(176, 164, 206),
            Stroke::new(0.7, fade(amethyst, 150)),
        );
        // Luminous amethyst cybernetic wing venation ribs.
        for &(tx, ty, w) in &[
            (outer_tip_x - 16.0, outer_tip_y + 10.0, 1.3),
            (mid_pinion_x - 8.0, mid_pinion_y - 2.0, 1.15),
            (low_pinion_x - 6.0, low_pinion_y - 4.0, 1.0),
            (272.0, 616.0, 0.9),
            (196.0, 646.0, 0.85),
        ] {
            line(
                p,
                c,
                &[
                    Pos2::new(sx(96.0), 582.0 - lift * 0.3),
                    Pos2::new(sx(tx), ty),
                ],
                fade(amethyst, (68.0 + pulse * 52.0) as u8),
                w,
            );
            p.circle_filled(
                c.xy(sx(tx), ty),
                2.0 * c.scale,
                fade(moonlit, (95.0 + pulse * 60.0) as u8),
            );
        }
    }

    // 2. Coiling segmented Leviathan tail sweeping behind the right flank.
    let tail = [
        Pos2::new(628.0, 668.0),
        Pos2::new(804.0, 782.0 + tail_wave * 0.35),
        Pos2::new(1118.0, 748.0 - tail_wave * 0.5),
        Pos2::new(1042.0, 618.0 + tail_wave * 0.7),
    ];
    let tail_steps = 48;
    let tail_pts: Vec<_> = (0..=tail_steps)
        .map(|i| cubic(tail, i as f32 / tail_steps as f32))
        .collect();
    line(
        p,
        c,
        &tail_pts,
        Color32::from_rgba_unmultiplied(2, 3, 8, 220),
        38.0,
    );
    for i in 0..tail_steps {
        let t = i as f32 / tail_steps as f32;
        let a = tail_pts[i];
        let b = tail_pts[i + 1];
        let dir = (b - a).normalized();
        let n = Vec2::new(-dir.y, dir.x);
        let w = 24.0 * (1.0 - t * 0.82) + 3.5;
        line(
            p,
            c,
            &[a, b],
            blend(
                Color32::from_rgb(30, 28, 48),
                Color32::from_rgb(13, 16, 29),
                t,
            ),
            w,
        );
        // Specular upper ridge + subtle amethyst dorsal vein.
        line(
            p,
            c,
            &[a + n * (w * 0.34), b + n * (w * 0.34)],
            fade(edge, (115.0 * (1.0 - t * 0.45)) as u8),
            1.1,
        );
        if i % 3 == 0 && i < tail_steps - 2 {
            let spine_h = 7.5 * (1.0 - t * 0.55) + 3.0;
            polygon(
                p,
                c,
                &[a - dir * 3.0, a + n * (w * 0.5 + spine_h), b + dir * 2.0],
                Color32::from_rgb(42, 35, 66),
                Stroke::new(0.6, fade(moonlit, 90)),
            );
        }
    }
    // Split cybernetic blade-fin at the tail tip.
    let tip = tail_pts[tail_steps];
    let tip_dir = (tail_pts[tail_steps] - tail_pts[tail_steps - 2]).normalized();
    let tip_n = Vec2::new(-tip_dir.y, tip_dir.x);
    polygon(
        p,
        c,
        &[
            tip - tip_dir * 8.0,
            tip + tip_n * 22.0 + tip_dir * 6.0,
            tip + tip_dir * 42.0,
            tip - tip_n * 18.0 + tip_dir * 10.0,
        ],
        Color32::from_rgb(31, 25, 51),
        Stroke::new(1.1, fade(moonlit, 155)),
    );
    line(
        p,
        c,
        &[tip - tip_dir * 4.0, tip + tip_dir * 38.0],
        fade(amethyst, (125.0 + pulse * 60.0) as u8),
        1.4,
    );

    // 3. Armored forelimbs & hooked obsidian talons gripping the sanctuary.
    for side in [-1.0_f32, 1.0] {
        let sx = |v: f32| 600.0 + side * v;
        polygon(
            p,
            c,
            &[
                Pos2::new(sx(102.0), 600.0 - lift * 0.2),
                Pos2::new(sx(196.0 + expand * 0.4), 642.0),
                Pos2::new(sx(214.0 + expand * 0.5), 694.0),
                Pos2::new(sx(144.0), 686.0),
            ],
            Color32::from_rgb(22, 23, 39),
            Stroke::new(1.1, fade(edge, 145)),
        );
        // Glowing shoulder conduit trim.
        line(
            p,
            c,
            &[
                Pos2::new(sx(118.0), 618.0),
                Pos2::new(sx(176.0), 648.0),
                Pos2::new(sx(190.0), 678.0),
            ],
            fade(amethyst, (75.0 + pulse * 45.0) as u8),
            1.2,
        );
        for n in 0..3 {
            let xx = sx(168.0 + n as f32 * 16.0);
            polygon(
                p,
                c,
                &[
                    Pos2::new(xx, 680.0),
                    Pos2::new(xx + side * 12.0, 710.0),
                    Pos2::new(xx - side * 8.0, 694.0),
                ],
                Color32::from_rgb(168, 156, 194),
                Stroke::new(0.6, Color32::from_rgb(42, 34, 62)),
            );
        }
    }

    // 4. Sculpted obsidian thoracic carapace with breathing rib-chevrons.
    polygon(
        p,
        c,
        &[
            Pos2::new(600.0, 528.0 - lift * 0.5),
            Pos2::new(738.0 + expand, 566.0 - lift * 0.3),
            Pos2::new(758.0 + expand, 646.0),
            Pos2::new(696.0 + expand * 0.5, 718.0),
            Pos2::new(600.0, 750.0),
            Pos2::new(504.0 - expand * 0.5, 718.0),
            Pos2::new(442.0 - expand, 646.0),
            Pos2::new(462.0 - expand, 566.0 - lift * 0.3),
        ],
        Color32::from_rgb(8, 11, 21),
        Stroke::new(2.0, Color32::from_rgb(92, 72, 128)),
    );
    for row in 0..5 {
        let y = 566.0 + row as f32 * 29.0 - lift * (0.45 - row as f32 * 0.08);
        let w = 122.0 + expand * 0.8 - row as f32 * 9.5;
        for side in [-1.0_f32, 1.0] {
            polygon(
                p,
                c,
                &[
                    Pos2::new(600.0 + side * 10.0, y),
                    Pos2::new(600.0 + side * w, y - 7.0),
                    Pos2::new(600.0 + side * (w - 15.0), y + 24.0),
                    Pos2::new(600.0 + side * 20.0, y + 34.0),
                ],
                blend(
                    Color32::from_rgb(20, 23, 38),
                    Color32::from_rgb(76, 52, 108),
                    row as f32 * 0.06 + pulse * 0.06,
                ),
                Stroke::new(0.85, fade(edge, 130)),
            );
            // Subtle amethyst bioluminescent seam between thoracic plates.
            if row < 4 {
                line(
                    p,
                    c,
                    &[
                        Pos2::new(600.0 + side * 56.0, y + 22.0),
                        Pos2::new(600.0 + side * (w - 18.0), y + 18.0),
                    ],
                    fade(amethyst, (42.0 + pulse * 48.0) as u8),
                    0.9,
                );
            }
        }
    }
}

fn neck(p: &egui::Painter, c: Canvas, pose: HeadPose, index: usize, input: &SceneInput<'_>) {
    let head = &input.heads[index];
    let accent = head.accent;
    let trim = secondary(index);
    let activity = safe_activity(head);
    let focused = input.selected == Some(index) || input.hovered == Some(index);
    let detail = match input.quality {
        SceneQuality::Low => 22,
        SceneQuality::Medium => 36,
        SceneQuality::High => 54,
    };
    let points: Vec<_> = (0..=detail)
        .map(|i| cubic(pose.neck, i as f32 / detail as f32))
        .collect();

    // Soft layered depth drop-shadow behind each neck.
    let shadow_pts: Vec<_> = points.iter().map(|v| *v + Vec2::new(0.0, 5.0)).collect();
    line(
        p,
        c,
        &shadow_pts,
        Color32::from_rgba_unmultiplied(1, 2, 7, 205),
        62.0,
    );

    let base_width = match index {
        3 => 33.0,
        0 | 6 => 24.5,
        _ => 27.5,
    };
    let side_dir = if index < 3 { -1.0 } else { 1.0 };

    for i in 0..detail {
        let t = i as f32 / detail as f32;
        let t_next = (i + 1) as f32 / detail as f32;
        let a = points[i];
        let b = points[i + 1];
        let tangent = (b - a).normalized();
        let n = Vec2::new(-tangent.y, tangent.x);
        // Smooth organic muscular taper from chest to cervical base.
        let taper = |u: f32| 1.0 - u * 0.34 + (u * PI).sin() * 0.05;
        let wa = base_width * taper(t);
        let wb = base_width * taper(t_next);

        // Core obsidian neck body.
        polygon(
            p,
            c,
            &[a + n * wa, b + n * wb, b - n * wb, a - n * wa],
            metal(accent),
            Stroke::NONE,
        );
        // Specular rim-lit dorsal flank.
        let lit_side = n * side_dir;
        polygon(
            p,
            c,
            &[
                a + lit_side * wa,
                b + lit_side * wb,
                b + lit_side * wb * 0.14,
                a + lit_side * wa * 0.14,
            ],
            blend(
                metal(accent),
                accent,
                0.14 + if focused { 0.08 } else { 0.0 },
            ),
            Stroke::NONE,
        );
        // Shadowed ventral underside.
        polygon(
            p,
            c,
            &[
                a - lit_side * wa,
                b - lit_side * wb,
                b - lit_side * wb * 0.42,
                a - lit_side * wa * 0.42,
            ],
            obsidian_dark(accent),
            Stroke::NONE,
        );
        // Crisp rim highlights & shadow outline.
        line(
            p,
            c,
            &[a + lit_side * wa, b + lit_side * wb],
            fade(trim, if focused { 165 } else { 105 }),
            1.1,
        );
        line(
            p,
            c,
            &[a - lit_side * wa, b - lit_side * wb],
            Color32::from_rgb(4, 6, 13),
            1.9,
        );

        // Segmented overlapping metallic dragon scales & ventral scutes.
        if i % 2 == 0 {
            let q = a.lerp(b, 0.5);
            let scale_len = if input.quality == SceneQuality::Low {
                9.5
            } else {
                7.2
            };
            for column in -1i32..=1 {
                let at = q + n * (column as f32 * wa * 0.50);
                let is_ventral = (column as f32 * side_dir) < -0.5;
                let fill = if is_ventral {
                    blend(Color32::from_rgb(24, 26, 40), trim, 0.14)
                } else if (i + column.unsigned_abs() as usize).is_multiple_of(4) {
                    blend(metal(accent), accent, 0.28)
                } else {
                    blend(metal(accent), Color32::WHITE, 0.06)
                };
                polygon(
                    p,
                    c,
                    &[
                        at - tangent * scale_len,
                        at + n * wa * 0.27,
                        at + tangent * (scale_len + 3.2),
                        at - n * wa * 0.27,
                    ],
                    fill,
                    Stroke::new(0.55, Color32::from_rgb(4, 7, 15)),
                );
                line(
                    p,
                    c,
                    &[at - tangent * scale_len, at + n * wa * 0.27],
                    fade(accent, if focused { 95 } else { 58 }),
                    0.6,
                );
            }
        }

        // Sweeping curved dorsal spine blades along the outer ridge.
        if i % 4 == 0 && i > 1 && i < detail - 2 {
            let at = a + lit_side * wa;
            let spine_reach = 15.0 + (1.0 - t) * 8.0;
            polygon(
                p,
                c,
                &[
                    at - tangent * 6.5,
                    at + lit_side * spine_reach - tangent * 10.5,
                    at + tangent * 6.5,
                ],
                blend(metal(accent), trim, 0.30),
                Stroke::new(0.65, fade(trim, 125)),
            );
        }
    }

    // Visible structural energy artery connecting the Amethyst Heart to this head.
    let heart_anchor = heart_pylon_point(index);
    let mut conduit = Vec::with_capacity(points.len() + 1);
    conduit.push(heart_anchor);
    for (i, v) in points.iter().copied().enumerate() {
        let zig = if i % 3 == 0 { 1.2 * side_dir } else { 0.0 };
        conduit.push(v + Vec2::new((index as f32 - 3.0) * 0.5 + zig, 0.0));
    }
    let amethyst = Color32::from_rgb(186, 122, 255);
    let pulse = heart_pulse(input.time, input.motion);
    let base_conduit_alpha = if focused {
        155
    } else {
        (72.0 + pulse * 38.0) as u8
    };
    // Soft luminous sheath around the heart-to-head neural conduit.
    line(
        p,
        c,
        &conduit,
        fade(blend(amethyst, accent, 0.45), base_conduit_alpha / 3),
        if focused { 5.2 } else { 3.6 },
    );
    line(
        p,
        c,
        &conduit,
        fade(blend(amethyst, accent, 0.60), base_conduit_alpha),
        if focused { 2.0 } else { 1.45 },
    );

    // Truthful operational energy: active surging plasma streams ONLY when activity > 0.
    if activity > 0.0 {
        line(
            p,
            c,
            &conduit,
            fade(accent, (65.0 + activity * 145.0).clamp(0.0, 235.0) as u8),
            2.3,
        );
        if input.motion && input.time.is_finite() {
            let t = input.time.rem_euclid(3600.0);
            for stream in 0..2 {
                let travel = (t * 0.34 + index as f32 * 0.13 + stream as f32 * 0.5).rem_euclid(1.0);
                let energy = cubic(pose.neck, travel);
                glow_ex(p, c, energy, 8.0, accent, 1.3 * activity, input.quality);
                p.circle_filled(
                    c.p(energy),
                    3.6 * c.scale,
                    blend(accent, Color32::WHITE, 0.45),
                );
                // Tapered luminous plasma tail trailing behind the surge packet.
                for tail_i in 1..=3 {
                    let back = (travel - tail_i as f32 * 0.032).clamp(0.0, 1.0);
                    let pt = cubic(pose.neck, back);
                    let a = ((150.0 - tail_i as f32 * 38.0) * activity).clamp(0.0, 200.0) as u8;
                    p.circle_filled(
                        c.p(pt),
                        (2.8 - tail_i as f32 * 0.55) * c.scale,
                        fade(accent, a),
                    );
                }
            }
        }
    }

    // Silver/lilac pips mean observed local-history changes, never order success.
    // Reduced motion removes travel; static labels retain truthfulness.
    if input.motion
        && input.time.is_finite()
        && head.observation.is_finite()
        && head.observation > 0.0
    {
        let strength = head.observation.clamp(0.0, 1.0);
        let travel = (input.time * 0.21 + index as f32 * 0.13).rem_euclid(1.0);
        let silver = Color32::from_rgb(186, 182, 214);
        for offset in [0.0, 0.03, 0.06] {
            let pip = cubic(pose.neck, (travel - offset).rem_euclid(1.0)) + Vec2::new(7.0, 0.0);
            p.circle_filled(
                c.p(pip),
                (2.1 - offset * 12.0) * c.scale,
                fade(silver, (strength * (155.0 - offset * 1100.0)) as u8),
            );
        }
    }
}

fn head(p: &egui::Painter, c: Canvas, pose: HeadPose, index: usize, input: &SceneInput<'_>) {
    let state = &input.heads[index];
    let accent = state.accent;
    let trim = secondary(index);
    let activity = safe_activity(state);
    let selected = input.selected == Some(index);
    let hovered = input.hovered == Some(index);
    let is_blocked = state.status == "BLOCKED";
    let t = if input.motion && input.time.is_finite() {
        input.time.rem_euclid(3600.0)
    } else {
        0.0
    };

    // Subtle attention elevation when focused or hovered.
    let focus_lift = if selected {
        -5.0
    } else if hovered {
        -3.0
    } else {
        0.0
    };
    let head_center = pose.center + Vec2::new(0.0, focus_lift);

    let transform = |x: f32, y: f32| {
        let (sn, cs) = pose.angle.sin_cos();
        head_center + Vec2::new(x * cs - y * sn, x * sn + y * cs)
    };
    let poly = |v: &[(f32, f32)], fill: Color32, s: Stroke| {
        polygon(
            p,
            c,
            &v.iter().map(|&(x, y)| transform(x, y)).collect::<Vec<_>>(),
            fill,
            s,
        )
    };
    let path = |v: &[(f32, f32)], color: Color32, w: f32| {
        line(
            p,
            c,
            &v.iter().map(|&(x, y)| transform(x, y)).collect::<Vec<_>>(),
            color,
            w,
        )
    };

    // 1. Focal selection / hover resonance halo.
    if selected || hovered {
        let halo_strength = if selected { 1.35 } else { 0.75 };
        glow_ex(
            p,
            c,
            head_center,
            if selected { 68.0 } else { 58.0 },
            accent,
            halo_strength,
            input.quality,
        );
        let ring_alpha = if selected { 165 } else { 85 };
        let rot = if input.motion { t * 0.25 } else { 0.0 };
        arc(
            p,
            c,
            head_center,
            78.0,
            78.0,
            0.22 + rot,
            PI - 0.22 + rot,
            fade(accent, ring_alpha),
            if selected { 1.4 } else { 0.9 },
        );
        arc(
            p,
            c,
            head_center,
            78.0,
            78.0,
            PI + 0.22 + rot,
            TAU - 0.22 + rot,
            fade(accent, ring_alpha),
            if selected { 1.4 } else { 0.9 },
        );
    }

    // 2. Behind-skull mythic signature auras (ORACLE astrolabe, AEGIS subtle/active shield, HUNTER radar).
    if index == 5 {
        // ORACLE: dual tilted concentric temporal astrolabe rings with lunar ticks.
        let spin = if input.motion { t * 0.18 } else { 0.0 };
        for &(ring, ry_ratio, alpha) in &[(74.0, 0.36, 115_u8), (88.0, 0.44, 72_u8)] {
            arc(
                p,
                c,
                head_center,
                ring,
                ring * ry_ratio,
                -PI * 0.92 + spin,
                PI * 0.68 + spin,
                fade(trim, alpha),
                1.05,
            );
        }
        // Orbiting lunar time-node.
        let orb_a = spin * 1.4 - 0.6;
        let orb = head_center + Vec2::new(orb_a.cos() * 74.0, orb_a.sin() * 26.6);
        p.circle_filled(
            c.p(orb),
            2.8 * c.scale,
            fade(Color32::from_rgb(235, 226, 255), 185),
        );
    }
    if index == 0 && activity > 0.0 {
        // HUNTER active radar scanning sweep arc.
        let sweep_a = if input.motion {
            -PI * 0.85 + (t * 1.4).sin() * 0.55
        } else {
            -PI * 0.65
        };
        arc(
            p,
            c,
            head_center,
            84.0,
            84.0,
            sweep_a - 0.35,
            sweep_a + 0.35,
            fade(accent, (150.0 * activity) as u8),
            2.2,
        );
    }

    // 3. Individualized dragon horns, sweeping cheek fins & rear jaw spurs.
    // Horn tips sweep proudly outward and stay gracefully below the floating nameplate.
    let (horn_out, horn_top, horn_mid_x) = match index {
        0 => (54.0, -96.0, 42.0),  // HUNTER: sleek swept raptor sensor horns
        1 => (62.0, -98.0, 48.0),  // FANG: fierce wide hooked predator horns
        2 => (48.0, -104.0, 38.0), // COGNITION: tall regal crown spires
        3 => (60.0, -98.0, 48.0),  // AEGIS: broad fortress bastion horns
        4 => (50.0, -92.0, 42.0),  // NEXUS: machined parallel conduit horns
        5 => (58.0, -104.0, 45.0), // ORACLE: graceful moonlit crescent horns
        _ => (52.0, -104.0, 40.0), // GENESIS: soaring prismatic spire horns
    };
    for side in [-1.0_f32, 1.0] {
        let sx = |x: f32| x * side;
        // Primary sculpted dragon horn.
        poly(
            &[
                (sx(17.0), -34.0),
                (sx(horn_mid_x), -58.0),
                (sx(horn_out), horn_top),
                (sx(24.0), -72.0),
                (sx(8.0), -44.0),
            ],
            blend(metal(accent), trim, 0.30),
            Stroke::new(1.05, fade(trim, 185)),
        );
        // Inner horn highlight ridge.
        path(
            &[
                (sx(19.0), -44.0),
                (sx(horn_mid_x * 0.88), -66.0),
                (sx(horn_out), horn_top),
            ],
            fade(trim, 210),
            1.2,
        );
        // Swept armored lateral cheek-fin blades.
        poly(
            &[
                (sx(28.0), -26.0),
                (sx(64.0), -50.0),
                (sx(56.0), -12.0),
                (sx(33.0), 14.0),
            ],
            Color32::from_rgb(18, 23, 36),
            Stroke::new(1.0, fade(accent, 140)),
        );
        // Secondary lower swept mandible spur.
        poly(
            &[
                (sx(34.0), -2.0),
                (sx(60.0), 9.0),
                (sx(38.0), 34.0),
                (sx(24.0), 24.0),
            ],
            blend(metal(accent), trim, 0.17),
            Stroke::new(0.85, fade(trim, 115)),
        );
        // Cybernetic jaw-hinge pivot node.
        p.circle_filled(
            c.p(transform(sx(31.0), 16.0)),
            4.8 * c.scale,
            Color32::from_rgb(6, 9, 18),
        );
        p.circle_stroke(
            c.p(transform(sx(31.0), 16.0)),
            4.8 * c.scale,
            Stroke::new(c.scale, fade(trim, 165)),
        );
        p.circle_filled(
            c.p(transform(sx(31.0), 16.0)),
            1.7 * c.scale,
            fade(accent, if activity > 0.0 || selected { 225 } else { 115 }),
        );
    }

    // 4. Fierce, angular tapered dragon mandible, narrow predator maw & razor fangs.
    let jaw_breath = if input.motion {
        (t * 0.78 + index as f32 * 1.1).sin().max(0.0) * 1.8 + activity * 4.2
    } else {
        activity * 2.5
    };
    poly(
        &[
            (-22.0, 20.0),
            (-17.0, 50.0 + jaw_breath),
            (-8.0, 66.0 + jaw_breath),
            (0.0, 74.0 + jaw_breath),
            (8.0, 66.0 + jaw_breath),
            (17.0, 50.0 + jaw_breath),
            (22.0, 20.0),
        ],
        blend(metal(accent), trim, 0.14),
        Stroke::new(1.25, fade(trim, 150)),
    );
    // Pointed armored chin crest on lower mandible.
    poly(
        &[
            (-7.0, 56.0 + jaw_breath),
            (0.0, 74.0 + jaw_breath),
            (7.0, 56.0 + jaw_breath),
            (0.0, 61.0 + jaw_breath),
        ],
        blend(metal(accent), trim, 0.28),
        Stroke::new(0.7, fade(trim, 140)),
    );
    // Fierce narrow dark maw slit (glows with energy fire when actively researching!).
    let maw_fill = if activity > 0.0 {
        blend(Color32::from_rgb(4, 5, 11), accent, activity * 0.52)
    } else {
        Color32::from_rgb(3, 4, 9)
    };
    poly(
        &[
            (-18.0, 32.0),
            (-13.0, 47.0 + jaw_breath),
            (0.0, 56.0 + jaw_breath),
            (13.0, 47.0 + jaw_breath),
            (18.0, 32.0),
            (0.0, 39.0),
        ],
        maw_fill,
        Stroke::NONE,
    );
    // Razor-sharp curved dragon canines framing the pointed snout.
    for side in [-1.0_f32, 1.0] {
        let canine_len = if index == 1 { 18.5 } else { 13.0 };
        let fang_col = if index == 1 {
            trim
        } else {
            Color32::from_rgb(218, 222, 234)
        };
        poly(
            &[
                (side * 12.5, 34.0),
                (side * 16.5, 34.0),
                (side * 13.8, 34.0 + canine_len + jaw_breath * 0.35),
            ],
            fang_col,
            Stroke::NONE,
        );
        poly(
            &[
                (side * 6.0, 38.0),
                (side * 9.2, 37.0),
                (side * 7.2, 38.0 + 8.5 + jaw_breath * 0.25),
            ],
            Color32::from_rgb(198, 204, 218),
            Stroke::NONE,
        );
    }

    // 5. Sculpted fierce diamond-spearhead dragon cranium & tapered armored snout.
    let breadth = match index {
        3 => 44.0,
        0 => 33.0,
        1 => 36.5,
        _ => 38.5,
    };
    poly(
        &[
            (0.0, -55.0),
            (-breadth, -31.0),
            (-breadth - 3.0, -5.0),
            (-26.0, 19.0),
            (-15.0, 39.0),
            (0.0, 51.0),
            (15.0, 39.0),
            (26.0, 19.0),
            (breadth + 3.0, -5.0),
            (breadth, -31.0),
        ],
        metal(accent),
        Stroke::new(1.5, fade(trim, 170)),
    );
    // Directional dual-plane cranial bevel shading for sharp 3D sculptural depth.
    poly(
        &[
            (0.0, -51.0),
            (-breadth + 3.0, -28.0),
            (-20.0, -5.0),
            (0.0, 10.0),
        ],
        blend(metal(accent), trim, 0.22),
        Stroke::NONE,
    );
    poly(
        &[
            (0.0, -51.0),
            (breadth - 3.0, -28.0),
            (20.0, -5.0),
            (0.0, 10.0),
        ],
        blend(metal(accent), accent, 0.14),
        Stroke::NONE,
    );
    // Fierce angular overhanging brow plates framing the eye sockets.
    for side in [-1.0_f32, 1.0] {
        poly(
            &[
                (side * 3.5, -14.0),
                (side * 29.0, -29.0),
                (side * 40.5, -14.0),
                (side * 27.5, -3.0),
                (side * 6.5, -1.0),
            ],
            blend(metal(accent), trim, 0.28),
            Stroke::new(0.85, fade(trim, 155)),
        );
    }
    // Elongated sculpted nasal ridge crest & armored diamond snout tip.
    poly(
        &[
            (0.0, -32.0),
            (-8.5, 9.0),
            (-13.5, 31.0),
            (0.0, 44.0),
            (13.5, 31.0),
            (8.5, 9.0),
        ],
        blend(metal(accent), trim, 0.19),
        Stroke::new(0.85, fade(accent, 125)),
    );
    poly(
        &[
            (-15.0, 23.0),
            (-20.0, 32.0),
            (-9.5, 43.0),
            (0.0, 49.0),
            (9.5, 43.0),
            (20.0, 32.0),
            (15.0, 23.0),
            (0.0, 30.0),
        ],
        blend(metal(accent), Color32::WHITE, 0.13),
        Stroke::new(0.85, fade(trim, 160)),
    );
    // Flared dragon nostrils & glowing cheek circuit vents.
    for side in [-1.0_f32, 1.0] {
        path(
            &[(side * 8.5, 31.0), (side * 14.0, 28.5), (side * 16.5, 32.0)],
            Color32::from_rgb(2, 4, 9),
            2.1,
        );
        if activity > 0.0 {
            p.circle_filled(
                c.p(transform(side * 12.5, 30.0)),
                1.8 * c.scale,
                fade(accent, (180.0 * activity) as u8),
            );
        }
        path(
            &[(side * 26.0, -1.0), (side * 30.0, 9.0), (side * 21.0, 19.0)],
            fade(accent, if activity > 0.0 || selected { 215 } else { 130 }),
            1.15,
        );
    }

    // 6. Living dragon eyes: smooth organic blinking, multi-layer ocular fire & slit pupils.
    let open = eye_openness(index, input.time, input.motion);
    let eye_col = if state.warning {
        Color32::from_rgb(255, 190, 82)
    } else if activity > 0.0 || selected {
        blend(accent, Color32::WHITE, 0.22 + activity * 0.2)
    } else {
        accent
    };
    let eye_glow_strength = 0.7 + activity * 1.1 + if selected || hovered { 0.45 } else { 0.0 };
    for side in [-1.0_f32, 1.0] {
        let eye_pos = transform(side * 21.0, -10.5);
        glow_ex(
            p,
            c,
            eye_pos,
            3.5 + activity * 3.0,
            eye_col,
            eye_glow_strength * open,
            input.quality,
        );
        // Dark socket rim.
        poly(
            &[
                (side * 9.0, -9.5),
                (side * 32.5, -17.5),
                (side * 29.0, -6.5),
                (side * 13.0, -4.5),
            ],
            Color32::from_rgb(4, 6, 12),
            Stroke::NONE,
        );
        // Luminous sculpted predatory eye aperture modulated by smooth eyelid closure.
        let top_y_inner = -10.5 + (1.0 - open) * 2.2;
        let top_y_outer = -16.2 + (1.0 - open) * 4.5;
        let bot_y_outer = -10.5 + 3.6 * open;
        let bot_y_inner = -8.2 + 2.6 * open;
        poly(
            &[
                (side * 10.5, top_y_inner),
                (side * 30.8, top_y_outer),
                (side * 27.6, bot_y_outer),
                (side * 14.0, bot_y_inner),
            ],
            eye_col,
            Stroke::NONE,
        );
        if open > 0.25 {
            // Vertical dragon slit pupil + subtle specular highlight glint.
            path(
                &[
                    (side * 21.2, top_y_outer * 0.65 + top_y_inner * 0.35),
                    (side * 20.0, bot_y_outer * 0.6 + bot_y_inner * 0.4),
                ],
                Color32::from_rgb(4, 5, 11),
                1.75,
            );
            p.circle_filled(
                c.p(transform(side * 17.2, -11.5)),
                1.25 * c.scale,
                fade(Color32::WHITE, (185.0 * open) as u8),
            );
        }
    }

    // 7. Distinct head-specific mythic-cybernetic crown & crest signatures.
    match index {
        0 => {
            // HUNTER: holographic targeting visor-crest & brow telemetry reticle.
            arc(
                p,
                c,
                transform(0.0, -33.0),
                24.0,
                12.0,
                PI * 1.06,
                TAU * 0.94,
                fade(accent, 210),
                1.2,
            );
            path(&[(-3.0, -43.0), (-3.0, -21.0), (3.0, -14.0)], accent, 1.1);
            for side in [-1.0_f32, 1.0] {
                path(
                    &[(side * 14.0, -34.0), (side * 24.0, -38.0)],
                    fade(trim, 190),
                    1.0,
                );
            }
        }
        1 => {
            // FANG: molten gold predator temple crests & oversized saber fangs.
            for side in [-1.0_f32, 1.0] {
                poly(
                    &[
                        (side * 26.0, -33.0),
                        (side * 50.0, -29.0),
                        (side * 30.0, -21.0),
                    ],
                    trim,
                    Stroke::NONE,
                );
                poly(
                    &[
                        (side * 17.5, 36.0),
                        (side * 24.0, 58.0),
                        (side * 11.5, 43.0),
                    ],
                    trim,
                    Stroke::new(0.5, Color32::from_rgb(95, 52, 18)),
                );
            }
            // Crimson-gold tactical forehead chevron.
            path(&[(-11.0, -34.0), (0.0, -23.0), (11.0, -34.0)], trim, 1.4);
        }
        2 => {
            // COGNITION: 3 levitating faceted amethyst crown shards + neural filigree + mind-eye.
            let bob = if input.motion {
                (t * 1.45).sin() * 2.6
            } else {
                0.0
            };
            for j in 0..3 {
                let x = (j as f32 - 1.0) * 12.5;
                let h = if j == 1 { 30.0 } else { 23.0 };
                let lift_j = bob * if j == 1 { 1.15 } else { 0.80 };
                poly(
                    &[
                        (x - 6.0, -43.0 - lift_j),
                        (x, -43.0 - h - lift_j),
                        (x + 6.0, -43.0 - lift_j),
                        (x, -32.0 - lift_j),
                    ],
                    blend(accent, Color32::WHITE, 0.32 + j as f32 * 0.08),
                    Stroke::new(0.85, fade(trim, 175)),
                );
            }
            path(
                &[
                    (-31.0, -22.0),
                    (-19.0, -32.0),
                    (-9.0, -25.0),
                    (0.0, -36.0),
                    (9.0, -25.0),
                    (19.0, -32.0),
                    (31.0, -22.0),
                ],
                fade(accent, 215),
                1.1,
            );
            // Diamond cognitive third-eye resonance crystal.
            poly(
                &[(0.0, -34.0), (-4.5, -26.0), (0.0, -18.0), (4.5, -26.0)],
                Color32::from_rgb(232, 206, 255),
                Stroke::new(0.6, accent),
            );
        }
        3 => {
            // AEGIS: Sovereign Guardian crested bastion helm & hexagonal protective energy barrier.
            poly(
                &[
                    (-32.0, -37.0),
                    (0.0, -65.0),
                    (32.0, -37.0),
                    (22.0, -24.0),
                    (0.0, -36.0),
                    (-22.0, -24.0),
                ],
                blend(metal(accent), trim, 0.38),
                Stroke::new(1.1, trim),
            );
            poly(
                &[(0.0, -54.0), (-6.0, -42.0), (0.0, -33.0), (6.0, -42.0)],
                accent,
                Stroke::new(0.7, trim),
            );
            // Hexagonal Aegis barrier: vivid when active, warned, blocked, Guardian/Suspended mode, or selected.
            let mode_shield = matches!(
                input.operational_mode,
                crate::treasury::OperationalMode::Guardian
                    | crate::treasury::OperationalMode::Suspended
            );
            if activity > 0.0 || state.warning || is_blocked || selected || mode_shield {
                let shield_col = if state.warning
                    || is_blocked
                    || input.operational_mode == crate::treasury::OperationalMode::Suspended
                {
                    Color32::from_rgb(255, 168, 78)
                } else if input.operational_mode == crate::treasury::OperationalMode::Guardian {
                    Color32::from_rgb(104, 245, 192)
                } else {
                    accent
                };
                let shield_alpha = if state.warning
                    || is_blocked
                    || input.operational_mode == crate::treasury::OperationalMode::Suspended
                {
                    195
                } else if mode_shield {
                    155
                } else if activity > 0.0 {
                    (110.0 + activity * 95.0) as u8
                } else {
                    70
                };
                let scale_r = if state.warning || is_blocked || mode_shield {
                    1.05
                } else {
                    1.0
                };
                let hex = [
                    transform(0.0, -92.0 * scale_r),
                    transform(74.0 * scale_r, -50.0 * scale_r),
                    transform(68.0 * scale_r, 34.0 * scale_r),
                    transform(0.0, 96.0 * scale_r),
                    transform(-68.0 * scale_r, 34.0 * scale_r),
                    transform(-74.0 * scale_r, -50.0 * scale_r),
                ];
                let closed: Vec<_> = hex.into_iter().chain([hex[0]]).collect();
                line(
                    p,
                    c,
                    &closed,
                    fade(shield_col, shield_alpha),
                    if state.warning || is_blocked || mode_shield {
                        2.0
                    } else {
                        1.4
                    },
                );
            }
        }
        4 => {
            // NEXUS: machined parallel conduit fins, precision spine bus & execution brackets.
            for side in [-1.0_f32, 1.0] {
                for row in 0..3 {
                    let yy = -31.0 + row as f32 * 10.5;
                    path(&[(side * 29.0, yy), (side * 44.0, yy + 3.5)], trim, 2.0);
                }
                poly(
                    &[
                        (side * 9.0, -41.0),
                        (side * 13.0, -68.0),
                        (side * 19.0, -41.0),
                    ],
                    trim,
                    Stroke::NONE,
                );
            }
            path(&[(0.0, -46.0), (0.0, -16.0), (6.0, -9.0)], accent, 1.6);
            path(&[(0.0, -16.0), (-6.0, -9.0)], fade(accent, 170), 1.2);
        }
        5 => {
            // ORACLE: luminous moonlit seer third eye on the forehead.
            poly(
                &[(0.0, -53.0), (-8.0, -37.0), (0.0, -26.0), (8.0, -37.0)],
                trim,
                Stroke::new(0.75, accent),
            );
            p.circle_filled(
                c.p(transform(0.0, -38.0)),
                3.2 * c.scale,
                Color32::from_rgb(245, 240, 255),
            );
            p.circle_stroke(
                c.p(transform(0.0, -38.0)),
                5.4 * c.scale,
                Stroke::new(c.scale, fade(accent, 180)),
            );
        }
        _ => {
            // GENESIS: radiant 5-spire prismatic evolutionary crown & orbiting pearl nodes when optimizing.
            for j in 0..5 {
                let offset = j as f32 - 2.0;
                let x = offset * 9.5;
                let h = 42.0 - offset.abs() * 7.5;
                poly(
                    &[
                        (x - 5.2, -41.0),
                        (x, -41.0 - h),
                        (x + 5.2, -41.0),
                        (x, -30.0),
                    ],
                    blend(accent, Color32::WHITE, 0.34 + (2.0 - offset.abs()) * 0.14),
                    Stroke::new(0.85, fade(trim, 165)),
                );
            }
            if activity > 0.0 && input.motion {
                for k in 0..3 {
                    let a = t * 1.1 + k as f32 * TAU / 3.0;
                    let orb = head_center + Vec2::new(a.cos() * 68.0, a.sin() * 44.0 - 20.0);
                    p.circle_filled(
                        c.p(orb),
                        2.6 * c.scale,
                        fade(trim, (190.0 * activity) as u8),
                    );
                }
            }
        }
    }

    // 8. High-contrast alert arc (always visible even under reduced motion).
    if state.warning {
        let warning = Color32::from_rgb(255, 181, 86);
        arc(
            p,
            c,
            head_center,
            71.0,
            75.0,
            0.08,
            PI - 0.08,
            fade(warning, 215),
            2.2,
        );
    }

    // 9. Refined obsidian-glass floating nameplate cleanly above the horn silhouette.
    let label = Pos2::new(head_center.x, head_center.y - 144.0);
    let badge_w = 118.0;
    let badge_h = 33.0;
    let badge_rect = Rect::from_center_size(
        c.p(label + Vec2::new(0.0, 7.0)),
        Vec2::new(badge_w * c.scale_x.min(c.scale * 1.15), badge_h * c.scale),
    );
    p.rect_filled(
        badge_rect,
        CornerRadius::same((5.0 * c.scale).clamp(2.0, 6.0) as u8),
        Color32::from_rgba_unmultiplied(7, 10, 19, 200),
    );
    p.rect_stroke(
        badge_rect,
        CornerRadius::same((5.0 * c.scale).clamp(2.0, 6.0) as u8),
        Stroke::new(
            (if selected { 1.35 } else { 0.85 }) * c.scale,
            fade(
                if state.warning {
                    Color32::from_rgb(248, 178, 86)
                } else {
                    accent
                },
                if selected || hovered { 195 } else { 88 },
            ),
        ),
        egui::StrokeKind::Inside,
    );
    // Subtle luminous top bar on the nameplate pill.
    p.line_segment(
        [
            Pos2::new(badge_rect.left() + 8.0 * c.scale, badge_rect.top() + 0.5),
            Pos2::new(badge_rect.right() - 8.0 * c.scale, badge_rect.top() + 0.5),
        ],
        Stroke::new(
            1.4 * c.scale,
            fade(
                if state.warning {
                    Color32::from_rgb(248, 178, 86)
                } else {
                    accent
                },
                if selected || hovered { 235 } else { 145 },
            ),
        ),
    );
    p.text(
        c.p(label),
        Align2::CENTER_CENTER,
        state.name,
        FontId::monospace((11.5 * c.scale).clamp(8.0, 15.0)),
        accent,
    );
    p.text(
        c.p(label + Vec2::new(0.0, 15.0)),
        Align2::CENTER_CENTER,
        state.status,
        FontId::monospace((8.2 * c.scale).clamp(7.0, 10.5)),
        if state.warning {
            Color32::from_rgb(252, 186, 98)
        } else if activity > 0.0 {
            blend(accent, Color32::WHITE, 0.35)
        } else {
            Color32::from_rgb(148, 160, 186)
        },
    );
}

fn heart_pylon_point(index: usize) -> Pos2 {
    let at = Pos2::new(600.0, 622.0);
    let angle = -PI * 0.88 + (index.min(6) as f32 / 6.0) * PI * 0.76;
    at + Vec2::new(angle.cos() * 75.0, angle.sin() * 75.0)
}

fn heart(p: &egui::Painter, c: Canvas, input: &SceneInput<'_>) {
    let at = Pos2::new(600.0, 622.0);
    let purple = Color32::from_rgb(179, 106, 250);
    let lavender = Color32::from_rgb(225, 192, 255);
    let silver = Color32::from_rgb(184, 170, 212);
    let pulse = heart_pulse(input.time, input.motion);
    let t = if input.motion && input.time.is_finite() {
        input.time.rem_euclid(3600.0)
    } else {
        0.0
    };
    let active_peak = input
        .heads
        .iter()
        .map(safe_activity)
        .fold(0.0_f32, f32::max);

    // Living physiological heartbeat aura.
    glow_ex(
        p,
        c,
        at,
        58.0 + pulse * 14.0,
        purple,
        1.15 + pulse * 0.55 + active_peak * 0.35,
        input.quality,
    );

    // Counter-rotating arcane-cybernetic containment rings.
    let rot_a = if input.motion { t * 0.16 } else { 0.0 };
    let rot_b = if input.motion { -t * 0.11 } else { 0.0 };
    for &(r, alpha) in &[(64.0_f32, 115_u8), (72.0, 88), (82.0, 42)] {
        p.circle_stroke(
            c.p(at),
            r * c.scale,
            Stroke::new(c.scale, fade(silver, alpha)),
        );
    }
    for j in 0..12 {
        let angle = j as f32 * TAU / 12.0 + rot_a;
        arc(
            p,
            c,
            at,
            68.0,
            68.0,
            angle + 0.05,
            angle + 0.30,
            fade(purple, (95.0 + pulse * 65.0) as u8),
            2.8,
        );
        let tick_a = j as f32 * TAU / 12.0 + rot_b;
        let v = Vec2::new(tick_a.cos(), tick_a.sin());
        line(
            p,
            c,
            &[at + v * 73.0, at + v * 80.0],
            fade(silver, 165),
            1.7,
        );
    }

    // Seven directional energy pylon sockets linking all 7 heads to the Amethyst Heart.
    for (i, head) in input.heads.iter().enumerate() {
        let pylon = heart_pylon_point(i);
        let act = safe_activity(head);
        let emph = input.selected == Some(i) || input.hovered == Some(i);
        let col = if head.warning {
            Color32::from_rgb(252, 184, 86)
        } else if act > 0.0 || emph {
            head.accent
        } else {
            blend(purple, head.accent, 0.45)
        };
        let r = if act > 0.0 || emph { 4.2 } else { 3.0 };
        if act > 0.0 || emph {
            glow_ex(p, c, pylon, 7.0, col, 0.8 + act, input.quality);
        }
        p.circle_filled(
            c.p(pylon),
            r * c.scale,
            fade(col, if act > 0.0 || emph { 235 } else { 155 }),
        );
        p.circle_stroke(
            c.p(pylon),
            (r + 1.6) * c.scale,
            Stroke::new(0.8 * c.scale, fade(silver, 130)),
        );
    }

    // Sculpted 8-edge faceted Royal Amethyst Heart Crystal with internal prismatic depth.
    let crystal = [
        Pos2::new(600.0, 563.0),
        Pos2::new(640.0, 588.0),
        Pos2::new(650.0, 629.0),
        Pos2::new(626.0, 667.0),
        Pos2::new(600.0, 684.0),
        Pos2::new(572.0, 662.0),
        Pos2::new(550.0, 624.0),
        Pos2::new(564.0, 585.0),
    ];
    polygon(
        p,
        c,
        &crystal,
        Color32::from_rgb(44, 21, 76),
        Stroke::new(1.6, Color32::from_rgb(208, 168, 252)),
    );

    // Subtle caustic drift of the inner crystal focus point.
    let sheen = if input.motion {
        Vec2::new((t * 0.72).cos() * 2.5, (t * 0.91).sin() * 2.0)
    } else {
        Vec2::ZERO
    };
    let inner = Pos2::new(597.0, 616.0) + sheen;
    let facet_palette = [
        Color32::from_rgb(206, 162, 248),
        Color32::from_rgb(106, 62, 162),
        Color32::from_rgb(62, 36, 108),
        Color32::from_rgb(132, 84, 188),
        Color32::from_rgb(58, 32, 106),
        Color32::from_rgb(116, 76, 172),
        Color32::from_rgb(196, 146, 244),
        Color32::from_rgb(154, 112, 214),
    ];
    for j in 0..8 {
        let base_col = facet_palette[j];
        let lit_col = blend(base_col, lavender, pulse * 0.24 + active_peak * 0.12);
        polygon(
            p,
            c,
            &[inner, crystal[j], crystal[(j + 1) % 8]],
            lit_col,
            Stroke::new(0.75, fade(lavender, 110)),
        );
    }
    // Inner prismatic table facet for gem-cut brilliance.
    let table: Vec<_> = crystal.iter().map(|v| inner.lerp(*v, 0.44)).collect();
    polygon(
        p,
        c,
        &table,
        fade(blend(purple, Color32::WHITE, 0.34 + pulse * 0.2), 125),
        Stroke::new(0.85, fade(Color32::from_rgb(245, 228, 255), 145)),
    );

    // Radiant living inner star-core + fine prismatic caustic cross.
    glow_ex(
        p,
        c,
        inner,
        14.0 + pulse * 6.0,
        Color32::from_rgb(236, 206, 255),
        1.25 + pulse * 0.75,
        input.quality,
    );
    let flare = 12.0 + pulse * 6.5;
    line(
        p,
        c,
        &[
            inner + Vec2::new(-flare, 0.0),
            inner + Vec2::new(flare, 0.0),
        ],
        fade(
            Color32::from_rgb(252, 240, 255),
            (145.0 + pulse * 85.0) as u8,
        ),
        1.1,
    );
    line(
        p,
        c,
        &[
            inner + Vec2::new(0.0, -flare),
            inner + Vec2::new(0.0, flare),
        ],
        fade(
            Color32::from_rgb(252, 240, 255),
            (145.0 + pulse * 85.0) as u8,
        ),
        1.1,
    );
    p.circle_filled(
        c.p(inner),
        (3.4 + pulse * 1.2) * c.scale,
        Color32::from_rgb(252, 242, 255),
    );

    // Heart title & truthful operational status line.
    p.text(
        c.xy(600.0, 709.0),
        Align2::CENTER_CENTER,
        "THE AMETHYST HEART",
        FontId::monospace((10.5 * c.scale).clamp(8.0, 13.5)),
        Color32::from_rgb(214, 194, 242),
    );
    p.text(
        c.xy(600.0, 728.0),
        Align2::CENTER_CENTER,
        input.heart_status,
        FontId::monospace((8.8 * c.scale).clamp(7.0, 11.5)),
        Color32::from_rgb(156, 170, 192),
    );

    // Permanent, reverent memorial sanctuary plaque for Ashlan Haley Rapponotti.
    let plaque_center = Pos2::new(600.0, 756.0);
    // Luminous mini amethyst diamond crest centered on the altar divider.
    polygon(
        p,
        c,
        &[
            plaque_center + Vec2::new(0.0, -5.5),
            plaque_center + Vec2::new(5.0, 0.0),
            plaque_center + Vec2::new(0.0, 5.5),
            plaque_center + Vec2::new(-5.0, 0.0),
        ],
        blend(purple, Color32::WHITE, 0.38 + pulse * 0.2),
        Stroke::new(0.8, fade(lavender, 195)),
    );
    for side in [-1.0_f32, 1.0] {
        line(
            p,
            c,
            &[
                plaque_center + Vec2::new(side * 12.0, 0.0),
                plaque_center + Vec2::new(side * 102.0, 0.0),
                plaque_center + Vec2::new(side * 114.0, -4.0),
            ],
            fade(silver, (90.0 + pulse * 45.0) as u8),
            0.9,
        );
    }
    p.text(
        c.xy(600.0, 780.0),
        Align2::CENTER_CENTER,
        "In Loving Memory of Ashlan Haley Rapponotti",
        FontId::proportional((13.5 * c.scale).clamp(9.5, 17.0)),
        Color32::from_rgb(216, 196, 240),
    );
    p.text(
        c.xy(600.0, 800.0),
        Align2::CENTER_CENTER,
        "June 21, 2003 – November 13, 2026",
        FontId::proportional((10.2 * c.scale).clamp(8.0, 13.0)),
        Color32::from_rgb(156, 148, 182),
    );
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneResponse {
    pub clicked: Option<usize>,
    pub hovered: Option<usize>,
}

/// Draw one connected native creature in the caller's region. Only supplied
/// positive activity creates operational energy; reduced motion freezes travel.
pub fn show_interactive(ui: &mut egui::Ui, input: &SceneInput<'_>) -> SceneResponse {
    let available = ui.available_size();
    let size = Vec2::new(available.x.max(1.0), available.y.max(1.0));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    let c = Canvas::new(rect);
    let poses: [HeadPose; 7] = std::array::from_fn(|i| head_pose(i, input.time, input.motion));

    // Hit-test first so hovering a head responds immediately on the same frame!
    let hits = head_hit_regions(rect, input.time, input.motion);
    let mut response_out = SceneResponse::default();
    for (i, hit) in hits.into_iter().enumerate() {
        let response = ui.interact(
            hit.intersect(rect),
            ui.id().with(("leviathan-head", i)),
            Sense::click(),
        );
        if response.hovered() {
            response_out.hovered = Some(i);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            response.clone().on_hover_text(format!(
                "{} · {}\nSelect this head to focus its workspace",
                input.heads[i].name, input.heads[i].status
            ));
        }
        if response.clicked() {
            response_out.clicked = Some(i);
        }
    }

    let effective_input = SceneInput {
        heads: input.heads,
        selected: input.selected,
        hovered: input.hovered.or(response_out.hovered),
        time: input.time,
        motion: input.motion,
        quality: input.quality,
        heart_status: input.heart_status,
        wgpu_atmosphere: input.wgpu_atmosphere,
        offering_glow: if input.offering_glow.is_finite() {
            input.offering_glow.clamp(0.0, 1.0)
        } else {
            0.0
        },
        operational_mode: input.operational_mode,
    };

    atmosphere(&p, rect, c, &poses, &effective_input);
    body(&p, c, &effective_input);
    // Outer necks first and guardian last preserve a coherent shared silhouette.
    for i in [0, 6, 1, 5, 2, 4, 3] {
        neck(&p, c, poses[i], i, &effective_input);
    }
    // Genuine Offering illumination: animates Treasury -> Amethyst Heart -> AEGIS & NEXUS.
    if effective_input.offering_glow > 0.0 {
        let g = effective_input.offering_glow;
        let heart_core = Pos2::new(600.0, 622.0);
        let gold = Color32::from_rgb(245, 194, 92);
        glow_ex(
            &p,
            c,
            heart_core,
            78.0 + g * 24.0,
            blend(Color32::from_rgb(184, 122, 255), gold, 0.45),
            1.4 * g,
            effective_input.quality,
        );
        for &(target_idx, beam_col) in &[
            (3usize, Color32::from_rgb(58, 232, 165)), // AEGIS (Protection)
            (4usize, Color32::from_rgb(73, 151, 255)), // NEXUS (Execution)
        ] {
            let curve = poses[target_idx].neck;
            let pts: Vec<_> = (0..=28).map(|k| cubic(curve, k as f32 / 28.0)).collect();
            line(
                &p,
                c,
                &pts,
                fade(beam_col, (145.0 * g).clamp(20.0, 220.0) as u8),
                2.6,
            );
            if effective_input.motion && effective_input.time.is_finite() {
                let u = (effective_input.time * 0.48 + target_idx as f32 * 0.25).rem_euclid(1.0);
                let spark = cubic(curve, u);
                glow_ex(
                    &p,
                    c,
                    spark,
                    9.0,
                    beam_col,
                    1.3 * g,
                    effective_input.quality,
                );
                p.circle_filled(c.p(spark), 3.8 * c.scale, blend(beam_col, gold, 0.4));
            }
        }
    }
    for i in [0, 6, 1, 5, 2, 4, 3] {
        head(&p, c, poses[i], i, &effective_input);
    }
    heart(&p, c, &effective_input);
    response_out
}

/// Draw one connected native creature and return any clicked head index.
pub fn show(ui: &mut egui::Ui, input: &SceneInput<'_>) -> Option<usize> {
    show_interactive(ui, input).clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    fn states() -> [HeadPresentation; 7] {
        std::array::from_fn(|_| HeadPresentation {
            name: "HEAD",
            status: "UNAVAILABLE",
            accent: Color32::LIGHT_BLUE,
            activity: 0.,
            observation: 0.,
            warning: false,
        })
    }
    #[test]
    fn seven_anatomical_necks_share_torso_and_reach_skulls() {
        for i in 0..7 {
            let pose = head_pose(i, 0., false);
            assert!((pose.neck[0].x - 600.).abs() <= 90.);
            assert_eq!(pose.neck[0].y, 615.);
            assert!((pose.neck[3].x - pose.center.x).abs() < 0.01);
            assert!(pose.neck[0].distance(pose.neck[3]) > 290.);
            for t in 0..=100 {
                let v = cubic(pose.neck, t as f32 / 100.);
                assert!(v.is_finite());
            }
        }
    }
    #[test]
    fn reduced_motion_is_exactly_time_invariant() {
        for i in 0..7 {
            assert_eq!(head_pose(i, 0., false), head_pose(i, 999., false));
            assert_eq!(head_pose(i, f32::NAN, false), head_pose(i, 0., false));
            assert_ne!(head_pose(i, 0., true), head_pose(i, 3., true));
            assert_eq!(eye_openness(i, 0., false), eye_openness(i, 12.3, false));
        }
        assert_eq!(breath_cycle(0., false), breath_cycle(42.0, false));
        assert_eq!(heart_pulse(0., false), heart_pulse(42.0, false));
    }
    #[test]
    fn organic_life_waves_remain_bounded_and_finite() {
        for step in 0..200 {
            let t = step as f32 * 0.07;
            let b = breath_cycle(t, true);
            let h = heart_pulse(t, true);
            assert!((0.0..=1.0).contains(&b));
            assert!((0.0..=1.0).contains(&h));
            for i in 0..7 {
                let e = eye_openness(i, t, true);
                assert!((0.05..=1.0).contains(&e));
            }
        }
        assert!(breath_cycle(f32::NAN, true).is_finite());
        assert!(heart_pulse(f32::INFINITY, true).is_finite());
        assert!(eye_openness(0, f32::NAN, true).is_finite());
    }
    #[test]
    fn responsive_hit_targets_are_distinct_and_contained() {
        for size in [
            Vec2::new(2560., 1440.),
            Vec2::new(1200., 820.),
            Vec2::new(640., 480.),
        ] {
            let rect = Rect::from_min_size(Pos2::ZERO, size);
            let hits = head_hit_regions(rect, 0., false);
            for i in 0..7 {
                assert!(rect.contains_rect(hits[i]));
                assert!(hits[i].width() > 40.);
                for j in i + 1..7 {
                    assert!(!hits[i].intersects(hits[j]));
                }
            }
        }
    }
    #[test]
    fn native_painter_tessellates_all_quality_and_motion_states() {
        let ctx = egui::Context::default();
        let mut heads = states();
        heads[3].warning = true;
        heads[0].activity = 0.8;
        for quality in [SceneQuality::Low, SceneQuality::Medium, SceneQuality::High] {
            for motion in [false, true] {
                let input = SceneInput {
                    heads: &heads,
                    selected: Some(3),
                    hovered: Some(0),
                    time: 3.,
                    motion,
                    quality,
                    heart_status: "UNAVAILABLE / RESEARCH ONLY",
                    wgpu_atmosphere: false,
                    offering_glow: 0.0,
                    operational_mode: crate::treasury::OperationalMode::Dormant,
                };
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 820.))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            assert_eq!(show(ui, &input), None);
                        });
                    },
                );
                assert!(output.shapes.len() > 300);
                let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
                assert!(!meshes.is_empty());
                for mesh in meshes {
                    if let egui::epaint::Primitive::Mesh(mesh) = mesh.primitive {
                        assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    fn heads() -> [HeadPresentation; 7] {
        std::array::from_fn(|_| HeadPresentation {
            name: "HEAD",
            status: "UNAVAILABLE",
            accent: Color32::LIGHT_BLUE,
            activity: 0.,
            observation: 0.,
            warning: false,
        })
    }
    fn draw(
        ctx: &egui::Context,
        heads: &[HeadPresentation; 7],
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Option<usize>) {
        draw_motion(ctx, heads, events, false)
    }
    fn draw_motion(
        ctx: &egui::Context,
        heads: &[HeadPresentation; 7],
        events: Vec<egui::Event>,
        motion: bool,
    ) -> (egui::FullOutput, Option<usize>) {
        let mut clicked = None;
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 820.))),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        clicked = show(
                            ui,
                            &SceneInput {
                                heads,
                                selected: None,
                                hovered: None,
                                time: 0.,
                                motion,
                                quality: SceneQuality::Low,
                                heart_status: "UNAVAILABLE",
                                wgpu_atmosphere: false,
                                offering_glow: 0.0,
                                operational_mode: crate::treasury::OperationalMode::Dormant,
                            },
                        );
                    });
            },
        );
        (out, clicked)
    }
    #[test]
    fn click_selects_the_dragon_head() {
        let ctx = egui::Context::default();
        let heads = heads();
        let at = Pos2::new(125., 300.);
        draw(&ctx, &heads, vec![]);
        draw(
            &ctx,
            &heads,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let (_, clicked) = draw(
            &ctx,
            &heads,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(clicked, Some(0));
    }
    #[test]
    fn operational_energy_requires_positive_finite_activity() {
        let ctx = egui::Context::default();
        let mut heads = heads();
        let (idle, _) = draw(&ctx, &heads, vec![]);
        heads[0].activity = f32::NAN;
        let (invalid, _) = draw(&ctx, &heads, vec![]);
        assert_eq!(idle.shapes.len(), invalid.shapes.len());
        heads[0].activity = 1.;
        let (active, _) = draw(&ctx, &heads, vec![]);
        assert!(active.shapes.len() > idle.shapes.len());
    }
    #[test]
    fn history_pips_are_distinct_evidence_gated_and_removed_by_reduced_motion() {
        let ctx = egui::Context::default();
        let mut heads = heads();
        let (idle, _) = draw_motion(&ctx, &heads, vec![], true);
        heads[0].observation = f32::NAN;
        let (invalid, _) = draw_motion(&ctx, &heads, vec![], true);
        assert_eq!(idle.shapes.len(), invalid.shapes.len());
        heads[0].observation = 1.;
        let (observed, _) = draw_motion(&ctx, &heads, vec![], true);
        assert_eq!(observed.shapes.len(), idle.shapes.len() + 3);
        let (reduced, _) = draw_motion(&ctx, &heads, vec![], false);
        heads[0].observation = 0.;
        let (reduced_idle, _) = draw_motion(&ctx, &heads, vec![], false);
        assert_eq!(reduced.shapes.len(), reduced_idle.shapes.len());
    }
    #[test]
    fn offering_illumination_is_gated_by_genuine_validation() {
        let ctx = egui::Context::default();
        let heads = heads();
        let render = |glow_val: f32| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200., 820.))),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            let _ = show(
                                ui,
                                &SceneInput {
                                    heads: &heads,
                                    selected: None,
                                    hovered: None,
                                    time: 1.5,
                                    motion: true,
                                    quality: SceneQuality::Low,
                                    heart_status: "DORMANT",
                                    wgpu_atmosphere: false,
                                    offering_glow: glow_val,
                                    operational_mode: crate::treasury::OperationalMode::Dormant,
                                },
                            );
                        });
                },
            )
            .shapes
            .len()
        };
        let idle = render(0.0);
        assert_eq!(idle, render(f32::NAN));
        assert!(render(1.0) > idle);
    }
}
