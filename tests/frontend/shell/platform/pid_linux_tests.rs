//! Linux 那一臂的进程读法（读 `/proc`）：本进程自己就是样本。

use super::*;

/// ★ 起始时刻 = `/proc/<pid>/stat` 第 22 格；往上的祖先第一个是父进程、带名字；本进程活着，一个不存在的进程号不活。
#[test]
fn start_stamp_ancestors_and_liveness_read_proc() {
    let me = std::process::id();
    let stat = std::fs::read_to_string(format!("/proc/{me}/stat")).unwrap();
    let after = &stat[stat.rfind(')').unwrap() + 2..];
    let f: Vec<&str> = after.split_whitespace().collect();
    assert_eq!(start_stamp(me), f[19].parse::<u64>().ok());
    let ppid: u32 = f[1].parse().unwrap();
    let up = ancestors(me);
    assert_eq!(up.first().map(|(p, _)| *p), Some(ppid), "{up:?}");
    assert!(!up[0].1.is_empty(), "祖先带名字");
    assert!(!up.iter().any(|(p, _)| *p == me), "不含自己");
    assert!(is_alive(me));
    assert!(!is_alive(u32::MAX - 7));
    assert_eq!(start_stamp(u32::MAX - 7), None);
}
