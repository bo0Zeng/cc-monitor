//! 帧面 `session-fork` 的宿主 —— 与 `read_face` / `footprint` 同形的一层壳：
//! 找家目录（与帧面其余几条同一个出处 `observe::history_query::agent_home`）、交给本体。
//!
//! # 为什么要这一层壳（而不是 `control/fork_write.rs` 自己找家）
//!
//! 本体住 `control/`，而 `control → observe` 是反向边（`layering_guard` 零容忍）；本体直接问适配层要配置根
//! 又会让「加一家 agent 要回来改的地方」多一处（`agent_locality_guard` 那张只许降的表）。CLI 那一面的家目录
//! 由 `main.rs` 解析好传进去；帧面没有那一跳 ⇒ 由这层顶层壳补上，与 `read_face` 拿家目录是同一句。
//!
//! **零写盘**：写的是本体（`control/fork_write.rs`，`readonly_guard` 白名单层那一处 `O_EXCL` 新建），本文件只转交。

/// 帧面入口：`{sid, uuid} → {sessionId, jsonlPath}`（形状与失败码住本体那一份）。
pub(crate) fn answer(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (&'static str, String)> {
    crate::control::fork_write::answer_wire_at(&crate::observe::history_query::agent_home(), args)
}
