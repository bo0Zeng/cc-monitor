//! F03（unify-launch）：探测 `ccm`（F02 统一启动 CLI）装没装及其能力集。
//! 〔MIG-2 · `99 §2.1 ⑬`〕远端那一条（`probe_ccm_cli`，一次性 headless SSH exec，照当年 F60 抓屏那条
//! `capture_remote_pane`〔散文墓碑〕的范式）与前端缓存 `src/ccm-probe.ts` 删了：
//! 起会话的渲染住进那台后端，装没装由它在自己机器上现查（`src/backend/control/launch_render/wire.rs::render_ccm_launch`：`ccm` 就是那台后端本身（V28），能力是它自己的）。
//! 本文件今天只剩**本机 PATH 上那个 `ccm`** 的探测（`local_ccm_entry_status` 那一族用）。

use crate::copy_table::copy_text;
use serde::Serialize;

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
    /// 〔FIX3 · `99 §2.2 ㉔`〕登录 shell 里 `command -v ccm` 答的那一句（一般是一个路径；函数 / 别名时是它们自己的写法）。
    /// 只有「问 PATH 上那个」那一条探针带它；它不参与「答没答出名片」（`installed`）—— 答不出名片的旧入口照样有住址。
    #[cfg_attr(test, ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

/// 解析 `ccm --ccm-probe` 的输出。首行非字面 `name=ccm` → 判定未装/不兼容——防止 PATH 里
/// 已有同名但无关的自定义 `ccm`（用户自己的脚本）被误判为本工具的 CLI。
fn parse_probe_output(out: &str) -> CcmProbeResult {
    // 〔FIX3〕`at=` 那一行是探针自己补在最后的（[`CCM_PROBE_CMD`]），不归名片 ⇒ 先摘出来、不论名片认不认得。
    let at = out
        .lines()
        .filter_map(|l| l.strip_prefix("at="))
        .next_back()
        .filter(|a| !a.is_empty())
        .map(str::to_string);
    let mut lines = out.lines();
    if lines.next() != Some("name=ccm") {
        return CcmProbeResult {
            installed: false,
            version: None,
            capabilities: vec![],
            build: None,
            at,
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
        at,
    }
}

/// 探「本机交互 shell 的 `PATH` 上那个 `ccm`」的命令（交给 `bash -lic`）。
///
/// 〔E2〕远端那一跳不再用它：`ccm` 就是那台后端本身，改问那台后端的 `ccm-probe`（〔MIG-3b〕今天界面经通道直问，`src/ccm-probe.ts`）。
/// 本机仍问它：`KR69D2`「你 PATH 上那个是不是我们这一份」答的正是交互 shell 的 `PATH`。
/// 〔FIX3 · `99 §2.2 ㉔`〕名片之后补一行 `at=<command -v ccm>`：敲 `ccm` 走到的是哪一份（旧入口答不出名片也说得出住址）。
const CCM_PROBE_CMD: &str = "command -v ccm >/dev/null 2>&1 && { ccm -- --ccm-probe; printf '\\nat=%s\\n' \"$(command -v ccm)\"; } || printf 'NO_CCM\\n'";

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
/// 调用方是 〔MIG-2〕原先是 `history.rs` 的本机拉起（`resume_history_session` / `new_local_session`〔散文墓碑〕
/// 两个 `#[tauri::command]` 的同步调用链）。远端那条是 async 因为 ssh 本来就要 await；
/// 本机没有那一跳，`bash -lic` 直接跑 —— 为了它把整条链改成 async 是纯粹的传染。
///
/// # 为什么必须真跑一次，而不是渲染完让 shell 自己探
///
/// 旧路（`build_local_posix_command`）用的是 shell 里的 `if command -v cc; then …; else …; fi` 〔散文墓碑〕
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
        // 〔V151〕`ccm -- --ccm-probe`：ccm 自己的诊断口写在 `--` 右边。
        c.args(["--", "--ccm-probe"])
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
    // 〔W5-VIS · E 吞错普查点名〕读错**带回来**：原先 `let _ =` 吞掉，读坏了与「没输出」长得一样（都判成没装）。
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let read = match stdout {
            Some(mut s) => s.read_to_end(&mut buf).err().map(|e| e.to_string()),
            None => None,
        };
        (buf, read)
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
    let (buf, read_err) = reader.join().unwrap_or_default();
    if let Some(e) = &read_err {
        tracing::warn!(
            "本机 ccm 探测：读它的输出出错（{e}）—— 这一次按没装处理，但那不是「确认没装」"
        );
        // 读坏了的那一截同超时那一形：**不采信半截输出**（理由见下面超时那一支）。
        return parse_probe_output("");
    }
    if timed_out {
        // 超时那次**不采信半截输出**：`parse_probe_output` 只看首行，
        // 半截的首行恰好可能是 `name=ccm` 而 `capabilities=` 还没来 ⇒ 会被读成「装了但没能力」，
        // 那是个比「没装」更难查的假象。
        return parse_probe_output("");
    }
    parse_probe_output(&String::from_utf8_lossy(&buf))
}

// 🪦〔MIG-2 · `99 §2.1 ⑬`〕这里原来是 Tauri 命令 `probe_ccm_cli`〔散文墓碑〕（界面先问那台后端 `ccm-probe`、再把结果带去渲染）：
//   `ccm …` 调用行的渲染进了那台后端，能力问它自己（`launch_render/wire.rs::render_ccm_launch`），这一跳删了。

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
    /// 〔FIX3 · `99 §2.2 ㉔` · `15 §5.4 D5`〕机器列表本机那一格（`ccm`）记什么：`Some(true)` = 两件都成
    /// （我们那份装下来了 ＋ 登录 shell 里敲 `ccm` 走到的就是它）· `Some(false)` = 有一件不成 · `None` = 说不清（不写账本）。
    pub ok: Option<bool>,
    /// 那一格的一句话，两件都说。
    pub summary: String,
}

/// 〔FIX3 · `99 §2.2 ㉔`〕登录 shell 里敲 `ccm` 落在哪 —— 与我们落点上那一份是不是**同一个文件**。
///
/// 名片（[`classify_path_ccm`]）分不开「同一套契约、不同文件」（旧 shim 转给一份同版本的后端就是这样）；
/// ㉔ 问的是「走到的是不是它」⇒ 能落到文件上的先比文件本身（解过链接），落不到文件上的（函数 / 别名）才回退比名片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    /// `command -v ccm` 什么都没答。
    Nothing,
    /// 解过链接就是我们落点上那一份。
    Landing,
    /// 另一个文件。`old_entry` = 认得出是 cc-monitor 早先放的旧入口（三行 shim / bash 启动器，`deploy_core::is_ours`）。
    OtherFile { path: String, old_entry: bool },
    /// 不是一个文件路径（shell 函数 / 别名）。
    NotAFile,
}

/// `at` = 登录 shell 里 `command -v ccm` 的答；`landing` = 我们那一份的绝对路径（没装 ⇒ `None`）。
pub(crate) fn reach_of(at: Option<&str>, landing: Option<&std::path::Path>) -> Reach {
    let Some(at) = at else {
        return Reach::Nothing;
    };
    let p = std::path::Path::new(at);
    if !p.is_absolute() || !p.is_file() {
        return Reach::NotAFile;
    }
    let same = |a: &std::path::Path, b: &std::path::Path| matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y);
    if landing.is_some_and(|l| same(p, l)) {
        return Reach::Landing;
    }
    // 只读开头一小截认记号（记号在第二行）；读不到就当认不出。
    let head: Vec<u8> = std::fs::read(p)
        .map(|b| b.into_iter().take(4096).collect())
        .unwrap_or_default();
    Reach::OtherFile {
        path: at.to_string(),
        old_entry: deploy_core::is_ours(&String::from_utf8_lossy(&head)),
    }
}

/// **纯函数**：名片 ＋ 落在哪 ⇒ 判词。我们自己那份没装 ⇒ 说不出；落到文件上的按文件判，落不到的回退比名片。
pub(crate) fn judge_path_ccm(
    ours: &CcmProbeResult,
    on_path: &CcmProbeResult,
    reach: &Reach,
) -> PathCcmVerdict {
    if !ours.installed {
        return PathCcmVerdict::Undetermined;
    }
    match reach {
        Reach::Nothing => PathCcmVerdict::Absent,
        Reach::Landing => PathCcmVerdict::Ours,
        Reach::OtherFile { .. } => PathCcmVerdict::NotOurs,
        Reach::NotAFile => classify_path_ccm(ours, on_path),
    }
}

/// **纯函数**：本机那一格记什么（[`LocalCcmEntry::ok`] ＋ [`LocalCcmEntry::summary`]）。
/// `landed` = 落点上有文件 · `ours_bytes` = 那份字节是我们编的后端。
pub(crate) fn local_ccm_cell(
    landed: bool,
    ours_bytes: bool,
    verdict: PathCcmVerdict,
    on_path: &CcmProbeResult,
) -> (Option<bool>, String) {
    if !landed {
        return (Some(false), copy_text("rsCcmProbe.cell.notLanded", &[]));
    }
    if !ours_bytes {
        return (Some(false), copy_text("rsCcmProbe.cell.notOurBytes", &[]));
    }
    match verdict {
        PathCcmVerdict::Ours => (Some(true), copy_text("rsCcmProbe.cell.ours", &[])),
        PathCcmVerdict::NotOurs => (
            Some(false),
            copy_text(
                "rsCcmProbe.cell.elsewhere",
                &[(
                    "at",
                    &on_path.at.clone().unwrap_or_else(|| "ccm".to_string()),
                )],
            ),
        ),
        PathCcmVerdict::Absent => (Some(false), copy_text("rsCcmProbe.cell.absent", &[])),
        // 我们那份字节是对的、却问不出名片 / 这台问不了 PATH ⇒ 说不清，不替用户下结论。
        PathCcmVerdict::Undetermined => (None, String::new()),
    }
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
    old_entry: bool,
) -> String {
    let ours_card = describe_card(ours);
    let not_installed = copy_text("rsCcmProbe.hint.notInstalled", &[]);
    let where_ours = entry.unwrap_or(&not_installed);
    // 〔FIX3 · ㉔〕走到别处就明说是哪一份：`command -v ccm` 答的原话。
    let at = on_path.at.clone().unwrap_or_else(|| "ccm".to_string());
    match verdict {
        PathCcmVerdict::Ours => String::new(),
        // 认得出是我们早先放的旧入口 ⇒ 说清怎么清（不代清，V157 ③）。
        PathCcmVerdict::NotOurs if old_entry => copy_text(
            "rsCcmProbe.hint.oldEntry",
            &[("at", &at), ("where", &where_ours.to_string())],
        ),
        PathCcmVerdict::NotOurs => copy_text(
            "rsCcmProbe.hint.notOurs",
            &[
                ("at", &at),
                ("theirs", &(describe_card(on_path)).to_string()),
                ("ours", &ours_card.to_string()),
                ("where", &where_ours.to_string()),
            ],
        ),
        PathCcmVerdict::Absent => copy_text(
            "rsCcmProbe.hint.absent",
            &[
                ("where", &where_ours.to_string()),
                ("ours", &ours_card.to_string()),
            ],
        ),
        PathCcmVerdict::Undetermined => copy_text(
            "rsCcmProbe.hint.undetermined",
            &[("where", &where_ours.to_string())],
        ),
    }
}

/// 一张名片的人话。**不含路径** —— 这一格判的就是「不许只比路径」，
/// 措辞里混进路径会让读的人以为判据比的是它〔固定项 12 那条 `6g` 的同族〕。
fn describe_card(r: &CcmProbeResult) -> String {
    if !r.installed {
        return copy_text("rsCcmProbe.card.notFound", &[]);
    }
    copy_text(
        "rsCcmProbe.card.summary",
        &[
            (
                "version",
                &(r.version
                    .as_deref()
                    .unwrap_or(&copy_text("rsCcmProbe.card.noVersion", &[])))
                .to_string(),
            ),
            ("count", &(r.capabilities.len()).to_string()),
        ],
    )
}

/// 探我们**自己装的那一份**的上限。它是我们的后端、参数只是打印一张名片 ⇒
/// 不需要 [`LOCAL_PROBE_TIMEOUT`] 那么宽（那一档的宽度是留给用户 rc 的）。
const OURS_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// 你 PATH 上那个 `ccm` 是谁。**Windows 上今天答不了，如实回 `None`。**
///
/// 🔴 **登记一条诚实边界，别读成「做了」**：唯一一条「问 PATH 上那个」的机制是
/// [`CCM_PROBE_CMD`] ＋ `bash -lic`（非走登录 shell 不可 —— PATH 就是 rc 决定的，
/// `src/shared/ccm-aliases.sh` 里那行往 `PATH` 前插 `~/.cc-monitor/bin` 的就是活例）。
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

/// 〔E2 · `96 §7.2.2`〕这个文件的字节是不是我们编的后端（身份戳恰一个）—— 只读字节，不跑它。
pub(crate) fn ours_by_bytes(p: &std::path::Path) -> bool {
    std::fs::read(p).is_ok_and(|b| {
        matches!(
            deploy_core::identity_of_bytes(&b, crate::sftp::STAMP_MARKS),
            deploy_core::RemoteIdentity::Stamp(_)
        )
    })
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
    // 〔E2 · `96 §7.2.2`〕**先读字节认身份，再决定跑不跑**：落点上那一份自报的身份戳恰一个（是我们编的后端）才起它问
    //   `--ccm-probe`；认不出（不是我们的 / 读不了）⇒ 不跑，按「没装我们这一份」答。
    let ours_bytes = installed.is_some_and(|p| ours_by_bytes(p));
    let ours = match installed {
        Some(p) if ours_bytes => probe_binary_uncached(p, OURS_PROBE_TIMEOUT),
        _ => parse_probe_output(""),
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
    // 这台问不了 PATH（Windows）⇒ 说不清，不说成「没有」。
    let (on_path, verdict, old_entry) = match probe_path_ccm() {
        None => (parse_probe_output(""), PathCcmVerdict::Undetermined, false),
        Some(on_path) => {
            let reach = reach_of(on_path.at.as_deref(), installed.map(|p| p.as_path()));
            let old_entry = matches!(
                reach,
                Reach::OtherFile {
                    old_entry: true,
                    ..
                }
            );
            let v = judge_path_ccm(&ours, &on_path, &reach);
            (on_path, v, old_entry)
        }
    };
    let message = render_path_ccm_hint(verdict, &ours, &on_path, entry.as_deref(), old_entry);
    let (ok, summary) = local_ccm_cell(installed.is_some(), ours_bytes, verdict, &on_path);
    LocalCcmEntry {
        entry,
        ours,
        on_path,
        verdict,
        message,
        ok,
        summary,
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/ccm_probe_tests.rs"]
mod tests;
