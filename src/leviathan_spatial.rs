//! Hybrid World-Space Camera-Facing Holographic Interface Manager for
//! **HYDRA: Project Synapse**.
//!
//! Design architecture:
//! - True 3D spatial anchors attach smoothly to animated skull bones or the central
//!   Amethyst Heart and persist optional operator repositioning in `%LOCALAPPDATA%\Hydra\spatial_prefs.json`.
//! - Every active workspace projects cleanly through `LeviathanCamera::project_point` with
//!   camera-basis orientation checks so controls remain upright, unmirrored, interactive,
//!   back-to-front depth ordered, and clear of the fixed safety controls and Ashlan memorial.
//! - Private credential fields stay strictly in native `egui` widgets (never serialized to
//!   persistent textures or telemetry).

use crate::leviathan_3d_math::{LeviathanCamera, Vec3};
use eframe::egui::{self, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresentationMode {
    #[default]
    Spatial,
    Classic,
}

impl PresentationMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Spatial => "SPATIAL",
            Self::Classic => "CLASSIC",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GlobalPanelKind {
    Overview,
    Treasury,
    Evolution,
    Positions,
    Timeline,
    Source,
    Controls,
}

impl GlobalPanelKind {
    pub const ALL: [Self; 7] = [
        Self::Overview,
        Self::Treasury,
        Self::Evolution,
        Self::Positions,
        Self::Timeline,
        Self::Source,
        Self::Controls,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Treasury => "Treasury",
            Self::Evolution => "Updates",
            Self::Positions => "Positions",
            Self::Timeline => "Timeline",
            Self::Source => "Source",
            Self::Controls => "Spatial",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Overview => "COMMAND OVERVIEW",
            Self::Treasury => "LEVIATHAN TREASURY",
            Self::Evolution => "LEVIATHAN EVOLUTION",
            Self::Positions => "PORTFOLIO / POSITIONS",
            Self::Timeline => "EVENT TIMELINE",
            Self::Source => "SOURCE / CONNECTION",
            Self::Controls => "SPATIAL & GRAPHICS CONTROLS",
        }
    }

    pub fn accent(self) -> Color32 {
        match self {
            Self::Overview => Color32::from_rgb(179, 116, 250),
            Self::Treasury => Color32::from_rgb(245, 194, 92),
            Self::Evolution => Color32::from_rgb(244, 127, 224),
            Self::Positions => Color32::from_rgb(58, 232, 165),
            Self::Timeline => Color32::from_rgb(54, 205, 245),
            Self::Source => Color32::from_rgb(246, 188, 83),
            Self::Controls => Color32::from_rgb(206, 168, 255),
        }
    }

    pub fn base_size(self) -> Vec2 {
        match self {
            Self::Treasury | Self::Evolution => Vec2::new(620.0, 440.0),
            Self::Overview => Vec2::new(560.0, 380.0),
            Self::Controls => Vec2::new(470.0, 400.0),
            Self::Positions | Self::Timeline | Self::Source => Vec2::new(520.0, 360.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpatialPanelId {
    Head(usize),
    Global(GlobalPanelKind),
}

impl SpatialPanelId {
    pub fn head_index(self) -> Option<usize> {
        match self {
            Self::Head(i) if i < 7 => Some(i),
            _ => None,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Head(0) => "HUNTER · SURVEILLANCE",
            Self::Head(1) => "FANG · STRATEGY",
            Self::Head(2) => "COGNITION · REGIME",
            Self::Head(3) => "AEGIS · RISK SHIELD",
            Self::Head(4) => "NEXUS · EXECUTION",
            Self::Head(5) => "ORACLE · VALIDATION",
            Self::Head(6) => "GENESIS · EVOLUTION",
            Self::Head(_) => "SUBSYSTEM",
            Self::Global(g) => g.title(),
        }
    }

    pub fn accent(self) -> Color32 {
        match self {
            Self::Head(0) => Color32::from_rgb(54, 205, 245),
            Self::Head(1) => Color32::from_rgb(247, 95, 105),
            Self::Head(2) => Color32::from_rgb(173, 133, 255),
            Self::Head(3) => Color32::from_rgb(58, 232, 165),
            Self::Head(4) => Color32::from_rgb(73, 151, 255),
            Self::Head(5) => Color32::from_rgb(208, 199, 247),
            Self::Head(6) => Color32::from_rgb(244, 127, 224),
            Self::Head(_) => Color32::from_rgb(179, 116, 250),
            Self::Global(g) => g.accent(),
        }
    }

    pub fn default_base_size(self) -> Vec2 {
        match self {
            Self::Head(_) => Vec2::new(595.0, 415.0),
            Self::Global(g) => g.base_size(),
        }
    }

    /// World-space offset relative to its anatomical anchor (skull for Heads, Heart for Globals).
    pub fn default_anchor_offset(self) -> Vec3 {
        match self {
            Self::Head(0) => Vec3::new(1.85, 0.25, 1.35),
            Self::Head(1) => Vec3::new(2.05, 0.05, 1.25),
            Self::Head(2) => Vec3::new(-2.05, -0.15, 1.30),
            Self::Head(3) => Vec3::new(2.25, -0.35, 1.20),
            Self::Head(4) => Vec3::new(2.05, -0.15, 1.30),
            Self::Head(5) => Vec3::new(-2.05, 0.05, 1.25),
            Self::Head(6) => Vec3::new(-1.85, 0.25, 1.35),
            Self::Head(_) => Vec3::new(0.0, 0.6, 1.4),
            Self::Global(GlobalPanelKind::Overview) => Vec3::new(0.0, 0.45, 2.25),
            Self::Global(GlobalPanelKind::Treasury) => Vec3::new(0.0, 0.55, 2.35),
            Self::Global(GlobalPanelKind::Evolution) => Vec3::new(1.35, 0.70, 2.15),
            Self::Global(GlobalPanelKind::Positions) => Vec3::new(-1.45, 0.45, 2.05),
            Self::Global(GlobalPanelKind::Timeline) => Vec3::new(1.45, 0.45, 2.05),
            Self::Global(GlobalPanelKind::Source) => Vec3::new(-1.25, 0.78, 1.95),
            Self::Global(GlobalPanelKind::Controls) => Vec3::new(2.35, 1.15, 1.65),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpatialPreferences {
    pub mode: PresentationMode,
    pub spatial_intensity: f32,
    pub panel_opacity: f32,
    pub attention_intensity: f32,
    pub body_responsiveness: f32,
    pub camera_sensitivity: f32,
    pub cinematic_orbit: bool,
    pub animation_intensity: f32,
    pub pinned_panels: Vec<SpatialPanelId>,
    pub panel_world_offsets: Vec<(SpatialPanelId, [f32; 3])>,
}

impl Default for SpatialPreferences {
    fn default() -> Self {
        Self {
            mode: PresentationMode::Spatial,
            spatial_intensity: 1.0,
            panel_opacity: 0.90,
            attention_intensity: 1.0,
            body_responsiveness: 1.0,
            camera_sensitivity: 1.0,
            cinematic_orbit: false,
            animation_intensity: 1.0,
            pinned_panels: Vec::new(),
            panel_world_offsets: Vec::new(),
        }
    }
}

impl SpatialPreferences {
    pub fn sanitize(&mut self) {
        let clamp_f = |v: &mut f32, lo: f32, hi: f32, def: f32| {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { def };
        };
        clamp_f(&mut self.spatial_intensity, 0.2, 1.5, 1.0);
        clamp_f(&mut self.panel_opacity, 0.55, 0.98, 0.90);
        clamp_f(&mut self.attention_intensity, 0.0, 1.5, 1.0);
        clamp_f(&mut self.body_responsiveness, 0.0, 1.5, 1.0);
        clamp_f(&mut self.camera_sensitivity, 0.3, 2.2, 1.0);
        clamp_f(&mut self.animation_intensity, 0.0, 1.5, 1.0);
        self.panel_world_offsets.retain(|(_, off)| {
            off[0].is_finite()
                && off[1].is_finite()
                && off[2].is_finite()
                && off[0].abs() <= 6.0
                && off[1].abs() <= 5.0
                && off[2].abs() <= 6.0
        });
    }

    pub fn prefs_path() -> Option<PathBuf> {
        crate::startup_diagnostics::app_data_root().map(|r| r.join("spatial_prefs.json"))
    }

    pub fn load_default() -> Self {
        let Some(path) = Self::prefs_path() else {
            return Self::default();
        };
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut parsed: Self = serde_json::from_str(&raw).unwrap_or_default();
        parsed.sanitize();
        parsed
    }

    pub fn save_default(&self) {
        let Some(path) = Self::prefs_path() else {
            return;
        };
        let mut copy = self.clone();
        copy.sanitize();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&copy) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpatialPanelRuntime {
    pub id: SpatialPanelId,
    pub origin_world: Vec3,
    pub user_offset: Vec3,
    pub open: bool,
    pub pinned: bool,
    pub collapsed: bool,
    pub expand_t: f32,
}

impl SpatialPanelRuntime {
    pub fn world_anchor(&self) -> Vec3 {
        self.origin_world + self.id.default_anchor_offset() + self.user_offset
    }
}

#[derive(Clone, Debug)]
pub struct ProjectedPanel {
    pub id: SpatialPanelId,
    pub world_anchor: Vec3,
    pub source_world: Vec3,
    pub rect: Rect,
    pub anchor_screen: Pos2,
    pub source_screen: Pos2,
    pub depth_z: f32,
    pub expand_t: f32,
    pub collapsed: bool,
    pub pinned: bool,
    pub behind_camera: bool,
}

pub struct SpatialWorkspaceManager {
    pub panels: Vec<SpatialPanelRuntime>,
    pub active: Option<SpatialPanelId>,
}

impl Default for SpatialWorkspaceManager {
    fn default() -> Self {
        Self::new(&SpatialPreferences::default())
    }
}

impl SpatialWorkspaceManager {
    pub fn new(prefs: &SpatialPreferences) -> Self {
        let mut ids = Vec::with_capacity(14);
        for i in 0..7 {
            ids.push(SpatialPanelId::Head(i));
        }
        for g in GlobalPanelKind::ALL {
            ids.push(SpatialPanelId::Global(g));
        }
        let mut panels = Vec::with_capacity(ids.len());
        for id in ids {
            let pinned = prefs.pinned_panels.contains(&id);
            let user_offset = prefs
                .panel_world_offsets
                .iter()
                .find(|(k, _)| *k == id)
                .map(|(_, o)| Vec3::new(o[0], o[1], o[2]))
                .unwrap_or(Vec3::ZERO);
            panels.push(SpatialPanelRuntime {
                id,
                origin_world: Vec3::ZERO,
                user_offset,
                open: pinned,
                pinned,
                collapsed: false,
                expand_t: if pinned { 1.0 } else { 0.0 },
            });
        }
        Self {
            panels,
            active: None,
        }
    }

    pub fn open_exclusive_or_pinned(&mut self, id: SpatialPanelId) {
        self.active = Some(id);
        for p in &mut self.panels {
            if p.id == id {
                p.open = true;
                p.collapsed = false;
            } else if !p.pinned {
                p.open = false;
            }
        }
    }

    pub fn close_panel(&mut self, id: SpatialPanelId) {
        for p in &mut self.panels {
            if p.id == id {
                p.open = false;
                p.pinned = false;
            }
        }
        if self.active == Some(id) {
            self.active = self.panels.iter().find(|p| p.open).map(|p| p.id);
        }
    }

    pub fn close_all_unpinned(&mut self) {
        for p in &mut self.panels {
            if !p.pinned {
                p.open = false;
            }
        }
        self.active = self.panels.iter().find(|p| p.open).map(|p| p.id);
    }

    pub fn reset_offsets(&mut self) {
        for p in &mut self.panels {
            p.user_offset = Vec3::ZERO;
            p.collapsed = false;
        }
    }

    pub fn sync_preferences(&self, prefs: &mut SpatialPreferences) {
        prefs.pinned_panels = self
            .panels
            .iter()
            .filter(|p| p.pinned)
            .map(|p| p.id)
            .collect();
        prefs.panel_world_offsets = self
            .panels
            .iter()
            .filter(|p| p.user_offset.length() > 1e-3)
            .map(|p| (p.id, p.user_offset.to_array()))
            .collect();
        prefs.sanitize();
    }

    pub fn nudge_world_offset(&mut self, id: SpatialPanelId, delta_world: Vec3) {
        if let Some(p) = self.panels.iter_mut().find(|p| p.id == id) {
            let next = p.user_offset + delta_world;
            p.user_offset = Vec3::new(
                next.x.clamp(-5.5, 5.5),
                next.y.clamp(-3.2, 4.0),
                next.z.clamp(-5.5, 5.5),
            );
        }
    }

    pub fn open_head_anchors(&self) -> [Option<Vec3>; 7] {
        let mut out = [None; 7];
        for p in &self.panels {
            if let Some(h) = p.id.head_index() {
                if p.open && !p.collapsed {
                    out[h] = Some(p.world_anchor());
                }
            }
        }
        out
    }

    /// Update smoothed world-anchor attachment and expansion states.
    pub fn step(
        &mut self,
        head_world: &[Vec3; 7],
        heart_world: Vec3,
        dt: f32,
        reduced_motion: bool,
    ) {
        let dt = dt.clamp(0.001, 0.08);
        let anchor_alpha = if reduced_motion {
            1.0
        } else {
            1.0 - (-dt * 4.5).exp()
        };
        let expand_alpha = if reduced_motion {
            1.0
        } else {
            1.0 - (-dt * 11.0).exp()
        };
        for p in &mut self.panels {
            let target_origin = match p.id.head_index() {
                Some(h) => head_world[h],
                None => heart_world,
            };
            if p.origin_world == Vec3::ZERO && target_origin != Vec3::ZERO {
                p.origin_world = target_origin;
            } else {
                p.origin_world = p.origin_world.lerp(target_origin, anchor_alpha);
            }
            let target_e = if p.open { 1.0 } else { 0.0 };
            p.expand_t += (target_e - p.expand_t) * expand_alpha;
        }
    }

    /// Project visible floating panels into camera-facing, back-to-front depth-ordered
    /// screen rectangles while preserving Ashlan's bottom memorial and top safety bar.
    pub fn project_visible(
        &self,
        stage_rect: Rect,
        camera: &LeviathanCamera,
        head_world: &[Vec3; 7],
        heart_world: Vec3,
    ) -> Vec<ProjectedPanel> {
        let mut out = Vec::new();
        let basis = camera.basis();
        for (idx, p) in self.panels.iter().enumerate() {
            if !p.open && p.expand_t < 0.04 {
                continue;
            }
            let source_world = match p.id.head_index() {
                Some(h) => head_world[h],
                None => heart_world,
            };
            let world_anchor = p.world_anchor();
            let raw_proj = camera.project_point(world_anchor, stage_rect);
            let behind_camera = raw_proj.map(|(_, _, z)| z < 0.0 || z > 1.0).unwrap_or(true);

            // Guarantee upright unmirrored fallback anchor in front of camera even during steep orbits
            let safe_world = if behind_camera {
                let side_x = if idx % 2 == 0 { 1.8 } else { -1.8 };
                camera.eye + basis.forward * 5.2 + basis.right * side_x + basis.up * 0.2
            } else {
                world_anchor
            };
            let (anchor_screen, _, depth_z) = camera
                .project_point(safe_world, stage_rect)
                .unwrap_or((stage_rect.center(), [0.5, 0.5], 0.5));
            let source_screen = camera
                .project_point(source_world, stage_rect)
                .map(|(pt, _, _)| pt)
                .unwrap_or_else(|| stage_rect.center());

            let dist = (safe_world - camera.eye).length().clamp(2.5, 14.0);
            let dist_scale = (7.8 / dist).clamp(0.76, 1.12);
            let mut size = p.id.default_base_size() * dist_scale;
            if p.collapsed {
                size = Vec2::new((size.x * 0.65).clamp(240.0, 320.0), 38.0);
            } else {
                size.x = size.x.clamp(340.0, (stage_rect.width() - 28.0).max(340.0));
                size.y = size
                    .y
                    .clamp(240.0, (stage_rect.height() - 120.0).max(240.0));
            }
            let scale_e = 0.88 + 0.12 * p.expand_t.clamp(0.0, 1.0);
            size *= scale_e;

            // Keep lower 96 px free for Ashlan Haley Rapponotti's permanent memorial & Heart status!
            let safe_rect = Rect::from_min_max(
                Pos2::new(stage_rect.left() + 12.0, stage_rect.top() + 44.0),
                Pos2::new(
                    stage_rect.right() - 12.0,
                    (stage_rect.bottom() - 96.0).max(stage_rect.top() + 160.0),
                ),
            );
            let min_x = (safe_rect.right() - size.x).max(safe_rect.left());
            let min_y = (safe_rect.bottom() - size.y).max(safe_rect.top());
            let tl = Pos2::new(
                (anchor_screen.x - size.x * 0.5).clamp(safe_rect.left(), min_x),
                (anchor_screen.y - size.y * 0.5).clamp(safe_rect.top(), min_y),
            );
            let rect = Rect::from_min_size(tl, size).intersect(safe_rect);

            out.push(ProjectedPanel {
                id: p.id,
                world_anchor: safe_world,
                source_world,
                rect,
                anchor_screen: rect.center(),
                source_screen,
                depth_z,
                expand_t: p.expand_t,
                collapsed: p.collapsed,
                pinned: p.pinned,
                behind_camera,
            });
        }

        // Back-to-front painter's order (largest depth first, active panel always on top)
        out.sort_by(|a, b| {
            let a_active = self.active == Some(a.id);
            let b_active = self.active == Some(b.id);
            a_active
                .cmp(&b_active)
                .then_with(|| b.depth_z.total_cmp(&a.depth_z))
        });

        // Gentle non-overlapping separation when multiple pinned panels coexist
        if out.len() > 1 {
            let safe_max_x = stage_rect.right() - 12.0;
            for i in 1..out.len() {
                let prev = out[i - 1].rect;
                let cur = out[i].rect;
                if prev.intersects(cur) && !out[i].collapsed {
                    let shift_x = if cur.center().x <= prev.center().x {
                        -(prev.width() * 0.38)
                    } else {
                        prev.width() * 0.38
                    };
                    let shifted_x = (cur.left() + shift_x).clamp(
                        stage_rect.left() + 12.0,
                        (safe_max_x - cur.width()).max(stage_rect.left() + 12.0),
                    );
                    out[i].rect = Rect::from_min_size(Pos2::new(shifted_x, cur.top()), cur.size());
                    out[i].anchor_screen = out[i].rect.center();
                }
            }
        }

        out
    }
}

/// Paint luminous holographic conduits & depth-ordered obsidian-glass frames behind open panels.
pub fn paint_spatial_conduits(
    p: &Painter,
    stage_rect: Rect,
    projected: &[ProjectedPanel],
    time: f32,
    intensity: f32,
    reduced_motion: bool,
) {
    let inten = intensity.clamp(0.2, 1.5);
    for pan in projected {
        let accent = pan.id.accent();
        let alpha_t = pan.expand_t.clamp(0.0, 1.0);
        if alpha_t <= 0.02 {
            continue;
        }
        let target_edge = if pan.source_screen.x <= pan.rect.center().x {
            Pos2::new(pan.rect.left(), pan.rect.center().y)
        } else {
            Pos2::new(pan.rect.right(), pan.rect.center().y)
        };
        let src = Pos2::new(
            pan.source_screen
                .x
                .clamp(stage_rect.left(), stage_rect.right()),
            pan.source_screen
                .y
                .clamp(stage_rect.top(), stage_rect.bottom()),
        );
        let ctrl = Pos2::new(
            (src.x + target_edge.x) * 0.5,
            (src.y + target_edge.y) * 0.5 - 32.0,
        );
        let steps = 18usize;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let u = 1.0 - t;
                Pos2::new(
                    u * u * src.x + 2.0 * u * t * ctrl.x + t * t * target_edge.x,
                    u * u * src.y + 2.0 * u * t * ctrl.y + t * t * target_edge.y,
                )
            })
            .collect();

        let outer_a = (46.0 * alpha_t * inten).clamp(0.0, 220.0) as u8;
        let core_a = (155.0 * alpha_t * inten).clamp(0.0, 240.0) as u8;
        p.add(egui::Shape::line(
            pts.clone(),
            Stroke::new(
                4.2,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), outer_a),
            ),
        ));
        p.add(egui::Shape::line(
            pts,
            Stroke::new(
                1.35,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), core_a),
            ),
        ));
        p.circle_stroke(
            src,
            6.5,
            Stroke::new(
                1.2,
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), core_a),
            ),
        );
        if !reduced_motion && time.is_finite() {
            let phase = (time * 0.55).rem_euclid(1.0);
            let u = 1.0 - phase;
            let pip = Pos2::new(
                u * u * src.x + 2.0 * u * phase * ctrl.x + phase * phase * target_edge.x,
                u * u * src.y + 2.0 * u * phase * ctrl.y + phase * phase * target_edge.y,
            );
            p.circle_filled(pip, 3.0, accent);
        }

        // Subtle dimensional backplate shimmer behind native egui::Area
        let r = pan.rect;
        p.rect_filled(
            r.expand(4.0),
            CornerRadius::same(8),
            Color32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                (14.0 * alpha_t * inten) as u8,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_preferences_json_roundtrip_and_sanitization() {
        let mut prefs = SpatialPreferences {
            mode: PresentationMode::Spatial,
            panel_opacity: 4.5, // out of bounds -> sanitized
            attention_intensity: f32::NAN,
            pinned_panels: vec![SpatialPanelId::Head(3)],
            panel_world_offsets: vec![(SpatialPanelId::Head(1), [1.25, -0.5, 0.75])],
            ..Default::default()
        };
        prefs.sanitize();
        assert!((prefs.panel_opacity - 0.98).abs() < 1e-6);
        assert!((prefs.attention_intensity - 1.0).abs() < 1e-6);
        let raw = serde_json::to_string(&prefs).expect("serialize spatial prefs");
        let parsed: SpatialPreferences = serde_json::from_str(&raw).expect("deserialize");
        assert_eq!(parsed, prefs);
    }

    #[test]
    fn projected_panels_stay_upright_readable_and_clear_of_memorial() {
        let mgr = {
            let mut m = SpatialWorkspaceManager::default();
            m.open_exclusive_or_pinned(SpatialPanelId::Head(1));
            m
        };
        let stage = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 760.0));
        let heads = [Vec3::ZERO; 7];
        // Normal front view & behind-camera steep view must both keep panel within safe stage bounds
        for eye in [
            Vec3::new(0.0, 0.34, 7.85),
            Vec3::new(0.0, 0.34, -7.85),
            Vec3::new(0.0, 6.0, 0.1),
        ] {
            let cam = LeviathanCamera {
                eye,
                target: Vec3::ZERO,
                fov_y_rad: 48.0_f32.to_radians(),
            };
            let proj = mgr.project_visible(stage, &cam, &heads, Vec3::ZERO);
            assert_eq!(proj.len(), 1);
            let r = proj[0].rect;
            assert!(r.width() >= 300.0 && r.height() >= 200.0);
            assert!(r.top() >= stage.top() + 40.0);
            assert!(
                r.bottom() <= stage.bottom() - 90.0,
                "panel occluded memorial zone: {}",
                r.bottom()
            );
        }
    }
}
