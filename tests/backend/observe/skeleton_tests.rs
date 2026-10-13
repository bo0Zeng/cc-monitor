//! 大记录的骨架落盘 —— 判据（盘上那一层：键 · 版本 · 份数上限）。读多少字节的判据住两个用它的地方
//! （`watcher_prime_tests` 的预读 · `record_scan_tests` 的冷开扫描图）。
//! 家在沙箱的数据目录里（`CCM_DATA_DIR` 指过去），不碰真家目录。

use super::*;
use std::path::PathBuf;

/// 一个沙箱数据目录 ＋ 指着它的写句柄。
pub(crate) fn sandbox(tag: &str) -> (PathBuf, Own) {
    let d = std::env::temp_dir().join(format!(
        "ccm-skeleton-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    let data = d.join("data").to_string_lossy().into_owned();
    let home = d.to_string_lossy().into_owned();
    let own = Own::issue_with(&|k| match k {
        creds_core::store::DATA_DIR_ENV => Some(data.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    });
    (d, own)
}

fn mark(len: u64, mtime: Option<u128>, cursor: u64) -> Mark {
    Mark {
        len,
        mtime,
        cursor,
        witness: vec![b'x'; 4],
    }
}

#[test]
fn look_same_only_when_length_and_mtime_both_held() {
    let m = mark(100, Some(7), 90);
    assert_eq!(m.look(100, Some(7)), Look::Same);
    assert_eq!(
        m.look(100, Some(8)),
        Look::Changed,
        "同长、修改时刻变了 ⇒ 改写过"
    );
    assert_eq!(m.look(100, None), Look::Changed, "问不到修改时刻不算没变");
    assert_eq!(m.look(140, Some(9)), Look::Grown);
    assert_eq!(m.look(99, Some(9)), Look::Changed, "变短 ⇒ 整份");
    assert_eq!(mark(100, None, 90).look(100, None), Look::Changed);
}

#[test]
fn a_stored_skeleton_reads_back_and_a_foreign_one_does_not() {
    let (d, own) = sandbox("rt");
    let s = Skeletons::at(own, 0);
    let rec = d.join("a.jsonl");
    assert!(s.load::<Vec<u32>>(&rec, "t").is_none(), "没落过 ⇒ 没有");
    s.store(&rec, "t", &vec![1u32, 2, 3]);
    assert_eq!(s.load::<Vec<u32>>(&rec, "t"), Some(vec![1, 2, 3]));
    assert!(
        s.load::<Vec<u32>>(&d.join("b.jsonl"), "t").is_none(),
        "别的路径读到了它"
    );
    // 落点在数据目录的 cache/skeleton 底下，按路径摘要起名。
    let f = d
        .join("data")
        .join("cache")
        .join("skeleton")
        .join(format!("{}.t.json", digest(&rec)));
    assert!(f.is_file(), "没落在数据目录的 cache/skeleton 底下");
    // 版本对不上 ⇒ 当没有（整份重扫）。
    let raw = std::fs::read_to_string(&f).unwrap();
    std::fs::write(&f, raw.replace(&format!("\"v\":{VERSION}"), "\"v\":999")).unwrap();
    assert!(
        s.load::<Vec<u32>>(&rec, "t").is_none(),
        "认不出的版本也读进来了"
    );
    // 路径对不上（摘要撞了）⇒ 当没有。
    std::fs::write(&f, raw.replace("a.jsonl", "c.jsonl")).unwrap();
    assert!(
        s.load::<Vec<u32>>(&rec, "t").is_none(),
        "别人的骨架被认成这一份"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn small_records_are_not_kept_and_the_dir_is_capped() {
    let (d, own) = sandbox("cap");
    assert!(!Skeletons::new(own.clone()).wants(MIN_LEN - 1));
    assert!(Skeletons::new(own.clone()).wants(MIN_LEN));
    let s = Skeletons::at(own, 0);
    // 上一个进程原子写到一半被杀留下的旁名：下一次落的时候删掉。
    let dir = d.join("data").join("cache").join("skeleton");
    s.store(&d.join("first.jsonl"), "t", &0);
    let stale = dir.join("x.t.json.123.ThreadId(1).0.tmp");
    std::fs::write(&stale, b"half").unwrap();
    for i in 0..MAX_FILES + 3 {
        s.store(&d.join(format!("r{i}.jsonl")), "t", &i);
    }
    assert!(!stale.exists(), "写到一半的旁名留着没删");
    let at = |i: usize| dir.join(format!("{}.t.json", digest(&d.join(format!("r{i}.jsonl")))));
    let kept = (0..MAX_FILES + 3).filter(|&i| at(i).is_file()).count();
    assert_eq!(kept, MAX_FILES, "份数没封顶");
    assert!(at(MAX_FILES + 2).is_file(), "刚写的那一份被删了");
    let _ = std::fs::remove_dir_all(&d);
}
