//! Codex 一次性跑一件活那一形（`LaunchFace.task`）：`codex exec`，结果写进交给它的结果文件。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`，只读源码、没起真 codex）：
//! - 起会话那一行垫在最前面的 `--no-daemon` 在 `exec` 之前照收（顶层命令行摊平了交互那一组参数），`exec` 那一支不看它也不拒。
//! - 不在 git 仓里时 `exec` 不带 `--skip-git-repo-check` 就打 `Not inside a trusted directory …` 退 1；派活的目录不保证是仓 ⇒ 带上。
//!   它不放开审批与沙箱，`exec` 照它自己的默认档跑。
//! - `--output-last-message <文件>`：最后那段话写进那个文件；说明是位置参数 `PROMPT`，放在 `--` 之后（以 `-` 开头也不被认成旗标）。

/// 一件活交给 `codex` 的那几个参数：`exec --skip-git-repo-check --output-last-message <结果文件> -- <说明>`。
pub(crate) fn task_args(say: &str, out: &str) -> Vec<String> {
    [
        "exec",
        "--skip-git-repo-check",
        "--output-last-message",
        out,
        "--",
        say,
    ]
    .map(str::to_string)
    .to_vec()
}

/// 注册表那一行的 `task` 那一格：结果从结果文件读。
pub(crate) const TASK: crate::agents::TaskFace = crate::agents::TaskFace {
    args: task_args,
    text: crate::agents::TaskText::File,
};

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/exec_tests.rs"]
mod tests;
