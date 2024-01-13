use actix::{Actor, AsyncContext, Handler, Recipient};
use notify::{ErrorKind, RecursiveMode, Watcher};
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
        if let Ok(mut watcher) =
            notify::recommended_watcher(move |res: Result<notify::Event, _>| match res {
                Ok(event) => {
                    tracing::trace!("{:#?}, {:?}", event.kind, event.paths);
                    self_address.do_send(FsWatchEvent {
                        absolute_path: event.paths.get(0).unwrap().into(),
                    })
                }
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
    fn handle(&mut self, msg: FsWatchEvent, ctx: &mut Self::Context) -> Self::Result {
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

    fn handle(&mut self, msg: WatchPath, ctx: &mut Self::Context) -> Self::Result {
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
