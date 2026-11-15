//! 假 agent 的本机布局与判活（`LocalFace`）：解析本机 home · pidfile 目录 · 判活 cmdline · 任务目录 · 进程事实。

use super::AGENT_KIND;
use std::path::{Path, PathBuf};

/// 解析本机 home 的环境变量。⚠ **刻意没有默认值**（Claude 那家恒有值）——
/// 少了这个差别，`visible_among` 里 `None` 那条分支就永远不会被这家走一遍。
pub const HOME_ENV: &str = "CCM_FAKE_AGENT_HOME";

/// 能力 8：解析本机 home。`None` = 连候选路径都说不出。
pub fn home() -> Option<PathBuf> {
    std::env::var_os(HOME_ENV).map(PathBuf::from)
}

/// 能力 4：pidfile 目录。**`live` 不是 `sessions`**。
pub fn pidfile_root(home: &Path) -> PathBuf {
    home.join("live")
}

/// 能力 7：判活时"这个 cmdline 看起来像不像本 agent"。
///
/// ⚠ 与 Claude 那家**方向相同但词表不同**（那边是「明显不像才判冒名」，含 `claude`/`node`）。
pub fn cmdline_may_be_agent(lower: &str) -> crate::agents::CmdlineSays {
    use crate::agents::CmdlineSays;
    if lower.trim().is_empty() || lower.contains("fakeagent") {
        CmdlineSays::Session
    } else {
        CmdlineSays::Unlike
    }
}

/// 本机布局面。家目录：给了账号配置目录就是它，否则 [`home`]（没有默认 ⇒ 空路径）。
pub(super) fn local_face(
    pidfile_dir: fn(&Path) -> PathBuf,
    cmdline_may_be_agent: fn(&str) -> crate::agents::CmdlineSays,
) -> crate::agents::LocalFace {
    crate::agents::LocalFace {
        home_at: |config_dir, _| {
            config_dir
                .map(Path::to_path_buf)
                .or_else(home)
                .unwrap_or_default()
        },
        pidfile_dir,
        cmdline_may_be_agent,
        // 假的那一家有任务目录、不认任何任务文件（任务的读法在真那一家的判据里钉）。
        tasks: Some(crate::agents::TasksFace {
            dir: |h| h.join("todo"),
            task: |_| None,
        }),
        proc_of: |v| crate::agents::ProcFact {
            kind: AGENT_KIND,
            sid: v
                .get("sid")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            cwd: v
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            proc_start: v.get("procStart").and_then(serde_json::Value::as_u64),
            background: v.get("bg").and_then(serde_json::Value::as_bool) == Some(true),
            ..Default::default()
        },
    }
}
