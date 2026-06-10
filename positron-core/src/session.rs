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
//! The client passes `last_seen` revisions; the substrate MAY skip a
//! snapshot whose revision the client already holds. The substrate
//! never replays history: a reconnect can therefore never flood
//! (no transcript replay) and never gap (the snapshot covers the
//! outage; live covers the rest). Renderers reconcile by revision
//! diff and re-render purely from the newest state.
//!
//! This contract is what makes reconnect tolerance *structural* for
//! consumers: a widget with no local source-of-truth cache cannot be
//! corrupted by a dropped transport, because the next subscribe
//! rebuilds its entire world from one snapshot.

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
    /// Register an AI observer subscription (perception budget
    /// enforced substrate-side).
    Observe(ObserverSpec),
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
}
