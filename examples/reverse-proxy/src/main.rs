//! Reverse proxy listening in "localhost:4000" will proxy all requests to "localhost:3000"
//! endpoint.
//!
//! Run with
//!
//! ```not_rust
//! cargo run -p example-reverse-proxy
//! ```

use axum::body::Bytes;
use axum::extract::FromRef;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::routing::head;
use axum::{
    body::Body,
    extract::{Request, State},
    http::uri::Uri,
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use http_body_util::BodyExt;
use hyper::StatusCode;
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_reverse_proxy=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let client: Client =
        hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
            .build(HttpConnector::new());

    let config = Config {
        host: "example.com".into(),
    };

    let app_state = AppState { config, client };

    let app = Router::new()
        .route("/", get(handler).head(handler))
        .layer(middleware::from_fn(print_request_response))
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

async fn print_request_response(
    req: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let (parts, body) = req.into_parts();
    let bytes = buffer_and_print("request", body).await?;
    let req = Request::from_parts(parts, Body::from(bytes));

    let res = next.run(req).await;

    let (parts, body) = res.into_parts();
    let bytes = buffer_and_print("response", body).await?;
    let res = Response::from_parts(parts, Body::from(bytes));

    Ok(res)
}

async fn buffer_and_print<B>(direction: &str, body: B) -> Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("failed to read {direction} body: {err}"),
            ));
        }
    };

    if let Ok(body) = std::str::from_utf8(&bytes) {
        tracing::debug!("{direction} body = {body:?}");
    }

    Ok(bytes)
}
