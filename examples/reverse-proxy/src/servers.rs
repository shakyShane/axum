use crate::fs_watcher::FsWatchEvent;
use crate::input::Input;
use crate::server_actor::Stop2;
use crate::server_config::ServerConfig;
use crate::server_updates::{Patch, PatchOne};
use crate::{server_actor, ServerHandler};
use actix::Actor;
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
    fn started(&mut self, _ctx: &mut Self::Context) {}
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct StartMessage {
    pub server_configs: Vec<ServerConfig>,
}

impl actix::Handler<StartMessage> for Servers {
    type Result = ();

    fn handle(&mut self, msg: StartMessage, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("creating server actors {:?}", msg.server_configs);

        let server_handlers = msg
            .server_configs
            .into_iter()
            .map(|server_config| {
                let server = server_actor::ServerActor::new_from_config(server_config.clone());
                let actor_addr = server.start();
                ServerHandler {
                    actor_address: actor_addr,
                    bind_address: server_config.bind_address.clone(),
                }
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

    fn handle(&mut self, _msg: StopMsg, _ctx: &mut Self::Context) -> Self::Result {
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

impl actix::Handler<FsWatchEvent> for Servers {
    type Result = ();

    /// todo: accept more messages here
    fn handle(&mut self, msg: FsWatchEvent, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("FsWatchEvent for Servers");
        tracing::trace!("  └ {:?}", msg.absolute_path);
        let is_input = true;
        if is_input {
            // todo(Shane): implement removal of routes
            let input = Input::from_yaml_path(&msg.absolute_path);
            if let Ok(input) = input {
                tracing::trace!("  └ read input {:?}", input);

                for server_config in input.servers {
                    if let Some(matching_child) = self
                        .handlers
                        .iter()
                        .find(|h| h.bind_address == server_config.bind_address)
                    {
                        tracing::trace!(
                            "  └ found matching for bind_address {}",
                            server_config.bind_address
                        );
                        matching_child
                            .actor_address
                            .do_send(PatchOne { server_config });
                    }
                }
            } else if let Err(e) = input {
                tracing::error!("{:?}", e);
            }
        } else {
            tracing::debug!("  └ discarding change event for {:?}", msg.absolute_path);
        }
    }
}

impl actix::Handler<Patch> for Servers {
    type Result = ();

    fn handle(&mut self, msg: Patch, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("Handler<Patch> for Servers");
        tracing::trace!(
            "  └ {} incoming msg.server_configs",
            msg.server_configs.len()
        );
        for server_config in msg.server_configs {
            if let Some(matching_child) = self
                .handlers
                .iter()
                .find(|h| h.bind_address == server_config.bind_address)
            {
                tracing::trace!(
                    "  └ found matching for bind_address {}",
                    server_config.bind_address
                );
                tracing::trace!("    └ sending PatchOne to {}", server_config.bind_address);
                matching_child
                    .actor_address
                    .do_send(PatchOne { server_config });
            }
        }
    }
}
