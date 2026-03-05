use anyhow::Result;
use axum::{
    body::Body,
    http::Request,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::{
    cors::{CorsLayer, Any},
    trace::TraceLayer,
};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

mod config;
mod handlers;
mod pipeline;
mod stages;

use handlers::AppState;
use pipeline::TextCleanupPipeline;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(long, env = "PORT", default_value_t = 8787)]
    port: u16,

    /// Path to ONNX punctuation model
    #[arg(long, default_value = "models/pcs_47lang.onnx")]
    model_path: PathBuf,

    /// Path to tokenizer file (tokenizer.json)
    #[arg(long, default_value = "models/tokenizer.json")]
    tokenizer_path: PathBuf,

    /// Path to Hunspell dictionary directory
    #[arg(long, default_value = "models/dictionaries")]
    dict_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    let args = Args::parse();

    info!("Starting OpenWhisper Cleanup Server on port {}", args.port);

    // Initialize pipeline
    let pipeline = Arc::new(TextCleanupPipeline::new(
        args.model_path.clone(),
        args.tokenizer_path.clone(),
        args.dict_dir.clone(),
    ));

    let state = AppState { pipeline };

    // Build router
    let app = Router::new()
        .route("/v1/chat/completions", post(handlers::chat_completions))
        .route("/v1/responses", post(handlers::responses))
        .route("/v1/models", get(handlers::list_models))
        .fallback(handlers::fallback)
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(middleware::from_fn(log_all_requests))
        .layer(TraceLayer::new_for_http());

    // Run server
    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on {}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn log_all_requests(req: Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    let has_auth = headers.contains_key("authorization");

    warn!(
        "[REQ] {} {} | origin={} ct={} auth={} ua={}",
        method, uri, origin, content_type, has_auth, user_agent
    );

    let resp = next.run(req).await;

    warn!(
        "[RES] {} {} -> {}",
        method,
        uri,
        resp.status()
    );

    resp
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Signal received, shutting down (force exit in 3s)...");

    // Use OS thread - tokio runtime is shutting down so spawned tasks won't run
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        eprintln!("Graceful shutdown timed out, forcing exit.");
        std::process::exit(1);
    });
}
