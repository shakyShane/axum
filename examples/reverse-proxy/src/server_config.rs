use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_address: String,
    pub routes: Vec<ServerConfigRoute>,
}
#[derive(Debug, Clone)]
pub struct ServerConfigRoute {
    pub path: PathBuf,
    pub route: Route,
}

#[derive(Debug, Clone)]
pub enum Route {
    Html(String),
}
