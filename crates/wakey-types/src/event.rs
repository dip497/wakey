use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Every subsystem speaks one language: events.
/// The event spine routes these between crates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WakeyEvent {
    // ── SENSES (perception) ──
    WindowFocusChanged {
        app: String,
        title: String,
        timestamp: DateTime<Utc>,
    },
    ScreenCaptured {
        width: u32,
        height: u32,
        ocr_text: Vec<String>,
        timestamp: DateTime<Utc>,
    },
    ClipboardChanged {
        content: String,
        timestamp: DateTime<Utc>,
    },
    FileChanged {
        path: PathBuf,
        kind: FileChangeKind,
        timestamp: DateTime<Utc>,
    },
    GitStatusChanged {
        repo: PathBuf,
        branch: String,
        timestamp: DateTime<Utc>,
    },
    NotificationReceived {
        app: String,
        title: String,
        body: String,
        timestamp: DateTime<Utc>,
    },
    SystemVitals {
        battery_percent: Option<u8>,
        cpu_usage: f32,
        ram_usage_mb: u64,
        timestamp: DateTime<Utc>,
    },
    UserIdle {
        duration: Duration,
        timestamp: DateTime<Utc>,
    },
    UserReturned {
        idle_duration: Duration,
        timestamp: DateTime<Utc>,
    },

    // ── HEARTBEAT (consciousness rhythm) ──
    Tick,
    Breath,
    Reflect,
    Dream,

    // ── CORTEX (decisions) ──
    ShouldSpeak {
        reason: String,
        urgency: Urgency,
        suggested_text: Option<String>,
    },
    ShouldAct {
        plan: ActionPlan,
    },
    ShouldRemember {
        content: String,
        importance: Importance,
    },
    ShouldForget {
        memory_id: String,
    },
    StayQuiet {
        reason: String,
    },

    // ── ACTION (output) ──
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseClick {
        x: i32,
        y: i32,
        button: MouseButton,
    },
    KeyType {
        text: String,
    },
    KeyCombo {
        keys: Vec<String>,
    },
    Scroll {
        x: i32,
        y: i32,
        delta: i32,
    },
    RunCommand {
        command: String,
        working_dir: Option<PathBuf>,
    },
    OpenUrl {
        url: String,
    },
    Speak {
        text: String,
        emotion: Emotion,
    },

    // ── PERSONA (internal state) ──
    MoodChanged {
        from: Mood,
        to: Mood,
        reason: String,
    },
    UserModelUpdated {
        field: String,
        value: String,
    },

    // ── LEARNING ──
    SkillExtracted {
        name: String,
        description: String,
    },
    SkillRefined {
        name: String,
        version: u32,
    },

    // ── SAFETY ──
    ActionDenied {
        action: String,
        reason: String,
        policy: String,
    },
    ActionApproved {
        action: String,
    },
    UserConfirmationRequired {
        action: String,
        description: String,
    },
    UserConfirmationResponse {
        action: String,
        approved: bool,
    },

    // ── AGENT SUPERVISION ──
    AgentSpawned {
        agent_type: String,
        task: String,
        worktree: Option<String>,
    },
    AgentProgress {
        agent_type: String,
        phase: String,
        detail: String,
    },
    AgentStuck {
        agent_type: String,
        reason: String,
        duration_secs: u64,
    },
    AgentError {
        agent_type: String,
        error: String,
        auto_fixable: bool,
    },
    AgentFixed {
        agent_type: String,
        fix: String,
    },
    AgentCompleted {
        agent_type: String,
        summary: String,
    },
    AgentFailed {
        agent_type: String,
        reason: String,
    },

    // ── SYSTEM ──
    Shutdown,
    Error {
        source: String,
        message: String,
    },

    // ── VOICE (real-time speech) ──
    VoiceListeningStarted,
    VoiceListeningStopped,
    VoiceUserSpeaking {
        text: String,
        is_final: bool,
    },
    VoiceWakeyThinking,
    VoiceWakeySpeaking {
        text: String,
    },
    VoiceSessionEnded,
    VoiceError {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Importance {
    Fleeting,
    ShortTerm,
    LongTerm,
    Core,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mood {
    Neutral,
    Happy,
    Empathetic,
    Focused,
    Playful,
    Concerned,
    Sleepy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Emotion {
    Neutral,
    Excited,
    Gentle,
    Encouraging,
    Teasing,
    Worried,
    Calm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionPlan {
    pub steps: Vec<ActionStep>,
    pub description: String,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStep {
    pub action: String,
    pub description: String,
    pub params: serde_json::Value,
}
