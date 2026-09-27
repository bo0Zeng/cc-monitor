use super::*;

/// ★★ **本机 `~/.ssh` 的读面：恰好一处，且只读 `config`**〔audit-0805 08-08，Phase G 第 85 件〕。
///
/// # 「谁能读」这一侧此前只有半张表
///
/// 本会话把「谁能**写**」（`write_site_registry`）、「谁能**执行**」
/// （`exec_site_registry` / 本机起进程 / 构建期执行面）都钉成了默认拒绝，
/// 而**读**那一侧只有两块：backend 的 `readonly_guard`（整个 crate 不许写），
/// 与 monitor 的 `local_read_surface_registry` —— 而后者的人群是**五个手写针**，
/// 全部对着 **claude 目录**（`claude_dir` / `CLAUDE_CONFIG_DIR` / `.claude` …）。
///
/// ⇒ 用户机器上**别的**敏感目录不在任何人群里。08-08 实测：monitor 生产段里
/// 碰本机 `.ssh` 的只有**一处**（本文件读 `~/.ssh/config` 列别名给前端），
/// 而**没有任何判据钉住它保持一处** —— 加一句读 `~/.ssh/id_ed25519` 或
/// `known_hosts`，全仓判据一条不会红。
///
/// ⚠ `pubkey.rs` 里那段 `$HOME/.ssh` **刻意不在人群里**：它是拼给**远端**执行的
/// shell 串（往远端 `authorized_keys` 追加公钥），归 `exec_site_registry` 管。
/// 人群只取**本机路径构造**（`join(".ssh")` / `expand_tilde("~/.ssh`），
/// 不取字符串里出现的 `.ssh` —— 否则「远端的事」会被算成「读了用户本机的东西」。
#[test]
fn the_local_ssh_read_surface_is_exactly_one_site() {
    /// `(文件, 碰的是谁的 `.ssh`, 读/写的是什么, 为什么可以)`。**默认拒绝**。
    ///
    /// ⚠ 人群按**目录名本身**取（下面用 `contains_word(".ssh")`），
    /// 不按「路径是怎么拼的」取 —— 08-08 第一版按 `join(".ssh"` / `~/.ssh` 两种**写法**
    /// 取样，被 `needle_anchor` 棘轮当场判为「语料上的裸匹配」。棘轮是对的：
    /// 按写法取样正是本工作区一直在治的病（`join(SSH_DIR)` 换个常量就绕过去了）。
    /// ⇒ 人群取「谁提到了这个目录」，**本机还是远端由登记回答**，不由语法判。
    const SSH_SITES: &[(&str, &str, &str, &str)] = &[
        (
            "ssh_source.rs",
            "本机（用户自己的机器）",
            "读 `~/.ssh/config`",
            "Tier 1 的「导入别名」：只解析 Host 行拿到一份可点的别名清单，\
                 不展开 Include、不解析 Match、**不碰任何密钥文件**；真正的参数解析交给 `ssh -G`",
        ),
        (
            "pubkey.rs",
            "**远端**（用户的服务器）",
            "拼一段往远端 `~/.ssh/authorized_keys` 追加公钥的 shell 串",
            "免密登录的安装动作；命令串本身由 `exec_site_registry` 管（它是 `Builder` 类），\
                 这里登记是为了让「谁碰 .ssh」这张表**没有沉默的第二类**",
        ),
    ];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree!(&root, &["rs"]);
    assert!(
        files.len() >= 40,
        "只扫到 {} 个 .rs —— 遍历坏了，本条会零命中地绿",
        files.len()
    );
    // 被测的 `ssh_source.rs` —— 它走普通遍历**本来就在**人群里，这一份是补的第二份。
    // ⚠ 〔`P4` 2026-09-21〕先前这一行写着「本文件被 `scan_tree!` 按构造摘掉了
    //   （它是调用者）⇒ 手动补回」。那一刀**在这一处不生效**（判据由 `#[path]`
    //   挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中），而且今天的调用者是本判据文件、
    //   不是 `ssh_source.rs`。重复在这里无害（下面 `found` 排序后 `dedup`），
    //   而它**刻意不删**：它把「被测那一份一定在人群里」钉成一件不依赖扫描面的事。
    // ⚠ **变量名刻意不叫 `me`**：`needle_anchor` 棘轮按**文件文本**推断「语料变量」，
    // 而本文件另一条判据里有一句刻意演示旧近似的 `me.split("\n#[cfg(test)]")`（见 2890 行附近）。
    // 叫 `me` 会让那处旧 split 被算成「语料变量上的裸匹配」，棘轮当场红 —— 08-08 实测过。
    // 名字影响判据结果，这件事本身值得写下来。
    let self_src = std::fs::read_to_string(root.join("ssh_source.rs")).expect("读不到本文件");
    let mut corpus: Vec<(String, String)> = files
        .iter()
        .map(|(p, s)| {
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                s.clone(),
            )
        })
        .collect();
    corpus.push(("ssh_source.rs".to_string(), self_src));

    let dot = format!(".{}", "ssh");
    let mut found: Vec<String> = Vec::new();
    for (name, src) in &corpus {
        for l in guard_core::production_code(src).lines() {
            // `contains_word`：`.sshx` 之类不算，而 `~/.ssh/config`、`join(".ssh")`、
            // `"$HOME/.ssh"` 都算 —— 它认的是**那个目录名**，不是某一种拼法。
            if guard_core::contains_word(l, &dot) {
                found.push(name.clone());
            }
        }
    }
    found.sort();
    found.dedup();
    let mut declared: Vec<String> = SSH_SITES.iter().map(|(f, _, _, _)| f.to_string()).collect();
    declared.sort();
    assert_eq!(
        found, declared,
        "本机 `~/.ssh` 的读面变了。\n\
             实测：{found:?}    登记：{declared:?}\n\
             ★ 多出来的：那是**用户机器上最敏感的目录之一** —— 私钥、`known_hosts`、\n\
             `authorized_keys` 都在里面。要读就登记，并说清「读的是什么、为什么可以读」。\n\
             ⚠ 少了的：本文件那一处若被改写/挪走，说明「导入别名」这条路变了形，\n\
             上面那三条 `parse_host_aliases` 的行为判据可能已经不在生产路径上。\n\
             ⚠ 人群按**目录名**取，本机/远端由登记的第二列回答 —— \n\
             别再退回按拼法取样（那是 `needle_anchor` 棘轮 08-08 当场拦下的写法）。"
    );
}

/// ★ 那一处读出来的东西**只许是别名**：配置里的敏感值一个都不许流出去。
///
/// 既有三条行为判据钉的是「别名抽得对」；本条钉反面 ——
/// 把 `IdentityFile` / `HostName` / `ProxyCommand` / `User` 一起喂进去，
/// 输出里**不许出现它们的值**。改法一旦变成「收集每条指令的第二个 token」，
/// 私钥路径与跳板机命令就会经 `list_ssh_host_aliases` 一路到前端。
#[test]
fn parse_host_aliases_never_leaks_sensitive_values() {
    let cfg = "\
Host box
    HostName 10.0.0.7
    User alice
    IdentityFile ~/.ssh/id_ed25519_secret
    ProxyCommand ssh -W %h:%p jump.example.com
    Port 2222
";
    let aliases = parse_host_aliases(cfg);
    assert_eq!(aliases, vec!["box"], "只该抽出别名本身");
    for leak in [
        "10.0.0.7",
        "alice",
        "id_ed25519_secret",
        "jump.example.com",
        "2222",
    ] {
        assert!(
            !aliases.iter().any(|a| a.contains(leak)),
            "别名清单里出现了 `{leak}` —— 那是 `~/.ssh/config` 里的敏感值，\n\
                 它会经 `list_ssh_host_aliases` 直接到前端（并被渲染/记日志）。\n\
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
    assert_eq!(alias_base("aya_wan"), "devbox");
    assert_eq!(alias_base("devbox.internal"), "devbox");
    assert_eq!(alias_base("pi"), "pi");
}

#[test]
fn aggregate_same_machine_multi_address() {
    // devbox-lan / devbox-wan 同 key+user+基名 → 聚合成 1 台多地址;pi 基名不同 → 独立。
    let groups = aggregate_ssh_hosts(vec![
        ("devbox-lan".into(), rh("10.0.0.2", Some("/k"), "user", None)),
        (
            "devbox-wan".into(),
            rh("devbox.example.com", Some("/k"), "user", None),
        ),
        ("pi".into(), rh("pi.local", Some("/k"), "user", None)),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].label, "devbox");
    assert_eq!(groups[0].host, "10.0.0.2");
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
            rh("10.0.0.2", Some("/k"), "u", Some("bastion")),
        ),
        ("devbox-wan".into(), rh("10.0.0.2", Some("/k"), "u", None)),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].jump.as_deref(), Some("bastion"));
    assert!(groups[0].addresses.is_empty(), "同 host → 去重无额外地址");
}

/// allowlist：合法字符通过，含空格 / 选项前缀 / shell 元字符的别名被拒。
///
/// 〔IV1 · V121〕要求住址：`INVARIANTS §47`（外部值拼进 shell / 交给对端之前本侧先过放行判定）；①形。
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
    // resolve_ssh_host 里单独的 starts_with('-') 检查兜住，这里只验字符集。
    assert!(is_safe_alias("-oProxyCommand"));
}

/// `~` 展开：`~` / `~/x` 展开到 home；非 `~` 前缀原样。
#[test]
fn expand_tilde_basics() {
    let home = dirs::home_dir().expect("home dir for test");
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

/// RemoteConfig 反序列化：camelCase key + 空串可选字段归 None + port 默认 22 + 忽略 enabled。
#[test]
fn remote_config_deserializes_frontend_shape() {
    // 前端 collect() 的形状：所有字段都在，可选字段可能是空串，外加 enabled。
    let json = r#"{
            "enabled": true,
            "host": "pi.local",
            "port": 2200,
            "user": "pi",
            "keyPath": "",
            "backendPath": "/home/pi/cc-monitor-backend",
            "hostKeyFingerprint": ""
        }"#;
    let cfg: RemoteConfig = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(cfg.host, "pi.local");
    assert_eq!(cfg.port, 2200);
    assert_eq!(cfg.user, "pi");
    assert_eq!(cfg.key_path, None, "空串 keyPath → None");
    // 〔E2 · V41〕盘上旧的 `backendPath` 不读、不报错（那一格删了）。
    assert_eq!(cfg.host_key_fingerprint, None, "空串指纹 → None");
}

/// RemoteConfig：缺 port 时默认 22，非空可选字段保留为 Some。
#[test]
fn remote_config_defaults_and_some() {
    let json = r#"{
            "host": "h",
            "user": "u",
            "keyPath": "C:\\k",
            "backendPath": "d",
            "hostKeyFingerprint": "SHA256:abc"
        }"#;
    let cfg: RemoteConfig = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(cfg.port, 22, "缺 port → 默认 22");
    assert_eq!(cfg.key_path.as_deref(), Some("C:\\k"));
    assert_eq!(cfg.host_key_fingerprint.as_deref(), Some("SHA256:abc"));
}

/// ★★ **hello-then-die 的后端不许把退避永远按在 2 秒**〔audit-0805 F05 / 报告 I-1〕。
///
/// `connected` 是收到 hello 的那一刻置位的。一个「发完 hello 就死」的 backend
/// （小机器整读 jsonl 触发 OOM 被杀，正是本区 F04 治的那条链）每轮都算「连上过」
/// ⇒ 退避每次重置回 2 秒 ⇒ **永远不增长**，而每次重连要付 3 次完整 SSH 登录
/// ⇒ 约 **90 次握手/分钟/台**，正好砸在那台已经撑不住的机器上。
///
/// ⇒ 判据必须是「**活过多久**」，不是「握没握上手」。
#[test]
fn a_backend_that_dies_right_after_hello_does_not_keep_resetting_the_backoff() {
    // hello 收到了，但连接只活了 1 秒 —— 这正是 hello-then-die。
    assert!(
        !should_reset_backoff(true, Duration::from_secs(1)),
        "收到 hello 但只活了 1 秒就重置退避 —— 那是 I-1 那个自激循环：\n\
             每分钟约 90 次 SSH 握手砸在一台已经 OOM 的机器上。"
    );
    // 活够了才算站住。
    assert!(
        should_reset_backoff(true, MIN_HEALTHY_UPTIME),
        "活过 MIN_HEALTHY_UPTIME 还不重置 —— 正常的长连接会被误当成 flapping"
    );
    assert!(should_reset_backoff(true, Duration::from_secs(600)));
    // 从没握上手的，活多久都不算健康（否则「连了很久但一直没 hello」会被当成好连接）。
    assert!(
        !should_reset_backoff(false, Duration::from_secs(600)),
        "没收到过 hello 却算健康 —— 两个条件是**与**不是或"
    );
}

/// next_backoff：翻倍直到封顶 RECONNECT_MAX(30s)，封顶后饱和不再增长。
#[test]
fn next_backoff_doubles_then_caps() {
    assert_eq!(next_backoff(Duration::from_secs(2)), Duration::from_secs(4));
    assert_eq!(next_backoff(Duration::from_secs(4)), Duration::from_secs(8));
    // 16s*2=32s 被封顶到 30s。
    assert_eq!(
        next_backoff(Duration::from_secs(16)),
        Duration::from_secs(30)
    );
    // 已在上界 → 翻倍后仍被 min 拉回 30s（饱和）。
    assert_eq!(
        next_backoff(Duration::from_secs(30)),
        Duration::from_secs(30)
    );
}

// === F45：地址解析 + endpoints ===

fn ep(host: &str, port: u16) -> Endpoint {
    Endpoint {
        host: host.into(),
        port,
    }
}

#[test]
fn parse_address_line_four_forms() {
    assert_eq!(parse_address_line("pi.local", 22), Some(ep("pi.local", 22)));
    assert_eq!(
        parse_address_line("10.0.0.2:2222", 22),
        Some(ep("10.0.0.2", 2222))
    );
    // [IPv6]:port 与 [IPv6]
    assert_eq!(
        parse_address_line("[fe80::1]:2200", 22),
        Some(ep("fe80::1", 2200))
    );
    assert_eq!(parse_address_line("[::1]", 22), Some(ep("::1", 22)));
    // 裸 IPv6（trap #7：>1 冒号不误当 host:port）
    assert_eq!(parse_address_line("::1", 22), Some(ep("::1", 22)));
    assert_eq!(parse_address_line("fe80::1", 22), Some(ep("fe80::1", 22)));
}

#[test]
fn parse_address_line_rejects_garbage() {
    assert_eq!(parse_address_line("", 22), None);
    assert_eq!(parse_address_line("   ", 22), None);
    assert_eq!(parse_address_line("h:notaport", 22), None);
    assert_eq!(parse_address_line(":2222", 22), None); // 无 host
    assert_eq!(parse_address_line("[", 22), None); // 未闭合方括号
    assert_eq!(parse_address_line("[]:22", 22), None); // 空 host
    assert_eq!(parse_address_line("[fe80::1]:bad", 22), None); // 端口非法
}

#[test]
fn endpoints_host_first_dedup_preserve_order() {
    let cfg = RemoteConfig {
        host: "pi.local".into(),
        label: "pi".into(),
        port: 22,
        user: "pi".into(),
        key_path: None,
        host_key_fingerprint: None,
        addresses: vec![
            "10.0.0.2".into(),
            "pi.local".into(),    // 与 host 重复 → 去重
            "10.0.0.2:22".into(), // 与上面同 (host,port) → 去重
            "pub.example.com:2222".into(),
            "".into(), // 空行跳过
        ],
        jump: None,
    };
    assert_eq!(
        cfg.endpoints(),
        vec![
            ep("pi.local", 22),
            ep("10.0.0.2", 22),
            ep("pub.example.com", 2222),
        ]
    );
}

#[test]
fn endpoints_empty_addresses_is_just_host() {
    let cfg = RemoteConfig {
        host: "h".into(),
        label: String::new(),
        port: 2200,
        user: "u".into(),
        key_path: None,
        host_key_fingerprint: None,
        addresses: vec![],
        jump: None,
    };
    assert_eq!(cfg.endpoints(), vec![ep("h", 2200)]);
}

// === F45：winner_order（last-good 排首）===

#[test]
fn winner_order_puts_last_good_first() {
    let eps = vec![ep("a", 22), ep("b", 22), ep("c", 22)];
    // last-good = b → b 排首，其余保序
    assert_eq!(
        winner_order(eps.clone(), Some(&ep("b", 22))),
        vec![ep("b", 22), ep("a", 22), ep("c", 22)]
    );
    // last-good = 已移除的 endpoint → 无视，原序
    assert_eq!(
        winner_order(eps.clone(), Some(&ep("gone", 22))),
        eps.clone()
    );
    // 无 last-good → 原序
    assert_eq!(winner_order(eps.clone(), None), eps);
    // last-good 已在首位 → 幂等
    assert_eq!(winner_order(eps.clone(), Some(&ep("a", 22))), eps);
}

// === F45：winner_address（喂 remote-launch 的拨号地址）===

fn cfg_with(label: &str, host: &str, port: u16, addresses: Vec<String>) -> RemoteConfig {
    RemoteConfig {
        host: host.into(),
        label: label.into(),
        port,
        user: "u".into(),
        key_path: None,
        host_key_fingerprint: None,
        addresses,
        jump: None,
    }
}

#[test]
fn winner_address_falls_back_to_host_when_no_last_good() {
    let cfg = cfg_with("wa-none", "h.example", 2200, vec!["10.0.0.9".into()]);
    assert_eq!(winner_address(&cfg), ep("h.example", 2200));
}

#[test]
fn winner_address_uses_last_good_then_invalidates_on_config_change() {
    // 用独立 origin 避免与其它测试共享的 last-good store 串味。
    let cfg = cfg_with("wa-lg", "h.example", 22, vec!["10.0.0.9".into()]);
    record_last_good("wa-lg", &ep("10.0.0.9", 22));
    assert_eq!(
        winner_address(&cfg),
        ep("10.0.0.9", 22),
        "已连过 → last-good 胜者"
    );
    // 配置改掉备用地址 → 旧 last-good 不在 endpoints 里 → 回退 host。
    let cfg2 = cfg_with("wa-lg", "h.example", 22, vec![]);
    assert_eq!(
        winner_address(&cfg2),
        ep("h.example", 22),
        "配置变更失效 last-good"
    );
}

// 〔E2 · V28〕`backendPath` 那一格删了（落点恒是 `relay_route_core::BACKEND_LANDING_SHELL`）⇒ 从前那道放行判定
//   `backend_path_for_shell`〔散文墓碑〕与它的两条判据没有外来值可判，一起删了。
