use {
    crate::protocols::{
        color_management_v1::wp_color_manager_v1::{
            WpColorManagerV1, WpColorManagerV1Primaries, WpColorManagerV1RenderIntent,
            WpColorManagerV1TransferFunction,
        },
        color_representation_v1::{
            wp_color_representation_manager_v1::WpColorRepresentationManagerV1,
            wp_color_representation_surface_v1::WpColorRepresentationSurfaceV1AlphaMode,
        },
        single_pixel_buffer_v1::wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1,
        viewporter::wp_viewporter::WpViewporter,
        wayland::{
            wl_compositor::WlCompositor,
            wl_display::WlDisplay,
            wl_registry::WlRegistry,
            wl_shm::{WlShm, WlShmFormat},
            wl_subcompositor::WlSubcompositor,
        },
        xdg_shell::{xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase},
    },
    arrayvec::ArrayVec,
    half::f16,
    std::{cell::Cell, io::Write, os::fd::AsFd},
    uapi::c,
    wl_client::{Libwayland, proxy::OwnedProxy},
};

mod protocols {
    include!(concat!(env!("OUT_DIR"), "/wayland-protocols/mod.rs"));
}

fn main() {
    let lib = Libwayland::open().unwrap();
    let con = lib.connect_to_default_display().unwrap();
    let queue = con.create_local_queue(c"test-client");
    let wl_display = queue.display::<WlDisplay>();
    let wl_registry = wl_display.get_registry();
    let wp_single_pixel_buffer_manager_v1 = Cell::new(None::<WpSinglePixelBufferManagerV1>);
    let wp_color_representation_manager_v1 = Cell::new(None::<WpColorRepresentationManagerV1>);
    let wp_color_manager_v1 = Cell::new(None::<WpColorManagerV1>);
    let wp_viewporter = Cell::new(None::<WpViewporter>);
    let wl_compositor = Cell::new(None::<WlCompositor>);
    let wl_subcompositor = Cell::new(None::<WlSubcompositor>);
    let wl_shm = Cell::new(None::<WlShm>);
    let xdg_wm_base = Cell::new(None::<XdgWmBase>);
    queue.dispatch_scope_blocking(|s| {
        s.set_event_handler_local(
            &wl_registry,
            WlRegistry::on_global(|_, name, interface, _| match interface {
                WpSinglePixelBufferManagerV1::INTERFACE => {
                    wp_single_pixel_buffer_manager_v1.set(Some(wl_registry.bind(name, 1)));
                }
                WpColorRepresentationManagerV1::INTERFACE => {
                    wp_color_representation_manager_v1.set(Some(wl_registry.bind(name, 1)));
                }
                WpColorManagerV1::INTERFACE => {
                    wp_color_manager_v1.set(Some(wl_registry.bind(name, 1)));
                }
                WpViewporter::INTERFACE => {
                    wp_viewporter.set(Some(wl_registry.bind(name, 1)));
                }
                WlCompositor::INTERFACE => {
                    wl_compositor.set(Some(wl_registry.bind(name, 1)));
                }
                WlSubcompositor::INTERFACE => {
                    wl_subcompositor.set(Some(wl_registry.bind(name, 1)));
                }
                WlShm::INTERFACE => {
                    wl_shm.set(Some(wl_registry.bind(name, 1)));
                }
                XdgWmBase::INTERFACE => {
                    xdg_wm_base.set(Some(wl_registry.bind(name, 1)));
                }
                _ => {}
            }),
        );
        queue.dispatch_roundtrip_blocking().unwrap();
    });
    let wp_single_pixel_buffer_manager_v1 = wp_single_pixel_buffer_manager_v1.take().unwrap();
    let wp_color_representation_manager_v1 = wp_color_representation_manager_v1.take().unwrap();
    let wp_color_manager_v1 = wp_color_manager_v1.take().unwrap();
    let wp_viewporter = wp_viewporter.take().unwrap();
    let wl_compositor = wl_compositor.take().unwrap();
    let wl_subcompositor = wl_subcompositor.take().unwrap();
    let wl_shm = wl_shm.take().unwrap();
    let xdg_wm_base = xdg_wm_base.take().unwrap();

    let wl_surface_root = wl_compositor.create_surface();
    let wp_viewport_root = wp_viewporter.get_viewport(&wl_surface_root);
    let wl_buffer_root = wp_single_pixel_buffer_manager_v1.create_u32_rgba_buffer(0, 0, 0, 0);
    let xdg_surface = xdg_wm_base.get_xdg_surface(&wl_surface_root);
    let xdg_toplevel = xdg_surface.get_toplevel();
    wl_surface_root.commit();

    let red_linear = 0.5;
    let alpha = 0.5;

    let wp_image_description_creator_params_v1 = wp_color_manager_v1.create_parametric_creator();
    wp_image_description_creator_params_v1.set_tf_named(WpColorManagerV1TransferFunction::GAMMA22);
    wp_image_description_creator_params_v1.set_primaries_named(WpColorManagerV1Primaries::SRGB);
    let wp_image_description_v1 = wp_image_description_creator_params_v1.create();

    let create_subsurface = |alpha_mode, red| {
        let mut res = ArrayVec::<_, 2>::new();
        for shm in [false, true] {
            let wl_buffer = if shm {
                let mut memfd =
                    uapi::memfd_create(c"", c::MFD_CLOEXEC | c::MFD_ALLOW_SEALING).unwrap();
                uapi::fcntl_add_seals(memfd.raw(), c::F_SEAL_SHRINK).unwrap();
                let data = [
                    f16::from_f64(red).to_ne_bytes(),
                    f16::from_f64(0.0).to_ne_bytes(),
                    f16::from_f64(0.0).to_ne_bytes(),
                    f16::from_f64(alpha).to_ne_bytes(),
                ];
                memfd.write_all(uapi::as_bytes(&data)).unwrap();
                let wl_shm_pool = wl_shm.create_pool(memfd.as_fd(), 8);
                wl_shm_pool.create_buffer(0, 1, 1, 8, WlShmFormat::ABGR16161616F)
            } else {
                let f_to_u32 = |l: f64| (l * u32::MAX as f64) as u32;
                wp_single_pixel_buffer_manager_v1.create_u32_rgba_buffer(
                    f_to_u32(red),
                    0,
                    0,
                    f_to_u32(alpha),
                )
            };
            let wl_surface = wl_compositor.create_surface();
            let wp_viewport = wp_viewporter.get_viewport(&wl_surface);
            let wl_subsurface = wl_subcompositor.get_subsurface(&wl_surface, &wl_surface_root);
            let wp_color_representation_surface =
                wp_color_representation_manager_v1.get_surface(&wl_surface);
            wp_color_representation_surface.set_alpha_mode(alpha_mode);
            let wp_color_management_surface = wp_color_manager_v1.get_surface(&wl_surface);
            wp_color_management_surface.set_image_description(
                &wp_image_description_v1,
                WpColorManagerV1RenderIntent::PERCEPTUAL,
            );
            wl_surface.attach(Some(&wl_buffer), 0, 0);
            res.push((wl_surface, wp_viewport, wl_subsurface));
        }
        res
    };

    let linear_to_gamma = |l: f64| l.powf(1.0 / 2.2);

    let ss = [
        create_subsurface(
            WpColorRepresentationSurfaceV1AlphaMode::PREMULTIPLIED_ELECTRICAL,
            linear_to_gamma(red_linear) * alpha,
        ),
        create_subsurface(
            WpColorRepresentationSurfaceV1AlphaMode::PREMULTIPLIED_OPTICAL,
            linear_to_gamma(red_linear * alpha),
        ),
        create_subsurface(
            WpColorRepresentationSurfaceV1AlphaMode::STRAIGHT,
            linear_to_gamma(red_linear),
        ),
    ];

    let width = Cell::new(800);
    let height = Cell::new(600);

    queue.dispatch_scope_blocking(|s| {
        s.set_event_handler_local(
            &xdg_wm_base,
            XdgWmBase::on_ping(|_, serial| {
                xdg_wm_base.pong(serial);
            }),
        );
        s.set_event_handler_local(
            &xdg_toplevel,
            XdgToplevel::on_configure(|_, w, h, _| {
                if w != 0 {
                    width.set(w);
                }
                if h != 0 {
                    height.set(h);
                }
            }),
        );
        s.set_event_handler_local(
            &xdg_surface,
            XdgSurface::on_configure(|_, serial| {
                let w = width.get();
                let h = height.get();
                xdg_surface.ack_configure(serial);
                for (x, s) in ss.iter().enumerate() {
                    for (y, s) in s.iter().enumerate() {
                        s.2.set_position((x as i32 * w) / 3, (y as i32 * h) / 2);
                        s.1.set_destination(w / 3, h / 2);
                        s.0.commit();
                    }
                }
                wp_viewport_root.set_destination(w, h);
                wl_surface_root.attach(Some(&wl_buffer_root), 0, 0);
                wl_surface_root.damage(0, 0, i32::MAX, i32::MAX);
                wl_surface_root.commit();
            }),
        );
        loop {
            queue.dispatch_blocking().unwrap();
        }
    });
}
