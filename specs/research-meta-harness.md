# Research: Meta-Harness Paper — Integration for Wakey Skill Evolution

## Goal
Read the Meta-Harness paper thoroughly. Extract the exact patterns we need for Wakey's skill evolution system. Write an integration spec.

## Paper
https://arxiv.org/html/2603.28052v1
PDF: https://yoonholee.com/meta-harness/paper.pdf

## Focus Areas

### 1. Execution Trace Storage
- What exactly is stored per execution? (code, logs, prompts, outputs, scores)
- What format? How much data per trace?
- How does the proposer query traces? (grep, cat — filesystem access)
- What's the median files read per iteration? (paper says 82 files, 41% code, 40% traces)

### 2. Proposer Diagnosis Pattern
- How does it identify WHY something failed vs just THAT it failed?
- The "causal diagnosis" pattern — isolating confounded edits
- How does it form and test hypotheses?
- The regression handling — pivoting strategy after repeated failures

### 3. What We Adapt for Wakey
Map each Meta-Harness concept to Wakey's skill evolution:
- Harness → Wakey Skill
- Proposer → Wakey's evolution LLM call
- Execution traces → skill execution logs
- Pareto frontier → quality metrics + version lineage
- Iteration loop → evolution triggers (OpenSpace pattern)

### 4. Minimal Implementation
What's the SMALLEST addition to wakey-skills that gets 80% of the benefit?
- traces/ directory per skill?
- How much trace to store? (full vs summarized)
- How to feed traces to evolution LLM prompt?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/meta-harness-integration.md
End with a concrete code spec for what to add to wakey-skills.
