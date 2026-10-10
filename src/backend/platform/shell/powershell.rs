//! **PowerShell 那几句写法** —— 执行策略的读与设 · `--ccm-print` 在 Windows 上那一行的写法，只调这里。
//!
//! ⚠ 这一臂只到「编得过」：本机没有 PowerShell，这几句一次都没被 PowerShell 解析过。

use super::PsHost;
use crate::platform::child::Deadline;

// ─── 执行策略：块装进 `$PROFILE` 之前先问这一代 PowerShell 会不会加载它 ───

/// 生效的那一档 ＋ 组策略两档（有值 ⇒ 改当前用户那一档也没用）。三行，只读。
const POLICY_QUERY: &str =
    "Get-ExecutionPolicy; Get-ExecutionPolicy -Scope MachinePolicy; Get-ExecutionPolicy -Scope UserPolicy";

/// 标准做法那一下：只动当前用户那一档、只设成 `RemoteSigned`（本地脚本照跑，下载来的要签名）。
/// 只由 `powershell-policy-set` 跑 —— 用户点了、确认了之后（不代改）。
const POLICY_ALLOW_LOCAL: &str =
    "Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned -Force";

/// 这一代 PowerShell 的执行策略现状（`aliases-read` 每份 `$PROFILE` 候选带一份）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExecPolicy {
    pub host: PsHost,
    /// 生效那一档的原词（`Get-ExecutionPolicy`）；问不到 ⇒ `None`，原因词在 `error`。
    pub effective: Option<String>,
    /// 按这一档，这一代会不会加载本地、未签名的 profile；说不清 ⇒ `None`。
    pub loads: Option<bool>,
    /// 组策略（`MachinePolicy` / `UserPolicy`）钉着 ⇒ 改当前用户那一档没用。
    pub group_policy: bool,
    /// 问不到的原因词（原因词闭集里的一个；PowerShell / 系统的原话只记一行日志，不随应答走）。
    pub error: Option<String>,
}

/// 本地未签名的脚本（我们装进 `$PROFILE` 的块就是）在这一档下跑不跑。只认 PowerShell 自己的那几个词，别的 ⇒ 说不清。
/// `Undefined` / `Default` 的实际效果随 Windows 版本变（客户端 = `Restricted`，服务器 = `RemoteSigned`）⇒ 不猜。
pub(crate) fn policy_loads_local_script(policy: &str) -> Option<bool> {
    match policy.to_ascii_lowercase().as_str() {
        "restricted" | "allsigned" => Some(false),
        "remotesigned" | "unrestricted" | "bypass" => Some(true),
        _ => None,
    }
}

/// [`POLICY_QUERY`] 那三行 → 现状。行数不对 ⇒ 「内容无法解析」（那几行原话记一行日志）。
pub(crate) fn read_policy_listing(host: PsHost, text: &str) -> ExecPolicy {
    let words: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let [effective, machine, user] = words[..] else {
        tracing::warn!("powershell: policy listing unreadable: {:?}", text.trim());
        return policy_unknown(host, copy_core::copy_text("reason.content.unparsable", &[]));
    };
    let pinned = |w: &str| !w.eq_ignore_ascii_case("undefined");
    ExecPolicy {
        host,
        effective: Some(effective.to_string()),
        loads: policy_loads_local_script(effective),
        group_policy: pinned(machine) || pinned(user),
        error: None,
    }
}

fn policy_unknown(host: PsHost, error: String) -> ExecPolicy {
    ExecPolicy {
        host,
        effective: None,
        loads: None,
        group_policy: false,
        error: Some(error),
    }
}

// ─── 连接表 ＋ 进程表：↗ 那一问「这条连接是这台电脑上哪个进程开的、它往上是谁」───

/// 问 / 设执行策略那一趟的期限：界面等别名那一族的预算是 30 s，收一档到 15 s。
const POLICY_WITHIN: Deadline = Deadline::secs(15);

/// 起那一代跑一段固定脚本：`Ok(stdout)`；起不来 / 非零退出 / 过了期限 ⇒ `Err(原因词)`，原话（系统报错 · 退出码 ＋ stderr）
/// 在这里记一行日志、不随回。这台不说 PowerShell ⇒ 「未装」。
fn run_fixed(host: PsHost, script: &str, within: Deadline) -> Result<String, String> {
    let cmd = super::powershell_on(host, script)
        .ok_or_else(|| copy_core::copy_text("reason.spawn.notInstalled", &[]))?;
    let out = cmd.run(within).map_err(|e| {
        tracing::warn!("powershell: {host:?} did not run: {e}");
        match &e {
            crate::platform::child::ChildFail::NotFound(io)
            | crate::platform::child::ChildFail::Io(io) => copy_core::spawn_reason(io.kind()),
            crate::platform::child::ChildFail::TimedOut { .. } => {
                copy_core::io_reason(std::io::ErrorKind::TimedOut)
            }
        }
    })?;
    if !out.status.success() {
        tracing::warn!(
            "powershell: {host:?} exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return Err(copy_core::io_reason(std::io::ErrorKind::Other));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 现问这一代的执行策略（只读）。
pub(crate) fn execution_policy(host: PsHost) -> ExecPolicy {
    match run_fixed(host, POLICY_QUERY, POLICY_WITHIN) {
        Ok(text) => read_policy_listing(host, &text),
        Err(e) => policy_unknown(host, e),
    }
}

/// 设成当前用户 `RemoteSigned`（[`POLICY_ALLOW_LOCAL`]）再现问一次。设的那一下报错（组策略压着时 PowerShell 会报）⇒ 原因词随回（原话记日志）。
pub(crate) fn allow_local_scripts(host: PsHost) -> (ExecPolicy, Option<String>) {
    let set = run_fixed(host, POLICY_ALLOW_LOCAL, POLICY_WITHIN).err();
    (execution_policy(host), set)
}

// ─── 一行命令的 PowerShell 写法（`ccm --ccm-print` / 别名预览在 Windows 上吐的那一行）───
// 与 `posix.rs` 那几句一一对应；词由调用方先过 `dialect::ps_literal`。

/// `$env:VAR = <词>; `
pub(crate) fn set_env(var: &str, word: &str) -> String {
    format!("$env:{var} = {word}; ")
}

/// `Remove-Item Env:A, Env:B -ErrorAction Ignore; `（不在也不报）。
pub(crate) fn remove_env<S: AsRef<str>>(vars: &[S]) -> String {
    let names: Vec<String> = vars.iter().map(|v| format!("Env:{}", v.as_ref())).collect();
    format!("Remove-Item {} -ErrorAction Ignore; ", names.join(", "))
}

/// `Set-Location -LiteralPath <词>; `
pub(crate) fn set_location(dir_word: &str) -> String {
    format!("Set-Location -LiteralPath {dir_word}; ")
}

/// 跑那条命令：`& <词> <词> …`（第一个词是程序名）。
pub(crate) fn call<S: AsRef<str>>(words: &[S]) -> String {
    let w: Vec<&str> = words.iter().map(AsRef::as_ref).collect();
    format!("& {}", w.join(" "))
}

/// 一个词：`<head>` ＋ 家目录底下 `rel` 那份文件的内容（现读、去掉尾部换行）＋ `<tail>`（head / tail 已成词）。
pub(crate) fn home_file_between(head: &str, rel: &str, tail: &str) -> String {
    format!(
        "({head} + (Get-Content -Raw -LiteralPath (Join-Path $HOME {})).Trim() + {tail})",
        super::dialect::ps_literal(rel)
    )
}
