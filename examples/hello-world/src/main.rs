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
use std::fmt::Formatter;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower::{Service, ServiceExt};
use tower_http::services::{ServeDir, ServeFile};
#[derive(Clone)]
struct AppState {
    pub routes: Arc<Mutex<matchit::Router<Content>>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Clone, Debug)]
struct Route {
    pub path: PathBuf,
    pub content: Content,
}

#[derive(Clone, Debug)]
enum Content {
    Raw(RawContent),
    Dir(String),
}

#[derive(Clone, Debug)]
enum RawContent {
    Html(String),
    Css(String),
    Js(String),
}

#[tokio::main]
async fn main() {
    // build our application with a route
    async fn raw_loader(
        State(app): State<Arc<AppState>>,
        req: Request,
        next: Next,
    ) -> impl IntoResponse {
        let uri = req.uri().clone();
        println!("1 before: {}", uri);

        {
            let mut locked = app.routes.lock().unwrap();
            let m = locked.at("/home");
            dbg!(m);
        }

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

    let mut router = matchit::Router::new();
    router
        .insert(
            "/home",
            crate::Content::Raw(RawContent::Html(String::from("haha!"))),
        )
        .unwrap();

    let other = Arc::new(AppState {
        routes: Arc::new(Mutex::new(router)),
    });

    async fn maybe_serve_dir(req: Request, next: Next) -> impl IntoResponse {
        let response = next.run(req).await;
        return response.into_response();
    }

    let app = any(handler)
        .layer(
            tower::ServiceBuilder::new()
                .layer(middleware::from_fn_with_state(other.clone(), raw_loader)),
        )
        .with_state(other.clone());
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

async fn handler(State(app): State<Arc<AppState>>, req: Request) -> Response {
    // let a = app.name.lock().unwrap();
    {
        println!("locking a ting...");
        let mut locked = app.routes.lock().unwrap();
        let m = locked.at("/home");
        dbg!(m);
        let mut next = matchit::Router::new();
        next.insert(
            "/home",
            crate::Content::Raw(RawContent::Css(String::from("some nice css"))),
        )
        .unwrap();
        *locked = next;
    };

    {
        let mut locked = app.routes.lock().unwrap();
        let m = locked.at("/home");
        dbg!(m);
    }

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
