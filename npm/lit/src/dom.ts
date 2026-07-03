/**
 * The one DOM-touching seam in `@positron/lit` — the analogue of the
 * Rust wgpu outlier's `gpu.rs` (the file that talks to the device),
 * kept separate from the pure `renderer.ts`/`host.ts` projection so the
 * rest of the package stays headlessly testable.
 *
 * `domCommit` builds a {@link CommitSink} that commits a rendered
 * `TemplateResult` into a live DOM container via Lit's `render`. This is
 * the sink you pass to a {@link "./host".LitHost} in a browser; tests
 * pass a capturing sink instead and never import this file.
 *
 * Importing this module is safe in Node (Lit's `render` reaches for
 * `document` only when CALLED, not on import), but calling the returned
 * sink requires a real DOM — which is exactly why it lives behind the
 * seam and out of the headless test path.
 */
import { render } from "lit";
import type { TemplateResult } from "lit";

import type { CommitSink } from "./host";

/**
 * A DOM-backed {@link CommitSink}: each committed template is rendered
 * into `container`. Lit tracks the root part on the container itself, so
 * repeat commits diff against the previous render — no manual part
 * bookkeeping.
 *
 * @param container the live DOM node the widget owns — an `HTMLElement`,
 *   shadow root, or `DocumentFragment`.
 */
export function domCommit(
  container: HTMLElement | DocumentFragment,
): CommitSink {
  return (result: TemplateResult): void => {
    render(result, container);
  };
}
