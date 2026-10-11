//! [`super`] 的判据 —— monitor 那一侧怎么等借控制台的小助手。

use super::*;
use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};

/// 起一个替身（POSIX `sh -c …` · Windows `cmd /c …`），同小助手那一处的起法（隐藏控制台 · 不随 monitor 死 · stderr 丢掉）。
/// `dir`：替身在哪个目录里跑（脚本只写相对名，路径不进命令行 —— 免得 `cmd /c` 与参数转义打架）。
fn stand_in(
    script: String,
    dir: Option<std::path::PathBuf>,
) -> impl Fn() -> std::io::Result<crate::spawn_managed::ManagedChild> {
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
        if let Some(d) = &dir {
            c.current_dir(d);
        }
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
///
/// 不按墙钟判：替身一直等到判据放行（目录里出现 `go`）才答话、留记号，所以 `ask_helper` 回来那一刻它一定还没答完，
/// 机器多忙都一样；放行之后记号出现 ⇒ 它没被掐。等的那几处只是上限（卡死时别挂住整套），不是判据。
#[test]
fn a_late_helper_is_left_to_finish_not_killed() {
    let dir = std::env::temp_dir().join(format!("ccm-console-helper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let held = if cfg!(windows) {
        "for /l %i in (0,0,1) do @if exist go (echo late& type nul > finished& exit)".to_string()
    } else {
        "while [ ! -e go ]; do sleep 0.05; done; echo late; touch finished".to_string()
    };
    let spawn = stand_in(held, Some(dir.clone()));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(ask_helper(std::time::Duration::from_millis(200), &spawn));
    });
    let got = rx.recv_timeout(std::time::Duration::from_secs(60));
    if got.is_err() {
        // 红之前先放行替身，别留一个一直在等的进程。
        let _ = std::fs::write(dir.join("go"), b"");
    }
    assert!(
        got.is_ok(),
        "替身还没放行（答不了话），ask_helper 却一直没回来 —— 到点了还在等它"
    );
    assert_eq!(got.unwrap(), None, "没答完却收了半截");
    assert!(!dir.join("finished").exists(), "量具：没放行替身就跑完了");
    std::fs::write(dir.join("go"), b"").unwrap();
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !dir.join("finished").exists() && std::time::Instant::now() < limit {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        dir.join("finished").exists(),
        "到点没答完的小助手被掐掉了（放行之后没跑完它自己那一段）"
    );
    let quick = ask_helper(
        std::time::Duration::from_secs(60),
        &stand_in("echo 42 shown 0x1".into(), None),
    );
    assert_eq!(quick.as_deref().map(str::trim_end), Some("42 shown 0x1"));
    let _ = std::fs::remove_dir_all(&dir);
}
