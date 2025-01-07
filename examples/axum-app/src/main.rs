use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Router, serve};
use rinja::Template;
use tokio::runtime;
use tower_http::trace::TraceLayer;
use tracing::{Level, info};

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .try_init()
        .map_err(Error::Log)?;

    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(Error::Rt)?
        .block_on(amain())
}

async fn amain() -> Result<(), Error> {
    let app = Router::new()
        .route("/", get(counter_index))
        .route("/increment/", post(counter_increment))
        .route("/decrement/", post(counter_decrement))
        .route("/reset/", post(counter_reset))
        .fallback(not_found)
        .with_state(AppState::default())
        .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .map_err(Error::Bind)?;
    if let Ok(addr) = listener.local_addr() {
        info!("Listening on http://{addr}/");
    }
    serve(listener, app).await.map_err(Error::Run)
}

#[derive(thiserror::Error, pretty_error_debug::Debug)]
enum Error {
    #[error("could not setup logger")]
    Log(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("could not setup async runtime")]
    Rt(#[source] std::io::Error),
    #[error("could not bind socket")]
    Bind(#[source] std::io::Error),
    #[error("could not run server")]
    Run(#[source] std::io::Error),
}

async fn counter_index(app_state: State<AppState>) -> Result<Response, AppError> {
    let counter = app_state.counter.load(Ordering::Relaxed);
    let tmpl = CounterIndex { counter };
    Ok(Html(tmpl.render()?).into_response())
}

async fn counter_increment(app_state: State<AppState>) -> Result<Response, AppError> {
    let counter = app_state.counter.fetch_add(1, Ordering::Relaxed) + 1;
    let tmpl = CounterModified { counter };
    Ok(Html(tmpl.render()?).into_response())
}

async fn counter_decrement(app_state: State<AppState>) -> Result<Response, AppError> {
    let counter = app_state.counter.fetch_sub(1, Ordering::Relaxed) - 1;
    let tmpl = CounterModified { counter };
    Ok(Html(tmpl.render()?).into_response())
}

async fn counter_reset(app_state: State<AppState>) -> Result<Response, AppError> {
    app_state.counter.store(0, Ordering::Relaxed);
    let tmpl = CounterModified { counter: 0 };
    Ok(Html(tmpl.render()?).into_response())
}

async fn not_found() -> Result<Response, AppError> {
    Err(AppError(rinja::Error::custom("not found")))
}

#[derive(Debug, Default, Clone)]
struct AppState {
    counter: Arc<AtomicI64>,
}

#[derive(Debug, Template)]
#[template(path = "counter.html")]
struct CounterIndex {
    counter: i64,
}

#[derive(Debug, Template)]
#[template(path = "counter.html", block = "current")]
struct CounterModified {
    counter: i64,
}

#[derive(Debug, Template)]
#[template(path = "error.html")]
struct ErrorPage {
    err: rinja::Error,
}

#[derive(Debug)]
struct AppError(rinja::Error);

impl From<rinja::Error> for AppError {
    fn from(value: rinja::Error) -> Self {
        Self(value)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Ok(msg) = (ErrorPage { err: self.0 }).render() {
            if let Ok(resp) = Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(msg.into())
            {
                return resp;
            }
        }
        (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong").into_response()
    }
}
