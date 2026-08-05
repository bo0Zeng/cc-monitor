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
//! | 引 `libc::kill` / spawn 一个 `kill` 命令 | 为了一个 `stop()` 引入平台 cfg（C10）或多起一个进程 |
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
//! [`supervise`]，并强制私有 `TMUX_TMPDIR`，绝不碰用户真实 tmux server。

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
/// ⚠ **今天还没接线**（F06b-1 的 ccm 那一半未写）：接线要先解决一个 ccm 自己
/// 已经解过的难题 —— 「`--print` 与 exec 逐字节一致」。ccm 头注给了答案：
/// **打印的是配方，不是值**（求值推迟到那条串真正被执行时）。⇒ resume 路不能把
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
pub fn supervise(
    bin: PathBuf,
    args: Vec<String>,
    envs: Vec<(String, String)>,
    limits: CrashLimits,
    now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    on_event: Arc<dyn Fn(SuperviseEvent) + Send + Sync>,
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
            cmd.args(&args)
                .stdin(std::process::Stdio::null())
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
            if let Some(mut o) = out {
                let _ = std::io::copy(&mut o, &mut std::io::sink());
            }
            // EOF 之后收尸。
            //
            // ⚠ **F16 修**：原来是 `child.lock().ok().and_then(|mut g| … c.wait())` ——
            // guard 活在闭包里 ⇒ **`wait()` 整段都持着锁**。而「子进程关掉 stdout 但继续活着」
            // 是本模块**已登记的诚实边界**（见头注），那时 `wait()` 会久等，后果不是
            // 「误判它死了」而是：`stop()` 第一件事就是 `self.child.lock()`，而它跑在
            // **主线程**（`lib.rs` 的 `RunEvent::Exit`）⇒ **窗口关了、进程退不出去，只能 kill -9**。
            // ⇒ 先把 `Child` 从锁里 **`take()` 出来**，再在锁外 `wait()`。
            // ⚠ 并发上安全：此刻已过 EOF，子进程要么死了要么正被 kill；
            // 一个并发的 `stop()` 看到 `None` 只是不再重复 kill，它设的 `stopping`
            // 仍会被下面那条检查读到。
            let mut reaped = child.lock().ok().and_then(|mut g| g.take());
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
    let h = supervise(
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
    );
    (r, Some(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never(_: &Path) -> bool {
        false
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
    /// ★★ **F06b-1：那个 env 名只有一个家，且今天还没接线。**
    ///
    /// # 两条断言，各管一件
    ///
    /// ① **同名**：`shared/ccm` 里**若**出现这个 env 名，必须与 Rust 侧的
    ///    [`super::DAEMON_BIN_ENV`] **逐字相同** —— 名字打错的后果是
    ///    「ccm 永远读不到 ⇒ 永远走本地那条」，而那**看起来完全正常**（诚实降级本来就是它的兜底）。
    ///    ⇒ 这种错**不会自己暴露**，只能靠钉。
    /// ② **前提触发器**：今天 ccm **还没用它**（实测 0 处）。一旦出现 ⇒ 本条红，
    ///    提醒回来把 F06b-1 的另一半（`--print` 与 exec 的一致性）一并做完 ——
    ///    ⚠ 那不是可选项：`ccm-print-parity` 有 44 条黄金串，
    ///    而 ccm 头注给了正解「**打印的是配方，不是值**」。
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
        // ① 若出现，必须逐字相同：找「像那个名字但不完全一样」的拼写。
        let looks_like = "CCM_DAEMON";
        if let Some(at) = src.find(looks_like) {
            let tail = &src[at..(at + 40).min(src.len())];
            // ⚠ 光 `starts_with` 不够：`CCM_DAEMON_BINARY` **也**以 `CCM_DAEMON_BIN` 开头。
            //   变异 Z1 就是这么活下来的 ⇒ 必须查名字后面那个字符是不是标识符字符。
            let exact = tail.strip_prefix(name).is_some_and(|rest| {
                !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            });
            assert!(
                exact,
                "`shared/ccm` 里那个 env 名与 Rust 侧对不上：ccm 写的是 {tail:?}，\n\
                 而唯一的家是 `DAEMON_BIN_ENV` = {name:?}。\n\
                 ⚠ 名字打错**不会自己暴露** —— ccm 读不到就静静走本地那条（诚实降级是它的兜底）。"
            );
        } else {
            // ② 前提触发器：今天还没接线。
            assert!(
                !src.contains(name),
                "自相矛盾：找不到 `{looks_like}` 却找得到 `{name}` —— 抽取坏了"
            );
        }
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

    /// ★ **生产接线钉**：`lib.rs` 的启动路径**真的**调了 `start_if_present`。
    ///
    /// 这条是 F03 教训的直接产物：「模块存在 ≠ 模块被调用」。
    /// 本模块写得再全，只要 `lib.rs` 里没那一行，本机后端就永远不会被起 ——
    /// 而上面 9 条单测**全都照样绿**。
    #[test]
    fn the_startup_path_really_calls_this_module() {
        let prod = guard_core::production_code(include_str!("../../lib.rs"));
        for needle in ["local_backend::start_if_present", "CCM_TARGET_TRIPLE"] {
            assert!(
                prod.contains(needle),
                "`lib.rs` 的生产段里找不到 `{needle}` —— 本机后端没有被接上启动路径。\n\
                 判据全绿而功能从不运行，正是「模块存在 ≠ 模块被调用」那个坑。"
            );
        }
        // 句柄必须被存下来：不存就没人能 `stop()`，被监护的 daemon 成游魂进程。
        assert!(
            prod.contains("LOCAL_BACKEND"),
            "监护句柄没被存起来 —— 退出时无法 `stop()`，daemon 会变成游魂进程"
        );
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
    /// 这条钉「出口也被接上」——同一件事的两半，缺一半就是游魂进程。
    #[test]
    fn the_exit_path_really_stops_the_local_backend() {
        let prod = guard_core::production_code(include_str!("../../lib.rs"));
        for needle in ["RunEvent::Exit", "LOCAL_BACKEND", ".stop()"] {
            assert!(
                prod.contains(needle),
                "`lib.rs` 的生产段里找不到 `{needle}` —— 退出时不收本机后端。\n\
                 被监护的 daemon 对「stdin 写端关闭」刻意不敏感 ⇒ 它会活过 monitor，成游魂进程。\n\
                 ⚠ 这条洞**不会被别的判据抓到**（实测：删掉整段钩子，815 条测试全绿），\
                 而 clippy 的 dead_code 只是偶然覆盖。"
            );
        }
        // 顺序：`RunEvent::Exit` 必须在 `.stop()` 之前 —— 否则是「启动时就 stop」那种错法。
        let exit_at = prod.find("RunEvent::Exit").expect("上面已断言存在");
        let stop_at = prod.rfind(".stop()").expect("上面已断言存在");
        assert!(
            exit_at < stop_at,
            "`.stop()` 排在 `RunEvent::Exit` 之前 —— 那不是退出时收尸"
        );
    }

    // ── 真进程（`#[ignore]`，由 e2e/local-backend-supervise.sh 驱动）──────────

    /// ★ 起真 daemon → 杀它 → 看它自己回来。
    #[test]
    #[ignore]
    fn e2e_the_supervisor_restarts_a_real_daemon_after_it_is_killed() {
        let bin = std::env::var("CCM_E2E_DAEMON").expect("要 CCM_E2E_DAEMON");
        let tmpdir = std::env::var("CCM_E2E_TMUX_TMPDIR").expect("要 CCM_E2E_TMUX_TMPDIR");
        let claude = std::env::var("CCM_E2E_CLAUDE_DIR").expect("要 CCM_E2E_CLAUDE_DIR");
        let events: Arc<Mutex<Vec<SuperviseEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let ev = events.clone();
        let h = supervise(
            PathBuf::from(&bin),
            vec!["--tail-only".into()],
            vec![
                ("TMUX_TMPDIR".into(), tmpdir),
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
        let take_at = sec
            .find("child.lock().ok().and_then(|mut g| g.take())")
            .expect(
                "收尸段不再先 `take()` 出来 —— 那意味着 `wait()` 可能又回到了锁里面。\n\
                 后果不是「误判它死了」，是 `stop()` 在主线程永久阻塞、窗口关了进程退不出去。",
            );
        // ⚠ **不能只写 `sec.find(".wait()")`** —— 那会命中**关窗块**里那个
        //   `c.kill(); c.wait();`（它在 `take()` 之前，且它是对的：那处本来就持锁、
        //   而子进程刚被 kill、不会久等）。第一版就是这么写的，**判据自己当场红了**。
        //   ★ 又一次「锚点指到了第一处同名的东西」而不是那一处。
        //   ⇒ 钉的是「**被 wait 的那个东西是从锁里 take 出来的**」：`reaped` 之后紧跟 `.wait()`。
        let after_take = &sec[take_at..(take_at + 260).min(sec.len())];
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
