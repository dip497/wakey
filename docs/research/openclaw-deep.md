# OpenClaw Internals: Deep Research

> **Source**: https://github.com/openclaw/openclaw (cloned to /tmp/openclaw-research/)
> **Language**: TypeScript/Node.js
> **Key Packages**: `@mariozechner/pi-agent-core`, `@mariozechner/pi-coding-agent`, `@mariozechner/pi-ai`

---

## 1. Gateway Architecture

### Message Routing

The gateway sits at the center of OpenClaw, routing messages between channels (Discord, Slack, WhatsApp, etc.) and the embedded agent.

**File**: `src/gateway/server.impl.ts`

```typescript
// Core imports show the architecture
import { createChannelManager } from "./server-channels.js";
import { createAgentEventHandler, createSessionEventSubscriberRegistry,
         createSessionMessageSubscriberRegistry } from "./server-chat.js";
import { buildGatewayCronService } from "./server-cron.js";
import { createNodeSubscriptionManager } from "./server-node-subscriptions.js";
```

### Session Creation and Management

Sessions are identified by keys following the pattern:
- `agent:{agentId}:{channel}:{kind}:{id}` for agent-scoped sessions
- `global` for global sessions

**File**: `src/sessions/session-key-utils.ts`

```typescript
export function parseAgentSessionKey(
  sessionKey: string | undefined | null,
): ParsedAgentSessionKey | null {
  const raw = (sessionKey ?? "").trim().toLowerCase();
  if (!raw) return null;
  
  const parts = raw.split(":").filter(Boolean);
  if (parts.length < 3) return null;
  if (parts[0] !== "agent") return null;
  
  const agentId = parts[1]?.trim();
  const rest = parts.slice(2).join(":");
  if (!agentId || !rest) return null;
  
  return { agentId, rest };
}

// Session key types
export type SessionKeyChatType = "direct" | "group" | "channel" | "unknown";

export function deriveSessionChatType(sessionKey: string | undefined | null): SessionKeyChatType {
  const raw = (sessionKey ?? "").trim().toLowerCase();
  if (!raw) return "unknown";
  
  const scoped = parseAgentSessionKey(raw)?.rest ?? raw;
  const tokens = new Set(scoped.split(":").filter(Boolean));
  
  if (tokens.has("group")) return "group";
  if (tokens.has("channel")) return "channel";
  if (tokens.has("direct") || tokens.has("dm")) return "direct";
  
  return "unknown";
}
```

### Gateway Entry Point

**File**: `src/gateway/server.impl.ts` (lines 1-100)

```typescript
export async function createGatewayServer(options: GatewayServerOptions) {
  const cfg = options.config ?? loadConfig();
  
  // Initialize auth
  const authBootstrap = await ensureGatewayStartupAuth({
    cfg,
    agentDir: options.agentDir,
    ...
  });
  
  // Create channel manager for message routing
  const channelManager = createChannelManager({
    cfg,
    runtime: options.runtime,
    ...
  });
  
  // Session event subscribers
  const sessionEventSubscribers = createSessionEventSubscriberRegistry();
  const sessionMessageSubscribers = createSessionMessageSubscriberRegistry();
  
  // Agent event handler bridges channel events to agent runs
  const agentEventHandler = createAgentEventHandler({
    cfg,
    runEmbeddedPiAgent,
    ...
  });
  
  // Start heartbeat runner for periodic proactive behavior
  const heartbeatRunner = startHeartbeatRunner({
    cfg,
    runtime: options.runtime,
  });
  
  return {
    stop: () => { /* cleanup */ },
    updateConfig: (newCfg) => heartbeatRunner.updateConfig(newCfg),
  };
}
```

---

## 2. Agent Memory

### Memory Configuration

**File**: `src/agents/memory-search.ts`

```typescript
export type ResolvedMemorySearchConfig = {
  enabled: boolean;
  sources: Array<"memory" | "sessions">;
  extraPaths: string[];
  multimodal: MemoryMultimodalSettings;
  provider: string;  // "auto" or specific embedding provider
  remote?: {
    baseUrl?: string;
    apiKey?: SecretInput;
    headers?: Record<string, string>;
    batch?: {
      enabled: boolean;
      wait: boolean;
      concurrency: number;
      pollIntervalMs: number;
      timeoutMinutes: number;
    };
  };
  store: {
    driver: "sqlite";
    path: string;
    fts: { tokenizer: "unicode61" | "trigram"; };
    vector: { enabled: boolean; extensionPath?: string; };
  };
  chunking: { tokens: number; overlap: number; };
  sync: {
    onSessionStart: boolean;
    onSearch: boolean;
    watch: boolean;
    watchDebounceMs: number;
    intervalMinutes: number;
    sessions: {
      deltaBytes: number;
      deltaMessages: number;
      postCompactionForce: boolean;
    };
  };
  query: {
    maxResults: number;
    minScore: number;
    hybrid: {
      enabled: boolean;
      vectorWeight: number;
      textWeight: number;
      candidateMultiplier: number;
      mmr: { enabled: boolean; lambda: number; };
      temporalDecay: { enabled: boolean; halfLifeDays: number; };
    };
  };
};
```

### Memory/Session Compaction

**File**: `src/agents/pi-embedded-runner/compact.ts`

```typescript
export async function compactEmbeddedPiSessionDirect(
  params: CompactEmbeddedPiSessionParams,
): Promise<EmbeddedPiCompactResult> {
  const startedAt = Date.now();
  const diagId = params.diagId?.trim() || createCompactionDiagId();
  const trigger = params.trigger ?? "manual";  // "budget" | "overflow" | "manual"
  
  // Resolve model for compaction
  const { model, error, authStorage, modelRegistry } = await resolveModelAsync(
    provider, modelId, agentDir, params.config
  );
  
  // Create session manager
  const sessionManager = guardSessionManager(SessionManager.open(params.sessionFile), {
    agentId: sessionAgentId,
    sessionKey: params.sessionKey,
    inputProvenance: params.inputProvenance,
    allowedToolNames,
  });
  
  // Run compaction hooks
  await runBeforeCompactionHooks(hookRunner, {
    messageCount: messages.length,
    sessionFile: params.sessionFile,
  }, hookCtx);
  
  // Core compaction via pi-agent-core
  const { session } = await createAgentSession({
    cwd: resolvedWorkspace,
    agentDir,
    authStorage,
    modelRegistry,
    model: runtimeModel,
    thinkingLevel: mapThinkingLevel(params.thinkLevel),
    tools: builtInTools,
    customTools,
    sessionManager,
    settingsManager,
    resourceLoader,
  });
  
  // Compact and write result
  const compactResult = await compactWithSafetyTimeout(session, {
    tokenBudget: params.tokenBudget,
    currentTokenCount: params.currentTokenCount,
    force: params.force,
    compactionTarget: params.compactionTarget,  // "budget" or "budget+tool_results"
    timeoutMs: resolveCompactionTimeoutMs(params.config),
  });
  
  await runAfterCompactionHooks(hookRunner, {
    messageCount: messages.length,
    compactedCount: compactResult.compactedCount,
    tokenCount: compactResult.tokensAfter,
    sessionFile: params.sessionFile,
  }, hookCtx);
  
  return {
    ok: true,
    compacted: compactResult.compacted,
    tokensBefore: compactResult.tokensBefore,
    tokensAfter: compactResult.tokensAfter,
    messagesRemoved: compactResult.messagesRemoved,
  };
}
```

### Memory Persistence Format

Memory is stored in SQLite with FTS (full-text search) and optional vector extensions:

```typescript
// Default paths
const storePath = resolveStorePath(agentId, overrides?.store?.path);
// Resolves to: ~/.local/state/openclaw/memory/{agentId}.sqlite

// Chunking defaults
const DEFAULT_CHUNK_TOKENS = 400;
const DEFAULT_CHUNK_OVERLAP = 80;

// Hybrid search weights
const DEFAULT_HYBRID_VECTOR_WEIGHT = 0.7;
const DEFAULT_HYBRID_TEXT_WEIGHT = 0.3;
```

---

## 3. Heartbeat Implementation

### Timer-Based Heartbeat

**File**: `src/infra/heartbeat-runner.ts`

```typescript
export function startHeartbeatRunner(opts: {
  cfg?: OpenClawConfig;
  runtime?: RuntimeEnv;
  abortSignal?: AbortSignal;
}): HeartbeatRunner {
  const state = {
    cfg: opts.cfg ?? loadConfig(),
    agents: new Map<string, HeartbeatAgentState>(),
    timer: null as NodeJS.Timeout | null,
    stopped: false,
  };
  
  const scheduleNext = () => {
    if (state.stopped) return;
    
    const now = Date.now();
    let nextDue = Number.POSITIVE_INFINITY;
    for (const agent of state.agents.values()) {
      if (agent.nextDueMs < nextDue) {
        nextDue = agent.nextDueMs;
      }
    }
    
    const delay = Math.max(0, nextDue - now);
    state.timer = setTimeout(() => {
      requestHeartbeatNow({ reason: "interval", coalesceMs: 0 });
    }, delay);
    state.timer.unref?.();  // Don't block process exit
  };
  
  return { stop: cleanup, updateConfig };
}
```

### HEARTBEAT.md Context

**File**: `src/auto-reply/heartbeat.ts`

```typescript
// Default heartbeat prompt
export const HEARTBEAT_PROMPT =
  "Read HEARTBEAT.md if it exists (workspace context). " +
  "Follow it strictly. Do not infer or repeat old tasks from prior chats. " +
  "If nothing needs attention, reply HEARTBEAT_OK.";

export const DEFAULT_HEARTBEAT_EVERY = "30m";
export const DEFAULT_HEARTBEAT_ACK_MAX_CHARS = 300;

// Check if HEARTBEAT.md has actionable content
export function isHeartbeatContentEffectivelyEmpty(content: string): boolean {
  const lines = content.split("\n");
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    // Skip markdown headers
    if (/^#+(\s|$)/.test(trimmed)) continue;
    // Skip empty list items
    if (/^[-*+]\s*(\[[\sXx]?\]\s*)?$/.test(trimmed)) continue;
    // Found actionable content
    return false;
  }
  return true;
}
```

### Light Context Mode

**File**: `src/infra/heartbeat-runner.ts`

```typescript
// lightContext reduces token cost by limiting bootstrap context
const bootstrapContextMode: "lightweight" | undefined =
  heartbeat?.lightContext === true ? "lightweight" : undefined;

const replyOpts = heartbeatModelOverride
  ? { isHeartbeat: true, heartbeatModelOverride, bootstrapContextMode }
  : { isHeartbeat: true, bootstrapContextMode };
```

### Isolated Session

**File**: `src/infra/heartbeat-runner.ts`

```typescript
// When isolatedSession is enabled, create a fresh session each run
// This avoids sending full conversation history (~100K tokens)
const useIsolatedSession = heartbeat?.isolatedSession === true;

if (useIsolatedSession) {
  const isolatedKey = `${sessionKey}:heartbeat`;
  const cronSession = resolveCronSession({
    cfg,
    sessionKey: isolatedKey,
    agentId,
    nowMs: startedAt,
    forceNew: true,
  });
  cronSession.store[isolatedKey] = cronSession.sessionEntry;
  await saveSessionStore(cronSession.storePath, cronSession.store);
  runSessionKey = isolatedKey;
}
```

---

## 4. Agent Loop

### Main Entry Point

**File**: `src/agents/pi-embedded-runner/run.ts`

```typescript
export async function runEmbeddedPiAgent(
  params: RunEmbeddedPiAgentParams,
): Promise<EmbeddedPiRunResult> {
  const sessionLane = resolveSessionLane(params.sessionKey?.trim() || params.sessionId);
  const globalLane = resolveGlobalLane(params.lane);
  
  // Queue in both session and global lanes for ordering
  return enqueueSession(() =>
    enqueueGlobal(async () => {
      const started = Date.now();
      
      // Resolve workspace
      const workspaceResolution = resolveRunWorkspaceDir({
        workspaceDir: params.workspaceDir,
        sessionKey: params.sessionKey,
        agentId: params.agentId,
        config: params.config,
      });
      
      // Load model and auth
      const { model, error, authStorage, modelRegistry } = await resolveModelAsync(
        provider, modelId, agentDir, params.config
      );
      
      // Initialize context engine (for memory/RAG)
      ensureContextEnginesInitialized();
      const contextEngine = await resolveContextEngine(params.config);
      
      // Main run loop with retry logic
      while (true) {
        if (runLoopIterations >= MAX_RUN_LOOP_ITERATIONS) {
          return { payloads: [...], meta: { error: { kind: "retry_limit" } } };
        }
        runLoopIterations += 1;
        
        // Run single attempt
        const attempt = await runEmbeddedAttempt({
          sessionId: params.sessionId,
          sessionKey: params.sessionKey,
          workspaceDir: resolvedWorkspace,
          agentDir,
          config: params.config,
          contextEngine,
          contextTokenBudget: ctxInfo.tokens,
          skillsSnapshot: params.skillsSnapshot,
          prompt,
          images: params.images,
          provider,
          modelId,
          model: applyAuthHeaderOverride(effectiveModel, apiKeyInfo, params.config),
          tools: effectiveTools,
          ...
        });
        
        // Handle context overflow
        if (contextOverflowError) {
          if (overflowCompactionAttempts < MAX_OVERFLOW_COMPACTION_ATTEMPTS) {
            const compactResult = await contextEngine.compact({
              sessionId: params.sessionId,
              sessionFile: params.sessionFile,
              tokenBudget: ctxInfo.tokens,
              force: true,
              compactionTarget: "budget",
            });
            if (compactResult.compacted) continue;
          }
          return { payloads: [...], meta: { error: { kind: "context_overflow" } } };
        }
        
        // Handle rate limits, timeouts, auth failures
        if (shouldRotate) {
          const rotated = await advanceAuthProfile();
          if (rotated) continue;
          if (fallbackConfigured) {
            throw new FailoverError(message, { reason, provider, model, profileId });
          }
        }
        
        // Success - return payloads
        return {
          payloads: buildEmbeddedRunPayloads(attempt),
          meta: { durationMs: Date.now() - started, agentMeta, ... },
        };
      }
    })
  );
}
```

### Context Assembly

**File**: `src/agents/pi-embedded-runner/run/attempt.ts`

```typescript
export async function runEmbeddedAttempt(
  params: EmbeddedRunAttemptParams,
): Promise<EmbeddedRunAttemptResult> {
  const resolvedWorkspace = resolveUserPath(params.workspaceDir);
  const runAbortController = new AbortController();
  
  // Resolve sandbox context
  const sandbox = await resolveSandboxContext({
    config: params.config,
    sessionKey: sandboxSessionKey,
    workspaceDir: resolvedWorkspace,
  });
  
  // Load skills snapshot
  const { shouldLoadSkillEntries, skillEntries } = resolveEmbeddedRunSkillEntries({
    workspaceDir: effectiveWorkspace,
    config: params.config,
    skillsSnapshot: params.skillsSnapshot,
  });
  
  // Apply skill env overrides
  restoreSkillEnv = applySkillEnvOverridesFromSnapshot({
    snapshot: params.skillsSnapshot,
    config: params.config,
  });
  
  // Build skills prompt
  const skillsPrompt = resolveSkillsPromptForRun({
    skillsSnapshot: params.skillsSnapshot,
    entries: skillEntries,
    config: params.config,
    workspaceDir: effectiveWorkspace,
  });
  
  // Load bootstrap/context files
  const { bootstrapFiles, contextFiles } = await resolveBootstrapContextForRun({
    workspaceDir: effectiveWorkspace,
    config: params.config,
    sessionKey: params.sessionKey,
    sessionId: params.sessionId,
    contextMode: params.bootstrapContextMode,
  });
  
  // Build system prompt
  const appendPrompt = buildEmbeddedSystemPrompt({
    workspaceDir: effectiveWorkspace,
    defaultThinkLevel: params.thinkLevel,
    reasoningLevel: params.reasoningLevel,
    extraSystemPrompt: params.extraSystemPrompt,
    skillsPrompt,
    docsPath,
    ttsHint,
    sandboxInfo,
    tools: effectiveTools,
    runtimeInfo,
    userTimezone,
    userTime,
    contextFiles,
    ...
  });
  
  // Create session with pi-agent-core
  const { session } = await createAgentSession({
    cwd: resolvedWorkspace,
    agentDir,
    authStorage: params.authStorage,
    modelRegistry: params.modelRegistry,
    model: params.model,
    thinkingLevel: mapThinkingLevel(params.thinkLevel),
    tools: builtInTools,
    customTools: allCustomTools,
    sessionManager,
    settingsManager,
    resourceLoader,
  });
  
  // Apply system prompt
  applySystemPromptOverrideToSession(session, systemPromptText);
  
  // Sanitize and limit history
  const prior = await sanitizeSessionHistory({
    messages: session.messages,
    modelApi: params.model.api,
    provider: params.provider,
    allowedToolNames,
    config: params.config,
  });
  const truncated = limitHistoryTurns(prior, historyLimit);
  session.agent.replaceMessages(truncated);
  
  // Context engine integration
  if (params.contextEngine) {
    const assembled = await assembleAttemptContextEngine({
      contextEngine: params.contextEngine,
      sessionId: params.sessionId,
      sessionKey: params.sessionKey,
      messages: session.messages,
      tokenBudget: params.contextTokenBudget,
    });
    if (assembled.messages !== session.messages) {
      session.agent.replaceMessages(assembled.messages);
    }
    if (assembled.systemPromptAddition) {
      systemPromptText = prependSystemPromptAddition({
        systemPrompt: systemPromptText,
        systemPromptAddition: assembled.systemPromptAddition,
      });
    }
  }
  
  // Subscribe to streaming events
  const subscription = subscribeEmbeddedPiSession(session, {
    onAssistantMessage: params.onAssistantMessageStart,
    onBlockReply: params.onBlockReply,
    onReasoningStream: params.onReasoningStream,
    onToolResult: params.onToolResult,
    ...
  });
  
  // Run the prompt
  try {
    await session.prompt(params.prompt, {
      images: params.images,
      signal: runAbortController.signal,
    });
  } finally {
    subscription.unsubscribe();
  }
  
  return {
    aborted,
    timedOut,
    sessionIdUsed: session.sessionId,
    lastAssistant: session.lastAssistant,
    assistantTexts: session.assistantTexts,
    toolMetas: session.toolMetas,
    messagesSnapshot: session.messages,
    ...
  };
}
```

### Tool Execution

Tools are created via `createOpenClawCodingTools` and passed to `createAgentSession`. The agent core handles tool dispatch:

```typescript
const tools = createOpenClawCodingTools({
  agentId: sessionAgentId,
  trigger: params.trigger,
  exec: { ...params.execOverrides, elevated: params.bashElevated },
  sandbox,
  messageProvider: params.messageChannel,
  sessionKey: sandboxSessionKey,
  sessionId: params.sessionId,
  runId: params.runId,
  workspaceDir: effectiveWorkspace,
  config: params.config,
  abortSignal: runAbortController.signal,
  modelProvider: params.model.provider,
  ...
});

// Tools are split into built-in (SDK tools) and custom (OpenClaw tools)
const { builtInTools, customTools } = splitSdkTools({ tools, sandboxEnabled });
```

### Streaming

Streaming is handled by `subscribeEmbeddedPiSession`:

**File**: `src/agents/pi-embedded-subscribe.ts`

```typescript
export function subscribeEmbeddedPiSession(
  session: AgentSession,
  options: SubscribeEmbeddedPiSessionOptions,
): { unsubscribe: () => void } {
  const handlers = createStreamHandlers(options);
  
  session.agent.on("text", (text) => {
    handlers.handleText(text);
    options.onBlockReply?.({ text, isDelta: true });
  });
  
  session.agent.on("tool_use", (toolUse) => {
    handlers.handleToolUse(toolUse);
  });
  
  session.agent.on("tool_result", (result) => {
    handlers.handleToolResult(result);
    options.onToolResult?.(result);
  });
  
  session.agent.on("reasoning", (reasoning) => {
    options.onReasoningStream?.(reasoning);
  });
  
  return { unsubscribe: () => session.agent.removeAllListeners() };
}
```

---

## 5. Skills Snapshot

### Loading Skills

**File**: `src/agents/skills/workspace.ts`

```typescript
function loadSkillEntries(
  workspaceDir: string,
  opts?: { config?: OpenClawConfig; ... },
): SkillEntry[] {
  const limits = resolveSkillsLimits(opts?.config);
  
  // Skills directories in precedence order (lowest to highest):
  // 1. Extra dirs from config
  // 2. Bundled skills (openclaw-bundled)
  // 3. Managed skills (~/.config/openclaw/skills)
  // 4. Personal agents skills (~/.agents/skills)
  // 5. Project agents skills ($workspace/.agents/skills)
  // 6. Workspace skills ($workspace/skills)
  
  const managedSkillsDir = opts?.managedSkillsDir ?? path.join(CONFIG_DIR, "skills");
  const workspaceSkillsDir = path.resolve(workspaceDir, "skills");
  const bundledSkillsDir = opts?.bundledSkillsDir ?? resolveBundledSkillsDir();
  
  const personalAgentsSkillsDir = path.resolve(os.homedir(), ".agents", "skills");
  const projectAgentsSkillsDir = path.resolve(workspaceDir, ".agents", "skills");
  
  // Load from each source
  const bundledSkills = loadSkills({ dir: bundledSkillsDir, source: "openclaw-bundled" });
  const managedSkills = loadSkills({ dir: managedSkillsDir, source: "openclaw-managed" });
  const personalAgentsSkills = loadSkills({ dir: personalAgentsSkillsDir, source: "agents-skills-personal" });
  const projectAgentsSkills = loadSkills({ dir: projectAgentsSkillsDir, source: "agents-skills-project" });
  const workspaceSkills = loadSkills({ dir: workspaceSkillsDir, source: "openclaw-workspace" });
  
  // Merge with precedence (later overwrites earlier)
  const merged = new Map<string, Skill>();
  for (const skill of [...bundledSkills, ...managedSkills, 
                        ...personalAgentsSkills, ...projectAgentsSkills, 
                        ...workspaceSkills]) {
    merged.set(skill.name, skill);
  }
  
  return Array.from(merged.values()).map((skill) => ({
    skill,
    frontmatter: readSkillFrontmatterSafe(skill.filePath),
    metadata: resolveOpenClawMetadata(frontmatter),
    invocation: resolveSkillInvocationPolicy(frontmatter),
  }));
}
```

### Skill Precedence

```typescript
// Precedence: extra < bundled < managed < agents-skills-personal < agents-skills-project < workspace
// Higher precedence skills override lower ones by name
```

### Skill Matching

Skills are filtered by eligibility context:

```typescript
function filterSkillEntries(
  entries: SkillEntry[],
  config?: OpenClawConfig,
  skillFilter?: string[],
  eligibility?: SkillEligibilityContext,
): SkillEntry[] {
  let filtered = entries.filter((entry) => 
    shouldIncludeSkill({ entry, config, eligibility })
  );
  
  // If skillFilter provided, only include named skills
  if (skillFilter !== undefined) {
    const normalized = normalizeSkillFilter(skillFilter) ?? [];
    filtered = filtered.filter((entry) => 
      normalized.includes(entry.skill.name)
    );
  }
  
  return filtered;
}
```

### Building Skills Prompt

```typescript
export function buildWorkspaceSkillsPrompt(
  workspaceDir: string,
  opts?: WorkspaceSkillBuildOptions,
): string {
  const skillEntries = loadSkillEntries(workspaceDir, opts);
  const eligible = filterSkillEntries(skillEntries, opts?.config, opts?.skillFilter);
  const promptSkills = compactSkillPaths(eligible.map((e) => e.skill));
  
  // Apply token/char limits
  const { skillsForPrompt, truncated, compact } = applySkillsPromptLimits({
    skills: promptSkills,
    config: opts?.config,
  });
  
  // Format for prompt
  return compact 
    ? formatSkillsCompact(skillsForPrompt)  // Name + location only
    : formatSkillsForPrompt(skillsForPrompt);  // Full format with descriptions
}
```

### Snapshot Structure

```typescript
export type SkillSnapshot = {
  prompt: string;  // Pre-built prompt for the model
  skills: Array<{
    name: string;
    primaryEnv?: string[];
    requiredEnv?: string[];
  }>;
  skillFilter?: string[];
  resolvedSkills: Skill[];  // Full skill objects for runtime
  version?: number;
};
```

---

## Key Architectural Insights for Wakey

### 1. Event Spine Pattern
OpenClaw uses a broadcast-based event system (`tokio` equivalent in TypeScript). Wakey should use `tokio::broadcast` for the spine.

### 2. Session Key Routing
Session keys encode routing information:
- Agent ID
- Channel type (direct/group/channel)
- Channel/conversation ID
- Thread binding

### 3. Compaction Strategy
- Token budget-based truncation
- Preserve "real conversation" messages (user questions, assistant answers)
- Drop tool results first, then old turns
- Safety timeout to prevent infinite compaction

### 4. Heartbeat Isolation
- `isolatedSession` creates fresh transcripts per heartbeat
- `lightContext` limits bootstrap context
- Deduplication via `lastHeartbeatText` comparison

### 5. Skills as Prompt
Skills are injected into the system prompt, not as tools. This keeps the model aware of all capabilities without tool definition overhead.

### 6. Streaming via Events
The agent core emits events (`text`, `tool_use`, `tool_result`, `reasoning`) that subscribers handle. Wakey should use a similar event-driven streaming model.

---

## File Reference

| Component | Key Files |
|-----------|-----------|
| Gateway | `src/gateway/server.impl.ts`, `src/gateway/server-chat.ts` |
| Agent Loop | `src/agents/pi-embedded-runner/run.ts`, `run/attempt.ts` |
| Compaction | `src/agents/pi-embedded-runner/compact.ts` |
| Memory | `src/agents/memory-search.ts` |
| Heartbeat | `src/infra/heartbeat-runner.ts`, `src/auto-reply/heartbeat.ts` |
| Skills | `src/agents/skills/workspace.ts`, `skills/types.ts` |
| Sessions | `src/sessions/session-key-utils.ts` |
| Channels | `src/channels/plugins/`, `src/channels/session.ts` |