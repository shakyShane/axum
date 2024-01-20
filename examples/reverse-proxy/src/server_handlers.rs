use crate::panic_handler::handle_panic;
use crate::server_actor::AppState;
use crate::server_config::{DirRoute, RouteKind};
use axum::extract::{Query, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::Uri;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, MethodRouter};
use axum::{http, Json, Router};
use std::convert::Infallible;
use std::sync::Arc;

use tower::{service_fn, Layer, ServiceBuilder, ServiceExt};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::BoxError;

use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub fn make_router(state: &Arc<AppState>) -> Router {
    let router = Router::new()
        .merge(built_ins(state.clone()))
        .merge(dynamic_loaders(state.clone()));
    router.layer(TraceLayer::new_for_http())
}

pub fn built_ins(state: Arc<AppState>) -> Router {
    async fn handler(State(app): State<Arc<AppState>>, uri: Uri) -> impl IntoResponse {
        // let v = app.lock().await;
        let routes = app.routes.lock().await;
        format!("route-- {:?}", uri.path()).into_response()
    }

    route("/foo", any(handler)).with_state(state.clone())
}

fn route(path: &str, method_router: MethodRouter<Arc<AppState>>) -> Router<Arc<AppState>> {
    Router::new().route(path, method_router)
}

pub fn dynamic_loaders(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", any(never))
        .layer(
            ServiceBuilder::new()
                .layer(from_fn_with_state(state.clone(), raw_loader))
                .layer(from_fn_with_state(state.clone(), serve_dir_loader)),
        )
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

    for (path, v) in bindings.iter() {
        if let RouteKind::Dir(DirRoute { dir }) = &v.kind {
            let mut r1 = Router::new().nest_service(path, ServeDir::new(dir));

            if v.opts.as_ref().is_some_and(|v| v.cors) {
                r1 = r1.layer(CorsLayer::new().allow_origin(Any).allow_methods(Any));
            }

            // if path == "/here" || path == "/a-file-2" {
            //     app = app.merge(r1.layer(CompressionLayer::new()));
            // } else {
            //     app = app.merge(r1);
            // }

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

        match &content.kind {
            RouteKind::Raw { raw } => {
                tracing::trace!("-> served Route::Raw {} {} bytes", content.path, raw.len());
                return text_asset_response(req.uri().path(), raw);
            }
            RouteKind::Html { html } => {
                tracing::trace!(
                    "-> served Route::Html {} {} bytes",
                    content.path,
                    html.len()
                );
                return Html(html.clone()).into_response();
            }
            RouteKind::Json { json } => {
                tracing::trace!("-> served Route::Json {} {}", content.path, json);
                return Json(json).into_response();
            }
            RouteKind::Dir(_) => {
                // deliberate fall through
            }
        }
    }

    let response = next.run(req).await;
    tracing::trace!("<- raw_loader.next");
    response
}

async fn raw_loader_2(
    State(_app): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    tracing::trace!(" -> raw_loader 2");
    let response = next.run(req).await;
    tracing::trace!(" <- raw_loader 2.next");
    response
}

fn text_asset_response(path: &str, css: &str) -> Response {
    let mime = mime_guess::from_path(path);
    let aas_str = mime.first_or_text_plain();
    let cloned = css.to_owned();
    ([(CONTENT_TYPE, aas_str.to_string())], cloned).into_response()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::server_config::{Opts, Route, ServerConfig};
    use axum::body::Body;

    async fn to_resp_body(res: Response) -> String {
        use http_body_util::BodyExt;
        let (_parts, body) = res.into_parts();
        let b = body.collect().await.unwrap();
        let b = b.to_bytes();
        let as_str = std::str::from_utf8(&b).unwrap();
        as_str.to_owned()
    }

    #[tokio::test]
    async fn test_handlers() -> Result<(), anyhow::Error> {
        let state: AppState = ServerConfig {
            bind_address: "127.0.0.1".to_string(),
            routes: vec![Route {
                path: "/hello".to_string(),
                opts: None,
                kind: RouteKind::html("🐥"),
            }],
        }
        .into();

        let app = make_router(&Arc::new(state));
        let req = Request::get("/hello").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();

        assert_eq!(res.headers().get("content-length").unwrap(), "4");
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );

        let body = to_resp_body(res).await;
        assert_eq!(body, "🐥");
        Ok(())
    }

    #[tokio::test]
    async fn test_handlers_raw() -> Result<(), anyhow::Error> {
        let state: AppState = ServerConfig {
            bind_address: "127.0.0.1".to_string(),
            routes: vec![Route {
                path: "/styles.css".to_string(),
                opts: None,
                kind: RouteKind::Raw {
                    raw: "body{}".into(),
                },
            }],
        }
        .into();

        let app = make_router(&Arc::new(state));
        let req = Request::get("/styles.css").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();

        assert_eq!(res.headers().get("content-length").unwrap(), "6");
        assert_eq!(res.headers().get("content-type").unwrap(), "text/css");

        let body = to_resp_body(res).await;
        assert_eq!(body, "body{}");
        Ok(())
    }
    #[tokio::test]
    async fn test_cors_handlers() -> Result<(), anyhow::Error> {
        let state: AppState = ServerConfig {
            bind_address: "127.0.0.1".to_string(),
            routes: vec![
                Route {
                    path: "/".to_string(),
                    opts: Some(Opts { cors: true }),
                    kind: RouteKind::html("home"),
                },
                Route {
                    path: "/hello".to_string(),
                    opts: None,
                    kind: RouteKind::html("🐥"),
                },
            ],
        }
        .into();

        dbg!(&state);

        let app = make_router(&Arc::new(state));
        let req = Request::get("/").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        for (k, v) in res.headers() {
            dbg!(k);
            dbg!(v);
        }
        let body = to_resp_body(res).await;
        assert_eq!(body, "home");
        Ok(())
    }
}
