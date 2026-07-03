/**
 * @positron/lit — the Lit DOM renderer for positron.
 *
 * The "web ≠ DOM as a peer paradigm" surface: a {@link LitRenderer}
 * projects a `@positron/core` `ViewState` to a Lit `TemplateResult`, and
 * a {@link LitHost} drives that renderer and commits the result into
 * live DOM. It is the TS/DOM outlier of the same `Renderer`/`Host`
 * contract the Rust wgpu backend implements for the GPU — one contract,
 * many surfaces.
 *
 * Two halves, split by whether they touch the DOM (the same pure/impure
 * split as the Rust wgpu outlier's `frame.rs` vs `gpu.rs`):
 *
 * - **Pure, headlessly testable:** `renderer.ts` (`LitRenderer` — the
 *   `Renderer<S, TemplateResult>` specialization) and `host.ts`
 *   (`LitHost` — glue with an injected commit sink and event→command
 *   mapper). A `TemplateResult` is a plain value; tests assert on its
 *   `strings`/`values` with no live DOM.
 * - **DOM-touching seam:** `dom.ts` (`domCommit` — the sink that calls
 *   Lit's `render` into a container). Only browser code imports it.
 */
export type { LitRenderer } from "./renderer";
export type {
  CommitSink,
  EventToCommand,
  LitHostOptions,
} from "./host";
export { LitHost } from "./host";
export { domCommit } from "./dom";
