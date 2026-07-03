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

> The boxes illustrate the **role** — every surface is a `Renderer<S>` projection
> of the same `ViewState`. They are *not* the primary-renderer choice: as
> § *Renderer paradigms are GPU-first* (below) makes precise, one Rust wgpu
> renderer covers web + native + AR/VR, so "DOM" and "native" are not separate
> surfaces but two of that one renderer's targets.

The four "environments" collapse to **two roles**:

| Role | Consumes | Produces | Surfaces |
|---|---|---|---|
| `Renderer<S>` | the `ViewState` | a surface (GPU frame / cells / DOM) | **wgpu GPU frame** — one Rust renderer running native (Metal / Vulkan / DX12), web (WebGPU, WebGL fallback, via WASM), and AR/VR (Bevy is wgpu underneath); terminal (ratatui); optional DOM (Lit, a11y/text-reflow) |
| `Observer<S>` | the **same** `ViewState` | perception → RAG, then `CommandEnvelope`s | AI persona |

The AI persona is **not a special case**. It is the fourth projection of the
identical `ViewState` — it perceives what a human sees, and it acts through the
same command vocabulary a human's host emits. That is the positron principle,
made structural.

### Renderer paradigms are GPU-first — "web" ≠ "the DOM"

`Renderer<S>`'s `type Output` is renderer-specific, so nothing in the contract
assumes a CPU text tree or a DOM. That is what lets the **primary** surface be a
single Rust **wgpu** GPU renderer that compiles to *every* paradigm from one
codebase:

| Paradigm | How the one wgpu renderer reaches it |
|---|---|
| Native desktop | wgpu → Metal (macOS) / Vulkan (Linux) / DX12 (Windows) |
| Web | wgpu → WebGPU (WebGL fallback), shipped as WASM — **same source** |
| AR/VR | Bevy, which is wgpu underneath |
| Terminal | `positron-ratatui` — CPU cells, a genuinely different `Output` |
| DOM | `positron-lit` — optional a11y / text-reflow surface, not the primary web story |

The DOM (Lit) and terminal (ratatui) are **peer paradigms**, not the definition
of "web" and "native." This makes the GPU renderer the real **outlier B** for
validating `Renderer<S>`: `counter-cli`'s `String` vs `Vec<String>` are both CPU
text (a weak outlier pair); a GPU draw-list/frame is maximally different and
proves the trait carries no hidden CPU-tree assumption.

This also means positron does **not** invent a new GPU path — it *converges*
with one continuum already has: continuum's avatar renderer is Bevy/wgpu emitting
backend-neutral `RgbaFrame`s over a crossbeam channel that LiveKit and PNG
consume without knowing Bevy (a `RenderBackend` seam). `SceneDescription`/
`RgbaFrame` (continuum) is the same shape as `ViewState`/frame (positron); the
two reconcile at **O5**, not by rewriting either.

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

The seam that performs this lowering is the **`ContinuumHost`** (referenced by
name in a `session.rs` test comment — not yet a wired symbol). It is the single
adapter where positron frames meet `Commands`/`Events`. Positron contributes the
frame; the substrate contributes the dispatch and the bus.

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
| Reference renderer crates (`positron-ratatui`, `positron-wgpu`, `positron-lit`) | **positron** | surface projections anyone in the problem domain reuses |
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
- **Theming (open Q #3): renderer concern (strong default, not a hard structural
  constraint).** Same *direction* as layout — presentation belongs to the surface,
  so `ViewState` stays theme-free and each `Renderer` knows themes. Unlike layout
  geometry, a theme *token* is arguably semantic and could ride in state, so this
  is the recommended default rather than a primitive-enforced necessity; DESIGN.md
  leans the same way.
- **`@positron/core` (npm) + `positron-lit` vs continuum `sdk/typescript`: decided
  at O4b, not before.** Both generate types from Rust via ts-rs; whether the positron
  DOM stack *subsumes* or *complements* the continuum SDK is resolved when the Lit
  renderer actually lands. (O4 is `positron-wgpu`, the GPU surface — it has no bearing
  on the npm/SDK reconcile, which is the DOM stack's concern.)

## Organizational task roadmap (one PR per unit)

| # | Unit | Proves | Depends on |
|---|---|---|---|
| **O1** | This separation contract | the boundary is pinned before any renderer is written | — |
| **O2** | `examples/counter-cli` | renderer-agnostic **and** AI-perceives-same-state, in-process, zero transport (one `Counter` `ViewState`, ≥2 `Renderer`s, 1 `Observer`) | O1 |
| **O3** | `positron-ratatui` | terminal `Renderer` reference + `Host` event loop (**outlier A**: CPU cells, real stateful surface) | O2 |
| **O4** | `positron-wgpu` | the run-everywhere GPU `Renderer` (**outlier B**: native Metal/Vulkan/DX12 + web WebGPU/WASM from one codebase) — proves `Renderer<S>` carries no CPU-tree assumption; the load-bearing web+native surface | O2 |
| **O4b** | `positron-lit` *(optional)* | Lit DOM `Renderer` + regenerate `@positron/core`; a11y / text-reflow surface where the GPU path isn't ideal; reconcile with continuum `sdk/typescript` (subsume vs complement) | O4 |
| **O5** | `ContinuumHost` (in continuum) | positron session ↔ Commands/Events; first real `ViewState` (`ChatViewState`) flows to a positron renderer; reconciles positron's frame output with continuum's existing `RenderBackend`/`RgbaFrame` GPU seam; resolves the two-wire merge question | O3 or O4 |
| **O6** | persona `Observer` → RAG/tool bridge (in continuum) | perception into cognition + action as `CommandEnvelope` — closes "AI persona rag/tool integration" | O5 |

O1 and O2 have landed: the boundary is pinned, and `examples/counter-cli` proves
"one `ViewState`, many renderers, plus an observer perceiving the same state" in
a single process, before any transport or substrate. **O3 is the next unit** —
`positron-ratatui`, the first real stateful `Renderer` + `Host` event loop
(outlier A), ahead of the GPU renderer (O4, outlier B) that makes the "web ≠ DOM"
claim real.
