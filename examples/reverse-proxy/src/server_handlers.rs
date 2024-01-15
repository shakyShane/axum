use crate::server_actor::AppState;
use crate::server_config::{Content, RawContent};
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, any_service, MethodRouter};
use axum::{http, Router};
use std::sync::Arc;
use tower::{Service, ServiceBuilder, ServiceExt};
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;

pub fn built_ins(state: Arc<AppState>) -> Router {
    async fn handler(State(app): State<Arc<AppState>>, uri: Uri) -> impl IntoResponse {
        // let v = app.lock().await;
        let routes = app.routes.lock().await;
        format!("route: {:?}", routes.at(uri.path())).into_response()
    }

    route("/foo", any(handler)).with_state(state.clone())
}

fn route(path: &str, method_router: MethodRouter<Arc<AppState>>) -> Router<Arc<AppState>> {
    Router::new().route(path, method_router)
}

pub fn dynamic_loaders(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", any(never))
        .layer(from_fn_with_state(state.clone(), serve_dir_loader))
        // .layer(CompressionLayer::new())
        .layer(from_fn_with_state(state.clone(), raw_loader))
        .with_state(state.clone())
}

async fn never(State(app): State<Arc<AppState>>, req: Request) -> impl IntoResponse {
    println!("    -> never");
    (
        http::StatusCode::NOT_FOUND,
        format!("unreachable {}", req.uri()),
    )
}

async fn serve_dir_loader(State(app): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    tracing::trace!("  -> serve_dir_loader {}", req.uri().path());

    let bindings = app.dir_bindings.lock().await;
    let mut app = Router::new();

    for (num, (k, v)) in bindings.iter().enumerate() {
        app = app.nest_service(k, any_service(ServeDir::new(v)));
    }

    let r = app.oneshot(req).await.unwrap();
    let r = r.into_response();
    tracing::trace!("  <- serve_dir_loader");
    return r;
}

async fn raw_loader(
    State(app): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    tracing::trace!("-> raw_loader");

    {
        let v = app.routes.lock().await;
        let matched = v.at(req.uri().path());

        let Ok(matched) = matched else {
            drop(v);
            let r = next.run(req).await;
            println!("<- raw_loader");
            return r;
        };

        let content = matched.value;
        let params = matched.params;

        for (key, value) in params.iter() {
            tracing::trace!("-> {}={}", key, value);
        }

        match content {
            Content::Raw {
                raw: RawContent::Html { html },
            } => {
                tracing::trace!("-> served HTML");
                return Html(html.clone()).into_response();
            }
            Content::Raw {
                raw: RawContent::Css { css },
            } => {
                tracing::trace!("-> served css");
                return text_asset_response(req.uri().path(), css);
            }
            Content::Raw {
                raw: RawContent::Js { js },
            } => {
                tracing::trace!("-> served js");
                return text_asset_response(req.uri().path(), js);
            }
            Content::Dir { dir } => {
                // tracing::trace!("-> ignoring a Dir match {}", dir);
                // nothing...
            }
        }
    }

    let r = next.run(req).await;
    println!("<- raw_loader");
    r
}

fn text_asset_response(path: &str, css: &str) -> Response {
    let mime = mime_guess::from_path(path);
    let aas_str = mime.first_or_text_plain();
    let cloned = css.to_owned();
    ([(CONTENT_TYPE, aas_str.to_string())], cloned).into_response()
}
