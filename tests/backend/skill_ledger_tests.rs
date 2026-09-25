//! 〔SU1 · 第四波 4C〕`skill_ledger.rs`（skill 装记录，第四层）的判据：记得对、并得对、摘得对、读不懂不覆盖、只有一个家。
//!
//! 守的要求（住址）：用户裁决 **V116**「要，只删装时写进去的文件」—— 原文「装的时候记下写了哪些文件，卸只删这些（装完用户自己改过的先问）」；
//! `调研/第四波记录/SU1.md §1.1`（记什么 · 键是目录 · `created` 取第一次的）· `readonly_guard` 第四层（后端自有状态、全仓一个写口）。
//!
//! 买到：临时目录上真读真写 —— add 并进去的逐格相等（新路径加 · 旧路径换摘要 · `created` 取第一次）· drop 逐格相等（剩零整条摘）·
//! 目录由记录那一侧按 `skills 根 / name` 自己算（调用方给的路径不收）· 坏入参拒且文件逐字节不变 · 读不懂 / 另一版本的不覆盖 ·
//! 没变不写（逐字节 ＋ inode 不换）· 文件名在全部生产代码里恰好一个家（两向 ＋ 正控）。
//! 买不到：两个后端**进程**同时写（进程内那把锁挡不住）· 真 Windows 上 `rename` 覆盖既有文件。

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-skill-ledger-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

fn rec(digest: &str, created: bool) -> Value {
    json!({ "digest": digest, "created": created })
}

const D1: &str = "0000000000000001";
const D2: &str = "0000000000000002";
const D3: &str = "0000000000000003";

fn read_ok(path: &Path) -> Ledger {
    match read_at(path) {
        Read::Ok(l) => l,
        other => panic!("读不回来：{other:?}"),
    }
}

#[test]
fn add_merges_new_paths_updates_digests_and_keeps_the_first_created() {
    let d = temp_dir("add");
    let file = d.join("state/skill-installs.json");
    let root = d.join("skills");
    let first = record_at(
        &file,
        Some(&root),
        &json!({"op": "add", "name": "demo", "files": {"SKILL.md": rec(D1, true), "run.sh": rec(D2, false)}}),
    )
    .expect("第一次记");
    let dir = root.join("demo").display().to_string();
    assert_eq!(
        first,
        json!({"dir": dir, "name": "demo", "changed": true, "remaining": 2})
    );
    // 再装一次：SKILL.md 换了摘要、这一回说是「盖掉」的 —— `created` 仍取第一次的；多了一个新文件。
    record_at(
        &file,
        Some(&root),
        &json!({"op": "add", "name": "demo", "files": {"SKILL.md": rec(D3, false), "new.md": rec(D1, true)}}),
    )
    .expect("第二次记");
    let l = read_ok(&file);
    let want: BTreeMap<String, Recorded> = [
        ("SKILL.md", D3, true),
        ("new.md", D1, true),
        ("run.sh", D2, false),
    ]
    .into_iter()
    .map(|(p, dg, c)| {
        (
            p.to_string(),
            Recorded {
                digest: dg.into(),
                created: c,
            },
        )
    })
    .collect();
    assert_eq!(l.installs.len(), 1);
    assert_eq!(l.installs[&dir].files, want);
    assert_eq!(l.installs[&dir].name, "demo");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_directory_is_computed_here_and_never_taken_from_the_caller() {
    let d = temp_dir("dir");
    let file = d.join("skill-installs.json");
    let root = d.join("skills");
    let v = record_at(
        &file,
        Some(&root),
        &json!({"op": "add", "name": "demo", "dir": "/etc", "files": {"a.md": rec(D1, true)}}),
    )
    .expect("记");
    let want = root.join("demo").display().to_string();
    assert_eq!(v["dir"], json!(want));
    assert_eq!(
        read_ok(&file).installs.keys().cloned().collect::<Vec<_>>(),
        vec![want]
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn drop_removes_exactly_the_named_paths_and_the_whole_entry_at_zero() {
    let d = temp_dir("drop");
    let file = d.join("skill-installs.json");
    let root = d.join("skills");
    record_at(
        &file,
        Some(&root),
        &json!({"op": "add", "name": "demo", "files": {"a": rec(D1, true), "b": rec(D2, true), "c": rec(D3, false)}}),
    )
    .unwrap();
    let dir = root.join("demo").display().to_string();
    let v = record_at(
        &file,
        None,
        &json!({"op": "drop", "dir": dir, "paths": ["a", "c"]}),
    )
    .expect("摘");
    assert_eq!(
        v,
        json!({"dir": dir, "name": "demo", "changed": true, "remaining": 1})
    );
    assert_eq!(
        read_ok(&file).installs[&dir]
            .files
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["b".to_string()]
    );
    // 不在记录里的路径 ⇒ 拒，一个字节不动
    let before = std::fs::read(&file).unwrap();
    let e = record_at(
        &file,
        None,
        &json!({"op": "drop", "dir": dir, "paths": ["b", "zzz"]}),
    )
    .expect_err("摘一个没记的");
    assert_eq!(e.0, "bad_args");
    assert_eq!(std::fs::read(&file).unwrap(), before, "拒了还动了文件");
    let e = record_at(
        &file,
        None,
        &json!({"op": "drop", "dir": "/nowhere", "paths": ["b"]}),
    )
    .expect_err("没记着的目录");
    assert_eq!(e.0, "not_found");
    // 摘到零 ⇒ 整条走
    let v = record_at(
        &file,
        None,
        &json!({"op": "drop", "dir": dir, "paths": ["b"]}),
    )
    .unwrap();
    assert_eq!(v["remaining"], json!(0));
    assert!(read_ok(&file).installs.is_empty(), "剩零个的那一条还挂着");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_input_is_refused_and_the_file_is_untouched() {
    let d = temp_dir("bad");
    let file = d.join("skill-installs.json");
    let root = d.join("skills");
    record_at(
        &file,
        Some(&root),
        &json!({"op": "add", "name": "demo", "files": {"a": rec(D1, true)}}),
    )
    .unwrap();
    let before = std::fs::read(&file).unwrap();
    for bad in [
        json!({}),
        json!({"op": "put"}),
        json!({"op": "add", "name": "../x", "files": {"a": rec(D1, true)}}),
        json!({"op": "add", "name": "demo", "files": {}}),
        json!({"op": "add", "name": "demo", "files": {"../a": rec(D1, true)}}),
        json!({"op": "add", "name": "demo", "files": {"a": rec("XYZ", true)}}),
        json!({"op": "add", "name": "demo", "files": {"a": rec("000000000000000G", true)}}),
        json!({"op": "add", "name": "demo", "files": {"a": {"digest": D1}}}),
        json!({"op": "add", "name": "demo", "files": {"a": {"digest": D1, "created": true, "extra": 1}}}),
        json!({"op": "drop", "paths": ["a"]}),
        json!({"op": "drop", "dir": "x", "paths": "a"}),
    ] {
        let e = record_at(&file, Some(&root), &bad).expect_err("坏入参被收下");
        assert_eq!(e.0, "bad_args", "{bad}");
        assert_eq!(
            std::fs::read(&file).unwrap(),
            before,
            "{bad} 拒了还动了文件"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_unreadable_or_other_version_ledger_is_never_overwritten() {
    let d = temp_dir("unreadable");
    let file = d.join("skill-installs.json");
    let root = d.join("skills");
    for body in [
        b"{not json".to_vec(),
        br#"{"v":2,"installs":{},"more":true}"#.to_vec(),
        br#"{"v":1,"installs":{"/x":{"name":"x","files":{"a":{"digest":"0000000000000001","created":"yes"}}}}}"#.to_vec(),
    ] {
        std::fs::write(&file, &body).unwrap();
        let e = record_at(
            &file,
            Some(&root),
            &json!({"op": "add", "name": "demo", "files": {"a": rec(D1, true)}}),
        )
        .expect_err("读不懂的被当成空的覆盖了");
        assert_eq!(e.0, "ledger_unreadable");
        assert_eq!(std::fs::read(&file).unwrap(), body, "读不懂的那份被动了");
        assert!(matches!(load_at(&file), Err(("ledger_unreadable", _))));
    }
    std::fs::remove_file(&file).unwrap();
    assert_eq!(load_at(&file).unwrap(), Ledger::default(), "没有 ⇒ 空的");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn nothing_changed_nothing_written() {
    let d = temp_dir("same");
    let file = d.join("skill-installs.json");
    let root = d.join("skills");
    let args = json!({"op": "add", "name": "demo", "files": {"a": rec(D1, true)}});
    record_at(&file, Some(&root), &args).unwrap();
    let before = std::fs::read(&file).unwrap();
    #[cfg(unix)]
    let ino = {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::metadata(&file).unwrap().ino()
    };
    let v = record_at(&file, Some(&root), &args).unwrap();
    assert_eq!(v["changed"], json!(false));
    assert_eq!(std::fs::read(&file).unwrap(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        assert_eq!(
            std::fs::metadata(&file).unwrap().ino(),
            ino,
            "没变也换了一个 inode（写了一遍）"
        );
    }
    // 临时文件没留下
    let left: Vec<String> = std::fs::read_dir(&d)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec![FILE_NAME.to_string()]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn digest_sees_every_byte_and_nothing_else() {
    assert_eq!(digest_of("a\n"), digest_of("a\n"));
    assert_ne!(digest_of("a\n"), digest_of("a\r\n"));
    assert_ne!(digest_of(""), digest_of(" "));
    assert!(valid_digest(&digest_of("x")));
}

/// 文件名在 `src/` 全部生产代码里恰好一个家（第二个写者 / 空转两向都红）。正控：把针塞进一份副本里数得出来。
#[test]
fn the_file_name_has_exactly_one_home_in_all_production_code() {
    let src = crate::guard_support::repo_root().join("src");
    let needle = format!("{}-{}.json", "skill", "installs");
    let mut scanned = 0usize;
    let mut homes: std::collections::BTreeSet<String> = Default::default();
    for (path, body) in guard_core::scan_tree_excluding(&src, &["rs", "ts", "sh"], &[]) {
        scanned += 1;
        let code = match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => crate::guard_support::production_code(&body),
            Some("ts") => guard_core::strip_comment_lines(&body),
            _ => guard_core::strip_hash_comment_lines(&body),
        };
        if code.contains(needle.as_str()) {
            homes.insert(
                path.strip_prefix(&src)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert!(scanned > 500, "只扫到 {scanned} 份源码 —— 遍历坏了");
    let want: std::collections::BTreeSet<String> = ["backend/skill_ledger.rs".to_string()].into();
    assert_eq!(
        homes, want,
        "`{needle}` 在生产代码里的家对不上（多 = 第二个写者；少 = 空转）"
    );
    // 正控：同一个判法在一份塞了针的副本上数得出来
    let planted =
        crate::guard_support::production_code(&format!("const X: &str = \"{needle}\";\n"));
    assert!(planted.contains(needle.as_str()));
}
