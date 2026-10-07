//! 本机 `~/.cc-monitor/bin` 在不在用户级 PATH 上：生成 / 探 / 加 / 撤那几条 PowerShell 命令（本机后端的引导那一格）。
//! 别名块那一族在那台机器的后端（`src/backend/assets/aliases/block.rs`）；留在 monitor 的只有这一格：它改的是本机用户级环境变量、
//! 跑的是本机那一个 `powershell.exe`，属于 monitor 引导本机后端那一类（`MONITOR_OWN` 的「起停引导」）。

use crate::copy_table::copy_text;

//
// ══════════════════════════════════════════════════════════════════
// 装上了、能跑，还要用户敲得到：`ccm.exe` 在 `%USERPROFILE%\.cc-monitor\bin\`，那个目录要在 PATH 上
// ══════════════════════════════════════════════════════════════════
//
// 产品生成、用户应用（`INVARIANTS.md §41.6` 同向）：生成命令，由用户点一下执行，也能管理删除。
// 只有改用户级 PATH 一条路让三种 shell 都通：往 PowerShell `$PROFILE` 塞一段只管 PowerShell（`cmd` 没有 profile，Git Bash 读 `~/.bashrc`），
// 而「一格通、两格不通」比「三格都不通」更难查。命令文本住 [`render_user_path_setup_command`] 与 [`render_user_path_removal_command`]，
// 状态那一格住 [`user_path_has_our_bin`]。状态是二值的：没点按钮 ⇒ 哪儿都敲不到；点了 ⇒ 哪儿都敲得到。
// 用户 profile 里旧版写过的那一段在 `BEGIN/END` 围栏里，再走一次装或卸（[`install_to_profile`] / [`uninstall_from_profile`]）就没了；
// 一直没再装过的用户会留着它（无害：只是把一个目录再前置一次）。

/// 本机 `ccm` 入口所在目录的 **Windows 写法**（`%USERPROFILE%` 之下的相对路径）。
///
/// 目录本身取自共享 crate 里后端的落点 `relay_route_core::BACKEND_LANDING_REL`（本机远端同一个；
/// 足迹申报表进了后端，那一格与它相等由后端判据对拍），这里只做一件事：把 `/` 换成 `\`。**本函数体内没有任何目录字面量。**
fn ccm_bin_dir_windows() -> Option<String> {
    ccm_bin_dir_rel().map(|d| d.replace('/', "\\"))
}

/// 后端落点所在的目录（家目录相对，`/` 分隔）。取不到 ⇒ `None`（不发明一个目录）。
pub(crate) fn ccm_bin_dir_rel() -> Option<&'static str> {
    relay_route_core::BACKEND_LANDING_REL
        .rsplit_once('/')
        .map(|(d, _)| d)
}

/// 让用户自己跑一次的那条命令（用户级 PATH）。产品只生成这段文字，一个字节都不执行；这是三种 shell 里唯一一条 `cmd` 也认的路。
///
/// 两个经典地雷都绕开（判据在数）：
/// 1. 不用 `setx`：它把值截断在 1024 字符，给的只是一句警告、退出码照样 0 ⇒ 静默截断用户 PATH。
/// 2. 只读 `'User'` 那一档，不读 `$env:PATH`：进程里那份是机器级 ＋ 用户级拼起来的，写回用户级会把整条系统 PATH 复制进用户 PATH。
/// 不写 `'Machine'`：要管理员，卸载时的垃圾是全机的。
/// 要重开终端才生效（已经开着的进程拿的是启动那一刻的环境块副本）—— 这句说明归显示这段命令的界面，不混进可执行文本（用户复制一整段会连注释一起跑）。
pub fn render_user_path_setup_command() -> Option<String> {
    let head = user_path_read_prelude()?;
    Some(format!(
        "{head}\
         if (-not ($p -split ';' | Where-Object {{ [Environment]::ExpandEnvironmentVariables($_) -eq $d }})) {{\n\
         \x20   $n = if ($p -eq '') {{ $d }} elseif ($p.EndsWith(';')) {{ $p + $d + ';' }} else {{ $p + ';' + $d }}\n\
         \x20   $k.SetValue('Path', $n, $t)\n\
         {SETTING_CHANGE_BROADCAST}\
         }}\n"
    ))
}

/// 加 / 撤两条共用的开头：我们那个目录 ＋ 用户级 `Path` 的原值与原类型。
/// 读注册表 `HKCU\Environment` 里未展开的原值（`DoNotExpandEnvironmentNames`）与它的类型（`GetValueKind`，不在 ⇒ 按 Windows 的缺省 `ExpandString`）；
/// 写回按原类型 ⇒ `%USERPROFILE%` 这类写法原样留着、`REG_EXPAND_SZ` 不被改成 `REG_SZ`。
/// 比「是不是我们那一格」按每格展开之后比：与状态那一格读的展开值是同一种相等（[`user_path_has_our_bin`]）。
fn user_path_read_prelude() -> Option<String> {
    let dir = ccm_bin_dir_windows()?;
    Some(format!(
        "$d = Join-Path $env:USERPROFILE '{dir}'\n\
         $k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')\n\
         $p = [string]$k.GetValue('Path', '', 'DoNotExpandEnvironmentNames')\n\
         $t = if ($null -ne $k.GetValue('Path')) {{ $k.GetValueKind('Path') }} else {{ 'ExpandString' }}\n"
    ))
}

/// 直接写了注册表 ⇒ 自己广播 `WM_SETTINGCHANGE`（`"Environment"`），新开的终端才读得到新值、不用重新登录
/// （`[Environment]::SetEnvironmentVariable` 替你做这一步，我们不走它 —— 它按 `REG_SZ` 写）。
const SETTING_CHANGE_BROADCAST: &str = "    Add-Type -Namespace CcMonitor -Name Env -MemberDefinition '[DllImport(\"user32.dll\", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, UIntPtr w, string l, uint f, uint t, out UIntPtr r);'\n\
    $r = [UIntPtr]::Zero\n\
    [void][CcMonitor.Env]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$r)\n";

/// 把我们那一段从用户级 PATH 上摘掉的那条命令（「加」是上面那一条）。
///
/// 同样不用 `setx`（撤是整条重写，截断掉的是用户本来就有的那一截）、只读 `'User'` 那一档（撤的语境里「先读一下现在的 PATH」更顺手，更容易踩）。
/// 只摘自己那一段：过滤条件 `-ne $d` 是整格比较（每格先展开 `%VAR%` 再比；与加那一侧的 `-eq` 是同一种相等）。
/// 不用 `-replace` / `-like` / `.Replace()`：子串口径会把 `…\.cc-monitor\bin-old` 这类目录一起打掉 —— 误删用户的目录，不可逆。
/// 连空项都不顺手清（PATH 上的空项在 Windows 上语义是「当前目录」）⇒ 除我们那一格之外逐字复原。
/// 摘完剩下空串 ⇒ 删掉这个值（`DeleteValue`），机器级那一档一个字节都没碰。我们那一格不在 ⇒ 一个字节不写；写回按原类型、别的格逐字 ⇒ 加了再撤回到原样。
pub fn render_user_path_removal_command() -> Option<String> {
    let head = user_path_read_prelude()?;
    Some(format!(
        "{head}\
         $kept = @($p -split ';' | Where-Object {{ [Environment]::ExpandEnvironmentVariables($_) -ne $d }})\n\
         if ($kept.Count -ne @($p -split ';').Count) {{\n\
         \x20   $n = $kept -join ';'\n\
         \x20   if ($n -eq '') {{ $k.DeleteValue('Path', $false) }} else {{ $k.SetValue('Path', $n, $t) }}\n\
         {SETTING_CHANGE_BROADCAST}\
         }}\n"
    ))
}

/// 🔴 `KR135D1` 的第一样：**`~/.cc-monitor\bin` 在不在用户级 PATH 上。**
///
/// 这是那一格的**判定内核**（`现算，不缓存` 指的是调用方每次重新读一遍 `'User'`
/// 那一档再问它，本函数自己不持有任何状态）。
///
/// - `user_path_raw`：从 **`'User'` 那一档**读回来的原始串。
///   🔴 **不许传 `$env:PATH`**（`§0b` 第 2 条）—— 那一份是机器级 ＋ 用户级拼起来的，
///   拿它判「在不在**用户级**上」会把「机器级上有」读成「用户级上有」，
///   于是「撤」那个按钮点下去什么都没发生，而界面还说它撤掉了。
/// - `dir_abs`：`%USERPROFILE%` 展开之后我们那个目录。
///
/// # 🔴 相等口径与生成的那两条命令**必须是同一种**，否则界面会撒谎
///
/// 加那一条用 `-eq $d`、撤那一条用 `-ne $d`（都比每格展开 `%VAR%` 之后的样子），两者在 PowerShell 里都是
/// **整格 · 大小写不敏感**。这里逐字照同一种（`user_path_raw` 是探针读回的展开值）：按 `;` 切开比**整格**，
/// 用 `eq_ignore_ascii_case`。
///
/// ⚠ **刻意不做路径规范化**（不砍末尾 `\`、不解析 `..`、不 `canonicalize`）——
/// 理由是**一致压过聪明**：规范化之后本函数会说「已经在了」，而那两条命令
/// （它们不规范化）会说「不在」⇒ 界面显示 ✓、点一下却又多出一格重复。
/// **两边同样地笨，比一边聪明一边笨要好。**
/// ⇒ 代价如实写在这里：用户手写过一格**带末尾反斜杠**的同一个目录时，这一格显示「不在」，
/// 点「加」会再插一格。要治它得三处一起改（本函数 ＋ 两条命令），不许只改这一处。
///
/// ⚠ `dir_abs` 为空时**恒回 `false`** —— 不然 PATH 上任何一个空项（连着两个 `;`）
/// 都会与它相等，于是「拿不到目录」会被读成「已经装好了」。
pub fn user_path_has_our_bin(user_path_raw: &str, dir_abs: &str) -> bool {
    if dir_abs.is_empty() {
        return false;
    }
    user_path_raw
        .split(';')
        .any(|seg| seg.eq_ignore_ascii_case(dir_abs))
}

/// 问「现在状态」的那条命令（只读）。吐两行：第 1 行是我们那个 bin 目录的绝对路径（`%USERPROFILE%` 展开之后），第 2 行是 `'User'` 那一档的原始 PATH 串
/// （都不可能含换行 ⇒ 按行切安全）。目录由同一个 PowerShell 进程用 `Join-Path $env:USERPROFILE` 展开，与加 / 撤两条同一句（Rust 这一侧自己拼就是第二个住址）。
/// 只读 `'User'` 那一档：读 `$env:PATH` 会把「机器级上有」读成「用户级上有」。
/// 两行自己编成 UTF-8 字节、直写标准输出流：Windows PowerShell 5.1 往管道写用控制台的 OEM 代码页（中文系统是 936），读回按 UTF-8 解 ⇒
/// 用户目录含非 ASCII 时会解坏。只改这一条（它只给机器读）；加 / 撤两条用户会抄去自己的终端跑，不动。
pub fn render_user_path_probe_command() -> Option<String> {
    let dir = ccm_bin_dir_windows()?;
    Some(format!(
        "$d = Join-Path $env:USERPROFILE '{dir}'\n\
         $u = [Environment]::GetEnvironmentVariable('Path', 'User')\n\
         $b = [Text.Encoding]::UTF8.GetBytes($d + [char]10 + $u + [char]10)\n\
         $o = [Console]::OpenStandardOutput()\n\
         $o.Write($b, 0, $b.Length)\n\
         $o.Flush()\n"
    ))
}

/// 探针那两行 ⇒ `(目录, 在不在用户级 PATH 上)`。目录空 ⇒ `None`（此时恒不在，见 [`user_path_has_our_bin`]）。
fn read_probe_lines(raw: &str) -> (Option<String>, bool) {
    let mut lines = raw.lines();
    let dir = lines.next().unwrap_or("").trim().to_string();
    let on = user_path_has_our_bin(lines.next().unwrap_or("").trim_end(), &dir);
    ((!dir.is_empty()).then_some(dir), on)
}

/// 用户级 PATH 那一格的现状，现算，不缓存。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPathStatus {
    /// 这台机器有没有「用户级 PATH」这一档 —— **它是 Windows 独有的**。
    /// `false` 时下面三格一律不许被读成「没装」（见 [`user_path_status`] 头注）。
    pub supported: bool,
    /// 我们那个 bin 目录的**绝对路径**（探针展开回来的那一行）。探不动 ⇒ `None`。
    pub dir: Option<String>,
    /// 在不在用户级 PATH 上。**探不动时恒 `false`，而那时 `error` 非空** ——
    /// 两者要一起读，别单看这一格。
    pub on_user_path: bool,
    /// 「加」那条命令的逐字文本（给不想点按钮的人复制；与按钮跑的**是同一份字节**）。
    pub add_command: Option<String>,
    /// 「撤」那条命令的逐字文本。
    pub remove_command: Option<String>,
    /// 探不动时的原话。探不动 ≠ 不在 PATH 上 —— 界面必须把这一格显示出来，不许静默成「未安装」。
    pub error: Option<String>,
}

/// 本模块唯一一处起进程：用户点按钮执行生成的那段字节（「点按钮」与「自己复制去跑」逐字同一份，实现只有一处）；「现在状态」那一格不可能靠用户去跑。
/// 不用 Rust 直接写注册表：那是第二份 PATH 编辑实现。广播 `WM_SETTINGCHANGE` 不能忘（忘了：改了、新开的终端看不到，要重登录）；
/// `[Environment]::SetEnvironmentVariable(…,'User')` 自带广播，但它按 `REG_SZ` 写、读回的是展开值 ⇒ 生成的那段在 PowerShell 里按原类型写注册表、
/// 广播自己做（[`SETTING_CHANGE_BROADCAST`]）。
///
/// argv：`powershell.exe -NoProfile -NonInteractive -Command <脚本>`（`write_site_registry::SPAWNS` 那一行记着）。
/// `-NoProfile` 是承重的（这一跳的行为不被用户配置左右）；`-NonInteractive`：绝不弹提示等人回车。
/// `<脚本>` 只可能是本模块那几个 `render_*` 函数的输出，不吃任何用户输入（唯一的变量是 `tool_registry` 申报的那个目录）。
/// 非 Windows 上不起进程，直接如实回错（有没有用户级 PATH 这一档由 `platform::login_shell` 答）。
fn run_user_path_powershell(script: &str) -> Result<String, String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    if !crate::platform::login_shell::LOGIN_SHELL.has_user_level_path() {
        return Err(copy_text("rsProfileInstaller.userPath.notWindows", &[]));
    }
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(std::process::Stdio::piped());
    // 三条策略：
    // · `Hidden` —— 🔴 **先前是裸 `.output()`，也就是没人回答过这个问题**：`-NonInteractive`
    //   只保证它不等人回车，**挡不住 Windows 给它新开一个控制台窗口**。用户点一下
    //   「加到 PATH」就闪一个黑框，而这一跳的全部意义是「点一下、悄悄改好」。
    // · `JobKillOnClose` —— 就地等它退；`SetEnvironmentVariable` 那段若起了别的东西，
    //   不许留在后面。
    // · `Captured` —— stderr **是返回值的一部分**（下面那句 `退出码 …；stderr：…`
    //   逐字要用它），不是被丢了。
    let out = spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::JobKillOnClose,
        StderrSink::Captured,
    )
    .and_then(|c| c.wait_with_output())
    .map_err(|e| {
        copy_text(
            "rsProfileInstaller.ps.spawnFailed",
            &[("e", &e.to_string())],
        )
    })?;
    if !out.status.success() {
        return Err(copy_text(
            "rsProfileInstaller.ps.exitCode",
            &[
                ("status", &format!("{:?}", out.status.code())),
                // stderr 是 PowerShell 按控制台代码页写的（stdout 那一路探针自己写 UTF-8，不走这里）。
                (
                    "detail",
                    &crate::platform::console_text::console_text(&out.stderr)
                        .trim()
                        .to_string(),
                ),
            ],
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// `KR135D1` ①：**现在状态**。每调一次真跑一趟探针，**不缓存**。
///
/// 🔴 **探不动时不许假装「不在 PATH 上」**：那时 `on_user_path = false` 而 `error` 非空，
/// 界面要显示 `error` 那一句。把「问不出来」显示成「没装」，用户会去点「加」，
/// 而那一下同样会失败 —— 两次失败之间他学不到任何东西。
pub fn user_path_status() -> UserPathStatus {
    let has_user_path = crate::platform::login_shell::LOGIN_SHELL.has_user_level_path();
    let add_command = render_user_path_setup_command();
    let remove_command = render_user_path_removal_command();
    let probe = match render_user_path_probe_command() {
        Some(p) => p,
        None => {
            return UserPathStatus {
                supported: has_user_path,
                dir: None,
                on_user_path: false,
                add_command,
                remove_command,
                error: Some(copy_text("rsProfileInstaller.userPath.noBinDir", &[])),
            };
        }
    };
    if !has_user_path {
        return UserPathStatus {
            supported: false,
            dir: None,
            on_user_path: false,
            add_command,
            remove_command,
            error: None,
        };
    }
    match run_user_path_powershell(&probe) {
        Ok(raw) => {
            let (dir, on_user_path) = read_probe_lines(&raw);
            UserPathStatus {
                supported: true,
                on_user_path,
                dir,
                add_command,
                remove_command,
                error: None,
            }
        }
        Err(e) => UserPathStatus {
            supported: true,
            dir: None,
            on_user_path: false,
            add_command,
            remove_command,
            error: Some(e),
        },
    }
}

/// `KR135D1` ②：**一个按钮加**。跑的就是 [`render_user_path_setup_command`] 那段字节。
pub fn user_path_add() -> Result<(), String> {
    let script = render_user_path_setup_command()
        .ok_or(&copy_text("rsProfileInstaller.userPath.addNoDir", &[]))?;
    run_user_path_powershell(&script).map(|_| ())
}

/// `KR135D1` ③：**一个按钮撤**。跑的就是 [`render_user_path_removal_command`] 那段字节
/// —— **只摘自己那一格**（整格比，不碰用户 PATH 里别的东西）。
pub fn user_path_remove() -> Result<(), String> {
    let script = render_user_path_removal_command()
        .ok_or(&copy_text("rsProfileInstaller.userPath.removeNoDir", &[]))?;
    run_user_path_powershell(&script).map(|_| ())
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/profile_installer_tests.rs"]
mod tests;

/// 把 PS↔monitor 握手的顺序与数字钉在 `src/doc/IPC-PROTOCOL.md` 上：照一张画错的时序图重新实现一遍 PS 侧，会复刻「每个新 shell 首次 `cc` 固定烧满超时」的故障。
/// 顺序是两个文件之间的时序约束，两边看起来都合理，只有合起来看才错 —— 不能只靠注释。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/profile_installer_handshake_doc_guard.rs"]
mod handshake_doc_guard;
