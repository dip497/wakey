//! LLM Provider — OpenAI-compatible HTTP client.
//!
//! Based on ZeroClaw's `OpenAiCompatibleProvider` pattern (src/providers/compatible.rs).
//! Minimal implementation: non-streaming chat completions only.
//! Works with Ollama, OpenRouter, any `/v1/chat/completions` endpoint.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use wakey_types::config::LlmProviderConfig;
use wakey_types::{ChatMessage, WakeyError, WakeyResult};

/// LLM provider trait — runtime polymorphism for different backends.
///
/// Following ZeroClaw's Provider pattern (src/providers/traits.rs).
/// Send + Sync for `Arc<dyn LlmProvider>` usage.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send messages and get response text.
    ///
    /// Simple non-streaming call. Returns the assistant's reply.
    async fn chat(&self, messages: &[ChatMessage]) -> WakeyResult<String>;

    /// Provider name for error context.
    fn name(&self) -> &str;
}

/// OpenAI-compatible HTTP client.
///
/// Works with:
/// - Ollama (localhost:11434/v1)
/// - OpenRouter (openrouter.ai/api/v1)
/// - vLLM, LM Studio, any OpenAI-compatible server
///
/// Config-driven: reads API key from env var specified in config.
/// Uses reqwest with rustls-tls (no openssl).
pub struct OpenAiCompatible {
    name: String,
    base_url: String,
    model: String,
    api_key_env: String,
    timeout: Duration,
    client: Client,
}

impl OpenAiCompatible {
    /// Create provider from config.
    ///
    /// HTTP client is built with rustls-tls and configured timeout.
    pub fn new(config: &LlmProviderConfig) -> WakeyResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| WakeyError::Llm {
                provider: config.name.clone(),
                message: format!("Failed to build HTTP client: {e}"),
            })?;

        Ok(Self {
            name: config.name.clone(),
            base_url: config.api_base.trim_end_matches('/').to_string(),
            model: config.model.clone(),
            api_key_env: config.api_key_env.clone(),
            timeout: Duration::from_secs(30),
            client,
        })
    }

    /// Override default timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        // Rebuild client with new timeout
        self.client = Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| Client::new());
        self
    }

    /// Build the chat completions URL.
    ///
    /// Detects if base_url already includes `/chat/completions` path.
    /// Handles: `localhost:11434/v1`, `api.openrouter.ai/v1`, etc.
    fn chat_url(&self) -> String {
        let has_endpoint = self.base_url.ends_with("/chat/completions");
        if has_endpoint {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }

    /// Get API key from environment.
    ///
    /// Returns None if env var is not set or empty.
    fn get_api_key(&self) -> Option<String> {
        std::env::var(&self.api_key_env)
            .ok()
            .filter(|s| !s.is_empty())
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatible {
    async fn chat(&self, messages: &[ChatMessage]) -> WakeyResult<String> {
        let url = self.chat_url();
        let api_key = self.get_api_key();

        // Build request body
        let request = ApiChatRequest {
            model: self.model.clone(),
            messages: messages
                .iter()
                .map(|m| ApiMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            temperature: 0.7,
            stream: false,
        };

        tracing::debug!(
            provider = self.name,
            model = self.model,
            url = url,
            message_count = messages.len(),
            "Sending chat request"
        );

        // Build HTTP request
        let mut req = self.client.post(&url).json(&request);

        // Add auth header if API key is set (Ollama doesn't need one)
        if let Some(key) = api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }

        // Send request
        let response = req.send().await.map_err(|e| WakeyError::Llm {
            provider: self.name.clone(),
            message: format!("HTTP request failed: {e}"),
        })?;

        // Check status
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            return Err(WakeyError::Llm {
                provider: self.name.clone(),
                message: format!("API error ({}): {}", status, sanitize_error(&error_body)),
            });
        }

        // Parse response
        let body = response.text().await.map_err(|e| WakeyError::Llm {
            provider: self.name.clone(),
            message: format!("Failed to read response body: {e}"),
        })?;

        let api_response: ApiChatResponse = serde_json::from_str(&body).map_err(|e| {
            let snippet = body.chars().take(200).collect::<String>();
            WakeyError::Llm {
                provider: self.name.clone(),
                message: format!("Failed to parse response: {e} (body: {snippet}...)"),
            }
        })?;

        // Extract content from first choice
        let content = api_response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| WakeyError::Llm {
                provider: self.name.clone(),
                message: "No response content returned".to_string(),
            })?;

        tracing::debug!(
            provider = self.name,
            response_len = content.len(),
            "Chat request completed"
        );

        Ok(content)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ── API request/response types ──

/// Request body for `/v1/chat/completions`
#[derive(Debug, Serialize)]
struct ApiChatRequest {
    model: String,
    messages: Vec<ApiMessage>,
    temperature: f64,
    stream: bool,
}

/// Single message in API request
#[derive(Debug, Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

/// Response from `/v1/chat/completions`
#[derive(Debug, Deserialize)]
struct ApiChatResponse {
    choices: Vec<ApiChoice>,
}

/// Single choice in response (usually just one)
#[derive(Debug, Deserialize)]
struct ApiChoice {
    message: ApiResponseMessage,
}

/// Message in response
#[derive(Debug, Deserialize)]
struct ApiResponseMessage {
    #[serde(default)]
    content: Option<String>,
}

// ── Helpers ──

/// Sanitize error messages to avoid leaking secrets.
///
/// Truncates and removes potential credential patterns.
fn sanitize_error(error: &str) -> String {
    let mut sanitized = error.chars().take(500).collect::<String>();
    // Simple pattern removal without regex dependency
    // Remove common patterns like "api_key: sk-xxx" or "token: xxx"
    if let Some(start) = sanitized.find("api_key")
        && let Some(end) = sanitized[start..].find(' ')
    {
        sanitized.replace_range(start..start + end, "[REDACTED]");
    }
    if let Some(start) = sanitized.find("token")
        && let Some(end) = sanitized[start..].find(' ')
    {
        sanitized.replace_range(start..start + end, "[REDACTED]");
    }
    sanitized
}
