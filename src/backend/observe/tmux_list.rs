//! `tmux-list` 出**成品**：这台机器的 tmux 会话逐行解析好了交出去（原先交原样行、解析住 monitor 的
//! `parse_tmux_ls`〔散文墓碑〕，界面经 monitor 那两条 Tauri 命令问）。解析规则从 monitor 原样搬来（F74 `@ccm_sid` 字符集 · K-R12 段数上下溢），
//! 本机远端同一份；界面经 `chan.call(origin, "tmux-list")` 直接问（`src/frontend/ui/tmux-reads.ts`）。
//!
//! 与流里那份 tmux 观测**同一趟** `tmux ls -F`（`tmux_observe::list_for_query`：同一段脚本、同一个格式串、同一个四态分类）。

use serde::Serialize;
use serde_json::{json, Value};

/// 一个 tmux 会话（成品）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TmuxRow {
    pub(crate) name: String,
    /// `#{pane_current_path}`。
    pub(crate) path: String,
    /// `#{pane_current_command}`。
    pub(crate) command: String,
    pub(crate) attached: bool,
    pub(crate) windows: u32,
    /// F74：`@ccm_sid`（此 tmux 所跑 claude 会话的 sid；几个窗格各跑一个时取活动窗格那个，它没挂就取第一个）。
    /// 一个都没挂 / 不是合法 sid 字符集 ⇒ `None`。
    pub(crate) sid: Option<String>,
    /// 前台命令是注册表里某一家 agent 的进程（`agents::is_agent_process`；Claude 是 `claude` / `node`）。
    /// 从前界面按画像表的判活进程名自己判（`tmux-sessions.ts::isClaudeTmuxCommand`〔散文墓碑〕），判定进了后端。
    pub(crate) agent: bool,
}

// 格式串的列数（`tmux_observe::TMUX_LS_FMT`：name ⇥ path ⇥ cmd ⇥ attached ⇥ windows ⇥ @ccm_sid ⇥ session_id ⇥ 各窗格的 @ccm_sid）。
use crate::observe::tmux_observe::TMUX_LS_FMT_FIELDS as FIELDS;

/// 原样行 → 成品。字段数不符 / 名字空的行丢掉（半截行、非法行不进结果）；windows 非数字回退 0。
///
/// K-R12：段数**下溢**（通道被改写，六列塌成 1 段）与**过溢**（`pane_current_path` 里有真 TAB，合法内容）都丢行，但都出声、分开说。
/// ⚠ 过溢那格是已登记的误伤（带 TAB 的目录名那一行会从名单上消失），处置待单独立件 —— 原样照搬，不在本件改。
pub(crate) fn rows(raw: &str) -> Vec<TmuxRow> {
    raw.lines()
        .filter_map(|line| {
            if line.is_empty() {
                return None;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != FIELDS || f[0].is_empty() {
                if f.len() < FIELDS && !line.trim().is_empty() {
                    tracing::warn!(
                        "CCM_TMUX_UNPARSABLE tmux-list 段数下溢（{} < {FIELDS}）—— tmux 打印通道被改写（K-R12），整行丢弃。原样行：{line:?}",
                        f.len()
                    );
                } else if f.len() > FIELDS {
                    tracing::warn!(
                        "tmux-list 段数过溢（{} > {FIELDS}）—— 多半是 `pane_current_path` 里有真 TAB（合法内容），\
                         这一行的会话会从名单上消失（点名的误伤）。原样行：{line:?}",
                        f.len()
                    );
                }
                return None;
            }
            // 只认合法 sid 字符集（`pane_sids` 筛）：极老 tmux（<3.0）不展开 `#{@ccm_sid}`、原样留字面量
            // （含 `#{}`），当成 sid 会让「有没有 sid」恒真（`INVARIANTS §30`）。
            let sid = crate::common::session_snapshot::pane_sids(f[5], f[7])
                .into_iter()
                .next();
            Some(TmuxRow {
                name: f[0].to_string(),
                path: f[1].to_string(),
                command: f[2].to_string(),
                attached: f[3] == "1",
                windows: f[4].parse().unwrap_or(0),
                sid,
                agent: crate::agents::is_agent_process(f[2]),
            })
        })
        .collect()
}

/// `tmux-list` 的成品：`{installed, sessions}`。观测无效 ⇒ `unobservable`（不是零会话）。
pub(crate) fn answer() -> Result<Value, (&'static str, String)> {
    let (installed, lines) =
        crate::observe::tmux_observe::list_for_query().map_err(|m| ("unobservable", m))?;
    Ok(product(installed, &lines.join("\n")))
}

/// 纯的那一半（判据逐格喂）。
pub(crate) fn product(installed: bool, raw: &str) -> Value {
    json!({ "installed": installed, "sessions": rows(raw) })
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/tmux_list_tests.rs"]
mod tests;
