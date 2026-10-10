use super::*;

/// 停进程那几步没做成：句子只带原因词，系统原话另带（不上句子）。
#[test]
fn a_failed_stop_step_says_a_reason_word_and_keeps_the_os_words_aside() {
    let s = said_with(
        std::io::ErrorKind::PermissionDenied,
        "Operation not permitted (os error 1)",
        |why| copy_core::copy_text("beStop.signal.failed", &[("sig", "9"), ("why", why)]),
    );
    assert_eq!(
        s.said,
        copy_core::copy_text(
            "beStop.signal.failed",
            &[
                ("sig", "9"),
                (
                    "why",
                    &copy_core::io_reason(std::io::ErrorKind::PermissionDenied)
                )
            ]
        )
    );
    assert!(!s.said.contains("os error"), "{}", s.said);
    assert_eq!(
        s.raw.as_deref(),
        Some("Operation not permitted (os error 1)")
    );
}
