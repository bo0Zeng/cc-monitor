//! `asset_catalog.rs` 的判据。
//!
//! 守的要求（住址）：用户裁决 **V113**逐字「本机后端在本机看见一个skill并记录下来, 就会和远端后端同步」
//! ＋「目录自动同步，装要你点」· 没有主机器 · 后端自有状态住 `~/.cc-monitor/`、只有后端写。
//!
//! # 买到的
//!
//! - 合并规则逐格相等：同一台取 `gen` 大的整份 · 自己那一格不收 · 幂等 · 与到达顺序无关。
//! - 「这台缺什么」三态逐行相等（`missing` / `differs` / `same`，按种类 ＋ 名字）。
//! - MCP 的密钥值**零命中**于目录（正控：同一串在原文的规范写法里数得出 1），而摘要看得见它（只差密钥值 ⇒ 摘要不同）。
//! - 真变了才 `gen + 1`、才落盘；id 生一次就不变；读不懂 / 更新的格式 ⇒ 不覆盖（逐字节比对）。
//! - 目录文件名在 `src/` 全部生产代码里恰好一个家（两向相等 ＋ 正控）。
//!
//! # 买不到的
//!
//! - 两个后端**进程**同时写：读—改—写整段在目录的跨进程锁里（`platform/lock.rs`）⇒ 不再「后写的整份盖掉先写的」。
//!   本族的锁判据是两个线程各开一次描述（`flock` 锁在打开文件描述上，与两个进程同一种互斥），没有真起第二个进程。
//! - 🔴 真 Windows：`USERPROFILE` 与 `COMPUTERNAME` 那一支没在 Windows 上跑过。

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ccm-assetcat-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("建临时目录");
    p
}

fn snap(gen: u64, names: &[&str]) -> Snapshot {
    Snapshot {
        label: format!("m@{gen}"),
        gen,
        seen_at: 1000 + gen,
        assets: names
            .iter()
            .map(|n| Asset {
                kind: KIND_SKILL.into(),
                name: (*n).into(),
                project: None,
                dir: None,
                digest: format!("d-{n}"),
                summary: json!({}),
            })
            .collect(),
        project_dirs: vec![],
        notes: Default::default(),
    }
}

fn cat_with(self_id: &str, machines: &[(&str, Snapshot)]) -> Catalog {
    let mut c = fresh(self_id.into());
    for (id, s) in machines {
        c.machines.insert((*id).into(), s.clone());
    }
    c
}

#[test]
fn merge_takes_the_higher_generation_whole_and_never_the_self_slot() {
    let mut mine = cat_with(
        "me",
        &[
            ("me", snap(3, &["a"])),
            ("r1", snap(2, &["x", "y"])),
            ("r2", snap(5, &["z"])),
        ],
    );
    let incoming: BTreeMap<String, Snapshot> = [
        ("me".to_string(), snap(99, &["forged"])), // 别人传来的「我」：不收
        ("r1".to_string(), snap(3, &["x"])),       // 更新：整份替换（y 随之消失）
        ("r2".to_string(), snap(4, &["old"])),     // 更旧：不收
        ("r3".to_string(), snap(1, &["new"])),     // 没见过：收
    ]
    .into();
    assert!(merge(&mut mine, incoming), "有新东西并进来，应回 changed");
    let got: BTreeMap<String, (u64, Vec<String>)> = mine
        .machines
        .iter()
        .map(|(k, s)| {
            (
                k.clone(),
                (s.gen, s.assets.iter().map(|a| a.name.clone()).collect()),
            )
        })
        .collect();
    let want: BTreeMap<String, (u64, Vec<String>)> = [
        ("me".to_string(), (3, vec!["a".to_string()])),
        ("r1".to_string(), (3, vec!["x".to_string()])),
        ("r2".to_string(), (5, vec!["z".to_string()])),
        ("r3".to_string(), (1, vec!["new".to_string()])),
    ]
    .into();
    assert_eq!(got, want);
}

#[test]
fn merge_is_idempotent_and_does_not_depend_on_arrival_order() {
    let a: BTreeMap<String, Snapshot> = [
        ("r1".to_string(), snap(2, &["p"])),
        ("r2".to_string(), snap(1, &["q"])),
    ]
    .into();
    let b: BTreeMap<String, Snapshot> = [
        ("r1".to_string(), snap(4, &["p2"])),
        ("r3".to_string(), snap(7, &["s"])),
    ]
    .into();
    let mut x = fresh("me".into());
    merge(&mut x, a.clone());
    merge(&mut x, b.clone());
    let mut y = fresh("me".into());
    merge(&mut y, b.clone());
    merge(&mut y, a.clone());
    assert_eq!(x, y, "先 a 后 b 与先 b 后 a 必须并出同一份");
    assert!(!merge(&mut x, a), "同一份再并一次不许回 changed");
    assert!(!merge(&mut x, b));
}

#[test]
fn refresh_bumps_the_generation_only_when_the_scan_really_changed() {
    let mut c = fresh("me".into());
    let one = snap(0, &["a"]).assets;
    assert!(refresh_self(&mut c, one.clone(), vec![], "u@h", 10));
    assert_eq!((c.machines["me"].gen, c.machines["me"].seen_at), (1, 10));
    assert!(
        !refresh_self(&mut c, one.clone(), vec![], "u@h", 20),
        "没变不许升代"
    );
    assert_eq!((c.machines["me"].gen, c.machines["me"].seen_at), (1, 10));
    assert!(refresh_self(
        &mut c,
        snap(0, &["a", "b"]).assets,
        vec![],
        "u@h",
        30
    ));
    assert_eq!(c.machines["me"].gen, 2);
    assert!(
        refresh_self(&mut c, snap(0, &["a", "b"]).assets, vec![], "u@other", 40),
        "称呼变了也算变了"
    );
    assert_eq!(c.machines["me"].gen, 3);
}

#[test]
fn rows_judge_missing_differs_same_per_kind_and_name() {
    let asset = |kind: &str, name: &str, d: &str, project: Option<&str>| Asset {
        kind: kind.into(),
        name: name.into(),
        project: project.map(str::to_string),
        dir: None,
        digest: d.into(),
        summary: json!({}),
    };
    let me = Snapshot {
        label: "me".into(),
        gen: 1,
        seen_at: 0,
        assets: vec![
            asset(KIND_SKILL, "same-skill", "d1", None),
            asset(KIND_SKILL, "diff-skill", "d2", None),
            asset(KIND_MCP, "fs", "m-other", Some("/p/a")),
            asset(KIND_MCP, "fs", "m1", Some("/p/b")), // 两个项目里有同名的，一个相同 ⇒ same
            asset(KIND_MCP, "only-here", "m9", Some("/p/a")),
        ],
        project_dirs: vec![],
        notes: Default::default(),
    };
    let there = Snapshot {
        label: "r".into(),
        gen: 1,
        seen_at: 0,
        assets: vec![
            asset(KIND_SKILL, "same-skill", "d1", None),
            asset(KIND_SKILL, "diff-skill", "dX", None),
            asset(KIND_SKILL, "new-skill", "d3", None),
            asset(KIND_MCP, "fs", "m1", Some("/home/r/x")),
            asset(KIND_MCP, "gh", "m2", Some("/home/r/x")),
            // 同名不同种：不许跟 skill 那一行混
            asset(KIND_MCP, "same-skill", "zz", Some("/home/r/x")),
        ],
        project_dirs: vec![],
        notes: Default::default(),
    };
    let c = cat_with("me", &[("me", me), ("r", there)]);
    let got: Vec<(String, String, String)> = rows(&c)
        .iter()
        .map(|r| {
            (
                r["kind"].as_str().unwrap().to_string(),
                r["name"].as_str().unwrap().to_string(),
                r["state"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let want: Vec<(String, String, String)> = [
        (KIND_MCP, "fs", HERE_SAME),
        (KIND_MCP, "gh", HERE_MISSING),
        (KIND_MCP, "same-skill", HERE_MISSING),
        (KIND_SKILL, "diff-skill", HERE_DIFFERS),
        (KIND_SKILL, "new-skill", HERE_MISSING),
        (KIND_SKILL, "same-skill", HERE_SAME),
    ]
    .iter()
    .map(|(k, n, s)| (k.to_string(), n.to_string(), s.to_string()))
    .collect();
    assert_eq!(
        got, want,
        "只在这台有的（only-here）不出行；同名不同种各算各的"
    );
    // 线上态闭集 == 本模块发的那几个（两向）
    let emitted: std::collections::BTreeSet<&str> =
        want.iter().map(|(_, _, s)| s.as_str()).collect();
    let closed: std::collections::BTreeSet<&str> = HERE_STATES.iter().copied().collect();
    assert_eq!(
        emitted, closed,
        "三态闭集里有一态这组夹具没打到 —— 夹具该补"
    );
}

#[test]
fn mcp_secrets_never_enter_the_catalog_but_the_digest_sees_them() {
    let secret = "FIXTURE-SECRET-VALUE-ONE";
    let def = json!({
        "command": "npx",
        "args": ["-y", "@x/server"],
        "env": { "API_KEY": secret, "MODE": "prod" },
        "headers": { "Authorization": format!("Bearer {secret}") },
        "weird": { "token": secret },
    });
    // 正控：原文的规范写法里真有它
    assert_eq!(canonical(&def).matches(secret).count(), 3);
    let a = mcp_asset(Some("/p"), "x", &def, None);
    let on_wire = serde_json::to_string(&a).unwrap();
    assert_eq!(
        on_wire.matches(secret).count(),
        0,
        "密钥值进了目录：{on_wire}"
    );
    assert_eq!(
        a.summary,
        json!({
            "command": "npx",
            "args": ["-y", "@x/server"],
            "envKeys": ["API_KEY", "MODE"],
            "headersKeys": ["Authorization"],
            "otherKeys": ["weird"],
        })
    );
    let mut def2 = def.clone();
    def2["env"]["API_KEY"] = json!("FIXTURE-SECRET-VALUE-TWO");
    assert_ne!(
        mcp_asset(Some("/p"), "x", &def2, None).digest,
        a.digest,
        "只差密钥值也必须判得出「不同」"
    );
    // 键序无关
    let reordered: Value = serde_json::from_str(
        r#"{"weird":{"token":"FIXTURE-SECRET-VALUE-ONE"},"headers":{"Authorization":"Bearer FIXTURE-SECRET-VALUE-ONE"},"env":{"MODE":"prod","API_KEY":"FIXTURE-SECRET-VALUE-ONE"},"args":["-y","@x/server"],"command":"npx"}"#,
    )
    .unwrap();
    assert_eq!(
        mcp_asset(Some("/p"), "x", &reordered, None).digest,
        a.digest
    );
}

#[test]
fn skill_digest_sees_paths_and_content_and_the_summary_carries_no_content() {
    let d = temp_dir("skill");
    let s = d.join("demo");
    std::fs::create_dir_all(s.join("scripts")).unwrap();
    std::fs::write(
        s.join("SKILL.md"),
        "---\ndescription: x\n---\nBODY-MARKER\n",
    )
    .unwrap();
    std::fs::write(s.join("scripts/run.sh"), "echo hi\n").unwrap();
    let a = skill_asset(None, "demo", &s, Some("x"));
    assert_eq!(a.summary["files"], json!(2));
    assert_eq!(a.summary["binary"], json!(false));
    assert_eq!(
        serde_json::to_string(&a)
            .unwrap()
            .matches("BODY-MARKER")
            .count(),
        0
    );
    std::fs::write(s.join("scripts/run.sh"), "echo ho\n").unwrap();
    let b = skill_asset(None, "demo", &s, Some("x"));
    assert_ne!(a.digest, b.digest, "改一个字节，摘要必须变");
    std::fs::rename(s.join("scripts/run.sh"), s.join("scripts/go.sh")).unwrap();
    let c = skill_asset(None, "demo", &s, Some("x"));
    assert_ne!(b.digest, c.digest, "换个名字，摘要必须变");
    std::fs::write(s.join("blob.bin"), [0u8, 159, 146, 150]).unwrap();
    let with_blob = skill_asset(None, "demo", &s, None);
    assert_eq!(with_blob.summary["binary"], json!(true));
    assert_eq!(with_blob.summary["truncated"], json!(false));
    // 超上限的那一个：只按长度算，并且**说出是哪一个**（降级要说清）
    std::fs::write(
        s.join("huge.dat"),
        vec![b'x'; SKILL_MAX_FILE_BYTES as usize + 1],
    )
    .unwrap();
    let huge = skill_asset(None, "demo", &s, None);
    assert_eq!(huge.summary["truncated"], json!(true));
    let notice = huge.summary["notice"].as_array().unwrap();
    assert_eq!(notice.len(), 1, "{notice:?}");
    assert!(
        notice[0].as_str().unwrap().contains("huge.dat"),
        "{notice:?}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

fn scan_of(names: &[&str]) -> Scanned {
    Scanned {
        assets: snap(0, names).assets,
        ..Scanned::default()
    }
}

#[test]
fn update_writes_only_when_changed_and_keeps_its_id() {
    let d = temp_dir("update");
    let f = d.join(".cc-monitor").join(FILE_NAME);
    let first = update_at(&f, scan_of(&["a"]), "u@h", None).expect("第一次");
    assert_eq!(first["changed"], json!(true));
    let id = first["self"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 16);
    let bytes1 = std::fs::read(&f).unwrap();
    #[cfg(unix)]
    let ino1 = {
        use std::os::unix::fs::MetadataExt as _;
        std::fs::metadata(&f).unwrap().ino()
    };
    let again = update_at(&f, scan_of(&["a"]), "u@h", None).unwrap();
    assert_eq!(again["changed"], json!(false));
    assert_eq!(again["self"], json!(id), "id 生一次就不变");
    assert_eq!(std::fs::read(&f).unwrap(), bytes1, "没变不许写");
    // 字节相同也可能是「又整份写了一遍」：写口是换名上位，写过就换了 inode（逐字节比看不出来这一形）
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        assert_eq!(
            std::fs::metadata(&f).unwrap().ino(),
            ino1,
            "没变不许写（inode 换了 = 又写了一遍）"
        );
    }
    let inc: BTreeMap<String, Snapshot> = [("r".to_string(), snap(1, &["z"]))].into();
    let merged = update_at(&f, scan_of(&["a"]), "u@h", Some(inc)).unwrap();
    assert_eq!(merged["changed"], json!(true));
    assert_eq!(merged["rows"][0]["name"], json!("z"));
    assert_eq!(merged["rows"][0]["state"], json!(HERE_MISSING));
    // 临时文件没留下（按它的名字问一次，不遍历目录）
    let tmp = f
        .parent()
        .unwrap()
        .join(format!("{FILE_NAME}.{}.tmp", std::process::id()));
    assert!(!tmp.exists(), "临时文件留下了：{}", tmp.display());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_unreadable_or_newer_catalog_is_never_overwritten() {
    let d = temp_dir("unreadable");
    let f = d.join(FILE_NAME);
    for body in [
        "{not json".to_string(),
        serde_json::to_string(&json!({"v": 2, "self": "x", "machines": {}})).unwrap(),
    ] {
        std::fs::write(&f, &body).unwrap();
        let e = update_at(&f, scan_of(&["a"]), "u@h", None).expect_err("读不懂也写了");
        assert_eq!(e.0, "catalog_unreadable");
        assert_eq!(
            std::fs::read_to_string(&f).unwrap(),
            body,
            "读不懂的那份被改了"
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_wire_shape_feeds_straight_back_into_merge() {
    let c = cat_with(
        "me",
        &[("me", snap(2, &["a"])), ("r", snap(5, &["b", "c"]))],
    );
    let w = wire(&c, &[], false, None);
    assert_eq!(machines_from_wire(&w).expect("线上形状读回"), c.machines);
}

#[test]
fn a_malformed_incoming_catalog_is_refused_not_guessed() {
    let good = wire(&cat_with("me", &[("r", snap(1, &["a"]))]), &[], false, None);
    assert!(machines_from_wire(&good).is_ok());
    for (path, bad) in [
        ("machines", json!("nope")),
        ("gen", json!(-1)),
        ("seenAt", Value::Null),
        ("id", json!("")),
        ("label", json!(3)),
        ("kind", json!("plugin")),
    ] {
        let mut w = good.clone();
        match path {
            "machines" => w["machines"] = bad,
            "kind" => w["machines"][0]["assets"][0]["kind"] = bad,
            k => w["machines"][0][k] = bad,
        }
        assert!(machines_from_wire(&w).is_err(), "坏了 `{path}` 仍被收下");
    }
}

/// 目录文件名在 `src/` 全部生产代码里**谁叫得出它**（字面量或契约常量名）—— 两向相等 ＋ 本模块自己必须命中 = 正控。
/// 字面量挪进契约 crate（`relay_route_core::ASSET_CATALOG_REL`）：定义那一处 · 写者（本模块）·
/// monitor 数据位置页（只 stat、不读写）三个家；第四个 ⇒ 红（第二个写者冒出来了）。
#[test]
fn the_file_name_has_exactly_one_home_in_all_production_code() {
    let src = crate::guard_support::repo_root().join("src");
    let needle = format!("{}-{}.json", "assets", "catalog");
    let via_const = format!("{}_{}_REL", "ASSET", "CATALOG");
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
    let want: std::collections::BTreeSet<String> = [
        "common/relay-route-core/src/lib.rs".to_string(),
        "backend/assets/asset_catalog.rs".to_string(),
        "frontend/shell/src/data_paths.rs".to_string(),
    ]
    .into();
    assert_eq!(
        homes, want,
        "`{needle}` 在生产代码里的家对不上（多 = 第二个写者；少 = 空转）"
    );
}

/// 🔴 L3：**资产目录首建时，第二个写者读到第一个写下的那份、不再生第二个 `self` id**（审计 E14a 幽灵机器）。
///
/// 要求住址：题面 HX2 逐字「后端自有状态文件跨进程锁（`flock` 一类，Windows 对应）」；「机器身份：每台后端第一次记目录时
/// 生成一个 id … 之后不变」。
/// 做法：本线程拿住那个目录的锁（`platform/lock.rs`，与 `update_at` 拿的是同一把），另一线程对一份还不存在的目录文件跑 `update_at`
/// ⇒ 限期内它不许落盘；本线程在锁里写下一份 `self = "first"` 的目录再放锁 ⇒ 它读到的是那一份，回的 `self` == `"first"`。
/// （刀：`update_at` 不拿锁 ⇒ 它先读到「没有」、生一个新 id 回来。）
#[test]
fn hx2_the_first_catalog_is_born_once_even_when_two_writers_race() {
    let dir = temp_dir("hx2-race");
    let path = dir.join(FILE_NAME);
    let held = crate::platform::lock::hold(&dir).expect("拿不到锁");
    let (tx, rx) = std::sync::mpsc::channel();
    let p2 = path.clone();
    let racer = std::thread::spawn(move || {
        let v = update_at(&p2, Scanned::default(), "racer@x", None).expect("另一个写者失败");
        tx.send(v).unwrap();
    });
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(400))
            .is_err(),
        "锁还在别人手里，另一个写者就读完写完了"
    );
    assert!(!path.exists(), "锁还在别人手里，目录文件就被写出来了");
    let first = fresh("first".into());
    std::fs::write(&path, serde_json::to_string(&first).unwrap()).unwrap();
    drop(held);
    let v = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("放了锁，另一个写者还是没做完");
    racer.join().unwrap();
    assert_eq!(
        v["self"], "first",
        "第二个写者生了自己的 id —— 幽灵机器那一形"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
