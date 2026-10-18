//! The client's committed V5 byte fixture is produced by `visionclaw-wire`.
//!
//! `client/src/__tests__/fixtures/v5_frame.bin` is what the desktop client's
//! `protocol.test.ts` decodes field by field (BASELINE I2, I4). This test pins
//! it to the frame helpers of the published wire crate, so the client never
//! needs a copy of VisionClaw's TypeScript decoder: if the wire crate and the
//! fixture ever disagree, this fails.
//!
//! Regenerate after a deliberate wire change (which needs an ADR) with
//! `CODESCAPE_REGEN_FIXTURES=1 cargo test -p codescape-server --test wire_fixture`,
//! then update the expectations in `protocol.test.ts` to match.

use std::path::PathBuf;

use codescape_model::wire::AGENT_FLAG;
use visionclaw_wire::{v5_frame, NodeRecord, NODE_ID_MASK, NODE_RECORD_BYTES, V5_SEQ_BYTES};

/// Above 2^32, so the high bytes of the u64 matter, and below 2^53, so the
/// client's `Number` conversion is exact.
const SEQUENCE: u64 = 0x0001_0203_0405_0607;

/// Four records, every field distinct within a record so an offset swap fails;
/// every float is exactly representable in f32.
fn records() -> Vec<NodeRecord> {
    vec![
        // A code node: no flag bits.
        NodeRecord {
            wire_id: 4660,
            position: [1.5, -2.25, 3.0],
            velocity: [0.5, -0.125, 0.0625],
            sssp_distance: 4.0,
            sssp_parent: 77,
            cluster_id: 3,
            anomaly: 0.75,
            community_id: 11,
            centrality: 0.375,
        },
        // An actor: AGENT_FLAG set, unreached by SSSP, no parent.
        NodeRecord {
            wire_id: 9 | AGENT_FLAG,
            position: [-8.0, 16.5, 0.25],
            velocity: [1.0, 2.0, -4.0],
            sssp_distance: f32::INFINITY,
            sssp_parent: -1,
            cluster_id: 7,
            anomaly: 1.0,
            community_id: 2,
            centrality: 0.0625,
        },
        // A code node at the top of the 26-bit id range.
        NodeRecord {
            wire_id: NODE_ID_MASK,
            position: [0.5, 0.0, -1.0],
            velocity: [0.0, 0.25, -0.5],
            sssp_distance: 1.0,
            sssp_parent: 4660,
            cluster_id: 5,
            anomaly: 0.125,
            community_id: 13,
            centrality: 0.875,
        },
        // A code node the client's index does not know.
        NodeRecord {
            wire_id: 123_456,
            position: [10.0, 20.0, 30.0],
            velocity: [-1.0, -2.0, -3.0],
            sssp_distance: 2.0,
            sssp_parent: 9,
            cluster_id: 6,
            anomaly: 0.5,
            community_id: 17,
            centrality: 0.25,
        },
    ]
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../client/src/__tests__/fixtures/v5_frame.bin")
}

#[test]
fn client_v5_fixture_matches_visionclaw_wire() {
    let records = records();
    let frame = v5_frame(SEQUENCE, &records);
    assert_eq!(
        frame.len(),
        1 + V5_SEQ_BYTES + records.len() * NODE_RECORD_BYTES
    );

    let path = fixture_path();
    if std::env::var_os("CODESCAPE_REGEN_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &frame).unwrap();
        eprintln!("wrote {} ({} bytes)", path.display(), frame.len());
    }
    let committed = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}; regenerate with CODESCAPE_REGEN_FIXTURES=1",
            path.display()
        )
    });
    assert_eq!(
        committed, frame,
        "client/src/__tests__/fixtures/v5_frame.bin differs from visionclaw-wire's v5_frame"
    );
}
