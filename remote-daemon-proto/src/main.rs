//! Phase-0 SSH-remote daemon prototype (Linux-only at runtime).
//!
//! Step 1 only scaffolds the wire protocol in [`wire`]. The real tokio runtime
//! and the inotify watcher land in Step 2.

// Step 1 only exercises the wire types from tests; the binary itself does not
// yet emit frames. Step 2 wires `to_line`/`Frame`/`SeqCounter::next` into the
// real runtime, at which point this allow can go away.
#![allow(dead_code)]

mod wire;

fn main() {
    // Touch a `wire` item so the binary keeps the module live until Step 2
    // wires up the real runtime. Costs nothing and avoids dead-code warnings.
    let _ = wire::SeqCounter::new();
}
