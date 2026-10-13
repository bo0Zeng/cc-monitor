//! **第 7 批换形的壳那一半**（下一版整份删）：「对那台做事时怎么做」那几格已经搬到各台后端的 `prefs.json`
//! （本机常驻后端起来时换，`src/backend/control/prefs_migration.rs`）。`config.json` 只有本壳写（[`crate::config::patch_config_at`]），
//! 所以删旧键是这里的事：本机后端 `prefs-read` 说 `ok`（本机那一份写成了）以后，从 `config.json` 删掉搬走的那几个顶层键，
//! 删前原文另存 `config.json.pre-7`（只存一次，不拿删过的盖掉原文）。本机那一份没写成 ⇒ 不删，数据不丢。
//!
//! 机器表（`remote`）搬进本机 `machines.json`：本机 `machines-read` 说 `ok` 才删（[`after_machines_read`]）。
//!
//! 搬走的那几个键：`resumeCommand`（通用页那一格）· `localResumeCommand`（本机那一格）· `resumeInTmux` · `contextLimits` ·
//! `accounts`（只剩 `byMachine.<origin>.modelByAccount`，每号模型）· `claudeDir`（数据目录那一格，进本机 `agentHomes`）。机器表每台的 `resumeCommand` 那一格界面不再读写，
//! 那一台下次存的时候自然没了（机器表那一格不拒多出来的键）。

use crate::copy_table::copy_text;
use crate::detail::Said;
use serde_json::{json, Value};
use std::path::Path;

/// 搬进本机 `prefs.json` 的那几个顶层键（`src/frontend/ui/config.ts` 把它们登记给本文件）：本机 `prefs-read` 说 `ok` 才删。
pub(crate) const MOVED_KEYS: [&str; 6] = [
    "resumeCommand",
    "localResumeCommand",
    "resumeInTmux",
    "contextLimits",
    "accounts",
    "claudeDir",
];

/// 搬进本机 `machines.json` 的那一个（机器表）：本机 `machines-read` 说 `ok` 才删（[`after_machines_read`]）。
pub(crate) const MOVED_MACHINES: [&str; 1] = ["remote"];

/// 原文那一份的名字（与 `config.json` 同一个目录）。
pub(crate) const BACKUP_NAME: &str = "config.json.pre-7";

/// 一趟的结局。
#[derive(Debug, PartialEq)]
pub(crate) enum Finished {
    /// 盘上已经没有那几个键（换过了 / 新装 / 文件不在）：什么都不做。
    NothingLeft,
    /// 删了几个。
    Removed(usize),
}

/// 本机后端那一答说本机那一份写成了没有（只认 `ok`；`prefs-read` · `machines-read` 同一形）。
pub(crate) fn local_prefs_ready(reply: &Value) -> bool {
    reply.get("state").and_then(Value::as_str) == Some("ok")
}

/// 删 `path` 那份 `config.json` 里搬走的那几个键（路径 · 键是参数）。读不懂 ⇒ 一个字节不写、照实报。
pub(crate) fn finish_at(path: &Path, keys: &[&str]) -> Result<Finished, Said> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Finished::NothingLeft),
        Err(e) => {
            return Err(Said::with_raw(
                copy_text(
                    "rsConfig.write.readFailed",
                    &[
                        ("path", &path.display().to_string()),
                        ("why", &copy_core::io_reason(e.kind())),
                    ],
                ),
                &e,
            ))
        }
    };
    let root = match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(m)) => m,
        _ => {
            return Err(Said::from(copy_text(
                "rsConfig.write.unreadable",
                &[("path", &path.display().to_string())],
            )))
        }
    };
    let present: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|k| root.contains_key(*k))
        .collect();
    if present.is_empty() {
        return Ok(Finished::NothingLeft);
    }
    // 原文只存一次：已有那一份 ⇒ 不动它（它才是换形之前的那一份）。
    let backup = path.with_file_name(BACKUP_NAME);
    if !backup.exists() {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        crate::platform::fs::only_me_on_create(&mut opts);
        opts.open(&backup)
            .and_then(|mut f| f.write_all(text.as_bytes()).and_then(|()| f.sync_all()))
            .map_err(|e| {
                Said::with_raw(
                    copy_text(
                        "rsConfig.write.readFailed",
                        &[
                            ("path", &backup.display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
            })?;
    }
    let edits: Vec<crate::config::ConfigEdit> = present
        .iter()
        .map(|k| crate::config::ConfigEdit::Remove {
            path: vec![(*k).to_string()],
        })
        .collect();
    crate::config::patch_config_at(path, &edits)
        .map_err(crate::config::ConfigWriteError::into_said)?;
    Ok(Finished::Removed(present.len()))
}

/// 本机入方向通道登记上了（两条载体各调一次）：后台问本机后端 `prefs-read`，说 `ok` ⇒ 删旧键。没成只记一行日志，下次起来再来。
pub(crate) fn after_local_ready() {
    tauri::async_runtime::spawn(async {
        let deadline = crate::frame_query::Deadline::within(std::time::Duration::from_secs(10));
        let reply = crate::frame_query::call(
            &crate::origin::Origin::local(),
            "prefs-read",
            json!({}),
            None,
            deadline,
        )
        .await;
        match reply {
            Ok(r) if local_prefs_ready(&r) => {}
            Ok(_) => return,
            Err(e) => {
                tracing::info!("没问到本机的偏好，旧设置里那几格这次不删：{e}");
                return;
            }
        }
        finish_now(&MOVED_KEYS).await;
    });
}

/// 本壳问到本机 `machines-read` 那一答之后调（[`crate::machine_table::refresh`]）：说 `ok` ⇒ 删 `config.json` 的 `remote`。
pub(crate) fn after_machines_read(reply: &Value) {
    if !local_prefs_ready(reply) {
        return;
    }
    tauri::async_runtime::spawn(finish_now(&MOVED_MACHINES));
}

async fn finish_now(keys: &'static [&'static str]) {
    let Some(path) = crate::config::resolve_config_path() else {
        return;
    };
    match tauri::async_runtime::spawn_blocking(move || finish_at(&path, keys)).await {
        Ok(Ok(Finished::Removed(n))) => {
            tracing::info!("config.json 里搬到本机后端的 {n} 个键删了（原文留 {BACKUP_NAME}）")
        }
        Ok(Ok(Finished::NothingLeft)) => {}
        Ok(Err(e)) => tracing::warn!("config.json 里搬走的那几个键没删成：{}", e.said),
        Err(e) => tracing::warn!("config.json 里搬走的那几个键没删成：{e}"),
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/prefs_migration_tests.rs"]
mod tests;
