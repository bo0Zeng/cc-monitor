//! **后端自有状态文件的读三态** —— 判据（原子写那一半的判据随本体住 `platform/fs/own.rs` 的测试）。
//!
//! | # | 判据 | 形状 |
//! |---|---|---|
//! | S1 | 不在 ⇒ `Absent`；是目录 / 超上限 ⇒ `Unreadable`；在且读得懂 ⇒ `Present` | 真文件 |
//! | S6 | 状态读不懂（坏 JSON · 超上限 · 是目录 · 解码拒）⇒ `Unreadable`；缓存同样几样 ⇒ `Absent`；三种读法都一样 | 真文件 |
//! | S7 | 流式读：读得懂 ⇒ `Present`；不在 ⇒ `Absent` | 真文件 |

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

/// S5：读不出的那一句只带路径与原因词，下层原话（解析器 · 系统）进 `raw`（复制详情）。
#[test]
fn s5_the_sentence_carries_the_reason_word_and_the_raw_words_go_to_the_detail() {
    let base = sandbox("s5");
    let p = base.join("x.json");
    std::fs::write(&p, b"{").expect("seed");
    let Read::Unreadable(s) = read_json::<serde_json::Value>(&p, 64) else {
        panic!("坏 JSON 没读成读不出");
    };
    let shown = p.display().to_string();
    assert_eq!(
        s.said,
        copy_text("beOwnState.read.notJson", &[("path", &shown)])
    );
    assert!(
        s.raw.as_deref().is_some_and(|r| r.contains("EOF")),
        "解析器原话没进详情：{s:?}"
    );
    std::fs::remove_dir_all(&base).ok();
}

// 读三态那几条不看版本：用不带版本的两行。
const STATE: Doc<serde_json::Value> = Doc::of(
    &relay_route_core::Decl::<relay_route_core::State>::foreign(".cc-monitor/x.json", 16),
);
const CACHE: Doc<serde_json::Value> = Doc::of(
    &relay_route_core::Decl::<relay_route_core::Cache>::foreign(".cc-monitor/x.json", 16),
);

fn stream_json(r: &mut dyn std::io::Read) -> Result<serde_json::Value, Said> {
    serde_json::from_reader(r).map_err(|e| Said::with_raw("bad".to_string(), e))
}

fn bytes_json(b: Vec<u8>) -> Result<serde_json::Value, Said> {
    serde_json::from_slice(&b).map_err(|e| Said::with_raw("bad".to_string(), e))
}

/// 三种读法各读一遍：（整份 JSON, 自己解, 流式）。
fn read_three(d: &Doc<serde_json::Value>, p: &Path) -> [Read<serde_json::Value>; 3] {
    [
        d.read(p),
        d.read_with(p, bytes_json),
        d.read_stream(p, stream_json),
    ]
}

#[test]
fn s6_state_unreadable_stays_unreadable_and_cache_unreadable_is_absent() {
    let base = sandbox("s6");
    let p = base.join("x.json");
    let seeds: [(&str, Box<dyn Fn()>); 3] = [
        (
            "坏 JSON",
            Box::new(|| std::fs::write(&p, b"{").expect("seed")),
        ),
        (
            "超上限",
            Box::new(|| std::fs::write(&p, vec![b' '; 17]).expect("seed")),
        ),
        (
            "落点是个目录",
            Box::new(|| std::fs::create_dir(&p).expect("dir")),
        ),
    ];
    for (what, seed) in &seeds {
        std::fs::remove_dir(&p).ok();
        std::fs::remove_file(&p).ok();
        seed();
        for r in read_three(&STATE, &p) {
            assert!(matches!(r, Read::Unreadable(_)), "状态 · {what}：{r:?}");
        }
        for r in read_three(&CACHE, &p) {
            assert!(matches!(r, Read::Absent), "缓存 · {what}：{r:?}");
        }
    }
    std::fs::remove_dir(&p).ok();
    std::fs::write(&p, b"1").expect("seed");
    let refuse = |_: Vec<u8>| -> Result<serde_json::Value, Said> { Err("no".to_string().into()) };
    assert!(matches!(STATE.read_with(&p, refuse), Read::Unreadable(_)));
    assert!(matches!(CACHE.read_with(&p, refuse), Read::Absent));
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn s7_streamed_read_reads_and_absent_is_absent() {
    let base = sandbox("s7");
    let p = base.join("x.json");
    for d in [&STATE, &CACHE] {
        assert!(matches!(d.read_stream(&p, stream_json), Read::Absent));
    }
    std::fs::write(&p, b"{\"a\":1}").expect("seed");
    for d in [&STATE, &CACHE] {
        for r in read_three(d, &p) {
            match r {
                Read::Present(v) => assert_eq!(v["a"], 1),
                other => panic!("读得懂的没读出来：{other:?}"),
            }
        }
    }
    std::fs::remove_dir_all(&base).ok();
}

// ── 版本：声明从表取（`Doc::of`），读写口自己管 `version` ──

const V_STATE: relay_route_core::Decl<relay_route_core::State> =
    relay_route_core::Decl::<relay_route_core::State>::json(".cc-monitor/v.json", 2, 2, 1024);
const V_CACHE: relay_route_core::Decl<relay_route_core::Cache> =
    relay_route_core::Decl::<relay_route_core::Cache>::json(".cc-monitor/v.json", 2, 1024);
const V_UP: relay_route_core::Decl<relay_route_core::State> =
    relay_route_core::Decl::<relay_route_core::State>::json(".cc-monitor/v.json", 3, 1, 1024);

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Shape {
    a: u32,
}

const STATE_V: Doc<Shape> = Doc::of(&V_STATE);
const CACHE_V: Doc<Shape> = Doc::of(&V_CACHE);

/// 1 → 2：`x` 改名 `a`。
fn one_to_two(mut v: serde_json::Value) -> Result<serde_json::Value, Said> {
    let x = v
        .as_object_mut()
        .and_then(|o| o.remove("x"))
        .ok_or_else(|| Said::from("no x".to_string()))?;
    v["a"] = x;
    Ok(v)
}

/// 2 → 3：`a` 翻倍。
fn two_to_three(mut v: serde_json::Value) -> Result<serde_json::Value, Said> {
    let a = v["a"]
        .as_u64()
        .ok_or_else(|| Said::from("no a".to_string()))?;
    v["a"] = serde_json::json!(a * 2);
    Ok(v)
}

const UP: Doc<Shape> = upgrade!(V_UP, [one_to_two, two_to_three]);

fn own_in(base: &Path) -> (crate::platform::fs::Own, crate::platform::fs::OwnPath) {
    let (data, home) = (
        base.join("data").to_string_lossy().into_owned(),
        base.to_string_lossy().into_owned(),
    );
    std::fs::create_dir_all(base.join("data")).expect("data");
    let own = crate::platform::fs::Own::issue_with(&|k| match k {
        creds_core::store::DATA_DIR_ENV => Some(data.clone()),
        "HOME" => Some(home.clone()),
        _ => None,
    });
    let dir = own.path("v").expect("dir");
    own.ensure_dir(&dir).expect("mkdir");
    (own, dir)
}

/// V1：状态文件的 `version` 比声明的新 ⇒ `Unreadable`、盘上字节不变；缓存同一份 ⇒ 当没有。
#[test]
fn v1_a_newer_state_is_unreadable_and_untouched_and_a_newer_cache_is_absent() {
    let base = sandbox("v1");
    let p = base.join("v.json");
    let seed = b"{\"version\":3,\"a\":1}\n";
    std::fs::write(&p, seed).expect("seed");
    assert!(
        matches!(STATE_V.read(&p), Read::Unreadable(_)),
        "新版状态被读成了别的"
    );
    assert!(matches!(CACHE_V.read(&p), Read::Absent), "新版缓存没当没有");
    assert_eq!(
        std::fs::read(&p).expect("read back"),
        seed,
        "盘上字节被动了"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// V2：缺 `version` 当 1；读出时 `version` 那一格摘掉再解（`deny_unknown_fields` 的形也读得进）。
#[test]
fn v2_a_missing_version_is_one_and_the_version_cell_is_not_handed_to_the_shape() {
    let base = sandbox("v2");
    let p = base.join("v.json");
    let one: Doc<Shape> = Doc::of(&relay_route_core::Decl::<relay_route_core::State>::json(
        "x", 1, 1, 1024,
    ));
    std::fs::write(&p, b"{\"a\":7}").expect("seed");
    assert!(matches!(one.read(&p), Read::Present(Shape { a: 7 })));
    std::fs::write(&p, b"{\"version\":1,\"a\":7}").expect("seed");
    assert!(matches!(one.read(&p), Read::Present(Shape { a: 7 })));
    std::fs::write(&p, b"{\"version\":\"1\",\"a\":7}").expect("seed");
    assert!(
        matches!(one.read(&p), Read::Unreadable(_)),
        "版本不是整数也读进来了"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// V3：在下限与版本之间 ⇒ 依次过升级步（从盘上那一版起）。
#[test]
fn v3_upgrades_run_in_order_from_the_version_on_disk() {
    let base = sandbox("v3");
    let p = base.join("v.json");
    std::fs::write(&p, b"{\"x\":5}").expect("seed");
    assert!(
        matches!(UP.read(&p), Read::Present(Shape { a: 10 })),
        "{:?}",
        UP.read(&p)
    );
    std::fs::write(&p, b"{\"version\":2,\"a\":5}").expect("seed");
    assert!(matches!(UP.read(&p), Read::Present(Shape { a: 10 })));
    std::fs::write(&p, b"{\"version\":3,\"a\":5}").expect("seed");
    assert!(matches!(UP.read(&p), Read::Present(Shape { a: 5 })));
    std::fs::write(&p, b"{\"version\":1,\"y\":5}").expect("seed");
    assert!(
        matches!(UP.read(&p), Read::Unreadable(_)),
        "升级步拒了却读成了别的"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// V4：比下限老 ⇒ 当没有（状态缓存都是），盘上字节不变。
#[test]
fn v4_older_than_the_floor_is_absent() {
    let base = sandbox("v4");
    let p = base.join("v.json");
    std::fs::write(&p, b"{\"version\":1,\"a\":1}").expect("seed");
    assert!(matches!(STATE_V.read(&p), Read::Absent));
    assert!(matches!(CACHE_V.read(&p), Read::Absent));
    assert_eq!(
        std::fs::read(&p).expect("read back"),
        b"{\"version\":1,\"a\":1}"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// V5：写那一口自动带 `version`；盖新版状态 ⇒ 拒、字节不变；盖旧版 ⇒ 原文先留成 `<名>.v<N>`（已有不动）。
#[test]
fn v5_writes_carry_the_version_refuse_a_newer_state_and_keep_an_older_one() {
    let base = sandbox("v5");
    let (own, dir) = own_in(&base);
    let held = own.lock(&dir).expect("lock");
    let p = dir.as_path().join("v.json");
    STATE_V
        .write(&own, &dir, &held, &Shape { a: 1 })
        .expect("fresh write");
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&p).expect("read")).expect("json");
    assert_eq!(v, serde_json::json!({"version": 2, "a": 1}));

    std::fs::write(&p, b"{\"version\":9,\"a\":1}").expect("seed");
    assert!(
        STATE_V.write(&own, &dir, &held, &Shape { a: 2 }).is_err(),
        "盖了新版状态"
    );
    assert_eq!(std::fs::read(&p).expect("read"), b"{\"version\":9,\"a\":1}");
    CACHE_V
        .write(&own, &dir, &held, &Shape { a: 2 })
        .expect("新版缓存照盖");

    let old = b"{\"x\":4}";
    std::fs::write(&p, old).expect("seed");
    UP.write(&own, &dir, &held, &Shape { a: 3 })
        .expect("over old");
    assert_eq!(
        std::fs::read(dir.as_path().join("v.json.v1")).expect("kept"),
        old
    );
    std::fs::write(&p, b"{\"x\":8}").expect("seed again");
    UP.write(&own, &dir, &held, &Shape { a: 3 })
        .expect("over old again");
    assert_eq!(
        std::fs::read(dir.as_path().join("v.json.v1")).expect("kept"),
        old,
        "已有的旧版被盖了"
    );
    std::fs::remove_dir_all(&base).ok();
}

/// V6：盘上那份状态在却读不懂（不是 JSON · `version` 不是整数 · 超上限 · 是目录）⇒ 写回错，那句与读那一侧同一句；
/// 盘上原样、不留 `.v<N>`。缓存同几样 ⇒ 照写（缓存读不懂本来就当没有）。
#[test]
fn v6_an_unreadable_state_on_disk_refuses_the_write_with_the_read_sentence() {
    const SMALL_STATE: relay_route_core::Decl<relay_route_core::State> =
        relay_route_core::Decl::<relay_route_core::State>::json(".cc-monitor/v.json", 1, 1, 64);
    const SMALL_CACHE: relay_route_core::Decl<relay_route_core::Cache> =
        relay_route_core::Decl::<relay_route_core::Cache>::json(".cc-monitor/v.json", 1, 64);
    let state: Doc<Shape> = Doc::of(&SMALL_STATE);
    let cache: Doc<Shape> = Doc::of(&SMALL_CACHE);
    let base = sandbox("v6");
    let (own, dir) = own_in(&base);
    let held = own.lock(&dir).expect("lock");
    let p = dir.as_path().join("v.json");
    let seeds: [(&str, Option<Vec<u8>>); 4] = [
        ("not json", Some(b"{".to_vec())),
        (
            "version not an integer",
            Some(b"{\"version\":\"1\"}".to_vec()),
        ),
        ("over the cap", Some(vec![b' '; 65])),
        ("a directory", None),
    ];
    for (what, seed) in &seeds {
        std::fs::remove_dir_all(&p).ok();
        std::fs::remove_file(&p).ok();
        match seed {
            Some(b) => std::fs::write(&p, b).expect("seed"),
            None => std::fs::create_dir(&p).expect("dir in place"),
        }
        let Read::Unreadable(read_said) = state.read(&p) else {
            panic!("{what}：读那一侧没答读不出");
        };
        let Err(write_said) = state.write(&own, &dir, &held, &Shape { a: 1 }) else {
            panic!("{what}：盖掉了一份读不懂的状态");
        };
        assert_eq!(write_said.said, read_said.said, "{what}：写与读说两样");
        match seed {
            Some(b) => assert_eq!(
                &std::fs::read(&p).expect("read back"),
                b,
                "{what}：盘上字节被动了"
            ),
            None => assert!(p.is_dir(), "{what}：目录被动了"),
        }
        assert!(
            !dir.as_path().join("v.json.v1").exists(),
            "{what}：留了旁件"
        );
    }
    for (what, seed) in &seeds[..3] {
        std::fs::remove_dir_all(&p).ok();
        std::fs::write(&p, seed.as_ref().expect("bytes")).expect("seed");
        cache
            .write(&own, &dir, &held, &Shape { a: 2 })
            .unwrap_or_else(|e| panic!("{what}：缓存没照写：{e:?}"));
    }
    std::fs::remove_dir_all(&base).ok();
}
