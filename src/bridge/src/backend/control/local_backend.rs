//! F05a：**本机后端进程的「起与看住」**（定框 C7：没有 daemonless，本机也要有后端进程）。
//!
//! # 这一层是什么、不是什么
//!
//! 它是 C13 两半拆法在「起一个本机后端」上的落地：
//!
//! - **决策那半住这里**：去哪儿找那个二进制（[`resolve_with`]）· 崩了要不要再起（[`decide`]）。
//!   两条都是**纯函数**，不碰进程、不碰时钟 —— 时间由调用方喂进来。
//! - **宿主知识留调用方**：exe 目录在哪、target triple 是什么，由 `lib.rs` 那一侧给。
//!   本模块过 `backend::tests::the_backend_layer_stays_host_agnostic`（禁 `AppHandle` 等把手）。
//!
//! 用 `std::process::Command`，**刻意不引入 `tauri-plugin-shell`**：
//! 摸底实测 monitor 今天的 tauri 插件只有 opener/dialog/notification/single-instance，
//! 而 Tauri 的 sidecar API 要 `AppHandle` —— 那会让本模块**当场违反**宿主无关那条机检。
//! `externalBin` 只是**打包器**的事（把二进制放进安装包并按 triple 命名），与「怎么起它」无关。
//!
//! # ★ 为什么没有「退避睡眠」
//!
//! 崩溃循环的常规解法是 `sleep(backoff)` —— 而那正是定框 C12 要消灭的「**自己醒过来**」的构件。
//! monitor 的 Rust 侧今天**没有**对应护栏（`polling_registry` 只管 TS 与 `shared/ccm`，
//! 它的头注逐字写着 monitor 的 Rust 侧「如实登记为未做，不假装覆盖了」）——
//! 也就是说这里写一个 `sleep` 不会红。**恰恰因此更该自己守住。**
//!
//! 换成**窗口内计数上限**：崩溃时刻记进一个账，`window_ms` 内崩了 `max_crashes` 次
//! ⇒ [`Decision::GiveUp`]，否则**立刻**重起。天花板是「`max_crashes` 次立即重试」，
//! 不会无限自旋，而且**一个定时器都不需要**。
//! ⇒ 🔴 **这一格有人量过**（`K-R30` 09-06，**沙箱读数、真机未验**）：读数与量具住址住下面 [`SPAWN_ETXTBSY_TRIES`] 头注的出处那一节 —— 它只买下「这条取舍在**这一格**上还有一个此前没写过的好处」，**不是**「`C12` 处处成立」。
//!
//! # ★ 等它死：读 stdout 到 EOF，不是 `wait()`、更不是 `try_wait()` 轮询
//!
//! 这里有一个具体的所有权难题，值得写下来：`Child::wait()` 要 `&mut Child`，
//! 于是**等待线程必须独占 `Child`**；那 [`SuperviseHandle::stop`] 就再也拿不到它去 `kill`。
//! 三条路各有代价：
//!
//! | 路 | 代价 |
//! |---|---|
//! | `try_wait()` 轮着看 | **那是轮询** —— 违反 C12，本模块的立身之本就没了 |
//! | 引 `libc::kill` / spawn 一个 `kill` 命令 | 为了一个 `stop()` 引入平台 cfg（`backend-split` 的 C10）或多起一个进程 |
//! | ⭐ **读子进程 stdout 到 EOF** | 进程一死管道就 EOF，**事件驱动**；而 `Child` 本体可以留在 `Mutex` 里给 `stop()` 用 |
//!
//! 选第三条。`stdout` 是一个**独立的 owned handle**（`child.stdout.take()`），
//! 拿走它之后 `Child` 仍可锁着共享 ⇒ 等待与 kill 互不打扰，零平台代码，零轮询。
//!
//! ⚠ **它的诚实边界**：如果子进程**关掉 stdout 但继续活着**，本模块会误判它死了。
//! 被监护的对象是我们自己的后端（`--tail-only` 持续往 stdout 写帧），不会这么干；
//! 换成别的程序前要重新想。EOF 之后仍会 `wait()` 收尸（那时它已经死了，不阻塞）。
//!
//! # ⚠ 它只认安装包里那一份，**绝不扫仓库 dev 产物** —— 这是刻意的
//!
//! 摸底量到一件安全相关的事：backend 一启动就**无条件**往它能连到的 tmux server 上装三条
//! 全局 hook（`observe/watcher.rs::install_tmux_hooks_best_effort` → `set-hook -g`），
//! **而且没有关掉它的开关**。所以「顺手在 dev 环境里扫到 `target/debug/cc-monitor-backend`
//! 就起它」会去改用户真实 tmux server 的状态。
//!
//! ⇒ [`resolve_with`] **只认打包进安装包的那一份本机后端**（exe 同目录、按 target triple 命名），
//! **不扫仓库里的 dev 产物**；找不到时是 [`Resolved::Missing`] 的**诚实降级**
//! （定框 §5：tagged + `reason`，不是 `Err`），零副作用。
//!
//! 🔴 **订正（2026-09-10 现打，v3.7.0）**：本段原话「今天安装包里还没有那个文件
//! （`externalBin` 是 **F05b**）⇒ 生产路径**恒走**降级」——**两句今天都不成立**。
//! F05b 已经做完并随 v3.7.0 发出去了：`externalBin` 配在
//! `src/bridge/tauri.sidecar.conf.json`，发版那一步用
//! `npx tauri build --config src/bridge/tauri.sidecar.conf.json` 注入（`.github/workflows/release.yml`）。
//! ⚠ 它**刻意不进基础 `tauri.conf.json`** —— 进了会让 `cargo test` 也要求当前 target
//! 的那份二进制存在（那条头注住 `release.yml` 的 `tauri build` 那一步）。
//! ⇒ **「基础配置里没有」≠「没配」，别再把这两句写成一句。**
//!
//! 干净 win11 虚拟机上现打（PM，09-10，真安装包 + 真裸 exe 各一趟）：
//! **装出来那份** `C:\Program Files\cc-monitor\` 下 `cc-monitor-backend.exe` **2 个进程在跑**；
//! **裸 `monitor.exe`** 那份 **0 个**。⇒ 走 [`Resolved::Found`] 还是 [`Resolved::Missing`]，
//! 取决于**用户手里是哪一份产物**，不再是一个常数。
//! **C7 由 F05a + F05b 两件共同满足**，ROADMAP §3 就是这么记的 —— 两件今天都在了。
//!
//! 🔴 **`K-R42`（09-10 同日，上面那次读数之后）：上面那句「取决于哪一份产物」被这一件改小了。**
//! 那次读数**没有被推翻**（它量的是 v3.7.0 的产物，那一版的裸 exe 确实是 0 个）——
//! 变的是**它之后的机制**：本模块这条路今天多了第二个二进制来源
//! （`build.rs::embed_native_backend` 按 `TARGET` 嵌进来的那一份；〔DP1〕今天与其余几份一起由宿主经
//! `byte_table::pick` 按这台机器的 (OS, arch) 取来，本层只收字节），
//! 于是 [`resolve_with`] 的 `Missing` **不再等于「这台机器上没有本机后端」**，
//! 它只等于「**旁边**没有」。裸 exe 那一支从此走的是「自己释放一份再起」。
//! ⚠ 三句话别混：① 旁边有没有（[`resolve_with`]）· ② 这份产物带没带（宿主交进来的 `embedded`）
//! · ③ 放不放得下来（[`extraction_failure_reason`]）。09-10 那一形的病根就是把三件事说成一件。
//!
//! 真进程行为由 `tests/e2e/local-backend-supervise.sh` 验：它**显式**把二进制路径喂给
//! [`supervise_with_stdio`]，并强制私有 tmux 隔离，绝不碰用户真实 tmux server。
//! ⚠ 〔`P0e` 08-12〕隔离**换过机制**：原来靠私有 `TMUX_TMPDIR`，而 `$TMUX` 一有值就压过它
//! （08-11 就是这么打没用户 9 个真实会话的）⇒ `C7i` 逐字禁掉那条路。
//! 现在给后端一条**前面挂着 shim 的 PATH**（`tests/e2e/tmux-shim.sh`），它 shell out 的 tmux
//! 被强插 `-L` —— **显式选择器压得过 `$TMUX`**。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// 本机后端的基名。Tauri 的 `externalBin` 会把它按 `<基名>-<target-triple>[.exe]`
/// 放到 app 可执行文件旁边。
pub const LOCAL_BACKEND_STEM: &str = "cc-monitor-backend";

/// F06b-1：**monitor 告诉 `ccm` 「backend 二进制在哪」的那个 env 名**。
///
/// # 为什么用 env（三条路里裁的第 ③ 条）
///
/// `ccm` 是终端里的一次性 bash，它够不着 monitor 的 [`resolve_beside_this_exe`]。三条路：
/// ① ccm 自己实现一份「找本机后端」⇒ **第二份实现**（定框 §4 逐字禁）·
/// ② 装 ccm 时写进配置 ⇒ 要新机制（ccm 有 SFTP 部署 / 手装 / 仓内相对路径三条安装路径）·
/// ③ **monitor 拼 env 时告诉它** ⇒ 零新机制（monitor 本来就在拼 env 前缀）。
///
/// ⚠ ③ 的代价如实记：**用户手敲 `cc` 时没有这个 env** ⇒ 那条路**保持今天的本地行为**
/// （诚实降级，不是报错）。
///
/// ⚠ **这个名字只有一个家** —— `shared/ccm` 读的必须是同一个字面量，
/// 由 `the_backend_bin_env_name_has_exactly_one_home` 钉住（定框 §4）。
///
/// ⚠ **ccm 那一半已接**〔F06b-1c〕：旧 `shared/ccm` 的 `resolve_from_backend` 〔散文墓碑〕（函数，exec 路用）
/// 与 `resolve_recipe`（文本，print 路用），照该文件里 `derive_bus_id`/`BUS_ID_RECIPE` 的先例写；
/// 一致性由 `tests/e2e/ccm-contract-parity.sh` 的 **A′/A′d 组**钉住（print↔exec 的 argv 差分）。
///
/// ⚠ **monitor 这一半还没接**：本 `const` 今天**没有生产调用点**（只有判据读它，
/// 于是 `dead_code` 警告仍在 —— 那个警告就是「没接上」的诚实标记，刻意不 `#[allow]`）。
/// 挡在前面的是一条**架构题**，不是工作量：`payload.rs` 编译的 `EnvOp` 由 **TS 前端经 wire
/// 送来**（`launch_wire.rs`），而后端路径是**后端的知识**（[`resolve_beside_this_exe`]）
/// ⇒ 让前端携带它正对着 C2 与 C9。裁这道题是 F06b-1d 的第一件事。⇒ resume 路不能把
/// backend 的答案烤进打印串，得打印一段「执行时去问后端」的配方。
pub(crate) const BACKEND_BIN_ENV: &str = "CCM_BACKEND_BIN";

/// 找二进制的结果。**tagged 而不是 `Result`** —— 定框 §5：「拿不到依赖」是诚实降级。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Found(PathBuf),
    /// 没找到。`looked_at` 必须**逐个列出找过的路径** —— 只说「没找到」的诊断等于没有诊断。
    Missing {
        reason: String,
        looked_at: Vec<PathBuf>,
    },
}

/// 本机后端的候选路径。**只在 exe 同目录找**，理由见模块头注（不扫仓库 dev 产物）。
///
/// 两个候选：Tauri `externalBin` 的 triple 后缀形态，以及 bundler 剥掉后缀后的裸名
/// （两种形态都出现过，取决于打包器版本；**都列出来比猜一个强**）。
pub fn local_backend_candidates(
    exe_dir: &Path,
    target_triple: &str,
    exe_suffix: &str,
) -> Vec<PathBuf> {
    vec![
        exe_dir.join(format!("{LOCAL_BACKEND_STEM}-{target_triple}{exe_suffix}")),
        exe_dir.join(format!("{LOCAL_BACKEND_STEM}{exe_suffix}")),
    ]
}

/// 纯函数版的解析：`exists` 由调用方注入，便于单测不碰文件系统。
pub fn resolve_with(
    exe_dir: &Path,
    target_triple: &str,
    exe_suffix: &str,
    exists: &dyn Fn(&Path) -> bool,
) -> Resolved {
    let cands = local_backend_candidates(exe_dir, target_triple, exe_suffix);
    for c in &cands {
        if exists(c) {
            return Resolved::Found(c.clone());
        }
    }
    Resolved::Missing {
        reason: format!(
            "这一份 monitor 旁边没有本机后端（`{LOCAL_BACKEND_STEM}`）。\
             它**随安装包一起发**（`*-setup.exe` / `*.msi` 里都带，装完与 monitor 同目录）。\
             ⚠ **「旁边没有」不等于「这台机器上没有本机后端」**：产物里内嵌了本机后端时，\
             monitor 会自己释放一份再起它 —— 那一步走没走成由**它自己**报，不由本行断言。\
             ⇒ 两条都没有时的下一步：**装一次安装包**（Releases 页）。远端功能不受影响"
        ),
        looked_at: cands,
    }
}

/// 崩溃频率上限。**不是退避** —— 见模块头注「为什么没有退避睡眠」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrashLimits {
    pub max_crashes: u32,
    pub window_ms: u64,
}

impl Default for CrashLimits {
    fn default() -> Self {
        // 3 次 / 10 秒：一个真的起不来的二进制会在 3 次立即重试内被判死，
        // 而一次偶发崩溃（被 OOM、被人手动 kill）不会触发放弃。
        Self {
            max_crashes: 3,
            window_ms: 10_000,
        }
    }
}

/// 崩了之后干什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 立刻重起（**没有 `after_ms`** —— 本模块刻意不产生任何需要定时器的东西）。
    Restart,
    GiveUp {
        reason: String,
    },
}

/// 纯决策：`crash_times_ms` 是**已经发生过的**崩溃时刻（含刚刚这一次），`now_ms` 是现在。
///
/// 判据：窗口 `[now - window_ms, now]` 内的崩溃数 ≥ `max_crashes` ⇒ 放弃。
/// ⚠ **用 `>=` 不是 `>`**：`max_crashes = 3` 意思是「崩到第 3 次就别再起了」，
/// 而不是「第 4 次才放弃」。这条差一位的错在本仓出现过，所以单测里逐个边界都钉。
pub fn decide(crash_times_ms: &[u64], now_ms: u64, limits: CrashLimits) -> Decision {
    let floor = now_ms.saturating_sub(limits.window_ms);
    let recent = crash_times_ms.iter().filter(|t| **t >= floor).count() as u32;
    if recent >= limits.max_crashes {
        return Decision::GiveUp {
            reason: format!(
                "本机后端在 {}ms 内崩了 {recent} 次（上限 {}）⇒ 放弃重起，\
                 避免崩溃循环。远端功能不受影响；日志里有每次的退出状态",
                limits.window_ms, limits.max_crashes
            ),
        };
    }
    Decision::Restart
}

/// 消费者这一侧观测到的两维证据 —— **只搬观测，不做判断**。
///
/// # 为什么这一维要有一个「没观测到」的档〔`K-P3b KP3W2`〕
///
/// `supervise_with_stdio` 有两种客户：**接了消费者**的（backend —— 有人解帧、有人读错误）
/// 与**没接消费者**的（中转 —— 缺省那支只把 stdout `io::copy` 进 `sink`）。
/// 后者身上「它跟我们说过话没有」这件事**一次都没有被观测过**。
///
/// ⚠ 把它填成「观测到它没说话」是**假证据**，而那正是 2026-07-09 那次事故的判别式：
/// 「非零退出 **且** 从来没说过话」= 被拒了，两个条件缺一不可。
/// 用一个没人观测过的值去顶第二个条件，判出来的「被拒了」是编的。
/// ⇒ 这里给它一个**明写「没观测到」**的档，由宿主层决定拿它怎么办
/// （今天：backend 那条路上不该出现它，出现了就出声、不上账）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamWitness {
    /// **这条路上没有消费者**（缺省那支）⇒ 两维都没有观测者。
    NoConsumer,
    /// **消费者自己没了**（体内 panic 被兜底外壳接住）⇒ 它手里那两维随它一起没了。
    ConsumerGone,
    /// 消费者交回来的两维。类型取自 [`crate::backend_policy`]（判据那一侧的**同一份**表示）——
    /// 在这里另造一套平行的 `bool` + `Option<String>` 就是同一个事实的第二份表示，
    /// 而两份表示会漂。
    Observed {
        handshake: crate::backend_policy::Handshake,
        reader: crate::backend_policy::ReaderEnd,
    },
}

/// 监护器对外说的话。**调用方决定怎么呈现** —— 本模块不 emit 任何事件（宿主无关）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuperviseEvent {
    Started {
        pid: u32,
        attempt: u32,
    },
    Exited {
        code: Option<i32>,
        attempt: u32,
        /// ★ `K-P3b`：收尸拿到的**原样退出状态**。
        ///
        /// `code` 在**被信号打死**时是 `None`，**信号号已经丢了** —— 而
        /// 「崩了」那一格恰恰要它（`backend_policy::exit_status` 逐字打 `signal N`）。
        /// 取信号号要 `std::os::unix::process::ExitStatusExt`，而
        /// `the_backend_half_stays_platform_agnostic` 的禁针含 `std::os::unix`
        /// ⇒ **这一层只能原样把它交上去**，由宿主层取。
        /// `None` = 连 `wait()` 都没成功（子进程句柄已被别处摘走）。
        status: Option<std::process::ExitStatus>,
        /// ★ `K-P3b`：消费者这一侧观测到的两维。见 [`StreamWitness`]。
        witness: StreamWitness,
    },
    GaveUp {
        reason: String,
    },
}

/// 监护句柄。
pub struct SuperviseHandle {
    stopping: Arc<AtomicBool>,
    /// 当前子进程。**留在锁里**（不被等待线程独占）正是为了让 [`Self::stop`] 能 kill 它 ——
    /// 等待走的是 stdout 的 EOF，见模块头注。
    child: Arc<Mutex<Option<crate::spawn_managed::ManagedChild>>>,
    pid: Arc<AtomicU32>,
    attempts: Arc<AtomicU32>,
}

impl SuperviseHandle {
    pub fn attempts(&self) -> u32 {
        self.attempts.load(Ordering::SeqCst)
    }

    pub fn current_pid(&self) -> Option<u32> {
        let p = self.pid.load(Ordering::SeqCst);
        (p != 0).then_some(p)
    }

    /// 请求停止**并杀掉当前子进程**。
    ///
    /// 杀掉是必须的：被监护的后端不会因为父进程退出而自己走
    /// （它的入方向对「写端关闭」是刻意不敏感的），不杀就成了游魂进程。
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Ok(mut g) = self.child.lock() {
            if let Some(c) = g.as_mut() {
                let _ = c.kill();
            }
        }
    }
}

/// **起并看住**一个本机后端进程。
///
/// 形态：起 → **读它 stdout 到 EOF**（事件驱动，没有定时器）→ 按 [`decide`] 决定重起或放弃。
/// 全程在一个专用线程上；`now_ms` 由注入的时钟给（测试可以喂假时钟）。
///
/// `envs` 是给子进程的环境变量 —— e2e 用它强制私有 `TMUX_TMPDIR`，
/// **绝不让被监护的后端碰用户真实的 tmux server**。
/// P2：每次 spawn 之后，把这一条命的 **stdin 写端 + stdout 读端**交给调用方。
///
/// # 为什么是「消费者」而不是「把 stdout 拿走」
///
/// `supervise` 判死的唯一事件源是 **stdout 读到 EOF**（头注逐字「这不是定时器，也不是轮询」）。
/// 入方向通道要的是**读帧**。二者看起来在抢同一个 stdout —— **其实是同一个事件**：
/// 「读帧一直读到流结束」就是 EOF。远端那条路正是这么干的（`stream_loop` 一边解帧一边靠断流判掉线）。
/// ⇒ 不夺所有权，而是让调用方**替 supervise 把它读到底**：消费者返回 = 流结束 = 判死。
///
/// # 缺省行为一个字节不变
///
/// 不给消费者时仍是 `io::copy(&mut o, &mut io::sink())` —— F16 修的那条
/// （「不许把持续产帧的 stdout 攒进一个永不释放的 `Vec`」）**性质不变**。
/// 给了消费者，就由它自己负责不缓冲。
/// 消费者**为什么返回** —— `supervise` 据此决定要不要补一刀。
///
/// # 为什么由消费者说，而不是去探子进程
///
/// 本模块有一条已登记的不变量：**生产段不许出现「自己醒过来」的构件**
/// （`nothing_in_the_production_path_wakes_itself_up`：等子进程死请读它 stdout 到 EOF）。
/// B4 的第一版正是拿那个构件去探子进程死没死 —— **判据当场逮到**。
/// ⇒ 换成这个形状：**知道原因的那一方自己报**，一次探测都不做。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerExit {
    /// 读到 EOF —— 子进程真的走了。收尸按自然退出码走。
    Eof,
    /// 早退（读错误 / 消费者 panic）—— **子进程可能还活着**，`supervise` 要补一刀。
    Early,
}

/// 消费者返回时交回来的**全部**东西〔`K-P3b`〕。
///
/// ⚠ [`ConsumerExit`] 一个字节没动：它回答的是「`supervise` 要不要补一刀」，
/// 与「这次死亡的证据是什么」是**两件事**。把两件事塞进同一个枚举，
/// 就是本工作区最贵的那一类病（一个值装了两件事）。⇒ 并排放两个字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumerReport {
    /// `supervise` 要不要补一刀。
    pub exit: ConsumerExit,
    /// 这一命里消费者观测到的两维证据。
    pub witness: StreamWitness,
}

pub type StdioSink = Arc<
    dyn Fn(std::process::ChildStdin, std::process::ChildStdout) -> ConsumerReport + Send + Sync,
>;

// ─────────────────────────────────────────────────────────────────────────────
// ★★ `K-R28`：起后端那一跳，**「这台机器上它就是起不来」与「这一刻恰好撞上了」
//    先前装在同一个值里** —— 同一句话、同一个结局（放弃）。这一段把它们分开。
//
// # 为什么这两件事必须分开
//
// | 装进去的 | 该怎么办 |
// |---|---|
// | 这台机器上它就是起不来（路径错 / 没执行位 / 架构不对 / 被杀毒软件挡了） | 放弃，并说清是哪一种 |
// | 这一刻恰好撞上了一个**会自己过去**的竞态（`ETXTBSY`） | 等一下再试；试够了再说话 |
//
// `ETXTBSY` 的定义是「execve 的目标文件此刻正被某个进程打开着写」。我们自己那把写句柄
// 在 `extract_embedded_to` 里写完就关了，剩下的唯一来路是**别的线程**：起进程要么 fork
// 要么 posix-spawn，两者都把父进程此刻打开的 fd 复制一份给子进程，而 close-on-exec 要到
// 子进程 **execve 那一刻**才生效 ⇒ 「fork 之后、exec 之前」那段窗口里，那个子进程就是
// 一个握着我们刚写这个文件的写 fd 的进程。此刻 execve 它 = `ETXTBSY`。
//
// ★ 同一个成因在别的工具链上是有名的：go 与 cargo 都是靠**对 `ETXTBSY` 有上限地重试**收的。
//   本仓的**测试台**先前已经分开过一次（`launch.rs` 那一族），而生产段没有 ——
//   这一段就是把那个分类**抬进生产段**，两个落点（`supervise_with_stdio` 与
//   `local_backend_host::spawn_detached`）共用这一份，不各写一份。
//
// ⚠ **诚实边界（`§4`）**：这一段买的是「撞上了认得出、说得对、会重试」，
//   **不是**「它不会再发生」。真机上这个竞态多久撞一次，本层量不到。
// ─────────────────────────────────────────────────────────────────────────────

/// 一条 spawn 错误是不是 `ETXTBSY`。
///
/// 🔴 **认的是 `os error 26` 那一半，不是 `Text file busy` 那一半**：后半句由 C 库按
/// `LC_MESSAGES` 打（glibc 有中文翻译），拿它当判据等于再挂一条**隐式的 locale 前提**。
/// 前半句是 errno 的十进制，与 locale 无关。口径逐字取自 `launch.rs` 测试台那一处
/// （`fn spawn_error_is_etxtbsy`），本件是把它**抬出来共用**，不是发明第二份。
///
/// ⚠ **本层不许有平台 `cfg`**（`the_backend_half_stays_platform_agnostic` 的禁针）
/// ⇒ 这里不写 `#[cfg(not(windows))]`。代价是**已知的**、写在这儿别装作没有：
/// Windows 上 `io::Error` 的 `os error 26` 是 Win32 的 `ERROR_NOT_DOS_DISK`，
/// 不是 `ETXTBSY`。撞上它的后果只是**多试几次（上限 [`SPAWN_ETXTBSY_TRIES`]）再换一句话**，
/// 不改结局；而 Windows 上根本不存在 `ETXTBSY`，所以这条在那边是**多余而无害**，不是错答案。
pub fn spawn_error_is_etxtbsy(err: &str) -> bool {
    err.contains("os error 26")
}

/// 起进程失败之后**分得开的那两件事**。
///
/// ⚠ 刻意不是 `Result<Child, String>` 加一个 `bool`：那样同一个事实就有了两份表示，
/// 而两份表示会漂 —— 那正是本件在治的病。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnFailure {
    /// **这一刻恰好撞上了**：`ETXTBSY`，而且**试到了上限**。会自己过去，再开一次多半就好。
    TransientBusy {
        /// 试了几次（= [`SPAWN_ETXTBSY_TRIES`]，除非调用方另给）。
        tries: u32,
        /// 最后一次撞上时的**逐字错误**。
        ///
        /// ⚠ 它**只装那一句原话**，不许再把「撞了几次」揉进来 —— 那个数住 `tries`。
        /// 〔本轮自查逮到的：初版把 `last` 写成 `format!("撞了 {n} 次，最后一次逐字：{e}")`，
        /// 于是同一个事实有了两份表示，实打出来的那句话是
        /// 「…最后一次逐字：撞了 8 次，最后一次逐字：Text file busy…」——
        /// **本件在治的病，长在治它的代码里。**〕
        last: String,
    },
    /// **这台机器上它就是起不来**：路径错 / 没执行位 / 架构不对 / 被挡了。放弃。
    Broken(String),
}

/// 允许撞几次 `ETXTBSY` 才算「这一次是真等不到了」。
///
/// 🔴 **刻意不照抄 `launch.rs` 的 50** —— 那是**测试台**的口径（50 × 20ms ≈ 1s，
/// 一条判据慢慢等没人有意见）。**生产段是用户在等一个窗口**，多等一秒是看得见的卡顿。
///
/// # 这个数怎么定的（`8`）
///
/// 要等的那个窗口是**别人的 fork 到 execve 之间**，`launch.rs` 那一段头注实测记着它是
/// **亚毫秒级**。而每一次重试**自己就要花时间**：一次失败的 spawn 是一整趟
/// fork + execve + 回错，在 Linux 上是几百微秒量级 ⇒ 这个上限本身已经跨过那个窗口**好几倍**，
/// 而**总代价仍是几毫秒**，落在「用户感觉是瞬间」那一档（100ms 界）里，
/// 而且**远远小于**测试台那条路的 1 秒。
/// ⇒ 撞上一次几乎必然被头一两次重试吸收；连撞到上限都没让开，那已经不是「等一下就好」，
/// 该换一句话说给用户听。
/// ⚠ **这一句被读数订正过一次**：`K-R28` 立它时逐字写的是「跨过那个窗口**一个数量级**」，
/// 而 `K-R30` 实测是 **4.9 倍**（空闲）/ **3.6 倍**（满载）—— 方向对，倍数写大了。
/// 订正的依据在下一节；**这一段是理由，下一节才是读数**，两者别读成一件事。
///
/// 🔴 **上界的另一半理由是「不许把它变成一个定时器」**：见 [`spawn_with_etxtbsy_retry`]
/// 头注那条 `C12` —— 本模块生产段一个 `sleep` 都不许有，所以这个数不能靠「睡久一点」兜底，
/// 只能靠「次数够，但每次都是一次真的尝试」。
///
/// # 这个数的**出处**（`K-R30`，09-06 沙箱实测）
///
/// 〔出处·K-R30〕上面几节是**理由**，不是读数。`K-R30` 去把读数打了出来：
/// 量具 `tests/evidence/K-R30-etxtbsy-window.py`（读数落在同名 `.out`）。
/// 台架 = 真造那个竞态：一个进程攥着目标文件的写 fd（`fork` 继承来的 —— `CLOEXEC` 要到它
/// `execve` 那一刻才生效），我们自己那把立刻关掉，然后**立即重试**到 `execve` 成功，
/// 量「从关掉写 fd 到 exec 成功」。容器内 · Linux 7.0.0-30 · glibc 2.39 · nproc 16 ·
/// 每格 200–300 个样本 · 时长一律单调钟。
///
/// | 量的是什么 | 空闲 | 满载（16 个自旋进程） |
/// |---|---|---|
/// | 一个孩子 `fork`→`execve` 那个窗口的**上界**（= 一次 `posix_spawn` 的墙钟：glibc 用 `CLONE_VFORK`，调用线程被挂起到那个孩子 `execve` 为止） | p50 148 µs · p99 212 µs | p50 2978 µs · p99 4307 µs |
/// | **8 次立即重试打完**要多久 | p50 1046 µs · p99 2536 µs | p50 15319 µs · p99 24414 µs |
/// | ⇒ 这个上限盖住那个窗口上界几倍（重试 p50 ÷ 窗口 p99） | **4.9 倍** | **3.6 倍** |
///
/// ★ **最值钱的是第三行**：满载时那个窗口涨了约 20 倍，而重试的代价**同步**涨了约 15 倍
/// ⇒ 「按次数封顶」这把尺子的**单位与被等的那件事是同一种物理操作**（两边都是一趟
/// fork+execve），于是它**跟着机器一起变慢**，倍数只从 4.9 掉到 3.6。
/// 一个固定时长的退避没有这条性质：按空闲调好就在满载时不够，按满载调好就在空闲时白等。
/// ⇒ `C12` 那条「按次数、不按时间」在这一格上买到的**不只是「没有定时器」**，
/// 还有「这把尺子自己会随负载伸缩」。
///
/// ⚠ **它买不到的三样，别读大**（`K-R30 §4`）：
/// ① 这是**沙箱**读数 —— 真机的负载 / 文件系统 / 内核版本未必同值；
/// ② 上表第一行是那个窗口的**上界**，不是窗口本身；
/// ③ 泄漏方那个孩子若在 `fork` 与 `execve` 之间**还要干活**（量具的臂 B：一个 CPython 子进程，
///    窗口 ≈ 1.2 ms），这个上限**盖不住** —— 空闲实测只有 4.7% 落在预算内（n=300）。
///    那一档今天靠的是**本仓一处 `pre_exec` 都没有** ⇒ 孩子在 fork 与 exec 之间不干活，
///    **不是**靠这个数。口径写死（`K-R30` 09-06 现打，`grep -rn "pre_exec" --include=*.rs`，
///    分母 = 整棵仓、含测试段）：在 `39ae7fd` 上 **0 命中**；在本拍之后是 **1 处，
///    而那一处就是这句话自己** —— 剥掉注释行仍是 0。
///    ⚠ **这个前提今天没有判据守着**：有人加一处 `pre_exec` 不会红，而这个数就得重量。
///
/// 🔴 **改这个数就要同改这一节** —— 判据
/// `the_retry_budget_number_has_a_measured_origin_pinned_to_it` 钉着这件事。
pub const SPAWN_ETXTBSY_TRIES: u32 = 8;

/// 有上限地重试，**只对 `ETXTBSY`**；别的错**一次都不重试**。
///
/// 重试一个真缺陷 = 把它变成偶尔绿的偶发红，那比今天更糟。
///
/// `attempt` 与 `backoff` 都是注入的：**这一层唯一的决策是「哪种错该再试一次」**，
/// 而 spawn 本身与「两次之间做什么」没什么可判的 ⇒ 抽出来才能不起真进程就单测。
/// 生产段的那一份接线是 [`spawn_with_etxtbsy_retry`]。
///
/// ⚠ **最后一次失败之后不叫 `backoff`** —— 那一次的等待没有任何人会用到，
/// 而它是用户真的在等的时间。⇒ `backoff` 恰好被调用 `tries - 1` 次。
pub fn spawn_retrying_etxtbsy<T>(
    tries: u32,
    mut attempt: impl FnMut() -> Result<T, String>,
    mut backoff: impl FnMut(u32),
) -> Result<T, SpawnFailure> {
    let budget = tries.max(1);
    let mut last = String::new();
    for i in 0..budget {
        match attempt() {
            Ok(v) => return Ok(v),
            Err(e) if spawn_error_is_etxtbsy(&e) => {
                // ⚠ 只存那一句原话；「撞了几次」是 `TransientBusy::tries`，不在这里再存一份。
                last = e;
                if i + 1 < budget {
                    backoff(i);
                }
            }
            Err(e) => return Err(SpawnFailure::Broken(e)),
        }
    }
    Err(SpawnFailure::TransientBusy {
        tries: budget,
        last,
    })
}

/// 生产段两个落点共用的那一跳：起一个进程，并把两件事**分开交回**。
///
/// ⚠ 刻意**不**把配置那一半（`Command` 怎么建、env / stdio 怎么设）搬进来：
/// 那两处的配置是各自的领地（一处要管子、一处要 `process_group`），
/// 而且「谁会在用户机器上起进程」那张申报表
/// （`write_site_registry::tests::SPAWNS`）按**外层函数名**认账 ——
/// 把建 `Command` 那一行搬走，等于把两条申报从表上抹掉。
///
/// # 🔴 两次尝试之间**今天什么都不做**，而这不是疏忽 —— 写清楚，别让下一个人以为是
///
/// go / cargo 那条公认的解法是「**有上限地重试 + 每次之间睡一小会儿**」，而这里只有前半。
/// 后半今天做不了，理由是**两道现成的闸**，两道都不在本件的写区里：
/// - `nothing_in_the_production_path_wakes_itself_up`（本文件，`C12` 的源码钉）：
///   **本模块生产段一个 `thread::sleep` 都不许有**；
/// - `rust_timer_registry::every_periodic_wake_in_the_rust_tree_is_registered`：
///   monitor Rust 树里**每一处** `sleep` 都要在那张登记表里有一条，
///   而那张表住 `src/rust_timer_registry.rs`。
///
/// ⇒ 加一句退避 = 同时动那两处，其中第二处是**第三个文件**。**如实记为没做到，交回 PM 裁。**
/// ⚠ **别读大**：没有退避买到的仍然是真的（重试本身要花几百微秒一趟、次数上限是真的、
/// 两件事真的分开了），但它**比 go/cargo 那条路弱** —— 机器很忙、那个孩子迟迟排不上
/// execve 时，8 趟连着打完可能仍在同一个窗口里。这一格**量不到**（要真机 + 真负载），
/// 原样进诚实边界。
///
/// `backoff` 这个注入点是**特意留着的**：PM 裁定之后，接上去只改这一行。
/// ⚠ **`spawn` 是注入进来的**〔`15 §5.1 A3`，09-18〕：起进程那一下要回答三个问题
/// （要不要窗口 · 要不要随我死 · 错误往哪去），而那三个答案要落成**平台原语**
/// （`creation_flags` / `process_group` / Job Object）—— 它们进不了本层
/// （`the_backend_half_stays_platform_agnostic` 的禁针，而平台例外表有递减棘轮）。
/// ⇒ 本层只知道「起进程这一下由宿主给的这个东西来做」，形状照
/// [`start_or_extract`] 的 `make_executable: &dyn Fn(...)`。
pub fn spawn_with_etxtbsy_retry(
    cmd: &mut std::process::Command,
    spawn: &crate::spawn_managed::ManagedSpawn,
) -> Result<crate::spawn_managed::ManagedChild, SpawnFailure> {
    spawn_retrying_etxtbsy(
        SPAWN_ETXTBSY_TRIES,
        || spawn(cmd).map_err(|e| e.to_string()),
        // 今天是空的 —— 理由见上方那一段，**不是忘了写**。
        |_| {},
    )
}

/// 撞上那个会自己过去的竞态、而且试到上限之后**该说的那句话**。
///
/// 🔴 **只此一份**：两个落点共用它。各写一份，两句话就会漂，
/// 而「用户看到的那句话」正是本件唯一交付给用户的东西。
///
/// 它与「起不来」那句的分工：这一句必须让用户读得出**再开一次多半就好**，
/// 那一句必须让用户读得出**这台机器上今天就是起不来**。
pub fn etxtbsy_gave_up_reason(bin: &Path, tries: u32, last: &str) -> String {
    format!(
        "起 {} 时连着 {tries} 次撞上「这个文件正被谁打开着写」（ETXTBSY，os error 26）——\
         这不是它起不来，是这一刻恰好有别的子进程还攥着我们刚写它时的那个写 fd。\
         这个状态会自己过去，再开一次多半就好。最后一次逐字：{last}",
        bin.display()
    )
}

/// 一次子进程生命周期里，**最多往滚动日志搬这么多字节**的 stderr。
///
/// 🔴 它**不是**「读到这里就不读了」—— 那是把病换个方向犯（见 [`drain_child_stderr_into_log`]
/// 头注的第二条）。超出之后照旧读到 EOF，只是不再逐行记，收尾报一个**行数**。
const STDERR_LOG_BUDGET_BYTES: u64 = 256 * 1024;

/// 单行上界。对端一个 `\n` 都不发时，`read_until` 会一直把内存吃下去。
///
/// ⚠ 超了**不丢字节**：这一段照记，只是末尾打一句 [`STDERR_CUT_MARK`] 说「它在这里被切开」，
/// 余下的字节成为下一条。⇒ 超限语义是「**截断+说清**」里的「说清」那一半承重 ——
/// 没有那句标记的话，日志里会出现一条**看起来完整、其实是半句**的诊断。
const STDERR_MAX_LINE_BYTES: u64 = 8 * 1024;

/// 一行被 [`STDERR_MAX_LINE_BYTES`] 切开时贴在断口上的话。
const STDERR_CUT_MARK: &str = " …〔这一行超过单行上界，在此切开，下一条接着它〕";

/// 把一个子进程的 stderr **读到 EOF**，前 [`STDERR_LOG_BUDGET_BYTES`] 字节逐行接进
/// monitor 的滚动日志（`logging.rs`：`~/.claude/claudecode-frontend/logs/monitor.<日期>.log`）。
///
/// # 三条设计约束，每一条都是别人踩过的坑
///
/// 1. 🔴 **它必须读到 EOF，哪怕已经不想记了。** 管道的另一端是子进程：读的一侧停下来，
///    内核缓冲写满之后**子进程下一次 `write(2)` 就阻塞**——而它阻塞在打印一句诊断上。
///    那会把「诊断没人看」升级成「**打印诊断会把后端挂住**」，比原来的病重。
///    ⇒ 预算用完之后走的是 `continue`（继续读、只是不记），不是 `break`。
///
/// 2. **记账要有上界，但上界只砍「记」不砍「读」。** 后端崩溃循环时 stderr 可以很吵，
///    而滚动日志是按天滚的；没有上界的话一次崩溃循环能把当天那份撑爆，
///    于是**下一次故障的线索反而被这一次的噪音淹掉**。
///
/// 3. **级别一律 `warn`，绝不 `error`。** `logging.rs` 的 `ErrorEmitterLayer` 只拦
///    `Level::ERROR`，拦到就 emit `monitor-error` → 前端弹红色 toast。而这里搬的是
///    **子进程说的话**，它自己的级别在文本里（后端那侧 `tracing_subscriber` 的 fmt 前缀），
///    我们**没有**可靠办法把它还原成本进程的级别。
///    ⇒ 拿不准就别替用户决定「这值得弹一个红框」：进日志文件，不进 toast。
///    ⚠ 这一格是**刻意的诚实边界**：用户看得到的是日志文件，不是弹窗。真要某一类后端错误
///    弹到脸上，那是**按内容分类**的活（谁分类、分哪几类 = 一次产品裁定），不是这里加个 `if`。
///
/// ⚠ 非 UTF-8 走 `from_utf8_lossy`，不走 `read_line` —— 后者遇到非法字节返回
/// `InvalidData`，那会把本函数踢出循环，于是约束 1 当场失守（读的一侧停了，子进程会卡）。
pub(crate) fn drain_child_stderr_into_log(err: std::process::ChildStderr, pid: u32) {
    use std::io::{BufRead, Read};
    let mut reader = std::io::BufReader::new(err);
    let mut budget = STDERR_LOG_BUDGET_BYTES;
    let mut buf: Vec<u8> = Vec::new();
    let mut unlogged = 0usize;
    loop {
        buf.clear();
        let n = match (&mut reader)
            .take(STDERR_MAX_LINE_BYTES)
            .read_until(b'\n', &mut buf)
        {
            Ok(n) => n,
            // 读不动了 = 管子那头没了（子进程走了 / fd 被关）。这里 `break` 不违反约束 1：
            // 已经没有「另一端会被我卡住」的另一端了。
            Err(_) => break,
        };
        if n == 0 {
            break; // EOF：子进程关掉了 stderr（通常就是它退了）。
        }
        if budget == 0 {
            unlogged += 1;
            continue; // ← 约束 1：照旧读，只是不记。
        }
        budget = budget.saturating_sub(n as u64);
        // 没读到 `\n` 而恰好读满上界 ⇒ 这一条是被**切开**的，断口要说出来。
        let cut = n as u64 == STDERR_MAX_LINE_BYTES && buf.last() != Some(&b'\n');
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\n', '\r']);
        if line.is_empty() {
            continue;
        }
        if cut {
            tracing::warn!("本机后端[pid {pid}] {line}{STDERR_CUT_MARK}");
        } else {
            tracing::warn!("本机后端[pid {pid}] {line}");
        }
    }
    if unlogged > 0 {
        tracing::warn!(
            "本机后端[pid {pid}] 另有 {unlogged} 行 stderr 没有记进来 —— \
             这一条子进程的日志预算（{STDERR_LOG_BUDGET_BYTES} 字节）用完了。\
             ⚠ 别把它读成「后端只说了这些」：它说的比记下来的多。"
        );
    }
}

/// 见 [`StdioSink`]。`stdio` 为 `None` ⇒ 不接消费者（stdin 恒 `null`）。
/// 〔RL1 · V107〕先前还有一个 `stdio=None` 的薄壳入口，它唯一的生产客户是 monitor 另起的本机中转；
/// 中转并进本机常驻后端之后那个薄壳没了客户，随之删掉 —— 生产上只剩这一个入口。
#[allow(clippy::too_many_arguments)]
pub fn supervise_with_stdio(
    bin: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    limits: CrashLimits,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
    stdio: Option<StdioSink>,
    spawn: Arc<crate::spawn_managed::ManagedSpawn>,
) -> SuperviseHandle {
    let stopping = Arc::new(AtomicBool::new(false));
    let child: Arc<Mutex<Option<crate::spawn_managed::ManagedChild>>> = Arc::new(Mutex::new(None));
    let pid = Arc::new(AtomicU32::new(0));
    let attempts = Arc::new(AtomicU32::new(0));
    let handle = SuperviseHandle {
        stopping: stopping.clone(),
        child: child.clone(),
        pid: pid.clone(),
        attempts: attempts.clone(),
    };
    let crashes: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));

    std::thread::spawn(move || {
        loop {
            if stopping.load(Ordering::SeqCst) {
                return;
            }
            let mut cmd = std::process::Command::new(&bin);
            // ★★ **`TMUX` 一律不继承**〔事故订正 08-11〕。
            //
            // 被监护的后端会跑 `tmux ls`。tmux 客户端在 `TMUX` 有值时**按它给的 socket 走，
            // `TMUX_TMPDIR` 完全不起作用** —— 那正是「私有 socket 隔离」被绕过的机制。
            // 我在一次探针里漏了这一条，结果用户 9 个真实 tmux 会话没了。
            // ⇒ 这里无条件清掉：backend 该按自己的 `TMUX_TMPDIR`（或默认 socket）解析，
            // 而不是继承「monitor 恰好从哪个 tmux 里被启动」这个偶然。
            cmd.env_remove("TMUX");
            // ★★ `00 §1.5.1` 步 1（不开控制台窗口）与 `15 §5.1 A2`（stderr 接进滚动日志）
            //    **都已经不在这一层了**〔A3 落地，09-18〕。
            //
            // 先前这里有两段平台/宿主知识：一句 `crate::local_backend_host::hide_console_window(&mut cmd)`
            // （那是 A3 之前的止血，它自己的头注逐字写着「A3 落地时它会被换成注入参数，
            // 和 `make_executable` 一样」），加一句 `.stderr(Stdio::piped())` ＋ 下面一条泵。
            // ⇒ 今天两样都是 `spawn` 这个注入参数说了算：宿主那侧声明
            // `ConsolePolicy::Hidden` ＋ `StderrSink::ToLog`，本层一个平台原语都不认识。
            //
            // ⚠ 剩下的 `stdin` / `stdout` **仍归本层**：那两根管子是本模块的协议
            //（stdout 的 EOF 是「进程死了」这个事件的唯一来源），不是三条策略里的任何一条。
            cmd.args(&args)
                // P2：有消费者才接 stdin。无消费者时**逐字维持 `null`** ——
                // 「本机后端收不了入方向命令」是 C4 量出来的缺口，
                // 但没人要那根管子时接出来只会多一个没人写的 fd。
                .stdin(if stdio.is_some() {
                    std::process::Stdio::piped()
                } else {
                    std::process::Stdio::null()
                })
                // ★ stdout 必须是管道：它的 EOF 就是「进程死了」这个事件的来源。
                .stdout(std::process::Stdio::piped());
            for (k, v) in &envs {
                cmd.env(k, v);
            }
            // ★★ `K-R28`：这一跳**分得开两件事**（见 `spawn_error_is_etxtbsy` 上方那一段）。
            //    先前这里是一句裸 `cmd.spawn()` + 一个桶装所有失败 + 当场 `return`，
            //    于是「这台机器上它就是起不来」与「这一刻恰好撞上了一个会自己过去的竞态」
            //    同一句话、同一个结局。**分类那一份是共用的，不在这里再写一遍。**
            let mut spawned = match spawn_with_etxtbsy_retry(&mut cmd, &*spawn) {
                Ok(c) => c,
                // 这一刻恰好撞上了 —— 而且试到了上限。换一句话，别说成「起不来」。
                Err(SpawnFailure::TransientBusy { tries, last }) => {
                    on_event(SuperviseEvent::GaveUp {
                        reason: etxtbsy_gave_up_reason(&bin, tries, &last),
                    });
                    return;
                }
                // 这台机器上它就是起不来 —— **今天那句照旧，一个字不改**。
                Err(SpawnFailure::Broken(e)) => {
                    on_event(SuperviseEvent::GaveUp {
                        reason: format!("起不来 {}：{e}", bin.display()),
                    });
                    return;
                }
            };
            let this_pid = spawned.id();
            // 把 stdout 拿走（独立 owned handle），Child 本体交给锁 —— 见模块头注。
            let out = spawned.stdout.take();
            // P2：只有接了消费者时这里才是 Some（上面 `stdin(…)` 按 `stdio` 分流）。
            let in_ = spawned.stdin.take();
            // ★★ `A2` 那条泵（stderr → 滚动日志）**现在由 `StderrSink::ToLog` 在唯一出口里起**
            //    —— 实现仍是本模块的 [`drain_child_stderr_into_log`]（那三条设计约束都在它头注里），
            //    只是「谁在什么时候把它接上」收成了一处。本层因此连 `stderr` 这个字都不用再提。
            pid.store(this_pid, Ordering::SeqCst);
            if let Ok(mut g) = child.lock() {
                *g = Some(spawned);
            }
            // ★ **F16 关窗**：`stopping` 原来只在循环顶部与 EOF 之后检查 ⇒ 存在一个窗口 ——
            // 刚过顶部检查就 `spawn`，此刻 `stop()` 执行：它置位 `stopping`，但锁里还是 `None`
            // ⇒ **一个字节的 kill 都没发**；线程接着把子进程存进锁、进 `io::copy` 永久阻塞
            // （backend 对 stdin 关闭刻意不敏感、也不会自己退）⇒ **monitor 退了、backend 还在跑，
            // 而且没人再能 kill 它** —— 那正是 `stop()` 头注说的「游魂进程」。
            // 触发条件：启动后极短时间内退出（single-instance 第二实例、启动即关窗）。
            if stopping.load(Ordering::SeqCst) {
                if let Ok(mut g) = child.lock() {
                    if let Some(c) = g.as_mut() {
                        let _ = c.kill();
                        let _ = c.wait();
                    }
                    *g = None;
                }
                pid.store(0, Ordering::SeqCst);
                return;
            }
            let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
            on_event(SuperviseEvent::Started {
                pid: this_pid,
                attempt,
            });

            // ★ 等它死：读到 EOF。**这不是定时器，也不是轮询** —— 没有「隔多久看一眼」。
            //
            // ⚠ **F16 修**：原来是 `read_to_end(&mut Vec::new())` —— 那会把被监护后端的
            // **全部 stdout 攒在一个永不释放的 `Vec` 里**，而它是**持续产帧**的
            // （那正是本模块头注用来论证「它不会关掉 stdout」的理由）⇒ 增长速度 =
            // 本机所有会话的 jsonl 产出速度，且没有任何消费者。
            // 〔订正 09-10：原话「今天不咬人只因为 `resolve_beside_this_exe` 恒 `Missing`
            // （`tauri.conf.json` 里没有 `externalBin`）—— **离生效只差一个配置项**」**已过期**。
            // F05b 随 v3.7.0 发出去了：装出来那份现打有 2 个后端进程在跑（读数住模块头注）
            // ⇒ 这条路**在装了安装包的机器上已经生效**，不再是「差一个配置项」。
            // 也就是说这个修不是预防性的，它今天就在挡真事。〕
            // `tests/e2e/local-backend-supervise.sh` 那条真进程路径也一直在跑它。
            // ⇒ `io::copy` 到 `io::sink()`：**EOF 语义完全不变**，但一个字节都不留。
            let report = match (&stdio, out, in_) {
                // P2：消费者**负责把 stdout 读到底** —— 它返回就等于流结束。
                // B4 之后它还要说清**为什么**返回（EOF 还是早退），见 `ConsumerExit`。
                // `K-P3b` 之后它还要交回**观测到的两维证据**，见 `StreamWitness`。
                (Some(f), Some(o), Some(i)) => f(i, o),
                // 缺省：F16 那条 —— EOF 语义不变，一个字节都不留。`copy` 返回即 EOF。
                // ⚠ `K-P3b`：这一支**没有消费者** ⇒ 「它说过话没有」一次都没被观测过。
                //   如实报 `NoConsumer`，不拿「观测到它没说话」去顶（见 `StreamWitness` 头注）。
                (_, Some(mut o), _) => {
                    let _ = std::io::copy(&mut o, &mut std::io::sink());
                    ConsumerReport {
                        exit: ConsumerExit::Eof,
                        witness: StreamWitness::NoConsumer,
                    }
                }
                (_, None, _) => ConsumerReport {
                    exit: ConsumerExit::Eof,
                    witness: StreamWitness::NoConsumer,
                },
            };
            let exit_reason = report.exit;
            // EOF 之后收尸。
            //
            // ⚠ **F16 修**：原来是 `child.lock().ok().and_then(|mut g| … c.wait())` ——
            // guard 活在闭包里 ⇒ **`wait()` 整段都持着锁**。而「子进程关掉 stdout 但继续活着」
            // 是本模块**已登记的诚实边界**（见头注），那时 `wait()` 会久等，后果不是
            // 「误判它死了」而是：`stop()` 第一件事就是 `self.child.lock()`，而它跑在
            // **主线程**（`lib.rs` 的 `RunEvent::Exit`）⇒ **窗口关了、进程退不出去，只能 kill -9**。
            // ⇒ 先把 `Child` 从锁里 **`take()` 出来**，再在锁外 `wait()`。
            // ⚠⚠ **F16 那句「此刻已过 EOF」在 P2 之后不再恒成立**〔D 阶段补审 08-11 修，B4〕。
            //
            // 消费者返回有三种原因，只有第一种符合那个前提：
            // ① EOF（子进程真的走了）· ② stdout 读错误 · ③ 消费者 panic（被兜底外壳接住）。
            // ②③ 时**子进程还活着**，而它已经被从共享 `Mutex` 里摘走 ⇒
            // 并发的 `stop()` 看到 `None`，**一个字节的 kill 都不发**，却仍返回「本机后端已停」。
            // 子进程要等到下一次写 stdout 拿 EPIPE 才死；`--tail-only` 空闲期那可以是很久，
            // 期间 `wait()` 还一直阻塞着本线程。
            //
            // ⇒ **消费者自己报原因**（[`ConsumerExit`]）：`Early` 才补一刀，让「take 出来再 wait」
            // 这个做法的前提**由自己保证**，而不是靠调用路径碰巧成立。
            //
            // ⚠ 刻意**不无条件 kill**：那会把正常退出码换成信号死，而 `decide()` 正是按退出码
            // 分「崩溃 / 正常退出」（`ROADMAP` 风险 5d 记着「`code: Some(0)` 被计成崩溃」那次观测）。
            // ⚠ 也刻意**不探子进程**：本模块禁「自己醒过来」的构件，见 [`ConsumerExit`] 头注 ——
            // B4 的第一版正是拿那个构件去探，判据当场逮到。
            let mut reaped = {
                let mut g = child.lock().unwrap_or_else(|e| e.into_inner());
                if exit_reason == ConsumerExit::Early {
                    if let Some(c) = g.as_mut() {
                        let _ = c.kill();
                    }
                }
                g.take()
            };
            // ★ `K-P3b`：`status` **原样**留着交上去 —— `code()` 在被信号打死时是 `None`，
            //   而信号号只有宿主层取得到（`ExitStatusExt` 在本层的禁针里）。
            let status = reaped.as_mut().and_then(|c| c.wait().ok());
            let code = status.and_then(|s| s.code());
            pid.store(0, Ordering::SeqCst);
            on_event(SuperviseEvent::Exited {
                code,
                attempt,
                status,
                witness: report.witness,
            });

            if stopping.load(Ordering::SeqCst) {
                return;
            }
            let t = now_ms();
            let decision = {
                let mut g = crashes.lock().expect("crash 账被 poison");
                g.push(t);
                let d = decide(&g, t, limits);
                // ⚠ **F16 顺手修**：原来只 push 不修剪 ⇒ 崩溃间隔大于窗口时向量单调增长，
                // 且每次决策都 O(n) 全扫（每 20s 崩一次跑一个月 ≈ 13 万条）。
                // `decide` 本来只看窗口内的那些 ⇒ 修剪**不改语义**（由 `decide` 的单测钉着）。
                g.retain(|x| t.saturating_sub(*x) < limits.window_ms);
                d
            };
            match decision {
                Decision::Restart => continue,
                Decision::GiveUp { reason } => {
                    on_event(SuperviseEvent::GaveUp { reason });
                    return;
                }
            }
        }
    });
    handle
}

/// 在**本可执行文件同目录**找本机后端。这是 [`resolve_with`] 的真文件系统版。
///
/// `current_exe()` 不是 GUI 把手（不违反宿主无关那条机检）——
/// 它是「我这个二进制装在哪」这一条**部署事实**，正是本机后端该在的位置。
/// F06b-1d（**C9** 的逐字落地）：给 backend 亲手开的那个终端窗口，配上后端路径。
///
/// # 为什么是「给窗口设 env」而不是「拼进启动串」
///
/// C9 逐字：frontend 只剩「在用户桌面上**开一个终端窗口**」，**窗口里跑什么由 backend 给**。
/// 而开窗这一步本来就在 backend 手里（`launch.rs::launch_local_posix` /
/// `launch_powershell_window`）⇒ **backend 直接把环境交给它开的窗口**，是这条原则最直接的形态。
///
/// 另外三条候选都要**把一个后端事实塞进前端**（`payload.rs` 编译的 `EnvOp` 由 TS 经 wire 送来）
/// —— 那正对着 **C2**（backend 是 monitor 自己的一半，不是外部依赖）。⇒ 裁 ⑤，理由进 `DECISIONS.md §2`。
///
/// # 返回 `None` 的含义
///
/// **local_backend 不在就不设** —— 导一个指向空处的路径不会让 ccm 更聪明（它那边 `[ -x ]` 一样过不了），
/// 只会让「这台机到底有没有本机后端」这个问题多一个假阳性来源。⇒ 空值 ≠ 未设（Z01 那条支点）。
pub(crate) fn backend_bin_env_for_window(target_triple: &str) -> Option<(&'static str, String)> {
    // 〔WIN1 · RT1 F2〕问「正在用的那份」，不只问 exe 旁边（裸 exe 的机器上旁边永远没有，见 [`resolve_in_use`]）。
    env_from_resolved(resolve_in_use(target_triple))
}

/// 〔WIN1 · RT1 F2〕这个进程**这一趟解析出来、交去起的那份**本机后端住哪。
///
/// 由 [`resolve_or_extract`]（两条起法 —— 监护的 stdio 那条与常驻宿主那条 —— 共用的唯一解析口）
/// 在找到的那一刻记下；只记路径，不记别的。进程内一份，不落盘（重启后下一次起后端时重记）。
static IN_USE: Mutex<Option<PathBuf>> = Mutex::new(None);

fn note_in_use(bin: &Path) {
    *IN_USE.lock().unwrap_or_else(|e| e.into_inner()) = Some(bin.to_path_buf());
}

/// 〔WIN1 · RT1 F2〕**读面与给窗口的环境要的那份本机后端**：先认这个进程正在用的那份
/// （[`resolve_or_extract`] 记下的，自释放到 `~/.cc-monitor/bin/` 的那份也算），它不在了才找 exe 旁边。
///
/// 真 Win11 现打（`第四波记录/RT1.md §8` F2）：裸 `monitor.exe` 旁边没有后端，后端自释放到
/// `~\.cc-monitor\bin\` 并已在跑（设置页「已连上（pid 3256）」），而账号页同一屏说「本机后端不在」——
/// 从前读面只问 [`resolve_beside_this_exe`]。那是 `设计/01 §5 D11`「所有东西都不要假设后端没起来」的反面。
pub fn resolve_in_use(target_triple: &str) -> Resolved {
    in_use_or(
        IN_USE.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        || resolve_beside_this_exe(target_triple),
        &|p: &Path| p.is_file(),
    )
}

/// [`resolve_in_use`] 的纯内核（记下的那份 · 旁边那一问 · 「文件在不在」都是注入的）。
pub(crate) fn in_use_or(
    remembered: Option<PathBuf>,
    beside: impl FnOnce() -> Resolved,
    exists: &dyn Fn(&Path) -> bool,
) -> Resolved {
    match remembered {
        Some(p) if exists(&p) => Resolved::Found(p),
        _ => beside(),
    }
}

/// 上面那个函数的**纯**内核 —— 抽出来是为了两个分支都测得到。
///
/// ⚠ 不抽的话判据只走得到 `Missing`（测试环境旁边没有本机后端），
/// 于是「`Found` 时用的是 [`BACKEND_BIN_ENV`] 这个名字」这半**永远验不了** ——
/// 那正是「判据的探针改变了被观察的路」的近亲：**探针到不了的分支等于没判据**。
fn env_from_resolved(r: Resolved) -> Option<(&'static str, String)> {
    match r {
        Resolved::Found(p) => Some((BACKEND_BIN_ENV, p.to_string_lossy().into_owned())),
        _ => None,
    }
}

pub fn resolve_beside_this_exe(target_triple: &str) -> Resolved {
    let exe_dir = match std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        Some(d) => d,
        None => {
            return Resolved::Missing {
                reason: "拿不到自身可执行文件路径 ⇒ 无法定位本机后端 local_backend".into(),
                looked_at: Vec::new(),
            }
        }
    };
    resolve_with(
        &exe_dir,
        target_triple,
        std::env::consts::EXE_SUFFIX,
        &|p: &Path| p.is_file(),
    )
}

/// # 诚实边界 8c：**不清理旧的按 build_id 命名的文件**
///
/// 文件名带 build_id 是为了幂等与不撞版（见下方 D1 段），代价是
/// **每换一次后端构建就在 `~/.cc-monitor/bin/` 多留一个 10MB 级的旧文件，永不回收**。
/// 今天没有任何清理逻辑，也没有判据钉它。
/// ⇒ 刻意不做：按 mtime/版本回收要先定「谁还可能在跑旧的那份」，那是 `P2d`（认已有实例）的前提。
///
/// 🔴 **`K-R42` 09-10 补一条：这笔账的分母变大了，而处置没变。**
/// 本段写下时，这条路**只在 Linux 上真跑过**（宿主那侧压着 `cfg!(target_os = "linux")`），
/// 而 Linux 上安装包与 dev 树旁边多半就有 local_backend ⇒ 释放这一支很少走到。
/// 本件把 Windows 那一格接上之后，**裸 exe 每换一个后端版本就在
/// `%USERPROFILE%\.cc-monitor\bin\` 多留一份**（这一次是 14 MB 级，见 `K-R42` 交回的体积读数）。
/// ⇒ **仍然不清理**，三条理由都还成立、且新添一条：
/// ① 判「谁还在跑旧的那份」仍是 `P2d` 的前提，本件没做那件事；
/// ② 真删要扩 `sweep_stale_partials` 的射程（它今天**只删自己那套 `.partial` 命名**），
///    而那条射程逐字登记在 `write_site_registry::WRITE_SITES` 里 —— 那张表不在本件写区，
///    改了代码不改登记 = 让一条登记变成假话，比多留一个文件贵；
/// ③ 幂等这一半是好的：**同一个 build_id 不会重复写**（下面那个 size 相等就跳过的分支）
///    ⇒ 留下的份数上界是「这台机上装过几个不同后端版本」，不是「起过几次 monitor」。
/// ⚠ **这是一笔如实记着的欠账，不是「已解决」** —— 建议作跟进件，与 `P2d` 同拍做。
///
/// P2z（`control-parity` 的定框 C10 —— 单 exe 那一条，不是 `backend-split` 那条平台原语）：**单 exe 自释放** —— 把 app 里**已经内嵌**的那份 musl backend
/// 写到 `dir` 下，文件名**带 build_id**，返回落点。
///
/// # 为什么文件名必须带 build_id（自批 D1，别改成和远端部署同一个文件）
///
/// 远端自部署的落点也是 `~/.cc-monitor/bin/`（`sftp.rs` 头注 F08）。**实测 08-11**：本机那份
/// `.build_id` 是 `p1r-event-liveness`（别的 monitor 把这台当远端连时装的，当时还有进程跑在上面），
/// 而本机源码是 `p1x-overflow-identity`。两边对同一个文件有**不同期望** ⇒ 各自判对方 stale、
/// 互相覆盖 ⇒ **无限重装循环**。`build.rs` 那段 panic 逐字警告过同一个形状：
/// 「装上去之后**永远判 stale** ⇒ 无限重装循环。这不是「慢一点」，是坏的。」
/// ⇒ 本机这条路**按 build_id 命名**，与远端那条**结构上不可能撞**（不是靠「配置别配成一样」）。
///
/// # 宿主知识留调用方
///
/// `dir` 由调用方给（`lib.rs` 那侧算 `~/.cc-monitor/bin`）——同 `resolve_beside_this_exe`
/// 把 exe 目录留给调用方的理由，本模块过 `backend::tests::the_backend_layer_stays_host_agnostic`。
///
/// # 它不做什么
///
/// **不校验写完的字节是不是真能跑** —— `deploy_decision` 只回答「要不要装」，不回答「装完对不对」。
/// 起不起得来由监护层（[`supervise_with_stdio`]）的崩溃计数说话。
/// 本机释放的**文件名**（唯一真相源）。
///
/// ⚠ 抽成函数不是为了好看：判据 `the_local_extract_path_is_build_id_scoped` 要断言这条命名规则，
/// 而如果判据自己**抄一份** `format!` 就成了「测自己的副本」—— 改了这里判据照样绿。
/// 本仓在别处栽过同族（`strip-comments` 那次两份手抄语义漂移）。⇒ 两边共用这一个。
///
/// # `K-R42`：名字尾巴上那个后缀
///
/// 释放出来的这份是要**被起成进程**的 ⇒ 在把扩展名当身份的平台上它得带着自己那个后缀。
/// 🔴 **后缀不是在这里现算的** —— 算它要 `env::consts::EXE_SUFFIX`，那是**平台原语**，
/// 而本文件在 `backend/backend_tests.rs::PLATFORM_EXCEPTIONS` 里**只有一格例外额度**
/// （那张表挂着递减棘轮 `len() <= 1`，今天正好占满，占的是 `resolve_beside_this_exe`）。
/// ⇒ 由 `build.rs` 从 **`TARGET`** 算好、当编译期常量交进来
/// （`CCM_TARGET_EXE_SUFFIX`，同 `CCM_TARGET_TRIPLE` 那条先例）。
/// 顺带买到一件现算买不到的事：交叉编译时 `env::consts::` 给的是**构建机**的后缀，
/// 而这里要的是**目标机**的。
/// ⚠ 非 Windows 上这个常量是**空串** ⇒ 名字与本行改动之前**逐字相同**，盘上已有的那份照旧命中。
pub fn local_extract_name(build_id: &str) -> String {
    format!(
        "cc-monitor-backend-{build_id}{}",
        env!("CCM_TARGET_EXE_SUFFIX")
    )
}

// 〔DP1 · 第四波〕这里原来住着这一份产物按 `TARGET` 内嵌的本机后端那一槽（`K-R42`）。它搬进了 `src/bridge/src/byte_table.rs`：全仓只有一处按 (OS, arch) 取字节（`设计/96 §7.1.1b`），
//   本层不再自己问「这份产物带没带」—— 宿主从那张表取来、经 `embedded` 交进来，本层只管释放。

/// 陈旧 `.partial` 的年龄阈值。
///
/// 释放一份后端是**一次几 MB 的顺序写**（本机实测 2.4 MB），正常在毫秒级完成。
/// 24 小时给的是**五个数量级**的余量 —— 宁可多留一天垃圾，也不要在某台慢机器上
/// 把**正在写的**那份删掉（那会把这道防线变成它自己要防的东西）。
const STALE_PARTIAL_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// 收掉**够老**的 `.partial` 残骸（崩在 rename 之前留下的）。
///
/// ⚠ **只按年龄**，不按 pid 活没活 —— 后者是平台知识，`C10` 不许它住在 backend。
/// ⚠ 只认自己这套命名（`.<释放名>.<pid>.partial`）：目录是与远端自部署**共用**的
/// （`~/.cc-monitor/bin`），乱扫会碰到别人的东西。
/// ⚠ 删不掉就算了（别的进程占着 / 权限）——**清扫失败绝不能挡住释放**，那是主线动作。
fn sweep_stale_partials(dir: &std::path::Path, extract_name: &str) {
    let prefix = format!(".{extract_name}.");
    let Ok(rd) = std::fs::read_dir(dir) else {
        return; // 目录还不存在（首次释放）——没有残骸可收
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with(&prefix) || !name.ends_with(".partial") {
            continue;
        }
        let old = ent
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map(|age| age >= STALE_PARTIAL_AGE)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_file(ent.path());
        }
    }
}

pub fn extract_embedded_to(
    dir: &Path,
    build_id: &str,
    bytes: &[u8],
    make_executable: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let dest = dir.join(local_extract_name(build_id));
    // 已经在且大小对得上 ⇒ 幂等跳过（不重写，省一次 IO，也不动 mtime）。
    if let Ok(m) = std::fs::metadata(&dest) {
        if m.is_file() && m.len() == bytes.len() as u64 {
            return Ok(dest);
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    // 先写临时文件再 rename：半截文件不许被当成可执行的后端（rename 在同一文件系统上原子）。
    // ★★ 临时名**带 pid**〔`P2t` 摸底 08-12〕：原来是**固定名**，两个同版本 monitor 同时释放
    // 会写同一个 `.partial` —— 一个写到一半、另一个 `rename` 走，出来的可能是**半截文件**，
    // 而这道 `.partial` + `rename` 存在的全部理由就是「半截文件不许被当成可执行的后端起起来」。
    // ⚠ 这不是理论：`tauri_plugin_single_instance` **只在 `#[cfg(windows)]` 注册**
    // （`lib.rs::run` 里那段 `#[cfg(windows)]`）⇒ Linux/macOS 上两个 monitor 天然并存。
    // ⇒ 每个进程写自己那份，`rename` 仍是原子的，互不覆盖。
    let tmp = dir.join(format!(
        ".{}.{}.partial",
        local_extract_name(build_id),
        std::process::id()
    ));
    // 带 pid 之后，崩在中途的那些**不会再被下一次覆盖掉** ⇒ 得自己收。
    // ⚠ 不按「pid 还活着吗」判：那是**平台知识**，而本模块按 `C10` 不许认识平台
    // （`bind.rs::is_pid_alive` 在非 Windows 上恒 false，拿来用会误删活的）。
    // ⇒ 按**年龄**判，阈值给得极宽（见常量头注）。
    sweep_stale_partials(dir, &local_extract_name(build_id));
    std::fs::write(&tmp, bytes).map_err(|e| format!("写 {} 失败: {e}", tmp.display()))?;
    // `backend-split` 的 C10：**「怎么置可执行位」是平台知识，不许住在 backend**。
    // 这里只知道「写完要让它可执行」，那句话在本平台上怎么落由宿主注入
    // （`platform_fs::make_executable`）。原来这处是个 `#[cfg(unix)]` 块，
    // `the_backend_half_stays_platform_agnostic` 逮到了它。
    make_executable(&tmp)?;
    std::fs::rename(&tmp, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename 到 {} 失败: {e}", dest.display())
    })?;
    Ok(dest)
}

/// 〔RM1f〕**把本机的代码全景小程序放到 `dir/<file>`**（`dir` 由调用方给 ＝ `~/.cc-monitor/bin`，
/// 本机后端找小程序的第二个候选 —— 后端在 monitor 旁边还是在这个目录里，这一格都找得到）。
///
/// 与 [`extract_embedded_to`] 同一套落盘：暂存旁名 `.<file>.<pid>.partial` → 置可执行位（平台知识由宿主注入）
/// → `rename` 上位（半截文件不许被当成可执行的插件起起来）。
///
/// # 幂等：盘上那份**逐字节相等**才跳过
///
/// 本机后端的释放名带 `build_id`，而小程序**没有身份戳**（不随 `BUILD_ID` 走）⇒ 名字说明不了它是哪一版，
/// 只能比字节（约 20 MB 的一次顺序读，只在本机后端答「没装 / 装的太旧」时才走到这里）。
/// 字节不同 ⇒ 覆盖（旧的那份正被起着时 Windows 上 `rename` 会失败 ⇒ 如实报，下次再放）。
///
/// # 它不做什么
///
/// 不起它、不问它会什么 —— 那是本机后端插件口的事；这里只让「去盘上找」那一格有东西可找。
pub fn place_local_panorama(
    dir: &Path,
    file: &str,
    bytes: &[u8],
    make_executable: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let dest = dir.join(file);
    if let Ok(m) = std::fs::metadata(&dest) {
        if m.is_file()
            && m.len() == bytes.len() as u64
            && std::fs::read(&dest).is_ok_and(|on_disk| on_disk == bytes)
        {
            return Ok(dest);
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let tmp = dir.join(format!(".{file}.{}.partial", std::process::id()));
    sweep_stale_partials(dir, file);
    std::fs::write(&tmp, bytes).map_err(|e| format!("写 {} 失败: {e}", tmp.display()))?;
    make_executable(&tmp)?;
    std::fs::rename(&tmp, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename 到 {} 失败: {e}", dest.display())
    })?;
    Ok(dest)
}

// ── `K-R69`（09-12）：**本机的 `ccm` 入口** ──────────────────────────────
//
// 立件时现打（量于 `79bf97d`）：闭集 `tool_registry::TOOLS` 里落点是 `…/ccm` 的**只有一条**，
// 而它是 `RemoteHomeRelative(".local/bin/ccm")` ⇒ **本机侧 0 条**；装口也只有远端那一个
// （`sftp::install_remote_ccm_helper`〔散文墓碑〕，〔MC1〕今天那一半在 `sftp::deploy_remote_backend`）。⇒ 用户 `K34` 逐字要的「装了新版后
// `~/.local/bin/ccm` 可以干净退役」**今天没有承接方** —— 不是「没验过旧的能不能退役」，
// 是**本机压根没有新的那一份**。
//
// 🔴 **它不是第二个 `ccm`**。`K33` 逐字「不要有什么单独的 ccm，所有命令只许有一处」，
//    而 `K-R48` 花两拍删掉的正是那份 1592 行的 bash。本机这条落点是**后端二进制自己**，
//    只是换了个名字：`control::ccm::intercept` 的入口①逐字写着「`argv[0]` 的 basename
//    是 `ccm`（别名 / 软链 / **改名拷贝**指过来）」。⇒ 零新增实现、零新增 argv 解析。

/// 🔴 `ccm` 这个词的**唯一住址**〔`13b`：闭集只许有一个住址〕。
///
/// 本机与远端两条落点都取自这里：
/// · **本机** —— 它是文件名（[`local_ccm_entry_name`]）⇒ 走 `intercept` 的入口①（basename）；
/// · **远端** —— 它是 shim 里交给后端的那个子命令（[`ccm_entry_shim`]）⇒ 走入口②（`<bin> ccm …`）。
///
/// ⇒ **「两条同源」不是一句声明**：它们把 argv 交给的是同一份后端里的同一处解析
/// （`remote_daemon_proto::control::ccm::intercept`），两条路上一处第二实现都没有。
pub const CCM_ENTRY_WORD: &str = "ccm";

/// 本机 `ccm` 入口的**文件名**（唯一真相源，判据与生产共用这一个）。
///
/// 后缀与 [`local_extract_name`] 同一个来路：`build.rs` 按 **`TARGET`** 算好的编译期常量
/// `CCM_TARGET_EXE_SUFFIX`。这一份是要**被起成进程**的 ⇒ 在把扩展名当身份的平台上
/// 它得带着自己那个后缀。
/// ⚠ 这里**不许**现算 `env::consts::EXE_SUFFIX` —— 那是平台原语，而本文件在
/// `backend/backend_tests.rs::PLATFORM_EXCEPTIONS` 上只有一格例外额度，今天已被
/// `resolve_beside_this_exe` 占满（那张表挂着递减棘轮 `len() <= 1`）。
pub fn local_ccm_entry_name() -> String {
    format!("{CCM_ENTRY_WORD}{}", env!("CCM_TARGET_EXE_SUFFIX"))
}

/// 远端 `~/.local/bin/ccm` 的内容：**一个入口，不是一份实现**。
///
/// 🔴 **它里面不许有第二个 `case` / `if` / 任何行为** —— 一旦有，`K33` 那句
/// 「所有命令只许有一处」就又破了，而这正是 `K-R48` 这一整件要根除的东西。
/// 它做且只做一件事：把 argv 原样交给后端。
///
/// **为什么不是软链**：软链更干净（`intercept` 头一条入口逐字写着「别名 / 软链指过来」），
/// 但本仓的 SFTP 客户端今天**一处都没用过 `symlink`**，而这条路**没有任何一台真远端机器可以验**
/// （`K-R48` `§0-Bx-7` 同族）⇒ 用已经被 12 条 print-parity + 真机验收盯过的 `upload_atomic`
/// 那条路，把「没验过的新机制」这个变量拿掉。换软链是一件独立的活，别搭在这一拍上。
///
/// # 🔴 `K-R69` 09-12：它从 `sftp.rs` 搬到这里，理由是**它有了第二个读者**
///
/// 从前只有远端那条装口读它，住在 `sftp.rs` 里刚好；今天本机也要一条入口，
/// 而「本机那条与远端那条同源」这句话**只有在两边取自同一处时才是结构性的**。
/// ⇒ 生成器与 [`CCM_ENTRY_WORD`] 一起住在后端层，`sftp.rs` 改成调它。
/// ⚠ 搬过来**没有**把平台知识带进 `backend/`：它是个纯字符串生成器，
/// 一处 `cfg`、一处平台原语都没有（`the_backend_half_stays_platform_agnostic` 照旧绿）。
/// # 〔MC1 · 2026-09-24〕它不再传 `CCM_SELF`（09-15 加的那一行删了）
///
/// 09-15 真机逮到过：容器路（`--tmux`）的**内层命令**以「我是被怎么叫的」开头，而那时它只取
/// `argv[0]` ⇒ 经本 shim `exec <后端> ccm …` 进来时内层变成 `cc-monitor-backend --cwd …`，
/// **缺了 `ccm` 这个子命令词**，当场 `unknown argument: --cwd`。补法是让 shim 先
/// `CCM_SELF="${CCM_SELF:-$0}"` 再 exec。
/// ⇒ CC1 把根治放进了后端：`control/ccm/mod.rs::self_invocation` 从进程 argv 里取「被 `intercept`
/// 吃掉的那一段」，入口② 自己就带着那个词 ⇒ 那个环境变量没有要补的东西了，`设计/01 §6.7b`
/// 逐字「`CCM_SELF` 这个环境变量随之删掉」—— **shim 制造了它自己要解决的那个问题**。
/// 代价如实写：pane 里显示的是 `<后端真身> ccm …` 而不是用户 PATH 上那个 `ccm` 名字（行为一样）。
/// 🔴 shim 本身还在：「`ccm` 就是后端二进制本身」卡在写区外（`tests/evidence/MC1-AL1-摸底.md` 第三节）。
pub fn ccm_entry_shim(backend_path: &str) -> String {
    format!(
        "#!/bin/sh\n# cc-monitor: {CCM_ENTRY_WORD} = 后端本体的一次性模式（K33：所有命令只许有一处）\nexec {} {CCM_ENTRY_WORD} \"$@\"\n",
        shell_quote_core::posix_quote(backend_path)
    )
}

/// 🔴 `K-R69`：把**本机的 `ccm` 入口**放到后端二进制旁边（`dir` 由调用方给 ＝ `~/.cc-monitor/bin`）。
///
/// # 它写的是什么：`backend_bin` 的**逐字节副本**，改名成 [`local_ccm_entry_name`]
///
/// 三条路各自为什么不走，写清楚免得下一个人以为是随手选的：
/// · **不写一个壳** —— `K33` 逐字「所有命令只许有一处」。多一份壳就多一处要跟着改的东西，
///   而 `KR69D1` 的失效方向逐字写着「在本机再写一个 `ccm` 壳 ⇒ 不算兑现」。
/// · **不写 shim** —— 远端那条只能是 shim（后端落点由用户配置的 `backend_path` 决定，
///   而且推过去的是文本）；本机这一份的字节**我们手里就有**，直接给它一个名字最省。
///   而且 `#!/bin/sh` 那一形在 Windows 上根本起不来，本层不许认识平台（`C10`）。
/// · **不做软链** —— `std::os::unix::fs::symlink` 与 Windows 那条都是**平台原语**，
///   本文件的例外额度已被占满（见 [`local_ccm_entry_name`]）。
///
/// # 落点为什么是 `~/.cc-monitor/bin`，不是 `~/.local/bin`
///
/// 后者是**用户那份旧 `ccm` 住的地方**。往那儿写就是覆盖用户的文件，而 `K34` 逐字
/// 「原本的配置**要手动删除**」、`K31`「不许动用户机器」⇒ **产品一个字节都不动它**。
/// 写进 monitor 自己的目录还买到第二件事：两份**同时在盘上**，
/// 「你 PATH 上那个不是我们装的这一份」才有得可判（`KR69D2`）。
///
/// # 幂等
///
/// 已经在、长度与后端相同、且**不比后端旧** ⇒ 跳过。否则写 `.<名字>.<pid>.partial`
/// → 置可执行位 → `rename` 覆盖（与 [`extract_embedded_to`] 同一套，理由住那儿）。
/// ⚠ **这两条不是「内容相同」的证明，是便宜的止损**：长度相同的两版二进制是可能的，
/// 所以第二条要「不比后端旧」——换了一版后端，释放出来那份是新的 ⇒ 这一份跟着重写。
/// 真正的保证在写的那一步：整份字节**从 `backend_bin` 读**，没有第二个来源。
///
/// # 它不做什么
///
/// **不碰 PATH、不碰任何 rc、不碰用户的 `~/.local/bin`。** 「怎么让终端里那句 `ccm`
/// 指到它」是别名层与 `KR69D3` 的事，不是这里。
pub fn install_local_ccm_entry(
    dir: &Path,
    backend_bin: &Path,
    make_executable: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let name = local_ccm_entry_name();
    let dest = dir.join(&name);
    // 后端自己就叫 `ccm`（有人把它改名部署了）⇒ 本机那条落点**已经是它**，没有第二份要放。
    if dest == backend_bin {
        return Ok(dest);
    }
    let src_meta = std::fs::metadata(backend_bin)
        .map_err(|e| format!("读不到后端二进制 {}: {e}", backend_bin.display()))?;
    if let Ok(m) = std::fs::metadata(&dest) {
        if m.is_file() && m.len() == src_meta.len() && !older_than(&m, &src_meta) {
            return Ok(dest);
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("建目录 {} 失败: {e}", dir.display()))?;
    let tmp = dir.join(format!(".{}.{}.partial", name, std::process::id()));
    sweep_stale_partials(dir, &name);
    std::fs::copy(backend_bin, &tmp).map_err(|e| {
        format!(
            "把后端 {} 复制成本机 ccm 入口失败: {e}",
            backend_bin.display()
        )
    })?;
    make_executable(&tmp)?;
    std::fs::rename(&tmp, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("rename 到 {} 失败: {e}", dest.display())
    })?;
    Ok(dest)
}

/// 盘上那份比后端旧吗。**取不到时间就当旧的**（宁可多写一次，也不要留一份过期的入口）。
fn older_than(dest: &std::fs::Metadata, src: &std::fs::Metadata) -> bool {
    match (dest.modified(), src.modified()) {
        (Ok(d), Ok(s)) => d < s,
        _ => true,
    }
}

/// 「带着后端但放不下来」这一形的**认路标记**。
///
/// ⚠ 它是给**判据**认的，不是给用户读的措辞规范：判据钉「这一句与『没带后端』那一句
/// 分得开」，而分得开这件事得有个不靠措辞的抓手。
/// 🔴 **刻意不取自任何路径 / 目录名 / 夹具名**（固定项 12 那条 `6g`：诊断把路径原样印进输出，
/// 于是「输出里含某句话」会**靠路径恒真**，把整支实现换掉都不红）。
const EXTRACTION_REFUSED_MARKER: &str = "放不下来";

/// 🔴 `K-R42` 硬要求①：**权限写不进去时响亮失败，不许静默退回「没有后端」。**
///
/// # 为什么这不是「换个措辞」
///
/// 两件事今天共用一个返回形状（[`Resolved::Missing`]），而它们的**下一步完全不同**：
///
/// | 这一形 | 用户该干嘛 |
/// |---|---|
/// | 这份产物**没带**本机后端 | 去装一次安装包 |
/// | 带了，但**这台机器不让放** | 去看那个目录的权限 / 杀毒软件；或者装安装包绕开它 |
///
/// 把后者说成前者，就是 09-10 一整天在治的那一形（读面把「读不到」说成「你没有」）。
/// ⇒ 这一支自己拼一句**结构上分得开**的话（[`EXTRACTION_REFUSED_MARKER`]），
/// 并由 [`resolve_or_extract`] 在返回之前 `tracing::error!` 吼一声 ——
/// 〔`K-R43` 订正住址：那一声原先在 [`start_or_extract`] 体内，抽进共用那份之后
///  **两条生产路共用这一声** —— 常驻那条（`local_backend_host.rs`）从此也吼得出来〕
/// 光靠返回值不够：调用方可能只把它记进 `info`。
///
/// # 纯函数
///
/// 不碰文件系统、不碰时钟 ⇒ 两种输入的两句话都测得到（同本模块 [`decide`] 的理由）。
pub fn extraction_failure_reason(dir: &Path, err: &str) -> String {
    format!(
        "这一份 monitor **自己带着**本机后端，但它{EXTRACTION_REFUSED_MARKER} —— \
         往 `{}` 里写的时候失败了（{err}）。\n\
         🔴 这**不是**「这份产物没有本机后端」：那是另一回事，下一步也不一样。\n\
         ⇒ 下一步挑一条：① 看那个目录是不是只读、或者被杀毒软件挡着，给它写权限；\
         ② 让 monitor 跑在一个家目录写得进去的账号下；\
         ③ 不想动它就**装一次安装包**（Releases 页）—— 装出来的那份后端与 monitor 同目录，\
         根本不用写这里。",
        dir.display()
    )
}

/// **生产入口**：找得到就起并看住；找不到就**诚实降级**（定框 §5）。
///
/// ⚠ **走哪一支取决于用户手里是哪一份产物**〔订正 2026-09-10 现打，v3.7.0〕：
/// 安装包（NSIS / MSI）里**带着** local_backend —— 干净 win11 虚拟机上装完现打，
/// `C:\Program Files\cc-monitor\` 下 `cc-monitor-backend.exe` **2 个进程在跑**；
/// 而**裸 `monitor.exe`** 那份 **0 个**，走的才是降级那一支。
/// 〔本行原话「今天恒走降级那一支 —— 安装包里还没有本机后端（`externalBin` 是 F05b）」
/// 已被那次读数证伪。`externalBin` 配着，只是住 `src/bridge/tauri.sidecar.conf.json`
/// 而不是基础 `tauri.conf.json` —— 分工见模块头注。〕
/// 降级不是「接线没做」，是**接线做了、这一份产物里没带**：两者的区别就在这个返回值上，
/// 调用方能把 `reason` 与 `looked_at` 原样记进日志。
pub fn start_if_present(
    target_triple: &str,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
    spawn: Arc<crate::spawn_managed::ManagedSpawn>,
) -> (Resolved, Option<SuperviseHandle>) {
    let r = resolve_beside_this_exe(target_triple);
    let Resolved::Found(bin) = &r else {
        return (r, None);
    };
    // P2：本机后端**起来就带入方向通道** —— 这一步不是可选项，也不由宿主决定。
    // 「本机 = 不走 ssh 的远端」（`INVARIANTS §40`）：远端一连上就 attach 通道，本机同理。
    let h = supervise_with_stdio(
        bin.clone(),
        LOCAL_STREAM_ARGS.iter().map(|a| a.to_string()).collect(),
        Vec::new(),
        CrashLimits::default(),
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        }),
        on_event,
        Some(Arc::new(local_stdio_consumer_guarded)),
        spawn,
    );
    (r, Some(h))
}

/// [`local_stdio_consumer`] 用的**同步有界读行** —— 远端 `ssh_source::read_capped_line` 的孪生。
///
/// # 为什么不复用那一份
///
/// 那一份是 `async`（吃 `AsyncBufRead`），而本机消费者跑在**裸 `std::thread`** 上。
/// 机制可以同构，代码跨不了 sync/async 这道边。
/// ⇒ **共用的是上限常量**（`ssh_source::BACKEND_FRAME_LINE_CAP`），那才是会漂的东西；
/// 机制各写一份，两边头注互指。
///
/// # 机制：`fill_buf`/`consume`，超限之后只找换行、不再往 buf 里塞字节
///
/// 逐字抄远端那条头注记的教训：backend 侧第一版用无界 `read_until`、读完再看长度，
/// D 审计实测**喂 512 MiB 无换行的流 ⇒ RSS 从 6 MiB 涨到 518 MiB**，
/// 而它照样回了一条「看起来对」的 `line_too_long`。
///
/// # 返回
///
/// `Ok(None)` = EOF（**判死信号**）· `Ok(Some(s))` = 一行（超限的整行丢弃，回空串）·
/// `Err` = 真的读错误。
///
/// ⚠ **字节转字符串走 `from_utf8_lossy`**〔D 阶段补审 08-11 修〕：
/// 原版用 `BufReader::lines()`，那是 **UTF-8 严格**的，一个坏字节就回 `InvalidData`，
/// 而调用方把它和 EOF 一起 `break` ⇒ backend 被我们读死、还被记成一次「崩溃」，
/// 三次之后**整个进程周期不再起来**，日志写「崩了 3 次」——**一个错误的诊断**。
/// 远端那条路早就明确取了相反的取舍（`ssh_source` 里 `from_utf8_lossy`，注释逐字
/// 「非 UTF-8 不该让整条连接死掉」），本机这条当时把它漏了。
fn read_capped_line_sync<R: std::io::BufRead>(
    rd: &mut R,
    cap: usize,
) -> std::io::Result<Option<String>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut seen: usize = 0;
    let mut overflowed = false;
    loop {
        let chunk = match rd.fill_buf() {
            Ok(c) => c,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if chunk.is_empty() {
            return Ok(if seen == 0 {
                None // 真 EOF
            } else if overflowed {
                Some(String::new())
            } else {
                Some(String::from_utf8_lossy(&buf).into_owned())
            });
        }
        let (take, done) = match chunk.iter().position(|&c| c == b'\n') {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        seen += take;
        if seen > cap {
            overflowed = true;
        }
        if !overflowed {
            buf.extend_from_slice(&chunk[..take]);
        }
        let consume = if done { take + 1 } else { take };
        rd.consume(consume);
        if done {
            return Ok(if overflowed {
                Some(String::new())
            } else {
                Some(String::from_utf8_lossy(&buf).into_owned())
            });
        }
    }
}

/// 〔CF1 · 2026-09-24〕本机后端的**流模式起参** —— 两条载体（stdio 监护 · 常驻脱离）共用这一份。
///
/// - `--tail-only`：连接不重放历史，历史由 monitor 经旁路快照拉（与远端同一套：`ssh_source::LineIntake`）。
///   本机内容消费者据此认定「这条流恒为 tail-only」（`ssh_source::LOCAL_STREAM_TAIL_ONLY`）。
/// - `--with-bg`：bg 会话也宣告、也发行 —— 本机会话内容从这条流来之后，少了它 bg 会话的内容就静默没了
///   （monitor 的 `showBgSessions` 缺省是开的）。显示与否在 monitor 那一侧按 `session_kind` 定。
///
/// 两个字面量都必须是后端 `lib.rs::STREAM_FLAGS` 的成员（后端据它剥旗标；不认的会被当成一次性查询跑完就退）——
/// 由判据对拍后端源码。
pub(crate) const LOCAL_STREAM_ARGS: &[&str] = &["--tail-only", "--with-bg"];

/// P3 刀 1 的**唯一**吸收点：本机后端推来的帧里，哪些要进账本。
///
/// # 为什么抽成函数〔`P3` 08-12〕
///
/// 原来这三行**长在读行循环里**，而那个循环要有一个**真的在跑的 backend** 才进得去
/// ⇒ `P3-Y1`（acceptor: **实测**）唯一的证据只能是一条会起真 tmux 的测试，而那条
/// 08-11 出过**误伤用户 9 个真实会话**的事故后被 `#[ignore]` 了 ——
/// 于是这条 DoD **至今没有兑现证据**（如实登记在件的 `§0h`：「不是『测试暂时关着』，
/// 是这条 DoD 今天没有兑现」）。
///
/// 抽出来之后，「帧 → 账本」这一跳**不需要后端、不需要 tmux** 就能验 ——
/// 与 `P5L` 把终端出口做成入参是同一手：**把够得到的那半做成可测，别拿够不到的当借口**。
///
/// ⚠ **射程如实登记**：本函数可测的是「**收到帧之后**账本里有」。
/// 「backend **真的会发**这个帧」仍归后端侧 `EMITS "tmux_sessions"` 的登记
/// （逐字「登记 = 承诺真发」）与协议文档守卫 —— 那一跳本判据**够不到**，
/// 那条会起真 tmux 的实测因此**留着**（仍 `#[ignore]`），不是删掉了事。
///
/// 〔SR1a · 2026-09-24〕它从此是本机那条流上**所有**非 hello 帧的吸收点（两条读循环各调一次：
/// 本文件的 [`local_stdio_consumer`] 与宿主的 `local_backend_host::attach_stream`）：
/// - `reply` / `cancelled` ⇒ 交本机那条入方向客户端按 `id` 路由回请求方。
///   ⚠ **此前本机两条读循环一条都不路由应答**（现打逐行读过）⇒ 在本机那条通道上发出去的入方向命令
///   **只会等到超时**。开链路（`link-open`）要这条应答，所以这一格在这一拍补上。
/// - `link_data` / `link_end` ⇒ 交 [`crate::link_mux`]（链路的 monitor 这一侧）。
/// - 〔SR1b〕`transfer` ⇒ 交 [`crate::sftp_pool::deliver`]（传输台的中继）。
///
/// 〔CF1 · 2026-09-24〕**内容三种（`line` · `session_added` · `session_removed`）原样交回调用方**
/// （返回 `Some`），由读循环送进 `crate::local_lines`（再进与远端同一个 `LineIntake`）。
/// 此前这三种落在最后那个 `_ => {}` 里整个丢掉（注释逐字「本机会话走本地 watcher」）——
/// 那是「同一批 jsonl 同机读两遍、第二遍扔掉」的那一半（`真相源/10 §7.1`）。
/// ⚠ 交回而不是在这里就地送：两条读循环一条是 tokio 任务、一条是裸线程，送法不同（`local_lines` 头注）。
/// 其余帧仍就地吸收，返回 `None`。
pub(crate) fn absorb_local_frame(
    frame: crate::ssh_source::InboundFrame,
    client: Option<&std::sync::Arc<crate::backend::control::inbound_client::InboundClient>>,
) -> Option<crate::ssh_source::InboundFrame> {
    use crate::ssh_source::InboundFrame;
    match frame {
        // P3 刀 1：**本机的 tmux 帧也要收**。backend 的 `watch_loop` 周期跑本机 `tmux ls`
        // 并推 `TmuxSessions` 帧。P2 写这个消费者时只需要通道，把非 hello 帧全丢了 ——
        // 于是**本机 tmux 会话对 monitor 不可见，不是拿不到，是我们扔了**。
        //
        // ⚠ 收它有前置：本地 sid 进这张表之后，`/branch` 会走 `(Some(origin), …)`
        // ⇒ 必须先有「本地也判得出 `Superseded`」（P3 刀 0）。没有刀 0 就收帧 =
        // 把「永远消不掉的灰点」那个 bug 请回来。
        //
        // 〔U4b · 第四波 · G2〕本机也有「可重连」了（`lib.rs` 本机那一臂改走 `classify_removed`）⇒
        //   「可重连 → 已结束」要有产出者，与远端 `stream_loop` 的收帧收割器同一个判定（`reconcile_step`）。
        //   结论交 `session_facts::retire_local_idle`（写 idle 账本只许在 `lib.rs`）。
        InboundFrame::TmuxSessions { raw, observation } => {
            let origin = crate::backend::control::inbound_client::LOCAL_ORIGIN;
            let idle = crate::ssh_source::snapshot_idle_for_origin(origin);
            let retire = {
                let mut st = local_reaper_state()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                local_idle_retirements(&mut st, &raw, observation.as_deref(), &idle)
            };
            crate::ssh_source::record_tmux_raw(
                crate::backend::control::inbound_client::LOCAL_ORIGIN,
                raw,
            );
            let n_retire = retire.len();
            if n_retire > 0 {
                tracing::info!("tmux-reconcile(本机): retire {n_retire} 条可重连会话（本机 tmux 里已不见它们）");
                crate::session_facts::retire_local_idle(retire);
            }
        }
        // 〔U4b · 第四波 · G2〕本机 tmux 会话关了（正向死亡帧）—— 此前这一帧在本机这条流上被直接丢掉。
        //   与远端同序：**先把那一行从账本摘掉，再退** —— 否则 claude 随后退出时 `classify_removed`
        //   仍看得见它 ⇒ 判成可重连（`#60` 现象 2 的本机版）。那条会话若已是可重连 ⇒ 立刻落已结束。
        InboundFrame::TmuxSessionClosed { name } => {
            let origin = crate::backend::control::inbound_client::LOCAL_ORIGIN;
            if let Some(raw) = crate::ssh_source::tmux_raw_for(origin) {
                let (rest, sid) = local_tmux_closed(&raw, &name);
                crate::ssh_source::record_tmux_raw(
                    crate::backend::control::inbound_client::LOCAL_ORIGIN,
                    rest,
                );
                let idle_now = crate::ssh_source::snapshot_idle_for_origin(origin);
                if let Some(sid) = sid.filter(|s| idle_now.contains(s)) {
                    tracing::info!("本机 tmux 会话 {name} 关闭 ⇒ 可重连的 sid={sid} 落已结束");
                    crate::session_facts::retire_local_idle(vec![sid]);
                }
            }
        }
        // 〔U4b · 第四波 · G3〕本机活会话的容器事实：与远端流同一个口（`session_facts`）、同一个事件。
        // 〔CF1 · 2026-09-24〕记完容器，这一帧**照样交回**读循环 —— 它的 `path` / `lines` 是本机旁路快照的起点
        //   （本机会话的行从此走这条流，见下面内容三种那一臂）；本机会话的起停仍归 `session_map`。
        f @ InboundFrame::SessionAdded { .. } => {
            if let InboundFrame::SessionAdded { sid, container, .. } = &f {
                crate::session_facts::note_container(sid, *container);
            }
            return Some(f);
        }
        InboundFrame::Reply {
            id,
            ok,
            code,
            message,
            data,
        } => match client {
            Some(c) => {
                c.route_reply(&id, ok, code, message, data);
            }
            None => tracing::warn!("本机后端在 hello 之前就回了应答（id={id}）—— 协议倒错，丢掉"),
        },
        InboundFrame::Cancelled { id } => match client {
            Some(c) => {
                c.route_cancelled(&id);
            }
            None => tracing::warn!("本机后端在 hello 之前就回了 cancelled（id={id}）—— 丢掉"),
        },
        InboundFrame::LinkData { link, data } => crate::link_mux::deliver_data(&link, data),
        InboundFrame::LinkEnd { link, error } => crate::link_mux::deliver_end(&link, error),
        // 〔SR1b〕传输台住本机后端：进度 / 终局交中继（`sftp_pool::deliver`，从不阻塞）。
        InboundFrame::Transfer {
            id,
            got,
            total,
            end,
        } => crate::sftp_pool::deliver(&id, got, total, end),
        // 〔CF1〕内容三种（`session_added` 在上面那一臂记完容器也交回）：交回读循环，送进本机内容通道。
        f @ (InboundFrame::Line { .. } | InboundFrame::SessionRemoved { .. }) => return Some(f),
        // 其余帧（hello · 会话状态 · 溢出 …）本机这条流今天不消费。
        _ => {}
    }
    None
}

/// 〔U4b · 第四波 · G2〕本机收割器的对账状态（跨帧累计缺失计数）。一份常驻：本机只有一条后端流，
/// 流断了重连后 idle 集没变，计数接着累计是对的（与远端「每连接一份」不同：远端断连有 flush 兜底归档）。
fn local_reaper_state() -> &'static std::sync::Mutex<crate::tmux_reconcile::ReconcileState> {
    static STATE: std::sync::OnceLock<std::sync::Mutex<crate::tmux_reconcile::ReconcileState>> =
        std::sync::OnceLock::new();
    STATE.get_or_init(Default::default)
}

/// 〔U4b · 第四波 · G2〕**本机收割的纯决策**：这一帧 `tmux ls` 之后，哪些本机「可重连」该落到已结束。
///
/// 与远端 `stream_loop` 的 `TmuxSessions` 臂同一个判定：观测无效（`Skip`）⇒ 本帧不算；有效 ⇒
/// `reconcile_step`（去抖 `RETIRE_MISS_THRESHOLD` 拍）。`tracked` 与 `pre_bound` 都是**本机 idle 集**：
/// 本机活会话的死活由 monitor 的 `session_map`（pidfile）判，不归这里管（远端 `tracked` 里的 `announced` 在本机没有对应物）；
/// idle sid 的 `@ccm_sid` 在原文里出现过 = 铁证绑过 tmux ⇒ 直接播种 `ever_bound`（同远端那条 D 审计②）。
pub(crate) fn local_idle_retirements(
    state: &mut crate::tmux_reconcile::ReconcileState,
    raw: &str,
    observation: Option<&str>,
    idle: &std::collections::HashSet<String>,
) -> Vec<String> {
    match crate::backend::control::tmux::classify_tmux_observation(raw, observation) {
        crate::backend::control::tmux::TmuxObservation::Backend(backend) => {
            crate::tmux_reconcile::reconcile_step(
                state,
                idle,
                &backend,
                idle,
                crate::tmux_reconcile::RETIRE_MISS_THRESHOLD,
            )
        }
        crate::backend::control::tmux::TmuxObservation::Skip(_) => Vec::new(),
    }
}

/// 〔U4b · 第四波 · G2〕本机 tmux 会话 `name` 关了：摘掉那一行后的原文 ＋ 那一行上挂着的 sid（纯函数）。
/// 按名字**逐字相等**找（`remove_tmux_line` 同一条纪律：不做前缀匹配）。
pub(crate) fn local_tmux_closed(raw: &str, name: &str) -> (String, Option<String>) {
    let sid = crate::backend::control::tmux::parse_tmux_ls(raw)
        .into_iter()
        .find(|e| e.name == name)
        .and_then(|e| e.sid);
    (crate::ssh_source::remove_tmux_line(raw, name), sid)
}

/// # 诚实边界 10a + 10e：通道**通了**，但没人往里发命令，也没验命令真能执行
///
/// 10a：`client_for("<local>")` 今天的生产消费者**只有读**（`backend_status` 问「通道在不在」）。
/// **一条入方向命令都还没有人从本机发出去** —— 那是 P3 刀 2/3 的活。
/// 「通道在」与「控制面走通了」是两件事，别把前者当成后者的证据。
///
/// 10e：起后端时只验了它**照发 hello、且声明接受 `launch`/`kill`**。
/// **没验入方向命令真的被执行** —— 要等 P3 有真实调用方才验得动。
/// ⇒ 今天的证据链止于「对面说它接受」，不含「它真的做了」。
///
/// P2（定框 C1/C4）：**本机后端的 stdio 消费者** —— 把这条命的 stdin/stdout 接成入方向通道。
///
/// # 它做的事只有一点胶水
///
/// 按行读 stdout → `parse_frame` → **首帧是 hello** 就 `park_owned_writer(stdin).into_client(见证)`
/// + `register(LOCAL_ORIGIN, …)`；其余帧丢弃；读到 EOF 返回 ——
/// **返回就等于流结束，也就是 `supervise` 的判死信号**（与缺省那支 `io::copy → sink` 同一个事件）。
///
/// # 复用边界（为什么不用 `stream_loop`）
///
/// `ssh_source::stream_loop` **不是泛型**：它吃 `&RemoteConfig` + `&tauri::AppHandle`，
/// 还管重放 / 会话变更 / 连接状态 / hello 确认。本机没有那些。
/// 但它下面那三个零件**是纯的**，本函数复用的正是它们：
/// `parse_frame(&str) -> Option<InboundFrame>` · `BackendHello::from_hello_frame(&InboundFrame)` ·
/// `ParkedWriter::into_client(BackendHello)`。
/// ⇒ 复用**纯零件**，不把一个绑传输的循环硬掰成泛型。
///
/// # 为什么要 `block_on` 一下
///
/// `tokio::process::ChildStdin::from_std` 要**把 fd 注册进 IO driver**，因此必须在 runtime
/// 上下文里调。而 `supervise` 那条是**裸 `std::thread`**（模块头注说明了它为什么不是异步的）。
/// ⇒ 借 `tauri::async_runtime::block_on` 进一次上下文，**只包住这一次转换**，不把整条循环异步化。
///
/// # 不缓冲
///
/// 逐行读、读完即弃。backend 是持续产帧的，攒任何东西都是无界增长。
pub(crate) fn local_stdio_consumer(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
) -> ConsumerReport {
    use std::io::BufRead;

    let stdin =
        match tauri::async_runtime::block_on(
            async move { tokio::process::ChildStdin::from_std(stdin) },
        ) {
            Ok(w) => w,
            Err(e) => {
                // 转换失败 ⇒ 写不出去，但**stdout 还得读到底**（那是判死信号）。
                tracing::warn!("本机 stdin 转 tokio 失败（{e}）；入方向通道不登记，仍读完 stdout");
                let mut o = stdout;
                let copied = std::io::copy(&mut o, &mut std::io::sink());
                // ⚠ `K-P3b`：这一支两维都**观测得到**，如实交 ——
                //   「一句 hello 都没解过」是真的（这条路根本没进解帧循环），
                //   而读端怎么结束的就是 `copy` 的返回值。
                return ConsumerReport {
                    exit: ConsumerExit::Eof,
                    witness: StreamWitness::Observed {
                        handshake: crate::backend_policy::Handshake::NeverSpoke,
                        reader: match copied {
                            Ok(_) => crate::backend_policy::ReaderEnd::CleanEof,
                            Err(e) => crate::backend_policy::ReaderEnd::Broken(e.to_string()),
                        },
                    },
                };
            }
        };
    let mut parked = Some(crate::backend::control::inbound_client::park_owned_writer(
        stdin,
    ));
    // 留一份副本给 `unregister` —— 它要 `&Arc` 比对身份（「不摘别人的 client」）。
    let mut registered: Option<
        std::sync::Arc<crate::backend::control::inbound_client::InboundClient>,
    > = None;
    let mut early = false;
    // ★ `K-P3b`：**我们这一侧的读端怎么结束的** —— 观测在这里，判在宿主层。
    //   初值是「干净 EOF」，而它**只在真的读到 EOF 时才成立**：下面那条 `Err` 支
    //   会把它换成 `Broken`，两个出口各写各的，没有第三条路能带着初值出去。
    let mut reader_end = crate::backend_policy::ReaderEnd::CleanEof;

    let mut rd = std::io::BufReader::new(stdout);
    loop {
        let line = match read_capped_line_sync(&mut rd, crate::ssh_source::BACKEND_FRAME_LINE_CAP) {
            Ok(Some(l)) => l,
            Ok(None) => break, // EOF = 流结束 = 判死
            Err(e) => {
                // 真读错误（不是坏字节 —— 那条已被 `from_utf8_lossy` 吸收）。
                // ⚠ **子进程可能还活着** ⇒ 报 `Early`，让 `supervise` 补一刀（B4）。
                tracing::warn!("本机后端 stdout 读错误（{e}）；按早退处理");
                // ★ `K-P3b`：那句错**原样**带上去 —— 账上那一行要它
                //   （`B1` 逐字：「读坏了」说的是我们这一侧，**不算它崩了一次**）。
                reader_end = crate::backend_policy::ReaderEnd::Broken(e.to_string());
                early = true;
                break;
            }
        };
        if line.is_empty() {
            continue; // 超长行已整行丢弃，或空行
        }
        let Some(frame) = crate::ssh_source::parse_frame(&line) else {
            continue;
        };
        // 还没登记时先看它是不是 hello；不是 hello（或已经登记过了）⇒ 交吸收点。
        // P3 刀 1 ＋〔SR1a〕应答与链路帧：**理由与前置条件写在 `absorb_local_frame` 的头注上**
        // ——〔08-12〕抽函数时这段散文一度**两处各一份**，那是第二份真相源，收敛掉。
        let witness = if parked.is_some() {
            crate::backend::control::inbound_client::BackendHello::from_hello_frame(&frame)
        } else {
            None
        };
        let Some(witness) = witness else {
            // 〔CF1〕交回来的内容帧送进本机内容通道 —— 这是裸线程 ⇒ `_blocking` 那一形。
            if let Some(f) = absorb_local_frame(frame, registered.as_ref()) {
                crate::local_lines::deliver_blocking(f);
            }
            continue;
        };
        // 日志取自**帧**而不是 client —— `InboundClient` 的 `commands` 是私有的，
        // 为了打一行日志去开访问器是把封装换成方便。帧的字段本来就是公开的。
        let (build_id, commands) = match &frame {
            crate::ssh_source::InboundFrame::Hello {
                build_id, commands, ..
            } => (build_id.clone(), commands.clone()),
            // `from_hello_frame` 只对 `Hello` 返回 `Some` ⇒ 走不到这里。
            _ => (String::new(), Vec::new()),
        };
        let client = parked
            .take()
            .expect("上面刚判过 is_some")
            .into_client(witness);
        crate::backend::control::inbound_client::register(
            crate::backend::control::inbound_client::LOCAL_ORIGIN,
            client.clone(),
        );
        registered = Some(client);
        tracing::info!(
            "本机入方向通道已登记：origin={} build_id={build_id} commands={commands:?}",
            crate::backend::control::inbound_client::LOCAL_ORIGIN
        );
    }

    // 〔SR1a〕流没了 ⇒ 经它开的在飞链路全部带原因结束（不让调用方干等到超时）。
    if let Some(mine) = registered.as_ref() {
        crate::link_mux::fail_owned_by(mine, "本机后端的流断了（stdio 载体）");
        // 〔SR1b〕经它开的传输也一律收场（后端的票表随那条流一起撤了）。
        crate::sftp_pool::fail_owned_by(mine, "本机后端的流断了（stdio 载体）");
    }
    // 〔CF1〕告诉本机内容消费者这条流结束了（冲掉残批、下一条流换新的收口）。
    crate::local_lines::stream_ended_blocking();
    // ★ `K-P3b`：**「它跟我们说过话没有」的唯一变真处就是上面那一行 `registered = Some(client)`**
    //   —— 而那一行只在 `BackendHello::from_hello_frame` 给出见证之后才跑得到。
    //   ⇒ 这一维是**观测**，不是默认值：把它在这里读一次，别在别处猜。
    let handshake = if registered.is_some() {
        crate::backend_policy::Handshake::Spoke
    } else {
        crate::backend_policy::Handshake::NeverSpoke
    };
    // 流结束 ⇒ 摘掉登记，别在表里留一个写不进去的 client。
    if let Some(mine) = registered {
        crate::backend::control::inbound_client::unregister(
            crate::backend::control::inbound_client::LOCAL_ORIGIN,
            &mine,
        );
    }
    // ★ **本机那份 tmux 原文也要清**〔D 阶段补审 08-11，判据路〕。
    //
    // 远端断连早就清了（`ssh_source` 里 Batch9-F28 那处），而本机这侧流结束时
    // **只摘入方向 client、不碰这张表** ⇒ 停掉本机后端之后 `<local>` 那份原文永久留着，
    // 成了「tmux 还在」的**陈旧证据**：`find_tmux_origin_for_sid` 仍返回 `Some(<local>)`
    // ⇒ `classify_removed(Some(_), Gone)` = `Idle` = 那个「永远消不掉、也 attach 不上的灰点」。
    crate::ssh_source::forget_tmux_raw(crate::backend::control::inbound_client::LOCAL_ORIGIN);
    ConsumerReport {
        exit: if early {
            ConsumerExit::Early
        } else {
            ConsumerExit::Eof
        },
        witness: StreamWitness::Observed {
            handshake,
            reader: reader_end,
        },
    }
}

/// [`local_stdio_consumer`] 的**兜底外壳**〔D 阶段补审 08-11 新增，B3〕。
///
/// # 为什么要它
///
/// 消费者跑在 `supervise` 的裸 `std::thread` 上，那里**没有 `catch_unwind`**。
/// 它体内任何一次 panic（锁中毒、切片越界、`expect`）都会 unwind 出去，于是：
/// `child` 锁里还留着活的 `Child` · `pid` 没归 0 · `unregister` 被跳过
/// ⇒ 没人读后端的 stdout ⇒ 管道缓冲填满 ⇒ **backend 阻塞在 write 上冻死**。
/// 而 `backend_status` 照回 `channel: true` + 活 pid，`start_local_backend` 也拒绝重起。
/// **全绿的死锁态，没有任何一处会响。**
///
/// # 它做什么、不做什么
///
/// 只保证**「消费者返回」这件事一定发生**（返回 = 流结束 = `supervise` 的判死信号）。
/// 它**不**吞掉问题：panic 照样 LOUD 记一条 error。
/// ⚠ 它也**不**保证 `unregister` 跑过 —— panic 点可能在登记之后、摘除之前。
/// 那一格由 `inbound_client::register` 的「后来者顶掉前一个」兜住（诚实边界，见 11i）。
fn local_stdio_consumer_guarded(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
) -> ConsumerReport {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        local_stdio_consumer(stdin, stdout)
    }));
    if let Ok(report) = r {
        return report;
    }
    tracing::error!(
        "本机 stdio 消费者 panic —— 已兜住并按流结束处理。\n\
             ⚠ 不兜的话它会 unwind 出 supervise 线程，留下一个「状态全绿的死人」：\n\
         backend 还活着但没人读它的 stdout ⇒ 管道填满冻死，而 UI 显示一切正常。"
    );
    // panic 时**子进程多半还活着** ⇒ 报 `Early`，让 `supervise` 补一刀（B4）。
    // ★ `K-P3b`：那两维**随 panic 一起没了** —— `catch_unwind` 拿不回它体内的局部状态。
    //   如实报 `ConsumerGone`，不替它编一个 handshake：编出来的那个值会被
    //   `verdict` 当成 2026-07-09 判别式的第二个条件用。**如实登记的降级，不是漏洞。**
    ConsumerReport {
        exit: ConsumerExit::Early,
        witness: StreamWitness::ConsumerGone,
    }
}

/// P2z（`control-parity` 的定框 C10）：**「那个后端二进制在哪」的唯一一份答案。**
///
/// 顺序刻意是 **先找旁边、再释放**：开发构建里 `target/debug/` 旁边就有一个**更新**的二进制，
/// 那条路径优先于内嵌那份（内嵌的是打包时的快照）。
///
/// ⚠ **这个顺序也是 P2z-Y1 的验收陷阱**：dev 构建里第一步恒命中 ⇒ 不把旁边那个挪开，
/// 测到的是旧路径，而读数看起来和「释放成功」一模一样。
///
/// `embedded` 由调用方给（〔DP1〕宿主经 `byte_table::choose` 按这台机器的 (OS, arch) 取来；取不到时是那句拒绝的话）—— 本模块不认识那张表，
/// 也不认识「当前是什么 arch」，那都是宿主知识。`make_executable` 同理（`C10`）。
///
/// # `embedded` 只有一个来源（〔DP1〕）
///
/// 〔墓碑 —— `K-R42` 那一版这里写着「`embedded` 给 `None` 时还有第二个来源」：宿主那侧只在 Linux 上给 musl 字节，
///  本层在它交白卷时再问这一份产物按 `TARGET` 内嵌的那一份（那一槽今天住 `byte_table.rs`）。
///  09-10 干净 win11 上的读数（**裸 `monitor.exe` 跑着 0 个本机后端进程**）就是那道「只在 Linux 上给」的闸造成的。〕
/// 今天宿主按这台机器的 (OS, arch) 从 `byte_table` 那一张表里取（那张表里本机原生那一槽挂在这一份产物的 `TARGET` 上），
/// 取不到时交进来的是**那句拒绝的话**（`Err`），本层把它接在「旁边没有」那句后面 —— 不再自己问第二处。
///
/// # 🔴 `K-R43`：本函数**为什么是从 [`start_or_extract`] 里抽出来的**
///
/// 抽出来之前，「找那个二进制」有**两份手写实现**：本模块的 [`start_or_extract`]
/// 与 `local_backend_host.rs::resolve_backend_bin`（常驻那条路不要监护那半，用不了前者）。
/// 两份之间只有一条 `the_two_resolution_paths_still_agree_on_the_order` 盯着，而它**只对拍顺序**。
///
/// ⚠ **那条判据眼皮底下真的漂了一次，而它全程绿**〔`K-R43` 现打，读数住件文件 `§9`〕：
/// `K-R42` 只给 [`start_or_extract`] 接上了上面那两段（问产物带没带 · 释放失败说一句分得开的话），
/// `resolve_backend_bin` 一个字没动 —— 顺序仍是「先旁边、再释放」⇒ 那条判据**照样绿**。
/// 于是同一台机器上，两条路对**同一个失败**给出的是两句性质不同的话。
/// ⇒ 处置**不是**再加一条「两边内容也要一样」的对拍（那是「测自己的副本」的近亲，
/// 本模块 [`local_extract_name`] 的头注逐字论证过同一件事），是**只留一份**。
///
/// # 它不做什么
///
/// **不监护**。监护是 [`start_or_extract`] 那一半 —— 常驻那条路（`local_backend_host.rs`）
/// 起完就脱离，它要的只是这个答案。**「找」与「监护」焊在一起，正是当初逼出第二份实现的那颗钉子。**
pub fn resolve_or_extract(
    target_triple: &str,
    extract_dir: &Path,
    embedded: Result<(&str, &[u8]), String>,
    make_executable: &dyn Fn(&Path) -> Result<(), String>,
) -> Resolved {
    // 🔴 `K-R69`：整段解析包进一个**带标号的块**，只为在返回之前多做一件事
    //    （放本机那条 `ccm` 入口）。**刻意不抽成第二个函数** ——
    //    `the_self_extract_path_really_asks_the_product_whether_it_carries_one`
    //    切的就是本函数的体，抽走等于把那三条断言切到一段空文本上（它们会恒答）。
    let resolved = 'resolve: {
        let beside = resolve_beside_this_exe(target_triple);
        if matches!(beside, Resolved::Found(_)) {
            break 'resolve beside;
        }
        // 〔DP1〕宿主从那张表里取不到 ⇒ 它交进来的是那句拒绝的话：接在「旁边没有」后面，两件事都说。
        let (build_id, bytes) = match embedded {
            Ok(b) => b,
            Err(why) => {
                break 'resolve match beside {
                    Resolved::Missing { reason, looked_at } => Resolved::Missing {
                        reason: format!("{reason}\n{why}"),
                        looked_at,
                    },
                    found @ Resolved::Found(_) => found,
                };
            }
        };
        match extract_embedded_to(extract_dir, build_id, bytes, make_executable) {
            Ok(p) => Resolved::Found(p),
            Err(e) => {
                // 🔴 `K-R42` 硬要求①：**这一支不许被读成「这份产物没带后端」。**
                //    它是「带了，但这台机器不让我把它放下来」——两件事的下一步完全不同
                //    （前者去装安装包，后者去看那个目录的权限 / 杀毒软件）。
                //    09-10 一整天治的正是这一形：读面把「读不到」说成「你没有」。
                let reason = extraction_failure_reason(extract_dir, &e);
                // 光靠返回值不够响：调用方可能只把它记进 `tracing::info!`
                //（`K-R43` 之前，`local_backend_host.rs` 那条自动起的路对没有记录的失败就是这么走的
                //  —— 而它当时**根本走不到这里**，那条路自己拼了一句分不开的话）。
                // ⇒ 这一支自己吼一声 error，日志里一定留得下。**两条路今天共用这一声。**
                tracing::error!("{reason}");
                Resolved::Missing {
                    reason,
                    looked_at: match beside {
                        Resolved::Missing { looked_at, .. } => looked_at,
                        Resolved::Found(_) => Vec::new(),
                    },
                }
            }
        }
    };
    // 🔴 `K-R69`：**后端在哪儿，本机那条 `ccm` 入口就跟到哪儿。**
    //    放在这里而不是放在两个生产入口里，理由与 `K-R43` 抽出本函数时那条逐字相同：
    //    两份手写实现之间只会漂，而漂开的后果是同一台机器上两条路给出不同的答案。
    // ⚠ **它失败不许拖垮后端**：少一条终端命令 ≠ 后端起不来。诚实吼一声，照常返回。
    if let Resolved::Found(bin) = &resolved {
        // 〔WIN1 · RT1 F2〕记下「这个进程用的是哪一份」—— 读面（[`resolve_in_use`]）从此认得它。
        note_in_use(bin);
        if let Err(e) = install_local_ccm_entry(extract_dir, bin, make_executable) {
            tracing::warn!("本机 ccm 入口没放下来（后端本身没事，只是终端里少一条 `ccm`）：{e}");
        }
    }
    resolved
}

/// P2z：**生产入口的自释放版** —— [`resolve_or_extract`] 找到就起并看住它。
/// 这就是「单 exe 也能起后端进程」那句话的落点。
///
/// ⚠ 本函数 = **那一份共用的解析 + 监护**。「在哪找、找不到说什么」一个字都不住这里
/// （`K-R43` 抽走了，理由住 [`resolve_or_extract`] 的头注）。
pub fn start_or_extract(
    target_triple: &str,
    extract_dir: &Path,
    embedded: Result<(&str, &[u8]), String>,
    make_executable: &dyn Fn(&Path) -> Result<(), String>,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
    spawn: Arc<crate::spawn_managed::ManagedSpawn>,
    // 〔RL1 · V107〕交给后端的环境（中转端口 ＋ 凭据路径）由**宿主**给 —— 本层不认识中转，只原样转交。
    envs: Vec<(String, String)>,
) -> (Resolved, Option<SuperviseHandle>) {
    let resolved = resolve_or_extract(target_triple, extract_dir, embedded, make_executable);
    let Resolved::Found(bin) = resolved else {
        // 诚实降级：`reason` / `looked_at` 原样交回，这一层不再包一句自己的话。
        return (resolved, None);
    };
    // P2：本机后端**起来就带入方向通道** —— 不是可选项，也不由宿主决定。
    // 「本机 = 不走 ssh 的远端」（`INVARIANTS §40`）：远端一连上就 attach 通道，本机同理。
    let h = supervise_with_stdio(
        bin.clone(),
        LOCAL_STREAM_ARGS.iter().map(|a| a.to_string()).collect(),
        envs,
        CrashLimits::default(),
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        }),
        on_event,
        Some(Arc::new(local_stdio_consumer_guarded)),
        spawn,
    );
    (Resolved::Found(bin), Some(h))
}

#[cfg(test)]
#[path = "../../../../../tests/bridge/backend/control/local_backend_tests.rs"]
mod tests;
