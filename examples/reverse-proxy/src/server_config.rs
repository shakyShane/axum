use crate::server_actor::AppState;

use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    pub bind_address: String,
    #[serde(default)]
    pub routes: Vec<Route>,
}

impl From<ServerConfig> for AppState {
    fn from(val: ServerConfig) -> AppState {
        AppState {
            routes: Arc::new(Mutex::new(val.routes.clone())),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Route {
    pub path: String,
    #[serde(flatten)]
    pub opts: Option<Opts>,
    #[serde(flatten)]
    pub kind: RouteKind,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum RouteKind {
    Html { html: String },
    Json { json: serde_json::Value },
    Raw { raw: String },
    Dir(DirRoute),
}

impl RouteKind {
    #[allow(dead_code)]
    pub fn html(s: &str) -> Self {
        Self::Html { html: s.into() }
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DirRoute {
    pub dir: String,
}

impl Route {
    pub fn path(&self) -> &str {
        self.path.as_str()
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Opts {
    pub cors: bool,
}
