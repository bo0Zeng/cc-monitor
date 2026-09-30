//! 〔RK1〕中转口的门（`relay/door.rs`）的纯判据：进门三问逐形 · 钥匙文件的住址 / 权限 / 跨重起不变 / 坏了就换。
//!
//! 守的要求：`INVARIANTS §48.1a`（中转口的钥匙），逐字一句：「过了才剥掉那一段交给路由（`/s/` 与 `/t/` 一样要过）
//! ⇒ 「钥匙对、表里没这一行」仍是 **404**，与 403 **可分**；三种拒法各带一句说得清是哪一问的话。」
//! 设计住 `调研/第四波记录/RK1.md §1`。走真 socket 的那一半（403/421 与 404 真可分 · 对的钥匙真转发 ·
//! 钥匙不出现在日志 / tee / argv / env / 上游）住 `server_tests.rs` 的 `rk1_*` 那几条。

use super::*;
use crate::relay::http1;

/// 判据用的那一把（形状合法、一眼认得出是测试值）。同层判据（`server_tests` · `host_tests` · `wire_golden`）经
/// `super::door::door_tests::TEST_KEY` 取。
pub(crate) const TEST_KEY: &str =
    "7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57";

impl Key {
    /// 判据用：[`TEST_KEY`] 那一把。
    pub(crate) fn for_tests() -> Key {
        Key(TEST_KEY.to_string())
    }
}

/// 判据用：在一个**夹具家目录**里预先放好 [`TEST_KEY`]（`0600`，走生产段那一份落盘），回这个家目录。
/// 起真中转的判据都要它 —— 不给的话中转会去**用户真实的家目录**里铸钥匙。
pub(crate) fn seed_test_home(home: &Path) -> PathBuf {
    let path = home.join(KEY_FILE_REL);
    // 〔HX2〕那一层目录由生产段 `ensure_key` 在拿锁之前建（`write_key` 不再建）⇒ 夹具这里自己建。
    std::fs::create_dir_all(path.parent().expect("钥匙路径有父目录")).expect("建夹具目录");
    write_key(&path, &Key::for_tests()).expect("写夹具钥匙");
    home.to_path_buf()
}

/// 用一段请求头原文造 `RequestHead`（走生产段那一份解析，不手搓结构体）。
fn head(raw: &str) -> RequestHead {
    http1::parse_request(raw.as_bytes()).unwrap_or_else(|| panic!("夹具请求头不成形：{raw:?}"))
}

fn req(target: &str, headers: &str) -> RequestHead {
    head(&format!("POST {target} HTTP/1.1\r\n{headers}\r\n"))
}

const LOOP: &str = "Host: 127.0.0.1:8788\r\n";

/// ① 钥匙那一问：对的过、剥得干净；没有 / 错 / 前缀 / 多一截 / 大小写翻转 / 空段 ⇒ `BadKey`。
/// （绝对形式 `POST http://…` 到不了门：`http1::parse_request` 先拒，下游拿 400。）
#[test]
fn only_the_exact_key_as_the_first_segment_gets_in() {
    let k = Key::for_tests();
    let good = format!("/{TEST_KEY}/s/claude-code/acctA/sid/v1/messages?beta=true");
    assert_eq!(
        admit(&req(&good, LOOP), &k),
        Verdict::Pass("/s/claude-code/acctA/sid/v1/messages?beta=true".into()),
        "对的钥匙必须过，且只剥掉钥匙那一段"
    );
    assert_eq!(
        admit(
            &req(
                &format!("/{TEST_KEY}/t/claude-code/0/sid/v1/messages"),
                LOOP
            ),
            &k
        ),
        Verdict::Pass("/t/claude-code/0/sid/v1/messages".into()),
        "`/t/` 同样先过门"
    );
    let upper = TEST_KEY.to_ascii_uppercase();
    let wrong = format!("{}0", &TEST_KEY[..TEST_KEY.len() - 1]);
    for bad in [
        "/s/claude-code/acctA/sid/v1/messages".to_string(), // 没钥匙（今天的老形状）
        "/t/claude-code/0/sid/v1/messages".to_string(),
        format!("/{wrong}/s/claude-code/acctA/sid/v1/messages"), // 错一个字符
        format!("/{}/s/a/b/c/v1", &TEST_KEY[..32]),              // 前缀
        format!("/{TEST_KEY}0/s/a/b/c/v1"),                      // 多一截
        format!("/{upper}/s/a/b/c/v1"),                          // 大小写
        "//s/claude-code/acctA/sid/v1/messages".to_string(),     // 空段（钥匙文件不在时的展开形）
        format!("/{TEST_KEY}"),                                  // 只有钥匙、后面没有路径
    ] {
        assert_eq!(
            admit(&req(&bad, LOOP), &k),
            Verdict::BadKey,
            "{bad:?} 不该过门"
        );
    }
}

/// ③ `Origin` 那一问：带了就拒，**不管值是什么**、不管钥匙对不对（排在钥匙之前）。
#[test]
fn any_origin_header_is_refused_before_the_key_is_looked_at() {
    let k = Key::for_tests();
    let good = format!("/{TEST_KEY}/s/claude-code/acctA/sid/v1/messages");
    for o in [
        "Origin: https://evil.example\r\n",
        "origin: null\r\n",
        "ORIGIN: http://127.0.0.1:8788\r\n",
    ] {
        assert_eq!(
            admit(&req(&good, &format!("{LOOP}{o}")), &k),
            Verdict::Browser,
            "{o:?}"
        );
    }
}

/// ③ `Host` 那一问：回环三形（可带口、大小写不敏感）放行；别的、缺、重复 ⇒ `NotLoopbackHost`。
#[test]
fn only_a_loopback_literal_host_gets_in() {
    let k = Key::for_tests();
    let good = format!("/{TEST_KEY}/s/claude-code/acctA/sid/v1/messages");
    for h in [
        "127.0.0.1",
        "127.0.0.1:8788",
        "localhost",
        "LOCALHOST:8788",
        "[::1]",
        "[::1]:8788",
    ] {
        assert!(host_header_is_loopback_literal(h), "{h:?} 是回环字面量");
        assert!(
            matches!(
                admit(&req(&good, &format!("Host: {h}\r\n")), &k),
                Verdict::Pass(_)
            ),
            "{h:?}"
        );
    }
    for h in [
        "evil.example",
        "evil.example:8788",
        "127.0.0.1.evil.example",
        "localhost.evil.example:8788",
        "127.0.0.2",
        "0.0.0.0:8788",
        "[::1].evil",
        "127.0.0.1:",
        "127.0.0.1:99999",
        "127.0.0.1:80:80",
        "",
    ] {
        assert!(!host_header_is_loopback_literal(h), "{h:?} 不是回环字面量");
        assert_eq!(
            admit(&req(&good, &format!("Host: {h}\r\n")), &k),
            Verdict::NotLoopbackHost,
            "{h:?}"
        );
    }
    assert_eq!(
        admit(&req(&good, ""), &k),
        Verdict::NotLoopbackHost,
        "没有 Host"
    );
    assert_eq!(
        admit(&req(&good, "Host: 127.0.0.1\r\nHost: evil.example\r\n"), &k),
        Verdict::NotLoopbackHost,
        "两个 Host"
    );
}

/// 拒绝那几格：三种结局的（状态行, 说法）两两不同；`Pass` 没有拒绝的说法。
#[test]
fn the_three_refusals_are_distinct_faces() {
    let faces: Vec<_> = [Verdict::Browser, Verdict::NotLoopbackHost, Verdict::BadKey]
        .iter()
        .map(|v| v.refusal().expect("拒绝那几格都有说法"))
        .collect();
    assert_eq!(faces[0].0, FORBIDDEN);
    assert_eq!(faces[1].0, MISDIRECTED);
    assert_eq!(faces[2].0, FORBIDDEN);
    // 〔FIX3 · `99 §2.2 ⑫`〕原因头的值手写：两个 403 靠它分开。
    let reasons: Vec<_> = faces.iter().map(|f| f.1).collect();
    assert_eq!(reasons, ["browser-origin", "host-not-loopback", "bad-key"]);
    let whys: std::collections::BTreeSet<_> = faces.iter().map(|f| f.2).collect();
    assert_eq!(whys.len(), 3, "三句为什么必须两两不同：{faces:?}");
    assert_eq!(Verdict::Pass("/".into()).refusal(), None);
}

/// ⑤ 钥匙文件：第一次铸（64 位小写十六进制 · `0600`）→ 第二次读回**同一把**（跨重起不变）；
///    坏文件（空 / 短 / 非十六进制 / 大写）⇒ 换一把新的、形状合法。
#[test]
fn the_key_file_is_minted_once_private_and_read_back_across_restarts() {
    let home = std::env::temp_dir().join(format!(
        "ccm-rk1-door-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).expect("建夹具家目录");
    let get = |k: &str| (k == "HOME").then(|| home.display().to_string());
    let p = key_path(&get).expect("有 HOME 就有路径");
    assert_eq!(p, home.join(".cc-monitor").join("relay-key"));
    assert!(!p.exists(), "夹具家目录一开始不该有钥匙");

    let first = ensure_key(&p).expect("第一次铸");
    let on_disk = std::fs::read_to_string(&p).expect("读回");
    assert_eq!(on_disk, first.expose(), "盘上就是那一把");
    assert!(
        key_shape_ok(first.expose()),
        "形状：{}",
        first.expose().len()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&p).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "钥匙文件必须只给本人");
    }
    let again = ensure_key(&p).expect("第二次");
    assert_eq!(
        again.expose(),
        first.expose(),
        "重起中转必须读回同一把（端口固定 ⇒ 活会话靠它）"
    );

    for junk in [
        "",
        "abc",
        &"z".repeat(64),
        &first.expose().to_ascii_uppercase(),
    ] {
        std::fs::write(&p, junk).expect("写坏文件");
        let fresh = ensure_key(&p).expect("坏文件 ⇒ 换一把");
        assert!(key_shape_ok(fresh.expose()), "换出来的形状要对");
        assert_ne!(fresh.expose(), junk, "坏的那一份不许被认下");
        assert_eq!(std::fs::read_to_string(&p).expect("读回"), fresh.expose());
    }
    // 两次铸出来的不是同一把（随机数真在转）。
    std::fs::write(&p, "").expect("清空");
    let a = ensure_key(&p).expect("a");
    std::fs::write(&p, "").expect("清空");
    let b = ensure_key(&p).expect("b");
    assert_ne!(a.expose(), b.expose(), "两次铸出同一把 —— 随机源是死的");
    // 临时文件都挪走了（本进程那一个临时名不在盘上）。
    let tmp = p
        .parent()
        .expect("父")
        .join(format!("relay-key.{}.tmp", std::process::id()));
    assert!(!tmp.exists(), "临时文件留在盘上了：{}", tmp.display());
    let _ = std::fs::remove_dir_all(&home);
}

/// 家目录解析不出来 ⇒ 没有路径（上层据此**拒绝起**，不猜一个路径去写）。
#[test]
fn no_home_means_no_key_path() {
    assert_eq!(key_path(&|_| None), None);
    assert_eq!(
        key_path(&|k| (k == "HOME").then(String::new)),
        None,
        "空 HOME == 没有"
    );
    assert_eq!(
        key_path(&|k| (k == "USERPROFILE").then(|| "C:/Users/u".to_string())),
        Some(std::path::Path::new("C:/Users/u").join(KEY_FILE_REL)),
        "没有 HOME 退 USERPROFILE"
    );
}

/// `Key` 不许被 `{:?}` 打出值（谁把它塞进日志，打出来的也只有 `Key(…)`）。
#[test]
fn a_key_never_prints_its_value() {
    let shown = format!("{:?}", Key::for_tests());
    assert_eq!(shown, "Key(…)");
    assert!(!shown.contains(TEST_KEY));
}

/// ⑦ 〔US1 · 4D〕钥匙文件的相对路径与钥匙形状**只住共享 crate**（`relay_route_core::KEY_FILE_REL` · `key_shape_ok`）：
/// 本模块那两个名字是它的别名（先前两半各写一份字面量、本条现抠 monitor `payload.rs` 那一行对拍 —— 那条跨半边边随之出列；
/// 「两半零字面量」由 monitor 侧 `payload_tests::us1_the_monitor_holds_no_upstream_selection_and_no_route_grammar` 两棵树一起扫）。
/// 这里钉本模块铸的长度与共享 crate 认的形状对得上（异源：铸法是 `KEY_BYTES` 算出来的，形状闸是 crate 里写死的 64）。
#[test]
fn the_key_file_and_the_key_shape_come_from_the_shared_crate() {
    assert_eq!(KEY_FILE_REL, relay_route_core::KEY_FILE_REL);
    assert_eq!(2 * KEY_BYTES, 64, "铸的长度与共享 crate 的形状闸不一致");
    assert!(key_shape_ok(TEST_KEY), "夹具钥匙过不了共享的形状闸");
}

/// 住址：`设计/05 §9` 第 11 条「门（`relay/door.rs`）的读盘那一半只从宿主调（`§4.3`）没有判据」·
/// `05 §4.3`「读 / 铸钥匙文件那一半只由宿主调（`relay/listen.rs` 绑上口之后 `ensure_key`）…；成员 `comms/outward/server.rs` 只收宿主交进来的 `door::Key`」。
///
/// `door` 是 `relay` 的**私有**子模块 ⇒ 编译器只让 `relay` 模块那几份够得着它（面 B 成员 `src/comms/outward/*` ＋ 宿主 `listen.rs`）。
/// 那几份的生产段里（`door.rs` 自己除外）读盘三件的 `(文件, 名字) → 次数` == 宿主那两处（两向）；`mod door;` 仍是私有（人群的前提）。
/// 正控：往一份成员副本里塞一处 `door::read_key` 数得出。
#[test]
fn the_key_files_disk_half_is_called_only_by_the_host() {
    use std::collections::BTreeMap;
    const DISK_HALF: &[&str] = &["key_path", "read_key", "ensure_key"];
    let words_in = |prod: &str| -> BTreeMap<&'static str, usize> {
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        let mut out = BTreeMap::new();
        for w in DISK_HALF {
            let n = prod
                .match_indices(w)
                .filter(|(i, _)| {
                    !prod[..*i].chars().next_back().is_some_and(ident)
                        && !prod[i + w.len()..].chars().next().is_some_and(ident)
                })
                .count();
            if n > 0 {
                out.insert(*w, n);
            }
        }
        out
    };
    let repo = crate::guard_support::repo_root();
    let mut got = BTreeMap::new();
    for (p, src) in
        guard_core::scan_tree_excluding(&crate::guard_support::relay_root(), &["rs"], &[])
    {
        let mut norm = PathBuf::new();
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => {
                    norm.pop();
                }
                std::path::Component::CurDir => {}
                other => norm.push(other),
            }
        }
        let rel = norm
            .strip_prefix(&repo)
            .unwrap_or(&norm)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "src/backend/relay/door.rs" {
            continue;
        }
        for (w, n) in words_in(&crate::guard_support::production_side_of(&p, &src)) {
            got.insert((rel.clone(), w), n);
        }
    }
    let want = BTreeMap::from([
        (("src/backend/relay/listen.rs".to_string(), "ensure_key"), 1),
        (("src/backend/relay/listen.rs".to_string(), "key_path"), 1),
    ]);
    assert_eq!(
        got, want,
        "门读盘那一半（算路径 · 读 · 铸）只许宿主 `listen.rs` 绑上口之后调；成员只收宿主交进来的 `door::Key`"
    );
    let mod_rs = std::fs::read_to_string(crate::guard_support::relay_root().join("mod.rs"))
        .expect("读 relay mod.rs");
    guard_core::pin_line(&guard_core::production_code(&mod_rs), "mod door;")
        .unwrap_or_else(|e| panic!("`door` 不再是 relay 的私有子模块 ⇒ 本条的人群前提不成立：{e}"));
    let planted = "fn f(p: &std::path::Path) {\n    let _ = super::door::read_key(p);\n}\n";
    assert_eq!(words_in(planted), BTreeMap::from([("read_key", 1)]));
}
