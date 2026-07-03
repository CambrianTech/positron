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

/// What the live loop should do with one terminal event. Extracting this as a
/// pure decision keeps the loop's gating — the live-only part `drive` never
/// sees — testable without a TTY (see [`classify`] and its unit tests).
#[derive(Debug, PartialEq)]
enum LoopAction {
    /// A quit key fired — leave the loop.
    Quit,
    /// A key press to dispatch through [`step`].
    Dispatch(KeyEvent),
    /// Something changed the surface size — redraw only.
    Redraw,
    /// Not our concern (key release, mouse, paste, focus).
    Ignore,
}

/// Classify one crossterm [`Event`] into a [`LoopAction`]. Pure — no I/O, no
/// host — so the Press/quit/resize gating (the live-only surface) is unit-tested
/// headlessly. Only key **presses** dispatch: Windows also emits `Release`,
/// which would otherwise double every key.
fn classify(event: Event, quit_on: &impl Fn(&KeyEvent) -> bool) -> LoopAction {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if quit_on(&key) {
                LoopAction::Quit
            } else {
                LoopAction::Dispatch(key)
            }
        }
        Event::Resize(_, _) => LoopAction::Redraw,
        _ => LoopAction::Ignore,
    }
}

/// Live TTY driver: enter raw mode + the alternate screen, seed `initial`
/// state, then block on real crossterm key events until `quit_on` fires.
/// Always restores the terminal, even on error — and if restoring itself fails,
/// the loop's original (root-cause) error still wins.
///
/// The wrapper is thin: the dispatch ([`step`]/[`redraw`]) and the event gating
/// ([`classify`]) are both unit-tested headlessly; only the blocking
/// `event::read` I/O here is genuinely TTY-bound.
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
    // mode is worse than the error that got us here. `result.and(restore)`
    // surfaces the loop's error first: a restore failure must never mask the
    // root cause ("fail loud and NAME the cause").
    let restore = (|| -> io::Result<()> {
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()
    })();
    result.and(restore)
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
        match classify(event::read()?, quit_on) {
            LoopAction::Quit => return Ok(()),
            LoopAction::Dispatch(key) => {
                step(host, key, apply);
                redraw(terminal, host)?;
            }
            LoopAction::Redraw => redraw(terminal, host)?,
            LoopAction::Ignore => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Press,
        ))
    }

    fn is_quit(key: &KeyEvent) -> bool {
        key.code == KeyCode::Esc
    }

    // what this catches: the live-only event gating that `drive` never
    // exercises — only key PRESSES dispatch (a Release is ignored, so Windows'
    // duplicate events don't double every key), a quit key leaves the loop, a
    // resize redraws, and non-key events are ignored.
    #[test]
    fn classify_gates_press_quit_resize_and_ignores_the_rest() {
        match classify(press(KeyCode::Up), &is_quit) {
            LoopAction::Dispatch(key) => assert_eq!(key.code, KeyCode::Up),
            other => panic!("expected Dispatch(Up), got {other:?}"),
        }
        assert_eq!(classify(press(KeyCode::Esc), &is_quit), LoopAction::Quit);

        let release = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Up,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert_eq!(classify(release, &is_quit), LoopAction::Ignore);

        assert_eq!(
            classify(Event::Resize(80, 24), &is_quit),
            LoopAction::Redraw
        );
        assert_eq!(classify(Event::FocusGained, &is_quit), LoopAction::Ignore);
    }
}
