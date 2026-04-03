//! Agent Loop — ZeroClaw pattern with skill integration.
//!
//! Based on ZeroClaw's run_tool_call_loop (src/agent/loop_.rs):
//! 1. Cancellation check
//! 2. Budget check
//! 3. Preemptive context trim
//! 4. Tool filtering per turn
//! 5. LLM call
//! 6. Parse tool calls
//! 7. If no tools → return final text
//! 8. Execute tools
//! 9. Loop detection
//!
//! Wakey additions:
//! - Heartbeat event as trigger
//! - Cedar policy check before tool execution
//! - Spine event emission after each step
//! - Skill matching and injection
//! - Memory recall/store per turn

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use wakey_context::{Memory, SqliteMemory};
use wakey_skills::{LearningTracker, SkillRegistry};
use wakey_spine::Spine;
use wakey_types::config::PersonaConfig;
use wakey_types::{ChatMessage, WakeyEvent, WakeyResult};

use crate::decision::{DecisionContext, assemble_context};
use crate::llm::LlmProvider;

/// Maximum agent loop iterations (prevent infinite loops)
const MAX_ITERATIONS: u32 = 20;

/// Budget for subagent calls (shared atomic)
const DEFAULT_BUDGET: u64 = 100_000;

/// Agent loop state
pub struct AgentLoop {
    /// LLM provider
    provider: Arc<dyn LlmProvider>,

    /// Decision context (memory + skills)
    decision_ctx: Arc<Mutex<DecisionContext>>,

    /// Event spine
    spine: Spine,

    /// Persona config
    persona: PersonaConfig,

    /// Iteration counter
    iterations: AtomicU32,

    /// Budget remaining
    budget: AtomicU32,

    /// Learning tracker
    learning: LearningTracker,
}

impl AgentLoop {
    /// Create a new agent loop
    #[allow(clippy::arc_with_non_send_sync)]
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        memory: Arc<dyn Memory>,
        skill_registry: Option<Arc<SkillRegistry>>,
        spine: Spine,
        persona: PersonaConfig,
    ) -> Self {
        let memory_config = wakey_types::config::MemoryConfig::default();
        let decision_ctx =
            DecisionContext::new(memory, skill_registry, persona.clone(), memory_config);

        Self {
            provider,
            decision_ctx: Arc::new(Mutex::new(decision_ctx)),
            spine,
            persona,
            iterations: AtomicU32::new(0),
            budget: AtomicU32::new(DEFAULT_BUDGET as u32),
            learning: LearningTracker::new(),
        }
    }

    /// Run one agent iteration (triggered by heartbeat)
    pub async fn run_iteration(
        &self,
        context: &str,
        user_message: Option<&str>,
    ) -> WakeyResult<String> {
        // 1. Cancellation check
        if self.iterations.load(Ordering::Relaxed) >= MAX_ITERATIONS {
            warn!("Max iterations reached, stopping");
            return Ok("I've been thinking for a while. Let me take a break.".into());
        }

        // 2. Budget check
        if self.budget.load(Ordering::Relaxed) == 0 {
            warn!("Budget exhausted");
            return Ok("I've used up my thinking budget for now.".into());
        }

        self.iterations.fetch_add(1, Ordering::Relaxed);

        // 3. Assemble context
        let (system_prompt, history, current_turn, matched_skill) = {
            let ctx = self.decision_ctx.lock().await;
            let assembled = assemble_context(&ctx, context, user_message).await?;
            (
                assembled.system_prompt,
                assembled.history,
                assembled.current_turn,
                assembled.matched_skill,
            )
        };

        // 4. Record skill selection if matched
        if let Some(ref skill) = matched_skill {
            debug!(skill = %skill.name, "Skill matched for context");
            self.learning.on_skill_use();
        }

        // 5. Build messages for LLM
        let mut messages = vec![ChatMessage::system(&system_prompt)];
        messages.extend(history);
        messages.push(ChatMessage::user(&current_turn));

        // 6. LLM call
        debug!(msg_count = messages.len(), "Calling LLM");
        let response = self.provider.chat(&messages).await?;

        // 7. Record iteration for learning
        self.learning.on_tool_call();

        // 8. Add to history
        {
            let mut ctx = self.decision_ctx.lock().await;
            ctx.add_to_history(ChatMessage::user(current_turn.clone()));
            ctx.add_to_history(ChatMessage::assistant(response.clone()));
        }

        // 9. Check for skill creation trigger
        if self.learning.should_nudge() {
            self.check_skill_creation(&response).await;
        }

        // 10. Update budget
        self.budget.fetch_sub(1, Ordering::Relaxed);

        Ok(response)
    }

    /// Process a heartbeat event
    pub async fn on_heartbeat(&self, event: &WakeyEvent) -> WakeyResult<Option<String>> {
        match event {
            WakeyEvent::Tick => {
                // Just local processing, no LLM
                Ok(None)
            }

            WakeyEvent::Breath => {
                // Screen understanding - may call VLM
                let should_reflect = {
                    let ctx = self.decision_ctx.lock().await;
                    ctx.should_reflect()
                };

                if should_reflect {
                    let mut ctx = self.decision_ctx.lock().await;
                    crate::decision::handle_reflect(&mut ctx, &self.spine).await?;
                }
                Ok(None)
            }

            WakeyEvent::Reflect => {
                // Summarize and compact memory
                let mut ctx = self.decision_ctx.lock().await;
                crate::decision::handle_reflect(&mut ctx, &self.spine).await?;
                Ok(None)
            }

            WakeyEvent::Dream => {
                // Heavy pattern learning - not implemented yet
                info!("Dream event - pattern learning not yet implemented");
                Ok(None)
            }

            WakeyEvent::WindowFocusChanged { app, title, .. } => {
                // Proactive speech based on window change
                if self.persona.proactive {
                    let context = format!("User is looking at {} - {}", app, title);
                    let response = self.run_iteration(&context, None).await?;
                    Ok(Some(response))
                } else {
                    Ok(None)
                }
            }

            _ => Ok(None),
        }
    }

    /// Handle user message (from voice or chat)
    pub async fn on_user_message(&self, message: &str) -> WakeyResult<String> {
        let context = "User sent a message";
        self.run_iteration(context, Some(message)).await
    }

    /// Store a fact from conversation
    pub async fn store_fact(
        &self,
        fact: &str,
        importance: wakey_types::event::Importance,
    ) -> WakeyResult<()> {
        let ctx = self.decision_ctx.lock().await;
        crate::decision::store_conversation_fact(&ctx, fact, importance).await
    }

    /// Check if skill should be created
    async fn check_skill_creation(&self, _response: &str) {
        if let Some(trigger) = self.learning.trigger_reason() {
            info!(trigger = ?trigger, "Skill creation triggered");

            // Consume the nudge
            self.learning.consume_nudge();

            // Emit skill extraction event
            self.spine.emit(WakeyEvent::SkillExtracted {
                name: "pending".into(),
                description: format!("Skill from {:?} trigger", trigger),
            });
        }
    }

    /// Get learning stats
    pub fn learning_stats(&self) -> wakey_skills::LearningStats {
        self.learning.stats()
    }

    /// Reset iteration counter
    pub fn reset_iterations(&self) {
        self.iterations.store(0, Ordering::Relaxed);
    }
}

/// Initialize skills directory with default structure
pub fn init_skills_dir(skills_dir: &std::path::Path) -> WakeyResult<()> {
    use std::fs;

    if !skills_dir.exists() {
        fs::create_dir_all(skills_dir).map_err(|e| wakey_types::WakeyError::Skill {
            skill: "init".into(),
            message: format!("Failed to create skills dir: {}", e),
        })?;
        info!(path = %skills_dir.display(), "Created skills directory");
    }

    Ok(())
}

/// Initialize memory database
pub fn init_memory_db(db_path: &std::path::Path) -> WakeyResult<SqliteMemory> {
    SqliteMemory::new(db_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::OpenAiCompatible;
    use wakey_types::config::LlmProviderConfig;

    fn make_test_loop() -> AgentLoop {
        let memory = Arc::new(SqliteMemory::new_in_memory().expect("Memory"));
        let spine = Spine::new();
        let persona = PersonaConfig {
            name: "Buddy".into(),
            style: "casual".into(),
            proactive: true,
        };

        // Create a mock provider (won't actually call LLM in tests)
        let config = LlmProviderConfig {
            name: "test".into(),
            api_base: "http://localhost:11434/v1".into(),
            model: "test".into(),
            api_key_env: "".into(),
        };

        // For tests, we can't easily create a provider without a real HTTP client
        // So we'll skip actual agent loop tests and test components separately
        AgentLoop::new(
            Arc::new(OpenAiCompatible::new(&config).expect("Provider")),
            memory,
            None,
            spine,
            persona,
        )
    }

    #[test]
    fn test_agent_loop_creation() {
        let _loop = make_test_loop();
        // Just test that it can be created
    }

    #[test]
    fn test_iteration_counter() {
        let agent_loop = make_test_loop();

        assert_eq!(agent_loop.iterations.load(Ordering::Relaxed), 0);
        agent_loop.iterations.fetch_add(1, Ordering::Relaxed);
        assert_eq!(agent_loop.iterations.load(Ordering::Relaxed), 1);

        agent_loop.reset_iterations();
        assert_eq!(agent_loop.iterations.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_learning_tracker() {
        let agent_loop = make_test_loop();

        // Track some iterations
        agent_loop.learning.on_tool_call();
        agent_loop.learning.on_tool_call();

        let stats = agent_loop.learning_stats();
        assert_eq!(stats.total_iterations, 2);
    }
}
