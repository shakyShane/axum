use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    pub bind_address: String,
    #[serde(default)]
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Route {
    pub path: PathBuf,
    pub content: Content,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type")]
pub enum Content {
    Raw { raw: RawContent },
    Dir { dir: String },
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type")]
pub enum RawContent {
    Html { html: String },
    Css { css: String },
    Js { js: String },
}
