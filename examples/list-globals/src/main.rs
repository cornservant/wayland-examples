#[macro_use]
extern crate tracing;

use smithay_client_toolkit::registry::RegistryState;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::wl_registry::WlRegistry,
};

fn main() {
    tracing_subscriber::fmt::init();

    let conn = Connection::connect_to_env().unwrap();

    let (globals, event_queue) = registry_queue_init(&conn).unwrap();
    globals.contents().with_list(|list| {
        for g in list {
            warn!("global: {:?}", g)
        }
    });
    let _qh: QueueHandle<Client> = event_queue.handle();

    let _client = Client {
        registry_state: RegistryState::new(&globals),
    };
}

pub struct Client {
    pub registry_state: RegistryState,
}

impl Dispatch<WlRegistry, GlobalListContents> for Client {
    fn event(
        _state: &mut Self,
        _proxy: &WlRegistry,
        _event: <WlRegistry as wayland_client::Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        todo!()
    }
}
