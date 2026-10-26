use super::*;
use std::sync::mpsc;
use std::time::Duration;

/// 起一个 `sleep` 替身（不是 claude，不碰终端）。
fn stand_in() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("起 sleep 替身")
}

/// 判死 ③：看守挂上之后进程一退，`on_dead` 当场就来（内核叫醒，不是隔一阵看一眼）。
#[test]
fn the_watch_fires_when_the_process_exits() {
    let mut child = stand_in();
    let (tx, rx) = mpsc::channel();
    watch_pid_until_exit(child.id(), || true, move || tx.send(()).unwrap());
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "进程还活着，看守就报了死"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    rx.recv_timeout(Duration::from_secs(1))
        .expect("进程退了 1 秒，看守还没报");
}

/// 判死 ①：开不出来（进程已不在）⇒ 立刻报死，不静默丢。
#[test]
fn a_process_already_gone_is_reported_dead_at_once() {
    let mut child = stand_in();
    let pid = child.id();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(pidfd_open(pid).is_err(), "已回收的 pid 不该开得开");
    let (tx, rx) = mpsc::channel();
    watch_pid_until_exit(pid, || true, move || tx.send(()).unwrap());
    rx.try_recv().expect("开不出来的那一刻就该报死");
}

/// 判死 ②：开到了，调用方说「不是那个进程了」（PID 复用）⇒ 当场报死。
#[test]
fn an_impostor_is_reported_dead_at_once() {
    let (tx, rx) = mpsc::channel();
    watch_pid_until_exit(std::process::id(), || false, move || tx.send(()).unwrap());
    rx.try_recv().expect("冒名者那一刻就该报死");
}

/// **「`poll` 真错误不判死」这条语义的结构性钉子。**
///
/// 那条路径要求 `poll(2)` 返回负值**且不是 `EINTR`** —— 在测试里可靠地制造它需要
/// 伪造一个坏 fd 或注入 syscall 失败，成本远超收益。而它恰恰是**最容易丢的一条**：
/// 另外三条都「发死亡事件」，只有它不发；重写的人很自然会把四条统一成「都发」，
/// 而后果是**一次系统调用失败就把活着的进程判死**。
///
/// ⇒ 退而求其次：钉住**代码形状** —— EINTR 之后那段里不许出现 `on_dead`。
#[test]
fn poll_hard_error_must_not_report_dead() {
    let src = include_str!("../../../src/common/pidwatch-core/src/linux.rs");
    let prod = guard_core::production_code(src);
    let i = prod
        .find("ErrorKind::Interrupted")
        .expect("找不到 EINTR 分支 —— 本钉子的锚点没了，先确认 poll 循环还在");
    // 从 EINTR 判断到函数收尾这一段 = 「真错误」的处理段。
    let tail = &prod[i..];
    let end = tail.find("\n    });").unwrap_or(tail.len());
    let hard_error_arm = &tail[..end];
    assert!(
        !hard_error_arm.contains("on_dead"),
        "`poll` 真错误的处理段里出现了 `on_dead` —— 那条路径**必须不判死**。\n实际那一段：\n{hard_error_arm}"
    );
    assert!(
        hard_error_arm.contains("放弃看守"),
        "「放弃看守（不报死）」那句 warn 不见了 —— 它是这条路径唯一的可观测痕迹"
    );
}
