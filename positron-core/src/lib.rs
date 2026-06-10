//! # positron-core
//!
//! The multi-target widget contract: typed `ViewState`s produced by a
//! state-owner are consumed by any renderer (DOM, terminal, AR/VR,
//! future native) AND by any AI observer with a perception budget.
//! One widget definition, multiple surfaces, AI-perceivable by
//! construction.
//!
//! See `DESIGN.md` at the repo root for the contract rationale and
//! `README.md` for the broader vision (the "positron principle":
//! AIs and humans consume the same widget system).
//!
//! ## The four primitives
//!
//! - [`ViewState`] — a typed, serializable, immutable snapshot of what
//!   a widget should display. Substrate produces; renderers and
//!   observers consume.
//! - [`Renderer`] — pure-ish `state -> tree`. One ViewState type can
//!   have many renderer impls (Lit DOM, ratatui terminal, Bevy AR/VR).
//! - [`Host`] — glue between substrate and renderer; routes state-down
//!   and event-up.
//! - [`Observer`] — perceives `ViewState` changes; what makes the
//!   substrate AI-native. Cognition budget enforced by the substrate.
//!
//! ## What this crate does NOT provide
//!
//! - No concrete `ViewState` types (those live in consumer code, e.g.
//!   continuum's `ChatViewState`)
//! - No reference renderers (those ship as separate crates:
//!   `positron-ratatui`, `positron-lit`, etc.)
//! - No substrate state management (positron is the surface contract;
//!   substrates use whatever they want — Redux, signals, Rust
//!   channels, airc events — to PRODUCE `ViewState` updates)
//!
//! ## Versioning
//!
//! v0.0.x — contract design + reference renderers. Breaking changes
//! allowed. v1.0 is the first stable contract; consumers should pin
//! to a major-version range from there.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::fmt::Debug;

// ============================================================================
// VIEW STATE
// ============================================================================

/// A typed, serializable, immutable snapshot of what a widget should
/// display at this moment.
///
/// Produced by the state-owner (substrate); consumed by zero-or-more
/// [`Renderer`]s and zero-or-more [`Observer`]s. Same source of truth
/// for human-facing rendering AND AI perception — the architecturally
/// load-bearing part of the positron principle.
///
/// ## What this trait IS NOT
///
/// - **Not a render tree** — that's what [`Renderer::render`] produces.
/// - **Not a state machine** — substrate decides state transitions.
/// - **Not a DOM/terminal/3D primitive** — those are renderer concerns.
/// - **Not an entity** — entities are storage; ViewStates are projection.
///
/// ## Concrete impls live in CONSUMER code
///
/// Positron defines the trait; consumers (Continuum, etc.) define
/// their concrete `ViewState` types (e.g. `ChatViewState`,
/// `UserListViewState`). Same as how airc defines `Peer` but
/// consumers define what flies between peers.
///
/// ## Layered cadence (see `DESIGN.md` § "The 4 state layers")
///
/// Not every update is the same urgency. Substrates classify
/// ViewState updates as Ephemeral (60Hz) / Session (1–10Hz) /
/// Persistent (<1Hz) / Semantic (on-demand). Renderers and observers
/// subscribe at the layer their target can sustain. The classifier
/// itself is substrate-side; the layering is documented in `kind()`
/// + observer budget conventions.
pub trait ViewState: Clone + Send + Sync + Debug + 'static {
    /// Stable string identifier for this widget kind. Used by hosts
    /// to route state updates to the correct renderer and by
    /// observers to filter perception subscriptions.
    ///
    /// Conventionally lower-kebab-case, e.g. `"chat"`, `"user-list"`,
    /// `"profile"`. Globally unique within a substrate's namespace;
    /// recommended prefix when ambiguity is possible (e.g.
    /// `"continuum/chat"`).
    fn kind(&self) -> &'static str;

    /// Optional revision marker. Lets renderers and observers detect
    /// "did anything change since the last state I saw?" without
    /// deep-equality. `None` means "treat every update as new"
    /// (default — substrates opt in to revisioning when meaningful).
    fn revision(&self) -> Option<u64> {
        None
    }
}

// ============================================================================
// RENDERER
// ============================================================================

/// Translates a [`ViewState`] into a surface-specific output tree.
///
/// **Pure-ish** in the strict sense: the output is a deterministic
/// function of `(state, viewport-context)`. Renderers MAY allocate,
/// query the surface (DOM focus, terminal size), and produce
/// surface-shaped objects. Renderers MUST NOT carry widget-local
/// mutable history.
///
/// > The anti-pattern this trait kills: 2304-line widgets with three
/// > coexisting state systems (signal stores + instance fields +
/// > global registries) fighting to be the source of truth. If you
/// > can't render the widget from state alone, your state type is
/// > incomplete — not your renderer.
///
/// ## Multiple renderers per `ViewState`
///
/// One `ChatViewState` can have a `LitChatRenderer`, a
/// `RatatuiChatRenderer`, and a `BevyChatRenderer`. Each implements
/// this trait with its own `Output` type. The substrate doesn't know
/// which renderer is consuming its state.
pub trait Renderer<S: ViewState> {
    /// The rendered output type for this renderer. DOM renderers
    /// produce Lit `TemplateResult`; terminal renderers produce
    /// ratatui `Widget`s; AR/VR renderers produce Bevy `Bundle`s.
    /// Positron doesn't constrain the shape; renderer crates do.
    type Output;

    /// Pure-ish render of state → output. May allocate; must not
    /// mutate the state. May read surface-context (viewport size,
    /// focus state) treating it as an additional input.
    fn render(&self, state: &S) -> Self::Output;
}

// ============================================================================
// HOST BRIDGE
// ============================================================================

/// Glue between the substrate (producing [`ViewState`] updates) and
/// the [`Renderer`] (turning them into a surface).
///
/// The host owns the I/O — it subscribes to substrate state changes,
/// drives the renderer with new state, captures user events, and
/// routes them back to the substrate as commands. Each surface
/// (browser DOM, terminal, AR/VR, mobile native) needs its own host
/// implementation; renderers are surface-agnostic.
///
/// ## State-down, event-up
///
/// The unidirectional dataflow shape. New state arrives → host calls
/// `on_state` → renderer produces a new surface tree → host updates
/// the surface. User interacts → host translates raw input to a
/// typed `Event` → `on_event` returns an optional `Command` → host
/// forwards to substrate.
pub trait Host {
    /// The `ViewState` type this host renders.
    type State: ViewState;
    /// The `Renderer` impl this host uses.
    type Renderer: Renderer<Self::State>;
    /// Typed command vocabulary — what the substrate can act on. Hosts
    /// translate user input (clicks, keys, gestures) to commands.
    type Command;
    /// Typed event vocabulary — surface-specific input shapes. Hosts
    /// receive these from their surface (DOM events, terminal key
    /// presses, AR controller buttons) and translate to commands.
    type Event;

    /// New state arrived from the substrate. Re-render and update
    /// the surface accordingly.
    fn on_state(&mut self, state: Self::State);

    /// User (or AI) interaction happened. Translate to a typed
    /// command if applicable; substrate consumes commands.
    fn on_event(&mut self, event: Self::Event) -> Option<Self::Command>;
}

// ============================================================================
// OBSERVER (AI PERCEPTION)
// ============================================================================

/// Perceives [`ViewState`] changes. What makes positron AI-native:
/// AI personas consume the SAME `ViewState` updates that drive
/// human-facing rendering. No separate "AI view" of the system; one
/// source of truth, many consumers.
///
/// ## Cognition budget
///
/// Perception isn't free. Each observer declares a `budget_hz` —
/// how frequently it wants to see state updates. Substrates enforce
/// the budget: under load, an observer asking for 60 Hz might be
/// quantized down to 4 Hz. The observer doesn't crash; it just sees
/// fewer updates.
///
/// This is the throttling primitive that lets one `ViewState` source
/// serve a 60fps DOM renderer AND a 4-Hz AI observer simultaneously
/// without coordination between them.
///
/// ## Observation, not mutation
///
/// Observers don't write to state. They perceive it. If an observer
/// wants to act on what it sees, it emits commands through its own
/// pathway (e.g. an AI persona's cognition pipeline produces
/// commands that flow through the same substrate the human-facing
/// host uses).
pub trait Observer<S: ViewState>: Send + Sync {
    /// Substrate-routed identifier of this observer (e.g. an AI
    /// persona's UUID, rendered as a string for portability). Used to
    /// scope cognition-budget accounting and to support typed audit
    /// logs ("Maya observed the chat at t=...").
    fn observer_id(&self) -> &str;

    /// Maximum perception frequency this observer requests, in Hz.
    /// Substrate may quantize down under load (governor concern).
    /// Values:
    /// - `60` — animation-coupled (rare for AI observers)
    /// - `10` — typical Session-tier human-perceivable update rate
    /// - `4`–`1` — typical AI cognition pacing
    /// - `0` — observer wants pull, not push (substrate provides
    ///   state on demand only)
    fn budget_hz(&self) -> u32;

    /// State change happened. Pure observation — observers don't
    /// mutate. May emit commands through their own pathways
    /// (substrate routes them through the normal command path).
    fn on_change(&self, state: &S);
}

// ============================================================================
// MINIMAL TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: a trivial `ViewState` impl compiles and the trait
    /// shape is sensible. The intentionally tiny shape — a counter —
    /// is what the first reference renderer (`examples/counter-cli`,
    /// landing next) will consume.
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

    /// Smoke test: a trivial `Renderer` impl compiles. Pretends to
    /// render to a `String`, which is what a "renderer for tests"
    /// would do. Real renderers produce surface-shaped outputs.
    struct StringRenderer;

    impl Renderer<Counter> for StringRenderer {
        type Output = String;
        fn render(&self, state: &Counter) -> String {
            format!("counter @ rev {} = {}", state.revision, state.value)
        }
    }

    #[test]
    fn view_state_kind_is_stable() {
        let c = Counter {
            value: 0,
            revision: 0,
        };
        assert_eq!(c.kind(), "counter");
    }

    #[test]
    fn view_state_revision_round_trips() {
        let c = Counter {
            value: 42,
            revision: 7,
        };
        assert_eq!(c.revision(), Some(7));
    }

    #[test]
    fn renderer_produces_deterministic_output() {
        let c = Counter {
            value: 13,
            revision: 1,
        };
        let r = StringRenderer;
        assert_eq!(r.render(&c), "counter @ rev 1 = 13");
        // Determinism: same input → same output.
        assert_eq!(r.render(&c), r.render(&c));
    }
}
