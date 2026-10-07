//! 「文件与数据」那一份成品（帧面 `data-report`）：同一份足迹（[`super::rows`]）按设置窗那一页要的三样重排 —— 只读。
//!
//! - `changedFiles`：cc-monitor 写进**你的**文件的那几处（今天在、cc-monitor 装口放的、不在 `~/.cc-monitor/` 里），
//!   每处说改了什么、撤回在哪一页（页 · 栏 · 锚点，界面照它跳）。
//! - `todo`：「要你动手」各件（[`super::chores`]）；其中「要装」那一类来自 [`needs_install`]：你自己装、cc-monitor 只查的那几样里，
//!   这台**确实缺**的（查不动的不算缺，不报；tmux 一律可选、不进这里）；`required` ＝ 缺了起不了会话。
//! - `tmux`：这台有没有 tmux（查不动 ⇒ `null`；与起新会话那一问同一个判法 `control::terminals::rows_here`）。
//! - `chores`：「要你动手」里进角标的件数（要做 ＋ 要装 ＋ 要你定，还没做完的），设置窗左栏角标与主窗口状态栏那一枚读这一个数。
//!
//! 另带 `home`（显示用：路径的 `~` 缩写按它）。
//!
//! 判定只在这里：哪一行算「改过你的文件」、撤回去哪、哪一样缺了起不了会话、怎么装的链接。

use super::registry::EnvTier;
use super::rows::{ConfigSurfaceReport, SurfaceRow, SurfaceState};
use serde_json::{json, Value};

/// cc-monitor 自己的家（相对家目录）：这下面的不是「你的文件」。
const OWN_HOME: &str = "~/.cc-monitor";

/// 撤回在哪：设置窗的页 · 栏 · 锚点（与设置窗「带目的地打开」同一种形状）。
fn undo_of(tool_id: &str) -> Option<Value> {
    match tool_id {
        // 接上终端那几行：别名与配置文件那一栏里「接上终端」那一行（卸载在那里）。
        "ccm" => Some(json!({"page": "machine", "tab": "config", "anchor": "connect-terminal"})),
        // 扩展装进 `~/.claude/skills/` 的：扩展页卸载。
        "cc-bus" | "skill-install" => Some(json!({"page": "ext"})),
        _ => None,
    }
}

/// 缺了起不了会话的那几样（其余缺了只少一个功能）。
fn required(tool_id: &str) -> bool {
    matches!(tool_id, "claude-cli")
}

/// 怎么装：那个工具自己的安装说明（外链）；没有通用的 ⇒ `None`。
fn how_url(tool_id: &str) -> Option<&'static str> {
    match tool_id {
        "claude-cli" => Some("https://docs.anthropic.com/en/docs/claude-code/setup"),
        "login-shell" => Some("https://www.gnu.org/software/bash/"),
        _ => None,
    }
}

fn changed(r: &SurfaceRow) -> Option<Value> {
    if r.tier != EnvTier::AppInstalls || !matches!(r.state, SurfaceState::Present { .. }) {
        return None;
    }
    let p = r.path_declared;
    if !p.starts_with("~/") || p == OWN_HOME || p.starts_with(&format!("{OWN_HOME}/")) {
        return None;
    }
    Some(json!({
        "path": p,
        "what": r.tool_name,
        "undo": undo_of(r.tool_id),
    }))
}

fn missing(r: &SurfaceRow) -> Option<Value> {
    if r.tier != EnvTier::UserInstallsWePrompt || r.state != SurfaceState::Absent {
        return None;
    }
    // tmux 一律可选：缺了不进「要装」（有没有它只走 `tmux` 那一格，没有的那台不出 tmux 相关项）。
    if r.tool_id == "tmux" {
        return None;
    }
    Some(json!({
        "id": r.tool_id,
        "name": r.tool_name,
        "what": r.path_declared,
        "required": required(r.tool_id),
        "howUrl": how_url(r.tool_id),
    }))
}

/// 足迹里这台确实缺的那几样（「要你动手」里「要装」那一类的事实）。
pub(crate) fn needs_install(report: &ConfigSurfaceReport) -> Vec<Value> {
    report.rows.iter().filter_map(missing).collect()
}

/// 整份足迹 ＋「要你动手」各件 ＋ 有没有 tmux ⇒ 这一页的成品。
pub(crate) fn shape(report: &ConfigSurfaceReport, todo: Vec<Value>, tmux: Option<bool>) -> Value {
    let changed_files: Vec<Value> = report.rows.iter().filter_map(changed).collect();
    let chores = super::chores::badge(&todo);
    json!({
        "home": report.home,
        "changedFiles": changed_files,
        "todo": todo,
        "tmux": tmux,
        "chores": chores,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/footprint/data_tests.rs"]
mod tests;
