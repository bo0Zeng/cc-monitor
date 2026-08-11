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
//! `C10` 要求前者不认识后者，所以两边不能合并。

use crate::backend::control::local_backend::{self, Resolved, SuperviseHandle};

/// F05a：本机后端监护句柄。存起来是为了退出前按策略 `stop()`（`C8`）。
///
/// ⚠ **P2s 把它从 `OnceLock` 换成了 `Mutex<Option<_>>`，理由是结构性的**：
/// `SuperviseHandle::stop()` 把 `stopping` 永久置位、并杀掉当前子进程 ⇒ **那个句柄之后就是死的**，
/// 再起必须换一个新的。`OnceLock` 写一次就锁死 ⇒ 有「停」就不可能有「再起」，
/// 而 `C8`② 要的正是「起 / 停 / 状态」三件。
pub static LOCAL_BACKEND: std::sync::Mutex<Option<SuperviseHandle>> =
    std::sync::Mutex::new(None);

/// P2s（`C8`②）：**起本机后端并把句柄存进 `LOCAL_BACKEND`**。
///
/// 从 `run()` 里抽出来，因为「再起」要走同一条路 —— 抽出来之前，
/// 那段宿主知识（落点目录 / 当前 arch / 平台注入）只在 `run()` 的一个块里存在，
/// 「停了还能起回来」就无从谈起。
///
/// **已经在跑就不重复起**：`C8`① 是「每台机各一个」。
pub fn start_local_backend() -> Resolved {
    {
        let g = LOCAL_BACKEND.lock().expect("LOCAL_BACKEND 锁毒化");
        if let Some(h) = g.as_ref() {
            if h.current_pid().is_some() {
                return Resolved::Missing {
                    reason: "本机后端已经在跑（C8①：每台机只许一个）".into(),
                    looked_at: Vec::new(),
                };
            }
        }
    }
    // 两样宿主知识在这里给（backend 层不认识它们）：
    //   · 落点 `~/.cc-monitor/bin`：与远端自部署同一个目录，但**文件名带 build_id**
    //     ⇒ 与远端那份结构上不可能撞（理由见 `extract_embedded_to` 头注的 D1 段）。
    //   · 当前 arch：`crate::sftp::daemon_binary` 按它挑内嵌字节；缺内嵌（`cfg(embedded_daemons)`
    //     未置）时给 None，函数会诚实降级、不伪造理由。
    let extract_dir = dirs::home_dir()
        .map(|h| h.join(".cc-monitor").join("bin"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/.cc-monitor/bin"));
    let embedded = crate::sftp::daemon_binary(std::env::consts::ARCH).map(|d| (d.build_id, d.bytes));
    let (resolved, sup) = local_backend::start_or_extract(
        env!("CCM_TARGET_TRIPLE"),
        &extract_dir,
        embedded,
        // C10：平台知识由宿主注入，backend 那半不认识 `#[cfg(unix)]`。
        &crate::platform_fs::make_executable,
        std::sync::Arc::new(|e| tracing::info!("本机后端: {e:?}")),
    );
    if let Some(h) = sup {
        *LOCAL_BACKEND.lock().expect("LOCAL_BACKEND 锁毒化") = Some(h);
    }
    resolved
}

/// P2s（`C8`②）：**这台机的 daemon 现在什么状态**。本机与远端**同一个口**。
///
/// # 为什么「通道在不在」是两侧共用的那个真相
///
/// `inbound_client` 的登记表按 origin 存活着的通道 —— 远端在 hello 之后登记，
/// 本机（P2 之后）也在 hello 之后登记，**同一张表、同一个时机**。
/// ⇒ 问「这台机的 daemon 在不在」不需要两套实现，那正是 `C1`。
///
/// `pid` / `attempts` 只有本机有（远端的进程在别人机器上，我们手里只有一条流）——
/// 这**不是欠账，是天然不对称**，所以它们是 `Option`，不是「远端那边填 0」。
#[tauri::command]
pub fn daemon_status(origin: String) -> Result<serde_json::Value, String> {
    if origin.trim().is_empty() {
        return Err("origin 不许为空 —— 状态是 per-host 的，没有「全局」这一档".into());
    }
    let channel = crate::inbound_client::client_for(&origin).is_some();
    let (pid, attempts) = if origin == crate::inbound_client::LOCAL_ORIGIN {
        let g = LOCAL_BACKEND.lock().map_err(|e| format!("锁毒化: {e}"))?;
        match g.as_ref() {
            Some(h) => (h.current_pid(), Some(h.attempts())),
            None => (None, None),
        }
    } else {
        (None, None)
    };
    Ok(serde_json::json!({
        "origin": origin,
        "channel": channel,
        "pid": pid,
        "attempts": attempts,
        "killOnExit": crate::daemon_policy::kill_on_exit(&origin),
    }))
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
        let pid1 = daemon_status(crate::inbound_client::LOCAL_ORIGIN.into())
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
        assert!(wait_channel(true), "停了之后起不回来 —— 那就只有「停」没有「起」");
        let pid2 = daemon_status(crate::inbound_client::LOCAL_ORIGIN.into())
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

    /// P2s（`C8`①）：**已经在跑就不重复起**。
    ///
    /// 不起真进程 —— 只验「有一个活着的句柄时，`start_local_backend` 直接返回、不 spawn」。
    /// ⚠ 因此它**只覆盖幂等这一条**，不覆盖起进程那条路（那条由上面的实测管）。
    #[test]
    fn starting_twice_does_not_spawn_a_second_local_daemon() {
        let src = include_str!("local_daemon.rs");
        let prod = guard_core::production_code(src);
        let at = // 锚点取**带引号的完整字面量** —— 只写中文那段的话，左边紧挨着汉字，
        // `find_pinned` 判它「被撑大」（两侧要有边界）。引号就是边界。
        guard_core::find_pinned(&prod, "\"本机后端已经在跑（C8①：每台机只许一个）\"").unwrap_or_else(
            |e| {
                panic!(
                    "`start_local_backend` 里没有「已经在跑就直接返回」那一支（{e}）。\n\
                     没有它，开关每按一次「起」就多一个 daemon —— 与 `C8`① 直接冲突。"
                )
            },
        );
        let head = guard_core::find_pinned(&prod, "pub fn start_local_backend(").expect("入口在");
        assert!(
            head < at,
            "那一支跑到 `start_local_backend` 之前去了 —— 抽取面画错了，本条在空转"
        );
    }
}
