use crate::panic_handler::handle_panic;
use crate::server_actor::AppState;
use crate::server_config::Route;
use axum::extract::{Query, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::Uri;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, MethodRouter};
use axum::{http, Router};
use std::sync::Arc;
use tower::ServiceExt;
use tower_http::catch_panic::CatchPanicLayer;
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
        .layer(from_fn_with_state(state.clone(), raw_loader))
        .layer(CatchPanicLayer::custom(handle_panic))
        .with_state(state.clone())
}

async fn never(State(_app): State<Arc<AppState>>, req: Request) -> impl IntoResponse {
    println!("    -> never");
    (
        http::StatusCode::NOT_FOUND,
        format!("unreachable {}", req.uri()),
    )
}

#[derive(serde::Deserialize, Debug)]
struct P {
    pub encoding: Option<Encoding>,
}

#[derive(serde::Deserialize, Debug)]
pub enum Encoding {
    Br,
    Zip,
}

async fn serve_dir_loader(
    State(app): State<Arc<AppState>>,
    query: Query<P>,
    req: Request,
    _next: Next,
) -> Response {
    tracing::trace!("  -> serve_dir_loader {}", req.uri().path());
    tracing::trace!("  -> serve_dir_loader {:?}", query);

    let bindings = app.dir_bindings.lock().await;
    let mut app = Router::new();

    for (k, v) in bindings.iter() {
        let r1 = Router::new().nest_service(k, ServeDir::new(v));
        if k == "/here" || k == "/a-file-2" {
            app = app.merge(r1.layer(CompressionLayer::new()));
        } else {
            app = app.merge(r1);
        }
    }

    if let Some(Encoding::Br) = &query.encoding {
        todo!("implement Encoding::Br");
        // app = app.layer(CompressionLayer::new().br(true).no_gzip());
    }

    if let Some(Encoding::Zip) = &query.encoding {
        // app = app.layer(CompressionLayer::new().no_br().gzip(true));
        todo!("implement Encoding::Zip");
    }

    match app.oneshot(req).await {
        Ok(r) => {
            let r = r.into_response();
            tracing::trace!("  <- serve_dir_loader");
            r
        }
        Err(e) => (
            http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("unreachable {:?}", e),
        )
            .into_response(),
    }
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
            tracing::trace!("<- raw_loader.next");
            return r;
        };

        let content = matched.value;
        let params = matched.params;

        for (key, value) in params.iter() {
            tracing::trace!("-> {}={}", key, value);
        }

        match content {
            Route::Raw { path, raw } => {
                tracing::trace!("-> served Route::Raw {} {}bytes", path, raw.len());
                return text_asset_response(req.uri().path(), raw);
            }
            Route::Html { path, html } => {
                tracing::trace!("-> served Route::Html {} {}bytes", path, html.len());
                return Html(html.clone()).into_response();
            }
            Route::Dir { .. } => {
                // deliberate fall through
            }
        }
    }

    let response = next.run(req).await;
    tracing::trace!("<- raw_loader.next");
    response
}

fn text_asset_response(path: &str, css: &str) -> Response {
    let mime = mime_guess::from_path(path);
    let aas_str = mime.first_or_text_plain();
    let cloned = css.to_owned();
    ([(CONTENT_TYPE, aas_str.to_string())], cloned).into_response()
}
