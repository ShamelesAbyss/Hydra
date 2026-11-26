//! A single-window plan that preserves the creature while details are expanded.
use eframe::egui::{Pos2, Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnifiedLayout {
    pub bounds: Rect,
    pub command: Rect,
    pub canvas: Rect,
    pub left: Option<Rect>,
    pub right: Option<Rect>,
    pub details: Rect,
    pub footer: Rect,
    pub compact: bool,
}
impl UnifiedLayout {
    /// Peripheral regions collapse together below the wide-layout threshold.
    /// Their content belongs in inline details at compact sizes, not new windows.
    /// Even an excessive/nonfinite detail-height request cannot cover the canvas.
    pub fn for_rect(bounds: Rect, desired_details_height: f32) -> Self {
        let bounds = if bounds.min.x.is_finite()
            && bounds.min.y.is_finite()
            && bounds.max.x.is_finite()
            && bounds.max.y.is_finite()
            && bounds.width().is_finite()
            && bounds.height().is_finite()
            && bounds.width() >= 0.0
            && bounds.height() >= 0.0
        {
            bounds
        } else {
            Rect::from_min_size(Pos2::ZERO, Vec2::ZERO)
        };
        let w = bounds.width();
        let h = bounds.height();
        let command_h = (h * 0.14).min(94.0);
        let footer_h = (h * 0.06).min(40.0);
        let content_h = (h - command_h - footer_h).max(0.0);
        let gap = (content_h * 0.02).min(10.0);
        let desired = if desired_details_height.is_finite() {
            desired_details_height
        } else {
            content_h * 0.3
        };
        let details_h = desired.clamp(content_h * 0.18, content_h * 0.43);
        let top = bounds.top() + command_h;
        let footer_top = bounds.bottom() - footer_h;
        let details_top = footer_top - details_h;
        let canvas_bottom = (details_top - gap).max(top);
        let compact = w < 1800.0;
        let left_w = if compact {
            0.0
        } else {
            (w * 0.1).clamp(220.0, 270.0)
        };
        let right_w = if compact {
            0.0
        } else {
            (w * 0.115).clamp(260.0, 310.0)
        };
        let horizontal_gap = if compact { 0.0 } else { gap };
        let command = Rect::from_min_max(bounds.min, Pos2::new(bounds.right(), top));
        let footer = Rect::from_min_max(Pos2::new(bounds.left(), footer_top), bounds.max);
        let details = Rect::from_min_max(
            Pos2::new(bounds.left(), details_top),
            Pos2::new(bounds.right(), footer_top),
        );
        let canvas = Rect::from_min_max(
            Pos2::new(bounds.left() + left_w + horizontal_gap, top),
            Pos2::new(bounds.right() - right_w - horizontal_gap, canvas_bottom),
        );
        let left = (!compact).then(|| {
            Rect::from_min_max(
                Pos2::new(bounds.left(), top),
                Pos2::new(bounds.left() + left_w, canvas_bottom),
            )
        });
        let right = (!compact).then(|| {
            Rect::from_min_max(
                Pos2::new(bounds.right() - right_w, top),
                Pos2::new(bounds.right(), canvas_bottom),
            )
        });
        Self {
            bounds,
            command,
            canvas,
            left,
            right,
            details,
            footer,
            compact,
        }
    }
    /// Regions the application may fill; gaps remain intentional breathing room.
    pub fn regions(&self) -> Vec<Rect> {
        let mut regions = vec![self.command, self.canvas, self.details, self.footer];
        regions.extend(self.left);
        regions.extend(self.right);
        regions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn positive_overlap(a: Rect, b: Rect) -> bool {
        let overlap = a.intersect(b);
        overlap.width() > 0.01 && overlap.height() > 0.01
    }
    #[test]
    fn all_regions_remain_contained_and_nonoverlapping_across_real_sizes() {
        for (w, h) in [
            (960.0, 640.0),
            (1024.0, 768.0),
            (1280.0, 800.0),
            (1440.0, 900.0),
            (1920.0, 1080.0),
            (2560.0, 1440.0),
            (3840.0, 2160.0),
        ] {
            let bounds = Rect::from_min_size(Pos2::new(37.0, 19.0), Vec2::new(w, h));
            for requested in [-500.0, 180.0, 420.0, 100000.0, f32::INFINITY, f32::NAN] {
                let plan = UnifiedLayout::for_rect(bounds, requested);
                let regions = plan.regions();
                for (i, a) in regions.iter().enumerate() {
                    assert!(a.width() >= 0.0 && a.height() >= 0.0);
                    assert!(bounds.contains_rect(*a));
                    for b in &regions[i + 1..] {
                        assert!(!positive_overlap(*a, *b), "overlap {w} {h}: {a:?} {b:?}");
                    }
                }
                assert!(plan.canvas.width() >= w * 0.6);
                assert!(
                    plan.canvas.height() >= h * 0.4,
                    "creature disappeared under detail tray at {w}x{h}"
                );
            }
        }
    }
    #[test]
    fn compact_mode_reclaims_canvas_without_detached_regions() {
        let compact = UnifiedLayout::for_rect(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0)),
            300.0,
        );
        assert!(compact.compact && compact.left.is_none() && compact.right.is_none());
        assert_eq!(compact.canvas.width(), 1280.0);
        let wide = UnifiedLayout::for_rect(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(2560.0, 1440.0)),
            420.0,
        );
        assert!(!wide.compact && wide.left.is_some() && wide.right.is_some());
        assert!(wide.canvas.width() > 1800.0);
    }
    #[test]
    fn detail_growth_never_removes_central_creature() {
        let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(960.0, 640.0));
        let small = UnifiedLayout::for_rect(bounds, 180.0);
        let large = UnifiedLayout::for_rect(bounds, 100000.0);
        assert!(large.details.height() > small.details.height());
        assert!(large.canvas.height() > 250.0 && large.canvas.height() < small.canvas.height());
        assert_eq!(large.command, small.command);
        assert_eq!(large.footer, small.footer);
    }
    #[test]
    fn degenerate_or_nonfinite_input_cannot_emit_nonfinite_regions() {
        for bounds in [
            Rect::from_min_size(Pos2::ZERO, Vec2::ZERO),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(f32::NAN, 600.0)),
            Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::ZERO),
        ] {
            let layout = UnifiedLayout::for_rect(bounds, f32::NAN);
            for r in layout.regions() {
                assert!(
                    r.min.x.is_finite()
                        && r.min.y.is_finite()
                        && r.max.x.is_finite()
                        && r.max.y.is_finite()
                );
                assert!(r.width() >= 0.0 && r.height() >= 0.0);
            }
        }
    }
}
