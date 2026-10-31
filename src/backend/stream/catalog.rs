//! 列全部命令与用途的那个口：`ccm -- --commands`。
//!
//! 料全是现成的、一处不另抄：帧命令 ⇒ [`crate::stream::inbound::REGISTRY`]（名字 · 一句话 · 收不收参数）；
//! 上不上 CLI 面 ⇒ [`crate::stream::cli::cli_exposed`]；CLI 独有的那几条 ⇒ [`crate::stream::oneshot::ONESHOTS`]。
//! 读它的：这个子命令本身（随二进制走，装了 ccm 的机器上都问得到）· 协议参考 `IPC-COMMANDS.md` 的 CLI 那一节 ·
//! skill 说明书里的命令段（两份都是测试里照 [`catalog`] 现打的生成物）。
//! 口列的命令集 ＝ 分派表（帧面 `REGISTRY` · CLI 面 `stream::cli::is_subcommand`）两向相等，判据住测试那一侧。

use serde::Serialize;
use std::io::Write;

/// 一条命令：帧命令 `cmd` 与子命令 `cli` 至少有一样（只在流上的没有 `cli`，CLI 独有的没有 `cmd`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Entry {
    /// 帧命令名（流上 `{cmd}` 的那个）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cmd: Option<&'static str>,
    /// 一次性子命令（`ccm -- <cli>`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cli: Option<String>,
    /// CLI 独有那几条在子命令后面跟的词。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) usage: Option<&'static str>,
    /// 一句话：这条命令做什么。
    pub(crate) what: &'static str,
    /// 收不收参数（一段 JSON，CLI 面从 stdin 读）；CLI 独有那几条照 `usage`，不带这一格。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) args: Option<bool>,
}

/// 全部命令：帧命令照 `REGISTRY` 的先后，接着 CLI 独有的照 `ONESHOTS` 的先后。
pub(crate) fn catalog() -> Vec<Entry> {
    let frame = crate::stream::inbound::REGISTRY.iter().map(|s| Entry {
        cmd: Some(s.name),
        cli: crate::stream::cli::cli_exposed(s).then(|| crate::stream::cli::flag_of(s.name)),
        usage: None,
        what: s.summary.trim_end_matches('。'),
        args: Some(s.takes_input),
    });
    let cli_only = crate::stream::oneshot::ONESHOTS
        .iter()
        .map(|(f, usage, what, _)| Entry {
            cmd: None,
            cli: Some(f.to_string()),
            usage: (!usage.is_empty()).then_some(*usage),
            what,
            args: None,
        });
    frame.chain(cli_only).collect()
}

/// `--commands`：stdout 一行 `{commands: [...]}`、退出 0。
pub fn run() -> i32 {
    run_to(&mut std::io::stdout())
}

pub(crate) fn run_to(out: &mut dyn Write) -> i32 {
    let line = serde_json::json!({ "commands": catalog() });
    match writeln!(out, "{line}") {
        Ok(()) => 0,
        Err(_) => 2,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/stream/catalog_tests.rs"]
mod tests;
