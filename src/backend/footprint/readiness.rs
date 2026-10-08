//! 首次运行「开始用 · 剩 N 步」那份数（帧面 `readiness`）：三步各自由这台的事实打勾，判定只在这里。
//!
//! - `terminal`（让终端认得 ccm 和别名）：这台某一份启动文件里有别名块（与「别名与配置文件」那一行同一份候选）。
//! - `named`（给现在登录的号起个名字，可选）：这台启用了多账号（账号库清单读得出，与账号页 `meta.enabled` 同一个判法）。
//! - `remote`（加一台远端机器，可选）：机器表里至少有一台。机器表住 monitor 那一侧，台数由问的那一方带上（`remotes`），这里只判。
//!
//! 哪步必做也在这里定（`required`：今天只有 `terminal`；另两步可选）。`left` ＝ 必做而还没打勾的几步（主窗口状态栏那一枚只数它，
//! 可选的不为它一直催）；`skipped` ＝ 「开始用」那一块点过「跳过」（记在 `chores.json`，经 `chores-mark` 的 `skipStart` 写）。只读。

use crate::assets::door::Door;
use crate::platform::shell::dialect::Shell;
use serde_json::{json, Value};

type Answer = Result<Value, (&'static str, String)>;

/// 三步的事实 ⇒ 成品。**纯函数**。
pub(crate) fn product(terminal: bool, named: bool, remotes: u64, skipped: bool) -> Value {
    let steps = [
        ("terminal", terminal, true),
        ("named", named, false),
        ("remote", remotes > 0, false),
    ];
    let left = steps
        .iter()
        .filter(|(_, done, required)| *required && !done)
        .count();
    json!({
        "steps": steps
            .iter()
            .map(|(id, done, required)| json!({"id": id, "done": done, "required": required}))
            .collect::<Vec<_>>(),
        "left": left,
        "skipped": skipped,
    })
}

/// 问的那一方带上的机器表台数（必填，非负整数）。
pub(crate) fn remotes_arg(args: &Value) -> Result<u64, (&'static str, String)> {
    args.get("remotes").and_then(Value::as_u64).ok_or_else(|| {
        (
            "bad_args",
            crate::common::contract::malformed("`remotes` must be a non-negative integer"),
        )
    })
}

/// 这台某一份启动文件里有别名块（读不动 ⇒ 当没接上：那一步照实没打勾）。
fn terminal_here(door: &dyn Door) -> bool {
    let shell = if cfg!(windows) {
        Shell::PowerShell
    } else {
        Shell::Posix
    };
    let marks = super::chores::marks::current(super::chores::marks::marks_path().as_deref());
    crate::assets::aliases::read_via(door, shell, marks.self_paste.as_deref())
        .map(|l| l.rc_candidates.iter().any(|c| c.block.present))
        .unwrap_or(false)
}

/// `readiness {remotes}`：帧面入口（只读）。
pub(crate) fn answer(door: &dyn Door, args: &Value) -> Answer {
    let remotes = remotes_arg(args)?;
    let skipped =
        super::chores::marks::current(super::chores::marks::marks_path().as_deref()).start_skipped;
    Ok(product(
        terminal_here(door),
        crate::observe::accounts_query::accounts_enabled_here(),
        remotes,
        skipped,
    ))
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/readiness_tests.rs"]
mod tests;
