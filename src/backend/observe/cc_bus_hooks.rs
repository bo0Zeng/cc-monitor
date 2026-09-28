//! 〔MIG-3b · `设计/95 §6`「本机远端两条路、两个命令」· `05 §14.3`〕**cc-bus 钩子诊断** —— 帧命令 `hooks-diag` 的本体（只读）。
//!
//! 这台后端读**它自己那台**的 agent 配置根下的 `settings.json`、就地 stat 钩子点名的程序，出整份成品
//! （诊断 ＋ 两种待贴片段 ＋ 读的是哪份文件）。本机远端同一条命令（`chan.call(origin, "hooks-diag")`）；
//! monitor 那两条 Tauri 命令（本机自己读盘 · 远端问三趟再判）连同判定本体一起删了。帧面宿主在顶层 `feature_face`。
//!
//! # 判据为什么不是字符串等值（B04，从 monitor 原样搬来）
//!
//! 用户盘上装的是 `"$HOME/.local/bin/cc-register" >/dev/null 2>&1 || true`，规范片段是 `cc-register …`：
//! 功能等价、字符串不等 ⇒ 按「被执行的程序」判，而不是比原文。看不懂的形态答「无法判断」，不猜「未装」。
//!
//! 只读：不写 `settings.json`（用户 2026-07-28 定调，与 `cc-bus-install.sh` 同一条约定），只生成待贴文本。

use crate::platform::shell::posix;
use copy_core::copy_text;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// 钩子命令里那两个程序（`SessionStart` → `cc-register` · `Stop` → `cc-bus-stop-hook`）。
pub(crate) const PROGRAMS: [&str; 2] = ["cc-register", "cc-bus-stop-hook"];

/// 读 `settings.json` 的上限（它是几 KB 的配置；超了按「读不到」说）。
const SETTINGS_CAP_BYTES: u64 = 1 << 20;

/// 一个钩子的诊断结论。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum HookState {
    /// 没有任何一条 command 调到这个程序。
    NotInstalled,
    /// 裸命令形态（走 PATH）。
    InstalledViaPath { command: String },
    /// 显式路径且**该路径存在**（用户当前的实际状态，不是问题）。
    InstalledAtPath { command: String, path: String },
    /// 显式路径但**该路径不存在** —— 看着像装了，其实指不到东西。
    PathMissing { command: String, path: String },
    /// **无法判断**：命令里出现了目标程序名，但它不是被直接执行的那个（`sh -c` / `env` / `timeout` 包着）。
    /// 猜「未装」和猜「已装」一样是猜（B04-4）。
    Unknown { command: String },
}

impl HookState {
    /// 只有这两种算「能用」；`PathMissing` 刻意不算。
    fn is_working(&self) -> bool {
        matches!(
            self,
            HookState::InstalledViaPath { .. } | HookState::InstalledAtPath { .. }
        )
    }
}

/// 整份诊断。`note` 装「为什么没读到」（文件缺失 / 坏 JSON），不为空即应展示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HooksDiagnosis {
    pub(crate) session_start: HookState,
    pub(crate) stop: HookState,
    pub(crate) note: String,
}

/// 生成待贴片段时盘上的实况。`None` = 取不到，不猜。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SnippetProbe {
    /// `$HOME/.local/bin/` 下那两个程序是否都在。
    pub(crate) home_path_exists: Option<bool>,
    /// 裸命令是否都解析得到（按这台 `PATH`）。
    pub(crate) on_path: Option<bool>,
}

/// 一段待贴片段 ＋ 形态与实况冲突时的警示（`None` = 没冲突）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Snippet {
    pub(crate) text: String,
    pub(crate) warning: Option<String>,
}

/// 成品（形状与界面 `HooksReport` 逐字同；跨语言金样 `tests/__fixtures__/hooks-diag.golden.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HooksReport {
    pub(crate) diagnosis: HooksDiagnosis,
    pub(crate) snippet_home: Snippet,
    pub(crate) snippet_bare: Snippet,
    /// 读的是哪份文件（这台后端看到的路径）。
    pub(crate) source: String,
}

/// 剥掉**配对的**一层包裹引号；不配对原样返回（不替用户猜本意）。
fn unquote_once(tok: &str) -> &str {
    let b = tok.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &tok[1..tok.len() - 1]
    } else {
        tok
    }
}

/// 一条 shell 命令里**被执行的程序**：剥前导 `VAR=x` → 第一个词 → 剥一层配对引号 → (basename, 原词)。
/// 看不懂 ⇒ `None`（不猜）。
pub(crate) fn program_of(cmd: &str) -> Option<(String, String)> {
    let mut rest = cmd.trim();
    loop {
        let tok = rest.split_whitespace().next()?;
        let is_assign = match tok.find('=') {
            Some(i) if i > 0 => tok[..i]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            _ => false,
        };
        if !is_assign {
            break;
        }
        rest = rest[tok.len()..].trim_start();
    }
    let unq = unquote_once(rest.split_whitespace().next()?);
    if unq.is_empty() {
        return None;
    }
    let base = unq.rsplit('/').next().unwrap_or(unq);
    if base.is_empty() {
        return None;
    }
    Some((base.to_string(), unq.to_string()))
}

/// 命令里是否**出现过**目标程序（不一定是被直接执行的那个）—— 包装写法据此判「无法判断」。
fn mentions_program(cmd: &str, want: &str) -> bool {
    cmd.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ';' | '&' | '|' | '(' | ')'))
        .any(|tok| {
            let t = unquote_once(tok);
            !t.is_empty() && t.rsplit('/').next().unwrap_or(t) == want
        })
}

/// 判一条命令是不是在调 `want` 并定态；`exists` 问「这个路径在不在」（注入，纯函数好测）。
pub(crate) fn classify_command(
    cmd: &str,
    want: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Option<HookState> {
    let unknown = || {
        mentions_program(cmd, want).then(|| HookState::Unknown {
            command: cmd.trim().to_string(),
        })
    };
    let Some((base, full)) = program_of(cmd) else {
        return unknown();
    };
    if base != want {
        return unknown();
    }
    let command = cmd.trim().to_string();
    Some(if !full.contains('/') {
        HookState::InstalledViaPath { command }
    } else if exists(&full) {
        HookState::InstalledAtPath {
            command,
            path: full,
        }
    } else {
        HookState::PathMissing {
            command,
            path: full,
        }
    })
}

/// 某个事件下调 `want` 的钩子。逐层容忍（坏条目跳过）；多条里能用的优先，其次 `PathMissing` / `Unknown`。
pub(crate) fn diagnose_event(
    root: &serde_json::Value,
    event: &str,
    want: &str,
    exists: &dyn Fn(&str) -> bool,
) -> HookState {
    let mut fallback: Option<HookState> = None;
    let entries = root
        .get("hooks")
        .and_then(|h| h.get(event))
        .and_then(|e| e.as_array());
    for entry in entries.into_iter().flatten() {
        let inner = entry.get("hooks").and_then(|h| h.as_array());
        for hk in inner.into_iter().flatten() {
            let Some(cmd) = hk.get("command").and_then(|c| c.as_str()) else {
                continue;
            };
            if cmd.trim().is_empty() {
                continue;
            }
            if let Some(st) = classify_command(cmd, want, exists) {
                if st.is_working() {
                    return st;
                }
                fallback.get_or_insert(st);
            }
        }
    }
    fallback.unwrap_or(HookState::NotInstalled)
}

/// 完整诊断。`raw` = settings.json 原文；读不到 / 坏 JSON / 顶层不是对象 ⇒ 两态皆 `NotInstalled` ＋ `note` 说原因。
pub(crate) fn diagnose(raw: Option<&str>, exists: &dyn Fn(&str) -> bool) -> HooksDiagnosis {
    let noted = |note: String| HooksDiagnosis {
        session_start: HookState::NotInstalled,
        stop: HookState::NotInstalled,
        note,
    };
    let Some(raw) = raw else {
        return noted(copy_text("rsHooksDiag.diagnose.noSettings", &[]));
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim_start_matches('\u{feff}'))
    else {
        return noted(copy_text("rsHooksDiag.diagnose.badJson", &[]));
    };
    if !v.is_object() {
        return noted(copy_text("rsHooksDiag.diagnose.notObject", &[]));
    }
    HooksDiagnosis {
        session_start: diagnose_event(&v, "SessionStart", PROGRAMS[0], exists),
        stop: diagnose_event(&v, "Stop", PROGRAMS[1], exists),
        note: String::new(),
    }
}

/// 待贴 JSON 片段（`home` = `$HOME/.local/bin/…` 显式路径形态；否则裸命令）；形态与实况**确定**冲突才带警示。
pub(crate) fn snippet(home: bool, probe: &SnippetProbe) -> Snippet {
    let (reg, stop) = if home {
        // 〔OSA · V156〕`"$HOME/…"` 那个词的写法住 `platform::shell::posix`。
        (
            format!(
                "{} >/dev/null 2>&1 || true",
                posix::home_path_word(".local/bin/cc-register")
            ),
            posix::home_path_word(".local/bin/cc-bus-stop-hook"),
        )
    } else {
        (
            "cc-register >/dev/null 2>&1 || true".to_string(),
            "cc-bus-stop-hook".to_string(),
        )
    };
    let text = format!(
        "{{\n  \"hooks\": {{\n    \"SessionStart\": [ {{ \"hooks\": [ {{ \"type\": \"command\",\n      \"command\": \"{}\" }} ] }} ],\n    \"Stop\": [ {{ \"hooks\": [ {{ \"type\": \"command\",\n      \"command\": \"{}\" }} ] }} ]\n  }}\n}}",
        reg.replace('"', "\\\""),
        stop.replace('"', "\\\"")
    );
    let warning = if home && probe.home_path_exists == Some(false) {
        Some(copy_text("rsHooksDiag.snippet.homeMissing", &[]))
    } else if !home && probe.on_path == Some(false) {
        Some(copy_text("rsHooksDiag.snippet.bareMissing", &[]))
    } else {
        None
    };
    Snippet { text, warning }
}

/// 按 `PATH` 逐目录反查一个裸命令在不在。切分走 `std::env::split_paths`（这台后端自己的平台规矩：
/// Windows `;`、POSIX `:`，T03 阻塞 1）。`PATH` 取不到 / 空 ⇒ `None`（不猜）。
pub(crate) fn resolves_on_path(
    prog: &str,
    path_env: Option<&str>,
    exists: &dyn Fn(&str) -> bool,
) -> Option<bool> {
    let pe = path_env?;
    if pe.trim().is_empty() {
        return None;
    }
    Some(std::env::split_paths(pe).any(|d| {
        let d = d.to_string_lossy();
        let d = d.trim_end_matches(['/', '\\']);
        !d.is_empty() && exists(&format!("{d}/{prog}"))
    }))
}

/// `$HOME/…` · `${HOME}/…` · `~/…` 按这台家目录展开；其余原样（B04-3：花括号那一形也要认）。
fn expand(s: &str, home: Option<&Path>) -> PathBuf {
    // 〔OSA · V156〕哪几种写法算「家目录底下」住 `platform::shell::posix`。
    match (posix::home_relative(s), home) {
        (Some(r), Some(h)) => h.join(r),
        _ => PathBuf::from(s),
    }
}

/// 帧面入口：这台后端进程的 `HOME`（缺 ⇒ `USERPROFILE`）· `PATH` · agent 配置根。
pub(crate) fn answer() -> HooksReport {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let home = env("HOME")
        .or_else(|| env("USERPROFILE"))
        .map(PathBuf::from);
    answer_at(
        home.as_deref(),
        &crate::observe::history_query::agent_home(),
        env("PATH").as_deref(),
    )
}

/// [`answer`] 的本体（家目录 · agent 配置根 · `PATH` 是参数，判据拿夹具喂）。
pub(crate) fn answer_at(
    home: Option<&Path>,
    agent_home: &Path,
    path_env: Option<&str>,
) -> HooksReport {
    let settings = agent_home.join("settings.json");
    // 超上限 / 不是普通文件 / 读不了 ⇒ 按「没读到」降级，`note` 说出来（`diagnose(None)`）。
    let raw = match std::fs::metadata(&settings) {
        Ok(meta) if meta.is_file() && meta.len() <= SETTINGS_CAP_BYTES => {
            std::fs::read_to_string(&settings).ok()
        }
        _ => None,
    };
    let exists = |s: &str| expand(s, home).exists();
    let probe = SnippetProbe {
        home_path_exists: home.map(|h| {
            PROGRAMS
                .iter()
                .all(|p| h.join(format!(".local/bin/{p}")).exists())
        }),
        // 任一取不到就整体说「不知道」。
        on_path: PROGRAMS
            .iter()
            .map(|p| resolves_on_path(p, path_env, &exists))
            .try_fold(true, |acc, r| r.map(|b| acc && b)),
    };
    HooksReport {
        diagnosis: diagnose(raw.as_deref(), &exists),
        snippet_home: snippet(true, &probe),
        snippet_bare: snippet(false, &probe),
        source: settings.display().to_string(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/cc_bus_hooks_tests.rs"]
mod tests;
