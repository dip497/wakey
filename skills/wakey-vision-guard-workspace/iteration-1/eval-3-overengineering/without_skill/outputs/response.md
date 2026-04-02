# Response: Microservice Architecture Proposal for Wakey

I would recommend against adding a microservice architecture with an API gateway, message queue, and database service to Wakey. This would be a significant departure from the project's core design and would work against its fundamental goals. Here is why:

## Wakey is a local desktop companion, not a networked service

Wakey's entire identity is "an AI companion that lives on your desktop." It reads your screen, controls your mouse and keyboard, watches your local files, and runs as a single lightweight binary. Every architectural decision in the project reflects this:

- **Rust single binary** — chosen specifically for ultra-lightweight 24/7 local operation
- **<20MB idle RAM target** — a microservice fleet would blow past this immediately
- **<15MB binary size** — an API gateway alone would likely exceed this
- **Event spine using tokio broadcast channels** — designed for in-process async communication, not network RPC
- **Platform-native input injection** (xdotool/Win32/CoreGraphics) — inherently local

## The proposed architecture contradicts the project's design principles

| Proposed Addition | Conflict |
|---|---|
| API Gateway | Wakey communicates internally via typed events on an in-process bus. An API gateway adds network hops, serialization overhead, and failure modes for zero benefit in a local app. |
| Message Queue (Kafka, RabbitMQ, etc.) | The event spine (`wakey-spine`) already serves this role using lock-free tokio broadcast channels with backpressure. Replacing it with a networked message queue would add latency, memory, and operational complexity. |
| Database Service | Memory is handled by the OpenViking tiered system (L0/L1/L2) backed by the local filesystem. A separate database service adds a process to manage, network round-trips, and breaks the single-binary deployment model. |
| Multi-user networking | Wakey models a single user deeply — it builds a personal user model, tracks individual behavior patterns, and evolves a personality for one person. Multi-user support would fundamentally change what the product is. |

## What "scaling" means for Wakey

Wakey does not need to scale horizontally across users on a network. Each user runs their own Wakey instance locally. The scaling challenges for Wakey are:

1. **Keeping RAM low** while running vision, memory, and personality systems concurrently
2. **Minimizing LLM API costs** through the tiered vision pipeline (a11y before OCR before VLM) and the learning loop (learned skills replace LLM calls over time)
3. **Managing local storage** for the growing memory and skill systems

These are solved by the existing architecture: efficient Rust, tiered processing, aggressive caching, and WASM-sandboxed skills.

## If you need multi-device sync later

If the goal is eventually letting a user's Wakey instance sync across their own devices (not multi-user), the right approach within the existing architecture would be:

- Add a sync trait to `wakey-memory` that can optionally replicate L2 (long-term) memory to a cloud store
- Keep all processing local; only sync the durable state
- This is a small addition to the existing crate, not a new architectural layer

## Summary

Adding microservices, an API gateway, and a message queue to Wakey would be overengineering that directly undermines the project's core value proposition: a lightweight, local, single-binary AI companion. The existing event-driven Cargo workspace architecture is the right fit for what Wakey is. I would recommend focusing engineering effort on the systems that make Wakey unique — the tiered vision pipeline, the learning loop, and the personality system — rather than adding distributed systems infrastructure that the product does not need.
