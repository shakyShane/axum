use crate::server_config::Route;

#[derive(actix::Message, Clone)]
#[rtype(result = "()")]
pub struct Patch {
    pub routes: Vec<Route>,
}
