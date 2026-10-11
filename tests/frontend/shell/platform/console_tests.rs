//! [`super`] 的判据 —— monitor 那一侧怎么等借控制台的小助手。

use super::*;
use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};

/// 起一个替身（POSIX `sh -c …` · Windows `cmd /c …`），同小助手那一处的起法（隐藏控制台 · 不随 monitor 死 · stderr 丢掉）。
fn stand_in(script: String) -> impl Fn() -> std::io::Result<crate::spawn_managed::ManagedChild> {
    move || {
        let mut c = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.args(["/c", &script]);
            c
        } else {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", &script]);
            c
        };
        c.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped());
        spawn_managed_cmd(
            &mut c,
            ConsolePolicy::Hidden,
            Lifetime::Detached,
            StderrSink::Null,
        )
    }
}

/// 🔴 **monitor 不掐小助手**：等到点了它还没答完 ⇒ 不再等（回 `None`），但它照样跑完（它自己有预算，到点写回标题再退 ——
/// 掐断它会把记号留在终端标题上）。正控：答得及的那一趟照收。
#[test]
fn a_late_helper_is_left_to_finish_not_killed() {
    let dir = std::env::temp_dir().join(format!("ccm-console-helper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mark = dir.join("finished");
    // 约一秒之后才答、再留一个记号文件。
    let late = if cfg!(windows) {
        format!(
            "ping -n 2 127.0.0.1 >nul & echo late & type nul > \"{}\"",
            mark.display()
        )
    } else {
        format!("sleep 1; echo late; touch '{}'", mark.display())
    };
    let t0 = std::time::Instant::now();
    let got = ask_helper(std::time::Duration::from_millis(200), &stand_in(late));
    assert_eq!(got, None, "没答完却收了半截");
    assert!(
        t0.elapsed() < std::time::Duration::from_millis(800),
        "到点了还在等：{:?}",
        t0.elapsed()
    );
    for _ in 0..60 {
        if mark.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        mark.exists(),
        "到点没答完的小助手被掐掉了（没跑完它自己那一段）"
    );
    let quick = ask_helper(
        std::time::Duration::from_secs(5),
        &stand_in("echo 42 shown 0x1".into()),
    );
    assert_eq!(quick.as_deref().map(str::trim_end), Some("42 shown 0x1"));
    let _ = std::fs::remove_dir_all(&dir);
}
