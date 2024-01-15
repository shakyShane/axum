use crate::server_config::Route;

#[derive(actix::Message, Clone)]
#[rtype(result = "()")]
pub struct Patch {
    pub server_configs: Vec<crate::server_config::ServerConfig>,
}

#[derive(actix::Message, Clone)]
#[rtype(result = "anyhow::Result<()>")]
pub struct PatchOne {
    pub server_config: crate::server_config::ServerConfig,
}
