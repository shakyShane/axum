use crate::server_config::{Content, RawContent, Route, ServerConfig};
use crate::server_signals::ServerSignals;
use crate::server_updates::{Patch, PatchOne};
use actix::{ActorContext, AsyncContext, Running};
use actix_rt::Arbiter;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use hyper::header::CONTENT_TYPE;
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, oneshot::Receiver, oneshot::Sender};
use tower::{Service, ServiceExt};
use tower_http::services::ServeDir;

pub struct ServerActor {
    pub config: ServerConfig,
    pub signals: Option<ServerSignals>,
    pub app_state: Option<Arc<AppState>>,
}

impl ServerActor {
    pub fn new_from_config(config: ServerConfig) -> Self {
        Self {
            config,
            signals: None,
            app_state: None,
        }
    }
    pub fn install_signals(&mut self) -> (Sender<()>, Receiver<()>) {
        let (stop_server_sender, stop_server_receiver) = oneshot::channel();
        let (shutdown_complete, shutdown_complete_receiver) = oneshot::channel();

        self.signals = Some(ServerSignals {
            stop_msg_sender: Some(stop_server_sender),
            complete_mdg_receiver: Some(shutdown_complete_receiver),
        });

        (shutdown_complete, stop_server_receiver)
    }
}

#[derive(Clone)]
struct AppState {
    pub routes: Arc<Mutex<matchit::Router<Content>>>,
}

impl actix::Actor for ServerActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = self.config.bind_address.clone();
        let (send_complete, received_stop) = self.install_signals();

        let router = matchit::Router::new();
        let app_state = Arc::new(AppState {
            routes: Arc::new(Mutex::new(router)),
        });

        self.app_state = Some(app_state.clone());

        let server = async move {
            let app = any(get_dyn).with_state(app_state.clone());
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            tracing::debug!("axum: listening on {}", listener.local_addr().unwrap());

            match axum::serve(listener, app.into_make_service())
                .with_graceful_shutdown(async { received_stop.await.unwrap() })
                .await
            {
                Ok(_) => {
                    tracing::debug!("axum: Server all done");
                    match send_complete.send(()) {
                        Ok(_) => {}
                        Err(_) => {}
                    };
                }
                Err(_) => {
                    tracing::error!("axum: Server all done, but error");
                }
            }
        };
        Arbiter::current().spawn(server);
    }

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        tracing::debug!("Server stopping (), {}", &self.config.bind_address);
        Running::Stop
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        tracing::debug!("Server stopped (), {}", &self.config.bind_address);
    }
}

async fn raw_loader_alt(
    State(app): State<Arc<AppState>>,
    req: Request,
    uri: Uri,
    next: Next,
) -> impl IntoResponse {
    tracing::trace!("-> raw_loader_alt");
    {
        let v = app.routes.lock().unwrap();
        // let matched = v.at(uri.path());
        println!("v===s");
    }
    let svc = ServeDir::new(".");
    let result = svc.oneshot(req).await;
    tracing::trace!("<- raw_loader_alt");
    return result;
}

async fn raw_loader(
    State(app): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    tracing::trace!("-> raw_loader");
    // let response = next.run(req).await;
    let mut router = Router::new();

    {
        router = router.nest_service("/assets", ServeDir::new("."));
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

    tracing::trace!("<- raw_loader");

    return r;
}

async fn get_dyn(State(app): State<Arc<AppState>>, uri: Uri, req: Request) -> impl IntoResponse {
    tracing::trace!("get_dyn handler incoming, uri={:?}", uri);
    let v = app.routes.lock().unwrap();
    let matched = v.at(uri.path());

    let Ok(matched) = matched else {
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
        } => Html(html.clone()).into_response(),
        Content::Raw {
            raw: RawContent::Css { css },
        } => text_asset_response(uri.path(), css),
        Content::Raw {
            raw: RawContent::Js { js },
        } => text_asset_response(uri.path(), js),
        Content::Dir { .. } => "{}".into_response(),
    }
}

fn text_asset_response(path: &str, css: &str) -> Response {
    let mime = mime_guess::from_path(path);
    let aas_str = mime.first_or_text_plain();
    let cloned = css.to_owned();
    ([(CONTENT_TYPE, aas_str.to_string())], cloned).into_response()
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct Stop2;

impl actix::Handler<Stop2> for ServerActor {
    type Result = Pin<Box<dyn Future<Output = ()>>>;

    fn handle(&mut self, msg: Stop2, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("actor(Server): Stop2");
        let Some(signals) = self.signals.take() else {
            todo!("how can we get here?")
        };
        if let Some(stop_msg_sender) = signals.stop_msg_sender {
            tracing::trace!("actor(Server): state when trying to stop {:?}", ctx.state());
            match stop_msg_sender.send(()) {
                Ok(_) => tracing::trace!("actor(Server): sending signal to shutdown"),
                Err(_) => tracing::error!("actor(Server): could not send signal"),
            }
        } else {
            tracing::error!("actor(Server): could not take sender");
            todo!("cannot get here?")
        }
        if let Some(complete_msg_receiver) = signals.complete_mdg_receiver {
            Box::pin(async {
                complete_msg_receiver.await.unwrap();
            })
        } else {
            todo!("cannot get here?")
        }
    }
}

impl actix::Handler<PatchOne> for ServerActor {
    type Result = ();

    fn handle(&mut self, msg: PatchOne, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("Handler<PatchOne> for ServerActor");
        if let Some(app_state) = &self.app_state {
            let mut router = app_state.routes.lock().unwrap();
            for route in msg.server_config.routes {
                let path = route.path.to_str().unwrap();
                let existing = router.at_mut(path);
                if let Ok(mut prev) = existing {
                    *prev.value = route.content;
                    tracing::trace!(" └ updated mutable route at {}", path)
                } else if let Err(err) = existing {
                    match router.insert(path, route.content.clone()) {
                        Ok(_) => tracing::trace!("  └ inserted {} with {:?}", path, route.content),
                        Err(_) => tracing::error!("  └ could not insert {:?}", err.to_string()),
                    };
                }
            }
        }
    }
}
