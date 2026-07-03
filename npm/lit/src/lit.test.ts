/**
 * Smoke tests for `@positron/lit` — the DOM outlier of the positron
 * `Renderer`/`Host` contract, mirroring `@positron/core`'s single
 * `contract.test.ts` and the Rust wgpu outlier's headless `Frame`
 * assertions.
 *
 * The projection is proven WITHOUT a live DOM: a `LitRenderer` returns a
 * Lit `TemplateResult`, a plain value whose `strings` (static parts) and
 * `values` (dynamic interpolations) we assert on directly — exactly how
 * the wgpu outlier asserts on `Frame` quads instead of pixels. `LitHost`
 * is driven with a capturing commit sink, so the whole file runs under
 * `node --test` with no jsdom. The DOM-touching `domCommit` seam is
 * deliberately not exercised here (it needs a real container).
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import type { ViewState } from "@positron/core";
import { html, type TemplateResult } from "lit";

import type { LitRenderer } from "./renderer";
import { LitHost } from "./host";

/** The same tiny fixture the core smoke tests use. */
class Counter implements ViewState {
  readonly kind = "counter";
  constructor(
    readonly value: number,
    readonly revision: number,
  ) {}
}

/** Commands this widget can emit up to the substrate. */
interface IncrementCommand {
  readonly kind: "increment";
}

/** Surface events the host maps to commands. */
type CounterEvent = { readonly type: "click" } | { readonly type: "hover" };

/**
 * The DOM renderer: state → a `<div>` with a sign class, the value as an
 * attribute, and one `<span class="pip">` per unit of magnitude. Written
 * so the interpolations land in the template's `values` (sign, value,
 * pips) rather than being baked into the static `strings`.
 */
class CounterLitRenderer implements LitRenderer<Counter> {
  render(state: Counter): TemplateResult {
    const sign = state.value >= 0 ? "positive" : "negative";
    const pips = Array.from(
      { length: Math.abs(state.value) },
      (_unused, i) => html`<span class="pip" data-i=${i}></span>`,
    );
    return html`<div class="counter ${sign}" data-value=${state.value}>${pips}</div>`;
  }
}

// what this catches: the DOM projection is a pure function of state —
// sign, value, and per-unit pips land in the template's dynamic `values`
// (not hardcoded into the static markup), and the surrounding structure
// is in `strings`. If a renderer starts baking state into the static
// parts (defeating Lit's diffing) this is where it shows.
test("renderer projects state into template values", () => {
  const r = new CounterLitRenderer();
  const result = r.render(new Counter(3, 1));

  // Dynamic interpolations, in source order: sign, value, pips array.
  assert.equal(result.values[0], "positive");
  assert.equal(result.values[1], 3);
  assert.equal((result.values[2] as readonly unknown[]).length, 3);

  // Static structure is stable and carries no state.
  const markup = result.strings.join("");
  assert.match(markup, /class="counter /);
  assert.match(markup, /data-value=/);
  assert.match(markup, /<\/div>/);
});

// what this catches: the sign branch flips and the pip count tracks
// |value| — proves the projection is deterministic across the value
// domain, not just the positive path.
test("renderer reflects sign and magnitude", () => {
  const r = new CounterLitRenderer();

  const negative = r.render(new Counter(-2, 4));
  assert.equal(negative.values[0], "negative");
  assert.equal(negative.values[1], -2);
  assert.equal((negative.values[2] as readonly unknown[]).length, 2);

  const zero = r.render(new Counter(0, 0));
  assert.equal(zero.values[0], "positive");
  assert.equal((zero.values[2] as readonly unknown[]).length, 0);
});

// what this catches: state flows DOWN through the host — onState renders
// and hands the template to the commit sink exactly once, and
// lastRendered exposes what was committed without re-running the
// renderer. The injected sink is why this needs no DOM.
test("host commits rendered state through the sink", () => {
  const committed: TemplateResult[] = [];
  const host = new LitHost<Counter, IncrementCommand, CounterEvent>({
    renderer: new CounterLitRenderer(),
    commit: (result) => committed.push(result),
    toCommand: (event) =>
      event.type === "click" ? { kind: "increment" } : undefined,
  });

  assert.equal(host.lastRendered, undefined);
  host.onState(new Counter(2, 1));
  assert.equal(committed.length, 1);
  assert.equal(host.lastRendered, committed[0]);
  assert.equal(committed[0]?.values[1], 2);
});

// what this catches: events flow UP as optional commands — the mapped
// event yields a command, the unmapped event yields undefined (the Rust
// Option::None path). If the None branch ever collapses to a bogus
// command this fails.
test("host maps events up to optional commands", () => {
  const host = new LitHost<Counter, IncrementCommand, CounterEvent>({
    renderer: new CounterLitRenderer(),
    commit: () => {},
    toCommand: (event) =>
      event.type === "click" ? { kind: "increment" } : undefined,
  });

  assert.equal(host.onEvent({ type: "click" })?.kind, "increment");
  assert.equal(host.onEvent({ type: "hover" }), undefined);
});
