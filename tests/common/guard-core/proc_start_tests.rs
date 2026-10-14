//! **读进程起始时刻先剥掉进程名**：`/proc/<pid>/stat` 的第 2 格是带括号的进程名，名字里能有空格（也能有 `)`）。
//! 直接按空格数到第 22 格（`awk '{print $22}'`），名字带空格时列就错位，读到的是别的格 ——
//! 喂给后端的 `procStart` 对不上，会话判不活。
//!
//! | # | 判什么 |
//! |---|---|
//! | P1 | 仓里每个 shell 脚本：按列号取 `/proc/…/stat` 的那一行先剥到最后一个 `) `（`sed 's/.*) //'` 或 `${s##*) }`） |
//! | P2 | e2e 共用帮手 `sandbox-env.sh` 的 `proc_start <pid>` 对一个名字带空格和 `)` 的进程读出的数 == 按最后一个 `)` 切出来的第 22 格 |
//! | P3 | P1 的认法在正反两个样本上判对 |
//!
//! 买不到：取列藏在别的函数里、跨行拼的（`stat=$(cat …)` 下一行再 `awk`），P1 认不出 —— 今天仓里没有这种写法。

use regex::Regex;
use std::sync::OnceLock;

fn stat_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"/proc/[^\s]*/stat\b").unwrap())
}

fn column_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"print \$\d+|\$\{?\d{2}\}?|-f ?\d+|cut -d").unwrap())
}

const HELPER: &str = "tests/e2e/sandbox-env.sh";
/// 本文件：样本写在字符串里，不扫自己（它也不是 shell 脚本，这里只是写明）。
const SELF: &str = "tests/common/guard-core/proc_start_tests.rs";

/// 一行 shell 是不是「按列号取 /proc/<pid>/stat、却没先剥进程名」。
fn reads_stat_column_unstripped(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with('#') {
        return false;
    }
    if !stat_re().is_match(t) || !column_re().is_match(t) {
        return false;
    }
    !(t.contains(") //") || t.contains("##*) "))
}

#[test]
fn every_shell_read_of_proc_stat_columns_strips_the_process_name_first() {
    let root = super::no_outside_refs::repo();
    let mut bad = Vec::new();
    let mut seen = 0usize;
    for rel in crate::shell_scripts(&root) {
        if rel == SELF {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        for (i, line) in src.lines().enumerate() {
            if stat_re().is_match(line) {
                seen += 1;
            }
            if reads_stat_column_unstripped(line) {
                bad.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        seen > 0,
        "一处读 /proc/<pid>/stat 的都没扫到 —— 人群空了，判据空真"
    );
    assert!(
        bad.is_empty(),
        "这几行按列号取 /proc/<pid>/stat、没先剥到最后一个 `) `（进程名带空格时列会错位；e2e 里用 `sandbox-env.sh` 的 `proc_start`）：\n{}",
        bad.join("\n")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn the_shared_e2e_helper_reads_start_time_of_a_process_whose_name_has_spaces() {
    let root = super::no_outside_refs::repo();
    let env = std::fs::read_to_string(root.join(HELPER)).expect("sandbox-env.sh 读不动");
    let def = env
        .lines()
        .find(|l| l.starts_with("proc_start()"))
        .unwrap_or_else(|| panic!("{HELPER} 里没有一行 `proc_start() …` 的定义"));

    // 一个名字带空格和 `)` 的进程：脚本的进程名是脚本文件名（`sleep` 是多合一的那种，拷了改名不认）。
    // 它等 stdin 读到头就走：这边丢掉管子它就退，不留进程。
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("proc-start-judge-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("a b) c");
    // 本进程只写一份旁稿，`exe` 由 `cp` 那个子进程写：本进程里从没有过 `exe` 的写 fd。
    // 在本进程里直接写 ⇒ 并行测试里别的线程正在 fork 时，子进程在 fork 与 exec 之间还攥着那份写 fd，
    // 这时 exec 它 ＝ `ETXTBSY`（`Text file busy (os error 26)`，整套并行、机器忙时撞得到）。进程名要是脚本名，不能改经 `/bin/sh` 起。
    let draft = dir.join("draft");
    std::fs::write(&draft, "#!/bin/sh\nread _ || :\n").unwrap();
    let cp = std::process::Command::new("cp")
        .arg(&draft)
        .arg(&exe)
        .status()
        .expect("cp 起不来");
    assert!(cp.success(), "cp 没写成：{cp:?}");
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = std::process::Command::new(&exe)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("起不来");
    let pid = child.id();
    // 等它真变成那个名字（exec 完之前 comm 还是父进程的）。
    for () in crate::wall::rounds() {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        if s.contains("(a b) c)") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    assert!(stat.contains("(a b) c)"), "造的进程名没生效：{stat}");
    let after = &stat[stat.rfind(')').unwrap() + 2..];
    let want = after.split(' ').nth(19).unwrap().to_string();

    let out = std::process::Command::new("bash")
        .arg("-c")
        .arg(format!("{def}\nproc_start {pid}"))
        .output()
        .expect("bash");
    drop(child.stdin.take());
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        got,
        want,
        "proc_start 对名字带空格的进程读错了格（stderr：{}）",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_unstripped_form_is_told_apart_on_samples() {
    for bad in [
        r#"ticks="$(awk '{print $22}' "/proc/$SLEEPER/stat")""#,
        r#"PROC_START=$(awk '{print $22}' "/proc/$HOST_PID/stat")"#,
        r#"s=$(cut -d' ' -f22 /proc/$$/stat)"#,
    ] {
        assert!(reads_stat_column_unstripped(bad), "该认出的没认出：{bad}");
    }
    for good in [
        r#"proc_start() { sed 's/.*) //' "/proc/$1/stat" | awk '{print $20}'; }"#,
        r#"s=$(sed "s/.*) //" /proc/$$/stat | cut -d" " -f20)"#,
        r#"# 旧写法 awk '{print $22}' /proc/$pid/stat 会错位"#,
        r#"[ -r "/proc/$pid/stat" ] || exit 1"#,
    ] {
        assert!(!reads_stat_column_unstripped(good), "不该认的认了：{good}");
    }
}
