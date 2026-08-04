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

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// 本机 sidecar 的基名。Tauri 的 `externalBin` 会把它按 `<基名>-<target-triple>[.exe]`
/// 放到 app 可执行文件旁边。
pub const SIDECAR_STEM: &str = "cc-monitor-remote";

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
            let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
            on_event(SuperviseEvent::Started {
                pid: this_pid,
                attempt,
            });

            // ★ 等它死：读到 EOF。**这不是定时器，也不是轮询** —— 没有「隔多久看一眼」。
            if let Some(mut o) = out {
                let mut sink = Vec::new();
                let _ = o.read_to_end(&mut sink);
            }
            // EOF 之后收尸：此刻它已经死了（或正被 kill），`wait()` 不会久等。
            let code = child
                .lock()
                .ok()
                .and_then(|mut g| g.as_mut().and_then(|c| c.wait().ok()))
                .and_then(|s| s.code());
            if let Ok(mut g) = child.lock() {
                *g = None;
            }
            pid.store(0, Ordering::SeqCst);
            on_event(SuperviseEvent::Exited { code, attempt });

            if stopping.load(Ordering::SeqCst) {
                return;
            }
            let t = now_ms();
            let decision = {
                let mut g = crashes.lock().expect("crash 账被 poison");
                g.push(t);
                decide(&g, t, limits)
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
}
