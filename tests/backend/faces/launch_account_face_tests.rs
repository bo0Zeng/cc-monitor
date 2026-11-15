//! 起会话挑号那一份宿主自己接的几处：`list_at` 照起的那一家列号 · `last_of` 读这台的启动台账 · `pretrust_with` 起之前只标要用的那个号 ·
//! `with_facts` 交请求那一家与有没有账号这一维。
//! 账号库那一侧的本体（同家判 `row_of_family` · 预标信任 `pretrust` 只写清单里那个号）各钉在 `library_tests.rs` 与
//! `trust_share_exec_tests.rs`；这里只钉宿主把那一家 · 那个目录 · 这台的数据目录交对了。

use super::*;
use std::path::{Path, PathBuf};

/// 账号库清单（这台家目录底下那一份）：`rows` ＝ （号名, 哪一家；`None` ⇒ 不写那一格）。各号目录一并建好，回各号目录。
pub(crate) fn library(home: &Path, rows: &[(&str, Option<&str>)]) -> Vec<PathBuf> {
    let accts = crate::platform::paths::home_join(home, relay_route_core::ACCOUNTS_DIR_REL);
    let dirs: Vec<PathBuf> = rows.iter().map(|(n, _)| accts.join(n)).collect();
    for d in &dirs {
        std::fs::create_dir_all(d).expect("建号目录");
    }
    let entries: Vec<serde_json::Value> = rows
        .iter()
        .zip(&dirs)
        .map(|((name, agent), dir)| {
            let mut e = serde_json::json!({
                "name": name, "email": "", "configDir": dir.display().to_string(), "mode": "isolated",
            });
            if let Some(a) = agent {
                e["agent"] = serde_json::json!(a);
            }
            e
        })
        .collect();
    std::fs::write(
        accts.join(relay_route_core::ACCOUNTS_MANIFEST_NAME),
        serde_json::json!({
            "version": 1, "acctsDir": accts.display().to_string(), "accounts": entries,
        })
        .to_string(),
    )
    .expect("写清单");
    dirs
}

/// ★ `list_at`：清单列全这台的号（账号页 · 登录也读这一份），只有起的那一家的号进新建会话的选号表（`new_session` 为真）；家目录照给的那一个读，不读这台的。
#[test]
fn only_the_launching_familys_accounts_go_into_the_new_session_pick() {
    let home = test_home::scratch("lafam");
    library(
        &home,
        &[
            ("c1", Some(crate::agents::claudecode::AGENT_KIND)),
            ("x1", Some(crate::agents::codex::AGENT_KIND)),
        ],
    );
    let picks = |agent: &str| -> Vec<String> {
        list_at(&home, agent)
            .accounts
            .iter()
            .filter(|r| r.new_session)
            .map(|r| r.scanned.name.clone())
            .collect()
    };
    assert_eq!(
        picks(crate::agents::claudecode::AGENT_KIND),
        ["c1"],
        "起 claude ⇒ 只列 claude 的号"
    );
    assert_eq!(
        picks(crate::agents::codex::AGENT_KIND),
        ["x1"],
        "起 codex ⇒ 只列 codex 的号"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ `last_of`：这台数据目录里启动台账记着这条会话上次用的号 ⇒ 那个号；没记 ⇒ 没有。
#[test]
fn last_of_reads_this_machines_launch_book() {
    use crate::history::launch_book::{read_book, write_book};
    const SID: &str = "4c1f0d2e-7777-4aaa-8bbb-launchface01";
    const NEVER: &str = "4c1f0d2e-7777-4aaa-8bbb-launchface02";
    let data = crate::platform::paths::data_home().expect("测试档有数据目录");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let root = own.root().expect("数据目录");
    own.ensure_dir(&root).expect("建数据目录");
    {
        // 台账是这台进程共用的：锁里读盘、只添这一条、写回（同生产那一处 `launch_account::record`）。
        let held = own.lock(&root).expect("锁");
        let mut book = read_book(&data).expect("台账读得懂");
        book.sessions.insert(SID.into(), "acct-last".into());
        write_book(&own, &root, &held, &book).expect("写台账");
    }
    assert_eq!(last_of(SID).as_deref(), Some("acct-last"));
    assert_eq!(last_of(NEVER), None, "没记过的会话 ⇒ 没有");
}

#[cfg(unix)]
fn trusted(cfg: &Path, cwd: &str) -> bool {
    std::fs::read_to_string(cfg)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .is_some_and(|v| v["projects"][cwd]["hasTrustDialogAccepted"] == serde_json::json!(true))
}

/// ★ `pretrust_with`：起之前只把要用的那个号标成信任过这个目录，同库别的号与家目录下那一份一个字节不变；
/// 认不出的那一家 · 没有文件面 ⇒ 一处不写。
#[cfg(unix)]
#[test]
fn pretrust_with_marks_only_the_account_being_launched() {
    let home = test_home::scratch("lapre").canonicalize().expect("家");
    let dirs = library(&home, &[("b", None), ("c", None)]);
    let body = "{\"projects\":{}}\n";
    for d in &dirs {
        std::fs::write(d.join(".claude.json"), body).expect("号配置");
    }
    std::fs::write(home.join(".claude.json"), body).expect("家目录那一份");
    let work = home.join("work");
    std::fs::create_dir_all(&work).expect("工作目录");
    let cwd = work.display().to_string();
    let door: Files = Box::leak(Box::new(crate::assets::aliases::test_support::HomeDoor(
        home.clone(),
    )));
    let (b, c) = (&dirs[0], &dirs[1]);
    let claude = crate::agents::claudecode::AGENT_KIND;

    pretrust_with(None, claude, &b.display().to_string(), &cwd);
    assert!(!trusted(&b.join(".claude.json"), &cwd), "没有文件面 ⇒ 不写");
    pretrust_with(Some(door), "nobody", &b.display().to_string(), &cwd);
    assert!(
        !trusted(&b.join(".claude.json"), &cwd),
        "认不出的那一家 ⇒ 不写"
    );

    pretrust_with(Some(door), claude, &b.display().to_string(), &cwd);
    assert!(trusted(&b.join(".claude.json"), &cwd), "要用的那个号标上");
    assert_eq!(
        std::fs::read_to_string(c.join(".claude.json")).expect("c"),
        body,
        "同库别的号不碰"
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".claude.json")).expect("家"),
        body,
        "家目录下那一份不碰"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ `with_facts`：交出去的那一家就是请求说的那一家，不换成默认那一家；有没有账号这一维照那一家答，认不出的那一家 ⇒ 没有。
#[test]
fn with_facts_hands_the_asked_family_and_whether_it_has_accounts() {
    let of = |agent: &str| with_facts(agent, |f| (f.agent.clone(), f.has_accounts));
    let claude = crate::agents::claudecode::AGENT_KIND;
    assert_eq!(of(claude), (cmd_wire::AgentKind::new(claude), true));
    assert_eq!(of("nobody"), (cmd_wire::AgentKind::new("nobody"), false));
}

/// ★ `list_of`：读的是这台家目录底下那一份账号库（清单落点照这台的家），与照这台的家问 [`list_at`] 同一份成品，两家各一问。
/// 测试档的家是这个进程共用的沙箱、里面没有账号库：往里写号会让同进程别的判据读到，这里不写；
/// 照那一家滤号钉在 `only_the_launching_familys_accounts_go_into_the_new_session_pick`（`list_at`）。
#[test]
fn list_of_reads_the_library_under_this_machines_home() {
    let home = crate::platform::paths::home_dir().expect("测试档有家");
    let manifest = crate::platform::paths::home_join(&home, relay_route_core::ACCOUNTS_DIR_REL)
        .join(relay_route_core::ACCOUNTS_MANIFEST_NAME);
    for agent in [
        crate::agents::claudecode::AGENT_KIND,
        crate::agents::codex::AGENT_KIND,
    ] {
        let got = list_of(agent);
        assert_eq!(
            got.meta.manifest_path,
            manifest.display().to_string(),
            "{agent}：清单落点不是这台家目录底下那一份"
        );
        assert_eq!(
            serde_json::to_value(&got).expect("成品"),
            serde_json::to_value(list_at(&home, agent)).expect("成品"),
            "{agent}：与照这台的家问的那一份不同"
        );
    }
}

/// ★ `key_of`：照账号 id 找 key 表那一行，交那一行自己的（掩码，端点地址）；表里没有那个 id ⇒ 没有。
#[test]
fn key_of_hands_the_row_with_that_id() {
    use crate::accounts::upstream_select::file_face::KeyFact;
    let keys = [
        KeyFact {
            id: "k1".into(),
            masked: Some("sk-…a1".into()),
            base_url: None,
        },
        KeyFact {
            id: "k2".into(),
            masked: None,
            base_url: Some("https://up.example/v1".into()),
        },
    ];
    let of = key_of(&keys);
    assert_eq!(of("k1"), Some((Some("sk-…a1".into()), None)));
    assert_eq!(
        of("k2"),
        Some((None, Some("https://up.example/v1".into()))),
        "第二行照第二行答，不拿头一行"
    );
    assert_eq!(of("k3"), None, "表里没有 ⇒ 没有");
}

/// ★ `host_of`：这台数据目录里启动台账记着这条会话起在哪种宿主里 ⇒ 那一种（两条各记一种、各读各的）；没记 ⇒ 没有。
#[test]
fn host_of_reads_this_machines_launch_book() {
    use crate::history::launch_book::{read_book, write_book};
    use crate::wire::TerminalHost;
    const TMUX: &str = "4c1f0d2e-7777-4aaa-8bbb-launchface03";
    const MANAGED: &str = "4c1f0d2e-7777-4aaa-8bbb-launchface04";
    const NEVER: &str = "4c1f0d2e-7777-4aaa-8bbb-launchface05";
    let data = crate::platform::paths::data_home().expect("测试档有数据目录");
    let own = crate::platform::fs::Own::issue_data_at(&data);
    let root = own.root().expect("数据目录");
    own.ensure_dir(&root).expect("建数据目录");
    {
        // 台账是这台进程共用的：锁里读盘、只添这两条、写回（同 `last_of_reads_this_machines_launch_book`）。
        let held = own.lock(&root).expect("锁");
        let mut book = read_book(&data).expect("台账读得懂");
        book.hosts.insert(TMUX.into(), TerminalHost::Tmux);
        book.hosts.insert(MANAGED.into(), TerminalHost::Managed);
        write_book(&own, &root, &held, &book).expect("写台账");
    }
    assert_eq!(host_of(TMUX), Some(TerminalHost::Tmux));
    assert_eq!(host_of(MANAGED), Some(TerminalHost::Managed));
    assert_eq!(host_of(NEVER), None, "没记过的会话 ⇒ 没有");
}

/// ★ `list_in`：账号库照交进来的家、key 表照交进来的那一份、中转在不在照交进来的那一格，起的那一家照交进来的那一家：
/// 表里有那一行的号 ⇒ 按 API key 号算、带上那一行的掩码与端点；没交 key 表 ⇒ 那个号不按 API key 号算、两格都缺；
/// 中转在不在换了 ⇒ 那个号的徽章跟着换；起别的那一家 ⇒ 这一家的号一个都不进新建会话的选号表。
#[test]
fn list_in_takes_the_home_key_table_family_and_relay_it_was_handed() {
    let root = test_home::scratch("lain");
    let (home, keys) = (root.join("home"), root.join("keys.json"));
    crate::faces::rotation_face::tests::key_table(&keys, &["api1"]);
    let agent = crate::accounts::upstream_select::file_face::rows_at(&keys).agent;
    library(&home, &[("api1", None), ("sub1", None)]);
    let row = |l: &AccountsList, name: &str| -> serde_json::Value {
        let v = serde_json::to_value(l).expect("成品");
        v["accounts"]
            .as_array()
            .expect("号")
            .iter()
            .find(|a| a["name"] == name)
            .cloned()
            .unwrap_or_else(|| panic!("清单里没有 {name}：{v}"))
    };

    let got = list_in(&home, Some(&keys), Some(agent), true);
    assert_eq!(
        got.meta.manifest_path,
        crate::platform::paths::home_join(&home, relay_route_core::ACCOUNTS_DIR_REL)
            .join(relay_route_core::ACCOUNTS_MANIFEST_NAME)
            .display()
            .to_string(),
        "账号库照交进来的家"
    );
    let api = row(&got, "api1");
    assert_eq!(
        api["authKind"],
        acct_core::AUTH_KIND_API_KEY,
        "表里有那一行 ⇒ API key 号：{api}"
    );
    assert_eq!(
        api["keyMasked"],
        creds_core::SecretKey::new("sk-FIXTURE-api1-NOT-A-REAL-KEY").masked(),
        "那一行的掩码"
    );
    assert!(!api["endpointHost"].is_null(), "那一行的端点：{api}");
    let sub = row(&got, "sub1");
    assert!(
        sub["keyMasked"].is_null() && sub["authKind"] != acct_core::AUTH_KIND_API_KEY,
        "表里没有的号：{sub}"
    );

    let unkeyed = row(&list_in(&home, None, Some(agent), true), "api1");
    assert!(
        unkeyed["authKind"] != acct_core::AUTH_KIND_API_KEY
            && unkeyed["keyMasked"].is_null()
            && unkeyed["endpointHost"].is_null(),
        "没交 key 表：{unkeyed}"
    );

    let idle = row(&list_in(&home, Some(&keys), Some(agent), false), "api1");
    assert_ne!(api["badge"], idle["badge"], "中转在不在换了，徽章没跟着换");

    let picks = |l: &AccountsList| -> Vec<String> {
        l.accounts
            .iter()
            .filter(|r| r.new_session)
            .map(|r| r.scanned.name.clone())
            .collect()
    };
    assert_eq!(picks(&got), ["api1", "sub1"], "起的就是这一家");
    assert!(
        picks(&list_in(&home, Some(&keys), Some("nobody-family"), true)).is_empty(),
        "起别的那一家，这一家的号进了选号表"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// ★ 家目录说不出（空家）⇒ 清单照实拒：那一句就是 `platform::paths::under_home` 拒的那一句，清单路径空着，
/// 不拿根目录兜底去读 `/.cc-monitor/accounts/accounts.json`（读了的话这一格会是「读取账号清单失败 · /.cc-monitor/…」）。
#[test]
fn an_unsaid_home_refuses_the_listing_without_reading_any_path() {
    let fake = crate::agents::fake::AGENT_KIND;
    let why =
        crate::platform::paths::under_home(Some(Path::new("")), relay_route_core::ACCOUNTS_DIR_REL)
            .expect_err("空家 ⇒ 拒");
    for got in [
        list_in(Path::new(""), None, Some(fake), false),
        list_at(Path::new(""), fake),
    ] {
        assert!(!got.meta.enabled, "空家照样启用了账号库");
        assert_eq!(
            got.meta.error.as_deref(),
            Some(why.as_str()),
            "拒的不是家目录说不出那一句"
        );
        assert_eq!(got.meta.manifest_path, "", "空家落了一个清单路径");
        assert!(got.accounts.is_empty());
    }
}

/// ★ 家目录名里有非 UTF-8 的字节 ⇒ 账号库照那个家原样读（不经字符串转一道、读成替换字符拼出的另一个目录）。
#[cfg(unix)]
#[test]
fn a_home_with_non_utf8_bytes_is_read_as_it_is() {
    use std::os::unix::ffi::OsStrExt;
    let root = test_home::scratch("labytes");
    let home = root.join(std::ffi::OsStr::from_bytes(b"h\xffome"));
    let accts = home.join(".cc-monitor").join("accounts");
    std::fs::create_dir_all(accts.join("a1")).expect("建号目录");
    std::fs::write(
        accts.join(relay_route_core::ACCOUNTS_MANIFEST_NAME),
        serde_json::json!({
            "version": 1, "acctsDir": "x",
            "accounts": [{"name": "a1", "email": "", "configDir": "/nowhere/a1", "mode": "isolated"}],
        })
        .to_string(),
    )
    .expect("写清单");
    let got = list_in(&home, None, Some(crate::agents::fake::AGENT_KIND), false);
    assert!(
        got.meta.enabled,
        "非 UTF-8 的家读偏了：{:?}",
        got.meta.error
    );
    assert_eq!(got.meta.count, 1);
    let _ = std::fs::remove_dir_all(&root);
}
