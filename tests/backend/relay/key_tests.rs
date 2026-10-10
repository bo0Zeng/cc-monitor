//! 中转口钥匙的读盘那一半（`relay/key.rs`）：钥匙文件的住址 / 权限 / 跨重起不变 / 坏了就换 · 只由宿主调。
//!
//! 守的要求：`INVARIANTS §48.1a`（中转口的钥匙）。进门三问那一半的纯判据住中转那一份（`door_tests.rs`）；
//! 走真 socket 的那一半（403/421 与 404 真可分 · 对的钥匙真转发 · 钥匙不出现在日志 / tee / argv / env / 上游）住 `server_tests.rs` 的 `rk1_*` 那几条。

use super::*;

/// 判据用的那一把（形状合法、一眼认得出是测试值）。同层判据（`server_tests` · `host_tests` · `wire_golden`）经
/// `key::key_tests::TEST_KEY` 取。
pub(crate) const TEST_KEY: &str =
    "7e577e577e577e577e577e577e577e577e577e577e577e577e577e577e577e57";

/// 判据用的那把只许直通的钥匙：从 [`TEST_KEY`] 派生（生产同一个函数 [`pass_of`]）。
pub(crate) fn test_pass_key() -> String {
    pass_of(&test_key()).expose().to_string()
}

/// 判据用：[`TEST_KEY`] 那一把。
pub(crate) fn test_key() -> Key {
    Key::from_text(TEST_KEY).expect("夹具钥匙过得了形状闸")
}

/// 判据用：[`TEST_KEY`] ＋ 它派生出来的只许直通那一把（中转门上那两把，与生产同一个派生）。
pub(crate) fn test_keys() -> comms_outward::Keys {
    comms_outward::Keys {
        full: test_key(),
        pass: pass_of(&test_key()),
    }
}

/// 只许直通那一把是派生的：同一把根钥匙恒得同一把 · 形状合法 · 不等于根钥匙 · 换一把根钥匙就换一把；
/// 插地址（`keyed_with_key_on_disk`）用的就是它，盘上没有第二份文件。
#[test]
fn the_pass_key_is_derived_from_the_root_and_never_stored() {
    let a = test_key();
    let p1 = pass_of(&a);
    assert_eq!(p1.expose(), pass_of(&a).expose(), "同一把根钥匙派生出两把");
    assert!(relay_route_core::key_shape_ok(p1.expose()));
    assert_ne!(p1.expose(), a.expose(), "派生出来的就是根钥匙本身");
    let b = Key::from_text(&"b".repeat(64)).unwrap();
    assert_ne!(
        pass_of(&b).expose(),
        p1.expose(),
        "换了根钥匙，派生的却没变"
    );
    let home = std::env::temp_dir().join(format!("ccm-pass-derive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    seed_test_home(&home);
    let url = "http://127.0.0.1:8788/t/codex/_";
    assert_eq!(
        keyed_with_key_on_disk(&home, url, KeyKind::Pass).as_deref(),
        Some(format!("http://127.0.0.1:8788/{}/t/codex/_", p1.expose()).as_str())
    );
    assert_eq!(pass_key_on_disk(&home).as_deref(), Some(p1.expose()));
    assert!(home.join(KEY_FILE_REL).is_file(), "根钥匙不在盘上");
    assert!(
        !home.join(".cc-monitor").join("relay-pass-key").exists(),
        "只许直通那一把落了盘"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// 判据用：在一个**夹具家目录**里预先放好 [`TEST_KEY`]（`0600`，走生产段那一份落盘），回这个家目录。
/// 起真中转的判据都要它 —— 不给的话中转会去**用户真实的家目录**里铸钥匙。
pub(crate) fn seed_test_home(home: &Path) -> PathBuf {
    let path = home.join(KEY_FILE_REL);
    // 那一层目录由生产段 `ensure_key` 在拿锁之前建（`write_key` 不再建）⇒ 夹具这里自己建。
    std::fs::create_dir_all(path.parent().expect("钥匙路径有父目录")).expect("建夹具目录");
    write_key(&path, &test_key()).expect("写夹具钥匙");
    home.to_path_buf()
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
        relay_route_core::key_shape_ok(first.expose()),
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
        assert!(
            relay_route_core::key_shape_ok(fresh.expose()),
            "换出来的形状要对"
        );
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

/// ⑦ 钥匙文件的相对路径与钥匙形状**只住共享 crate**（`relay_route_core::KEY_FILE_REL` · `key_shape_ok`）：
/// 本模块那两个名字是它的别名（先前两半各写一份字面量、本条现抠 monitor `payload.rs` 那一行对拍 —— 那条跨半边边随之出列；
/// 「两半零字面量」由 monitor 侧 `payload_tests::us1_the_monitor_holds_no_upstream_selection_and_no_route_grammar` 两棵树一起扫）。
/// 这里钉本模块铸的长度与共享 crate 认的形状对得上（异源：铸法是 `KEY_BYTES` 算出来的，形状闸是 crate 里写死的 64）。
#[test]
fn the_key_file_and_the_key_shape_come_from_the_shared_crate() {
    assert_eq!(KEY_FILE_REL, relay_route_core::KEY_FILE_REL);
    assert_eq!(2 * KEY_BYTES, 64, "铸的长度与共享 crate 的形状闸不一致");
    assert!(
        relay_route_core::key_shape_ok(TEST_KEY),
        "夹具钥匙过不了共享的形状闸"
    );
}

/// 要求：「读 / 铸钥匙文件那一半只由宿主调（`relay/listen.rs` 绑上口之后 `ensure_key`）…；中转只收宿主交进来的 `Key`」。
///
/// 中转那一份是另一个 crate，够不着本模块（编译器挡着）。后端这一侧：`key` 是 `relay` 的**私有**子模块 ⇒
/// 只有 `relay/` 那几份够得着它；那几份的生产段里（`key.rs` 自己除外）读盘三件的 `(文件, 名字) → 次数` == 宿主那两处（两向）；
/// `mod key;` 仍是私有（人群的前提）。正控：往一份副本里塞一处 `key::read_key` 数得出。
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
        guard_core::scan_tree_excluding(&crate::guard_support::relay_host_root(), &["rs"], &[])
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
        if rel == "src/backend/relay/key.rs" {
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
        "钥匙读盘那一半（算路径 · 读 · 铸）只许宿主 `listen.rs` 绑上口之后调；中转只收宿主交进来的 `Key`"
    );
    let mod_rs = std::fs::read_to_string(crate::guard_support::relay_host_root().join("mod.rs"))
        .expect("读 relay/mod.rs");
    guard_core::pin_line(&guard_core::production_code(&mod_rs), "mod key;")
        .unwrap_or_else(|e| panic!("`key` 不再是 relay 的私有子模块 ⇒ 本条的人群前提不成立：{e}"));
    let planted = "fn f(p: &std::path::Path) {\n    let _ = super::key::read_key(p);\n}\n";
    assert_eq!(words_in(planted), BTreeMap::from([("read_key", 1)]));
}
