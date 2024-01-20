use crate::server_actor::AppState;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    pub bind_address: String,
    #[serde(default)]
    pub routes: Vec<Route>,
}

impl Into<AppState> for ServerConfig {
    fn into(self) -> AppState {
        let mut router = matchit::Router::new();
        let mut dir_bindings = HashMap::new();
        for route in &self.routes {
            let path = route.path();
            match &route.kind {
                RouteKind::Html { .. } | RouteKind::Json { .. } | RouteKind::Raw { .. } => {
                    let existing = router.at_mut(path);
                    if let Ok(prev) = existing {
                        *prev.value = route.clone();
                        tracing::trace!(" └ updated mutable route at {}", path)
                    } else if let Err(err) = existing {
                        match router.insert(path, route.clone()) {
                            Ok(_) => {
                                tracing::trace!("  └ inserted {} with {:?}", path, route)
                            }
                            Err(_) => {
                                tracing::error!("  └ could not insert {:?}", err.to_string())
                            }
                        }
                    }
                }
                RouteKind::Dir(DirRoute { dir }) => {
                    dir_bindings.insert(path.to_owned(), route.clone());
                    tracing::trace!(" └ updated dir_bindings at {} with {}", path, dir.clone());
                }
            }
        }
        AppState {
            routes: Arc::new(Mutex::new(router)),
            dir_bindings: Arc::new(Mutex::new(dir_bindings)),
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
