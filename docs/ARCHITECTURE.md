# Positron Architecture — Define Once, Project Many

> `DESIGN.md` defines the four primitives. This doc defines the **separation of
> concerns** between them and the substrate they sit on — the load-bearing
> contract that lets one app definition serve web, terminal, mobile, and an AI
> persona without any surface reinventing state, dispatch, or perception.

## The thesis

**An app is defined once as a `(ViewState, command-vocabulary)` pair. Every
surface is a pure projection of it.** Adding a surface is writing one function,
not rewriting the widget.

```
                    ┌───────────────────────────────┐
                    │  ONE definition               │
                    │  • ViewState  (what it shows)  │
                    │  • commands   (what it does)   │
                    │  • StateLayer (how fast)       │
                    └───────────────┬───────────────┘
                                    │  projected by
        ┌──────────────┬────────────┼────────────┬──────────────────┐
        ▼              ▼            ▼            ▼                  ▼
   Renderer<S>    Renderer<S>   Renderer<S>   Renderer<S>       Observer<S>
     → DOM          → cells       → native      (future)          → RAG
     (web/Lit)    (terminal)     (mobile)                      (AI persona)
                                                              acts via CommandEnvelope
```

The four "environments" collapse to **two roles**:

| Role | Consumes | Produces | Surfaces |
|---|---|---|---|
| `Renderer<S>` | the `ViewState` | a surface tree | web (Lit DOM), terminal (ratatui), mobile (native), AR/VR (Bevy) |
| `Observer<S>` | the **same** `ViewState` | perception → RAG, then `CommandEnvelope`s | AI persona |

The AI persona is **not a special case**. It is the fourth projection of the
identical `ViewState` — it perceives what a human sees, and it acts through the
same command vocabulary a human's host emits. That is the positron principle,
made structural.

## Separation of concerns — the layer boundary (the load-bearing part)

Positron **owns**:

- the view/render/perceive **contract** — `ViewState` / `Renderer` / `Host` / `Observer`
- the state/command **wire semantics** — `StateEnvelope` / `CommandEnvelope` / `ObserverSpec`
- the **session protocol** — snapshot-then-live resync (`ClientMessage` / `ServerMessage`)

Positron does **NOT** own — and must never grow:

- state management (substrates use whatever they want to *produce* `ViewState`)
- a command **vocabulary** (`chat/send` etc. are the consumer's)
- a **transport** (WebSocket / UDS / airc are the substrate's)
- **a second dispatch bus or event bus** ← the anti-pattern this doc exists to forbid

The substrate (Continuum is the reference consumer) owns state + the command
vocabulary, exposed through its **two universal primitives**: `Commands.execute`
(request/response) and `Events` (publish/subscribe).

### positron session **lowers onto** Commands/Events — it does not compete

The one rule that keeps this from becoming two parallel systems:

| positron frame | lowers to (substrate) |
|---|---|
| `ClientMessage::Command(CommandEnvelope)` | `Commands.execute(command, params)` — **the one dispatch owner**, not a second |
| `ClientMessage::Subscribe { kinds, layers }` | subscribe to the `Events` that feed those `ViewState` projections; substrate emits `ServerMessage::State` snapshot-then-live |
| `ClientMessage::Observe { spec }` | the same subscription, scoped by the observer's cognition budget |
| `ServerMessage::State(StateEnvelope)` | emitted by a **projector**: a substrate-side task that subscribes to `Events` and builds the `ViewState` |
| `ServerMessage::CommandFailed` | the loud failure path of `Commands.execute` (success needs no ack — the state change *is* the ack) |

The seam that performs this lowering is the **`ContinuumHost`** (already named in
`session.rs` tests). It is the single adapter where positron frames meet
`Commands`/`Events`. Positron contributes the frame; the substrate contributes
the dispatch and the bus.

### Why "two wire protocols" is not a duplication

- `continuum/ipc/ws.rs` (`WsClientMessage`/`WsServerMessage`, correlation-id
  command RPC) = the **raw** Commands/Events transport for thin clients.
- `positron/session.rs` (`Subscribe`/`Command`/`Observe` → `State`) = the
  **view-projection** protocol layered on top.

Resolution: **positron session is implemented *over* the substrate primitives.**
`ContinuumHost` maps `Subscribe → Events-subscription` and `Command →
Commands.execute`. One dispatch, one bus; positron adds view-projection,
snapshot-then-live resync, and AI-observer budgeting that the raw ws protocol
does not carry.

> **Open, deliberately not pre-decided:** whether the two wires should eventually
> *merge* — positron's session protocol becoming Continuum's canonical
> thin-client wire, retiring the bespoke `WsClientMessage`. That is a real
> candidate, but it is an **integration-time** decision (task O5 below), made with
> both protocols in front of us, not a guess made now. Flag it; don't foreclose it.

## Where each piece lives (separation of repos)

| Concern | Home | Why |
|---|---|---|
| The four traits + wire + session | **positron** (`positron-core`) | consumer-agnostic contract; independently versioned (the "airc test") |
| Reference renderer crates (`positron-ratatui`, `positron-ts`/lit) | **positron** | surface projections anyone in the problem domain reuses |
| Concrete `ViewState` types (`ChatViewState`, …) | **continuum** | domain vocabulary — positron never knows what a "chat message" is |
| Command vocabulary (360+ commands) | **continuum** | already the single source of truth; positron frames, does not define |
| `ContinuumHost` adapter (session ↔ Commands/Events) | **continuum** | binds the contract to *this* substrate's primitives |
| Persona `Observer` → RAG/tool bridge | **continuum** | perception into cognition is a substrate concern |
| The `apps/` (web/desktop/mobile) | **continuum** | consumers of the renderer crates |

## The "define once" mechanics (the compression)

To **define** a view: one `ViewState` type + the command names it may emit
(already in continuum) + the `StateLayer` it emits at.

- To add a **surface**: write **one** `Renderer<ThatViewState>`. Nothing else.
- To make it **AI-perceivable**: write **zero** extra code — the `Observer`
  consumes the same `ViewState`.

That is the compression principle at the UI layer: one logical view, one place;
the surfaces are pure functions of it. The anti-pattern it kills is the
2000-line widget where signal stores, instance fields, and global registries
fight to be the source of truth (see `DESIGN.md` § Renderer). If you can't render
the view from `ViewState` alone, the state type is incomplete — not the renderer.

## Constraints this doc pins on DESIGN.md's open questions

- **Layout (open Q #4): semantic content only.** "One `ViewState`, four surfaces"
  *requires* it — a terminal cannot consume DOM pixel coordinates, and a persona
  perceives meaning, not geometry. `ViewState` carries semantics; each `Renderer`
  owns its own layout. This is now a contract constraint, not a lean.
- **Theming (open Q #3): renderer concern.** Same reasoning — state stays
  presentation-free; a theme is a property of a surface, not of the app definition.
- **`positron-ts` vs continuum `sdk/typescript`: decided at O4, not before.** Both
  generate types from Rust via ts-rs; whether positron-ts *subsumes* or
  *complements* the continuum SDK is resolved when the web renderer actually lands.

## Organizational task roadmap (one PR per unit)

| # | Unit | Proves | Depends on |
|---|---|---|---|
| **O1** | This separation contract | the boundary is pinned before any renderer is written | — |
| **O2** | `examples/counter-cli` | renderer-agnostic **and** AI-perceives-same-state, in-process, zero transport (one `Counter` `ViewState`, ≥2 `Renderer`s, 1 `Observer`) | O1 |
| **O3** | `positron-ratatui` | terminal `Renderer` reference (outlier A surface) | O2 |
| **O4** | `positron-ts` / lit | web DOM `Renderer` + regenerate `@positron/core`; reconcile with continuum `sdk/typescript` (subsume vs complement) | O2 |
| **O5** | `ContinuumHost` (in continuum) | positron session ↔ Commands/Events; first real `ViewState` (`ChatViewState`) flows to a positron renderer; resolves the two-wire merge question | O3 or O4 |
| **O6** | persona `Observer` → RAG/tool bridge (in continuum) | perception into cognition + action as `CommandEnvelope` — closes "AI persona rag/tool integration" | O5 |

O2 is the next unit: it makes this doc non-vapor by proving "one `ViewState`,
many renderers, plus an observer perceiving the same state" in a single process,
before any transport or substrate is involved.
