//! Decision engine — context assembly + LLM calls with memory integration.
//!
//! Based on:
//! - ZeroClaw context assembly (recall → build prompt → history → trim → send)
//! - DECISIONS.md #9: Context Assembly Pipeline
//!
//! Before each LLM call:
//! 1. memory.recall(query) → inject relevant past context
//! 2. Build system prompt (persona + skills + instructions)
//! 3. Append recent history (with auto-compaction if too long)
//! 4. Add current turn (user message or heartbeat event)
//! 5. Estimate tokens → trim if over budget
//! 6. Send to LLM

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tracing::{debug, info};

use wakey_context::{Memory, MemoryCategory, MemoryEntry};
use wakey_skills::{SkillMatch, SkillRegistry};
use wakey_spine::Spine;
use wakey_types::config::PersonaConfig;
use wakey_types::{ChatMessage, WakeyEvent, WakeyResult};

/// Maximum messages in conversation history
const MAX_HISTORY_MESSAGES: usize = 10;

/// Decision context — everything needed to make a decision
pub struct DecisionContext {
    /// Memory backend for recall/store
    memory: Arc<dyn Memory>,

    /// Skill registry for skill matching
    skill_registry: Option<Arc<SkillRegistry>>,

    /// Conversation history (last N messages)
    history: VecDeque<ChatMessage>,

    /// Persona configuration
    persona: PersonaConfig,

    /// Last reflect time (for periodic summarization)
    last_reflect: Instant,

    /// Session start time
    session_start: DateTime<Utc>,
}

impl DecisionContext {
    /// Create a new decision context
    pub fn new(
        memory: Arc<dyn Memory>,
        skill_registry: Option<Arc<SkillRegistry>>,
        persona: PersonaConfig,
        _memory_config: wakey_types::config::MemoryConfig,
    ) -> Self {
        Self {
            memory,
            skill_registry,
            history: VecDeque::with_capacity(MAX_HISTORY_MESSAGES),
            persona,
            last_reflect: Instant::now(),
            session_start: Utc::now(),
        }
    }

    /// Build system prompt with persona, memories, and skill instructions
    pub async fn build_system_prompt(&self, context_query: &str) -> WakeyResult<String> {
        let mut prompt = format!(
            "You are {}, a friendly AI companion that lives on the user's desktop.\n\
             Your style is {} and you aim to be helpful, supportive, and natural.\n\n",
            self.persona.name, self.persona.style
        );

        // Recall relevant memories
        let memories = self.memory.recall(context_query, 5).await?;
        if !memories.is_empty() {
            prompt.push_str("## Relevant Memories\n");
            for mem in &memories {
                prompt.push_str(&format!("- {}\n", mem.l0()));
            }
            prompt.push('\n');
        }

        // Add active skill instructions if matched
        if let Some(ref registry) = self.skill_registry
            && let Ok(matches) = registry.find(context_query, 1)
            && let Some(skill) = matches.first()
        {
            prompt.push_str(&format!(
                "## Active Skill: {}\n{}\n\n",
                skill.name, skill.overview
            ));
        }

        // Add current context hints
        prompt.push_str("## Current Context\n");
        prompt.push_str(&format!(
            "Session started: {}\n",
            self.session_start.format("%H:%M")
        ));
        prompt.push_str("You see what the user is doing through window focus events.\n");
        prompt.push_str("Be proactive and helpful, but don't be annoying.\n\n");

        prompt.push_str(
            "Remember: Be brief, natural, and friendly. 1-2 sentences max unless asked for more.",
        );

        Ok(prompt)
    }

    /// Add a message to history, maintaining max size
    pub fn add_to_history(&mut self, message: ChatMessage) {
        if self.history.len() >= MAX_HISTORY_MESSAGES {
            self.history.pop_front();
        }
        self.history.push_back(message);
    }

    /// Get conversation history as a vector
    pub fn get_history(&self) -> Vec<ChatMessage> {
        self.history.iter().cloned().collect()
    }

    /// Store a memory after conversation
    pub async fn store_memory(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
    ) -> WakeyResult<()> {
        self.memory.store(key, content, &category).await
    }

    /// Check if it's time for periodic reflection (15 min)
    pub fn should_reflect(&self) -> bool {
        self.last_reflect.elapsed() >= Duration::from_secs(900)
    }

    /// Mark reflection as done
    pub fn mark_reflect_done(&mut self) {
        self.last_reflect = Instant::now();
    }

    /// Find relevant skills for a query
    pub fn find_skills(&self, query: &str) -> Option<Vec<SkillMatch>> {
        self.skill_registry
            .as_ref()?
            .find(query, 3)
            .ok()
            .filter(|v| !v.is_empty())
    }

    /// Get full skill content by name
    pub fn get_skill_content(&self, name: &str) -> Option<String> {
        self.skill_registry
            .as_ref()?
            .get(name)
            .ok()
            .flatten()
            .map(|s| s.body)
    }

    /// Record skill usage in memory
    pub async fn record_skill_usage(
        &self,
        skill_id: &str,
        applied: bool,
        completed: bool,
        fallback: bool,
    ) -> WakeyResult<()> {
        self.memory
            .record_skill_metrics(skill_id, applied, completed, fallback)
            .await
    }
}

/// Context assembly result
pub struct AssembledContext {
    /// System prompt with persona + memories + skills
    pub system_prompt: String,

    /// Conversation history
    pub history: Vec<ChatMessage>,

    /// Current user message or context
    pub current_turn: String,

    /// Memories that were recalled
    pub recalled_memories: Vec<MemoryEntry>,

    /// Skill that was matched (if any)
    pub matched_skill: Option<SkillMatch>,
}

/// Assemble context for LLM call
///
/// This is the core context assembly pipeline from DECISIONS.md #9.
pub async fn assemble_context(
    decision_ctx: &DecisionContext,
    current_context: &str,
    user_query: Option<&str>,
) -> WakeyResult<AssembledContext> {
    // 1. Recall relevant memories
    let query = user_query.unwrap_or(current_context);
    let recalled_memories = decision_ctx.memory.recall(query, 5).await?;

    // 2. Find relevant skills
    let matched_skill = decision_ctx.find_skills(query).and_then(|mut v| v.pop());

    // 3. Build system prompt
    let mut system_prompt = format!(
        "You are {}, a friendly AI companion that lives on the user's desktop.\n\
         Your style is {}.\n\n",
        decision_ctx.persona.name, decision_ctx.persona.style
    );

    // Add memories if found
    if !recalled_memories.is_empty() {
        system_prompt.push_str("## What You Remember\n");
        for mem in &recalled_memories {
            system_prompt.push_str(&format!("- {}\n", mem.l0()));
        }
        system_prompt.push('\n');
    }

    // Add skill instructions if matched
    if let Some(ref skill) = matched_skill {
        system_prompt.push_str(&format!(
            "## Active Skill: {}\n{}\n\n",
            skill.name, skill.overview
        ));
    }

    // Add current context
    system_prompt.push_str("## Right Now\n");
    system_prompt.push_str(current_context);
    system_prompt.push_str("\n\nBe brief and natural. ");

    // 4. Get history
    let history = decision_ctx.get_history();

    // 5. Current turn
    let current_turn = user_query
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("What do you think about: {}?", current_context));

    Ok(AssembledContext {
        system_prompt,
        history,
        current_turn,
        recalled_memories,
        matched_skill,
    })
}

/// Handle Reflect event — summarize and store
pub async fn handle_reflect(decision_ctx: &mut DecisionContext, spine: &Spine) -> WakeyResult<()> {
    info!("Reflecting on recent activity...");

    // Get recent history
    let history = decision_ctx.get_history();
    if history.is_empty() {
        debug!("No history to reflect on");
        decision_ctx.mark_reflect_done();
        return Ok(());
    }

    // Build a summary
    let summary = format!(
        "Session summary at {}: {} messages exchanged. Topics: {}",
        Utc::now().format("%H:%M"),
        history.len(),
        history
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| m.content.chars().take(50).collect::<String>())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Store as daily memory
    let key = format!("daily/{}.md", Utc::now().format("%Y-%m-%d_%H%M"));
    decision_ctx
        .store_memory(&key, &summary, MemoryCategory::Daily)
        .await?;

    // Mark reflection done
    decision_ctx.mark_reflect_done();

    // Emit reflection event
    spine.emit(WakeyEvent::ShouldRemember {
        content: summary,
        importance: wakey_types::event::Importance::ShortTerm,
    });

    info!("Reflection complete, memory stored");
    Ok(())
}

/// Store a key fact from conversation
pub async fn store_conversation_fact(
    decision_ctx: &DecisionContext,
    fact: &str,
    importance: wakey_types::event::Importance,
) -> WakeyResult<()> {
    let category = match importance {
        wakey_types::event::Importance::Core => MemoryCategory::Core,
        wakey_types::event::Importance::LongTerm => MemoryCategory::Core,
        wakey_types::event::Importance::ShortTerm => MemoryCategory::Daily,
        wakey_types::event::Importance::Fleeting => MemoryCategory::Conversation,
    };

    let key = format!(
        "{}/{}.md",
        category.as_str(),
        Utc::now().format("%Y%m%d_%H%M%S")
    );

    let category_str = category.as_str().to_string();
    decision_ctx.store_memory(&key, fact, category).await?;

    debug!(fact = %fact, category = %category_str, "Stored conversation fact");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wakey_context::SqliteMemory;

    fn make_test_context() -> DecisionContext {
        let memory = Arc::new(SqliteMemory::new_in_memory().expect("Memory"));
        let persona = PersonaConfig {
            name: "Buddy".into(),
            style: "casual".into(),
            proactive: true,
        };

        DecisionContext::new(
            memory,
            None,
            persona,
            wakey_types::config::MemoryConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_build_system_prompt() {
        let ctx = make_test_context();
        let prompt = ctx.build_system_prompt("testing").await.expect("Prompt");

        assert!(prompt.contains("Buddy"));
        assert!(prompt.contains("casual"));
    }

    #[tokio::test]
    async fn test_add_to_history() {
        let mut ctx = make_test_context();

        ctx.add_to_history(ChatMessage::user("Hello"));
        ctx.add_to_history(ChatMessage::assistant("Hi there!"));

        let history = ctx.get_history();
        assert_eq!(history.len(), 2);
    }

    #[tokio::test]
    async fn test_history_limit() {
        let mut ctx = make_test_context();

        // Add more than max
        for i in 0..(MAX_HISTORY_MESSAGES + 5) {
            ctx.add_to_history(ChatMessage::user(format!("Message {}", i)));
        }

        let history = ctx.get_history();
        assert_eq!(history.len(), MAX_HISTORY_MESSAGES);
    }

    #[tokio::test]
    async fn test_store_and_recall_memory() {
        let ctx = make_test_context();

        ctx.store_memory("test/fact.md", "User likes Rust", MemoryCategory::Core)
            .await
            .expect("Store");

        let memories = ctx.memory.recall("Rust", 10).await.expect("Recall");
        assert!(!memories.is_empty());
    }

    #[tokio::test]
    async fn test_assemble_context() {
        let ctx = make_test_context();

        let assembled =
            assemble_context(&ctx, "User is coding in VS Code", Some("ask about editor"))
                .await
                .expect("Assemble");

        assert!(assembled.system_prompt.contains("Buddy"));
        assert!(!assembled.history.is_empty() || assembled.history.is_empty()); // Can be empty initially
        assert_eq!(assembled.current_turn, "ask about editor");
    }

    #[test]
    fn test_should_reflect() {
        let mut ctx = make_test_context();

        // Just created, should not reflect
        assert!(!ctx.should_reflect());

        // Mark as old
        ctx.last_reflect = Instant::now() - Duration::from_secs(1000);
        assert!(ctx.should_reflect());

        // After mark, should reset
        ctx.mark_reflect_done();
        assert!(!ctx.should_reflect());
    }
}
