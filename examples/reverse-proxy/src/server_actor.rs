use crate::server_config::{Route, ServerConfig};
use crate::server_handlers::make_router;
use crate::server_signals::ServerSignals;
use crate::server_updates::PatchOne;
use actix::{ActorContext, Running};
use actix_rt::Arbiter;
use anyhow::anyhow;
use axum::response::{IntoResponse, Response};

use hyper::header::CONTENT_TYPE;

use axum_server::Handle;
use std::fmt::Formatter;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{oneshot, oneshot::Receiver, oneshot::Sender, Mutex};

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
    pub fn install_signals(&mut self) -> (Sender<()>, Receiver<()>, Handle) {
        let (stop_server_sender, stop_server_receiver) = oneshot::channel();
        let (shutdown_complete, shutdown_complete_receiver) = oneshot::channel();
        let handle = Handle::new();
        let h2 = handle.clone();

        self.signals = Some(ServerSignals {
            stop_msg_sender: Some(stop_server_sender),
            complete_mdg_receiver: Some(shutdown_complete_receiver),
            handle: Some(handle),
        });

        (shutdown_complete, stop_server_receiver, h2)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub routes: Arc<Mutex<Vec<Route>>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("routes", &"Arc<Mutex<matchit::Router<Route>>>")
            .field("dir_bindings", &"Arc<Mutex<HashMap<String, Route>>")
            .finish()
    }
}

impl actix::Actor for ServerActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        let addr = self.config.bind_address.clone();
        let (send_complete, received_stop, handle) = self.install_signals();

        let app_state = Arc::new(AppState {
            routes: Arc::new(Mutex::new(vec![])),
        });

        self.app_state = Some(app_state.clone());

        let server = async move {
            let router = make_router(&app_state);
            let addr: Result<SocketAddr, _> = addr.parse();

            let Ok(addr) = addr else {
                tracing::error!("axum: could not parse bind_address");
                return;
            };

            let server = axum_server::bind(addr)
                .handle(handle)
                .serve(router.into_make_service());

            match server.await {
                Ok(_) => {
                    tracing::debug!("axum: Server all done");
                    if send_complete.send(()).is_err() {
                        tracing::error!("axum: could not send complete message");
                    }
                }
                Err(_) => {
                    tracing::error!("axum: Server all done, but error");
                }
            }
        };
        Arbiter::current().spawn(server);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::debug!("Server stopping (), {}", &self.config.bind_address);
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::debug!("Server stopped (), {}", &self.config.bind_address);
    }
}

#[allow(dead_code)]
fn text_asset_response(path: &str, css: &str) -> Response {
    let mime = mime_guess::from_path(path);
    let aas_str = mime.first_or_text_plain();
    let cloned = css.to_owned();
    ([(CONTENT_TYPE, aas_str.to_string())], cloned).into_response()
}

#[derive(actix::Message)]
#[rtype(result = "String")]
pub struct Stop2;

impl actix::Handler<Stop2> for ServerActor {
    type Result = Pin<Box<dyn Future<Output = String>>>;

    fn handle(&mut self, _msg: Stop2, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("actor(Server): Stop2");
        ctx.stop();
        // don't accept any more messages
        let Some(signals) = self.signals.take() else {
            todo!("should be unreachable. close signal can only be sent once")
        };
        if let Some(handle) = signals.handle {
            tracing::trace!("actor(Server): shutting down...");
            handle.shutdown();
        }
        if let Some(complete_msg_receiver) = signals.complete_mdg_receiver {
            tracing::debug!("actor(Server): confirmed closed!");
            let bind_address = self.config.bind_address.clone();
            Box::pin(async move {
                complete_msg_receiver.await.unwrap();
                bind_address
            })
        } else {
            todo!("cannot get here?")
        }
    }
}

impl actix::Handler<PatchOne> for ServerActor {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: PatchOne, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("Handler<PatchOne> for ServerActor");
        let app_state = self
            .app_state
            .as_ref()
            .ok_or(anyhow!("could not access state"))?;
        let app_state_clone = app_state.clone();
        let routes = msg.server_config.routes.clone();
        let update_dn = async move {
            let mut mut_routes = app_state_clone.routes.lock().await;
            *mut_routes = routes;
        };

        Arbiter::current().spawn(update_dn);
        Ok(())
    }
}
