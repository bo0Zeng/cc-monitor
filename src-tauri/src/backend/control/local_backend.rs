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
//! 被监护的对象是我们自己的 daemon（`--tail-only` 持续往 stdout 写帧），不会这么干；
//! 换成别的程序前要重新想。EOF 之后仍会 `wait()` 收尸（那时它已经死了，不阻塞）。
//!
//! # ⚠ 今天它在生产上**不会真起一个进程**，这是刻意的
//!
//! 摸底量到一件安全相关的事：daemon 一启动就**无条件**往它能连到的 tmux server 上装三条
//! 全局 hook（`observe/watcher.rs::install_tmux_hooks_best_effort` → `set-hook -g`），
//! **而且没有关掉它的开关**。所以「顺手在 dev 环境里扫到 `target/debug/cc-monitor-remote`
//! 就起它」会去改用户真实 tmux server 的状态。
//!
//! ⇒ [`resolve_with`] **只认打包进安装包的 sidecar**（exe 同目录、按 target triple 命名），
//! **不扫仓库里的 dev 产物**。今天安装包里还没有那个文件（`externalBin` 是 **F05b**）
//! ⇒ 生产路径恒走 [`Resolved::Missing`] 的**诚实降级**（定框 §5：tagged + `reason`，不是 `Err`），
//! 零副作用。**C7 由 F05a + F05b 两件共同满足**，ROADMAP §3 就是这么记的。
//!
//! 真进程行为由 `e2e/local-backend-supervise.sh` 验：它**显式**把二进制路径喂给
//! [`supervise`]，并强制私有 tmux 隔离，绝不碰用户真实 tmux server。
//! ⚠ 〔`P0e` 08-12〕隔离**换过机制**：原来靠私有 `TMUX_TMPDIR`，而 `$TMUX` 一有值就压过它
//! （08-11 就是这么打没用户 9 个真实会话的）⇒ `C7i` 逐字禁掉那条路。
//! 现在给 daemon 一条**前面挂着 shim 的 PATH**（`e2e/tmux-shim.sh`），它 shell out 的 tmux
//! 被强插 `-L` —— **显式选择器压得过 `$TMUX`**。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// 本机 sidecar 的基名。Tauri 的 `externalBin` 会把它按 `<基名>-<target-triple>[.exe]`
/// 放到 app 可执行文件旁边。
pub const SIDECAR_STEM: &str = "cc-monitor-remote";

/// F06b-1：**monitor 告诉 `ccm` 「daemon 二进制在哪」的那个 env 名**。
///
/// # 为什么用 env（三条路里裁的第 ③ 条）
///
/// `ccm` 是终端里的一次性 bash，它够不着 monitor 的 [`resolve_beside_this_exe`]。三条路：
/// ① ccm 自己实现一份「找 sidecar」⇒ **第二份实现**（定框 §4 逐字禁）·
/// ② 装 ccm 时写进配置 ⇒ 要新机制（ccm 有 SFTP 部署 / 手装 / 仓内相对路径三条安装路径）·
/// ③ **monitor 拼 env 时告诉它** ⇒ 零新机制（monitor 本来就在拼 env 前缀）。
///
/// ⚠ ③ 的代价如实记：**用户手敲 `cc` 时没有这个 env** ⇒ 那条路**保持今天的本地行为**
/// （诚实降级，不是报错）。
///
/// ⚠ **这个名字只有一个家** —— `shared/ccm` 读的必须是同一个字面量，
/// 由 `the_daemon_bin_env_name_has_exactly_one_home` 钉住（定框 §4）。
///
/// ⚠ **ccm 那一半已接**〔F06b-1c〕：`shared/ccm` 的 `resolve_from_daemon`（函数，exec 路用）
/// 与 `resolve_recipe`（文本，print 路用），照该文件里 `derive_bus_id`/`BUS_ID_RECIPE` 的先例写；
/// 一致性由 `e2e/ccm-contract-parity.sh` 的 **A′/A′d 组**钉住（print↔exec 的 argv 差分）。
///
/// ⚠ **monitor 这一半还没接**：本 `const` 今天**没有生产调用点**（只有判据读它，
/// 于是 `dead_code` 警告仍在 —— 那个警告就是「没接上」的诚实标记，刻意不 `#[allow]`）。
/// 挡在前面的是一条**架构题**，不是工作量：`payload.rs` 编译的 `EnvOp` 由 **TS 前端经 wire
/// 送来**（`launch_wire.rs`），而 daemon 路径是**后端的知识**（[`resolve_beside_this_exe`]）
/// ⇒ 让前端携带它正对着 C2 与 C9。裁这道题是 F06b-1d 的第一件事。⇒ resume 路不能把
/// daemon 的答案烤进打印串，得打印一段「执行时去问 daemon」的配方。
pub(crate) const DAEMON_BIN_ENV: &str = "CCM_DAEMON_BIN";

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

/// 本机 sidecar 的候选路径。**只在 exe 同目录找**，理由见模块头注（不扫仓库 dev 产物）。
///
/// 两个候选：Tauri `externalBin` 的 triple 后缀形态，以及 bundler 剥掉后缀后的裸名
/// （两种形态都出现过，取决于打包器版本；**都列出来比猜一个强**）。
pub fn sidecar_candidates(exe_dir: &Path, target_triple: &str, exe_suffix: &str) -> Vec<PathBuf> {
    vec![
        exe_dir.join(format!("{SIDECAR_STEM}-{target_triple}{exe_suffix}")),
        exe_dir.join(format!("{SIDECAR_STEM}{exe_suffix}")),
    ]
}

/// 纯函数版的解析：`exists` 由调用方注入，便于单测不碰文件系统。
pub fn resolve_with(
    exe_dir: &Path,
    target_triple: &str,
    exe_suffix: &str,
    exists: &dyn Fn(&Path) -> bool,
) -> Resolved {
    let cands = sidecar_candidates(exe_dir, target_triple, exe_suffix);
    for c in &cands {
        if exists(c) {
            return Resolved::Found(c.clone());
        }
    }
    Resolved::Missing {
        reason: format!(
            "安装包里没有本机后端 sidecar（`{SIDECAR_STEM}`）—— \
             `tauri.conf.json` 今天还没有 `externalBin`，那是 F05b。\
             本机后端因此**未启动**；远端功能不受影响"
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

/// 监护器对外说的话。**调用方决定怎么呈现** —— 本模块不 emit 任何事件（宿主无关）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuperviseEvent {
    Started { pid: u32, attempt: u32 },
    Exited { code: Option<i32>, attempt: u32 },
    GaveUp { reason: String },
}

/// 监护句柄。
pub struct SuperviseHandle {
    stopping: Arc<AtomicBool>,
    /// 当前子进程。**留在锁里**（不被等待线程独占）正是为了让 [`Self::stop`] 能 kill 它 ——
    /// 等待走的是 stdout 的 EOF，见模块头注。
    child: Arc<Mutex<Option<std::process::Child>>>,
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
    /// 杀掉是必须的：被监护的 daemon 不会因为父进程退出而自己走
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
/// **绝不让被监护的 daemon 碰用户真实的 tmux server**。
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

pub type StdioSink =
    Arc<dyn Fn(std::process::ChildStdin, std::process::ChildStdout) -> ConsumerExit + Send + Sync>;

pub fn supervise(
    bin: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    limits: CrashLimits,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
) -> SuperviseHandle {
    supervise_with_stdio(bin, args, envs, limits, now_ms, on_event, None)
}

/// 见 [`StdioSink`]。`stdio` 为 `None` 时与 [`supervise`] 逐字等价。
#[allow(clippy::too_many_arguments)]
pub fn supervise_with_stdio(
    bin: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    limits: CrashLimits,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
    stdio: Option<StdioSink>,
) -> SuperviseHandle {
    let stopping = Arc::new(AtomicBool::new(false));
    let child: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
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
            // 被监护的 daemon 会跑 `tmux ls`。tmux 客户端在 `TMUX` 有值时**按它给的 socket 走，
            // `TMUX_TMPDIR` 完全不起作用** —— 那正是「私有 socket 隔离」被绕过的机制。
            // 我在一次探针里漏了这一条，结果用户 9 个真实 tmux 会话没了。
            // ⇒ 这里无条件清掉：daemon 该按自己的 `TMUX_TMPDIR`（或默认 socket）解析，
            // 而不是继承「monitor 恰好从哪个 tmux 里被启动」这个偶然。
            cmd.env_remove("TMUX");
            cmd.args(&args)
                // P2：有消费者才接 stdin。无消费者时**逐字维持 `null`** ——
                // 「本机 daemon 收不了入方向命令」是 C4 量出来的缺口，
                // 但没人要那根管子时接出来只会多一个没人写的 fd。
                .stdin(if stdio.is_some() {
                    std::process::Stdio::piped()
                } else {
                    std::process::Stdio::null()
                })
                // ★ stdout 必须是管道：它的 EOF 就是「进程死了」这个事件的来源。
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null());
            for (k, v) in &envs {
                cmd.env(k, v);
            }
            let mut spawned = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
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
            pid.store(this_pid, Ordering::SeqCst);
            if let Ok(mut g) = child.lock() {
                *g = Some(spawned);
            }
            // ★ **F16 关窗**：`stopping` 原来只在循环顶部与 EOF 之后检查 ⇒ 存在一个窗口 ——
            // 刚过顶部检查就 `spawn`，此刻 `stop()` 执行：它置位 `stopping`，但锁里还是 `None`
            // ⇒ **一个字节的 kill 都没发**；线程接着把子进程存进锁、进 `io::copy` 永久阻塞
            // （daemon 对 stdin 关闭刻意不敏感、也不会自己退）⇒ **monitor 退了、daemon 还在跑，
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
            // ⚠ **F16 修**：原来是 `read_to_end(&mut Vec::new())` —— 那会把被监护 daemon 的
            // **全部 stdout 攒在一个永不释放的 `Vec` 里**，而它是**持续产帧**的
            // （那正是本模块头注用来论证「它不会关掉 stdout」的理由）⇒ 增长速度 =
            // 本机所有会话的 jsonl 产出速度，且没有任何消费者。
            // 今天不咬人只因为 `resolve_beside_this_exe` 恒 `Missing`（`tauri.conf.json` 里没有
            // `externalBin`）—— **离生效只差一个配置项**，而 `e2e/local-backend-supervise.sh`
            // 那条真进程路径现在就在跑它。
            // ⇒ `io::copy` 到 `io::sink()`：**EOF 语义完全不变**，但一个字节都不留。
            let exit_reason = match (&stdio, out, in_) {
                // P2：消费者**负责把 stdout 读到底** —— 它返回就等于流结束。
                // B4 之后它还要说清**为什么**返回（EOF 还是早退），见 `ConsumerExit`。
                (Some(f), Some(o), Some(i)) => f(i, o),
                // 缺省：F16 那条 —— EOF 语义不变，一个字节都不留。`copy` 返回即 EOF。
                (_, Some(mut o), _) => {
                    let _ = std::io::copy(&mut o, &mut std::io::sink());
                    ConsumerExit::Eof
                }
                (_, None, _) => ConsumerExit::Eof,
            };
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
            let code = reaped
                .as_mut()
                .and_then(|c| c.wait().ok())
                .and_then(|s| s.code());
            pid.store(0, Ordering::SeqCst);
            on_event(SuperviseEvent::Exited { code, attempt });

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

/// 在**本可执行文件同目录**找 sidecar。这是 [`resolve_with`] 的真文件系统版。
///
/// `current_exe()` 不是 GUI 把手（不违反宿主无关那条机检）——
/// 它是「我这个二进制装在哪」这一条**部署事实**，正是 sidecar 该在的位置。
/// F06b-1d（**C9** 的逐字落地）：给 backend 亲手开的那个终端窗口，配上 daemon 路径。
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
/// **sidecar 不在就不设** —— 导一个指向空处的路径不会让 ccm 更聪明（它那边 `[ -x ]` 一样过不了），
/// 只会让「这台机到底有没有本机后端」这个问题多一个假阳性来源。⇒ 空值 ≠ 未设（Z01 那条支点）。
pub(crate) fn daemon_bin_env_for_window(target_triple: &str) -> Option<(&'static str, String)> {
    env_from_resolved(resolve_beside_this_exe(target_triple))
}

/// 上面那个函数的**纯**内核 —— 抽出来是为了两个分支都测得到。
///
/// ⚠ 不抽的话判据只走得到 `Missing`（测试环境旁边没有 sidecar），
/// 于是「`Found` 时用的是 [`DAEMON_BIN_ENV`] 这个名字」这半**永远验不了** ——
/// 那正是「判据的探针改变了被观察的路」的近亲：**探针到不了的分支等于没判据**。
fn env_from_resolved(r: Resolved) -> Option<(&'static str, String)> {
    match r {
        Resolved::Found(p) => Some((DAEMON_BIN_ENV, p.to_string_lossy().into_owned())),
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
                reason: "拿不到自身可执行文件路径 ⇒ 无法定位本机后端 sidecar".into(),
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
/// **每换一次 daemon 构建就在 `~/.cc-monitor/bin/` 多留一个 10MB 级的旧文件，永不回收**。
/// 今天没有任何清理逻辑，也没有判据钉它。
/// ⇒ 刻意不做：按 mtime/版本回收要先定「谁还可能在跑旧的那份」，那是 `P2d`（认已有实例）的前提。
///
/// P2z（`control-parity` 的定框 C10 —— 单 exe 那一条，不是 `backend-split` 那条平台原语）：**单 exe 自释放** —— 把 app 里**已经内嵌**的那份 musl daemon
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
/// 起不起得来由监护层（[`supervise`]）的崩溃计数说话。
/// 本机释放的**文件名**（唯一真相源）。
///
/// ⚠ 抽成函数不是为了好看：判据 `the_local_extract_path_is_build_id_scoped` 要断言这条命名规则，
/// 而如果判据自己**抄一份** `format!` 就成了「测自己的副本」—— 改了这里判据照样绿。
/// 本仓在别处栽过同族（`strip-comments` 那次两份手抄语义漂移）。⇒ 两边共用这一个。
pub fn local_extract_name(build_id: &str) -> String {
    format!("cc-monitor-local-{build_id}")
}

/// 陈旧 `.partial` 的年龄阈值。
///
/// 释放一份 daemon 是**一次几 MB 的顺序写**（本机实测 2.4 MB），正常在毫秒级完成。
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
    // 先写临时文件再 rename：半截文件不许被当成可执行的 daemon（rename 在同一文件系统上原子）。
    // ★★ 临时名**带 pid**〔`P2t` 摸底 08-12〕：原来是**固定名**，两个同版本 monitor 同时释放
    // 会写同一个 `.partial` —— 一个写到一半、另一个 `rename` 走，出来的可能是**半截文件**，
    // 而这道 `.partial` + `rename` 存在的全部理由就是「半截文件不许被当成可执行的 daemon 起起来」。
    // ⚠ 这不是理论：`tauri_plugin_single_instance` **只在 `#[cfg(windows)]` 注册**
    // （`lib.rs:220`）⇒ Linux/macOS 上两个 monitor 天然并存。
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

/// **生产入口**：找得到就起并看住；找不到就**诚实降级**（定框 §5）。
///
/// ⚠ 今天恒走降级那一支 —— 安装包里还没有 sidecar（`externalBin` 是 F05b）。
/// 这不是「接线没做」，是**接线做了、依赖还没到位**：两者的区别就在这个返回值上，
/// 调用方能把 `reason` 与 `looked_at` 原样记进日志。
pub fn start_if_present(
    target_triple: &str,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
) -> (Resolved, Option<SuperviseHandle>) {
    let r = resolve_beside_this_exe(target_triple);
    let Resolved::Found(bin) = &r else {
        return (r, None);
    };
    // P2：本机后端**起来就带入方向通道** —— 这一步不是可选项，也不由宿主决定。
    // 「本机 = 不走 ssh 的远端」（`INVARIANTS §40`）：远端一连上就 attach 通道，本机同理。
    let h = supervise_with_stdio(
        bin.clone(),
        vec!["--tail-only".into()],
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
    );
    (r, Some(h))
}

/// [`local_stdio_consumer`] 用的**同步有界读行** —— 远端 `ssh_source::read_capped_line` 的孪生。
///
/// # 为什么不复用那一份
///
/// 那一份是 `async`（吃 `AsyncBufRead`），而本机消费者跑在**裸 `std::thread`** 上。
/// 机制可以同构，代码跨不了 sync/async 这道边。
/// ⇒ **共用的是上限常量**（`ssh_source::DAEMON_FRAME_LINE_CAP`），那才是会漂的东西；
/// 机制各写一份，两边头注互指。
///
/// # 机制：`fill_buf`/`consume`，超限之后只找换行、不再往 buf 里塞字节
///
/// 逐字抄远端那条头注记的教训：daemon 侧第一版用无界 `read_until`、读完再看长度，
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
/// 而调用方把它和 EOF 一起 `break` ⇒ daemon 被我们读死、还被记成一次「崩溃」，
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

/// P3 刀 1 的**唯一**吸收点：本机 daemon 推来的帧里，哪些要进账本。
///
/// # 为什么抽成函数〔`P3` 08-12〕
///
/// 原来这三行**长在读行循环里**，而那个循环要有一个**真的在跑的 daemon** 才进得去
/// ⇒ `P3-Y1`（acceptor: **实测**）唯一的证据只能是一条会起真 tmux 的测试，而那条
/// 08-11 出过**误伤用户 9 个真实会话**的事故后被 `#[ignore]` 了 ——
/// 于是这条 DoD **至今没有兑现证据**（如实登记在件的 `§0h`：「不是『测试暂时关着』，
/// 是这条 DoD 今天没有兑现」）。
///
/// 抽出来之后，「帧 → 账本」这一跳**不需要 daemon、不需要 tmux** 就能验 ——
/// 与 `P5L` 把终端出口做成入参是同一手：**把够得到的那半做成可测，别拿够不到的当借口**。
///
/// ⚠ **射程如实登记**：本函数可测的是「**收到帧之后**账本里有」。
/// 「daemon **真的会发**这个帧」仍归 daemon 侧 `EMITS "tmux_sessions"` 的登记
/// （逐字「登记 = 承诺真发」）与协议文档守卫 —— 那一跳本判据**够不到**，
/// 那条会起真 tmux 的实测因此**留着**（仍 `#[ignore]`），不是删掉了事。
fn absorb_local_frame(frame: &crate::ssh_source::InboundFrame) {
    // P3 刀 1：**本机的 tmux 帧也要收**。daemon 的 `watch_loop` 周期跑本机 `tmux ls`
    // 并推 `TmuxSessions` 帧。P2 写这个消费者时只需要通道，把非 hello 帧全丢了 ——
    // 于是**本机 tmux 会话对 monitor 不可见，不是拿不到，是我们扔了**。
    //
    // ⚠ 收它有前置：本地 sid 进这张表之后，`/branch` 会走 `(Some(origin), …)`
    // ⇒ 必须先有「本地也判得出 `Superseded`」（P3 刀 0）。没有刀 0 就收帧 =
    // 把「永远消不掉的灰点」那个 bug 请回来。
    if let crate::ssh_source::InboundFrame::TmuxSessions { raw, .. } = frame {
        crate::ssh_source::record_tmux_raw(crate::inbound_client::LOCAL_ORIGIN, raw.clone());
    }
}

/// # 诚实边界 10a + 10e：通道**通了**，但没人往里发命令，也没验命令真能执行
///
/// 10a：`client_for("<local>")` 今天的生产消费者**只有读**（`daemon_status` 问「通道在不在」）。
/// **一条入方向命令都还没有人从本机发出去** —— 那是 P3 刀 2/3 的活。
/// 「通道在」与「控制面走通了」是两件事，别把前者当成后者的证据。
///
/// 10e：起 daemon 时只验了它**照发 hello、且声明接受 `launch`/`kill`**。
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
/// `parse_frame(&str) -> Option<InboundFrame>` · `DaemonHello::from_hello_frame(&InboundFrame)` ·
/// `ParkedWriter::into_client(DaemonHello)`。
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
/// 逐行读、读完即弃。daemon 是持续产帧的，攒任何东西都是无界增长。
pub(crate) fn local_stdio_consumer(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
) -> ConsumerExit {
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
                let _ = std::io::copy(&mut o, &mut std::io::sink());
                return ConsumerExit::Eof;
            }
        };
    let mut parked = Some(crate::inbound_client::park_owned_writer(stdin));
    // 留一份副本给 `unregister` —— 它要 `&Arc` 比对身份（「不摘别人的 client」）。
    let mut registered: Option<std::sync::Arc<crate::inbound_client::InboundClient>> = None;
    let mut early = false;

    let mut rd = std::io::BufReader::new(stdout);
    loop {
        let line = match read_capped_line_sync(&mut rd, crate::ssh_source::DAEMON_FRAME_LINE_CAP) {
            Ok(Some(l)) => l,
            Ok(None) => break, // EOF = 流结束 = 判死
            Err(e) => {
                // 真读错误（不是坏字节 —— 那条已被 `from_utf8_lossy` 吸收）。
                // ⚠ **子进程可能还活着** ⇒ 报 `Early`，让 `supervise` 补一刀（B4）。
                tracing::warn!("本机后端 stdout 读错误（{e}）；按早退处理");
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
        // P3 刀 1：本机的 tmux 帧也要收。**理由与前置条件写在 `absorb_local_frame` 的头注上**
        // ——〔08-12〕抽函数时这段散文一度**两处各一份**，那是第二份真相源，收敛掉。
        absorb_local_frame(&frame);
        // 下面只在**还没登记**时才找 hello；登记之后不再看它。
        if parked.is_none() {
            continue;
        }
        let Some(witness) = crate::inbound_client::DaemonHello::from_hello_frame(&frame) else {
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
        crate::inbound_client::register(crate::inbound_client::LOCAL_ORIGIN, client.clone());
        registered = Some(client);
        tracing::info!(
            "本机入方向通道已登记：origin={} build_id={build_id} commands={commands:?}",
            crate::inbound_client::LOCAL_ORIGIN
        );
    }

    // 流结束 ⇒ 摘掉登记，别在表里留一个写不进去的 client。
    if let Some(mine) = registered {
        crate::inbound_client::unregister(crate::inbound_client::LOCAL_ORIGIN, &mine);
    }
    // ★ **本机那份 tmux 原文也要清**〔D 阶段补审 08-11，判据路〕。
    //
    // 远端断连早就清了（`ssh_source` 里 Batch9-F28 那处），而本机这侧流结束时
    // **只摘入方向 client、不碰这张表** ⇒ 停掉本机 daemon 之后 `<local>` 那份原文永久留着，
    // 成了「tmux 还在」的**陈旧证据**：`find_tmux_origin_for_sid` 仍返回 `Some(<local>)`
    // ⇒ `classify_removed(Some(_), Gone)` = `Idle` = 那个「永远消不掉、也 attach 不上的灰点」。
    crate::ssh_source::forget_tmux_raw(crate::inbound_client::LOCAL_ORIGIN);
    if early {
        ConsumerExit::Early
    } else {
        ConsumerExit::Eof
    }
}

/// [`local_stdio_consumer`] 的**兜底外壳**〔D 阶段补审 08-11 新增，B3〕。
///
/// # 为什么要它
///
/// 消费者跑在 `supervise` 的裸 `std::thread` 上，那里**没有 `catch_unwind`**。
/// 它体内任何一次 panic（锁中毒、切片越界、`expect`）都会 unwind 出去，于是：
/// `child` 锁里还留着活的 `Child` · `pid` 没归 0 · `unregister` 被跳过
/// ⇒ 没人读 daemon 的 stdout ⇒ 管道缓冲填满 ⇒ **daemon 阻塞在 write 上冻死**。
/// 而 `daemon_status` 照回 `channel: true` + 活 pid，`start_local_backend` 也拒绝重起。
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
) -> ConsumerExit {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        local_stdio_consumer(stdin, stdout)
    }));
    if let Ok(reason) = r {
        return reason;
    }
    tracing::error!(
        "本机 stdio 消费者 panic —— 已兜住并按流结束处理。\n\
             ⚠ 不兜的话它会 unwind 出 supervise 线程，留下一个「状态全绿的死人」：\n\
         daemon 还活着但没人读它的 stdout ⇒ 管道填满冻死，而 UI 显示一切正常。"
    );
    // panic 时**子进程多半还活着** ⇒ 报 `Early`，让 `supervise` 补一刀（B4）。
    ConsumerExit::Early
}

/// P2z（`control-parity` 的定框 C10）：**生产入口的自释放版** —— exe 旁边找不到 sidecar 时，
/// 把内嵌的那份释放到 `extract_dir` 再起。这就是「单 exe 也能起 daemon 进程」那句话的落点。
///
/// 顺序刻意是 **先找旁边、再释放**：开发构建里 `target/debug/` 旁边就有一个**更新**的二进制，
/// 那条路径优先于内嵌那份（内嵌的是打包时的快照）。
///
/// ⚠ **这个顺序也是 P2z-Y1 的验收陷阱**：dev 构建里第一步恒命中 ⇒ 不把旁边那个挪开，
/// 测到的是旧路径，而读数看起来和「释放成功」一模一样。
///
/// `embedded` 由调用方给（`sftp::daemon_binary(arch)` 的产物）—— 本模块不认识 `sftp`，
/// 也不认识「当前是什么 arch」，那都是宿主知识。
pub fn start_or_extract(
    target_triple: &str,
    extract_dir: &Path,
    embedded: Option<(&str, &[u8])>,
    make_executable: &dyn Fn(&Path) -> Result<(), String>,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
) -> (Resolved, Option<SuperviseHandle>) {
    let beside = resolve_beside_this_exe(target_triple);
    let bin = match &beside {
        Resolved::Found(p) => p.clone(),
        Resolved::Missing { .. } => {
            let Some((build_id, bytes)) = embedded else {
                // 没内嵌（`cfg(embedded_daemons)` 未置：某个 arch 的二进制缺席）⇒
                // 诚实降级，把 `beside` 的 reason/looked_at 原样交回，别伪造一个新理由。
                return (beside, None);
            };
            match extract_embedded_to(extract_dir, build_id, bytes, make_executable) {
                Ok(p) => p,
                Err(e) => {
                    return (
                        Resolved::Missing {
                            reason: format!("exe 旁无 sidecar，且释放内嵌 daemon 失败: {e}"),
                            looked_at: match beside {
                                Resolved::Missing { looked_at, .. } => looked_at,
                                Resolved::Found(_) => Vec::new(),
                            },
                        },
                        None,
                    );
                }
            }
        }
    };
    // P2：本机后端**起来就带入方向通道** —— 不是可选项，也不由宿主决定。
    // 「本机 = 不走 ssh 的远端」（`INVARIANTS §40`）：远端一连上就 attach 通道，本机同理。
    let h = supervise_with_stdio(
        bin.clone(),
        vec!["--tail-only".into()],
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
    );
    (Resolved::Found(bin), Some(h))
}

#[cfg(test)]
mod tests {
    /// 本模块的**全部启动入口**。两条判据共用这一份人群
    /// （`the_startup_path_really_calls_this_module` 与 `the_production_entry_hands_the_stdio_consumer_down`）
    /// —— 各存一份迟早分叉：新增入口时只想得起改一处。
    const ENTRIES: &[&str] = &["start_if_present", "start_or_extract"];

    /// `P2t` 摸底交付的那一刀：**两个进程不写同一个 `.partial`**。
    ///
    /// # 它防的是什么
    ///
    /// `.partial` + `rename` 存在的全部理由是「**半截文件不许被当成可执行的 daemon 起起来**」。
    /// 而临时名原来是**固定的** ⇒ 两个同版本 monitor 同时释放会写同一个文件：
    /// 一个写到一半、另一个 `rename` 走 —— 出来的正是这道防线要防的东西。
    /// ⚠ 不是理论：`tauri_plugin_single_instance` **只在 `#[cfg(windows)]` 注册**
    /// ⇒ Linux/macOS 上两个 monitor 天然并存。
    #[test]
    fn two_processes_do_not_share_one_partial_file() {
        let prod = guard_core::production_code(include_str!("local_backend.rs"));
        let at = guard_core::find_pinned(&prod, "std::process::id()")
            .expect("临时名里必须有本进程 id —— 固定名会让两个 monitor 写同一个文件");
        // 位置性质：那个 id 必须落在**构造 tmp 名**的那几行里，不是别处随便一处。
        let head = prod[..at].rfind(".partial").is_some() || prod[at..].contains(".partial");
        assert!(head, "`process::id()` 不在 `.partial` 名的构造处");
    }

    /// `P2t`：清扫只收**够老**的残骸，绝不碰新鲜的。
    ///
    /// ★ 这条是行为判据（真的建文件、真的调），不是形状判据 ——
    /// 「按年龄判」这种阈值逻辑最容易写反（`>=` 写成 `<=` 一个字符的事）。
    #[test]
    fn the_sweep_only_takes_the_old_ones() {
        let dir = std::env::temp_dir().join(format!(
            "p2t-sweep-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let name = "cc-monitor-remote-testbuild";
        // 把 mtime 拨老 48h。⚠ 这一步是**判据的一部分**：首跑时我只放了「新鲜的别人的文件」，
        // 于是把命名法过滤整个删掉**照样绿** —— 年龄检查替它挡了。
        // **两道过滤各自的作用，必须各有一个只有它能挡住的夹具。**
        let age_back = |p: &std::path::Path| {
            let f = std::fs::File::options().write(true).open(p).unwrap();
            let old = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 3600);
            f.set_times(std::fs::FileTimes::new().set_modified(old))
                .unwrap();
        };

        let fresh = dir.join(format!(".{name}.4242.partial")); // 我们的、新鲜 ⇒ 留
        let stale = dir.join(format!(".{name}.9999.partial")); // 我们的、够老 ⇒ 收
        let other_fresh = dir.join("someone-elses-file"); // 别人的、新鲜 ⇒ 留
        let other_stale = dir.join("someone-elses-old-file"); // 别人的、够老 ⇒ **仍然留**
        for p in [&fresh, &stale, &other_fresh, &other_stale] {
            std::fs::write(p, b"x").unwrap();
        }
        age_back(&stale);
        age_back(&other_stale);

        sweep_stale_partials(&dir, name);

        assert!(
            fresh.exists(),
            "刚写的 `.partial` 被删了 —— 那正是这道防线要防的事：\
             把**正在写的**那份删掉，等于自己制造半截文件"
        );
        assert!(
            !stale.exists(),
            "够老的残骸没被收 —— 那清扫就是个摆设（带 pid 之后它们不会再被覆盖掉）"
        );
        assert!(
            other_fresh.exists(),
            "碰了不属于自己命名法的文件（这个目录与远端自部署共用）"
        );
        assert!(
            other_stale.exists(),
            "把**别人的**老文件也收了 —— 这个目录与远端自部署共用，\
             只有「够老」不构成删的理由，还得是**我们自己命名法**的"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `P3-Y1` 的**兑现证据**〔08-12 补〕：帧 → 账本这一跳，**不起 daemon、不起 tmux**。
    ///
    /// # 它补的是什么洞
    ///
    /// 下面那条 `the_local_tmux_frames_really_land_in_the_ledger` 是 `P3-Y1` 原本唯一的
    /// 证据，而它 `#[ignore]`（08-11 误伤用户 9 个真实会话之后的处置），且件的 `§0h` 逐字
    /// 承认「**就算解开它也验不出来**」——它的客户端与 daemon 用的**不是同一个 socket**。
    /// ⇒ 这条 DoD 一直挂着「没有兑现证据」。
    ///
    /// 本条走**另一条路**：既然「帧 → 账本」是 `absorb_local_frame` 一个函数，就直接喂它
    /// 一条**真实形状**的帧，再从**账本那一侧**（`snapshot_tmux_by_origin`，也就是 emitter
    /// 判 idle/archived 时读的那一份）读回来。
    ///
    /// ⚠ **射程**：本条钉的是「收到帧之后账本里有」。**daemon 真的会发**那个帧
    /// 归 daemon 侧 `EMITS "tmux_sessions"`（逐字「登记 = 承诺真发」）；那一跳这里够不到，
    /// 所以那条真 tmux 的实测**留着**，不是被本条替掉了。
    #[test]
    fn a_local_tmux_frame_lands_in_the_ledger_without_any_daemon() {
        // 用一个本条专属的 raw，避免与别的测试抢同一个 origin 的那一格。
        let raw = "p3y1-proof-cc: 1 windows (created Tue Aug 12 20:00:00 2026)";
        let line = format!(r#"{{"kind":"tmux_sessions","raw":{raw:?}}}"#);
        let frame = crate::ssh_source::parse_frame(&line).expect("这是真实帧形状，必须解析得出");
        assert!(
            matches!(frame, crate::ssh_source::InboundFrame::TmuxSessions { .. }),
            "解析出来的不是 TmuxSessions —— 后面的断言就没有意义了"
        );

        absorb_local_frame(&frame);

        let snap = crate::ssh_source::snapshot_tmux_by_origin();
        let got = snap
            .get(crate::inbound_client::LOCAL_ORIGIN)
            .map(String::as_str);
        assert_eq!(
            got,
            Some(raw),
            "帧收到了，但**账本里没有** —— DoD 自陈的失效方式逐字：\
             「『消费者收到了』不等于『账本里有』」。\
             账本这一份正是 emitter 判 idle/archived 时读的那一份。"
        );
    }

    /// `P3-Y1` 的**第二半**：读行循环**真的调**那个吸收点。
    ///
    /// ★ 上面那条只证明「函数管用」。把循环里那一行删掉，它**照样绿**，
    /// 而那时本机 tmux 会话又对 monitor 不可见了 —— 与 `P2` 当初「把非 hello 帧全丢了」
    /// 是同一个形状的退化。⇒ 位置性质要单独钉（`find_pinned`：恰好一处、有边界）。
    #[test]
    fn the_read_loop_really_calls_the_absorb_point() {
        let prod = guard_core::production_code(include_str!("local_backend.rs"));
        let at = guard_core::find_pinned(&prod, "absorb_local_frame(&frame);")
            .expect("读行循环里必须恰好有一处 `absorb_local_frame(&frame);`");
        let before = &prod[..at];
        assert!(
            before.contains("parse_frame(&line)"),
            "吸收点必须排在**解析出帧之后** —— 顺序反了就是拿没解析的东西去收"
        );
    }

    /// P3-Y1（acceptor: **实测**）：**本机 daemon 的 tmux 帧真的进了账本**。
    ///
    /// # 为什么必须从账本那一侧读
    ///
    /// DoD 自陈的失效方式逐字：「**「消费者收到了」不等于「账本里有」**」。
    /// 消费者里加一行 `tracing::info!` 也能让人以为通了。
    /// ⇒ 本条读的是 `snapshot_tmux_by_origin()`，即 emitter 判 idle/archived 时读的那一份。
    ///
    /// # 隔离：私有 `TMUX_TMPDIR` + 跑前跑后比对（`C7e`）
    ///
    /// daemon 跑 `tmux ls`。给它一个**私有的 `TMUX_TMPDIR`** ⇒ 它只看得见本条自己建的那台
    /// tmux server，碰不到用户真实的那台。
    /// ⚠ **比对本身就是判据的一部分，不是附带步骤** —— 隔离若没做对（比如忘了私有目录），
    /// 测试会连进用户的 server 而**照样通过**：通过与否与隔离无关。
    /// ⚠⚠ **默认不跑（`#[ignore]`）—— 08-11 事故之后的处置。**
    ///
    /// 我在写这条时，配套的 shell 探针漏了 `unset TMUX`，`TMUX_TMPDIR` 被压过，
    /// 命令打到了用户真实的 tmux server 上，**9 个真实会话没了**。
    ///
    /// 代码这一侧的坑已经堵掉（`-S` 显式 socket · 不用 `kill-server` · `supervise` 一律清 `TMUX`），
    /// 但「在一台跑着真实会话的机器上，让自动化去起 / 杀 tmux」这件事本身值得先停下来。
    /// ⇒ 本条改成显式触发：`cargo test -- --ignored the_local_tmux_frames_really_land_in_the_ledger`。
    ///
    /// **这是降级不是放弃**：P3-Y1 因此今天**没有实测证据**，如实登记在件的 §0h / 12e-1，
    /// 不拿「判据绿」冒充「验过了」。
    ///
    /// # ★ `K-R7`（08-31）：隔离原语从**自己手搓一份**换成**共享那一份**
    ///
    /// 本条原来在测试体里现场造 shim（`exec 真tmux -S <私有 sock>`）——形态是对的，
    /// 但那是 `C7i` 那条红线原语在 Rust 侧的**第二份实现**。
    /// `e2e/tmux-shim.sh` 的头注逐字写过为什么它要被抽成共享文件：
    /// 「红线的落地**不该有三份实现**：改一处漏两处」。
    /// ⇒ 改成问 `CCM_E2E_TMUX_SHIM_BIN` 要那一份（`$BIN/tmux` 强插 `-L`），
    /// **同时**让本条的人群判据与那两条同族测试**变成同一条**（见
    /// `every_test_that_starts_the_real_daemon_demands_a_private_tmux`）。
    ///
    /// ⚠ 跑前/跑后那两次 `tmux ls`（`user_tmux()`）**刻意绕开 shim、按绝对路径问** ——
    /// 它们要问的正是**用户那台真 server**「你变了没有」。走了 shim 就问到自己那台上去了，
    /// 那条比对会变成一句恒真的空话。**而它此前正是那样**（见函数体里那段实打记录）。
    #[cfg(all(embedded_daemons, target_os = "linux", target_arch = "x86_64"))]
    #[test]
    #[ignore = "会起真 tmux；08-11 出过误伤用户会话的事故，改成显式触发"]
    fn the_local_tmux_frames_really_land_in_the_ledger() {
        use std::time::Duration;

        // ★★ **fail closed，排在一切之前**（`K-R7`）。
        let shim = crate::local_daemon::demand_tmux_shim("本条起真 daemon 且起真 tmux");
        let _guard = crate::inbound_client::local_origin_test_lock();

        // ★★★ 「用户真实的 tmux」这一问**必须绕开 PATH 上的 shim**〔`K-R7` 08-31，实测逼出来的〕。
        //
        // 本条由 `e2e/local-backend-supervise.sh` 驱动，而那个脚本把 shim 目录挂在
        // **本测试进程自己的 `PATH`** 最前面（`tmux-shim.sh` 里那句 `export PATH=…`）
        // ⇒ 裸 `Command::new("tmux")` 解析到的是 **shim**，问到的是 e2e 自己那台 server，
        // **根本不是用户那台**。
        //
        // 〔实打 08-31：本条原来把自己的会话建在**另一个** socket 上，于是 before/after
        //   两侧都在问 shim 那台空 server、两侧恒为空串 ⇒ **这条比对是空真**：
        //   隔离坏掉时它照样绿，而它的诊断文案写着「用户真实的 tmux 变了」。
        //   把会话搬到 shim 那台之后它**当场红** —— 而那次红正好证明了它此前问错了对象。〕
        // ⇒ 从 `PATH` 里把 shim 那一段剔掉再找 `tmux`，按**绝对路径**问。
        let real_tmux: Option<std::path::PathBuf> = {
            let shim_dir = std::path::Path::new(&shim);
            std::env::var("PATH")
                .unwrap_or_default()
                .split(':')
                .filter(|p| !p.is_empty() && std::path::Path::new(p) != shim_dir)
                .map(|p| std::path::Path::new(p).join("tmux"))
                .find(|c| c.is_file())
        };
        // 反空真：找不到 shim 之外的真 tmux ⇒ 下面那条比对又会退化成「空 == 空」。
        let real_tmux = real_tmux.expect(
            "`PATH` 上除了 shim 之外找不到第二个 `tmux` —— \
             那么下面「用户真实的 tmux 变了没有」那条比对会退化成空真（空 == 空），\
             隔离坏掉时它照样绿。",
        );
        let user_tmux = || -> String {
            std::process::Command::new(&real_tmux)
                .arg("ls")
                .env_remove("TMUX")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default()
        };
        let user_tmux_before = user_tmux();

        let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("embedded-daemons")
            .join("cc-monitor-remote-x86_64");
        let base = std::env::temp_dir().join(format!("p3-tmux-{}", std::process::id()));
        let home = base.join("home");
        let cfg_dir = home.join(".claude");
        std::fs::create_dir_all(cfg_dir.join("projects")).expect("建沙箱 HOME");

        let sess = format!("ccm-p3-{}", std::process::id());
        // ★★ **socket 选择器写死在 shim 里**〔事故订正 08-11 · `K-R7` 08-31 换成共享原语〕。
        //
        // 原来只给 `TMUX_TMPDIR` + `env_remove("TMUX")`。那样**只要漏掉后者**，
        // `TMUX` 就会压过 `TMUX_TMPDIR`，命令直接打到用户真实的 server 上 ——
        // 我在一次 shell 探针里正是漏了它，用户 9 个会话没了。
        // ⇒ 客户端这一侧**也按绝对路径调 shim**（不是裸 `tmux`）：
        //   shim 与 daemon 走的是**同一个** `-L`，而「同一台 server」正是本条的全部要害。
        let shim_tmux = std::path::Path::new(&shim).join("tmux");
        assert!(
            shim_tmux.exists(),
            "`CCM_E2E_TMUX_SHIM_BIN` 指的目录里没有 `tmux` —— \
             那不是 `e2e/tmux-shim.sh` 造出来的那份，隔离无从谈起：{shim_tmux:?}"
        );
        let tmux = |args: &[&str]| {
            let mut c = std::process::Command::new(&shim_tmux);
            c.args(args).env_remove("TMUX").output()
        };

        let made = tmux(&["new-session", "-d", "-s", &sess]).expect("起私有 tmux 失败");
        assert!(
            made.status.success(),
            "私有 tmux 起不来：{}",
            String::from_utf8_lossy(&made.stderr)
        );

        let cleanup = |h: Option<&SuperviseHandle>| {
            if let Some(h) = h {
                h.stop();
            }
            // ⚠ **不用 `kill-server`** —— 那是个打整台 server 的大锤；
            // 一旦 socket 解析出偏差，它毁掉的是用户的全部会话（08-11 就是这么出的事）。
            // `kill-session -t <本条自己建的名字>` 最坏情况也只影响一个同名会话。
            let _ = tmux(&["kill-session", "-t", &sess]);
            let _ = std::fs::remove_dir_all(&base);
        };

        let h = supervise_with_stdio(
            bin,
            vec!["--tail-only".into()],
            vec![
                ("HOME".into(), home.display().to_string()),
                ("CLAUDE_CONFIG_DIR".into(), cfg_dir.display().to_string()),
                // ★★ 〔`P0e` 08-13〕**这条以前验不到帧，病根就写在原注释里**：
                // 「daemon 内部用默认 socket 名……上面客户端用另一个 socket。
                //  **两者不是同一个 socket**」⇒ `seen` 恒 false（`P3 §0h` 如实登记过）。
                //
                // 修法与 e2e 那边同一手：给 daemon 一条**前面挂着 shim 的 PATH**，
                // shim `exec` 真 tmux 并强插选择器 ⇒ 两边落在同一台 server 上。
                // ⚠ 〔`K-R7` 08-31〕客户端那一侧现在**按绝对路径调同一个 shim**，
                //   所以「同一台 server」不再靠两处各自写对一个 socket 名去对齐 ——
                //   它由**同一个 shim 文件**保证。选择器是 `-L` 还是 `-S` 归 `tmux-shim.sh` 管，
                //   本条一个字都不用知道（那正是把原语收成一处买到的东西）。
                (
                    "PATH".into(),
                    format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
                ),
            ],
            CrashLimits::default(),
            Arc::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            }),
            Arc::new(|e| println!("[P3 实测] {e:?}")),
            Some(Arc::new(local_stdio_consumer_guarded)),
        );

        let mut seen = false;
        for _ in 0..150 {
            let snap = crate::ssh_source::snapshot_tmux_by_origin();
            if snap
                .get(crate::inbound_client::LOCAL_ORIGIN)
                .is_some_and(|raw| raw.contains(sess.as_str()))
            {
                seen = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let user_tmux_after = user_tmux();
        cleanup(Some(&h));
        // 摘掉本条写进去的那一份，别留给同批别的用例。
        crate::ssh_source::record_tmux_raw(crate::inbound_client::LOCAL_ORIGIN, String::new());

        assert_eq!(
            user_tmux_before, user_tmux_after,
            "★ `C7e`：用户真实的 tmux 变了。\n\
             隔离没做对（shim 没挂上 / 客户端绕过了 shim）⇒ 本条刚才操作的是用户的 server。\n\
             ⚠ 这条比对**不是附带步骤**：隔离坏掉时下面那条断言**照样会过**。"
        );
        println!("E2E-OK P3 跑前跑后用户真实 tmux 一个字没变（隔离是断言，不是假设）");
        assert!(
            seen,
            "15s 内账本里没出现 `{sess}` —— 本机 tmux 帧没进 `snapshot_tmux_by_origin()`。\n\
             ★ 「消费者收到了」不算：消费者里加一行日志也能让人以为通了。\n\
             本条读的是 emitter 真正会读的那一份。"
        );
        println!("E2E-OK P3 本机 daemon 的 tmux 帧真的进了账本（`{sess}` 出现在快照里）");
    }

    /// ★★ **本机读帧不许被一个坏字节杀死，也不许无界**〔D 阶段补审 08-11 新增〕。
    ///
    /// 补审在同一个读循环上逮到两条：
    ///
    /// | # | 原版 | 后果 |
    /// |---|---|---|
    /// | B1 | `BufReader::lines()`（**UTF-8 严格**）+ `let Ok(line) = line else { break }` | 一个坏字节 ⇒ `InvalidData` 与 EOF 同路 ⇒ 消费者返回 = 判死 ⇒ daemon 因 EPIPE 自杀 ⇒ 记一次「崩溃」，三次后**整个进程周期不再起来**，日志写「崩了 3 次」——**一个错误的诊断** |
    /// | B2 | `read_line` 语义 ⇒ **完全无界** | 远端有 `read_capped_line`（64 MiB 上限，头注记着 daemon 侧「512 MiB 无换行流 ⇒ RSS 6→518 MiB」的实测）。同一个对端、同一种失效模式，只有本机这侧没上限 |
    ///
    /// 本条钉三件：① 不许再出现 `.lines()` 那条严格路 ② 必须走 `read_capped_line_sync`
    /// ③ **上限必须取远端那个常量**（两边各写一份机制，但值不许漂）。
    #[test]
    fn the_local_frame_reader_is_bounded_and_lossy() {
        let src = include_str!("local_backend.rs");
        let prod = guard_core::production_code(src);
        let at = guard_core::find_pinned(&prod, "fn local_stdio_consumer(")
            .expect("消费者不在了 —— 改了名就来改本条");
        let body: String = prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.len() > 400,
            "切出来的体只有 {} 字节 —— 切错了",
            body.len()
        );

        assert!(
            !body.contains(concat!(".li", "nes()")),
            "本机读帧又用回了按行迭代器 —— 那是 **UTF-8 严格**的，\n\
             一个坏字节会被当成 EOF ⇒ 把 daemon 读死，还记成一次「崩溃」，三次后永久放弃。\n\
             远端那条路早就取了相反的取舍（`from_utf8_lossy`，注释逐字「非 UTF-8 不该让整条连接死掉」）。"
        );
        guard_core::find_pinned(&body, "read_capped_line_sync(").unwrap_or_else(|e| {
            panic!("本机读帧没走有界读行（{e}）—— 无界读遇一条永不结束的行就是无界堆分配")
        });
        guard_core::find_pinned(&body, "DAEMON_FRAME_LINE_CAP").unwrap_or_else(|e| {
            panic!(
                "本机读帧的上限不是远端那个常量（{e}）。\n\
                 两边机制各写一份（sync/async 跨不过去），但**值不许漂** —— 漂了就没人知道哪边先炸。"
            )
        });
    }

    /// P2-Y1b（acceptor: 机检）：**生产入口真的把消费者传下去了**。
    ///
    /// 上面那条实测直接调 `supervise_with_stdio` 并自己传 `Some(local_stdio_consumer)`。
    /// 而生产走的是 `start_or_extract`。两者之间那根线**没有任何东西守着** ——
    /// 有人把它改回 `supervise(…)`（少一个参数、默认 `None`），实测照样全绿，
    /// 线上却一条入方向通道都不会登记。本条钉的就是那根线。
    ///
    /// ⚠ 它是**文本判据**，只证明「那行代码长这样」，不证明运行时真跑到。
    /// 运行时那半由上面的实测证；两条合起来才闭合，单独任何一条都不够。
    #[test]
    fn the_production_entry_hands_the_stdio_consumer_down() {
        let src = include_str!("local_backend.rs");
        // 人群 = 本模块的**全部启动入口**，与 `the_startup_path_really_calls_this_module`
        // 那条用的是同一个清单。只钉「今天 lib.rs 在调的那一个」= 给下一个用另一个入口的人留坑。
        for name in ENTRIES {
            // 按**行**取函数体：从 `pub fn <name>(` 那行起，到第一行**恰好是 `}`** 为止。
            // 不用 `src.find("\n}\n")` —— 那是语料上的裸 `.find`，`needle_anchor_registry`
            // 的递减棘轮不许再长；而且逐行判「整行等于 `}`」本来就比子串匹配更贴事实。
            let head = format!("pub fn {name}(");
            let mut lines = src.lines().skip_while(|l| !l.starts_with(&head)).peekable();
            assert!(lines.peek().is_some(), "人群塌了：入口 `{name}` 不在了");
            // ⚠ 变量名**刻意不叫 `body`**：`needle_anchor_registry` 按**名字**认语料变量，
            // 而本文件另一条判据早就有 `body.contains(".stop()")`。叫 `body` 会让那处
            // 被追认成「语料上的裸 contains」，把递减棘轮顶红 —— 明明我一行匹配都没加。
            let entry_src: String = lines
                // ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段
                // （`ssh_source::strip_cfg_test`），源码里多一个孤立的右花括号会让它**提前闭合**（`b'…'` 的字符字面量也算，我第一次「修」时就还带着一个），
                // 测试段整段泄漏进「生产段」⇒ 别的判据当场误报（08-11 实测：单写者守卫红了）。
                .take_while(|l| *l != "\u{7d}")
                .collect::<Vec<_>>()
                .join("\n");
            let entry_src = entry_src.as_str();
            // 用 `find_pinned`（恰好一处 + 两侧有边界）而不是裸 `contains`：
            // 后者在 `needle_anchor_registry` 的递减棘轮里，而且它的病正是「needle 被撑大时照样绿」。
            guard_core::find_pinned(entry_src, "supervise_with_stdio(").unwrap_or_else(|e| {
                panic!(
                    "`{name}` 不再恰好调一次 `supervise_with_stdio`（{e}）——\n\
                     十有八九被改回了 `supervise(`，那一支的 `stdio` 恒为 `None`\n\
                     ⇒ 本机 stdin 又变回 `Stdio::null()`（正是 C4 量出的那个缺口）。"
                )
            });
            guard_core::find_pinned(entry_src, "Some(Arc::new(local_stdio_consumer_guarded))").unwrap_or_else(
                |e| {
                    panic!(
                        "`{name}` 没把**兜底版**消费者 `local_stdio_consumer_guarded` 传下去（{e}）。\n\
                         ⚠ 传裸的 `local_stdio_consumer` 也不行：它体内一次 panic 会 unwind 出 supervise 线程，\n\
                         留下「daemon 活着但没人读它 stdout」的**全绿死锁态**（补审 B3）。\n\
                         管子接出来了却没人读 ⇒ hello 帧没人解 ⇒ `client_for(<local>)` 恒 None，\n\
                         且不会报任何错。"
                    )
                },
            );
        }
    }

    /// P2-Y1 + P2-Y3（acceptor: **实测**）：起**真的** daemon 二进制，
    /// 看入方向通道是不是真的登记上了；再关写端，看 daemon 是不是**还活着**。
    ///
    /// # 为什么不起 GUI
    ///
    /// 要验的性质是「本机后端起来 ⇒ `client_for(LOCAL_ORIGIN)` 拿得到通道」。
    /// 这条链的全部零件都在本模块 + `inbound_client` 里，GUI 一个字都不参与。
    /// 起整个 app 只会把「哪一步坏了」这个信息埋掉。
    ///
    /// # 沙箱 HOME 只给子进程
    ///
    /// daemon 会 tail `$HOME/.claude/projects`。用 `envs` 参数给**子进程**单独设 `HOME`
    /// ⇒ 测试进程自己的 env 一个字不动（`std::env::set_var` 是进程全局的，
    /// cargo 又是多线程跑测试 ⇒ 那样会污染同批别的用例）。
    ///
    /// # ⚠ 这条测试在缺内嵌二进制时**不存在**（诚实边界 10c）
    ///
    /// `cfg(embedded_daemons)` 由 `build.rs` 在 `embedded-daemons/` 齐全时才置，而那个目录是
    /// gitignore 的 ⇒ 干净 clone 上本条**不编译进去**，`cargo test` 照样全绿。
    /// 不做成「缺了就 red」是因为那会让干净 clone 无法跑测试；缺失不是静默的 ——
    /// `build.rs` 那处 `cargo:warning=缺少内嵌 daemon` 会喊（U-1 那次事故之后加的）。
    ///
    /// # ★★ `K-R7`（08-31）：**这一条此前没有人点过名，而它与那条正题完全同形**
    ///
    /// `K-R7` 的件文件 `§0` / `§2` 与风险 `6p` 讲的都只是
    /// `local_daemon::tests::the_local_daemon_can_be_stopped_and_started_again`。
    /// 而 `D3` 的全表现打之后，**本条是同一族的第二条**：普通 `#[test]`、同一个 `cfg`、
    /// 起同一个真 daemon 二进制（还起了两次：探针一次 + `supervise_with_stdio` 一次），
    /// 而 daemon 一上来就**无条件**往它连得到的 tmux server 装三条**全局** hook（槽位 `[50]`）。
    ///
    /// ⚠⚠ **本条原来那句 `.env_remove("TMUX")` 读起来像隔离，其实不是**：
    /// `TMUX` 一空，tmux 客户端就**回落到默认 socket** `/tmp/tmux-$UID/default` ——
    /// 那正是用户那台 server。清一个变量买不到隔离，**只有显式选择器**（shim 强插 `-L`/`-S`）能。
    /// 那句注释说的是另一件对的事（不继承「测试进程恰好在哪个 tmux 里」），别把它读成隔离。
    ///
    /// ⇒ 与那条正题同样三道锁：`#[ignore]` + `CCM_E2E_TMUX_SHIM_BIN` fail-closed（排在
    /// **任何 spawn 之前**）+ 那个 shim **真的挂进 daemon 的 `PATH` 最前面**（探针与被监护进程都要）。
    ///
    /// ⚠⚠ **第三道锁的射程要分两格看**〔`D1` 审计 08-31 查实，阻塞 4 的另一半〕——
    /// 与 `local_daemon.rs::the_local_daemon_can_be_stopped_and_started_again` 头注里那张表**同一份**：
    /// 走 `bash e2e/local-backend-supervise.sh` 时 `tmux-shim.sh` 已经把 shim 挂进
    /// **测试进程自己的 `PATH`**，而 `supervise_with_stdio` **从不 `env_clear()`**
    /// ⇒ 第三道锁在**那条跑法上是冗余的**；它真正买的是「**手工 `cargo test -- --ignored`、
    /// 变量设上但 shim 不在自己 `PATH` 上**」那一格。别把两格混着读。
    /// 看着它的判据是 `local_daemon.rs` 那条
    /// `every_test_that_starts_the_real_daemon_demands_a_private_tmux` 的 ㈡ 与 ㈢ **两格**
    /// ——㈡ 判**写法**（`"PATH"` 与 `"{<绑定名>}:` 同行，插值紧跟开引号），
    /// ㈢ 判**处数**（`PATH` 这个 env 键在本体里恰好写一次）。
    /// ⚠ ㈢ 是 `D2` 复审 09-01 逼出来的〔阻塞 1 形 ②〕：本条的 `envs` 里**再追加一条**
    /// `("PATH", …)` 就能把上面那条带 shim 的整个盖掉（`supervise_with_stdio` 是
    /// `for (k, v) in &envs { cmd.env(k, v); }`，**后写的赢**，见 `:336`-`:337`），
    /// 而在 ㈢ 落地之前那一刀**全量门禁新红 0**。两格的代价不是一种，判据也刻意分开（`K13`）。
    ///
    /// # 🔴 复跑纪律：**换了 `embedded-daemons/` 的有无之后，必须 `touch src-tauri/build.rs`**
    ///
    /// 〔`D2 §G-1` 的陈账，09-01 收 —— 此前只写在件文件里，**被守对象这一侧一个字都没有**。〕
    /// 本条由 `#[cfg(embedded_daemons)]` 门着。**实测的现象**（`D2` 复审 09-01，我没重打，
    /// 住址 `audits/K-R7-D2.md#§G-1`）：同一个 `CARGO_TARGET_DIR` 里把
    /// `src-tauri/embedded-daemons/` 从「无」加成「有」，**`build.rs` 不重跑** ⇒
    /// 读出的是「无 emb」那一档的数（`1208/0/10`），`touch build.rs` 之后才是 `1211/0/11`。
    /// ⚠⚠ **两档的输出面长得一模一样，这个坑看不出来。**
    /// ⇒ 换 emb 状态之后 `touch src-tauri/build.rs`，或**直接换一个全新的 target 目录名**。
    /// ⚠ 机制**我没有实验证明**；`local_daemon.rs` 那条姊妹测试的同名小节里记着两条候选，
    /// 其中「`rerun-if-changed` 只在文件存在时登记」那条**现打对不上源码**（那一行是无条件的）。
    #[cfg(all(embedded_daemons, target_os = "linux", target_arch = "x86_64"))]
    #[test]
    #[ignore = "K-R7：起真 daemon ⇒ 会装全局 tmux hook。走 e2e/local-backend-supervise.sh 那条带 shim 的路"]
    fn the_local_daemon_really_registers_an_inbound_client() {
        // ★★ **fail closed，而且排在一切之前** —— 见上面头注第三段。
        let shim = crate::local_daemon::demand_tmux_shim(
            "本条起真 daemon，而 daemon 一上来就往它连得到的 tmux server 装全局 hook",
        );
        let _guard = crate::inbound_client::local_origin_test_lock();
        let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("embedded-daemons")
            .join("cc-monitor-remote-x86_64");
        assert!(
            bin.exists(),
            "`cfg(embedded_daemons)` 置了但 {bin:?} 不在 —— build.rs 与磁盘不一致"
        );

        let home = std::env::temp_dir().join(format!("p2-local-inbound-{}", std::process::id()));
        let cfg_dir = home.join(".claude");
        std::fs::create_dir_all(cfg_dir.join("projects")).expect("建沙箱 HOME");
        // ⚠⚠ **只设 `HOME` 不够，而且这条是实测逼出来的**（P2s 摸底 08-11）：
        // daemon 的 `resolve_claude_dir()` 逐字「`$CLAUDE_CONFIG_DIR` if set, else `$HOME/.claude`」
        // ⇒ 继承来的 `CLAUDE_CONFIG_DIR` **压过** `HOME`。本条第一版只设 HOME，
        // 那一跑 daemon 读的其实是**真实**的配置目录（只读 tail，没有写，但隔离是假的）。
        let envs = vec![
            ("HOME".to_string(), home.display().to_string()),
            (
                "CLAUDE_CONFIG_DIR".to_string(),
                cfg_dir.display().to_string(),
            ),
            // ★ `K-R7`：隔离**真的用上**。探针那一跳走 `.envs(envs…)`，被监护那一跳走
            //   `supervise_with_stdio(.., envs, ..)` ⇒ 写在这里两跳都盖得到。
            (
                "PATH".to_string(),
                format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
            ),
        ];

        // 隔离**必须是断言，不能是假设**：起一趟一次性的，从 hello 帧里把 daemon 自陈的
        // `claude_dir` 读回来对一遍。上面那次教训就是「我以为设了 HOME 就隔离了」。
        {
            use std::io::BufRead;
            let mut probe = std::process::Command::new(&bin)
                .arg("--tail-only")
                .envs(envs.iter().map(|(k, v)| (k.clone(), v.clone())))
                .env_remove("TMUX") // 同上：不许继承「测试进程恰好在哪个 tmux 里」
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("起探针失败");
            let mut line = String::new();
            std::io::BufReader::new(probe.stdout.take().expect("有 stdout"))
                .read_line(&mut line)
                .expect("读 hello 失败");
            let _ = probe.kill();
            let _ = probe.wait();
            let frame = crate::ssh_source::parse_frame(&line).expect("首帧该是 hello");
            let crate::ssh_source::InboundFrame::Hello { claude_dir, .. } = &frame else {
                panic!("首帧不是 hello：{line}");
            };
            assert_eq!(
                Path::new(claude_dir),
                cfg_dir,
                "daemon 自陈的 claude_dir 不在沙箱里 —— 这一跑读的是**真实**配置目录。\n\
                 `resolve_claude_dir()` 是 `$CLAUDE_CONFIG_DIR` 优先、`$HOME/.claude` 兜底，\n\
                 两个都要设。（只设 HOME 那版跑起来一切正常，隔离却是假的。）"
            );
            // ★ `K-R7`：本条改成 `#[ignore]` 之后由 `e2e/local-backend-supervise.sh` 驱动，
            //   而那个脚本的收尾自检是「标记数 < 跑成的测试数 ⇒ 有测试提前退出」。
            println!("E2E-OK P2 daemon 自陈的 claude_dir 就在沙箱里（隔离是断言，不是假设）");
        }

        let h = supervise_with_stdio(
            bin,
            vec!["--tail-only".into()],
            envs,
            CrashLimits::default(),
            Arc::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            }),
            Arc::new(|e| println!("[P2 实测] {e:?}")),
            Some(Arc::new(local_stdio_consumer_guarded)),
        );

        // 轮询而不是睡死：进程起来 + 发 hello 的耗时不确定，睡固定值要么慢要么飘。
        let mut client = None;
        for _ in 0..100 {
            if let Some(c) = crate::inbound_client::client_for(crate::inbound_client::LOCAL_ORIGIN)
            {
                client = Some(c);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let cleanup = |h: &SuperviseHandle| {
            h.stop();
            let _ = std::fs::remove_dir_all(&home);
        };
        let Some(client) = client else {
            cleanup(&h);
            panic!(
                "5s 内 `client_for(<local>)` 仍是 None —— 本机入方向通道没登记上。\n\
                 ⚠ 「没报错」不算数：远端那条路的头注记着一次审计变异 ——\n\
                 把注册整段删掉（写半边永不解冻）`cargo test` 照样全绿。所以这里必须**看到通道**。"
            );
        };

        // P2-Y1：登记了还得是**能用的** —— daemon 声明的入方向命令要在。
        assert!(
            client.accepts("launch") && client.accepts("kill"),
            "通道登记了，但 daemon 没声明接受 launch/kill ⇒ P3 接过来也发不出去"
        );
        println!("E2E-OK P2 本机入方向通道登记上了，而且声明接受 launch/kill");

        // P2-Y3：关写端**不许**把 daemon 带走（C8 裁定「默认不 kill、daemon 继续跑」）。
        // 单独观测，不顺带看一眼 —— 件里 §0d 把这条从「推断」改成了实测，就是这个意思。
        let pid = h.current_pid().expect("已经收到 hello 了，进程必然在");
        client.close_write();
        std::thread::sleep(std::time::Duration::from_millis(500));
        let alive = Path::new(&format!("/proc/{pid}")).exists();
        crate::inbound_client::unregister(crate::inbound_client::LOCAL_ORIGIN, &client);
        cleanup(&h);
        assert!(
            alive,
            "关掉 stdin 写端之后 daemon（pid={pid}）没了。\n\
             这与 C8「默认不 kill」直接冲突，也推翻了 local_backend.rs:221 那句\n\
             「daemon 对 stdin 关闭刻意不敏感」——那句注释得改，不是这条测试得改。"
        );
        println!("E2E-OK P2 关掉 stdin 写端之后 daemon（pid={pid}）还活着（C8「默认不 kill」）");
    }

    use super::*;

    fn never(_: &Path) -> bool {
        false
    }

    /// P2z-Y2（自批 D1）：**本机释放点与远端部署点结构上不许撞**。
    ///
    /// 钉的是**构造方式**不是两个字面量不相等 —— 后者一改配置就绕过去了
    /// （远端落点由 `cfg.daemon_path` 给，是**运行期**的值，编译期比不了）。
    /// 所以断言：那条路径必须由 `build_id` 拼出来。
    ///
    /// 病史：实测 08-11 本机 `~/.cc-monitor/bin/.build_id` = `p1r-event-liveness`
    /// （别的 monitor 把这台当远端连时装的），而本机源码是 `p1x-overflow-identity`
    /// ⇒ 同名会让两个 monitor 互判 stale、互相覆盖 ⇒ **无限重装循环**。
    #[test]
    fn the_local_extract_path_is_build_id_scoped() {
        // ★ 用**生产函数**，不是判据自己抄一份 `format!`（那就成了「测自己的副本」）。
        let a = local_extract_name("p1x-overflow-identity");
        let b = local_extract_name("p1r-event-liveness");
        assert_ne!(
            a, b,
            "两个不同 build_id 竟然产出同一个文件名 —— 那就等于回到「同路径互相覆盖」"
        );
        for (id, name) in [("p1x-overflow-identity", &a), ("p1r-event-liveness", &b)] {
            assert!(
                name.contains(id),
                "本机释放文件名 `{name}` 里没有 build_id `{id}`。\n\
                 ⚠ 这条钉的是**构造方式**：远端自部署落点同为 `~/.cc-monitor/bin/`，\n\
                 只有把 build_id 拼进文件名才能让两条路**结构上**撞不上。\n\
                 改成固定名 = 把「无限重装循环」装回来（见 `extract_embedded_to` 头注 D1 段）。"
            );
        }
        assert!(
            !a.contains("cc-monitor-remote"),
            "本机释放名不许长成远端那个名字（`cc-monitor-remote`）—— 那正是要避开的那个文件"
        );
    }

    /// P2z-Y3：**本机那条路不许自己写版本比较** —— 复用 `sftp::deploy_decision`（纯函数）。
    ///
    /// 它会失效的地方（如实写）：`deploy_decision` 只回答「要不要装」，
    /// **不回答「装完对不对」**。本条只挡「另写一套比较逻辑」，不是完整校验。
    #[test]
    fn the_local_path_does_not_hand_roll_version_comparison() {
        let src = guard_core::production_code(include_str!("local_backend.rs"));
        // 判据串运行时拼，免得命中本文件自己的头注。
        let bad = format!("{}_id !=", "build");
        assert!(
            !src.contains(&bad),
            "生产段出现了手写的 build_id 比较（`{bad}`）。\n\
             版本比对只有一个真相源：`sftp::deploy_decision`（纯函数，可单测）。\n\
             另写一套 ⇒ 两处判「要不要装」的逻辑迟早分叉，而分叉的后果是无限重装。"
        );
    }

    #[test]
    fn missing_sidecar_is_an_honest_degrade_that_lists_every_path_it_tried() {
        let r = resolve_with(
            Path::new("/opt/app"),
            "x86_64-unknown-linux-gnu",
            "",
            &never,
        );
        let Resolved::Missing { reason, looked_at } = r else {
            panic!("应当是 Missing");
        };
        assert!(
            reason.contains("F05b"),
            "诊断要指出「谁负责补上它」：{reason}"
        );
        assert_eq!(looked_at.len(), 2, "两个候选都要列出来：{looked_at:?}");
        assert!(
            looked_at
                .iter()
                .any(|p| p.to_string_lossy().contains("x86_64-unknown-linux-gnu")),
            "triple 形态那个候选丢了：{looked_at:?}"
        );
    }

    #[test]
    fn the_triple_suffixed_candidate_wins_over_the_bare_one() {
        // Tauri 打包出来的就是带 triple 的那个名字；裸名只是兜底。
        let triple = "aarch64-apple-darwin";
        let want = PathBuf::from("/opt/app").join(format!("{SIDECAR_STEM}-{triple}"));
        let w = want.clone();
        let exists = move |p: &Path| p == w.as_path() || p.ends_with(SIDECAR_STEM);
        assert_eq!(
            resolve_with(Path::new("/opt/app"), triple, "", &exists),
            Resolved::Found(want)
        );
    }

    #[test]
    fn windows_exe_suffix_is_carried_into_both_candidates() {
        let c = sidecar_candidates(Path::new("C:/app"), "x86_64-pc-windows-msvc", ".exe");
        assert!(
            c.iter().all(|p| p.to_string_lossy().ends_with(".exe")),
            "Windows 上两个候选都得带 .exe：{c:?}"
        );
    }

    #[test]
    fn a_single_crash_restarts() {
        let l = CrashLimits {
            max_crashes: 3,
            window_ms: 10_000,
        };
        assert_eq!(decide(&[1_000], 1_000, l), Decision::Restart);
        assert_eq!(decide(&[1_000, 2_000], 2_000, l), Decision::Restart);
    }

    #[test]
    fn hitting_the_cap_gives_up_and_says_why() {
        let l = CrashLimits {
            max_crashes: 3,
            window_ms: 10_000,
        };
        // ★ 边界：第 3 次就该放弃（`>=`，不是 `>`）。
        let Decision::GiveUp { reason } = decide(&[1_000, 2_000, 3_000], 3_000, l) else {
            panic!("第 3 次崩溃就该放弃 —— 差一位的错本仓出现过");
        };
        assert!(reason.contains("崩了 3 次"), "诊断要带实际次数：{reason}");
        assert!(
            reason.contains("远端功能不受影响"),
            "放弃了要说清影响面：{reason}"
        );
    }

    #[test]
    fn crashes_outside_the_window_do_not_count() {
        let l = CrashLimits {
            max_crashes: 3,
            window_ms: 10_000,
        };
        // 两次很久以前 + 一次刚刚 ⇒ 窗口内只有 1 次 ⇒ 重起。
        assert_eq!(
            decide(&[1_000, 2_000, 100_000], 100_000, l),
            Decision::Restart
        );
        // 窗口**下沿是闭区间**：正好在 now-window 上的那次算进来。
        let Decision::GiveUp { .. } = decide(&[90_000, 95_000, 100_000], 100_000, l) else {
            panic!("窗口内 3 次就该放弃");
        };
    }

    #[test]
    fn the_window_floor_does_not_underflow_near_zero() {
        // `now_ms` 比 window 还小时 `saturating_sub` 兜住；不兜的话是 panic 而不是判错。
        let l = CrashLimits {
            max_crashes: 2,
            window_ms: 10_000,
        };
        assert_eq!(decide(&[0], 5, l), Decision::Restart);
        let Decision::GiveUp { .. } = decide(&[0, 1], 5, l) else {
            panic!("窗口下沿被钳到 0 之后，两次都该算进来");
        };
    }

    /// ★ **零定时器的编译期/源码钉**（C12）。
    ///
    /// 两条一起：`Decision` 里不许出现「隔多久再来」这种字段；
    /// 生产段里不许出现 `sleep`、也不许出现 `try_wait`（那是轮询，本模块的替代方案见头注）。
    #[test]
    fn nothing_in_the_production_path_wakes_itself_up() {
        match Decision::Restart {
            Decision::Restart => {}
            Decision::GiveUp { .. } => {}
        }
        let src = guard_core::production_code(include_str!("local_backend.rs"));
        // 判据串运行时拼，免得命中本文件自己的头注（那里逐字讨论过这两个词）。
        for bad in [format!("thread::{}", "sleep"), format!("try_{}", "wait()")] {
            assert!(
                !src.contains(&bad),
                "生产段出现了 `{bad}` —— 它是「自己醒过来」的构件（C12）。\n\
                 等子进程死请读它 stdout 到 EOF（见模块头注的三条路对比）。"
            );
        }
    }

    /// ★ 生产接线的**前提钉**：本模块今天**不许**被接成「扫仓库 dev 产物」。
    ///
    /// 摸底量到 daemon 一启动就无条件往 tmux server 装全局 hook 且没有开关 ⇒
    /// 扫到 dev 产物就起它会去改用户真实 tmux 的状态。这条钉住那个前提：
    /// 候选路径里**只能有 exe 同目录**，出现任何 `target`/`debug`/仓库相对路径就红。
    /// ★★ **F06b-1d：给窗口的那份 env —— 名字从唯一的家来，sidecar 不在就不设。**
    ///
    /// 判据形态：**纯函数**（跑法：单测 · 钉的性质：wire/边界映射 —— 两维分开写，见 `ROADMAP §4`
    /// 登记的计量缺陷）。它钉两件：
    /// ① `Found` ⇒ 键**必须**是 [`super::DAEMON_BIN_ENV`]（不是另抄一个字面量），值是那条真路径；
    /// ② `Missing` ⇒ **`None`，不是 `Some((名, ""))`** —— 导一个指向空处的路径不会让 ccm 更聪明
    ///    （它那边 `[ -x ]` 一样过不了），只会给「这台机有没有本机后端」多一个假阳性来源。
    #[test]
    fn the_window_env_uses_the_one_home_and_stays_silent_without_a_sidecar() {
        let found = super::env_from_resolved(super::Resolved::Found("/tmp/x/ccm-remote".into()));
        let (k, v) = found.expect("Found 必须给出一对 env");
        assert_eq!(
            k,
            super::DAEMON_BIN_ENV,
            "给窗口的 env 名没走唯一的家 —— 有人另抄了一个字面量"
        );
        assert_eq!(v, "/tmp/x/ccm-remote", "值必须是解析出来的那条真路径");

        let missing = super::env_from_resolved(super::Resolved::Missing {
            reason: "测试".into(),
            looked_at: vec![],
        });
        assert!(
            missing.is_none(),
            "sidecar 不在时必须**什么都不设**，而不是设一个空值 —— \n\
             空值 ≠ 未设（Z01 那条支点）：ccm 那边 `[ -x ]` 照样过不了，\n\
             却给「这台机有没有本机后端」多造了一个假阳性来源。"
        );
    }

    /// ★★ **那个 env 名只有一个家**〔F06b-1 立，F06b-1c 起转为实断言〕。
    ///
    /// # 两条断言，各管一件
    ///
    /// ① **同名**：`shared/ccm` 里**若**出现这个 env 名，必须与 Rust 侧的
    ///    [`super::DAEMON_BIN_ENV`] **逐字相同** —— 名字打错的后果是
    ///    「ccm 永远读不到 ⇒ 永远走本地那条」，而那**看起来完全正常**（诚实降级本来就是它的兜底）。
    ///    ⇒ 这种错**不会自己暴露**，只能靠钉。
    /// ② **原先是前提触发器**（「今天 ccm 还没用它」），它**如期红过一次** ——
    ///    F06b-1c 接线时 `shared/ccm` 一出现这个名字，本条当场红，提醒把一致性一并做完。
    ///    做完后它转成**实断言**：名字必须真出现在 `shared/ccm` 里（≥7 处，按实测）。
    ///    ⚠ 一致性那件事**理由不是「会撞红黄金串」**〔订正 F06b-1b·实测〕：
    ///    真把答案烤进打印串时，`ccm-print-parity`(12) 与 `ccm-contract-parity`(31)
    ///    **两套全绿** —— 前者的 resume 场景全走 `--tmux`（碰不到非容器打印路），
    ///    后者的 A 组比的是**环境键**、六格又全是 `new`。**当时根本没有网。**
    ///    （「44 条黄金串」是 `ccm-cli` 的地板，不是 `ccm-print-parity` 的 —— 那句混了套件。）
    ///    ⇒ 网已在 F06b-1b 补上：`ccm-contract-parity` 的 **A′ 组**比 print↔exec 的 **argv**。
    ///    正解仍是 ccm 头注那句「**打印的是配方，不是值**」。
    ///
    /// ⚠ 判据**不存那个名字的副本**：它从 Rust 的 `const` 读，再去 `shared/ccm` 里找（定框 §4）。
    #[test]
    fn the_daemon_bin_env_name_has_exactly_one_home() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级");
        let ccm = root.join("shared/ccm");
        let src = std::fs::read_to_string(&ccm).expect("读不到 shared/ccm");
        // 中间量自检：读到的必须是那个真文件（它是 696 行的大脚本，不是空串）。
        assert!(
            src.len() > 10_000 && src.contains("ccm"),
            "shared/ccm 只读到 {} 字节 —— 读错文件了，本条会零命中地绿",
            src.len()
        );
        let name = super::DAEMON_BIN_ENV;
        // ① 每一处「像那个名字」的拼写都必须逐字相同 —— **逐处查，不是只查第一处**：
        //    ccm 里现在有好几处（函数、配方、头注），打错任何一处都是静默失败。
        let looks_like = "CCM_DAEMON";
        let mut seen = 0usize;
        for (at, _) in src.match_indices(looks_like) {
            seen += 1;
            // ⚠ **不许 `&src[at..at + 40]`** —— 那是**按字节切**，而这后面紧跟中文注释，
            //   会切在字符中间直接 panic（第一版就是这么炸的）。切片必须落在字符边界上。
            let after = &src[at..];
            // ⚠ 光 `starts_with` 不够：`CCM_DAEMON_BINARY` **也**以 `CCM_DAEMON_BIN` 开头。
            //   变异 Z1 就是这么活下来的 ⇒ 必须查名字后面那个字符是不是标识符字符。
            let exact = after.strip_prefix(name).is_some_and(|rest| {
                !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            });
            let tail: String = after.chars().take(40).collect();
            assert!(
                exact,
                "`shared/ccm` 里那个 env 名与 Rust 侧对不上：ccm 写的是 {tail:?}，\n\
                 而唯一的家是 `DAEMON_BIN_ENV` = {name:?}。\n\
                 ⚠ 名字打错**不会自己暴露** —— ccm 读不到就静静走本地那条（诚实降级是它的兜底）。"
            );
        }
        // ② **接线已发生**〔F06b-1c〕：F06b-1 时这里是「今天还没接线」的前提触发器，
        //    ccm 一用它就红。它**如期红过一次**，本轮把接线做完，于是它转成一条实断言：
        //    名字必须真出现在 `shared/ccm` 里 —— 接线要是被谁整段删了，本条红。
        //    ⚠ **地板按实测写**：当时 7 处（第一版顺手写了 4，是猜的 —— 实测 7）。
        // ★★ **「唯一的家」只能用源码扫描钉，值比较钉不住**〔W3 存活教的，F06b-1d〕：
        //    判据原先只有 `assert_eq!(k, DAEMON_BIN_ENV)`，而**副本的值按定义就相等** ——
        //    把 `DAEMON_BIN_ENV` 换成字面量 `"CCM_DAEMON_BIN"`，那条断言照样绿。
        //    ⇒ **「同一个值」与「同一个来源」是两回事**；要钉来源，就得去源码里数它出现几次。
        let me = guard_core::production_code(include_str!("local_backend.rs"));
        let lit = format!("\"{name}\"");
        let n_lit = me.matches(lit.as_str()).count();
        assert_eq!(
            n_lit, 1,
            "`local_backend.rs` 的生产代码里字面量 {lit} 出现了 {n_lit} 次 —— \n\
             唯一的家是那个 `const`，其余一律引用它。\n\
             ⚠ 值相等的断言**看不见副本**（副本的值按定义就相等），只有数源码才看得见。"
        );
        // ★★ **地板 7 → 1**〔`P4e` 08-13〕：接线**没有缩水，是被去重了**。
        //
        // 原来查找规则在两处各写一遍（`resolve_from_daemon` 与 `resolve_recipe`，
        // 「逐行同构、改一边必须改另一边」），env 名因此出现 7 次。`P4e` 把规则收成
        // **同一个字面量** `DAEMON_BIN_RECIPE`，真跑那侧 `eval` 它 ⇒ 生产段里只剩 1 处。
        //
        // ⚠ **「数字降下来」在本仓通常是坏消息**（棘轮一律只许降是因为那个数是欠账）。
        //   这里方向相反：这个数是「接线在不在」的代理，而**代理变了** ——
        //   ⇒ 不是把地板调低让今天好过，是**把钉子挪到新家上**：下面第二条钉配方本身。
        assert!(
            seen >= 1,
            "`shared/ccm` 里一处 `{looks_like}` 都没有（实得 {seen}）—— \n\
             F06b-1c 的接线（`resolve_from_daemon` + `resolve_recipe`）是不是被删了？"
        );
        // 新家：查找规则本身。删掉它 = 接线没了，而上面那条**看不见**（env 名还在注释里）。
        let ccm_src = include_str!("../../../../shared/ccm");
        guard_core::find_pinned(&guard_core::strip_hash_comment_lines(ccm_src), "DAEMON_BIN_RECIPE=")
            .unwrap_or_else(|e| {
                panic!("{e}\n⇒ `P4e` 的查找规则不在了（或有两份）。它是 `ccm → daemon` 这条路的**唯一**入口：\n   没有它，只有 cc-monitor 亲自注入 env 时才够得着 daemon，而 skill 跑在普通 shell 里。")
            });
    }

    /// ★★ **F05b 接线钉：每一个打包 job 都必须给 sidecar 备好料。**
    ///
    /// # 为什么这条是「遍历发现」而不是「数一遍」
    ///
    /// `externalBin` 一旦注入，`tauri build` 就要求**当前 target** 的那份二进制存在，
    /// 少了它整个 job 以 `resource path ... doesn't exist` 失败（本机实测过）。
    /// ⇒ 发版流水线里**每一处** `tauri build` 都得配三件：原生编 daemon · 按 triple 命名放好 ·
    /// `--config` 注入补丁。少任何一件那个 job 就红，而**发版红是最贵的红**（tag 已经打出去了）。
    ///
    /// 所以发现机制是**遍历 workflow 里所有 `tauri build` 调用**，不是手写「有两个 job」。
    ///
    /// ⚠ 中间量自检：先断言真的找到了 `tauri build` 调用（找不到 = 抽取器坏了，本条零命中地绿）。
    #[test]
    fn every_bundle_job_stages_the_sidecar_before_building() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级");
        let wf = root.join(".github/workflows/release.yml");
        let src = std::fs::read_to_string(&wf).expect("读不到 release.yml");
        // 运行时拼，免得命中本文件自己的说明文字。
        let verb = format!("{} build", "tauri");
        let calls: Vec<&str> = src
            .lines()
            .filter(|l| l.contains(&verb) && l.trim_start().starts_with("run:"))
            .collect();
        assert!(
            calls.len() >= 2,
            "在 release.yml 里只找到 {} 处 `{verb}` 调用 —— 抽取器坏了或流水线变形了\n\
             （实测两处：Windows 的 nsis+msi 与 Linux 的 deb）。本条会零命中地绿。",
            calls.len()
        );
        for c in &calls {
            assert!(
                c.contains("tauri.sidecar.conf.json"),
                "这处打包没有注入 sidecar 补丁配置：{}\n\
                 ⇒ 安装包里不会带本机后端（C7），而 `local_backend` 会恒走诚实降级。",
                c.trim()
            );
        }
        // 三件里的另外两件：原生编 + 按 triple 命名。逐个 job 数得太脆，
        // 这里钉「整份 workflow 里这两件各至少与打包调用数一样多」。
        let native = src.matches("Build local backend sidecar").count();
        let staged = src.matches("Stage sidecar for externalBin").count();
        assert!(
            native >= calls.len() && staged >= calls.len(),
            "打包调用 {} 处，而「原生编 daemon」{native} 处、「按 triple 放好」{staged} 处 —— \n\
             有 job 会以 `resource path ... doesn't exist` 失败，而那是**发版时**才炸。",
            calls.len()
        );
        // sidecar 的名字只有一个家：这里不抄它，从 Rust 侧读。
        assert!(
            src.contains(super::SIDECAR_STEM),
            "release.yml 里没有出现 `{}` —— 拷过去的名字与消费侧对不上",
            super::SIDECAR_STEM
        );
    }

    #[test]
    fn candidates_never_point_into_a_build_tree() {
        let c = sidecar_candidates(Path::new("/opt/app"), "x86_64-unknown-linux-gnu", "");
        for p in &c {
            let s = p.to_string_lossy();
            for forbidden in ["target", "debug", "release", ".."] {
                assert!(
                    !s.contains(forbidden),
                    "候选路径含 `{forbidden}`：{s}\n\
                     扫 dev 产物就起 daemon = 去改用户真实 tmux server 的状态（它无条件装全局 hook，\
                     而且没有开关）。只许在 exe 同目录找。"
                );
            }
            assert!(p.starts_with("/opt/app"), "候选跑出了 exe 目录：{s}");
        }
    }

    /// ★ **生产接线钉**：`lib.rs` 的启动路径**真的**调了本模块的生产入口。
    ///
    /// 这条是 F03 教训的直接产物：「模块存在 ≠ 模块被调用」。
    /// 本模块写得再全，只要 `lib.rs` 里没那一行，本机后端就永远不会被起 ——
    /// 而上面那些单测**全都照样绿**。
    ///
    /// ⚠ **P2z 改过一次口径，记下为什么不是「改弱」**：原来钉的是字面量
    /// `local_backend::start_if_present`。P2z 把接线换成了 `start_or_extract`
    /// （exe 旁边没有就释放内嵌那份），那条字面量当场红 —— **它在做它的岗位**。
    /// 改法不是把它删掉、也不是换成两个名字任选其一（那会让「一个都没接」漏网），
    /// 而是钉「**至少接了一个已知生产入口，且那个入口确实存在于本模块**」。
    /// ⇒ 将来再改入口名，这条仍会红，除非同时在这张清单里登记 —— 那正是要的。
    #[test]
    fn the_startup_path_really_calls_this_module() {
        // ⚠ **P2s 搬过一次家**：接线原来住 `lib.rs` 的 `run()` 里，P2s 把它抽进
        // `local_daemon.rs`（理由是结构性的：`#[tauri::command]` 不能与 `generate_handler!`
        // 同模块）。⇒ 语料从「一个文件」变成「启动路径这两个文件」。
        // **这不是把判据放宽**：仍然要求「至少一个已知入口被接上」，只是接线可以住这两处之一；
        // 两个文件都不接，照样红（M8 变异实测）。
        let prod = format!(
            "{}\n{}",
            guard_core::production_code(include_str!("../../lib.rs")),
            guard_core::production_code(include_str!("../../local_daemon.rs")),
        );
        let prod = prod.as_str();
        let me = guard_core::production_code(include_str!("local_backend.rs"));
        // 本模块今天对外的生产入口清单。加入口 = 往这里加一条（**不许**留空清单）。
        assert!(
            !ENTRIES.is_empty(),
            "抽取器自检：入口清单空了 ⇒ 下面两条断言都会零命中地绿"
        );
        // ★★ **完备性自检**〔D 阶段补审 08-11 新增〕：`ENTRIES` 是**手写白名单**，
        // 原来只校验「清单里的名字存在」与「清单非空」，**没有任何一条校验它是完备的**。
        // ⇒ 新增第三个启动入口（不走 `supervise_with_stdio` / 不传消费者）时，
        // `the_production_entry_hands_the_stdio_consumer_down` **逮不到**——它只遍历清单。
        //
        // 完备性怎么判：本模块里**每一个调了 `supervise`（含 `_with_stdio`）的 `pub fn`**
        // 都必须在清单里。`supervise` / `supervise_with_stdio` 自己除外（它们是被调的那一方）。
        {
            let mut missing: Vec<String> = Vec::new();
            let mut seen = 0usize;
            for (i, l) in me.lines().enumerate() {
                let t = l.trim_start();
                if !t.starts_with("pub fn ") {
                    continue;
                }
                let name: String = t["pub fn ".len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if name == "supervise" || name == "supervise_with_stdio" {
                    continue;
                }
                let body: String = me
                    .lines()
                    .skip(i + 1)
                    .take_while(|l| *l != "\u{7d}")
                    .collect::<Vec<_>>()
                    .join("\n");
                if !body.contains("supervise") {
                    continue;
                }
                seen += 1;
                if !ENTRIES.contains(&name.as_str()) {
                    missing.push(name);
                }
            }
            assert!(
                seen >= 2,
                "只扫到 {seen} 个「会起进程的 pub fn」—— 抽取面坏了，完备性自检在空转"
            );
            assert!(
                missing.is_empty(),
                "这些 `pub fn` 会起被监护的进程，却**不在 `ENTRIES` 清单里**：{missing:?}\n\
                 ⇒ 手写白名单漏了它 ⇒ `the_production_entry_hands_the_stdio_consumer_down`\n\
                 只遍历清单，**逮不到这条新入口没接消费者**。清单要跟着实际入口走。"
            );
        }
        for e in ENTRIES {
            assert!(
                me.contains(&format!("pub fn {e}(")),
                "入口清单里的 `{e}` 在本模块生产段里不存在 —— 清单腐了。\n\
                 （这条防的是「把清单写宽以求绿」：写一个不存在的名字进去也不许过。）"
            );
        }
        let wired: Vec<&str> = ENTRIES
            .iter()
            .copied()
            .filter(|e| prod.contains(&format!("local_backend::{e}")))
            .collect();
        assert!(
            !wired.is_empty(),
            "`lib.rs` 的生产段里一个本模块生产入口都没有（找过 {ENTRIES:?}）—— \n\
             本机后端没有被接上启动路径。判据全绿而功能从不运行，\n\
             正是「模块存在 ≠ 模块被调用」那个坑。"
        );
        assert!(
            prod.contains("CCM_TARGET_TRIPLE"),
            "`lib.rs` 生产段里找不到 `CCM_TARGET_TRIPLE` —— 接线缺了 target triple 这个宿主知识"
        );
        // 句柄必须被**存下来**：不存就没人能 `stop()`。
        //
        // ⚠ **原版是恒绿的**〔D 阶段补审 08-11 逮到〕：它断言 `prod.contains("LOCAL_BACKEND")`，
        // 而 `prod` = `lib.rs` + `local_daemon.rs` 的生产段，**那个静态量的声明就在后者里**
        //（`pub static LOCAL_BACKEND: …`）。把「存进去」那一句删掉，字面量照样在（声明处 + 三个读者）
        // ⇒ 绿。要让它红只能删掉静态量本身，而那会先编译错。
        // ⇒ 改成钉**赋值那一句**（`*g = Some(h)`），那才是「存下来」这件事。
        guard_core::find_pinned(&prod, "*g = Some(h);").unwrap_or_else(|e| {
            panic!(
                "监护句柄没被**存**进 `LOCAL_BACKEND`（{e}）—— 退出时无法 `stop()`。\n\
                 ★ 别把「文件里提到过 LOCAL_BACKEND」当成证据：声明本身就提到它。"
            )
        });
    }

    /// ★ **接线钉之二：退出路径真的收尸。**
    ///
    /// ⚠ 这条是 **F+02 回看抓出来的洞**，而不是 F05a 自己想到的：
    /// F05a 当时把退出钩子整段删掉做变异，**815 条测试全绿**，CI 也不会红
    /// （本仓 clippy 只报警告、不带 `-D warnings`）。
    /// 也就是说「不 `stop()` 就成游魂进程」这条性质当时**只被 clippy 的 `dead_code` 偶然覆盖**
    /// —— 一旦别处也用到 `stop()`，那条告警就消失，这条性质彻底失守。
    ///
    /// **「靠一条告警守着」等于没守。** 上一条接线钉只钉了「入口被调用」，
    /// 这条钉「出口也被接上」——同一件事的两半。
    ///
    /// # ★★ P2s 翻面（08-11）：原来钉「必须 `stop()`」，现在钉「必须**按策略**决定 stop」
    ///
    /// `C8` 裁定「每台机一个开关，管随 monitor 退出是否 kill，**默认不 kill**」
    /// ⇒ 无条件 `stop()` 与定框直接冲突，这条判据必须翻面。
    ///
    /// ⚠ **翻面最容易翻成一条更弱的判据**（比如只钉「体内提到了策略这个词」）。
    /// 所以本条保留了它原来要挡的那个洞，并额外挡住翻面自身可能引入的两个新洞：
    ///
    /// | 洞 | 谁挡 |
    /// |---|---|
    /// | 整段退出钩子被删掉（原洞，实测「815 条测试全绿」） | `RunEvent::Exit` 那条 needle |
    /// | 退回无条件 `stop()`（默认被违反，用户没设也被杀） | 「`.stop()` 必须在策略读之后、且中间有条件」 |
    /// | 读了策略却永远不 stop（开关拨到「杀」也没反应） | 「体内必须有 `.stop()`」 |
    ///
    /// ★ **原理由（「不 stop 就成游魂进程」）实测是错的**〔08-11，P2s §0a〕：
    /// daemon 是纯 stdio 子进程，monitor 一退读端就断，它 **153 毫秒**内自己 broken-pipe 退出。
    /// ⇒ 不杀**不会**留游魂。那条理由曾是这条判据存在的全部依据，现在它的依据换成了
    /// 「开关必须真的起作用，两个方向都要」。**依据换了就写出来，不假装它没变。**
    #[test]
    fn the_exit_path_really_stops_the_local_backend() {
        let prod = guard_core::production_code(include_str!("../../lib.rs"));
        for needle in ["RunEvent::Exit", "LOCAL_BACKEND", ".stop()"] {
            assert!(
                prod.contains(needle),
                "`lib.rs` 的生产段里找不到 `{needle}` —— 退出路径没接上。\n\
                 ⚠ 这条洞**不会被别的判据抓到**（实测：删掉整段钩子，815 条测试全绿），\
                 而 clippy 的 dead_code 只是偶然覆盖。"
            );
        }
        // ⚠⚠ **08-08 订正：原来这里比的是「文件里最后一个 `.stop()` 在不在 Exit 之后」。**
        //
        // 实测：把退出臂里的 `h.stop()` 拿掉、在文件别处留一处，本条**照样绿** ——
        // 而它自陈要挡的正是「出口没接上 ⇒ 游魂进程」，头注还写着「删掉整段钩子，
        // 815 条测试全绿」。`rfind` 取的是**任意一处**，不是**这一处**。
        // ⇒ 改成把退出臂的**体**切出来，`.stop()` 必须在**体内**。
        let arm_at = prod.find("RunEvent::Exit").expect("上面已断言存在");
        let body = {
            // 从锚点往后找第一个左花括号，再按配平切到它的收尾。
            let bytes = prod.as_bytes();
            let open = (arm_at..bytes.len())
                .find(|&i| bytes[i] == b'{')
                .expect("`RunEvent::Exit` 之后找不到块起点 —— 形状变了，先修锚点");
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
            &prod[open..end]
        };
        assert!(
            body.len() > 40 && body.len() < 4000,
            "切出来的退出臂只有 {} 字节 —— 配平切错了，本条会零命中地绿",
            body.len()
        );
        // ⚠ 锚点必须**当场核唯一性**（`every_position_comparison_over_source_pins_and_bounds_its_anchors`
        // 的第三条纪律）：本文件自己就是那条判据头注里的活样本 ——
        // 旧版用的是**反向查找**（那个原语名字里带 r 的），命中「任意一处」，对不对全靠排序运气。
        // ⚠ 这行不许把那个原语连着左括号写出来 —— 检测器扫的就是那个字面量，
        // 写在散文里也会被算成「本判据用了它」（本条第一版就是这么被误判的）。
        // `find_pinned` = 恰好一处 + 两侧有边界，位置比较才站得住。
        let stop_at = guard_core::find_pinned(body, ".stop()").unwrap_or_else(|e| {
            panic!(
                "退出臂里没有 `.stop()`。\n\
                 ★ 「文件里某处有一个 `.stop()`」不算 —— 本条要的是**这一处**。\n\
                 没有它，开关拨到「monitor 退出时结束 daemon」也**不会有任何反应**。\n\
                 ⚠ 08-08 实测：把这一句拿掉、在别处留一个 `.stop()`，旧版本条照样绿。\n\
                 （锚点诊断：{e}）"
            )
        });
        // ── P2s 翻面新增的两条 ──────────────────────────────────────────
        let policy_at = guard_core::find_pinned(body, "kill_on_exit(").unwrap_or_else(|e| {
            panic!(
                "退出臂里没有读 `kill_on_exit(` —— 它在**无条件**收本机后端。\n\
                 那与 `C8`③「默认不 kill」直接冲突：用户什么都没设就被杀 daemon，\n\
                 而开关默认是关着的。（锚点诊断：{e}）"
            )
        });
        assert!(
            policy_at < stop_at,
            "退出臂里 `.stop()` 出现在读策略**之前** —— 那就不是「按策略决定」，\n\
             而是「先杀了再查开关」。（读策略那句写在后面也可能只是打日志用。）"
        );
        // ⚠ **原版只要求「两者之间有个 `if `」**〔D 阶段补审 08-11 逮到〕：
        // `let kill = kill_on_exit(…); if h.current_pid().is_some() { h.stop(); }`
        // **照样绿，而开关彻底失效**。三样东西都在，语义却不是那回事。
        // ⇒ 改成：从 `let <名> = …kill_on_exit(` 反推出绑定名，再要求 `if <名>` ——
        // 钉的是「**那个 if 判的就是策略值**」，而不是「有个 if」。
        // （从绑定名反推而不是写死 `if kill`：改变量名不该假红。）
        let bind = {
            let line = body[..policy_at]
                .lines()
                .last()
                .expect("策略那一行之前总有内容");
            let t = line.trim_start();
            let rest = t
                .strip_prefix("let ")
                .unwrap_or_else(|| panic!("读策略那一行不是 `let <名> = …` 的形状：{t:?}"));
            rest.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
        };
        assert!(
            !bind.is_empty(),
            "从读策略那一行抠不出绑定名 —— 抽取面画错了，本条在空转"
        );
        let between = &body[policy_at..stop_at];
        assert!(
            between.contains(&format!("if {bind}")),
            "读了策略、也有 `.stop()`，但中间那个条件**判的不是策略值** ——\n\
             绑定名是 `{bind}`，而这一段里没有 `if {bind}`：\n{between}\n\
             ★ 补审给的骗法：`let kill = kill_on_exit(…); if h.current_pid().is_some() {{ h.stop(); }}`\n\
             三样东西（读策略 / if / stop）都在，开关却彻底失效。"
        );
    }

    // ── 真进程（`#[ignore]`，由 e2e/local-backend-supervise.sh 驱动）──────────

    /// ★★ **起真 daemon 的 e2e 必须 fail-closed 地要一个私有 tmux 目录**
    /// 〔audit-0805 08-08，Phase G 第 51 件〕。
    ///
    /// 本模块头注逐字写着「**绝不让被监护的 daemon 碰用户真实的 tmux server**」。
    /// 那道保护今天全靠下面那句 `.expect("要 CCM_E2E_TMUX_TMPDIR")` ——
    /// 裸跑 `cargo test -- --ignored` 会 panic 而不是去连真 server，形态是对的。
    ///
    /// **但没人钉它**：08-08 实测把那句换成 `.unwrap_or_default()`，
    /// **全仓 982 条判据一条不红**；此后任何一次 `--ignored` 都会把真 daemon
    /// 接到用户的 tmux server 上。`#[ignore]` 测试平时不跑 ⇒ 它坏了也没人知道，
    /// 正是「新分支平时没人走」那一族（本仓已栽过六次）。
    ///
    /// 人群**从源码派生**：本文件里 `#[ignore]` 且体内出现 `CCM_E2E_DAEMON`
    /// （= 真的要一个 daemon 二进制）的测试。用假二进制的那条不在其中。
    ///
    /// # ⚠⚠ 射程订正〔`K-R7-D2`，08-31〕：**人群画在两个属性上，都够不着最危险的那一形**
    ///
    /// 本条的人群有两个条件，**两个都是「怎么标记的」**：`#[ignore]` · 提到 `CCM_E2E_DAEMON`。
    /// 而本文件里 `the_local_daemon_really_registers_an_inbound_client` 是**普通 `#[test]`**、
    /// 用的是**内嵌**那份二进制（不经 `CCM_E2E_DAEMON`）⇒ **两个条件各差一个**，
    /// 于是它起着真 daemon 而本条一声不吭。`local_daemon.rs` 那条姊妹判据同理够不着。
    /// ⇒ 正题已经搬到 `local_daemon.rs` 的
    /// `every_test_that_starts_the_real_daemon_demands_a_private_tmux`：
    /// 人群按「**那个二进制哪来的**」派生（内嵌目录 / `CCM_E2E_DAEMON` / `E2eSandbox::demand`），
    /// **两个文件一起扫**，不看任何属性。
    /// **本条留着**（它守的是一格更窄但仍然真的性质），但别把它读成「起真 daemon 有人守了」。
    #[test]
    fn every_real_daemon_e2e_demands_a_private_tmux_dir() {
        const REAL: &str = "CCM_E2E_DAEMON";
        // ⚠⚠ 〔`P0e` 08-12〕这个名字换过一次，**换的是机制不是名字**：
        //   原来是 `CCM_E2E_TMUX_TMPDIR`（把私有目录传给 daemon）—— 而 `$TMUX` 一有值
        //   就会压过它，那正是 08-11 打没用户 9 个真实会话的机制，`C7i` 因此逐字禁止
        //   「靠 `TMUX_TMPDIR` 做隔离」。
        //   现在传的是**带 shim 的 PATH**：daemon shell out 的 tmux 会被强插 `-L`，
        //   **显式选择器压得过 `$TMUX`**。本条钉的性质一个字没变：
        //   **起真 daemon 的 e2e 必须 fail-closed 地要一个私有 tmux 隔离**。
        const PRIVATE_TMUX: &str = "CCM_E2E_TMUX_SHIM_BIN";
        // 取 shim 的**唯一入口**〔`K-R7` 09-01〕：住 `local_daemon::tests::demand_tmux_shim`。
        const GATE: &str = "demand_tmux_shim(";
        let src = include_str!("local_backend.rs");
        // ⚠ **不在语料串上做裸 `split`**：`needle_anchor_registry` 的递减棘轮把它
        //   判为「匹配单位比事实小」的一族，且**不许调上限**（本条第一版就栽在这）。
        //   改成按行扫、遇到下一处 `#[test]` 收尾 —— 边界是「行」，比子串确定。
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
        // ⚠ 认属性要**整行相等**，不能 `contains` —— 本条的**文档注释里**就写着
        //   `#[ignore]` 与 `CCM_E2E_DAEMON`，第一版因此把自己也算进了人群，
        //   然后拿自己的 `const` 行去判 fail-closed，当场自红。
        //   （F24 那一族：匹配单位比事实大；这次事实是「一条属性」，而我匹配了「提到过」。）
        let is_ignored = |c: &String| c.lines().any(|l| l.trim() == concat!("#[ig", "nore]"));
        let real_e2e: Vec<&String> = chunks
            .iter()
            .filter(|c| is_ignored(c) && c.contains(REAL))
            .collect();
        // 抽取器自检：一条都没抓到 ⇒ 下面整条空转。
        assert!(
            !real_e2e.is_empty(),
            "本文件里找不到「`#[ignore]` 且要 {REAL}」的测试（08-08 实测 1 条）—— \
             抽取器坏了或那条 e2e 被删了，本条此刻无效"
        );
        // ⚠⚠ **剥注释这一步是 09-01（`C` 第六拍）补的**〔`D5` 阻塞 2；PM `§0s` 五 ①〕。
        //
        //   下面那两格原来判在 `c.lines()`（**没剥注释的原始行**）上，而下游
        //   —— `local_daemon.rs` 那条正题的 ㈡（`shim_first_on_path` 取绑定名）——
        //   读的是它自己那份 `code_only`（**剥掉 `//` 打头的行**）。**两条判在不同的文本上。**
        //   `D5-M4` 实证：在合法落点上方加**一行纯注释**提到那个口
        //   ⇒ **本条当场红（新红 1）、主守卫一声不吭**，而本条的报文却说
        //   「因为下游要从这一行上取绑定名」——**那句归因在唯一分岔的场合恰好是假的**。
        //
        //   🔴 **这个坑本仓在这条守卫上已经栽过一次**：主守卫 `code_only` 上方那段注释逐字
        //   写着「判人群要看**代码**，不能看文档注释 —— `local_backend.rs` 那条同族守卫在
        //   这上面**自红过一次**」，主守卫为此加了 `code_only`，
        //   **而 `C` 第五拍给本条新加的那一格没有跟着剥**
        //   ⇒ 那正是「**只修一半 / 同族当场复发**」，只换了个动词（从「量词」换成
        //   「判在剥没剥注释的哪一份文本上」）。
        //
        //   ⇒ 现在**先剥再判**，剥法逐字与主守卫那份 `code_only` 相同
        //   （`trim_start().starts_with("//")`，`///` 也在内）——**同一把尺子**。
        //   ⚠ 只让本条**更松**（少几行可看），不放水：被判的那一行本来就得是代码。
        let code_only = |c: &str| -> String {
            c.lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        for c in real_e2e {
            let code = code_only(c);
            let name = code
                .lines()
                .find_map(|l| l.trim().strip_prefix("fn "))
                .and_then(|r| r.split_once('('))
                .map(|(n, _)| n)
                .unwrap_or("<未知>");
            // ⚠⚠ **这一格 09-01 换过一次判法**〔`K-R7` `§0q` 裁一 · 出路乙，`D4` 阻塞 1〕。
            //   原来是两句：先找「含 `CCM_E2E_TMUX_SHIM_BIN` 的那一行」，再判那行含 `.expect(`。
            //   `D4` 那一刀（`var(SHIM).unwrap_or_else(|_| var(<真 PATH>).expect(..))`）
            //   **同一行上三样东西都还在** ⇒ 旧判据说合规，而 fail-closed 没了。
            //   🔴 **本条是那一族的第 4 处，而 `§0q` 只点了 `local_daemon.rs` 的 3 处** ——
            //     09-01 实测：只改本条人群里那条 e2e（`e2e_the_supervisor_restarts_…`）的取值，
            //     **全量门禁新红 0**，本条一声不吭。⇒ 一起换。
            //   现在判的是「**走没走取 shim 的那个唯一入口**」；
            //   「**那个口关不关得上**」由 `local_daemon.rs` 的
            //   `the_one_shim_gate_really_fails_closed` 在**默认门禁里真跑一遍**（不是文本钉）。
            //   ⚠ **判在剥过注释的 `code` 上**（见上面那段）—— 09-01 之前判的是原始行。
            let line = code
                .lines()
                .find(|l| l.contains(GATE))
                .unwrap_or_else(|| {
                    panic!(
                        "`{name}` 会起一个**真** daemon，却没走取 shim 的那个唯一入口 \
                         （`{GATE}`）—— 它会连上用户真实的 tmux server。\n\
                         本模块头注写的是「绝不」。⚠ 那个变量叫 `{PRIVATE_TMUX}`，\
                         而**取它只许从那一个口取**（`local_daemon::tests::demand_tmux_shim`）：\
                         口里那句 fail-closed 是被一条真跑的测试钉住的，\
                         自己现取就退回到「谁也没在守」。\n\
                         ⚠ 判的是**剥掉 `//` 打头的行之后**的本体：写在注释里提一句不算数，\
                         也不再让本条假红（`D5-M4`）。"
                    )
                });
            // 反空真：光找到那一行不够 —— 它得真是**赋值**给某个绑定的。
            assert!(
                line.trim_start().starts_with("let "),
                "`{name}` 里出现 `{GATE}` 的那一行（**剥掉注释之后**）不是一条绑定：\n  {}\n\
                 ⇒ 本条只认「`let <名字> = …` 一行到底」这一种写法，因为下游\
                 （`local_daemon.rs` 那条正题的 ㈡）要从**同一把尺子剥出来的这一行**上取绑定名。\n\
                 ⚠ **两条已知的边界，写在这里免得下一个人以为自己写错了**：\n\
                 ① **假阳**：把这条调用按 rustfmt 在 `=` 后断行（`let shim =` 换行再写调用）\
                 ⇒ 含口的那一行不再以 `let ` 打头，本条与下游 ㈡ **一起红**，\
                 而那是一条语义逐字不变的合法写法（`D5-M5` 实证）。**今天要的是单行写法。**\n\
                 ② 本条 09-01 起判在**剥过注释**的文本上（`D5-M4`：一行纯注释曾让本条假红，\
                 而下游一声不吭）—— 两条判在不同文本上的日子结束了。",
                line.trim()
            );
        }
    }

    /// ★ 起真 daemon → 杀它 → 看它自己回来。
    #[test]
    #[ignore]
    fn e2e_the_supervisor_restarts_a_real_daemon_after_it_is_killed() {
        let bin = std::env::var("CCM_E2E_DAEMON").expect("要 CCM_E2E_DAEMON");
        let shim = crate::local_daemon::demand_tmux_shim("本条起真 daemon");
        let claude = std::env::var("CCM_E2E_CLAUDE_DIR").expect("要 CCM_E2E_CLAUDE_DIR");
        let events: Arc<Mutex<Vec<SuperviseEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev = events.clone();
        let h = supervise(
            PathBuf::from(&bin),
            vec!["--tail-only".into()],
            vec![
                // `C7i`：给 daemon 一条**前面挂着 shim** 的 PATH —— 它 shell out 的 tmux
                // 会被强插 `-L`。比传 `TMUX_TMPDIR` 硬：`$TMUX` 压不过显式选择器。
                (
                    "PATH".into(),
                    format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
                ),
                ("CLAUDE_CONFIG_DIR".into(), claude),
            ],
            CrashLimits::default(),
            Arc::new(|| 0),
            Arc::new(move |e| ev.lock().expect("ev").push(e)),
        );
        let first = spin(|| h.current_pid()).expect("10s 内没起来");
        println!("E2E-OK 真 daemon 起来了 pid={first}");
        kill_for_test(first);
        let second = spin(|| h.current_pid().filter(|p| *p != first)).expect("10s 内没重起");
        assert_ne!(first, second, "pid 没变 ⇒ 没有真的重起");
        println!(
            "E2E-OK 被杀之后自己回来了 pid={second}（attempts={}）",
            h.attempts()
        );
        h.stop();
        // stop() 必须真的把它收掉 —— 不收就是游魂进程。
        assert!(
            spin(|| h.current_pid().is_none().then_some(())).is_some(),
            "stop() 之后当前 pid 还在"
        );
        println!("E2E-OK stop() 把当前子进程收掉了");
        let got = events.lock().expect("ev").clone();
        assert!(
            got.iter()
                .filter(|e| matches!(e, SuperviseEvent::Started { .. }))
                .count()
                >= 2,
            "Started 事件少于 2 次 ⇒ 事件面没有如实报告重起：{got:?}"
        );
        println!("E2E-OK 事件面报告了 {} 条", got.len());
    }

    /// ★ 一个**必崩**的二进制要在上限内被判死，而不是无限自旋。
    #[test]
    #[ignore]
    fn e2e_a_binary_that_always_dies_is_given_up_on_within_the_cap() {
        let dir = std::env::var("CCM_E2E_WORK").expect("要 CCM_E2E_WORK");
        let bin = PathBuf::from(&dir).join("always-dies.sh");
        std::fs::write(&bin, "#!/bin/sh\nexit 7\n").expect("写不出脚本");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let events: Arc<Mutex<Vec<SuperviseEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev = events.clone();
        let h = supervise(
            bin,
            vec![],
            vec![],
            CrashLimits {
                max_crashes: 3,
                window_ms: 60_000,
            },
            Arc::new(|| 1),
            Arc::new(move |e| ev.lock().expect("ev").push(e)),
        );
        let gave_up = spin(|| {
            events.lock().expect("ev").iter().find_map(|e| match e {
                SuperviseEvent::GaveUp { reason } => Some(reason.clone()),
                _ => None,
            })
        })
        .expect("10s 内没放弃 —— 它在无限自旋？");
        assert!(gave_up.contains("崩了 3 次"), "放弃理由不对：{gave_up}");
        let n = h.attempts();
        assert_eq!(n, 3, "应当正好起 3 次就放弃，实得 {n}");
        println!("E2E-OK 必崩二进制在 3 次内被判死，没有自旋");
    }

    /// ★ 真文件系统上「没有 sidecar」⇒ 诚实降级（这条不起任何进程）。
    #[test]
    #[ignore]
    fn e2e_a_missing_sidecar_degrades_honestly_against_the_real_filesystem() {
        let dir = std::env::var("CCM_E2E_WORK").expect("要 CCM_E2E_WORK");
        let r = resolve_with(
            Path::new(&dir),
            "x86_64-unknown-linux-gnu",
            "",
            &|p: &Path| p.exists(),
        );
        let Resolved::Missing { looked_at, .. } = r else {
            panic!("空目录里居然找到了 sidecar");
        };
        assert_eq!(looked_at.len(), 2);
        println!("E2E-OK 缺 sidecar 时诚实降级，且列出了 2 条找过的路径");
    }

    // ── 测试侧助手。**只在测试里**，生产段没有任何轮询 ─────────────────────
    fn spin<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
        for _ in 0..200 {
            if let Some(v) = f() {
                return Some(v);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        None
    }

    /// 测试侧杀进程：起一个 `kill`。**生产段不做这件事**（`stop()` 走 `Child::kill`）。
    fn kill_for_test(pid: u32) {
        #[cfg(unix)]
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // F16：三处失败模式（都是 F05a 我自己写的代码，F12 的 `/full-audit` 逐行核出来的）
    // ═══════════════════════════════════════════════════════════════════════

    /// 监护线程「等它死 + 收尸」那一段的生产源码。
    fn wait_section() -> String {
        let prod = guard_core::production_code(include_str!("local_backend.rs"));
        let at = prod
            .find("pub fn supervise(")
            .expect("找不到 `supervise` —— 改名了就把下面三条一起改");
        prod[at..].to_string()
    }

    /// ★ ①「等它死」不许把 stdout 攒起来。
    ///
    /// 原来是 `read_to_end(&mut Vec::new())` —— 被监护的 daemon **持续产帧**
    /// （那正是头注用来论证「它不会关 stdout」的理由）⇒ 那个 `Vec` 单调增长且没有消费者。
    /// ⚠ 今天不咬人只因为 `resolve_beside_this_exe` 恒 `Missing` —— **离生效只差一个配置项**。
    #[test]
    fn waiting_for_death_never_accumulates_the_child_stdout() {
        let sec = wait_section();
        // 运行时拼，免得命中本文件自己的说明。
        let bad = format!("read_to{}", "_end");
        assert!(
            !sec.contains(bad.as_str()),
            "`supervise` 里又出现了把 stdout 读进内存的写法 —— 被监护对象是**持续产帧**的，\n\
             那个缓冲区会随本机使用时长单调增长、且没有任何消费者。\n\
             ⇒ 用 `io::copy` 到 `io::sink()`：**EOF 语义完全不变**，但一个字节都不留。"
        );
        assert!(
            sec.contains("std::io::copy(") && sec.contains("std::io::sink()"),
            "找不到 `io::copy(… , io::sink())` —— 那是本条要求的那个形态"
        );
    }

    /// ★★ ②`wait()` 不许在持锁的情况下调。
    ///
    /// 「子进程关掉 stdout 但继续活着」是本模块**已登记的诚实边界**；那时 `wait()` 会久等，
    /// 而 `stop()` 第一件事就是 `self.child.lock()` 且它跑在**主线程**（`RunEvent::Exit`）
    /// ⇒ 持锁 `wait()` 会把「误判它死了」升级成**应用退不出去**。
    #[test]
    fn reaping_never_holds_the_child_lock_while_it_waits() {
        let sec = wait_section();
        // 先把子进程从锁里 `take()` 出来，再在锁外 `wait()`。
        // ⚠ **锚点从「字面形状」改成「性质」**〔D 阶段补审 08-11，B4〕。
        //
        // 原来钉的是逐字的 `child.lock().ok().and_then(|mut g| g.take())`。
        // B4 要在 take 之前按 `ConsumerExit` 补一刀（早退时子进程还活着），
        // 那一句必然变形 ⇒ 判据当场红。**但它守的性质一个字没变**：
        // 「持锁的那个块里不许有 `wait()`」。⇒ 改成钉那句话本身。
        //
        // ★ 这是「判据挡路 ≠ 把判据放宽」的又一例：形状换了，性质原样，
        // 而且新写法**更硬** —— 原版认一串特定链式调用，现版认「块内不许 wait」。
        let take_at = sec.find("let mut reaped = {").expect(
            "收尸段不再是「持锁块里 take 出来、块外 wait」那个形状 ——\n\
             那意味着 `wait()` 可能又回到了锁里面。后果不是「误判它死了」，\n\
             是 `stop()` 在主线程永久阻塞、窗口关了进程退不出去。",
        );
        let held_end = {
            let blk = &sec[take_at..];
            let open = blk.find('{').expect("上面刚 pin 过");
            let (mut depth, mut end) = (0i32, open);
            for (i, c) in blk.char_indices().skip(open) {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let held = &blk[open..=end];
            assert!(
                held.len() > 30 && held.len() < 2000,
                "切出来的持锁块 {} 字节 —— 配平切错了，本条在空转",
                held.len()
            );
            assert!(
                !held.contains("wait("),
                "持锁块里出现了 `wait(`：\n{held}\n\
                 ⇒ `stop()` 第一件事就是拿同一把锁，而它跑在**主线程**（`RunEvent::Exit`）\n\
                 ⇒ 持锁 `wait()` 会把「误判它死了」升级成**应用退不出去，只能 kill -9**。"
            );
            // ★★ **B4 的正题**：早退时必须补一刀 —— 变异实测「摘掉它全套件照样绿」，
            // 说明我修的那件事本来没有判据。补在这里而不是新开一条：
            // 它与「wait 不许在锁里」是**同一段代码的两条性质**，分开写会各自漂。
            assert!(
                held.contains("ConsumerExit::Early") && held.contains("kill()"),
                "持锁块里没有「早退就补一刀」那一支：\n{held}\n\
                 ⇒ 消费者因**读错误或 panic** 返回时子进程还活着，而它已被从共享锁里摘走\n\
                 ⇒ 并发的 `stop()` 看到 `None`，**一个字节的 kill 都不发**，却仍返回「已停」。\n\
                 ⚠ 不许改成无条件 kill：那会把正常退出码换成信号死，而 `decide()` 按退出码\n\
                 分崩溃/正常（风险 5d）。也不许去探子进程 —— 本模块禁那类构件。"
            );
            take_at + end
        };
        // ⚠ **不能只写 `sec.find(".wait()")`** —— 那会命中**关窗块**里那个
        //   `c.kill(); c.wait();`（它在 `take()` 之前，且它是对的：那处本来就持锁、
        //   而子进程刚被 kill、不会久等）。第一版就是这么写的，**判据自己当场红了**。
        //   ★ 又一次「锚点指到了第一处同名的东西」而不是那一处。
        //   ⇒ 钉的是「**被 wait 的那个东西是从锁里 take 出来的**」：`reaped` 之后紧跟 `.wait()`。
        // ⚠ 窗口起点从持锁块**结束处**算，不是从 `take_at` 算 ——
        // B4 让那个块变长了，固定 260 字符的窗口当场不够（判据自己红了一次）。
        let after_take = &sec[held_end..(held_end + 260).min(sec.len())];
        assert!(
            after_take.contains("reaped") && after_take.contains(".wait()"),
            "`take()` 之后没紧跟着对取出来的那个 `Child` 调 `wait()` —— 实得这一段：{after_take:?}"
        );
        // ★ 反向：`wait()` 那一行不许再出现在 `lock()` 的链式调用里。
        for l in sec.lines() {
            let t = l.trim_start();
            if t.starts_with("//") {
                continue;
            }
            assert!(
                !(l.contains(".lock()") && l.contains(".wait()")),
                "这一行同时有 `.lock()` 与 `.wait()` ⇒ 又变成持锁等了：{l}"
            );
        }
    }

    /// ★★ ③`spawn` 与「登记进锁」之间那个窗口必须关上。
    ///
    /// 原来 `stopping` 只在循环顶部与 EOF 之后检查 ⇒ 刚过顶部检查就 `spawn` 时，
    /// 一个并发的 `stop()` 会看到锁里还是 `None`、**一个字节的 kill 都没发**，
    /// 而线程接着把子进程存进锁并进 `io::copy` 永久阻塞
    /// ⇒ **monitor 退了、daemon 还在跑且没人能 kill 它** —— `stop()` 头注说的「游魂进程」。
    #[test]
    fn the_window_between_spawn_and_registration_is_closed() {
        let sec = wait_section();
        let reg_at = sec
            .find("*g = Some(spawned);")
            .expect("找不到「把子进程存进锁」那一行");
        let after = &sec[reg_at..];
        // 存完之后、发 `Started` 之前，必须再读一次 `stopping`。
        let started_at = after
            .find("SuperviseEvent::Started")
            .expect("找不到 `Started` 事件");
        let window = &after[..started_at];
        assert!(
            window.contains("stopping.load("),
            "登记子进程之后没有复查 `stopping` —— 那个窗口还开着：\n\
             `stop()` 落在里面就是一个**没人能 kill 的游魂 daemon**。\n\
             实得这一段：{window:?}"
        );
        assert!(
            window.contains("kill()"),
            "复查到 `stopping` 之后没有就地 kill —— 只 return 的话子进程留下来了"
        );
    }

    /// ★ **反向断言（「让它发生」那一半）**：改成 `io::copy` 之后，
    /// 「子进程写了远超任何缓冲区的量再退出」这条路**仍然**能被检测到死亡。
    ///
    /// ⚠ 只断言「没攒内存」是不够的 —— 那与「机制根本没跑」区分不开（F14 的 e2e 差点空绿）。
    /// 本条让它**真的发生一次**：8 MiB stdout + 正常退出 ⇒ `Exited` 必须来。
    #[test]
    fn a_child_that_floods_stdout_and_exits_is_still_detected_as_dead() {
        let (tx, rx) = std::sync::mpsc::channel::<SuperviseEvent>();
        let h = supervise(
            PathBuf::from("sh"),
            vec![
                "-c".into(),
                // 8 MiB 到 stdout，然后正常退出。
                "dd if=/dev/zero bs=1024 count=8192 2>/dev/null; exit 3".into(),
            ],
            vec![],
            CrashLimits {
                max_crashes: 1,
                window_ms: 60_000,
            },
            Arc::new(|| 0),
            Arc::new(move |e| {
                let _ = tx.send(e);
            }),
        );
        let mut saw_exit = None;
        for _ in 0..6 {
            match rx.recv_timeout(std::time::Duration::from_secs(20)) {
                Ok(SuperviseEvent::Exited { code, .. }) => {
                    saw_exit = Some(code);
                    break;
                }
                Ok(_) => continue,
                Err(e) => panic!("20s 内没等到 `Exited` —— EOF 语义被改坏了：{e}"),
            }
        }
        h.stop();
        assert_eq!(
            saw_exit,
            Some(Some(3)),
            "写了 8 MiB 之后退出的子进程没被正确收尸（或退出码丢了）"
        );
    }

    /// ★★ **反向断言**：子进程**关掉 stdout 但继续活着**时，`stop()` 必须**及时返回**。
    ///
    /// 这条是 ② 那个死锁链的行为面：持锁 `wait()` 会让这里永久卡住。
    /// ⚠ 测试里用 `recv_timeout`/带上限的等待是允许的 —— C12 的「零定时器」管的是
    /// **backend 生产代码**里不许有自己醒过来的构件，不是测试的等待上限。
    #[test]
    fn stop_returns_promptly_even_if_the_child_closed_stdout_but_lives_on() {
        let (tx, rx) = std::sync::mpsc::channel::<SuperviseEvent>();
        let h = supervise(
            PathBuf::from("sh"),
            // 关掉 stdout（制造 EOF）但继续活着 —— 那正是已登记的那个诚实边界。
            vec!["-c".into(), "exec 1>&-; sleep 30".into()],
            vec![],
            CrashLimits {
                max_crashes: 1,
                window_ms: 60_000,
            },
            Arc::new(|| 0),
            Arc::new(move |e| {
                let _ = tx.send(e);
            }),
        );
        // 等它真的起来（否则我们可能在 spawn 之前就 stop，测不到那条链）。
        match rx.recv_timeout(std::time::Duration::from_secs(20)) {
            Ok(SuperviseEvent::Started { .. }) => {}
            other => panic!("没等到 `Started`：{other:?}"),
        }
        // ★ `stop()` 在另一个线程上跑，主线程带上限地等它回来。
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let hh = std::sync::Arc::new(h);
        let h2 = hh.clone();
        std::thread::spawn(move || {
            h2.stop();
            let _ = done_tx.send(());
        });
        done_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect(
                "`stop()` 10s 没回来 —— 监护线程正持着 `child` 锁等一个还活着的子进程。\n\
                 生产上它跑在**主线程**（`RunEvent::Exit`）⇒ 窗口关了、进程退不出去，只能 kill -9。",
            );
    }
}
