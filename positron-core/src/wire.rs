//! The serializable boundary — what actually crosses a transport
//! between a substrate and its hosts/observers.
//!
//! The four traits in [`crate`] are the in-process contract; this
//! module is the *wire* contract. Every type here derives [`ts_rs::TS`]
//! and exports TypeScript definitions into the `@positron/core` npm
//! package, so the TS side is GENERATED from these structs — one
//! source of truth, no hand-maintained mirror types at the boundary.
//!
//! Positron frames; consumers fill. `StateEnvelope::payload` and
//! `CommandEnvelope::params` are opaque JSON here because concrete
//! widget vocabularies live in consumer code (continuum defines
//! `ChatViewState`, not positron). Consumers generate their own
//! payload types with the same ts-rs flow and slot them in.
//!
//! Wire stability: from v1.0 these JSON shapes are a cross-version
//! contract (old client / new substrate must interoperate). Until
//! then, breaking changes are allowed but must regenerate the npm
//! types in the same commit — the export test enforces drift can't
//! hide.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Update-cadence classification for a state change (see `DESIGN.md`
/// § "The 4 state layers"). Renderers and observers subscribe at the
/// layer their target can sustain; the substrate enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StateLayer {
    /// ~60 Hz — animations, hover, typing-in-progress. DOM/AR
    /// renderers only; AI observers never subscribe here.
    Ephemeral,
    /// 1–10 Hz — user-perceivable changes (message arrived, room
    /// switched). The default layer when unspecified.
    #[default]
    Session,
    /// < 1 Hz — long-lived state (profile edits, theme changes).
    Persistent,
    /// On-demand — AI-tier meaning extraction ("the conversation
    /// shifted topic"). Pull-oriented; produced by cognition, not UI.
    Semantic,
}

/// State-down: one `ViewState` snapshot crossing the transport.
///
/// The substrate-side analogue of calling `Host::on_state` /
/// `Observer::on_change` in-process. `kind` + `revision` mirror the
/// [`crate::ViewState`] trait methods; `payload` is the consumer's
/// concrete state, serialized.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StateEnvelope {
    /// Widget kind — routes to the correct renderer/observer
    /// subscription (`"chat"`, `"continuum/user-list"`, …).
    pub kind: String,
    /// Revision marker for cheap change detection. `None` = treat
    /// every update as new (mirrors `ViewState::revision`).
    ///
    /// TS type is `number`, not ts-rs's default `bigint` for `u64`:
    /// the JSON wire carries a number, and `bigint` breaks both
    /// directions (`JSON.parse` yields `number`; `JSON.stringify`
    /// throws on `bigint`). Revisions are monotonic counters —
    /// `Number.MAX_SAFE_INTEGER` (2^53−1) of them is not a real
    /// constraint; substrates that somehow exceed it must reset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub revision: Option<u64>,
    /// Cadence layer of this update. Hosts/observers may be
    /// subscribed to a subset of layers.
    #[serde(default)]
    pub layer: StateLayer,
    /// The consumer-typed state, serialized. Positron frames it;
    /// the consumer's own generated types describe its interior.
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,
}

/// Who initiated a command — typed provenance on every action.
///
/// The citizenship principle at the UI layer: an AI acting through a
/// widget is *first-class but never anonymous*. Substrate audit
/// trails, rate limits, and trust policies key off this without
/// string-sniffing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "source")]
#[ts(export)]
pub enum CommandSource {
    /// A human at the surface (mouse, keys, touch, voice).
    Human,
    /// An AI observer acting on what it perceived. Carries the same
    /// `observer_id` used for cognition-budget accounting, so
    /// perception and action share one identity.
    Observer {
        /// The acting observer's substrate-routed identifier.
        observer_id: String,
    },
}

/// Event-up: a typed command crossing the transport toward the
/// substrate.
///
/// The wire analogue of `Host::on_event` returning `Some(command)`.
/// Positron does not define command vocabularies — `command` names
/// and `params` shapes belong to the consumer (continuum's 360+
/// commands stay continuum's). Positron contributes the frame:
/// routing, correlation, and provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CommandEnvelope {
    /// Widget kind this command originates from.
    pub kind: String,
    /// Consumer-defined command name (e.g. `"chat/send"`).
    pub command: String,
    /// Consumer-typed parameters, serialized.
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    /// Correlates a command with its eventual result/effect for
    /// request-response flows and audit stitching.
    #[ts(type = "string")]
    pub correlation_id: Uuid,
    /// Who acted. Never inferred, always declared.
    pub source: CommandSource,
}

/// An observer's subscription request — the wire form of registering
/// an [`crate::Observer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ObserverSpec {
    /// Substrate-routed identity (e.g. a persona UUID as string).
    pub observer_id: String,
    /// Requested perception frequency in Hz. `0` = pull-only. The
    /// substrate may quantize down under load; it never raises.
    pub budget_hz: u32,
    /// Widget kinds to perceive. Empty = none (explicit opt-in per
    /// kind; perception is budgeted, not ambient).
    pub kinds: Vec<String>,
    /// Cadence layers to perceive — same explicit-opt-in semantics as
    /// `kinds` (empty = none). A typical AI observer subscribes to
    /// `[Session, Semantic]`; `Ephemeral` is for renderers, and an
    /// observer asking for it should expect aggressive quantization.
    pub layers: Vec<StateLayer>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shapes round-trip losslessly — the minimum bar for a
    /// boundary type.
    #[test]
    fn envelopes_round_trip() {
        let state = StateEnvelope {
            kind: "chat".into(),
            revision: Some(41),
            layer: StateLayer::Session,
            payload: serde_json::json!({"messages": [], "room": "general"}),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<StateEnvelope>(&json).unwrap(), state);

        let cmd = CommandEnvelope {
            kind: "chat".into(),
            command: "chat/send".into(),
            params: serde_json::json!({"text": "hello"}),
            correlation_id: Uuid::from_u128(0xc0ffee),
            source: CommandSource::Observer {
                observer_id: "maya".into(),
            },
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert_eq!(serde_json::from_str::<CommandEnvelope>(&json).unwrap(), cmd);
    }

    /// Provenance is tagged, not positional — a TS consumer reads
    /// `source` discriminant directly, and `human` carries no id to
    /// forge.
    #[test]
    fn command_source_wire_shape() {
        assert_eq!(
            serde_json::to_string(&CommandSource::Human).unwrap(),
            r#"{"source":"human"}"#
        );
        assert_eq!(
            serde_json::to_string(&CommandSource::Observer {
                observer_id: "maya".into()
            })
            .unwrap(),
            r#"{"source":"observer","observer_id":"maya"}"#
        );
    }

    /// Layer defaults to Session — the human-perceivable tier — so an
    /// unannotated update can never accidentally claim the 60 Hz lane.
    #[test]
    fn missing_layer_defaults_to_session() {
        let bare = r#"{"kind":"chat","payload":{}}"#;
        let env: StateEnvelope = serde_json::from_str(bare).unwrap();
        assert_eq!(env.layer, StateLayer::Session);
        assert_eq!(env.revision, None);
    }
}
