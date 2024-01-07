use crate::server_config::ServerConfig;
use crate::server_signals::ServerSignals;
use actix::{ActorContext, AsyncContext, Running};
use actix_rt::Arbiter;
use axum::routing::get;
use axum::Router;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::{oneshot, oneshot::Receiver, oneshot::Sender};

pub struct ServerActor {
    pub config: ServerConfig,
    pub signals: Option<ServerSignals>,
}

impl ServerActor {
    pub fn new_from_config(config: ServerConfig) -> Self {
        Self {
            config,
            signals: None,
        }
    }
    pub fn install_signals(&mut self) -> (Sender<()>, Receiver<()>) {
        let (stop_server_sender, stop_server_receiver) = oneshot::channel();
        let (shutdown_complete, shutdown_complete_receiver) = oneshot::channel();

        self.signals = Some(ServerSignals {
            stop_msg_sender: Some(stop_server_sender),
            complete_mdg_receiver: Some(shutdown_complete_receiver),
        });

        return (shutdown_complete, stop_server_receiver);
    }
    pub async fn stop_server(&mut self) {}
}

impl actix::Actor for ServerActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = self.config.bind_address.clone();
        let (send_complete, received_stop) = self.install_signals();

        let server = async {
            let app = Router::new().route("/", get(|| async { "foo" }));
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            tracing::debug!("axum: listening on {}", listener.local_addr().unwrap());
            match axum::serve(listener, app)
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
