mod fs_watcher;
mod input;
mod server_actor;
mod server_config;
mod server_signals;
mod server_updates;
mod servers;

use crate::fs_watcher::FsWatcher;
use crate::input::Input;
use crate::server_config::{Route, ServerConfig};
use crate::server_updates::Patch;
use crate::servers::{Servers, StartMessage};
use actix::dev::MessageResponse;
use actix::prelude::*;
use actix::Running::Stop;
use anyhow::Error;
use axum::body::Bytes;
use axum::extract::FromRef;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::{
    body::Body,
    extract::{Request, State},
    handler::HandlerWithoutStateExt,
    http::uri::Uri,
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use hyper::StatusCode;
use hyper_util::{client::legacy::connect::HttpConnector, rt::TokioExecutor};
use std::env::current_dir;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time;
use tokio::time::{interval, sleep};
use tracing::instrument::WithSubscriber;
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

#[derive(actix::Message)]
#[rtype(result = "usize")]
struct Ping(usize);

#[derive(Debug, Clone)]
struct ServerHandler {
    actor_address: actix::Addr<server_actor::ServerActor>,
    bind_address: String,
}
fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "example_reverse_proxy=trace,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let system = System::new();

    system.block_on(async {
        let servers = Servers::new();
        let watcher = FsWatcher::new();

        tracing::trace!("[actor] starting servers...");
        let servers_addr = servers.start();

        tracing::trace!("[actor] starting watcher");
        let watcher_addr = watcher.start();

        let cwd = PathBuf::from(current_dir().unwrap().to_string_lossy().to_string());
        let input_path = cwd.join("fixtures/input.yml");
        let input = Input::from_yaml_path(&input_path);

        if let Err(error) = input {
            tracing::error!("{}", error);
            return;
        }

        let Ok(input) = input else {
            todo!("unreachable");
        };

        watcher_addr.do_send(crate::fs_watcher::WatchPath {
            recipients: vec![servers_addr.clone().recipient()],
            path: input_path,
        });

        let servers_done = servers_addr
            .send(StartMessage {
                server_configs: input.servers.clone(),
            })
            .await;

        servers_addr.do_send(Patch {
            server_configs: input.servers.clone(),
        });

        sleep(Duration::from_secs(10000)).await;

        match servers_addr.send(Servers::STOP_MSG).await {
            Ok(_) => tracing::debug!("all stopped"),
            Err(_) => tracing::debug!("error stopping all"),
        }

        // for ref server_handler in server_handlers {
        //     match server_handler.actor_address.send(server::Stop2).await {
        //         Ok(v) => {
        //             tracing::trace!("wait over");
        //         }
        //         Err(_) => {}
        //     }
        // }
        // sleep(Duration::from_secs(10)).await;
        // println!("restarting...");

        // let server_handlers = servers
        //     .into_iter()
        //     .map(|bind_address| {
        //         let server = server::Server {
        //             config: ServerConfig {
        //                 bind_address: bind_address.to_string(),
        //             },
        //         };
        //         let actor_addr = server.start();
        //         let server_address = ServerHandler {
        //             actor_address: actor_addr,
        //         };
        //         server_address
        //     })
        //     .collect::<Vec<ServerHandler>>();

        // sleep(Duration::from_secs(200)).await;
        //
        // println!("all done...");

        // dbg!(&server_handlers);
        // let addr = MyActor { count: 10 }.start();
        //
        // // send message and get future for result
        // let res = addr.send(Ping(10)).await;
        //
        // // handle() returns tokio handle
        // println!("RESULT: {}", res.unwrap() == 20);
        //
        // tracing_subscriber::registry()
        //     .with(
        //         tracing_subscriber::EnvFilter::try_from_default_env()
        //             .unwrap_or_else(|_| "example_reverse_proxy=debug,tower_http=debug".into()),
        //     )
        //     .with(tracing_subscriber::fmt::layer())
        //     .init();
        //
        // let client: Client =
        //     hyper_util::client::legacy::Client::<(), ()>::builder(TokioExecutor::new())
        //         .build(HttpConnector::new());
        //
        // let config = Config {
        //     host: "example.com".into(),
        // };
        //
        // let app_state = AppState { config, client };
        //
        // let app = Router::new()
        //     .route("/", get(handler).head(handler))
        //     .layer(middleware::from_fn(print_request_response))
        //     .with_state(app_state);
        //
        // let listener = tokio::net::TcpListener::bind("127.0.0.1:5000")
        //     .await
        //     .unwrap();
        //
        // println!("listening on {}", listener.local_addr().unwrap());
        //
        // let n = addr.send(Stop2).await;
        //
        // axum::serve(listener, app).await.unwrap()
    });

    // stop system and exit
    System::current().stop();
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
    let uri = req.uri().to_string();
    let (parts, body) = req.into_parts();
    let bytes = buffer_and_print("request", &uri, body).await?;
    let req = Request::from_parts(parts, Body::from(bytes));

    let res = next.run(req).await;

    let (parts, body) = res.into_parts();
    let bytes = buffer_and_print("response", &uri, body).await?;
    let res = Response::from_parts(parts, Body::from(bytes));

    Ok(res)
}

async fn buffer_and_print<B>(
    direction: &str,
    uri: &str,
    body: B,
) -> Result<Bytes, (StatusCode, String)>
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
        tracing::debug!("{direction} {uri} body = {body:?}");
    }

    Ok(bytes)
}
