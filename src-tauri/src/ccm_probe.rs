//! F03（unify-launch）：探测远端是否已装 `ccm`（F02 统一启动 CLI）及其能力集，供前端
//! `src/ccm-probe.ts` 决定走 CLI 渲染器还是兜底渲染器。一次性 headless SSH exec，照
//! `tmux.rs::capture_remote_pane` 的范式（通道 B，不干扰前台终端、不涉及 daemon）。

use crate::ssh_source;
use serde::Serialize;
use tokio::io::{AsyncReadExt, BufReader};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
    probe_spawned(timeout, &|| {
        std::process::Command::new("bash")
            .args(["-lic", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
    })
}

/// 🔴 `K-R69`：**直接问一个二进制**「你是谁」——`<bin> --ccm-probe`，不经 shell。
///
/// # 它与上面那条问的不是同一件事，别混
///
/// [`probe_with`] 问的是「**你 PATH 上那个 `ccm` 是谁**」（所以非走登录 shell 不可 ——
/// 用户的 rc 会改 PATH，`shared/ccm-aliases.sh` 里就有一行往前插 `~/.local/bin`）。
/// 本条问的是「**我们放下去的那一份是谁**」，路径我们自己知道 ⇒ 一个 shell 都不需要，
/// 也就不吃用户 rc 的任何影响（那正是它该有的样子：这一份的身份与用户环境无关）。
///
/// ⇒ 两条**共用同一段等待与解析**（[`probe_spawned`]），只有「怎么起那个进程」不同。
/// 两份手写的 wait/read 之间只会漂，而漂开的后果是同一台机器上两个答案。
pub(crate) fn probe_binary_uncached(
    bin: &std::path::Path,
    timeout: std::time::Duration,
) -> CcmProbeResult {
    probe_spawned(timeout, &|| {
        std::process::Command::new(bin)
            .arg("--ccm-probe")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
    })
}

/// 一次探测的**等待 + 读 + 解析**那一半。「怎么起那个进程」由调用方给。
///
/// 🔴 `K-R69` 抽出来的：本机那条 `ccm` 入口要被**直接**问一次身份（不经 shell），
/// 而「起了之后怎么等」那一段有超时、有读线程、有「超时不采信半截输出」——
/// 抄第二份的话，两条路会在**最难查的那一格**（半截输出）上各说各话。
fn probe_spawned(
    timeout: std::time::Duration,
    spawn: &dyn Fn() -> std::io::Result<std::process::Child>,
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
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
/// `shared/ccm-aliases.sh` 里那行 `export PATH="$HOME/.local/bin:$PATH"` 就是活例）。
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
mod tests {
    use super::*;

    /// 🔴 `KR69D2`：**「你 PATH 上那个是旧的」这句话真的说得出来**，而且判的不是路径。
    ///
    /// # 死值验（`KR69D2` 逐字要的那一格）
    ///
    /// 造一个「PATH 上有旧 `ccm`、而我们也装了一份」的场景 ⇒ **必须指名说出来**。
    /// 把 [`render_path_ccm_hint`] 里 `NotOurs` 那一支的话掏空 ⇒ 本条红。
    ///
    /// # 失效方向（本条专门盯着它）
    ///
    /// **只比路径**。下面第三格喂的是「两份东西**路径可以完全一样**、身份不同」——
    /// 本条一个路径字符串都不看，所以那条路走不通。
    #[test]
    fn a_stale_ccm_on_path_is_named_out_loud() {
        let ours = CcmProbeResult {
            installed: true,
            version: Some("5".into()),
            capabilities: vec!["new".into(), "resume".into(), "detach".into()],
            build: None,
        };
        // 用户机器上那份 2026-07-27 的旧 bash：答得出 `--ccm-probe`（所以「在不在」判不了它），
        // 但版本与能力集都是上一代。
        let legacy = CcmProbeResult {
            installed: true,
            version: Some("4".into()),
            capabilities: vec!["new".into(), "resume".into()],
            build: None,
        };
        let v = classify_path_ccm(&ours, &legacy);
        assert_eq!(v, PathCcmVerdict::NotOurs, "旧的没被认出来");
        let hint = render_path_ccm_hint(v, &ours, &legacy, Some("$HOME/.cc-monitor/bin/ccm"));
        assert!(
            hint.contains("不是") && hint.contains("version=4") && hint.contains("version=5"),
            "那句话没把「它是谁 / 我们是谁」摆出来 —— 只说「不一样」等于没说：\n{hint}"
        );
        // 🔴 产品**不删**：措辞里不许出现祈使的「请删除」。
        assert!(
            !hint.contains("请删除"),
            "产品在催用户删他自己的东西 —— `K34` 逐字「原本的配置**要手动删除**」，\n\
             那是**用户的**动作；`K31` 更不许我们代劳。这一格只许指名。\n{hint}"
        );
        // ★ 同版本同能力 ⇒ 判 `Ours`，而且**没有话要说**（免得每次打开都吓人一跳）。
        assert_eq!(classify_path_ccm(&ours, &ours), PathCcmVerdict::Ours);
        assert_eq!(
            render_path_ccm_hint(PathCcmVerdict::Ours, &ours, &ours, None),
            ""
        );
        // ★ 我们自己那份没装 ⇒ **说不出**，不许说成「你 PATH 上那个是旧的」。
        let nothing = parse_probe_output("");
        assert_eq!(
            classify_path_ccm(&nothing, &legacy),
            PathCcmVerdict::Undetermined,
            "我们自己都没装，却对别人下了判词 —— 那就是替用户下一个他没做过的结论"
        );
        // ★ PATH 上什么都没有 ⇒ `Absent`，与「是旧的」分得开。
        assert_eq!(classify_path_ccm(&ours, &nothing), PathCcmVerdict::Absent);
    }

    /// `KR69D2` 的**失效方向那一格**：判词**不看路径**。
    ///
    /// 两张名片身份不同，而「它们在哪」这件事本条压根问不到 ——
    /// [`classify_path_ccm`] 的签名里没有路径。这条断的是**签名**买到的性质：
    /// 有人想把它改成比路径，得先改签名，那已经是明知故犯了。
    #[test]
    fn the_verdict_cannot_be_reached_by_comparing_paths() {
        let a = CcmProbeResult {
            installed: true,
            version: Some("5".into()),
            capabilities: vec!["new".into()],
            build: None,
        };
        let b = CcmProbeResult {
            installed: true,
            version: Some("5".into()),
            capabilities: vec!["new".into(), "detach".into()],
            build: None,
        };
        // 同版本、能力集差一项 ⇒ 仍判「不是我们那一份」。**路径在这里根本不存在。**
        assert_eq!(classify_path_ccm(&a, &b), PathCcmVerdict::NotOurs);
        // 反向：能力集顺序不同不算不同（那是名片的写法，不是身份）。
        let b2 = CcmProbeResult {
            installed: true,
            version: Some("5".into()),
            capabilities: vec!["detach".into(), "new".into()],
            build: None,
        };
        let a2 = CcmProbeResult {
            installed: true,
            version: Some("5".into()),
            capabilities: vec!["new".into(), "detach".into()],
            build: None,
        };
        assert_eq!(
            classify_path_ccm(&a2, &b2),
            PathCcmVerdict::Ours,
            "同一套能力换个顺序被判成两个东西 —— 那会让用户每次打开都看到一句假警报"
        );
    }

    #[test]
    fn parses_real_probe_output() {
        let out = "name=ccm\nversion=1\nself=/x/ccm\ncapabilities=new,resume,attach,tmux,account,cwd,agent,launcher,ccm-sid,print\nagents=claude,codex\n";
        let r = parse_probe_output(out);
        assert!(r.installed);
        assert_eq!(r.version.as_deref(), Some("1"));
        assert!(r.capabilities.contains(&"tmux".to_string()));
        assert!(r.capabilities.contains(&"ccm-sid".to_string()));
    }

    /// ★★ 🔴 `KR70D1`（09-12）：**产品从「那个进程自己」手里拿到构建身份。**
    ///
    /// # 它买的是哪一格
    ///
    /// `K-R68` 摸底的结论逐字：后端二进制的身份今天只能去读它**旁边**那个 `.build_id`
    /// 文本文件，而那是 `release.yml` 从源码常量抠出来写的一张标签 ——
    /// 三个载体的标签恒等 ⇒ 一格证据都不提供（`DECISIONS.md#R26` 裁定零）。
    /// [`probe_binary_uncached`] 这条路是**直接问那个二进制**（`<bin> --ccm-probe`，
    /// 不经 shell、不读它旁边任何文件），本条钉住那条握手**答得出身份**。
    ///
    /// # 三格
    ///
    /// ① 有 `build=` ⇒ 拿得到；② 旧后端（没有那一行）⇒ `None`，**不许猜**；
    /// ③ 它**不参与** [`classify_path_ccm`] 的判词（理由住 `CcmProbeResult::build` 的头注：
    /// 同一份后端的两个构建仍然是「我们这一份」）。
    #[test]
    fn the_probe_carries_the_build_identity_of_the_binary_itself() {
        let with = parse_probe_output(
            "name=ccm\nversion=5\nself=/x/ccm\ncapabilities=new\nagents=claude\nbuild=p9-sample\n",
        );
        assert_eq!(
            with.build.as_deref(),
            Some("p9-sample"),
            "对面自报了身份而我们没接住 —— 「问一份二进制它是谁」这条路断在解析这一跳"
        );
        // ② 旧后端不吐这一行 ⇒ 只许答「不知道」，不许拿别处的值顶上。
        let without = parse_probe_output(
            "name=ccm\nversion=5\nself=/x/ccm\ncapabilities=new\nagents=claude\n",
        );
        assert_eq!(
            without.build, None,
            "对面没说，我们替它编了一个 —— 那正是「把失败面换成假答案」那一族"
        );
        // ③ 身份不参与「是不是我们那一份」的判词。
        let a = CcmProbeResult {
            installed: true,
            version: Some("5".into()),
            capabilities: vec!["new".into()],
            build: Some("p9-old".into()),
        };
        let b = CcmProbeResult {
            build: Some("p9-new".into()),
            ..a.clone()
        };
        assert_eq!(
            classify_path_ccm(&a, &b),
            PathCcmVerdict::Ours,
            "两个构建的同一份后端被判成「不是我们那一份」—— \n\
             那会让用户每次打开都看到一句假警报（`CcmProbeResult::build` 头注逐字写着这条边界）"
        );
    }

    /// ★★ **超时那条路真的会返回**〔D 阶段补审 08-12〕。
    ///
    /// 这条**必须真起一个挂住的子进程**，不能拿构造好的字符串喂 `parse_probe_output` ——
    /// 本件要防的东西恰恰不在解析里，在「等」这一步：没有上限时用户点一次「恢复」
    /// 就是永远转圈，而那个形态**任何纯函数判据都看不见**。
    ///
    /// 用 50ms 的上限去等一个睡 30s 的 `bash`：
    /// ① 必须在远早于 30s 的时间内返回（否则超时根本没生效）；
    /// ② 结论必须是 `installed: false`（超时当没装 ⇒ 调用方降级回旧路，而不是拿半截当真）。
    ///
    /// 射程：够得到「会返回、且返回什么」；**够不到**「子进程真被收干净了」——
    /// 那要看 `/proc`，而本条不去做那件事（`kill` + `wait` 已在生产段，收尸由它负责）。
    /// ★ 生产段喂给 [`probe_with`] 的命令**只有那个常量**。
    ///
    /// 上一条判据为了能测「挂住」把命令做成了参数。那就开了一个口：
    /// 谁都能从生产段传别的串进去，而**行为判据看不见这件事**（传什么它都照跑）。
    /// ⇒ 这条按源码钉：生产段（剥掉测试段）里 `probe_with(` 只许出现一次，且实参是 `CCM_PROBE_CMD`。
    #[test]
    fn the_only_production_probe_command_is_the_constant() {
        let prod = guard_core::production_code(include_str!("ccm_probe.rs"));
        // ⚠ 排掉**定义行**：`fn probe_with(` 自己也含这个串。
        // 「判据被自己要钉的那个名字命中」本会话第五次 —— 它不是偶发，是按名字取样的固有形态。
        // ⚠ 取的是**整行**，不是「从命中处到行尾」——第一版取后者，于是定义行切出来的是
        // `probe_with(timeout: …)`，`fn ` 被切在了前面，滤不掉。
        // 「判据被自己要钉的那个名字命中」本会话第五次，而这次连**补的那道滤网也切错了**。
        let calls: Vec<&str> = prod
            .lines()
            .filter(|l| l.contains("probe_with(") && !l.contains("fn probe_with("))
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "生产段里 `probe_with(` 出现 {} 次 —— 不是恰好一次，那个「命令是参数」的口就管不住了：{calls:?}",
            calls.len()
        );
        assert!(
            calls[0].contains("CCM_PROBE_CMD"),
            "生产段调 `probe_with` 时喂的不是 `CCM_PROBE_CMD` —— \n\
             那个参数只为判据而开（要一个「一定挂住」的串），生产侧不许借它跑别的东西。实得：{}",
            calls[0]
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn a_hanging_shell_does_not_hang_the_resume_button() {
        let t0 = std::time::Instant::now();
        // ⚠ 上限要**跨过 rc 加载**（本机 `bash -lic` 实测 ~130ms）：给 50ms 的话子进程
        // 还没来得及吐字就被杀了，「半截输出」那一格永远构造不出来 —— 第三版夹具卡在这儿。
        // 800ms 足够它吐完首行 + 填充，又远小于下面 5s 那道断言。
        let r = probe_local_ccm_uncached_for_test_sleep(std::time::Duration::from_millis(800));
        let took = t0.elapsed();
        assert!(
            took < std::time::Duration::from_secs(5),
            "等了 {took:?} —— 超时没生效。生产上这意味着「恢复」按钮永远转圈，\n\
             而且锁一握不放之后**每一次**本机 resume 都堵在这里（D 阶段补审那两条的合体）"
        );
        assert!(
            !r.installed,
            "超时之后回了 `installed: true` —— 那会拿一段没读完的输出当能力集，\n\
             半截的首行恰好可能是 `name=ccm` 而 `capabilities=` 还没来 ⇒ 被读成「装了但没能力」，\n\
             那是个比「没装」更难查的假象"
        );
    }

    /// 上一条的夹具：**先吐半截、再挂住**，其余与生产逐字同一条路径。
    ///
    /// ⚠⚠ 第一版这里是光秃秃的 `sleep 30`（零输出），于是「超时那次不采信半截输出」
    /// 那条分支**判据根本够不到** —— 变异「超时后照样解析」当场**绿**。
    /// 零输出时「采信」与「不采信」的结果恰好相同，那不是它被守住了，是它没被测。
    /// ⇒ 夹具必须真吐出 `name=ccm` 再挂住：那正是生产上最难查的那一格，
    ///   首行成立而 `capabilities=` 还没来 ⇒ 被读成「装了但一个能力都没有」。
    #[cfg(not(windows))]
    fn probe_local_ccm_uncached_for_test_sleep(timeout: std::time::Duration) -> CcmProbeResult {
        // ⚠ 光 `printf` 冲不出来：`bash` 的 stdout 是管道 ⇒ **块缓冲**，
        // 杀掉时那半截还在它自己的缓冲区里，读到的是空 —— 第二版夹具就是在这儿又绿了一次。
        // 补一段够大的填充把缓冲挤过去，首行才真的到得了读端。
        probe_with(
            timeout,
            "printf 'name=ccm\\n'; head -c 200000 /dev/zero | tr '\\0' x; sleep 30",
        )
    }

    #[test]
    fn no_ccm_sentinel_not_installed() {
        let r = parse_probe_output("NO_CCM\n");
        assert!(!r.installed);
        assert_eq!(r.capabilities.len(), 0);
    }

    #[test]
    fn unrelated_same_name_binary_not_installed() {
        // PATH 上恰好有个不相关的同名 `ccm` 脚本——首行不是 "name=ccm" 就必须判定未装，
        // 不能因为 rc=0 就当已装（防止把用户自己的脚本误当成本工具的 CLI）。
        let r = parse_probe_output("some unrelated program output\n");
        assert!(!r.installed);
    }

    #[test]
    fn empty_output_not_installed() {
        let r = parse_probe_output("");
        assert!(!r.installed);
    }
}
