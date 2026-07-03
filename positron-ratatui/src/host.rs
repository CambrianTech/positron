//! [`TerminalHost`] — a positron [`Host`] over a ratatui terminal surface.

use positron_core::{Host, Renderer, ViewState};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

/// A key map lowers a terminal key press to an optional consumer command.
/// Boxed (rather than a generic type parameter) so `TerminalHost` stays a
/// single concrete type per `(S, R, C)` — and aliased here so the field type
/// reads plainly instead of tripping `clippy::type_complexity`.
type KeyMap<C> = Box<dyn FnMut(KeyEvent) -> Option<C>>;

/// A [`Host`] that renders a [`ViewState`] to a ratatui terminal and maps
/// terminal key presses back to a consumer-defined command type.
///
/// - **State-down:** [`Host::on_state`] stores the latest state; the driver
///   ([`crate::drive`] / [`crate::run_crossterm`]) re-renders it.
/// - **Event-up:** [`Host::on_event`] runs the key map, yielding an optional
///   command the substrate consumes.
///
/// The host owns **no** terminal I/O itself — the loop lives in the driver.
/// That keeps the state/event contract pure and unit-testable without a TTY:
/// [`render_current`](TerminalHost::render_current) renders straight into a
/// [`Buffer`] you can assert on.
pub struct TerminalHost<S, R, C>
where
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    renderer: R,
    state: Option<S>,
    key_map: KeyMap<C>,
}

impl<S, R, C> TerminalHost<S, R, C>
where
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    /// Build a host from a renderer and a key map. No state until the first
    /// [`Host::on_state`] arrives — the substrate is the source of state.
    pub fn new(renderer: R, key_map: impl FnMut(KeyEvent) -> Option<C> + 'static) -> Self {
        Self {
            renderer,
            state: None,
            key_map: Box::new(key_map),
        }
    }

    /// The latest state, or `None` before the first [`Host::on_state`].
    pub fn state(&self) -> Option<&S> {
        self.state.as_ref()
    }

    /// The renderer this host projects through. The driver borrows it each
    /// frame; consumers rarely need it directly.
    pub fn renderer(&self) -> &R {
        &self.renderer
    }

    /// Render the current state into a fresh [`Buffer`] of `area`, or `None`
    /// if no state has arrived yet. Headless — this is the seam the crate's
    /// tests assert against, no terminal required.
    pub fn render_current(&self, area: Rect) -> Option<Buffer> {
        self.state
            .as_ref()
            .map(|state| crate::render_to_buffer(&self.renderer, state, area))
    }
}

impl<S, R, C> Host for TerminalHost<S, R, C>
where
    S: ViewState,
    R: Renderer<S>,
    R::Output: Widget,
{
    type State = S;
    type Renderer = R;
    type Command = C;
    type Event = KeyEvent;

    fn on_state(&mut self, state: S) {
        self.state = Some(state);
    }

    fn on_event(&mut self, event: KeyEvent) -> Option<C> {
        (self.key_map)(event)
    }
}
