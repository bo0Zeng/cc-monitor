//! 〔HX1 · 4D〕机器页「停」：**先 SIGTERM、等一段、还在才强杀** —— 判据。
//!
//! 守的要求（住址）：主会话 4D 裁 D-a（`4d-lanes.md`「主会话本批裁的」）逐字「机器页『停』改 SIGTERM → 等 → 超时才 SIGKILL」；
//! 审计 E §E2「在机器页点『停』本机后端（SIGKILL）…… 不等正在跑的阻塞写做完」。设计与读数住 `调研/第四波记录/HX1.md` §3。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | S1 | 真子进程两形：收到 SIGTERM 就退的 ⇒ `Stopped`；忽略 SIGTERM 的 ⇒ 等满之后 `Forced`，进程表里没有它 | 真进程 ＋ 真信号 |
//! | S2 | 接管那一形「还在不在」的口径：`/proc/<pid>/stat` 状态字 `Z` 算没了（进程名里带括号 / 空格也认得对） | 纯函数逐格 ＋ 一个真僵尸 |
//! | S3 | 接线：`local_backend_host.rs` 生产段里强杀只出现在交给 `stop_gracefully` 的那两个闭包里；`stop_detached_locked` 两支都经它 | 文本，零富余 |

use super::*;

/// 起一个 `sh`，`trap` 决定它怎么对 SIGTERM。回 `(子进程, 它的 pid)`。
fn sh(script: &str) -> std::process::Child {
    let mut c = std::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起 sh");
    // 等它把 trap 装好（它装完才往 stdout 写一行 —— 事件，不是睡）。
    use std::io::BufRead as _;
    let mut line = String::new();
    std::io::BufReader::new(c.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("读就绪行");
    assert_eq!(line.trim(), "ready");
    c
}

fn term(pid: u32) -> Result<(), String> {
    let st = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    st.success().then_some(()).ok_or_else(|| format!("{st}"))
}

#[test]
#[cfg(unix)]
fn s1_a_polite_child_stops_and_a_deaf_one_is_forced_and_gone() {
    // 听话的：收到 SIGTERM 就退。
    let c = std::cell::RefCell::new(sh(
        "trap 'exit 0' TERM; echo ready; while :; do sleep 0.05; done",
    ));
    let pid = c.borrow().id();
    let end = stop_gracefully(
        || term(pid),
        || matches!(c.borrow_mut().try_wait(), Ok(Some(_))),
        || c.borrow_mut().kill().map_err(|e| e.to_string()),
        50,
        std::time::Duration::from_millis(20),
    );
    assert_eq!(end, StopEnd::Stopped);
    let st = c
        .borrow_mut()
        .try_wait()
        .expect("try_wait")
        .expect("已收尸");
    assert_eq!(st.code(), Some(0), "它自己给的退出码");

    // 聋的：SIGTERM 被忽略 ⇒ 等满、强杀、它真没了（已收尸，不是 Z）。
    let c = std::cell::RefCell::new(sh("trap '' TERM; echo ready; while :; do sleep 0.05; done"));
    let pid = c.borrow().id();
    let end = stop_gracefully(
        || term(pid),
        || matches!(c.borrow_mut().try_wait(), Ok(Some(_))),
        || c.borrow_mut().kill().map_err(|e| e.to_string()),
        10,
        std::time::Duration::from_millis(20),
    );
    assert_eq!(end, StopEnd::Forced);
    use std::os::unix::process::ExitStatusExt as _;
    let st = c
        .borrow_mut()
        .try_wait()
        .expect("try_wait")
        .expect("已收尸");
    assert_eq!(st.signal(), Some(9), "应当是被强杀的：{st:?}");
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "进程表里还有 {pid}"
    );

    // 发不出 SIGTERM ⇒ 直接强杀；强杀也发不出 ⇒ Stuck(Some)。
    let end = stop_gracefully(
        || Err("x".into()),
        || false,
        || Err("没有权限".into()),
        3,
        std::time::Duration::from_millis(1),
    );
    assert_eq!(end, StopEnd::Stuck(Some("没有权限".into())));
    // 强杀发出去了但它还在 ⇒ Stuck(None)。
    let end = stop_gracefully(
        || Ok(()),
        || false,
        || Ok(()),
        3,
        std::time::Duration::from_millis(1),
    );
    assert_eq!(end, StopEnd::Stuck(None));
}

#[test]
#[cfg(target_os = "linux")]
fn s2_a_zombie_counts_as_gone_for_the_adopted_path() {
    use crate::local_backend_host::{adopted_gone, proc_stat_says_zombie};
    for (stat, z) in [
        ("123 (cc-monitor-backend) S 1 2 3", false),
        ("123 (cc-monitor-backend) Z 1 2 3", true),
        ("123 (a b) c) Z 1", true),
        ("123 (Z) S 1", false),
        ("garbage", false),
    ] {
        assert_eq!(proc_stat_says_zombie(stat), z, "{stat:?}");
    }
    // 真僵尸：子进程退了、我们还没收尸 ⇒ 状态字 Z ⇒ 算没了；收完尸 ⇒ /proc 没了 ⇒ 也算没了；活着 ⇒ 不算。
    let mut c = sh("echo ready; exec sleep 30");
    let pid = c.id();
    assert!(!adopted_gone(pid), "活着的被当成没了");
    c.kill().expect("kill");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .is_ok_and(|s| proc_stat_says_zombie(&s))
    {
        assert!(std::time::Instant::now() < until, "等不到它变成僵尸");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(adopted_gone(pid), "僵尸没被当成没了");
    let _ = c.wait();
    assert!(adopted_gone(pid));
}

#[test]
fn s3_every_force_kill_goes_through_the_graceful_wait() {
    let prod =
        guard_core::production_code(include_str!("../../src/bridge/src/local_backend_host.rs"));
    // 强杀的两种写法：`Child::kill`（`.kill()`）与按 pid 发 `-KILL`（`signal_kill(`）。
    let kills: Vec<usize> = prod.match_indices(".kill()").map(|(i, _)| i).collect();
    let pid_kills: Vec<usize> = prod
        .match_indices("signal_kill(pid)")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(kills.len(), 1, "`.kill()` 不是恰好一处：{kills:?}");
    assert_eq!(
        pid_kills.len(),
        1,
        "`signal_kill(pid)` 调用不是恰好一处：{pid_kills:?}"
    );
    // 那两处都在 `stop_detached_locked` 里，且都在一次 `stop_gracefully(` 调用之内（它之后、下一个 `STOP_POLL` 之前）。
    assert_eq!(prod.matches("fn stop_detached_locked(").count(), 1);
    let seg = &prod[prod
        .find("fn stop_detached_locked(")
        .expect("stop_detached_locked")..];
    let seg_end = seg.find("\nfn ").expect("下一个函数");
    let seg = &seg[..seg_end];
    let calls: Vec<usize> = seg
        .match_indices("stop_gracefully(")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(calls.len(), 2, "两支各经一次 stop_gracefully：{calls:?}");
    for needle in [".kill()", "signal_kill(pid)"] {
        assert_eq!(
            seg.matches(needle).count(),
            1,
            "`{needle}` 不在 stop_detached_locked 里"
        );
        let at = seg.find(needle).expect("在");
        let inside = calls
            .iter()
            .any(|c| *c < at && seg[*c..at].matches("STOP_POLL").count() == 0);
        assert!(inside, "`{needle}` 不在交给 stop_gracefully 的那几个闭包里");
    }
    // 顺序：自己起的那一支交进去的**第一个**闭包（「请它收尾」）是 SIGTERM；接管那一支先 `kill_adopted`（核身份 ＋ SIGTERM）
    // 再进 `stop_gracefully`，交进去的第一个闭包是空动作（请求已经发过了）。⇒ 把强杀挪到第一格（「停」＝直接 SIGKILL 那一形）会红。
    let first_arg = |c: usize| {
        let rest = seg[c + "stop_gracefully(".len()..].trim_start();
        rest[..rest.find(",\n").expect("第一个实参的尾")].to_string()
    };
    assert_eq!(
        first_arg(calls[0]),
        "|| signal_term(pid)",
        "自己起的那一支第一步不是 SIGTERM"
    );
    assert_eq!(
        first_arg(calls[1]),
        "|| Ok(())",
        "接管那一支交进去的第一步变了"
    );
    let adopted_term = seg
        .find("kill_adopted(pid, &bin)")
        .expect("接管那一支先核身份发 SIGTERM");
    assert!(
        calls[0] < adopted_term && adopted_term < calls[1],
        "kill_adopted 不在接管那一支的等之前"
    );
    // 正控：数法认得出一处裸 `.kill()`。
    assert_eq!("let _ = c.kill();".matches(".kill()").count(), 1);
}
