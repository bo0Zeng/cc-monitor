//! 常驻后端起来那一下把 fd 软上限抬到硬上限（[`raise_open_files_limit`]）。
//! 改的是整个进程的上限 ⇒ 量的那一半在自己起的子进程里跑：先把软上限压到 [`LOW`]，再调它，读回来。

use super::*;

const CHILD_MARK: &str = "CCM_FD_LIMIT_CHILD";
const CHILD_TEST: &str =
    "platform::proc::fd_limit_tests::the_soft_open_files_limit_goes_up_to_the_hard_one";
/// 子进程先把软上限压到这么低（桌面会话缺省给的 1024 之下，好看出抬没抬）。
const LOW: u64 = 64;

#[cfg(unix)]
fn get() -> (u64, u64) {
    let mut r = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: 只读本进程的上限，写进栈上这一格。
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut r) }, 0);
    (r.rlim_cur as u64, r.rlim_max as u64)
}

/// 子进程那一半（判据带着 [`CHILD_MARK`] 重起自己时走这里）：压低 → 调 → 印读回来的两个数。
#[cfg(unix)]
fn as_child() {
    let (_, hard) = get();
    let low = libc::rlimit {
        rlim_cur: LOW as libc::rlim_t,
        rlim_max: hard as libc::rlim_t,
    };
    // SAFETY: 只动这个子进程自己的软上限。
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &low) }, 0);
    let said = raise_open_files_limit();
    let (soft, hard_after) = get();
    println!("FD-LIMIT {soft} {hard} {hard_after} {said}");
}

/// 软上限从 [`LOW`] 抬到硬上限，硬上限不动；说的那句里有起止两个数。
#[cfg(unix)]
#[test]
fn the_soft_open_files_limit_goes_up_to_the_hard_one() {
    // 带着记号重起的那一趟是子进程；没带的这一趟是判据。
    match std::env::var_os(CHILD_MARK) {
        Some(_) => as_child(),
        None => judge(),
    }
}

#[cfg(unix)]
fn judge() {
    let out = std::process::Command::new(std::env::current_exe().expect("测试二进制"))
        .args([CHILD_TEST, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_MARK, "1")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("起子进程");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        // libtest 的「test … 」前缀与这一行印在同一行上 ⇒ 按记号所在处切。
        .find_map(|l| l.find("FD-LIMIT ").map(|at| &l[at + "FD-LIMIT ".len()..]))
        .unwrap_or_else(|| {
            panic!(
                "子进程没印读数：\n{stdout}\n{}",
                String::from_utf8_lossy(&out.stderr)
            )
        });
    let mut it = line.splitn(4, ' ');
    let mut num = || -> u64 { it.next().and_then(|s| s.parse().ok()).expect("数") };
    let (soft, hard, hard_after) = (num(), num(), num());
    let said = it.next().unwrap_or_default().to_string();
    // 硬上限是「不限」（这几台都不是）⇒ 只要求抬上去了；否则恰好抬到硬上限。
    let want = if hard == libc::RLIM_INFINITY as u64 {
        soft.max(LOW + 1)
    } else {
        hard
    };
    assert_eq!(
        (soft, hard_after),
        (want, hard),
        "软上限该抬到 {want}、硬上限不动（{said}）"
    );
    assert!(
        said.contains(&LOW.to_string()) && said.contains(&want.to_string()),
        "说的那句要有起止两个数：{said}"
    );
}
