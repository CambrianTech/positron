/**
 * @positron/core — the positron contract for TypeScript consumers.
 *
 * Two halves:
 *
 * 1. The **wire types** in `./generated/` — GENERATED from the Rust
 *    structs in `positron-core/src/{wire,session}.rs` by ts-rs
 *    (`cargo test -p positron-core` regenerates). Do not edit generated
 *    files by hand; change the Rust struct — the single source of truth
 *    — and re-run the tests. These are the DATA that crosses transports.
 *
 * 2. The **contract interfaces** in `./contract.ts` — hand-authored
 *    (a trait has no serialized form for ts-rs to generate), the TS twin
 *    of the four Rust traits in `positron-core/src/lib.rs`. These are the
 *    BEHAVIOUR renderers and observers implement.
 *
 * Consumers (continuum, etc.) define their own payload types for
 * `StateEnvelope.payload` / `CommandEnvelope.params` with the same
 * ts-rs flow on their side; positron frames, consumers fill.
 */
export type { StateLayer } from "./generated/StateLayer";
export type { StateEnvelope } from "./generated/StateEnvelope";
export type { CommandSource } from "./generated/CommandSource";
export type { CommandEnvelope } from "./generated/CommandEnvelope";
export type { ObserverSpec } from "./generated/ObserverSpec";
export type { KindRevision } from "./generated/KindRevision";
export type { ClientMessage } from "./generated/ClientMessage";
export type { ServerMessage } from "./generated/ServerMessage";

// The four positron primitives as TS interfaces — hand-authored (traits
// have no wire form for ts-rs to generate); the TS twin of the Rust
// traits in positron-core/src/lib.rs. See ./contract.ts for the why.
export type { ViewState, Renderer, Host, Observer } from "./contract";
