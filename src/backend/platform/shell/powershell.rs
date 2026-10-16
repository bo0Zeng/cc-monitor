//! **PowerShell 那几句写法** —— 执行策略的读与设 · `--ccm-print` 在 Windows 上那一行的写法，只调这里。
//!
//! ⚠ 这一臂只到「编得过」：本机没有 PowerShell，这几句一次都没被 PowerShell 解析过。

use super::PsHost;
use crate::common::said::Said;
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        export_to = "../../frontend/ui/generated/",
        rename = "PsExecPolicy"
    )
)]
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
    run_said(cmd, within).map_err(|s| {
        tracing::warn!(
            "powershell: {host:?} did not run: {}",
            s.raw.as_deref().unwrap_or(&s.said)
        );
        s.said
    })
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

// ─── 新开一个终端时的 `PATH`（`platform::shell::fresh_terminal_path` 用） ───

/// 注册表里机器级 ＋ 用户级拼成的 `PATH`，夹在一对 `mark` 中间。自己编成 UTF-8 字节直写标准输出流：
/// 不经控制台编码（中文系统是 936）⇒ `PATH` 里有非 ASCII 目录时读回不坏。
#[cfg(any(not(unix), test))]
pub(crate) fn fresh_path_query(mark: &str) -> String {
    let m = super::dialect::ps_literal(mark);
    format!(
        "$t = (@([Environment]::GetEnvironmentVariable('Path', 'Machine'), [Environment]::GetEnvironmentVariable('Path', 'User')) | Where-Object {{ $_ }}) -join ';'\n\
$b = [Text.Encoding]::UTF8.GetBytes({m} + $t + {m})\n\
$o = [Console]::OpenStandardOutput()\n\
$o.Write($b, 0, $b.Length)\n\
$o.Flush()\n"
    )
}

// ─── 用户级 `PATH`：我们那个目录在不在上面 · 加 · 撤（`ccm-user-path` 用）───
//
// 用户级那一档 cmd · PowerShell · Git Bash 三种终端都读；只动它（`'Machine'` 那一档要管理员，卸载时的垃圾是全机的）。
// 加 / 撤两段也原样给人看（点开关跑的与复制去跑的是同一份字节）。两个老地雷都绕开：
// 不用 `setx`（值截断在 1024 字符、退出码照样 0）；不拿 `$env:PATH` 写回（那是机器级 ＋ 用户级拼起来的）。

/// 改用户级 `PATH` 那一趟的期限。
pub(crate) const USER_PATH_WITHIN: Deadline = Deadline::secs(12);

/// 加 / 撤共用的开头：我们那个目录 ＋ `HKCU\Environment` 里 `Path` 的未展开原值与原类型（不在 ⇒ 按缺省 `ExpandString`）。
/// 按原类型写回 ⇒ `%USERPROFILE%` 这类写法原样留着、`REG_EXPAND_SZ` 不被改成 `REG_SZ`。
fn user_path_head(dir: &str) -> String {
    format!(
        "$d = {}\n\
         $k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')\n\
         $p = [string]$k.GetValue('Path', '', 'DoNotExpandEnvironmentNames')\n\
         $t = if ($null -ne $k.GetValue('Path')) {{ $k.GetValueKind('Path') }} else {{ 'ExpandString' }}\n",
        super::dialect::ps_literal(dir)
    )
}

/// 直接写了注册表 ⇒ 自己广播 `WM_SETTINGCHANGE`（`"Environment"`），新开的终端才读得到新值、不用重新登录
/// （`[Environment]::SetEnvironmentVariable` 会广播，但它按 `REG_SZ` 写）。
pub(crate) const SETTING_CHANGE_BROADCAST: &str = "    Add-Type -Namespace CcMonitor -Name Env -MemberDefinition '[DllImport(\"user32.dll\", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, UIntPtr w, string l, uint f, uint t, out UIntPtr r);'\n\
    $r = [UIntPtr]::Zero\n\
    [void][CcMonitor.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$r)\n";

/// 把 `dir` 加到用户级 `PATH` 末尾；已经在（每格展开 `%VAR%` 之后整格比，大小写不敏感）⇒ 一个字节不写。
pub(crate) fn user_path_add(dir: &str) -> String {
    format!(
        "{}\
         if (-not ($p -split ';' | Where-Object {{ [Environment]::ExpandEnvironmentVariables($_) -eq $d }})) {{\n\
         \x20   $n = if ($p -eq '') {{ $d }} elseif ($p.EndsWith(';')) {{ $p + $d + ';' }} else {{ $p + ';' + $d }}\n\
         \x20   $k.SetValue('Path', $n, $t)\n\
         {SETTING_CHANGE_BROADCAST}\
         }}\n",
        user_path_head(dir)
    )
}

/// 从用户级 `PATH` 上只摘 `dir` 那一格（整格比，与加同一种相等；不用子串口径，`…\bin-old` 不受牵连）。
/// 空项不顺手清（空项在 Windows 上是「当前目录」）⇒ 别的格逐字复原；摘完剩空串 ⇒ 删掉这个值。不在 ⇒ 一个字节不写。
pub(crate) fn user_path_remove(dir: &str) -> String {
    format!(
        "{}\
         $kept = @($p -split ';' | Where-Object {{ [Environment]::ExpandEnvironmentVariables($_) -ne $d }})\n\
         if ($kept.Count -ne @($p -split ';').Count) {{\n\
         \x20   $n = $kept -join ';'\n\
         \x20   if ($n -eq '') {{ $k.DeleteValue('Path', $false) }} else {{ $k.SetValue('Path', $n, $t) }}\n\
         {SETTING_CHANGE_BROADCAST}\
         }}\n",
        user_path_head(dir)
    )
}

/// 用户级 `PATH`（展开之后）夹在一对 `mark` 中间，编成 UTF-8 字节直写标准输出流（同 [`fresh_path_query`]）。只读。
pub(crate) fn user_path_query(mark: &str) -> String {
    let m = super::dialect::ps_literal(mark);
    format!(
        "$u = [string][Environment]::GetEnvironmentVariable('Path', 'User')\n\
$b = [Text.Encoding]::UTF8.GetBytes({m} + $u + {m})\n\
$o = [Console]::OpenStandardOutput()\n\
$o.Write($b, 0, $b.Length)\n\
$o.Flush()\n"
    )
}

/// `dir` 在不在 `user_path` 上：按 `;` 切开整格比、ASCII 大小写不敏感 —— 与加 / 撤两段在 PowerShell 里的 `-eq` / `-ne` 同一种相等。
/// 不规范化（不砍末尾 `\`）：三处同样地比，界面才不会显示「在」而点了又多插一格。`dir` 空 ⇒ 恒不在（不与空项相等）。
pub(crate) fn user_path_has(user_path: &str, dir: &str) -> bool {
    !dir.is_empty()
        && user_path
            .split(';')
            .any(|seg| seg.eq_ignore_ascii_case(dir))
}

/// 夹在一对 `mark` 中间的那一段（可以是空串）；没有成对的标记 ⇒ `None`（脚本没跑到底）。
pub(crate) fn between_marks<'a>(out: &'a str, mark: &str) -> Option<&'a str> {
    let rest = out.split_once(mark)?.1;
    Some(rest.rsplit_once(mark)?.0)
}

/// 现问：`dir` 在不在用户级 `PATH` 上。这台不说 PowerShell ⇒ `None`。
pub(crate) fn user_path_holds(dir: &str) -> Option<Result<bool, Said>> {
    const MARK: &str = "@@ccm-user-path@@";
    let cmd = super::powershell_on(PsHost::Desktop, &user_path_query(MARK))?;
    Some(run_said(cmd, USER_PATH_WITHIN).and_then(|out| {
        between_marks(&out, MARK)
            .map(|p| user_path_has(p, dir))
            .ok_or_else(|| {
                Said::with_raw(
                    copy_core::copy_text("reason.content.unparsable", &[]),
                    out.trim(),
                )
            })
    }))
}

/// 跑 [`user_path_add`] / [`user_path_remove`] 那一段。这台不说 PowerShell ⇒ `None`。
pub(crate) fn user_path_set(dir: &str, on: bool) -> Option<Result<(), Said>> {
    let script = if on {
        user_path_add(dir)
    } else {
        user_path_remove(dir)
    };
    let cmd = super::powershell_on(PsHost::Desktop, &script)?;
    Some(run_said(cmd, USER_PATH_WITHIN).map(|_| ()))
}

/// 起一条 PowerShell：`Ok(stdout)`；起不来 / 过了期限 / 非零退出 ⇒ 原因词 ＋ 原话（系统报错 · stderr 按控制台代码页解）。
fn run_said(cmd: crate::platform::child::Child, within: Deadline) -> Result<String, Said> {
    use crate::platform::child::ChildFail;
    let out = cmd.run(within).map_err(|e| match &e {
        ChildFail::NotFound(io) | ChildFail::Io(io) => {
            Said::with_raw(copy_core::spawn_reason(io.kind()), &e)
        }
        ChildFail::TimedOut { .. } => {
            Said::with_raw(copy_core::io_reason(std::io::ErrorKind::TimedOut), &e)
        }
    })?;
    if !out.status.success() {
        let code = out.status.code().map(|c| c.to_string()).unwrap_or_default();
        let err = crate::platform::console_text::console_text(&out.stderr);
        return Err(Said::with_raw(
            copy_core::io_reason(std::io::ErrorKind::Other),
            format!("exit {code}\n{}", err.trim()),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/shell/powershell_user_path_tests.rs"]
mod user_path_tests;
