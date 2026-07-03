/**
 * `LitHost` — the DOM surface's {@link Host} implementation, the glue
 * between the positron substrate (producing {@link ViewState} updates and
 * consuming commands) and a {@link LitRenderer}.
 *
 * State flows down: `onState` renders the state to a `TemplateResult` and
 * hands it to a **commit sink** — the seam that actually touches the DOM.
 * Events flow up: `onEvent` maps a surface event to an optional command
 * via an injected mapper (the app owns which DOM interaction means which
 * command; the host stays app-agnostic).
 *
 * ## Why the commit sink is injected
 *
 * Committing a `TemplateResult` into live DOM is Lit's `render(result,
 * container)` — it needs a real `Element`. Keeping that call BEHIND an
 * injected sink is the same pure/impure split as the Rust wgpu outlier
 * (`frame.rs` pure, `gpu.rs` touches the device): `LitHost` itself is
 * DOM-free and headlessly testable (a test injects a capturing sink),
 * while the real DOM commit lives in `./dom` (`domCommit`). The host
 * never imports a DOM value — only the `TemplateResult` type.
 *
 * This mirrors positron-ratatui's Host, which owns the render→backend
 * commit and the event→command translation for the terminal surface;
 * `LitHost` is the DOM analogue.
 */
import type { Host, ViewState } from "@positron/core";
import type { TemplateResult } from "lit";

import type { LitRenderer } from "./renderer";

/**
 * Commits a rendered `TemplateResult` to a surface. The production sink
 * ({@link "./dom".domCommit}) calls Lit's `render` into a container; a
 * test sink captures the result. This is the one seam that touches (or
 * stands in for) the DOM.
 */
export type CommitSink = (result: TemplateResult) => void;

/**
 * Translates a surface event into an optional command. `undefined` (the
 * Rust `Option::None`) means "this event produces no command". The app
 * supplies this; the host does not hardcode DOM-event semantics.
 */
export type EventToCommand<Event, Command> = (
  event: Event,
) => Command | undefined;

/** Construction inputs for a {@link LitHost}. */
export interface LitHostOptions<S extends ViewState, Command, Event> {
  /** The renderer this host drives (one per widget kind). */
  readonly renderer: LitRenderer<S>;
  /** Where rendered templates go — DOM in production, capture in tests. */
  readonly commit: CommitSink;
  /** How surface events become commands for the substrate. */
  readonly toCommand: EventToCommand<Event, Command>;
}

/**
 * The DOM {@link Host}: renders {@link ViewState} to Lit templates and
 * commits them through an injected sink; maps surface events to commands
 * through an injected mapper.
 */
export class LitHost<S extends ViewState, Command, Event>
  implements Host<S, Command, Event>
{
  readonly #renderer: LitRenderer<S>;
  readonly #commit: CommitSink;
  readonly #toCommand: EventToCommand<Event, Command>;
  #lastRendered: TemplateResult | undefined;

  constructor(options: LitHostOptions<S, Command, Event>) {
    this.#renderer = options.renderer;
    this.#commit = options.commit;
    this.#toCommand = options.toCommand;
  }

  /** New state from the substrate — render and commit to the surface. */
  onState(state: S): void {
    const tree = this.#renderer.render(state);
    this.#lastRendered = tree;
    this.#commit(tree);
  }

  /** Surface interaction — map to a command for the substrate, or none. */
  onEvent(event: Event): Command | undefined {
    return this.#toCommand(event);
  }

  /**
   * The most recently committed template, or `undefined` before the
   * first `onState`. Lets a caller (or a test) inspect what was last
   * rendered without re-running the renderer.
   */
  get lastRendered(): TemplateResult | undefined {
    return this.#lastRendered;
  }
}
