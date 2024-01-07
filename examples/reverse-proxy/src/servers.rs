use crate::server_actor::Stop2;
use crate::server_config::ServerConfig;
use crate::{server_actor, ServerHandler};
use actix::{Actor, Running};
use futures::future::join_all;
use std::future::Future;
use std::pin::Pin;

pub struct Servers {
    handlers: Vec<ServerHandler>,
}

impl Servers {
    pub fn new() -> Self {
        Self { handlers: vec![] }
    }
    pub const STOP_MSG: StopMsg = StopMsg;
}

impl Actor for Servers {
    type Context = actix::Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {}
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct StartMessage {
    pub server_configs: Vec<ServerConfig>,
}

impl actix::Handler<StartMessage> for Servers {
    type Result = ();

    fn handle(&mut self, msg: StartMessage, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("creating server actors {:#?}", msg.server_configs);

        let server_handlers = msg
            .server_configs
            .into_iter()
            .map(|server_config| {
                let server = server_actor::ServerActor::new_from_config(server_config);
                let actor_addr = server.start();
                let server_address = ServerHandler {
                    actor_address: actor_addr,
                };
                server_address
            })
            .collect::<Vec<ServerHandler>>();

        self.handlers.extend(server_handlers);
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct StopMsg;

impl actix::Handler<StopMsg> for Servers {
    type Result = Pin<Box<dyn Future<Output = ()>>>;

    fn handle(&mut self, msg: StopMsg, ctx: &mut Self::Context) -> Self::Result {
        let aaddresses = self.handlers.clone();

        Box::pin(async move {
            tracing::debug!("stopping {} servers", aaddresses.len());
            let fts = aaddresses
                .iter()
                .map(|handler| handler.actor_address.send(Stop2))
                .collect::<Vec<_>>();
            join_all(fts).await;
        })
    }
}
