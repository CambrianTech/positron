//! The transport-session protocol — the typed frames a host exchanges
//! with a substrate over any transport (WebSocket, UDS, airc).
//!
//! [`crate::wire`] defines the envelopes; this module defines the
//! *conversation*: a self-describing union per direction, so a
//! transport carries exactly one client frame type and one server
//! frame type and nothing is ever stringly-routed.
//!
//! ## Snapshot-then-live — the resync contract
//!
//! On [`ClientMessage::Subscribe`] — first connect and every
//! reconnect alike — the substrate MUST:
//!
//! 1. Immediately emit the **current** [`crate::wire::StateEnvelope`]
//!    for each subscribed kind (revision-tagged), and
//! 2. then stream live updates from that moment forward.
//!
//! The substrate never replays history: a reconnect can therefore
//! never flood (no transcript replay) and never gap (the snapshot
//! covers the outage; live covers the rest). Renderers reconcile by
//! revision diff and re-render purely from the newest state.
//!
//! **Skip rule (exact equality).** The substrate MAY skip a kind's
//! snapshot **only when the client's `last_seen` revision EXACTLY
//! equals the substrate's current revision** for that kind. Never a
//! `>=` comparison: a substrate restart may reset its revision
//! counter, and under `>=` a client holding `last_seen: 500` against
//! a freshly-restarted substrate at revision 3 would keep stale state
//! forever. Exact equality makes counter resets safe by construction
//! — any mismatch, in either direction, sends the snapshot. (The skip
//! is purely an optimization; when in doubt, send.)
//!
//! **Subscribe is declarative (replace, not merge).** A `Subscribe`
//! frame declares the connection's complete interest set; it REPLACES
//! any previous subscription on the connection. Clients always send
//! their whole world — no incremental add/remove bookkeeping exists
//! to drift. Snapshot-then-live applies to every kind in the new set
//! (subject to the skip rule); kinds absent from the new set stop
//! flowing. Identical re-subscribes are therefore idempotent as a
//! special case of replacement.
//!
//! **Observers resync identically.** [`ClientMessage::Observe`] is
//! declarative per `observer_id` (re-observe REPLACES that observer's
//! registration) and triggers the same snapshot-then-live with the
//! same exact-equality skip on its `last_seen`. A reconnecting AI
//! observer rebuilds its perceived world exactly like a renderer
//! does — there is one resync contract, not a human one and an AI
//! one.
//!
//! **Ordering.** Within one kind, the substrate MUST emit `State`
//! frames in non-decreasing revision order over an ordered transport.
//! Consumers MUST drop an envelope whose revision is lower than one
//! already rendered for that kind (when both carry revisions) — this
//! makes accidental reordering harmless rather than corrupting.
//! **The drop watermark is scoped to the current subscription**: it
//! RESETS on every `Subscribe`/`Observe` the consumer sends, and
//! knowledge from before a reconnect travels ONLY in `last_seen`.
//! Without this scoping, a client that rendered revision 500, then
//! reconnected to a counter-reset substrate, would drop the
//! snapshot@3 it just asked for (3 < 500) and every frame after it —
//! the stale-forever gap reappearing one layer up. Watermark resets;
//! `last_seen` remembers; the two never share a counter.
//!
//! This contract is what makes reconnect tolerance *structural* for
//! consumers: a widget with no local source-of-truth cache cannot be
//! corrupted by a dropped transport, because the next subscribe
//! rebuilds its entire world from one snapshot.
//!
//! ## Deliberate v0 omissions
//!
//! There is no success-ack frame in [`ServerMessage`]: a successful
//! command's acknowledgement IS the state change it causes (the
//! unidirectional model). Failures are different — a failed command
//! with no reporting channel is a silent failure, so
//! [`ServerMessage::CommandFailed`] exists (v0.1.1). A full
//! `Result` frame for request-shaped success feedback remains a
//! v0.x candidate; adding `ServerMessage` variants is additive and
//! non-breaking for tagged unions.
//!
//! The exact-equality skip has one residual ABA case: a client whose
//! `last_seen` revision N came from a pre-restart substrate may meet
//! a restarted substrate whose counter has independently reached the
//! same N with different content — the skip then wrongly omits one
//! snapshot. The staleness is transient (any next mutation of that
//! kind bumps the revision and streams down), and closing it fully
//! requires an epoch/generation id on revisions — a v0.x candidate
//! if real substrates restart often enough to care. Substrates that
//! persist their revision counters never enter this case.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::wire::{CommandEnvelope, ObserverSpec, StateEnvelope, StateLayer};

/// A `(kind, revision)` pair the client already holds — sent on
/// (re)subscribe so the substrate may skip redundant snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KindRevision {
    /// Widget kind (matches [`StateEnvelope::kind`]).
    pub kind: String,
    /// Last revision of that kind the client rendered.
    ///
    /// TS `number`, same rationale as [`StateEnvelope::revision`].
    #[ts(type = "number")]
    pub revision: u64,
}

/// Client → substrate frames. One union; the transport is fully
/// self-describing and there is no out-of-band routing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "type")]
#[ts(export)]
pub enum ClientMessage {
    /// Declare (or re-declare, after a reconnect) what this client
    /// renders. Triggers snapshot-then-live per the module contract.
    /// Always idempotent: subscribing twice is a resync, never an
    /// error and never a duplicate live stream.
    Subscribe {
        /// Widget kinds to receive. Empty = none (explicit opt-in,
        /// consistent with [`ObserverSpec`]).
        kinds: Vec<String>,
        /// Cadence layers to receive. Empty = none.
        layers: Vec<StateLayer>,
        /// Revisions already held; substrate MAY skip matching
        /// snapshots. Empty = client holds nothing, send everything.
        #[serde(default)]
        last_seen: Vec<KindRevision>,
    },
    /// A user (or observer) action — see [`CommandEnvelope`] for the
    /// provenance contract.
    Command(CommandEnvelope),
    /// Register (or re-register, after a reconnect) an AI observer.
    /// Declarative per `spec.observer_id` — replaces that observer's
    /// previous registration — and triggers snapshot-then-live with
    /// the exact-equality skip, identically to `Subscribe`. One
    /// resync contract for humans and AIs. Perception budget enforced
    /// substrate-side.
    Observe {
        /// The observer's identity, budget, and perception scope.
        spec: ObserverSpec,
        /// Revisions this observer already perceived; same skip rule
        /// as `Subscribe::last_seen`.
        #[serde(default)]
        last_seen: Vec<KindRevision>,
    },
}

/// Substrate → client frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "type")]
#[ts(export)]
pub enum ServerMessage {
    /// A state update — either a snapshot (immediately after
    /// `Subscribe`) or a live change. Deliberately the SAME frame for
    /// both: consumers reconcile by `kind` + `revision`, never by
    /// "which phase am I in" bookkeeping. If a client must
    /// distinguish, the revision diff already tells it.
    State(StateEnvelope),
    /// A command could not be executed. Failures are LOUD — a
    /// rejected [`CommandEnvelope`] must never vanish silently.
    /// Success deliberately has no ack frame: a successful command's
    /// acknowledgement IS the state change it causes, streaming down
    /// as `State` (the unidirectional model). Consumers correlate via
    /// the `correlation_id` they sent.
    CommandFailed {
        /// Echo of [`CommandEnvelope::correlation_id`].
        #[ts(type = "string")]
        correlation_id: uuid::Uuid,
        /// Human-readable failure reason (consumer-displayable).
        error: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::CommandSource;
    use uuid::Uuid;

    /// The session frames round-trip and the tag layout is pinned —
    /// these JSON shapes are what ContinuumHost (Rust) and
    /// positron-lit (TS) both build against.
    #[test]
    fn client_frames_round_trip_with_pinned_tags() {
        let sub = ClientMessage::Subscribe {
            kinds: vec!["chat".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![KindRevision {
                kind: "chat".into(),
                revision: 41,
            }],
        };
        let json = serde_json::to_string(&sub).unwrap();
        assert!(json.starts_with(r#"{"type":"subscribe""#), "{json}");
        assert_eq!(serde_json::from_str::<ClientMessage>(&json).unwrap(), sub);

        let cmd = ClientMessage::Command(CommandEnvelope {
            kind: "chat".into(),
            command: "chat/send".into(),
            params: serde_json::json!({"text": "hi"}),
            correlation_id: Uuid::from_u128(7),
            source: CommandSource::Human,
        });
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.starts_with(r#"{"type":"command""#), "{json}");
        assert_eq!(serde_json::from_str::<ClientMessage>(&json).unwrap(), cmd);
    }

    /// Failures are loud and the tag is pinned; success has no ack
    /// frame by design (the state change is the ack).
    #[test]
    fn command_failed_round_trips_with_pinned_tag() {
        let fail = ServerMessage::CommandFailed {
            correlation_id: Uuid::from_u128(7),
            error: "chat/send: room not found".into(),
        };
        let json = serde_json::to_string(&fail).unwrap();
        assert!(json.starts_with(r#"{"type":"command_failed""#), "{json}");
        assert_eq!(serde_json::from_str::<ServerMessage>(&json).unwrap(), fail);
    }

    #[test]
    fn server_state_frame_round_trips() {
        let state = ServerMessage::State(StateEnvelope {
            kind: "chat".into(),
            revision: Some(42),
            layer: StateLayer::Session,
            payload: serde_json::json!({"messages": ["hi"]}),
        });
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.starts_with(r#"{"type":"state""#), "{json}");
        assert_eq!(serde_json::from_str::<ServerMessage>(&json).unwrap(), state);
    }

    /// `last_seen` is optional on the wire — a minimal subscribe from
    /// a fresh client (or an older build) decodes with "send
    /// everything" semantics, never an error.
    #[test]
    fn bare_subscribe_decodes_with_empty_last_seen() {
        let bare = r#"{"type":"subscribe","kinds":["chat"],"layers":["session"]}"#;
        let msg: ClientMessage = serde_json::from_str(bare).unwrap();
        match msg {
            ClientMessage::Subscribe { last_seen, .. } => assert!(last_seen.is_empty()),
            other => panic!("expected Subscribe, got {other:?}"),
        }
    }

    /// The `observe` frame's tag layout is pinned (it was the one
    /// variant without committed-test coverage), and a bare observe
    /// without `last_seen` decodes as perceive-from-scratch — the
    /// observer resync contract mirrors the subscriber one exactly.
    #[test]
    fn observe_round_trips_with_pinned_tag_and_default_last_seen() {
        let obs = ClientMessage::Observe {
            spec: ObserverSpec {
                observer_id: "maya".into(),
                budget_hz: 4,
                kinds: vec!["chat".into()],
                layers: vec![StateLayer::Session, StateLayer::Semantic],
            },
            last_seen: vec![KindRevision {
                kind: "chat".into(),
                revision: 12,
            }],
        };
        let json = serde_json::to_string(&obs).unwrap();
        assert!(json.starts_with(r#"{"type":"observe""#), "{json}");
        assert_eq!(serde_json::from_str::<ClientMessage>(&json).unwrap(), obs);

        let bare = r#"{"type":"observe","spec":{"observer_id":"maya","budget_hz":4,"kinds":["chat"],"layers":["session"]}}"#;
        match serde_json::from_str::<ClientMessage>(bare).unwrap() {
            ClientMessage::Observe { last_seen, .. } => assert!(last_seen.is_empty()),
            other => panic!("expected Observe, got {other:?}"),
        }
    }
}
