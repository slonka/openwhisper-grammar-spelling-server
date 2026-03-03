use axum::{
    extract::{State, Json},
    response::{IntoResponse, Response},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use tracing::{info, warn};

use crate::pipeline::TextCleanupPipeline;

#[derive(Clone)]
pub struct AppState {
    pub pipeline: Arc<TextCleanupPipeline>,
}

#[derive(Deserialize)]
pub struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
pub struct ChatCompletionRequest {
    messages: Vec<ChatMessage>,
    model: Option<String>,
}

pub async fn chat_completions(
    State(state): State<AppState>,
    Json(payload): Json<ChatCompletionRequest>,
) -> Response {
    // Find last user message
    let last_user_msg = payload.messages.iter().rev().find(|m| m.role == "user");
    
    let user_text = match last_user_msg {
        Some(msg) => &msg.content,
        None => "",
    };

    if user_text.is_empty() {
        info!("Received chat completion request with no user content");
    } else {
        info!("Received chat completion request (length: {})", user_text.len());
    }

    // Check if translation is requested
    // Default model (text-cleanup-pipeline) -> False
    // text-cleanup-translate-pl-en -> True
    let enable_translation = payload.model
        .as_deref()
        .unwrap_or("text-cleanup-pipeline")
        == "text-cleanup-translate-pl-en";

    let cleaned = state.pipeline.run(user_text, enable_translation).await;

    let response = json!({
        "id": format!("chatcmpl-{}", Uuid::new_v4().simple().to_string().chars().take(12).collect::<String>()),
        "object": "chat.completion",
        "created": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        "model": payload.model.as_deref().unwrap_or("text-cleanup-pipeline"),
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": cleaned
                },
                "finish_reason": "stop"
            }
        ],
        "usage": {
            "prompt_tokens": 0,
            "completion_tokens": 0,
            "total_tokens": 0
        }
    });

    Json(response).into_response()
}

pub async fn responses() -> Response {
    info!("Received fallback request to /v1/responses");
    (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))).into_response()
}

pub async fn fallback(req: axum::http::Request<axum::body::Body>) -> Response {
    warn!(
        "[FALLBACK] No route matched: {} {}",
        req.method(),
        req.uri()
    );
    (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))).into_response()
}

pub async fn list_models() -> Json<serde_json::Value> {
    info!("Received list_models request");
    Json(json!({
        "object": "list",
        "data": [
            {
                "id": "text-cleanup-pipeline",
                "object": "model",
                "created": 0,
                "owned_by": "local",
                "permission": [],
                "root": "text-cleanup-pipeline",
                "parent": null
            },
            {
                "id": "text-cleanup-translate-pl-en",
                "object": "model",
                "created": 0,
                "owned_by": "local",
                "permission": [],
                "root": "text-cleanup-translate-pl-en",
                "parent": null
            }
        ]
    }))
}
