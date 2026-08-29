#[macro_use]
extern crate tracing;

use std::collections::HashMap;
use std::{convert::TryInto, time::Duration};

use smithay_client_toolkit::compositor::SurfaceData;
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1::ZwlrLayerSurfaceV1;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceData,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::WaylandSurface,
    shm::{
        Shm, ShmHandler,
        slot::{Buffer, SlotPool},
    },
};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_callback::WlCallback;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{
    Connection, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output, wl_shm, wl_surface},
};
use wayland_client::{Dispatch, Proxy};
use wayland_protocols::xdg::toplevel_tag::v1::client::xdg_toplevel_tag_manager_v1::{
    self, XdgToplevelTagManagerV1,
};

// FIXME: This crashes sometimes when fighting with (for instance) noctalia shell for an exclusive zone.

fn main() {
    tracing_subscriber::fmt::init();

    let conn = Connection::connect_to_env().unwrap();

    let (globals, event_queue) = registry_queue_init(&conn).unwrap();
    let qh = event_queue.handle();
    let mut event_loop: EventLoop<SimpleWindow> =
        EventLoop::try_new().expect("Failed to initialize the event loop!");
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle)
        .unwrap();

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
    let shm = Shm::bind(&globals, &qh).expect("wl shm is not available.");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("no layer shell");

    // We don't know how large the window will be yet, so lets assume the minimum size we suggested for the
    // initial memory allocation.
    let pool = SlotPool::new(256 * 256 * 4, &shm).expect("Failed to create pool");

    let mut simple_window = SimpleWindow {
        // Seats and outputs may be hotplugged at runtime, therefore we need to setup a registry state to
        // listen for seats and outputs.
        compositor,
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm,
        layer_shell,

        exit: false,
        pool,
        outputs: HashMap::new(),
        bars: HashMap::new(),
    };

    // We don't draw immediately, the configure will notify us when to first draw.
    loop {
        event_loop
            .dispatch(Duration::from_millis(16), &mut simple_window)
            .unwrap();

        if simple_window.exit {
            println!("exiting example");
            break;
        }
    }
}

struct SimpleWindow {
    compositor: CompositorState,
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    layer_shell: LayerShell,

    exit: bool,
    pool: SlotPool,
    outputs: HashMap<ObjectId, WlOutput>,
    bars: HashMap<ObjectId, BarSet>,
}

impl LayerShellHandler for SimpleWindow {
    fn closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &smithay_client_toolkit::shell::wlr_layer::LayerSurface,
    ) {
    }

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        layer_surface: &smithay_client_toolkit::shell::wlr_layer::LayerSurface,
        configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
        _serial: u32,
    ) {
        for bar in Self::bars_mut(&mut self.bars) {
            if bar.layer_surface == *layer_surface {
                (bar.width, bar.height) = configure.new_size;
                bar.draw(conn, qh, &mut self.pool);
            }
        }
    }
}

impl Dispatch2<XdgToplevelTagManagerV1, SimpleWindow> for GlobalData {
    fn event(
        &self,
        _: &mut SimpleWindow,
        _: &xdg_toplevel_tag_manager_v1::XdgToplevelTagManagerV1,
        _: xdg_toplevel_tag_manager_v1::Event,
        _: &Connection,
        _: &QueueHandle<SimpleWindow>,
    ) {
    }
}

impl CompositorHandler for SimpleWindow {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Not needed for this example.
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
        // Not needed for this example.
    }

    fn frame(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.draw(conn, qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
        // Not needed for this example.
    }
}

struct BarSet {
    left: Option<Bar>,
    right: Option<Bar>,
}

impl BarSet {
    pub fn new<
        State: Dispatch<WlSurface, SurfaceData<()>>
            + Dispatch<ZwlrLayerSurfaceV1, LayerSurfaceData>
            + 'static,
    >(
        compositor: &CompositorState,
        layer_shell: &LayerShell,
        output: &WlOutput,
        qh: &QueueHandle<State>,
        left: Option<u32>,
        right: Option<u32>,
    ) -> Self {
        Self {
            left: left.map(|size| {
                Bar::new(
                    compositor,
                    layer_shell,
                    output,
                    qh,
                    size,
                    Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
                )
            }),
            right: right.map(|size| {
                Bar::new(
                    compositor,
                    layer_shell,
                    output,
                    qh,
                    size,
                    Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM,
                )
            }),
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Bar> {
        self.left.iter_mut().chain(self.right.iter_mut())
    }
}

struct Bar {
    surface: WlSurface,
    layer_surface: LayerSurface,
    buffer: Option<Buffer>,
    width: u32,
    height: u32,
}

impl Bar {
    pub fn new<
        State: Dispatch<WlSurface, SurfaceData<()>>
            + Dispatch<ZwlrLayerSurfaceV1, LayerSurfaceData>
            + 'static,
    >(
        compositor: &CompositorState,
        layer_shell: &LayerShell,
        output: &WlOutput,
        qh: &QueueHandle<State>,
        size: u32,
        anchor: Anchor,
    ) -> Self {
        let surface = compositor.create_surface(qh);
        let layer_surface = layer_shell.create_layer_surface(
            qh,
            surface.clone(),
            Layer::Top,
            Some("black-bars"),
            Some(&output),
        );
        layer_surface.set_anchor(anchor);
        layer_surface.set_size(size, 0);
        layer_surface.set_exclusive_zone(size as i32);
        layer_surface.commit();

        Self {
            surface,
            layer_surface,
            buffer: None,
            width: size,
            height: 0,
        }
    }

    pub fn draw<State: 'static + Dispatch<WlCallback, FrameCallbackData>>(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<State>,
        pool: &mut SlotPool,
    ) {
        let stride = self.width as i32 * 4;

        let buffer = self.buffer.get_or_insert_with(|| {
            pool.create_buffer(
                self.width as i32,
                self.height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .expect("create buffer")
            .0
        });

        let canvas = match pool.canvas(buffer) {
            Some(canvas) => canvas,
            None => {
                // This should be rare, but if the compositor has not released the previous
                // buffer, we need double-buffering.
                let (second_buffer, canvas) = pool
                    .create_buffer(
                        self.width as i32,
                        self.height as i32,
                        stride,
                        wl_shm::Format::Argb8888,
                    )
                    .expect("create buffer");
                *buffer = second_buffer;
                canvas
            }
        };

        // Draw to the window:
        {
            canvas.chunks_exact_mut(4).for_each(|chunk| {
                let array: &mut [u8; 4] = chunk.try_into().unwrap();
                *array = 0xff000000u32.to_le_bytes();
            });
        }

        // Damage the entire window
        self.surface
            .damage_buffer(0, 0, self.width as i32, self.height as i32);

        // Request our next frame
        self.surface
            .frame(qh, FrameCallbackData(self.surface.clone()));

        // Attach and commit to present.
        buffer.attach_to(&self.surface).expect("buffer attach");
        self.layer_surface.commit();
    }
}

impl OutputHandler for SimpleWindow {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        info!("new output: {output:?}");
        self.outputs.insert(output.id(), output.clone());

        self.bars.insert(
            output.id(),
            BarSet::new(
                &self.compositor,
                &self.layer_shell,
                &output,
                qh,
                Some(50),
                Some(200),
            ),
        );
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        info!("updated output: {output:?}");
        self.outputs.insert(output.id(), output);
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        info!("removed output: {output:?}");
        self.outputs.remove(&output.id());
        self.bars.remove(&output.id());
    }
}

impl ShmHandler for SimpleWindow {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl SimpleWindow {
    pub fn draw(&mut self, conn: &Connection, qh: &QueueHandle<Self>) {
        for (_, barset) in self.bars.iter_mut() {
            if let Some(left) = &mut barset.left {
                left.draw::<Self>(conn, qh, &mut self.pool);
            }
            if let Some(right) = &mut barset.right {
                right.draw::<Self>(conn, qh, &mut self.pool);
            }
        }
    }

    pub fn bars_mut(bars: &mut HashMap<ObjectId, BarSet>) -> impl Iterator<Item = &mut Bar> {
        bars.values_mut().flat_map(|set| set.iter_mut())
    }
}

impl Dispatch<WlCallback, ()> for SimpleWindow {
    fn event(
        _state: &mut Self,
        _proxy: &WlCallback,
        _event: <WlCallback as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}

delegate_registry!(SimpleWindow);

impl ProvidesRegistryState for SimpleWindow {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_dispatch2!(SimpleWindow);
