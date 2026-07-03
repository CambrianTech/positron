//! A live terminal counter, driven through positron's `Host` contract.
//!
//! Run: `cargo run -p positron-ratatui --example counter_tui`
//! Keys: `↑` +1 · `↓` -1 · `r` reset · `Esc` / `q` quit
//!
//! The same `Counter` `ViewState` that the crate's tests render headlessly is
//! here rendered live — the "define once, project many" thesis: nothing about
//! the view changes between a `TestBackend` assertion and a real TTY.

use positron_core::{Renderer, ViewState};
use positron_ratatui::{run_crossterm, TerminalHost};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::{Block, Paragraph};

#[derive(Debug, Clone)]
struct Counter {
    value: i64,
    revision: u64,
}

impl ViewState for Counter {
    fn kind(&self) -> &'static str {
        "counter"
    }
    fn revision(&self) -> Option<u64> {
        Some(self.revision)
    }
}

struct CounterRenderer;
impl Renderer<Counter> for CounterRenderer {
    type Output = Paragraph<'static>;
    fn render(&self, state: &Counter) -> Paragraph<'static> {
        Paragraph::new(format!(
            "\n  counter = {}   (rev {})\n\n  ↑ +1    ↓ -1    r reset    Esc quit",
            state.value, state.revision
        ))
        .block(Block::bordered().title(" positron-ratatui "))
    }
}

enum Cmd {
    Increment,
    Decrement,
    Reset,
}

fn main() -> std::io::Result<()> {
    // The mini-substrate: owns the state, applies commands, emits new state.
    let mut value = 0i64;
    let mut revision = 0u64;

    let mut host = TerminalHost::new(CounterRenderer, |key: KeyEvent| match key.code {
        KeyCode::Up => Some(Cmd::Increment),
        KeyCode::Down => Some(Cmd::Decrement),
        KeyCode::Char('r') => Some(Cmd::Reset),
        _ => None,
    });

    run_crossterm(
        &mut host,
        Counter {
            value: 0,
            revision: 0,
        },
        |cmd| {
            match cmd {
                Cmd::Increment => value += 1,
                Cmd::Decrement => value -= 1,
                Cmd::Reset => value = 0,
            }
            revision += 1;
            Some(Counter { value, revision })
        },
        |key| matches!(key.code, KeyCode::Esc | KeyCode::Char('q')),
    )
}
