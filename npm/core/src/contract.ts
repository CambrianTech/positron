/**
 * The four positron primitives as TypeScript interfaces — the
 * hand-authored TS side of the contract, parallel to the Rust traits in
 * `positron-core/src/lib.rs`.
 *
 * ## Why these are hand-authored (not in `./generated/`)
 *
 * `ts-rs` projects *data* — the structs and enums that cross the wire
 * (see `./generated/`: `StateEnvelope`, `CommandEnvelope`, …). These
 * four are *behavioural* contracts — traits with methods — and a trait
 * has no serialized form to generate. The Rust trait and this interface
 * are the SAME contract expressed in two languages; the generated wire
 * types are the data those contracts move. They change rarely (v0.x
 * contract) and the Rust side in `lib.rs` is the review source of truth
 * — keep the two in sync by hand when a primitive's shape changes.
 *
 * ## Naming
 *
 * Rust `snake_case` methods become TS `camelCase` members
 * (`observer_id` → `observerId`, `budget_hz` → `budgetHz`), matching the
 * continuum mixin convention. Rust associated types (`Renderer::Output`,
 * `Host::Command`/`Event`) become TS generic parameters.
 */

/**
 * A typed, immutable snapshot of what a widget should display — the TS
 * analogue of the Rust `ViewState` trait. Produced by the substrate,
 * consumed by zero-or-more {@link Renderer}s and {@link Observer}s.
 *
 * The Rust trait exposes `kind()` / `revision()` as methods; here they
 * are `readonly` members (per the house TS convention of property-style
 * accessors), which is also how a decoded `StateEnvelope` payload
 * presents to a renderer.
 */
export interface ViewState {
  /**
   * Stable identifier for this widget kind — routes state to the right
   * renderer and scopes observer subscriptions. Conventionally
   * lower-kebab-case (`"chat"`, `"user-list"`); matches
   * `StateEnvelope.kind` on the wire.
   */
  readonly kind: string;

  /**
   * Optional revision marker so consumers can detect "did anything
   * change since the last state I saw?" without deep-equality.
   * `undefined` (the Rust `None`) means "treat every update as new".
   * Matches `StateEnvelope.revision` on the wire.
   */
  readonly revision?: number;
}

/**
 * Translates a {@link ViewState} into a surface-specific output tree —
 * the TS analogue of the Rust `Renderer<S>` trait. The Rust associated
 * `type Output` becomes the second generic parameter here, so one
 * `ViewState` can have many renderers with different outputs (Lit
 * `TemplateResult`, a terminal string, a GPU frame).
 *
 * Pure-ish: the output is a deterministic function of `state`. A
 * renderer MAY allocate and read surface context (viewport, focus) as
 * additional input, but MUST NOT carry widget-local mutable history — if
 * you can't render from `state` alone, the state type is incomplete.
 */
export interface Renderer<S extends ViewState, Output> {
  /** Pure-ish render of state → output. Must not mutate `state`. */
  render(state: S): Output;
}

/**
 * Glue between the substrate (producing {@link ViewState} updates) and a
 * {@link Renderer} — the TS analogue of the Rust `Host` trait. State
 * flows down (`onState`), events flow up as optional commands
 * (`onEvent`). Each surface (DOM, terminal, native) needs its own host;
 * renderers stay surface-agnostic.
 *
 * The Rust trait's `State`/`Command`/`Event` associated types become
 * generic parameters; the renderer a host drives is an implementation
 * detail it holds, not part of this method surface.
 */
export interface Host<S extends ViewState, Command, Event> {
  /** New state arrived from the substrate — re-render the surface. */
  onState(state: S): void;

  /**
   * User (or AI) interaction happened. Translate to a typed command if
   * applicable; `undefined` (the Rust `Option::None`) means "no command
   * for this event". The substrate consumes returned commands.
   */
  onEvent(event: Event): Command | undefined;
}

/**
 * Perceives {@link ViewState} changes — the TS analogue of the Rust
 * `Observer<S>` trait, and what makes positron AI-native: an AI persona
 * consumes the SAME state updates that drive human rendering.
 *
 * An observer's `observerId` / `budgetHz` correspond to the wire
 * `ObserverSpec.observer_id` / `budget_hz` it registers with; the
 * substrate enforces the budget (quantizing down under load). Observers
 * perceive, they don't mutate — acting happens through the normal
 * command path.
 */
export interface Observer<S extends ViewState> {
  /**
   * Substrate-routed identifier of this observer (e.g. a persona UUID as
   * a string). Scopes cognition-budget accounting and audit logs.
   */
  readonly observerId: string;

  /**
   * Maximum perception frequency requested, in Hz. Substrate may
   * quantize down under load. `0` = pull, not push (state on demand).
   */
  readonly budgetHz: number;

  /** A state change happened — pure observation, no mutation. */
  onChange(state: S): void;
}
