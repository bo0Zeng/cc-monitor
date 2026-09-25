//! issue #16 P1a：远端历史浏览的 monitor 侧。
//!
//! 〔C4d · 第四波 4B 订正〕这里原先写着「每条查询走**独立 SSH 连接**一次性 exec `<backend_path> --list-projects` 等」
//! ＋「首行是 hello 就报 backend 版本过旧」—— 那是 issue #16 P1a 的形状。`C1`（09-24）起查询全走那台的长连接
//! （`backend::control::frame_query`，老后端由帧面的能力协商说「还不认」），逐次拨号那条路 C4d 删了。
//!
//! 只读铁律（INVARIANT § 1）：本模块只读远端；resume/delete 对远端在前端禁用。
//! INVARIANTS § 25：本路径是一次性读取（非 at-least-once 行流），SessionViewer
//! 每次 load 全新实例，无重投幂等义务。

use crate::ssh_source::RemoteConfig;

// 〔LOC1b · 第四波 4D〕读一整份会话的总量上限 `MAX_SESSION_BYTES` 与超限那句话（F06）搬去了 `history.rs`：
//   本机冷读也改走那台后端的 `history-read`，本机远端合成一条（`history·rs::stream_read_session_jsonl`）。

pub(crate) fn require_cfg_by_label(label: &str) -> Result<RemoteConfig, String> {
    crate::load_remote_config_by_label(label)
        .ok_or_else(|| format!("远端 '{label}' 未配置或未启用"))
}

// 〔C4d · 第四波 4B〕逐次拨号那条路（`run_list_query`〔散文墓碑〕与它的老后端识别、超时）删了：
//   `frame_query` 那张「仍拨号」的表 C4c 起就是空的 ⇒ 它一条都放不过去；主会话 09-25 裁删，
//   唯一调用方（`subagent·rs::Backend::query` 的远端回落）同拍改成「没有帧命令就当场说」。

// 〔C4a · 第四波〕远端全文搜索的 fan-out（issue #28）**搬到前端**：`src/views/history-search.ts`
//   对每台远端经通道说帧命令 `history-search`、逐行解释、补 `origin`、与本机索引合并 ——
//   三件事在那边各只有一个家，这里的那一份〔散文墓碑〕同拍删掉（旧名 `search_remote_all`）。

// 〔C4d · 第四波 4B〕远端历史清单那一族（`K-R83` / `K-R92` 的「不知道」三态 `Counted` / `WhyUnknown` / 判活入参 `LivenessOracle`
//   · 行 ⇒ 项目 / 会话的 join `history_project_from_row` / `remote_session_entry`〔散文墓碑〕· 逐台 fan-out
//   `fanout_list_projects` 与 `list_remote_history_projects`〔散文墓碑〕· 展开远端一个项目 `stream_remote_history_sessions`〔散文墓碑〕）
//   整族搬进了本机常驻后端（`src/backend/history_join.rs`：判定一字不改、判据跟着搬 —— `tests/backend/history_join_tests.rs`）。
//   主会话 09-25 裁「join 只一个家」：注解归本机后端，它经 `remote_ask` 问远端那台、并上注解、出成品；界面经 `chan.call`
//   （`src/history-reads.ts`，逐台 fan-out 也搬到那里）。这里只剩读一整份远端会话与删一份远端会话两件。
//   〔LOC1b · 4D〕读那一件也走了（见下）⇒ 只剩删一份远端会话。

// 〔LOC1b · 第四波 4D〕`stream_read_remote_session`〔散文墓碑〕删了：它是 `history·rs::stream_read_session_jsonl` 的远端那一支，
//   两支合成一条（本机也经那台后端的 `history-read` 分页取原文、同一个 `SessionPager` 解析）。

/// 删除一个远端历史会话的 jsonl（issue 未拆，F11）—— [`crate::history::delete_history_session`] 的远端那一支。
///
/// 🔴 **〔RW1 · 第四波 · 2026-09-24〕F11 按用户裁「按推荐改」：经那台远端的后端删**
/// （`files-delete-session`，只收 sid；落点由远端后端按 sid 在它自己的记录树里找）。
/// 从前那一道 SFTP 直删 ＋ 双重路径守卫（`sftp::remove_remote_file`〔散文墓碑〕）整条走了；
/// 「删 A 的文件、清 B 的注解」那个口由 [`crate::history::delete_via_backend`] 的一致性闸在两侧同时防。
pub(crate) async fn delete_remote_history_session(
    host: &str,
    session_id: String,
    jsonl_path: String,
) -> Result<(), String> {
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin(host.to_string()));
    crate::history::delete_via_backend(&door, &session_id, &jsonl_path).await
}

/// 远端 POSIX 路径取文件名 stem（去目录、去 `.jsonl`）。非 jsonl / 无文件名 → None。
pub(crate) fn jsonl_stem(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    name.strip_suffix(".jsonl").map(str::to_string)
}

#[cfg(test)]
#[path = "../../../tests/bridge/remote_history_tests.rs"]
mod tests;

// 〔C4d〕`K-R83` 那三条判据（`remote_history_kr83_tests.rs`）随被测的 join 一起搬进后端（`history_join_tests.rs`）。
