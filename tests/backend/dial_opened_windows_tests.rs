//! ↗ 的本机记录（`dial/opened_windows.rs`）：一份两行怎么读 · 前奏在两种方言里长什么样。标签的形状在 `WindowLabel` 那一处判（`observe/session_terminals_tests.rs`）。

use super::*;
use std::path::Path;

#[test]
fn a_record_is_the_label_then_the_machine() {
    let want = Some(Opened {
        label: WindowLabel {
            pid: 600,
            start: 20,
        },
        machine: "devbox".into(),
    });
    assert_eq!(parse("600-20\ndevbox\n"), want);
    assert_eq!(parse("600-20\r\ndevbox\r\n"), want, "CRLF 也认");
    assert_eq!(parse("600-20\n"), None, "缺机器名 ⇒ 当没有");
    assert_eq!(parse("\ndevbox\n"), None, "缺标签 ⇒ 当没有");
    assert_eq!(parse(""), None);
}

#[test]
fn a_record_whose_first_line_is_not_a_label_is_not_a_record() {
    assert_eq!(parse("123-abc\ndevbox\n"), None);
    assert_eq!(parse("123\ndevbox\n"), None);
}

const LINE_POSIX: &str = "'/usr/bin/ssh' -t -p 22 dev@192.0.2.9 -- 'bash -lic '\\''claude --resume x'\\'''";
const LINE_PS: &str = "& 'C:\\Windows\\System32\\OpenSSH\\ssh.exe' -t -p 22 dev@192.0.2.9 -- 'bash -lic ''claude --resume x'''";

/// 开窗那一串：前奏 · 那一行 · 收尾，收尾删的正是前奏写的那一份（那条 ssh 怎么退的都一样：「记录在」＝「开窗那条 ssh 还开着」）。
#[test]
fn the_window_line_forgets_the_record_once_the_ssh_returns() {
    let file = Path::new("/home/u/.cc-monitor/opened-windows/0b9c3f2e-1111-4222-8333-944455556666");
    let posix = window(false, file, "devbox", LINE_POSIX);
    assert!(posix.starts_with(&prelude(false, file, "devbox")), "{posix}");
    let after = posix.split_once(LINE_POSIX).expect("那一行原样在里面").1;
    assert_eq!(
        after,
        format!("; rm -f -- '{}'", file.display()),
        "那一行之后接删记录那一句"
    );

    let ps = window(true, file, "devbox", LINE_PS);
    assert!(ps.starts_with(&prelude(true, file, "devbox")), "{ps}");
    let rest = &ps[prelude(true, file, "devbox").len()..];
    assert_eq!(
        rest,
        format!(
            "try {{ {LINE_PS} }} finally {{ Remove-Item -LiteralPath '{}' -ErrorAction Ignore }}",
            file.display()
        ),
        "PowerShell：那一行包进 try，Ctrl+C 停下那一行也删（-NoExit 的窗口留着 shell）"
    );
}

/// 真跑一遍 POSIX 那一串（`sh -c`，那一行换成看一眼记录的命令）：那一行跑的时候记录在，跑完就没了。
#[cfg(target_os = "linux")]
#[test]
fn the_posix_window_line_really_writes_then_forgets_the_record() {
    let d = std::env::temp_dir().join(format!("ccm-opened-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let file = d.join("0b9c3f2e-1111-4222-8333-944455556666");
    let seen = d.join("seen");
    let line = format!(
        "cp {} {}",
        shell_quote_core::posix_quote(&file.to_string_lossy()),
        shell_quote_core::posix_quote(&seen.to_string_lossy())
    );
    let st = std::process::Command::new("sh")
        .arg("-c")
        .arg(window(false, &file, "devbox", &line))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .status()
        .unwrap();
    let seen_body = std::fs::read_to_string(&seen).ok();
    let left = file.exists();
    let _ = std::fs::remove_dir_all(&d);
    assert!(st.success());
    let rec = parse(&seen_body.expect("那一行跑的时候记录在")).expect("认得出");
    assert_eq!(rec.machine, "devbox");
    assert!(!left, "那一行跑完记录就没了");
}
