//! 历史会话的 monitor 这一侧：一件事都不做（清单 · 注解 · 删除都在那台后端）。
//!
//! ## 清单与注解不在这里
//!
//! 项目 / 会话清单与注解（星标 / 重命名 / 隐藏 / 上次账号）搬进了**本机常驻后端**（后端 `history_list.rs` ·
//! `history_annotations.rs`），界面经通道问（`src/frontend/ui/history-reads.ts`）。这里原先的「两级懒加载」
//! 与注解读写一起删了。
//! 注解那份文件住这台的家（`~/.cc-monitor/history-metadata.json`），那台后端按家自己推 —— star / 重命名 / 隐藏仍然**不改 jsonl**
//! （Claude Code 的数据保持零侵入）。
//!
//! ## 物理删除
//!
//! 用户明确选了「物理删除」。前端二次确认后经通道直说那台机器的后端 `files-delete-session`（monitor 那条转交删了）；
//! 删完那条注解由界面交会话所在那台的后端 `history-forget` 连带删（注解跟着会话住在那台）。Claude Code 自己也不再能 resume 这个会话。
