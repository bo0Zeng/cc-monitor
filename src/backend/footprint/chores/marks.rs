//! 「要你动手」里记下的选择：点过「不用了」的那几件 · 选了「我自己贴」的那份启动文件 · 首次运行「开始用」那一块点过「跳过」。
//! 住 `~/.cc-monitor/chores.json`（`relay_route_core::CHORES_REL`），后端自己的状态，不是用户数据；全仓唯一的写者是 [`answer_mark`]。
//! 读三态（不在 ＝ 什么都没记 · 读不懂 ＝ 不覆盖、照没记算判，写的那一刻回错）。

use copy_core::copy_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 那份文件最大多少字节（超了当读不懂）。
pub(crate) const MAX_BYTES: u64 = 64 * 1024;

/// 记下的那两样。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Marks {
    /// 点过「不用了」的那几件（件的 `id`）。
    #[serde(default)]
    pub declined: Vec<String>,
    /// 选了「我自己贴」的那份启动文件（绝对路径）；没选 ⇒ `None`。
    #[serde(default)]
    pub self_paste: Option<String>,
    /// 首次运行「开始用」那一块点过「跳过」（`footprint/readiness.rs` 读它）。
    #[serde(default)]
    pub start_skipped: bool,
}

/// 这台机器上那份文件的路径；家目录解析不出来 ⇒ `None`。
pub(crate) fn marks_path() -> Option<PathBuf> {
    Some(crate::platform::paths::home_dir()?.join(relay_route_core::CHORES_REL))
}

/// 读一次（三态）。
pub(crate) fn read_at(path: &Path) -> crate::common::own_state::Read<Marks> {
    crate::common::own_state::read_json(path, MAX_BYTES)
}

/// 判「要你动手」用：读不懂的那份照什么都没记算（写的那一刻才回错）。
pub(crate) fn current(path: Option<&Path>) -> Marks {
    match path.map(read_at) {
        Some(crate::common::own_state::Read::Present(m)) => m,
        _ => Marks::default(),
    }
}

/// 改一样：`{op: "decline" | "undecline", id}` · `{op: "selfPaste", rc}` · `{op: "unselfPaste"}` ⇒ 改完的那一份。
pub(crate) fn mark_at(path: &Path, args: &Value) -> Result<Value, (&'static str, String)> {
    let bad = |d: &str| ("bad_args", crate::common::contract::malformed(d));
    let op = args
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("`op` missing"))?;
    let str_arg = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let dir = path
        .parent()
        .ok_or_else(|| bad("marks path has no parent"))?;
    // 只建那一层、建的那一下就是 0700（`own_dir`：后端建自家目录的那一个函数）。
    crate::common::own_dir::ensure_private_dir(dir).map_err(|e| {
        (
            "io_failed",
            copy_text(
                "beChore.marks.mkdirFailed",
                &[("dir", &dir.display().to_string()), ("e", &e.to_string())],
            ),
        )
    })?;
    let _g = crate::platform::lock::hold(dir).map_err(|e| {
        (
            "io_failed",
            crate::common::said::Said::from(e).said_logging_raw(),
        )
    })?;
    let mut m = match read_at(path) {
        crate::common::own_state::Read::Absent => Marks::default(),
        crate::common::own_state::Read::Present(m) => m,
        crate::common::own_state::Read::Unreadable(why) => {
            return Err(("marks_unreadable", why.said_logging_raw()))
        }
    };
    match op {
        "decline" => {
            let id = str_arg("id").ok_or_else(|| bad("`id` missing"))?;
            if !m.declined.contains(&id) {
                m.declined.push(id);
            }
        }
        "undecline" => {
            let id = str_arg("id").ok_or_else(|| bad("`id` missing"))?;
            m.declined.retain(|d| *d != id);
        }
        "selfPaste" => m.self_paste = Some(str_arg("rc").ok_or_else(|| bad("`rc` missing"))?),
        "unselfPaste" => m.self_paste = None,
        "skipStart" => m.start_skipped = true,
        "unskipStart" => m.start_skipped = false,
        _ => {
            return Err(bad(
                "`op` is not one of decline / undecline / selfPaste / unselfPaste / skipStart / unskipStart",
            ))
        }
    }
    crate::common::own_state::write_json(path, &m)
        .map_err(|e| ("io_failed", e.said_logging_raw()))?;
    Ok(json!({"declined": m.declined, "selfPaste": m.self_paste, "startSkipped": m.start_skipped}))
}

/// `chores-mark`：帧面入口（**写口**，只从 `stream/inbound/` 进）。
pub(crate) fn answer_mark(args: &Value) -> Result<Value, (&'static str, String)> {
    let path = marks_path().ok_or(("io_failed", copy_text("beChore.marks.noHome", &[])))?;
    mark_at(&path, args)
}

#[cfg(test)]
#[path = "../../../../tests/backend/footprint/chores_marks_tests.rs"]
mod tests;
