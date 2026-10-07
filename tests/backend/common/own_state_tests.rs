//! **后端自有状态文件的读三态与原子写** —— 判据。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | S1 | 不在 ⇒ `Absent`；是目录 / 超上限 ⇒ `Unreadable`；在且读得懂 ⇒ `Present` | 真文件 |
//! | S2 | 写完读回逐字同；`write_json` 收尾一个换行；新建的文件 0600（unix） | 真文件 |
//! | S3 | 同一进程多线程不拿锁同时写同一份 ⇒ 每一下都成（旁名不撞），目录里只剩那一份 | 真文件、16 线程 |
//! | S4 | 挪不过去（目标是个非空目录）⇒ 报错、自己的旁名删了、目录里别人的文件一个没动 | 真文件 |

use super::*;
use std::path::PathBuf;

fn sandbox(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "ccm-own-state-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("sandbox");
    base
}

/// 目录底下留着的旁名（`.tmp` 结尾）。
fn side_files(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = guard_core::scan_tree!(dir, &["tmp"])
        .into_iter()
        .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn s1_three_states() {
    let base = sandbox("s1");
    let p = base.join("x.json");
    assert!(matches!(read_bytes(&p, 64), Read::Absent));
    std::fs::create_dir(&p).expect("dir in place");
    assert!(matches!(read_bytes(&p, 64), Read::Unreadable(_)));
    std::fs::remove_dir(&p).expect("rm");
    std::fs::write(&p, vec![b'a'; 65]).expect("seed");
    assert!(
        matches!(read_bytes(&p, 64), Read::Unreadable(_)),
        "超上限也读进来了"
    );
    std::fs::write(&p, b"{\"a\":1}").expect("seed");
    match read_json::<serde_json::Value>(&p, 64) {
        Read::Present(v) => assert_eq!(v["a"], 1),
        _ => panic!("读得懂的没读出来"),
    }
    std::fs::write(&p, b"{").expect("seed");
    assert!(matches!(
        read_json::<serde_json::Value>(&p, 64),
        Read::Unreadable(_)
    ));
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn s2_round_trip_newline_and_private() {
    let base = sandbox("s2");
    let p = base.join("x.json");
    write_json(&p, &serde_json::json!({"a": 1})).expect("write");
    assert_eq!(std::fs::read(&p).expect("read"), b"{\"a\":1}\n");
    write(&p, b"raw").expect("overwrite");
    assert_eq!(std::fs::read(&p).expect("read"), b"raw");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&p).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "后端自有状态文件不是只给本人");
    }
    assert!(side_files(&base).is_empty(), "留了旁名");
    assert!(p.is_file());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn s3_concurrent_writers_never_collide_on_the_side_name() {
    let base = sandbox("s3");
    let p = base.join("x.json");
    let fails: Vec<String> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..16)
            .map(|i| {
                let p = p.clone();
                s.spawn(move || {
                    (0..20)
                        .filter_map(|j| write(&p, format!("{i}-{j}").as_bytes()).err())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        hs.into_iter()
            .flat_map(|h| h.join().expect("join"))
            .collect()
    });
    assert!(
        fails.is_empty(),
        "并发写撞了旁名 {} 次，头一条：{:?}",
        fails.len(),
        fails.first()
    );
    assert!(side_files(&base).is_empty(), "留了旁名");
    assert!(p.is_file());
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn s4_failed_rename_removes_only_its_own_side_file() {
    let base = sandbox("s4");
    let target = base.join("x.json");
    std::fs::create_dir(&target).expect("target is a dir");
    std::fs::write(target.join("keep"), b"k").expect("non-empty");
    std::fs::write(base.join("x.json.other.tmp"), b"theirs").expect("foreign");
    assert!(write(&target, b"mine").is_err(), "挪到非空目录上居然成了");
    assert_eq!(
        side_files(&base),
        vec!["x.json.other.tmp".to_string()],
        "旁名没删，或删了别人的文件"
    );
    assert!(target.join("keep").is_file(), "挪的目标被动了");
    std::fs::remove_dir_all(&base).ok();
}
