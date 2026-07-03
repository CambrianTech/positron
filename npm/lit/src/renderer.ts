/**
 * The Lit DOM specialization of the positron {@link Renderer} contract.
 *
 * A {@link LitRenderer} is exactly a `Renderer<S, TemplateResult>` — the
 * `Output` associated type from `@positron/core` pinned to Lit's
 * `TemplateResult`. This is the "web ≠ DOM as a peer paradigm" surface:
 * the SAME `ViewState` a wgpu backend renders to a GPU `Frame`, a
 * terminal renders to cells, this one renders to a DOM template.
 *
 * It carries no new methods — a Lit renderer is just a positron renderer
 * whose output happens to be a `TemplateResult`. Keeping it a type alias
 * (not a fresh interface) is the compression rule: the render contract
 * lives once, in `@positron/core`; this file only names the DOM output.
 *
 * Like the Rust wgpu outlier, the projection is PURE and headlessly
 * testable: `render(state)` returns a `TemplateResult` value whose
 * `strings`/`values` can be asserted without a live DOM (see
 * `renderer.test.ts`), the way the wgpu outlier asserts on `Frame`
 * quads. The DOM-touching commit is a separate seam (`./dom`).
 */
import type { Renderer, ViewState } from "@positron/core";
import type { TemplateResult } from "lit";

/**
 * A positron renderer that projects a {@link ViewState} to a Lit
 * `TemplateResult`. Compose one per widget kind; the {@link LitHost}
 * drives it and commits the result into a real DOM node.
 */
export type LitRenderer<S extends ViewState> = Renderer<S, TemplateResult>;
