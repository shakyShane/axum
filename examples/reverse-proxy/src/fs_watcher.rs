use actix::{Actor, AsyncContext, Handler, Recipient};
use notify::event::{DataChange, ModifyKind};
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::PathBuf;

pub struct FsWatcher {
    watcher: Option<notify::FsEventWatcher>,
    receivers: Vec<Recipient<FsWatchEvent>>,
}

impl FsWatcher {
    pub fn new() -> Self {
        Self {
            watcher: None,
            receivers: vec![],
        }
    }
}

#[derive(actix::Message)]
#[rtype(result = "Result<(), FsWatchError>")]
pub struct WatchPath {
    pub recipients: Vec<Recipient<FsWatchEvent>>,
    pub path: std::path::PathBuf,
}

#[derive(actix::Message, Debug)]
#[rtype(result = "()")]
pub struct FsWatchEvent {
    pub absolute_path: PathBuf,
}

impl Actor for FsWatcher {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let self_address = ctx.address();
        if let Ok(watcher) =
            notify::recommended_watcher(move |res: Result<notify::Event, _>| match res {
                Ok(event) => match event.kind {
                    EventKind::Any => {}
                    EventKind::Access(_) => {}
                    EventKind::Create(_) => {}
                    EventKind::Modify(modify) => match modify {
                        ModifyKind::Any => {}
                        ModifyKind::Data(data) => match data {
                            DataChange::Any => {}
                            DataChange::Size => {}
                            DataChange::Content => {
                                tracing::debug!("{:?}, {:?}", event.kind, event.paths);
                                self_address.do_send(FsWatchEvent {
                                    absolute_path: event.paths.first().unwrap().into(),
                                })
                            }
                            DataChange::Other => {}
                        },
                        ModifyKind::Metadata(_) => {}
                        ModifyKind::Name(_) => {}
                        ModifyKind::Other => {}
                    },
                    EventKind::Remove(_) => {}
                    EventKind::Other => {}
                },
                Err(e) => {
                    tracing::error!("fswadtcher {:?}", e);
                    println!("watch error: {:?}", e);
                }
            })
        {
            self.watcher = Some(watcher)
        };
    }
}

impl Handler<FsWatchEvent> for FsWatcher {
    type Result = ();
    fn handle(&mut self, msg: FsWatchEvent, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("FsWatchEvent for FsWatcher");
        tracing::trace!("  └ sending to {} receivers", self.receivers.len());
        for x in &self.receivers {
            x.do_send(FsWatchEvent {
                absolute_path: msg.absolute_path.clone(),
            })
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FsWatchError {
    #[error("Watcher error, original error: {0}")]
    Watcher(#[from] notify::Error),
}

impl Handler<WatchPath> for FsWatcher {
    type Result = Result<(), FsWatchError>;

    fn handle(&mut self, msg: WatchPath, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(watcher) = self.watcher.as_mut() {
            match watcher.watch(&msg.path, RecursiveMode::NonRecursive) {
                Ok(_) => {
                    self.receivers.extend(msg.recipients);
                    tracing::debug!("👀 watching! {:?}", msg.path)
                }
                Err(err) => {
                    tracing::error!("cannot add: {}", err);
                    return Err(FsWatchError::Watcher(err));
                }
            }
        }
        Ok(())
    }
}
