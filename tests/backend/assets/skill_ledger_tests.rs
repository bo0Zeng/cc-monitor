//! `skill_ledger.rs`（skill 装记录，第四层）的判据：记得对、并得对、摘得对、读不懂不覆盖、只有一个家。
//!
//! 守的要求：「装的时候记下写了哪些文件，卸只删这些（装完用户自己改过的先问）」；
//! （记什么 · 键是目录 · `created` 取第一次的）· `readonly_guard` 第四层（后端自有状态、全仓一个写口）。
//!
//! 买到：临时目录上真读真写 —— add 并进去的逐格相等（新路径加 · 旧路径换摘要 · `created` 取第一次）· drop 逐格相等（剩零整条摘）·
//! 目录由记录那一侧按 `skills 根 / name` 自己算（调用方给的路径不收）· 坏入参拒且文件逐字节不变 · 读不懂 / 另一版本的不覆盖 ·
//! 没变不写（逐字节 ＋ inode 不换）· 文件名在全部生产代码里恰好一个家（两向 ＋ 正控）。
//! 买不到：两个后端进程同时写 —— 今天读—改—写在目录的跨进程锁里（`platform/lock.rs`），
//! 本族那一条用两个线程各开一次描述量（与两个进程同一种 `flock` 互斥），没有真起第二个进程 · 真 Windows 上 `rename` 覆盖既有文件。

use super::*;
use serde_json::json;

/// 入参仍用 JSON 写：经 `typed::take` 解成入参类型（同帧面那一跳；形状不对 ⇒ `bad_args`），回包写成 JSON 比。
fn record_at(
    own: &Own,
    root: Option<&Path>,
    a: &Value,
) -> Result<Value, crate::common::said::Said> {
    let req = crate::wire::typed::take(a)?;
    super::record_at(
        own,
        crate::assets::skill_install::Roots::here_with(root),
        &req,
    )
    .map(|r| serde_json::to_value(r).unwrap())
}

/// 写句柄：家目录 ＝ 那份装记录上两级（`<家>/.cc-monitor/skill-installs.json`）。
fn own_at(file: &std::path::Path) -> crate::platform::fs::Own {
    crate::platform::fs::Own::issue_at(
        file.parent()
            .and_then(std::path::Path::parent)
            .expect("两级"),
    )
}

fn rec(digest: &str, created: bool) -> Value {
    json!({ "digest": digest, "created": created })
}

const D1: &str = "0000000000000001";
const D2: &str = "0000000000000002";
const D3: &str = "0000000000000003";

fn read_ok(path: &Path) -> Ledger {
    match read_at(path) {
        Read::Present(l) => l,
        other => panic!("读不回来：{other:?}"),
    }
}

#[test]
fn add_merges_new_paths_updates_digests_and_keeps_the_first_created() {
    let d = test_home::scratch("add");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let root = d.join("skills");
    let first = record_at(
        &own_at(&file),
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
        &own_at(&file),
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
    let d = test_home::scratch("dir");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let root = d.join("skills");
    // 调用方给的 `dir` 不收：`add` 里没有这一格 ⇒ `bad_args`、一个字节不写。
    let e = record_at(
        &own_at(&file),
        Some(&root),
        &json!({"op": "add", "name": "demo", "dir": "/etc", "files": {"a.md": rec(D1, true)}}),
    )
    .expect_err("收了调用方给的 dir");
    assert_eq!(e.code.as_deref(), Some("bad_args"));
    assert!(!file.exists(), "拒了还写了");
    let v = record_at(
        &own_at(&file),
        Some(&root),
        &json!({"op": "add", "name": "demo", "files": {"a.md": rec(D1, true)}}),
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
    let d = test_home::scratch("drop");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let root = d.join("skills");
    record_at(
        &own_at(&file),
        Some(&root),
        &json!({"op": "add", "name": "demo", "files": {"a": rec(D1, true), "b": rec(D2, true), "c": rec(D3, false)}}),
    )
    .unwrap();
    let dir = root.join("demo").display().to_string();
    let v = record_at(
        &own_at(&file),
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
        &own_at(&file),
        None,
        &json!({"op": "drop", "dir": dir, "paths": ["b", "zzz"]}),
    )
    .expect_err("摘一个没记的");
    assert_eq!(e.code.as_deref(), Some("bad_args"));
    assert_eq!(std::fs::read(&file).unwrap(), before, "拒了还动了文件");
    let e = record_at(
        &own_at(&file),
        None,
        &json!({"op": "drop", "dir": "/nowhere", "paths": ["b"]}),
    )
    .expect_err("没记着的目录");
    assert_eq!(e.code.as_deref(), Some("not_found"));
    // 摘到零 ⇒ 整条走
    let v = record_at(
        &own_at(&file),
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
    let d = test_home::scratch("bad");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let root = d.join("skills");
    record_at(
        &own_at(&file),
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
        let e = record_at(&own_at(&file), Some(&root), &bad).expect_err("坏入参被收下");
        assert_eq!(e.code.as_deref(), Some("bad_args"), "{bad}");
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
    let d = test_home::scratch("unreadable");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let root = d.join("skills");
    for body in [
        b"{not json".to_vec(),
        // 形状本身读得懂、只是版本比这一版新 ⇒ 仍然不认（别让 `deny_unknown_fields` 替版本闸挡了这一形）
        br#"{"version":4,"installs":{}}"#.to_vec(),
    ] {
        std::fs::write(&file, &body).unwrap();
        let e = record_at(
            &own_at(&file),
            Some(&root),
            &json!({"op": "add", "name": "demo", "files": {"a": rec(D1, true)}}),
        )
        .expect_err("读不懂的被当成空的覆盖了");
        assert_eq!(e.code.as_deref(), Some("ledger_unreadable"));
        assert_eq!(std::fs::read(&file).unwrap(), body, "读不懂的那份被动了");
        assert!(matches!(load_at(&file), Err(e) if e.code.as_deref() == Some("ledger_unreadable")));
    }
    std::fs::remove_file(&file).unwrap();
    assert_eq!(load_at(&file).unwrap(), Ledger::default(), "没有 ⇒ 空的");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn nothing_changed_nothing_written() {
    let d = test_home::scratch("same");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let root = d.join("skills");
    let args = json!({"op": "add", "name": "demo", "files": {"a": rec(D1, true)}});
    record_at(&own_at(&file), Some(&root), &args).unwrap();
    let before = std::fs::read(&file).unwrap();
    #[cfg(unix)]
    let ino = {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::metadata(&file).unwrap().ino()
    };
    let v = record_at(&own_at(&file), Some(&root), &args).unwrap();
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
    assert_eq!(
        crate::common::digest::fnv_digest("a\n"),
        crate::common::digest::fnv_digest("a\n")
    );
    assert_ne!(
        crate::common::digest::fnv_digest("a\n"),
        crate::common::digest::fnv_digest("a\r\n")
    );
    assert_ne!(
        crate::common::digest::fnv_digest(""),
        crate::common::digest::fnv_digest(" ")
    );
    assert!(valid_digest(&crate::common::digest::fnv_digest("x")));
}

/// 文件名在 `src/` 全部生产代码里恰好一个家（第二个写者 / 空转两向都红）。正控：把针塞进一份副本里数得出来。
#[test]
fn the_file_name_has_exactly_one_home_in_all_production_code() {
    let src = guard_core::addr::repo_root().join("src");
    let needle = format!("{}-{}.json", "skill", "installs");
    // 字面量挪进契约 crate ⇒ 按「字面量或契约常量名」认谁叫得出它。
    let via_const = format!("{}_{}_REL", "SKILL", "LEDGER");
    // 足迹申报照布局宏拼（`concat!` 只认字面量）⇒ 宏名也算叫得出它。
    let via_macro = format!("{}_{}_rel!", "skill", "ledger");
    let mut scanned = 0usize;
    let mut homes: std::collections::BTreeSet<String> = Default::default();
    for (path, body) in guard_core::scan_tree_excluding(&src, &["rs", "ts", "sh"], &[]) {
        scanned += 1;
        let code = match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => guard_core::production_code(&body),
            Some("ts") => guard_core::strip_comment_lines(&body),
            _ => guard_core::strip_hash_comment_lines(&body),
        };
        if code.contains(needle.as_str())
            || code.contains(via_const.as_str())
            || code.contains(via_macro.as_str())
        {
            homes.insert(
                path.strip_prefix(&src)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    assert!(scanned > 500, "只扫到 {scanned} 份源码 —— 遍历坏了");
    // 三个家、各一个身份：定义（契约 crate）· 写者（本模块）· 足迹里的**申报字面量**（`skill-install` 那一格，随 Claude 布局那一半住
    // `agents/claudecode/footprint.rs`；不读不写 ——「足迹里看得见」）。数据页那一行由后端照契约那张表列（`OWN_HOME_ENTRIES`，同一个家）。
    // 第四个家 ⇒ 红（第二个写者或者第二份申报）。
    let want: std::collections::BTreeSet<String> = [
        "common/relay-route-core/src/lib.rs".to_string(),
        "backend/assets/skill_ledger.rs".to_string(),
        "backend/agents/claudecode/footprint.rs".to_string(),
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
        guard_core::addr::repo_root().join("src/backend/agents/claudecode/footprint.rs"),
    )
    .unwrap();
    // 申报照布局宏拼（字面量只在契约 crate 那个宏里）⇒ 数这一形，再核宏给的那一段就是本模块写的那一个。
    assert_eq!(
        relay_route_core::SKILL_LEDGER_REL,
        format!("{}/{}", crate::common::own_dir::DIR_NAME, FILE_NAME),
        "契约里的装记录路径与本模块写的那一个对不上"
    );
    let declared = "concat!(\"~/\", relay_route_core::skill_ledger_rel!())";
    assert_eq!(
        guard_core::production_code(&registry)
            .matches(declared)
            .count(),
        1,
        "足迹里申报的装记录路径（{declared}）与本模块写的那一个对不上"
    );
    // 正控：同一个判法在一份塞了针的副本上数得出来
    let planted = guard_core::production_code(&format!("const X: &str = \"{needle}\";\n"));
    assert!(planted.contains(needle.as_str()));
}

/// 🔴 L4：**别人在锁里记了一条，这一趟记的不会把它盖掉**（丢了补不回来）。
///
/// 要求：「后端自有状态文件跨进程锁（`flock` 一类，Windows 对应）」；「装的时候记下写了哪些文件，卸只删这些」
/// （记录丢一条 ⇒ 那一趟装的文件卸不掉）。
/// 做法：本线程拿住那个目录的锁（与 `record_at` 拿的是同一把），另一线程记 `beta` ⇒ 限期内不许记完；本线程在锁里直接落一份
/// 只含 `alpha` 的记录文件（不经写口：写口自己也要拿锁，同一线程再拿会自锁），放锁 ⇒ 另一线程读到的是这一份 ⇒ 两条都在。
/// （刀：`record_at` 不拿锁 ⇒ 它先读到空、写下只含 `beta` 的那一份，`alpha` 被盖掉。）
#[test]
fn hx2_a_record_written_under_someone_elses_lock_is_not_overwritten() {
    let d = test_home::scratch("hx2-race");
    let state = d.join(".cc-monitor");
    std::fs::create_dir_all(&state).unwrap();
    let file = state.join("skill-installs.json");
    let root = d.join("skills");
    let held = crate::platform::lock::hold(&state).expect("拿不到锁");
    let (tx, rx) = std::sync::mpsc::channel();
    let (f2, r2) = (file.clone(), root.clone());
    let racer = std::thread::spawn(move || {
        let v = record_at(
            &own_at(&f2),
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
    let mut alpha = serde_json::to_value(&alpha).unwrap();
    alpha["version"] = json!(3);
    std::fs::write(&file, alpha.to_string()).unwrap();
    drop(held);
    assert!(
        rx.recv_timeout(guard_core::wall::CEILING)
            .expect("放了锁还没记完"),
        "另一趟记失败了"
    );
    racer.join().unwrap();
    let l = read_ok(&file);
    let names: Vec<&str> = l.installs.values().map(|i| i.name.as_str()).collect();
    assert_eq!(names.len(), 2, "有一条被盖掉了：{names:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// 第 1 版（版本写在 `"v"` 里）· 第 2 版（没有 `touched`）经升级步读成这一版、原来那几条都在；读本身不动盘。
/// 常驻起来那一下（[`bring_forward`]）写回成第 3 版、原文逐字节留在 `.v<N>`；已是第 3 版 ⇒ 不再写。
#[test]
fn older_ledgers_read_upgraded_and_are_brought_forward_once() {
    let d = test_home::scratch("v1-v2");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let old_dir = d.join("skills").join("old").display().to_string();
    let installs = json!({old_dir.as_str(): {"name": "old", "files": {"a": rec(D1, true)}}});
    for (n, body) in [
        (1, json!({"v": 1, "installs": installs.clone()})),
        (2, json!({"version": 2, "installs": installs.clone()})),
    ] {
        let raw = serde_json::to_vec(&body).unwrap();
        let kept = file.with_file_name(format!("skill-installs.json.v{n}"));
        let _ = std::fs::remove_file(&kept);
        std::fs::write(&file, &raw).unwrap();
        assert!(
            load_at(&file).unwrap().installs.contains_key(&old_dir),
            "第 {n} 版那一条没读进来"
        );
        assert_eq!(std::fs::read(&file).unwrap(), raw, "读把盘写了");
        assert!(bring_forward(&own_at(&file)).unwrap(), "第 {n} 版 ⇒ 写回");
        let on_disk: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(on_disk["version"], json!(3), "{on_disk}");
        assert!(on_disk.get("v").is_none(), "{on_disk}");
        assert!(on_disk["installs"].get(&old_dir).is_some(), "{on_disk}");
        assert_eq!(std::fs::read(&kept).unwrap(), raw, "第 {n} 版的原文没留下");
        let now = std::fs::read(&file).unwrap();
        assert!(
            !bring_forward(&own_at(&file)).unwrap(),
            "已是第 3 版 ⇒ 不写"
        );
        assert_eq!(std::fs::read(&file).unwrap(), now);
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// 改过的项目级 MCP 配置：头一回记下改之前的原文、接得上的再改只换 `after` 并进名字（装 · 卸都记）；
/// 接不上（这一回改之前那一份不是上回写进去的：两回之间手改过）⇒ 留底换成这一回的、名字只留这一回的；
/// 撤回整格摘掉（连装过那一格）；`after` 不是摘要 ⇒ `bad_args`、一个字节不动。
#[test]
fn a_touched_project_mcp_keeps_the_first_original_until_reverted() {
    let d = test_home::scratch("touched");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let cfg = d.join("proj/.mcp.json").display().to_string();
    let own = own_at(&file);
    let mid = crate::common::digest::fnv_digest("mid");
    let first = record_at(
        &own,
        None,
        &json!({"op": "mcp-add", "file": cfg, "name": "a", "digest": D1, "before": "orig", "after": mid}),
    )
    .expect("记");
    assert!(first["changed"].as_bool().unwrap());
    record_at(
        &own,
        None,
        &json!({"op": "mcp-drop", "file": cfg, "name": "b", "before": "mid", "after": D3}),
    )
    .expect("记");
    let l = read_ok(&file);
    let t = &l.touched[&cfg];
    assert_eq!(t.before.as_deref(), Some("orig"), "留底换成了后一次的");
    assert_eq!(t.after, D3);
    assert_eq!(
        t.names.iter().map(String::as_str).collect::<Vec<_>>(),
        ["a", "b"]
    );
    // 两回之间手改过：这一回改之前那一份不是上回写进去的 ⇒ 另起。
    record_at(
        &own,
        None,
        &json!({"op": "mcp-drop", "file": cfg, "name": "c", "before": "hand", "after": D2}),
    )
    .expect("记");
    let t = &read_ok(&file).touched[&cfg];
    assert_eq!(t.before.as_deref(), Some("hand"), "留底没换成手改的那一份");
    assert_eq!(t.after, D2);
    assert_eq!(
        t.names.iter().map(String::as_str).collect::<Vec<_>>(),
        ["c"]
    );
    let before = std::fs::read(&file).unwrap();
    let e = record_at(
        &own,
        None,
        &json!({"op": "mcp-add", "file": cfg, "name": "a", "digest": D1, "before": null, "after": "x"}),
    )
    .expect_err("坏摘要也收了");
    assert_eq!(e.code.as_deref(), Some("bad_args"));
    assert_eq!(std::fs::read(&file).unwrap(), before);
    record_at(
        &own,
        None,
        &json!({"op": "mcp-revert", "file": cfg, "after": D2}),
    )
    .expect("摘");
    // 撤回回到手改的那一份：那时已装着的 a（头一条链装的）照旧记着，改过那一格摘掉。
    let l = read_ok(&file);
    assert!(l.touched.is_empty(), "撤回后改过那一格还在：{l:?}");
    assert_eq!(
        l.mcp
            .get(&cfg)
            .map(|m| m.keys().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["a"]),
        "{l:?}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 撤回摘记录在锁里核「最后写进去的摘要」：对不上（之间又有一回写）⇒ `stale`、一个字节不动；对得上才摘。
#[test]
fn a_revert_drops_the_record_only_when_the_last_written_digest_still_matches() {
    let d = test_home::scratch("revert-cas");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let cfg = d.join("proj/.mcp.json").display().to_string();
    let own = own_at(&file);
    record_at(
        &own,
        None,
        &json!({"op": "mcp-add", "file": cfg, "name": "a", "digest": D1, "before": "orig", "after": D2}),
    )
    .expect("记");
    let kept = std::fs::read(&file).unwrap();
    let e = record_at(
        &own,
        None,
        &json!({"op": "mcp-revert", "file": cfg, "after": D3}),
    )
    .expect_err("摘要对不上也摘了");
    assert_eq!(e.code.as_deref(), Some("stale"));
    assert_eq!(std::fs::read(&file).unwrap(), kept, "对不上还动了记录");
    record_at(
        &own,
        None,
        &json!({"op": "mcp-revert", "file": cfg, "after": D2}),
    )
    .expect("摘");
    let l = read_ok(&file);
    assert!(
        !l.touched.contains_key(&cfg) && !l.mcp.contains_key(&cfg),
        "{l:?}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// 撤回只摘这一回改动带进来的：头一回改之前就装着的那几条（装记录里记过）留着，以后卸它们不当外来的。
#[test]
fn a_revert_keeps_what_was_installed_before_the_first_change() {
    let d = test_home::scratch("revert-keep");
    let file = d.join(relay_route_core::SKILL_LEDGER_REL);
    let cfg = d.join("proj/.mcp.json").display().to_string();
    let own = own_at(&file);
    let first = crate::common::digest::fnv_digest("v1");
    record_at(
        &own,
        None,
        &json!({"op": "mcp-add", "file": cfg, "name": "old", "digest": D1, "before": null, "after": first}),
    )
    .expect("记");
    // 那一份被手改（另起一条链）之后，cc-monitor 再装一条 new。
    record_at(
        &own,
        None,
        &json!({"op": "mcp-add", "file": cfg, "name": "new", "digest": D3, "before": "hand", "after": D2}),
    )
    .expect("记");
    record_at(
        &own,
        None,
        &json!({"op": "mcp-revert", "file": cfg, "after": D2}),
    )
    .expect("摘");
    let l = read_ok(&file);
    assert_eq!(
        l.mcp
            .get(&cfg)
            .map(|m| m.keys().map(String::as_str).collect::<Vec<_>>()),
        Some(vec!["old"]),
        "头一回改之前就装着的那条被摘了：{l:?}"
    );
    assert!(!l.touched.contains_key(&cfg));
    let _ = std::fs::remove_dir_all(&d);
}
