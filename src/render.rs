//! How the window is put on a graphics chip.
//!
//! The window is drawn through `wgpu`, which reaches the chip through one of
//! the graphics interfaces of the platform: Vulkan, Direct3D 12 or OpenGL.
//! Each interface is started on its own, in the order the settings give
//! (`Settings::graphics`), and the next one only when the one before found no
//! graphics chip. Starting an interface costs: Vulkan loads the driver of
//! every chip it knows of, and OpenGL loads the driver of the display — about a
//! tenth of a second each on a desktop — so an interface the window does not
//! draw through is not started at all.
//!
//! Two more things have to be said explicitly, or a machine with a modest chip
//! ends up on the processor instead: the adapter has to be picked by hand,
//! because no hint of `wgpu` rules a software adapter out, and the limits asked
//! of the device have to be cut down to what the adapter actually offers,
//! because the defaults of `wgpu` exceed what a small chip such as the
//! Raspberry Pi V3D grants and the device request then fails outright.
//!
//! The texture sizes are the one limit asked for in full: the atlas the toolkit
//! draws its glyphs from grows with the fonts and the images of the interface,
//! so what the adapter grants is what is asked of it.
//!
//! Software rendering stays as the last resort — a machine that has nothing
//! else, a virtual one or a remote session, still gets its window — but it is
//! said in the log when it happens. It is the first software adapter of the
//! order.

use std::sync::Arc;

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew, WgpuSetupExisting};
use eframe::wgpu;
use serde::{Deserialize, Serialize};

/// A graphics interface the window can be drawn through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsApi {
    /// Vulkan.
    Vulkan,
    /// OpenGL, or OpenGL ES through EGL.
    Opengl,
    /// Direct3D 12, on Windows only.
    Dx12,
}

impl GraphicsApi {
    /// The backends of `wgpu` that speak this interface.
    fn backends(self) -> wgpu::Backends {
        match self {
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Opengl => wgpu::Backends::GL,
            Self::Dx12 => wgpu::Backends::DX12,
        }
    }
}

/// The order the interfaces are tried in when the settings say nothing:
/// Vulkan first, because it is what a chip of today is best driven through,
/// Direct3D 12 next on Windows, OpenGL last.
pub fn default_graphics() -> Vec<GraphicsApi> {
    let mut order = vec![GraphicsApi::Vulkan];
    if cfg!(target_os = "windows") {
        order.push(GraphicsApi::Dx12);
    }
    order.push(GraphicsApi::Opengl);
    order
}

/// The options the window is started with, drawn through the first interface
/// of `order` that has a graphics chip.
///
/// `WGPU_BACKEND` in the environment stands in for the order, as `wgpu` reads
/// it.
pub fn native_options(
    viewport: egui::ViewportBuilder,
    order: &[GraphicsApi],
) -> eframe::NativeOptions {
    let order: Vec<wgpu::Backends> = match wgpu::Backends::from_env() {
        Some(backends) => vec![backends],
        None => order.iter().map(|api| api.backends()).collect(),
    };
    let wgpu_setup = match open_before_window(&order) {
        Some(existing) => WgpuSetup::Existing(existing),
        None => WgpuSetup::CreateNew(open_with_window(order)),
    };

    eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: WgpuConfiguration {
            wgpu_setup,
            ..WgpuConfiguration::default()
        },
        ..eframe::NativeOptions::default()
    }
}

/// A device on a graphics chip, opened before the window through the
/// interfaces at the head of the order that need no display: Vulkan and
/// Direct3D 12, one after another.
///
/// OpenGL needs the display of the window, which `eframe` makes, so the walk
/// stops at it and the rest is left to `open_with_window`.
fn open_before_window(order: &[wgpu::Backends]) -> Option<WgpuSetupExisting> {
    let without_display = order
        .iter()
        .take_while(|backends| !backends.contains(wgpu::Backends::GL));
    for &backends in without_display {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::from_build_config().with_env(),
            backend_options: wgpu::BackendOptions::from_env_or_default(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = pollster::block_on(instance.enumerate_adapters(backends));
        log_offered(&adapters);
        let chip = adapters
            .into_iter()
            .find(|adapter| !is_software(&adapter.get_info()));
        if let Some(adapter) = chip
            && let Some(existing) = device(instance, adapter)
        {
            return Some(existing);
        }
        log::info!("{backends:?}: no graphics chip, the next interface is tried");
    }
    None
}

/// The setup `eframe` opens with the window: every interface of the order
/// started at once, with the display of the window, and the adapter picked by
/// the order.
///
/// It is reached when OpenGL is next in the order or no interface before it
/// had a chip. An interface after OpenGL is started together with it, since
/// whether OpenGL has a chip is known only once it is started.
fn open_with_window(order: Vec<wgpu::Backends>) -> WgpuSetupCreateNew {
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = order.iter().copied().collect();
    setup.native_adapter_selector = Some(Arc::new(move |adapters, surface| {
        log_offered(adapters);
        choose_adapter(&order, adapters, surface)
    }));
    setup.device_descriptor = Arc::new(device_descriptor);
    setup
}

/// Picks the adapter the window is drawn with: a graphics chip of the first
/// interface of the order that has one, and a software adapter by the same
/// order when none has.
fn choose_adapter(
    order: &[wgpu::Backends],
    adapters: &[wgpu::Adapter],
    surface: Option<&wgpu::Surface<'_>>,
) -> std::result::Result<wgpu::Adapter, String> {
    let first_by_order = |software: bool| {
        order.iter().find_map(|&backends| {
            adapters.iter().find(|adapter| {
                let info = adapter.get_info();
                backends.contains(info.backend.into())
                    && is_software(&info) == software
                    && surface.is_none_or(|surface| adapter.is_surface_supported(surface))
            })
        })
    };

    first_by_order(false)
        .or_else(|| first_by_order(true))
        .cloned()
        .ok_or_else(|| "no adapter serves the window".to_owned())
}

/// Writes the adapters an interface offered into the log.
fn log_offered(adapters: &[wgpu::Adapter]) {
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
}

/// The device and queue of an adapter, or a line in the log when the adapter
/// grants none.
fn device(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Option<WgpuSetupExisting> {
    let requested = pollster::block_on(adapter.request_device(&device_descriptor(&adapter)));
    match requested {
        Ok((device, queue)) => Some(WgpuSetupExisting {
            instance,
            adapter,
            device,
            queue,
        }),
        Err(error) => {
            log::warn!("{}: no device: {error}", adapter.get_info().name);
            None
        }
    }
}

/// What is asked of the device of an adapter.
fn device_descriptor(adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    wgpu::DeviceDescriptor {
        label: Some(WINDOW_DEVICE),
        required_limits: limits(&adapter.limits()),
        ..Default::default()
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

    /// The adapter is the first of the order: the container has no graphics
    /// chip, only the software adapters of Vulkan and of OpenGL, so the order
    /// alone says which of the two the window is drawn on.
    #[test]
    fn the_adapter_is_the_first_of_the_order() {
        let all = wgpu::Backends::VULKAN | wgpu::Backends::GL;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: all,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = pollster::block_on(instance.enumerate_adapters(all));
        let backend_of = |order: &[GraphicsApi]| {
            let order: Vec<wgpu::Backends> = order.iter().map(|api| api.backends()).collect();
            choose_adapter(&order, &adapters, None)
                .expect("an adapter of the order")
                .get_info()
                .backend
        };

        assert_eq!(backend_of(&[GraphicsApi::Vulkan]), wgpu::Backend::Vulkan);
        assert_eq!(backend_of(&[GraphicsApi::Opengl]), wgpu::Backend::Gl);
        assert_eq!(
            backend_of(&[GraphicsApi::Opengl, GraphicsApi::Vulkan]),
            wgpu::Backend::Gl
        );
        assert_eq!(
            backend_of(&[GraphicsApi::Vulkan, GraphicsApi::Opengl]),
            wgpu::Backend::Vulkan
        );
    }

    /// Before the window only a graphics chip is taken: a software adapter of
    /// Vulkan waits, so OpenGL further on still gets its turn at a chip.
    #[test]
    fn no_software_adapter_is_taken_before_the_window() {
        let opened = open_before_window(&[wgpu::Backends::VULKAN]);
        assert!(opened.is_none_or(|opened| !is_software(&opened.adapter.get_info())));
    }

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
