//! P2s（定框 `C8`）：**本机 daemon 的生命周期** —— 起 / 停 / 状态。
//!
//! # 为什么它不住 `lib.rs`
//!
//! ⚠ **本段刻意不把那个宏的名字连着 `!` 写全** —— `parity_ledger` 的命令清单解析器取的是
//! `lib.rs` 里**第一处**那个字面量，散文里写全会被它当成清单去解析（实测：只解析出 1 条命令）。
//! 这已经是本轮第二次被自己的散文绊到（另一次是位置比较判据扫 `rfind` 带括号那个）。
//!
//! `daemon_status` 是 `#[tauri::command]`，而 tauri 的 IPC 命令清单宏会把命令的辅助宏
//! **`use` 回它所在的那个模块** ⇒ 命令与 `generate_handler!` 同住一个模块必然重名冲突
//! （实测 `E0255: __cmd__daemon_status is defined multiple times / reimported here`）。
//! 仓里每条命令都写成 `模块::名字`，正是这个缘故。**这是结构性理由，不是嫌 `lib.rs` 长。**
//!
//! # 它与 `backend/control/local_backend.rs` 的分工
//!
//! 那边是**平台无关的监护机制**（起、看住、判死、重起）。这边是**宿主知识**：
//! 落点目录在哪、当前 arch 是什么、平台怎么置可执行位、句柄存哪。
//! **`backend-split` 的 C10**〔用 08-01〕要求前者不认识后者，所以两边不能合并。
//!
//! ⚠ **编号在跨工作区之间会撞**：`control-parity` 也有一条 C10，讲的是**单 exe 内嵌自释放**
//! （完全不同的事）。一次补审就是因为只读了后者的定框，把这里的引用判成了「引错 charter」。
//! ⇒ 代码里引 charter **一律带工作区名**，裸编号在几个月后没人分得清指哪一条。

use crate::backend::control::local_backend::{self, Resolved, SuperviseHandle};

/// F05a：本机后端监护句柄。存起来是为了退出前按策略 `stop()`（`C8`）。
///
/// ⚠ **P2s 把它从 `OnceLock` 换成了 `Mutex<Option<_>>`，理由是结构性的**：
/// `SuperviseHandle::stop()` 把 `stopping` 永久置位、并杀掉当前子进程 ⇒ **那个句柄之后就是死的**，
/// 再起必须换一个新的。`OnceLock` 写一次就锁死 ⇒ 有「停」就不可能有「再起」，
/// 而 `C8`② 要的正是「起 / 停 / 状态」三件。
pub static LOCAL_BACKEND: std::sync::Mutex<Option<SuperviseHandle>> = std::sync::Mutex::new(None);

/// P2s（`C8`②）：**起本机后端并把句柄存进 `LOCAL_BACKEND`**。
///
/// 从 `run()` 里抽出来，因为「再起」要走同一条路 —— 抽出来之前，
/// 那段宿主知识（落点目录 / 当前 arch / 平台注入）只在 `run()` 的一个块里存在，
/// 「停了还能起回来」就无从谈起。
///
/// **已经在跑就不重复起**：`C8`① 是「每台机各一个」。
///
/// # ⚠ 诚实边界 11b：这里的「一个」只到「**一个 monitor 进程内一份**」
///
/// 判据是 `LOCAL_BACKEND`（进程内的 `Mutex<Option<_>>`）⇒ 同机**两个 monitor 进程**
/// 仍然是两个 daemon，而且互相认不到。
///
/// ⚠ 补审 08-11 量到这条比原先登记的更糟：`tauri_plugin_single_instance` **只在
/// `#[cfg(windows)]` 注册**（`lib.rs` 那处）⇒ **Linux/macOS 上两个 monitor 天然能并存**，
/// 连那道兜底都没有。它们会撞同一个 `~/.cc-monitor/bin/.<name>.partial`（补审 C2）。
/// ⇒ 真正的「每台机一个」要等 `P2d`（daemon 自己有监听口 + 起时认已有实例）。
/// 起本机后端的结局 —— **三态，不是两态**〔D 阶段补审 08-11 新增，A6〕。
///
/// 原来三种结局全塞在 `Resolved` 里：`Found` 与两种 `Missing`（「已经在跑」与「起不来」）。
/// 而 `daemon_control::daemon_start` 把 `Missing{reason}` 当 `Ok(reason)` 返回
/// ⇒ 「没内嵌 daemon」「释放失败」「已经在跑」三种完全不同的结局在前端**都走 `console.info`**，
/// **一个 toast 都不弹**（补审 A6）。
///
/// 要分开就得在类型上分开 —— 靠 `reason` 字符串去猜是哪一种，是下一个人一定会写错的东西。
pub enum StartOutcome {
    /// 起来了。
    Started(std::path::PathBuf),
    /// 已经在跑（`C8`①）—— **不是失败**。
    AlreadyRunning,
    /// 起不来：没内嵌 / 释放失败 / exe 旁边也没有。
    Failed {
        reason: String,
        looked_at: Vec<std::path::PathBuf>,
    },
}

// ══════════════════════════════════════════════════════════════════════════
// `K-P1`：常驻那条路 —— **真脱离 · 一个监听口 · 起时认得出已有实例**
//
// 这一段全部住在**宿主知识层**，理由是硬的（`K-P1 §0b-5` 现打）：
// `process_group(0)` 来自 `std::os::unix::process::CommandExt`，而 `std::os::unix`
// 在 `backend/mod.rs::the_backend_half_stays_platform_agnostic` 的禁针里
// ⇒ **写进 `backend/` 当场红**；而「加一条平台例外」这条路被**递减棘轮**堵着
// （`assert!(PLATFORM_EXCEPTIONS.len() <= 1)`，今天正好 1 条）。
// ⇒ 落点只能是这里，形状照 `platform_fs::make_executable` 那个**注入**先例。
// ══════════════════════════════════════════════════════════════════════════

/// 握手那一行（hello / attach 应答）的字节上限。
///
/// hello 帧本机实测 ~1.1 KB（能力集 + emits + commands 三张表）。8 KiB 给了 7 倍余量，
/// 同时把「对端一直发字节不发换行」这条路堵死 —— 这条连接的对端是**同机任何进程**，
/// 不是我们自己的子进程，不能假设它讲道理。
/// 超限语义：**拒收 + 出声**（不静默截断成一行「看起来对」的 JSON）。
/// **登记住址** `src-tauri/src/byte_cap_registry.rs`（那张表默认拒绝：不登记就红）。
pub(crate) const LISTEN_HANDSHAKE_LINE_CAP: usize = 8 * 1024;

/// daemon 那侧收「听哪个口」的 env 名。
///
/// ⚠ **跨 crate 字面量**：daemon 那边是 `remote-daemon-proto/src/listen.rs::ENV_PORT`。
/// 两边漂了**不会报错** —— 起出来的 daemon 会当成「没设」而走 stdio 那条路，
/// 于是宿主等在一个永远不会有人 bind 的口上，日志里只有一句「连不上」。
/// 由 `the_listen_env_names_are_the_same_string_on_both_sides` 逐字对拍
/// （形状抄 `the_local_origin_is_the_same_string_on_both_sides`）。
pub(crate) const LISTEN_PORT_ENV: &str = "CCM_LISTEN_PORT";

/// 同上，token 那一个（daemon 侧 `listen::ENV_TOKEN`）。
pub(crate) const LISTEN_TOKEN_ENV: &str = "CCM_LISTEN_TOKEN";

/// 关掉「脱离」的逃生口。**存在的理由不是好心，是判据**：
/// `KPY5` 要一格「起的时候**没走**脱离那条路 ⇒ `detached` 必须为假」的**负例**，
/// 而没有这个开关，那一支在 Linux 上永远走不到 —— 那正是「只有正例的测试永远绿」那一形。
pub const NO_DETACH_ENV: &str = "CCM_NO_DETACH";

/// 监听口取值区间：IANA 的动态/私有口段 `49152..=65535`。
///
/// 为什么不固定一个口：**同一台机上两个用户各有各的 daemon**，固定口必然撞；
/// 而撞了之后的处置（见 [`probe_listen_port`]）是**出声并拒绝**，不是换个口再起一个
/// —— 换口 = 每台机 N 个 daemon 互相盖 tmux hook 的 `[50]` 槽位，比今天更糟。
const PORT_BASE: u16 = 49152;
const PORT_SPAN: u32 = 16384;

/// `K-P1`：这台机 + 这个数据目录对应的监听口。**全仓唯一一份实现。**
///
/// # 为什么端口由宿主算、而不是两边各算一份
///
/// 「按家目录 hash 出一个口」若两边各写一份，两份就会漂 —— 本仓有现成的同族先例：
/// `shared/ccm` 与 Rust 侧各算一份 origin，实测**分叉四处**（`daemon_control.rs` 头注那张表）。
/// ⇒ 只留一份实现，daemon 那侧只收一个数（`listen::ENV_PORT`）。
///
/// # 算法：FNV-1a，**刻意不用 `DefaultHasher`**
///
/// `std::collections::hash_map::DefaultHasher` 的输出**跨 Rust 版本不保证稳定**
/// （它自己的文档逐字说了）。而这个数必须在**升级 monitor 之后仍然算出同一个口**，
/// 否则下一次启动会去连一个空口、起第二个 daemon —— 那正是本件要防的那件事。
/// ⇒ 用一个写死的、永远不会变的 FNV-1a。
pub fn listen_port_for(home: &str) -> u16 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in home.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    PORT_BASE + ((h % u64::from(PORT_SPAN)) as u16)
}

/// monitor 自己的目录 —— token 与「谁在听」都住这里。**与 daemon 的落点同一个目录**
/// （`~/.cc-monitor`），因为它们本来就是同一件事的两半。
fn cc_monitor_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".cc-monitor"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.cc-monitor"))
}

/// attach token 的住址。**只此一份**：下一个宿主要接上上一个宿主留下的那个 daemon，
/// 靠的就是读回同一个串。
fn token_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("listen-token")
}

/// 「谁在听那个口」的住址。**按口分文件**：同一台机上不同数据目录各有各的 daemon。
fn pid_path(dir: &std::path::Path, port: u16) -> std::path::PathBuf {
    dir.join(format!("listen-{port}.pid"))
}

/// 生成（或读回）attach token。
///
/// # ★★ 这一格是一条**真裁决**，不是实现细节
///
/// 回环 TCP 上**同机任何本地进程都连得上**（含**别的用户**），Unix socket 有文件权限位
/// 而它没有。而流那一档能发 `launch` / `kill` —— 以本账号的身份执行。
/// 收窄只能靠一个 token；而 **daemon 只读铁律不许它自己写文件**（`readonly_guard`）
/// ⇒ **token 只能由宿主生成、当 env 传进去**。
/// ⇒ 权限位这件事在这里补回来：**文件本身 `0600`**，那才是真正挡住别的用户的东西。
///
/// # 为什么是 `create_new` 而不是每次重写
///
/// 每次重写 = 上一个宿主留下的那个 daemon 立刻变成「连得上但认证不过」的孤儿。
/// ⇒ **只创建一次**，之后一律读回。
///
/// # ★★ 空文件那一格：**两支都要查，否则它们合成一个自己好不了的闭环**
///
/// 〔`K-P1-D1` `阻-4`，08-27 回修〕`create_new` 与 `write_all` 之间**不是原子的**
/// （进程被杀 / 盘满 / `write_all` 报错都会在盘上留下一个**零字节**的 token 文件）。
/// 回修前：第一支查了空、第二支**没查** ⇒ 第一支读到空 → 落到 `create_new` →
/// `AlreadyExists` → 第二支读回空 → `Ok("")`。**每次都一样，自己好不了。**
///
/// 空 token 之后两条下游路**都是死路**，而且都 fail closed（这一点原来就做对了）：
/// 起新的 ⇒ daemon 的 `listen::mode_from` 把空串读成「没设」⇒ 退 `EXIT_BAD_LISTEN_CONFIG`；
/// 接已有的 ⇒ `tokens_match` 的 `a.is_empty()` 直接判不等 ⇒ `WrongToken`。
/// **问题从来不是它没关上，是它关上之后指错了地方** —— 用户看到的是
/// 「脱离的 daemon 起来了却连不上它」，`looked_at` 里是**那个二进制**，一个字没提 token 文件。
/// ⇒ 空文件在这里就地变成 `Err`，`start_detached` 的那一支会把
/// [`token_path`] 放进 `looked_at`，而下面这句话说得出**下一步删哪个文件**。
///
/// ⚠ 这一格有一个**窄窗**，如实记：另一个宿主刚 `create_new` 完、还没 `write_all` 时，
/// 我们会读到空并**如实报错**（而不是静默等它）。等它要么加定时器、要么加自旋
/// —— 而「再起一次就好了」这条路的代价明显更小。**不装作那个窗不存在。**
///
/// ⚠ 诚实边界：token 文件被人删掉 / 改掉之后，仍在跑的那个 daemon 就再也接不上了。
/// 那时 [`probe_listen_port`] 会**出声**（不是静默复用，也不是静默再起一个）。
fn ensure_listen_token(dir: &std::path::Path) -> Result<String, String> {
    let p = token_path(dir);
    if let Ok(s) = std::fs::read_to_string(&p) {
        let t = s.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let token = fresh_token();
    // `create_new` = O_EXCL：两个 monitor 同时起时只有一个写得成，另一个回头读它写的那份。
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    // ★ 权限位就是这一格买的东西 —— 少了它，同机别的用户读得到 token，
    //   而 token 是这条回环口上**唯一**的门。
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    match opts.open(&p) {
        Ok(mut f) => {
            use std::io::Write;
            f.write_all(token.as_bytes())
                .map_err(|e| format!("写 {} 失败: {e}", p.display()))?;
            Ok(token)
        }
        // 竞态：别人刚写完 ⇒ 读它那份（**不是**覆盖它）。
        // ★★ 这一支**必须与第一支查同一个条件**（`阻-4`）：只查得到「读不出来」、
        //    查不到「读出来是空的」，两支就合成一个自己好不了的闭环。
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => std::fs::read_to_string(&p)
            .map_err(|e| format!("读 {} 失败: {e}", p.display()))
            .and_then(|s| {
                let t = s.trim().to_string();
                if t.is_empty() {
                    // ★ 诊断指到**这一格**：说得出下一步删哪个文件。
                    Err(format!(
                        "{} 在盘上，但内容是空的 —— 上一次写它的进程在建文件与写内容之间没了。\
                         空 token 起不出也接不上任何 daemon（两边都会 fail closed 地拒绝），\
                         而它自己不会好。**下一步：删掉这个文件再起一次** —— \
                         `rm {}`（删了之后仍在跑的旧 daemon 也接不上了，一并停掉它）",
                        p.display(),
                        p.display()
                    ))
                } else {
                    Ok(t)
                }
            }),
        Err(e) => Err(format!("建 {} 失败: {e}", p.display())),
    }
}

/// 造一个新 token。
///
/// ⚠ **没有引入 `rand`**：本仓的依赖面是有代价的（`C18` 依赖树零 C / 二进制量级）。
/// 取的熵是三样：进程 id · 纳秒时钟 · 一个每次调用都变的进程内计数器。
/// 这**不是密码学随机数**，如实登记 —— 它挡的是「同机另一个用户想连上这个口」，
/// 而那需要猜中一个 128 位十六进制串；它挡不住能读到这台机内存或 `/proc` 的人，
/// 而那种人本来就已经能以你的身份跑东西了。
fn fresh_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let a = nanos ^ (u64::from(std::process::id()) << 32);
    let b = nanos
        .rotate_left(17)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ SEQ.fetch_add(1, Ordering::SeqCst).wrapping_mul(0xff51_afd7_ed55_8ccd);
    format!("{a:016x}{b:016x}")
}

/// 记下「谁在听那个口」。**只有起它的那个宿主写**。
///
/// 它买的是**一件事**：下一个宿主接上这个 daemon 之后，「停」按钮还按得动
/// （见 [`stop_local_backend`]）。没有它，接管者手里只有一条 socket，
/// 而 socket 关掉不会让对面停 —— 那时「停」就成了一句骗人的话。
///
/// ⚠ 它**不是**真相源：「那个 daemon 还在不在」的真相源永远是**那个口连不连得上**。
/// 这里记的 pid 只在**杀它**那一步用，且用之前还要过一道 `/proc/<pid>/exe` 的身份核对。
/// ⚠ **两行，不是一行**：pid **和**它跑的那个二进制。
/// 只记 pid 的话，接管者杀它之前那道身份核对就没有对照物 ⇒ 只能 fail-open 地杀
/// —— 而 pid 会被复用，那是不可逆的。**两个值一起记，核对才有意义。**
fn write_listen_pid(
    dir: &std::path::Path,
    port: u16,
    pid: u32,
    bin: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let p = pid_path(dir, port);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    let body = format!("{pid}\n{}\n", bin.display());
    opts.open(&p)
        .and_then(|mut f| f.write_all(body.as_bytes()))
        .map_err(|e| format!("写 {} 失败: {e}", p.display()))
}

/// 解那两行。**纯函数**，所以「只有一行」「pid 不是数字」这些残缺形都测得到 ——
/// 这个文件是上一个 monitor 写的，它可能被中途打断、可能是旧版本写的。
pub(crate) fn parse_listen_owner(body: &str) -> Option<(u32, std::path::PathBuf)> {
    let mut lines = body.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    if pid == 0 {
        return None;
    }
    let bin = lines.next()?.trim();
    if bin.is_empty() {
        return None;
    }
    Some((pid, std::path::PathBuf::from(bin)))
}

fn read_listen_owner(dir: &std::path::Path, port: u16) -> Option<(u32, std::path::PathBuf)> {
    parse_listen_owner(&std::fs::read_to_string(pid_path(dir, port)).ok()?)
}

/// 一条 hello 行的裁决。**纯函数**，所以三张脸都测得到。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HelloVerdict {
    /// 是我们的 daemon：build_id 与数据目录都对得上。
    Ours,
    /// 有人占着这个口，但**不是**我们要找的那个。带上说得清的理由。
    Stranger(String),
}

/// 判那一行 hello。
///
/// ⚠ **`EADDRINUSE` / 连得上，只说明「有人占着这个口」，不说明占着它的是我们的 daemon。**
/// ⇒ 连上去**先读 hello 比对**，对不上就出声并拒绝，**不许静默复用**
/// （`P2t §1` 第 3 问「陈旧端点怎么识别」问的正是这一格）。
pub(crate) fn hello_verdict(line: &str, want_build: &str, want_home: &str) -> HelloVerdict {
    let Some(frame) = crate::ssh_source::parse_frame(line) else {
        return HelloVerdict::Stranger(format!(
            "这个口上的第一行不是一帧合法的 daemon 帧（{} 字节）",
            line.len()
        ));
    };
    let crate::ssh_source::InboundFrame::Hello {
        build_id,
        claude_dir,
        ..
    } = &frame
    else {
        return HelloVerdict::Stranger("这个口的首帧不是 hello".into());
    };
    if build_id != want_build {
        return HelloVerdict::Stranger(format!(
            "这个口上的 daemon 是 build_id={build_id}，而本 monitor 期望 {want_build}"
        ));
    }
    if claude_dir != want_home {
        return HelloVerdict::Stranger(format!(
            "这个口上的 daemon 看的是 {claude_dir}，而本 monitor 看的是 {want_home}"
        ));
    }
    HelloVerdict::Ours
}

/// 探那个口上有没有一个**我们的** daemon。
pub(crate) enum Probe {
    /// 没人在听。
    Nobody,
    /// 是我们的那个。把连接与它的 hello 行原样交出来（**别再连第二次** ——
    /// 第二次连的可能已经是另一个进程了）。
    Ours(std::net::TcpStream, String),
    /// 有人占着，但不是我们的。**出声**。
    Stranger(String),
}

/// 一次握手的读写期限。
///
/// 它**不是定时器**：`SO_RCVTIMEO`/`SO_SNDTIMEO` 说的是「**这一次**阻塞的读写最多等多久」，
/// 有字节就立刻返回、没字节就报错返回，不让任何线程自己醒来、不驱动任何循环。
/// 形状与理由与 `relay/server.rs::DOWNSTREAM_DEADLINE` 逐字同源。
/// 值给 3 秒：对端**就在本机**，一行 ~1 KB 的 hello 在回环上是微秒级的事；
/// 3 秒比它高五六个量级，而它同时保证「起 monitor 时不会被一个哑口卡住」。
const HANDSHAKE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(3_000);

fn probe_listen_port(port: u16, want_home: &str) -> Probe {
    let addr = std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        port,
    );
    let sock = match std::net::TcpStream::connect_timeout(&addr, HANDSHAKE_DEADLINE) {
        Ok(s) => s,
        // 连不上 = 没人在听。这是**最常见**的那一格（第一次起 monitor）。
        Err(_) => return Probe::Nobody,
    };
    if let Err(e) = sock
        .set_read_timeout(Some(HANDSHAKE_DEADLINE))
        .and_then(|()| sock.set_write_timeout(Some(HANDSHAKE_DEADLINE)))
    {
        return Probe::Stranger(format!("装不上握手期限（{e}）⇒ 宁可拒绝，也不挂在这儿"));
    }
    let line = match read_handshake_line(&sock) {
        Ok(l) => l,
        Err(e) => return Probe::Stranger(format!("{port} 口连得上，但读不到 hello：{e}")),
    };
    match hello_verdict(&line, env!("DAEMON_BUILD_ID"), want_home) {
        HelloVerdict::Ours => Probe::Ours(sock, line),
        HelloVerdict::Stranger(why) => Probe::Stranger(why),
    }
}

/// 从一条 socket 上读**一行**，字节数有上限。
///
/// 逐字节读：这条握手只有一两行、每行 ~1 KB，而**带缓冲的读会读过头** ——
/// 读过头的那些字节留在 `BufReader` 里，而这条 socket 之后要原样交给流那一档，
/// 那几个字节就凭空丢了（第一版就是这么写的）。
/// 形状抄 `relay/http1::read_head` 的 `r.read(&mut one)?`。
fn read_handshake_line(mut sock: &std::net::TcpStream) -> Result<String, String> {
    use std::io::Read;
    let mut out: Vec<u8> = Vec::new();
    let mut one = [0u8; 1];
    loop {
        match sock.read(&mut one) {
            Ok(0) => return Err("对端关了连接（EOF）".into()),
            Ok(_) => {
                if one[0] == b'\n' {
                    return String::from_utf8(out).map_err(|e| format!("这一行不是 UTF-8：{e}"));
                }
                out.push(one[0]);
                if out.len() > LISTEN_HANDSHAKE_LINE_CAP {
                    // **拒收 + 出声**，不静默截断成一行「看起来对」的 JSON。
                    return Err(format!(
                        "握手行超过 {LISTEN_HANDSHAKE_LINE_CAP} 字节还没换行 ⇒ 拒收"
                    ));
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("读握手行失败：{e}")),
        }
    }
}

/// daemon 那侧「这条流已经有人占着」的拒绝理由。
///
/// ⚠ **跨 crate 字面量**（daemon 侧 `listen::REFUSE_BUSY`）。它与另外两个 env 名一起
/// 由 `the_listen_env_names_are_the_same_string_on_both_sides` 逐字对拍。
/// 漂了的后果很具体：「上一个 monitor 刚退、对面还没反应过来」会被当成一个
/// **不可恢复**的拒绝，于是新 monitor 直接报失败 —— 而它本来只要再等 20 毫秒。
pub(crate) const REFUSE_BUSY_REASON: &str = "stream-busy";

/// attach 被拒的两张脸。**分开是因为处置不同**：
/// 「占着」会自己好（上一个宿主的那条流正在断），别的不会。
enum AttachErr {
    /// 那一条流此刻被占着 —— **可重试**。
    Busy(String),
    /// 别的（token 不对 / 协议对不上 / 写不出去）—— **不重试**，重试只是把坏消息拖晚。
    Fatal(String),
}

/// 把 token 递过去，换一句「可以」。
fn send_attach(sock: &std::net::TcpStream, token: &str) -> Result<(), AttachErr> {
    use std::io::Write;
    let mut w = sock;
    let req = format!("{{\"attach\":\"{token}\"}}\n");
    w.write_all(req.as_bytes())
        .and_then(|()| w.flush())
        .map_err(|e| AttachErr::Fatal(format!("发 attach 请求失败：{e}")))?;
    let line = read_handshake_line(sock).map_err(AttachErr::Fatal)?;
    let v: serde_json::Value = serde_json::from_str(line.trim())
        .map_err(|e| AttachErr::Fatal(format!("attach 应答不是 JSON（{e}）：{line}")))?;
    match v.get("attach").and_then(|x| x.as_str()) {
        Some("ok") => Ok(()),
        // 对面**出声地**拒了 —— 把它的理由原样带上来，别翻译成一句更含糊的话。
        Some("refused") => {
            let reason = v.get("reason").and_then(|x| x.as_str()).unwrap_or("?");
            let msg = format!("对面拒绝了 attach（reason={reason}）");
            if reason == REFUSE_BUSY_REASON {
                AttachErr::Busy(msg)
            } else {
                AttachErr::Fatal(msg)
            }
            .into_err()
        }
        _ => Err(AttachErr::Fatal(format!("看不懂的 attach 应答：{line}"))),
    }
}

impl AttachErr {
    fn into_err(self) -> Result<(), Self> {
        Err(self)
    }

    fn message(&self) -> &str {
        match self {
            AttachErr::Busy(m) | AttachErr::Fatal(m) => m,
        }
    }
}

/// 起一个**真脱离**的 daemon。
///
/// 三样一起才叫脱离，缺一样都不算：
/// - **`process_group(0)`** —— 否则终端里 Ctrl-C 的 SIGINT 会打到整个前台进程组，
///   monitor 和它一起走。
/// - **stdio 全 null** —— 今天它死掉的**真正原因**就在这儿：`Stdio::piped()` 之后
///   宿主一退读端就断，它在 **153 毫秒**内 broken-pipe 退出（`daemon_policy.rs` 头注实测）。
/// - **协议改走监听口** —— 管子没了就得有别的说话方式，那正是 `listen::ENV_PORT`。
///
/// ⚠ **`process_group` 不改变父子关系** ⇒ **不 `wait` 就留僵尸**
/// （`launch.rs:196-198` 头注逐字）。收尸走 [`reap_detached`]，由「流断了」这个**事件**触发，
/// 不是轮询。monitor 自己退出之后那个进程被 init 接管，由 init 收 —— 那一格不归我们。
#[cfg(target_os = "linux")]
fn spawn_detached(
    bin: &std::path::Path,
    port: u16,
    token: &str,
    extra_env: &[(String, String)],
) -> Result<std::process::Child, String> {
    use std::os::unix::process::CommandExt;
    let mut cmd = std::process::Command::new(bin);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.arg("--tail-only")
        // ★★ **`TMUX` 一律不继承**〔08-11 事故订正，与 `supervise_with_stdio` 同一条〕：
        //   tmux 客户端在 `TMUX` 有值时按它给的 socket 走，`TMUX_TMPDIR` 完全不起作用。
        //   漏这一条，被起的 daemon 会去改「monitor 恰好从哪个 tmux 里被启动」的那个 server。
        .env_remove("TMUX")
        .env(LISTEN_PORT_ENV, port.to_string())
        .env(LISTEN_TOKEN_ENV, token)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0);
    cmd.spawn()
        .map_err(|e| format!("起脱离的 daemon 失败（{}）：{e}", bin.display()))
}

#[cfg(not(target_os = "linux"))]
fn spawn_detached(
    bin: &std::path::Path,
    _port: u16,
    _token: &str,
    _extra_env: &[(String, String)],
) -> Result<std::process::Child, String> {
    Err(format!(
        "本平台没有脱离那条路（{}）—— 如实降级，不假装起了一个常驻的",
        bin.display()
    ))
}

/// 脱离之后手里剩下的东西。
pub struct DetachedHandle {
    /// 那个 daemon 的 pid。**0 表示不知道**（接管来的、而上一次那个 monitor 没记下来）。
    pid: u32,
    /// **我们起的**那个 `Child`；接管别人起的那个时是 `None`。
    ///
    /// 它同时是收尸的凭据 —— `process_group` 不改父子关系，不 `wait` 就留僵尸。
    /// ⚠ **「是不是我们起的」不另存一个 `bool`**：那样同一个事实就有了两份表示，
    /// 而两份表示会漂（本区最贵的那一族：「一个值装了两件事」的近亲）。
    /// 要问这句话就问 `child.is_some()`。
    child: Option<std::process::Child>,
    /// 那个二进制的路径。杀它之前拿它核对 `/proc/<pid>/exe`（防 pid 复用误伤）。
    /// 接管来的那个从 pid 文件的第二行读回；读不回就是空的，而空的**不许杀**。
    bin: std::path::PathBuf,
}

/// `K-P1`：**常驻那条路的句柄**。
///
/// ⚠ **锁序：它只在持有 [`LOCAL_BACKEND`] 的锁时才取。**
/// 两个静态量各有各的锁，若分别取就会在「查」与「存」之间开一个窗口 ——
/// 而那个窗口正是 `start_local_backend` 头注花了一整段治的那件事（双起）。
/// ⇒ `LOCAL_BACKEND` 的锁是**两条路共用的那道门**，本表只在门内动。
pub static DETACHED: std::sync::Mutex<Option<DetachedHandle>> = std::sync::Mutex::new(None);

/// `K-P1-D1` `重-2`：**上一次「那个口上有东西，但接不上它」的下一步该干什么。**
///
/// # 为什么要有这么一个格子
///
/// 「对不上就**出声**并拒绝」这句话（`§0b-2㈡` / `listen.rs` 诚实边界②）在
/// **手动点「起」**那条路上是兑现的（`daemon_control::daemon_start` 回 `Err` ⇒ 前端 toast），
/// 而在**自动起**那条路上（`lib.rs` 的 `setup`，用户每天真正走的那条）
/// 回修前只进了 `tracing::info!` —— **不是 `warn`，也没有任何东西到用户眼前。**
///
/// 它有一个具体的触发场景，不是理论：端口按家目录确定性算（[`listen_port_for`]），
/// 而 `hello_verdict` 拿 `env!("DAEMON_BUILD_ID")` 逐字比 —— **升级 monitor 之后，
/// 上一次脱离留下的那个 daemon 还在听同一个口** ⇒ `Stranger` ⇒ `Adopt::Refused`
/// ⇒ **本机后端起不来，而界面上什么都不说。**
/// ⚠ 这一格是**常驻带来的新场景**：翻面之前 daemon 153ms 就死了，根本不存在「上一个还在听」。
///
/// # 为什么是一条**记录**，而不是从 `reason` 串里反推
///
/// 反推要拿字符串去认「这是不是一次拒绝」——那正是 `KPY5` 花一整条 DoD 治的那件事
/// （假信号不会报错，它只是一直说是）。⇒ 这里与 [`DETACHED`] 同一个形状：
/// **只在真的走过那条路时才被写下**，写它的唯一入口是 [`note_start_refusal`]。
///
/// ⚠ `StartOutcome` 的形状**不能动**（加一个变体或一个字段，`daemon_control.rs`
/// 那个 `match` 当场编不过，而它在本轮写区之外）⇒ 走这条旁路。
static LAST_START_REFUSAL: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 记下「这一次失败**用户动得了手**，而且下一步是什么」。**写 [`LAST_START_REFUSAL`] 的唯一入口。**
///
/// # 为什么要有这个函数〔`D2` `重-D2-1`，08-27 补〕
///
/// 上一轮只在 `Adopt::Refused` 那一臂写了记录，而 `StartOutcome::Failed` 的构造点
/// **现打 6 处**（`the_user_actionable_start_failures_all_reach_the_user` 把这个分母钉住了）。
/// 其中 `ensure_listen_token` 的 `Err` 那一支 —— **恰恰是同一轮 `阻-4` 刚修好的那一支** ——
/// 走的是 `None =>` ⇒ `tracing::info!` ⇒ **用户什么都看不到**。
///
/// ★ 而这撞的是 `lib.rs` 自己写下的分档标准：「**拒绝**（口上有东西、接不上）=
/// 一件**用户能动手解决的事** ⇒ 说到眼前；别的失败（安装包里还没有 sidecar…）=
/// 诚实降级 ⇒ 仍走日志」。「盘上有个零字节的 token 文件，删掉它再起一次」按这条标准
/// **属于前者**，而上一轮把它落在了后者。⇒ **`阻-4` 只修了一半**：它让诊断指对了地方，
/// 而「指对了的那句话被谁听见」落在了 `重-2` 的人群外面。
///
/// # 用它的口径（新增失败构造点时照这一条分档）
///
/// - **用户动得了手**（删一个文件、停一个进程、改一个权限位）⇒ 调本函数，
///   并且那句话要**说得出下一步**（判据钉着「下一步」这三个字）；
/// - **诚实降级**（安装包里还没有 sidecar、这台机不走脱离那条路）⇒ **不要**调本函数：
///   每次启动都弹一次就成了噪音。那一档仍走 `tracing::info!`。
fn note_start_refusal(next_step: String) {
    *LAST_START_REFUSAL
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(next_step);
}

/// 取走上一次拒绝的「下一步能做什么」。**取走**（不是读）——
/// 同一次拒绝不许被两条路各说一遍，也不许留到下一次启动还在那儿。
pub fn take_start_refusal() -> Option<String> {
    LAST_START_REFUSAL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// `KPY5` 的真相源：**起它的时候走没走脱离那条路**。
///
/// ⚠ **不许拿 `channel` 或 `pid` 反推** —— 那正是 `P2d §0a` 翻掉的 `SSH_CONNECTION` 那一形
/// （假信号不会报错，它只是**一直说是**，而在只有正例的测试里永远绿）。
/// 这里读的是一条**只在真的走过那条路时才会被写下**的记录。
pub fn is_detached() -> bool {
    DETACHED
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// 这台机今天走不走脱离那条路。**纯函数**，两个入参都是外面喂进来的 ——
/// 否则「关掉了」那一支在 Linux 上永远走不到（`KPY5` 要的负例就没了）。
pub(crate) fn detach_wanted(is_linux: bool, no_detach_env: Option<&str>) -> bool {
    // 空串按「没设」算：shell 里 `export CCM_NO_DETACH=` 是常态。
    is_linux && !no_detach_env.is_some_and(|v| !v.trim().is_empty())
}

/// 收尸。**由「流断了」这个事件触发，不是轮询。**
///
/// `process_group` 不改变父子关系 ⇒ 我们起的那个进程死掉之后会变成 `Z`，
/// 直到有人 `wait` 它。断言必须落在**进程表**上而不是落在我们自己的事件上
/// —— `KPY3` 钉的就是这一格。
/// ⚠⚠ **先把 `Child` 从锁里 `take()` 出来，再在锁外等** —— 这不是讲究，是一次实测死锁。
///
/// 〔`K-P1` e2e 08-26 实测〕第一版是「持着 `DETACHED` 的锁调 `c.wait()`」，
/// 而「流断了」**不等于**「那个进程死了」：daemon 现在也会在**它自己**读到 EOF 时主动收掉
/// 这一条流（那是上一格修的东西）。于是 `wait()` 会等一个**还活着**的进程，
/// **一直持着那把锁** ⇒ 下一次 `daemon_status` / `daemon_start` / `daemon_stop` 全部卡死。
/// 实测形状：测试线程 `futex_do_wait`、一个 tokio worker `do_wait`，200 秒不动。
/// ⇒ 与 `supervise_with_stdio` 头注记的是**同一条**（那边逐字写过「guard 活在闭包里
/// ⇒ `wait()` 整段都持着锁，而 `stop()` 第一件事就是取那把锁」）—— 本仓第二次。
///
/// 收尸落在一条**专用线程**上（形状抄 `launch.rs::launch_local_posix_via` 里那条），
/// 它随子进程结束而结束；`Child` 被取走之后句柄里只剩 pid + 二进制路径，
/// 「停」那一步照样有凭据（走 [`kill_adopted`] 的身份核对）。
fn reap_detached() {
    let taken = {
        let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
        g.as_mut().and_then(|h| h.child.take())
    };
    let Some(mut c) = taken else { return };
    std::thread::Builder::new()
        .name("ccm-detached-reaper".into())
        .spawn(move || {
            // `process_group` 不改变父子关系 ⇒ 不 `wait` 就留僵尸（`launch.rs:198` 逐字）。
            let _ = c.wait();
        })
        .map(|_| ())
        .unwrap_or_else(|e| tracing::warn!("起不来收尸线程（{e}）⇒ 那个 pid 会留成僵尸"));
}

/// 把一条已经认证过的连接接成入方向通道。
///
/// # 它与 `local_backend::local_stdio_consumer` 是同一件事的两种载体
///
/// 那一份吃 `ChildStdin`/`ChildStdout`，这一份吃一条 socket 的两半。
/// **复用的是纯零件**（`parse_frame` · `DaemonHello::from_hello_frame` ·
/// `park_owned_writer` · `read_capped_line`），没有把一个绑传输的循环硬掰成泛型 ——
/// 那是 `local_stdio_consumer` 头注自己给的分寸。
fn attach_stream(sock: std::net::TcpStream, hello_line: &str) -> Result<(), String> {
    let frame = crate::ssh_source::parse_frame(hello_line)
        .ok_or_else(|| "hello 行解析不出帧".to_string())?;
    let witness = crate::inbound_client::DaemonHello::from_hello_frame(&frame)
        .ok_or_else(|| "首帧不是 hello ⇒ 拿不到见证，按契约不许登记通道".to_string())?;
    sock.set_nonblocking(true)
        .map_err(|e| format!("socket 转非阻塞失败：{e}"))?;
    // 期限只属于**握手**那一段；进了流之后这条连接是长连接，装着期限反而会把它掐断。
    let _ = sock.set_read_timeout(None);
    let _ = sock.set_write_timeout(None);
    let (rd, wr) = tauri::async_runtime::block_on(async move {
        tokio::net::TcpStream::from_std(sock).map(tokio::net::TcpStream::into_split)
    })
    .map_err(|e| format!("socket 转 tokio 失败：{e}"))?;
    let client = crate::inbound_client::park_owned_writer(wr).into_client(witness);
    crate::inbound_client::register(crate::inbound_client::LOCAL_ORIGIN, client.clone());
    tracing::info!(
        "本机入方向通道已登记（常驻载体）：origin={}",
        crate::inbound_client::LOCAL_ORIGIN
    );
    tauri::async_runtime::spawn(async move {
        use crate::ssh_source::{CappedLine, DAEMON_FRAME_LINE_CAP};
        let mut reader = tokio::io::BufReader::new(rd);
        let mut buf: Vec<u8> = Vec::new();
        loop {
            match crate::ssh_source::read_capped_line(&mut reader, &mut buf, DAEMON_FRAME_LINE_CAP)
                .await
            {
                Ok(CappedLine::Eof) => break,
                Ok(CappedLine::TooLong(bytes)) => {
                    // 丢弃 + 带身份报告，绝不静默（定框 E4）。
                    tracing::warn!("本机常驻后端发来一行 {bytes} 字节，超过单行上限；整行丢弃");
                    continue;
                }
                Ok(CappedLine::Line) => {}
                Err(e) => {
                    tracing::warn!("本机常驻后端读错误（{e}）；按流结束处理");
                    break;
                }
            }
            let Ok(line) = std::str::from_utf8(&buf) else {
                continue;
            };
            let Some(f) = crate::ssh_source::parse_frame(line) else {
                continue;
            };
            // 本机的 tmux 帧也要收（`P3` 刀 1）—— 理由与前置条件写在
            // `local_backend::absorb_local_frame` 的头注上，这里不再抄一份散文。
            if let crate::ssh_source::InboundFrame::TmuxSessions { raw, .. } = &f {
                crate::ssh_source::record_tmux_raw(crate::inbound_client::LOCAL_ORIGIN, raw.clone());
            }
        }
        // 流结束 ⇒ 摘掉登记，别在表里留一个写不进去的 client；那份陈旧的 tmux 原文也要清
        // （留着它 `find_tmux_origin_for_sid` 仍会回 `Some(<local>)` ⇒ 那个永远消不掉的灰点）。
        crate::inbound_client::unregister(crate::inbound_client::LOCAL_ORIGIN, &client);
        crate::ssh_source::forget_tmux_raw(crate::inbound_client::LOCAL_ORIGIN);
        // 收尸：`process_group` 不改父子关系，不 `wait` 就留 `Z`。**事件驱动，不是轮询。**
        reap_detached();
        tracing::info!("本机常驻后端的流结束（EOF）");
    });
    Ok(())
}

/// 走常驻那条路的结局。
enum DetachOutcome {
    /// 这条路今天不走（平台不支持 / 被 `CCM_NO_DETACH` 关掉）⇒ 回落今天那条。
    NotTaken,
    /// 走了，结局在里面（含失败 —— 失败也不回落，见下）。
    Done(StartOutcome),
}

/// 常驻那条路的正题：**认得出已有实例 ⇒ 接上它；没有 ⇒ 起一个脱离的。**
///
/// 两个入参都是**注入**，理由各不相同：
/// - `resolve_bin`：生产路径喂 [`resolve_daemon_bin`]（exe 旁 → 内嵌释放）；
///   而判据喂一个现成的二进制 —— 否则那条真进程判据只能去动用户真实的 `~/.cc-monitor`。
/// - `extra_env`：生产路径喂**空表**；e2e 用它塞一条**前面挂着 shim 的 PATH**。
///   ⚠ 这一条不是方便，是 `C7i` 红线：被起的 daemon 一上来就往它连得到的 tmux server
///   装三条全局 hook、固定槽位 `[50]`、**没有关掉它的开关**。不隔离就是去改用户真实
///   tmux server 的状态（08-11 那次事故打没了用户 9 个真实会话）。
fn start_detached(
    resolve_bin: &dyn Fn() -> Result<std::path::PathBuf, (String, Vec<std::path::PathBuf>)>,
    extra_env: &[(String, String)],
) -> DetachOutcome {
    if !detach_wanted(
        cfg!(target_os = "linux"),
        std::env::var(NO_DETACH_ENV).ok().as_deref(),
    ) {
        return DetachOutcome::NotTaken;
    }
    let Some(home) = crate::paths::resolve_claude_dir() else {
        return DetachOutcome::NotTaken;
    };
    let home = home.to_string_lossy().into_owned();
    let port = listen_port_for(&home);
    let dir = cc_monitor_dir();
    let token = match ensure_listen_token(&dir) {
        Ok(t) => t,
        Err(e) => {
            // ★★ `重-D2-1`：**这一格也是「用户动得了手」那一档 ⇒ 也要说到眼前。**
            //    `阻-4` 让这句诊断指对了地方（说得出删哪个文件），但它只走到
            //    `StartOutcome::Failed` 的 `reason` 里 —— 而自动起那条路对没有记录的失败
            //    走 `tracing::info!` ⇒ 用户什么都看不到，那句「`rm <路径>`」只说给日志听。
            //    ⚠ `e` 本身就是那句话（`ensure_listen_token` 的空文件支逐字写着「下一步」）,
            //    这里**原样**转交，不另写一份 —— 两份措辞迟早对不上。
            note_start_refusal(e.clone());
            return DetachOutcome::Done(StartOutcome::Failed {
                reason: format!("拿不到 attach token（{e}）⇒ 拒绝起一个不设防的口"),
                looked_at: vec![token_path(&dir)],
            })
        }
    };

    // ── ① 起时先认已有实例 ────────────────────────────────────────────
    //
    // ★★ 这一条是硬的：daemon 一起来就**无条件**往它连得到的 tmux server 装三条全局 hook、
    //    **固定槽位 `[50]`**、**没有关掉它的开关**，载荷里烤着那一个 daemon 的 pid+starttime。
    //    ⇒ **脱离而不认已有实例 = 每台机 N 个 daemon 互相盖槽位，比今天更糟。**
    match adopt_existing(port, &home, &token) {
        Adopt::Attached => {
            let (pid, bin) = read_listen_owner(&dir, port).unwrap_or((0, std::path::PathBuf::new()));
            let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
            *g = Some(DetachedHandle {
                pid,
                child: None,
                bin,
            });
            return DetachOutcome::Done(StartOutcome::AlreadyRunning);
        }
        // ⚠ **不静默复用，也不静默再起一个** —— 两条都会让状态更糟。
        Adopt::Refused(why) => {
            // ★★ `重-2`：**这一臂是「出声并拒绝」里「出声」那一半的真相源。**
            //    自动起那条路（`lib.rs`）拿它决定要不要把话说到用户眼前 ——
            //    而不是去 `reason` 串里认字（那是 `KPY5` 治的那种假信号）。
            note_start_refusal(format!(
                "本机后端没起来：{port} 口上那个 daemon 接不上（{why}）。\
                 **下一步**：把那个进程停掉再重开 monitor —— 它的 pid 记在 {}。\
                 升级 monitor 之后最常见：口是按家目录算死的，而上一次留下的那个 daemon \
                 版本对不上，于是新的这个既不会换口、也不会再起第二个。",
                pid_path(&dir, port).display()
            ));
            return DetachOutcome::Done(StartOutcome::Failed {
                reason: format!(
                    "{port} 口上有东西，但接不上它：{why}。\
                     **不会**再起第二个、也**不会**换个口 —— 那正是「每台机 N 个 daemon\
                     互相盖 tmux hook 的 [50] 槽位」的入口。要重来请先把那个进程停掉"
                ),
                looked_at: vec![pid_path(&dir, port), token_path(&dir)],
            });
        }
        Adopt::None => {}
    }

    // ── ② 没人在听 ⇒ 起一个脱离的 ─────────────────────────────────────
    let bin = match resolve_bin() {
        Ok(b) => b,
        Err((reason, looked_at)) => {
            return DetachOutcome::Done(StartOutcome::Failed { reason, looked_at })
        }
    };
    let child = match spawn_detached(&bin, port, &token, extra_env) {
        Ok(c) => c,
        Err(e) => {
            return DetachOutcome::Done(StartOutcome::Failed {
                reason: e,
                looked_at: vec![bin],
            })
        }
    };
    let pid = child.id();
    if let Err(e) = write_listen_pid(&dir, port, pid, &bin) {
        // 记不下不算失败：那只影响「停」按钮，不影响它跑起来。**说出来**而不是静默。
        tracing::warn!("记不下「谁在听 {port}」（{e}）⇒ 下一个 monitor 接上它之后停不了它");
    }
    {
        let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
        *g = Some(DetachedHandle {
            pid,
            child: Some(child),
            bin: bin.clone(),
        });
    }
    // 起来了之后自己连上去 —— **走与「接管」完全同一条路**，不另写一份。
    match probe_and_attach_after_spawn(port, &home, &token) {
        Ok(()) => DetachOutcome::Done(StartOutcome::Started(bin)),
        Err(e) => {
            // 起来了但连不上 ⇒ 这不是「起了」。把它收掉，别留一个谁都够不着的进程。
            stop_detached_locked();
            DetachOutcome::Done(StartOutcome::Failed {
                reason: format!("脱离的 daemon 起来了却连不上它（{e}）⇒ 已把它收掉"),
                looked_at: vec![bin],
            })
        }
    }
}

/// 等刚起的那个 daemon 把口 bind 上 —— **次数与间隔都有上限**。
///
/// # 为什么非等不可（以及为什么 `connect_timeout` 不管用）
///
/// `spawn()` 返回时子进程可能还没跑到 `bind`。而 `connect_timeout` 在**没人在听**的口上
/// 拿到的是 `ECONNREFUSED`，**内核立刻返回**，它压根不等 —— 我第一版把这一格写反了，
/// 注释里写着「让内核等」，实测 0.00 秒就红了。**读数是这样来的，不是推的。**
///
/// # 它是 `wait-for-condition`，不是节拍器
///
/// 等的是**一次性条件**（那个口起没起来），有明确上限（`LISTEN_WAIT_TRIES` ×
/// `LISTEN_WAIT_INTERVAL_MS` ≈ 1 秒），等到就走、等不到就如实报错，**不无限重试**。
/// **登记住址** `src-tauri/src/rust_timer_registry.rs`（那张表按类别收，`wait-for-condition`
/// 这一类要求「说清等什么、上限是多少」）。
///
/// ⚠ 上限为什么是这个量级：对端**就在本机**，从 `execve` 到 `bind` 是毫秒级；
/// 1 秒高两个量级。给得再大只会让「那个进程起来就崩了」这一格拖着不报错。
const LISTEN_WAIT_TRIES: u32 = 50;
const LISTEN_WAIT_INTERVAL_MS: u64 = 20;

/// 接管已有实例的结果。
enum Adopt {
    /// 那个口上没人 —— 该起一个了。
    None,
    /// 接上了。
    Attached,
    /// 有东西，但接不上。**出声**，绝不静默复用、也绝不换个口再起一个。
    Refused(String),
}

/// 认已有实例并接上它。
///
/// # 只有两支会重试，而且理由不一样
///
/// - **口上还没人**（`Probe::Nobody`）：只在「刚亲手 spawn 完」那条路上会遇到
///   （`accept_new` = true）。接管路上遇到它就是「真没人」，立刻回 [`Adopt::None`]。
/// - **那一条流被占着**（`stream-busy`）：**上一个 monitor 刚退、对面还没反应过来**。
///   这一格是真的：daemon 那侧要等 `writer_task` 拿到写错误、`done` 信号回到 accept 循环，
///   才会把那张牌放回去。⇒ 有界重试。**不重试它，用户会看到「换台电脑重开 monitor 就没后端了」。**
///
/// 别的一律不重试 —— token 不对、口上是别人，重试只是把一个确定的坏消息拖晚。
fn adopt_existing(port: u16, home: &str, token: &str) -> Adopt {
    adopt_with(port, home, token, false)
}

/// 刚起完之后连上去。**与接管走同一条路**，差别只有一句：
/// 这一次「没人在听」是**还没 bind 完**（我们刚亲手起了一个），要等。
fn probe_and_attach_after_spawn(port: u16, home: &str, token: &str) -> Result<(), String> {
    match adopt_with(port, home, token, true) {
        Adopt::Attached => Ok(()),
        Adopt::Refused(why) => Err(why),
        Adopt::None => Err(format!("{port} 口上始终没人在听 —— 多半是它起来就崩了")),
    }
}

fn adopt_with(port: u16, home: &str, token: &str, wait_for_bind: bool) -> Adopt {
    let mut last = String::from("那个口上没人");
    for _ in 0..LISTEN_WAIT_TRIES {
        match probe_listen_port(port, home) {
            Probe::Stranger(why) => return Adopt::Refused(why),
            Probe::Nobody => {
                if !wait_for_bind {
                    return Adopt::None;
                }
                last = format!("{port} 口上还没人在听");
            }
            Probe::Ours(sock, hello) => match send_attach(&sock, token) {
                Ok(()) => {
                    return match attach_stream(sock, &hello) {
                        Ok(()) => Adopt::Attached,
                        Err(e) => Adopt::Refused(e),
                    }
                }
                Err(AttachErr::Busy(m)) => last = m,
                Err(e) => return Adopt::Refused(e.message().to_string()),
            },
        }
        std::thread::sleep(std::time::Duration::from_millis(LISTEN_WAIT_INTERVAL_MS));
    }
    Adopt::Refused(format!(
        "{last}（等了 {LISTEN_WAIT_TRIES}×{LISTEN_WAIT_INTERVAL_MS}ms 还是这样）"
    ))
}

/// 找那个二进制：**先找 exe 旁边，再释放内嵌的那份**。
///
/// ⚠⚠ **这个顺序与 `local_backend::start_or_extract` 的必须一样**，
/// 而它们是两份实现（那一份把「找」与「监护」焊在一起，常驻这条路不要监护那半）。
/// 两份实现就会漂 ⇒ 由 `the_two_resolution_paths_still_agree_on_the_order` 逐字对拍。
fn resolve_daemon_bin(
    extract_dir: &std::path::Path,
    embedded: Option<(&str, &[u8])>,
) -> Result<std::path::PathBuf, (String, Vec<std::path::PathBuf>)> {
    let beside = local_backend::resolve_beside_this_exe(env!("CCM_TARGET_TRIPLE"));
    match beside {
        Resolved::Found(p) => Ok(p),
        Resolved::Missing { reason, looked_at } => {
            let Some((build_id, bytes)) = embedded else {
                return Err((reason, looked_at));
            };
            local_backend::extract_embedded_to(
                extract_dir,
                build_id,
                bytes,
                // `backend-split` 的 C10：平台知识由宿主注入。
                &crate::platform_fs::make_executable,
            )
            .map_err(|e| (format!("exe 旁无 sidecar，且释放内嵌 daemon 失败: {e}"), looked_at))
        }
    }
}

/// 停掉常驻那个。**调用方必须已经持有 [`LOCAL_BACKEND`] 的锁**（锁序，见 [`DETACHED`]）。
fn stop_detached_locked() -> Option<String> {
    let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
    let h = g.take()?;
    let pid = h.pid;
    if let Some(mut c) = h.child {
        let _ = c.kill();
        // 收尸：**不 `wait` 就留 `Z`**（`launch.rs:198` 逐字）。
        let _ = c.wait();
        return Some(format!("本机后端已停（常驻，pid={pid}）"));
    }
    // 接管来的那个：手里没有 `Child`。**按 pid 杀之前先核身份** ——
    // pid 会被复用，杀错一个无关进程是不可逆的。
    match kill_adopted(pid, &h.bin) {
        Ok(()) => Some(format!("本机后端已停（接管的常驻实例，pid={pid}）")),
        Err(e) => Some(format!(
            "已断开与本机常驻后端的连接，但**没能停掉它**（pid={pid}：{e}）。\n\
             它是上一次 monitor 起的、脱离在跑的那个 —— 本 monitor 手里只有一条流。\
             要真停掉它，请手动结束那个进程。"
        )),
    }
}

/// 杀一个**不是我们起的**常驻实例。
///
/// ⚠ **先核身份再杀。**pid 会被复用，而「杀错一个无关进程」是不可逆的。
/// 核的是 `/proc/<pid>/exe` 是不是同一个二进制 —— 与 `--tmux-notify` 那条
/// pid+starttime 双钉同一条道理：**只有身份对得上才动手**。
#[cfg(target_os = "linux")]
fn kill_adopted(pid: u32, bin: &std::path::Path) -> Result<(), String> {
    if pid == 0 {
        return Err("不知道它的 pid（那次起它的 monitor 没能记下来）".into());
    }
    // ★ **fail closed**：没有对照物就不杀。空路径下「核对通过」等于没核对，
    //   而这一步的代价是不可逆的（杀掉一个恰好复用了那个 pid 的无关进程）。
    if bin.as_os_str().is_empty() {
        return Err(format!(
            "不知道 pid={pid} 该是哪个二进制（那份记录缺了第二行）⇒ **不动它**"
        ));
    }
    let exe = std::fs::read_link(format!("/proc/{pid}/exe"))
        .map_err(|e| format!("读不到 /proc/{pid}/exe（{e}）—— 它可能已经不在了"))?;
    let same = exe == bin || std::fs::canonicalize(bin).map(|c| c == exe).unwrap_or(false);
    if !same {
        return Err(format!(
            "pid={pid} 现在跑的是 {}，不是我们的 daemon ⇒ **不动它**（pid 被复用了）",
            exe.display()
        ));
    }
    // 只发 SIGTERM：daemon 自己有停机路径（`shutdown_signal`），SIGKILL 会跳过它。
    signal_term(pid)
}

#[cfg(not(target_os = "linux"))]
fn kill_adopted(_pid: u32, _bin: &std::path::Path) -> Result<(), String> {
    Err("本平台没有脱离那条路，也就没有「接管来的常驻实例」".into())
}

/// 发一次 SIGTERM。
///
/// ⚠ **为什么起一个进程而不是调 `libc::kill`**：monitor 今天**没有 `libc` 这条直接依赖**
/// （它只在依赖树里，靠传递依赖进来），为一次「停」按钮加一条直接依赖是更大的代价。
/// 这一处**已登记**在 `write_site_registry::spawn_sites::SPAWNS`（那张表默认拒绝）。
/// 参数是**我们自己算出来的 pid**，不吃任何用户输入。
#[cfg(target_os = "linux")]
fn signal_term(pid: u32) -> Result<(), String> {
    let st = std::process::Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("起不来 kill：{e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("kill -TERM {pid} 退出码 {st}"))
    }
}

pub fn start_local_backend() -> StartOutcome {
    // ★★ **锁全程持有**〔D 阶段补审 08-11 修，原版是阻塞级缺陷〕。
    //
    // # 原来错在哪
    //
    // 原判据是 `h.current_pid().is_some()`，而 `pid` 由 **supervise 线程**在 `cmd.spawn()`
    // 成功之后才 `store` —— 本函数返回时那个线程往往还没跑到那一行，`pid` 仍是 0。
    // ⇒ **串行点两下「起」就够**：第二次进来 `current_pid()` 是 `None`，门放行，
    // 起出 h2 并**覆盖** h1。h1 的句柄被 drop，但 `stopping`/`child` 都是 `Arc`
    // ⇒ 它的 supervise 线程照常活、子进程照常跑、崩了照常重起，**再没有任何代码能 `stop()` 它**。
    // 之后按「停」只 take 到 h2 ⇒ `daemon_status` 回 `channel: true` / `pid: null`，
    // **「状态说停了」与「进程真没了」当场分叉** —— 正是 `P2s-Y2` 自陈要守的那件事，
    // 而 Y2 的实测走的是被绕开的另一条路径（它自己 spawn，不经本函数）。
    //
    // # 现在的不变量
    //
    // **句柄在表里 = 在跑**（`stop_local_backend` 会把它 `take` 走）⇒ 判据不再问 pid。
    // 并且**锁横跨整个起的过程**，把「检查」与「存句柄」之间那个窗口关掉。
    //
    // ⚠ 代价如实登记：`extract_embedded_to` 会在持锁期间同步写 ~10MB，
    // 期间 `daemon_status` / `daemon_stop` 会短暂阻塞。这是**用一次可见的等待换掉一个
    // 起不掉也杀不掉的幽灵进程**；把释放挪出命令线程是另一件事（补审建议 C4）。
    let mut g = LOCAL_BACKEND.lock().expect("LOCAL_BACKEND 锁毒化");
    if g.is_some() {
        return StartOutcome::AlreadyRunning;
    }
    // ★ `K-P1`：常驻那条路也算「在跑」。
    //
    // ⚠ **锁序**：`DETACHED` 只在持有上面那把锁时才取（见它的头注）⇒ 这里**没有**第二个窗口。
    // 上面那一段头注花了一整段讲的就是「检查」与「存句柄」之间那个窗口，
    // 常驻这条路不许把它重新开出来。
    if is_detached() {
        return StartOutcome::AlreadyRunning;
    }
    // 两样宿主知识在这里给（backend 层不认识它们）：
    //   · 落点 `~/.cc-monitor/bin`：与远端自部署同一个目录，但**文件名带 build_id**
    //     ⇒ 与远端那份结构上不可能撞（理由见 `extract_embedded_to` 头注的 D1 段）。
    //   · 当前 arch：`crate::sftp::daemon_binary` 按它挑内嵌字节；缺内嵌（`cfg(embedded_daemons)`
    //     未置）时给 None，函数会诚实降级、不伪造理由。
    let extract_dir = dirs::home_dir()
        .map(|h| h.join(".cc-monitor").join("bin"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.cc-monitor/bin"));
    // ★★ **内嵌的那两份是 musl LINUX 二进制，本机不是 Linux 就一份都不能用**
    // 〔D 阶段补审 08-11 修，原版是阻塞级缺陷〕。
    //
    // # 原来错在哪
    //
    // `sftp::daemon_binary(ARCH)` **只按 arch 分派，不看 OS**；`cfg(embedded_daemons)` 也只由
    // `build.rs` 凭 `embedded-daemons/cc-monitor-remote-{x86_64,aarch64}`（musl Linux）在不在决定，
    // **同样不看目标 OS**。于是在 Windows 构建上：
    // ① 释放一个 Linux ELF 到 `%USERPROFILE%\.cc-monitor\bin\`（**没有 `.exe` 后缀**，
    //    `platform_fs::make_executable` 在非 unix 是 no-op）；
    // ② `start_or_extract` **返回 `Resolved::Found`** ⇒ 日志打「本机后端 sidecar: …」、
    //    前端回「已起」；③ 真正的失败发生在 supervise 线程里（`spawn` 报错 → `GaveUp` 只进日志）。
    //
    // ⇒ **每次启动往用户目录写一份 10MB 级的无用二进制，UI 与日志报告启动成功，进程从来没起来过。**
    // `Resolved::Found` 在那条路上是一个谎报 —— 它只证明「文件落地了」，不证明「那是本平台能跑的东西」。
    //
    // # 为什么过滤放在这里，不放进 `sftp::daemon_binary`
    //
    // 那个函数**同时供远端部署用**，而远端的目标就是 Linux —— 在那条路上用 musl 二进制是对的。
    // 「本机是什么 OS」是宿主知识，本模块正是它的家。
    let embedded = if cfg!(target_os = "linux") {
        crate::sftp::daemon_binary(std::env::consts::ARCH).map(|d| (d.build_id, d.bytes))
    } else {
        None
    };
    // ★★ `K-P1`：**先走常驻那条路** —— 认得出已有实例就接上它，没有就起一个脱离的。
    //
    // 这就是「怎么起」那个注入点：`start_detached` 是**这一层**（宿主知识层）的东西，
    // 它认识 `process_group(0)` / `/proc` / `~/.cc-monitor`，而 `backend/` 那半一样都不许认识
    // （`the_backend_half_stays_platform_agnostic` 的禁针含 `std::os::unix`）。
    // 走不了（平台不支持 / `CCM_NO_DETACH` 关掉了）才回落到下面那条今天的路。
    // ★★ `K-H2b` `KH2B2`①：**中转的启动方就是这一行**（在本件之前 `--relay` 生产调用点 = 0）。
    //
    // ⚠ 放在这里而不是放进下面那两条分支里，是因为**两条路都要有中转**：
    // 常驻那条（`start_detached`）会 `return`，接在它后面等于「走常驻就没有中转」。
    // ⚠ 中转与 daemon 是**两个进程**（`relay/mod.rs` 自陈「独立进程」），
    //   所以这里不是「多给 daemon 一个参数」，是**再监护一个**。
    // 拿不到二进制时**什么都不做**：那条路上 daemon 自己也起不来，
    // 下面的 `Resolved::Missing` 会把理由报出去 —— 不在这里再报一遍同一件事。
    if let Ok(bin) = resolve_daemon_bin(&extract_dir, embedded) {
        start_local_relay(bin);
    }
    match start_detached(&|| resolve_daemon_bin(&extract_dir, embedded), &[]) {
        DetachOutcome::Done(out) => return out,
        DetachOutcome::NotTaken => {}
    }
    let (resolved, sup) = local_backend::start_or_extract(
        env!("CCM_TARGET_TRIPLE"),
        &extract_dir,
        embedded,
        // `backend-split` 的 C10：平台知识由宿主注入，backend 那半不认识 `#[cfg(unix)]`。
        &crate::platform_fs::make_executable,
        std::sync::Arc::new(|e| tracing::info!("本机后端: {e:?}")),
    );
    if let Some(h) = sup {
        *g = Some(h);
    }
    match resolved {
        Resolved::Found(p) => StartOutcome::Started(p),
        Resolved::Missing { reason, looked_at } => StartOutcome::Failed { reason, looked_at },
    }
}

/// 本机独有的两个读数（远端没有对应物：那个进程在别人机器上）。
pub fn local_pid_and_attempts() -> Result<(Option<u32>, Option<u32>), String> {
    let g = LOCAL_BACKEND.lock().map_err(|e| format!("锁毒化: {e}"))?;
    if let Some(h) = g.as_ref() {
        return Ok((h.current_pid(), Some(h.attempts())));
    }
    // ★ `K-P1`：常驻那条路。`attempts` 这里**恒 `None`** 而不是 0 ——
    // 那一格的含义是「监护器起过它几次」，而常驻这条路**没有监护器**（`K14` 裁的第一档）。
    // 报 0 会让 UI 显示一个看起来正常的数，而它背后没有任何东西在数。**空值 ≠ 0。**
    let d = DETACHED.lock().map_err(|e| format!("锁毒化: {e}"))?;
    Ok(match d.as_ref() {
        Some(h) if h.pid != 0 => (Some(h.pid), None),
        Some(_) => (None, None),
        None => (None, None),
    })
}

/// P2s（`C8`②）：停本机后端。**句柄取走**（`take`）而不是留着 ——
/// `stop()` 之后那个句柄就是死的（`stopping` 永久置位），留着只会让下一次「起」
/// 误以为还在跑。
// ═════════════════════════════════════════════════════════════════════════════
// `K-H2b` `KH2B2`：**本机中转的启动方** —— 在本件之前，`--relay` 生产调用点是 **0**
// ═════════════════════════════════════════════════════════════════════════════
//
// # 为什么走「再监护一个进程」而不是「折进常驻 daemon」（两条路里选的这一条，理由写下来）
//
// `--relay` 今天住 `remote-daemon-proto/src/main.rs` 的**一次性子命令分派臂**，
// `relay/mod.rs` 头注自陈**是独立进程**，`serve()` **永不返回**。
// ⇒ 折进常驻 daemon 要动的是 daemon 的进程模型（那条 wire 流与中转的 stdout 会撞，
//   `relay/mod.rs` 头注逐字记着「谁在同一个进程里既跑流式又跑中转，今天没有任何判据挡着」）。
// 而「再监护一个」用的是**现成的** `local_backend::supervise`（收 `args` + `envs`），
// 一行新机制都不用发明。⇒ 本件选 ㈠。
//
// # ⚠ 本件**不做端口通告面**（`§0e` 裁五，跟进件 `己1-f26`）
//
// 端口是 `payload::RELAY_PORT` 这一个常量，**显式**以 `CCM_RELAY_PORT` 交给子进程
// ⇒ 注入侧与中转侧用的是同一个值，daemon 那份 `DEFAULT_PORT` 在这条路上不参与。
// **同机第二个 monitor** 的形状：第二个中转绑不上那个口 ⇒ `run_with` 印
// `cannot bind loopback port …` 并**退 2** ⇒ 监护器按崩溃计数，三次之后 `GaveUp` 出声。
// **不静默**，但也**不会自动换口** —— 换口要先答「谁来分配 / 冲突了怎么办 / 远端怎么知道」，
// 那是另一件的体量。
//
// # ⚠ 判不了的（别读成「没问题」）
//
// - **中转能不能承受所有会话都走它**：`server.rs::INFLIGHT_CONNECTIONS` 有上界、超了回 503，
//   那个数够不够**我没量**。（本件裁的是「只接 api-key 号」⇒ 今天的量级远小于「所有会话」。）
// - **Windows 上这条路的运行时行为**：内嵌的那两份 sidecar 是 musl Linux 二进制，
//   `start_local_backend` 里那条 `cfg!(target_os = "linux")` 闸对本函数**同样适用**
//   —— 非 Linux 宿主上 `resolve_daemon_bin` 拿不到东西，本函数就不会被调到。

/// `K-H2b`：本机中转的监护句柄。形状与 [`LOCAL_BACKEND`] 同族（`Mutex<Option<_>>`，
/// 停了要能再起）。
pub static LOCAL_RELAY: std::sync::Mutex<Option<SuperviseHandle>> =
    std::sync::Mutex::new(None);

/// `KH2B2`①：**起本机中转**。已经在跑就不重复起（同 `C8`①「每台机各一个」）。
///
/// ⚠ 返回 `bool` = 「本次调用起了一个新的」，**不是**「现在有没有在跑」——
/// 后者问 [`relay_running`]。两件事分开，是因为「已经在跑」不该被报成失败。
/// 交给中转子进程的那份 **argv 尾巴**。
///
/// ⚠ 抽出来的理由与下面那份 env 逐字同一条〔`D6 阻-1` 的同族，08-29〕：
/// 不抽的话，「这条子进程是不是按 `--relay` 起的」只能靠**源码里有没有这段文本**来钉，
/// 而那一形本件已经被打穿过两次（`D5` 的 `X1` · `D6` 的 `Y1`）。
pub(crate) fn relay_child_args() -> Vec<String> {
    vec!["--relay".into()]
}

/// 交给中转子进程的那份 **环境**。**判据读它产出来的东西，不读源码文本。**
///
/// ★ 端口**显式传**：注入侧（`payload::RELAY_PORT`）与中转侧用同一个值。
/// ★★ `D1 阻-3`：**凭据路径也显式传**，同一条理由。
///
/// 不传的话，中转走它自己那条 `resolve_path` → `resolve_home()`，而那一条**认
/// `CLAUDE_CONFIG_DIR`** ⇒ monitor 是从一个**被监护进程继承来的环境变量**里
/// 决定「中转去读哪份凭据」的。而 monitor 自己写的那份**不跟随** `claudeDir`
/// （`creds_store::resolve_path` 头注逐字）⇒ 两侧读写的是两份文件，
/// 症状是「界面上配好了，中转说没配」——**一个静默的 404**。
/// ⇒ 由**写那份文件的那一侧**把路径说出来，别让它从环境里猜。
///
/// ⚠ 它**读一次真实家目录**（`creds_store::resolve_path()` 走 `dirs::home_dir()`）——
/// 只读，不写。拿不到家目录时那一格**缺席**（不是空串）：中转那时退回它自己那条
/// `resolve_home()`，而那正是上面这段话说的那个静默 404 的成因 ⇒ 缺席这一格不许被读成「安全」。
pub(crate) fn relay_child_envs() -> Vec<(String, String)> {
    let mut envs = vec![(
        "CCM_RELAY_PORT".into(),
        crate::backend::control::payload::RELAY_PORT.to_string(),
    )];
    if let Some(p) = crate::creds_store::resolve_path() {
        envs.push(("CCM_RELAY_CREDENTIALS".into(), p.display().to_string()));
    }
    envs
}

pub fn start_local_relay(bin: std::path::PathBuf) -> bool {
    let mut g = LOCAL_RELAY.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return false;
    }
    let h = local_backend::supervise(
        bin,
        relay_child_args(),
        relay_child_envs(),
        local_backend::CrashLimits::default(),
        std::sync::Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        }),
        // `KH2B2`②的一半：**起不来要出声**。`GaveUp` 单独抬到 `warn`
        // —— 它是「这台机器上的 api-key 号今天都发不出请求」的唯一线索。
        // ★★ `D2 阻-6`：**中转子进程的 stderr 被 `supervise` null 掉了**
        //（`local_backend::supervise_with_stdio` 里那行 `.stderr(Stdio::null())`），
        // 而中转**所有**诊断都写 stderr：启动 announce · 重载 announce ·
        // `cannot bind loopback port`。⇒ 那几句话**生产上一句都到不了人**。
        //
        // ⚠ 那一行不在本件写区（改它会同时改掉 daemon 那条监护路）⇒ **抬进上报口**。
        // 这里做的是**在写区内能做的那一半**：把监护器**已经交给我的事件**用起来 ——
        // ① `Exited` 带着退出码（中转的「起不来」恒是退 2）⇒ 出声；
        // ② `GaveUp` 时**把句柄从表里摘掉**，让 `relay_running()` 从此说真话。
        //    没有②的话：监护器已经放弃了，而句柄还在表里 ⇒ `relay_running()` 恒真 ⇒
        //    起会话那一侧**不再拒**，于是那条 api-key 会话被静默地起成一条连不上中转的会话
        //    —— 中转诊断到不了人的时候，这一格是用户**唯一**看得见的说法。
        std::sync::Arc::new(|e| match &e {
            local_backend::SuperviseEvent::GaveUp { reason } => {
                tracing::warn!(
                    "本机中转起不来（api-key 号的会话会被起会话那一侧拒掉）：{reason}"
                );
                // ⚠ 不能调 `stop_local_relay()`：那会 `stop()` 一个已经死了的句柄，
                //   而且这里就在监护线程上。只把它摘出表 —— 状态从此与事实一致。
                let mut g = LOCAL_RELAY.lock().unwrap_or_else(|e| e.into_inner());
                *g = None;
            }
            local_backend::SuperviseEvent::Exited { code, attempt } => {
                tracing::warn!(
                    "本机中转退出（第 {attempt} 次，退出码 {code:?}）—— \
                     中转的 `--relay` 起不来时恒退 2（端口被占 / 上游基址解析不了）。\
                     ⚠ 它自己的 stderr 被监护器 null 掉了，这一行是今天唯一的线索"
                );
            }
            other => tracing::info!("本机中转: {other:?}"),
        }),
    );
    // ⚠ 写法刻意不用 `*g = Some(h);` —— `local_backend` 那条接线判据用它当**锚点针**，
    //   而那条针要求全文件**恰好一处**（它的报文逐字：「断言指不明是哪一处」）。
    g.replace(h);
    true
}

/// `KH2B2`②的另一半：**起会话那一侧问得到「中转在不在」**。
///
/// ⚠ **诚实边界**：它问的是「**我们起过它、而且没停过**」，**不是**「那个口上真有人听」。
/// 两者分家的窗口是真的：子进程刚 spawn 还没 bind 的那几毫秒、以及 `GaveUp` 之后
/// （句柄还在表里，但监护器已经不再重起了）。
/// ⇒ 本函数**买不到**「一定连得上」；它买的是「**没起过就一定连不上**」那一侧 ——
/// 而那正是 `§0c-3` 成因㈡今天完全看不见的那一格。真要买另一侧得去连一次那个口，
/// 那是一次网络往返，**本件没做**。
pub fn relay_running() -> bool {
    LOCAL_RELAY
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// 停本机中转（形状照 [`stop_local_backend`]：句柄 `take` 走，`stop()` 之后它就是死的）。
pub fn stop_local_relay() -> Option<u32> {
    let mut g = LOCAL_RELAY.lock().unwrap_or_else(|e| e.into_inner());
    let h = g.take()?;
    let pid = h.current_pid();
    h.stop();
    pid
}

// ═════════════════════════════════════════════════════════════════════════════
// `D7 阻-3`：退出臂里**那两条自己不会死的起法**，各收成一个具名收口点
// ═════════════════════════════════════════════════════════════════════════════
//
// 它们先前是写在 `lib.rs` 那条 `RunEvent::Exit` 臂里的两段就地代码，
// 而守着它们的是一条**量文本**的判据 ⇒ `D7` 的刀 `T13` 把行为摘掉、文本留住 ⇒ 全绿。
// ⇒ 抽成具名函数 + 进 `lib::ExitShutdownSinks` 那条缝之后，
//   「勾了收几个」变成了一件**判据装替身就能数**的事（`lib::shutdown_detached_ways_on_exit`）。
//
// ⚠ **第三条（被监护那条）不在这里** —— 它必须留在退出臂体内，理由与残留的洞
//   逐字写在 `lib::ExitShutdownSinks` 的头注里（写区外那条判据要求它在臂里）。
// ⚠ 两个都返回 `bool`（「这一趟真的动手收了没有」）而不是 `Result`：
//   **退出路上没有人接得住错误**，失败只能靠日志说出来 —— 这一格先前就是这么做的，
//   本轮不改语义，返回值只供缝里那一行日志与判据的替身用。

/// 收口点 ①：**常驻（脱离）**那条起法〔`K-P1`〕。
///
/// 它没有 `SuperviseHandle`（那条路上**没有监护器** —— `K14` 裁的第一档），
/// 手里只有 pid + 二进制路径 ⇒ 收它走 [`stop_local_backend`]
///（我们起的那个直接 kill+wait；接管来的那个先核 `/proc/<pid>/exe` 再 SIGTERM）。
///
/// ⚠ **没脱离就什么都不做** —— 那一格由 [`is_detached`] 判，不是猜的。
pub fn stop_detached_backend_on_exit() -> bool {
    if !is_detached() {
        return false;
    }
    match stop_local_backend() {
        Ok(msg) => {
            tracing::info!("退出：{msg}");
            true
        }
        // **说出来**：这一格失败的后果是「用户勾了却没停」，静默就成了骗人。
        Err(e) => {
            tracing::warn!("退出：停常驻后端失败（{e}）—— 它还在跑");
            false
        }
    }
}

/// 收口点 ②：🔴 **中转是第三个进程**〔`D2 阻-5`（`K-H2b`）〕。
///
/// `relay/mod.rs` 自陈「独立进程」，[`stop_local_backend`] 一个字都碰不到它
/// ⇒ 必须单独收一次。没有它，用户勾了「退出时结束它」、退出，
/// **中转还在那儿听着那个口** —— 一个说谎的开关。
pub fn stop_relay_on_exit() -> bool {
    match stop_local_relay() {
        Some(pid) => {
            tracing::info!("退出：本机中转已停（pid={pid}）");
            true
        }
        None => {
            tracing::info!("退出：本机中转本来就没在跑");
            false
        }
    }
}

pub fn stop_local_backend() -> Result<String, String> {
    let mut g = LOCAL_BACKEND.lock().map_err(|e| format!("锁毒化: {e}"))?;
    // ★ `K-H2b`：中转跟着一起停。**锁序**与起那一侧一致（`LOCAL_BACKEND` → `LOCAL_RELAY`）。
    //   ⚠ 它**不改**下面那两条返回的文案 —— 那两条被判据逐字钉着，
    //     而「中转停没停」是另一件事，塞进同一句话里会让两个状态又合成一个值。
    let relay_pid = stop_local_relay();
    if let Some(p) = relay_pid {
        tracing::info!("本机中转已停（pid={p}）");
    }
    // ★ `K-P1`：常驻那条路的「停」。**锁序**：仍在 `LOCAL_BACKEND` 的锁里动 `DETACHED`。
    if let Some(msg) = stop_detached_locked() {
        return Ok(msg);
    }
    match g.take() {
        Some(h) => {
            let pid = h.current_pid();
            h.stop();
            Ok(format!("本机后端已停（pid={pid:?}）"))
        }
        None => Ok("本机后端本来就没在跑".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★★★ `D1 阻-6` 刀 B 的反面：**`relay_running()` 真的在看那张表，不是一个常量。**
    ///
    /// `D1` 实测过：把它整个换成 `true`，**1221 passed / 0 failed** ——
    /// 「中转在不在跑」这个**取值口**当时一条判据都没有，而它一旦恒真，
    /// `KH2B2`② 那道「起不来就当场拒」的闸就整个失效，**而且全绿**。
    ///
    /// # ⚠ 它**不起真 daemon**（红线）
    ///
    /// 喂给监护器的是一条**不存在的路径** ⇒ `Command::spawn` 立刻失败、监护器发 `GaveUp`
    /// 就收工。⇒ 这一趟里**没有任何子进程真的跑起来**，本条量的是
    /// 「句柄在不在表里」这条状态机，不是「那个进程活没活」（后者见本函数头注的诚实边界）。
    ///
    /// ⚠ 它动的是**进程内的全局** `LOCAL_RELAY` ⇒ 起完必须停掉，否则会影响同进程别的判据。
    #[test]
    fn relay_running_really_reads_the_handle_table() {
        // 前置：本条跑之前它必须是「没在跑」（否则下面第一条断言是空真）。
        assert!(
            !relay_running(),
            "起手就说在跑 —— 要么这个取值口恒真，要么别的判据把句柄留在表里了"
        );
        let bogus = std::path::PathBuf::from("/nonexistent/ccm-relay-that-cannot-spawn");
        assert!(start_local_relay(bogus.clone()), "第一次起应当报「起了一个新的」");

        // ★★ `D2 阻-6` 的那一半：**监护器放弃之后，这个取值口必须跟着说真话。**
        //
        // 那条二进制根本 spawn 不了 ⇒ 监护线程立刻 `GaveUp`。
        // 在本轮之前，`GaveUp` **只写一行日志**，句柄留在表里 ⇒ `relay_running()` **恒真**
        // ⇒ 起会话那一侧不再拒 ⇒ 那条 api-key 会话被静默地起成一条连不上中转的会话。
        // ⚠ 而中转自己的 stderr 被监护器 null 掉了（那一行不在本件写区）
        //   ⇒ **这一格是用户今天唯一看得见的说法**。
        let mut cleared = false;
        for _ in 0..400 {
            if !relay_running() {
                cleared = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            cleared,
            "监护器放弃之后句柄还留在表里 —— `relay_running()` 从此恒真，\n\
             而起会话那一侧那道「中转没起来就当场拒」的闸整个失效"
        );
        // 反面：表被真的清空了 ⇒ 还能再起一个（恒真的话这里会回 false）。
        assert!(start_local_relay(bogus), "表没被清干净 —— 再起时被当成「已经在跑」");
        stop_local_relay();
        assert!(!relay_running(), "停掉之后它还说在跑");

        // ⚠⚠ **另一半是源码形状，不是行为** —— `D6 阻-1` 把这一形判死了，而这一格
        //   **今天换不掉**，如实登记（08-29）：
        //
        //   上面几条量的是「它会不会**从真变假**」（那一格是行为，`D2 阻-6` 的正题）；
        //   量不出「它**恒假**」—— 要量恒假得让表里**稳定地**有一个句柄，而
        //   ① `SuperviseHandle` 造不出替身（构造子不对外）；
        //   ② 唯一能把句柄放进表的路是 `start_local_relay`，而它喂什么二进制都会被监护器
        //      在毫秒级内 `GaveUp` 摘掉 ⇒ 「刚起完那一瞬 `relay_running()` 是真」这条断言**有竞态**；
        //   ③ 喂一个**真活着**的进程就等于在判据里起一个常驻子进程（红线）。
        //   ⇒ 这一格留着一条文本断言，**它的绕过形态**逐字：把 `LOCAL_RELAY` 这个词留在
        //      这 200 字节窗口里（一个用不到的绑定就够）、把返回值换成常量 ⇒ 本格照绿。
        //
        //   ⚠ **失效方向要一起写**：`relay_running()` 恒假 ⇒ 每一次 api-key 号的拉起都被
        //   **当场拒**（`KH2B2`② 那条出声的路）—— 是 fail-closed、有声的，
        //   与「恒真」那一形（静默起成一条连不上中转的会话）**不同类**。恒真那一形有行为判据接着。
        let prod = guard_core::production_code(include_str!("local_daemon.rs"));
        let at = guard_core::find_pinned(&prod, "pub fn relay_running() -> bool {")
            .expect("`relay_running` 不是恰好一处");
        let body = &prod[at..at + 200.min(prod.len() - at)];
        assert!(
            body.contains("LOCAL_RELAY"),
            "`relay_running` 不再读 `LOCAL_RELAY` —— 它成了一个常量"
        );
    }

    // ★★★ `D5 阻-1`：**`the_two_inputs_at_the_call_site_are_still_the_two_take_points`
    //    这条判据整条删了**，新住址是 `history.rs` 里那**三条**判据
    //    （行为：`the_launch_side_really_asks_those_two_take_points_and_uses_their_answers` ·
    //     `the_ui_status_side_asks_those_two_take_points_and_uses_their_answers`；
    //     按函数地址对拍：`the_production_relay_facts_are_those_two_take_points`）。
    //
    // 删它的理由是一个实测读数，不是风格：它量的是「`relay_prefix_for_launch` 的体切出
    // 700 字节，那个窗口里**有没有**那两段文本」。`D5` 现打：在同一个窗口里加一行把那两段
    // 文本原样留住的死赋值，同时把真入参换成空表 / 常量 ⇒ 文本一处不少、判据照绿、
    // **全量门禁四个数与干净树逐字相同**，而中转注入在生产上被整个摘掉。
    // ⇒ 铁律 13「删之前先证明它恒绿」：`D5` 那一刀就是那份证明。
    //
    // ⚠ **别在这里补一个「更聪明的文本判据」**（比如切实参表按逗号分段再比字面量）——
    //   `D4` 那一轮的修法（把判据搬出被扫文件）买到的东西正是被下一层的量法漏掉的，
    //   而两轮的量法都是「量文本」。这一族已经连着五层了，出路是**不量文本**：
    //   两个事实走 `history.rs::RelayFactSources` 那条缝，判据喂替身、断言前缀随答案变。
    //
    // ⚠ 本文件上一条 `relay_running_really_reads_the_handle_table` **留着**，
    //   它买的是另一半（那个取值口自己真的读 `LOCAL_RELAY`），两者不重叠。

    /// ★★★ `K-H2b` `KH2B2`①：**中转有一个具名的启动方**，而且它在**起本机后端的那条路上**。
    ///
    /// # 非空对照写在这里（这一条的分母）
    ///
    /// 本件之前，全仓 `--relay` 的**生产调用点是 0** —— 中转是一条「有实现、没人起」的路。
    /// ⇒ 本条钉的就是那个 0 变成了 1，而且**不是随便哪儿的 1**：它必须落在
    /// `start_local_backend` 里、且排在 `start_detached` 那条**会 `return` 的**分支**之前**
    /// （接在它后面 = 走常驻那条路时中转不会被起，而那是生产上的主路）。
    ///
    /// # 它买不到什么
    ///
    /// 只买「接线在」，**不买「那个进程真的起来了」** —— 后者要真 spawn 一个 daemon
    /// 二进制，而它 `#[cfg(embedded_daemons)]` 门着、CI 上根本不铺（姊妹条
    /// `the_local_daemon_can_be_stopped_and_started_again` 那条边界原样适用）。
    ///
    /// # 🔴 本条里哪几格是**文本**，为什么今天只能是文本〔`D6` 回修，08-29，别读宽〕
    ///
    /// `D6 阻-1` 把「窗口里有没有这段文本」这一形判死了，而本条**没有全换掉** ——
    /// 换掉了的与没换掉的逐格写在这里：
    ///
    /// | 格 | 今天的量法 | 为什么 |
    /// |---|---|---|
    /// | argv 尾巴是 `--relay` | ✅ **行为**（读 [`relay_child_args`] 产出来的东西） | 抽成纯函数就够 |
    /// | 端口 == `payload::RELAY_PORT` | ✅ **行为**（读 [`relay_child_envs`]） | 同上 |
    /// | 凭据路径 == `creds_store::resolve_path()` | ✅ **行为**（读 [`relay_child_envs`]） | 同上 |
    /// | 起中转**落在** `start_local_backend` 里、在 `start_detached` **之前** | 🔴 **文本** | 这是一条**调用图**性质：按行为量要真跑 `start_local_backend`，而它 `#[cfg(embedded_daemons)]` 门着、且吃**真实**的 `~/.cc-monitor/bin` ⇒ 红线（不许手工起真 daemon） |
    /// | `start_local_relay` 真的把那份 spec 交出去了 | 🔴 **文本** | 按行为量要在 `local_backend::supervise` 上开一条缝，而 `backend/control/local_backend.rs` **不在本件写区** |
    ///
    /// ⇒ 那两格的**绕过形态**逐字：把 `relay_child_envs()` 的结果算出来扔掉、就地再拼一份
    /// （行为那三格照绿、文本这一格也照绿，因为那两个调用还在）。
    /// **登记，不假装钉住了。** 解锁条件 = 写区扩到 `backend/control/local_backend.rs`（一条 supervise 缝）。
    #[test]
    fn the_relay_has_a_named_starter_and_it_runs_before_the_detached_branch_returns() {
        // ① 🔴 `D6 阻-1` 同族：**行为** —— 交给中转子进程的那份 argv 尾巴与环境，
        //    量的是 `relay_child_args()` / `relay_child_envs()` **产出来的东西**，
        //    不是「源码里有没有这几段文本」（那一形本件已被打穿两次：`D5` 的 `X1` · `D6` 的 `Y1`）。
        assert_eq!(
            relay_child_args(),
            vec!["--relay".to_string()],
            "交给中转子进程的 argv 尾巴不再是 `--relay` —— 起出来的不是中转"
        );
        let envs = relay_child_envs();
        assert_eq!(
            envs.iter()
                .find(|(k, _)| k == "CCM_RELAY_PORT")
                .map(|(_, v)| v.as_str()),
            Some(crate::backend::control::payload::RELAY_PORT.to_string().as_str()),
            "端口没显式交给子进程、或交的不是注入侧那个常量 —— 注入侧\
             （`payload::RELAY_PORT`）与中转侧（daemon 的 `DEFAULT_PORT`）就成了各读各的两份默认值。\
             实得：{envs:?}"
        );
        assert_eq!(
            envs.iter()
                .find(|(k, _)| k == "CCM_RELAY_CREDENTIALS")
                .map(|(_, v)| v.clone()),
            crate::creds_store::resolve_path().map(|p| p.display().to_string()),
            "凭据路径不是从 monitor 写它的那条路（`creds_store::resolve_path`）来的 ——\
             中转会去读 `CLAUDE_CONFIG_DIR` 底下那份，而 monitor 写的那份**不跟随**它：\
             两侧读写的是两份文件，症状是一个静默的 404。实得：{envs:?}"
        );
        // 反空真：这把尺子分得出「少了一格」（不是恒相等）。
        assert!(
            envs.len() >= 2 && crate::creds_store::resolve_path().is_some(),
            "这台机器上算不出凭据路径 ⇒ 上面那条相等断言退化成 `None == None`，本条按红处理"
        );

        let me = include_str!("local_daemon.rs");
        let prod = guard_core::production_code(me);
        assert!(prod.len() > 5_000, "剥完只剩 {} 字节 —— 剥过头了", prod.len());
        // ② 它落在起本机后端那条路上，且**在常驻那条会 return 的分支之前**。
        let at = guard_core::find_pinned(&prod, "pub fn start_local_backend()")
            .unwrap_or_else(|e| panic!("`start_local_backend` 不是恰好一处：{e}"));
        let body = &prod[at..];
        let start = body
            .find("start_local_relay(bin)")
            .expect("`start_local_backend` 里没有那次起中转 —— 走这条路的机器上中转不会起");
        let detached = body
            .find("match start_detached(")
            .expect("找不到常驻那条分支");
        assert!(
            start < detached,
            "★ 顺序反了：起中转排在 `start_detached` **之后**，而那条分支会 `return` \n\
             ⇒ 走常驻那条路（生产主路）的机器上中转**根本不会被起**，\n\
             而症状是「api-key 号的会话被起会话那一侧拒掉」，指不向这里。"
        );
        // ③ 上面第 ① 格量的是那份 spec **产得对不对**（行为）；这一格量的是
        //    `start_local_relay` **真的把它交出去了**。
        //    ⚠ 这一格今天**只能量文本**，如实登记（见本条头注最后一节）：
        //      要按行为量得先在 `local_backend::supervise` 上开一条缝，而那个文件不在本件写区。
        //    ⚠ 作用域是 `start_local_relay` 的函数体，**不是** `start_local_backend` 的
        //      —— 第一版写错了作用域，被本条自己当场逮住（那也是「量具的作用域对不上事实」）。
        let relay_at = guard_core::find_pinned(&prod, "pub fn start_local_relay(")
            .unwrap_or_else(|e| panic!("`start_local_relay` 不是恰好一处：{e}"));
        let relay_body = &prod[relay_at..relay_at + 1_200.min(prod.len() - relay_at)];
        assert!(
            relay_body.contains("relay_child_args()") && relay_body.contains("relay_child_envs()"),
            "起中转那一处不再把 `relay_child_args()` / `relay_child_envs()` 交出去 ——\n\
             上面第 ① 格量的那份 spec 就成了一份没人用的摆设（端口与凭据路径又回到各读各的）。\n\
             实得片段：{relay_body}"
        );
    }

    // ══ 第二道锁 ② 的**唯一入口**〔`K-R7` 09-01，`§0q` 裁一 · 出路乙〕═══════════
    //
    // # 它为什么存在：`D4` 一刀走过了原来那道判据
    //
    // 原来 ② 靠的是一条**同行文本形状**：「本体里有一行同时含 `CCM_E2E_TMUX_SHIM_BIN`
    // 与 `.expect(`」。`D4` 09-01 实测把取值换成
    // `var(SHIM).unwrap_or_else(|_| var(<真 PATH 那个变量>).expect(..))` ——
    // **同一行上两样都还在** ⇒ 判据说合规，而变量缺席时 shim = 真 PATH，
    // daemon 沿真 PATH 找到真 tmux ⇒ 装用户的 `[50]`。**全量门禁新红 0。**
    // 那正是 `§1` 表里 08-26 / 08-27 / 08-29 各犯一次的**降级裸跑**。
    //
    // # 买到的是什么：**把「fail-closed」这件事从文本形状换成运行期读数**
    //
    // 下面 `require_tmux_shim` 是**纯函数**（喂它一个 `Result`，不读环境）
    // ⇒ 「拿不到就炸」这条性质能被一条**普通 `#[test]`**（不带 `#[ignore]`）
    // 在**默认门禁里真跑一遍**：[`the_one_shim_gate_really_fails_closed`]。
    // 谁把这里改成降级，**门禁当场红** —— 那是机器守的，不是纪律、也不是文本钉。
    //
    // # 买不到的是什么（别读大）
    //
    // 「**这条测试走不走这个口**」仍然是一条**文本判据**（守卫里的 ㈠：本体里出现
    // `demand_tmux_shim(`）。攻击面从**5 处**缩到**1 处**，不是缩到 0：
    // 一条根本不进人群的测试（诚实边界 5「第四种来历」）照旧够不着。
    //
    // 🔴🔴 **这一段 09-01（`C` 第六拍）整段改掉了 —— 原文是一句已经被证伪的全称**
    //   〔`D5` 阻塞 1；PM `§0s` 五 ②，**无条件改，与那个缺口修不修无关**〕。
    //   **原文逐字**（留在这里，因为它下过一次结论，被引过）：
    //     「⚠ 而**本体里真的调到了这个口**（不是写在死代码里、不是包在一个没跑到的闭包里）时，
    //       缺变量就会炸 —— 因为**炸在口里，不在调用处的写法上**。
    //       这一格比原来那条同行判据强的正是这里：调用处再怎么套 `unwrap_or_else`，
    //       只要那一跳最终落进这个口，fail-closed 就还在。」
    //   **`D5` 一刀证伪它**（实跑，**全量门禁 `GATE: OK`、新红 0**）：
    //     let shim = std::panic::catch_unwind(|| crate::local_daemon::demand_tmux_shim(".."))
    //         .unwrap_or_else(|_| std::env::var("PATH").unwrap_or_default());
    //   闭包**真跑了** · `panic!` **真发生在口里** · 调用处套的**恰好就是 `unwrap_or_else`**
    //   —— 而变量缺席时 `shim` 就是**真 `PATH`**，daemon 沿真 `PATH` 找到真 tmux。
    //   ⇒ 那句话把「**口是 fail-closed 的**」（真）读成了「**走口的测试就是 fail-closed 的**」（假）。
    //     两者之间隔着「**谁能接住那声 panic**」，而**这一格今天全量门禁一声不吭** ——
    //     分母写清楚（`brief` 12）：= `D5-M6` 那一趟跑到的全部判据，逐格
    //     `cargo 1306 · daemon 488 · npm 1480 · pb check FAIL=0`，**新红 0**。
    //     ⇒ 只能说到这里：**我们跑的这套门禁里没有一条守它**；不能说「不存在任何判据」。
    //
    // # 🔴 说得准的话：这道闸买到的是**防一部分手滑**，买不到**挡住存心绕**
    //
    // 🔴🔴 **「防手滑」这个量词 09-01（`C` 第七拍）收窄了一格，别再读成全称**〔`D6` `B3`；
    //   PM `§0u` 二〕：**多行块注释那一形（把真调用整段包进 `/* */` 换个桩值）就是一次手滑，
    //   而它今天一声不吭地过** —— 全量门禁 `GATE: OK`、新红 0（本拍 `M-c7-D` 实打）。
    //   ⇒ 逐条登记在下面守卫头注**诚实边界 9**，归跟进件 `K-R8`。
    //   **说得准的写法是「我量过的这几形挡得住」，不是「手滑挡得住」**（`brief` 12：
    //   写「全部 / 任何」就是在报一个数，同句给分母 —— 而「手滑的所有形状」这个分母给不出）。
    //
    // 〔PM `§0s` 五 ③ 逐字要求写清这条缝，并**不许再用「保证」「买断」「就还在」这类词**。〕
    //
    // **买到**（每条都有实跑读数，住址在 `audits/K-R7-D5.md`，我没有重打的会写明）：
    // · 谁把口里那句 `panic!` 改成降级 ⇒ **门禁当场红**（`D5-M1`：新红 1，只点
    //   [`the_one_shim_gate_really_fails_closed`]；`C` 第六拍**重打过，读数逐字相同**）；
    // · 反空真那半格不是仪式（`D5-M9`：退掉 ⇒ 新红 1；**这一刀我没重打**）；
    // · 旧的「自己现取」写法今天被拒（`D5-M3` 与 PM 各打一趟 ⇒ 新红 2；
    //   `C` 第六拍的 `M-c6-1` 又打了一遍同形，新红 2）；
    // · `D5` 另试的 3 形**全被现有判据逮住或本来就会炸**（分母 = 它试的那 3 形，不是全体）。
    //
    // **买不到**（两条，第二条 09-01 补上来）：
    // ① **调用方在「口」与「调用点」之间接住那声 panic 再降级**（上面那一刀）。
    // ② **把真调用留成多行块注释、换个桩值** —— 内层那一行**只要自己不以 `//` / `*` / `/*`
    //    打头**就剥不掉、照旧被读成一条合法绑定
    //    ⇒ 四道判据一条都不红（本拍 `M-c7-D`：全量门禁 `GATE: OK`，新红 0）。
    //    **这一条是手滑，不是存心绕**（多行块注释正是编辑器「注释掉这几行」的默认产物）。
    //    ⚠ **买不到的只是这一形，不是「多行块注释」这一族**〔`C8` 09-01 收窄，`D7` `B1`〕：
    //    内层行写成 ` * let …`（块注释的 `*` 对齐续行写法）今天**剥得掉** ⇒ 两条守卫
    //    一起红（实打，新红 2）—— **那一形是买到的**。分母 = 我量过的这几形。
    // ⚠ **这条缝不是纯理论的**：`catch_unwind` 本来就是这两个文件里的**现成写法** ——
    //   `local_backend.rs` 的 `local_stdio_consumer_guarded` 里一处，
    //   本文件 [`the_one_shim_gate_really_fails_closed`] 自己也用了一处 ⇒ **照抄的成本为零**。
    //   （`D5` 自己也如实写了另一侧：`catch_unwind` 得**有意**去接住 panic，
    //   它不主张这一形与「手滑」同样可能。两句话都留着，别只引一半。）
    //
    // 🔴🔴 **别再在这里加第五层文本钉**〔PM `§0s` 五 ④ 逐字：想加要顶回去说理由，不许自批〕。
    //   前五拍每一拍都给调用处钉一个形状，下一拍就找到另一个**同行为**的形状
    //   （08-26 / 08-27 / 08-29 三次降级裸跑 · `D4` 的 fallback · `D5` 的 `catch_unwind`）。
    //   PM 09-01 裁定这条路**买不到这条性质**，两条理由各自独立成立：
    //   ① **panic 是可以被接住的** —— 入口做得再好也守不住调用方；
    //   ② **生产合法地需要那条不隔离的路**（真实用户就是不带 shim 起 daemon）
    //      ⇒ **在这两个前提下**（生产要留一条不带 shim 的入口 · 测试与生产同 crate、
    //        因此调得到那条入口），**没有类型能让「带 shim」变成强制的**。
    //        ⚠ 前提是承重的，别把这句读成无条件的：把这几条 e2e 搬进一个独立的集成测试
    //        crate、只暴露带见证的入口，这句话就不成立了 —— 那是另一件事的体量。
    //   ⇒ 真正让这条性质成立的做法是让 **daemon 只碰别人显式交给它的那个 tmux server**；
    //     那要动生产段，**另立跟进件**，不在本件写区。
    //
    //   ⚠⚠ **它也管不到「调用排在 spawn 之前还是之后」** —— 那一格没钉，登记在
    //   下面守卫头注 ㈠ 那一段（`D4 §F-3`）。炸得晚，daemon 已经起来了。
    /// 那个变量的名字。
    ///
    /// ⚠ **别把下面这句写宽**（`brief` 12：写「只有 / 全部」就是在报一个数，同句给分母）。
    /// **分母 = 仓内全部 `.rs`（去 `target` / `node_modules` / `vendor` / `.git` / `dist` /
    /// `embedded-daemons`）里非 `//` 打头的代码行；量于 09-01**。现打：
    /// · 提到这个名字的代码行 **7** 处（`local_daemon.rs` 5 · `local_backend.rs` 2）——
    ///   其中 5 处是**判据自己的 `const`**（`SHIM` / `PRIVATE_TMUX` / 本 `const` / 报文串）；
    /// · 真去 `std::env::var(..)` **取它的值**的 **3** 处：本模块的 [`demand_tmux_shim`]
    ///   （`:1372`，它经上面那个 `const` 取，所以**不在**刚才那 7 行里）
    ///   ＋ 两条 `let _ = std::env::var(..)` 的**纯读点名行**（`:4071` / `:4138`，
    ///   它们存在的理由是喂饱姊妹守卫 `every_ignored_test_here_…` 的 `c.contains(PRIVATE_TMUX)`，
    ///   **不取值、不影响 fail-closed**）。
    ///   🔴🔴 **这三个住址 09-01（`C` 第六拍）全部改过 —— 原来三个数都是错的**〔`D5` 阻塞 2；
    ///   PM `§0s` 五 ①〕。原文写的是 `:1266` / `:3672` / `:3739`：
    ///   · `:1266` **指错了函数** —— 那一行是 [`require_tmux_shim`] 的签名，不是这一个；
    ///   · `:3672` / `:3739` 在**父提交上是对的**，而写下它们的那个提交自己把文件推后了 16 行
    ///     ⇒ **落盘那一刻就馊了**。
    ///   ⚠⚠ **行号是「每轮都变的量」**（`brief` 12 逐字：一句话里嵌了这种量，
    ///   它下一轮自动变成假话）。⇒ 上面三个数**只是一次快照**，量于本模块这一拍的尖，
    ///   **别当常量抄**。
    ///   🔴 **这不是理论：`C` 第六拍自己就把这三个数馊了两次** —— 第一次写下 `:1322` 一族，
    ///   下一个提交在它们上方加了注释就馊；改成 `:1333` 一族，收工自查又加了四行，
    ///   **又馊一次**。⇒ 谁再动本文件 1213 行以下的注释，这三个数就该重打。
    ///   🔴 **`C` 第七拍（09-01）第三次兑现了这一条，而且是自己撞的**：那一拍在 `:1259` 一带
    ///   与守卫头注里加了注释 ⇒ 三个数全部推后（`:1343`/`:3911`/`:3978` →
    ///   `:1359`/`:4052`/`:4119`，文件 4002 → 4143 行）。⚠ **它前四遍写的 `:1354` / `:1358` /
    ///   `:4042` / `:4046` 一族都还是错的** —— 每写一句订正就又往上面加行，**它四次把自己推后**。
    ///   🔴 **`C` 第八拍（09-01）第四次兑现，同样是自己撞的**：本拍只改话（三处活报文 ＋
    ///   两处注释 ＋ 本段），而改话照样加行 ⇒ 三个数又全部推后（`:1359`/`:4052`/`:4119` →
    ///   `:1372`/`:4071`/`:4138`，文件 4143 → **4162** 行）。上面那三个住址已按本拍的尖重写。
    ///   那 7 处 / 5 · 2 的分布**重打后没变** —— 真理由是**新增行一行都进不了这个分母**：
    ///   `aee8b9f..0b8dade` 的 **202** 行新增里，含本 `const` 那个字面量的 **0** 行
    ///   （`C8` 09-01 现打，量具 `scratchpad/kr7-c8-b3.py --diff aee8b9f 0b8dade`）。
    ///   🔴 **原来写在这里的理由「新加的字全在 `//` 与 `///` 行上」是假的**〔`D7` `B3`，
    ///   `C8` 复打成立〕：那 202 行里 `trim_start()` 后**不以 `//` 打头**的有 **50** 行
    ///   （`local_daemon.rs` 21 · `local_backend.rs` 29）—— 它们只是**一个都不含那个字面量**。
    ///   ⇒ **结论对、理由假是两件事**：照那句假理由，下一个人会判「这次不用重打」。
    ///   要重打就跑这一条（站在工作树根，与本段口径逐字相同）：
    ///   · `git ls-files '*.rs' | grep -vE '(^|/)(target|node_modules|vendor|dist|embedded-daemons)/'`
    ///     再逐文件取「`trim_start()` 后不以 `//` 打头、且含 `CCM_E2E_TMUX_SHIM_BIN`」的行。
    /// ⇒ 只能这么说：**「拿它当 shim 目录用」的取法只有 [`demand_tmux_shim`] 这一处**；
    ///   **不能说**「全仓只有一处碰这个变量」——那句话是假的。
    pub(crate) const TMUX_SHIM_VAR: &str = "CCM_E2E_TMUX_SHIM_BIN";

    /// **纯函数**：把「取变量的结果」变成 shim 目录。**拿不到就炸，绝不降级。**
    ///
    /// 它刻意**不自己读环境** —— 读环境的是下面那个 [`demand_tmux_shim`]。
    /// 分开的理由只有一条：**这样「fail-closed」才测得了**，
    /// 不用去动进程级的环境变量（那会跟别的测试线程打架，也会让读数依赖跑法）。
    pub(crate) fn require_tmux_shim(got: Result<String, std::env::VarError>, why: &str) -> String {
        match got {
            Ok(dir) => dir,
            Err(e) => panic!(
                "要 {TMUX_SHIM_VAR}（{e:?}）—— {why}\n\
                 ★ 被起的 daemon 一上来就往它连得到的 tmux server 装三条**全局** hook\n\
                 （固定槽位 [50]，**没有关掉它的开关**）⇒ 不隔离就是去改用户真实 tmux 的状态。\n\
                 ⚠ 这不是理论：08-11 打没过用户 9 个真实会话；08-26 / 08-27 / 08-29 各盖过一次 [50]。\n\
                 ⇒ 必须 **fail closed**：拿不到 shim 就炸，**绝不降级裸跑**。\n\
                 跑法：bash e2e/local-backend-supervise.sh"
            ),
        }
    }

    /// 五个落点取 shim 的**唯一**口子。加新的起真 daemon 的测试，也从这里取。
    pub(crate) fn demand_tmux_shim(why: &str) -> String {
        require_tmux_shim(std::env::var(TMUX_SHIM_VAR), why)
    }

    /// P2s-Y2（acceptor: **实测**）：**停得掉 · 起得回来 · 状态跟着变**。
    ///
    /// # 为什么按 `/proc` 看而不是读状态字段
    ///
    /// DoD 自陈的失效方式逐字：「**「状态说停了」不等于「进程真没了」**」。
    /// 只读我们自己维护的那个字段是**自证** —— 把 `stop()` 整个换成「只改字段」也会绿。
    /// ⇒ 每一步都按 pid 看 `/proc`，状态字段只作为**第二条**断言。
    ///
    /// # 沙箱：`HOME` 与 `CLAUDE_CONFIG_DIR` 都要设
    ///
    /// daemon 的 `resolve_agent_home()`〔`S4b` 前叫 `resolve_claude_dir()`〕是
    /// `$CLAUDE_CONFIG_DIR` 优先、`$HOME/.claude` 兜底。
    /// 只设 `HOME` 那版跑起来一切正常，隔离却是假的（P2 那条实测栽过一次，见它的头注）。
    ///
    /// ⚠ 本条**不调 `start_local_backend()`** —— 那个函数吃的是**真实**的 `~/.cc-monitor/bin`
    /// 且不接受环境注入 ⇒ 在测试里调它就会读用户真实的配置目录。
    /// 它那半（宿主知识 + 幂等）由下面那条机检管。
    /// ⚠⚠ **诚实边界（补审 08-11 逮到本条没登记）**：它 `#[cfg(embedded_daemons)]` 门着，
    /// 而 **CI 的 `cargo test` 之前一步都不铺 `src-tauri/embedded-daemons/`**
    ///（铺它的是 `release.yml`，不是 `ci.yml`）⇒ `build.rs` 不置 cfg
    /// ⇒ **本条在 CI 上等于不存在**。
    ///
    /// 姊妹条 `the_local_daemon_really_registers_an_inbound_client` 登记了这条边界（10c），
    /// **本条当时没登记** —— 读它的人会以为 `P2s-Y2` 有持续的实测证据。今天它只在
    /// 「开发机上、且 `embedded-daemons/` 齐」时才跑过。
    ///
    /// ⚠ 另一条边界：它**不调生产的 `start_local_backend` / `stop_local_backend`**
    /// （前者读真实 `~/.cc-monitor`、不接受环境注入）⇒ **生产的停口零覆盖**，
    /// 那一格由 `the_stop_command_really_calls_this_module` 的源码接线钉补上。
    ///
    /// # ★★ `K-R7`（08-31）：本条从「普通 `#[test]`」改成「`#[ignore]` + fail-closed + 真用 shim」
    ///
    /// 〔用 08-29〕逐字：「**你只能做产品, 不能动机器**」。而在此之前**本条自己在动机器**：
    /// 它是**普通 `#[test]`**（只由 `cfg(embedded_daemons)` 门着），起一个真 daemon，
    /// 而 daemon 一上来就**无条件**往它连得到的 tmux server 装三条**全局** hook
    /// （固定槽位 `[50]`，**没有关掉它的开关**）。
    /// ⇒ **任何人在铺了 `embedded-daemons/` 的树上跑一次 `cargo test`**（**包括用户自己
    /// clone 下来跑一遍**）**都会改这台机器的 tmux 全局状态**。已经发生过三次
    /// （08-26 实现方 · 08-27 PM · 08-29 PM）⇒ **靠纪律这一档已经实证无效**。
    ///
    /// 三道锁一起上，少一道都不够：
    /// ① `#[ignore]` ⇒ 默认 `cargo test` **跑不到它**；
    /// ② `CCM_E2E_TMUX_SHIM_BIN` **fail-closed**（`expect`，且排在**任何 spawn 之前**）
    ///    ⇒ 连 `cargo test -- --ignored` 也不能裸跑它；
    /// ③ 那个 shim **真的挂进被监护 daemon 的 `PATH` 最前面**。
    ///
    /// # ⚠⚠ ③ 的射程要**分两格看**〔`D1` 审计 08-31 查实，阻塞 4 的另一半〕
    ///
    /// 本段原来只写一句「③ 不是装饰：只做 ①② 会变成一次仪式 —— 要了一个变量却不用它，
    /// daemon 照样沿真 `PATH` 找到真 tmux」。**那句话把两格混着写了**，逐格拆开：
    ///
    /// | 跑法 | ③ 买到什么 | 为什么 |
    /// |---|---|---|
    /// | **文档指定的那条**：`bash e2e/local-backend-supervise.sh` | **冗余** | `e2e/tmux-shim.sh` 里那句 `export PATH="$TMUX_SHIM_BIN:$PATH"` 已经把 shim 挂进了**测试进程自己的 `PATH`**，而 `supervise_with_stdio` 只做 `env_remove("TMUX")` + 逐条 `env(k, v)`、**从不 `env_clear()`** ⇒ 子进程本来就继承那份带 shim 的 `PATH` |
    /// | **手工 `cargo test -- --ignored`**（变量设上、但 shim 不在自己 `PATH` 上） | ★ **这一格是真的** | 没有 ③ 的话，daemon 继承的是那个人的真 `PATH` ⇒ 沿真 `PATH` 找到真 tmux ⇒ 盖用户的 `[50]` |
    ///
    /// ⇒ **③ 值得留，但别把它读成「e2e 那条路上也靠它挡着」** —— 那条路上挡住的是 shim 的继承。
    /// 看着 ③ 的判据是 `every_test_that_starts_the_real_daemon_demands_a_private_tmux`
    /// 的 ㈡ 与 ㈢ **两格**；在它们落地之前，
    /// **③ 退掉全量门禁一条都不红**（审计 `M-α` / `M-α′` 两刀实测，新红各 0）。
    /// - ㈡ **写法**（`"PATH"` 与 `"{<绑定名>}:` 同行，插值**紧跟开引号**）—— `D1` 08-31 买回，
    ///   `D2` 09-01 把「冒号左边」收紧成「串首」〔阻塞 1 形 ①：`format!("/usr/bin:{shim}:{}", PATH)`
    ///   写法上 shim 排第二，原判准照样放行，实测新红 0〕；
    /// - ㈢ **处数**（`PATH` 这个 env 键在本体里**恰好写一次**）—— `D2` 09-01 买回
    ///   〔阻塞 1 形 ②：同一张 `envs` 里再追加一条 `("PATH", …)`，`supervise_with_stdio` 的
    ///   `for (k, v) in &envs { cmd.env(k, v); }` **后写的赢**，daemon 拿到的 `PATH` 里
    ///   一点 shim 都没有，而 ㈡ 照样说合规，实测新红 0〕。
    /// ⚠ 两格**刻意分开**（`K13`）：破 ㈡ 要「特意在串首前面塞一段」，破 ㈢ 只要「顺手再加一条环境变量」。
    ///
    /// ⚠⚠ 而**只清 `TMUX` 是不够的**（`supervise_with_stdio` 内部就在清它）：
    /// `TMUX` 一空，tmux 客户端**回落到默认 socket** `/tmp/tmux-$UID/default` ——
    /// 那**正是**用户那台 server。隔离必须靠**显式选择器**（shim 强插 `-L`/`-S`），
    /// 不能靠「不继承某个变量」。
    ///
    /// # 🔴 复跑纪律：**换了 `embedded-daemons/` 的有无之后，必须 `touch src-tauri/build.rs`**
    ///
    /// 〔`D2 §G-1` 的陈账，09-01 收。**此前它只写在件文件里 PM 的裁定段中，
    /// 被守对象这一侧一个字都没有** ⇒ 落在了读不到它的地方（铁律 14 那一族：
    /// 写进功能件**并**落进被守对象才算存在）。本条与下面姊妹条各留一份。〕
    ///
    /// 本条由 `#[cfg(embedded_daemons)]` 门着 ⇒ **它在不在，取决于 `build.rs` 那一趟看见了什么**。
    /// **实测的现象**（09-01 `D2` 复审，我没重打，住址 `audits/K-R7-D2.md#§G-1` 与件文件 `§0k` 末段）：
    /// 在**同一个 `CARGO_TARGET_DIR`** 里把 `src-tauri/embedded-daemons/` 从「无」加成「有」，
    /// **`build.rs` 不重跑**，第一趟读到 `1208 / 0 / 10`，`touch build.rs` 之后才是 `1211 / 0 / 11`。
    /// ⚠⚠ **这个坑看不出来**：两档的输出面长得一模一样，读到的是「无 emb」那一档的数。
    ///
    /// ⇒ **纪律**：换 emb 状态之后 `touch src-tauri/build.rs`；
    /// 最省事的做法是**换一个全新的 target 目录名**（`build.rs` 从零跑一遍，结构上撞不到）。
    ///
    /// ⚠ **机制我没有实验证明是哪一条，两条候选都记在这里，别当结论用**：
    /// ① 那几个二进制常是 `cp -p` 拷进来的，**mtime 停在拷贝源那一刻**（比上一次 build 还旧）
    ///    ⇒ cargo 的指纹看不出「输入变新了」；
    /// ② 有一处把它归因到「`build.rs` 里那句 `rerun-if-changed` 只在文件存在时登记」——
    ///    **09-01 现打源码，那一行是无条件的**（`for arch in [..]` 里直接 `println!`，
    ///    不在任何 `exists()` 分支中；`exists()` 那个分支只管 staleness 提示）
    ///    ⇒ **那个归因对不上源码**。现象是真的，归因不是。
    #[cfg(all(embedded_daemons, target_os = "linux", target_arch = "x86_64"))]
    #[test]
    #[ignore = "K-R7：起真 daemon ⇒ 会装全局 tmux hook。走 e2e/local-backend-supervise.sh 那条带 shim 的路"]
    fn the_local_daemon_can_be_stopped_and_started_again() {
        use std::path::Path;
        use std::sync::Arc;
        use std::time::Duration;

        // ★★ **fail closed，而且排在一切之前**：拿不到 shim 就当场炸，绝不降级裸跑。
        //    降级裸跑 = 去改用户真实 tmux server 的 `[50]` 槽位（08-11 / 08-26 / 08-27 / 08-29 各一次）。
        let shim = demand_tmux_shim(
            "本条起真 daemon，而 daemon 一上来就往它连得到的 tmux server 装全局 hook",
        );
        let _guard = crate::inbound_client::local_origin_test_lock();
        let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("embedded-daemons")
            .join("cc-monitor-remote-x86_64");
        let home = std::env::temp_dir().join(format!("p2s-restart-{}", std::process::id()));
        let cfg_dir = home.join(".claude");
        std::fs::create_dir_all(cfg_dir.join("projects")).expect("建沙箱 HOME");
        let envs = vec![
            ("HOME".to_string(), home.display().to_string()),
            (
                "CLAUDE_CONFIG_DIR".to_string(),
                cfg_dir.display().to_string(),
            ),
            // ★ 隔离**真的用上**：shim 目录挂在 `PATH` 最前面，daemon shell out 的 `tmux`
            //   会被强插显式选择器 ⇒ 它连得到的只有 e2e 自己那台 server。
            (
                "PATH".to_string(),
                format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
            ),
        ];
        let now = || {
            Arc::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            })
        };
        let spawn = || {
            local_backend::supervise_with_stdio(
                bin.clone(),
                vec!["--tail-only".into()],
                envs.clone(),
                local_backend::CrashLimits::default(),
                now(),
                Arc::new(|e| println!("[P2s 实测] {e:?}")),
                Some(Arc::new(local_backend::local_stdio_consumer)),
            )
        };
        let wait_channel = |want: bool| -> bool {
            for _ in 0..100 {
                let on = crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN)
                    .is_some();
                if on == want {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            false
        };
        let alive = |pid: u32| Path::new(&format!("/proc/{pid}")).exists();

        // ── 起 ────────────────────────────────────────────────────────
        *LOCAL_BACKEND.lock().expect("锁") = Some(spawn());
        assert!(wait_channel(true), "5s 内通道没登记上 —— daemon 没起来");
        let pid1 = crate::daemon_control::daemon_status(crate::inbound_client::LOCAL_ORIGIN.into())
            .expect("查状态")
            .get("pid")
            .and_then(|v| v.as_u64())
            .expect("起来了却没有 pid") as u32;
        assert!(alive(pid1), "状态给了 pid={pid1}，但 /proc 里没有这个进程");
        // ★ `K-R7`：本条改成 `#[ignore]` 之后由 `e2e/local-backend-supervise.sh` 驱动，
        //   而那个脚本的收尾自检是「**标记数 < 跑成的测试数 ⇒ 有测试提前退出**」
        //   ⇒ 不打标记的话，它一进那条 `--ignored` 路就会把套件判红，
        //   而红的理由是**假的**（不是断言没走完，是本条从来不打标记）。
        println!("E2E-OK P2s 本机后端起得来（pid={pid1}，/proc 里真有）");

        // ── 停：进程必须**真的**没了 ──────────────────────────────────
        LOCAL_BACKEND
            .lock()
            .expect("锁")
            .as_ref()
            .expect("刚起的")
            .stop();
        let mut gone = false;
        for _ in 0..100 {
            if !alive(pid1) {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            gone,
            "`stop()` 之后 pid={pid1} 还在 /proc 里 —— 「状态说停了」不等于「进程真没了」。\n\
             ★ 这条断言刻意不读状态字段：只改字段的实现会让那种断言绿着过。"
        );
        assert!(wait_channel(false), "进程没了，通道却还挂在登记表里");
        println!("E2E-OK P2s `stop()` 之后 pid={pid1} 真的从 /proc 里消失、通道也摘了");

        // ── 再起：必须是**新的**一条命 ────────────────────────────────
        *LOCAL_BACKEND.lock().expect("锁") = Some(spawn());
        assert!(
            wait_channel(true),
            "停了之后起不回来 —— 那就只有「停」没有「起」"
        );
        let pid2 = crate::daemon_control::daemon_status(crate::inbound_client::LOCAL_ORIGIN.into())
            .expect("查状态")
            .get("pid")
            .and_then(|v| v.as_u64())
            .expect("再起之后没有 pid") as u32;
        assert!(alive(pid2), "再起给了 pid={pid2}，但 /proc 里没有");
        assert_ne!(
            pid1, pid2,
            "两次拿到同一个 pid —— 那说明「再起」其实什么都没做，\n\
             或者句柄根本没被换掉（`OnceLock` 时代就是这个形态：写一次就锁死）。"
        );
        println!("E2E-OK P2s 停了之后起得回来，而且是**新的**一条命（{pid1} → {pid2}）");

        // ── 收尾 ──────────────────────────────────────────────────────
        if let Some(h) = LOCAL_BACKEND.lock().expect("锁").take() {
            h.stop();
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    /// ★ **接线钉之三：停口**〔D 阶段补审 08-11 新增〕。
    ///
    /// 补审逐字：起那半有 `the_startup_path_really_calls_this_module` 钉着，
    /// **停那半没有对称物** —— `daemon_stop → stop_local_backend` 这根线断了不会有任何东西红，
    /// 而 `stop_local_backend` 里那句 `g.take()`（头注专门解释「不 take 会让下一次『起』
    /// 误以为还在跑」）改成 `g.as_ref()` 也不会红。
    ///
    /// ⚠ 射程：本条是**源码接线钉**，只证明那根线写在那里；
    /// 「停了进程真没了」那半由 `local_daemon::tests` 里那条实测管（而它今天 `#[cfg]` 门着，见 10c）。
    #[test]
    fn the_stop_command_really_calls_this_module() {
        let dc = guard_core::production_code(include_str!("daemon_control.rs"));
        guard_core::find_pinned(&dc, "local_daemon::stop_local_backend()").unwrap_or_else(|e| {
            panic!(
                "`daemon_control` 的停口没有接到 `stop_local_backend`（{e}）——\n\
                 那么 UI 上的「停」对本机是个空动作，而它照样回一句成功的话。"
            )
        });
        let me = guard_core::production_code(include_str!("local_daemon.rs"));
        let at = guard_core::find_pinned(&me, "pub fn stop_local_backend(").expect("停口不在了");
        let body: String = me[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        guard_core::find_pinned(&body, "g.take()").unwrap_or_else(|e| {
            panic!(
                "`stop_local_backend` 不再把句柄 `take()` 走（{e}）。\n\
                 ★ 留着它的后果很具体：`start_local_backend` 的判据是「句柄在表里 = 在跑」\n\
                 ⇒ 停完之后再点「起」会被当成「已经在跑」拒绝，本机后端再也起不回来。"
            )
        });
    }

    /// ★★ **本机只许用本平台能跑的二进制**〔D 阶段补审 08-11 新增〕。
    ///
    /// 内嵌的两份是 **musl Linux**（`build.rs` 只认 `embedded-daemons/cc-monitor-remote-{x86_64,aarch64}`），
    /// 而 `sftp::daemon_binary(ARCH)` **只按 arch 分派、不看 OS**。
    /// 少了这道门，Windows/macOS 上会释放一个 Linux ELF、`start_or_extract` 回 `Resolved::Found`
    /// ⇒ **UI 与日志报告「已起」，而进程从来没起来过**（补审阻塞 C1）。
    ///
    /// # 为什么钉在这里而不是钉 `sftp::daemon_binary`
    ///
    /// 那个函数**同时供远端部署用**，目标就是 Linux ⇒ 在那条路上用 musl 二进制是对的。
    /// 本条只钉「**本机这条取用点**必须先问 OS」。
    ///
    /// ⚠ 射程：它是**源码判据**，只证明那道门写在那里；证明不了「Windows 上真的不会释放」——
    /// 那要一台 Windows（归 `auto-e2e`）。如实登记，不拿源码判据冒充跨平台实测。
    #[test]
    fn the_local_backend_only_takes_a_binary_this_platform_can_run() {
        let prod = guard_core::production_code(include_str!("local_daemon.rs"));
        let at = guard_core::find_pinned(&prod, "pub fn start_local_backend(").expect("入口不在了");
        let body: String = prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        let take = guard_core::find_pinned(&body, "daemon_binary(").unwrap_or_else(|e| {
            panic!("`start_local_backend` 里没有恰好一处 `daemon_binary(`（{e}）—— 取用点变了就来改本条")
        });
        let head = &body[..take];
        assert!(
            head.contains("target_os = \"linux\""),
            "本机取内嵌二进制之前**没有问 OS**。\n\
             内嵌的是 musl **Linux** 二进制，而 `daemon_binary` 只按 arch 分派 ⇒\n\
             Windows/macOS 上会释放一个跑不起来的 ELF，然后 `Resolved::Found` 让 UI 报「已起」。\n\
             ★ 那是一句谎报：它只证明文件落地了，不证明那是本平台能跑的东西。"
        );
    }

    /// P2s（`C8`①）：**已经在跑就不重复起** —— 钉的是**机制**，不是那句诊断文案。
    ///
    /// # 这条判据被补审判过一次死刑
    ///
    /// 原版全部内容是「`find_pinned(prod, "本机后端已经在跑…")` + 位置在函数头之后」。
    /// 审计逐字：「它不验 `return`、不验条件、不验「不 spawn」、甚至不验那句话在
    /// `start_local_backend` 体内」。骗过它的改法（幂等当场失效而判据全绿）：
    ///
    /// ```text
    /// if h.current_pid().is_some() {
    ///     tracing::warn!("本机后端已经在跑（C8①：每台机只许一个）");   // 不 return
    /// }
    /// ```
    ///
    /// # 现在钉三件（都在切出来的函数体内）
    ///
    /// ① 判据是 **`g.is_some()`**，不是 `current_pid()` —— 后者有竞态：
    ///    `pid` 由 supervise 线程在 spawn 后才写，本函数返回时它还是 0，门形同虚设。
    /// ② 体内**只许出现一次** `LOCAL_BACKEND.lock()` —— 两次就意味着锁被放开过，
    ///    「检查」与「存句柄」之间又有窗口。
    /// ③ 那一支必须 **`return`** —— 只打日志不返回等于没有门。
    #[test]
    fn starting_twice_does_not_spawn_a_second_local_daemon() {
        let prod = guard_core::production_code(include_str!("local_daemon.rs"));
        let at = guard_core::find_pinned(&prod, "pub fn start_local_backend(")
            .expect("入口不在了 —— 改了名就来改本条");
        let body: String = prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.len() > 200,
            "切出来的函数体只有 {} 字节 —— 切错了，本条在空转",
            body.len()
        );

        guard_core::find_pinned(&body, "if g.is_some() {").unwrap_or_else(|e| {
            panic!(
                "`start_local_backend` 的幂等判据不是 `if g.is_some()`（{e}）。\n\
                 ⚠ 若换回了 `current_pid()`：那是**竞态**判据 —— `pid` 由 supervise 线程在 spawn\n\
                 之后才写，本函数返回时仍是 0 ⇒ 串行点两下就能起出第二个 daemon，\n\
                 而第一个句柄被覆盖、**再没有任何代码能 stop 它**。"
            )
        });

        let locks = body.matches("LOCAL_BACKEND.lock()").count();
        assert_eq!(
            locks, 1,
            "体内出现 {locks} 次 `LOCAL_BACKEND.lock()` —— 只许一次。\n\
             多于一次 = 锁在「检查」与「存句柄」之间被放开过，那个窗口正是双起的入口。"
        );

        let guard_at = body.find("if g.is_some() {").expect("上面刚 pin 过");
        let tail = &body[guard_at..];
        let ret = tail.find("return").unwrap_or(usize::MAX);
        let close = tail.find("\n    ").unwrap_or(0);
        assert!(
            ret < tail.len() && ret < close.max(ret + 1) + 400,
            "幂等那一支里找不到 `return` —— 只打日志不返回等于没有门（补审给的就是这个骗法）"
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // `K-P1`：常驻那条路的判据
    // ══════════════════════════════════════════════════════════════════

    /// 按**行**切一个函数体：从 `fn <name>(` 那行起，到第一行**恰好是 `}`** 为止。
    /// （与本文件既有那几条判据同一个切法 —— 别造第二种。）
    /// `min` = 这个体最少该有多少字节。**逐条给**而不是写死一个数：
    /// `is_detached` 只有三行，拿一个统一的地板量它必然假红，而假红的判据最后会被人删掉。
    fn body_of(prod: &str, head: &str, min: usize) -> String {
        let at = guard_core::find_pinned(prod, head)
            .unwrap_or_else(|e| panic!("切不出 `{head}`（{e}）—— 改了名就来改本条"));
        let body: String = prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        // ★ 反空真自检：切不出函数体 ⇒ 红（`KP4` 第四轮买牙的两个价钱之一）。
        assert!(
            body.len() >= min,
            "`{head}` 切出来的体只有 {} 字节（下限 {min}）—— 切错了，调用方那条判据在空转",
            body.len()
        );
        body
    }

    /// 按**花括号配平**切一个块：从 `marker` 之后的第一个 `{` 起，到配平的那个 `}` 止。
    ///
    /// ⚠ **本文件只许有这一份切法**〔08-27 抽出来〕：`the_exit_path_covers_both_ways_…`
    /// 原先内联了一份，`重-2` 那条又要一份 —— 而这个文件自己的注释逐字写着
    /// 「刻意不另造一种 —— 两种切法迟早在同一段代码上给出两个答案」。
    /// `lo` / `hi` 是这一块的字节数上下限，**逐条给**（反空真：切错了就红，别在空串上绿着）。
    ///
    /// # ⚠⚠ `marker` 必须**唯一** —— 它认的是身份，不是位置〔`D2` `重-D2-3`，08-27 补〕
    ///
    /// 本函数第一版只写了 `prod.find(marker)`，**取文本上第一处，一个字都没断言它唯一**。
    /// 而实测 `Adopt::Refused(why) =>` 在 `local_daemon.rs` 的生产段里**命中 2 处**
    /// （`start_detached` 那一臂 · `probe_and_attach_after_spawn` 里那条 `=> Err(why),`）——
    /// 今天靠 `start_detached` 排在前面**恰好**切中了对的那一臂。
    ///
    /// ★ 病不在「今天切错了」，在**它是靠位置对的，不是靠身份对的**：换个函数顺序就**静默换人**，
    /// 而换人之后红出来的诊断是「配平切错了」—— **那是一句假诊断**（切法没错，是标记指到了
    /// 另一个人身上）。本文件下面逐字写着「**假诊断比不红更贵：它把人引到错的地方**」。
    /// ⇒ 唯一性先断言。同文件的姊妹切法 [`body_of`] 走 `guard_core::find_pinned`（整行相等），
    /// 本函数是这个文件里唯一一个靠文本位置定人群的切法，所以这一行由它自己补上。
    fn braced_block<'a>(prod: &'a str, marker: &str, lo: usize, hi: usize) -> &'a str {
        let hits = prod.matches(marker).count();
        assert_eq!(
            hits, 1,
            "`{marker}` 在人群里命中 {hits} 处（该恰好 1 处）——\n\
             ★ 命中 ≥2 处说明这个标记**不唯一**：本条切的是哪一处**取决于文本顺序**，\n\
             此刻它很可能正在**切错人**，而切错之后红出来的诊断会是「配平切错了」——那是假诊断。\n\
             ★ 命中 0 处 = 它搬家或改名了，本条会零命中地绿。\n\
             ⇒ 把 `marker` 加长到能**认出身份**（比如带上 `=> {{`），**别放宽这条断言**。"
        );
        let at = prod
            .find(marker)
            .unwrap_or_else(|| panic!("找不到 `{marker}` —— 它搬家或改名了，本条会零命中地绿"));
        let bytes = prod.as_bytes();
        // ⚠ 从 `marker` **之后**找块起点，不是从它开头找 —— `marker` 自己可能就带着一对
        //   花括号（`StartOutcome::Failed { reason, looked_at }` 那种解构），
        //   从开头找会切到那一对上去（实测：切出 21 字节）。
        // ★ **例外：`marker` 自己以 `{` 结尾时，那个 `{` 就是块起点。**
        //   〔08-27 实测：上面那条唯一性断言逼着把标记加长成 `Adopt::Refused(why) => {`，
        //   而跳过它之后找到的第一个 `{` 落在块体里 `format!("{port} 口上那个 daemon…")`
        //   的 `{port}` 上 ⇒ **切出 6 字节**、红在「配平切错了」—— 又是一句假诊断。〕
        //   加长标记到 `… => {` 是本文件给「同名两臂」消歧的标准手段，所以这一格在这里接住。
        let scan_from = if marker.ends_with('{') {
            at + marker.len() - 1
        } else {
            at + marker.len()
        };
        let open = (scan_from..bytes.len())
            .find(|&i| bytes[i] == b'{')
            .unwrap_or_else(|| panic!("`{marker}` 之后找不到块起点"));
        let mut depth = 0i32;
        let mut end = bytes.len();
        for i in open..bytes.len() {
            if bytes[i] == b'{' {
                depth += 1;
            } else if bytes[i] == b'}' {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
        }
        let body = &prod[open..end];
        assert!(
            body.len() > lo && body.len() < hi,
            "`{marker}` 切出来 {} 字节（该在 {lo}..{hi} 之间）—— 配平切错了，本条会零命中地绿",
            body.len()
        );
        body
    }

    /// ★★ `KPY1`：**脱离的落点不许住 `backend/`，而且那个注入点真的被用上了。**
    ///
    /// # 两半，缺一半都不成立
    ///
    /// ① **`backend/` 那半干净** —— 那条现成的 `the_backend_half_stays_platform_agnostic`
    ///    在门① 里管着禁针（含 `std::os::unix`）。本条只**补它够不到的一格**：
    ///    整棵 `backend/` 里 `process_group(` 零命中。
    ///    ⚠ 那条判据查的是**禁针字面**，有人把脱离藏进一个跨平台包装 crate（或换个名字）
    ///    就零命中地绿 —— 所以下面②必须是**位置性**的，不能只查「文件里有这个词」。
    ///
    /// ② **位置性**（`P2t-Y1` 逐字栽过这一条：「钉『文件里有 `process::id()`』钉不住它用在哪」）：
    ///    · `start_local_backend` 体内**恰好一处** `start_detached(`；
    ///    · 它排在 `start_or_extract(` **之前**（回落是回落，不是主路）；
    ///    · `spawn_detached` 体内真的有 `process_group(0)` 与 stdio 全 null。
    #[test]
    fn the_detach_landing_is_the_host_layer_and_the_injection_is_really_used() {
        // ── ① `backend/` 那半 ──────────────────────────────────────────
        let backend_mod = guard_core::production_code(include_str!("backend/mod.rs"));
        guard_core::find_pinned(&backend_mod, "fn the_backend_half_stays_platform_agnostic")
            .unwrap_or_else(|_| {
                // 它住在 `#[cfg(test)]` 段里，`production_code` 会把它剥掉 ⇒ 换整份找。
                let raw = include_str!("backend/mod.rs");
                assert!(
                    raw.contains("fn the_backend_half_stays_platform_agnostic"),
                    "`backend/mod.rs` 里那条平台无关判据不在了 —— \n\
                     本条①整半就没了依靠，而脱离那几行随时可以搬进 `backend/`。"
                );
                0
            });
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
        let files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"]);
        assert!(
            files.len() >= 5,
            "只扫到 {} 个 backend/ 文件 —— 抽取坏了，本条在空转",
            files.len()
        );
        let hits: Vec<String> = files
            .iter()
            .filter(|(_, s)| guard_core::production_code(s).contains("process_group("))
            .map(|(p, _)| p.display().to_string())
            .collect();
        assert!(
            hits.is_empty(),
            "`backend/` 里出现了 `process_group(`：{hits:?}\n\
             它来自 `std::os::unix::process::CommandExt`，而 `std::os::unix` 在\n\
             `the_backend_half_stays_platform_agnostic` 的禁针里 —— 那一层不认识平台。\n\
             ⇒ 脱离的落点只能是**宿主知识层**（`local_daemon.rs`），\n\
             照 `platform_fs::make_executable` 那个注入先例把「怎么起」喂进去。\n\
             ⚠ 「加一条平台例外」这条路走不通：`PLATFORM_EXCEPTIONS.len() <= 1` 是递减棘轮，今天正好 1 条。"
        );

        // ── ② 位置性 ────────────────────────────────────────────────────
        let me = guard_core::production_code(include_str!("local_daemon.rs"));
        let start = body_of(&me, "pub fn start_local_backend(", 400);
        assert_eq!(
            start.matches("start_detached(").count(),
            1,
            "`start_local_backend` 体内 `start_detached(` 不是恰好一处 —— \
             零处 = 注入点没被用上（常驻整条路死在那里，而所有判据照样绿）；\
             多处 = 有第二条起法。"
        );
        let a = start.find("start_detached(").expect("上面刚数过");
        let b = start
            .find("start_or_extract(")
            .expect("回落那条路不在了 —— 那非 Linux 平台就没有本机后端了");
        assert!(
            a < b,
            "`start_or_extract(`（今天那条）排在 `start_detached(` 前面 —— \
             那样常驻那条路永远走不到，而它「写在那里」这件事看起来一切正常。"
        );

        // ⚠ needle 要**唯一确定那一个事实**：`fn spawn_detached(` 命中 2 处
        //   （`#[cfg(target_os = "linux")]` 那个 + 非 Linux 的诚实降级壳）。
        //   `F19` 逐字栽过同一形：「把 needle 扩到能唯一确定那个事实的大小」。
        //   Linux 那份的参数**不带下划线前缀**（另一份是 `_port`/`_token`）⇒ 用它分辨。
        let spawn = body_of(&me, "    port: u16,\n    token: &str,", 300);
        // ⚠ 后两个针是**常量名**不是那两个串：串本身住在常量声明里，
        //   而它与 daemon 那侧逐字一致由 `the_listen_env_names_are_the_same_string_on_both_sides` 管。
        //   钉「这里用的是那个常量」而不是「这里出现了那个串」，正好挡住「顺手在这里写死一个串」。
        for needle in [
            "process_group(0)",
            "Stdio::null()",
            "LISTEN_PORT_ENV",
            "LISTEN_TOKEN_ENV",
        ] {
            assert!(
                spawn.contains(needle),
                "`spawn_detached` 体内找不到 `{needle}`。\n\
                 三样一起才叫脱离：`process_group(0)`（否则 Ctrl-C 的 SIGINT 打到整个前台进程组）\
                 + stdio 全 null（今天它 153ms 内死掉的**真正原因**就是那对管子）\
                 + 协议改走监听口（管子没了总得有别的说话方式）。"
            );
        }
        assert!(
            spawn.contains("env_remove(\"TMUX\")"),
            "`spawn_detached` 没有清掉 `TMUX` —— tmux 客户端在 `TMUX` 有值时**按它给的 socket 走**，\
             `TMUX_TMPDIR` 完全不起作用。那正是 08-11 打没用户 9 个真实 tmux 会话的机制。"
        );
    }

    /// ★★★ **退出路径要收的是「三条起法」，不是「新那条」——** 而且这一条**量的是行为**。
    ///
    /// # 它治的是一个会骗人的开关
    ///
    /// 「monitor 退出时结束它」这个勾，在 `K-P1` 之前只对**被监护的那条路**生效
    /// （退出钩子读的是 `LOCAL_BACKEND` 里的 `SuperviseHandle`，而脱离那条路根本没有它）；
    /// `D2 阻-5` 又补上了第三条（**中转是另一个进程**）。
    /// ⇒ 用户勾了、退出、它没被结束 —— 而界面上一个字都不会说。
    /// **一个说谎的开关比「做不到但说出来」更坏**，所以这一格不是文案能补的。
    ///
    /// # 🔴🔴🔴 上一版是**文本判据**，而 `D7` 的刀 `T13` 把它打穿了
    ///
    /// 上一版的形态是：`production_code(include_str!("lib.rs"))` → `braced_block(…, "RunEvent::Exit", …)`
    /// → `body.matches("kill_on_exit(").count() == 1` + `body.contains(needle)` **×4** + 两处位置序。
    /// **= 「那个窗口里有没有这几段文本」**，与本件病史里被打穿过四次的那一形逐字同族。
    ///
    /// 刀 `T13`（只动 `lib.rs` 退出臂一处：`if kill {` → `if kill && !kill {`，
    /// **`stop_local_relay()` 那段文本一字不动**；四个锚点 6/20/9/34 逐个与干净树相同）
    /// ⇒ **`1229 passed; 0 failed` + `GATE: OK`，四个数与干净树逐字相同。**
    /// **生产后果**：用户勾了「退出时结束它」、退出，**本机中转还在那儿听着那个口** ——
    /// **正是本件自己往这条判据里加的那颗针逐字说要防的「说谎的开关」。**
    ///
    /// ⚠ **这一条当时不在那张「量文本的判据」全表里** —— 不是它不在写脚印里
    /// （`D2 阻-5` 那颗针就是本件加的），是那张表的尺子只看得见「diff 里出现原语的行」。
    ///
    /// # ⇒ 换成量行为：那两条**自己不会死**的起法进 [`crate::ExitShutdownSinks`] 那条缝
    ///
    /// 判据装一份**会记账的替身**，断言**两支**（`kill` 是这一格的分叉点，两支都买）：
    /// - **勾了** ⇒ 两个收口点**各被调恰好一次**；
    /// - **没勾** ⇒ **一个都没被调**（这一支上一版根本没有 —— 它只数文本在不在，
    ///   而文本在不在与「没勾时会不会误收」无关）。
    ///
    /// 第三格按**函数地址**对拍「生产上插进那条缝的就是那两个口」，不按文本
    ///（形状照 `history::the_production_relay_facts_are_those_two_take_points`）。
    ///
    /// # 🔴🔴 它买不到什么（射程边缘 —— **这一栏是写区拦出来的，不是我不想买**）
    ///
    /// - **被监护那条起法（`LOCAL_BACKEND` 的 `.stop()`）不在这条缝里。**
    ///   写区外的 `backend/control/local_backend.rs::the_exit_path_really_stops_the_local_backend`
    ///   逐条要求退出臂**体内**恰好一处 `.stop()`、恰好一处 `kill_on_exit(`、策略在前、
    ///   中间那个 `if` 判的就是策略绑定名 ⇒ 抽走它那条判据当场红，
    ///   **而那个文件不在本件登记的 27 项写区里**。
    ///   ⚠ **残留的洞**：`if kill && !kill { h.stop(); }` 过得了那条判据
    ///   （它断的是 `between.contains("if kill")`），**今天没有行为判据接住那一形**。
    ///   **重新裁定的落点**：`crate::ExitShutdownSinks` 头注那一栏 + 本轮上报口。
    /// - **`RunEvent::Exit` 那个闭包本身驱动不了** —— 那要真跑一次 tauri app（红线内够不着）。
    ///   ⇒ 臂里那几行由 [`the_exit_arm_hands_the_other_two_ways_to_the_seam`] 的零命中守卫看着。
    ///   **那一行委托本身没有行为级判据，这是这条链上今天最后一跳** —— 登记，不假装钉住了。
    /// - **收口点自己收干净了没有**是它们各自的活（`stop_local_backend` /
    ///   [`stop_local_relay`] 的头注与判据），本条只买「收不收 · 收哪几个」。
    #[test]
    fn the_exit_path_covers_both_ways_of_starting_the_local_backend() {
        use std::cell::Cell;
        thread_local! {
            static DETACHED: Cell<u32> = const { Cell::new(0) };
            static RELAY: Cell<u32> = const { Cell::new(0) };
        }
        fn spy_detached() -> bool {
            DETACHED.with(|c| c.set(c.get() + 1));
            true
        }
        fn spy_relay() -> bool {
            RELAY.with(|c| c.set(c.get() + 1));
            true
        }
        fn counts() -> (u32, u32) {
            (DETACHED.with(Cell::get), RELAY.with(Cell::get))
        }
        // ⚠ 归零写成闭包而不是 `fn`：`structural_scan` 那道闸把测试段里「无参无返回的
        //   `fn 名()`」一律当成**忘了加 `#[test]` 的死判据**（08-11 全树逮到过三条）。
        let zero = || {
            DETACHED.with(|c| c.set(0));
            RELAY.with(|c| c.set(0));
        };

        let spies = crate::ExitShutdownSinks {
            detached: spy_detached,
            relay: spy_relay,
        };

        // ── 支一：**勾了** ⇒ 那两条起法一个不漏 ────────────────────────
        zero();
        crate::shutdown_detached_ways_on_exit(true, spies);
        assert_eq!(
            counts(),
            (1, 1),
            "\n★★ **用户勾了「退出时结束它」，而那两条起法没有被逐个收掉。**\n\
             这正是刀 `T13` 的形状：`if kill {{` → `if kill && !kill {{`，\n\
             `stop_local_relay()` 那段文本一字不动 ⇒ 上一版那条数文本的判据照绿，\n\
             而**中转还在那儿听着那个口**。\n\
             读数是 (常驻, 中转)，期望 (1, 1)。"
        );

        // ── 支二：**没勾** ⇒ 一个都不收（上一版整支缺失）────────────────
        zero();
        crate::shutdown_detached_ways_on_exit(false, spies);
        assert_eq!(
            counts(),
            (0, 0),
            "\n★★ **用户没勾，而退出路上照样动手收了东西。**\n\
             `P2s`（C8②③）逐字：缺省**不杀** —— 被监护的 daemon 是纯 stdio 子进程，\n\
             monitor 一退它自己就死；无条件收掉等于把这个勾变成一个装饰品，\n\
             方向与「说谎的开关」相反、同样是骗人。\n\
             ⚠ 这一支上一版**根本没有**：它只数「窗口里有没有那几段文本」，\n\
             而文本在不在与「没勾时会不会误收」毫无关系。\n\
             读数是 (常驻, 中转)，期望 (0, 0)。"
        );

        // ── ③ 生产上插进这条缝的**就是那两个口**（按函数地址对拍，不按文本）──
        let p = crate::PRODUCTION_EXIT_SHUTDOWN;
        for (got, want, who) in [
            (
                p.detached as usize,
                stop_detached_backend_on_exit as usize,
                "常驻（脱离）那条起法的收口",
            ),
            (
                p.relay as usize,
                stop_relay_on_exit as usize,
                "🔴 中转那条起法的收口（第三个进程）",
            ),
        ] {
            assert_eq!(
                got, want,
                "`PRODUCTION_EXIT_SHUTDOWN` 里「{who}」插的不是那个函数 —— \n\
                 上面两支量的是**替身**，这一格才是「生产上插进去的就是它」。\n\
                 两条合起来才等于「退出时真的会收那两条起法」。"
            );
        }
    }

    /// ★★ 上一条的**射程边缘**：那两条起法在退出臂里只剩**一行委托**，由本条看着。
    ///
    /// # 为什么还需要它
    ///
    /// `RunEvent::Exit` 那个闭包驱动不了（要真跑一次 tauri app），
    /// ⇒ 上一条量到的只有 [`crate::shutdown_detached_ways_on_exit`] 往里那一段。
    /// **谁在那条臂里再就地收一样东西**（或者把委托那一行删掉），上一条一格都不动。
    /// ⇒ 本条钉三件：
    /// ① 那条臂里那行委托**恰好一处**；
    /// ② `stop_local_backend(` / `stop_local_relay(` 在臂里**零命中**
    ///    （有一个就说明有人又把它们搬回臂里就地写了 —— 那正是刀 `T13` 打穿的那一版）；
    /// ③ 全文件里那条缝的**调用点恰好一个**（定义 1 + 调用 1 = 2 处），
    ///    生产常量 `PRODUCTION_EXIT_SHUTDOWN` 同理 —— 第二个调用点意味着第二条退出路径。
    ///
    /// ⚠ **臂里那一处 `.stop()` 与那一处 `kill_on_exit(` 是刻意留着的**，不在禁针里：
    /// 被监护那条起法必须留在臂里（理由与残留的洞见 `crate::ExitShutdownSinks` 头注），
    /// 而写区外那条 `the_exit_path_really_stops_the_local_backend` 正是数它们的。
    ///
    /// ⚠ **它是文本判据，如实登记**：本条量的是「有没有人在这条臂里另起炉灶」，
    /// 不是「退出时真的收了东西」（那是上一条的活）。
    /// **绕过形态**：把收口点包一层别的名字再在臂里调 —— 本条零命中地绿。
    /// ⇒ 那一形今天没实测，也没有第二道闸接住；重新裁定的落点就是这一栏。
    ///
    /// ⚠ 切法与上一版**同一个**（按花括号配平切退出臂的体），刻意不另造一种 ——
    /// 两种切法迟早在同一段代码上给出两个答案。
    #[test]
    fn the_exit_arm_hands_the_other_two_ways_to_the_seam() {
        let prod = guard_core::production_code(include_str!("lib.rs"));
        // 反空真在 `braced_block` 里（切错了就红，别在一个空串上绿着）。
        // ⚠ 找不到 `RunEvent::Exit` 就是整段钩子没了 —— 实测：删掉它，全仓判据一条不红。
        let body = braced_block(&prod, "RunEvent::Exit", 200, 3000);
        assert_eq!(
            body.matches("shutdown_detached_ways_on_exit(").count(),
            1,
            "退出臂里那条委托不是恰好一处 —— 零处 = 常驻与中转两条起法退出时都不收\
             （而上一条判据照绿，因为它量的是那条缝往里那一段）；多处 = 有第二条退出路径"
        );
        for (needle, why) in [
            (
                "stop_local_backend(",
                "常驻那条路的收口应当住 `stop_detached_backend_on_exit`，由那条缝调",
            ),
            (
                "stop_local_relay(",
                "★ `D2 阻-5`：中转那条收口应当住 `stop_relay_on_exit`。\
                 它就地写在臂里的那一版，正是刀 `T13` 打穿的那一版",
            ),
        ] {
            assert!(
                !body.contains(needle),
                "退出臂里出现了 `{needle}` —— 有人又开始在这条臂里就地收东西了。\n\
                 说法：{why}\n\
                 ★ 这两条起法只许经 `shutdown_detached_ways_on_exit(kill, PRODUCTION_EXIT_SHUTDOWN)` 走。\n\
                 实得臂体：{body}"
            );
        }
        assert_eq!(
            prod.matches("shutdown_detached_ways_on_exit(").count(),
            2,
            "`lib.rs` 生产段里 `shutdown_detached_ways_on_exit(` 不是 2 处（定义 1 + 调用点 1）—— \
             多出来的那处是第二条退出路径，它不在上面那条行为判据的射程里"
        );
        assert_eq!(
            prod.matches("PRODUCTION_EXIT_SHUTDOWN").count(),
            2,
            "`lib.rs` 生产段里 `PRODUCTION_EXIT_SHUTDOWN` 不是 2 处（定义 1 + 调用点 1）"
        );
    }

    /// ★★ `KPY5`：**`detached` 的真相源只能是「起它的时候走没走那条路」。**
    ///
    /// 拿 `channel` / `pid` 反推是**假信号** —— `P2d §0a` 翻掉的 `SSH_CONNECTION` 就是这一形：
    /// 「假信号不会报错，它只是**一直说是**」，而在只有正例的测试里永远绿。
    /// ⇒ 本条既钉**接线**（读的是哪一份记录），也给一格**负例**。
    #[test]
    fn detached_reads_the_path_that_was_taken_not_a_guess() {
        let me = guard_core::production_code(include_str!("local_daemon.rs"));
        let is_det = body_of(&me, "pub fn is_detached(", 60);
        assert!(
            is_det.contains("DETACHED"),
            "`is_detached` 不再读那条「真的走过脱离路」的记录 —— 它现在读的是什么？"
        );
        for forbidden in ["client_for", "current_pid", "LOCAL_BACKEND", "channel"] {
            assert!(
                !is_det.contains(forbidden),
                "`is_detached` 体内出现了 `{forbidden}` —— 那是**反推**。\n\
                 反推出来的信号不会报错，它只会一直说是（`SSH_CONNECTION` 那一形）。"
            );
        }
        let dc = guard_core::production_code(include_str!("daemon_control.rs"));
        let status = body_of(&dc, "pub fn daemon_status(origin: String)", 300);
        assert_eq!(
            status.matches("local_daemon::is_detached()").count(),
            1,
            "`daemon_status` 里 `is_detached()` 不是恰好一处 —— 零处 = 那一格没接上（前端永远读到缺席）"
        );
        assert!(
            status.contains("serde_json::Value::Null"),
            "远端那一支不再是 `null` —— 「它脱没脱离」这句话在远端这条路上没有意义，\
             填 `false` 是**编一个读数**，而不是承认不对称。"
        );

        // ── 负例①：**纯函数**的那张真值表（四格全走到）─────────────────
        assert!(
            !detach_wanted(false, None),
            "非 Linux 平台不许走脱离那条路 —— `process_group` 只在那儿有"
        );
        assert!(
            !detach_wanted(true, Some("1")),
            "`{NO_DETACH_ENV}=1` 关不掉脱离 —— 那 `KPY5` 要的负例就永远走不到了"
        );
        assert!(detach_wanted(true, None), "Linux + 没关 ⇒ 该走那条路");
        assert!(
            detach_wanted(true, Some("  ")),
            "空串按「没设」算 —— shell 里 `export {NO_DETACH_ENV}=` 是常态"
        );

        // ── 负例②：**没走那条路 ⇒ `daemon_status` 必须回 `detached: false`** ──
        //    这一格走的是**真命令**，不是读源码。
        let _guard = crate::inbound_client::local_origin_test_lock();
        assert!(
            DETACHED.lock().expect("锁").is_none(),
            "测试开始时 `DETACHED` 就不是空的 —— 前一条判据留了状态，本条读数不可信"
        );
        let st = crate::daemon_control::daemon_status(crate::inbound_client::LOCAL_ORIGIN.into())
            .expect("查状态");
        assert_eq!(
            st.get("detached").and_then(|v| v.as_bool()),
            Some(false),
            "没走过脱离那条路，`detached` 却不是 false —— 那一格在猜"
        );
        let remote =
            crate::daemon_control::daemon_status("某台远端".into()).expect("查远端状态");
        assert!(
            remote.get("detached").is_some_and(|v| v.is_null()),
            "远端的 `detached` 不是 null —— 那是在替一台看不见的机器编读数"
        );
    }

    /// ★★ `KPY7`（monitor 侧那半）：**本件一行都不许放宽已有判据。**
    ///
    /// # 为什么只对**断言那几行**取指纹，而不是整张表
    ///
    /// 「加行是收紧、动断言是放宽」这句话**本身不是机检**。
    /// 若对整张表取 md5 ⇒ **合法加行也会红** ⇒ 下一个人就会把它调松（那是最坏的结局）。
    /// ⇒ 指纹只覆盖那几行**断言**，表体排除在外。
    ///
    /// ⚠ 量法说明（`brief` 第 11 条）：这里**不是**用 `grep` 数加行 ——
    /// 每条锚点都要求**恰好一处**、且是整行相等（`pin_line`），
    /// 比 md5 更好的地方是：它红的时候会告诉你**哪一行变了**。
    #[test]
    fn this_item_loosened_none_of_the_ratchets_it_touched() {
        // `(文件, 那一行, 恰好几处, 它在守什么)`
        //
        // ⚠ **次数逐条给**（`KP4` 第四轮买牙的两个价钱之一）：`write_site_registry.rs` 里
        // 那句默认拒绝**真的有两处** —— 一处管写盘落点、一处管起进程落点，两张表各一条。
        // 写死成 1 会让本条以「多了一处」的形式假红，而假红的判据最后会被人删掉。
        let pins: &[(&str, &str, usize, &str)] = &[
            (
                "backend/mod.rs",
                "PLATFORM_EXCEPTIONS.len() <= 1,",
                1,
                "递减棘轮：平台例外只许少不许多。**本件正是被它堵着**才把脱离放进宿主层的 —— \
                 松掉它，下一个人就能把 `process_group` 直接写进 `backend/`。",
            ),
            (
                "write_site_registry.rs",
                "missing.is_empty(),",
                2,
                "两张表的**默认拒绝**各一条（写盘落点 / 起进程落点）：人群从源码派生，不申报就红。\
                 本件往两张表各加了两行 —— **加行是收紧**，而这两行是「不申报会不会红」本身。",
            ),
            (
                "write_site_registry.rs",
                "found.len() >= 5,",
                1,
                "起进程那张表的**反空真地板**：抽取器坏掉时它先红，而不是让默认拒绝空着绿。",
            ),
            (
                "write_site_registry.rs",
                "found.len() >= 15,",
                1,
                "写盘那张表的**反空真地板**（同上）。",
            ),
        ];
        for (file, line, want, why) in pins {
            let raw: &str = match *file {
                "backend/mod.rs" => include_str!("backend/mod.rs"),
                "write_site_registry.rs" => include_str!("write_site_registry.rs"),
                other => panic!("本表里出现了没接语料的文件：{other}"),
            };
            assert!(raw.len() > 1000, "`{file}` 只读到 {} 字节 —— 语料坏了", raw.len());
            let n = raw.lines().filter(|l| l.trim() == *line).count();
            assert_eq!(
                n, *want,
                "`{file}` 里 `{line}` 出现 {n} 次（应恰好 {want} 次）。\n\
                 说法：{why}\n\
                 ★ 变少 = 那条棘轮被改写或删掉了（**放宽**）；变多 = 结构变了，回来重判。"
            );
        }
    }

    /// ★★ **跨 crate 字面量对拍**：两个 env 名两侧必须逐字一样。
    ///
    /// 漂了**不会报错**：起出来的 daemon 会把它当成「没设」而走 stdio 那条路，
    /// 于是宿主等在一个永远不会有人 bind 的口上，日志里只有一句「连不上」。
    /// 形状抄 `the_local_origin_is_the_same_string_on_both_sides`。
    ///
    /// ⚠ 这条 `include_str!` 是一条**跨半边**（monitor→daemon）的语料边，
    /// 已登记在 `cross_half_edge_registry`（它自己那条判据管着「不许长到生产段」）。
    #[test]
    fn the_listen_env_names_are_the_same_string_on_both_sides() {
        let daemon = include_str!("../../remote-daemon-proto/src/listen.rs");
        for (rust_name, ours) in [
            ("ENV_PORT", LISTEN_PORT_ENV),
            ("ENV_TOKEN", LISTEN_TOKEN_ENV),
            // ⚠ 第三条不是 env 名，但**同一族**：它是宿主判「这次拒绝会不会自己好」的依据。
            //   漂了的后果：「上一个 monitor 刚退、对面还没反应过来」会被当成不可恢复，
            //   于是新 monitor 直接报失败 —— 而它本来只要再等 20 毫秒。
            ("REFUSE_BUSY", REFUSE_BUSY_REASON),
        ] {
            let head = format!("pub const {rust_name}: &str =");
            let line = daemon
                .lines()
                .find(|l| l.trim_start().starts_with(&head))
                .unwrap_or_else(|| panic!("daemon 那份里找不到 `{head}` —— 名字改了就来改这条"));
            let lit = line
                .split('"')
                .nth(1)
                .unwrap_or_else(|| panic!("`{rust_name}` 那一行不是 `= \"…\";` 的形状"));
            assert_eq!(
                lit, ours,
                "监听口的 env 名两侧漂了（`{rust_name}`）：daemon {lit:?} / monitor {ours:?}\n\
                 ⚠ 这种漂**不会报错** —— daemon 会当成「没设」走 stdio 那条路，\n\
                 而宿主等在一个永远没人 bind 的口上。"
            );
        }
    }

    /// ★★ **两份「找那个二进制」的实现必须同序**。
    ///
    /// 常驻这条路不要监护那半，所以它没法直接用 `start_or_extract`
    /// （那一份把「找」与「监护」焊在一起）⇒ 今天是**两份实现**。
    /// 两份就会漂，而漂的后果是「同一台机上两条路找到不同的二进制」——
    /// 那正是「每台机 N 个 daemon」的另一个入口。
    /// ⇒ 逐字对拍**顺序**这一件事：两边都必须**先 `resolve_beside_this_exe`、再释放内嵌那份**。
    #[test]
    fn the_two_resolution_paths_still_agree_on_the_order() {
        let mine = body_of(
            &guard_core::production_code(include_str!("local_daemon.rs")),
            "fn resolve_daemon_bin(",
            300,
        );
        let theirs = body_of(
            &guard_core::production_code(include_str!("backend/control/local_backend.rs")),
            "pub fn start_or_extract(",
            300,
        );
        for (who, body) in [("常驻这条", &mine), ("今天那条", &theirs)] {
            let beside = body
                .find("resolve_beside_this_exe(")
                .unwrap_or_else(|| panic!("{who}路里找不到 `resolve_beside_this_exe(`"));
            let extract = body
                .find("extract_embedded_to(")
                .unwrap_or_else(|| panic!("{who}路里找不到 `extract_embedded_to(`"));
            assert!(
                beside < extract,
                "{who}路把「释放内嵌那份」排在「找 exe 旁边」之前 —— 顺序反了。\n\
                 开发构建里 exe 旁边那个是**更新**的，内嵌那份是打包时的快照；\
                 顺序一反，两条路就会在同一台机上找到不同的二进制。"
            );
        }
    }

    /// 监听口是**算出来的**：同一个家目录恒等，不同的家目录基本不撞，且落在动态口段里。
    #[test]
    fn the_listen_port_is_deterministic_and_inside_the_dynamic_range() {
        let a = listen_port_for("/home/someone/.claude");
        assert_eq!(a, listen_port_for("/home/someone/.claude"), "同一个输入两次算出不同的口");
        assert!(
            (PORT_BASE..=u16::MAX).contains(&a),
            "算出来的口 {a} 不在动态/私有口段里 —— 那可能撞上系统服务"
        );
        assert_ne!(
            a,
            listen_port_for("/home/other/.claude"),
            "两个不同的家目录算出同一个口 —— 同机两个用户就会互相撞（撞了会出声拒绝，但没必要）"
        );
        // ★ 反向锚点：**不许用 `DefaultHasher`**（它跨 Rust 版本不保证稳定）。
        //   升级一次 monitor 就换一个口 = 下一次启动去连空口、起第二个 daemon，
        //   而那正是本件要防的那件事。
        let me = guard_core::production_code(include_str!("local_daemon.rs"));
        assert!(
            !me.contains("DefaultHasher"),
            "端口用上了 `DefaultHasher` —— 它的输出**跨 Rust 版本不保证稳定**（标准库自己写的）。\
             升一次版就换一个口 ⇒ 认不出已有实例 ⇒ 每台机 N 个 daemon。"
        );
    }

    /// hello 的三张脸。**「有人占着」不等于「占着它的是我们的」** ——
    /// 这一格就是 `EADDRINUSE` 那条诚实边界的落点。
    #[test]
    fn a_stranger_on_our_port_is_refused_out_loud_not_silently_reused() {
        let ours = format!(
            "{{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"host_arch\":\"x86_64\",\
              \"claude_dir\":\"/h/.claude\",\"capabilities\":[],\"emits\":[],\"commands\":[]}}"
        );
        assert_eq!(hello_verdict(&ours, "b1", "/h/.claude"), HelloVerdict::Ours);
        // ① 版本不对
        match hello_verdict(&ours, "b2", "/h/.claude") {
            HelloVerdict::Stranger(w) => assert!(w.contains("b1") && w.contains("b2"), "{w}"),
            v => panic!("旧版本的 daemon 被当成了我们的：{v:?}"),
        }
        // ② 看的目录不对（同机两个用户撞了口就是这一形）
        match hello_verdict(&ours, "b1", "/other/.claude") {
            HelloVerdict::Stranger(w) => assert!(w.contains("/other/.claude"), "{w}"),
            v => panic!("另一个数据目录的 daemon 被当成了我们的：{v:?}"),
        }
        // ③ 压根不是我们的协议
        assert!(matches!(
            hello_verdict("HTTP/1.1 200 OK", "b1", "/h/.claude"),
            HelloVerdict::Stranger(_)
        ));
        assert!(matches!(
            hello_verdict("", "b1", "/h/.claude"),
            HelloVerdict::Stranger(_)
        ));
    }

    /// 「谁在听那个口」那份记录**是上一个 monitor 写的** —— 它可能残缺。
    /// 残缺一律回 `None`，而 `None` 的处置是**不杀**（fail closed）。
    #[test]
    fn a_half_written_owner_record_is_refused_rather_than_guessed() {
        assert_eq!(
            parse_listen_owner("123\n/opt/ccm/daemon\n"),
            Some((123, std::path::PathBuf::from("/opt/ccm/daemon")))
        );
        assert_eq!(parse_listen_owner("123\n"), None, "只有 pid 没有二进制 ⇒ 核对不了身份 ⇒ 不许杀");
        assert_eq!(parse_listen_owner("123\n\n"), None, "第二行是空的 ⇒ 同上");
        assert_eq!(parse_listen_owner("abc\n/x\n"), None);
        assert_eq!(parse_listen_owner(""), None);
        assert_eq!(parse_listen_owner("0\n/x\n"), None, "pid 0 不是一个进程");
    }

    /// ★★ **起真 daemon 的判据必须 fail-closed 地要一个私有 tmux 隔离。**
    ///
    /// # 它不是纪律，是一次事故的直接产物
    ///
    /// 被起的 daemon 一上来就**无条件**往它连得到的 tmux server 装三条全局 hook、
    /// 固定槽位 `[50]`、**没有关掉它的开关**，载荷里烤着那一个 daemon 的 pid+starttime。
    /// 不隔离就是去改用户真实 tmux server 的状态 —— 08-11 那次同族事故打没了用户 **9 个**真实会话。
    /// ⚠⚠ **08-26 本件实现期间又发生了一次**：我在写这段代码时手工起了一个真 daemon 做冒烟，
    /// 没走 shim ⇒ 它把用户真实 tmux server 的 `[50]` 槽位**整个盖成了我那个进程的 pid**
    /// （当时真有一个 monitor 的 daemon 在跑）。当场按原值恢复了，但**那正是本条要防的东西**。
    ///
    /// # 形状
    ///
    /// `local_backend.rs` 有一条同职的（`every_real_daemon_e2e_demands_a_private_tmux_dir`），
    /// 而它**只扫它自己那个文件** —— 本件的真进程判据住这里，落在它的扫描面之外。
    /// 「守卫范围 ≠ 性质范围」那一族，这里是它的又一形。⇒ 本文件自己补一条。
    ///
    /// # ⚠⚠ 射程订正〔`K-R7-D2`，08-31〕：**本条的人群画在属性上，够不着最危险的那一形**
    ///
    /// 本条的人群逐字是「**带 `#[ignore]` 的**」。而**属性是可以不写的** ——
    /// 08-31 现打，本仓有 **2** 条起真 daemon 的**普通 `#[test]`**
    /// （`the_local_daemon_can_be_stopped_and_started_again` 与
    /// `backend/control/local_backend.rs` 的 `the_local_daemon_really_registers_an_inbound_client`），
    /// **本条与那条姊妹判据谁也够不着它们**。
    /// ⇒ 正题已经搬到本文件下面那条
    /// [`every_test_that_starts_the_real_daemon_demands_a_private_tmux`]：
    /// 它的人群按「**那个二进制哪来的**」派生，**两个文件一起扫**，不看任何属性。
    /// **本条留着**，它今天守的是一格更窄但仍然真的性质（`#[ignore]` 那一族的形状回归），
    /// 别把它读成「起真 daemon 这件事有人守了」—— 守它的是下面那条。
    #[test]
    fn every_ignored_test_here_that_spawns_a_real_daemon_demands_private_tmux() {
        const PRIVATE_TMUX: &str = "CCM_E2E_TMUX_SHIM_BIN";
        let src = include_str!("local_daemon.rs");
        // 按行切成「一个 `#[test]` 到下一个 `#[test]`」的块。
        // ⚠ **不在语料串上做裸 `split`**（`needle_anchor_registry` 把它判为「匹配单位比事实小」那一族）。
        let mut chunks: Vec<String> = Vec::new();
        let mut cur: Vec<&str> = Vec::new();
        for line in src.lines() {
            if line.trim() == concat!("#[te", "st]") {
                if !cur.is_empty() {
                    chunks.push(cur.join("\n"));
                    cur.clear();
                }
                continue;
            }
            cur.push(line);
        }
        chunks.push(cur.join("\n"));
        // 抽取器自检：切不出块就下面全空转。
        assert!(chunks.len() >= 5, "只切出 {} 个测试块 —— 切法坏了", chunks.len());
        let ignored: Vec<&String> = chunks
            .iter()
            .filter(|c| c.lines().any(|l| l.trim() == "#[ignore]"))
            .collect();
        // ⚠ **反向锚点**：本文件今天真的有 `#[ignore]` 的真进程判据。
        //   一条都没有的话，下面那个循环零圈 ⇒ 本条是安慰剂。
        assert!(
            !ignored.is_empty(),
            "本文件里一条 `#[ignore]` 都没有 —— 那本条此刻在空转。\n\
             真进程判据被删了？那 `KPY2`/`KPY3` 就没有兑现证据了。"
        );
        for c in &ignored {
            let name = c
                .lines()
                .find_map(|l| l.trim().strip_prefix("fn "))
                .unwrap_or("<读不出名字>");
            assert!(
                c.contains(PRIVATE_TMUX),
                "`{name}` 是一条 `#[ignore]` 的真进程判据，却没有要 `{PRIVATE_TMUX}`。\n\
                 ★ 被起的 daemon 一上来就往它连得到的 tmux server 装三条**全局** hook\n\
                 （固定槽位 `[50]`，**没有关掉它的开关**）⇒ 不隔离就是去改用户真实 tmux 的状态。\n\
                 ⚠ 这不是理论：08-11 打没过用户 9 个真实会话；08-26 本件实现期间又盖过一次 `[50]`。\n\
                 ⇒ 必须 **fail closed**：拿不到 shim 就 `expect` 炸掉，绝不降级裸跑。"
            );
        }
    }

    /// ★★ **取 shim 那个口自己，在默认门禁里真跑一遍**〔`K-R7` 09-01，`§0q` 裁一 · 出路乙〕。
    ///
    /// # 它买的是「**口自己**不再是形状钉」这一格 —— 只有这一格
    ///
    /// 🔴 **这个标题 09-01（`C` 第六拍）改过：原文写的是「它买的是「② 不再是形状钉」」**
    /// 〔`D5` 阻塞 1〕。**那是把「口」读大成了「②」**：本条钉的是**这个纯函数**，
    /// 而 ② 这条锁还包含「谁进了这个口」（文本钉）与
    /// 「**进了口的人会不会把那声炸接住再降级**」（`D5-M6` 一刀走过去，**今天没人守**）。
    /// ⇒ 逐条登记在正题守卫头注的**诚实边界 8**；**别把本条读成「② 关上了」**。
    ///
    /// `D4` 09-01 实测：原来 ② 靠「同一行上既有 `CCM_E2E_TMUX_SHIM_BIN` 又有 `.expect(`」
    /// 这条**文本形状**判。把取值换成 `var(SHIM).unwrap_or_else(|_| var(<真 PATH>).expect(..))`
    /// ⇒ 三样东西都还在、判据说合规，而 fail-closed **没了**，全量门禁新红 **0**。
    /// ⇒ 现在取 shim 只剩 [`demand_tmux_shim`] 一个口，而**这条测试盯的正是那个口**：
    /// 喂它一个「变量不在」的读数，它**必须炸**。
    ///
    /// # 为什么是纯函数而不是去动环境变量
    ///
    /// `cargo test` 一个进程里跑很多线程，改进程级环境变量会跟别人打架；
    /// 而且 `e2e/local-backend-supervise.sh` 那条路**是设了**这个变量的
    /// ⇒ 「按环境当场试一把」的写法会让读数随跑法翻面。
    /// [`require_tmux_shim`] 收的是 `Result`，所以这一格**与环境无关**，两条路上读数相同。
    ///
    /// # 反空真
    ///
    /// 只断「缺变量会炸」是**半格**：一个恒 `panic!` 的实现也能过。
    /// ⇒ 另加一格「有值时原样交出来」，两格一起才钉住「**恰好是 fail-closed，不是恒炸**」。
    #[test]
    fn the_one_shim_gate_really_fails_closed() {
        // ① 缺变量 ⇒ 必须炸。**这是运行期读数，不是源码文本的形状。**
        let r = std::panic::catch_unwind(|| {
            require_tmux_shim(Err(std::env::VarError::NotPresent), "本条自检")
        });
        assert!(
            r.is_err(),
            "`require_tmux_shim` 拿不到 {TMUX_SHIM_VAR} 却**没有炸** —— 这就是降级裸跑。\n\
             ★ 它一降级，所有走这个口的 e2e 都会沿真 PATH 找到真 tmux，\n\
             daemon 一上来就往用户那台 server 装三条全局 hook（槽位 [50]，没有开关）。\n\
             08-11 打没过用户 9 个真实会话；08-26 / 08-27 / 08-29 各盖过一次。\n\
             ⚠ 这一格是 `D4` 09-01 买回来的：在它之前，② 只由一条**同行文本形状**钉着，\n\
             而那条形状被一刀走过去了（`unwrap_or_else` 里再套一个 `.expect(`）。"
        );
        // ② 反空真：有值时必须**原样**交出来。少了这一格，「恒 panic」也能过上面那一格。
        let got = require_tmux_shim(Ok("/夹具/shim-bin".to_string()), "本条自检");
        assert_eq!(
            got, "/夹具/shim-bin",
            "`require_tmux_shim` 拿到值却没有原样交出来 —— 那么上面那一格是空真：\n\
             一个「永远炸」的实现照样能过，而它会把 e2e 全部打死在起跑线上。"
        );
    }

    /// ★★★ **人群画在「会起真 daemon 的测试」上 —— 不是画在某个属性上**〔`K-R7-D2`，08-31〕。
    ///
    /// # 它为什么必须是新的一条，而不是把上面那条改宽一点
    ///
    /// 上面那条（`every_ignored_test_here_that_spawns_a_real_daemon_demands_private_tmux`）
    /// 的人群是「**带 `#[ignore]` 的**」，`local_backend.rs` 那条
    /// （`every_real_daemon_e2e_demands_a_private_tmux_dir`）的人群是
    /// 「**带 `#[ignore]` 且提到 `CCM_E2E_DAEMON` 的**」。两条**都把人群画在属性上**。
    ///
    /// ⇒ 而**属性是可以不写的，而不写的那一个恰恰最危险**：
    /// `the_local_daemon_can_be_stopped_and_started_again` 与
    /// `the_local_daemon_really_registers_an_inbound_client` 两条**当时是普通 `#[test]`**，
    /// 于是**两条守卫谁也够不着它们**，而它们起的是同一个真 daemon，
    /// 那个 daemon 一上来就**无条件**装三条**全局** tmux hook（槽位 `[50]`，没有开关）。
    /// 后果不是理论：08-11 打没用户 **9 个**真实会话；08-26 / 08-27 / 08-29 各盖过一次 `[50]`。
    ///
    /// ⚠ **上面那一段是过去时，别照它去核今天的盘**〔`D4 §F-2` 09-01 逮到，本拍收掉〕：
    /// 那两条**今天都带 `#[ignore = "K-R7：…"]`** —— 那正是本件自己加上去的。
    /// **结论仍然成立，但理由换了，两个数都现打给出**（分母 = 这两个文件，量于 09-01）：
    /// 姊妹守卫的人群判据是 `l.trim() == "#[ignore]"`（**逐字相等**）⇒ 它认不出 `#[ignore = "…"]`。
    /// 现打：`local_daemon.rs` 裸 `#[ignore]` **2** 处 · `#[ignore = ` **1** 处；
    /// `local_backend.rs` 裸 `#[ignore]` **3** 处 · `#[ignore = ` **2** 处。
    /// ⇒ 「两条姊妹守卫谁也够不着它们」今天靠的是「**属性写法不同**」，
    /// **不再是「没有属性」** —— 而本条按**来历**派生，两种写法都够得着。
    ///
    /// # 人群怎么派生的 —— 按「**那个二进制哪来的**」，不按「怎么标记的」
    ///
    /// 一条测试会不会装全局 hook，取决于**它起的是不是那个真 daemon**。
    /// ⇒ 判准取**二进制的来历**，三条（默认拒绝，出现任何一条就进人群）：
    ///
    /// | 来历 | 长什么样 |
    /// |---|---|
    /// | 内嵌那份 | 源码里出现 `embedded-daemons` |
    /// | e2e 传进来的那份 | 源码里出现 `CCM_E2E_DAEMON` |
    /// | 共享 e2e 沙箱 | 源码里出现 `E2eSandbox::demand` |
    ///
    /// **两个文件一起扫**：`local_daemon.rs` 与 `backend/control/local_backend.rs`。
    /// 「守卫范围 ≠ 性质范围」在本仓已经出过七形，上面那两条各是其中一形
    /// （各自**只扫自己那个文件**）。
    ///
    /// # 要求：fail-closed 地要一个私有 tmux **并且真的用上它** —— 两种合法形态
    ///
    /// ① **自己要 + 自己挂**（**三格缺一不可**）：
    ///    ㈠ 本体里走了取 shim 的**唯一入口** `demand_tmux_shim(..)`（**第二道锁**：取到）；
    ///       ⚠⚠ **这一格 09-01 换过一次判法**〔`§0q` 裁一 · 出路乙，`D4` 阻塞 1〕：
    ///       原来判的是「本体里有一行**同时**含 `CCM_E2E_TMUX_SHIM_BIN` 与 `.expect(`」——
    ///       `D4` 把取值换成 `var(SHIM).unwrap_or_else(|_| var(<真 PATH>).expect(..))`
    ///       ⇒ **同一行上三样东西都还在**、判据说合规，而变量缺席时 shim = 真 `PATH`
    ///       ⇒ daemon 沿真 `PATH` 找到真 tmux，**全量门禁新红 0**。那正是 `§1` 里
    ///       08-26 / 08-27 / 08-29 各犯一次的「降级裸跑」。
    ///       ⇒ 今天 ㈠ 只判「**走没走那个口**」；「**口关不关得上**」由
    ///       [`the_one_shim_gate_really_fails_closed`] 在**默认门禁里真跑一遍**
    ///       （纯函数 + `catch_unwind`，与环境无关）—— 那一格是**机器守的，不是文本钉**。
    ///       🔴🔴 **㈠ 买到 / 买不到 —— 这一段 09-01（`C` 第六拍）改过一次，原文是假的**
    ///       〔`D5` 阻塞 1；PM `§0s` 五 ②，**无条件改**〕。**原文逐字**：
    ///       「买到的是「**只要本体真的调到了那个口，缺变量就一定炸**」—— 炸在口里，
    ///        与调用处怎么写无关（`unwrap_or_else(|_| demand_tmux_shim(..))` 照样 fail-closed）」。
    ///       **`D5` 一刀证伪**（实跑，**全量门禁 `GATE: OK`、新红 0**）：把落点写成
    ///       `catch_unwind(|| demand_tmux_shim(..)).unwrap_or_else(|_| var("PATH").unwrap_or_default())`
    ///       —— 闭包**真跑了**、`panic!` **真发生在口里**、调用处套的**恰好就是 `unwrap_or_else`**，
    ///       而变量缺席时 `shim` 就是**真 `PATH`**。
    ///       ⇒ **今天说得准的话**：这一格买到的是「**口自己关得上**」（那是运行期读数，
    ///       由 [`the_one_shim_gate_really_fails_closed`] 真跑）——
    ///       **不是**「走这个口的测试就 fail-closed」。两者之间隔着「**谁能接住那声 panic**」，
    ///       而**那一格今天没有判据在守**，逐条登记在下面**诚实边界 8**。
    ///       ⇒ 这道闸买到的是**防一部分手滑**，**不是**挡住存心绕的人 ——
    ///       ⚠ 「一部分」这个量词是 09-01 收窄的：**多行块注释那一形本身就是手滑，
    ///       而它今天静默地过**（诚实边界 9）。
    ///       **买不到**的还有「这条测试**走不走**那个口」：那仍然是一条
    ///       **文本判据**，与人群判据（来历字面量）同一档 ⇒ 一条根本不进人群的测试
    ///       （诚实边界 5「第四种来历」）照旧够不着。**攻击面从 5 处缩到 1 处，不是缩到 0。**
    ///       ⚠ 还有一格**没钉**：那个口的调用**排在任何 spawn 之前**〔`D4 §F-3` 登记，本拍不加钉〕。
    ///       今天六条实测「取在前、起在后」（相对行 7/4/4/4/2/2 vs 41/–/–/38/41/6，`D4 §D-1`，
    ///       **那个读数我没重打**），而顶着它的是**数据依赖**（`envs` 要用 `shim`，取值挪不到 spawn 之后）
    ///       —— 那是一条**结构性理由，不是判据**。⇒ 逐形登记，不加第二层文本钉
    ///       （`§0q` 裁一逐字：**不许给 ㈠ 再加一层文本钉**）。
    ///    ㈡ 本体里有一行**同时**含 `"PATH"` 与 `"{<绑定名>}:`（**第三道锁**：挂进 `PATH` 最前面）
    ///       —— 注意开头那个**双引号**：插值必须是那个格式串的**串首**，不只是「冒号左边」；
    ///    ㈢ `PATH` 作为 env 键在本体里**恰好被写一次**（**同一张 `envs` 里后写的赢**）。
    /// ② 委托给 `E2eSandbox::demand()` —— 而**转发者自己被本条单独钉住**（见下面第 ③ 段），
    ///    ㈠㈡㈢ 三格都钉，不然「委托」就是一张空头支票。
    ///    ⚠ 委托方**自己本体里 `PATH` 要写 0 次**（㈢ 在这条腿上的取值）：`PATH` 全由
    ///    `demand()` 一处代办（`envs()` 逐字 `vec![self.shim_path.clone()]`），
    ///    委托方再写一条排在它后面就是**把它盖掉**。
    ///
    /// ⚠ **㈡ 与 ㈢ 是两件事，刻意不合成一格**（`K13`：一个值装了两件事）：
    /// ㈡ 判**那一行长什么样**（写法上 shim 排第几），㈢ 判**本体里写了几次**（有没有被后一条盖掉）。
    /// 两者的**代价也不是一种**：破 ㈡ 要「特意在串首前面塞一段」，破 ㈢ 只要「顺手再加一条环境变量」。
    /// 合成一格之后，诊断就说不出「你差在哪一件」。
    ///
    /// ⚠⚠ **㈡ 是 08-31 `D1` 审计买回来的**〔阻塞 4〕：本条第一版只判 ㈠。
    /// 审计两刀实测 —— 把 `format!("{shim}:{}", PATH)` 换成 `format!("{}:{shim}", PATH)`
    /// （shim 从最前挪到最后）、以及把那一格整个换成 `("CCM_AUDIT_UNUSED", shim)`
    /// （第三道锁整个退掉）—— **全量门禁新红都是 0**。
    /// 「取到了一个变量」与「用上了它」是两件事，而本条当时只判前一件。
    ///
    /// # ⚠ 诚实边界（逐形登记，不假装覆盖）
    ///
    /// 1. **人群是「起真 daemon」，不是「起任何进程」。** 用**自造脚本**喂 `supervise()`
    ///    的那几条（`a_child_that_floods_stdout_and_exits_is_still_detected_as_dead` 等）
    ///    **刻意不在人群里**：它们起的东西不装 tmux hook。判准是「后果」不是「动作」。
    /// 2. **扫描面是这两个文件。** 别处新写一条起真 daemon 的测试，本条看不见。
    ///    ⚠⚠ **订正〔`D1` 审计 08-31，阻塞 2〕**：本段原来写的是
    ///    「今天真 daemon 的两个来源（内嵌目录 · `CCM_E2E_DAEMON`）**都只在这两个文件里出现**」——
    ///    **后半句是假的**。现打（分母 = 仓内 187 个 `.rs`，去 `target` / `node_modules` /
    ///    `vendor` / `.git` / `dist` / `embedded-daemons`）：
    ///    - `CCM_E2E_DAEMON` 在 **2** 个 `.rs` 里（就是这两个）—— 这半是真的；
    ///    - `E2eSandbox` 在 **2** 个 `.rs` 里（就是这两个）—— 这半也是真的；
    ///    - **`embedded-daemons` 在 8 个 `.rs` 里**：本文件 · `local_backend.rs` ·
    ///      `src-tauri/build.rs`（`Path::new("embedded-daemons")`，**是代码不是散文**）·
    ///      `src-tauri/src/tool_registry.rs`（登记表数据）· `src-tauri/src/sftp.rs` ·
    ///      `src-tauri/src/write_site_registry.rs` · `remote-daemon-proto/src/main.rs` ·
    ///      `remote-daemon-proto/src/build_id_guard.rs`（后四个是文档注释 / 错误文案）。
    ///    ⇒ **结论不变**（那 6 个文件里一条起真 daemon 的测试都没有，逐个看过），
    ///    **坏的是论证的分母** —— 而那句话是本条关于「人群完整性今天够用」的**唯一**正面论证。
    ///    今天它只能说到这里：扫描面是这两个文件，别处**没有守**。
    /// 3. **守卫自己要排除掉。** 判准是「本体里出现 `include_str!` 这个宏」——
    ///    读源码的是守卫，不是运行期测试。⇒ 一条**既起真 daemon 又读源码**的测试会被漏掉。
    ///    ⚠ **订正证据〔`D1` 审计 08-31，`§C-3`〕**：本段原来的证据是「现打：人群 6 条，
    ///    无一读源码」—— 那是**循环的**：人群正是先把读源码的排除掉之后才得到的，
    ///    换任何一棵树都成立，**一格都买不到**。正确的问法是「**被排除掉的那些块里
    ///    有没有真起 daemon 的**」。现打（分母 = 两个文件切出的 57 个块）：被
    ///    `include_str!` 规则排除、且带来历字面量的块 **3** 个 —— 本条本体 · 姊妹守卫
    ///    `every_real_daemon_e2e_demands_a_private_tmux_dir` · `the_listen_token_file_is_pinned_cell_by_cell`
    ///    （最后那条用 `temp_dir()` 造目录调 `ensure_listen_token(&dir)`，**一个进程都不起**）。
    ///    ⇒ 今天没有这一形。
    ///    ⚠ 那个判准串在下面是**拼出来的**（`concat!`），不是写死的字面量 ——
    ///    `cross_half_edge_registry::every_non_literal_include_is_registered_with_a_reason`
    ///    数的是「`include_*!` 后面跟着 `(`」的**出现次数**，注释与字符串里也算
    ///    ⇒ 直接写字面量会让本条把自己变成那张登记表上的两处「解析不出路径的 include」。
    ///    〔08-31 实打：第一版就是这么红的，`left: [("src-tauri/src/local_daemon.rs", 2), …]`。〕
    /// 4. **它证不了 shim 真的挡住了。** 它只证「要了、缺了就炸、写在串首、而且只写了一次」。
    ///    「真的落在私有 server 上」那一格由 `the_local_tmux_frames_really_land_in_the_ledger`
    ///    的**跑前跑后比对**买（那条自己是 `#[ignore]`）。
    ///
    ///    ⚠⚠ **归因订正〔`D2` 复审 09-01，阻塞 1〕—— 本段原来的归因是错的，不是不完整。**
    ///    原文逐字：「㈡ 是**形状钉**：它证『shim 被写在 `PATH` 最前面』这个**写法**，
    ///    不证『解析真的先到 shim』—— 后者是 `PATH` 本身的语义（最前面那个目录赢），
    ///    **是操作系统的性质，不是本仓的代码**」。
    ///    复审造了**两形**，**两形新红都是 0**，而**两形都在「形状」这一层、都是本仓的代码**：
    ///    ① `format!("/usr/bin:{shim}:{}", PATH)` —— 写法上 shim **就排第二**；
    ///    ② 同一张 `envs` 里**再追加一条** `("PATH", …)` —— `supervise_with_stdio` 是
    ///       `for (k, v) in &envs { cmd.env(k, v); }`（`local_backend.rs:336`-`337`），**后写的赢**。
    ///    ⇒ 把缺口推给「操作系统的性质」是**把自己的射程写宽了一格**：
    ///    那两形本仓的判据**本来就看得见**，只是当时没看。
    ///    **两形 09-01 都买回来了**（㈡ 收紧到「插值在串首」· 新增 ㈢「`PATH` 恰好写一次」，
    ///    先量后选的分母与全表见件文件 `§0l-1`），本段不再拿它们当边界。
    ///
    ///    **今天真正的边界，逐字写准**：㈠㈡㈢ 三格判的都是**源码文本的形状**
    ///    ——「某一行长什么样」与「某个东西在本体里出现几次」——
    ///    它们**不追这个值的去向**。
    ///    ⇒ **我量过的这几形逮不到**，逐形登记在下面第 6 条。
    ///    🔴 **这句话 09-01 改过一次，改的正是它的量词**〔`D3` 复审，阻塞 1〕：
    ///    原文逐字是「⇒ **逮不到的是**下面第 6 条那几形」——那是一个**全称**（分母 = 全体），
    ///    而 `D3` **一刀就证伪了**：把前缀挪进 shim 那条绑定本身（现在的 `6g`）⇒ 六格全盖不住。
    ///    `brief` 12 逐字：给不出分母就只能写「**我量过的这几形里没有**」。⇒ 现在这么写。
    ///    「解析真的先到 shim」那句话**留着**，但它现在只是一句**补充说明**：
    ///    ㈡ 钉的是写法，而写法与解析结果之间那一跳靠 `PATH` 的语义 —— 那**不是**本条的缺口，
    ///    本条的缺口是下面第 6 条。
    /// 5. 🔴 **第四种来历逮不到**〔`D1` 审计 08-31 造出反例并实跑，阻塞 3〕。
    ///    形状：一条**普通 `#[test]`**（无 `#[ignore]`、无 `cfg`）、**就在这两个文件里**、
    ///    真调 `local_backend::supervise_with_stdio(..)` 起 daemon，只把二进制来历换成
    ///    **第四种**取法（例如一个新环境变量 `CCM_AUDIT_DAEMON`）⇒ 三条来历字面量一条都不出现
    ///    ⇒ 它不进人群，本条与两条姊妹守卫**全不出声**（审计实跑：`1210 passed; 0 failed`）。
    ///    ⚠ 它也**不会被别的网接住**：`write_site_registry::spawn_sites::SPAWNS` 登记的是
    ///    **生产侧** spawn 落点，而这样一条测试调的正是那个**已登记**的落点
    ///    ⇒ `every_local_spawn_is_declared` 照样绿。
    ///
    ///    **为什么不换成「按调用派生」的判准**（`调了 supervise* / spawn_detached
    ///    且没 fail-closed 地要 shim`）—— 08-31 先量后选（铁律 18），量在同一个扫描面上：
    ///
    ///    | 判准 | 人群 | 真阳 | 假阳 | 真阳率 |
    ///    |---|---|---|---|---|
    ///    | 现判准（来历字面量） | 6 | 6 | 0 | **6/6** |
    ///    | 候选（按调用派生） | 9 | 4 | 5 | **4/9** |
    ///    | 候选（再剔掉「首个 `#[test]` 之前那一块」这个显然 bug 之后的最好情形） | 7 | 4 | 3 | **4/7** |
    ///
    ///    候选那 5 条假阳的来历有两种，**两种都不是能调参数调掉的**：
    ///    ㈠ **2 条根本不是测试** —— 本切法的第 0 块是「首个 `#[test]` 之前的全部内容」，
    ///       也就是**这两个文件的生产代码本体**，而生产代码里当然有 `supervise(` /
    ///       `spawn_detached(`（那正是被调的落点）⇒ 候选判准会把**生产代码**判成两条违例，
    ///       **在一棵干净的树上当场假红**；
    ///    ㈡ **3 条是诚实边界 1 那一族** —— 拿**自造脚本**喂 `supervise()` 的
    ///       （`a_child_that_floods_stdout_and_exits_is_still_detected_as_dead` ·
    ///       `stop_returns_promptly_even_if_the_child_closed_stdout_but_lives_on` ·
    ///       `e2e_a_binary_that_always_dies_is_given_up_on_within_the_cap`）。
    ///       它们起的东西不装 tmux hook，**要求它们要 shim 是一条没有意义的要求**。
    ///    而且候选判准**还净漏 2 条今天已经管住的**：两条 `e2e_*` 委托给 `E2eSandbox`，
    ///    自己不直接调 `supervise*` ⇒ 候选覆盖 4 条，现判准覆盖 6 条。
    ///    ⇒ **换判准是拿 100% 真阳率换 44%、同时丢掉三分之一覆盖**，压不住噪声。
    ///    **选：留现判准，把这一形逐形登记在此**（铁律 14：写进功能件**并**落进被守对象头注）。
    ///    **另一条路的代价**（登记而不换判准）：这一形今天**真的没有机器守着**，
    ///    靠的是代码评审 + 本段这条登记。谁要加第四种取二进制的方式，
    ///    **请把它加进下面 `PROVENANCE` 那张表**，那是本条唯一的入口。
    /// 6. 🔴 **㈡㈢ 判的是文本形状，不追值的去向**〔`D2` 复审 09-01 逼出来的，逐形登记〕。
    ///    ⚠ 这一条**替换**掉上面第 4 条原来那句错归因（那句话把缺口推给了「操作系统的性质」）。
    ///    下面每一形都带**今天现打的处数**（分母 = 本条扫描面那两个文件；量于 09-01）：
    ///
    ///    | # | 逮不到什么 | 为什么 | 今天有几处 |
    ///    |---|---|---|---|
    ///    | 6a | 那张 `envs` 写得完全对，却**根本没被交给起进程那一跳**（换成另一个 vec / `clone()` 被丢掉） | ㈡㈢ 只读源码文本，**不做数据流** | **0**（五个落点逐条读过，都真把那张表传进 `supervise*`） |
    ///    | 6b | 绑定名**被重新绑**：`let shim = var(SHIM).expect(..); … let shim = <别的东西>;` | 绑定名从「要 shim 那一行」现取，**只取第一处**，后面再绑一次它看不见 | **0**（`grep -c 'let shim = '` ⇒ `local_daemon.rs` **2** · `local_backend.rs` **3**，逐处看过，**五处全是那条 `expect` 行**，无一是重绑） |
    ///    | 6c | `PATH` 由本体**调的另一个函数**追加（helper / builder），或写在转发者那 **20 行窗口之外** | ㈢ 数的是**本体**（转发者是 `demand()` 起 20 行）里的处数 | **0**（今天五处全是本体里的字面写法） |
    ///    | 6d | ㈢ 的「写」口径是**正向数写位形状**（`"PATH"` 后面紧跟一个方法调用）⇒ 键名**拼出来**的（`concat!("PA","TH")` / `format!("PAT{}", "H")` 这类）数不到 | 按字面串数，不是按语义数 | **0**（两个文件 `grep -c 'concat!.*PA'` ⇒ **0 / 0**）。⚠ `D3` 的 `M-δ2` 在这一格上实证过：**新红 0 = 假绿，而登记是真的**。⚠⚠ 09-01 口径从「减法」换成「正向数」之后，**这一格的措辞跟着改了，性质没变**（两种口径都数不到非字面量的键名）|
    ///    | 6e | 反方向的**假红**：把 `let old = var("PATH")…;` 提到上一行、写 `format!("{shim}:{old}")` ⇒ 那一行没有 `"PATH"` ⇒ ㈡ 判不合规 | ㈡ 要求 `"PATH"` 与插值**同一行** | **0**（今天五处写法全同）。⚠ 这一形是**看得见的**（红了就会去读诊断），与假绿不是一种代价 |
    ///    | 6f | 委托腿上，第二条 `PATH` 被写进 **`E2eSandbox::envs()` 或别的 helper**，而不是测试本体 | ㈢ 在委托腿上数的是**测试本体**（要 0 次）；helper 不是测试块，不进人群 | **0**（`envs()` 逐字 `vec![self.shim_path.clone()]`，全仓今天只有这一处） |
    ///    | 6g | **shim 那条绑定本身是个复合串**：`let shim = format!("/usr/bin:{}", var(SHIM).expect(..));` ⇒ ㈡ 取到绑定名 `shim`、也看见 `"{shim}:` 紧跟开引号；㈢ 写位仍是 1 ⇒ **㈡㈢ 两格全绿**，而 daemon 实得 `PATH = /usr/bin:<shim>:<真PATH>`，真 `/usr/bin/tmux` 先被解析到 | ㈡ 只看**绑定名**出现在串首，**不看那个名字里装的是什么** | **0**（五个落点的绑定行现打，右手边**全部逐字以 `std::env::var("CCM_E2E_TMUX_SHIM_BIN")` 打头**，无一是拼出来的串；量具 `scratchpad/kr7-c4-pathwrite.py` 末节）|
    ///    | 6h | ㈢ 换成**正向数写位**之后新出现的那一族：写位形状不是「`"PATH"` 后紧跟方法调用」的写法。我枚举出四种 —— ㈠ `(format!("PATH"), v)`（键名是字面量，但被 `format!` **裹了一层**）· ㈡ `("PATH", v)` 这种 `&str` 元组 · ㈢ `cmd.env("PATH", v)` · ㈣ `map.insert("PATH", v)` | 正向数只认那一种形状；**代价方向与减法相反**：减法只多算（假红），正向数只少算（假绿）—— 这四种**旧的减法口径都数得到**，是这次换口径**新掉出来的** | **0**（四种今天一处都没有）。⚠ 分母话，逐种交代：㈡ **`envs: Vec<(String, String)>` 的类型过不了**；㈢㈣ **根本不走 `envs` 这条路**（自己造 `Command` / 别的容器，属 6a 那一族）；**只有 ㈠ 是「类型过得了、也真进 `envs`」的**。🔴 **本格结尾那句话 09-01 被删掉了**〔`D4` 阻塞 2；`§0q` 裁二〕：原文逐字「⇒ **真能进 `envs` 又数不到的，今天是 6d ＋ 本格的 ㈠ 这两格**，不是一格」—— 那是**全称**，而 `M-B` 一刀就找到了第三格（见 `6i`）。**分母是「一个键落进 `envs` 的所有走法」，那个数没人给得出** ⇒ 今天只写「**我枚举的这四种**」 |
    ///    | 6i | **先把键名绑到一个名字上**：`const KEY: &str = "PATH"; … (KEY.to_string(), <真 PATH>)` ⇒ 正向数看的是**字面量**，而 `"PATH"` 后面跟着的是 `;`，一处都数不到 ⇒ 新红 **0**（`D4` `M-B` 实跑）。★ 它**不是 `6d`**（键名是规规矩矩的字面量，没拼），`6a`–`6h` 逐格也盖不住 | 正向数认的是「字面量与转换**相邻**」；**转换确实要发生一次，但它不必发生在那个字面量身上** —— 绑一次名，转换就落在名字上了 | **0**（现打：两个文件里 `const … : &str = "PATH"` 的代码行 **0** 处）。⚠ **按 `§0n` 裁二只登记、不加钉** —— 这一族是构造性的，再加一钉下一轮还有第 10 形 |
    ///
    ///    🔴🔴 **一条要点名的形状：`§0n` 裁一刚要求改掉同族全称，而同一个提交里又新写了两句**
    ///    〔`D4` 09-01 逮到，`§0q` 裁二点名要写进这里〕。
    ///    `§0n` 裁一（08-31→09-01）要的是把「逮不到的**是**这几形」改成「**我量过的**这几形」。
    ///    `C` 第四拍**在诚实边界 4 那一处改了**（现打就在上面第 4 条末尾），
    ///    **却在同一个提交里新写了两句同族全称**：本格原来那句「今天是……这两格」，
    ///    以及 `path_writes` 上方那句「这样的转换**只有四种写法**」。
    ///    ⇒ **「只修一半 / 同族当场复发」** —— `brief` 15 那句「我治的是**这一处**，
    ///    还是**所有同职的地方**」这一拍没兑现，而且**复发就长在治它的那一批字里**。
    ///    ⚠ 这条不是「改完就算」的记账：**下一个在这份文件里写下任何「只有 N 种 / 今天是这 N 格 /
    ///    全部 / 任何」的人，先在同句给出分母；给不出就写「我量过的这几种」。**
    ///    （公道话：`C` 第四拍**自己**在收工前逮到 `6h` 少一格并重跑门禁订正过 —— 那是对的做法；
    ///    但订正之后写下的仍是一句**新的全称**，只是把数从 1 改成了 2。）
    ///
    ///    ⇒ 一句话：**㈡㈢ 买的是「这几行写对了」，不是「这个值真的到了 daemon 手里」。**
    ///    后一格今天由 `the_local_tmux_frames_really_land_in_the_ledger` 的**跑前跑后比对**买
    ///    （见第 4 条），而那条自己是 `#[ignore]`。
    ///
    /// 7. 🔴🔴 **③ 的形状钉到此为止 —— 这是本件的停机条件**〔PM 09-01 裁二，收 `D3`〕。
    ///
    ///    **此后新发现的 ③ 绕过形，一律走登记**（`6i`、`6j`…往下排），**不再加钉。**
    ///    三条理由，都是裁定原文：
    ///
    ///    ① ③ 自己的边界（第 4 条）已经写明它是「**源码文本的形状**」钉、「**不追值的去向**」。
    ///       **用文本匹配钉运行期行为，永远能被换一种写法绕过** —— `D2` 找到 2 形、`D3` 又找到 1 形，
    ///       **这是构造性的，不是没做好**。再加一钉，下一轮还会有第 8 形。
    ///    ② **真正买到安全的是 ①②，不是 ③**。
    ///       🔴🔴 **这半句 09-01 被订正过一次，原文只成立一半**〔`D4` 阻塞 1；`§0q`〕。
    ///       原文逐字：「真正买到安全的是 **①②**」—— 而当时 **② 也只是一条形状钉**
    ///       （同一行上既有 `SHIM` 又有 `.expect(`），`D4` 一刀就走过去了（新红 **0**）。
    ///       ⇒ 写这句话的时候**没有像量 ③ 那样去量 ②**，这是同一个病：
    ///       **写了一句关于判准的话，而没有现打那个判准盘上长什么样。**
    ///       **今天的准确说法（逐格给「靠什么守」，不给全称）**：
    ///       · ① `#[ignore]`（默认 `cargo test` 够不到）—— **机器守的**：`D4` `M-E` 实测，
    ///         退掉一条的 `#[ignore]` ⇒ 门禁当场红（`1210/1/12`），
    ///         `NotPresent` panic 落在本体**第 2 条语句、早于任何 spawn**。
    ///       · ② fail-closed 取变量 —— **机器守的只有「口自己关得上」这一格**
    ///         （`§0q` 裁一 · 出路乙）：取 shim 收敛到 `demand_tmux_shim` 一个口，而
    ///         [`the_one_shim_gate_really_fails_closed`] 在**默认门禁里真跑那个口**。
    ///         🔴🔴 **这一行 09-01（`C` 第六拍）又订正一格 —— 原文写的是「09-01 起也是
    ///         机器守的」，那是把「口关得上」读大成了「② 关得上」**〔`D5` 阻塞 1〕。
    ///         今天准确的三句：
    ///         ㈠ **「口自己关得上」是机器守的**（运行期读数，`D5-M1`/`D5-M9` 各自单断）；
    ///         ㈡ **「谁进了那个口」仍是文本钉**（人群那一档）；
    ///         ㈢ **「进了口的人会不会把那声炸接住再降级」我们这套门禁里没有一条在守** ——
    ///            `D5-M6` 一刀实跑，全量门禁 `GATE: OK`、新红 0。逐条登记在**诚实边界 8**。
    ///         ⇒ ② 今天买到的是**防一部分手滑**，**不是**「② 关上了」
    ///            （量词 09-01 收窄：诚实边界 9 那一形是手滑，且今天静默地过）。
    ///       · ③ shim 挂进 `PATH` —— **形状钉**，本条停机，只登记。
    ///       ③ 只覆盖「**手工 `--ignored` ＋ 变量设上 ＋ shim 不在自己 `PATH` 上**」那一格 ——
    ///       这一格 `C` 第二拍已经如实拆开写在两条被守测试的头注里。
    ///    ③ `6g` 那一形要有人**主动写** `let shim = format!("/usr/bin:{}", …)` —— 那不是手滑，
    ///       是有人在**拆这道锁**。**对「主动拆锁」加更多形状钉，收益递减而假阳递增**
    ///       （我们**已经有过一个假阳了**：`var_os` 那一形，`D3` `M-δ4` 实证）。
    ///       铁律 18 逐字：**假阳会训练人绕过判据，比没有判据更坏。**
    ///
    ///    ⚠⚠ **这条停机只管 ③。** ①（`#[ignore]`：默认 `cargo test` 够不到）与
    ///    ②（fail-closed：缺 `CCM_E2E_TMUX_SHIM_BIN` 当场炸）**若被找到绕过，那是真缺口，照修不误** ——
    ///    停机条件**一个字都盖不到它们**。
    ///    ★ **09-01 兑现过两次，而两次的兑现方式不同 —— 别只读第一次**：
    ///      · 第一次：`D4` 找到 ② 的绕过，`§0q` 裁一**换了做法**（唯一入口 ＋ 一条真跑的
    ///        fail-closed 测试）。⇒ 这条豁免不是纸面上的。
    ///      · 第二次：`D5` 又找到 ② 的绕过（接住那声 panic），PM `§0s` 裁**这条路到此为止**
    ///        ——「钉调用处」买不到这条性质，**不是这一拍没做好**。
    ///        ⇒ 这一次的「照修不误」落成**另立跟进件**（让 daemon 只碰别人显式交给它的那个
    ///        tmux server，要动生产段），**不是再加一钉**。缺口本身**今天仍然开着**，
    ///        逐条登记在下面**诚实边界 8**。
    /// 8. 🔴🔴 **② 今天仍然开着的那条缝：调用方接住那声 panic**〔`D5` 阻塞 1；PM `§0s` 五 ③〕。
    ///
    ///    **形状**（`D5-M6` 实跑，**全量门禁 `GATE: OK`：cargo 1306 · daemon 488 · npm 1480，新红 0**）
    ///    ——逐字（**故意不放进代码块**：它是一份病历，不是给人抄的示范）：
    ///    · `let shim = std::panic::catch_unwind(|| crate::local_daemon::demand_tmux_shim(".."))`
    ///    · `    .unwrap_or_else(|_| std::env::var("PATH").unwrap_or_default());`
    ///    **它为什么全绿，逐格**：㈠ 含那个口 ⇒ 绿 · 姊妹守卫那一行真是 `let ` 绑定 ⇒ 绿 ·
    ///    ㈡ `format!("{shim}:{}", …)` 那一行没动 ⇒ 绿 · ㈢ 写位仍是 1 ⇒ 绿 ·
    ///    口自己一个字没动 ⇒ [`the_one_shim_gate_really_fails_closed`] 绿。
    ///    而变量缺席时 `shim` 就是**真 `PATH`** ⇒ 与 08-26 / 08-27 / 08-29 那三次降级裸跑
    ///    **机制一个字没变**。
    ///
    ///    **⇒ 这道闸买到的是「**防一部分**手滑」，买不到「挡住存心绕」。**
    ///    🔴 **别再用「保证」「买断」「fail-closed 就还在」这类词描述它**（PM `§0s` 五 ③ 逐字）。
    ///    🔴🔴 **也别把「防手滑」写成全称**〔09-01 收窄，`D6` `B3`〕：下面**诚实边界 9**
    ///    那一形（多行块注释 ＋ 桩值）**就是一次手滑**，而它今天**一声不吭地过**。
    ///    ⇒ 说得准的是「**我量过的这几形挡得住**」，分母写在各条边界里。
    ///
    ///    | | 说得准的话 | 依据（没重打的写明） |
    ///    |---|---|---|
    ///    | **买到** | 口里那句 `panic!` 被改成降级 ⇒ 门禁当场红 | `D5-M1` 新红 1、只点那条测试（`C` 第六拍**重打过，逐字相同**） |
    ///    | **买到** | 反空真那半格不是仪式（「恒炸」的实现过不了） | `D5-M9` 新红 1（**这一刀我没重打**） |
    ///    | **买到** | 旧的「自己现取」写法今天被拒 | `D5-M3` / PM 各一趟 ⇒ 新红 2；`C` 第六拍 `M-c6-1` 同形复打 ⇒ 新红 2 |
    ///    | **买到** | `D5` 另试的 3 形全被逮住或本来就会炸 | 分母 = **它试的那 3 形**，不是全体 |
    ///    | **买不到** | 调用方在「口」与「调用点」之间**接住那声 panic** 再降级 | `D5-M6`（上面那一刀） |
    ///
    ///    ⚠ **这条缝不是纯理论的，这一句要写清楚**：`catch_unwind` **本来就是这两个文件里的
    ///    现成写法** —— `local_backend.rs` 的 `local_stdio_consumer_guarded` 一处、
    ///    [`the_one_shim_gate_really_fails_closed`] 自己一处 ⇒ **照抄的成本为零**。
    ///    （另一侧也照抄 `D5` 自己的话，别只引一半：`catch_unwind` 得**有意**去接住那声 panic，
    ///    `D5` **不主张**这一形与「手滑」同样可能。）
    ///
    ///    **为什么不再加一钉**（PM `§0s` 三，两条理由各自独立成立，**不是实现方能自批的**）：
    ///    ① **panic 是可以被接住的** —— 「唯一入口 ＋ 入口 fail-closed」再硬，
    ///       调用方一句 `catch_unwind` 就降级了；**入口做得再好，也守不住调用方**。
    ///    ② **生产合法地需要那条不隔离的路** —— 真实用户就是不带 shim 起 daemon
    ///       ⇒ 那条入口必须存在、而同 crate 的测试调得到它
    ///       ⇒ **在这两个前提下**（生产要留一条不带 shim 的入口 · 测试与生产同 crate、
    ///       因此调得到那条入口），**没有类型能让「带 shim」变成强制的**。
    ///       ⚠ 前提是承重的，别把它读成无条件的：把这几条 e2e 搬进一个**独立的集成测试
    ///       crate**、只暴露带见证的入口，这句话就不成立 —— 那是另一件事的体量（`§0r-1①`）。
    ///    ⇒ 前五拍每一拍都钉一个形状，下一拍就找到另一个**同行为**的形状
    ///      （08-26 · 08-27 · 08-29 · `D4` · `D5`，**五次同一个机制**）。
    ///    🔴 **真正的修法另立跟进件**：让被起的 daemon **只碰别人显式交给它的那个 tmux server**
    ///      —— 那样测试侧写成什么样都伤不到这台机器。它**要动生产段**（本件五拍守着
    ///      「生产段零改动」）⇒ **是排期决定，不放进本件**。
    /// 9. ~~🔴🔴 **今天开着的一条静默假绿：把真调用留成多行块注释**~~
    ///    **★★ 09-04（`K-R9`）：这一条已经关上了 —— 下面整段留作病历，别读成现状。**
    ///
    ///    🔴🔴 **09-04 落定拍就地订正：「关上了」要带射程 —— 关上的是「整份文件」那一档。**
    ///    在 `4eb271f` 上实测，同一形状**原样复现了一次全绿**：剥法在本条里真正跑的单位是
    ///    **`#[test]` 切出来的块**（本条先切块、再对每块调 `guard_core::strip_comment_lines`），
    ///    而看门判据量的单位是**文件** ⇒ 让块注释的 `/*` 与 `*/` **分落两块**
    ///    （整份文件配平、单块不配平，对 rustc 就是一条普通的跨行块注释、照样编译），
    ///    真调用与那行合规的 `PATH` 写位**都躺在注释里**、活着的只有一个桩值，
    ///    于是 **本条判「合规」· 看门判据绿 · 全量 monitor `1278 passed; 0 failed`**。
    ///    ⇒ **别把下面这段读成「这一族已经没有了」** —— 它今天仍然开着，只是换了单位。
    ///    读数与三刀逐字落在 `evidence/K-R9-R2-fallback-watch-scope.md`。
    ///
    ///    关它的**不是**本条加了钉，而是共用剥法 `guard_core::production_code` 补上了块注释
    ///    （`guard-core/src/lib.rs::try_strip_block_comments`）。⇒ 块注释的内容被抹成等长空格，
    ///    `asks_itself` 在它上面**不再为真**，本条落回 `MISS_LOCK2` **当场红**。
    ///
    ///    🔴 **下面那句「换共享原语也关不掉 —— 这一句是实测，不是推断」今天不成立了。**
    ///    它当时是对的：那一刻的共享原语**按行前缀**判，分不开逐字相同的两行。
    ///    `K-R9` 换的不是「尺子的松紧」，是**尺子的种类** —— 从「看这一行长什么样」换成
    ///    **带状态的词法**（串 / 行注释 / 块注释开合与深度），而「块注释的内层行」与
    ///    「真调用那一行」在**状态**上从来就不同，只是在**单行文本**上相同。
    ///    ⇒ 原文那句「结构性地分不开」限定的是**任何只看单行前缀的尺子**，那个限定词是对的；
    ///    错在把它读成了「所有剥法都分不开」。
    ///
    ///    **`M-c7-D` 那把刀 09-04 在本工作树上重打，两个读数都留下**（`K-R9` `D3`，
    ///    语料 = `local_backend.rs::the_local_tmux_frames_really_land_in_the_ledger`）：
    ///    · 旧剥法：monitor `1275 passed / 0 failed` —— **静默假绿，逐字复现**；
    ///    · 新剥法：`every_test_that_starts_the_real_daemon_demands_a_private_tmux` **红**，
    ///      报文逐字点名「走的腿：两条腿都没走（连第二道锁 ② 都没有）」。
    ///
    ///    ⚠ 边界换了个地方，**没有消失**：新剥法带一条**静默兜底**（词法与文本对不上时一个字都不剥），
    ///    兜底触发的那份文件上这个洞会重开。那件事由
    ///    [`tests::no_monitor_file_falls_back_to_leaving_block_comments_in`] 看着
    ///    （现打：105 份 `.rs`，走兜底 0 份）。
    ///
    ///    ★ **09-04 落定拍逐条复核** —— 上面这几行写于上一拍的旧基点，中间合进来 7 个合并
    ///    （`K-R2` `K-R12` `K-R16` `K-R22` `K-P4` `K-R23` `K-R24`）。**逐条现打的结果，
    ///    全部量于 `4eb271f`、本树未铺 `src-tauri/embedded-daemons/`、跑在沙箱里**：
    ///    · 「105 份 / 走兜底 0 份」——**今天仍是真**；daemon 侧「73 份 / 0 份」**同样仍是真**。
    ///      ⚠ 分母没变**不是**「没人动过」，是那 7 个合并动的不是「文件数」这一维。
    ///    · 「新剥法当场红，报文逐字『两条腿都没走』」——**今天复打成立**（同形一刀、块内配平）。
    ///    · 「旧剥法 monitor `1275 passed / 0 failed`」——**历史读数，量于上一拍的旧基点**。
    ///      今天这棵树上 monitor 那个包是 **1287 条**（1277 passed ＋ 10 ignored）
    ///      ⇒ **别拿 1275 当今天的分母**。复打它要把共用剥法退回去，而 guard-core 不在本拍写区。
    ///    · 「一处改，252 个调用点一起受益」——**今天是 256 处 / 81 份**（同一把尺子：
    ///      `production_code(` 排掉 guard-core 本体与纯注释行）。那把尺子先在上一拍的出货尖上
    ///      复现出 252 才拿来量今天，**不然两个数不可比**。
    ///    · 「那件事由 [`tests::no_monitor_file_falls_back_to_leaving_block_comments_in`] 看着」
    ///      ——**半真，射程要收窄**：它看的是**文件**。本条的剥法跑在**块**上
    ///      ⇒ **块级兜底它一个字都看不见**（上面那段订正里的那一刀就是这么全绿过去的）。
    ///    · 下面原文「`guard_core::strip_comment_lines` 同样按行前缀判」，以及它那句
    ///      「只剥整行的那种」的**逐字引用**——**今天两句都不成立了**：它今天第一步就是
    ///      `strip_block_comments`，而被引的那句头注 `K-R9` 当拍改过，**逐字已经不在那儿**。
    ///    · 下面原文「本拍实测读数 …… cargo 1306 · daemon 488 · npm 1480」——**那是另一棵树、
    ///      另一个口径**（`k-r7-c7`，`DEVBOX_NET=host`，树上**铺着** `embedded-daemons`）。
    ///      今天本树（**未铺**）是 cargo **1387** · daemon **538** · npm **1541**。
    ///      **这不是订正，是口径不同 —— 别把两组数并排读。**
    ///
    ///    〔以下为 09-04 之前的原文病历，逐字保留〕
    ///    〔`D6` `B3` 实打 · PM `§0u` 一 独立复现 · `C` 第七拍 `M-c7-D` **在本拍这棵树上重打，
    ///    读数逐字相同**〕。**它不是假阳（假阳红着、看得见），它一声不吭。**
    ///
    ///    **① 形状**（逐字，**故意不放进代码块**：这是病历，不是给人抄的示范）——
    ///    在一条**起真 daemon** 的 e2e（`local_backend.rs` 的
    ///    `e2e_the_supervisor_restarts_a_real_daemon_after_it_is_killed`）里：
    ///    `/*` **独占一行** · 下一行是原来那句 `let shim = crate::local_daemon::demand_tmux_shim("..");`
    ///    · `*/` **独占一行** · 再写一句 `let shim = String::new();`。
    ///    ⇒ **daemon 拿到的 `PATH` 首段是空串，一点 shim 都没有** ——
    ///    它沿真 `PATH` 找到用户那台真 tmux，一上来装三条**全局** hook `[50]`。
    ///    那正是 `§1` 那张表里 08-26 / 08-27 / 08-29 各犯过一次的那件事。
    ///    **逐格为什么全绿**：`asks_itself` 在块注释文本上为真 ⇒ 进「自己要」那条腿 ·
    ///    含口那一行 `trim_start()` 之后**以 `let ` 打头** ⇒ 绑定名照样取成 `shim` ·
    ///    把 `shim` 挂上 `PATH` 串首那一行**一个字没动** ⇒ ㈡ `Front` · ㈢ 写位仍是 1
    ///    ⇒ **判「合规」**；姊妹守卫同理（它读的也是同一把剥法剥出来的那一行）。
    ///    **本拍实测读数**（沙箱 `.claude/devbox/gate <工作树> k-r7-c7`，`DEVBOX_NET=host`，
    ///    树上**铺着** `embedded-daemons`）：**全量门禁 `GATE: OK` · `REAL_EXIT=0`** ——
    ///    cargo **1306** · generated 一致 · daemon **488** · npm **1480**，**新红 0**。
    ///    **两个必要条件**（缺一就红；`D6` 各实打过一趟，本拍复打了后一条的反面 `M-c7-C`）：
    ///    ㈠ 块注释里**必须含 `demand_tmux_shim(` 这个字面**（不含则 `asks_itself` 为假
    ///       ⇒ 走 `MISS_LOCK2` / `LEG_NEITHER` ⇒ 红）；
    ///    ㈡ 含口那一行 `trim_start()` 之后**必须以 `let ` 打头**（即 `/*` 要另起一行）——
    ///       **一行式** `/* … */` 以 `/*` 打头、今天被剥掉 ⇒ 落回真调用那一行 ⇒ 绿得正确。
    ///
    ///    **② 为什么「扫源码文本」这条路关不掉它**：两条守卫的剥法是**按行前缀**判的，
    ///    **不跟踪块注释的开合状态** ⇒ 多行块注释的内层行**只要自己不以 `//` / `*` / `/*` 打头，
    ///    就一个字都不会被剥掉**（⚠ 09-01 收窄一格：原句写的是「内层行一个字都不会被剥掉」，
    ///    那是**全称**，而一条以 `*` 打头的内层行今天恰好剥得掉 —— `brief` 15 收工自查逮到的）。
    ///    🔴 **换共享原语也关不掉** —— 这一句是**实测**，不是推断：
    ///    `guard_core::strip_comment_lines` 同样按行前缀判，它自己的头注
    ///    （`guard-core/src/lib.rs::strip_comment_lines` 头注）逐字写着
    ///    「⚠ **别把这条边界读成「注释都剥干净了」** —— 只剥整行的那种」。
    ///    `D6` `M-D6-9` 打过一趟；**本拍 `M-c7-D` 是在「已经换成共享原语」的这棵树上打的**
    ///    （`B2` 已收口）⇒ 上面那个 `GATE: OK` 本身就是这句话的证据。
    ///    ⇒ **`B2`（收口剥法）与本条是两件事**：前者关掉的是一形**假阳**（`D6-M1`，
    ///    一行式块注释），本条换尺子关不掉 —— **理由不是「试了两把都不行」，是结构性的**：
    ///    内层那一行与真调用那一行**逐字相同**，而按行前缀的尺子只看「这一行长什么样」
    ///    ⇒ 它分不开这两者。**分母写清**：我实测过的是**两把**（本件原来那份私有副本 ·
    ///    `guard_core::strip_comment_lines`），**「所有可能的剥法」这个分母给不出**；
    ///    能确定的是**任何只看单行前缀的尺子**都分不开逐字相同的两行。
    ///    〔★ 09-04 `K-R9` 就地批注 · **09-04 落定拍按 PM 正式的认法重写（`audits/K-R9-PM.md` `§三`）**。
    ///     🔴 **别写成「PM 认错了」那一句** —— 那会把 PM 对的两句一起丢掉。**要分三层看**：
    ///
    ///     **① 下面那段里今天仍然对的两句**（PM 逐字认它们**仍然成立**）：
    ///        · 「**任何只看单行前缀的尺子**都分不开逐字相同的两行」—— 限定词下得准；
    ///        · 「『所有可能的剥法』这个分母给不出」—— 写下那句话时确实没有那个分母。
    ///
    ///     **② 错的只有从它们推出来的那一句**：🔴「**出路不是**把剥法做成块注释开合配对」。
    ///        它**超出了那两句证得出的射程** —— 那两句限定的是**只看单行前缀**的尺子，
    ///        而 `K-R9` 换的不是形状、是**尺子的种类**（带状态的词法：串 / 行注释 / 块注释开合与深度）。
    ///        「块注释的内层行」与「真调用那一行」**在状态上从来就不同**，只在单行文本上相同
    ///        ⇒ 那不是多钉一个形状，是**把那一整类关掉**。
    ///        ★ PM 自己给的定性逐字：**「从一个正确的限定命题，推出了一个超出它射程的否定命题」**。
    ///
    ///     **③ 「本条只登记，不加钉」这一句 —— 对 `K-R7` 那一件仍然成立**（它守着「生产段零改动」）。
    ///        ⇒ 被推翻的是「出路不是……」那一句，**不是**「只登记不加钉」那一句。
    ///
    ///     ⚠ 数的订正：原批注写「一处改，252 个调用点一起受益」，**今天同一把尺子是 256 处 / 81 份**
    ///     （量于 `4eb271f`；那把尺子先在上一拍出货尖上复现出 252，两个数才可比）。
    ///     ⇒ 别拿下面那句去否掉 `K-R9`；也别拿 `K-R9` 去否掉上面 ① 那两句。〕
    ///    🔴 **出路不是「把剥法做成块注释开合配对」** —— 那是重新走进 PM `§0s` 三 关掉的循环
    ///    （每钉一个形状，下一拍就有下一个形状）。**本条只登记，不加钉**〔PM `§0u` 四 逐字〕。
    ///
    ///    **③ 它归哪儿**：归跟进件 **`K-R8`** —— **让被起的 daemon 只碰别人显式交给它的
    ///    那个 tmux server**。那才是让「测试不许改这台机器」这条性质成立的做法：
    ///    那时测试侧写成什么样都伤不到这台机器。它**要动生产段**（本件七拍守着「生产段零改动」）
    ///    ⇒ 是排期决定，不在本件。
    ///
    ///    ⚠⚠ **这一形落在「防手滑」这个天花板以内 —— 比第 8 条那个 `catch_unwind` 更该被点名**
    ///    〔PM `§0u` 二 逐字〕：`catch_unwind` 要人**有意**去接住那声 panic；
    ///    而**多行块注释正是编辑器 / IDE「注释掉这几行」的默认产物** ——
    ///    调试时顺手注掉那一句、换个桩值、忘了改回来，就是这一形。
    ///    PM `§0s` 三 裁定这道闸买到的是「防手滑」，**按那把尺子量，这一格是天花板以内的漏，
    ///    不是天花板以外的**。⇒ **别把它读成一个理论边角。**
    ///
    ///    ⚠ **分母，逐字**：我实打的是**这一形**（`M-c7-D`：整段包进 `/* */` ＋ 桩值）
    ///    与它两个必要条件的反面；**「块注释这一族一共有几种走法」的分母给不出**
    ///    ⇒ 这里只登记**我量过的这几形**，不写全称。
    /// 10. ⚠ **`B2` 把剥法收口成共享原语，带进来一个新的盲点方向**〔`C` 第七拍 09-01〕。
    ///    共享原语连 **`*` 与 `/*` 打头的整行**一起剥，而 Rust 的**解引用行**
    ///    （`*g = Some(..)` / `*DETACHED.lock()… = None;`）正是那个形状
    ///    ⇒ 一条把来历字面量或 `"PATH".to_string()` 写在 `*deref = …` 行上的测试，
    ///    本条会**看不见**（那会是假绿）。
    ///    **今天的处数**（量具 `scratchpad/kr7-c7-starline.py`，量于 09-01，
    ///    被测对象 = 本工作树 `track/k-r7` **本拍出货尖**）：两个文件里 `trim_start()` 后以
    ///    `*` / `/*` 打头的行共 **21** 条（本文件 **17** = 10 条解引用赋值 ＋ 7 条多行字符串里
    ///    以 `**` 打头的续行 · `local_backend.rs` **4** = 2 条解引用赋值 ＋ 2 条**本拍新写的
    ///    报文串里以 `**` 打头的续行**），其中落在本条人群那 6 个块里的 **6** 条
    ///    （三条测试各 2 条 `*XXX.lock()… = …;`）。
    ///    ⚠ **这个数是快照，不是常量**（`brief` 12）：入场尖 `aee8b9f` 上是 **19** 条，
    ///    本拍自己写的两行报文把它抬成 21 —— 谁再往这两个文件里写以 `**` 打头的续行都会变。
    ///    重打就跑量具那一条，或 `awk` 取「`trim_start()` 后以 `*` 或 `/*` 打头、且不以 `//` 打头」的行。
    ///    两种剥法**逐格对照**（人群进出 · 委托 · 要口 · `writes` · ㈡ 的三值）：
    ///    **人群同为 6 条，判决不同 0 处**；姊妹守卫人群 1 条、转发者 20 行窗口同样全同。
    ///    ⇒ **今天代价 0；这里登记的是一个方向，不是一个今天存在的漏。**
    #[test]
    fn every_test_that_starts_the_real_daemon_demands_a_private_tmux() {
        const SHIM: &str = "CCM_E2E_TMUX_SHIM_BIN";
        // ㈠ 判的是「**走没走那个唯一的口**」，不再是「同一行上有没有 `.expect(`」
        // 〔`K-R7` 09-01，`§0q` 裁一 · 出路乙；`D4` 把旧那条同行形状一刀走过去了〕。
        // ★ 口子本身 fail-closed 这件事**不在这里判** —— 它由
        //   `the_one_shim_gate_really_fails_closed` 在默认门禁里**真跑一遍**。
        //   ⇒ 这一条现在只管「谁进了那个口」，那条管「口是不是关得上」。
        const GATE: &str = "demand_tmux_shim(";
        const DELEGATE: &str = "E2eSandbox::demand";
        // 真 daemon 的三种来历。**默认拒绝**：出现任何一条就进人群。
        const PROVENANCE: [&str; 3] = ["embedded-daemons", "CCM_E2E_DAEMON", DELEGATE];
        // 两条合法的腿。**取常量比字面量**：下面的反空真③要按腿数人，
        // 而按字面量数就会在改一个字的时候静默数成 0（那正是「人群缩水」那一族）。
        const LEG_SELF_SERVED: &str = "自己要 + 自己挂";
        const LEG_DELEGATES: &str = "委托给 E2eSandbox::demand()";
        const LEG_NEITHER: &str = "两条腿都没走（连第二道锁 ② 都没有）";
        // 「差在哪一格」——㈡ 与 ㈢ **各占一条**，不许合成一句（`K13`，09-01 `D2` 阻塞 1）。
        const MISS_NONE: &str = "";
        const MISS_LOCK2: &str = "没走取 shim 的那个唯一入口 `demand_tmux_shim(..)`，\
             也没委托给 `E2eSandbox::demand()`（**第二道锁 ②**）。\n      \
             ⚠ 09-01 起本格判的是「**走没走那个口**」，不是「同一行上有没有 `.expect(`」——\
             后者被 `D4` 一刀走过去了（`unwrap_or_else(|_| var(<真 PATH>).expect(..))` \
             同行上三样都在，而 fail-closed 没了）。**口本身关不关得上**由 \
             `the_one_shim_gate_really_fails_closed` 在默认门禁里真跑一遍";
        // ⚠⚠ **下面这两条 09-01 从一条拆成两条**〔`D5` 阻塞 2；PM `§0s` 五 ①〕：
        //   `shim_first_on_path` 原来用**一个 `bool`** 装了「读不出绑定名」与
        //   「读出来了但没挂串首」两件事，而报文只会说后一件。
        //   `D5-M5` 实证：把那条合法调用按 rustfmt 在 `=` 后断行 ⇒ 落的是前一件，
        //   印出来的却是「没把它写在 `PATH` 串首」，**而串首那一行一个字没动**。
        //   （本工作区最贵的那一族：**一个值装了两件事**。）
        const MISS_NO_BINDING: &str = "含取 shim 那个口的那一行**读不出绑定名**\
             （**第三道锁 ③ · ㈡ 取名**）—— 本条要的形状是**一行到底**的 \
             `let <名字> = …demand_tmux_shim(..)`。\n      \
             ⚠ 判决仍然是 fail closed（读不懂就不说「合规」），**变的只是这句话说的是哪一件事**：\
             说的是「**我读不懂那一行**」，不是「你没把它挂在 `PATH` 串首」。\n      \
             ⚠ **一形已知会落到这里的假阳**：按 rustfmt 在 `=` 后断行（`let shim =` 换行再写调用）\
             —— 语义逐字不变而本条会红（`D5-M5`）。今天要的是**单行写法**；\
             这一格没有放宽（放宽要另做一次「先量再选」，不在本拍写区）。\n      \
             ★ **第二形（多行块注释）09-04 起不再落到这里 —— `K-R9` 把它关上了**〔`D6` `B1` 立，\
             `K-R9` 关〕。原文逐字：「把旧写法留成**多行块注释**（`/*` 与 `*/` 各独占一行）\
             —— 剥法是**按行前缀**的，内层那几行**只要自己不以 `//` / `*` / `/*` 打头**\
             就一个字都剥不掉……内层行**以 `let ` 打头**时更糟：**连红都不会红**，\
             那是一次静默的假绿，归跟进件 `K-R8`。」\n      \
             今天：共用剥法 `guard_core::production_code` 带**块注释的开合状态**\
             （`guard-core/src/lib.rs::try_strip_block_comments`），块注释内容被抹成等长空格\
             ⇒ `asks_itself` 在它上面为假 ⇒ 三种内层行写法**一律**落到上面那条 `MISS_LOCK2`，\
             不再有「以 `let ` 打头就静默通过」那一格。\n      \
             🔴 **09-04 落定拍就地订正：上面这句「一律」要带射程。**\
             本条剥的单位是**按 `#[test]` 切出来的块**，不是整份文件 ⇒ 只在\
             **这一块自己块注释配平**时成立。把 `/*` 与 `*/` **分落两块**（整份配平、单块不配平，\
             rustc 眼里就是条普通跨行块注释）⇒ 这一块掉进剥法的兜底、一个字都不剥\
             ⇒ 「以 `let ` 打头就静默通过」那一格**原样回来**，而下面那条看门判据**还是绿的**。\
             实测于 `4eb271f`：本条判「合规」，全量 monitor `1278 passed; 0 failed`。\n      \
             〔`K-R9` `D3` 两个读数：同一把刀（`local_backend.rs::the_local_tmux_frames_really_land_in_the_ledger`）\
             旧剥法 monitor `1275 passed / 0 failed`；新剥法本条当场红。\
             ⚠ 那个 1275 量于上一拍的旧基点，**别当今天的分母**（今天 monitor 那个包是 1287 条）。〕\n      \
             ⚠ 边界**换了地方没有消失**：新剥法词法与某份文件对不上时**一个字都不剥**（兜底），\
             那份文件上这个洞会重开 —— 由 `no_monitor_file_falls_back_to_leaving_block_comments_in` 看着\
             （⚠ 它量的是**文件**这个单位，看不见上面说的**块级**兜底）";
        const MISS_FRONT: &str = "要了 shim、绑定名也读出来了，**却没有一行把它写在给 daemon 的 \
             `PATH` 串首**（**第三道锁 ③ · ㈡ 写法**）—— 要的形状是 \
             `(\"PATH\", format!(\"{shim}:{}\", ..))`，插值**紧跟开引号**；\
             写成 `\"/usr/bin:{shim}:{}\"` 也不算：那样真 tmux 先被解析到。\n      \
             ⚠ 这一条 09-01 起**只**说这一件事 —— 「读不出绑定名」已经拆去 `MISS_NO_BINDING`";
        // ⚠⚠ 下面两条报文 09-01 改过一次〔`D3` `§E-1`〕：原来它们**断言**「你写了 `PATH`」，
        //   而判据看的只是一个**文本形状** ⇒ 对着一次纯读也会这么说，**那是报文在说假话**。
        //   （`M-δ4` 实证：加一行 `var_os("PATH")` 纯读 ⇒ 假红 + 逐字「你在自己本体里也写了」。）
        //   ⇒ 现在只说**我数到了什么形状**、要的是几处，并把本体里提到 `PATH` 的行**原样贴出来**。
        const MISS_TWICE: &str = "shim 那一行写对了，但本体里 `PATH` 这个 env 键的\
             **写位形状不止一处**（**第三道锁 ③ · ㈢ 处数**）—— `supervise_with_stdio` 是 \
             `for (k, v) in &envs { cmd.env(k, v); }`，**后写的赢** ⇒ 后面那条会把 shim 整个盖掉。\n      \
             ⚠ 判的是**形状**不是语义：数的是「`\"PATH\"` 后面紧跟一个方法调用」\
             （`.to_string()` / `.into()` / `.to_owned()`；`String::from(\"PATH\")` 先归一成这个形状）\
             —— 那是 `envs: Vec<(String, String)>` 逼出来的**键位**写法。\
             **纯读**（`var(\"PATH\")` / `var_os(\"PATH\")` / `vars()` 里比键名）**不算**";
        const MISS_DELEGATE_OVERRIDE: &str = "它走的是**委托**腿（`PATH` 由 \
             `E2eSandbox::demand()` 一处代办），而本体里出现了**把 `PATH` 当 env 键写下的形状**\
             （**第三道锁 ③ · ㈢ 处数**）—— 起进程那一跳后写的赢，\
             排在 `sb.envs()` 之后的那条会把 shim 整个盖掉。委托方本体里这个形状应当**0 处**。\n      \
             ⚠ 判的是形状不是语义（同上：`\"PATH\"` 后面紧跟方法调用；纯读不算）";
        let files: [(&str, &str); 2] = [
            ("local_daemon.rs", include_str!("local_daemon.rs")),
            (
                "backend/control/local_backend.rs",
                include_str!("backend/control/local_backend.rs"),
            ),
        ];

        // ── 抽取：按行切成「一个 `#[test]` 到下一个 `#[test]`」的块 ──────────
        // ⚠ **不在语料串上做裸 `split`**（`needle_anchor_registry` 判它「匹配单位比事实小」）。
        let chunks_of = |src: &str| -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            let mut cur: Vec<&str> = Vec::new();
            for line in src.lines() {
                if line.trim() == concat!("#[te", "st]") {
                    if !cur.is_empty() {
                        out.push(cur.join("\n"));
                        cur.clear();
                    }
                    continue;
                }
                cur.push(line);
            }
            out.push(cur.join("\n"));
            out
        };
        // ⚠ 判人群要看**代码**，不能看文档注释 —— 本条的头注里就写着那三条来历字面量。
        //   `local_backend.rs` 那条同族守卫在这上面**自红过一次**（它的注释里写着
        //   `#[ignore]` 与 `CCM_E2E_DAEMON`，第一版把自己算进了人群）。
        //
        // ⚠⚠ **剥注释这一步 09-01（`C` 第七拍）收口成共享原语**〔`D6` `B2`〕。
        //   这里原来是一份**私有副本**（墓碑，原文逐字）：
        //   `let code_only = |c: &str| -> String { c.lines()`
        //   `    .filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n") };`
        //   ⇒ 它只剥「`//` 打头的整行」。现在两处调用点直接调
        //   `guard_core::strip_comment_lines`（`guard-core/src/lib.rs::strip_comment_lines`）。
        //
        //   **为什么换**（两条，各自独立成立）：
        //   ① **仓规**：`structural_scan.rs:425` 那条
        //      `every_comment_stripping_transformer_is_registered`，头注逐字
        //      「剥注释只许有一个权威实现」「想再加一个 ⇒ 先问『共享原语为什么不够』，
        //      答得出来才加进下面这张表」。上面那份私有副本**没问、也没登记**。
        //      ⚠ 它当时没红**不等于它同意**：那条判据的采集面是 `fn … -> String` / `-> Vec<`
        //      （`structural_scan.rs:508`-`:526`），**看不见闭包** —— 那是它自己的盲点。
        //      〔那条判据不在本拍写区，一个字没动，已走上报口。〕
        //   ② **换过去关掉一形真的假阳**：在一个完全合法的落点上方加**一行式**块注释
        //      （`/* 历史写法留档：demand_tmux_shim("旧") */`）⇒ 旧剥法剥不掉它，
        //      下面 ㈡ 的 `find(..GATE..)` 先撞上那一行、`strip_prefix("let ")` 失败
        //      ⇒ 两条守卫一起**假红**（`D6-M1` 实打，新红 2）。
        //
        //   🔴 **换之前先量了，不是照抄**（铁律 18）。量具 `scratchpad/kr7-c7-starline.py`
        //   （照本条与姊妹守卫的切法重写），被测对象 = 本工作树尖 `aee8b9f`，量于 09-01：
        //   `strip_comment_lines` 连 **`*` 与 `/*` 打头的整行**一起剥，而 Rust 的**解引用行**
        //   （`*g = Some(..)`）正是那个形状 ⇒ 换过去会多剥掉一批**代码行**。
        //   现打：两个文件里这样的行共 **19** 条（本文件 17 · `local_backend.rs` 2；
        //   其中 10 条是解引用赋值，7 条是多行字符串里以 `**` 打头的续行），
        //   落在本条人群那 6 个块里的有 **6** 条（三条测试各 2 条 `*XXX.lock()… = …;`）。
        //   两种剥法**逐格对照**（人群进出 · 委托 · 要口 · `writes` · ㈡ 的三值）：
        //   **人群同为 6 条，判决不同 0 处**；姊妹守卫人群 1 条、转发者 20 行窗口同样全同。
        //   ⇒ **今天代价 0**；换进来的**新盲点方向**逐形登记在头注「诚实边界 10」。
        //   ⚠ 它**换空行、不删行** ⇒ 行位不变（下面转发者那 20 行窗口因此仍是原文前 20 行）。

        // ── 第三道锁 ③ 的判据〔`D1` 回修 08-31，阻塞 4〕：**取到 shim ≠ 用上 shim** ──
        //
        // 审计两刀实测：把 `format!("{shim}:{}", PATH)` 换成 `format!("{}:{shim}", PATH)`
        // （shim 从最前挪到最后）、以及把那一格整个换成 `("CCM_AUDIT_UNUSED", shim)`
        // （③ 整个退掉），**全量门禁新红都是 0** —— 因为上面那条「同一行 SHIM + `.expect(`」
        // 只看**取没取到**，不看**挂没挂上**。⇒ 这里补一条。
        //
        // ㈡ 判什么：本体里有一行**同时**含 `"PATH"` 与 `"{<绑定名>}:`（**注意开头那个双引号**）——
        // 插值必须是那个格式串的**串首** = 那个目录挂在 `PATH` **最前面**，而 `PATH` 的语义是
        // **最前面那个目录赢**。挂在后面（`{}:{shim}`）解析先撞到真 tmux ⇒ 隔离没了。
        // 绑定名不写死，从「要 shim 那一行」上现取（`let <名字> = std::env::var(SHIM)…`），
        // 改个变量名不该变成一次假红。
        //
        // ⚠⚠ **那个开头的双引号是 09-01 `D2` 复审买回来的**〔阻塞 1，形 ①〕：
        //   本条第一版只判「`{绑定名}:` 出现在含 `"PATH"` 的某一行上」——
        //   而 `format!("/usr/bin:{shim}:{}", PATH)` 照样满足它（冒号左边有插值），
        //   **写法上 shim 却排第二**，真 `/usr/bin/tmux` 先被解析到。复审实测**新红 0**。
        //   ⇒ 「在冒号左边」买不到「在最前面」，**要钉到串首**。
        //   先量后选（铁律 18，分母 = 走「自己要 + 自己挂」腿的 4 条 + 转发者 1 处 = 5 个落点）：
        //   收紧之后**假阳 0/5**（五处写法全同、全部通过）⇒ 买。全表见件文件 `§0l-1`。
        // ⚠⚠ **一个 `bool` 装了两件事，09-01（`C` 第六拍）拆开**〔`D5` 阻塞 2；PM `§0s` 五 ①〕。
        //
        //   原来它返回 `bool`，而**两件不同的事挤在同一个 `false` 里**：
        //   ① 含口的那一行**读不出绑定名**（不是 `let <名字> = …` 的形状）；
        //   ② 读出来了，但**没有一行把它挂在 `PATH` 串首**。
        //   下游只有一句报文 `MISS_FRONT`（「要了 shim 却没把它写在 `PATH` **串首**」）
        //   ⇒ 落到 ① 的时候，**印出来的是 ② 的说法**。
        //   `D5-M5` 实证：把那条合法调用按 rustfmt 在 `=` 后断行（语义逐字不变）
        //   ⇒ 含口的那一行不再以 `let ` 打头 ⇒ 走的是 ①，
        //   而印出来的是「要了 shim 却没把它写在 `PATH` 串首」——**串首那一行一个字没动**。
        //   ★ 这正是本工作区记着的最贵那一族：**一个值装了两件事**。
        //
        //   ⇒ 现在返回**三值**，两种「不合规」各带各的报文与实得。
        //   🔴 **判决一个字没改**（两种都仍然 fail closed、都仍然算违例）——
        //   改的只是**它说的是哪一件**。这一拍**不许给调用处加任何新判据**〔PM `§0s` 五 ④〕，
        //   拆值不是加钉：人群、通过条件、处数口径逐字照旧。
        enum ShimOnPath {
            /// 读出了绑定名，而且真有一行把它挂在 `PATH` 串首。
            Front,
            /// **读不出绑定名** —— 含口的那一行不是 `let <名字> = …` 的形状（附那一行原文）。
            NoBinding(String),
            /// 读出了绑定名，但没有一行同时含 `"PATH"` 与 `"{<绑定名>}:`（附那个绑定名）。
            NotFront(String),
        }
        let shim_first_on_path = |code: &str| -> ShimOnPath {
            let gate_line = code.lines().find(|l| l.contains(GATE)).map(|l| l.trim());
            let binding = gate_line
                .and_then(|l| l.strip_prefix("let "))
                .and_then(|r| r.split_once('='))
                .map(|(n, _)| n.trim().to_string());
            match binding {
                Some(n) => {
                    // ⚠ 头上那个 `\"` 就是「串首」那一格：`"{shim}:` 而不是 `{shim}:`。
                    let front = format!("\"{{{n}}}:");
                    if code
                        .lines()
                        .any(|l| l.contains("\"PATH\"") && l.contains(&front))
                    {
                        ShimOnPath::Front
                    } else {
                        ShimOnPath::NotFront(n)
                    }
                }
                // 取不出绑定名 ⇒ **判不合规**（fail closed）：读不懂就别说「合规」。
                // 变的只是**说法**：说「我读不懂那一行」，不说「你没挂在串首」。
                None => ShimOnPath::NoBinding(
                    gate_line
                        .unwrap_or("<本体里根本没有那个口 —— 那该由 ㈠ 先拦下>")
                        .to_string(),
                ),
            }
        };

        // ── ㈢ `PATH` 作为 env 键**恰好写一次**〔`D2` 复审 09-01 买回来的，阻塞 1，形 ②〕──
        //
        // 复审第二刀：**原来那一行一个字没动**，只在同一张 `envs` 里**再追加一条**
        // `("PATH".to_string(), std::env::var("PATH").unwrap_or_default())`。
        // `local_backend::supervise_with_stdio` 是 `for (k, v) in &envs { cmd.env(k, v); }`
        // （`local_backend.rs:336`-`337`）⇒ **后写的赢** ⇒ daemon 拿到的 `PATH` 里
        // **一点 shim 都没有**，而 ㈡ 照样说合规（它看见第一条合规的行就够了）。
        // 复审实测**新红 0**。⇒ 光判「有没有一行写对」不够，还要判「**有没有第二行把它盖掉**」。
        //
        // ⚠ 数的是「**写**」不是「**读**」。**怎么数，09-01 换过一次口径，换的理由写在这里。**
        //
        // 〔**旧口径（减法），`D3` 复审 09-01 打回**〕原来写的是
        // `count("PATH") − count(var("PATH"))`。它数的其实是「`PATH` 被**提到**几次，
        // 减掉**我认得的那一种读法**」—— 于是**任何别的读法都被算成一次写**。
        // `D3` 的 `M-δ4` 实跑：往委托腿里加一行**纯读** `std::env::var_os("PATH");`
        // ⇒ **假红**，而且诊断逐字说「你在自己本体里也写了 `PATH`」——**那句话是假的**。
        // ★ 病根不是「漏了 `var_os`」，是**「读」这一族的分母根本给不出**：
        //   再减一个 `var_os` 就是同一个病的第三次，下一个读法照样漏。
        //
        // 〔**新口径（正向数写位）**〕改成**正面数「它落在写位上」这个形状**：
        //   `"PATH"` 后面**紧跟一个方法调用**（中间只许空白），并且先把
        //   `String::from("PATH")` 归一成 `"PATH".to_string()`。
        // ★ **这个形状为什么算「写位」**：`envs` 的类型是 `Vec<(String, String)>`
        //   （`local_backend::supervise_with_stdio` 的签名）⇒ 一个 `&'static str` 字面量
        //   要当键放进去，**必须先走一次 `&str → String` 的转换**，而**我枚举的转换写法有四种**：
        //   `.to_string()` / `.into()` / `.to_owned()`（都是「后面紧跟点」）
        //   ＋ `String::from(..)`（上面那一步归一掉）。
        //
        // 🔴🔴 **这一段 09-01 改过一次，改的是它的量词，而且改的理由是一刀实测**
        //   〔`D4` 复审，阻塞 2；`§0q` 裁二〕。**原文逐字**是
        //   「而这样的转换**只有四种写法**……**「读」是开放的，「写位」是这个类型封死的**」——
        //   **那是一句全称，而且是假的。** `D4` 的 `M-B` 一刀（实跑，全量门禁新红 **0**）：
        //     const KEY: &str = "PATH";              // ← `"PATH"` 后面是 `;`，正向数看不见
        //     (KEY.to_string(), <真 PATH>),          // ← 后写的赢，shim 被整个盖掉
        //   ⇒ **转换确实要发生一次，但它不必发生在那个字面量身上** ——
        //     **先把字面量绑到一个名字上，转换就落在名字上了**。
        //   ⇒ 那句话把「**转换的四种拼法**」错当成了「**字面量到写位的四条路**」，
        //     **分母搞错了一层**：类型封死的是**类型**，不是**「字面量与转换的相邻关系」**。
        //   ⇒ 今天只能这么说：**「读」是开放的；「写位」我枚举到这几种，
        //     而这个枚举的分母（= 一个键最终落进 `envs` 的所有走法）我给不出**
        //     （`brief` 12：给不出分母就只写「我量过的这几种」）。这一形登记为 `6i`。
        //
        // 〔09-01 先量后选（铁律 18）。量具 `scratchpad/kr7-c4-pathwrite.py`，全表在件文件 `§0o-1`。
        //   **三层分母，逐层写明 —— 干净树那一层分不出来，别拿它当依据**：
        //     · **干净树**（分母 = ㈢ 判的 7 个落点）：四个候选**全部假阳 0/7**；
        //       噪声面（分母 = 两个文件切出的 57 个块）四个候选也**逐格相同** `{0:52, 1:5}`。
        //       ⇒ 这一层**分辨不出好坏**，选择依据不在这里。
        //     · **读法探针**（分母 = 我枚举的 **8** 种纯读写法，**不是「所有读法」**）：
        //       把纯读误判成写 —— 减法 **6/8** · 减法再减 `var_os` **4/8** · **正向数 0/8**。
        //       ⇒ **噪声压得住的是正向数**，而且它压住的方式是**不需要那个给不出的分母**。
        //     · **写位探针**（分母 = 我枚举的 8 种写法）：正向数**数不到 4 种**，逐种核过 ——
        //       `("PATH", v)` 这种 `&str` 元组**类型过不了**；`cmd.env("PATH", v)` 与
        //       `map.insert("PATH", v)` **根本不走 `envs` 这条路**（属 6a 那一族）；
        //       **只有 `(format!("PATH"), v)` 是「类型过得了、也真进 `envs`」的** ⇒ 它是这次换口径
        //       **真正新掉出来的那一格**，登记进 `6h`。
        //       ⇒ 合起来：㈢ **我量到的**假绿面是 `6d`（键名拼出来）＋ `6h` 的那一格
        //         ＋ `6i`（先把字面量绑到名字上）。
        //       🔴 **这一句 09-01 改过一次**〔`D4` 阻塞 2〕：原文逐字是
        //         「㈢ **今天的**假绿面 = 6d ＋ 6h 的那一格，**两格，不是一格**」——
        //         那是一句**全称**（分母 = 全体），而 `D4` 的 `M-B` 一刀就证伪了（见上面那段）。
        //         ⇒ 现在写的是「**我量到的**这几格」，**分母 = 我枚举过的写位探针 + 复审打过的刀**，
        //         **不是**「所有写法」——那个分母没人给得出。〕
        //
        // 🔴 **代价说清楚：这一换把出错的方向翻了个个儿。**
        //   减法那一版 PM 09-01 验算过：`path_writes = W + R₂ ≥ W` ⇒ **只会多算、永不少算**
        //   （错在**假红**这一侧，看得见）。正向数反过来：**只会少算、不会多算**
        //   （错在**假绿**这一侧，悄悄过去）。
        //   🔴 **买它的理由 09-01 换过一次，因为原来那条理由是假的**〔`D4` 阻塞 2；`§0q` 裁二〕。
        //   **原文逐字**：「之所以仍然买它：假绿面被上面那个类型**封死到只剩两格**
        //   （6d ＋ 6h 的 `format!` 那一格）」—— `M-B` 一刀证伪（新红 0，见上面 `6i`）。
        //   **今天的理由只能这么写（两条，都不靠那个给不出的分母）**：
        //     ① **假绿面我数不全，假红面我也数不全 —— 但两侧的代价不对称**：
        //        假红会**训练人绕过判据**（铁律 18 逐字：比没有判据更坏），
        //        还会让报文对着一次纯读说假话（`K-R4` 那一族，`D3` `M-δ4` 实证过一次）；
        //        假绿只是「这一格没守到」，**它不会把别的格一起毁掉**。
        //     ② **实测的噪声比**：读法探针上减法 6/8 假阳、正向数 0/8（分母写在上一段里）。
        //   ⚠ **这两条都不含全称**。谁要再写一句「假绿面只剩 N 格」，先给出
        //     「一个键落进 `envs` 的所有走法」这个分母 —— 给不出就别写。
        //   而假红面在减法那边是**开放**的
        //   （读法这一族的分母给不出），且假红会**训练人绕过判据**
        //   （铁律 18 逐字：比没有判据更坏），还会让报文对着一次纯读说假话（`K-R4` 那一族）。
        //
        // ⚠ **两条腿都要数，只是要的数不一样**（`brief` 15：治的是所有同职的地方，不是一处）：
        //   · 走「自己要 + 自己挂」的 ⇒ **恰好 1**（那一条就是它自己写的 shim 那条）；
        //   · 走「委托」的 ⇒ **恰好 0** —— `PATH` 全由 `E2eSandbox::demand()` 一处代办
        //     （`envs()` 逐字 `vec![self.shim_path.clone()]`），委托方**自己再写一条就是覆盖**。
        //     〔09-01 现打：两条委托的本体里写位形状各 **0** 处 ⇒ 这一格假阳 0。〕
        let path_writes = |code: &str| -> usize {
            let norm = code.replace("String::from(\"PATH\")", "\"PATH\".to_string()");
            norm.match_indices("\"PATH\"")
                .filter(|&(i, m)| {
                    norm[i + m.len()..]
                        .chars()
                        .find(|c| !c.is_whitespace())
                        == Some('.')
                })
                .count()
        };
        // 报文只贴**证据**，不替读者下结论〔`D3` 09-01：一条判据的报文说假话，比它红错更坏〕。
        // ⇒ 把本体里**提到** `PATH` 的行原样贴出来，让人自己看哪一行是写、哪一行是读。
        let path_mentions = |code: &str| -> String {
            let hit: Vec<String> = code
                .lines()
                .filter(|l| l.contains("\"PATH\""))
                .map(|l| format!("        {}", l.trim()))
                .collect();
            if hit.is_empty() {
                "        （本体里一行都没提到 \"PATH\"）".to_string()
            } else {
                hit.join("\n")
            }
        };

        // (名字, 合规吗, **走的是哪条腿**, **差在哪一格**)
        // ⚠ 「哪条腿」记的是**它归谁管**，不是「它过没过」——
        //   下面的反空真③要按腿数「有几条被 ㈡ 查着」，把没过的那几条从腿里踢出去
        //   会让「全员违例」自动变成「这条腿没人走」。
        // ⚠⚠ **这句话 09-01 改准了一格，别再读大**〔`D2` 复审 `§F`，`D3` 现打确认原句还在〕：
        //   这一刀买到的**只有一个方向** —— ①「**全员违例**」不会被读成「这条腿没人走」
        //   （它们仍然归 `LEG_SELF_SERVED`，地板照旧数到 4，正题那张违例列表照常印出来）。
        //   ②「**有人改走别的腿 / 掉出人群**」这个方向**照旧会让地板抢先红**：
        //   `D2` 的 `M-N4` 实测 `self_served` 跌到 2 ⇒ 地板先炸，那一趟正题的违例列表**没印出来**
        //   （⚠ 那一刀是 `D2` 打的，**我没重打**，住址 `audits/K-R7-D2.md#§C` / `§F`）。
        //   ⇒ 代价是「**要跑两趟**」，不是「诊断被吃掉」：地板自己的报文里带着**腿名与全员名单**，
        //   而人群缩水那一格另有反空真②逐条点名。**原句写的是「两种病搅在一起就都读不出来了」——
        //   那对 ① 成立、对 ② 不成立，是把这一刀的射程写宽了一格。**
        //   〔没有把地板挪到正题之后：那是**行为改动**，要它自己的死值验，而本轮 PM 写死了射程。〕
        // ⚠⚠ 第 4 格（差在哪）是 09-01 加的〔`D2` 阻塞 1〕：㈡ 与 ㈢ 是**两件事**
        //   （写法 vs 处数），代价也不是一种 ⇒ **诊断必须分得开**，不许合成一格（`K13`）。
        //   它是「合规吗」那个 `bool` 的**说明**，不是它的替身 —— 两个值各装一件事。
        // ⚠ 第 4 格 09-01 从 `&'static str` 换成 `String`〔`D3` `§E-1`〕：报文要带**实得的证据**
        //   （数到几处 · 本体里提到 `PATH` 的那几行），不然读的人只能猜判据看见了什么。
        //   **仍然是派生时定死**，不是打印时按腿回猜 —— 那一条纪律没变。
        let mut population: Vec<(String, bool, &'static str, String)> = Vec::new();
        let mut total_chunks = 0usize;
        for (who, src) in files {
            let chunks = chunks_of(src);
            total_chunks += chunks.len();
            for c in &chunks {
                let code = guard_core::strip_comment_lines(c);
                // 守卫排除：读源码的是守卫（诚实边界 3）。**串拼出来，别写死**（同上）。
                if code.contains(concat!("include_", "str!(")) {
                    continue;
                }
                if !PROVENANCE.iter().any(|p| code.contains(p)) {
                    continue;
                }
                let name = code
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("fn "))
                    .and_then(|r| r.split_once('('))
                    .map(|(n, _)| n.to_string())
                    .unwrap_or_else(|| "<读不出名字>".to_string());
                // 两种合法形态：**自己要 + 自己挂**，或委托给 `E2eSandbox::demand()`。
                let asks_itself = code.contains(GATE);
                let writes = path_writes(&code);
                let delegates = code.contains(DELEGATE);
                let (ok, leg, missing) = if delegates {
                    // 委托 ⇒ `PATH` 归转发者一处代办，本体里再写一条就是把它盖掉。
                    (
                        writes == 0,
                        LEG_DELEGATES,
                        format!(
                            "{MISS_DELEGATE_OVERRIDE}\n      实得：写位形状 {writes} 处（要的是 0 处）。\
                             本体里提到 `PATH` 的行，原样贴在这里，自己核对：\n{}",
                            path_mentions(&code)
                        ),
                    )
                } else if !asks_itself {
                    (false, LEG_NEITHER, MISS_LOCK2.to_string())
                } else {
                    // 走「自己要」这条腿 ⇒ 归 ㈡㈢ 管。
                    // ⚠ ㈡ 今天分**两件事各说各的**〔`D5` 阻塞 2〕：读不出名字 / 读出来了但没挂串首。
                    match shim_first_on_path(&code) {
                        ShimOnPath::NoBinding(l) => (
                            false,
                            LEG_SELF_SERVED,
                            format!(
                                "{MISS_NO_BINDING}\n      \
                                 实得那一行（剥掉「`//` / `*` / `/*` 打头的整行」之后）：{l}"
                            ),
                        ),
                        ShimOnPath::NotFront(n) => (
                            false,
                            LEG_SELF_SERVED,
                            format!(
                                "{MISS_FRONT}\n      实得绑定名：`{n}`。\
                                 本体里提到 `PATH` 的行，原样贴在这里，自己核对：\n{}",
                                path_mentions(&code)
                            ),
                        ),
                        // ㈢：写对的那一行会不会被**后面又一条写位**盖掉。
                        ShimOnPath::Front if writes != 1 => (
                            false,
                            LEG_SELF_SERVED,
                            format!(
                                "{MISS_TWICE}\n      实得：写位形状 {writes} 处（要的是 1 处）。\
                                 本体里提到 `PATH` 的行，原样贴在这里，自己核对：\n{}",
                                path_mentions(&code)
                            ),
                        ),
                        ShimOnPath::Front => (true, LEG_SELF_SERVED, MISS_NONE.to_string()),
                    }
                };
                population.push((format!("{who}::{name}"), ok, leg, missing));
            }
        }

        // ── 反空真①：抽取器真的切出了东西 ────────────────────────────────
        assert!(
            total_chunks >= 40,
            "两个文件一共只切出 {total_chunks} 个测试块 —— 切法坏了，下面整条在空转"
        );
        // ── 反空真②：人群非空，而且**该在里面的那几条真在里面** ───────────
        //    〔判据规范 7：先证「够得到」（地板），再问有没有违例；否则「零违例」是空真。〕
        //    ⚠ 这是**地板不是等号**：新写一条起真 daemon 的测试会自动进人群、自动被要求合规。
        //      地板只挡「已知的那几条**静默掉出人群**」（改名 / 换来历 / 被删）。
        let names: Vec<&str> = population.iter().map(|(n, _, _, _)| n.as_str()).collect();
        for must in [
            "local_daemon.rs::the_local_daemon_can_be_stopped_and_started_again",
            "local_daemon.rs::e2e_a_second_host_adopts_the_running_daemon_instead_of_starting_a_second_one",
            "local_daemon.rs::e2e_a_detached_daemon_that_dies_leaves_no_zombie",
            "backend/control/local_backend.rs::the_local_tmux_frames_really_land_in_the_ledger",
            "backend/control/local_backend.rs::the_local_daemon_really_registers_an_inbound_client",
            "backend/control/local_backend.rs::e2e_the_supervisor_restarts_a_real_daemon_after_it_is_killed",
        ] {
            assert!(
                names.contains(&must),
                "`{must}` 掉出了人群 —— 它起真 daemon 的来历不见了（改名？换写法？）。\n\
                 ★ 人群缩水与「本来就没有违例」在输出上一模一样，只有这条地板认得出来。\n\
                 现打人群：{names:?}"
            );
        }

        // ── 反空真③：「自己要 + 自己挂」那条腿今天真的有人走 ────────────────
        //    〔`D1` 回修 08-31，阻塞 4〕上面那条第三道锁的判据只落在**这条腿**上；
        //    全员都走「委托」的话它一圈都不转，而「零违例」会读起来像「都合规」。
        //    地板取 3（今天实打 4：`the_local_daemon_can_be_stopped_and_started_again` ·
        //    `the_local_tmux_frames_really_land_in_the_ledger` ·
        //    `the_local_daemon_really_registers_an_inbound_client` ·
        //    `e2e_the_supervisor_restarts_a_real_daemon_after_it_is_killed`），
        //    留一格给「某一条改走委托」，掉到 2 就该回来改本条。
        let self_served = population
            .iter()
            .filter(|(_, _, leg, _)| *leg == LEG_SELF_SERVED)
            .count();
        assert!(
            self_served >= 3,
            "走「{LEG_SELF_SERVED}」那条腿的只剩 {self_served} 条 —— \
             第三道锁 ③（shim 真的挂进 daemon 的 `PATH` 最前面）的判据**只落在这条腿上**，\n\
             人少到这个份上它就快成空转了：那时「零违例」与「没人被查」在输出上一模一样。\n\
             现打人群（名字 · 走哪条腿）：{:?}",
            population
                .iter()
                .map(|(n, _, leg, _)| format!("{n} [{leg}]"))
                .collect::<Vec<_>>()
        );

        // ── ③ 转发者自己必须是 fail-closed 的（不然「委托」是空头支票）─────
        let me = include_str!("local_daemon.rs");
        let at = me
            .find(concat!("fn dem", "and() -> Self {"))
            .expect("`E2eSandbox::demand()` 不在了 —— 委托那条腿没了，来改本条");
        // ⚠ 与人群那一侧同一把尺子：**先剥掉整行注释再判**〔09-01 加的；同日收口成共享原语〕。
        //   ㈢ 是**按处数**判的 ⇒ 转发者这一段里随便一句注释提到 `"PATH"` 就会变成一次假红。
        //   剥注释只会让 ㈠㈡ 更严（少几行可看），不会放水。
        //   ⚠ 共享原语把被剥的行**换成空行**（不删行）⇒ 这仍然是 `demand()` 起**原文** 20 行，
        //   行位不变；下面报文贴出来的也是这一份（空行就是被剥掉的注释行）。
        let demand: String = guard_core::strip_comment_lines(
            &me[at..].lines().take(20).collect::<Vec<_>>().join("\n"),
        );
        // ⚠⚠ **必须落在同一行上判**〔08-31 死值验当场逮到的，就在我自己刚写的这一行里〕：
        //   本条第一版写的是 `demand.contains(SHIM) && demand.contains(".expect(")`。
        //   实测把那一行换成 `.unwrap_or_default()` ⇒ **1210 passed / 0 failed，一条都不红** ——
        //   因为 `demand()` 体内**另外两行**（`CCM_E2E_DAEMON` / `CCM_E2E_WORK`）还带着 `.expect(`，
        //   而 `SHIM` 那个**变量名**照样在。三样东西都在，语义却不是那回事。
        //   ★ 这正是本文件下面 `local_backend.rs` 那条退出臂判据头注里逐字骂过的同一形
        //   （`let kill = kill_on_exit(..); if h.current_pid().is_some() { h.stop(); }`）——
        //   **我在治它的这一轮里又犯了一次。**
        assert!(
            demand.contains(GATE),
            "`E2eSandbox::demand()` 不再走取 shim 的那个唯一入口（`{GATE}`）——\n\
             那么所有委托给它的测试都在裸跑，而本条会对着一张空头支票说「合规」。\n\
             ⚠⚠ **判的不再是「同一行上既有 `{SHIM}` 又有 `.expect(`」**〔`K-R7` 09-01 换的〕：\n\
             `D4` 实测把取值换成 `var({SHIM}).unwrap_or_else(|_| var(<真 PATH>).expect(..))`\n\
             ⇒ 同一行上三样东西都还在、旧判据说合规，而 fail-closed 没了，全量门禁新红 **0**。\n\
             ⇒ 现在只判「走没走那个口」，「口关不关得上」由\n\
             `the_one_shim_gate_really_fails_closed` 在默认门禁里**真跑一遍**。\n\
             实得：\n{demand}"
        );
        // ⚠ 转发者的**第三道锁**也要钉〔`D1` 回修 08-31，阻塞 4〕：
        //   委托那条腿上，「挂进 `PATH` 最前面」这件事全由 `demand()` 一处代办
        //   ⇒ 它退掉，两条 `e2e_*` 的隔离一起没，而人群那一侧一声不吭。
        // ⚠ 三值拆开之后这里也跟着说准〔`D5` 阻塞 2〕：**判决不变（两种都不合规），
        //   变的是它说的是哪一件事** —— 原来两件都印「没挂在 `PATH` 最前面」。
        let (fwd_ok, fwd_why) = match shim_first_on_path(&demand) {
            ShimOnPath::Front => (true, String::new()),
            ShimOnPath::NoBinding(l) => (
                false,
                format!(
                    "含取 shim 那个口的那一行**读不出绑定名** —— 要的形状是一行到底的\n\
                     `let <名字> = …{GATE}..)`。⚠ **这句话说的是「我读不懂那一行」**，\n\
                     不是「没挂在串首」：那两件事 09-01 从一个 `bool` 里拆开了\n\
                     （`D5-M5`：rustfmt 在 `=` 后断行会落到这一条，而串首那一行一个字没动）。\n\
                     实得那一行：{l}"
                ),
            ),
            ShimOnPath::NotFront(n) => (
                false,
                format!(
                    "绑定名读出来了（`{n}`），**却没有一行把它挂到给 daemon 的 `PATH` 最前面** ——\n\
                     取到了 shim 却不用它 = 第二道锁做了、第三道锁没做，daemon 照样沿真 `PATH`\n\
                     找到真 tmux（手工 `cargo test -- --ignored` 那一格）。\n\
                     ⚠ 判的是「同一行上既有 `\"PATH\"` 又有 `\"{{<绑定名>}}:`」——\n\
                     插值要**紧跟开引号**（= 那个格式串的串首）。写成 `\"/usr/bin:{{shim}}:{{}}\"`\n\
                     也不算：那样真 `/usr/bin/tmux` 先被解析到。"
                ),
            ),
        };
        assert!(
            fwd_ok,
            "`E2eSandbox::demand()` 的**第三道锁 ③ · ㈡** 没过：\n{fwd_why}\n\
             实得（`demand()` 起**原文** 20 行，剥掉「`//` / `*` / `/*` 打头的整行」之后；\n\
             被剥的行**换成空行**，所以行位与原文一一对应）：\n{demand}"
        );
        // ⚠ 转发者的 ㈢ 也要钉〔`D2` 回修 09-01，阻塞 1 形 ②〕：
        //   `demand()` 里那条 `shim_path` 写对了，可要是同一段里再写一次 `PATH`，
        //   委托那条腿上的两条 `e2e_*` 一起没隔离，而 ㈡ 照样说合规。
        //   ⚠ 射程：它看的是 `demand()` **起 20 行**这个窗口（`take(20)`）——
        //     写在窗口之外的第二条 `PATH` 它看不见（头注「诚实边界 6c」已登记）。
        assert!(
            path_writes(&demand) == 1,
            "`E2eSandbox::demand()` 里 `PATH` 这个 env 键的**写位形状**数到 {} 处（要的是恰好 1 处）——\n\
             起进程那一跳是 `for (k, v) in &envs {{ cmd.env(k, v); }}`，**后写的赢**\n\
             ⇒ 后面那条会把带 shim 的那条整个盖掉，daemon 拿到的 `PATH` 里一点 shim 都没有。\n\
             ⚠ 判的是**形状**不是语义：数的是「`\"PATH\"` 后面紧跟一个方法调用」\n\
             （`.to_string()` / `.into()` / `.to_owned()`；`String::from(\"PATH\")` 先归一）——\n\
             那是 `envs: Vec<(String, String)>` 逼出来的**键位**写法；**纯读不算**\n\
             （`var(\"PATH\")` / `var_os(\"PATH\")` / `vars()` 里比键名）。\n\
             ⇒ 下面是 `demand()` 起**原文** 20 行、剥掉「`//` / `*` / `/*` 打头的整行」之后的样子\
             （被剥的行换成空行），**自己核对哪一行是写、哪一行是读**：\n{demand}",
            path_writes(&demand)
        );

        // ── 正题 ─────────────────────────────────────────────────────────
        let bad: Vec<String> = population
            .iter()
            .filter(|(_, ok, _, _)| !ok)
            // ⚠ 「差在哪」是**派生时就定好的那一格**，不是在这里按腿回猜〔09-01，`D2` 阻塞 1〕：
            //   ㈡（写法）与 ㈢（处数）都归 `LEG_SELF_SERVED`，按腿回猜就把两件事说成一件。
            .map(|(n, _, leg, missing)| format!("{n}\n      走的腿：{leg}\n      差在：{missing}"))
            .collect();
        assert!(
            bad.is_empty(),
            "这几条会起**真** daemon，却没有 fail-closed 地要 `{SHIM}` **并真的用上它**：\n  {}\n\
             ★ 被起的 daemon 一上来就往它连得到的 tmux server 装三条**全局** hook\n\
             （固定槽位 `[50]`，**没有关掉它的开关**）⇒ 不隔离就是去改用户真实 tmux 的状态。\n\
             ⚠ 这不是理论：08-11 打没用户 9 个真实会话；08-26 / 08-27 / 08-29 各盖过一次 `[50]`。\n\
             ⚠⚠ **只写 `env_remove(\"TMUX\")` 不算** —— `TMUX` 一空，tmux 就回落到默认 socket\n\
             `/tmp/tmux-$UID/default`，那**正是**用户那台 server。隔离要靠**显式选择器**。\n\
             ⚠⚠ **只取到 shim 也不算**（第三道锁）—— 取到一个值却不把它挂进\n\
             daemon 的 `PATH` 最前面，那是一次仪式：daemon 照样沿真 `PATH` 找到真 tmux。\n\
             ⚠⚠ **写对一行也不够**（第三道锁 · ㈢）—— 同一张 `envs` 里再写一条 `PATH`，\n\
             `for (k, v) in &envs {{ cmd.env(k, v); }}` **后写的赢**，shim 那条就被盖掉了。\n\
             ⇒ 两条合法出路：本体里 `let <名字> = {GATE}..)`（**一行到底**）\n\
             **并**写一行 `(\"PATH\", format!(\"{{<名字>}}:{{}}\", ..))`（插值**紧跟开引号**、\n\
             且本体里 `PATH` 这个键**只写这一次**）；或走 `{DELEGATE}()`。\n\
             🔴 **这两行 09-01（`C` 第六拍）改过 —— 原文教的正是今天会被拒的写法**：\n\
             逐字写着「本体里 `env::var(\"{SHIM}\").expect(..)`」，而 ㈠ 从 09-01 起判的是\n\
             「**走没走那个唯一入口**」，自己现取当场红（`D5-M3` 实证，新红 2）。\n\
             ⇒ 那是「**只修一半**」的第三个动词：`C` 第五拍换了 ㈠ 的判法，\n\
             **却没跟着改这句教人怎么写的话**，于是判据与它自己的建议互相矛盾。",
            bad.join("\n  ")
        );
    }

    /// token 每次都不一样、够长，而且**不是空串**（空串会让 attach 那道门形同虚设）。
    #[test]
    fn every_token_is_fresh_and_long_enough() {
        let a = fresh_token();
        let b = fresh_token();
        assert_ne!(a, b, "两次拿到同一个 token —— 那说明熵源里没有「每次都变」的东西");
        assert_eq!(a.len(), 32, "token 长度变了（32 个十六进制 = 128 位）");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// ★★ `重-2`：**自动起那条路的「拒绝」不许只进日志。**
    ///
    /// # 它治的那一格
    ///
    /// 「连上去先读 hello 比对，**对不上就出声并拒绝**」（`§0b-2㈡` / `listen.rs` 诚实边界②）——
    /// 两条起法只有**手动点「起」**那条兑现了「出声」（`daemon_control::daemon_start` 回 `Err`
    /// ⇒ 前端 toast，A6 那次修就是为这个）；**自动起**那条（`lib.rs` 的 `setup`，
    /// 用户每天真正走的那条）回修前是 `tracing::info!` —— 连 `warn` 都不是。
    ///
    /// 触发场景不是理论：口按家目录算死（[`listen_port_for`]），`hello_verdict` 逐字比
    /// `DAEMON_BUILD_ID` ⇒ **升级 monitor 之后上一次脱离留下的那个 daemon 还在听同一个口**
    /// ⇒ `Stranger` ⇒ `Adopt::Refused` ⇒ 本机后端起不来，而界面上什么都不说。
    /// ⚠ 这是**常驻带来的新场景**，是本件引入的：翻面之前 daemon 153ms 就死了。
    /// `K14` 那条精神逐字是「不许只做常驻不做那句话」。
    ///
    /// # 钉四件（少一件这条就只证明了一半）
    ///
    /// ① 分档的依据是那条**只在真的被拒绝时才写下**的记录，不是去 `reason` 串里认字
    ///    （认字就是 `KPY5` 治的那种假信号：它不会报错，只会一直说是）；
    /// ② 拒绝那一支里有 `warn!`（不是 `info!`）；
    /// ③ **把日志行整段拿掉之后，那一支里仍然剩着一个用户看得见的出口** ——
    ///    这一条才是「不许**只**进日志」的正面表述，只查「有没有 warn」是查不出来的；
    /// ④ 那句话**说得出下一步能做什么**（不是只报「起不来」）。
    /// ⑤ **那句话真的进了那个出口** —— 见下面那段。
    ///
    /// # ⑤ 为什么必须单列：「有出口」离「话进了出口」还差一个参数〔`D2` `重-D2-1`，08-27 补〕
    ///
    /// `D2` 实打了这一刀：把 `lib.rs` 那条通知的 `.body(&next_step)` 换成 `.body("")`
    /// ⇒ **1194 passed / 0 failed，一条都不红**，五条判据全绿。
    /// ⇒ 那句由 ④ 逐字钉过（必须含「下一步」、必须含 `pid_path(&dir, port)`）、
    /// 在 `local_daemon` 里花了 8 行去写的话，**可以一个字都不进通知**，
    /// 而用户看到的是一个标题写着「本机后端没起来」、**正文空白**的通知。
    ///
    /// ★ ③ 钉的是「**有没有出口**」（那一臂里有 `.notification()` 和 `.show()`），
    /// **不是「被钉过内容的那个串真的流进了那个出口」** —— 这两件事之间隔着一个参数。
    /// 而这一格**不需要真机就能钉**（一行），所以它不该躺在诚实边界里。
    ///
    /// # 它挡不住什么
    ///
    /// 它认的是 `lib.rs` 那一臂里的字面锚点。有人把通知换成另一条同样可见的通道
    /// （比如 emit 一个事件给前端）⇒ 本条会红，而那**正是要的**：换出口就该有人回来重判。
    /// 反过来，它证不了那条通知**在真机上真的弹出来了** —— 那要真跑一次 GUI，
    /// 本轮没有，**如实登记为未验**。
    /// ⚠ 这一句今天的射程要读准：加上 ⑤ 之后它管到「**那句话进了出口的参数**」为止；
    /// **出口自己有没有把它画到屏幕上**（通知权限被系统关掉、桌面环境不支持…）仍然不管。
    #[test]
    fn the_auto_start_refusal_is_not_only_a_log_line() {
        let lib = guard_core::production_code(include_str!("lib.rs"));
        // 反空真①：自动起那条路本身还在（0 处 = 接线没了，下面切什么都没意义）。
        assert_eq!(
            lib.matches("start_local_backend()").count(),
            1,
            "`lib.rs` 里 `start_local_backend()` 不是恰好一处 —— \
             自动起那条路搬家或没了，本条会零命中地绿"
        );
        let arm = braced_block(&lib, "StartOutcome::Failed { reason, looked_at }", 300, 4000);

        // ① 分档的依据是那条记录，不是认字符串。
        assert!(
            arm.contains("take_start_refusal()"),
            "自动起那条路不再问「这一次是不是**被拒绝**」——\n\
             ★ 别改成去 `reason` 串里认字：那是 `KPY5` 花一整条 DoD 治的那种假信号。"
        );
        // ② 拒绝那一支要 `warn`，不是 `info`。
        //
        // ⚠⚠ **人群必须切到那一支里去**〔08-27 死值验当场逮到的〕：本条第一版查的是
        //    **整条 `Failed` 臂**含不含 `tracing::warn!` —— 而这条臂里还有另一个 `warn!`
        //    （「通知发不出去」那条兜底）⇒ 把拒绝那支的 `warn!` 改回 `info!`，
        //    实测 **1194 passed / 0 failed，一条都不红**。它是个安慰剂，而它读起来完全正常。
        //    ★ 这就是「守卫范围 ≠ 性质范围」在本轮的第七形：**人群比性质大了一格**。
        // ⚠ 下限刻意压到 100：拒绝那一支只剩「一句 warn」时它有 189 字节 ——
        //   下限定在 200 会让这一刀红在「配平切错了」上，而那句诊断是**假的**
        //   （切法没错，是那一支被掏空了）。假诊断比不红更贵：它把人引到错的地方。
        let refusal_branch = braced_block(&lib, "Some(next_step) =>", 100, 3000);
        assert!(
            refusal_branch.contains("tracing::warn!"),
            "拒绝那一支里没有 `warn!` —— 「出声」这两个字在这条路上就没兑现"
        );
        assert!(
            !refusal_branch.contains("tracing::info!"),
            "拒绝那一支里出现了 `info!` —— 「口被别人占着」不是一条 info：\
             它是一件用户能动手解决、而且不动手就没有本机后端的事"
        );
        // ③ ★ 正题：**把日志行整段拿掉之后，仍然剩着一个用户看得见的出口。**
        let without_logs: String = refusal_branch
            .lines()
            .filter(|l| !l.trim_start().starts_with("tracing::"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            without_logs.contains(".notification()") && without_logs.contains(".show()"),
            "把 `tracing::` 那些行拿掉之后，这一臂里**只剩日志** ——\n\
             ★ 这正是 `重-2` 那条病：日志不是「说给用户听」，是「说给下一个来查日志的人听」。\n\
             ⇒ 拒绝这一格必须有一个用户看得见的出口（今天是 `tauri_plugin_notification`）。\n\
             换出口可以，回来改本条并写明新出口是什么；**别退回只写日志**。"
        );

        // ⑤ ★★ 那句话**真的进了那个出口** —— ③ 只买到「有出口」，中间隔着一个参数。
        //    〔`D2` 实打：`.body(&next_step)` → `.body("")` ⇒ 1194/0，五条判据全绿。〕
        //    ⚠ 认的是「`.body(` 那一行的实参里有 `next_step`」，不是那一行的逐字长相 ——
        //    `.body(next_step.clone())` 这种等价写法照样过，`.body("")` 过不去。
        //    ⚠⚠ **先断言它唯一再取**〔收工前自查逮到的：`重-D2-3` 那一形长在我自己刚写的这一行上〕：
        //    `.lines().find(…)` 又是一次「**取第一处**」。`.body(` 出现两次时（builder 被调两遍）
        //    生效的是**后一个**，而这里读的是**前一个** ⇒ 本条会对着一个不生效的参数说「进了」。
        let body_lines: Vec<&str> = refusal_branch
            .lines()
            .filter(|l| l.contains(".body("))
            .collect();
        assert_eq!(
            body_lines.len(),
            1,
            "拒绝那一支里 `.body(` 有 {} 行（该恰好 1 行）：{body_lines:?}\n\
             ★ 0 行 = 用户可见出口换了形状 ⇒ **回来把本条改成新出口那一格**：\n\
               ③ 只钉「有出口」、不钉「那句话进没进出口」，光靠 ③ 会留下一个正文空白的通知。\n\
             ★ ≥2 行 = 生效的是**后一个**，而下面读的是**前一个** —— 本条会对着一个\n\
               不生效的参数说「进了」。**别放宽这条断言，把人群切窄。**",
            body_lines.len()
        );
        let body_line = body_lines[0];
        assert!(
            body_line.contains("next_step"),
            "通知正文里没有 `next_step`：{}\n\
             ★ 上面 ④ 逐字钉过的那句话（含「下一步」、含 `pid_path(&dir, port)`）\n\
             **一个字都没进到用户眼前** —— 用户看到的是一个标题写着「本机后端没起来」、\n\
             正文**空白**的通知，而那句他照着做就能解决问题的话躺在日志里。\n\
             ⇒ 「有一个出口」与「那句话进了出口」是两件事，中间隔着一个参数。",
            body_line.trim()
        );

        // ④ 那句话说得出**下一步能做什么**，而且它是在**真的被拒绝**那一臂里写下的。
        let me = guard_core::production_code(include_str!("local_daemon.rs"));
        // ⚠ 标记带上 `=> {`：裸的 `Adopt::Refused(why) =>` 在生产段里**命中 2 处**
        //   （另一处是 `probe_and_attach_after_spawn` 的 `=> Err(why),`），
        //   而 `braced_block` 今天会断言唯一 ⇒ 不加这两个字符本条当场红。
        let refused = braced_block(&me, "Adopt::Refused(why) => {", 300, 3000);
        assert!(
            refused.contains("note_start_refusal("),
            "那条记录不再写在 `Adopt::Refused` 那一臂里 —— \
             写在别处就等于又回到「从别的信号反推这一次是不是拒绝」"
        );
        for (needle, why) in [
            ("下一步", "只报「起不来」是没用的：用户要的是**现在该干什么**"),
            (
                "pid_path(&dir, port)",
                "「把那个进程停掉」要说得出它是哪个进程 —— pid 记在那个文件里",
            ),
        ] {
            assert!(
                refused.contains(needle),
                "拒绝那句话里没有 `{needle}`。\n说法：{why}"
            );
        }
    }

    /// ★★ `重-D2-1`：**「用户动得了手」那一档的每一条路，都要走到用户眼前。**
    ///
    /// # 它治的那一格（分母是 `D2` 实打出来的）
    ///
    /// `重-2` 上一轮只治了**一条**路：`Adopt::Refused` 那一臂。而 `StartOutcome::Failed`
    /// 的构造点**现打 6 处**，写记录的只有 **1** 处 —— 而没写记录的那 5 处里，
    /// `ensure_listen_token` 的 `Err` 那一支**正是同一轮 `阻-4` 刚改好的那一支**：
    /// 空 token ⇒ `Err` ⇒ **自动起**那条路对没有记录的失败走 `tracing::info!`
    /// ⇒ 用户什么都看不到，`阻-4` 花力气写出来的那句「`rm <路径>`」**只说给日志听**。
    ///
    /// ★ 这撞的是 `lib.rs` 自己写下的分档标准（逐字）：「**拒绝**（口上有东西、接不上）=
    /// **一件用户能动手解决的事** ⇒ 说到眼前；别的失败（安装包里还没有 sidecar…）=
    /// 诚实降级 ⇒ 仍走日志」。「盘上有个零字节的 token 文件，删掉它再起一次」按这条标准
    /// **属于前者**，而上一轮把它落在了后者。
    /// ⇒ **`阻-4` 只修了一半**：它让诊断**指对了地方**，而「指对了的那句话**被谁听见**」
    /// 落在 `重-2` 的人群外面 —— 两处修各治一半，中间那一格谁都没管。
    ///
    /// # 钉四件
    ///
    /// ① **分母钉死**：`StartOutcome::Failed` 的构造点恰好 6 处 —— 新增或删掉一处本条就红，
    ///    **回来给它分档**。这正是上一轮缺的那道门：没有它，第 7 处失败照样可以静默走日志。
    /// ② **拿 token 那一格**真的调了 [`note_start_refusal`]（`阻-4` 那条诊断今天到得了用户眼前）。
    /// ③ 转交的是 `ensure_listen_token` **那句话本身**，而那句话说得出下一步删哪个文件。
    /// ④ 写记录的**唯一入口**就是它，今天恰好 3 处（1 定义 + 2 调用点）。
    ///
    /// ⚠ **②③ 排在 ④ 前面是有意的**：把那个调用删掉时，先红的该是「拿 token 那一格不再写记录」
    /// 这句**说得出病在哪**的诊断，而不是「计数从 3 变成 2」这句要人再去查一遍的记账话。
    ///
    /// # 它挡不住什么（如实登记，别读大）
    ///
    /// 「哪一条失败算**用户动得了手**」是**自然语言判断**，机检不了。本条买的是两样：
    /// **分母不许在没人回来重判的情况下变**，以及已经分好档的那两条路不许悄悄退回日志。
    /// 有人新加一处 `Failed` 再把 ① 的数字改大就能过 —— 但那时他**必须动本条**，
    /// 而动本条要读这段话。**这是触发器，不是证明**（本仓已成型的那一档）。
    ///
    /// ⚠ 还有一格如实记：它钉的是**源码形状**，不是「用户真的看到了」——
    /// 那半在 `the_auto_start_refusal_is_not_only_a_log_line` 的 ③⑤ 两条，
    /// 而**真机上弹没弹**两条判据都证不了（要真跑一次 GUI）。
    #[test]
    fn the_user_actionable_start_failures_all_reach_the_user() {
        let prod = guard_core::production_code(include_str!("local_daemon.rs"));
        // 反空真：语料塌了 ⇒ 下面几条全是 `0 == 0` 的空真。
        assert!(
            prod.len() > 20_000,
            "`local_daemon.rs` 的生产段只剩 {} 字节（下限 20_000）—— 剥过头了，本条在空转",
            prod.len()
        );
        // ① 分母：失败的构造点今天恰好 6 处。
        assert_eq!(
            prod.matches("StartOutcome::Failed {").count(),
            6,
            "`StartOutcome::Failed` 的构造点不是 6 处了 —— \n\
             ★ **新增一处失败就要给它分档**：用户动得了手（删文件 / 停进程 / 改权限位）\n\
             ⇒ 调 `note_start_refusal` 把话说到眼前；诚实降级（还没有 sidecar 那种）⇒ 仍走日志。\n\
             ⇒ 改完把这个数改对，**别只改数**。"
        );
        // ②③ 拿 token 那一格：真的写了记录，而且转交的是那句说得出下一步的话。
        let token_lane = braced_block(&prod, "match ensure_listen_token(&dir)", 150, 1200);
        assert!(
            token_lane.contains("note_start_refusal("),
            "拿 token 失败那一格不再写记录 —— 它会退回 `tracing::info!`：\n\
             ★ 用户看到的是「本机后端没起来」四个字都没有，而盘上躺着一个他删掉就好了的空文件。"
        );
        assert!(
            token_lane.contains("looked_at: vec![token_path(&dir)]"),
            "拿 token 失败那一格不再把 token 文件放进 `looked_at`"
        );
        let ensure = body_of(&prod, "fn ensure_listen_token(", 400);
        assert!(
            ensure.contains("**下一步：删掉这个文件再起一次**"),
            "`ensure_listen_token` 空文件支那句话不再说「下一步」——\n\
             ★ 这句话现在是**直接转交给用户**的（不再只进日志），\n\
             它少了「下一步」这三个字，用户拿到的就只是一句「它坏了」。"
        );
        // ④ 写记录只有一个入口，今天恰好两条路在用它。
        assert_eq!(
            prod.matches("note_start_refusal(").count(),
            3,
            "`note_start_refusal` 在生产段里不是 3 处（1 个定义 + 2 个调用点）——\n\
             少了 = 某一条「用户动得了手」的路退回了只写日志；\n\
             多了 = 又有一条路被分进这一档，回来把本条与那段分档说明一起改。"
        );
    }

    /// ★★ `重-4`：**登记表的说法必须覆盖那处 `sleep` 的每一个用途。**
    ///
    /// 〔`K-P1-D1` `重-4`〕`rust_timer_registry` 那一行原先逐字只写
    /// 「`probe_and_attach_after_spawn` 等**刚脱离起来的那个 daemon** 把回环口 bind 上」——
    /// 而那处 `sleep` 住 `adopt_with`，`adopt_with` 有**两个**调用方：
    /// 另一个 `adopt_existing`（`wait_for_bind = false`）**根本没有 spawn**，
    /// 它等的是 `stream-busy` 那张牌被还回来。⇒ 说清了两件里的一件。
    ///
    /// ★ **登记表的说法就是那条判据的诚实边界** —— 说法与代码不是一回事时，
    /// 判据在替一个不存在的性质背书。而「说法」是散文，散文不会自己红
    /// ⇒ 本条把它变成机检：**调用方的名字从生产码里派生**（不写死），
    /// 逐个要求登记表那一行点得到。多一个调用方而不去改那行说法 ⇒ 当场红。
    #[test]
    fn the_timer_registry_names_every_caller_of_the_one_wait_here() {
        let prod = guard_core::production_code(include_str!("local_daemon.rs"));
        // 反空真①：本文件生产段里就该恰好一处 `sleep`（登记表登的 `处数` 就是这个 1）。
        let sleeps = prod.matches("thread::sleep(").count();
        assert_eq!(
            sleeps, 1,
            "生产段里 `thread::sleep(` 有 {sleeps} 处（登记表登的是 1 处）——\
             多一处就要回答它属哪一类、等什么、上限多少"
        );

        // 每一行的「外层函数是谁」：按行扫，遇到 `fn 名(` 就换人。
        let enclosing = |needle: &str, skip_def: bool| -> Vec<String> {
            let mut cur = String::new();
            let mut out: Vec<String> = Vec::new();
            for l in prod.lines() {
                let t = l.trim_start();
                if let Some(rest) = t
                    .strip_prefix("pub async fn ")
                    .or_else(|| t.strip_prefix("pub fn "))
                    .or_else(|| t.strip_prefix("async fn "))
                    .or_else(|| t.strip_prefix("fn "))
                {
                    if let Some(name) = rest.split('(').next() {
                        cur = name.trim().to_string();
                    }
                }
                if l.contains(needle) && !(skip_def && t.starts_with("fn ") ) && cur != needle.trim_end_matches('(')
                {
                    out.push(cur.clone());
                }
            }
            out.sort();
            out.dedup();
            out
        };

        // 那一处 `sleep` 住哪个函数。
        let home_of_sleep = enclosing("thread::sleep(", false);
        assert_eq!(
            home_of_sleep,
            vec!["adopt_with".to_string()],
            "那处 `sleep` 不住 `adopt_with` 了（现在住 {home_of_sleep:?}）——\
             它搬家了，本条与登记表那一行都要跟着重判"
        );

        // 谁在用它 —— **从代码派生，不写死**。
        let callers = enclosing("adopt_with(", true);
        // 反空真②：少于两个调用方 ⇒ 抽取坏了（或真的只剩一条路，那也要回来重判说法）。
        assert!(
            callers.len() >= 2,
            "只抽到 {} 个 `adopt_with` 的调用方：{callers:?} —— \
             `重-4` 那条病的形状正是「两个调用方只写了一个」，抽不出两个本条就在空转",
            callers.len()
        );

        // 登记表那一行必须点到每一个。
        // 按**行**切那一条登记（不按字节偏移切 —— 中文在这份文件里到处都是，
        // 按字节切会切在字符中间，那是 CRASH 不是读数）。
        let reg_lines: Vec<&str> = include_str!("rust_timer_registry.rs").lines().collect();
        let at = reg_lines
            .iter()
            .position(|l| l.trim() == "\"src/local_daemon.rs\",")
            .expect("登记表里没有 `src/local_daemon.rs` 那一条 —— 它被删了？");
        let entry: String = reg_lines[at..]
            .iter()
            .take_while(|l| l.trim() != "),")
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            entry.len() > 200,
            "切出来的那条登记只有 {} 字节 —— 切错了，下面整段在空转",
            entry.len()
        );
        for c in &callers {
            assert!(
                entry.contains(c.as_str()),
                "`rust_timer_registry` 那一行没有点到 `{c}` —— 而它是那处 `sleep` 的调用方之一。\n\
                 ★ 那张表对 `wait-for-condition` 这一类的要求逐字是「**说清等什么**、上限是多少」，\n\
                 而两个调用方等的**不是同一件事**（一个等 bind、一个等 `stream-busy` 那张牌）。\n\
                 ⇒ 只写其中一个 = 登记表在替一个不存在的性质背书。\n\
                 抽到的调用方全体：{callers:?}"
            );
        }
    }

    /// ★★ `阻-4`：**token 文件那三格逐格钉死** —— 新建 · 已存在非空 · **已存在但空**。
    ///
    /// # 为什么这三格非钉不可
    ///
    /// 回修前这条路**零覆盖**（`K-P1-D1` 现打，分母 = 本文件全文）：
    /// `ensure_listen_token` 命中 **2 处** —— 一处定义、一处生产调用点，**测试段 0 处**；
    /// 非空对照：同文件 `parse_listen_owner` 命中 **8 处**（它有测试）。
    /// ⇒ `0600` 权限位 · `create_new` 竞态支 · 空文件支，**三格一格都没有判据看着**。
    ///
    /// - **`0600`**：头注逐字「★ 权限位就是这一格买的东西 —— 少了它，同机别的用户读得到 token」。
    /// - **竞态支**：`create_new` 输的那一方**读回赢家那份**，绝不覆盖
    ///   （覆盖 = 上一个宿主留下的那个 daemon 当场变孤儿）。
    /// - **空文件支**：零字节的 token 会让第一支落到 `create_new`、`AlreadyExists` 再读回空 ——
    ///   回修前两支合成闭环、返回 `Ok("")`，**每次都一样，自己好不了**。
    ///
    /// ⚙ ③ 走的**就是**竞态支：第一支读到空 ⇒ 不 return ⇒ `create_new` ⇒ `AlreadyExists`。
    /// 这不是巧合，是那条闭环的形状本身 —— 所以这一格同时是「竞态支查不查空」的判据。
    ///
    /// ⚠⚠ **射程如实记，别把这条读大一格**：竞态支里「读回来是**非空**」那半个分支
    /// （`Ok(t)`）在本判据里**走不到** —— 走到它要求「我们读的时候文件还不在，
    /// 而在我们 `create_new` 之前另一个进程把它建好并写完了」，那是**真的跨进程竞态**，
    /// 单进程里造不出来。⇒ 那半格由下面 ⑤ 的**源码钉**（`create_new(true)` 恰好一处）兜，
    /// 而 ⑤ 买的是「不覆盖」，不是「读回的是赢家那份」。**这一格今天没有行为判据。**
    #[test]
    fn the_listen_token_file_is_pinned_cell_by_cell() {
        let dir = std::env::temp_dir().join(format!(
            "ccm-token-cells-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let p = token_path(&dir);

        // ── ① 新建：目录都还不在 ⇒ 建目录 + 建文件 + 写内容 ────────────
        let first = ensure_listen_token(&dir).expect("第一次该建得出来");
        assert_eq!(first.len(), 32, "新建那一支回的不是一个完整 token：{first:?}");
        assert_eq!(
            std::fs::read_to_string(&p).expect("读回").trim(),
            first,
            "盘上那份与返回的不是同一个串 —— 下一个宿主读回的就接不上这一个"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).expect("stat").permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "token 文件的权限位是 {mode:o}，不是 0600 —— \
                 回环 TCP 上同机**别的用户**也连得上这个口，token 是那条口上**唯一**的门。\
                 权限位就是这一格买的东西。"
            );
        }

        // ── ② 已存在非空：幂等读回，**不许覆盖** ─────────────────────
        let bytes_before = std::fs::read(&p).expect("读原始字节");
        let second = ensure_listen_token(&dir).expect("第二次该读回来");
        assert_eq!(
            second, first,
            "第二次拿到的 token 变了 —— 那说明它把文件重写了，\
             而重写 = 上一个宿主留下的那个 daemon 立刻变成「连得上但认证不过」的孤儿"
        );
        assert_eq!(
            std::fs::read(&p).expect("再读字节"),
            bytes_before,
            "盘上那份被动过了（`create_new` 那条路只许创建一次）"
        );

        // ── ③ 已存在但**空**：必须 `Err`，而且诊断要指到这个文件 ────────
        std::fs::write(&p, b"").expect("造一个零字节 token");
        let e = ensure_listen_token(&dir)
            .expect_err("零字节 token 必须是 `Err` —— 回它 `Ok(\"\")` 就是那个自己好不了的闭环");
        assert!(
            e.contains(&p.display().to_string()),
            "空 token 的诊断里没有那个文件的路径：{e:?}\n\
             ★ 行为上 fail closed 是不够的：用户看到的是「脱离的 daemon 起来了却连不上它」，\n\
             而 `looked_at` 里是那个**二进制** —— 一个字不提 token 文件，他就不知道该删哪个文件。"
        );
        assert!(
            std::fs::metadata(&p).is_ok(),
            "报错的同时把文件删了 —— 那是替用户做决定（这里只出声，删由人来）"
        );

        // ── ④ 诊断的另一半：生产调用点真的把 `token_path` 放进 `looked_at` ──
        let prod = guard_core::production_code(include_str!("local_daemon.rs"));
        let start = body_of(&prod, "fn start_detached(", 800);
        assert!(
            start.contains("looked_at: vec![token_path(&dir)]"),
            "`start_detached` 里 `ensure_listen_token` 的 `Err` 那一支不再把 token 文件\
             放进 `looked_at` —— 上面 ③ 那句话就传不到用户眼前了"
        );

        // ── ⑤ 「只创建一次、绝不覆盖」这一格**只有源码钉认得出** ──────────
        //
        // ⚠ 射程如实记：行为上「第二次拿到同一个串」有**两条**独立的路
        //   （第一支的提前 `return` + 竞态支的读回）⇒ 单改一处杀不掉上面 ② 那一格。
        //   而「覆盖」这件事本身是**一个方法名**的事，所以这里拿它当锚点。
        let ensure = body_of(&prod, "fn ensure_listen_token(", 400);
        assert_eq!(
            ensure.matches("create_new(true)").count(),
            1,
            "`ensure_listen_token` 里 `create_new(true)` 不是恰好一处。\n\
             换成 `create(true).truncate(true)` 就会**覆盖**盘上已有的那份 token ——\n\
             而覆盖 = 上一个宿主留下的那个 daemon 立刻变成「连得上但认证不过」的孤儿，\n\
             且它自己不知道，用户只看到「停不掉也接不上」。"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ★★ `K-R9` 09-04：**monitor 这棵树上，没有一份文件让块注释原样穿过去。**
    ///
    /// # 它看着的是哪件事
    ///
    /// 本文件里十来条判据都在数 `guard_core::production_code` 从本文件源码剥出来的那份文本。
    /// `K-R9` 给共用剥法补上块注释之后，它带了一条**静默兜底**：词法与某份文件对不上时
    /// （`depth` 不收口 / 还停在字符串里）**一个字都不剥**、原文原样返回 ——
    /// 那是「宁可留洞，不许造假红」的取舍（剥狠了会砍掉真代码，那是拿假阳换真阳）。
    ///
    /// ⇒ 兜底一旦触发，那份文件上「把真代码包进 `/* */` 再换个桩值」的洞**当场重开**，
    /// 而所有判据一个数都不动。**本条就是不让那件事静悄悄发生的那道判据。**
    ///
    /// 现打（09-04，本工作树，**未铺** `src-tauri/embedded-daemons/`）：
    /// `src-tauri/src` 下 **105 份 `.rs`，走兜底 0 份**。
    /// 地板 100 与 monitor 全树那条「剥得干不干净」的自检同职：**扫到的份数低于它说明遍历坏了**，
    /// 不是代码变干净了。⚠ 棘轮纪律：只许升不许降。
    ///
    /// # ★★ 09-04 落定拍：**两个数复打成立；而上面那句「所有判据一个数都不动」被打穿了一半**
    ///
    /// 现打（量于 `4eb271f`，本树未铺 `src-tauri/embedded-daemons/`，跑在沙箱里）：
    /// **105 份 / 走兜底 0 份**，与上一拍逐字相同 —— 但这是量出来的，不是抄的
    /// （分母 = `find src-tauri/src -name '*.rs'`，与本条的遍历同语义）。daemon 侧 **73 / 0** 同样成立。
    ///
    /// **死值验两刀**（逐字落在 `evidence/K-R9-R2-fallback-watch-scope.md`）：
    ///
    /// · **刀一 · 本条有牙**：塞一行能编译的 `(br"\", "/*")` 让本文件真的走进兜底
    ///   —— 本条**当场红并逐份点名**，同一状态下全量 monitor 是 `1276 passed; 1 failed`
    ///   ⇒ **一份文件整个掉进兜底，全树只有本条会说话**（那正是它存在的理由，现在它是读数不是散文）。
    ///
    /// · 🔴 **刀二 · 本条的射程被打穿**：本条量的单位是**文件**，而剥法在
    ///   [`tests::every_test_that_starts_the_real_daemon_demands_a_private_tmux`] 里
    ///   真正跑的单位是**按 `#[test]` 切出来的块**（它先切块、再对每块调 `strip_comment_lines`）。
    ///   ⇒ 让 `/*` 与 `*/` **分落两块**（整份配平、单块不配平，rustc 眼里就是条普通跨行块注释），
    ///   把真调用与那行合规的 `PATH` 写位一起放进注释、只留一个桩值活着
    ///   ⇒ **本条绿 · 那条守卫判「合规」· 全量 monitor `1278 passed; 0 failed`**。
    ///   **09-01 那个静默假绿的形状，在 `K-R9` 的修补之上原样复现了一次。**
    ///
    /// ⚠ **今天的曝光是 0，但那是巧合、不是被谁钉住的**：用 guard-core 自己那把词法
    /// （`guard_core::block_comment_model_holds`）按 `#[test]` 切块普查 `src-tauri/src`
    /// ⇒ **105 份 · 1278 块 · 块级走兜底 0 个**。刀二只改了一处，块级就变成 1 而本条还是绿的。
    ///
    /// ⚠ **覆盖面另有缺口，一并登记**：本条 105 ＋ daemon 侧 73 ＋ guard-core 自检 1 = **179 份**，
    /// 而排掉 vendor 的全仓是 **188 份**。差的 9 份里，`crates/usage-core/src/lib.rs`
    /// **确实被 `guard_core::production_code` 吃**（`usage.rs::tests::the_usage_kou_jing_has_exactly_one_home`）
    /// ⇒ 它掉进兜底的话，三条判据一条都不会响。
    /// 〔本拍写区只有本文件，收口要动 guard-core / 那两条守卫的切法 ⇒ **只登记，不动手**。〕
    #[test]
    fn no_monitor_file_falls_back_to_leaving_block_comments_in() {
        guard_core::assert_block_comment_model_holds(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            100,
        );
    }

    // ══════════════════════════════════════════════════════════════════
    // `KPY2` / `KPY3`：**真进程**判据。⛔ 它们**不在 `npm run gate` 里**
    //
    // 只在 CI 的 `E2E real-machine` job 里跑（`ci.yml` 的
    // `assert-pass-floor.sh local-backend <地板>`），入口是
    // `e2e/local-backend-supervise.sh`。本机 `npm run gate` **看不见它们**
    // —— 这句话是 `§1` 那张「在哪一步会被执行」表逐字要求写明的。
    // ══════════════════════════════════════════════════════════════════

    /// 起真 daemon 的沙箱。**三样东西一个都不许缺**（fail closed）。
    #[cfg(target_os = "linux")]
    struct E2eSandbox {
        bin: std::path::PathBuf,
        /// 前面挂着 tmux shim 的 PATH —— `C7i` 的**唯一**隔离原语。
        shim_path: (String, String),
        work: std::path::PathBuf,
        /// 本轮起过的那些进程。**一交出 `DETACHED` 就立刻塞进这里**，
        /// 中间不留任何「拿在手里但没人管」的窗口 —— 那个窗口正是 08-26 漏网的机制。
        kept: std::sync::Mutex<Vec<std::process::Child>>,
    }

    #[cfg(target_os = "linux")]
    impl E2eSandbox {
        fn demand() -> Self {
            let bin = std::env::var("CCM_E2E_DAEMON").expect("要 CCM_E2E_DAEMON");
            // ★ **fail closed**：拿不到 shim 就炸，绝不降级裸跑 ——
            //   裸跑 = 去改用户真实 tmux server 的 `[50]` 槽位（08-11 / 08-26 各出过一次）。
            let shim = demand_tmux_shim("委托腿上的 PATH 全由本处一处代办");
            let work = std::path::PathBuf::from(
                std::env::var("CCM_E2E_WORK").expect("要 CCM_E2E_WORK"),
            );
            Self {
                bin: std::path::PathBuf::from(bin),
                shim_path: (
                    "PATH".to_string(),
                    format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
                ),
                work,
                kept: std::sync::Mutex::new(Vec::new()),
            }
        }

        /// 「上一个宿主退了」= 把进程内那点状态清掉，**但不碰那个进程**；
        /// 它手里那个 `Child` **当场**交给沙箱看着。
        fn forget_like_a_host_that_exited(&self) {
            // ★ **真宿主退出时那条 socket 会关掉** —— 只清进程内的句柄是**演砸的模型**：
            //   那条流还挂着，下一个宿主拿到的是 `stream-busy`。
            //   〔实测：第一版就是这么写的，`KPY2` 当场红在「第二个宿主没有认出已有实例」。〕
            if let Some(c) = crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN) {
                crate::inbound_client::unregister(crate::inbound_client::LOCAL_ORIGIN, &c);
                // ⚠ `shutdown()` 只叫醒等着的调用方，**它不关 socket**。
                //   要让对面知道我们走了，得真的关掉写半边 —— 那才是 daemon 那边的 EOF。
                c.close_write();
                c.shutdown();
                // 关写半边是**入队**的（`WriteJob::CloseWrite`）⇒ 给写任务一拍把它送出去。
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            let taken = DETACHED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take()
                .and_then(|h| h.child);
            if let Some(c) = taken {
                self.kept.lock().unwrap_or_else(|e| e.into_inner()).push(c);
            }
        }

        /// 造一个**独立的家** —— 端口是家目录算出来的，所以不同的家 = 不同的口。
        /// 这正是 `KPY2` 那条「非空对照」要用的东西。
        fn become_host_with_home(&self, tag: &str) -> std::path::PathBuf {
            let home = self.work.join(format!("host-{tag}"));
            let claude = home.join(".claude");
            std::fs::create_dir_all(claude.join("projects")).expect("建沙箱 HOME");
            std::fs::create_dir_all(claude.join("sessions")).expect("建沙箱 sessions");
            // ⚠ 进程级环境变量 ⇒ 这几条判据**必须 `--test-threads=1`**
            //   （e2e 脚本正是这么跑的）。写在这里是因为「宿主是谁」本来就是进程级的事实。
            std::env::set_var("HOME", &home);
            std::env::set_var("CLAUDE_CONFIG_DIR", &claude);
            claude
        }

        fn envs(&self) -> Vec<(String, String)> {
            vec![self.shim_path.clone()]
        }
    }

    /// ★★ **判据不许在机器上留活进程** —— 而且这一条比它听起来重要得多。
    ///
    /// 〔08-26 实测教训，本件实现期间真发生了一次〕漏一个的后果**不是**「多一个进程」：
    /// e2e 的 trap 会把 shim 目录 `rm -rf` 掉，而那个漏网的 daemon **还活着**；
    /// 它下一次装 tmux hook（watcher 换人时会重装）沿 PATH 找不到 shim ⇒ **落到真 tmux 上**
    /// ⇒ 用户真实 server 的 `[50]` 槽位被盖成它的 pid。当时按原值恢复了，但那是靠人。
    ///
    /// ⇒ 收尾放进 `Drop`：**测试 panic 时它照样跑**（unwind 会走析构），
    /// 而写在测试体末尾的收尾在第一条断言红掉时就被跳过了 —— 那正是这次漏网的机制。
    #[cfg(target_os = "linux")]
    impl Drop for E2eSandbox {
        fn drop(&mut self) {
            // ⚠ **按句柄收，不扫 `/proc`**：扫目录会撞 `scanning_guard_registry`
            // （「有扫描型判据在测试段里裸遍历目录」），而且按 exe 路径杀是**按模式杀** ——
            // 这台机器上还跑着用户自己的真 daemon，那种收法迟早会误伤。
            // ⇒ 只收**我们自己起出来的那几个句柄**，一个不多一个不少。
            let mut all: Vec<std::process::Child> = std::mem::take(
                &mut *self.kept.lock().unwrap_or_else(|e| e.into_inner()),
            );
            if let Some(h) = DETACHED.lock().unwrap_or_else(|e| e.into_inner()).take() {
                all.extend(h.child);
            }
            for mut c in all {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }

    /// 把结局拆开看。**别写成 `matches!` + 一句「起不出来」** ——
    /// 那样红的时候只知道「不是 `Started`」，而四种结局的排查方向完全不同
    /// （没走那条路 / 已经在跑 / 起不出来各有各的下一步）。
    #[cfg(target_os = "linux")]
    fn expect_started(out: DetachOutcome, who: &str) -> std::path::PathBuf {
        match out {
            DetachOutcome::Done(StartOutcome::Started(p)) => p,
            DetachOutcome::Done(StartOutcome::AlreadyRunning) => {
                panic!("{who}：期望起一个新的，实得「已经在跑」—— 上一条判据留了活进程？")
            }
            DetachOutcome::Done(StartOutcome::Failed { reason, looked_at }) => {
                panic!("{who}：起不出脱离的 daemon —— {reason}；找过 {looked_at:?}")
            }
            DetachOutcome::NotTaken => panic!(
                "{who}：**这条路根本没走**（`detach_wanted` 判了 false）。\
                 不是 Linux？还是 `{NO_DETACH_ENV}` 被设了？"
            ),
        }
    }

    /// 同上，反过来那一面。**「没认出来」的四种来路完全不同**，红的时候要说得出是哪一种。
    #[cfg(target_os = "linux")]
    fn expect_adopted(out: DetachOutcome, who: &str) {
        match out {
            DetachOutcome::Done(StartOutcome::AlreadyRunning) => {}
            DetachOutcome::Done(StartOutcome::Started(p)) => panic!(
                "{who}：**又起了一个**（{}）—— 那正是「每台机 N 个 daemon 互相盖 [50] 槽位」",
                p.display()
            ),
            DetachOutcome::Done(StartOutcome::Failed { reason, .. }) => {
                panic!("{who}：没认出已有实例 —— {reason}")
            }
            DetachOutcome::NotTaken => panic!("{who}：这条路根本没走"),
        }
    }

    #[cfg(target_os = "linux")]
    fn proc_state(pid: u32) -> Option<char> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let after = &stat[stat.rfind(')')? + 1..];
        after.split_whitespace().next()?.chars().next()
    }

    #[cfg(target_os = "linux")]
    fn alive(pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }

    /// ★★ `KPY2`：**同机第二个宿主不许起第二个 daemon —— 它必须接上第一个。**
    ///
    /// # 为什么这一格是硬的
    ///
    /// daemon 一起来就**无条件**往 tmux server 装三条全局 hook、固定槽位 `[50]`、
    /// **没有关掉它的开关**，载荷里烤着那一个 daemon 的 pid+starttime
    /// ⇒ **脱离而不认已有实例 = 每台机 N 个 daemon 互相盖槽位，比今天更糟**。
    ///
    /// # 它怎么会失效 —— 以及本条为此付了什么
    ///
    /// 「夹具只起一个宿主 ⇒『认已有实例』那一支恒不走，判据是**空真**」。
    /// ⇒ 本条有**两个宿主**，而且配了一个**非空对照**：换一个家目录（= 换一个口）之后
    /// 同样两步必须**真的起出第二个进程**。两个读数一起看才说明「不起第二个」是**判据在起作用**，
    /// 而不是「这条路本来就起不出进程」。
    #[test]
    #[ignore]
    fn e2e_a_second_host_adopts_the_running_daemon_instead_of_starting_a_second_one() {
        #[cfg(target_os = "linux")]
        {
            let _guard = crate::inbound_client::local_origin_test_lock();
            let sb = E2eSandbox::demand();
            let claude = sb.become_host_with_home("adopt");
            *DETACHED.lock().expect("锁") = None;

            // ── 宿主① ────────────────────────────────────────────────
            let bin = sb.bin.clone();
            expect_started(start_detached(&|| Ok(bin.clone()), &sb.envs()), "第一个宿主");
            let pid1 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
            assert!(alive(pid1), "起出来的 pid={pid1} 不在进程表里");
            println!("E2E-OK KPY2 第一个宿主起出脱离的 daemon（pid={pid1}）");

            // 「谁在听那个口」那份记录要真的落了盘 —— 没有它，下一个宿主停不了它。
            let port = listen_port_for(&claude.to_string_lossy());
            let owner = read_listen_owner(&cc_monitor_dir(), port);
            assert_eq!(owner.as_ref().map(|(p, _)| *p), Some(pid1), "「谁在听」没记对");
            println!("E2E-OK KPY2 「谁在听 {port}」落了盘且带二进制路径（停按钮的凭据）");

            // ── 宿主①退出（不碰那个进程 —— 那正是「脱离」）────────────
            sb.forget_like_a_host_that_exited();
            assert!(alive(pid1), "上一个宿主一退，那个 daemon 就跟着走了 —— 那不叫脱离");
            println!("E2E-OK KPY2 宿主退出之后那个 daemon **还活着**（真脱离）");

            // ── 宿主② ────────────────────────────────────────────────
            let bin2 = sb.bin.clone();
            expect_adopted(start_detached(&|| Ok(bin2.clone()), &sb.envs()), "第二个宿主");
            let pid2 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
            assert_eq!(pid2, pid1, "第二个宿主接上的不是同一个进程");
            assert!(
                crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN).is_some(),
                "接上了却没登记入方向通道 —— 那只是「连上了」，不是「接上了」"
            );
            println!("E2E-OK KPY2 第二个宿主**接上**了同一个 daemon（pid 不变），没起第二个");

            // ── ★ 非空对照：换一个家 = 换一个口 ⇒ 必须**真的**起出第二个 ──
            sb.forget_like_a_host_that_exited();
            let claude_b = sb.become_host_with_home("control");
            assert_ne!(
                listen_port_for(&claude_b.to_string_lossy()),
                port,
                "两个家目录算出同一个口 —— 那这条对照说明不了任何事"
            );
            *DETACHED.lock().expect("锁") = None;
            let bin3 = sb.bin.clone();
            expect_started(
                start_detached(&|| Ok(bin3.clone()), &sb.envs()),
                "非空对照（换一个数据目录 = 换一个口）",
            );
            let pid3 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
            assert_ne!(pid3, pid1, "对照组拿到了同一个 pid —— 对照不成立");
            println!("E2E-OK KPY2 非空对照：换一个数据目录（换一个口）**真的**起出了第二个进程");

            // ── 收尾：全交给沙箱的 `Drop`（**panic 时它照样跑**，写在这里的收尾不会）──
            //    最后那个还在 `DETACHED` 里，`Drop` 会把它一起收掉。
            let _ = std::env::var("CCM_E2E_TMUX_SHIM_BIN"); // 让本条自己也点名那个隔离
        }
    }

    /// ★★ `KPY3`：**脱离之后不留僵尸。**
    ///
    /// # 断言必须落在**进程表**上，不是落在我们自己的事件上
    ///
    /// `launch.rs:196-198` 头注逐字：「`process_group` **不改变父子关系** ⇒ 不 `wait` 就留僵尸」。
    /// 若断言落在「消费者收到了 EOF」上，僵尸照样在 —— 那条断言会绿着放过整个缺陷。
    /// ⇒ 这里读 `/proc/<pid>/stat` 的**状态字**：既不许是 `Z`，最后也不许还在。
    #[test]
    #[ignore]
    fn e2e_a_detached_daemon_that_dies_leaves_no_zombie() {
        #[cfg(target_os = "linux")]
        {
            let _guard = crate::inbound_client::local_origin_test_lock();
            let sb = E2eSandbox::demand();
            let _ = sb.become_host_with_home("zombie");
            *DETACHED.lock().expect("锁") = None;

            let bin = sb.bin.clone();
            expect_started(start_detached(&|| Ok(bin.clone()), &sb.envs()), "本条");
            let pid = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
            assert!(alive(pid), "起出来的 pid={pid} 不在进程表里");

            // **从外面**把它结束掉（不是走我们的 `stop`）—— 那才是「它自己崩了」的形状。
            signal_term(pid).expect("发不出 SIGTERM");

            // 流断了 ⇒ 读方拿到 EOF ⇒ 收尸。**事件驱动**，这里只是等那个事件走完。
            let mut gone = false;
            let mut ever_zombie = None;
            for _ in 0..200 {
                match proc_state(pid) {
                    None => {
                        gone = true;
                        break;
                    }
                    Some('Z') => ever_zombie = Some('Z'),
                    Some(_) => {}
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            assert!(
                gone,
                "pid={pid} 10 秒后还在进程表里（最后见到的状态：{:?}）。\n\
                 ★ 若它是 `Z`：那就是这条 DoD 要防的僵尸 —— `process_group` 不改父子关系，\n\
                 不 `wait` 它就一直挂在进程表上，而我们自己的事件（EOF）**早就到了**。",
                ever_zombie
            );
            println!("E2E-OK KPY3 脱离的 daemon 被结束之后进程表里那个 pid 消失了（不是 Z）");
            // ★ 收尸走的是**我们自己那条线**：`Child` 被 `reap_detached` 取走交给收尸线程，
            //   而句柄里 **pid 与二进制路径都还在** —— 「停」那一步照样有凭据。
            //   ⚠ 这两半要一起断：只断「pid 没了」的话，把整条收尸线删掉、改成
            //   「进程自己被 init 收走」也会绿（而那要等 monitor 退出）。
            {
                let g = DETACHED.lock().expect("锁");
                let h = g.as_ref().expect("句柄");
                assert!(
                    h.child.is_none(),
                    "`Child` 还留在句柄里 —— `reap_detached` 没把它交出去，那条收尸线没跑"
                );
                assert_eq!(h.pid, pid, "收尸之后 pid 记录被抹了 —— 「停」就没凭据了");
                assert!(!h.bin.as_os_str().is_empty(), "二进制路径被抹了 —— 身份核对没了对照物");
            }
            println!("E2E-OK KPY3 收尸走的是我们自己那条线（`Child` 交给了收尸线程，pid+二进制仍在）");
            *DETACHED.lock().expect("锁") = None;
            let _ = std::env::var("CCM_E2E_TMUX_SHIM_BIN");
        }
    }
}

// ══ 取 shim 那个唯一入口的**跨文件出口**〔`K-R7` 09-01，`§0q` 裁一 · 出路乙〕════
//
// `backend/control/local_backend.rs` 的三个落点也要走这个口，而 `mod tests` 是私有的
// ⇒ 在这里 `pub(crate)` 重导出一次。
//
// 🔴 **为什么不是直接把 `mod tests` 改成 `pub(crate) mod tests`** —— 09-01 实打，
//   那一改**当场打红 17 条**（`cargo test -p monitor --lib`：`1194 passed; 17 failed`）。
//   机制现打自 `src-tauri/crates/guard-core/src/lib.rs:141`：`test_module_ranges` 认测试模块的条件是
//   「`#[cfg(test)]` 的下一行 `trim()` 后 **`starts_with("mod ")`** 且以 `{` 收尾」——
//   `pub(crate) mod tests {` 过不了这一关 ⇒ 整个测试段被当成**生产段**，
//   于是走 `production_source` / `production_code` 的守卫跟着去扫测试代码
//   —— **那一趟实打 17 条红**（分母 = `cargo test -p monitor --lib` 的判定行：
//   `1194 passed; 17 failed; 13 ignored`；**我没有逐条去数「全仓有多少条这样的守卫」**），
//   `guard-core` 的反向自检逐字：「剥完仍残留 **23** 个测试属性 —— 剥法坏了」。
//   ⇒ **本行这种写法是被那次实测选出来的，不是随手写的**：`#[cfg(test)]` 底下不是
//     `mod X {` 的东西，剥法会原样跳过（`i = attr_end; continue;`），一个字节都不影响剥法。
//   ⚠ 它也**排在 `mod tests` 之后** ⇒ 「文件里首个 `#[cfg(test)]` 之前那一段」逐字节没变
//     （本件四拍守着的「生产段零改动」按的就是那个切法）。
#[cfg(test)]
pub(crate) use tests::demand_tmux_shim;
