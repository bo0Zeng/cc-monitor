//! F03（unify-launch）：探测远端是否已装 `ccm`（F02 统一启动 CLI）及其能力集，供前端
//! `src/ccm-probe.ts` 决定走 CLI 渲染器还是兜底渲染器。一次性 headless SSH exec，照
//! `tmux.rs::capture_remote_pane` 的范式（通道 B，不干扰前台终端、不涉及 daemon）。

use crate::ssh_source;
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct CcmProbeResult {
    pub installed: bool,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    /// 🔴 `K-R70`：对面**那一份二进制**自报的构建身份（`--ccm-probe` 的 `build=` 那一行）。
    ///
    /// # 它与 `version` 不是同一个问题，别合并
    ///
    /// `version` 是**CLI 的契约版本**（`control::ccm::CCM_VERSION`：`4` 是最后一版 bash、
    /// `5` 起是后端本体）—— 它答「你认得哪些参数」。`build` 答「**你是哪一次构建**」。
    /// 在 `K-R70` 之前，后者只能去读那份二进制**旁边**的 `.build_id` 文本文件，
    /// 而那是一张从源码常量抄来的标签（三个载体恒等 ⇒ 零证据，`K-R68` · `R26` 裁定零）。
    ///
    /// ⚠ **`None` 有两种来历，这里分不开**：对面没装 / 对面是 `p2f-build-stamp` 之前的
    /// 旧后端（它根本不吐这一行）。要分开得再问一次别的东西 —— 本件不做，如实登记。
    ///
    /// ⚠ **它刻意不进 [`classify_path_ccm`] 的判据**：那一格问的是「PATH 上那个是不是
    /// 我们这一份」，而**同一份后端的两个构建仍然是「我们这一份」**。拿 `build` 去判
    /// 会把「我们装的比 PATH 上那个新」误报成「PATH 上那个不是我们的」。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

/// 解析 `ccm --ccm-probe` 的输出。首行非字面 `name=ccm` → 判定未装/不兼容——防止 PATH 里
/// 已有同名但无关的自定义 `ccm`（用户自己的脚本）被误判为本工具的 CLI。
fn parse_probe_output(out: &str) -> CcmProbeResult {
    let mut lines = out.lines();
    if lines.next() != Some("name=ccm") {
        return CcmProbeResult {
            installed: false,
            version: None,
            capabilities: vec![],
            build: None,
        };
    }
    let (mut version, mut capabilities, mut build) = (None, vec![], None);
    for line in lines {
        if let Some(v) = line.strip_prefix("version=") {
            version = Some(v.to_string());
        } else if let Some(b) = line.strip_prefix("build=") {
            // 🔴 `K-R70`：**这一行来自那个进程自己**（`control::ccm::probe_output` 里
            // 直接读 `crate::BUILD_ID`），不是我们去读它旁边的哪个文件。
            build = Some(b.to_string());
        } else if let Some(c) = line.strip_prefix("capabilities=") {
            capabilities = c
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    CcmProbeResult {
        installed: true,
        version,
        capabilities,
        build,
    }
}

/// 探测命令本身 —— **本机与远端逐字同一条**。
///
/// P3t-Y2：这就是 §40「本地 = 不走 ssh 的远端」在探测这一跳的落点 ——
/// 同一个命令串，远端包进 ssh，本机直接交给 `bash -lic`。
/// 抽成常量不是为了省字，是为了让「两侧探的是不是同一件事」这个问题**不必靠读两遍确认**
/// （`the_local_and_remote_probe_ask_the_same_question` 钉住它只有一处定义）。
const CCM_PROBE_CMD: &str =
    "command -v ccm >/dev/null 2>&1 && ccm --ccm-probe || printf 'NO_CCM\\n'";

/// 本机探测结果的缓存。TTL 与前端 `ccm-probe.ts::CCM_PROBE_TTL_MS` 同为 5 分钟 ——
/// 用户装完 ccm 不必重启 app，但也不必每次拉起都付一次 `bash -lic` 的钱。
///
/// ⚠ 与前端那个缓存**不是同一个** —— 那个缓存的是远端 origin 的探测结果（走 IPC），
/// 这个缓存的是本机的（不走 IPC）。同一个 TTL 是刻意的，不是它们共用了什么。
static LOCAL_PROBE_CACHE: std::sync::Mutex<Option<(std::time::Instant, CcmProbeResult)>> =
    std::sync::Mutex::new(None);
const LOCAL_PROBE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// P3t-Y2：**本机**的 ccm 探测 —— 同步、带缓存。
///
/// # 为什么本机这条是同步的，而远端那条是 async
///
/// 调用方是 `history.rs::launch_local`（`resume_history_session` / `new_local_session`
/// 两个 `#[tauri::command]` 的同步调用链）。远端那条是 async 因为 ssh 本来就要 await；
/// 本机没有那一跳，`bash -lic` 直接跑 —— 为了它把整条链改成 async 是纯粹的传染。
///
/// # 为什么必须真跑一次，而不是渲染完让 shell 自己探
///
/// 旧路（`build_local_posix_command`）用的是 shell 里的 `if command -v cc; then …; else …; fi`
/// —— 那种探法只答得了「在不在」，答不了**能力集**。而渲染器要按 caps 决定
/// 「这条修饰说不说得出来」（§35/§37）：把 caps 猜成「全都有」，遇到老 ccm 会渲染出一条
/// 带未知 flag 的命令，而那时 `else` 分支**已经不在了**，没有回落可走 ⇒ 是个 fail-open。
///
/// # 未装不是错误
///
/// 与远端同款：`command -v` 找不到 → `NO_CCM` 哨兵 → `installed: false`，
/// 调用方据此诚实降级回旧路。`bash` 本身跑不起来（罕见）也走这条 —— 探不到就当没装，
/// 绝不让「探测失败」升级成「拉不起来」。
#[cfg(not(windows))]
pub(crate) fn probe_local_ccm() -> CcmProbeResult {
    // ★★ **锁不跨子进程**〔D 阶段补审 08-12〕。
    //
    // 第一版把 `MutexGuard` 一路握到 `Command::output()` 之后。那形状有两个后果：
    // ① 两次并发 resume 会**排队**，第二次白等第一次的一整个 `bash -lic`；
    // ② 真出现下面那种挂死时，锁**永远不释放** ⇒ 此后**每一次**本机 resume 都堵在这里，
    //    而 UI 上什么都不会响 —— 与 `local_backend` 那条「reap 不许握着锁等」同一个病。
    // ⇒ 读一次就放手；探完再拿一次写回去。竞态下最多多探一次（纯读、幂等），代价远小于排队。
    {
        let g = LOCAL_PROBE_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, cached)) = g.as_ref() {
            if at.elapsed() < LOCAL_PROBE_TTL {
                return cached.clone();
            }
        }
    }
    let r = probe_local_ccm_uncached(LOCAL_PROBE_TIMEOUT);
    *LOCAL_PROBE_CACHE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((std::time::Instant::now(), r.clone()));
    r
}

/// 探测的**上限**。
///
/// 本机实测一次 0.12–0.13s（`bash -lic` 那层要跑完用户的交互式 rc）。给到 5s
/// 是留给「rc 里有 nvm/conda 这类慢初始化」的机器，而不是留给「rc 会卡住」的机器 ——
/// 后者要的是**有个头**，不是等得更久。
#[cfg(not(windows))]
const LOCAL_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// 带超时地跑一次探测。**超时不是错误，是「当没装」** —— 调用方据此降级回旧路。
///
/// # 为什么非有超时不可〔D 阶段补审 08-12〕
///
/// 它跑的是 `bash -lic`，也就是**用户自己的交互式 rc**，内容不在本仓控制之下
/// （联网的 `nvm use`、等 agent 的 `ssh-add`、写错的 `read`……任何一条都能让它永不返回）。
/// 而调用链是 `resume_history_session`（**同步** `#[tauri::command]`）→ `launch_local`
/// ⇒ 没有上限的话，用户点一次「恢复」就是**永远转圈、且不再有任何本机会话拉得起来**。
///
/// `Command::output()` 没有超时形态（std 不提供 `wait_timeout`），所以这里自己拼：
/// stdout 交给一条读线程（不读会在管道写满时把子进程堵死），主线程按截止时间轮询 `try_wait`。
#[cfg(not(windows))]
pub(crate) fn probe_local_ccm_uncached(timeout: std::time::Duration) -> CcmProbeResult {
    probe_with(timeout, CCM_PROBE_CMD)
}

/// [`probe_local_ccm_uncached`] 的内层。**命令是参数只为了让判据能喂一个「一定挂住」的串**；
/// 生产侧唯一的实参是 [`CCM_PROBE_CMD`]（由 `the_only_production_probe_command_is_the_constant` 钉）。
#[cfg(not(windows))]
fn probe_with(timeout: std::time::Duration, cmd: &str) -> CcmProbeResult {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    probe_spawned(timeout, &|| {
        let mut c = std::process::Command::new("bash");
        c.args(["-lic", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped());
        // 三条策略（`00 §1.5.2`）：`Hidden`（探针绝不该在用户桌面上闪窗口）·
        // `JobKillOnClose`（超时那条路要把 `-lic` 起出来的**整棵**树收掉 —— 用户 rc 里
        // 起了什么我们不知道，只杀 `bash` 本身漏得掉）· `Null`（rc 的抱怨不是我们的诊断，
        // 而这一跳唯一有用的字节在 stdout 上）。
        spawn_managed_cmd(
            &mut c,
            ConsolePolicy::Hidden,
            Lifetime::JobKillOnClose,
            StderrSink::Null,
        )
    })
}

/// 🔴 `K-R69`：**直接问一个二进制**「你是谁」——`<bin> --ccm-probe`，不经 shell。
///
/// # 它与上面那条问的不是同一件事，别混
///
/// [`probe_with`] 问的是「**你 PATH 上那个 `ccm` 是谁**」（所以非走登录 shell 不可 ——
/// 用户的 rc 会改 PATH，`src/shared/ccm-aliases.sh` 里就有一行往前插 `~/.local/bin`）。
/// 本条问的是「**我们放下去的那一份是谁**」，路径我们自己知道 ⇒ 一个 shell 都不需要，
/// 也就不吃用户 rc 的任何影响（那正是它该有的样子：这一份的身份与用户环境无关）。
///
/// ⇒ 两条**共用同一段等待与解析**（[`probe_spawned`]），只有「怎么起那个进程」不同。
/// 两份手写的 wait/read 之间只会漂，而漂开的后果是同一台机器上两个答案。
pub(crate) fn probe_binary_uncached(
    bin: &std::path::Path,
    timeout: std::time::Duration,
) -> CcmProbeResult {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    probe_spawned(timeout, &|| {
        let mut c = std::process::Command::new(bin);
        c.arg("--ccm-probe")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped());
        // 三条策略与上一条逐字相同，理由只有 `Hidden` 那格更重：这一跳在 Windows 上
        // 起的是我们自己放下去的那份 `ccm.exe`（控制台子系统），不带 `CREATE_NO_WINDOW`
        // 就是每问一次身份闪一次黑框。
        spawn_managed_cmd(
            &mut c,
            ConsolePolicy::Hidden,
            Lifetime::JobKillOnClose,
            StderrSink::Null,
        )
    })
}

/// 一次探测的**等待 + 读 + 解析**那一半。「怎么起那个进程」由调用方给。
///
/// 🔴 `K-R69` 抽出来的：本机那条 `ccm` 入口要被**直接**问一次身份（不经 shell），
/// 而「起了之后怎么等」那一段有超时、有读线程、有「超时不采信半截输出」——
/// 抄第二份的话，两条路会在**最难查的那一格**（半截输出）上各说各话。
fn probe_spawned(
    timeout: std::time::Duration,
    spawn: &dyn Fn() -> std::io::Result<crate::spawn_managed::ManagedChild>,
) -> CcmProbeResult {
    use std::io::Read;
    let Ok(mut child) = spawn() else {
        return parse_probe_output("");
    };
    // ⚠ 读线程必须在**等之前**起：管道缓冲写满时子进程会阻塞在 write 上，
    // 那时再怎么等都等不到它退出 —— 「等它退出再读」是个会自锁的顺序。
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut s) = stdout {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = std::time::Instant::now() + timeout;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Err(_) => break false,
            Ok(None) => {}
        }
        if std::time::Instant::now() >= deadline {
            break true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    if timed_out {
        // 杀掉它，读线程随管道关闭而结束 —— 否则那条线程会跟着挂死的子进程一起留下。
        let _ = child.kill();
        let _ = child.wait();
        tracing::debug!("ccm 本机探测超时（{timeout:?}）—— 当作未装，降级回旧路");
    }
    let buf = reader.join().unwrap_or_default();
    if timed_out {
        // 超时那次**不采信半截输出**：`parse_probe_output` 只看首行，
        // 半截的首行恰好可能是 `name=ccm` 而 `capabilities=` 还没来 ⇒ 会被读成「装了但没能力」，
        // 那是个比「没装」更难查的假象。
        return parse_probe_output("");
    }
    parse_probe_output(&String::from_utf8_lossy(&buf))
}

/// 探测远端 `ccm` 是否已装 + 能力集。`command -v` 找不到 → 走 `NO_CCM` 哨兵分支，不报错
/// （未装是正常状态之一，不是异常）。
#[tauri::command]
pub async fn probe_ccm_cli(origin: String) -> Result<CcmProbeResult, String> {
    let cfg = crate::load_remote_config_by_label(&origin)
        .ok_or_else(|| format!("未找到远端配置: {origin:?}"))?;
    let stream = ssh_source::connect_and_exec_cmd(&cfg, CCM_PROBE_CMD).await?;
    let mut reader = BufReader::new(stream);
    let mut buf: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("探测 ccm 失败: {e}"))?;
    Ok(parse_probe_output(&String::from_utf8_lossy(&buf)))
}

// ═══════════════════════════════════════════════════════════════════════════
// 🔴 `K-R69` / `KR69D2`：**装了之后，产品说得出「你 PATH 上那个是旧的」**
// ═══════════════════════════════════════════════════════════════════════════
//
// 用户 `K34` 逐字要的是「**原本的配置要手动删除**」——**产品不删，但要说得出**。
// `K-R62` 已经买到「你 rc 里那几行是旧的」（`profile_installer::scan_legacy_rc_lines`）；
// 这里是**它的兄弟**：**PATH 上那个 `ccm` 是不是我们装的那一份**。
//
// 🔴 **不许只比路径字符串**。比路径认不出「同名不同物」，而本件的题面恰恰就是
//    「本机上另有一个也叫 `ccm` 的东西」（用户 `~/.local/bin/ccm` 那份旧 bash）。
//    ⇒ 比的是**两边自报的身份**：`--ccm-probe` 那条握手现成的，`version=` 与
//    `capabilities=` 就是它交出来的名片。

/// PATH 上那个 `ccm`，与我们装的那一份是什么关系。**四态，没有兜底档。**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum PathCcmVerdict {
    /// 终端里敲 `ccm` 走到的就是我们这一份。**没有话要说。**
    Ours,
    /// 🔴 PATH 上有一个 `ccm`，**但不是我们这一份** —— 这就是「旧的」那一格。
    NotOurs,
    /// PATH 上没有一个答得出 `--ccm-probe` 的 `ccm`。**这不是坏事**：
    /// 没贴别名块之前本来就没有。
    Absent,
    /// **说不出** —— 我们自己那一份都没装 / 探不到，那就没资格判别人。
    /// （查不了要说成「查不了」，不许说成「缺失」：本仓那条老纪律。）
    Undetermined,
}

/// 本机 `ccm` 这一格的全貌：我们那一份 · PATH 上那一份 · 判词 · 那句话。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct LocalCcmEntry {
    /// 我们装的那一份在哪 —— **`$HOME/…` 形态**（别名要用它，写绝对路径会把
    /// 「换台机器 / 换个用户」堵死）。没装就是 `None`。
    pub entry: Option<String>,
    /// 我们那一份自报的身份（直接跑它，不经 shell）。
    pub ours: CcmProbeResult,
    /// 你 PATH 上那个自报的身份（经登录 shell，因为 PATH 就是 rc 决定的）。
    pub on_path: CcmProbeResult,
    pub verdict: PathCcmVerdict,
    /// 给人读的那句话。没有话要说时是**空串**（`Ours` 那一档）。
    pub message: String,
}

/// **纯函数**：两张名片，判 PATH 上那个是不是我们这一份。
///
/// # 判据是「身份」不是「路径」
///
/// `version=` 是这套 CLI 的版本号（`control::ccm::CCM_VERSION`：`4` 是最后一版 bash，
/// `5` 起是后端本体），`capabilities=` 是能力集。两样都相同 ⇒ 判 [`PathCcmVerdict::Ours`]。
///
/// ⚠ **诚实边界，写死别读宽**：两份东西的 `version` 与能力集**完全相同**时本条分不开它们。
/// 那不是漏 —— 那时它们在**行为契约上**就是同一份（消费者按这两样分支，见
/// `ccm_invocation::CLI_REQUIRED_CAPS` 与前端 `ccm-probe.ts`）。
/// 要分「同契约但不同文件」得比字节，而那**不是**这一格要买的东西。
pub fn classify_path_ccm(ours: &CcmProbeResult, on_path: &CcmProbeResult) -> PathCcmVerdict {
    if !ours.installed {
        return PathCcmVerdict::Undetermined;
    }
    if !on_path.installed {
        return PathCcmVerdict::Absent;
    }
    let same_caps = {
        let a: std::collections::BTreeSet<&str> =
            ours.capabilities.iter().map(String::as_str).collect();
        let b: std::collections::BTreeSet<&str> =
            on_path.capabilities.iter().map(String::as_str).collect();
        a == b
    };
    if ours.version == on_path.version && same_caps {
        PathCcmVerdict::Ours
    } else {
        PathCcmVerdict::NotOurs
    }
}

/// **纯函数**：那句话。措辞刻意**不是**「请删除」——
/// 产品一个字节都不删（`K31` ＋ 用户 `K34` 逐字「原本的配置要手动删除」），
/// 边界只有用户自己知道。同 `profile_installer::render_manual_cleanup_hint` 那一族。
pub fn render_path_ccm_hint(
    verdict: PathCcmVerdict,
    ours: &CcmProbeResult,
    on_path: &CcmProbeResult,
    entry: Option<&str>,
) -> String {
    let ours_card = describe_card(ours);
    let where_ours = entry.unwrap_or("（还没装下来）");
    match verdict {
        PathCcmVerdict::Ours => String::new(),
        PathCcmVerdict::NotOurs => format!(
            "🔴 你 PATH 上那个 `ccm` **不是** cc-monitor 装的这一份。\n\
             · 它自报：{}\n\
             · 我们这一份：{ours_card}（在 {where_ours}）\n\
             ⇒ 你在终端里敲 `ccm`（以及任何调 `ccm` 的别名）走到的是**它**，不是我们这一份。\n\
             产品**不动它**：要不要删、什么时候删，由你自己定。想让终端认我们这一份，\n\
             要么把它挪开、要么让上面那个目录排在 PATH 前面、要么用下面生成的那条命令\n\
             （它显式指向我们这一份，不靠 PATH 撞运气）。",
            describe_card(on_path)
        ),
        PathCcmVerdict::Absent => format!(
            "你 PATH 上没有 `ccm`。我们这一份在 {where_ours}（{ours_card}）——\n\
             下面生成的那条命令会显式指向它，不需要你改 PATH。"
        ),
        PathCcmVerdict::Undetermined => format!(
            "说不出你 PATH 上那个 `ccm` 是谁 —— **我们自己这一份没探到**（{where_ours}）。\n\
             这是「查不了」，不是「你缺了什么」。"
        ),
    }
}

/// 一张名片的人话。**不含路径** —— 这一格判的就是「不许只比路径」，
/// 措辞里混进路径会让读的人以为判据比的是它〔固定项 12 那条 `6g` 的同族〕。
fn describe_card(r: &CcmProbeResult) -> String {
    if !r.installed {
        return "探不到（没有一个答得出 `--ccm-probe` 的 ccm）".to_string();
    }
    format!(
        "version={} · {} 项能力",
        r.version.as_deref().unwrap_or("(没报版本)"),
        r.capabilities.len()
    )
}

/// 探我们**自己装的那一份**的上限。它是我们的后端、参数只是打印一张名片 ⇒
/// 不需要 [`LOCAL_PROBE_TIMEOUT`] 那么宽（那一档的宽度是留给用户 rc 的）。
const OURS_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 你 PATH 上那个 `ccm` 是谁。**Windows 上今天答不了，如实回 `None`。**
///
/// 🔴 **登记一条诚实边界，别读成「做了」**：唯一一条「问 PATH 上那个」的机制是
/// [`CCM_PROBE_CMD`] ＋ `bash -lic`（非走登录 shell 不可 —— PATH 就是 rc 决定的，
/// `src/shared/ccm-aliases.sh` 里那行 `export PATH="$HOME/.local/bin:$PATH"` 就是活例）。
/// Windows 上没有对应物，而**照着 `PATH` 变量自己走一遍不是同一件事**
/// （少了 rc 那一层，还要按 `PATHEXT` 判可执行 —— 那一格已经是一条待决 `KU22`）。
/// ⇒ 这里**不发明第二套机制**，回 `None`，由上面那句话说成「查不了」。
#[cfg(not(windows))]
fn probe_path_ccm() -> Option<CcmProbeResult> {
    Some(probe_local_ccm())
}
#[cfg(windows)]
fn probe_path_ccm() -> Option<CcmProbeResult> {
    None
}

/// 🔴 `K-R69` / `KR69D2` 的生产入口：本机 `ccm` 这一格现在是什么样。
///
/// 读面：`~/.cc-monitor/bin/<本机 ccm 入口名>` 在不在（`local_read_surface_registry`
/// 的 `HOME_REACHES` 里登记着 —— 那是 monitor 自己的目录，不是伸手拿用户的东西）。
/// 起进程面：两次 `--ccm-probe`（`write_site_registry::SPAWNS` 里登记着）。
/// **一个字节都不写。**
#[tauri::command]
pub fn local_ccm_entry_status() -> LocalCcmEntry {
    let home = dirs::home_dir();
    let path = home.as_ref().map(|h| {
        h.join(".cc-monitor")
            .join("bin")
            .join(crate::backend::control::local_backend::local_ccm_entry_name())
    });
    let installed = path.as_ref().filter(|p| p.is_file());
    let ours = match installed {
        Some(p) => probe_binary_uncached(p, OURS_PROBE_TIMEOUT),
        None => parse_probe_output(""),
    };
    // ⚠ **`$HOME/…` 形态，不是绝对路径**：这个串会被别名生成器嵌进用户的 shell 命令里，
    //   而用户 09-11 明裁「这些命令都是可以自定义的」（`R19`）⇒ 写死绝对路径把自定义堵死，
    //   换台机器 / 换个用户也当场失效。
    let entry = installed.map(|_| {
        format!(
            "$HOME/.cc-monitor/bin/{}",
            crate::backend::control::local_backend::local_ccm_entry_name()
        )
    });
    let on_path = probe_path_ccm().unwrap_or_else(|| parse_probe_output(""));
    let verdict = classify_path_ccm(&ours, &on_path);
    let message = render_path_ccm_hint(verdict, &ours, &on_path, entry.as_deref());
    LocalCcmEntry {
        entry,
        ours,
        on_path,
        verdict,
        message,
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/ccm_probe_tests.rs"]
mod tests;
