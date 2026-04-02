# Vision Guard Response: Microservice Architecture Proposal

## Course Correction: Over-Engineering Detected

I appreciate the intent here -- you want Wakey to be able to handle multiple users on a network, and you're thinking ahead about scalability. That's thoughtful. But this proposal trips several red flags, and I need to pull us back to what Wakey actually is.

**Wakey is not a server. It is your laptop, alive.**

A microservice architecture with an API gateway, message queue, and database service is the architecture for a cloud SaaS product. Wakey is a single-binary desktop companion that runs on *your* machine, sees *your* screen, and talks to *you*. It has a heartbeat, not an HTTP endpoint.

Let me run this through the gates to make it concrete.

## Validation: Microservice Architecture (API Gateway + Message Queue + Database Service)

| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | **FAIL** | Microservices, API gateways, and message queues are infrastructure for distributed systems. None of this makes Wakey feel more alive. It makes Wakey feel like a cloud platform. A living companion does not need an API gateway -- it needs a heartbeat. |
| Weight | **FAIL** | Running an API gateway, a message queue (RabbitMQ, Kafka, NATS, etc.), and a separate database service would blow past the <20MB idle RAM target by an order of magnitude. We'd be looking at hundreds of megabytes just for the infrastructure, before Wakey even does anything. |
| Skill | N/A | This isn't a feature that can be a skill or core -- it's a complete architectural rewrite that contradicts Wakey's fundamental design as a single-binary Rust application. |
| Friend | **FAIL** | No friend needs a message queue. This is enterprise infrastructure thinking applied to a personal companion. Your friend doesn't need to "scale to handle multiple users" -- your friend is *your* friend. |
| Offline | **FAIL** | Message queues and API gateways imply network services. This introduces hard network dependencies into something that must work fully offline as a core principle. |

**Verdict**: REJECTED

## Why This Doesn't Fit

1. **Wakey is single-user by design.** It runs on YOUR laptop. It sees YOUR screen. It knows YOU. "Multiple users on a network" is a completely different product -- that's a multi-tenant AI service, not a personal companion.

2. **Wakey is a single binary.** The architecture (defined in CLAUDE.md) is a Cargo workspace that compiles to one binary, <15MB, <20MB idle RAM. A microservice architecture is the opposite of this -- it's multiple processes, multiple services, orchestration overhead, and deployment complexity.

3. **Wakey already has its "message queue."** It's called the Event Spine (`wakey-spine`), built on tokio broadcast channels. It's lock-free, async, in-process, and zero-overhead. Replacing it with an external message queue would add latency, memory, and a network dependency for no benefit.

4. **Wakey already has its "database service."** It's the OpenViking tiered memory system (`wakey-memory`), designed for a single user's context with L0/L1/L2 tiers. An external database service adds operational complexity that contradicts "lightweight, not bloated."

## What You Might Actually Want

If the underlying need is "I want Wakey running on multiple machines and I want them to be aware of each other," here's how a companion would approach that:

### Feature: Shared Memories (Companion-Friendly Alternative)
**Dimension**: Connection / Memory
**One-liner**: Your Wakey instances on different machines can optionally sync memories so they all know you.
**Alive score**: 3 -- It makes Wakey feel like it "follows" you across devices, which is a companion behavior.
**Weight cost**: low -- Periodic sync, not always-on infrastructure.
**Core or Skill?**: Skill -- optional, loads only when configured.
**Crate**: Could live in `wakey-skills` as a WASM plugin or as a trait implementation in `wakey-memory`.
**Priority**: P3 -- Nice to have, not essential for the core companion experience.
**The friend test**: A good friend remembers what you told them even when you see them in a different context. Yes, this passes.

The implementation would be simple: an optional peer-to-peer sync of the L2 memory tier between Wakey instances, using something lightweight like a shared file, a simple TCP connection, or even a cloud storage file. No API gateway. No message queue. No microservices. Just two companions sharing notes about the same human.

## The Bottom Line

The impulse to think about multi-user scenarios is natural, but Wakey's power comes from being the opposite of enterprise architecture. It's small, personal, and local. Every layer of distributed systems infrastructure we add makes it less alive and more like a platform. The Event Spine is our message bus. The memory system is our database. The single binary is our deployment. These are features, not limitations.

If we ever need multi-device awareness, we add it as a thin sync layer on top of what exists -- not by rebuilding the foundation as a distributed system.
