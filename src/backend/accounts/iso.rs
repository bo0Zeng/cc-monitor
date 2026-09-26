//! 〔`A3` 第二波〕**这台机器上的 `cc-acct-iso`** —— 两条查询（〔LOC1a · 第四波 4D〕帧命令，CLI 面自动派生同名 `--…`）：
//!
//! | 帧命令 | 答什么 | 远端那一侧今天怎么答（对照） |
//! |---|---|---|
//! | `acct-iso-status` | 装没装、装在哪 | monitor 经 SSH 跑 `PATH="$HOME/.local/bin:$PATH" command -v cc-acct-iso` |
//! | `acct-iso-shellinit` | `cc-acct-iso shellinit` 吐的那段 rc 片段（原样） | monitor 经 SSH 跑 `cc-acct-iso shellinit` |
//! | `acct-iso-cmd`（〔DUP2 · J4〕） | 一个部署 / 维护步骤在终端里要跑的那**一行**（已校验、已 quote；**只出这一行，不起进程、不碰盘**） | 同一条（本机远端都问那台自己的后端） |
//!
//! 〔LOC1a〕此前是两条 argv 形一次性子命令，唯一调用方是 monitor 每问 exec 一次本机后端（`local_query`〔散文墓碑〕，删了）；
//! 本机那两问改走 `<local>` 长连接之后 argv 形那两臂零调用方 ⇒ 退役，只留帧面这一份（`设计/05 §14.6`）。
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
use copy_core::copy_text;
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
        &copy_text("beIso.locate.notInstalled", &[]),
    )
}

/// `acct-iso-status` 的 `data` —— **纯函数**。
///
/// 没找到**不是错误**（exit 0）：「没装」是一个确定的答案，与远端那条 `installed:false` 同形。
/// `looked` 只在没找到时带：说清查过哪儿（`plugin::discover::not_installed_message` 那句话）。
pub(crate) fn status_value(found: &Result<PathBuf, String>) -> serde_json::Value {
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
}

/// `acct-iso-shellinit` 跑完之后怎么交差 —— **纯函数**：`Ok` = 原样交出片段；
/// `Err((code, message))` = 那条帧命令的失败码。
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
            copy_text(
                "beIso.shellinitOutcome.timedOut",
                &[
                    ("tool", &TOOL.to_string()),
                    ("secs", &SHELLINIT_DEADLINE_SECS.to_string()),
                ],
            ),
        )),
        Ok(done) => Err((
            "tool_failed",
            copy_text(
                "beIso.shellinitOutcome.noOutput",
                &[
                    ("tool", &TOOL.to_string()),
                    ("exit", &format!("{:?}", done.code)),
                    ("detail", &(done.diagnosis()).to_string()),
                ],
            ),
        )),
        Err(NotRun::ArgListTooLong) => Err((
            "not_run",
            copy_text(
                "beIso.shellinitOutcome.argsTooLong",
                &[("tool", &TOOL.to_string())],
            ),
        )),
        Err(NotRun::Failed(why)) => Err(("not_run", why)),
    }
}

/// 找到它、起一次 `shellinit`、按 [`shellinit_outcome`] 交差。
fn shellinit_now() -> Result<String, (&'static str, String)> {
    match locate() {
        Err(looked) => Err(("not_installed", looked)),
        Ok(bin) => shellinit_outcome(crate::plugin::invoke::run(
            &bin,
            &["shellinit"],
            SHELLINIT_DEADLINE_SECS,
            &[],
        )),
    }
}

/// 〔LOC1a · 第四波 4D〕帧面 `acct-iso-status → {installed, path, looked}`（`设计/05 §14.6`：本机那几问从
/// 「exec 一次性本机后端」改走 `<local>` 长连接）。从不报错 ——「没装」是一个答案。CLI 面由帧面自动派生（同名 `--acct-iso-status`）。
pub(crate) fn answer_wire_status() -> Result<serde_json::Value, (&'static str, String)> {
    Ok(status_value(&locate()))
}

/// 〔LOC1a · 第四波 4D〕帧面 `acct-iso-shellinit → {snippet}`：片段原样（围栏校验仍归 monitor，理由见模块头注「诚实边界」）。
pub(crate) fn answer_wire_shellinit() -> Result<serde_json::Value, (&'static str, String)> {
    shellinit_now().map(|snippet| serde_json::json!({ "snippet": snippet }))
}

// ═══════════════════════════════════════════════════════════════════════════
// 〔DUP2 · 主会话 09-26 裁 J4〕`acct-iso-cmd`：**后端出这条命令，界面的预览与弹终端都经 `chan.call` 问它**。
//
// 此前这一行在界面里拼（`settings/acct-deploy.ts` 的一份 POSIX 单引号 ＋ 一个命令构造器 ＋ 一道快照路径校验）——
// `设计/01 §1.1`「命令串……都不在前端」· `设计/90 §3` 判据 2（那份单引号是 `shell_quote_core::posix_quote` 的第二份）。
// 先例：`ccm-print`（别名预览经通道问后端，W5-ALIAS）。
//
// 形状：入 `{step, name?, credFile?}`（键集合按 step 严格收）→ 回 `{cmd}`。只出这一行：**不起进程、不碰盘**
// （跑它的是用户自己面前的那个终端 —— DESIGN §6：动凭据的一切走终端、用户看着跑）。
// 两层校验照原样搬（规则不变，只换住址）：账号名过 `shell_quote_core::account_name_ok`（J18 那一份，与建号工具逐字同 ——
// 原先界面那份比它宽）；快照路径 = 非空 · 无 `"` · 无 C0 / DEL / C1 · 不以 `-` 开头（`-` 开头会被 cc-acct-iso 当成选项，
// 单引号挡不住选项解析；`"` 与控制符是 monitor `launch.rs` 远端那条命令的拒收面）；然后唯一的 quote。
// ═══════════════════════════════════════════════════════════════════════════

/// `acct-iso-cmd` 入参里快照路径最长多少字节（表单产出的远小于它）。超了 ⇒ 契约错（`bad_args`）。
pub(crate) const CMD_MAX_PATH_BYTES: usize = 4096;

/// 一个部署 / 维护步骤（线上 `step` 那一格的七个取值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// `init <名>`：dry-run，零落盘。
    InitPreview(String),
    /// `init <名> --apply`：落盘迁移（用户在终端里看着跑）。
    InitApply(String),
    Verify,
    Shellinit,
    /// `sync --apply`。
    SyncApply,
    /// `add <名> [--from-credentials <快照>] --apply`。
    AddApply {
        name: String,
        cred_file: Option<String>,
    },
    /// `run <名>`：这个号唯一的登录入口（去 `/login`）。
    Login(String),
}

fn malformed(detail: &str) -> (&'static str, String) {
    ("bad_args", crate::common::contract::malformed(detail))
}

/// 入参 ⇒ [`Step`]（**纯函数**）。形状不对（不认识的 step · 该有的格没有 · 多格 · 类型不对 · 超上界）⇒ `bad_args`（契约错，英文诊断）；
/// 账号名 / 快照路径过不了 ⇒ `refused`（句子走表：用户真填得出来）。
pub(crate) fn parse_cmd_args(args: &serde_json::Value) -> Result<Step, (&'static str, String)> {
    let obj = args
        .as_object()
        .ok_or_else(|| malformed("args must be an object {step, name?, credFile?}"))?;
    let step = obj
        .get("step")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| malformed("missing `step` (a string)"))?;
    let (needs_name, may_cred) = match step {
        "init-preview" | "init-apply" | "login" => (true, false),
        "add-apply" => (true, true),
        "verify" | "shellinit" | "sync-apply" => (false, false),
        other => return Err(malformed(&format!("unknown `step`: {other:?}"))),
    };
    for k in obj.keys() {
        let known = k == "step" || (k == "name" && needs_name) || (k == "credFile" && may_cred);
        if !known {
            return Err(malformed(&format!(
                "unexpected field `{k}` with step {step:?}"
            )));
        }
    }
    let name = if needs_name {
        let n = obj
            .get("name")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| malformed(&format!("step {step:?} needs `name` (a string)")))?;
        if !shell_quote_core::account_name_ok(n) {
            return Err((
                "refused",
                copy_text(
                    "beIso.cmd.badName",
                    &[
                        ("name", &format!("{n:?}")),
                        ("max", &shell_quote_core::ACCOUNT_NAME_MAX.to_string()),
                    ],
                ),
            ));
        }
        n.to_string()
    } else {
        String::new()
    };
    let cred_file = match obj.get("credFile") {
        None => None,
        Some(v) => {
            let p = v
                .as_str()
                .ok_or_else(|| malformed("`credFile` must be a string"))?;
            if p.len() > CMD_MAX_PATH_BYTES {
                return Err(malformed(&format!(
                    "`credFile` is too long: {} bytes (max {CMD_MAX_PATH_BYTES})",
                    p.len()
                )));
            }
            check_snapshot_path(p)?;
            Some(p.to_string())
        }
    };
    Ok(match step {
        "init-preview" => Step::InitPreview(name),
        "init-apply" => Step::InitApply(name),
        "login" => Step::Login(name),
        "add-apply" => Step::AddApply { name, cred_file },
        "verify" => Step::Verify,
        "shellinit" => Step::Shellinit,
        _ => Step::SyncApply,
    })
}

/// 快照路径那道校验（规则逐条照原样搬，见本节头注）。
fn check_snapshot_path(p: &str) -> Result<(), (&'static str, String)> {
    let shown = format!("{p:?}");
    let said = if p.is_empty() {
        copy_text("beIso.cmd.snapshotEmpty", &[])
    } else if p.contains('"') {
        copy_text("beIso.cmd.snapshotQuote", &[("path", &shown)])
    } else if p
        .chars()
        .any(|c| matches!(c, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}'))
    {
        copy_text("beIso.cmd.snapshotControl", &[("path", &shown)])
    } else if p.starts_with('-') {
        copy_text("beIso.cmd.snapshotDash", &[("path", &shown)])
    } else {
        return Ok(());
    };
    Err(("refused", said))
}

/// [`Step`] ⇒ 终端里那一行（**纯函数**；值一律过唯一的 quote `shell_quote_core::posix_quote`）。
///
/// ⚠ 刻意**逐处直呼** `shell_quote_core::posix_quote(…)`、不起一个局部别名：`INVARIANTS §47` 的人群判据
/// （`lib_invariant_population_tests.rs` 的 `QUOTE_SITES`）按 `posix_quote(` 的调用形数拼接点，局部别名它看不见。
pub(crate) fn render_cmd(step: &Step) -> String {
    match step {
        Step::InitPreview(n) => format!("{TOOL} init {}", shell_quote_core::posix_quote(n)),
        Step::InitApply(n) => format!("{TOOL} init {} --apply", shell_quote_core::posix_quote(n)),
        Step::Verify => format!("{TOOL} verify"),
        Step::Shellinit => format!("{TOOL} shellinit"),
        Step::SyncApply => format!("{TOOL} sync --apply"),
        Step::AddApply { name, cred_file } => match cred_file {
            Some(p) => format!(
                "{TOOL} add {} --from-credentials {} --apply",
                shell_quote_core::posix_quote(name),
                shell_quote_core::posix_quote(p)
            ),
            None => format!("{TOOL} add {} --apply", shell_quote_core::posix_quote(name)),
        },
        Step::Login(n) => format!("{TOOL} run {}", shell_quote_core::posix_quote(n)),
    }
}

/// 〔DUP2 · J4〕帧面 `acct-iso-cmd → {cmd}`（CLI 面由帧面自动派生，同名 `--acct-iso-cmd`，入参从 stdin 读）。
pub(crate) fn answer_wire_cmd(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (&'static str, String)> {
    let step = parse_cmd_args(args)?;
    Ok(serde_json::json!({ "cmd": render_cmd(&step) }))
}

#[cfg(test)]
#[path = "../../../tests/backend/accounts/iso_tests.rs"]
mod tests;
