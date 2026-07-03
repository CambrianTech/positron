/**
 * Smoke tests for the TS contract interfaces — the mirror of the
 * `#[cfg(test)] mod tests` in `positron-core/src/lib.rs`. Same trivial
 * `Counter` fixture, same three assertions, so the TS side of the
 * contract is proven usable the way the Rust side is: a `ViewState`
 * compiles and reports a stable `kind`/`revision`, and a `Renderer`
 * produces deterministic output from state alone.
 *
 * These run under `node --test` via `tsx`; typechecking (`tsc --noEmit`)
 * is the primary gate, this is the it-actually-runs backstop.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

import type { Observer, Renderer, ViewState } from "./contract";

/** The same tiny fixture the Rust smoke tests use. */
class Counter implements ViewState {
  readonly kind = "counter";
  constructor(
    readonly value: number,
    readonly revision: number,
  ) {}
}

/** A "renderer for tests" — renders to a string, like the Rust one. */
class StringRenderer implements Renderer<Counter, string> {
  render(state: Counter): string {
    return `counter @ rev ${state.revision} = ${state.value}`;
  }
}

// what this catches: a ViewState impl compiles and reports a stable kind.
test("view state kind is stable", () => {
  const c = new Counter(0, 0);
  assert.equal(c.kind, "counter");
});

// what this catches: the optional revision marker round-trips as a
// number (the TS analogue of Rust's Some(u64)).
test("view state revision round trips", () => {
  const c = new Counter(42, 7);
  assert.equal(c.revision, 7);
});

// what this catches: a Renderer produces output from state alone and is
// deterministic — same input, same output. If a renderer starts carrying
// hidden mutable history, this is where it shows.
test("renderer produces deterministic output", () => {
  const c = new Counter(13, 1);
  const r = new StringRenderer();
  assert.equal(r.render(c), "counter @ rev 1 = 13");
  assert.equal(r.render(c), r.render(c));
});

// what this catches: the Observer contract is implementable and its
// budget/id surface reads as plain members — the perception primitive
// that later carries an AI persona, exercised here so the interface
// can't silently rot before positron-lit's LitObserver consumes it.
test("observer exposes id and budget and perceives state", () => {
  const seen: number[] = [];
  const obs: Observer<Counter> = {
    observerId: "maya",
    budgetHz: 4,
    onChange: (state) => seen.push(state.value),
  };
  assert.equal(obs.observerId, "maya");
  assert.equal(obs.budgetHz, 4);
  obs.onChange(new Counter(99, 2));
  assert.deepEqual(seen, [99]);
});
