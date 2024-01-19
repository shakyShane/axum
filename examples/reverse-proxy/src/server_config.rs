#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    pub bind_address: String,
    #[serde(default)]
    pub routes: Vec<Route>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum Route {
    Raw {
        path: String,
        raw: String,
    },
    Dir {
        path: String,
        dir: String,
    },
    Html {
        path: String,
        html: String,
    },
    Json {
        path: String,
        json: serde_json::Value,
    },
}
