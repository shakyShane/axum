//! Run with
//!
//! ```not_rust
//! cargo run -p example-hello-world
//! ```

use axum::body::Body;
use axum::extract::{MatchedPath, Request, State};
use axum::handler::Handler;
use axum::http::HeaderValue;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, post, MethodRouter};
use axum::*;
use mime_guess::mime;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::{Service, ServiceBuilder, ServiceExt};
use tower_http::classify::StatusInRangeFailureClass::StatusCode;
use tower_http::compression::CompressionLayer;
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

#[derive(Clone)]
struct MyState {
    name: Arc<Mutex<String>>,
}

#[tokio::main]
async fn main() {
    // build our application with a route

    let state = Arc::new(MyState {
        name: Arc::new(Mutex::new(String::from("shane"))),
    });

    let router = Router::new()
        .merge(built_ins(state.clone()))
        .merge(dynamic_loaders(state.clone()));

    // run it
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, router).await.unwrap();
}

fn built_ins(state: Arc<MyState>) -> Router {
    async fn handler(State(app): State<Arc<MyState>>) -> impl IntoResponse {
        let v = app.name.lock().await;
        v.to_owned().into_response()
    }

    route("/foo", any(handler)).with_state(state.clone())
}

fn dynamic_loaders(state: Arc<MyState>) -> Router {
    Router::new()
        .route("/", any(never))
        .layer(from_fn_with_state(state.clone(), serve_dir_loader))
        // .layer(CompressionLayer::new())
        .layer(from_fn_with_state(state.clone(), raw_loader))
        .with_state(state.clone())
}

async fn never(State(app): State<Arc<MyState>>, req: Request) -> impl IntoResponse {
    println!("    -> never");
    (
        http::StatusCode::NOT_FOUND,
        format!("unreachable {}", req.uri()),
    )
}

async fn serve_dir_loader(State(app): State<Arc<MyState>>, req: Request, next: Next) -> Response {
    println!("  -> serve_dir_loader");

    if req.uri().path() == "/Cargo.toml" || req.uri().path() == "/CHANGELOG.md" {
        let v = app.name.lock().await;
        format!("--{}", v.to_owned()).into_response();

        let s = ServeDir::new(".");
        let mut service = ServiceBuilder::new()
            .boxed()
            .layer(CompressionLayer::new())
            .service(s);

        let r = service.ready().await.unwrap().call(req).await;
        let r = r.into_response();
        return r;
    }

    if req.uri().path() == "/deny.toml" {
        let v = app.name.lock().await;
        format!("--{}", v.to_owned()).into_response();

        let mut s = ServeDir::new(".");
        let r = s.oneshot(req).await;
        let r = r.into_response();
        return r;
    }

    let r = next.run(req).await;
    println!("  <- serve_dir_loader");
    r.into_response()
}

async fn raw_loader(
    State(app): State<Arc<MyState>>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    println!("-> raw_loader");

    if req.uri().path() == "/style.css" {
        let v = app.name.lock().await;
        let mut r = format!("--css{}", v.to_owned()).into_response();
        r.headers_mut()
            .insert("shane", HeaderValue::from_static("lol"));
        return r;
    }

    let r = next.run(req).await;
    println!("<- raw_loader");
    r
}

fn route(path: &str, method_router: MethodRouter<Arc<MyState>>) -> Router<Arc<MyState>> {
    Router::new().route(path, method_router)
}

async fn raw_loader_2(req: Request) -> Response {
    println!("-> raw_loader_2");
    let service = ServeDir::new(".");
    let result = service.oneshot(req).await;
    let r = result.into_response();
    println!("<- raw_loader_2");
    r
}

async fn raw_loader_3(req: Request, next: Next) -> Response {
    println!("  -> raw_loader_3");
    let r = "hello raw_loader_3".into_response();
    // let r = next.run(req).await;
    println!("  <- raw_loader_3");
    r
}

// #[tokio::main]
// async fn main2() {
//     // build our application with a route
//     let mut router = matchit::Router::new();
//     router
//         .insert(
//             "/home",
//             crate::Content::Raw(RawContent::Html(String::from("haha!"))),
//         )
//         .unwrap();
//
//     let other = Arc::new(AppState {
//         routes: Arc::new(Mutex::new(router)),
//     });
//
//     async fn maybe_serve_dir(req: Request, next: Next) -> impl IntoResponse {
//         let response = next.run(req).await;
//         return response.into_response();
//     }
//
//     let app = any(catch_all)
//         .layer(
//             tower::ServiceBuilder::new()
//                 .layer(middleware::from_fn_with_state(other.clone(), raw_loader)),
//         )
//         .with_state(other.clone());
//
//     let svc = app.into_make_service();
//
//     // run it
//     let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
//         .await
//         .unwrap();
//     println!("listening on {}", listener.local_addr().unwrap());
//     axum::serve(listener, svc).await.unwrap();
// }
//
// async fn catch_all(State(app): State<Arc<AppState>>, req: Request) -> Response {
//     // let a = app.name.lock().unwrap();
//     {
//         println!("locking a ting...");
//         let mut locked = app.routes.lock().unwrap();
//         let m = locked.at("/home");
//         dbg!(m);
//         let mut next = matchit::Router::new();
//         next.insert(
//             "/home",
//             crate::Content::Raw(RawContent::Css(String::from("some nice css"))),
//         )
//         .unwrap();
//         *locked = next;
//     };
//
//     {
//         let mut locked = app.routes.lock().unwrap();
//         let m = locked.at("/home");
//         dbg!(m);
//     }
//
//     let mut router = Router::new();
//
//     {
//         router = router.nest_service("/assets", ServeDir::new("."));
//         router = router.nest_service("/kitten", ServeFile::new("styles.css"));
//         router = router.route("/sa", get(another));
//     }
//
//     let r = router
//         .as_service()
//         .ready()
//         .await
//         .unwrap()
//         .call(req)
//         .await
//         .unwrap()
//         .into_response();
//     return r;
//
//     // format!("<h1>Hello, World!</h1> = {}", req.uri());
//     // let response = app
//     //     .route
//     //     .lock()
//     //     .unwrap()
//     //     .as_service()
//     //     .ready()
//     //     .await
//     //     .unwrap()
//     //     .call(req)
//     //     .await
//     //     .unwrap();
//     //
//     // return response;
//     Response::builder()
//         .body(Body::empty())
//         .unwrap()
//         .into_response()
// }
//
// async fn raw_loader(
//     State(app): State<Arc<AppState>>,
//     req: Request,
//     next: Next,
// ) -> impl IntoResponse {
//     let uri = req.uri().clone();
//     println!("1 before: {}", uri);
//
//     {
//         let mut locked = app.routes.lock().unwrap();
//         let m = locked.at("/home");
//         dbg!(m);
//     }
//
//     if uri.to_string() == "/lol.css" {
//         let guess = mime_guess::from_path(uri.path());
//         let mime = guess
//             .first_raw()
//             .map(HeaderValue::from_static)
//             .unwrap_or_else(|| {
//                 HeaderValue::from_str(mime::APPLICATION_OCTET_STREAM.as_ref()).unwrap()
//             });
//
//         return Response::builder()
//             .status(200)
//             .header("content-type", mime)
//             .body(":root { --lol:red }".to_string())
//             .unwrap()
//             .into_response();
//     }
//     if uri.to_string() == "/" {
//         let guess = mime_guess::from_path(uri.path());
//         let mime = guess
//             .first_raw()
//             .map(HeaderValue::from_static)
//             .unwrap_or_else(|| HeaderValue::from_str(mime::TEXT_HTML_UTF_8.as_ref()).unwrap());
//         return Response::builder()
//             .status(200)
//             .header("content-type", mime)
//             .body("homepage".to_string())
//             .unwrap()
//             .into_response();
//     }
//     println!("no exact match, falling back..");
//     let response = next.run(req).await;
//     println!("1 after: {}", uri.clone());
//     response
// }
//
// async fn another() -> impl IntoResponse {
//     "another fn".into_response()
// }
