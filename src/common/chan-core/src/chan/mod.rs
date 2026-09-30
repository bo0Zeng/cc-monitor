//! 通道的五份（成员三份原地住 `src/comms/inward/chan/`；非成员两份住本目录）。逐份的分工住壳里 `chan/mod.rs` 那张表。

#[path = "../../../../comms/inward/chan/client.rs"]
pub mod client;
pub mod dial;
pub mod handoff;
#[path = "../../../../comms/inward/chan/router.rs"]
pub mod router;
#[path = "../../../../comms/inward/chan/wire.rs"]
pub mod wire;
