use crate::server_config::{Route, ServerConfig};
use crate::server_handlers::make_router;
use crate::server_signals::ServerSignals;
use crate::server_updates::PatchOne;
use actix::{ActorContext, AsyncContext, Running};
use actix_rt::Arbiter;
use anyhow::anyhow;
use axum::response::{IntoResponse, Response};

use hyper::header::CONTENT_TYPE;

use axum_server::Handle;
use std::fmt::Formatter;
use std::future::Future;
use std::net::{SocketAddr, TcpListener};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{oneshot, oneshot::Sender, Mutex};

pub struct ServerActor {
    pub config: ServerConfig,
    pub signals: Option<ServerSignals>,
    pub app_state: Option<Arc<AppState>>,
    pub addr: Option<SocketAddr>,
}

impl ServerActor {
    pub fn new_from_config(config: ServerConfig) -> Self {
        Self {
            config,
            signals: None,
            app_state: None,
            addr: None,
        }
    }
    pub fn install_signals(&mut self) -> (Sender<()>, Handle) {
        let (shutdown_complete, shutdown_complete_receiver) = oneshot::channel();
        let handle = Handle::new();
        let h2 = handle.clone();

        self.signals = Some(ServerSignals {
            complete_mdg_receiver: Some(shutdown_complete_receiver),
            handle: Some(handle),
        });

        (shutdown_complete, h2)
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

    fn started(&mut self, _ctx: &mut Self::Context) {}

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        tracing::debug!(
            "{:?} [lifecycle] Server stopping",
            &self.config.bind_address
        );
        Running::Stop
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::debug!("{:?} [lifecycle] Server stopped", &self.config.bind_address);
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
#[rtype(result = "Result<(Option<SocketAddr>, actix::Addr<ServerActor>), anyhow::Error>")]
pub struct Listen;

impl actix::Handler<Listen> for ServerActor {
    type Result = Pin<
        Box<
            dyn Future<
                Output = Result<(Option<SocketAddr>, actix::Addr<ServerActor>), anyhow::Error>,
            >,
        >,
    >;

    fn handle(&mut self, msg: Listen, ctx: &mut Self::Context) -> Self::Result {
        let bind_address = self.config.bind_address.clone();
        tracing::debug!("actor started for {:?}", bind_address);
        let (send_complete, handle) = self.install_signals();
        let h1 = handle.clone();
        let h2 = handle.clone();

        let app_state = Arc::new(AppState {
            routes: Arc::new(Mutex::new(self.config.routes.clone())),
        });

        self.app_state = Some(app_state.clone());
        let self_addr = ctx.address();
        let self_addr_2 = self_addr.clone();

        let server = async move {
            let router = make_router(&app_state);
            let socket_addr: Result<SocketAddr, _> = bind_address
                .clone()
                .unwrap_or_else(|| format!("127.0.0.1:{}", get_available_port().expect("port?")))
                .parse();

            let Ok(addr) = socket_addr else {
                tracing::error!("{:?} [started] could not parse bind_address", bind_address);
                return;
            };

            tracing::debug!("listening on {:?}", addr);
            self_addr_2.do_send(Listening { addr });

            let server = axum_server::bind(addr)
                .handle(h1)
                .serve(router.into_make_service());

            match server.await {
                Ok(_) => {
                    tracing::debug!("{:?} [started] Server all done", bind_address);
                    if send_complete.send(()).is_err() {
                        tracing::error!(
                            "{:?} [started] could not send complete message",
                            bind_address
                        );
                    }
                }
                Err(_) => {
                    tracing::error!("{:?} [started] Server all done, but error", bind_address);
                }
            }
        };
        Arbiter::current().spawn(server);
        let self_addr = self_addr.clone();
        Box::pin(async move {
            let re = h2.listening().await;
            Ok((re, self_addr))
        })
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct Listening {
    addr: SocketAddr,
}

impl actix::Handler<crate::server_actor::Listening> for ServerActor {
    type Result = ();

    fn handle(&mut self, msg: Listening, ctx: &mut Self::Context) -> Self::Result {
        self.addr = Some(msg.addr);
    }
}

#[derive(actix::Message)]
#[rtype(result = "SocketAddr")]
pub struct Stop2;

impl actix::Handler<Stop2> for ServerActor {
    type Result = Pin<Box<dyn Future<Output = SocketAddr>>>;

    fn handle(&mut self, _msg: Stop2, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("{:?} [Stop2]", self.addr);

        ctx.stop();

        // don't accept any more messages
        let Some(signals) = self.signals.take() else {
            todo!("should be unreachable. close signal can only be sent once")
        };
        let Some(handle) = signals.handle else {
            todo!("cannot get here handle cannnot be absent can it??")
        };
        tracing::trace!("{:?} using handle to shutdown", self.config.bind_address);
        handle.shutdown();
        if let Some(complete_msg_receiver) = signals.complete_mdg_receiver {
            tracing::debug!("{:?} confirmed closed via signal", self.addr);
            let c = self.addr.clone();
            Box::pin(async move {
                complete_msg_receiver.await.unwrap();
                c.expect("needs a valid bind address here")
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

pub fn get_available_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|socket_addr| socket_addr.port())
        .ok()
}
