use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind_address: String,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub path: PathBuf,
    pub content: Content,
}

#[derive(Clone, Debug)]
pub enum Content {
    Raw(RawContent),
    Dir(String),
}

#[derive(Clone, Debug)]
pub enum RawContent {
    Html(String),
    Css(String),
    Js(String),
}
