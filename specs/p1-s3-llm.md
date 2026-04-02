# P1-S3: OpenAI-Compatible LLM Client

## Goal
Minimal HTTP client that talks to any OpenAI-compatible API. Streaming support.

## Crate
wakey-cortex (src/llm.rs)

## What to implement

### Trait
```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, messages: &[ChatMessage]) -> WakeyResult<String>;
    fn name(&self) -> &str;
}
```

### ChatMessage
Add to wakey-types if not exists:
```rust
pub struct ChatMessage {
    pub role: String,      // "system", "user", "assistant"
    pub content: String,
}
```

### OpenAiCompatible implementation
1. `pub struct OpenAiCompatible { config: LlmProviderConfig, client: reqwest::Client }`
2. POST to `{api_base}/chat/completions` with JSON body
3. Read API key from env var (config.api_key_env)
4. Parse response: extract choices[0].message.content
5. Use reqwest with rustls-tls (no openssl)
6. Timeout: 30s default
7. Error handling: map HTTP errors to WakeyError::Llm

### NOT in this slice
- No streaming yet (add later)
- No vision/images yet
- No tool calls yet

## Read first
- docs/research/zeroclaw-deep.md #5 (Provider trait)
- docs/architecture/DECISIONS.md #1 (agent loop)
- crates/wakey-cortex/AGENTS.md

## Verify
```bash
cargo check --workspace
# Manual test: set OPENROUTER_API_KEY and run
```

## Acceptance criteria
- OpenAiCompatible::chat() sends request and returns response text
- Works with Ollama (localhost:11434/v1)
- Works with OpenRouter (api key from env)
- Proper error handling (no unwrap)
- cargo check passes
