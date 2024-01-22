use crate::fs_watcher::FsWatchEvent;
use crate::input::Input;
use crate::server_actor::{ServerActor, Stop2};
use crate::server_config::ServerConfig;
use crate::server_updates::{Patch, PatchOne};
use crate::{server_actor, ServerHandler};
use actix::{Actor, Addr, AsyncContext};
use futures::future::join_all;
use std::collections::{HashMap, HashSet};

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

        tracing::trace!(
            "Handler<Stopped> remaining handlers: {}",
            self.handlers.len()
        )
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
            let curr: HashSet<_> = self
                .handlers
                .iter()
                .map(|s| s.bind_address.as_str())
                .collect();

            let actor_addresses: HashMap<String, Addr<ServerActor>> = self
                .handlers
                .iter()
                .map(|h| (h.bind_address.to_owned(), h.actor_address.clone()))
                .collect();

            let lookup_next: HashMap<String, ServerConfig> = input
                .servers
                .iter()
                .map(|h| (h.bind_address.to_owned(), h.clone()))
                .collect();

            let next: HashSet<_> = input
                .servers
                .iter()
                .map(|s| s.bind_address.as_str())
                .collect();

            let shutdown: Vec<String> = curr.difference(&next).map(|s| String::from(*s)).collect();
            let startup: Vec<String> = next.difference(&curr).map(|s| String::from(*s)).collect();
            let patch: Vec<String> = curr.intersection(&next).map(|s| String::from(*s)).collect();

            let shutdown_addrs: Vec<_> = shutdown
                .into_iter()
                .filter_map(|bind| actor_addresses.get(&bind).map(|c| c.to_owned()))
                .collect();

            let startup_jobs: Vec<_> = startup
                .into_iter()
                .filter_map(|bind| lookup_next.get(&bind).map(|c| c.to_owned()))
                .collect();

            let patch_jobs: Vec<_> = patch
                .into_iter()
                .map(
                    |bind_a| match (lookup_next.get(&bind_a), actor_addresses.get(&bind_a)) {
                        (Some(config), Some(handle)) => (config.to_owned(), handle.to_owned()),
                        _ => unreachable!("if we get here it's a bug"),
                    },
                )
                .collect();

            tracing::debug!("{} shutdown jobs", shutdown_addrs.len());
            tracing::debug!("{} startup jobs", startup_jobs.len());
            tracing::debug!("{} patch jobs", patch_jobs.len());

            let async_jobs = async move {
                for addr in shutdown_addrs {
                    match addr.send(Stop2).await {
                        Ok(bind_address) => self_addr.do_send(Stopped { bind_address }),
                        Err(e) => {
                            tracing::error!("{}", e);
                        }
                    }
                }

                if !startup_jobs.is_empty() {
                    self_addr.do_send(StartMessage {
                        server_configs: startup_jobs,
                    });
                }

                for (config, addr) in patch_jobs {
                    addr.do_send(PatchOne {
                        server_config: config,
                    });
                }
            };

            Arbiter::current().spawn(async_jobs);
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
