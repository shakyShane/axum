use crate::server_actor::AppState;
use crate::server_config::{Content, RawContent};
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, MethodRouter};
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
    println!("  -> serve_dir_loader {}", req.uri().path());

    {
        let v = app.routes.lock().await;
        let matched = v.at(req.uri().path());

        let Ok(matched) = matched else {
            tracing::trace!("returning a not found... {}", req.uri());
            return (StatusCode::NOT_FOUND, "not_found").into_response();
        };

        let content = matched.value;
        let params = matched.params;

        for (key, value) in params.iter() {
            println!("{}={}", key, value);
        }

        match content {
            Content::Dir { dir } => {
                tracing::trace!("pther {} {}", dir, req.uri().path());
                let s = ServeDir::new(dir);
                let mut service = ServiceBuilder::new()
                    .boxed()
                    .layer(CompressionLayer::new())
                    .service(s);

                let r = service.ready().await.unwrap().call(req).await;
                return r.into_response();
            }
            _ => {}
        }
    }

    let r = next.run(req).await;
    println!("  <- serve_dir_loader");
    r.into_response()
}

async fn raw_loader(
    State(app): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    println!("-> raw_loader");

    {
        let v = app.routes.lock().await;
        let matched = v.at(req.uri().path());

        let Ok(matched) = matched else {
            tracing::trace!("returning a not found... {}", req.uri());
            return (StatusCode::NOT_FOUND, "not_found").into_response();
        };

        let content = matched.value;
        let params = matched.params;

        for (key, value) in params.iter() {
            println!("{}={}", key, value);
        }

        match content {
            Content::Raw {
                raw: RawContent::Html { html },
            } => return Html(html.clone()).into_response(),
            Content::Raw {
                raw: RawContent::Css { css },
            } => return text_asset_response(req.uri().path(), css),
            Content::Raw {
                raw: RawContent::Js { js },
            } => return text_asset_response(req.uri().path(), js),
            Content::Dir { dir } => {
                tracing::trace!("ignoring a Dir match {}", dir);
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
