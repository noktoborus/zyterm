//! How the window is put on a graphics chip.
//!
//! The window is drawn through `wgpu`, which reaches the chip through Vulkan,
//! DirectX or OpenGL, whichever the machine carries. Two things have to be
//! said explicitly, or a machine with a modest chip ends up on the processor
//! instead: the adapter has to be picked by hand, because neither
//! `PowerPreference::HighPerformance` nor any other hint rules a software
//! adapter out, and the limits asked of the device have to be cut down to what
//! the adapter actually offers, because the defaults of `wgpu` exceed what a
//! small chip such as the Raspberry Pi V3D grants and the device request then
//! fails outright.
//!
//! The texture sizes are the one limit asked for in full: the atlas the toolkit
//! draws its glyphs from grows with the fonts and the images of the interface,
//! so what the adapter grants is what is asked of it.
//!
//! Software rendering stays as the last resort — a machine that has nothing
//! else, a virtual one or a remote session, still gets its window — but it is
//! said in the log when it happens.

use std::sync::Arc;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
use eframe::wgpu;

/// The options the window is started with.
pub fn native_options(viewport: egui::ViewportBuilder) -> eframe::NativeOptions {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.power_preference = wgpu::PowerPreference::LowPower;
    setup.native_adapter_selector = Some(Arc::new(choose_adapter));
    setup.device_descriptor = Arc::new(|adapter| wgpu::DeviceDescriptor {
        label: Some(WINDOW_DEVICE),
        required_limits: limits(&adapter.limits()),
        ..Default::default()
    });

    eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(setup),
            ..WgpuConfiguration::default()
        },
        ..eframe::NativeOptions::default()
    }
}

/// Writes the adapter the window ended up on into the log.
pub fn log_adapter(state: &eframe::egui_wgpu::RenderState) {
    let info = state.adapter.get_info();
    log::info!(
        "adapter: {} [{:?}] type={:?} driver={} {}",
        info.name,
        info.backend,
        info.device_type,
        info.driver,
        info.driver_info
    );
    if is_software(&info) {
        log::warn!("the window is drawn by the processor, no graphics chip was usable");
    }
}

/// What is asked of the device: the defaults cut down to the adapter, and every
/// texture size raised to what the adapter grants.
///
/// The defaults of `wgpu` exceed what a small chip such as the Raspberry Pi V3D
/// offers, and a device request that asks for more than the adapter has fails
/// outright — hence the cut. The texture sizes go the other way: the atlas of
/// the toolkit grows with the glyphs of the fonts and the images of the
/// interface, and a limit of 8192 where the adapter grants 16384 is a limit
/// nothing asked for, so what the adapter offers is what is asked of it.
fn limits(offered: &wgpu::Limits) -> wgpu::Limits {
    let mut asked = wgpu::Limits::default().or_worse_values_from(offered);
    asked.max_texture_dimension_1d = offered.max_texture_dimension_1d;
    asked.max_texture_dimension_2d = offered.max_texture_dimension_2d;
    asked.max_texture_dimension_3d = offered.max_texture_dimension_3d;
    asked
}

/// Widest row the glyph atlas of the toolkit is cut into.
///
/// The device is asked for the largest texture it grants, because a texture that
/// may be created is worth having, and the atlas of the toolkit is not what that
/// is for: its rows are as wide as whatever the window reports and its height
/// doubles as glyphs arrive, so a row of sixteen thousand pixels makes every step
/// of that doubling cost four times what a row of four thousand would — in memory
/// here and in the same texture on the chip. Eight thousand is more than the
/// glyphs of two families at every size a terminal is read at.
pub const ATLAS_SIDE: usize = 8192;

/// The label the device carries in the messages of `wgpu`.
const WINDOW_DEVICE: &str = "window device";

/// Picks the adapter the window is drawn with, hardware before software.
fn choose_adapter(
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> std::result::Result<wgpu::Adapter, String> {
    let serves_window = |adapter: &&wgpu::Adapter| {
        surface.is_none_or(|surface| adapter.is_surface_supported(surface))
    };

    for adapter in adapters {
        let info = adapter.get_info();
        log::debug!(
            "adapter offered: {} [{:?}] type={:?} software={}",
            info.name,
            info.backend,
            info.device_type,
            is_software(&info)
        );
    }

    adapters
        .iter()
        .filter(serves_window)
        .find(|adapter| !is_software(&adapter.get_info()))
        .or_else(|| adapters.iter().find(serves_window))
        .cloned()
        .ok_or_else(|| "no adapter serves the window".to_owned())
}

/// True for an adapter that draws on the processor instead of a graphics chip.
fn is_software(info: &wgpu::AdapterInfo) -> bool {
    let name = info.name.to_ascii_lowercase();
    info.device_type == wgpu::DeviceType::Cpu
        || ["llvmpipe", "softpipe", "swrast", "lavapipe"]
            .iter()
            .any(|marker| name.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chip that grants more than the defaults of `wgpu` ask for is asked for
    /// all of it, and one that grants less is asked for what it has.
    #[test]
    fn the_texture_size_asked_for_is_the_one_the_adapter_grants() {
        let defaults = wgpu::Limits::default();

        let generous = wgpu::Limits {
            max_texture_dimension_2d: defaults.max_texture_dimension_2d * 2,
            ..defaults.clone()
        };
        assert_eq!(
            limits(&generous).max_texture_dimension_2d,
            generous.max_texture_dimension_2d
        );

        let modest = wgpu::Limits {
            max_texture_dimension_2d: 2048,
            ..defaults
        };
        assert_eq!(limits(&modest).max_texture_dimension_2d, 2048);
    }
}
