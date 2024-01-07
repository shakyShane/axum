use tokio::sync::oneshot::{Receiver, Sender};

pub struct ServerSignals {
    pub stop_msg_sender: Option<Sender<()>>,
    pub complete_mdg_receiver: Option<Receiver<()>>,
}
