# Positron

> AIs don't just *use* the UI — they *perceive*, *hook into*, and *act through* it as digital citizens. Same widget system, two consumers.

Positron is a **multi-target widget contract**: typed `ViewState`s produced by a state-owner are consumed by any renderer (DOM, terminal, AR/VR, future native) AND by any AI observer with a perception budget. One widget definition, multiple surfaces, AI-perceivable by construction.

It's the renderer-side companion to peer-to-peer agent protocols like [airc](https://github.com/CambrianTech/airc): airc defines how agents talk; positron defines what agents and humans *see together*.

---

## Why this exists

The widget systems most apps grow into entangle three things:

1. **State management** — what is the widget showing right now?
2. **Render logic** — how does that state turn into pixels (or terminal cells, or 3D meshes)?
3. **Event/command wiring** — what does the widget *do* when a user (or AI) acts?

Treating these as one inseparable concern produces 2000-line widgets where signal stores, instance fields, and global registries fight to be the source of truth — and where adding a second render target (terminal, AR/VR, mobile) means rewriting the widget.

Positron splits them at the right seam:

- **State** is a typed `ViewState`, produced by the substrate (Rust-authored, ts-rs-exported).
- **Render** is a `Renderer<S: ViewState>` impl — pure `render(state) -> tree`. Multiple renderers per `ViewState`.
- **Action** flows back through commands (event-up); state updates flow forward (state-down). Standard unidirectional dataflow.
- **AI perception** is a fourth consumer of the same `ViewState` — agents subscribe to state changes with a cognition budget, no extra wiring.

If you've worked with React/Svelte/SwiftUI you know the rendering shape. Positron adds: *the same `ViewState` renders to a TUI just as cleanly, and an AI agent perceives it the same way a user does*.

---

## Where this fits

```
┌─────────────────────────────────────────────────────────────┐
│  Substrate (your app's state-owner — e.g. Continuum, etc.) │
│  Produces typed ViewState updates                           │
└────────────────────┬────────────────────────────────────────┘
                     │  ViewState (ts-rs typed wire)
                     ▼
┌─────────────────────────────────────────────────────────────┐
│  positron-core                                              │
│  • ViewState trait                                          │
│  • Renderer<S: ViewState> trait                             │
│  • Host bridge (state-down / event-up)                      │
│  • Perception hooks (AI observers with cognition budget)    │
└────────────────────┬────────────────────────────────────────┘
                     │  same ViewState, many renderers
        ┌────────────┼────────────┬──────────────┬────────────┐
        ▼            ▼            ▼              ▼            ▼
   ┌─────────┐  ┌─────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
   │ Lit DOM │  │ ratatui │  │ Bevy AR  │  │ Mobile   │  │ AI agent │
   │ (web)   │  │ (term)  │  │ /VR      │  │ native   │  │ observer │
   └─────────┘  └─────────┘  └──────────┘  └──────────┘  └──────────┘
```

The substrate doesn't know which renderer is consuming its `ViewState`. The renderer doesn't know which substrate produced it. They only share the typed contract.

---

## The "airc test" — why this is a library, not an app

The four properties that qualify a layer as separately-versionable:

1. **Owns its own primitives.** Positron defines `ViewState`, `Renderer`, host bridge, perception hooks — none of which encode any specific application's domain.
2. **No consumer-specific knowledge.** Positron doesn't know what a "chat message" or a "persona" is. Continuum (the reference consumer) defines those as concrete `ViewState` impls.
3. **Consumable by anyone in its problem domain.** Any project with a state-producer + multi-target render benefit. Examples: a non-AI CLI tool wanting a terminal UI, a game UI with PC/console/VR targets, an IDE plugin wanting AI assistants to perceive its widgets.
4. **Independently versionable.** Contract evolves on its own clock; substrates and renderers track major versions.

Positron passes all four. That's why it's a separate repo.

---

## Status

**v0.1.x — contract design + first proof.** `positron-core` defines the four
primitives (`ViewState` / `Renderer` / `Host` / `Observer`) plus the wire and
session protocols; `examples/counter-cli` proves the "define once, project many"
thesis end-to-end in one process (one `Counter` `ViewState`, two
different-`Output` renderers, one `Observer` that perceives the same state and
acts through a `CommandEnvelope`). No transport, no substrate — the contract
standing on its own.

Run the proof: `cargo run -p counter-cli`.

Next (see `docs/ARCHITECTURE.md` § roadmap O3–O6):
- `positron-ratatui` — terminal renderer reference impl (O3)
- `positron-lit` — Lit DOM reference renderer + regenerate `@positron/core` (O4)
- `ContinuumHost` (in continuum) — session ↔ Commands/Events, first real `ViewState` (O5)
- persona `Observer` → RAG/tool bridge (O6)
- Theme pack (Loki / Matrix / Fallout / Tron) ported from the cyberpunk-cli experiment

See `DESIGN.md` for the contract design and the rationale behind each trait, and
`docs/ARCHITECTURE.md` for the **separation of concerns** — how one app definition
projects to web, terminal, mobile, and an AI persona, and where positron ends and
the substrate's `Commands`/`Events` begin.

---

## License

MIT (matching cambriantech's house pattern).
