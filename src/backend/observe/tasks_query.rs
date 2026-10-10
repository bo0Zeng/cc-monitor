//! **会话的任务列表** —— `tasks-list` 那条帧命令的本体。
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

/// 任务列表的根：后端盯着的那一家（注册表 `LocalFace.tasks_dir`）。那一家没有任务列表 ⇒ 家目录下一个不会有任务的位置（读出零条）。
pub(crate) fn tasks_root(home: &Path) -> std::path::PathBuf {
    crate::agents::tree_local_face()
        .and_then(|f| f.tasks_dir)
        .map_or_else(|| home.join(NO_TASKS), |f| f(home))
}

/// 注册表里没有任务列表那一家时 [`tasks_root`] 指的那个名字（不建、不写，只读出零条）。
const NO_TASKS: &str = ".no-task-dir";

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
            crate::common::said::IntoNote::into_note(crate::common::said::Said::with_raw(
                copy_text(
                    "beTasksQuery.sessionTaskLines.unreadable",
                    &[
                        ("dir", &(dir.display()).to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                e,
            )),
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

/// 一个任务对象 → **成品**（`tasks-list` 的 `data.tasks` 里的一格）。**纯函数**。
///
/// 字段语义**只住这里**（「业务解释只有一个家」）：此前后端只保证「每行是一个对象」、
/// 字段由 monitor 那边的 `parse_task_lines`〔散文墓碑〕解（serde `TaskEntry`）；搬过来之后口径逐字照那一份：
/// - `id` / `subject` / `status` **必填、是串**（缺 / 不是串 ⇒ 这一条不算任务，`None`）；
/// - `description` / `activeForm` 可缺、可为 `null`（⇒ 成品里**不出现**这一格），出现就必须是串；
/// - `blocks` / `blockedBy` 缺 ⇒ 空表；出现就必须是**串的数组**（`null` / 别的 ⇒ 这一条不算，同 serde `Vec<String>`）；
/// - 别的键一律不带（成品只有这七格，界面按形状严格收）。
pub(crate) fn task_entry(v: &serde_json::Value) -> Option<serde_json::Value> {
    use serde_json::Value;
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let (id, subject, status) = (s("id")?, s("subject")?, s("status")?);
    let opt = |k: &str| -> Option<Option<String>> {
        match v.get(k) {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(x)) => Some(Some(x.clone())),
            Some(_) => None,
        }
    };
    let list = |k: &str| -> Option<Vec<String>> {
        match v.get(k) {
            None => Some(Vec::new()),
            Some(Value::Array(a)) => a.iter().map(|x| x.as_str().map(str::to_string)).collect(),
            Some(_) => None,
        }
    };
    let (description, active_form) = (opt("description")?, opt("activeForm")?);
    let (blocks, blocked_by) = (list("blocks")?, list("blockedBy")?);
    let mut out = serde_json::json!({
        "id": id,
        "subject": subject,
        "status": status,
        "blocks": blocks,
        "blockedBy": blocked_by,
    });
    if let Some(d) = description {
        out["description"] = Value::String(d);
    }
    if let Some(a) = active_form {
        out["activeForm"] = Value::String(a);
    }
    Some(out)
}

/// 一个会话的任务**成品**：[`session_task_lines`] 读出的每个对象过 [`task_entry`]，解不成任务的跳过（`trace!`），
/// 顺序原样（任务号升序）。
pub(crate) fn session_tasks(home: &Path, sid: &str) -> Result<Vec<serde_json::Value>, Refusal> {
    Ok(session_task_lines(home, sid)?
        .iter()
        .filter_map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).ok()?;
            let t = task_entry(&v);
            if t.is_none() {
                tracing::trace!("任务对象缺必填格或类型不对，跳过：{l}");
            }
            t
        })
        .collect())
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/tasks_query_tests.rs"]
mod tests;
