/**
 * @positron/core — the positron wire contract for TypeScript consumers.
 *
 * Every type in `./generated/` is GENERATED from the Rust structs in
 * `positron-core/src/wire.rs` by ts-rs (`cargo test -p positron-core`
 * regenerates). Do not edit generated files by hand; change the Rust
 * struct — it is the single source of truth — and re-run the tests.
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
