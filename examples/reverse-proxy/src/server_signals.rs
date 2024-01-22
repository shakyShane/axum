use axum_server::Handle;
use tokio::sync::oneshot::{Receiver};

pub struct ServerSignals {
    pub complete_mdg_receiver: Option<Receiver<()>>,
    pub handle: Option<Handle>,
}
