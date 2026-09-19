use super::*;

#[test]
fn a_real_session_id_is_accepted() {
    assert!(sid_is_safe("9d66c46d-bf88-4f99-877e-455555555555"));
    assert!(sid_is_safe("a_b-1"));
}

/// fail closed：能破坏 tmux 格式串 / 命令语义的都不许过。
///
/// ⚠ **`-t` 这种「像旗标」的值刻意不在这张表里** —— 它由 `[A-Za-z0-9_-]` 放行，
/// 而那是对的：`set-option -t <handle> @ccm_sid <值>` 里的值在非选项参数之后，
/// tmux 不会把它再当选项解析；且我们 argv 直传、不过 shell。
/// 写在这里是因为「看起来危险就该禁」是个很容易顺手加进来的错判 —— 真 sid（UUID）
/// 本来就带 `-`，收窄到禁 `-` 会把正常会话全挡掉。
#[test]
fn a_sid_that_could_break_the_format_string_is_rejected() {
    for bad in ["", "a b", "#{session_name}", "a;b", "$(id)", "a\nb", "a'b"] {
        assert!(!sid_is_safe(bad), "{bad:?} 不该被放行");
    }
    assert!(!sid_is_safe(&"x".repeat(129)), "超长 sid 不该被放行");
}

/// ★ 空 pane 目标必须挡住 —— 实测 `-t ''` 会静默解析成「某个会话」。
#[test]
fn an_empty_pane_target_is_rejected() {
    assert!(
        !pane_is_safe(""),
        "空目标放行 ⇒ sid 会被打到一个碰巧的会话上"
    );
    assert!(!pane_is_safe("%"), "只有 % 没有数字也不是 pane id");
}

#[test]
fn only_percent_digits_is_a_pane_id() {
    assert!(pane_is_safe("%0"));
    assert!(pane_is_safe("%12"));
    for bad in ["0", "$0", "@0", "%a", "%1x", " %1", "%1 "] {
        assert!(!pane_is_safe(bad), "{bad:?} 不该被当成 pane id");
    }
}

/// 一个不存在的 pid 拿不到 `TMUX_PANE` ⇒ 走「不在 tmux 里」，**不会**去猜一个会话。
///
/// ⚠ 本条**不起 tmux**：`pane_of` 在 `gate::probe` 之前返回 `None`。
#[test]
fn a_pid_without_tmux_pane_never_reaches_tmux() {
    // PID 0 在 Linux 上不是一个可读的 `/proc` 目录 ⇒ 读不到环境。
    assert_eq!(
        tag(0, "9d66c46d-bf88-4f99-877e-455555555555"),
        Outcome::NotInTmux
    );
}

/// sid 不合法时**连环境都不读**（顺序也是判据的一部分：先 fail closed 再做 IO）。
#[test]
fn a_bad_sid_short_circuits_before_any_io() {
    assert_eq!(tag(std::process::id(), "bad sid"), Outcome::RejectedSid);
}
