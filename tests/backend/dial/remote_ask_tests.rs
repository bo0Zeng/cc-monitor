//! `remote_ask` 那一跳没成时说哪一句（`settle_pulled`）。
//!
//! ssh 自己出错（没连上 · 中途断开）退 255；那台一个字不回 ⇒ 是这一跳的连接断了，句子落「连接失败」那一句，
//! 不落「后端报错 · 原因不明」。别的非 0 退出、没有一个字 ⇒ 照旧「原因不明」（那台的命令自己失败了，没说为什么）。
//! 拨号核心记下的收尾（[`CaptureEnd`]）：没等到 Close 链路就断了 ⇒ 同「中途断开」；进程被信号杀 ⇒ 照信号说。

use super::*;

/// 一趟收全的替身结果（拨号核心 `uses::capture` 那一形）：正常退出、带码。
fn captured(stdout: &str, stderr: &str, exit_status: u32) -> crate::dial::Captured {
    ended_as(stdout, stderr, CaptureEnd::Exited { code: exit_status })
}

/// 同上，收尾由判据给。
fn ended_as(stdout: &str, stderr: &str, ended: CaptureEnd) -> crate::dial::Captured {
    crate::dial::Captured {
        stdout: stdout.into(),
        stderr: stderr.into(),
        ended,
    }
}

async fn said_of(got: crate::dial::Captured) -> Said {
    pull_over_coded(async move { Ok(got) }).await.unwrap_err()
}

/// ★ 那一跳 ssh 退 255、一个字不回 ⇒「连接失败」那一句（带「中途断开」），不是「原因不明」。
#[tokio::test]
async fn an_ssh_exit_255_with_no_output_says_the_link_dropped() {
    let e = said_of(captured("", "", 255)).await;
    assert_eq!(
        e.said,
        copy_text(
            "beRemoteAsk.run.unreachable",
            &[("why", &copy_text("beRemoteAsk.run.hopDropped", &[]))]
        ),
        "{e:?}"
    );
    assert_ne!(e.said, copy_text("beRemoteAsk.run.failedNoReason", &[]));
    // 只有空白也算没回字。
    let e = said_of(captured("", " \n", 255)).await;
    assert_ne!(
        e.said,
        copy_text("beRemoteAsk.run.failedNoReason", &[]),
        "{e:?}"
    );
}

/// ★ 那一跳发完数据、没送退出码，链路没等到 Close 就断了 ⇒「连接失败」那一句（带「中途断开」），不是「原因不明」。
#[tokio::test]
async fn a_link_dropped_before_close_says_the_link_dropped() {
    let dropped = copy_text(
        "beRemoteAsk.run.unreachable",
        &[("why", &copy_text("beRemoteAsk.run.hopDropped", &[]))],
    );
    let e = said_of(ended_as("half a line", "", CaptureEnd::Dropped)).await;
    assert_eq!(e.said, dropped, "{e:?}");
    // 断之前那台写了半截 stderr 也一样：断的是链路，不是那台的命令。
    let e = said_of(ended_as("", "half", CaptureEnd::Dropped)).await;
    assert_eq!(e.said, dropped, "{e:?}");
}

/// ★ 那台的进程被信号杀掉（送了 exit-signal、没有退出码）⇒ 照信号说，不说链路断了、也不是「原因不明」。
#[tokio::test]
async fn a_process_killed_by_a_signal_says_the_signal() {
    let e = said_of(ended_as(
        "",
        "",
        CaptureEnd::Signaled {
            signal: "KILL".into(),
        },
    ))
    .await;
    assert_eq!(
        e.said,
        copy_text("beRemoteAsk.run.signaled", &[("sig", "KILL")]),
        "{e:?}"
    );
    assert!(e.said.contains("KILL"), "{e:?}");
}

/// 正常 Close、退出码与信号都没给、一个字不回 ⇒ 照旧「原因不明」。
#[tokio::test]
async fn a_bare_close_keeps_the_no_reason_sentence() {
    let e = said_of(ended_as("", "", CaptureEnd::ClosedBare)).await;
    assert_eq!(
        e.said,
        copy_text("beRemoteAsk.run.failedNoReason", &[]),
        "{e:?}"
    );
}

/// 边界另一向：别的退出码、一个字不回 ⇒ 照旧「原因不明」；255 但那台写了 stderr ⇒ 照旧「后端报错」（不吞那台的话）。
#[tokio::test]
async fn other_silent_failures_keep_their_sentences() {
    let e = said_of(captured("", "", 1)).await;
    assert_eq!(
        e.said,
        copy_text("beRemoteAsk.run.failedNoReason", &[]),
        "{e:?}"
    );
    let e = said_of(captured("", "bash: boom", 255)).await;
    assert_eq!(e.said, copy_text("beRemoteAsk.run.failed", &[]), "{e:?}");
}
