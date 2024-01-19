#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    pub bind_address: String,
    #[serde(default)]
    pub routes: Vec<Route>,
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
