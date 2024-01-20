use crate::fs_watcher::FsWatchEvent;
use crate::input::Input;
use crate::server_actor::Stop2;
use crate::server_config::ServerConfig;
use crate::server_updates::{Patch, PatchOne};
use crate::{server_actor, ServerHandler};
use actix::{Actor, AsyncContext, MailboxError};
use futures::future::join_all;

use actix_rt::Arbiter;
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
pub struct Stopped {
    pub bind_address: String,
}

impl actix::Handler<Stopped> for Servers {
    type Result = ();

    fn handle(&mut self, msg: Stopped, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("Handler<Stopped> for Servers {:?}", msg.bind_address);

        let next = self
            .handlers
            .clone()
            .into_iter()
            .filter(|h| h.bind_address != msg.bind_address)
            .collect();
        self.handlers = next;
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
    fn handle(&mut self, msg: FsWatchEvent, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("FsWatchEvent for Servers");
        tracing::trace!("  └ {:?}", msg.absolute_path);

        let self_addr = ctx.address();
        let input = Input::from_yaml_path(&msg.absolute_path);
        tracing::trace!("  └ read input {:?}", input);

        if let Ok(input) = input {
            let existing: Vec<_> = self
                .handlers
                .iter()
                .map(|s| s.bind_address.as_str())
                .collect();

            let new_addresses: Vec<_> = input
                .servers
                .iter()
                .map(|s| s.bind_address.as_str())
                .collect();

            let new_configs: Vec<_> = input
                .servers
                .iter()
                .filter(|s| !existing.contains(&s.bind_address.as_str()))
                .collect();

            let evictions: Vec<_> = self
                .handlers
                .iter()
                .filter(|s| !new_addresses.contains(&s.bind_address.as_str()))
                .map(|h| h.actor_address.clone())
                .collect();

            let duplicates: Vec<_> = input
                .servers
                .iter()
                .filter(|s| existing.contains(&s.bind_address.as_str()))
                .collect();

            tracing::debug!("exising: {:?}", existing);
            tracing::debug!("new_addresses: {:?}", new_addresses);
            tracing::debug!("evicted: {}", evictions.len());
            tracing::debug!("next: {}", new_configs.len());
            tracing::debug!("duplicates: {}", duplicates.len());

            for handler in evictions {
                tracing::trace!("sending Stop2");
                handler.do_send(Stop2);
            }
            // let evictions = async move {
            // };
            //
            // Arbiter::current().spawn(evictions);

            // servers to start

            // servers to update

            // servers to remove

            // let addresses: Vec<_> = input
            //     .servers
            //     .iter()
            //     .map(|s| s.bind_address.as_str())
            //     .collect();
            //
            // let removed: Vec<_> = self
            //     .handlers
            //     .iter()
            //     .filter(|handler| !addresses.contains(&handler.bind_address.as_str()))
            //     .collect();
            //
            // for removed_server in removed {
            //     tracing::trace!("  ❌ stopping bind_address {}", removed_server.bind_address);
            //     // removed_server.actor_address.do_send(Stop2);
            //     let add = removed_server.actor_address.clone();
            //     Arbiter::current().spawn(async move {
            //         add.send(Stop2).await;
            //     });
            // }
            //
            // for server_config in input.servers {
            //     if let Some(matching_child) = self
            //         .handlers
            //         .iter()
            //         .find(|h| h.bind_address == server_config.bind_address)
            //     {
            //         tracing::trace!(
            //             "  └ found matching for bind_address {}",
            //             server_config.bind_address
            //         );
            //         matching_child
            //             .actor_address
            //             .do_send(PatchOne { server_config });
            //     } else {
            //         tracing::debug!("missing {:?}", server_config);
            //         addr.do_send(StartMessage {
            //             server_configs: vec![server_config.clone()],
            //         });
            //         addr.do_send(Patch {
            //             server_configs: vec![server_config],
            //         });
            //     }
            // }
        } else if let Err(e) = input {
            tracing::error!("{:?}", e);
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
