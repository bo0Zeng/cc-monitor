//! **PowerShell 那几句写法** —— 执行策略的读与设 · 连接表与进程表的现问，只调这里。
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

// ─── 连接表 ＋ 进程表：↗ 那一问「这条连接是这台电脑上哪个进程开的、它往上是谁」───

/// 一趟固定脚本、不吃任何输入：已建立的 TCP 连接（两端地址与端口 ＋ 拥有者进程号）＋ 进程表**只取四格**
/// （进程号 · 父进程号 · 名字 · 启动时刻，后者换成 FILETIME；`-Property` 只向系统要这四格）。打成一行 JSON。
const CONN_PROC_QUERY: &str = "$ErrorActionPreference = 'Stop'; \
$t = @(Get-NetTCPConnection -State Established | ForEach-Object { [pscustomobject]@{ la = $_.LocalAddress; lp = $_.LocalPort; ra = $_.RemoteAddress; rp = $_.RemotePort; pid = $_.OwningProcess } }); \
$p = @(Get-CimInstance -ClassName Win32_Process -Property ProcessId, ParentProcessId, Name, CreationDate | ForEach-Object { [pscustomobject]@{ pid = $_.ProcessId; ppid = $_.ParentProcessId; name = $_.Name; start = $(if ($_.CreationDate) { $_.CreationDate.ToFileTimeUtc() } else { 0 }) } }); \
[pscustomobject]@{ tcp = $t; proc = $p } | ConvertTo-Json -Depth 3 -Compress";

/// 现问一次连接表与进程表（只读，5.1 那一代）：`Ok(那一行 JSON)`；这台没有 PowerShell / 起不来 / 报错 ⇒ `Err(原话)`。
pub(crate) fn connection_and_process_tables() -> Result<String, String> {
    run_fixed(PsHost::Desktop, CONN_PROC_QUERY, CONN_PROC_WITHIN)
}

/// 问连接表 · 进程表那一趟的期限：界面等 `terminal-processes` 的预算是 15 s，收一档到 10 s。
const CONN_PROC_WITHIN: Deadline = Deadline::secs(10);
/// 问 / 设执行策略那一趟的期限：界面等别名那一族的预算是 30 s，收一档到 15 s。
const POLICY_WITHIN: Deadline = Deadline::secs(15);

/// 起那一代跑一段固定脚本：`Ok(stdout)`；起不来 / 非零退出 / 过了期限 ⇒ `Err(原话)`。这台不说 PowerShell ⇒ `Err`。
fn run_fixed(host: PsHost, script: &str, within: Deadline) -> Result<String, String> {
    let cmd = super::powershell_on(host, script)
        .ok_or_else(|| copy_core::copy_text("rsShellDialect.ps.noPowerShellHere", &[]))?;
    let out = cmd.run(within).map_err(|e| e.to_string())?;
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
    match run_fixed(host, POLICY_QUERY, POLICY_WITHIN) {
        Ok(text) => read_policy_listing(host, &text),
        Err(e) => policy_unknown(host, e),
    }
}

/// 设成当前用户 `RemoteSigned`（[`POLICY_ALLOW_LOCAL`]）再现问一次。设的那一下报错（组策略压着时 PowerShell 会报）⇒ 原话随回。
pub(crate) fn allow_local_scripts(host: PsHost) -> (ExecPolicy, Option<String>) {
    let set = run_fixed(host, POLICY_ALLOW_LOCAL, POLICY_WITHIN).err();
    (execution_policy(host), set)
}
