use crate::server_config::{Content, RawContent, Route, ServerConfig};
use crate::server_handlers::{built_ins, dynamic_loaders};
use crate::server_signals::ServerSignals;
use crate::server_updates::{Patch, PatchOne};
use actix::{ActorContext, AsyncContext, Running};
use actix_rt::Arbiter;
use anyhow::anyhow;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use axum::Router;
use hyper::header::CONTENT_TYPE;
use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{oneshot, oneshot::Receiver, oneshot::Sender, Mutex};
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
pub struct AppState {
    pub routes: Arc<Mutex<matchit::Router<Content>>>,
    pub dir_bindings: Arc<Mutex<HashMap<String, String>>>,
}

impl actix::Actor for ServerActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = self.config.bind_address.clone();
        let (send_complete, received_stop) = self.install_signals();

        let router = matchit::Router::new();
        let dir_bindings = HashMap::new();

        let app_state = Arc::new(AppState {
            routes: Arc::new(Mutex::new(router)),
            dir_bindings: Arc::new(Mutex::new(dir_bindings)),
        });

        self.app_state = Some(app_state.clone());

        let server = async move {
            let router = Router::new()
                .merge(built_ins(app_state.clone()))
                .merge(dynamic_loaders(app_state.clone()));

            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            tracing::debug!("axum: listening on {}", listener.local_addr().unwrap());

            match axum::serve(listener, router)
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

async fn get_dyn(State(app): State<Arc<AppState>>, uri: Uri, req: Request) -> impl IntoResponse {
    tracing::trace!("get_dyn handler incoming, uri={:?}", uri);
    let v = app.routes.lock().await;
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
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: PatchOne, ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("Handler<PatchOne> for ServerActor");
        let app_state = self
            .app_state
            .as_ref()
            .ok_or(anyhow!("could not access state"))?;
        let s_c = app_state.clone();
        let routes = msg.server_config.routes.clone();
        let update_dn = async move {
            let mut router = s_c.routes.lock().await;
            for route in routes {
                match route.content {
                    Content::Raw { .. } => {
                        let path = route.path.to_str().unwrap();
                        let existing = router.at_mut(path);
                        if let Ok(mut prev) = existing {
                            *prev.value = route.content;
                            tracing::trace!(" └ updated mutable route at {}", path)
                        } else if let Err(err) = existing {
                            match router.insert(path, route.content.clone()) {
                                Ok(_) => {
                                    tracing::trace!(
                                        "  └ inserted {} with {:?}",
                                        path,
                                        route.content
                                    )
                                }
                                Err(_) => {
                                    tracing::error!("  └ could not insert {:?}", err.to_string())
                                }
                            };
                        }
                    }
                    Content::Dir { dir } => {
                        let path = route.path.to_str().unwrap();
                        let mut dir_bindings = s_c.dir_bindings.lock().await;
                        dir_bindings.insert(path.to_owned(), dir.clone());
                        tracing::trace!(" └ updated dir_bindings at {} with {}", path, dir.clone());
                        drop(dir_bindings);
                    }
                }
            }
        };

        Arbiter::current().spawn(update_dn);
        Ok(())
    }
}
