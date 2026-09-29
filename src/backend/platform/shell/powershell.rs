//! 〔OSA · `设计/99 §1` V156〕**PowerShell 那几句写法** —— 本机（Windows）起会话载荷与中转前缀只调这里
//! （原各自在 `control/launch_render/{payload,local}.rs` 里手写，逐字搬来，产出逐字节不变）。
//!
//! ⚠ 这一臂只到「编得过」：本机没有 PowerShell，这几句一次都没被 PowerShell 解析过（`K-H2` 那条登记原样延续）。
//! 〔WF1 · M/N〕值进单引号一律经 [`ps_literal`]（唯一出口，认全 PowerShell 的五个引号字符）。

use super::dialect::ps_literal;
use super::PsHost;

/// `$env:VAR='<值>'; `
pub(crate) fn set_env(var: &str, value: &str) -> String {
    format!("$env:{var}={}; ", ps_literal(value))
}

/// `$env:VAR=$null; `（清掉这个变量）。
pub(crate) fn clear_env(var: &str) -> String {
    format!("$env:{var}=$null; ")
}

/// 有那条命令 ⇒ 跑 `then`，否则跑 `otherwise`。
pub(crate) fn if_command(name: &str, then: &str, otherwise: &str) -> String {
    format!(
        "if (Get-Command {name} -ErrorAction SilentlyContinue) {{ {then} }} else {{ {otherwise} }}"
    )
}

/// `VAR` 已经有值 ⇒ 说一句 `say`（原话）、不动它；否则设成 `expr`（一个 PowerShell 表达式）。
pub(crate) fn set_unless_set(var: &str, expr: &str, say: &str) -> String {
    let say = ps_literal(say);
    format!("if ($env:{var}) {{ Write-Host {say} }} else {{ $env:{var}={expr} }}; ")
}

/// 一个表达式：`'<head>'` ＋ 家目录底下 `rel` 那份文件的内容（现读、去首尾空白）＋ `'<tail>'`。
/// PowerShell 的 `$HOME` 即 `USERPROFILE`（与后端 `door::key_path` 的退路同一个）。
pub(crate) fn home_file_between(head: &str, rel: &str, tail: &str) -> String {
    let (head, rel, tail) = (ps_literal(head), ps_literal(rel), ps_literal(tail));
    format!("{head} + (Get-Content -Raw -LiteralPath (Join-Path $HOME {rel})).Trim() + {tail}")
}

// ─── 〔WF1 · L · `设计/99 §2.3`〕执行策略：块装进 `$PROFILE` 之前先问这一代 PowerShell 会不会加载它 ───

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
    /// 生效那一档的原词（`Get-ExecutionPolicy`）；问不到 ⇒ `None`，原话在 `error`。
    pub effective: Option<String>,
    /// 按这一档，这一代会不会加载本地、未签名的 profile；说不清 ⇒ `None`。
    pub loads: Option<bool>,
    /// 组策略（`MachinePolicy` / `UserPolicy`）钉着 ⇒ 改当前用户那一档没用。
    pub group_policy: bool,
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

/// [`POLICY_QUERY`] 那三行 → 现状。行数不对 ⇒ 说认不出（原话带上）。
pub(crate) fn read_policy_listing(host: PsHost, text: &str) -> ExecPolicy {
    let words: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let [effective, machine, user] = words[..] else {
        return policy_unknown(
            host,
            copy_core::copy_text(
                "rsPsPolicy.listing.unreadable",
                &[("out", &text.trim().to_string())],
            ),
        );
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

/// 起那一代跑一段固定脚本：`Ok(stdout)`；起不来 / 非零退出 ⇒ `Err(原话)`。这台不说 PowerShell ⇒ `Err`。
fn run_fixed(host: PsHost, script: &str) -> Result<String, String> {
    let mut cmd = super::powershell_on(host, script)
        .ok_or_else(|| copy_core::copy_text("rsShellDialect.ps.noPowerShellHere", &[]))?;
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 现问这一代的执行策略（只读）。
pub(crate) fn execution_policy(host: PsHost) -> ExecPolicy {
    match run_fixed(host, POLICY_QUERY) {
        Ok(text) => read_policy_listing(host, &text),
        Err(e) => policy_unknown(host, e),
    }
}

/// 设成当前用户 `RemoteSigned`（[`POLICY_ALLOW_LOCAL`]）再现问一次。设的那一下报错（组策略压着时 PowerShell 会报）⇒ 原话随回。
pub(crate) fn allow_local_scripts(host: PsHost) -> (ExecPolicy, Option<String>) {
    let set = run_fixed(host, POLICY_ALLOW_LOCAL).err();
    (execution_policy(host), set)
}
