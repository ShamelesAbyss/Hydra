//! Original procedural seven-headed Leviathan. Decoration never invents telemetry.
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
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
    pub time: f32,
    pub motion: bool,
    pub quality: SceneQuality,
    pub heart_status: &'a str,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadPose {
    pub center: Pos2,
    pub angle: f32,
    pub neck: [Pos2; 4],
}
/// Logical creature geometry, independent of platform DPI. Idle motion is decorative.
pub fn head_pose(index: usize, time: f32, motion: bool) -> HeadPose {
    let i = index.min(6);
    let centers = [
        (125., 310.),
        (280., 235.),
        (440., 200.),
        (600., 180.),
        (760., 200.),
        (920., 235.),
        (1075., 310.),
    ];
    let t = if motion && time.is_finite() {
        time.rem_euclid(3600.)
    } else {
        0.
    };
    let phase = i as f32 * 1.73;
    let sway = if motion {
        (t * 0.47 + phase).sin() * 3.2
    } else {
        0.
    };
    let rise = if motion {
        (t * 0.63 + phase).sin() * 2.0
    } else {
        0.
    };
    let (x, y) = centers[i];
    let center = Pos2::new(x + sway, y + rise);
    let spread = i as f32 - 3.;
    HeadPose {
        center,
        angle: spread * 0.10
            + if motion {
                (t * 0.38 + phase).sin() * 0.018
            } else {
                0.
            },
        neck: [
            Pos2::new(600. + spread * 30., 615.),
            Pos2::new(600. + spread * 75., 470.),
            Pos2::new(x - spread * 8., y + 170.),
            Pos2::new(center.x, center.y + 48.),
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
        let scale = (rect.width() / 1200.).min(rect.height() / 820.).max(0.0001);
        let scale_x = (rect.width() / 1200.).min(scale * 1.35);
        Self {
            origin: Pos2::new(
                rect.center().x - 600. * scale_x,
                rect.center().y - 410. * scale,
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
            c.p(p.center + Vec2::new(0., -10.)),
            Vec2::new(115. * c.scale_x, 165. * c.scale),
        )
    })
}
fn cubic(p: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1. - t;
    Pos2::new(
        u * u * u * p[0].x + 3. * u * u * t * p[1].x + 3. * u * t * t * p[2].x + t * t * t * p[3].x,
        u * u * u * p[0].y + 3. * u * u * t * p[1].y + 3. * u * t * t * p[2].y + t * t * t * p[3].y,
    )
}
fn fade(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0., 1.);
    Color32::from_rgb(
        (a.r() as f32 * (1. - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1. - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1. - t) + b.b() as f32 * t) as u8,
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
fn glow(p: &egui::Painter, c: Canvas, at: Pos2, r: f32, color: Color32, quality: SceneQuality) {
    let layers = match quality {
        SceneQuality::Low => 1,
        SceneQuality::Medium => 3,
        SceneQuality::High => 5,
    };
    for layer in (1..=layers).rev() {
        p.circle_filled(
            c.p(at),
            (r + layer as f32 * 5.) * c.scale,
            fade(color, (14 / layer.max(1)) as u8),
        );
    }
}
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
    let pts: Vec<_> = (0..=40)
        .map(|i| {
            let a = start + (end - start) * i as f32 / 40.;
            center + Vec2::new(a.cos() * rx, a.sin() * ry)
        })
        .collect();
    line(p, c, &pts, color, width);
}
fn metal(accent: Color32) -> Color32 {
    blend(Color32::from_rgb(17, 24, 35), accent, 0.12)
}
fn secondary(index: usize) -> Color32 {
    match index {
        1 => Color32::from_rgb(239, 184, 77),
        2 => Color32::from_rgb(215, 180, 255),
        3 => Color32::from_rgb(180, 211, 202),
        4 => Color32::from_rgb(200, 220, 243),
        5 => Color32::from_rgb(180, 148, 242),
        6 => Color32::from_rgb(245, 225, 255),
        _ => Color32::from_rgb(117, 237, 255),
    }
}

fn atmosphere(p: &egui::Painter, c: Canvas, input: &SceneInput<'_>) {
    let purple = Color32::from_rgb(127, 73, 199);
    if input.quality != SceneQuality::Low {
        for j in (1..=7).rev() {
            p.circle_filled(
                c.xy(600., 455.),
                (135. + j as f32 * 37.) * c.scale,
                Color32::from_rgba_unmultiplied(52, 21, 79, 4),
            );
        }
        // Fixed astronomical dust is decoration, not market or network activity.
        for i in 0..64 {
            let x = 35. + ((i * 173 + 41) % 1130) as f32;
            let y = 70. + ((i * 137 + 7) % 625) as f32;
            let alpha = if i % 9 == 0 { 90 } else { 30 };
            p.circle_filled(
                c.xy(x, y),
                (if i % 9 == 0 { 1.1 } else { 0.65 }) * c.scale,
                fade(Color32::from_rgb(191, 176, 225), alpha),
            );
        }
    }
    for r in [215., 290., 365.] {
        arc(
            p,
            c,
            Pos2::new(600., 400.),
            r,
            r * 0.94,
            PI * 1.05,
            PI * 1.95,
            fade(purple, 27),
            0.8,
        );
    }
    for x in [52., 1148.] {
        line(
            p,
            c,
            &[
                Pos2::new(x, 670.),
                Pos2::new(x, 190.),
                Pos2::new(x + if x < 600. { 33. } else { -33. }, 140.),
            ],
            fade(purple, 35),
            1.,
        );
    }
    line(
        p,
        c,
        &[
            Pos2::new(320., 745.),
            Pos2::new(470., 745.),
            Pos2::new(500., 760.),
            Pos2::new(700., 760.),
            Pos2::new(730., 745.),
            Pos2::new(880., 745.),
        ],
        fade(purple, 65),
        1.,
    );
}
fn body(p: &egui::Painter, c: Canvas, _input: &SceneInput<'_>) {
    let edge = Color32::from_rgb(95, 79, 124);
    // Shared tail curls behind the chest, with an armored fin at its tip.
    let tail = [
        Pos2::new(625., 665.),
        Pos2::new(795., 780.),
        Pos2::new(1090., 755.),
        Pos2::new(1015., 634.),
    ];
    let tailpoints: Vec<_> = (0..=45).map(|i| cubic(tail, i as f32 / 45.)).collect();
    line(p, c, &tailpoints, Color32::from_rgb(3, 5, 11), 36.);
    for i in 0..45 {
        let t = i as f32 / 45.;
        let a = cubic(tail, t);
        let b = cubic(tail, (i + 1) as f32 / 45.);
        line(
            p,
            c,
            &[a, b],
            blend(
                Color32::from_rgb(34, 31, 49),
                Color32::from_rgb(16, 19, 31),
                t,
            ),
            25. * (1. - t) + 3.,
        );
        if i % 3 == 0 {
            line(
                p,
                c,
                &[
                    a + Vec2::new(-3., -8. * (1. - t)),
                    b + Vec2::new(-3., -8. * (1. - t)),
                ],
                fade(edge, 100),
                1.,
            );
        }
    }
    polygon(
        p,
        c,
        &[
            Pos2::new(1015., 640.),
            Pos2::new(994., 621.),
            Pos2::new(1023., 591.),
            Pos2::new(1028., 625.),
        ],
        Color32::from_rgb(29, 25, 43),
        Stroke::new(1., edge),
    );
    // Rear scapular fins and hooked foreclaws are continuous with the torso.
    for side in [-1., 1.] {
        let x = |v: f32| 600. + side * v;
        polygon(
            p,
            c,
            &[
                Pos2::new(x(72.), 574.),
                Pos2::new(x(220.), 498.),
                Pos2::new(x(183.), 572.),
                Pos2::new(x(244.), 607.),
                Pos2::new(x(154.), 625.),
                Pos2::new(x(190.), 671.),
                Pos2::new(x(85.), 651.),
            ],
            Color32::from_rgb(15, 19, 31),
            Stroke::new(1.4, edge),
        );
        polygon(
            p,
            c,
            &[
                Pos2::new(x(105.), 603.),
                Pos2::new(x(191.), 646.),
                Pos2::new(x(203.), 692.),
                Pos2::new(x(140.), 681.),
            ],
            Color32::from_rgb(27, 27, 42),
            Stroke::new(1., edge),
        );
        for n in 0..3 {
            let xx = x(166. + n as f32 * 15.);
            polygon(
                p,
                c,
                &[
                    Pos2::new(xx, 678.),
                    Pos2::new(xx + side * 11., 705.),
                    Pos2::new(xx - side * 9., 692.),
                ],
                Color32::from_rgb(147, 133, 166),
                Stroke::NONE,
            );
        }
    }
    polygon(
        p,
        c,
        &[
            Pos2::new(600., 536.),
            Pos2::new(730., 573.),
            Pos2::new(749., 644.),
            Pos2::new(690., 714.),
            Pos2::new(600., 749.),
            Pos2::new(510., 714.),
            Pos2::new(451., 644.),
            Pos2::new(470., 573.),
        ],
        Color32::from_rgb(8, 12, 22),
        Stroke::new(2., Color32::from_rgb(80, 64, 106)),
    );
    for row in 0..5 {
        let y = 574. + row as f32 * 29.;
        let w = 116. - row as f32 * 9.;
        for side in [-1., 1.] {
            polygon(
                p,
                c,
                &[
                    Pos2::new(600. + side * 8., y),
                    Pos2::new(600. + side * w, y - 6.),
                    Pos2::new(600. + side * (w - 14.), y + 25.),
                    Pos2::new(600. + side * 18., y + 35.),
                ],
                blend(
                    Color32::from_rgb(24, 27, 41),
                    Color32::from_rgb(70, 51, 93),
                    row as f32 * 0.05,
                ),
                Stroke::new(0.8, fade(edge, 110)),
            );
        }
    }
}
fn neck(p: &egui::Painter, c: Canvas, pose: HeadPose, index: usize, input: &SceneInput<'_>) {
    let head = &input.heads[index];
    let accent = head.accent;
    let detail = match input.quality {
        SceneQuality::Low => 22,
        SceneQuality::Medium => 36,
        SceneQuality::High => 52,
    };
    let points: Vec<_> = (0..=detail)
        .map(|i| cubic(pose.neck, i as f32 / detail as f32))
        .collect();
    line(
        p,
        c,
        &points,
        Color32::from_rgba_unmultiplied(0, 0, 0, 190),
        58.,
    );
    let width = if index == 3 {
        31.
    } else if index == 0 {
        23.
    } else {
        26.
    };
    for i in 0..detail {
        let t = i as f32 / detail as f32;
        let a = points[i];
        let b = points[i + 1];
        let tangent = (b - a).normalized();
        let n = Vec2::new(-tangent.y, tangent.x);
        let wa = width * (1. - t * 0.32);
        let wb = width * (1. - (t + 1. / detail as f32) * 0.32);
        polygon(
            p,
            c,
            &[a + n * wa, b + n * wb, b - n * wb, a - n * wa],
            metal(accent),
            Stroke::NONE,
        );
        polygon(
            p,
            c,
            &[a + n * wa, b + n * wb, b + n * wb * 0.12, a + n * wa * 0.12],
            blend(metal(accent), accent, 0.10),
            Stroke::NONE,
        );
        line(
            p,
            c,
            &[a + n * wa, b + n * wb],
            fade(secondary(index), 95),
            1.,
        );
        line(
            p,
            c,
            &[a - n * wa, b - n * wb],
            Color32::from_rgb(5, 8, 16),
            2.,
        );
        if i % 2 == 0 {
            let q = a.lerp(b, 0.5);
            let length = if input.quality == SceneQuality::Low {
                9.
            } else {
                7.
            };
            for column in -1i32..=1 {
                let at = q + n * (column as f32 * wa * 0.53);
                let fill = if (i + column.unsigned_abs() as usize) % 4 == 0 {
                    blend(metal(accent), accent, 0.23)
                } else {
                    blend(metal(accent), Color32::WHITE, 0.055)
                };
                polygon(
                    p,
                    c,
                    &[
                        at - tangent * length,
                        at + n * wa * 0.28,
                        at + tangent * (length + 3.),
                        at - n * wa * 0.28,
                    ],
                    fill,
                    Stroke::new(0.55, Color32::from_rgb(5, 9, 18)),
                );
                line(
                    p,
                    c,
                    &[at - tangent * length, at + n * wa * 0.28],
                    fade(accent, 60),
                    0.6,
                );
            }
        }
        if i % 4 == 0 {
            let side = if index < 3 { -1. } else { 1. };
            let at = a + n * wa * side;
            polygon(
                p,
                c,
                &[
                    at - tangent * 6.,
                    at + n * (14. + (1. - t) * 7.) * side - tangent * 9.,
                    at + tangent * 7.,
                ],
                blend(metal(accent), secondary(index), 0.26),
                Stroke::new(0.6, fade(secondary(index), 100)),
            );
        }
    }
    // Circuit spine is static when idle; travelling light requires actual supplied activity.
    let circuit: Vec<_> = points
        .iter()
        .copied()
        .enumerate()
        .map(|(i, v)| v + Vec2::new((index as f32 - 3.) * 0.6, 0.) + Vec2::new(0., (i % 2) as f32))
        .collect();
    line(p, c, &circuit, fade(accent, 45), 1.3);
    let activity = if head.activity.is_finite() {
        head.activity.clamp(0., 1.)
    } else {
        0.
    };
    if activity > 0. {
        line(p, c, &circuit, fade(accent, (activity * 115.) as u8), 2.0);
        if input.motion && input.time.is_finite() {
            let travel = (input.time * 0.28 + index as f32 * 0.11).rem_euclid(1.);
            let energy = cubic(pose.neck, travel);
            glow(p, c, energy, 7., accent, input.quality);
            p.circle_filled(c.p(energy), 3.5 * c.scale, accent);
        }
    }
    // Silver/lilac pips mean observed local-history changes, never order success.
    // Reduced motion removes travel; the parent retains truthful static labels.
    if input.motion
        && input.time.is_finite()
        && head.observation.is_finite()
        && head.observation > 0.
    {
        let strength = head.observation.clamp(0., 1.);
        let travel = (input.time * 0.19 + index as f32 * 0.13).rem_euclid(1.);
        let silver = Color32::from_rgb(175, 172, 201);
        for offset in [0., 0.028, 0.056] {
            let pip = cubic(pose.neck, (travel - offset).rem_euclid(1.)) + Vec2::new(6., 0.);
            p.circle_filled(
                c.p(pip),
                (1.9 - offset * 12.) * c.scale,
                fade(silver, (strength * (140. - offset * 1100.)) as u8),
            );
        }
    }
}
fn head(p: &egui::Painter, c: Canvas, pose: HeadPose, index: usize, input: &SceneInput<'_>) {
    let state = &input.heads[index];
    let accent = state.accent;
    let trim = secondary(index);
    let selected = input.selected == Some(index);
    let transform = |x: f32, y: f32| {
        let (sn, cs) = pose.angle.sin_cos();
        pose.center + Vec2::new(x * cs - y * sn, x * sn + y * cs)
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
    if selected {
        glow(p, c, pose.center, 66., accent, input.quality);
        arc(
            p,
            c,
            pose.center,
            76.,
            77.,
            0.18,
            PI - 0.18,
            fade(accent, 130),
            1.,
        );
        arc(
            p,
            c,
            pose.center,
            76.,
            77.,
            PI + 0.18,
            TAU - 0.18,
            fade(accent, 130),
            1.,
        );
    }
    if index == 5 {
        for ring in [71., 83.] {
            arc(
                p,
                c,
                pose.center,
                ring,
                ring * 0.39,
                -PI * 0.9,
                PI * 0.62,
                fade(trim, 95),
                1.,
            );
        }
    }
    // Swept paired horns form a true dragon silhouette; each phenotype varies.
    let horn = match index {
        0 => (-16., -96.),
        1 => (-35., -99.),
        2 => (-9., -114.),
        3 => (-39., -95.),
        4 => (-30., -90.),
        5 => (-17., -113.),
        _ => (-5., -111.),
    };
    for side in [-1., 1.] {
        let sx = |x: f32| x * side;
        poly(
            &[
                (sx(22.), -32.),
                (sx(39.), -55.),
                (sx(42. + horn.0 * 0.25), horn.1),
                (sx(22.), -73.),
                (sx(9.), -42.),
            ],
            blend(metal(accent), trim, 0.27),
            Stroke::new(1., fade(trim, 160)),
        );
        path(
            &[
                (sx(25.), -42.),
                (sx(31.), -65.),
                (sx(42. + horn.0 * 0.25), horn.1),
            ],
            fade(trim, 180),
            1.1,
        );
        // Secondary swept cheek fins, articulation hinges, and rear spurs.
        poly(
            &[
                (sx(31.), -23.),
                (sx(62.), -47.),
                (sx(53.), -9.),
                (sx(36.), 22.),
            ],
            Color32::from_rgb(24, 28, 39),
            Stroke::new(1., fade(accent, 115)),
        );
        poly(
            &[
                (sx(39.), -1.),
                (sx(62.), 8.),
                (sx(42.), 35.),
                (sx(27.), 28.),
            ],
            blend(metal(accent), trim, 0.13),
            Stroke::new(0.8, fade(trim, 90)),
        );
        p.circle_filled(
            c.p(transform(sx(35.), 20.)),
            6. * c.scale,
            Color32::from_rgb(7, 11, 20),
        );
        p.circle_stroke(
            c.p(transform(sx(35.), 20.)),
            6. * c.scale,
            Stroke::new(c.scale, fade(trim, 155)),
        );
    }
    let jaw_move = if input.motion && input.time.is_finite() {
        (input.time * 0.7 + index as f32).sin().max(0.) * 1.5
    } else {
        0.
    };
    // Lower mandible behind upper snout, with real open mouth / ivory teeth.
    poly(
        &[
            (-28., 20.),
            (-22., 54. + jaw_move),
            (0., 70. + jaw_move),
            (22., 54. + jaw_move),
            (28., 20.),
        ],
        blend(metal(accent), trim, 0.12),
        Stroke::new(1.3, fade(trim, 130)),
    );
    poly(
        &[
            (-26., 26.),
            (-21., 43. + jaw_move),
            (0., 53. + jaw_move),
            (21., 43. + jaw_move),
            (26., 26.),
        ],
        Color32::from_rgb(3, 4, 10),
        Stroke::NONE,
    );
    for i in -3..=3 {
        let x = i as f32 * 6.;
        poly(
            &[(x - 2.4, 33.), (x + 2.4, 33.), (x, 42. + jaw_move * 0.5)],
            Color32::from_rgb(187, 189, 192),
            Stroke::NONE,
        );
    }
    let breadth = if index == 3 {
        45.
    } else if index == 0 {
        33.
    } else {
        39.
    };
    poly(
        &[
            (0., -51.),
            (-breadth, -31.),
            (-breadth - 4., -1.),
            (-27., 24.),
            (-19., 39.),
            (0., 45.),
            (19., 39.),
            (27., 24.),
            (breadth + 4., -1.),
            (breadth, -31.),
        ],
        metal(accent),
        Stroke::new(1.6, fade(trim, 155)),
    );
    poly(
        &[(0., -48.), (-breadth + 3., -27.), (-22., -6.), (0., 7.)],
        blend(metal(accent), trim, 0.17),
        Stroke::NONE,
    );
    poly(
        &[(0., -48.), (breadth - 3., -27.), (22., -6.), (0., 7.)],
        blend(metal(accent), accent, 0.12),
        Stroke::NONE,
    );
    // Fierce overhanging brow, elongated nasal ridge, plated muzzle.
    for side in [-1., 1.] {
        poly(
            &[
                (side * 5., -14.),
                (side * 31., -26.),
                (side * 40., -12.),
                (side * 29., -4.),
                (side * 8., -2.),
            ],
            blend(metal(accent), trim, 0.25),
            Stroke::new(0.8, fade(trim, 130)),
        );
    }
    poly(
        &[
            (0., -27.),
            (-10., 9.),
            (-18., 27.),
            (0., 34.),
            (18., 27.),
            (10., 9.),
        ],
        blend(metal(accent), trim, 0.14),
        Stroke::new(0.8, fade(accent, 100)),
    );
    poly(
        &[
            (-19., 20.),
            (-26., 30.),
            (-13., 39.),
            (0., 42.),
            (13., 39.),
            (26., 30.),
            (19., 20.),
            (0., 26.),
        ],
        blend(metal(accent), Color32::WHITE, 0.11),
        Stroke::new(0.8, fade(trim, 140)),
    );
    for side in [-1., 1.] {
        path(
            &[(side * 10., 28.), (side * 16., 26.), (side * 19., 29.)],
            Color32::from_rgb(2, 4, 9),
            2.2,
        );
        path(
            &[(side * 27., -1.), (side * 31., 8.), (side * 23., 17.)],
            fade(accent, 150),
            1.1,
        );
    }
    // Individually phased blinking is anatomy decoration, never an operation cue.
    let blink = if input.motion && input.time.is_finite() {
        let phase = (input.time + index as f32 * 0.91).rem_euclid(7.7);
        if phase < 0.12 {
            0.14
        } else {
            1.
        }
    } else {
        1.
    };
    for side in [-1., 1.] {
        let eye = transform(side * 22., -10.);
        glow(p, c, eye, 3., accent, input.quality);
        poly(
            &[
                (side * 10., -10.),
                (side * 31., -16.),
                (side * 28., -9. + 3. * blink),
                (side * 14., -7. + 2. * blink),
            ],
            accent,
            Stroke::NONE,
        );
        path(
            &[(side * 21., -14.), (side * 20., -7.)],
            Color32::from_rgb(4, 6, 12),
            1.7,
        );
    }
    match index {
        0 => {
            arc(
                p,
                c,
                transform(0., -32.),
                22.,
                12.,
                PI * 1.06,
                TAU * 0.95,
                fade(accent, 190),
                1.,
            );
            path(&[(-3., -41.), (-3., -20.), (3., -14.)], accent, 1.);
        }
        1 => {
            for side in [-1., 1.] {
                poly(
                    &[(side * 28., -31.), (side * 48., -28.), (side * 31., -21.)],
                    trim,
                    Stroke::NONE,
                );
                poly(
                    &[(side * 19., 37.), (side * 24., 54.), (side * 13., 43.)],
                    trim,
                    Stroke::NONE,
                );
            }
        }
        2 | 6 => {
            let count = if index == 6 { 5 } else { 3 };
            for j in 0..count {
                let x = (j as f32 - (count - 1) as f32 / 2.) * 10.;
                let high = if index == 6 { 37. } else { 29. };
                poly(
                    &[(x - 6., -41.), (x, -high - 32.), (x + 6., -41.), (x, -31.)],
                    blend(accent, Color32::WHITE, 0.30 + j as f32 * 0.08),
                    Stroke::new(0.8, fade(trim, 150)),
                );
            }
            if index == 2 {
                path(
                    &[
                        (-30., -22.),
                        (-19., -31.),
                        (-9., -25.),
                        (0., -35.),
                        (9., -25.),
                        (19., -31.),
                        (30., -22.),
                    ],
                    fade(accent, 200),
                    1.,
                );
            }
        }
        3 => {
            poly(
                &[
                    (-30., -36.),
                    (0., -61.),
                    (30., -36.),
                    (21., -24.),
                    (0., -35.),
                    (-21., -24.),
                ],
                blend(metal(accent), trim, 0.36),
                Stroke::new(1., trim),
            );
            if state.activity.is_finite() && state.activity > 0. {
                let shield = [
                    transform(0., -90.),
                    transform(70., -52.),
                    transform(65., 33.),
                    transform(0., 100.),
                    transform(-65., 33.),
                    transform(-70., -52.),
                ];
                line(
                    p,
                    c,
                    &shield.into_iter().chain([shield[0]]).collect::<Vec<_>>(),
                    fade(accent, 110),
                    1.4,
                );
            }
        }
        4 => {
            for side in [-1., 1.] {
                for row in 0..3 {
                    let yy = -31. + row as f32 * 11.;
                    path(&[(side * 30., yy), (side * 43., yy + 4.)], trim, 2.);
                }
                poly(
                    &[(side * 9., -40.), (side * 13., -65.), (side * 19., -40.)],
                    trim,
                    Stroke::NONE,
                );
            }
            path(&[(0., -44.), (0., -17.), (6., -10.)], accent, 1.5);
        }
        5 => {
            poly(
                &[(0., -51.), (-7., -36.), (0., -27.), (7., -36.)],
                trim,
                Stroke::new(0.6, accent),
            );
            p.circle_stroke(
                c.p(transform(0., -37.)),
                3. * c.scale,
                Stroke::new(c.scale, accent),
            );
        }
        _ => {}
    }
    if state.warning {
        let warning = Color32::from_rgb(255, 181, 86);
        arc(
            p,
            c,
            pose.center,
            69.,
            74.,
            0.05,
            PI - 0.05,
            fade(warning, 185),
            2.,
        );
    }
    // Nameplates are geometrically outside horns and remain separate from skull art.
    let label = Pos2::new(pose.center.x, pose.center.y - 130.);
    p.text(
        c.p(label),
        Align2::CENTER_CENTER,
        state.name,
        FontId::monospace((11.5 * c.scale).clamp(8., 15.)),
        accent,
    );
    p.text(
        c.p(label + Vec2::new(0., 16.)),
        Align2::CENTER_CENTER,
        state.status,
        FontId::monospace((8.5 * c.scale).clamp(7., 11.)),
        if state.warning {
            Color32::from_rgb(244, 178, 100)
        } else {
            Color32::from_rgb(137, 151, 174)
        },
    );
}
fn heart(p: &egui::Painter, c: Canvas, input: &SceneInput<'_>) {
    let at = Pos2::new(600., 626.);
    let purple = Color32::from_rgb(175, 104, 245);
    let silver = Color32::from_rgb(168, 154, 192);
    let breath = if input.motion && input.time.is_finite() {
        (input.time * 0.75).sin() * 0.9
    } else {
        0.
    };
    glow(p, c, at, 57. + breath, purple, input.quality);
    for r in [64., 71.] {
        p.circle_stroke(c.p(at), r * c.scale, Stroke::new(c.scale, fade(silver, 95)));
    }
    for j in 0..12 {
        let angle = j as f32 * TAU / 12.;
        arc(
            p,
            c,
            at,
            68.,
            68.,
            angle + 0.04,
            angle + 0.31,
            fade(purple, 100),
            3.,
        );
        let v = Vec2::new(angle.cos(), angle.sin());
        line(p, c, &[at + v * 73., at + v * 79.], fade(silver, 160), 2.);
    }
    let crystal = [
        Pos2::new(600., 571.),
        Pos2::new(636., 594.),
        Pos2::new(647., 634.),
        Pos2::new(622., 669.),
        Pos2::new(600., 686.),
        Pos2::new(574., 660.),
        Pos2::new(553., 626.),
        Pos2::new(568., 591.),
    ];
    polygon(
        p,
        c,
        &crystal,
        Color32::from_rgb(51, 27, 84),
        Stroke::new(1.5, Color32::from_rgb(192, 152, 239)),
    );
    let inner = Pos2::new(596., 619.);
    for j in 0..8 {
        polygon(
            p,
            c,
            &[inner, crystal[j], crystal[(j + 1) % 8]],
            match j {
                0 => Color32::from_rgb(193, 150, 237),
                1 => Color32::from_rgb(98, 59, 146),
                2 => Color32::from_rgb(65, 41, 106),
                3 => Color32::from_rgb(121, 78, 170),
                4 => Color32::from_rgb(67, 39, 115),
                5 => Color32::from_rgb(109, 74, 159),
                6 => Color32::from_rgb(185, 135, 232),
                _ => Color32::from_rgb(139, 102, 194),
            },
            Stroke::new(0.7, fade(Color32::from_rgb(225, 198, 255), 90)),
        );
    }
    glow(
        p,
        c,
        inner,
        9.,
        Color32::from_rgb(222, 184, 255),
        input.quality,
    );
    p.circle_filled(c.p(inner), 2.5 * c.scale, Color32::from_rgb(241, 220, 255));
    let font = (10. * c.scale).clamp(8., 13.);
    p.text(
        c.xy(600., 711.),
        Align2::CENTER_CENTER,
        "THE AMETHYST HEART",
        FontId::monospace(font),
        Color32::from_rgb(204, 185, 231),
    );
    p.text(
        c.xy(600., 730.),
        Align2::CENTER_CENTER,
        input.heart_status,
        FontId::monospace((9. * c.scale).clamp(7., 12.)),
        Color32::from_rgb(153, 166, 185),
    );
    p.text(
        c.xy(600., 779.),
        Align2::CENTER_CENTER,
        "In Loving Memory of Ashlan Haley Rapponotti",
        FontId::proportional((13. * c.scale).clamp(9., 16.)),
        Color32::from_rgb(194, 175, 218),
    );
    p.text(
        c.xy(600., 799.),
        Align2::CENTER_CENTER,
        "June 21, 2003 – November 13, 2026",
        FontId::proportional((10. * c.scale).clamp(8., 13.)),
        Color32::from_rgb(138, 132, 158),
    );
}

/// Draw one connected native creature in the caller's region. Only supplied
/// positive activity creates operational energy; reduced motion freezes travel.
pub fn show(ui: &mut egui::Ui, input: &SceneInput<'_>) -> Option<usize> {
    let available = ui.available_size();
    let size = Vec2::new(available.x.max(1.), available.y.max(1.));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter_at(rect);
    let c = Canvas::new(rect);
    p.rect_filled(rect, 0., Color32::from_rgb(5, 8, 15));
    atmosphere(&p, c, input);
    body(&p, c, input);
    // Outer necks first and guardian last preserve a coherent shared silhouette.
    for i in [0, 6, 1, 5, 2, 4, 3] {
        neck(&p, c, head_pose(i, input.time, input.motion), i, input);
    }
    for i in [0, 6, 1, 5, 2, 4, 3] {
        head(&p, c, head_pose(i, input.time, input.motion), i, input);
    }
    heart(&p, c, input);
    let hits = head_hit_regions(rect, input.time, input.motion);
    let mut clicked = None;
    for (i, hit) in hits.into_iter().enumerate() {
        let response = ui.interact(
            hit.intersect(rect),
            ui.id().with(("leviathan-head", i)),
            Sense::click(),
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            response.clone().on_hover_text(format!(
                "{} · {}\nSelect this head to focus its workspace",
                input.heads[i].name, input.heads[i].status
            ));
        }
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
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
        }
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
                    time: 3.,
                    motion,
                    quality,
                    heart_status: "UNAVAILABLE / RESEARCH ONLY",
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
                                time: 0.,
                                motion,
                                quality: SceneQuality::Low,
                                heart_status: "UNAVAILABLE",
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
}
