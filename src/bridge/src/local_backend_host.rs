//! P2s（定框 `C8`）：**本机后端的生命周期** —— 起 / 停 / 状态。
//!
//! # 为什么它不住 `lib.rs`
//!
//! ⚠ **本段刻意不把那个宏的名字连着 `!` 写全** —— `parity_ledger` 的命令清单解析器取的是
//! `lib.rs` 里**第一处**那个字面量，散文里写全会被它当成清单去解析（实测：只解析出 1 条命令）。
//! 这已经是本轮第二次被自己的散文绊到（另一次是位置比较判据扫 `rfind` 带括号那个）。
//!
//! `backend_status` 是 `#[tauri::command]`，而 tauri 的 IPC 命令清单宏会把命令的辅助宏
//! **`use` 回它所在的那个模块** ⇒ 命令与 `generate_handler!` 同住一个模块必然重名冲突
//! （实测 `E0255: __cmd__backend_status is defined multiple times / reimported here`）。
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
/// 仍然是两个后端，而且互相认不到。
///
/// ⚠ 补审 08-11 量到这条比原先登记的更糟：`tauri_plugin_single_instance` **只在
/// `#[cfg(windows)]` 注册**（`lib.rs` 那处）⇒ **Linux/macOS 上两个 monitor 天然能并存**，
/// 连那道兜底都没有。它们会撞同一个 `~/.cc-monitor/bin/.<name>.partial`（补审 C2）。
/// ⇒ 真正的「每台机一个」要等 `P2d`（backend 自己有监听口 + 起时认已有实例）。
/// 起本机后端的结局 —— **三态，不是两态**〔D 阶段补审 08-11 新增，A6〕。
///
/// 原来三种结局全塞在 `Resolved` 里：`Found` 与两种 `Missing`（「已经在跑」与「起不来」）。
/// 而 `backend_control::backend_start` 把 `Missing{reason}` 当 `Ok(reason)` 返回
/// ⇒ 「没内嵌后端」「释放失败」「已经在跑」三种完全不同的结局在前端**都走 `console.info`**，
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

impl StartOutcome {
    /// 起不来那一支的两个字段。`None` = 这一次不是「从来没起来」。
    ///
    /// # ⚠ 这里为什么写 `Self::` 而不是把类型名写全〔`K-P3b`，别改回去〕
    ///
    /// 同文件那条 [`tests::the_user_actionable_start_failures_all_reach_the_user`] 的第 ① 格
    /// 数的是**字面** `StartOutcome::Failed {`，它的分母自称是「**失败的构造点**」——
    /// 而**解构与构造在 Rust 里长得一模一样**。
    /// 照那个写法写这一处，那把尺子会多数出一处**不需要分档的东西**，
    /// 而它红出来的诊断（「新增一处失败就要给它分档」）**是假的**
    /// —— 本文件逐字记过：「假诊断比不红更贵：它把人引到错的地方」。
    /// ⇒ 这一处写 `Self::`：同一件事，而那把尺子的分母仍然只装构造点。
    fn failure(&self) -> Option<(&str, &[std::path::PathBuf])> {
        match self {
            Self::Failed { reason, looked_at } => Some((reason.as_str(), looked_at.as_slice())),
            Self::Started(_) | Self::AlreadyRunning => None,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// `K-P1`：常驻那条路 —— **真脱离 · 一个监听口 · 起时认得出已有实例**
//
// 这一段全部住在**宿主知识层**，理由是硬的（`K-P1 §0b-5` 现打）：
// `process_group(0)` 来自 `std::os::unix::process::CommandExt`，而 `std::os::unix`
// 在 `backend/backend_tests.rs::the_backend_half_stays_platform_agnostic` 的禁针里
// ⇒ **写进 `backend/` 当场红**；而「加一条平台例外」这条路被**递减棘轮**堵着
// （`assert!(PLATFORM_EXCEPTIONS.len() <= 1)`，今天正好 1 条）。
// ⇒ 落点只能是这里，形状照 `platform_fs::make_executable` 那个**注入**先例。
// ══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// 🪦〔散文墓碑〕 `CREATE_NO_WINDOW` / `hide_console_window` **搬走了**〔`15 §5.1 A3`，09-18〕
//
// 那是 `00 §1.5.1` 步 1 的止血，它自己的头注逐字写着终局：
// 「正确形状是 `spawn_managed(bin, args, ConsolePolicy, Lifetime, StderrSink)` ……
//   本函数是那个枚举的 `Hidden` 分支**提前落一处**，代价写明：`backend/` 这一侧因此
//   多了一条 `crate::local_backend_host::` 的反向边（A3 落地时它会被换成注入参数，
//   和 `make_executable` 一样）。」
//
// ⇒ 今天就是那一天：`ConsolePolicy::Hidden` 住 `spawn_managed.rs`，
// 那条反向边换成了 `local_backend::supervise_with_stdio` 的 `spawn` 注入参数。
// ⚠ 那条止血头注里的另一句也一起搬过去了，一个字没丢：
// **别把 `Hidden` 铺到 `launch.rs::launch_powershell_window` 头上** —— 它是 `NewVisible`。
// ═══════════════════════════════════════════════════════════════════════════

/// 握手那一行（hello / attach 应答）的字节上限。
///
/// hello 帧本机实测 ~1.1 KB（能力集 + emits + commands 三张表）。8 KiB 给了 7 倍余量，
/// 同时把「对端一直发字节不发换行」这条路堵死 —— 这条连接的对端是**同机任何进程**，
/// 不是我们自己的子进程，不能假设它讲道理。
/// 超限语义：**拒收 + 出声**（不静默截断成一行「看起来对」的 JSON）。
/// **登记住址** `src/bridge/src/byte_cap_registry.rs`（那张表默认拒绝：不登记就红）。
pub(crate) const LISTEN_HANDSHAKE_LINE_CAP: usize = 8 * 1024;

/// backend 那侧收「听哪个口」的 env 名。
///
/// ⚠ **跨 crate 字面量**：backend 那边是 `src/backend/listen.rs::ENV_PORT`。
/// 两边漂了**不会报错** —— 起出来的后端会当成「没设」而走 stdio 那条路，
/// 于是宿主等在一个永远不会有人 bind 的口上，日志里只有一句「连不上」。
/// 由 `the_listen_env_names_are_the_same_string_on_both_sides` 逐字对拍
/// （形状抄 `the_local_origin_is_the_same_string_on_both_sides`）。
pub(crate) const LISTEN_PORT_ENV: &str = "CCM_LISTEN_PORT";

/// 同上，token 那一个（backend 侧 `listen::ENV_TOKEN`）。
pub(crate) const LISTEN_TOKEN_ENV: &str = "CCM_LISTEN_TOKEN";

/// 关掉「脱离」的逃生口。**存在的理由不是好心，是判据**：
/// `KPY5` 要一格「起的时候**没走**脱离那条路 ⇒ `detached` 必须为假」的**负例**，
/// 而没有这个开关，那一支在 Linux 上永远走不到 —— 那正是「只有正例的测试永远绿」那一形。
pub const NO_DETACH_ENV: &str = "CCM_NO_DETACH";

/// 监听口取值区间：IANA 的动态/私有口段 `49152..=65535`。
///
/// 为什么不固定一个口：**同一台机上两个用户各有各的 backend**，固定口必然撞；
/// 而撞了之后的处置（见 [`probe_listen_port`]）是**出声并拒绝**，不是换个口再起一个
/// —— 换口 = 每台机 N 个后端互相盖 tmux hook 的 `[50]` 槽位，比今天更糟。
const PORT_BASE: u16 = 49152;
const PORT_SPAN: u32 = 16384;

/// `K-P1`：这台机 + 这个数据目录对应的监听口。**全仓唯一一份实现。**
///
/// # 为什么端口由宿主算、而不是两边各算一份
///
/// 「按家目录 hash 出一个口」若两边各写一份，两份就会漂 —— 本仓有现成的同族先例：
/// `shared/ccm` 与 Rust 侧各算一份 origin，实测**分叉四处**（`backend_control.rs` 头注那张表）。
/// ⇒ 只留一份实现，backend 那侧只收一个数（`listen::ENV_PORT`）。
///
/// # 算法：FNV-1a，**刻意不用 `DefaultHasher`**
///
/// `std::collections::hash_map::DefaultHasher` 的输出**跨 Rust 版本不保证稳定**
/// （它自己的文档逐字说了）。而这个数必须在**升级 monitor 之后仍然算出同一个口**，
/// 否则下一次启动会去连一个空口、起第二个 backend —— 那正是本件要防的那件事。
/// ⇒ 用一个写死的、永远不会变的 FNV-1a。
pub fn listen_port_for(home: &str) -> u16 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in home.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    PORT_BASE + ((h % u64::from(PORT_SPAN)) as u16)
}

/// monitor 自己的目录 —— token 与「谁在听」都住这里。**与后端的落点同一个目录**
/// （`~/.cc-monitor`），因为它们本来就是同一件事的两半。
fn cc_monitor_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".cc-monitor"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.cc-monitor"))
}

/// attach token 的住址。**只此一份**：下一个宿主要接上上一个宿主留下的那个后端，
/// 靠的就是读回同一个串。
fn token_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("listen-token")
}

/// 「谁在听那个口」的住址。**按口分文件**：同一台机上不同数据目录各有各的后端。
fn pid_path(dir: &std::path::Path, port: u16) -> std::path::PathBuf {
    dir.join(format!("listen-{port}.pid"))
}

/// 生成（或读回）attach token。
///
/// # ★★ 这一格是一条**真裁决**，不是实现细节
///
/// 回环 TCP 上**同机任何本地进程都连得上**（含**别的用户**），Unix socket 有文件权限位
/// 而它没有。而流那一档能发 `launch` / `kill` —— 以本账号的身份执行。
/// 收窄只能靠一个 token；而 **backend 只读铁律不许它自己写文件**（`readonly_guard`）
/// ⇒ **token 只能由宿主生成、当 env 传进去**。
/// ⇒ 权限位这件事在这里补回来：**文件本身 `0600`**，那才是真正挡住别的用户的东西。
///
/// # 为什么是 `create_new` 而不是每次重写
///
/// 每次重写 = 上一个宿主留下的那个后端立刻变成「连得上但认证不过」的孤儿。
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
/// 起新的 ⇒ backend 的 `listen::mode_from` 把空串读成「没设」⇒ 退 `EXIT_BAD_LISTEN_CONFIG`；
/// 接已有的 ⇒ `tokens_match` 的 `a.is_empty()` 直接判不等 ⇒ `WrongToken`。
/// **问题从来不是它没关上，是它关上之后指错了地方** —— 用户看到的是
/// 「脱离的后端起来了却连不上它」，`looked_at` 里是**那个二进制**，一个字没提 token 文件。
/// ⇒ 空文件在这里就地变成 `Err`，`start_detached` 的那一支会把
/// [`token_path`] 放进 `looked_at`，而下面这句话说得出**下一步删哪个文件**。
///
/// ⚠ 这一格有一个**窄窗**，如实记：另一个宿主刚 `create_new` 完、还没 `write_all` 时，
/// 我们会读到空并**如实报错**（而不是静默等它）。等它要么加定时器、要么加自旋
/// —— 而「再起一次就好了」这条路的代价明显更小。**不装作那个窗不存在。**
///
/// ⚠ 诚实边界：token 文件被人删掉 / 改掉之后，仍在跑的那个后端就再也接不上了。
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
                         空 token 起不出也接不上任何后端（两边都会 fail closed 地拒绝），\
                         而它自己不会好。**下一步：删掉这个文件再起一次** —— \
                         `rm {}`（删了之后仍在跑的旧后端也接不上了，一并停掉它）",
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
/// ⚠ **这里不用 `rand`，而理由不是「我们很克制、省下一棵依赖树」** ——
/// 先前这一行逐字写的是「没有引入 `rand`：本仓的依赖面是有代价的
/// （`C18` 依赖树零 C / 二进制量级）」，那句话今天**两头都不成立**：
/// ① `C18` 已被推翻 —— 盘上逐字「~~**C18** backend 不引 C 生态链~~ **已被推翻（08-29）**」
///    （住址 backend-consolidation 的 `MASTERPLAN.md:58`；现行版本是 `K30`
///    「装到任意 agent 机器就能跑」，不是「零 C」）。而且它逐字只约束 **backend** 那一侧，
///    本文件却编在 **monitor** 里 —— 这条从一开始就够不着这儿；
/// ② `rand` **早就在本 crate 的依赖图里**：`src/bridge/Cargo.lock` 现打两份
///    （`0.9.4` 与 `0.10.1`），直接依赖它的 4 个包是 `russh` · `internal-russh-num-bigint` ·
///    `pageant` · `tauri-plugin-notification` ⇒ 不写它，一棵依赖树也省不下来。
/// 真正的理由是下面那一段：这个 token 要挡的东西**不需要密码学随机数**。
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
    let b = nanos.rotate_left(17).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ SEQ
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_mul(0xff51_afd7_ed55_8ccd);
    format!("{a:016x}{b:016x}")
}

/// 记下「谁在听那个口」。**只有起它的那个宿主写**。
///
/// 它买的是**一件事**：下一个宿主接上这个后端之后，「停」按钮还按得动
/// （见 [`stop_local_backend`]）。没有它，接管者手里只有一条 socket，
/// 而 socket 关掉不会让对面停 —— 那时「停」就成了一句骗人的话。
///
/// ⚠ 它**不是**真相源：「那个后端还在不在」的真相源永远是**那个口连不连得上**。
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
    /// 是我们的后端：build_id 与数据目录都对得上。
    Ours,
    /// 有人占着这个口，但**不是**我们要找的那个。带上说得清的理由。
    Stranger(String),
}

/// 判那一行 hello。
///
/// ⚠ **`EADDRINUSE` / 连得上，只说明「有人占着这个口」，不说明占着它的是我们的后端。**
/// ⇒ 连上去**先读 hello 比对**，对不上就出声并拒绝，**不许静默复用**
/// （`P2t §1` 第 3 问「陈旧端点怎么识别」问的正是这一格）。
pub(crate) fn hello_verdict(line: &str, want_build: &str, want_home: &str) -> HelloVerdict {
    let Some(frame) = crate::ssh_source::parse_frame(line) else {
        return HelloVerdict::Stranger(format!(
            "这个口上的第一行不是一帧合法的后端帧（{} 字节）",
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
            "这个口上的后端是 build_id={build_id}，而本 monitor 期望 {want_build}"
        ));
    }
    if claude_dir != want_home {
        return HelloVerdict::Stranger(format!(
            "这个口上的后端看的是 {claude_dir}，而本 monitor 看的是 {want_home}"
        ));
    }
    HelloVerdict::Ours
}

/// 探那个口上有没有一个**我们的** backend。
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
/// 形状与理由与 `relay/listen.rs::DOWNSTREAM_DEADLINE` 逐字同源。
/// 值给 3 秒：对端**就在本机**，一行 ~1 KB 的 hello 在回环上是微秒级的事；
/// 3 秒比它高五六个量级，而它同时保证「起 monitor 时不会被一个哑口卡住」。
const HANDSHAKE_DEADLINE: std::time::Duration = std::time::Duration::from_millis(3_000);

fn probe_listen_port(port: u16, want_home: &str) -> Probe {
    let addr = std::net::SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), port);
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
    match hello_verdict(&line, env!("BACKEND_BUILD_ID"), want_home) {
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

/// backend 那侧「这条流已经有人占着」的拒绝理由。
///
/// ⚠ **跨 crate 字面量**（backend 侧 `listen::REFUSE_BUSY`）。它与另外两个 env 名一起
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

/// 起一个**真脱离**的后端。
///
/// 三样一起才叫脱离，缺一样都不算：
/// - **`process_group(0)`** —— 否则终端里 Ctrl-C 的 SIGINT 会打到整个前台进程组，
///   monitor 和它一起走。
/// - **stdio 全 null** —— 今天它死掉的**真正原因**就在这儿：`Stdio::piped()` 之后
///   宿主一退读端就断，它在 **153 毫秒**内 broken-pipe 退出（`backend_policy.rs` 头注实测）。
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
) -> Result<crate::spawn_managed::ManagedChild, String> {
    use crate::spawn_managed::{managed_spawner, ConsolePolicy, Lifetime, StderrSink};
    let mut cmd = std::process::Command::new(bin);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    // 〔CF1〕流模式起参与 stdio 那条载体共用一份（`local_backend::LOCAL_STREAM_ARGS`：`--tail-only --with-bg`）。
    cmd.args(local_backend::LOCAL_STREAM_ARGS)
        // ★★ **`TMUX` 一律不继承**〔08-11 事故订正，与 `supervise_with_stdio` 同一条〕：
        //   tmux 客户端在 `TMUX` 有值时按它给的 socket 走，`TMUX_TMPDIR` 完全不起作用。
        //   漏这一条，被起的后端会去改「monitor 恰好从哪个 tmux 里被启动」的那个 server。
        .env_remove("TMUX")
        .env(LISTEN_PORT_ENV, port.to_string())
        .env(LISTEN_TOKEN_ENV, token)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    // ★★ 三条策略（`00 §1.5.2`）—— 「三样一起才叫脱离」里的两样现在写在这儿：
    // · `Hidden` —— 脱离起来的后端**绝不该**在用户桌面上留一个黑框（那个框可关，
    //   一关就是 `CTRL_CLOSE_EVENT` ⇒ 常驻当场没了，而它的全部意义就是「常驻」）。
    //   🔴 这一格**先前没人回答过**：这条路上一个 creation flag 都没有。
    // · `Detached` —— 先前那句 `process_group(0)` 就是它：否则终端里 Ctrl-C 的 SIGINT
    //   会打到整个前台进程组，monitor 和这个常驻实例一起走。
    //   ⚠ 它**不改变父子关系** ⇒ 不 `wait` 就留僵尸，收尸仍走 `reap_detached`。
    // · `Null` —— stdio 全 null 是**刻意**的：`Stdio::piped()` 之后宿主一退读端就断，
    //   它在 **153 毫秒**内 broken-pipe 退出（`backend_policy.rs` 头注实测）。
    //   stdin/stdout 那两根在上面一行，stderr 这一根由策略说了算。
    let spawn = managed_spawner(ConsolePolicy::Hidden, Lifetime::Detached, StderrSink::Null);
    // ★★ `K-R28`：与 `supervise_with_stdio` **同一份分类**（住 `local_backend`，不各写一份）。
    //    这条路 exec 的正是 `resolve_backend_bin` 刚拿到的那个文件 —— 而它可能是
    //    `extract_embedded_to` 刚写出来的那一份（`K-R43` 之后经 `resolve_or_extract` 走）
    //    ⇒ 它是本仓两处「写了一个文件、随后 exec 它」的落点之一。
    local_backend::spawn_with_etxtbsy_retry(&mut cmd, &*spawn).map_err(|f| match f {
        // 这一刻恰好撞上了会自己过去的竞态 —— 那句话也是共用的那一份。
        local_backend::SpawnFailure::TransientBusy { tries, last } => {
            local_backend::etxtbsy_gave_up_reason(bin, tries, &last)
        }
        // 别的错 ⇒ **今天那句照旧，一个字不改**。
        local_backend::SpawnFailure::Broken(e) => {
            format!("起脱离的后端失败（{}）：{e}", bin.display())
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn spawn_detached(
    bin: &std::path::Path,
    _port: u16,
    _token: &str,
    _extra_env: &[(String, String)],
) -> Result<crate::spawn_managed::ManagedChild, String> {
    Err(format!(
        "本平台没有脱离那条路（{}）—— 如实降级，不假装起了一个常驻的",
        bin.display()
    ))
}

/// 脱离之后手里剩下的东西。
pub struct DetachedHandle {
    /// 那个后端的 pid。**0 表示不知道**（接管来的、而上一次那个 monitor 没记下来）。
    pid: u32,
    /// **我们起的**那个 `Child`；接管别人起的那个时是 `None`。
    ///
    /// 它同时是收尸的凭据 —— `process_group` 不改父子关系，不 `wait` 就留僵尸。
    /// ⚠ **「是不是我们起的」不另存一个 `bool`**：那样同一个事实就有了两份表示，
    /// 而两份表示会漂（本区最贵的那一族：「一个值装了两件事」的近亲）。
    /// 要问这句话就问 `child.is_some()`。
    child: Option<crate::spawn_managed::ManagedChild>,
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
/// **手动点「起」**那条路上是兑现的（`backend_control::backend_start` 回 `Err` ⇒ 前端 toast），
/// 而在**自动起**那条路上（`lib.rs` 的 `setup`，用户每天真正走的那条）
/// 回修前只进了 `tracing::info!` —— **不是 `warn`，也没有任何东西到用户眼前。**
///
/// 它有一个具体的触发场景，不是理论：端口按家目录确定性算（[`listen_port_for`]），
/// 而 `hello_verdict` 拿 `env!("BACKEND_BUILD_ID")` 逐字比 —— **升级 monitor 之后，
/// 上一次脱离留下的那个后端还在听同一个口** ⇒ `Stranger` ⇒ `Adopt::Refused`
/// ⇒ **本机后端起不来，而界面上什么都不说。**
/// ⚠ 这一格是**常驻带来的新场景**：翻面之前 backend 153ms 就死了，根本不存在「上一个还在听」。
///
/// # 为什么是一条**记录**，而不是从 `reason` 串里反推
///
/// 反推要拿字符串去认「这是不是一次拒绝」——那正是 `KPY5` 花一整条 DoD 治的那件事
/// （假信号不会报错，它只是一直说是）。⇒ 这里与 [`DETACHED`] 同一个形状：
/// **只在真的走过那条路时才被写下**，写它的唯一入口是 [`note_start_refusal`]。
///
/// ⚠ `StartOutcome` 的形状**不能动**（加一个变体或一个字段，`backend_control.rs`
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
/// ★ 而这撞的是 `lib.rs` 自己写下的分档标准〔引的是 **2026-09-10 之前**那一版原话，
/// 逐字保着 —— 这一段说的是 08-27 当时的判断，改写引文就是改写历史〕：
/// 「**拒绝**（口上有东西、接不上）=
/// 一件**用户能动手解决的事** ⇒ 说到眼前；别的失败（安装包里还没有 local_backend…）=
/// 诚实降级 ⇒ 仍走日志」。
/// ⚠ 括号里那个例子今天不成立（09-10 干净 win11 现打，PM：装出来那份跑着 2 个
/// `cc-monitor-backend.exe`、裸 `monitor.exe` 那份 0 个 ⇒ 安装包里带着本机后端），
/// `lib.rs` 那处已订正为「这一份产物里没带本机后端」。**分档标准本身一格没动。**
/// 「盘上有个零字节的 token 文件，删掉它再起一次」按这条标准
/// **属于前者**，而上一轮把它落在了后者。⇒ **`阻-4` 只修了一半**：它让诊断指对了地方，
/// 而「指对了的那句话被谁听见」落在了 `重-2` 的人群外面。
///
/// # 用它的口径（新增失败构造点时照这一条分档）
///
/// - **用户动得了手**（删一个文件、停一个进程、改一个权限位）⇒ 调本函数，
///   并且那句话要**说得出下一步**（判据钉着「下一步」这三个字）；
/// - **诚实降级**（**这一份产物里没带 local_backend** —— 裸 exe / 开发树、
///   或释放内嵌那一份也失败了；或这台机不走脱离那条路）⇒ **不要**调本函数：
///   每次启动都弹一次就成了噪音。那一档仍走 `tracing::info!`。
///   〔订正 2026-09-10（v3.7.0）—— 原话逐字：「**诚实降级**（安装包里还没有本机后端、
///    这台机不走脱离那条路）」。「安装包里还没有」今天不成立：09-10 干净 win11 现打
///    （PM）装出来那份跑着 2 个 `cc-monitor-backend.exe`、裸 `monitor.exe` 那份 0 个。
///    ⚠ **分档口径一格没动**，换掉的只是它举的那个例子。〕
fn note_start_refusal(next_step: String) {
    *LAST_START_REFUSAL.lock().unwrap_or_else(|e| e.into_inner()) = Some(next_step);
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
    DETACHED.lock().map(|g| g.is_some()).unwrap_or(false)
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
/// 而「流断了」**不等于**「那个进程死了」：backend 现在也会在**它自己**读到 EOF 时主动收掉
/// 这一条流（那是上一格修的东西）。于是 `wait()` 会等一个**还活着**的进程，
/// **一直持着那把锁** ⇒ 下一次 `backend_status` / `backend_start` / `backend_stop` 全部卡死。
/// 实测形状：测试线程 `futex_do_wait`、一个 tokio worker `do_wait`，200 秒不动。
/// ⇒ 与 `supervise_with_stdio` 头注记的是**同一条**（那边逐字写过「guard 活在闭包里
/// ⇒ `wait()` 整段都持着锁，而 `stop()` 第一件事就是取那把锁」）—— 本仓第二次。
///
/// 收尸落在一条**专用线程**上（形状抄 `launch.rs::launch_local_posix_via` 里那条），
/// 它随子进程结束而结束；`Child` 被取走之后句柄里只剩 pid + 二进制路径，
/// 「停」那一步照样有凭据（走 [`kill_adopted`] 的身份核对）。
fn reap_detached(
    handshake: crate::backend_policy::Handshake,
    reader: crate::backend_policy::ReaderEnd,
) {
    let taken = {
        let mut g = DETACHED.lock().unwrap_or_else(|e| e.into_inner());
        g.as_mut().and_then(|h| h.child.take())
    };
    let Some(mut c) = taken else { return };
    std::thread::Builder::new()
        .name("ccm-detached-reaper".into())
        .spawn(move || {
            // `process_group` 不改变父子关系 ⇒ 不 `wait` 就留僵尸（`launch.rs:198` 逐字）。
            // ★ `K-P3b`：**`wait()` 回来那一拍就是这条路上唯一拿得到退出状态的时刻** ——
            //   在这里记账，不在别处猜。
            match c.wait() {
                Ok(status) => note_detached_death(status, handshake, reader),
                Err(e) => tracing::warn!(
                    "收不了脱离那个后端的尸（{e}）⇒ 这一次死亡的退出状态拿不到，账上不记 —— \
                     ⚠ 如实降级：编一个退出状态比不记更坏"
                ),
            }
        })
        .map(|_| ())
        .unwrap_or_else(|e| tracing::warn!("起不来收尸线程（{e}）⇒ 那个 pid 会留成僵尸"));
}

/// 把一次 `wait()` 的结果翻成 [`crate::backend_policy::Outcome`]。
///
/// ★ **信号号只有这一层取得到**：它要 `std::os::unix::process::ExitStatusExt`，
/// 而 `std::os::unix` 在 `backend/backend_tests.rs::the_backend_half_stays_platform_agnostic`
/// 的禁针里 ⇒ `backend/` 那半只能把 `ExitStatus` **原样**交上来
/// （`SuperviseEvent::Exited.status`），翻译落在这里。
///
/// `None` = 既没有退出码也没有信号号。那一格**没有事实可说** ⇒ 调用方出声、不上账；
/// 编一个 `Exited(0)` 顶上去正是 `platform/fallback_guard.rs` 治的那一族。
fn outcome_of_status(status: std::process::ExitStatus) -> Option<crate::backend_policy::Outcome> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return Some(crate::backend_policy::Outcome::Signalled(sig));
        }
    }
    status.code().map(crate::backend_policy::Outcome::Exited)
}

/// 脱离那条路上「它跟我们说过话没有」这一维。
///
/// ★ **它是从一个事实推出来的，不是一个孤零零的字面量**：入参就是那道门的产物。
/// [`attach_stream`] 开头两行是 `parse_frame` + `BackendHello::from_hello_frame`，
/// 任一给不出东西就 `Err` 返回、**根本进不到流循环**，也就没有人调得到 [`reap_detached`]。
/// ⇒ **拿得出一份 [`crate::backend::control::inbound_client::BackendHello`] = 一帧合法 hello 已经到手**，
/// 而那正是「它说过话」的定义。要这个参数是为了让这条推理**在类型上**成立：
/// 没有见证就调不出这个函数（见证的构造入口只有 `from_hello_frame` 一个，
/// 由 `inbound_client` 那条「见证不许凭空造」的判据钉着）。
///
/// ⚠ 前提哪天变了（有人让 `attach_stream` 不带 hello 也能进流循环），
/// 这一句就成了假的 —— 由 [`tests::the_detached_handshake_is_derived_from_the_hello_gate`]
/// 钉住那道门还在。
fn handshake_from_hello(
    _witness: &crate::backend::control::inbound_client::BackendHello,
) -> crate::backend_policy::Handshake {
    crate::backend_policy::Handshake::Spoke
}

/// 脱离那条路的死亡账。**三维各自的来历逐条写在这儿，一个都不是默认值。**
fn note_detached_death(
    status: std::process::ExitStatus,
    handshake: crate::backend_policy::Handshake,
    reader: crate::backend_policy::ReaderEnd,
) {
    let Some(outcome) = outcome_of_status(status) else {
        tracing::warn!(
            "脱离的后端退出状态里既没有退出码也没有信号号（{status:?}）—— \
             这一格没有事实可说，账上不记"
        );
        return;
    };
    let ev = crate::backend_policy::DeathEvidence {
        // ① `Outcome`：`wait()` 的 `ExitStatus`，信号号在这一层取（见 `outcome_of_status`）。
        outcome,
        // ② `Handshake`：由 `attach_stream` 那道 hello 门推出来（见 `handshake_from_hello`）。
        handshake,
        // ③ `ReaderEnd`：流循环两个出口各自带上来的（EOF / 那句读错误原样）。
        reader,
        // 这条路上进程真的起来过 ⇒ 没有「起不来」那一支的两个字段。
        start_failure: None,
    };
    shout_if_the_ledger_refused(crate::backend_policy::record_death(
        crate::backend::control::inbound_client::LOCAL_ORIGIN,
        &ev,
        &mut crate::backend_policy::MonitorLog,
    ));
}

/// 记完一笔之后**不许把 [`crate::backend_policy::Recorded`] 丢掉**。
///
/// `#[must_use]` 拦得住 `let _ =` 之外的忘记，拦不住「写了 `let _ =`」。
/// ⇒ 三处接线一律把它交到这里：落点拒收时**再喊一声**。
/// `record_death` 自己已经 `error!` 过一次 —— 这一声证的是**调用方没把它吞了**
/// （`KP3A` 死值验那一刀分开的正是「没写」与「写了但吞了错」）。
fn shout_if_the_ledger_refused(rec: Option<crate::backend_policy::Recorded>) {
    let Some(r) = rec else { return };
    if let Some(why) = &r.sink_error {
        tracing::error!(
            "死亡账写不进去（{why}）—— 调用方这一侧也喊一声，别让它只死在落点里：{}",
            r.line
        );
    }
}

/// 把一条已经认证过的连接接成入方向通道。
///
/// # 它与 `local_backend::local_stdio_consumer` 是同一件事的两种载体
///
/// 那一份吃 `ChildStdin`/`ChildStdout`，这一份吃一条 socket 的两半。
/// **复用的是纯零件**（`parse_frame` · `BackendHello::from_hello_frame` ·
/// `park_owned_writer` · `read_capped_line`），没有把一个绑传输的循环硬掰成泛型 ——
/// 那是 `local_stdio_consumer` 头注自己给的分寸。
fn attach_stream(sock: std::net::TcpStream, hello_line: &str) -> Result<(), String> {
    let frame = crate::ssh_source::parse_frame(hello_line)
        .ok_or_else(|| "hello 行解析不出帧".to_string())?;
    let witness = crate::backend::control::inbound_client::BackendHello::from_hello_frame(&frame)
        .ok_or_else(|| "首帧不是 hello ⇒ 拿不到见证，按契约不许登记通道".to_string())?;
    // ★ `K-P3b`：**「它说过话没有」这一维就在这一行变真的** —— 上面那道门给出见证的那一刻。
    //   下面把它一路带到收尸那一拍，而不是在那边写一个 `Handshake::Spoke` 字面量。
    let handshake = handshake_from_hello(&witness);
    sock.set_nonblocking(true)
        .map_err(|e| format!("socket 转非阻塞失败：{e}"))?;
    // 期限只属于**握手**那一段；进了流之后这条连接是长连接，装着期限反而会把它掐断。
    let _ = sock.set_read_timeout(None);
    let _ = sock.set_write_timeout(None);
    let (rd, wr) = tauri::async_runtime::block_on(async move {
        tokio::net::TcpStream::from_std(sock).map(tokio::net::TcpStream::into_split)
    })
    .map_err(|e| format!("socket 转 tokio 失败：{e}"))?;
    let client =
        crate::backend::control::inbound_client::park_owned_writer(wr).into_client(witness);
    crate::backend::control::inbound_client::register(
        crate::backend::control::inbound_client::LOCAL_ORIGIN,
        client.clone(),
    );
    tracing::info!(
        "本机入方向通道已登记（常驻载体）：origin={}",
        crate::backend::control::inbound_client::LOCAL_ORIGIN
    );
    tauri::async_runtime::spawn(async move {
        use crate::ssh_source::{CappedLine, BACKEND_FRAME_LINE_CAP};
        let mut reader = tokio::io::BufReader::new(rd);
        let mut buf: Vec<u8> = Vec::new();
        // ★ `K-P3b`：**我们这一侧的读端怎么结束的**。初值只在真读到 EOF 时才成立 ——
        //   下面那条 `Err` 支会把它换掉，两个出口各写各的。
        let mut reader_end = crate::backend_policy::ReaderEnd::CleanEof;
        loop {
            match crate::ssh_source::read_capped_line(&mut reader, &mut buf, BACKEND_FRAME_LINE_CAP)
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
                    // ★ `K-P3b`：那句错**原样**带到账上 —— 「读坏了」说的是我们这一侧，
                    //   而它**不算它崩了一次**（`B1` 那条错误诊断的全部内容）。
                    reader_end = crate::backend_policy::ReaderEnd::Broken(e.to_string());
                    break;
                }
            }
            let Ok(line) = std::str::from_utf8(&buf) else {
                continue;
            };
            let Some(f) = crate::ssh_source::parse_frame(line) else {
                continue;
            };
            // 本机的 tmux 帧（`P3` 刀 1）·〔SR1a〕应答 · 链路帧 —— 与 stdio 那条载体**同一个吸收点**，
            // 理由与前置条件写在 `local_backend::absorb_local_frame` 的头注上，这里不再抄一份散文。
            // 〔CF1〕交回来的内容帧送进本机内容通道 —— 这是 tokio 任务 ⇒ `.await` 那一形（满了就停读：级 1 回推）。
            if let Some(f) =
                crate::backend::control::local_backend::absorb_local_frame(f, Some(&client))
            {
                crate::local_lines::deliver(f).await;
            }
        }
        // 〔CF1〕告诉本机内容消费者这条流结束了。
        crate::local_lines::stream_ended().await;
        // 〔SR1a〕流没了 ⇒ 经它开的在飞链路全部带原因结束（不让调用方干等到超时）。
        crate::link_mux::fail_owned_by(&client, "本机后端的流断了（常驻载体）");
        // 〔SR1b〕经它开的传输也一律收场（后端的票表随那条流一起撤了）。
        crate::sftp_pool::fail_owned_by(&client, "本机后端的流断了（常驻载体）");
        // 流结束 ⇒ 摘掉登记，别在表里留一个写不进去的 client；那份陈旧的 tmux 原文也要清
        // （留着它 `find_tmux_origin_for_sid` 仍会回 `Some(<local>)` ⇒ 那个永远消不掉的灰点）。
        crate::backend::control::inbound_client::unregister(
            crate::backend::control::inbound_client::LOCAL_ORIGIN,
            &client,
        );
        crate::ssh_source::forget_tmux_raw(crate::backend::control::inbound_client::LOCAL_ORIGIN);
        // 收尸：`process_group` 不改父子关系，不 `wait` 就留 `Z`。**事件驱动，不是轮询。**
        // ★ `K-P3b`：两维证据一起交下去 —— 收尸那一拍才拿得到第三维（退出状态）。
        reap_detached(handshake, reader_end);
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
/// - `resolve_bin`：生产路径喂 [`resolve_backend_bin`]（exe 旁 → 内嵌释放）；
///   而判据喂一个现成的二进制 —— 否则那条真进程判据只能去动用户真实的 `~/.cc-monitor`。
/// - `extra_env`：生产路径喂**空表**；e2e 用它塞一条**前面挂着 shim 的 PATH**。
///   ⚠ 这一条不是方便，是 `C7i` 红线：被起的后端一上来就往它连得到的 tmux server
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
            });
        }
    };

    // ── ① 起时先认已有实例 ────────────────────────────────────────────
    //
    // ★★ 这一条是硬的：backend 一起来就**无条件**往它连得到的 tmux server 装三条全局 hook、
    //    **固定槽位 `[50]`**、**没有关掉它的开关**，载荷里烤着那一个后端的 pid+starttime。
    //    ⇒ **脱离而不认已有实例 = 每台机 N 个后端互相盖槽位，比今天更糟。**
    match adopt_existing(port, &home, &token) {
        Adopt::Attached => {
            let (pid, bin) =
                read_listen_owner(&dir, port).unwrap_or((0, std::path::PathBuf::new()));
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
                "本机后端没起来：{port} 口上那个后端接不上（{why}）。\
                 **下一步**：把那个进程停掉再重开 monitor —— 它的 pid 记在 {}。\
                 升级 monitor 之后最常见：口是按家目录算死的，而上一次留下的那个 backend \
                 版本对不上，于是新的这个既不会换口、也不会再起第二个。",
                pid_path(&dir, port).display()
            ));
            return DetachOutcome::Done(StartOutcome::Failed {
                reason: format!(
                    "{port} 口上有东西，但接不上它：{why}。\
                     **不会**再起第二个、也**不会**换个口 —— 那正是「每台机 N 个 backend\
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
                reason: format!("脱离的后端起来了却连不上它（{e}）⇒ 已把它收掉"),
                looked_at: vec![bin],
            })
        }
    }
}

/// 等刚起的那个后端把口 bind 上 —— **次数与间隔都有上限**。
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
/// **登记住址** `src/bridge/src/rust_timer_registry.rs`（那张表按类别收，`wait-for-condition`
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
///   这一格是真的：backend 那侧要等 `writer_task` 拿到写错误、`done` 信号回到 accept 循环，
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

/// 找那个二进制 —— **这一层只做适配，答案取自那一份共用的解析**
/// （`local_backend::resolve_or_extract`：先找 exe 旁边、再问产物自己带没带、再释放）。
///
/// # 🔴 `K-R43`：本函数**曾经是第二份手写实现**，今天不是了
///
/// 原来这里自己走一遍「找旁边 → 释放内嵌」，与 `local_backend::start_or_extract` 并列两份，
/// 中间只有 `the_two_resolution_paths_still_agree_on_the_order` **对拍顺序**。
/// ⚠ **那条判据眼皮底下真的漂过一次，而它全程绿**：`K-R42` 只给那一份接上了
/// 「问产物自己带没带」与「释放失败说一句分得开的话」，本函数一个字没动 ——
/// 顺序没变 ⇒ 判据没红，而同一台机上两条路对同一个失败给出了两句性质不同的话
/// （本函数那一句逐字是 `K-R42` 从对面删掉的那一句）。读数住件文件 `K-R43 §9`。
/// ⇒ 处置是**只留一份**，本函数降为适配器：把 `Resolved` 换成这条路要的 `Result`，
/// 并补上两样**宿主知识**（目标三元组常量 · `make_executable`）。
///
/// ⚠ **本层不许再自己拼失败串** —— 拼了就是第二份说法。由
/// `the_two_resolution_paths_still_agree_on_the_order` 钉着（它今天钉的是「两条路都走那一份」）。
fn resolve_backend_bin(
    extract_dir: &std::path::Path,
    embedded: Result<(&str, &[u8]), String>,
) -> Result<std::path::PathBuf, (String, Vec<std::path::PathBuf>)> {
    match local_backend::resolve_or_extract(
        env!("CCM_TARGET_TRIPLE"),
        extract_dir,
        embedded,
        // `backend-split` 的 C10：平台知识由宿主注入。
        &crate::platform_fs::make_executable,
    ) {
        Resolved::Found(p) => Ok(p),
        Resolved::Missing { reason, looked_at } => Err((reason, looked_at)),
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
    let same = exe == bin
        || std::fs::canonicalize(bin)
            .map(|c| c == exe)
            .unwrap_or(false);
    if !same {
        return Err(format!(
            "pid={pid} 现在跑的是 {}，不是我们的 backend ⇒ **不动它**（pid 被复用了）",
            exe.display()
        ));
    }
    // 只发 SIGTERM：backend 自己有停机路径（`shutdown_signal`），SIGKILL 会跳过它。
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
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    let mut cmd = std::process::Command::new("kill");
    cmd.arg("-TERM")
        .arg(pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null());
    // 三条策略（`00 §1.5.2`）：`Hidden`（本函数只在 Linux 上编译，这一格是空的，
    // 但**得有人回答**）· `JobKillOnClose`（就地等它退，别留后代）·
    // `Null`（`kill(1)` 的抱怨这里用不上：结论在退出码里，下面那句 `退出码 {st}` 就是它）。
    let st = spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::JobKillOnClose,
        StderrSink::Null,
    )
    .and_then(|c| c.wait_for_status())
    .map_err(|e| format!("起不来 kill：{e}"))?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("kill -TERM {pid} 退出码 {st}"))
    }
}

/// backend 那条监护路的 `on_event` —— ★ **判与记都在这个闭包里**。
///
/// # 为什么不落进 `supervise_with_stdio` 体内
///
/// 死亡账是「**这台机的 backend**」的账，监护器本身是通用的（收 `bin` / `args` / `envs`）⇒ 落进监护器体内，
/// 它就得多一个「你在监护谁」的概念。⇒ 接线落在**客户这一侧**。
/// 〔RL1 · V107〕这条理由先前还有一半是「监护器有两种客户（后端与中转）」—— 中转并进常驻后端之后，
/// 生产上的客户只剩这一条；理由的另一半（监护器不认识客户）照旧成立。
/// 由 `backend_policy::tests::the_supervisor_itself_never_records_a_death` 钉着。
///
/// # 为什么是一个**返回闭包的函数**，而不是内联在下面那个调用里
///
/// 内联的闭包测试够不着 ⇒ `KP3W3` 那条「三只假后端各判成哪一格」就只能读源码。
/// 抽出来之后，判据可以拿**同一个闭包**跑一遍真进程
/// （[`tests::three_fake_backends_land_in_three_different_cells`]）。
fn backend_supervise_events() -> std::sync::Arc<dyn Fn(local_backend::SuperviseEvent) + Send + Sync>
{
    std::sync::Arc::new(|e| {
        let (code, attempt, status, witness) = match e {
            local_backend::SuperviseEvent::Exited {
                code,
                attempt,
                status,
                witness,
            } => (code, attempt, status, witness),
            other => {
                tracing::info!("本机后端: {other:?}");
                return;
            }
        };
        tracing::info!("本机后端: 第 {attempt} 次那一命结束（code={code:?}）");
        // ① 两维证据：只认**观测到的**那一档。
        let (handshake, reader) = match witness {
            local_backend::StreamWitness::Observed { handshake, reader } => (handshake, reader),
            unwitnessed => {
                // 到这里说明这一命没有消费者、或者消费者自己 panic 了 ——
                // 两种情况下「它说过话没有」都**没有被观测过**，而那正是 2026-07-09
                // 判别式的第二个条件。⇒ **出声，不上账**：编一个 handshake 顶上去，
                // 判出来的「被拒了 / 崩了」是假的。
                //
                // ⚠ 如实登记的降级：这一命的死亡**不会出现在账上**。
                // backend 这条路今天恒有消费者（`start_or_extract` 无条件传
                // `Some(Arc::new(local_stdio_consumer_guarded))`，那条线由
                // `the_production_entry_hands_the_stdio_consumer_down` 钉着）
                // ⇒ 生产上只有「消费者 panic」那一形到得了这里，而那一形兜底外壳已经
                // LOUD 记过一条。真常来 ⇒ 回来重判，别在这里补一个默认值。
                tracing::error!(
                    "本机后端这一命结束了，而那两维证据没有观测者（{unwitnessed:?}）—— \
                     死亡账这一笔不记：拿一个没人观测过的「说过话没有」去判，\
                     判出来的是编的（2026-07-09 那次死循环的判别式就是它）"
                );
                return;
            }
        };
        // ② 退出状态：`code` 在被信号打死时是 `None`，信号号在 `status` 里，
        //    而翻译它要 `ExitStatusExt`（宿主层的事，见 `outcome_of_status`）。
        let Some(status) = status else {
            tracing::error!("本机后端这一命结束了，而收尸没拿到退出状态 —— 账上不记");
            return;
        };
        let Some(outcome) = outcome_of_status(status) else {
            tracing::error!(
                "本机后端的退出状态里既没有退出码也没有信号号（{status:?}）—— 账上不记"
            );
            return;
        };
        let ev = crate::backend_policy::DeathEvidence {
            outcome,
            handshake,
            reader,
            // 这条路上进程真的起来过 ⇒ 没有「起不来」那一支的两个字段。
            start_failure: None,
        };
        shout_if_the_ledger_refused(crate::backend_policy::record_death(
            crate::backend::control::inbound_client::LOCAL_ORIGIN,
            &ev,
            &mut crate::backend_policy::MonitorLog,
        ));
    })
}

/// 「从来没起来」那条臂的**唯一生产喂点**：[`start_local_backend`] 的两个出口都从这里过。
///
/// ⚠ `reason` / `looked_at` **原样转**，不另写一份 —— 本文件那一族逐字的纪律：
/// 「两份措辞迟早对不上」。账上那一行里的原因与 `StartOutcome::Failed` 手上的那一句
/// **逐字相同**，由 `the_never_started_reason_is_the_same_string_the_caller_gets` 钉着。
fn note_never_started(out: StartOutcome) -> StartOutcome {
    if let Some((reason, looked_at)) = out.failure() {
        let ev = crate::backend_policy::DeathEvidence {
            outcome: crate::backend_policy::Outcome::NeverSpawned,
            // 进程一次都没存在过 ⇒ 它当然一个字节都没说过。这不是猜，是那条路的定义。
            handshake: crate::backend_policy::Handshake::NeverSpoke,
            // ★ **这条路上根本没有读端** ⇒ 明写「没观测到」，不冒充一次干净 EOF。
            reader: crate::backend_policy::ReaderEnd::NotObserved,
            start_failure: Some((reason.to_string(), looked_at.to_vec())),
        };
        shout_if_the_ledger_refused(crate::backend_policy::record_death(
            crate::backend::control::inbound_client::LOCAL_ORIGIN,
            &ev,
            &mut crate::backend_policy::MonitorLog,
        ));
    }
    out
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
    // 之后按「停」只 take 到 h2 ⇒ `backend_status` 回 `channel: true` / `pid: null`，
    // **「状态说停了」与「进程真没了」当场分叉** —— 正是 `P2s-Y2` 自陈要守的那件事，
    // 而 Y2 的实测走的是被绕开的另一条路径（它自己 spawn，不经本函数）。
    //
    // # 现在的不变量
    //
    // **句柄在表里 = 在跑**（`stop_local_backend` 会把它 `take` 走）⇒ 判据不再问 pid。
    // 并且**锁横跨整个起的过程**，把「检查」与「存句柄」之间那个窗口关掉。
    //
    // ⚠ 代价如实登记：`extract_embedded_to` 会在持锁期间同步写 ~10MB，
    // 期间 `backend_status` / `backend_stop` 会短暂阻塞。这是**用一次可见的等待换掉一个
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
    //   · 这台机器的 (OS, arch)：`byte_table::choose` 按它挑内嵌字节（下面那一段）；取不到时交进去的是那句拒绝的话，
    //     函数把它接在「旁边没有」后面，不伪造理由。
    let extract_dir = dirs::home_dir()
        .map(|h| h.join(".cc-monitor").join("bin"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.cc-monitor/bin"));
    // 〔DP1 · 第四波〕**本机的字节也按 (OS, arch) 从那一张表里取**（`设计/01 §6.7a` 规矩 4：本机只是「目标机器恰好是自己」）。
    //
    // 〔墓碑 —— 这里原来是一道 `cfg!(target_os = "linux")` 的闸（D 阶段补审 08-11）：远端那两份 musl 只按 arch 取、不认 OS，
    //  于是在 Windows 构建上会释放一个 Linux ELF、再报「已起」；那道闸挡住了它，代价是非 Linux 本机一份字节都拿不到，
    //  由 `local_backend` 那一层再问一次「这份产物自己带没带」补上。`设计/96 §7.1.3`：「这道闸要消失 —— 它是
    //  『表没有 OS 轴』逼出来的补丁」。〕今天 `byte_table::choose` 按这台机器的键查表：Windows 那一格就是这一份产物
    //  按 `TARGET` 内嵌的那份，Linux 那一格是 musl（开发树只有原生那份时给原生那份），不承诺 / 没带 ⇒ 那句拒绝的话。
    let this_machine = crate::byte_table::Key::this_machine();
    let embedded: Result<(&str, &[u8]), String> = crate::byte_table::choose(
        crate::byte_table::Product::Backend,
        crate::byte_table::Route::Local,
        this_machine,
    )
    .map_err(|r| r.say(crate::byte_table::Product::Backend, "本机"))
    .and_then(|p| match p.build_id {
        Some(id) => Ok((id, p.bytes)),
        None => Err("这一份本机后端的字节说不出自己是哪一版，没有拿它来起".to_string()),
    });
    // ★★ `K-P1`：**先走常驻那条路** —— 认得出已有实例就接上它，没有就起一个脱离的。
    //
    // 这就是「怎么起」那个注入点：`start_detached` 是**这一层**（宿主知识层）的东西，
    // 它认识 `process_group(0)` / `/proc` / `~/.cc-monitor`，而 `backend/` 那半一样都不许认识
    // （`the_backend_half_stays_platform_agnostic` 的禁针含 `std::os::unix`）。
    // 走不了（平台不支持 / `CCM_NO_DETACH` 关掉了）才回落到下面那条今天的路。
    // 〔RL1 · V107〕**中转不再是 monitor 另起的第三个进程**：它住本机常驻后端进程里，
    //   由后端自己 `bind`（`relay::listen::host`）。monitor 这一侧只剩一件事 —— 起后端时把
    //   端口与凭据路径交给它（[`relay_host_envs`]），**两条载体同一份**（常驻 / 被监护的 stdio）。
    let relay_envs = relay_host_envs();
    // 〔DP1〕`embedded` 里可能是那句拒绝的话（`String`），不再是 `Copy` ⇒ 常驻那条拿一份拷贝。
    let resolve = || resolve_backend_bin(&extract_dir, embedded.clone());
    match start_detached(&resolve, &relay_envs) {
        // ★ `K-P3b`：这是本函数**两个**返回 `Failed` 的出口之一 —— 都从 `note_never_started` 过。
        DetachOutcome::Done(out) => return note_never_started(out),
        DetachOutcome::NotTaken => {}
    }
    let (resolved, sup) = local_backend::start_or_extract(
        env!("CCM_TARGET_TRIPLE"),
        &extract_dir,
        embedded,
        // `backend-split` 的 C10：平台知识由宿主注入，backend 那半不认识 `#[cfg(unix)]`。
        &crate::platform_fs::make_executable,
        // ★ `K-P3b`：backend 这条监护路的死亡账**就记在这个闭包里**（见它的头注）。
        backend_supervise_events(),
        // ★ `15 §5.1 A3`：起进程那一下的三条答案由**宿主**给（backend 那半不认识平台）。
        crate::spawn_managed::local_backend_supervised(),
        // 〔RL1〕与常驻那条路交的是**同一份**（上面那个 `relay_envs`）。
        relay_envs,
    );
    if let Some(h) = sup {
        *g = Some(h);
    }
    // ★ `K-P3b`：另一个返回 `Failed` 的出口，同样从 `note_never_started` 过。
    note_never_started(match resolved {
        Resolved::Found(p) => StartOutcome::Started(p),
        Resolved::Missing { reason, looked_at } => StartOutcome::Failed { reason, looked_at },
    })
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

// ════════════════════════════════════════════════════════════════════════════
// 〔RL1 · V107〕**本机中转住本机常驻后端进程里** —— monitor 只交端口与凭据路径
// ════════════════════════════════════════════════════════════════════════════
//
// 先前这里是 `K-H2b` 那一族：monitor 起本机后端那一刻**另监护一个** `--relay` 子进程（第三个进程），
// 「停」按钮与退出臂各收它一次，「中转在不在」问的是 monitor 自己内存里那张句柄表
// （`真相源/70 §7b`：跨 monitor 重启认不出上一次那一个 ⇒ 孤儿必然，「孤儿总是中转」）。
// 用户 2026-09-24 裁 V107「中转 ＋ 上游选择住本机常驻后端进程，对外端口由它绑；
// monitor 不再单独起 / 收中转（本机固定两个进程）」⇒ 那一族整个删掉：
// - 起：后端流模式进程被交了端口就在本进程里起（`src/backend/relay/listen.rs::host`）；
// - 收：随常驻后端按「退出行为」留或退（`设计/01 §3.3b`），monitor 一行都不管；
// - 在不在：回环上连一次那个口（[`relay_running`]），与远端 `relay-status` 同一个判准。

/// 〔RL1〕起本机后端时交给它的那份**环境**：中转端口 ＋ 凭据文件路径。**两条载体交的是同一份**
/// （常驻那条 `start_detached` 的 `extra_env` · 被监护那条 `local_backend::start_or_extract` 的 `envs`）。
///
/// ★ 端口**显式交**：注入侧（`payload::RELAY_PORT`）与后端里 bind 的是同一个值；后端那一侧**没有缺省值**
///   （交了认不出的串就不开中转），它的 `DEFAULT_PORT` 只属于独立 `--relay` 那一形。
/// ★★ 凭据路径也显式交（`D1 阻-3` 那条理由原样）：不交的话，后端里的上游选择走它自己那条
///   `resolve_path` → `resolve_home()`，而那一条认 `CLAUDE_CONFIG_DIR`；monitor 写的那份**不跟随**它
///   （`creds_store::resolve_path` 头注）⇒ 两侧读写两份文件，症状是「界面上配好了，上游选择说没配」。
///   由**写那份文件的那一侧**把路径说出来 —— `CCM_DATA_DIR` 隔离跑时读者因此跟着 monitor 的数据目录走。
///
/// ⚠ 拿不到家目录时凭据那一格**缺席**（不是空串）：后端那时退回它自己那条解析，
/// 而那正是上面那个静默 404 的成因 ⇒ 缺席这一格不许被读成「安全」。
pub(crate) fn relay_host_envs() -> Vec<(String, String)> {
    let mut envs = vec![(
        "CCM_RELAY_PORT".into(),
        crate::backend::control::payload::RELAY_PORT.to_string(),
    )];
    if let Some(p) = crate::creds_store::resolve_path() {
        envs.push(("CCM_APIKEY_CREDENTIALS".into(), p.display().to_string()));
    }
    // 〔C4d · 第四波 4B〕历史注解的读写者换成本机常驻后端（主会话 09-25 裁：文件留在原处、同一路径）——
    //   同上一格的理由：由知道那份文件在哪的那一侧把路径说出来（值就是 monitor 从前读写它的那一个函数算的）。
    //   拿不到数据目录时这一格缺席 ⇒ 后端那一侧明说「不知道注解文件在哪」，不猜。
    if let Some(p) = crate::history::metadata_path() {
        envs.push(("CCM_HISTORY_METADATA".into(), p.display().to_string()));
    }
    envs
}

// 〔US1 · 第四波 4D〕`relay_running` / `relay_listening_at`〔散文墓碑〕退役：本机中转在不在由本机常驻后端自己答
//   （`launch-endpoint` · `apikey-routing` 的成品里那一格，判准是 RK1 的差分探针 —— 「口上有人 ≠ 我们的中转」那条诚实边界随之收掉）。

/// P2s（`C8`②）：停本机后端。**句柄取走**（`take`）而不是留着 ——
/// `stop()` 之后那个句柄就是死的（`stopping` 永久置位），留着只会让下一次「起」
/// 误以为还在跑。
pub fn stop_local_backend() -> Result<String, String> {
    let mut g = LOCAL_BACKEND.lock().map_err(|e| format!("锁毒化: {e}"))?;
    // 〔RL1 · V107〕中转住本机后端进程里 ⇒ 停后端就是停中转，这里不再另收一个。
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

// ══ 取 shim 那个唯一入口的**跨文件出口**〔`K-R7` 09-01，`§0q` 裁一 · 出路乙〕════
//
// `backend/control/local_backend.rs` 的三个落点也要走这个口 ⇒ 这个测试模块必须 `pub(crate)`，
// 它们按 `crate::local_backend_host::tests::demand_tmux_shim(..)` 取。
//
// 〔`K-R76` 09-12〕**这里原先多一道绕道，现在拆掉了**：模块写成私有 `mod tests`，再在
// 文件末尾补一行 `pub(crate) use tests::demand_tmux_shim;` 重导出一次。那道绕道**不是随手写的**，
// 它当时有一个真理由 —— 09-01 实打，直接写成 `pub(crate) mod tests` **当场打红 17 条**
// （`cargo test -p monitor --lib`：`1194 passed; 17 failed; 13 ignored`；`guard-core` 的反向
// 自检逐字「剥完仍残留 23 个测试属性 —— 剥法坏了」）：当时 `guard_core::test_module_ranges`
// 按**字面前缀**认 `mod `，`pub(crate) mod tests {` 过不了那一关 ⇒ 整段测试代码被当成
// 生产段，各条扫生产段的守卫跟着去扫测试代码。
// `K-R75`（09-12）把那一步换成**按形状**剥可见性修饰（`guard_core::strip_visibility`）之后
// **那个理由不再成立** ⇒ 绕道拆掉，模块写回它本来的样子。
//
// 🔴 **上一版这里指的是一个裸行号**（`guard-core/src/lib.rs` 加一个数字）——
//   那份文件 09-12 一动那个数就漂了，而**漂了没有任何东西会说话**（本区 `[J5 审计正文]` 那一族）。
//   ⇒ 今天指的是**函数名 ＋ 一段机检着的逐字校验位**，不是行号：剥法住
//   `src/bridge/crates/guard-core/src/lib.rs` 的 `test_module_ranges`；校验位（那一行的原文）
//   **只有一个家** —— 下面 [`tests::the_strip_rule_this_file_leans_on_is_still_on_disk`]
//   里那个 `PIN`，它在盘上找不着就当场红。
#[cfg(test)]
#[path = "../../../tests/bridge/local_backend_host_tests.rs"]
pub(crate) mod tests;
