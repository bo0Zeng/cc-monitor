use super::{is_process_alive, proc_stat_starttime};

/// ★ 自己这个进程必须被判活，且 `procStart` 要与 `/proc` 对得上。
///
/// 这条同时验了两件事：读得到、比得对。用**真进程**是刻意的 ——
/// 只喂夹具字符串的话，`/proc` 路径拼错、字段序错位都测不出来。
#[test]
fn the_current_process_is_alive_and_its_starttime_matches() {
    let me = std::process::id();
    assert!(
        is_process_alive(me, None),
        "自己这个进程都判成死的 —— /proc 读路径不对"
    );
    let raw = std::fs::read_to_string(format!("/proc/{me}/stat")).expect("读自己的 stat");
    let st = proc_stat_starttime(&raw).expect("抽不到 starttime");
    assert!(
        st.parse::<u64>().is_ok() && st != "0",
        "starttime 抽成了 {st:?} —— 字段序错位（`comm` 含空格时朴素切法就会得到 0）"
    );
    assert!(
        is_process_alive(me, Some(st)),
        "procStart 与 /proc 一致却判成死的"
    );
}

/// ★ **PID 复用防御**：pid 对、`procStart` 不对 ⇒ 判死。
///
/// 这条是双重校验的全部意义 —— 少了它，pid 被复用后僵尸条目会一直显示成活跃。
#[test]
fn a_mismatched_starttime_means_dead_even_though_the_pid_exists() {
    let me = std::process::id();
    assert!(
        !is_process_alive(me, Some("1")),
        "pid 存在但 procStart 对不上，仍被判活 —— PID 复用防御没生效"
    );
}

/// 不存在的 pid ⇒ 判死。用一个**刚退出**的真子进程，不是凭空编号。
#[test]
fn a_process_that_has_exited_is_dead() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("起不了子进程");
    let pid = child.id();
    child.wait().expect("wait");
    // 子进程已 reap，`/proc/<pid>` 应当没了。
    assert!(!is_process_alive(pid, None), "已退出并 reap 的进程仍被判活");
}

/// ★ `comm` 含空格时字段序不许错位 —— 实测本机 `tmux: server` 就是这种。
///
/// 朴素 `split_whitespace()` 在这条上读到 `0`，正确值是 `1042`。
#[test]
fn a_comm_containing_spaces_does_not_shift_the_field_index() {
    // 真实形状（截自 /proc/<tmux-server>/stat，starttime = 1042）
    let raw = "123 (tmux: server) S 1 123 123 0 -1 4194560 900 0 0 0 5 2 0 0 20 \
                   0 1 0 1042 12345678 900 18446744073709551615";
    let raw = raw.replace('\\', "").replace('\n', " ");
    assert_eq!(
        proc_stat_starttime(&raw),
        Some("1042"),
        "`comm` 里的空格让字段序错位了"
    );
    // 反向：朴素切法在同一条输入上会得到别的东西 —— 证明本测试不是空转。
    let naive = raw.split_whitespace().nth(21);
    assert_ne!(
        naive,
        Some("1042"),
        "朴素切法居然也对 —— 那这条测试没有区分力，换一条更刁的输入"
    );
}

/// `comm` 里带右括号（如 `(sd-pam)`）也不许错位 —— 靠的是找**最后一个** `)`。
#[test]
fn a_comm_containing_a_closing_paren_still_parses() {
    let raw = "7 ((sd-pam)) S 1 7 7 0 -1 4194368 100 0 0 0 0 0 0 0 20 0 1 0 1023 5000 1 1";
    assert_eq!(proc_stat_starttime(raw), Some("1023"));
}
