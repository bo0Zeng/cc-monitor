//! 〔`K-R132` · `KR135D1`〕**本机 `~/.cc-monitor/bin` 在不在用户级 PATH 上**：生成 / 探 / 加 / 撤那三条 PowerShell 命令（本机后端的引导那一格）。
//!
//! 〔MIG-3a · `设计/99 §2.1 ⑬` · 主会话 09-27 裁〕本模块从前还住着**别名块**那一整族（规划 · 围栏 · 装 / 卸 · `$PROFILE` 的块现状）——
//! 随别名规则与方言一起进了那台机器的后端（`src/backend/assets/aliases/block.rs`，界面经通道 `aliases-block-*` 直问）。
//! 留在 monitor 的只有这一格：它改的是**本机用户级环境变量**、跑的是**本机**那一个 `powershell.exe`，
//! 属于 monitor 引导本机后端那一类（⑬ `MONITOR_OWN` 的「起停引导」）。

use crate::copy_table::copy_text;

// ═══════════════════════════════════════════════════════════════════════════
// 🔴 `K-R132`：**装上了、能跑、用户敲不到** —— 这一段就是那条缺陷的修法
// ═══════════════════════════════════════════════════════════════════════════
//
// `K-R129` 在真机上（干净本地用户 · Release 上 `v3.8.0` 那份字节）实敲
// 三条安装路 × 三种 shell × 三个名字，**每一格都找不到**；而 `ccm.exe` 真在
// `%USERPROFILE%\.cc-monitor\bin\`。⇒ 唯一原因是**那个目录不在 PATH 上**。
//
// **病因不是 v3.8.0 的回归，是从来就没有过这个机制**：全仓一个 Windows PATH
// 写入点都没有（`setx` / `SetEnvironmentVariable` / `EnvVarUpdate` / `AddToPath`
// 扫 `*.rs *.ts *.nsi *.nsh *.wxs *.ps1 *.json` ⇒ 命中 0，量于本树 `95b6c93`）。
//
// ## 为什么补在这里，而不是补进安装器（`.nsi` / `.wxs`）
//
// 用户 `K33` 逐字：「**如果有需要动用户 alias 的就生成命令让用户自己填。
// 像是原本的填 PowerShell profile 和 bashrc 一样。**」
// ⇒ **产品生成、用户应用**，不是产品替用户改他的环境（同向 `INVARIANTS.md §41.6`）。
// 而「生成一段让用户装进自己 profile 的东西」这台机器**今天就在这里**
// （[`install_to_profile`]：两种方言 · 备份 → 原子替换 → 读回逐字比对 → 不符回滚）
// ⇒ 照 `K-R62` 那条走：**让这台已有的安装器多吐一行 PATH，不新起第四套**。
//
// ## 🔴 〔`R86` 09-15 用户裁〕**那条路后来被裁掉了 —— 这张表记的正是它为什么不够**
//
// | shell | 读不读 PowerShell `$PROFILE` | 往 profile 里塞一段 PATH 管不管用 |
// |---|---|---|
// | PowerShell | 读 | 管 —— 但**只管这一个进程** |
// | `cmd` | **不读**（它根本没有 profile 这个概念） | **不管** |
// | Git Bash | 不读（它读 `~/.bashrc`，那是 POSIX 那一臂） | 不管 |
//
// ⇒ 三格里只有一格通，而**「一格通、两格不通」比「三格都不通」更难查** ——
// 用户会把它读成「装好了」。⇒ 用户当场裁掉这条路（`R86` 逐字「既然要加用户 path，
// 这个就没用了」）：**要让三格都通只有改用户级 PATH 一条路**，
// 而那一步**由用户点一下**（`R85` 逐字「应该让用户手动点击加，也能管理删除」）。
// 命令文本住 [`render_user_path_setup_command`] 与 [`render_user_path_removal_command`]，
// 状态那一格住 [`user_path_has_our_bin`]。

/// 本机 `ccm` 入口所在目录的 **Windows 写法**（`%USERPROFILE%` 之下的相对路径）。
///
/// 目录本身取自共享 crate 里后端的落点 `relay_route_core::BACKEND_LANDING_REL`（本机远端同一个，V28；
/// 〔MIG-3b 续〕足迹申报表进了后端，那一格与它相等由后端判据对拍），这里只做一件事：把 `/` 换成 `\`。**本函数体内没有任何目录字面量。**
fn ccm_bin_dir_windows() -> Option<String> {
    ccm_bin_dir_rel().map(|d| d.replace('/', "\\"))
}

/// 后端落点所在的目录（家目录相对，`/` 分隔）。取不到 ⇒ `None`（不发明一个目录）。
pub(crate) fn ccm_bin_dir_rel() -> Option<&'static str> {
    relay_route_core::BACKEND_LANDING_REL
        .rsplit_once('/')
        .map(|(d, _)| d)
}

// 🔴 〔`R86` 09-15 用户裁〕**这里原本住着两个函数，本件把它们整个删掉了** ——
// 一个把我们那个 bin 目录塞进**本会话**的 `$env:PATH`，一个给它配一段说明注释。
// 用户逐字：「既然要加用户 path，这个就没用了。」
// ⚠ 旧名字刻意不写在这里：`structural_scan` 有一条判据在数「散文点名了一个代码里
// 根本不存在的符号」，而**指向不存在的判据比没有注释更坏**（testing.md 诚实边界 4）。
// 要查它们逐字长什么样，去 `git log -S` 那两个函数体，或读 `K-R132` 的件文件。
//
// ## PM 认，而且理由比「没用了」更硬一条：**留着它会一直制造那个「半通」状态**
//
// 那一段只动**这个 PowerShell 进程自己**的 `$env:PATH` ⇒
// `K-R129` / `K-R132` 两轮真机 3×3 表里最难看的那一格（**PowerShell 里能、`cmd` 里不能**）
// 就是它造出来的。**「看起来装好了、换个终端就没了」比「哪儿都没有」更难查。**
// ⇒ 删掉之后状态变**二值**：没点按钮 ⇒ 哪儿都敲不到；点了 ⇒ 哪儿都敲得到。
//
// ## 🔴 停止生成 ≠ 已有用户 profile 里那一段会自己消失
//
// `v3.8.0` 与 `main` 现在这一版**已经会往用户 profile 里写那一段**。现成机制接得住：
// 那一段住在 `BEGIN/END` 围栏里，[`install_to_profile`] **每次装 / 修复整块重写**、
// [`uninstall_from_profile`] 整块摘掉 ⇒ **只要用户再走一次装或卸，旧那段就没了。**
// ⚠ **但「一直没再装过」的用户会留着它** —— 它本身无害（只是把一个目录再前置一次），
// **可对没点过按钮的那位用户，半通状态会继续保留。别当它自动消失。**
//
// ⚠ 不为它做「检测到旧块」的提示，理由是**那条提示够不着它要治的人**：
// 会看见提示的人是**打开了这一格**的人，而那位用户点一下「加」就两头都好了；
// 真正留着旧块的是**再也没打开过这个面板**的人 —— 提示对他恒不可见。
// ⇒ 多一条恒静默的 UI 不如把状态做成二值。

/// 🔴 `KR132D1` 的结论落地：**让用户自己跑一次的那条命令**（路线①，用户级 PATH）。
///
/// 用户 `K33` 逐字「**生成命令让用户自己填**」⇒ 产品**只生成这段文字**，一个字节都不执行。
/// 这是三种 shell 里唯一一条 `cmd` 也认的路（`cmd` 不读任何 profile）。
///
/// # 两个经典地雷，这条命令都绕开了 —— 而绕开的方式是判据在数的
///
/// 1. **不用 `setx`。** `setx` 把值截断在 1024 字符，而它给的提示是一句警告、
///    退出码照样 0 ⇒ 一条**静默截断用户 PATH** 的命令。
/// 2. **只读 `'User'` 那一档，不读 `$env:PATH`。** 进程里的 `$env:PATH` 是
///    **机器级 ＋ 用户级拼起来的那一份**；拿它当新值写回用户级，会把整条系统 PATH
///    **复制进用户 PATH**（此后系统 PATH 的任何更新对这个用户都不再生效）。
///    这两条一起构成了 Windows 上「改 PATH 改坏机器」的绝大多数病例。
///
/// # 它不写 `'Machine'`
///
/// 机器级要管理员，而且卸载时留下的垃圾是**全机**的。用户级不需要管理员，
/// 卸载时也只影响这一个用户。
///
/// ⚠ **要重开终端才生效** —— 已经开着的进程拿的是自己启动那一刻的环境块副本。
/// 这句话**刻意不在这段命令里**（这里只放**能跑的那几行**）——
/// 它归**显示这段命令的那一侧**（界面那一格）去配说明。
/// 别把说明混进可执行文本里：混进去，用户复制一整段就会连注释一起跑。
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

/// 〔WF1 · K · `第四波记录/WIN3.md §2` K〕加 / 撤两条共用的开头：我们那个目录 ＋ 用户级 `Path` 的**原值与原类型**。
///
/// 读的是注册表 `HKCU\Environment` 里**未展开**的原值（`DoNotExpandEnvironmentNames`）与它的类型（`GetValueKind`，
/// 不在 ⇒ 按 Windows 的缺省 `ExpandString`）；写回按原类型 ⇒ `%USERPROFILE%` 这类写法原样留着、`REG_EXPAND_SZ` 不被改成 `REG_SZ`
/// （从前走 `[Environment]::GetEnvironmentVariable(…, 'User')` 读回的是展开值、`SetEnvironmentVariable` 写的是 `REG_SZ`）。
/// 比「是不是我们那一格」按**每格展开之后**比：与状态那一格读的展开值是同一种相等（[`user_path_has_our_bin`]）。
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

/// 🔴 `KR135D1` 的另一半（`R85` 逐字「**也能管理删除**」）：**把我们那一段从用户级
/// PATH 上摘掉的那条命令。** 「加」是上面那一条，这一条是「撤」。
///
/// # 🔴 撤这一侧会**再撞一次**同样那两个地雷 —— 逐条写出它是怎么绕的
///
/// 1. **不用 `setx`**：它把值截断在 1024 字符，提示只是一句警告、**退出码照样 0**。
///    ⚠ 撤这一侧踩它的后果比加那一侧**更重**：加那一次截断掉的是「多出来的尾巴」，
///    而撤是**整条重写**，截断掉的是用户本来就有的那一截。
/// 2. **读回来的是 `'User'` 那一档，不是 `$env:PATH`。** 进程里那份是机器级 ＋ 用户级
///    拼起来的；撤的时候拿它当基准写回用户级，后果与加那一侧**一模一样**
///    —— 整条系统 PATH 被复制进用户 PATH。
///    ★ **撤比加更容易踩这一个**，因为「先读一下现在的 PATH」在撤的语境里读起来非常顺手。
///
/// # 🔴 第三条，它是撤这一侧**独有**的：**只摘自己那一段，不碰别的**
///
/// 过滤条件 `-ne $d` 是**整格**比较（每格先展开 `%VAR%` 再比；PowerShell 的 `-ne` 对字符串默认大小写不敏感，
/// 与加那一侧的 `-eq` 是**同一种相等**）⇒ 摘掉的**恰好**是我们那一格。
///
/// **刻意不用** `-replace` / `-like` / `.Replace()` —— 那三种都是**子串**口径：
/// 用户 PATH 上存在 `…\.cc-monitor\bin-old`（任何以我们那一段为前缀的目录）时会被一起打掉，
/// 而那正是「碰了用户 PATH 里别的东西」。同一个病在加那一侧的形状是「误判成已经在了」，
/// 在撤这一侧的形状是**误删用户的目录** —— 后者不可逆。
///
/// ⚠ **连空项都不许顺手清**：过滤条件里**只有** `-ne $d` 一条，**没有**
/// `Where-Object {{ $_ }}` 那种「顺手把空项也滤掉」。PATH 上的空项在 Windows 上
/// 语义是「当前目录」，删掉它是一次**我们没被要求做的改动**（哪怕它算个改进）。
/// ⇒ 除我们那一格之外**逐字复原**，这是「只摘自己那一段」的字面意思。
///
/// ⚠ 摘完剩下空串是**正常**的：删掉这个值（`DeleteValue`），
/// 那正是「我们那一格是用户级 PATH 上唯一一格」时该有的结果 —— 不是清空了用户的 PATH
/// （**机器级那一档一个字节都没碰**，用户下次开终端仍然有完整的系统 PATH）。
/// 〔WF1 · K〕我们那一格不在 ⇒ 一个字节不写；写回按原类型、别的格逐字（`%VAR%` 不展开）⇒ 加了再撤回到原样。
///
/// ⚠ **要重开终端才看得到** —— 与加那一侧同理，已经开着的进程拿的是自己启动那一刻的
/// 环境块副本。这句话不放进可执行文本里（放进去，用户复制一整段就会连注释一起跑）。
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
/// 加那一条用 `-eq $d`、撤那一条用 `-ne $d`（〔WF1 · K〕都比每格展开 `%VAR%` 之后的样子），两者在 PowerShell 里都是
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

/// 🔴 `KR135D1`：**问「现在状态」的那条命令**（第三条，只读）。
///
/// 它吐两行：**第 1 行是我们那个 bin 目录的绝对路径**（`%USERPROFILE%` 展开之后），
/// **第 2 行是 `'User'` 那一档的原始 PATH 串**。
/// 目录路径与 PATH 串都**不可能含换行** ⇒ 按行切是安全的，不需要发明分隔符。
///
/// # 为什么要它把目录也一起吐回来
///
/// [`user_path_has_our_bin`] 比的是**整格**，而 PATH 上那一格是**绝对路径**；
/// 我们这一侧只知道 `%USERPROFILE%` **相对**的那一段（`tool_registry` 申报的就是相对路径）。
/// ⇒ 让**同一个 PowerShell 进程**用 `Join-Path $env:USERPROFILE` 展开，
/// 与「加」「撤」那两条命令**用的是同一句展开**（三条都写着同一个 `Join-Path`）
/// —— 在 Rust 这一侧自己拼一次 `%USERPROFILE%` 就是那个值的第二个住址。
///
/// # 两个地雷在这一侧同样适用
///
/// 它**只读 `'User'` 那一档**：读 `$env:PATH` 会把「机器级上有」读成「用户级上有」，
/// 于是「撤」那个按钮点下去什么都没发生、而界面还说它撤掉了。
/// 判据与另外两条走**同一批断言**。
///
/// # 〔P2 · `设计/99 §2.3` WF1 报备〕两行自己编成 UTF-8 字节、直写标准输出流
///
/// Windows PowerShell 5.1 往管道写输出用控制台的 OEM 代码页（中文系统是 936），而读回那一侧按 UTF-8 解
/// ⇒ 用户目录含非 ASCII 时第 1 行解坏、「在不在 PATH 上」判错。直写字节不经控制台编码、也不去改控制台代码页。
/// 只改这一条：它只给机器读；加 / 撤两条用户会抄去自己的终端跑，不动。
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

/// 用户级 PATH 那一格的**现状**。`R85` 逐字要的三样里的第一样，**现算，不缓存**。
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
    /// 探不动时的原话。🔴 **探不动 ≠ 不在 PATH 上** —— 界面必须把这一格显示出来，
    /// 不许把它静默成「未安装」（同 `launcher-diagnostics` 那条「扫不动不许静默」）。
    pub error: Option<String>,
}

/// 🔴 `R88` ＋ `KR135D1`：**本模块唯一一处起进程。**
///
/// # 为什么产品这一侧要起它（`R85` / `R88`，不是顺手）
///
/// `R85` 用户逐字「**应该让用户手动点击加，也能管理删除**」⇒ **点击即执行是允许的**
/// （`§0c`：`K33` 禁的是产品**替**用户决定，**用户点一下就是用户自己决定**）。
/// 而「现在状态」那一格**根本不可能靠用户去跑** —— 那正是 `R88` 推翻 `R87`
/// 「本件不需要起进程」那句假前提的地方。
///
/// # 为什么是起 PowerShell，而不是 Rust 直接写注册表
///
/// `R88` 裁定（两条，按份量）：
/// 1. **直接写注册表会造出第二份 PATH 编辑实现** —— 而走 Tauri 命令的理由本身就是
///    「别给同一族动作另起一条路」。⇒ 这一跳跑的**就是我们生成给用户看的那段字节**，
///    「点按钮」与「自己复制去跑」**逐字同一份**，实现真的只有一处（`K33`）。
/// 2. 广播 `WM_SETTINGCHANGE` 不能忘，忘了的后果是「改了、新开的终端看不到，要重登录」
///    —— **那正是本件在杀的那个形状**（`R86` 逐字「看起来装好了、换个终端就没了」）。
///    〔WF1 · K〕`[Environment]::SetEnvironmentVariable(…,'User')` 自带广播，可它按 `REG_SZ` 写、读回的是展开值
///    （`%USERPROFILE%` 被冻成字面、`REG_EXPAND_SZ` 变 `REG_SZ`）⇒ 生成的那段改在 PowerShell 里按原类型写注册表、
///    广播自己做（[`SETTING_CHANGE_BROADCAST`]）。仍是这一段生成的字节，不是 Rust 写注册表。
///
/// # argv 的形状（`write_site_registry::SPAWNS` 那一行逐字记的就是这个）
///
/// `powershell.exe -NoProfile -NonInteractive -Command <脚本>`。
/// - **`-NoProfile` 是承重的**：不读用户自己的 profile ⇒ 这一跳的行为不被用户配置左右
///   （而且本件刚刚才把我们自己那一段从 profile 里删掉，再去读 profile 是自相矛盾）。
/// - **`-NonInteractive`**：绝不弹提示等人回车 —— 界面点一下不许挂住。
/// - `<脚本>` **只可能是本模块那三个 `render_*` 函数的输出**，不吃任何用户输入；
///   里面唯一的变量是 `tool_registry` 申报的那个目录。判据在数这件事。
///
/// 非 Windows 上**不起进程**，直接如实回错 —— 那台机器上根本没有「用户级 PATH」这一档。
/// 〔P4b · 阶段 H〕有没有这一档由 `platform::login_shell` 答（原先是这里两份 cfg 分身）。
fn run_user_path_powershell(script: &str) -> Result<String, String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    if !crate::platform::login_shell::LOGIN_SHELL.has_user_level_path() {
        return Err(copy_text("rsProfileInstaller.userPath.notWindows", &[]));
    }
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(std::process::Stdio::piped());
    // 三条策略（`00 §1.5.2`）：
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
                // 〔P2〕stderr 是 PowerShell 按控制台代码页写的（stdout 那一路探针自己写 UTF-8，不走这里）。
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

/// U6a：把 **PS↔monitor 握手**的顺序与数字钉在 `src/doc/IPC-PROTOCOL.md` 上。
///
/// # 为什么需要这个
///
/// U6a 逐图核时序图时发现：图上画的是 **v2 修复之前**的握手 —— 顺序是旧的
/// （先写 await 文件、后设窗口标题）、deadline 写 800ms（实际早已是 3000ms）、
/// notify debouncer 写 100ms（实际 50ms）、monitor 侧的 ≤600ms 重试**整个没画**。
///
/// 也就是说：文档描述的是那个**已经被修掉的 bug 的行为**。照图重新实现一遍 PS 侧，
/// 会精确复刻 v2.21 那个「每个新 shell 首次 `cc` 固定烧满超时」的故障。
///
/// 顺序那一条尤其不能只靠注释：它是**两个文件之间的时序约束**，两边看起来都合理，
/// 只有合起来看才错。
#[cfg(test)]
#[path = "../../../../tests/frontend/shell/profile_installer_handshake_doc_guard.rs"]
mod handshake_doc_guard;
