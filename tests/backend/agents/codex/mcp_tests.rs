//! `agents/codex/mcp.rs` 的判据：`config.toml` 的 `[mcp_servers.<名>]` 读得出、只改那一节。
//!
//! 夹具手写，形照 codex-rs rust-v0.167.0（`codex_config::mcp_types::McpServerConfig`：stdio 那一形 `command` · `args` ·
//! `env`，http 那一形 `url` · `bearer_token_env_var` · `http_headers`，`enabled = false` 停用；`codex_core::config::edit::document_helpers`
//! 写的样子：每个服务器一节 `[mcp_servers.<名>]`，`env` · `http_headers` 是它底下的子表）。不碰真的 `~/.codex`。

use super::*;
use crate::agents::{McpLook, McpStatus};
use serde_json::{json, Value};

const CONFIG: &str = r#"# my codex config
model = "gpt-5"

[mcp_servers.docs]
command = "npx"   # runner
args = ["-y", "docs-mcp"]
startup_timeout_sec = 20.5
enabled = true

[mcp_servers.docs.env]
TOKEN_ENV = "X"

# remote one
[mcp_servers.remote]
url = "https://mcp.example/v1"
bearer_token_env_var = "REMOTE_TOKEN"
enabled = false

[projects."/w/p"]
trust_level = "trusted"
"#;

#[test]
fn servers_are_read_as_json_definitions() {
    let m = servers_in(CONFIG).unwrap();
    assert_eq!(m.keys().collect::<Vec<_>>(), ["docs", "remote"]);
    assert_eq!(
        m["docs"],
        json!({
            "command": "npx",
            "args": ["-y", "docs-mcp"],
            "startup_timeout_sec": 20.5,
            "enabled": true,
            "env": {"TOKEN_ENV": "X"}
        })
    );
    assert_eq!(m["remote"]["enabled"], json!(false));
}

#[test]
fn no_section_is_an_empty_table_and_broken_toml_is_an_error_without_values() {
    assert!(servers_in("model = \"x\"\n").unwrap().is_empty());
    assert!(servers_in("").unwrap().is_empty());
    let e = servers_in("[mcp_servers.a]\ncommand = \"sekrit\n").unwrap_err();
    assert!(!e.contains("sekrit"), "{e}");
    assert!(servers_in("mcp_servers = 3\n").is_err());
}

#[test]
fn editing_one_server_leaves_other_sections_and_comments_alone() {
    let def = json!({"command": "uvx", "args": ["docs2"], "env": {"A": "1"}});
    let out = server_set(CONFIG, "docs", Some(&def)).unwrap();
    let m = servers_in(&out).unwrap();
    assert_eq!(m["docs"], def);
    assert_eq!(m["remote"], servers_in(CONFIG).unwrap()["remote"]);
    for kept in [
        "# my codex config\nmodel = \"gpt-5\"\n",
        "# remote one\n[mcp_servers.remote]\nurl = \"https://mcp.example/v1\"\n",
        "[projects.\"/w/p\"]\ntrust_level = \"trusted\"\n",
    ] {
        assert!(out.contains(kept), "lost {kept:?} in:\n{out}");
    }
}

#[test]
fn adding_a_server_appends_its_own_section() {
    let def = json!({"url": "https://n.example", "http_headers": {"X-A": "b"}});
    let out = server_set(CONFIG, "new", Some(&def)).unwrap();
    assert!(out.starts_with(CONFIG.trim_end()), "{out}");
    assert!(out.contains("[mcp_servers.new]\n"), "{out}");
    assert_eq!(servers_in(&out).unwrap()["new"], def);
    // 文件还不在 ⇒ 只有这一节。
    let fresh = server_set("", "solo", Some(&json!({"command": "c"}))).unwrap();
    assert_eq!(fresh, "[mcp_servers.solo]\ncommand = \"c\"\n");
}

#[test]
fn deleting_a_server_drops_only_that_section() {
    let out = server_set(CONFIG, "docs", None).unwrap();
    let m = servers_in(&out).unwrap();
    assert_eq!(m.keys().collect::<Vec<_>>(), ["remote"]);
    assert!(!out.contains("docs"), "{out}");
    assert!(
        out.contains("# my codex config\nmodel = \"gpt-5\"\n"),
        "{out}"
    );
    assert!(out.contains("trust_level = \"trusted\""), "{out}");
    // 删一个不在的 ⇒ 原文不变。
    assert_eq!(server_set(CONFIG, "ghost", None).unwrap(), CONFIG);
}

#[test]
fn a_definition_toml_cannot_hold_is_refused() {
    assert!(server_set(CONFIG, "x", Some(&json!({"command": null}))).is_err());
    assert!(server_set(CONFIG, "x", Some(&json!("not a table"))).is_err());
    assert!(server_set("[mcp_servers\n", "x", Some(&json!({}))).is_err());
}

#[test]
fn the_read_face_lists_user_and_project_servers_with_disabled_status() {
    let d = test_home::scratch("codex-mcp-read");
    let user = d.join("config.toml");
    std::fs::write(&user, CONFIG).unwrap();
    let proj = d.join("proj");
    std::fs::create_dir_all(proj.join(".codex")).unwrap();
    std::fs::write(
        proj.join(".codex").join("config.toml"),
        "[mcp_servers.local]\ncommand = \"l\"\n",
    )
    .unwrap();

    let r = read_at(Some(&user), Some(&proj), &McpLook::default());
    let got: Vec<(&str, &str, McpStatus)> = r
        .entries
        .iter()
        .map(|e| (e.scope, e.name.as_str(), e.status))
        .collect();
    assert_eq!(
        got,
        [
            ("user", "docs", McpStatus::Unknown),
            ("user", "remote", McpStatus::Disabled),
            ("project", "local", McpStatus::Unknown),
        ]
    );
    assert_eq!(r.dirs, ["/w/p"]);
    assert!(r.problems.is_empty(), "{:?}", r.problems);

    std::fs::write(&user, "[mcp_servers.a\n").unwrap();
    let r = read_at(Some(&user), None, &McpLook::default());
    assert!(r.entries.is_empty());
    assert_eq!(r.problems.len(), 1, "{:?}", r.problems);
}

/// 逐号读：没设账号那一份在前，再各号的家各一份 `<家>/config.toml`；与某号那一份是同一个文件 ⇒ 只读一次；
/// 每一条照实出自它那一份（`source`），几个号里的同一服务器合成一条是核心的事。Codex 不在家里记「要登录」⇒ `login_in` 恒空。
#[test]
fn every_account_home_has_its_own_user_config() {
    let d = test_home::scratch("codex-mcp-homes");
    let (a, b) = (d.join("a"), d.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let bare = d.join("config.toml");
    std::fs::write(
        &bare,
        "[mcp_servers.base]\ncommand = \"x\"\n[projects.\"/w/q\"]\n",
    )
    .unwrap();
    std::fs::write(
        a.join("config.toml"),
        "[mcp_servers.docs]\ncommand = \"d\"\n[projects.\"/w/p\"]\n",
    )
    .unwrap();
    std::fs::write(
        b.join("config.toml"),
        "[mcp_servers.docs]\ncommand = \"d\"\nenabled = false\n",
    )
    .unwrap();
    let look = McpLook {
        homes: vec![
            (None, d.clone()),
            (Some("a".into()), a.clone()),
            (Some("b".into()), b.clone()),
        ],
        bare: Some(d.clone()),
        now_ms: 0,
    };
    let r = read_at(Some(&bare), None, &look);
    let got: Vec<(&str, McpStatus, String)> = r
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.status, e.source.clone()))
        .collect();
    assert_eq!(
        got,
        [
            ("base", McpStatus::Unknown, bare.display().to_string()),
            (
                "docs",
                McpStatus::Unknown,
                a.join("config.toml").display().to_string()
            ),
            (
                "docs",
                McpStatus::Disabled,
                b.join("config.toml").display().to_string()
            ),
        ]
    );
    assert!(r.entries.iter().all(|e| e.login_in.is_empty()));
    assert_eq!(r.dirs, ["/w/p", "/w/q"]);

    // 没设账号那一份就是 a 号那一份 ⇒ 只读一次。
    let r = read_at(Some(&a.join("config.toml")), None, &look);
    assert_eq!(r.entries.iter().filter(|e| e.name == "docs").count(), 2);
    assert!(r.entries.iter().all(|e| e.name != "base"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn login_is_a_codex_mcp_subcommand() {
    assert_eq!(login_argv("docs"), ["codex", "mcp", "login", "docs"]);
}

/// 各号之间能同步的那一份：`CODEX_HOME/.credentials.json`，顶层就是「键 → 一条」，令牌三格在每一条顶上
/// （codex-rs `rust-v0.167.0` `rmcp-client` 的 `FallbackTokenEntry`；`expires_at` 是毫秒）。
#[test]
fn the_shareable_login_file_is_codex_home_credentials_json() {
    let l = super::super::accounts::FACE
        .mcp_logins
        .expect("申报了存成文件的 MCP 登录");
    assert_eq!(l.file, ".credentials.json");
    assert_eq!(
        (l.access, l.refresh, l.expires_ms),
        ("access_token", "refresh_token", "expires_at")
    );
}

/// 写一条之前依次拿的两把锁，照 Codex 自己写这份文件时的次序：先那一条的排他文件锁
/// `mcp-oauth-locks/<键的 sha256 十六进制>.lock`，再整份那一把 `mcp-oauth-locks/file-store.lock`。
/// 哈希夹具是用 Python `hashlib` 异源算的。
#[test]
fn writing_one_login_takes_the_entry_lock_then_the_store_lock() {
    use crate::agents::McpLoginLock;
    let l = super::super::accounts::FACE.mcp_logins.expect("申报了");
    let [McpLoginLock::EntryFile { name }, McpLoginLock::WholeFile { name: whole }] = l.lock else {
        panic!("两把锁：那一条的 · 整份的");
    };
    assert_eq!(
        name("docs|0123456789abcdef"),
        "mcp-oauth-locks/c8b90d0b052664862ac0b7c0cf694954ce5c4d5d09855603a50ca16519a79d53.lock"
    );
    assert_eq!(*whole, "mcp-oauth-locks/file-store.lock");
}

/// 读：顶层对象原样交（不在的 ⇒ 空的那一份由核心说；不是对象 ⇒ 读不懂）。改：只换那一条（`None` ＝ 删），别的条原样，紧凑写回。
#[test]
fn the_login_file_reads_and_edits_one_entry() {
    let l = super::super::accounts::FACE.mcp_logins.expect("申报了");
    let text = json!({
        "a|1": {"server_name": "a", "access_token": "x1", "expires_at": 1},
        "b|2": {"server_name": "b", "access_token": "x2"}
    })
    .to_string();
    let got = (l.read)(&text).expect("读得懂");
    assert_eq!(got.len(), 2);
    assert_eq!(got["b|2"]["access_token"], "x2");
    assert!((l.read)("[1]").is_err());
    assert!((l.read)("not json").is_err());

    let new = json!({"server_name": "a", "access_token": "y1", "expires_at": 9});
    let out = (l.edit)(&text, "a|1", Some(&new)).expect("改得动");
    assert!(!out.contains('\n'));
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["a|1"], new);
    assert_eq!(v["b|2"]["access_token"], "x2");
    let out = (l.edit)(&out, "b|2", None).expect("删得动");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("b|2").is_none() && v.get("a|1").is_some());
    assert!((l.edit)("[1]", "a|1", None).is_err());
}

/// 要不要（重新）登录，照 Codex 自己离线那一判（codex-rs `rust-v0.167.0` `rmcp-client/src/oauth.rs` 的 `codex_rmcp_client::oauth::oauth_tokens_are_usable`）：
/// 那个号的 `.credentials.json` 里有这一条（`server_name` 与 `server_url` 都对上）却用不了 ⇒ 要登录 —— `client_id` 空；
/// 快过期（30 秒内）而没有签发方或没有 refresh_token；没过期而访问令牌空。没有这一条（要联网发现才知道）· 配了 bearer
/// 变量或 `Authorization` 头（不走 OAuth）· 停用 ⇒ 照旧。要登录在哪几个号：令牌所在的那个号（项目级那一条看每个号）。
#[test]
fn a_stored_token_codex_cannot_use_means_log_in_again_in_that_account() {
    let d = test_home::scratch("codex-mcp-auth");
    let (a, b) = (d.join("a"), d.join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let cfg = "[mcp_servers.docs]\nurl = \"https://d.example/mcp\"\n\
               [mcp_servers.keyed]\nurl = \"https://k.example/mcp\"\nbearer_token_env_var = \"K\"\n\
               [mcp_servers.off]\nurl = \"https://o.example/mcp\"\nenabled = false\n";
    std::fs::write(a.join("config.toml"), cfg).unwrap();
    std::fs::write(b.join("config.toml"), cfg).unwrap();
    let now: u64 = 10_000_000;
    let entry = |name: &str, url: &str, more: Value| {
        let mut e =
            json!({"server_name": name, "server_url": url, "client_id": "c", "access_token": "t"});
        e.as_object_mut()
            .unwrap()
            .extend(more.as_object().unwrap().clone());
        e
    };
    // a 号：docs 过期了、没有 refresh_token ⇒ 要登录；keyed · off 同样过期，但不走 OAuth / 停用 ⇒ 照旧。
    let expired = json!({"expires_at": now - 1});
    std::fs::write(
        a.join(".credentials.json"),
        json!({
            "docs|1": entry("docs", "https://d.example/mcp", expired.clone()),
            "keyed|1": entry("keyed", "https://k.example/mcp", expired.clone()),
            "off|1": entry("off", "https://o.example/mcp", expired.clone()),
        })
        .to_string(),
    )
    .unwrap();
    // b 号：docs 过期了但有签发方与 refresh_token ⇒ Codex 自己续得上，照旧。
    std::fs::write(
        b.join(".credentials.json"),
        json!({"docs|1": entry("docs", "https://d.example/mcp",
            json!({"expires_at": now - 1, "refresh_token": "r", "issuer": "https://i.example"}))})
        .to_string(),
    )
    .unwrap();
    let proj = d.join("proj");
    std::fs::create_dir_all(proj.join(".codex")).unwrap();
    std::fs::write(
        proj.join(".codex").join("config.toml"),
        "[mcp_servers.docs]\nurl = \"https://d.example/mcp\"\n",
    )
    .unwrap();
    let look = McpLook {
        homes: vec![(Some("a".into()), a.clone()), (Some("b".into()), b.clone())],
        bare: None,
        now_ms: now,
    };
    let r = read_at(None, Some(&proj), &look);
    let got: Vec<(&str, &str, McpStatus, Vec<Option<String>>)> = r
        .entries
        .iter()
        .map(|e| (e.scope, e.name.as_str(), e.status, e.login_in.clone()))
        .collect();
    let s = |v: &[&str]| v.iter().map(|x| Some(x.to_string())).collect::<Vec<_>>();
    assert_eq!(
        got,
        [
            ("user", "docs", McpStatus::NeedsLogin, s(&["a"])),
            ("user", "keyed", McpStatus::Unknown, s(&[])),
            ("user", "off", McpStatus::Disabled, s(&[])),
            ("user", "docs", McpStatus::Unknown, s(&[])),
            ("user", "keyed", McpStatus::Unknown, s(&[])),
            ("user", "off", McpStatus::Disabled, s(&[])),
            ("project", "docs", McpStatus::NeedsLogin, s(&["a"])),
        ]
    );
    assert_eq!(r.entries[0].seen_ms, Some(now));

    // 没过期、访问令牌在 ⇒ 用得上；client_id 空 ⇒ 用不了；没有这一条 ⇒ 判不了、照旧。
    let one = |e: Value| {
        std::fs::write(
            a.join(".credentials.json"),
            json!({"docs|1": e}).to_string(),
        )
        .unwrap();
        read_at(None, None, &look).entries[0].status
    };
    assert_eq!(
        one(entry(
            "docs",
            "https://d.example/mcp",
            json!({"expires_at": now + 3_600_000})
        )),
        McpStatus::Unknown
    );
    assert_eq!(
        one(entry(
            "docs",
            "https://d.example/mcp",
            json!({"expires_at": now + 10_000})
        )),
        McpStatus::NeedsLogin
    );
    assert_eq!(
        one(entry(
            "docs",
            "https://d.example/mcp",
            json!({"client_id": " "})
        )),
        McpStatus::NeedsLogin
    );
    assert_eq!(
        one(entry(
            "docs",
            "https://other.example/mcp",
            json!({"expires_at": 1})
        )),
        McpStatus::Unknown
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// `config.toml` 只有一个读法（`accounts::config_doc`）：Codex 这一家的源码里把原文解成 `toml_edit` 文档的只有那一处，
/// MCP 面读改那几格也经它（`servers_in` · `server_set` 判读不读得懂跟目录信任 · 钩子信任 · 插件开关是同一下）。
#[test]
fn config_toml_is_parsed_in_exactly_one_place() {
    let codex = guard_core::addr::backend_src_root().join("agents/codex");
    let mut at: Vec<String> = Vec::new();
    for (path, raw) in guard_core::scan_tree!(&codex, &["rs"]) {
        let code = guard_core::production_code(&raw);
        let n = [
            "parse::<DocumentMut>",
            "parse::<toml_edit::DocumentMut>",
            "DocumentMut::from_str",
        ]
        .iter()
        .map(|p| code.matches(p).count())
        .sum::<usize>();
        let name = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        at.extend(std::iter::repeat_n(name, n));
    }
    assert_eq!(at, ["accounts.rs"], "config.toml 的解法不止一处：{at:?}");
}
