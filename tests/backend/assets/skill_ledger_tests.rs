//! 〔SU1 · 第四波 4C〕`skill_ledger.rs`（skill 装记录，第四层）的判据：记得对、并得对、摘得对、读不懂不覆盖、只有一个家。
//!
//! 守的要求（住址）：用户裁决 **V116**「要，只删装时写进去的文件」—— 原文「装的时候记下写了哪些文件，卸只删这些（装完用户自己改过的先问）」；
//! `调研/第四波记录/SU1.md §1.1`（记什么 · 键是目录 · `created` 取第一次的）· `readonly_guard` 第四层（后端自有状态、全仓一个写口）。
//!
//! 买到：临时目录上真读真写 —— add 并进去的逐格相等（新路径加 · 旧路径换摘要 · `created` 取第一次）· drop 逐格相等（剩零整条摘）·
//! 目录由记录那一侧按 `skills 根 / name` 自己算（调用方给的路径不收）· 坏入参拒且文件逐字节不变 · 读不懂 / 另一版本的不覆盖 ·
//! 没变不写（逐字节 ＋ inode 不换）· 文件名在全部生产代码里恰好一个家（两向 ＋ 正控）。
//! 买不到：〔HX2 · 4D 订正〕两个后端进程同时写 —— 今天读—改—写在目录的跨进程锁里（`platform/lock.rs`），
//! 本族那一条用两个线程各开一次描述量（与两个进程同一种 `flock` 互斥），没有真起第二个进程 · 真 Windows 上 `rename` 覆盖既有文件。

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
        // 形状本身读得懂、只是版本号不同 ⇒ 仍然不认（别让 `deny_unknown_fields` 替版本闸挡了这一刀）
        br#"{"v":2,"installs":{}}"#.to_vec(),
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
    // 临时文件没留下（名字就是写口起的那一个）
    assert!(!d
        .join(format!("{FILE_NAME}.{}.tmp", std::process::id()))
        .exists());
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
    // 〔P3 · 主会话 09-29 裁〕字面量挪进契约 crate ⇒ 按「字面量或契约常量名」认谁叫得出它。
    let via_const = format!("{}_{}_REL", "SKILL", "LEDGER");
    let mut scanned = 0usize;
    let mut homes: std::collections::BTreeSet<String> = Default::default();
    for (path, body) in guard_core::scan_tree_excluding(&src, &["rs", "ts", "sh"], &[]) {
        scanned += 1;
        let code = match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => crate::guard_support::production_code(&body),
            Some("ts") => guard_core::strip_comment_lines(&body),
            _ => guard_core::strip_hash_comment_lines(&body),
        };
        if code.contains(needle.as_str()) || code.contains(via_const.as_str()) {
            homes.insert(
                path.strip_prefix(&src)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert!(scanned > 500, "只扫到 {scanned} 份源码 —— 遍历坏了");
    // 四个家、各一个身份：定义（契约 crate）· 写者（本模块）· 足迹里的**申报字面量**（`skill-install` 那一格，〔MIG-3b 续〕随 Claude 布局那一半住
    // `agents/claudecode/footprint.rs`；不读不写 —— V116「足迹里看得见」）· monitor 数据位置页（只 stat）。
    // 第五个家 ⇒ 红（第二个写者或者第二份申报）。
    let want: std::collections::BTreeSet<String> = [
        "common/relay-route-core/src/lib.rs".to_string(),
        "backend/assets/skill_ledger.rs".to_string(),
        "backend/agents/claudecode/footprint.rs".to_string(),
        "frontend/shell/src/data_paths.rs".to_string(),
    ]
    .into();
    assert_eq!(
        homes, want,
        "`{needle}` 在生产代码里的家对不上（多 = 第二个写者 / 第二份申报；少 = 空转）"
    );
    // 足迹申报的那条路径 == 本模块真写的那一个（家目录下 `DIR_NAME / FILE_NAME`）：改了文件名而足迹没跟 ⇒ 红。
    // 〔MG1 合 SU1〕直接从仓根拼整条路径：`test_tiers` 的 T2（根锚路径字面量要指得到盘上东西）把 `let src = repo_root().join("src")`
    // 读成「src 绑的是 repo_root」，`src.join("bridge/…")` 就被核成 `./bridge/…` 报断 —— 写成整条之后 T2 真核得到它。
    let registry = std::fs::read_to_string(
        crate::guard_support::repo_root().join("src/backend/agents/claudecode/footprint.rs"),
    )
    .unwrap();
    let declared = format!(
        "\"~/{}/{}\"",
        crate::control::exit_policy::DIR_NAME,
        FILE_NAME
    );
    assert_eq!(
        crate::guard_support::production_code(&registry)
            .matches(declared.as_str())
            .count(),
        1,
        "足迹里申报的装记录路径（{declared}）与本模块写的那一个对不上"
    );
    // 正控：同一个判法在一份塞了针的副本上数得出来
    let planted =
        crate::guard_support::production_code(&format!("const X: &str = \"{needle}\";\n"));
    assert!(planted.contains(needle.as_str()));
}

/// 🔴 〔HX2 · 第四波 4D〕L4：**别人在锁里记了一条，这一趟记的不会把它盖掉**（审计 `E-compat.md` §E6：丢了补不回来）。
///
/// 要求住址：题面 HX2 逐字「后端自有状态文件跨进程锁（`flock` 一类，Windows 对应）」；用户裁决 V116「装的时候记下写了哪些文件，卸只删这些」
/// （记录丢一条 ⇒ 那一趟装的文件卸不掉）。
/// 做法：本线程拿住那个目录的锁（与 `record_at` 拿的是同一把），另一线程记 `beta` ⇒ 限期内不许记完；本线程在锁里直接落一份
/// 只含 `alpha` 的记录文件（不经写口：写口自己也要拿锁，同一线程再拿会自锁），放锁 ⇒ 另一线程读到的是这一份 ⇒ 两条都在。
/// （刀：`record_at` 不拿锁 ⇒ 它先读到空、写下只含 `beta` 的那一份，`alpha` 被盖掉。）
#[test]
fn hx2_a_record_written_under_someone_elses_lock_is_not_overwritten() {
    let d = temp_dir("hx2-race");
    let state = d.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let file = state.join("skill-installs.json");
    let root = d.join("skills");
    let held = crate::platform::lock::hold(&state).expect("拿不到锁");
    let (tx, rx) = std::sync::mpsc::channel();
    let (f2, r2) = (file.clone(), root.clone());
    let racer = std::thread::spawn(move || {
        let v = record_at(
            &f2,
            Some(&r2),
            &json!({"op": "add", "name": "beta", "files": {"SKILL.md": rec(D2, true)}}),
        );
        tx.send(v.is_ok()).unwrap();
    });
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(400))
            .is_err(),
        "锁还在别人手里，另一趟就记完了"
    );
    let mut alpha = Ledger::default();
    add(
        &mut alpha,
        &root.join("alpha").display().to_string(),
        "alpha",
        [(
            "SKILL.md".to_string(),
            Recorded {
                digest: D1.into(),
                created: true,
            },
        )]
        .into_iter()
        .collect(),
    );
    std::fs::write(&file, serde_json::to_string(&alpha).unwrap()).unwrap();
    drop(held);
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("放了锁还没记完"),
        "另一趟记失败了"
    );
    racer.join().unwrap();
    let l = read_ok(&file);
    let names: Vec<&str> = l.installs.values().map(|i| i.name.as_str()).collect();
    assert_eq!(names.len(), 2, "有一条被盖掉了：{names:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// 〔MIG-3a · 子步 3 · 主会话 09-28 裁〕装在家目录底下的那一件记进**同一份**账：`at:"home"` ⇒ 键是本记录自己所在的那个家
/// （`<家>/.cc-monitor/<本文件>` 的上两层），不收调用方给的路径；`at` 认不得的值 ⇒ 拒、文件一个字节不动。
#[test]
fn an_install_under_home_is_keyed_by_the_home_the_ledger_lives_in() {
    let h = temp_dir("at-home");
    let path = h.join(".cc-monitor").join(FILE_NAME);
    let v = record_at(
        &path,
        None,
        &json!({ "op": "add", "name": "home-tool", "at": "home", "files": { ".local/bin/home-tool": rec(D1, true) } }),
    )
    .expect("记");
    assert_eq!(v["dir"], json!(h.display().to_string()));
    assert!(read_ok(&path)
        .installs
        .contains_key(&h.display().to_string()));
    let before = std::fs::read(&path).unwrap();
    let (code, _) = record_at(
        &path,
        None,
        &json!({ "op": "add", "name": "x", "at": "/etc", "files": {} }),
    )
    .expect_err("认不得的 at 也记了");
    assert_eq!(code, "bad_args");
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
