//! The render/event loop: [`drive`] (headless, testable) and
//! [`run_crossterm`] (live TTY). Both share one dispatch [`step`] and one
//! [`redraw`], so the state-down/event-up decision lives in exactly one place.

use std::io;

use positron_core::{Host, Renderer, ViewState};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::widgets::Widget;
use ratatui::Terminal;

use crate::host::TerminalHost;

/// Draw the host's current state, if any, filling the terminal.
fn redraw<B, S, R, C>(terminal: &mut Terminal<B>, host: &TerminalHost<S, R, C>) -> io::Result<()>
where
    B: Backend,
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    if let Some(state) = host.state() {
        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(host.renderer().render(state), area);
        })?;
    }
    Ok(())
}

/// The one dispatch decision: map a key to a command, hand it to the
/// substrate (`apply`), and store any state the substrate hands back.
fn step<S, R, C>(
    host: &mut TerminalHost<S, R, C>,
    key: KeyEvent,
    apply: &mut impl FnMut(C) -> Option<S>,
) where
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    if let Some(command) = host.on_event(key) {
        if let Some(next) = apply(command) {
            host.on_state(next);
        }
    }
}

/// Headless render/event loop: draw the current state, then thread each key
/// through the host and re-render. `apply` stands in for the substrate — it
/// consumes a command and optionally produces the next state (in a real
/// deployment this is the `Commands.execute` round-trip; in a self-contained
/// app it owns the state locally).
///
/// Generic over [`Backend`], so a `TestBackend` drives it with no TTY — this
/// is the loop the crate's tests exercise directly.
pub fn drive<B, S, R, C>(
    terminal: &mut Terminal<B>,
    host: &mut TerminalHost<S, R, C>,
    events: impl IntoIterator<Item = KeyEvent>,
    mut apply: impl FnMut(C) -> Option<S>,
) -> io::Result<()>
where
    B: Backend,
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    redraw(terminal, host)?;
    for key in events {
        step(host, key, &mut apply);
        redraw(terminal, host)?;
    }
    Ok(())
}

/// Live TTY driver: enter raw mode + the alternate screen, seed `initial`
/// state, then block on real crossterm key events until `quit_on` fires.
/// Always restores the terminal, even on error.
///
/// This is the thin, un-unit-tested wrapper around the same [`step`]/[`redraw`]
/// that [`drive`] exercises headlessly — the only genuinely TTY-bound surface.
pub fn run_crossterm<S, R, C>(
    host: &mut TerminalHost<S, R, C>,
    initial: S,
    mut apply: impl FnMut(C) -> Option<S>,
    quit_on: impl Fn(&KeyEvent) -> bool,
) -> io::Result<()>
where
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    host.on_state(initial);
    let result = run_loop(&mut terminal, host, &mut apply, &quit_on);

    // Restore unconditionally — a live loop that leaves the terminal in raw
    // mode is worse than the error that got us here.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop<B, S, R, C>(
    terminal: &mut Terminal<B>,
    host: &mut TerminalHost<S, R, C>,
    apply: &mut impl FnMut(C) -> Option<S>,
    quit_on: &impl Fn(&KeyEvent) -> bool,
) -> io::Result<()>
where
    B: Backend,
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    redraw(terminal, host)?;
    loop {
        match event::read()? {
            // Only Press — Windows also emits Release, which would double every key.
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if quit_on(&key) {
                    return Ok(());
                }
                step(host, key, apply);
                redraw(terminal, host)?;
            }
            Event::Resize(_, _) => redraw(terminal, host)?,
            _ => {}
        }
    }
}
