//! # counter-cli — the positron "define once, project many" proof
//!
//! This example makes `docs/ARCHITECTURE.md` non-vapor. It demonstrates the
//! whole positron thesis in a single process with **zero transport and zero
//! substrate**:
//!
//! - **ONE app definition** — the [`Counter`] `ViewState` (what it shows) plus
//!   the one command name it may emit (`counter/reset`).
//! - **Two renderers** — [`LineRenderer`] and [`GaugeRenderer`], deliberately
//!   *maximally different* (single-line text vs multi-row block art, and
//!   crucially **different `Renderer::Output` types**: `String` vs
//!   `Vec<String>`). This is the outlier-validation discipline: if the
//!   `Renderer` trait fits both extremes without forcing, it fits the middle
//!   (a DOM tree, a ratatui frame) too.
//! - **One observer** — [`ThresholdObserver`], an AI persona that perceives the
//!   *same* `Counter` a human sees and, on crossing a threshold, **acts through
//!   the same command vocabulary** by emitting a [`CommandEnvelope`] tagged
//!   `CommandSource::Observer`. No separate "AI view," no bespoke integration —
//!   the fourth projection of the identical state.
//!
//! Nothing here touches `session.rs`, `wire.rs` transport, or Continuum. Those
//! arrive at O5 (`ContinuumHost`). This unit proves the contract holds in the
//! small before any wire is involved.
//!
//! Run it: `cargo run -p counter-cli`.

use std::sync::{Arc, Mutex};

use positron_core::wire::{CommandEnvelope, CommandSource};
use positron_core::{Observer, Renderer, ViewState};
use uuid::Uuid;

/// The ONE app definition: a counter's value and a revision marker.
///
/// Every surface below is a pure projection of this. It carries **semantic
/// content only** — no colors, no widths, no layout — exactly as
/// `docs/ARCHITECTURE.md` pins (open Q#4: layout is a renderer concern). A
/// terminal, a block-art gauge, and an AI observer all consume this same value.
#[derive(Debug, Clone)]
struct Counter {
    value: i64,
    revision: u64,
}

impl ViewState for Counter {
    fn kind(&self) -> &'static str {
        "counter"
    }

    fn revision(&self) -> Option<u64> {
        Some(self.revision)
    }
}

/// Renderer A — a single line of human-readable text. `Output = String`.
struct LineRenderer;

impl Renderer<Counter> for LineRenderer {
    type Output = String;

    fn render(&self, state: &Counter) -> String {
        format!("counter = {} (rev {})", state.value, state.revision)
    }
}

/// Renderer B — the maximally-different outlier. Block-art gauge across a fixed
/// width, emitted as **multiple rows** (`Output = Vec<String>`). Same `Counter`,
/// a wholly different surface shape and a different associated `Output` type —
/// which is the point: the trait must not privilege one surface's tree shape.
struct GaugeRenderer {
    width: i64,
}

impl Renderer<Counter> for GaugeRenderer {
    type Output = Vec<String>;

    fn render(&self, state: &Counter) -> Vec<String> {
        let filled = state.value.clamp(0, self.width);
        let empty = self.width - filled;
        let bar = format!(
            "[{}{}]",
            "█".repeat(filled as usize),
            "░".repeat(empty.max(0) as usize),
        );
        let caption = format!("{}/{}", state.value.max(0).min(self.width), self.width);
        vec![bar, caption]
    }
}

/// The one command name this app's surfaces may emit. Kept as a `const` so the
/// "command vocabulary" is a single source of truth (in a real substrate this
/// lives in Continuum, never in positron).
const RESET_COMMAND: &str = "counter/reset";

/// An AI persona projecting the *same* `Counter`. It does not render — it
/// perceives, at a Session-tier cognition budget — and when the value crosses
/// its threshold it acts through the identical command vocabulary a human's
/// host would use, tagging provenance as `Observer` so perception and action
/// share one identity.
struct ThresholdObserver {
    id: String,
    threshold: i64,
    /// Where acted-upon commands land. In a real substrate this is
    /// `Commands.execute`; here it is a shared sink so the example stays
    /// transport-free while still proving the perceive→act loop. `Mutex<Vec<_>>`
    /// (not `mpsc::Sender`, which is `!Sync`) satisfies the `Observer: Sync`
    /// bound.
    emitted: Arc<Mutex<Vec<CommandEnvelope>>>,
}

impl Observer<Counter> for ThresholdObserver {
    fn observer_id(&self) -> &str {
        &self.id
    }

    fn budget_hz(&self) -> u32 {
        // Session-tier: an AI observer's cognition can't sustain Ephemeral.
        4
    }

    fn on_change(&self, state: &Counter) {
        println!(
            "  observer[{}] perceived rev {}: value={}",
            self.id, state.revision, state.value
        );
        if state.value >= self.threshold {
            let cmd = CommandEnvelope {
                kind: "counter".to_string(),
                command: RESET_COMMAND.to_string(),
                params: serde_json::json!({ "reason": "threshold", "at": state.value }),
                correlation_id: Uuid::new_v4(),
                source: CommandSource::Observer {
                    observer_id: self.id.clone(),
                },
            };
            self.emitted
                .lock()
                .expect("emit sink mutex poisoned")
                .push(cmd);
        }
    }
}

fn main() {
    // ONE definition, projected three ways: two renderers + one observer.
    let line = LineRenderer;
    let gauge = GaugeRenderer { width: 10 };
    let emitted = Arc::new(Mutex::new(Vec::new()));
    let observer = ThresholdObserver {
        id: "persona-asha".to_string(),
        threshold: 8,
        emitted: Arc::clone(&emitted),
    };

    println!("positron define-once proof — one Counter ViewState, two renderers, one observer\n");

    // The "substrate": owns state, produces ViewState updates. Zero transport.
    let mut value = 0i64;
    for step in 0..6 {
        value += 2;
        let state = Counter {
            value,
            revision: step + 1,
        };

        println!("── rev {} ──────────────", state.revision().unwrap());
        // Human surfaces — same state, different projections, neither mutates it.
        println!("  line : {}", line.render(&state));
        for row in gauge.render(&state) {
            println!("  gauge: {row}");
        }
        // AI surface — same state, perceived not rendered.
        observer.on_change(&state);
        println!();
    }

    // Drain what the observer acted on — perceive→act through the SAME frame.
    let commands = emitted.lock().expect("emit sink mutex poisoned");
    println!(
        "observer emitted {} command(s) through the shared vocabulary:",
        commands.len()
    );
    for c in commands.iter() {
        println!(
            "  {} {} (source={:?}, corr={})",
            c.kind, c.command, c.source, c.correlation_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the two renderers are pure projections of the SAME
    // Counter — different Output types, neither mutating state. If someone
    // reintroduces widget-local mutable history (the anti-pattern positron
    // exists to kill), rendering the same state twice would diverge or the
    // borrow would fail to compile.
    #[test]
    fn both_renderers_project_the_same_state_without_mutation() {
        let state = Counter {
            value: 4,
            revision: 2,
        };
        let line = LineRenderer;
        let gauge = GaugeRenderer { width: 10 };

        // Same &state feeds both renderers (shared immutable borrow proves
        // neither takes &mut).
        let line_out: String = line.render(&state);
        let gauge_out: Vec<String> = gauge.render(&state);

        assert_eq!(line_out, "counter = 4 (rev 2)");
        assert_eq!(gauge_out, vec!["[████░░░░░░]".to_string(), "4/10".to_string()]);

        // Rendering again yields byte-identical output — no hidden state drift.
        assert_eq!(line.render(&state), line_out);
        assert_eq!(gauge.render(&state), gauge_out);
    }

    // what this catches: the observer perceives the same ViewState and, on
    // threshold, acts through the identical command vocabulary — with its
    // perception identity carried onto the action provenance
    // (CommandSource::Observer { observer_id } == observer_id()). This is the
    // load-bearing positron claim: the AI is the fourth projection, not a
    // bespoke path.
    #[test]
    fn observer_perceives_then_acts_with_carried_identity() {
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let observer = ThresholdObserver {
            id: "persona-asha".to_string(),
            threshold: 8,
            emitted: Arc::clone(&emitted),
        };

        // Below threshold: perceives, does not act.
        observer.on_change(&Counter {
            value: 6,
            revision: 1,
        });
        assert!(emitted.lock().unwrap().is_empty());

        // At/over threshold: acts once, through the shared command name.
        observer.on_change(&Counter {
            value: 8,
            revision: 2,
        });
        let cmds = emitted.lock().unwrap();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].command, RESET_COMMAND);
        match &cmds[0].source {
            CommandSource::Observer { observer_id } => {
                assert_eq!(observer_id, observer.observer_id());
            }
            other => panic!("expected Observer provenance, got {other:?}"),
        }
    }
}
