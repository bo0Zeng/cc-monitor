//! 判据用的子进程夹具：起一个 `sleep`，等到 `/proc/<pid>/environ` 读到的是它自己的环境才交回。
//! 读进程环境的那几层（控制面打身份标 · 观测面认这条会话用哪个号）的判据都经这一份（[`super::EnvRead`] 那一段说的 exec 窗口）。

/// 夹具注进每个 `sleep` 的哨兵键（测试进程自己没有它）。
const OWN_ENVIRON_SENTINEL: &str = "CCM_TEST_KID_OWN_ENVIRON";

/// 起一个 `sleep 60`（`shape` 按需改它的环境），**回来时 `/proc/<pid>/environ` 读到的已经是它自己的环境**。
///
/// 🔴 「environ 读得出、非空」**不等于**读到的是子进程自己的环境：
/// `posix_spawn` 的 vfork 父进程在子进程 `execve` **换 mm 之前**就被放回来；子进程此刻若被调度走，
/// `/proc/<kid>/environ` 读的是**测试进程自己**的环境 —— 非空、没有令牌、没有 `TMUX_PANE`。
/// 本机要让父进程抢在子进程前面才现形（`sudo chrt -R -f 1 taskset -c 7`：旧门 200/200 读到父进程环境，本门 200/200 对）。
/// ⇒ 门是「读到哨兵」：它只在子进程 exec 完、换上自己的 mm 之后才在；`sleep` 不再 exec ⇒ 之后每次读都是它自己的。
/// 等时**睡着让出 CPU**（子进程可能排在同一个核上）；10 s 等不到就 panic —— 那时的 `None` 是「取不到」不是「没设」。
pub fn spawn_settled_sleep(shape: impl FnOnce(&mut std::process::Command)) -> std::process::Child {
    let mut cmd = std::process::Command::new("sleep");
    cmd.arg("60");
    shape(&mut cmd);
    // 哨兵在 `shape` 之后注：`shape` 里就算 `env_clear()` 也清不掉它。
    cmd.env(OWN_ENVIRON_SENTINEL, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut kid = cmd
        .spawn()
        .expect("起不来 `sleep` —— 夹具坏了，读数一个字都不能信");
    let pid = kid.id();
    let want = format!("{OWN_ENVIRON_SENTINEL}=1");
    let t0 = std::time::Instant::now();
    let own = loop {
        if let Ok(b) = std::fs::read(format!("/proc/{pid}/environ")) {
            if b.split(|c| *c == 0).any(|e| e == want.as_bytes()) {
                break true;
            }
        }
        if t0.elapsed() > std::time::Duration::from_secs(10) {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    if !own {
        let _ = kid.kill();
        let _ = kid.wait();
        panic!(
            "pid {pid} 等了 10 s 仍读不到它自己的 environ —— 夹具坏了，\
             此刻任何读数说的都不是这个子进程"
        );
    }
    kid
}
