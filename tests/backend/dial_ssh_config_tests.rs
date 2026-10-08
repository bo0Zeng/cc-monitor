//! 「`ssh -G` 解析与 `~/.ssh/config` 读取搬进本机常驻后端」—— `dial/ssh_config.rs` 的规则判据（从 monitor `stream_source/tier1_tests.rs` 原样搬来）。
use super::*;

/// ★ 那一处读出来的东西**只许是别名**：配置里的敏感值一个都不许流出去。
///
/// 既有三条行为判据钉的是「别名抽得对」；本条钉反面 ——
/// 把 `IdentityFile` / `HostName` / `ProxyCommand` / `User` 一起喂进去，
/// 输出里**不许出现它们的值**。改法一旦变成「收集每条指令的第二个 token」，
/// 私钥路径与跳板机命令就会经 `ssh-config-aliases` 一路到前端。
#[test]
fn parse_host_aliases_never_leaks_sensitive_values() {
    let cfg = "\
Host box
    HostName 192.0.2.7
    User alice
    IdentityFile ~/.ssh/id_ed25519_secret
    ProxyCommand ssh -W %h:%p jump.example.com
    Port 2222
";
    let aliases = parse_host_aliases(cfg);
    assert_eq!(aliases, vec!["box"], "只该抽出别名本身");
    for leak in [
        "192.0.2.7",
        "alice",
        "id_ed25519_secret",
        "jump.example.com",
        "2222",
    ] {
        assert!(
            !aliases.iter().any(|a| a.contains(leak)),
            "别名清单里出现了 `{leak}` —— 那是 `~/.ssh/config` 里的敏感值，\n\
                 它会经 `ssh-config-aliases` 直接到前端（并被渲染/记日志）。\n\
                 抽取规则只该看 `Host` 行。"
        );
    }
}

/// host 别名解析：取 Host 行 token、排除通配 / `!`、去重保序、跳过非 Host 指令。
#[test]
fn parse_host_aliases_basics() {
    let cfg = "\
# comment
Host pi server1 server2
    HostName 1.2.3.4
    User pi

Host=eqsign
    Port 2222

Host *.internal wild*card prod ?q !neg
    User admin

Host prod
    User dup
";
    let aliases = parse_host_aliases(cfg);
    // pi/server1/server2 来自第一块；eqsign 用 `=` 分隔；prod 出现两次只留一次；
    // `*.internal` / `wild*card` / `?q` / `!neg` 全被通配/否定规则排除。
    assert_eq!(aliases, vec!["pi", "server1", "server2", "eqsign", "prod"]);
}

/// 排除字面量 `*`（catch-all）。
#[test]
fn parse_host_aliases_excludes_star() {
    let aliases = parse_host_aliases("Host *\n    User x\n");
    assert!(aliases.is_empty());
}

/// 没有任何 Host 指令 → 空。
#[test]
fn parse_host_aliases_empty_when_no_host() {
    let aliases = parse_host_aliases("# just comments\nUser nobody\nPort 22\n");
    assert!(aliases.is_empty());
}

// === F57：智能聚合 ===

fn rh(host: &str, key: Option<&str>, user: &str, pj: Option<&str>) -> ResolvedHost {
    ResolvedHost {
        host: host.into(),
        port: 22,
        user: user.into(),
        key_path: key.map(String::from),
        proxy_jump: pj.map(String::from),
    }
}

#[test]
fn alias_base_variants() {
    assert_eq!(alias_base("devbox-lan"), "devbox");
    assert_eq!(alias_base("devbox_wan"), "devbox");
    assert_eq!(alias_base("devbox.internal"), "devbox");
    assert_eq!(alias_base("pi"), "pi");
}

#[test]
fn aggregate_same_machine_multi_address() {
    // devbox-lan / devbox-wan 同 key+user+基名 → 聚合成 1 台多地址;pi 基名不同 → 独立。
    let groups = aggregate_ssh_hosts(vec![
        ("devbox-lan".into(), rh("192.0.2.2", Some("/k"), "user", None)),
        (
            "devbox-wan".into(),
            rh("devbox.example.com", Some("/k"), "user", None),
        ),
        ("pi".into(), rh("pi.local", Some("/k"), "user", None)),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].label, "devbox");
    assert_eq!(groups[0].host, "192.0.2.2");
    assert_eq!(groups[0].addresses, vec!["devbox.example.com".to_string()]);
    let aliases: Vec<&str> = groups[0].members.iter().map(|m| m.alias.as_str()).collect();
    assert_eq!(aliases, vec!["devbox-lan", "devbox-wan"]);
    assert_eq!(groups[1].label, "pi");
    assert!(groups[1].addresses.is_empty(), "单机组无备用地址");
}

#[test]
fn aggregate_different_key_not_merged() {
    // 同基名但不同 key → 不聚合(一把 key 一台机)。
    let groups = aggregate_ssh_hosts(vec![
        ("web-a".into(), rh("1.1.1.1", Some("/ka"), "u", None)),
        ("web-b".into(), rh("2.2.2.2", Some("/kb"), "u", None)),
    ]);
    assert_eq!(groups.len(), 2, "不同 key → 独立");
    // F57-1：单成员组用**完整别名**当 label(否则都叫 web → 前端落卡碰撞丢一台)。
    assert_eq!(groups[0].label, "web-a");
    assert_eq!(groups[1].label, "web-b");
}

#[test]
fn parse_ssh_g_proxyjump_and_fields() {
    let with = parse_ssh_g_output(
        "hostname 1.2.3.4\nport 2222\nuser pi\nproxyjump bastion\n",
        "a",
    );
    assert_eq!(with.host, "1.2.3.4");
    assert_eq!(with.port, 2222);
    assert_eq!(with.user, "pi");
    assert_eq!(with.proxy_jump.as_deref(), Some("bastion"));
    // none(大小写不敏感)→ 无跳板;无 proxyjump 行 → None。
    assert_eq!(
        parse_ssh_g_output("hostname h\nproxyjump None\n", "a").proxy_jump,
        None
    );
    assert_eq!(
        parse_ssh_g_output("hostname h\nuser u\n", "a").proxy_jump,
        None
    );
    // hostname/port 缺 → 回退别名 / 22。
    let fb = parse_ssh_g_output("user u\n", "myalias");
    assert_eq!(fb.host, "myalias");
    assert_eq!(fb.port, 22);
}

#[test]
fn aggregate_proxyjump_and_dedup() {
    // 同 host 去重;jump 取组内首个非空 proxyjump。
    let groups = aggregate_ssh_hosts(vec![
        (
            "devbox-lan".into(),
            rh("192.0.2.2", Some("/k"), "u", Some("bastion")),
        ),
        ("devbox-wan".into(), rh("192.0.2.2", Some("/k"), "u", None)),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].jump.as_deref(), Some("bastion"));
    assert!(groups[0].addresses.is_empty(), "同 host → 去重无额外地址");
}

/// allowlist：合法字符通过，含空格 / 选项前缀 / shell 元字符的别名被拒。
///
/// 要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；①形。
#[test]
fn is_safe_alias_allowlist() {
    assert!(is_safe_alias("pi"));
    assert!(is_safe_alias("my-host.example.com"));
    assert!(is_safe_alias("user@host:22"));
    assert!(is_safe_alias("a_b.c-1"));

    assert!(!is_safe_alias(""));
    assert!(!is_safe_alias("has space"));
    assert!(!is_safe_alias("a;b")); // shell metachar
    assert!(!is_safe_alias("a$b"));
    assert!(!is_safe_alias("a/b")); // 路径分隔不在 allowlist
    assert!(!is_safe_alias("a&b"));
    // 以 `-` 开头本身在 allowlist 内（`-` 是合法字符），option-injection 由
    // `resolve` 里单独的 starts_with('-') 检查兜住，这里只验字符集。
    assert!(is_safe_alias("-oProxyCommand"));
}

/// `~` 展开：`~` / `~/x` 展开到 home；非 `~` 前缀原样。
#[test]
fn expand_tilde_basics() {
    #[allow(deprecated)]
    let home = std::env::home_dir().expect("home dir for test");
    assert_eq!(expand_tilde("~"), home);
    assert_eq!(
        expand_tilde("~/.ssh/id_ed25519"),
        home.join(".ssh/id_ed25519")
    );
    // 非 ~ 前缀原样（不是 home-relative）。
    assert_eq!(
        expand_tilde("/etc/ssh/key"),
        std::path::PathBuf::from("/etc/ssh/key")
    );
    // `~user` 形式（非 `~/`）不展开（我们只处理自己的 home）。
    assert_eq!(
        expand_tilde("~otheruser/key"),
        std::path::PathBuf::from("~otheruser/key")
    );
}

/// 「成品的两侧对拍」：三条 `ssh-config-*` 的线上形状 == 跨语言金样 `tests/__fixtures__/ssh-config.golden.json`
/// （TS 解码器 `src/frontend/ui/ssh-config-reads.ts` 读同一份）。成品由生产构造器出（`answer_*` 用的同一个 `to_value` ＋ 聚合）。
#[test]
fn the_three_products_match_the_cross_language_golden() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../__fixtures__/ssh-config.golden.json")).unwrap();
    let resolved = parse_ssh_g_output(
        "hostname 192.0.2.2\nport 2222\nuser user\nproxyjump bastion\nidentityfile /nonexistent/mig1-key\n",
        "devbox-lan",
    );
    let mut groups = aggregate_ssh_hosts(vec![
        ("devbox-lan".into(), resolved.clone()),
        ("devbox-wan".into(), rh("devbox.example.com", None, "user", None)),
        ("pi".into(), rh("pi.local", None, "pi", None)),
    ]);
    // 列表里已有 devbox 的第二个地址那一台 ⇒ devbox 那一组「已在列表里」；pi 不在。
    mark_in_list(
        &mut groups,
        &[Known {
            host: "devbox.example.com".into(),
            user: "user".into(),
            port: 22,
        }],
    );
    let got = serde_json::json!({
        "ssh-config-aliases": { "aliases": parse_host_aliases("Host devbox-lan devbox-wan\nHost pi *\n") },
        "ssh-config-resolve": to_value(&resolved),
        "ssh-config-import": { "groups": to_value(&groups) },
    });
    assert_eq!(
        got, golden,
        "三条成品的线上形状变了 ⇒ 金样与 TS 解码器同拍改"
    );
}

/// 「已在列表里」按地址 ＋ 用户 ＋ 端口认（不只按名字）：同地址换了用户 / 端口的不算；去首尾空白比。
#[test]
fn in_list_is_judged_by_host_user_and_port() {
    let mk = || aggregate_ssh_hosts(vec![("pi".into(), rh("pi.local", None, "pi", None))]);
    for (known, want) in [
        (
            Known {
                host: "pi.local".into(),
                user: "pi".into(),
                port: 22,
            },
            true,
        ),
        (
            Known {
                host: " pi.local ".into(),
                user: "pi ".into(),
                port: 22,
            },
            true,
        ),
        (
            Known {
                host: "pi.local".into(),
                user: "root".into(),
                port: 22,
            },
            false,
        ),
        (
            Known {
                host: "pi.local".into(),
                user: "pi".into(),
                port: 2222,
            },
            false,
        ),
        (
            Known {
                host: "other".into(),
                user: "pi".into(),
                port: 22,
            },
            false,
        ),
    ] {
        let mut g = mk();
        mark_in_list(&mut g, std::slice::from_ref(&known));
        assert_eq!(g[0].in_list, want, "{known:?}");
    }
    // 不交列表 ⇒ 拒（界面那一侧恒交，缺了是两侧对不上）。
    assert_eq!(
        answer_import(&serde_json::json!({}))
            .map(|_| ())
            .unwrap_err()
            .0,
        "bad_args"
    );
}
