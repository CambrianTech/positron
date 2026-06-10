# Positron Design

> The contract is the thing. This doc defines `ViewState`, `Renderer`, the host bridge, and the perception hook — the four primitives positron actually owns.

## The four primitives

### 1. `ViewState` — what the widget IS at this moment

A `ViewState` is a typed, serializable, immutable snapshot of what a widget should display. Produced by the state-owner; consumed by zero-or-more renderers and zero-or-more AI observers.

```rust
pub trait ViewState: Clone + Send + Sync + std::fmt::Debug {
    /// Stable identifier for this widget kind (e.g. "chat", "user-list").
    /// Used by hosts to route state updates to the correct renderer
    /// and by observers to filter perception subscriptions.
    fn kind(&self) -> &'static str;

    /// Optional revision marker — lets renderers and observers detect
    /// "did anything change since the last state I saw?" without
    /// deep-equality. None means "treat every update as new."
    fn revision(&self) -> Option<u64> { None }
}
```

**What `ViewState` is NOT:**
- Not a render tree (that's what `Renderer::render` produces)
- Not a state machine (state owners — substrates — decide transitions)
- Not a DOM/terminal/3D object (renderers translate it)
- Not an entity (entities are storage; ViewStates are projection)

Concrete `ViewState` impls live in CONSUMER code. Positron defines the trait; Continuum defines `ChatViewState`. Same as how airc defines `Peer` but consumers define what flies between peers.

### 2. `Renderer<S: ViewState>` — how state turns into a surface

A renderer is a pure-ish function: `state -> rendered_tree`. The tree shape is renderer-specific (Lit `TemplateResult`, ratatui `Widget`, Bevy `Bundle`); the contract is that the renderer commits to producing one consistent tree per state.

```rust
pub trait Renderer<S: ViewState> {
    /// The rendered output type for this renderer (e.g.
    /// `LitTemplate`, `RatatuiFrame`, `BevyEntity`).
    type Output;

    /// Pure-ish render. May allocate; must not mutate the state.
    /// Determinism is required up to the renderer's own targets
    /// (e.g. layout decisions on viewport size are inputs, not state).
    fn render(&self, state: &S) -> Self::Output;
}
```

**Why pure-ish:** real renderers allocate, ask the terminal for its size, query the DOM for focus state. The CONTRACT is that the rendered output is a function of (state, viewport-context) — not of widget-local mutable history. No `_signals`, no `currentRoom: Option<RoomEntity>` instance fields, no `PositronWidgetState.subscribeToWidget('profile', ...)` cross-widget reach-across.

### 3. Host bridge — state-down, event-up

A host is the glue between the substrate and the renderer. It subscribes to substrate state updates and pushes them through the renderer; it captures user interactions and emits them back as commands.

```rust
pub trait Host {
    type State: ViewState;
    type Renderer: Renderer<Self::State>;
    type Command;
    type Event;

    /// New state arrived from the substrate. Re-render and update
    /// the surface.
    fn on_state(&mut self, state: Self::State);

    /// User (or AI) interaction happened. Translate to a command
    /// and forward to the substrate.
    fn on_event(&mut self, event: Self::Event) -> Option<Self::Command>;
}
```

The DOM host implements this with Lit + browser event listeners. The terminal host implements this with ratatui + crossterm key events. The AR/VR host implements this with Bevy + input mappings. Same trait, three implementations.

### 4. Perception hooks — AIs observe what humans see

The positron principle: AI personas perceive widget state through the same `ViewState` updates that drive rendering. No separate "AI view" of the system; one source of truth, multiple consumers.

```rust
/// An observer that wants to be notified when a ViewState changes.
/// Perception has a budget — observers receive at most `budget_hz`
/// updates per second per widget. Substrate enforces.
pub trait Observer<S: ViewState>: Send + Sync {
    /// Substrate-routed identifier of this observer (e.g. an AI
    /// persona id). Used to scope cognition-budget accounting.
    fn observer_id(&self) -> &str;

    /// How frequently this observer wants to see state updates.
    /// Substrate may quantize down under load (governor concern).
    fn budget_hz(&self) -> u32;

    /// A state change happened. Pure observation — observers
    /// don't mutate state; they perceive it and may emit commands
    /// through their own pathways (e.g. AI cognition pipeline).
    fn on_change(&self, state: &S);
}
```

**Why this matters:** if an AI persona is sitting in a chat room, the persona's perception of the room's state IS the `ChatViewState` the user's screen renders. Not a separate AI-only API. Same source. Same fields. Same rev counter.

That's the bridge between human and AI consumers of the same UI — the architecturally-load-bearing part of the positron principle.

---

## The 4 state layers (per the original positron docs)

Not every widget update is the same urgency. Positron classifies state updates into four layers, each with its own SLA:

| Layer | Cadence | Use case | Example |
|---|---|---|---|
| **Ephemeral** | 60 Hz | Animations, hover, typing-in-progress | Cursor blink, drag preview |
| **Session** | 1–10 Hz | User-perceivable changes | New message arrived, room switched |
| **Persistent** | < 1 Hz | Long-lived state | User profile edit, theme change |
| **Semantic** | On-demand | AI-tier meaning extraction | "The conversation just shifted topic" |

Renderers subscribe at the layer their target can sustain. A terminal renderer might cap at Session (1–10 Hz); a DOM renderer can handle Ephemeral; an AR/VR renderer needs Ephemeral for head-tracking. AI observers typically operate at Session or Semantic — their cognition budget can't sustain Ephemeral perception.

This is the throttling primitive that lets one `ViewState` source serve a 60fps DOM renderer AND a 4-Hz AI observer without coordination.

---

## What positron does NOT do

- **Does not define your widgets.** No `ChatViewState` in positron. Consumers (like Continuum) define their concrete state types.
- **Does not own substrate-side state management.** Positron is the surface contract. Substrates use whatever they want (Redux, signals, Rust channels, airc events, etc.) to PRODUCE `ViewState` updates.
- **Does not bundle a renderer.** Reference renderers ship as separate crates (`positron-ratatui`, `positron-lit`); consumers pick the ones they need.
- **Does not own commands.** Commands flow from host → substrate. Positron defines the bridge shape; substrates define their command vocabulary.

---

## Open questions (v0 → v1 design space)

1. **Diffing strategy.** Should positron mandate a tree-diff at the renderer layer, or leave it to each renderer? (Lit has its own diffing; ratatui re-renders the frame; Bevy uses ECS reconciliation. Leaning: leave it.)
2. **Async ViewState arrival.** Does `Host::on_state` need to be async? (DOM hosts can be sync; AR/VR hosts might want async for vsync alignment. Leaning: provide both sync + async traits, hosts pick.)
3. **Theming.** Is theming a `ViewState` concern (every state carries theme info) or a `Renderer` concern (renderers know themes; state is theme-agnostic)? (Leaning: renderer concern. State stays semantic; presentation belongs to the surface.)
4. **Server-driven UI vs client-driven layout.** When the substrate ships `ViewState`, does it include layout (positions, sizes) or just semantic content? (Leaning: semantic content only. Layout is renderer/host responsibility — different surfaces have different layout primitives.)

These are TBD before v1.0; the v0 contract above is the foundation regardless of which way they resolve.

---

## Versioning

- `0.0.x` — contract design + reference renderers. Breaking changes allowed.
- `0.x.x` — first consumer (Continuum) integration; API hardening.
- `1.0.0` — stable contract. Semver from here.
