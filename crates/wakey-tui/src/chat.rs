//! Simple interactive chat TUI for Wakey

use anyhow::{Context, Result};
use crossterm::{ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::sync::Arc;
use wakey_context::SqliteMemory;
use wakey_cortex::{AgentLoop, LlmProvider, OpenAiCompatible};
use wakey_spine::Spine;
use wakey_types::config::WakeyConfig;
use wakey_types::ChatMessage;

pub async fn run_chat() -> Result<()> {
    if !io::stdin().is_terminal() {
        anyhow::bail!("Chat requires a TTY");
    }

    // Load config
    let config = WakeyConfig::load(&PathBuf::from("config/default.toml"))
        .context("Failed to load config")?;

    // Initialize memory
    let memory = Arc::new(SqliteMemory::new_in_memory()?);

    // Find provider config
    let provider_config = config
        .llm
        .providers
        .iter()
        .find(|p| p.name == config.llm.default_provider)
        .context(format!("Provider '{}' not found", config.llm.default_provider))?;

    // Create LLM provider
    let provider: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatible::new(provider_config)?);

    // Create agent loop
    let spine = Spine::new();
    let agent = Arc::new(AgentLoop::new(
        provider,
        memory,
        None,
        spine,
        config.persona,
    ));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut input = String::new();
    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut should_quit = false;
    let mut waiting = false;

    // Welcome message
    messages.push(ChatMessage::assistant(
        "Hi! I'm Wakey. Type a message and press Enter. Press 'q' to quit.".to_string(),
    ));

    while !should_quit {
        // Draw
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(3),
                ])
                .split(frame.area());

            // Messages
            let history: Vec<Line> = messages
                .iter()
                .flat_map(|msg| {
                    let role_color = match msg.role.as_str() {
                        "user" => Color::Cyan,
                        "assistant" => Color::Green,
                        _ => Color::White,
                    };
                    vec![
                        Line::from(Span::styled(
                            format!("{}:", msg.role),
                            Style::default().fg(role_color),
                        )),
                        Line::from(msg.content.clone()),
                        Line::from(""),
                    ]
                })
                .collect();

            let history_widget = Paragraph::new(history)
                .block(Block::default().title(" Chat ").borders(Borders::ALL));
            frame.render_widget(history_widget, chunks[0]);

            // Input
            let input_widget = Paragraph::new(input.as_str())
                .block(Block::default().title(" You: ").borders(Borders::ALL));
            frame.render_widget(input_widget, chunks[1]);

            if !waiting {
                frame.set_cursor_position((
                    chunks[1].x + input.len() as u16 + 1,
                    chunks[1].y + 1,
                ));
            }
        })?;

        // Handle input
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => should_quit = true,
                        KeyCode::Enter if !input.trim().is_empty() && !waiting => {
                            let user_msg = input.clone();
                            input.clear();
                            waiting = true;

                            // Add user message
                            messages.push(ChatMessage::user(user_msg.clone()));

                            // Get response
                            match agent.on_user_message(&user_msg).await {
                                Ok(response) => {
                                    messages.push(ChatMessage::assistant(response));
                                }
                                Err(e) => {
                                    messages.push(ChatMessage::assistant(format!("Error: {}", e)));
                                }
                            }

                            waiting = false;
                        }
                        KeyCode::Backspace => {
                            input.pop();
                        }
                        KeyCode::Char(c) => {
                            input.push(c);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Cleanup
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;

    Ok(())
}
