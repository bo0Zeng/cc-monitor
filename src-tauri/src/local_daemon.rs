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
    Ok(match g.as_ref() {
        Some(h) => (h.current_pid(), Some(h.attempts())),
        None => (None, None),
    })
}

/// P2s（`C8`②）：停本机后端。**句柄取走**（`take`）而不是留着 ——
/// `stop()` 之后那个句柄就是死的（`stopping` 永久置位），留着只会让下一次「起」
/// 误以为还在跑。
pub fn stop_local_backend() -> Result<String, String> {
    let mut g = LOCAL_BACKEND.lock().map_err(|e| format!("锁毒化: {e}"))?;
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
    /// daemon 的 `resolve_claude_dir()` 是 `$CLAUDE_CONFIG_DIR` 优先、`$HOME/.claude` 兜底。
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
    #[cfg(all(embedded_daemons, target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn the_local_daemon_can_be_stopped_and_started_again() {
        use std::path::Path;
        use std::sync::Arc;
        use std::time::Duration;

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
}
