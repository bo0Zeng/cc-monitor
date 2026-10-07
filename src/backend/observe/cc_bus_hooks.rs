//! cc-bus 钩子诊断 —— 帧命令 `hooks-diag` 的本体（只读）。
//!
//! 这台后端读它自己那台的 agent 配置根下的 `settings.json`、就地 stat 钩子点名的程序，出整份成品
//! （诊断 ＋ 要加的内容 ＋ 读的是哪份文件）。要加的内容只有一形：两条钩子直接指向这台装好的 cc-bus 里的那两个脚本
//! （脚本靠 `readlink -f` 认自己的目录；不依赖 `PATH`）；cc-bus 没装 ⇒ 不给。本机远端同一条命令（`chan.call(origin, "hooks-diag")`），
//! 帧面宿主在顶层 `feature_face`。
//!
//! 判据不是字符串等值：`"$HOME/.local/bin/cc-register" >/dev/null 2>&1 || true` 与规范片段 `cc-register …` 功能等价、字符串不等
//! ⇒ 按「被执行的程序」判。看不懂的形态答「无法判断」，不猜「未装」。
//! 只读：不写 `settings.json`（用户的共享全局配置，由用户自己合并），只生成要加的内容。

use crate::platform::shell::posix;
use copy_core::copy_text;
use serde::Serialize;
use serde_json::json;
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
    /// 无法判断：命令里出现了目标程序名，但它不是被直接执行的那个（`sh -c` / `env` / `timeout` 包着）。猜「未装」和猜「已装」一样是猜。
    Unknown { command: String },
}

impl HookState {
    /// 只有这两种算「能用」；`PathMissing` 刻意不算。
    pub(crate) fn is_working(&self) -> bool {
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

/// 成品（形状与界面 `HooksReport` 逐字同；跨语言金样 `tests/__fixtures__/hooks-diag.golden.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HooksReport {
    /// 这台跑得了 cc-bus（它要 tmux）；跑不了 ⇒ 不给要加的内容，界面只说这台不支持自动收信。
    pub(crate) supported: bool,
    pub(crate) diagnosis: HooksDiagnosis,
    /// 要合并进那份 `settings.json` 的内容；这台的 cc-bus 没装（那两个脚本不在）⇒ `None`。
    pub(crate) snippet: Option<String>,
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

/// 要加的内容：两条钩子直接指向 `<skills 根>/cc-bus/scripts/` 里那两个脚本（家目录底下 ⇒ `"$HOME/…"`，否则绝对路径）。
/// 两个脚本有一个不在 ⇒ `None`（cc-bus 没装，先装）。
pub(crate) fn snippet(skills: &Path, home: Option<&Path>) -> Option<String> {
    let scripts: Vec<PathBuf> = PROGRAMS
        .iter()
        .map(|p| {
            skills
                .join(crate::assets::cc_bus_install::NAME)
                .join("scripts")
                .join(p)
        })
        .collect();
    if !scripts.iter().all(|p| p.is_file()) {
        return None;
    }
    let word = |p: &Path| match home.and_then(|h| p.strip_prefix(h).ok()) {
        Some(rel) => posix::home_path_word(&rel.to_string_lossy().replace('\\', "/")),
        None => shell_quote_core::posix_quote(&p.display().to_string()),
    };
    let v = json!({ "hooks": {
        "SessionStart": [ { "hooks": [ { "type": "command",
            "command": format!("{} >/dev/null 2>&1 || true", word(&scripts[0])) } ] } ],
        "Stop": [ { "hooks": [ { "type": "command", "command": word(&scripts[1]) } ] } ],
    } });
    serde_json::to_string_pretty(&v).ok()
}

/// `$HOME/…` · `${HOME}/…` · `~/…` 按这台家目录展开；其余原样。
fn expand(s: &str, home: Option<&Path>) -> PathBuf {
    // 哪几种写法算「家目录底下」住 `platform::shell::posix`。
    match (posix::home_relative(s), home) {
        (Some(r), Some(h)) => h.join(r),
        _ => PathBuf::from(s),
    }
}

/// 帧面入口：这台后端进程的家目录 · agent 配置根 · skills 根 · 这份二进制所在平台有没有原生 tmux。
pub(crate) fn answer() -> HooksReport {
    let home = crate::platform::paths::home_dir();
    answer_at(
        home.as_deref(),
        &crate::observe::history_query::agent_home(),
        crate::assets::asset_kind()
            .and_then(crate::agents::skills_root)
            .as_deref(),
        !matches!(
            crate::TMUX_PLATFORM,
            crate::TmuxPlatform::AbsentUnlessExeOnPath
        ),
    )
}

/// [`answer`] 的本体（家目录 · agent 配置根 · skills 根 · 跑不跑得了 cc-bus 是参数，判据拿夹具喂）。
pub(crate) fn answer_at(
    home: Option<&Path>,
    agent_home: &Path,
    skills: Option<&Path>,
    supported: bool,
) -> HooksReport {
    // 钩子写在 cc-bus 装进的那一家的用户级设置文件里（注册表足迹面 `user_settings` 的用户那一份）。
    let settings = crate::assets::asset_kind()
        .and_then(|k| crate::agents::user_settings_of(k, agent_home))
        .unwrap_or_default();
    // 超上限 / 不是普通文件 / 读不了 ⇒ 按「没读到」降级，`note` 说出来（`diagnose(None)`）。
    let raw = match std::fs::metadata(&settings) {
        Ok(meta) if meta.is_file() && meta.len() <= SETTINGS_CAP_BYTES => {
            std::fs::read_to_string(&settings).ok()
        }
        _ => None,
    };
    let exists = |s: &str| expand(s, home).exists();
    HooksReport {
        supported,
        diagnosis: diagnose(raw.as_deref(), &exists),
        snippet: skills.filter(|_| supported).and_then(|k| snippet(k, home)),
        source: settings.display().to_string(),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/cc_bus_hooks_tests.rs"]
mod tests;
