//! 历史会话的 monitor 这一侧：只剩注解那份文件的路径（交给本机常驻后端）。
//! 〔MOD · `05 §14.3` C 组〕读一整份会话（`stream_read_session_jsonl`〔散文墓碑〕与它的分页器 `SessionPager`〔散文墓碑〕）退役：
//! 那台后端出记录行（`history-page`），界面经通道直问（`src/record-reads.ts`）；那句「超过上限」随判定进了后端。
//! 〔MIG-3b〕删一份会话与 **F62 从某轮建分支** 不在这里了：界面经通道直说那台机器的后端（`src/session-writes.ts`）。
//!
//! ## 〔C4d · 第四波 4B〕清单与注解不在这里了
//!
//! 项目 / 会话清单与注解（星标 / 重命名 / 隐藏 / 上次账号）搬进了**本机常驻后端**（历史跨机 join 的唯一的家：
//! 后端 `history_join.rs` · `history_annotations.rs`），界面经通道问（`src/history-reads.ts`）。这里原先的「两级懒加载」
//! （`list_history_projects`〔散文墓碑〕 · `stream_history_sessions_in_project`〔散文墓碑〕）与注解读写一起删了。
//! 注解那份文件**原地不动**（`<monitor_data_dir>/history-metadata.json`，路径仍由本文件 [`metadata_path`] 算，
//! 起本机后端时交过去）—— star / 重命名 / 隐藏仍然**不改 jsonl**（Claude Code 的数据保持零侵入）。
//!
//! ## 物理删除
//!
//! 用户明确选了「物理删除」。前端二次确认后经通道直说那台机器的后端 `files-delete-session`（〔MIG-3b〕monitor 那条转交删了）；
//! 删完那条注解由界面交本机后端 `history-forget` 连带删。Claude Code 自己也不再能 resume 这个会话。

use crate::paths;
use std::path::PathBuf;

// === 〔C4d · 第四波 4B〕历史清单与注解搬进了本机常驻后端 ===
//
// 主会话 09-25 裁（`调研/第四波记录/C4d.md`「主会话裁」第 2 条）：注解（`history-metadata.json`：星标 / 改名 / 隐藏 / 上次账号）的
// **读写者**换成本机常驻后端（文件原地不动：路径由本文件 [`metadata_path`] 算、起本机后端时交过去），它经 `remote_ask` 问远端那台、
// 并上注解、出成品；前端经 `chan.call`（`src/history-reads.ts`）。这里原先那一族随之删了〔散文墓碑〕：
// 两个线上形状（`HistoryProject` / `HistorySessionEntry`）· 注解三件（`HistoryMetadata` / `EntryMetadata` / `MetadataPatch`）·
// 本机项目清单（`list_history_projects` / `local_projects_via`〔散文墓碑〕· 判活绑定 `SessionMapLiveness`〔散文墓碑〕）·
// Codex 合成（`enumerate_codex_sessions` / `codex_projects_from` / `codex_session_entry` / `codex_first_user_excerpt`〔散文墓碑〕）·
// 展开一个项目（`stream_history_sessions_in_project` 与它的 `analyze_jsonl`〔散文墓碑〕一族）· 改注解 / 上次账号表那两条命令。
// 判定逐格搬进后端（`history_join.rs` · `history_annotations.rs` · `agents/codex/history.rs`），判据跟着搬；
// 「迁移前后读出来的注解逐条相等」由结构占位夹具 `tests/__fixtures__/history-metadata.fixture.json` ＋ 旧读者产出的金样
// `tests/__fixtures__/history-metadata.readout.golden.json` 钉着。

// === metadata 那份文件在哪（〔C4d〕读写者是本机常驻后端；路径仍由这里算）===

/// 历史注解那份文件的路径 —— **全仓只此一处算它**。〔C4d · 第四波 4B〕读写者换成了本机常驻后端（主会话 09-25 裁：
/// 文件留在原处、同一路径）：monitor 起本机后端时用 `CCM_HISTORY_METADATA` 把**本函数算出来的这一个**交过去
/// （`local_backend_host::relay_host_envs`），同一路径因此是构造出来的，不是两侧算法对齐出来的。
pub(crate) fn metadata_path() -> Option<PathBuf> {
    Some(paths::resolve_monitor_data_dir()?.join(creds_core::store::HISTORY_METADATA_FILE))
}

#[cfg(test)]
#[path = "../../../tests/bridge/history_tests.rs"]
mod tests;
