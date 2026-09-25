//! 〔`A3` 第二波〕**这台机器上的 `cc-acct-iso`** —— 两条一次性查询：
//!
//! | 子命令 | 答什么 | 远端那一侧今天怎么答（对照） |
//! |---|---|---|
//! | `--acct-iso-status` | 装没装、装在哪 | monitor 经 SSH 跑 `PATH="$HOME/.local/bin:$PATH" command -v cc-acct-iso` |
//! | `--acct-iso-shellinit` | `cc-acct-iso shellinit` 吐的那段 rc 片段（原样） | monitor 经 SSH 跑 `cc-acct-iso shellinit` |
//!
//! # 为什么住账号域、不住 `observe/`
//!
//! 用户逐字：「**中转层不要有账号, 账号就账号中转就中转**」—— 账号的事归本层（`accounts/`），
//! 不往 `relay/` 里塞。而 `observe/` 是**只读层、不起进程**（`layering_guard` 那条
//! 「观测层伸手进 plugin 就是只读层开始起进程」逐字禁）；`shellinit` 那一问**要起一次**
//! `cc-acct-iso`，所以它不能住那里。
//!
//! # 为什么起它、不在这里重写一份
//!
//! 片段的形态（默认号那行 `export` ＋ 每账号一个 `<名>cc()` ＋ 账号 0 的逃生口）是
//! **`cc-acct-iso` 的知识**。在这里照抄一份就多一个跨语言双写点 —— 与远端那一侧
//! `acct_iso_deploy.rs::remote_acct_iso_shellinit` 头注那条理由逐字相同：单一来源留在 bash。
//!
//! # 起进程只走插件通用调用口（`plugin::discover::find` ＋ `plugin::invoke::run`）
//!
//! 不另开一处 `Command::new`：`plugin/invoke.rs` 是全 crate 唯一一处起插件的口，
//! 期限靠 `timeout(1)` 前缀交给子进程（本层零计时器）、环境是白名单（`HOME` / `PATH` 在内，
//! `cc-acct-iso` 读 `~/.cc-acct-iso/config` 与 manifest 要它们）。
//! 被起的那一条 `cc-acct-iso shellinit` **只读**：`cmd_shellinit` 全是 `printf`，
//! 不写任何文件（vendored 那份逐行可查，`src/bridge/vendor/cc-acct-iso/scripts/cc-acct-iso`）。
//!
//! # 诚实边界
//!
//! - 「装没装」按**可执行文件在不在**判（`plugin::discover::is_executable`）——
//!   与远端 `command -v` 同一个意思，但**非 unix 上它恒答「没有」**（那一格的理由住
//!   `is_executable` 头注：Windows 上按扩展名判是另一份真实现，今天没有）。
//!   `cc-acct-iso` 本来就是 bash 脚本，Windows 上没有它的装法 ⇒ 这个「没有」是真话。
//! - 本层**不校验**片段的 BEGIN/END 围栏：围栏常量住 monitor 那一侧（有对拍 vendored 脚本的判据），
//!   在这里再写一份就是第二个双写点。⇒ 原样吐出，校验归 monitor（`local_accounts.rs`）。

use crate::plugin::invoke::Done;
use crate::plugin::invoke::NotRun;
use std::path::{Path, PathBuf};

/// 那个工具的命令名。
pub(crate) const TOOL: &str = "cc-acct-iso";

/// `shellinit` 的期限（秒，交给子进程的 `timeout` 前缀）。它只是读 manifest ＋ `printf`，
/// 秒级完成；给足冗余，只防它卡死把一次性查询挂住。
const SHELLINIT_DEADLINE_SECS: u64 = 20;

/// 找它时的固定候选：**`$HOME/.local/bin/cc-acct-iso`**（`cc-acct-iso-install.sh` 的软链落点）。
///
/// 远端那条把 `~/.local/bin` **前置**到 `PATH` 再 `command -v` —— 先查它、再查 `PATH`，
/// 就是同一个顺序。`HOME` 拿不到 ⇒ 没有固定候选，只剩 `PATH`。
pub(crate) fn fixed_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    home.map(|h| vec![h.join(".local").join("bin").join(TOOL)])
        .unwrap_or_default()
}

fn locate() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    crate::plugin::discover::find(
        TOOL,
        &fixed_candidates(home.as_deref()),
        true,
        "这台机器上还没装 cc-acct-iso。",
    )
}

/// `--acct-iso-status` 的那一行 —— **纯函数**。
///
/// 没找到**不是错误**（exit 0）：「没装」是一个确定的答案，与远端那条 `installed:false` 同形。
/// `looked` 只在没找到时带：说清查过哪儿（`plugin::discover::not_installed_message` 那句话）。
pub(crate) fn status_line(found: &Result<PathBuf, String>) -> String {
    match found {
        Ok(p) => serde_json::json!({
            "installed": true,
            "path": p.display().to_string(),
            "looked": null,
        }),
        Err(looked) => serde_json::json!({
            "installed": false,
            "path": null,
            "looked": looked,
        }),
    }
    .to_string()
}

/// `--acct-iso-shellinit` 跑完之后怎么交差 —— **纯函数**：`Ok` = 原样吐到 stdout；
/// `Err((code, message))` = stderr 一行结构化 JSON ＋ exit 2（同 `--account-trust` 的约定）。
///
/// ⚠ 码 → 语义的映射住**调用方这一侧**（`plugin` 层逐字：同一个码在不同插件里语义互斥）。
/// 这里只认一条：`0` 才算产出了片段。`cc-acct-iso` 的 warn（比如「manifest 里没有默认账号」）
/// 走 stderr、退出码仍是 0 ⇒ 那一类照样交出片段，由 monitor 那侧按围栏判它完不完整。
pub(crate) fn shellinit_outcome(
    ran: Result<Done, NotRun>,
) -> Result<String, (&'static str, String)> {
    match ran {
        Ok(done) if done.code == Some(0) => Ok(String::from_utf8_lossy(&done.stdout).into_owned()),
        Ok(done) if done.timed_out() => Err((
            "timed_out",
            format!("{TOOL} shellinit 超过 {SHELLINIT_DEADLINE_SECS} 秒没有返回"),
        )),
        Ok(done) => Err((
            "tool_failed",
            format!(
                "{TOOL} shellinit 没有产出片段（退出码 {:?}）：{}",
                done.code,
                done.diagnosis()
            ),
        )),
        Err(NotRun::ArgListTooLong) => {
            Err(("not_run", format!("{TOOL} shellinit 没能起来：参数过长")))
        }
        Err(NotRun::Failed(why)) => Err(("not_run", why)),
    }
}

/// 一次查询的**答案**：退出码 ＋ 要写到 stdout / stderr 的字节。
///
/// ⚠ **本层不自己 `print`**：`accounts/` 里的上游选择同时挂在 `--relay` 进程上，它的每一条日志都在
/// `relay::creds_guard` 的白名单底下（前缀 `[apikey]`、插值逐项登记 —— 防 key 漏进日志）。
/// 查询的输出不是日志、也不该带那个前缀 ⇒ 这里只**产出**答案，写出去的那一下归进程入口
/// （`main.rs` 的 `emit_answer`，与 `std::process::exit` 同一处）。
pub struct Answer {
    pub code: i32,
    pub stdout: String,
    pub stderr: Option<String>,
}

/// 查询模式入口。退出码约定同 `observe::accounts_query::run`（0 ok / 2 err），
/// 失败时 stderr 是一行结构化 `{code,message}`（同 `--account-trust`）。
pub fn answer(args: &[String]) -> Answer {
    let fail = |code: &str, message: String| Answer {
        code: 2,
        stdout: String::new(),
        stderr: Some(serde_json::json!({"code": code, "message": message}).to_string()),
    };
    match args.first().map(String::as_str) {
        Some("--acct-iso-status") => Answer {
            code: 0,
            stdout: format!("{}\n", status_line(&locate())),
            stderr: None,
        },
        Some("--acct-iso-shellinit") => {
            let got = match locate() {
                Err(looked) => Err(("not_installed", looked)),
                Ok(bin) => shellinit_outcome(crate::plugin::invoke::run(
                    &bin,
                    &["shellinit"],
                    SHELLINIT_DEADLINE_SECS,
                    &[],
                )),
            };
            match got {
                Ok(snippet) => Answer {
                    code: 0,
                    stdout: snippet,
                    stderr: None,
                },
                Err((code, message)) => fail(code, message),
            }
        }
        other => fail("bad_args", format!("unknown argument: {other:?}")),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/accounts/iso_tests.rs"]
mod tests;
