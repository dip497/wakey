# wakey-cortex — Agent Instructions

This is the **brain**. It decides: speak, act, remember, or stay quiet.

## What belongs here
- LLM provider trait and OpenAI-compatible implementation
- Decision engine (process events → decide response)
- Prompt construction
- Context window management

## LLM Client Rules
- ONLY OpenAI-compatible API (`POST /v1/chat/completions`)
- Use `reqwest` with rustls-tls (no openssl)
- Support streaming via SSE
- Support vision (image_url in messages) for VLM
- One trait: `LlmProvider` with `chat()` and `chat_stream()` methods
- One implementation: `OpenAiCompatible` — works with Ollama, OpenRouter, GLM, vLLM, anything

## Decision Engine
- Subscribes to ALL spine events
- Maintains a sliding context window of recent events
- On each Breath event: evaluate "should I speak?"
- Factors: user state, mood, last interaction time, urgency
- Output: `ShouldSpeak`, `ShouldAct`, `StayQuiet`, or `ShouldRemember` events
