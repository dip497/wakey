# Meta-Harness Integration for Wakey Skill Evolution

> Research analysis and integration spec for adapting Meta-Harness patterns to Wakey's skill evolution system.

**Paper:** Meta-Harness: End-to-End Optimization of Model Harnesses  
**Authors:** Lee, Nair, Zhang, Lee, Khattab, Finn (Stanford, MIT, KRAFTON)  
**Source:** https://arxiv.org/html/2603.28052v1  
**Analyzed:** April 2026

---

## Executive Summary

Meta-Harness demonstrates that **selective access to prior execution traces** enables causal reasoning about failures, dramatically outperforming text optimizers that compress feedback. The key insight: an evolution system needs the raw diagnostic signal, not summaries.

**What we adapt:**
1. **Execution trace filesystem** — per-skill traces directory with structured logs
2. **Proposer diagnosis pattern** — causal reasoning over confounded edits
3. **Pareto frontier tracking** — quality metrics + context cost tradeoffs
4. **Filesystem-based query interface** — let evolution LLM read what it needs

**What we don't need:**
- Full coding agent in the loop (Meta-Harness uses Claude Code as proposer; we can use simpler LLM calls)
- 10M tokens per iteration (skill evolution is smaller scope than full harness search)
- Complex program search infrastructure (skills are already structured)

---

## 1. Execution Trace Storage

### 1.1 What Meta-Harness Stores

Each harness candidate gets a directory containing:

```
candidates/
  candidate_001/
    harness.py           # Source code (the skill)
    scores.json          # Evaluation metrics
    traces/
      task_001.jsonl     # Execution trace per task
      task_002.jsonl
      ...
```

**Per-task trace contents:**
- Model prompts (input to LLM)
- Model outputs (raw responses)
- Tool calls (function name, args, result)
- State updates (memory changes, context modifications)
- Final score (success/failure, accuracy, etc.)

### 1.2 File Access Statistics

From Appendix A.1, TerminalBench-2 search run:

| Statistic | Value |
|-----------|-------|
| Files read per iteration (median) | 82 |
| Files read per iteration (range) | 69–99 |
| **File type breakdown** | |
| Harness source code | 41% |
| Execution traces | 40% |
| Score/summary files | 6% |
| Other | 13% |

**Key insight:** The proposer reads **both code and traces** in roughly equal measure. It's not just looking at what changed, but how the changes behaved.

### 1.3 Trace Format (JSON Lines)

Each task trace is a JSONL file — one JSON object per step:

```json
{"step": 1, "type": "prompt", "content": "Classify this text...", "tokens": 127}
{"step": 2, "type": "output", "content": "The label is positive because...", "tokens": 89}
{"step": 3, "type": "tool_call", "name": "store_example", "args": {"label": "positive"}, "result": "stored"}
{"step": 4, "type": "state_update", "key": "memory_count", "old": 5, "new": 6}
{"step": 5, "type": "final", "score": 1.0, "label": "correct"}
```

This format enables:
- `grep` for specific patterns
- `jq` for structured queries
- Line-by-line inspection without loading entire trace

### 1.4 Wakey Adaptation

Add a `traces/` directory to each skill:

```
skills/
  summarize-code/
    SKILL.md
    references/
    templates/
    traces/                    # NEW
      execution_20260403.jsonl # Per-execution trace
      execution_20260404.jsonl
      failures.jsonl           # Aggregated failures (copy of failed executions)
      summary.json             # Aggregate metrics
```

**Trace schema:**

```rust
/// Single step in execution trace
#[derive(Serialize, Deserialize)]
pub struct TraceStep {
    /// Step number (1-indexed)
    pub step: u32,
    
    /// Step type
    pub step_type: TraceStepType,
    
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Token count for this step
    pub tokens: Option<u32>,
    
    /// Step-specific data
    pub data: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub enum TraceStepType {
    /// Input prompt to LLM
    Prompt,
    
    /// LLM output
    Output,
    
    /// Tool/function call
    ToolCall,
    
    /// State update
    StateUpdate,
    
    /// Decision point
    Decision,
    
    /// Error
    Error,
    
    /// Final result
    Final,
}
```

**Storage policy:**
- Keep last N executions (configurable, default 50)
- Always keep failures (for diagnosis)
- Aggregate summary.json tracks:
  - Total executions
  - Success rate
  - Common failure patterns (detected keywords)
  - Average tokens used

---

## 2. Proposer Diagnosis Pattern

### 2.1 The Causal Reasoning Pattern

From Appendix A.2, the proposer exhibits a clear diagnostic pattern:

1. **Identify confounded edits** — Multiple changes bundled together, can't determine which caused regression
2. **Form hypothesis** — "The prompt rewrite is the likely cause, not the structural fix"
3. **Test hypothesis** — Revert prompt, keep structural fix, observe result
4. **Pivot after failures** — After repeated regressions, shift to safer modifications

### 2.2 Example from TerminalBench-2 Search

**Iteration 1-2:** Both bundle structural fixes with prompt changes. Both regress.

**Iteration 3 (key causal step):**
> "The regressions are not primarily due to the structural bugfixes themselves... The common factor across failures is the cleanup-heavy prompt rewrite."

The proposer:
1. Reads prior candidates' code and traces
2. Identifies the shared intervention (prompt rewrite)
3. Tests the confound-isolating hypothesis (revert prompt, keep bugfix)
4. Observes smaller regression → hypothesis confirmed

**Iteration 7 (winning candidate):**
> "After six consecutive regressions, the proposer shifts strategy from modifying the control loop to adding information before the loop begins."

This is the **pivoting strategy** — when a class of modifications proves fragile, try a different approach entirely.

### 2.3 Wakey Adaptation

The evolution prompt needs access to:

1. **Current skill code** (SKILL.md)
2. **Prior versions** (from lineage)
3. **Execution traces** (from traces/)
4. **Quality metrics** (from registry)

**Evolution prompt structure:**

```markdown
# Skill Evolution Task

## Current Skill
{skill_name} (v{version})

### Quality Metrics
- Selections: {total_selections}
- Applied: {total_applied}
- Completions: {total_completions}
- Fallbacks: {total_fallbacks}
- Effective rate: {effective_rate:.1%}

### Recent Executions
{#for trace in recent_traces}
- {trace.timestamp}: {trace.result} ({trace.tokens} tokens)
{#if trace.failed}
  - Error: {trace.error_summary}
{#endif}
{/for}

### Failure Patterns
{#for pattern in failure_patterns}
- {pattern.description} ({pattern.count} occurrences)
{/for}

## Prior Versions
{#for version in prior_versions}
### {version.skill_id} ({version.created_at})
- Change: {version.change_summary}
- Result: {version.outcome}
{/for}

## Evolution Request
Analyze the above and propose an evolution:
- FIX: If the skill has bugs or outdated instructions
- DERIVED: If you see an opportunity to enhance or specialize
- CAPTURED: If you discovered a new pattern worth saving

Your analysis should:
1. Identify the root cause of failures (not just symptoms)
2. Check for confounded changes in prior versions
3. Consider pivoting if a class of modifications hasn't worked

Output your evolution decision and reasoning.
```

### 2.4 Diagnosis Heuristics (Embedded in Prompt)

From the paper's observed proposer behavior:

```
DIAGNOSIS HEURISTICS:

1. CONFOUND ISOLATION
   - If multiple changes bundled together, identify shared interventions
   - Test whether removing one intervention changes outcome
   - Don't assume correlation = causation

2. PATTERN MATCHING
   - Look for recurring failure keywords in traces
   - Check if failures cluster around specific task types
   - Identify common preconditions for success

3. PIVOT DETECTION
   - After 3+ consecutive regressions with similar approach, pivot
   - "Similar approach" = same category of change (prompt, control flow, retrieval)
   - Pivot = try a different category

4. VERSION LINEAGE ANALYSIS
   - Check if prior "successful" versions actually degraded
   - Look for version chains that progressively worsened
   - Consider reverting to earlier version if current is worse
```

---

## 3. What We Adapt for Wakey

### 3.1 Concept Mapping

| Meta-Harness | Wakey |
|--------------|-------|
| Harness | Skill (SKILL.md + supporting files) |
| Proposer | Evolution LLM call in `wakey-skills::evolution` |
| Execution traces | `traces/` directory per skill |
| Pareto frontier | Quality metrics in `SkillRecord` |
| Iteration loop | Evolution triggers (post-execution, degradation, metric monitor) |
| Filesystem access | Skill directory + traces directory |
| Population | Skill registry (SQLite) |

### 3.2 Architecture Integration

```
┌─────────────────────────────────────────────────────────────┐
│                     WAKEY-SKILLS CRATE                       │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐   │
│  │   Registry   │───▶│   Quality    │───▶│  Evolution   │   │
│  │   (SQLite)   │    │   Metrics    │    │   Engine     │   │
│  └──────────────┘    └──────────────┘    └──────┬───────┘   │
│         │                                         │          │
│         ▼                                         ▼          │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐   │
│  │   Skills     │    │   Traces     │◀───│   Trace      │   │
│  │   Directory  │───▶│   Store      │    │   Collector  │   │
│  └──────────────┘    └──────────────┘    └──────────────┘   │
│         │                   │                               │
│         ▼                   ▼                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                     FILESYSTEM                        │   │
│  │  skills/                                              │   │
│  │    my-skill/                                          │   │
│  │      SKILL.md                                         │   │
│  │      traces/                                          │   │
│  │        exec_001.jsonl                                 │   │
│  │        failures.jsonl                                 │   │
│  │        summary.json                                   │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.3 Data Flow

1. **Execution** → `TraceCollector` records steps → `traces/exec_TIMESTAMP.jsonl`
2. **Completion** → `QualityMetrics` updated → `summary.json` regenerated
3. **Trigger** → `EvolutionEngine` invoked with:
   - Current skill content
   - Trace directory path
   - Quality metrics
   - Prior versions (from lineage)
4. **Evolution** → LLM reads traces, proposes change → New skill version created
5. **Lineage** → Evolution recorded in `skill_lineage` table

---

## 4. Minimal Implementation

### 4.1 SMALLEST Addition (80% of benefit)

**Goal:** Add execution traces with filesystem access for evolution.

**Changes:**

1. **Add `TraceStore` struct** (new file: `trace.rs`)

```rust
// crates/wakey-skills/src/trace.rs

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Execution trace store — one per skill
pub struct TraceStore {
    /// Skill traces directory
    traces_dir: PathBuf,
    
    /// Maximum traces to keep
    max_traces: usize,
}

/// Single trace step
#[derive(Serialize, Deserialize)]
pub struct TraceStep {
    pub step: u32,
    pub step_type: TraceStepType,
    pub timestamp: DateTime<Utc>,
    pub tokens: Option<u32>,
    pub data: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub enum TraceStepType {
    Prompt, Output, ToolCall, StateUpdate, Decision, Error, Final,
}

/// Trace summary (summary.json)
#[derive(Serialize, Deserialize, Default)]
pub struct TraceSummary {
    pub total_executions: u64,
    pub successful: u64,
    pub failed: u64,
    pub avg_tokens: f64,
    pub failure_patterns: Vec<FailurePattern>,
}

#[derive(Serialize, Deserialize)]
pub struct FailurePattern {
    pub pattern: String,
    pub count: u64,
    pub last_seen: DateTime<Utc>,
}

impl TraceStore {
    pub fn new(skill_dir: &std::path::Path, max_traces: usize) -> Self {
        let traces_dir = skill_dir.join("traces");
        fs::create_dir_all(&traces_dir).ok();
        Self { traces_dir, max_traces }
    }
    
    /// Start a new trace
    pub fn start_trace(&self) -> ActiveTrace {
        ActiveTrace {
            steps: Vec::new(),
            started_at: Utc::now(),
            traces_dir: self.traces_dir.clone(),
        }
    }
    
    /// Get recent traces (for evolution prompt)
    pub fn recent_traces(&self, n: usize) -> Vec<PathBuf> {
        let mut traces: Vec<_> = fs::read_dir(&self.traces_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "jsonl").unwrap_or(false))
            .filter(|e| !e.file_name().to_string_lossy().starts_with("failures"))
            .map(|e| e.path())
            .collect();
        
        traces.sort_by(|a, b| b.cmp(a)); // Newest first
        traces.into_iter().take(n).collect()
    }
    
    /// Get failures trace (all failures aggregated)
    pub fn failures_trace(&self) -> Option<PathBuf> {
        let path = self.traces_dir.join("failures.jsonl");
        if path.exists() { Some(path) } else { None }
    }
    
    /// Get summary
    pub fn summary(&self) -> TraceSummary {
        let path = self.traces_dir.join("summary.json");
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    
    /// Prune old traces
    pub fn prune(&self) {
        let mut traces: Vec<_> = fs::read_dir(&self.traces_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "jsonl").unwrap_or(false))
            .filter(|e| !e.file_name().to_string_lossy().starts_with("failures"))
            .collect();
        
        traces.sort_by_key(|e| e.file_name());
        
        // Keep last max_traces
        if traces.len() > self.max_traces {
            for old in traces.iter().take(traces.len() - self.max_traces) {
                fs::remove_file(old.path()).ok();
            }
        }
    }
}

/// Active trace being written
pub struct ActiveTrace {
    steps: Vec<TraceStep>,
    started_at: DateTime<Utc>,
    traces_dir: PathBuf,
}

impl ActiveTrace {
    /// Record a step
    pub fn step(&mut self, step_type: TraceStepType, data: serde_json::Value, tokens: Option<u32>) {
        self.steps.push(TraceStep {
            step: self.steps.len() as u32 + 1,
            step_type,
            timestamp: Utc::now(),
            tokens,
            data,
        });
    }
    
    /// Finish trace and write to file
    pub fn finish(self, success: bool) -> std::io::Result<()> {
        let timestamp = self.started_at.format("%Y%m%d_%H%M%S");
        let filename = format!("exec_{}.jsonl", timestamp);
        let path = self.traces_dir.join(&filename);
        
        // Write trace
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&path)?;
        
        for step in &self.steps {
            writeln!(file, "{}", serde_json::to_string(step)?)?;
        }
        
        // If failed, append to failures.jsonl
        if !success {
            let failures_path = self.traces_dir.join("failures.jsonl");
            let mut failures = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&failures_path)?;
            
            for step in &self.steps {
                writeln!(failures, "{}", serde_json::to_string(step)?)?;
            }
            writeln!(failures, "---")?; // Separator
        }
        
        Ok(())
    }
}
```

2. **Add trace collection to skill execution** (in `dag.rs` or skill runner)

```rust
// In skill execution, wrap with trace collection:

let mut trace = skill.trace_store.start_trace();

trace.step(TraceStepType::Prompt, json!({
    "content": prompt,
}), Some(prompt_tokens));

// ... execute skill ...

trace.step(TraceStepType::Output, json!({
    "content": output,
}), Some(output_tokens));

// On completion:
trace.finish(success)?;
```

3. **Enhance evolution prompt with trace access**

```rust
// In evolution.rs, build prompt with trace context:

fn build_evolution_prompt(&self, skill_id: &str) -> WakeyResult<String> {
    let skill = self.registry.get_skill(skill_id)?;
    let traces = skill.trace_store.recent_traces(10);
    let failures = skill.trace_store.failures_trace();
    let summary = skill.trace_store.summary();
    let versions = self.get_versions(&skill.name)?;
    
    // Read trace contents
    let recent_trace_content = traces.iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .map(|s| s.lines().take(50).collect::<Vec<_>>().join("\n"))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    
    let failure_content = failures
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|s| s.lines().take(100).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default();
    
    // Build prompt (see template above)
    // ...
}
```

4. **Add CLI commands for trace inspection**

```bash
# List traces for a skill
wakey skill traces my-skill

# Show recent failures
wakey skill failures my-skill

# Show trace summary
wakey skill summary my-skill
```

### 4.2 What NOT to Store

From the paper, Meta-Harness operates with up to **10M tokens per iteration**. For skill evolution, we can be much smaller:

| What Meta-Harness stores | What Wakey stores |
|--------------------------|-------------------|
| Full harness code (100-1000 lines) | SKILL.md (already stored) |
| All prompts to LLM | Summarized prompts (first 500 chars) |
| All tool calls | Tool names + result status |
| Full output | First 500 chars + success/failure |
| State at every step | State diffs only |
| All 60+ candidates | Last 10 traces + all failures |

**Target:** ~50KB per skill for traces (vs 10M tokens = ~40KB for text alone in Meta-Harness)

### 4.3 Evolution Prompt Template

```markdown
# Skill Evolution: {skill_name}

## Current State

**Quality Metrics:**
- Executions: {total_executions}
- Success rate: {success_rate:.1%}
- Avg tokens: {avg_tokens}

**Recent Executions (last 10):**
```
{recent_traces_summary}
```

## Failure Analysis

**Failure count:** {failure_count}

**Recent failures:**
```
{failure_traces}
```

**Detected patterns:**
{#for pattern in failure_patterns}
- `{pattern.keyword}` appears in {pattern.count} failures
{/for}

## Version History

{#for version in prior_versions}
### Version {version.generation} ({version.created_at})
- **Change:** {version.change_summary}
- **Result:** {version.outcome} (success rate was {version.success_rate:.1%})
{/for}

## Evolution Request

Based on the above, propose an evolution:

1. **Diagnose**: What is the root cause of failures? (Not just symptoms)
2. **Check confounds**: Are there bundled changes that could be isolated?
3. **Consider pivot**: If similar changes haven't worked, try a different approach
4. **Propose**: What specific change will improve success rate?

Output format:
```yaml
evolution_type: fix | derived | captured
diagnosis: |
  <root cause analysis>
change_summary: |
  <what to change>
new_content: |
  <new SKILL.md content if fix/derived, or new skill if captured>
```
```

---

## 5. Implementation Checklist

### Phase 1: Trace Infrastructure (Priority: High)

- [ ] Create `trace.rs` with `TraceStore`, `ActiveTrace`, `TraceStep`
- [ ] Add `traces/` directory creation to skill initialization
- [ ] Integrate trace collection into skill execution
- [ ] Implement trace pruning (keep last N)
- [ ] Implement failures aggregation

### Phase 2: Evolution Integration (Priority: High)

- [ ] Enhance `SkillEvolver` with trace access
- [ ] Build evolution prompt template with trace context
- [ ] Add lineage analysis to evolution prompt
- [ ] Implement confound detection heuristics (in prompt)
- [ ] Add pivot detection after N failures

### Phase 3: Quality Metrics (Priority: Medium)

- [ ] Extend `SkillRecord` with trace-derived metrics
- [ ] Implement `failure_patterns` detection
- [ ] Add `summary.json` generation after each execution
- [ ] Track per-skill token usage

### Phase 4: CLI & Debugging (Priority: Low)

- [ ] Add `wakey skill traces` command
- [ ] Add `wakey skill failures` command
- [ ] Add `wakey skill summary` command
- [ ] Add trace export for debugging

---

## 6. Key Takeaways

### From Meta-Harness

1. **Raw traces > summaries** — The proposer needs diagnostic detail, not compressed feedback
2. **Causal reasoning requires history** — You can't diagnose confounds without seeing prior versions
3. **Filesystem access enables selective attention** — Let the LLM read what it needs, not everything
4. **Pivoting is essential** — When an approach fails repeatedly, try something different

### For Wakey

1. **Start small** — 10 traces + failures is enough for most skill evolution
2. **Keep failures forever** — They're the diagnostic gold mine
3. **Token budget matters** — Don't dump 10M tokens; summarize strategically
4. **Evolution is a skill** — The evolution prompt itself can evolve

---

## Appendix: Trace Schema (JSON Schema)

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "step": { "type": "integer", "minimum": 1 },
    "step_type": {
      "type": "string",
      "enum": ["prompt", "output", "tool_call", "state_update", "decision", "error", "final"]
    },
    "timestamp": { "type": "string", "format": "date-time" },
    "tokens": { "type": "integer", "minimum": 0 },
    "data": {
      "type": "object",
      "oneOf": [
        {
          "properties": {
            "content": { "type": "string" }
          },
          "required": ["content"]
        },
        {
          "properties": {
            "name": { "type": "string" },
            "args": { "type": "object" },
            "result": { "type": "string" }
          },
          "required": ["name"]
        },
        {
          "properties": {
            "key": { "type": "string" },
            "old": {},
            "new": {}
          },
          "required": ["key", "new"]
        },
        {
          "properties": {
            "message": { "type": "string" },
            "stack": { "type": "string" }
          },
          "required": ["message"]
        },
        {
          "properties": {
            "score": { "type": "number" },
            "label": { "type": "string" }
          },
          "required": ["score"]
        }
      ]
    }
  },
  "required": ["step", "step_type", "timestamp", "data"]
}
```