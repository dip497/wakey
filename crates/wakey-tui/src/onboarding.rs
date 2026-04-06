//! Wakey Onboarding TUI (Simplified)
//!
//! Welcome screen and basic setup wizard.
//! Full implementation can be added later.

use anyhow::{Context, Result};
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    prelude::Widget,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::io::{self, IsTerminal};

use super::theme;
use super::widgets::Banner;

/// Application state
struct OnboardingApp {
    should_quit: bool,
    step: usize,
}

impl OnboardingApp {
    fn new() -> Self {
        Self {
            should_quit: false,
            step: 0,
        }
    }
}

/// Run the onboarding TUI
pub async fn run_tui_onboarding() -> Result<()> {
    if !io::stdin().is_terminal() {
        anyhow::bail!("Onboarding TUI requires a TTY");
    }

    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = io::stdout();
    stdout
        .execute(EnterAlternateScreen)
        .context("Failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

    let mut app = OnboardingApp::new();

    let result = run_app(&mut terminal, &mut app).await;

    disable_raw_mode().ok();
    terminal.backend_mut().execute(LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut OnboardingApp,
) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame, app))?;

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                        KeyCode::Enter => {
                            if app.step < 2 {
                                app.step += 1;
                            } else {
                                app.should_quit = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn render(frame: &mut Frame, app: &OnboardingApp) {
    let size = frame.area();

    match app.step {
        0 => render_welcome(frame, size),
        1 => render_config_info(frame, size),
        _ => render_complete(frame, size),
    }
}

fn render_welcome(frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .constraints([
            Constraint::Length(10),
            Constraint::Min(0),
            Constraint::Length(5),
        ])
        .split(area);

    // Banner
    Banner.render(chunks[0], frame.buffer_mut());

    // Welcome text
    let welcome = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Welcome to Wakey — Your laptop, alive.",
            theme::heading_style(),
        )),
        Line::from(""),
        Line::from("Wakey is an open-source AI companion that:"),
        Line::from("  • Lives as an always-on-top desktop overlay"),
        Line::from("  • Perceives your screen and context"),
        Line::from("  • Talks proactively and remembers everything"),
        Line::from("  • Grows with you through skills"),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to continue, or 'q' to quit",
            theme::accent_style(),
        )),
    ])
    .wrap(Wrap { trim: false })
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::border_style())
            .title(" Welcome "),
    );

    frame.render_widget(welcome, chunks[1]);
}

fn render_config_info(frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(5),
        ])
        .split(area);

    // Title
    let title = Paragraph::new("Configuration")
        .alignment(Alignment::Center)
        .style(theme::title_style());
    frame.render_widget(title, chunks[0]);

    // Config info
    let info = Paragraph::new(vec![
        Line::from(""),
        Line::from("Wakey uses configuration files in:"),
        Line::from(""),
        Line::from(Span::styled(
            "  config/default.toml",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Edit this file to configure:"),
        Line::from("  • LLM provider and API keys"),
        Line::from("  • Persona and behavior"),
        Line::from("  • Voice settings"),
        Line::from("  • Memory and skills"),
        Line::from(""),
        Line::from(Span::styled(
            "Press Enter to finish, or 'q' to quit",
            theme::accent_style(),
        )),
    ])
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::border_style())
            .title(" Setup "),
    );

    frame.render_widget(info, chunks[1]);
}

fn render_complete(frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .constraints([
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(5),
        ])
        .split(area);

    // Title
    let title = Paragraph::new("✓ Setup Complete")
        .alignment(Alignment::Center)
        .style(theme::success_style());
    frame.render_widget(title, chunks[0]);

    // Complete message
    let msg = Paragraph::new(vec![
        Line::from(""),
        Line::from("You're ready to use Wakey!"),
        Line::from(""),
        Line::from("Run Wakey with:"),
        Line::from(""),
        Line::from(Span::styled(
            "  cargo run --bin wakey",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Or try the chat TUI:"),
        Line::from(""),
        Line::from(Span::styled(
            "  cargo run --bin wakey -- chat",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(theme::border_style())
            .title(" Done "),
    );

    frame.render_widget(msg, chunks[1]);
}
