//! Native presentation selection, independent of engine and research execution.
//!
//! Pinned eframe/egui-wgpu 0.35 contracts are used here. The creature scene can
//! continue emitting egui meshes on either backend. A future WGPU-only WGSL layer
//! should use `egui_wgpu::Callback::new_paint_callback` / `CallbackTrait` and keep
//! pipelines in the renderer's callback resources, with prepare/paint work bounded
//! to the root canvas. Keep the mesh path for Glow and reduced-resource operation;
//! custom shaders, extra windows and engine coupling are not introduced here.

use clap::ValueEnum;
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Backend {
    /// Try WGPU when compiled in, then the existing Glow backend on startup failure.
    #[default]
    Auto,
    /// Native WGPU; an unavailable adapter or device is an explicit startup error.
    Wgpu,
    /// Existing OpenGL presentation. The driver may implement this in hardware or software.
    Glow,
    /// WGPU adapter explicitly reporting DeviceType::Cpu, if surface-compatible.
    Software,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Wgpu => "wgpu",
            Self::Glow => "glow",
            Self::Software => "software",
        }
    }
}

/// Ordered startup attempts, not evidence that any renderer is active. The caller
/// retries only failed initialization; closing a healthy application is not failure.
/// Explicit requests never silently switch to a different backend.
pub fn attempts(requested: Backend) -> Result<Vec<Backend>, String> {
    match requested {
        Backend::Auto => {
            #[cfg(feature = "desktop-wgpu")]
            {
                Ok(vec![Backend::Wgpu, Backend::Glow])
            }
            #[cfg(not(feature = "desktop-wgpu"))]
            {
                Ok(vec![Backend::Glow])
            }
        }
        Backend::Glow => Ok(vec![Backend::Glow]),
        Backend::Wgpu | Backend::Software => {
            #[cfg(feature = "desktop-wgpu")]
            {
                Ok(vec![requested])
            }
            #[cfg(not(feature = "desktop-wgpu"))]
            {
                Err(format!("{} requires a build with desktop-wgpu; use --renderer glow or a headless/terminal mode", requested.name()))
            }
        }
    }
}

/// Configure one root native window. Width/height are logical points; physical
/// pixels depend on OS DPI. Fullscreen fills the current monitor, while borderless
/// only removes decorations and remains a resizable window unless fullscreen too.
pub fn native_options(
    backend: Backend,
    fullscreen: bool,
    borderless: bool,
) -> Result<eframe::NativeOptions, String> {
    if backend == Backend::Auto {
        return Err(
            "resolve automatic renderer attempts before constructing native options".into(),
        );
    }
    attempts(backend)?;
    let viewport = egui::ViewportBuilder::default()
        .with_title("HYDRA — PROJECT LEVIATHAN")
        .with_inner_size([2400.0, 1320.0])
        .with_min_inner_size([960.0, 640.0])
        .with_resizable(true)
        .with_decorations(!borderless)
        .with_fullscreen(fullscreen);
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        run_and_return: true,
        centered: true,
        // The cockpit must not gain filesystem persistence merely from rendering.
        persist_window: false,
        ..Default::default()
    };
    options.glow_options.vsync = true;
    #[cfg(feature = "desktop-wgpu")]
    if matches!(backend, Backend::Wgpu | Backend::Software) {
        options.renderer = eframe::Renderer::Wgpu;
        let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
        // Explicit project preference; real adapter identity is observed at creation.
        setup.power_preference = eframe::wgpu::PowerPreference::HighPerformance;
        if backend == Backend::Software {
            setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, surface| {
                adapters.iter()
                    .filter(|adapter| adapter.get_info().device_type == eframe::wgpu::DeviceType::Cpu)
                    .find(|adapter| surface.is_none_or(|surface| adapter.is_surface_supported(surface)))
                    .cloned()
                    .ok_or_else(|| "No surface-compatible WGPU CPU adapter is available. Software rendering was not activated; try --renderer glow (driver-dependent) or a headless/terminal mode.".into())
            }));
        }
        options.wgpu_options = eframe::WgpuConfiguration {
            // AutoVsync allows a supported synchronized mode; two queued frames
            // favor smooth layered canvas throughput without forcing a GPU rate.
            surface: eframe::SurfaceConfig::HIGH_THROUGHPUT,
            wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(setup),
            ..Default::default()
        };
    }
    Ok(options)
}

/// Observed render state only: never report a requested or compiled-in backend as
/// active. Call during app creation, while eframe owns the current GL context.
pub fn renderer_description(cc: &eframe::CreationContext<'_>) -> String {
    #[cfg(feature = "desktop-wgpu")]
    if let Some(state) = &cc.wgpu_render_state {
        let info = state.adapter.get_info();
        return format!(
            "WGPU / {:?} · {} · {:?} · driver {} {}",
            info.backend,
            bounded_label(&info.name),
            info.device_type,
            bounded_label(&info.driver),
            bounded_label(&info.driver_info)
        );
    }
    if let Some(gl) = &cc.gl {
        use eframe::glow::HasContext;
        // SAFETY: eframe invokes the app creator with this GL context current;
        // these read-only string queries neither retain pointers nor mutate GL state.
        let (renderer, vendor, version) = unsafe {
            (
                gl.get_parameter_string(eframe::glow::RENDERER),
                gl.get_parameter_string(eframe::glow::VENDOR),
                gl.get_parameter_string(eframe::glow::VERSION),
            )
        };
        return format!(
            "Glow / OpenGL · {} · {} · {} · acceleration not classified",
            bounded_label(&renderer),
            bounded_label(&vendor),
            bounded_label(&version)
        );
    }
    "Renderer state unavailable; no active GPU or software backend verified".into()
}

fn bounded_label(label: &str) -> String {
    let result: String = label
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect();
    if result.is_empty() {
        "unknown".into()
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auto_order_is_compile_capability_not_a_runtime_success_claim() {
        #[cfg(feature = "desktop-wgpu")]
        assert_eq!(
            attempts(Backend::Auto).unwrap(),
            vec![Backend::Wgpu, Backend::Glow]
        );
        #[cfg(not(feature = "desktop-wgpu"))]
        assert_eq!(attempts(Backend::Auto).unwrap(), vec![Backend::Glow]);
        assert_eq!(attempts(Backend::Glow).unwrap(), vec![Backend::Glow]);
        assert!(native_options(Backend::Auto, false, false).is_err());
    }
    #[test]
    fn explicit_renderer_requests_do_not_silently_fallback() {
        for backend in [Backend::Wgpu, Backend::Software] {
            #[cfg(feature = "desktop-wgpu")]
            assert_eq!(attempts(backend).unwrap(), vec![backend]);
            #[cfg(not(feature = "desktop-wgpu"))]
            {
                assert!(attempts(backend).is_err());
                assert!(native_options(backend, false, false).is_err());
            }
        }
    }
    #[test]
    fn root_window_configuration_supports_modes_and_minimum_geometry() {
        let normal = native_options(Backend::Glow, false, false).unwrap();
        assert_eq!(normal.viewport.inner_size, Some(egui::vec2(2400.0, 1320.0)));
        assert_eq!(
            normal.viewport.min_inner_size,
            Some(egui::vec2(960.0, 640.0))
        );
        assert_eq!(normal.viewport.resizable, Some(true));
        assert_eq!(normal.viewport.decorations, Some(true));
        assert_eq!(normal.viewport.fullscreen, Some(false));
        assert!(normal.run_and_return && normal.glow_options.vsync);
        assert!(!normal.persist_window);
        let full = native_options(Backend::Glow, true, true).unwrap();
        assert_eq!(full.viewport.fullscreen, Some(true));
        assert_eq!(full.viewport.decorations, Some(false));
    }
    #[cfg(feature = "desktop-wgpu")]
    #[test]
    fn wgpu_modes_are_explicit_and_software_has_cpu_selector() {
        for backend in [Backend::Wgpu, Backend::Software] {
            let o = native_options(backend, false, false).unwrap();
            assert_eq!(o.renderer, eframe::Renderer::Wgpu);
            assert_eq!(
                o.wgpu_options.surface,
                eframe::SurfaceConfig::HIGH_THROUGHPUT
            );
            match o.wgpu_options.wgpu_setup {
                eframe::egui_wgpu::WgpuSetup::CreateNew(setup) => {
                    assert_eq!(
                        setup.power_preference,
                        eframe::wgpu::PowerPreference::HighPerformance
                    );
                    assert_eq!(
                        setup.native_adapter_selector.is_some(),
                        backend == Backend::Software
                    );
                    if let Some(selector) = setup.native_adapter_selector {
                        assert!(selector(&[], None).unwrap_err().contains("CPU adapter"));
                    }
                }
                _ => panic!("expected new native device setup"),
            }
        }
    }
    #[test]
    fn driver_labels_are_bounded_and_do_not_invent_unknown_information() {
        assert_eq!(bounded_label("\n\r\t"), "unknown");
        assert_eq!(bounded_label("GPU\nname"), "GPUname");
        assert_eq!(bounded_label(&"x".repeat(500)).len(), 256);
    }
}
