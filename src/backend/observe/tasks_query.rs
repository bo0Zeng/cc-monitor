//! 〔RM1b · 第四波〕**会话的任务列表** —— `tasks-list` 那条帧命令的本体。
//!
//! # 它补的是哪一格
//!
//! monitor 那头的任务面板（`tasks-panel.ts`）此前只读**本机** `tasks/<sid>/*.json`
//! （`parity_ledger` 的 `session.tasks` 那笔 `ParityDebt`）：远端会话的任务在远端机器上，
//! 远端 tab 永远拿不到。⇒ 读这件事搬进后端：**本机后端与远端后端是同一个二进制**，
//! monitor 按 origin 问那一台，本机也走这里（monitor 里那份读实现随之退役）。
//!
//! # 本层**不懂任务的字段**
//!
//! 只做四件事：走 `<tasks>/<sid>/`、只认 `<数字>.json`、剥 BOM、解成 **JSON 对象**
//! —— 然后按那个数字升序（= 创建顺序 = 终端里看到的顺序），把对象**原样**压成一行一个。
//! 字段怎么读（`subject` / `status` / `blockedBy` …）只在 monitor 的 `TaskEntry` 一处：
//! 后端再抄一份字段表，就是两份会各自漂的真相。
//!
//! # 跳过谁、为什么
//!
//! - `.lock` / `.highwatermark` / 非 `json` / 文件名不是纯数字：**不是任务文件**，不算跳过。
//! - 读不出、解不成对象：写者持锁那一刻读到半截是**正常时序**（下一次变更会再触发重读）
//!   ⇒ 跳过这一条、`trace!`，与搬家前的口径逐字相同。
//! - 超过 [`TASK_FILE_CAP_BYTES`]：**跳过＋说清**（`warn!` 点名那个文件）——
//!   一个坏文件不该毁掉整张表，但也不许不说一声就少一条。
//!
//! # 读失败不许说成「没有任务」
//!
//! 那个 sid 的目录**不存在** ⇒ 空表（诚实的空：这个会话没用过任务）。
//! 目录**在但读不了** ⇒ 命令级 `failed`：回空表就与上一行长得一模一样。

use copy_core::copy_text;
use std::path::Path;

/// 单个任务文件的读上限。本机实测任务文件是几百字节量级（一条 subject ＋ 一段 description）；
/// 1 MiB 留了三个数量级的余量。超了走**跳过＋说清**（见模块头注）。
pub(crate) const TASK_FILE_CAP_BYTES: u64 = 1 << 20;

/// `<home>/tasks` —— 任务列表的根。
///
/// ⚠ **目录名 `tasks` 是 Claude 的布局知识，它住在这里而不在 `agents/claudecode/paths.rs`**，
/// 理由如实写：`observe/` 这一层今天本来就是 Claude 专属的（`observe/mod.rs` 头注），
/// 与 `watcher` / `history_query` 认得 `.jsonl` 同一处境；而把它搬进适配层、从这里调过去，
/// 就是 `agent_locality_guard::ADAPTER_CALL_SITES` 里新长一处 —— 那张表要求同拍给
/// `agents::fake::CAPABILITIES` 补一种能力（`S6` 的反向夹具），那一刀归 `L2`/`S6` 收接口那轮。
/// ⇒ 欠账登记在 RM1b 的记录文件里，不在这里假装已经分层。
pub(crate) fn tasks_root(home: &Path) -> std::path::PathBuf {
    home.join("tasks")
}

/// 本族的错误出口：`(code, message)`。
pub(crate) type Refusal = (&'static str, String);

/// `sid` 只许是**一段**普通路径名 —— 它要被拼进 `<tasks>/<sid>/`，
/// 空串 / 含分隔符 / `.` / `..` 都会让那一拼走出任务根。
fn check_sid(sid: &str) -> Result<(), Refusal> {
    let bad = sid.is_empty()
        || sid == "."
        || sid == ".."
        || sid.contains('/')
        || sid.contains('\\')
        || sid.contains('\0');
    if bad {
        return Err((
            "bad_args",
            crate::common::contract::malformed(&format!("`sid` is not a session id: {sid:?}")),
        ));
    }
    Ok(())
}

/// 一个会话的任务，按任务号升序、每条一行（原样的 JSON 对象，压成一行）。
pub(crate) fn session_task_lines(home: &Path, sid: &str) -> Result<Vec<String>, Refusal> {
    check_sid(sid)?;
    let dir = tasks_root(home).join(sid);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| {
        (
            "failed",
            copy_text(
                "beTasksQuery.sessionTaskLines.unreadable",
                &[("dir", &(dir.display()).to_string()), ("e", &e.to_string())],
            ),
        )
    })?;

    let mut out: Vec<(u64, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let ext_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"));
        if !ext_ok {
            continue;
        }
        let Some(num) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
        else {
            continue;
        };
        match std::fs::metadata(&path) {
            Ok(m) if m.len() > TASK_FILE_CAP_BYTES => {
                tracing::warn!(
                    "任务文件 {} 有 {} 字节，超过单个任务文件的读上限 ⇒ 这一条跳过",
                    path.display(),
                    m.len()
                );
                continue;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::trace!("stat 任务文件 {} 失败：{e}", path.display());
                continue;
            }
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                tracing::trace!("读任务文件 {} 失败：{e}", path.display());
                continue;
            }
        };
        let v: serde_json::Value = match serde_json::from_str(raw.trim_start_matches('\u{feff}')) {
            Ok(v) => v,
            Err(e) => {
                tracing::trace!("解析任务文件 {} 失败：{e}", path.display());
                continue;
            }
        };
        if !v.is_object() {
            tracing::trace!("任务文件 {} 不是一个 JSON 对象", path.display());
            continue;
        }
        out.push((num, v.to_string()));
    }
    out.sort_by_key(|(n, _)| *n);
    Ok(out.into_iter().map(|(_, l)| l).collect())
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/tasks_query_tests.rs"]
mod tests;
