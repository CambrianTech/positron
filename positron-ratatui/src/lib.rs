#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

//! # positron-ratatui
//!
//! The terminal reference [`Renderer`] and [`Host`](positron_core::Host) event
//! loop for positron — **outlier A** in the contract's outlier-validation: a
//! genuinely different `type Output` from `counter-cli`'s `String`. Here a
//! renderer produces real terminal **cells** (any `ratatui` [`Widget`]), which
//! the loop draws into a `ratatui` [`Buffer`].
//!
//! Three pieces, each single-purpose:
//! - [`render_to_buffer`] — project a [`ViewState`] into a headless [`Buffer`]
//!   (the primitive both loops and all tests build on).
//! - [`TerminalHost`] — the [`Host`](positron_core::Host) impl: state-down via
//!   `on_state`, event-up via a key map.
//! - [`drive`] / [`run_crossterm`] — the render/event loop, headless-testable
//!   and live-TTY respectively.
//!
//! positron owns the *contract*; this crate owns *one surface projection* of
//! it. It knows nothing of any substrate's state or command vocabulary — it
//! renders whatever `ViewState` it's given and emits whatever command the key
//! map returns.

mod driver;
mod host;

pub use driver::{drive, run_crossterm};
pub use host::TerminalHost;

use positron_core::{Renderer, ViewState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

/// Render a [`ViewState`] through a [`Renderer`] into a fresh `ratatui`
/// [`Buffer`] sized to `area`. Headless: no terminal, no TTY. This is the seam
/// the crate's tests assert against and the primitive both loops
/// ([`drive`], [`run_crossterm`]) draw with.
pub fn render_to_buffer<S, R>(renderer: &R, state: &S, area: Rect) -> Buffer
where
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    let mut buffer = Buffer::empty(area);
    renderer.render(state).render(area, &mut buffer);
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use positron_core::Host;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyEvent};
    use ratatui::widgets::Paragraph;
    use ratatui::Terminal;

    // Minimal outlier-A fixture: a counter rendered as terminal cells.
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

    // Output = Paragraph (a real Widget), NOT a String — the point of outlier A.
    struct CounterRenderer;
    impl Renderer<Counter> for CounterRenderer {
        type Output = Paragraph<'static>;
        fn render(&self, state: &Counter) -> Paragraph<'static> {
            Paragraph::new(format!(
                "counter = {} (rev {})",
                state.value, state.revision
            ))
        }
    }

    #[derive(Debug, PartialEq)]
    enum CounterCmd {
        Increment,
        Decrement,
        Reset,
    }

    fn key_map(key: KeyEvent) -> Option<CounterCmd> {
        match key.code {
            KeyCode::Up => Some(CounterCmd::Increment),
            KeyCode::Down => Some(CounterCmd::Decrement),
            KeyCode::Char('r') => Some(CounterCmd::Reset),
            _ => None,
        }
    }

    // what this catches: a Renderer whose Output is real terminal cells (not a
    // String) projects into a Buffer at the expected position — proof the
    // contract carries a non-text surface, the whole reason this is outlier A.
    #[test]
    fn renderer_projects_view_state_into_terminal_cells() {
        let area = Rect::new(0, 0, 20, 1);
        let buffer = render_to_buffer(
            &CounterRenderer,
            &Counter {
                value: 4,
                revision: 2,
            },
            area,
        );
        // "counter = 4 (rev 2)" is 19 cells; the 20th is Paragraph's space pad.
        assert_eq!(buffer, Buffer::with_lines(["counter = 4 (rev 2) "]));
    }

    // what this catches: TerminalHost honors state-down (on_state stores;
    // render_current reflects it) and event-up (on_event maps keys to the
    // consumer command; unmapped keys yield None; no state renders to None).
    #[test]
    fn host_stores_state_and_maps_keys_to_commands() {
        let mut host = TerminalHost::new(CounterRenderer, key_map);
        let area = Rect::new(0, 0, 20, 1);

        assert!(host.state().is_none());
        assert!(host.render_current(area).is_none());

        host.on_state(Counter {
            value: 7,
            revision: 1,
        });
        assert_eq!(host.state().map(|c| c.value), Some(7));
        let buffer = host.render_current(area).expect("state was set");
        assert_eq!(buffer, Buffer::with_lines(["counter = 7 (rev 1) "]));

        assert_eq!(
            host.on_event(KeyEvent::from(KeyCode::Up)),
            Some(CounterCmd::Increment)
        );
        assert_eq!(host.on_event(KeyEvent::from(KeyCode::Char('x'))), None);
    }

    // what this catches: the real render/event loop (drive) threads keys
    // through the host, applies substrate-returned state back, and re-renders —
    // the full state-down/event-up cycle, verified headlessly via TestBackend.
    #[test]
    fn drive_runs_the_full_state_down_event_up_cycle() {
        let mut host = TerminalHost::new(CounterRenderer, key_map);
        let mut terminal = Terminal::new(TestBackend::new(20, 1)).expect("test backend");

        // Mini-substrate: owns the value, applies each command, emits new state.
        let mut value = 0i64;
        let mut rev = 0u64;
        let apply = |cmd: CounterCmd| -> Option<Counter> {
            match cmd {
                CounterCmd::Increment => value += 1,
                CounterCmd::Decrement => value -= 1,
                CounterCmd::Reset => value = 0,
            }
            rev += 1;
            Some(Counter {
                value,
                revision: rev,
            })
        };

        host.on_state(Counter {
            value: 0,
            revision: 0,
        });
        let keys = [
            KeyCode::Up,
            KeyCode::Up,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Char('r'),
            KeyCode::Up,
        ]
        .into_iter()
        .map(KeyEvent::from);

        drive(&mut terminal, &mut host, keys, apply).expect("drive");

        // Up*3=3, Down=2, r=0, Up=1 → value 1; 6 mapped keys → 6 applies → rev 6.
        assert_eq!(host.state().map(|c| c.value), Some(1));
        assert_eq!(
            terminal.backend().buffer(),
            &Buffer::with_lines(["counter = 1 (rev 6) "])
        );
    }
}
