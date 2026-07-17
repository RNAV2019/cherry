use std::os::unix::net::UnixListener;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::{Window, WindowAttributes, WindowId};

const SOCKET_PATH: &str = "/tmp/cherry.sock";
const LOGICAL_WIDTH: f64 = 900.0;
const LOGICAL_HEIGHT: f64 = 520.0;

#[derive(Debug)]
enum UserEvent {
    Toggle,
    Kill,
}

/// GPU resources that survive hide/show cycles.
struct GpuState {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    egui_renderer: egui_wgpu::Renderer,
    surface_format: wgpu::TextureFormat,
}

/// Per-window state created on show, destroyed on hide.
///
/// `wgpu_surface` MUST be declared before `window` so it is dropped first —
/// wgpu holds its own Arc<Window> reference internally and must release it
/// before we drop our Arc here, allowing the Wayland wl_surface to be destroyed.
struct WindowState {
    wgpu_surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    window: Arc<Window>,
    egui_state: egui_winit::State,
}

/// `app` is created once in `init_full` and never rebuilt — this is what
/// makes the wallpaper texture/dimension cache persist across toggles.
struct State {
    gpu: GpuState,
    win: Option<WindowState>,
    egui_ctx: egui::Context,
    app: crate::app::CherryApp,
}

impl State {
    fn reconfigure_surface(&self) {
        if let Some(win) = &self.win {
            win.wgpu_surface.configure(&self.gpu.device, &win.surface_config);
        }
    }

    fn request_redraw(&self) {
        if let Some(win) = &self.win {
            win.window.request_redraw();
        }
    }
}

struct Daemon {
    state: Option<State>,
    proxy: EventLoopProxy<UserEvent>,
}

fn window_attrs() -> WindowAttributes {
    WindowAttributes::default()
        .with_title("Cherry")
        .with_name("uk.co.ryannavsaria.cherry", "cherry")
        .with_inner_size(LogicalSize::new(LOGICAL_WIDTH, LOGICAL_HEIGHT))
        .with_min_inner_size(LogicalSize::new(LOGICAL_WIDTH, LOGICAL_HEIGHT))
        .with_max_inner_size(LogicalSize::new(LOGICAL_WIDTH, LOGICAL_HEIGHT))
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
}

impl Daemon {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Daemon { state: None, proxy }
    }

    /// Full init: create GPU + window together (needed so adapter is surface-compatible),
    /// then create the app and kick off its initial background decode.
    fn init_full(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = window_attrs();
        let window = Arc::new(event_loop.create_window(attrs).expect("Failed to create window"));
        let scale = window.scale_factor();
        let phys = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let wgpu_surface = instance.create_surface(window.clone()).expect("Failed to create wgpu surface");

        let (adapter, device, queue) = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&wgpu_surface),
                    ..Default::default()
                })
                .await
                .expect("No compatible GPU adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Failed to get device");
            (adapter, device, queue)
        });

        let caps = wgpu_surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let alpha_mode = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|&m| m == wgpu::CompositeAlphaMode::PreMultiplied)
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: phys.width.max(1),
            height: phys.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        wgpu_surface.configure(&device, &surface_config);

        let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);

        let egui_ctx = egui::Context::default();
        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = crate::ui::theme::BG;
        egui_ctx.set_visuals(visuals);
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        egui_ctx.set_fonts(fonts);

        let max_texture_side = device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale as f32),
            None,
            Some(max_texture_side),
        );

        let app = crate::app::CherryApp::new();
        app.request_initial_load(egui_ctx.clone());

        self.state = Some(State {
            gpu: GpuState { instance, adapter, device, queue, egui_renderer, surface_format: format },
            win: Some(WindowState { wgpu_surface, surface_config, window, egui_state }),
            egui_ctx,
            app,
        });
    }

    /// Recreate only the window + surface using existing GPU state and the
    /// existing `app` (and its caches).
    fn open_window(&mut self, event_loop: &ActiveEventLoop) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };

        let attrs = window_attrs();
        let window = Arc::new(event_loop.create_window(attrs).expect("Failed to create window"));
        let scale = window.scale_factor();
        let phys = window.inner_size();

        let wgpu_surface = state.gpu.instance
            .create_surface(window.clone())
            .expect("Failed to create wgpu surface");
        let caps = wgpu_surface.get_capabilities(&state.gpu.adapter);

        let format = caps.formats.iter().copied()
            .find(|&f| f == state.gpu.surface_format)
            .unwrap_or(caps.formats[0]);
        if format != state.gpu.surface_format {
            state.gpu.surface_format = format;
            state.gpu.egui_renderer =
                egui_wgpu::Renderer::new(&state.gpu.device, format, None, 1, false);
        }

        let alpha_mode = caps
            .alpha_modes
            .iter()
            .copied()
            .find(|&m| m == wgpu::CompositeAlphaMode::PreMultiplied)
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: phys.width.max(1),
            height: phys.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        wgpu_surface.configure(&state.gpu.device, &surface_config);

        let max_texture_side = state.gpu.device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(
            state.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale as f32),
            None,
            Some(max_texture_side),
        );

        state.win = Some(WindowState { wgpu_surface, surface_config, window, egui_state });
    }

    /// No fade animation: showing/hiding a window is instant (matches
    /// cherry's current UX). `showing` == a window currently exists.
    fn toggle(&mut self, event_loop: &ActiveEventLoop) {
        let showing = self.state.as_ref().map_or(false, |s| s.win.is_some());

        if showing {
            let state = self.state.as_mut().unwrap();
            state.app.on_hide();
            state.win = None;
        } else {
            if self.state.is_none() {
                self.init_full(event_loop);
            } else {
                self.open_window(event_loop);
            }
            if let Some(state) = self.state.as_mut() {
                let ctx = state.egui_ctx.clone();
                state.app.on_show(&ctx);
                state.request_redraw();
            }
        }
    }

    fn render(&mut self) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };
        if state.win.is_none() {
            return;
        }

        let raw_input = {
            let win = state.win.as_mut().unwrap();
            win.egui_state.take_egui_input(&win.window)
        };

        let full_output = state.egui_ctx.run(raw_input, |ctx| {
            state.app.ui(ctx);
        });

        {
            let win = state.win.as_mut().unwrap();
            win.egui_state.handle_platform_output(&win.window, full_output.platform_output);
        }

        for (id, delta) in &full_output.textures_delta.set {
            state.gpu.egui_renderer.update_texture(&state.gpu.device, &state.gpu.queue, *id, delta);
        }
        for id in &full_output.textures_delta.free {
            state.gpu.egui_renderer.free_texture(id);
        }

        let screen_descriptor = {
            let win = state.win.as_ref().unwrap();
            egui_wgpu::ScreenDescriptor {
                size_in_pixels: [win.surface_config.width, win.surface_config.height],
                pixels_per_point: win.window.scale_factor() as f32,
            }
        };

        let surface_texture = {
            let win = state.win.as_mut().unwrap();
            match win.wgpu_surface.get_current_texture() {
                Ok(t) => t,
                Err(wgpu::SurfaceError::Lost) => {
                    win.wgpu_surface.configure(&state.gpu.device, &win.surface_config);
                    return;
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    eprintln!("wgpu: out of memory");
                    return;
                }
                Err(_) => return,
            }
        };

        let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = state.gpu.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor::default(),
        );

        let primitives = state.egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        state.gpu.egui_renderer.update_buffers(
            &state.gpu.device,
            &state.gpu.queue,
            &mut encoder,
            &primitives,
            &screen_descriptor,
        );

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let mut render_pass = render_pass.forget_lifetime();
            state.gpu.egui_renderer.render(&mut render_pass, &primitives, &screen_descriptor);
        }

        state.gpu.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        // Drain actions after presenting, so Hide/Apply this frame still shows
        // the frame that triggered them (e.g. the row highlight on click).
        let actions = state.app.drain_actions();
        for action in actions {
            match action {
                crate::app::AppAction::Hide => {
                    state.app.on_hide();
                    state.win = None;
                    return;
                }
                crate::app::AppAction::Apply(path) => match crate::apply::apply(&path) {
                    Ok(()) => {
                        state.app.on_hide();
                        state.win = None;
                        return;
                    }
                    Err(err) => {
                        eprintln!("cherry: {err}");
                        crate::apply::notify(&err);
                        std::process::exit(1);
                    }
                },
            }
        }

        for (_id, viewport) in &full_output.viewport_output {
            if viewport.repaint_delay.is_zero() {
                state.request_redraw();
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for Daemon {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Lazy init: GPU and window are created on first toggle-show.
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::Init = cause {
            let proxy = self.proxy.clone();
            std::thread::spawn(move || {
                let _ = std::fs::remove_file(SOCKET_PATH);
                let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind Unix socket");
                for stream in listener.incoming() {
                    if let Ok(mut stream) = stream {
                        let mut buf = String::new();
                        use std::io::Read;
                        let _ = stream.read_to_string(&mut buf);
                        for line in buf.lines() {
                            match line.trim() {
                                "toggle" => {
                                    let _ = proxy.send_event(UserEvent::Toggle);
                                }
                                "kill" => {
                                    let _ = proxy.send_event(UserEvent::Kill);
                                }
                                _ => {}
                            }
                        }
                    }
                }
            });
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Toggle => self.toggle(event_loop),
            UserEvent::Kill => {
                let _ = std::fs::remove_file(SOCKET_PATH);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Feed event to egui-winit for input translation
        {
            let state = match self.state.as_mut() {
                Some(s) => s,
                None => return,
            };
            let win = match state.win.as_mut() {
                Some(w) => w,
                None => return,
            };
            let response = win.egui_state.on_window_event(&win.window, &event);
            if response.repaint {
                win.window.request_redraw();
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                let state = match self.state.as_mut() {
                    Some(s) => s,
                    None => return,
                };
                if width > 0 && height > 0 {
                    if let Some(win) = state.win.as_mut() {
                        win.surface_config.width = width;
                        win.surface_config.height = height;
                    }
                    state.reconfigure_surface();
                    state.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let state = match self.state.as_mut() {
                    Some(s) => s,
                    None => return,
                };
                let phys = state.win.as_ref().map(|w| w.window.inner_size());
                if let Some(phys) = phys {
                    if phys.width > 0 && phys.height > 0 {
                        if let Some(win) = state.win.as_mut() {
                            win.surface_config.width = phys.width;
                            win.surface_config.height = phys.height;
                        }
                        state.reconfigure_surface();
                    }
                }
                state.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            _ => {}
        }
    }
}

pub fn run_daemon() {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut daemon = Daemon::new(proxy);
    event_loop.run_app(&mut daemon).expect("Event loop error");
}
