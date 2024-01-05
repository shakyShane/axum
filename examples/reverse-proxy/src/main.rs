//! Reverse proxy listening in "localhost:4000" will proxy all requests to "localhost:3000"
//! endpoint.
//!
//! Run with
//!
//! ```not_rust
//! cargo run -p example-reverse-proxy
//! ```

use axum::extract::FromRef;
use axum::http::HeaderValue;
use axum::routing::head;
use axum::{
    body::Body,
    extract::{Request, State},
    http::uri::Uri,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use hyper::StatusCode;
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};

type Client = hyper_util::client::legacy::Client<HttpConnector, Body>;

#[derive(Debug, Clone)]
struct Config {
    pub host: String,
}

#[derive(Clone)]
struct AppState {
    config: Config,
    client: Client,
}

impl FromRef<AppState> for Config {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}
impl FromRef<AppState> for Client {
    fn from_ref(state: &AppState) -> Self {
        state.client.clone()
    }
}

#[tokio::main]
async fn main() {
    // tokio::spawn(server());

    let client: Client =
        hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
            .build(HttpConnector::new());

    let config = Config {
        host: "example.com".into(),
    };

    let app_state = AppState { config, client };

    let app = Router::new()
        .route("/", get(handler))
        .route("/", head(handler))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:5000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn handler(
    State(client): State<Client>,
    State(config): State<Config>,
    mut req: Request,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    let path_query = req
        .uri()
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(path);

    // let uri = format!("http://example.com{}", path_query);
    let Ok(uri) = Uri::builder()
        .scheme("http")
        .authority(config.host.clone())
        .path_and_query(path_query)
        .build()
    else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };

    *req.uri_mut() = uri;

    req.headers_mut()
        .insert("host", HeaderValue::from_str(&config.host).expect("header"));

    dbg!(&req);

    Ok(client
        .request(req)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .into_response())
}
