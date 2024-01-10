//! Run with
//!
//! ```not_rust
//! cargo run -p example-hello-world
//! ```

use axum::body::Body;
use axum::extract::{MatchedPath, Request, State};
use axum::handler::Handler;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{middleware, response::Html, routing::get, Extension, Router};
use mime_guess::mime;
use std::sync::{Arc, Mutex};
use tower::{Service, ServiceExt};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
struct AppState {
    route: Arc<Mutex<Router>>,
    // pub name: Arc<String>,
}

#[tokio::main]
async fn main() {
    // build our application with a route
    async fn mw(req: Request, next: Next) -> impl IntoResponse {
        let uri = req.uri().clone();
        println!("1 before: {}", uri);
        if uri.to_string() == "/lol.css" {
            let guess = mime_guess::from_path(uri.path());
            let mime = guess
                .first_raw()
                .map(HeaderValue::from_static)
                .unwrap_or_else(|| {
                    HeaderValue::from_str(mime::APPLICATION_OCTET_STREAM.as_ref()).unwrap()
                });

            return Response::builder()
                .status(200)
                .header("content-type", mime)
                .body(":root { --lol:red }".to_string())
                .unwrap()
                .into_response();
        }
        if uri.to_string() == "/" {
            let guess = mime_guess::from_path(uri.path());
            let mime = guess
                .first_raw()
                .map(HeaderValue::from_static)
                .unwrap_or_else(|| HeaderValue::from_str(mime::TEXT_HTML_UTF_8.as_ref()).unwrap());
            return Response::builder()
                .status(200)
                .header("content-type", mime)
                .body("homepage".to_string())
                .unwrap()
                .into_response();
        }
        println!("no exact match, falling back..");
        let response = next.run(req).await;
        println!("1 after: {}", uri.clone());
        response
    }

    let other = AppState {
        route: Arc::new(Mutex::new(
            Router::new().route("/s", get(another)).with_state::<()>(()),
        )),
    };
    // let other = AppState {
    //     name: Arc::new(String::from("lol!")),
    // };

    async fn maybe_serve_dir(State(app): State<AppState>, req: Request) -> impl IntoResponse {
        let uri = req.uri().clone();
        println!("2 before: {}", uri);
        let response = app
            .route
            .lock()
            .unwrap()
            .as_service()
            .ready()
            .await
            .unwrap()
            .call(req)
            .await
            .unwrap();

        return response;
    }

    let app = any(handler)
        .with_state(other)
        .layer(tower::ServiceBuilder::new().layer(middleware::from_fn(mw)));
    // let app = any(handler).layer(
    //     tower::ServiceBuilder::new()
    //         .layer(middleware::from_fn(mw))
    //         .layer(middleware::from_fn(maybe_serve_dir)),
    // );
    // .with_state(other);
    let svc = app.into_make_service();

    // run it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, svc).await.unwrap();
}

async fn handler(State(app): State<AppState>, req: Request) -> Response {
    // let mut lock = app.route.lock().unwrap();
    let mut router = Router::new();

    {
        router = router.nest_service("/assets", ServeDir::new("."));
        router = router.nest_service("/kitten", ServeFile::new("styles.css"));
        router = router.route("/sa", get(another));
    }

    let r = router
        .as_service()
        .ready()
        .await
        .unwrap()
        .call(req)
        .await
        .unwrap()
        .into_response();
    return r;

    // format!("<h1>Hello, World!</h1> = {}", req.uri());
    // let response = app
    //     .route
    //     .lock()
    //     .unwrap()
    //     .as_service()
    //     .ready()
    //     .await
    //     .unwrap()
    //     .call(req)
    //     .await
    //     .unwrap();
    //
    // return response;
    Response::builder()
        .body(Body::empty())
        .unwrap()
        .into_response()
}

async fn another() -> impl IntoResponse {
    "another fn".into_response()
}
