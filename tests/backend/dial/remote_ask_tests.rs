//! `remote_ask` 那一跳没成时说哪一句（`settle_pulled`）。
//!
//! ssh 自己出错（连不上 · 中途断开）退 255；那台一个字不回 ⇒ 是这一跳的连接断了，句子落「连接失败」那一句，
//! 不落「后端报错 · 原因不明」。别的非 0 退出、没有一个字 ⇒ 照旧「原因不明」（那台的命令自己失败了，没说为什么）。

use super::*;

/// 一趟收全的替身结果（拨号核心 `uses::capture` 那一形）。
fn captured(stdout: &str, stderr: &str, exit_status: u32) -> crate::dial::Captured {
    crate::dial::Captured {
        stdout: stdout.into(),
        stderr: stderr.into(),
        exit_status: Some(exit_status),
    }
}

async fn said_of(got: crate::dial::Captured) -> Said {
    pull_over_coded(async move { Ok(got) }).await.unwrap_err()
}

/// ★ 那一跳 ssh 退 255、一个字不回 ⇒ 连不上那一句（带「中途断开」），不是「原因不明」。
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
    assert_ne!(e.said, copy_text("beRemoteAsk.run.failedNoReason", &[]), "{e:?}");
}

/// 边界另一向：别的退出码、一个字不回 ⇒ 照旧「原因不明」；255 但那台写了 stderr ⇒ 照旧「后端报错」（不吞那台的话）。
#[tokio::test]
async fn other_silent_failures_keep_their_sentences() {
    let e = said_of(captured("", "", 1)).await;
    assert_eq!(e.said, copy_text("beRemoteAsk.run.failedNoReason", &[]), "{e:?}");
    let e = said_of(captured("", "bash: boom", 255)).await;
    assert_eq!(e.said, copy_text("beRemoteAsk.run.failed", &[]), "{e:?}");
}
