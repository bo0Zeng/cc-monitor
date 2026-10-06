//! 历史会话的 monitor 这一侧：已经一件事都不做了（只剩判据）。
//! 读一整份会话（`stream_read_session_jsonl`〔散文墓碑〕与它的分页器 `SessionPager`〔散文墓碑〕）退役：
//! 那台后端出记录行（`history-page`），界面经通道直问（`src/frontend/ui/record-reads.ts`）；那句「超过上限」随判定进了后端。
//! 删一份会话与 **F62 从某轮建分支** 不在这里了：界面经通道直说那台机器的后端（`src/frontend/ui/session-writes.ts`）。
//!
//! ## 清单与注解不在这里了
//!
//! 项目 / 会话清单与注解（星标 / 重命名 / 隐藏 / 上次账号）搬进了**本机常驻后端**（后端 `history_list.rs` ·
//! `history_annotations.rs`），界面经通道问（`src/frontend/ui/history-reads.ts`）。这里原先的「两级懒加载」
//! （`list_history_projects`〔散文墓碑〕）与注解读写一起删了。
//! 注解那份文件住这台的家（`~/.cc-monitor/history-metadata.json`），那台后端按家自己推 —— star / 重命名 / 隐藏仍然**不改 jsonl**
//! （Claude Code 的数据保持零侵入）。
//!
//! ## 物理删除
//!
//! 用户明确选了「物理删除」。前端二次确认后经通道直说那台机器的后端 `files-delete-session`（monitor 那条转交删了）；
//! 删完那条注解由界面交本机后端 `history-forget` 连带删。Claude Code 自己也不再能 resume 这个会话。

// === 历史清单与注解搬进了本机常驻后端 ===
//
// 注解（`history-metadata.json`：星标 / 改名 / 隐藏 / 上次账号）的
// **读写者**换成本机常驻后端（文件住家里，那台后端按家推），它经 `remote_ask` 问远端那台、
// 并上注解、出成品；前端经 `chan.call`（`src/frontend/ui/history-reads.ts`）。这里原先那一族随之删了〔散文墓碑〕：
// 两个线上形状（`HistoryProject` / `HistorySessionEntry`）· 注解三件（`HistoryMetadata` / `EntryMetadata` / `MetadataPatch`）·
// 本机项目清单（`list_history_projects` / `local_projects_via`〔散文墓碑〕· 判活绑定 `SessionMapLiveness`〔散文墓碑〕）·
// Codex 合成（`enumerate_codex_sessions` / `codex_projects_from` / `codex_session_entry` / `codex_first_user_excerpt`〔散文墓碑〕）·
// 展开一个项目（`analyze_jsonl`〔散文墓碑〕一族）· 改注解 / 上次账号表那两条命令。
// 判定逐格搬进后端（`history_list.rs` · `history_annotations.rs` · `agents/codex/history.rs`），判据跟着搬；
// 「迁移前后读出来的注解逐条相等」由结构占位夹具 `tests/__fixtures__/history-metadata.fixture.json` ＋ 旧读者产出的金样
// `tests/__fixtures__/history-metadata.readout.golden.json` 钉着。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/history_tests.rs"]
mod tests;
