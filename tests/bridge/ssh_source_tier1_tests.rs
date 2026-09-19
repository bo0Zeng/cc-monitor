use super::*;

/// ★★ **本机 `~/.ssh` 的读面：恰好一处，且只读 `config`**〔audit-0805 08-08，Phase G 第 85 件〕。
///
/// # 「谁能读」这一侧此前只有半张表
///
/// 本会话把「谁能**写**」（`write_site_registry`）、「谁能**执行**」
/// （`exec_site_registry` / 本机起进程 / 构建期执行面）都钉成了默认拒绝，
/// 而**读**那一侧只有两块：daemon 的 `readonly_guard`（整个 crate 不许写），
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
    // 本文件被 `scan_tree!` 按构造摘掉了（它是调用者）⇒ 手动补回：人群里必须有它。
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
    let mut declared: Vec<String> =
        SSH_SITES.iter().map(|(f, _, _, _)| f.to_string()).collect();
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
    assert_eq!(alias_base("aya-lan"), "aya");
    assert_eq!(alias_base("aya_wan"), "aya");
    assert_eq!(alias_base("aya.internal"), "aya");
    assert_eq!(alias_base("pi"), "pi");
}

#[test]
fn aggregate_same_machine_multi_address() {
    // aya-lan / aya-wan 同 key+user+基名 → 聚合成 1 台多地址;pi 基名不同 → 独立。
    let groups = aggregate_ssh_hosts(vec![
        ("aya-lan".into(), rh("10.0.0.2", Some("/k"), "zbl", None)),
        (
            "aya-wan".into(),
            rh("aya.example.com", Some("/k"), "zbl", None),
        ),
        ("pi".into(), rh("pi.local", Some("/k"), "zbl", None)),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].label, "aya");
    assert_eq!(groups[0].host, "10.0.0.2");
    assert_eq!(groups[0].addresses, vec!["aya.example.com".to_string()]);
    let aliases: Vec<&str> = groups[0].members.iter().map(|m| m.alias.as_str()).collect();
    assert_eq!(aliases, vec!["aya-lan", "aya-wan"]);
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
            "aya-lan".into(),
            rh("10.0.0.2", Some("/k"), "u", Some("bastion")),
        ),
        ("aya-wan".into(), rh("10.0.0.2", Some("/k"), "u", None)),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].jump.as_deref(), Some("bastion"));
    assert!(groups[0].addresses.is_empty(), "同 host → 去重无额外地址");
}

/// allowlist：合法字符通过，含空格 / 选项前缀 / shell 元字符的别名被拒。
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
            "daemonPath": "/home/pi/cc-monitor-remote",
            "hostKeyFingerprint": ""
        }"#;
    let cfg: RemoteConfig = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(cfg.host, "pi.local");
    assert_eq!(cfg.port, 2200);
    assert_eq!(cfg.user, "pi");
    assert_eq!(cfg.key_path, None, "空串 keyPath → None");
    assert_eq!(cfg.daemon_path, "/home/pi/cc-monitor-remote");
    assert_eq!(cfg.host_key_fingerprint, None, "空串指纹 → None");
}

/// RemoteConfig：缺 port 时默认 22，非空可选字段保留为 Some。
#[test]
fn remote_config_defaults_and_some() {
    let json = r#"{
            "host": "h",
            "user": "u",
            "keyPath": "C:\\k",
            "daemonPath": "d",
            "hostKeyFingerprint": "SHA256:abc"
        }"#;
    let cfg: RemoteConfig = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(cfg.port, 22, "缺 port → 默认 22");
    assert_eq!(cfg.key_path.as_deref(), Some("C:\\k"));
    assert_eq!(cfg.host_key_fingerprint.as_deref(), Some("SHA256:abc"));
}

/// ★★ **hello-then-die 的 daemon 不许把退避永远按在 2 秒**〔audit-0805 F05 / 报告 I-1〕。
///
/// `connected` 是收到 hello 的那一刻置位的。一个「发完 hello 就死」的 daemon
/// （小机器整读 jsonl 触发 OOM 被杀，正是本区 F04 治的那条链）每轮都算「连上过」
/// ⇒ 退避每次重置回 2 秒 ⇒ **永远不增长**，而每次重连要付 3 次完整 SSH 登录
/// ⇒ 约 **90 次握手/分钟/台**，正好砸在那台已经撑不住的机器上。
///
/// ⇒ 判据必须是「**活过多久**」，不是「握没握上手」。
#[test]
fn a_daemon_that_dies_right_after_hello_does_not_keep_resetting_the_backoff() {
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

// F43：check_server_key 三分支——匹配接受 / 失配拒绝 / 无期望指纹 TOFU 接受，
// 且无论哪支都把实际指纹写回 observed cell（测试连接据此展示 + 固化）。
use russh::client::Handler as _; // check_server_key 是 trait 方法

fn handler_with(expected: Option<&str>) -> (ClientHandler, Arc<Mutex<Option<String>>>) {
    let observed = Arc::new(Mutex::new(None));
    let h = ClientHandler {
        expected_fingerprint: expected.map(String::from),
        observed_fingerprint: Arc::clone(&observed),
        stage_emitter: None,
        endpoint: None,
    };
    (h, observed)
}

/// 固定 ed25519 公钥 + 其 SHA256 指纹（ssh-keygen 一次性生成后固化进测试，
/// SAMPLE_FP 可用 `ssh-keygen -lf` 对 SAMPLE_PUB 独立复算核对）。
const SAMPLE_PUB: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEwdsNpXeLF3bjmkjNIpFsbGCxLntS8RsfA6BPOv/Ykv f43";
const SAMPLE_FP: &str = "SHA256:fPFSH7moeRu2I96lFjdo8lO2iB7KgVLtL4LXvHVZWDk";

fn sample_key() -> PublicKey {
    PublicKey::from_openssh(SAMPLE_PUB).expect("parse sample pubkey")
}

#[tokio::test]
async fn check_server_key_matching_fingerprint_accepts() {
    let (mut h, observed) = handler_with(Some(SAMPLE_FP));
    assert!(h.check_server_key(&sample_key()).await.unwrap());
    assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
}

#[tokio::test]
async fn check_server_key_matching_tolerates_trailing_whitespace() {
    let padded = format!("{SAMPLE_FP}\n  ");
    let (mut h, _observed) = handler_with(Some(&padded));
    assert!(
        h.check_server_key(&sample_key()).await.unwrap(),
        "尾随空白不应误判 MITM"
    );
}

#[tokio::test]
async fn check_server_key_mismatch_rejects_but_records() {
    let (mut h, observed) =
        handler_with(Some("SHA256:deadbeefwrongfingerprintvalueAAAAAAAAAAAA"));
    assert!(
        !h.check_server_key(&sample_key()).await.unwrap(),
        "失配必须拒绝"
    );
    // 失配也把实际指纹写回 cell（供「重置为 TOFU / 重新固化」）。
    assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
}

#[tokio::test]
async fn check_server_key_no_expected_tofu_accepts() {
    let (mut h, observed) = handler_with(None);
    assert!(
        h.check_server_key(&sample_key()).await.unwrap(),
        "TOFU 首连接受"
    );
    assert_eq!(observed.lock().unwrap().as_deref(), Some(SAMPLE_FP));
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
        daemon_path: "d".into(),
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
        daemon_path: "d".into(),
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

// === F45：race_connect 编排（可控 mock，不依赖真 SSH）===
// 用一个内存 TCP listener 模拟「快地址」（accept 即断=握手必失败但 TCP 连得上），
// 及不存在端口模拟「立即拒绝」；**本地静默对端**（`spawn_silent_peer`）模拟握手挂起。
// 断言编排语义：首个可用者决定结果、全失败聚合、看门狗生效。注：这些测走 race_connect
// 的错误路径（无真 SSH server 故握手都失败），验证的是编排（顺序/聚合/超时/取消），
// 非握手成功路径。
//
// 🔴 **本段的前提纪律**〔`K-R24` 09-04〕：这几条判据要的对端一律**自己起在 loopback 上**。
// 不许再靠「这台机器到某个外部地址是什么反应」—— 那是一条**没人建立、也没人检查**的
// 环境前提，前提不成立时它吐的红与「被测的东西真坏了」**长得一模一样**。
// ⚠ 判据不是靠一张「禁用哪些 IP」的词表守的（那种表迟早腐）：守它的是**门禁自己的
// 默认口径 `--network none`** —— 断网下还能绿，才说明这条判据的前提是它自己建立的。

use tokio::net::TcpListener;

async fn dead_port() -> u16 {
    // 绑后立即释放 → 该端口大概率无监听 → connect 立即 RST（快速失败）。
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

/// 一个「TCP 接了，但**永不吐 SSH 版本串**」的本地对端 —— 看门狗那条判据的前提。
///
/// # 为什么不是黑洞 IP〔`K-R24`〕
///
/// 旧写法拨 `10.255.255.1:22`，靠「这台机器到那个地址**有路由**、且包被静默丢弃」
/// 让 connect 挂起。那条前提是**环境性的，而它既不建立、也不检查**：沙箱默认
/// `--network none` 里没有那条路由 ⇒ connect 立刻 `ENETUNREACH` ⇒ 内层瞬间跑完
/// ⇒ 走 `race_connect` 的**聚合失败**支而不是 deadline 支
/// ⇒ 「看门狗坏了」与「本机没有到黑洞的路由」共用了同一个红。
///
/// # 它挂在哪一段（这一格是本修的要害）
///
/// 本对端 `accept()` 之后**一个字节都不回**。russh `client::connect_stream` 的次序是
/// **先写出自己的 `SSH-2.0-…` 标识，再停在「读对端标识」那一步等着**，而
/// `client::Config::default()` 的 `inactivity_timeout` 是 `None`（无客户端侧超时）
/// ⇒ 卡住的是**握手**，不是 TCP 连接。
/// （那一步的上游函数名此处刻意不点：它是仓外符号，点了就要进 `structural_scan` 的
/// 仓外名字登记表，而那张表不在本件写区 —— 机制上面已经说全，不靠那个名字承重。）
/// ★ 这才对得上断言原文那句「握手超时」—— 黑洞地址连 TCP connect 都没完成过，
///   它驱动的其实是「连接挂起」那条路，措辞却是「握手」那条路的。
///
/// 返回 `(地址, 痕迹)`。痕迹让**前提本身可被断言**，见 `PeerTrace`。
async fn spawn_silent_peer() -> (Endpoint, Arc<Mutex<PeerTrace>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let trace: Arc<Mutex<PeerTrace>> = Arc::new(Mutex::new(PeerTrace::default()));
    let trace_w = Arc::clone(&trace);
    tokio::spawn(async move {
        // 只接一条。收下之后读一次（记下对端的 SSH 标识），然后**攥着不放** ——
        // 既不回字节、也不主动关，让对端一直卡在「读对端标识」那一步。
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 128];
            if let Ok(n) = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await {
                let mut t = trace_w.lock().unwrap();
                t.banner = Some(String::from_utf8_lossy(&buf[..n]).into_owned());
                t.banner_at = Some(std::time::Instant::now());
            }
            // 继续读，但**永远不回一个字节** —— 客户端挂在「读对端标识」那一步时这一读
            // 一直 pend；等 `race_connect` 胜出/到点后 `abort_all` 把在飞那一路 drop 掉、
            // socket 随之关闭，这一读才以 EOF（或 reset）返回。
            // ⇒ 记下这一刻 = 记下「那条连接一直挂到被整批收走为止」。
            // ⚠ 循环而不是只读一次：客户端可能在标识之后紧跟着又写了 KEXINIT，
            //   只读一次会把「又来了几个字节」误读成「客户端还没走」。
            let mut sink = [0u8; 256];
            loop {
                match tokio::io::AsyncReadExt::read(&mut stream, &mut sink).await {
                    Ok(0) | Err(_) => {
                        trace_w.lock().unwrap().client_went_away_at =
                            Some(std::time::Instant::now());
                        break;
                    }
                    Ok(_) => continue,
                }
            }
            std::future::pending::<()>().await;
        }
    });
    (ep("127.0.0.1", addr.port()), trace)
}

/// 静默对端**被走到了什么程度**的痕迹〔`K-R24` 下一拍〕。
///
/// # 为什么判据不能只断言结果
///
/// `race_live_server_wins_when_a_hung_peer_is_first` 要证的是「首地址挂起也不吊死整批」。
/// 但它的**结果**（live 胜出）**从来不随环境翻转** —— 首地址不管是挂住、还是瞬间
/// `ENETUNREACH`、还是被 RST 拒了，live 都照样赢。⇒ **只断言结果的判据，在
/// 「那半根本没被驱动」时也是绿的** —— 那不是「红说不清是什么红」，是**「绿说不清测没测到」**。
///
/// ⇒ 出路不是把断言写得更狠，是**让夹具记下它被走到了什么程度**，判据去断言那个痕迹。
#[derive(Default, Clone)]
struct PeerTrace {
    /// 对端读到的首批字节（正常即客户端的 `SSH-2.0-…` 标识）。
    /// 有它 ⇒ TCP 早已连上、且卡的是**握手**那一段，不是 TCP 连接那一段。
    banner: Option<String>,
    /// 读到那批字节的时刻 —— 用来断言这事发生在竞速**进行中**，而不是事后。
    banner_at: Option<std::time::Instant>,
    /// 客户端那一侧先撒手了（socket 关掉 ⇒ 这边读到 EOF / reset）的时刻。
    /// 🔴 本夹具**自己从不主动关、也从不回一个字节** ⇒ 这一格有值 **⇔**
    /// 那条连接从收到标识起一直挂着，直到被 `race_connect` 的 `abort_all` 收走。
    /// **这才是「首地址真的挂住了」那件事本身**，而不是它的后果。
    client_went_away_at: Option<std::time::Instant>,
}

/// 等一个条件成立，最多 ~2s（立刻成立就立刻返回，不花这 2s）。
/// 用在断言「前提确已建立」之前 —— 免得把**调度抖动**读成「前提不成立」。
async fn settled(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

fn test_config() -> Arc<client::Config> {
    Arc::new(client::Config::default())
}

/// race_connect 的 Ok 分支持有不实现 Debug 的 Handle,不能直接 unwrap_err；
/// 这个 helper 压成错误串便于断言错误路径。
fn race_err(
    r: Result<
        (
            client::Handle<ClientHandler>,
            Arc<Mutex<Option<String>>>,
            Endpoint,
        ),
        String,
    >,
) -> String {
    match r {
        Ok(_) => panic!("expected Err, got a live connection"),
        Err(e) => e,
    }
}

#[tokio::test]
async fn race_all_dead_aggregates_errors() {
    let p1 = dead_port().await;
    let p2 = dead_port().await;
    let order = vec![ep("127.0.0.1", p1), ep("127.0.0.1", p2)];
    let err =
        race_err(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
    // trap #3：聚合报告，含「所有地址连接失败」且提到两个地址（至少首个立即失败）。
    assert!(err.contains("所有地址连接失败"), "应聚合: {err}");
    assert!(err.contains(&p1.to_string()), "应含首地址: {err}");
}

/// 看门狗：到点整批 abort、不吊死。对端是**本地静默 listener**（握手挂起）。
///
/// 🔴 这条判据的前提由它**自己建立**（`spawn_silent_peer`），并且**自己断言**。
/// 从前「红」这一个读数装着三件事，现在三件各有各的话：
///   ① **前提没建立**（对端没收到客户端标识）⇒ 「前提不成立……这条今天判不了」；
///   ② **看门狗吊死**（deadline 根本不兑现）⇒ 「3 秒内没返回」，当场红而不是把测试二进制挂住；
///   ③ **走错了支**（快速失败顺路带出措辞 / 措辞变了）⇒ 「没等到 deadline」或「应超时」。
#[tokio::test]
async fn race_watchdog_times_out_on_a_silent_peer() {
    let (silent, trace) = spawn_silent_peer().await;
    let start = std::time::Instant::now();
    // 外层再兜一道 3s：看门狗真坏时**当场红并说清楚**，而不是把整个测试二进制吊死。
    // （原来那条 `elapsed < 3s` 的上界改由这一道守 —— 上界没丢，换了个说得出话的地方。）
    let raced = tokio::time::timeout(
        Duration::from_secs(3),
        race_connect(
            test_config(),
            None,
            vec![silent],
            Duration::from_millis(400),
            None,
        ),
    )
    .await;
    let elapsed = start.elapsed();
    let Ok(inner) = raced else {
        panic!("看门狗没兑现：deadline 给的是 400ms，3 秒内 race_connect 没返回 —— 它吊死了");
    };
    let err = race_err(inner);

    // ① 前提这一半：对端确实收到了客户端的 SSH 标识 ⇒ TCP 连上了、卡的是**握手**那一段。
    assert!(
        settled(|| trace.lock().unwrap().banner.is_some()).await,
        "前提不成立：该卡住的那个静默对端一个字节都没收到 —— 拨的根本不是它？\
             本机 loopback 不通？总之这条今天判不了，**它不是「看门狗坏了」**"
    );
    let banner = trace.lock().unwrap().banner.clone().unwrap_or_default();
    assert!(
        banner.starts_with("SSH-2.0-"),
        "前提不成立：对端收到的不是 SSH 标识而是 {banner:?} —— 卡住的不是握手，\
             这条今天判不了，**它不是「看门狗坏了」**"
    );

    // ② 被测性质这一半：到点走 deadline 支（那句「握手超时」只在那一支出现）。
    assert!(err.contains("握手超时"), "应超时: {err}");
    // ③ 它是**等到 deadline 才**返回的 —— 不靠措辞一条腿站着：快速失败那一支
    //    （`ENETUNREACH` / RST）会在几毫秒内返回，这一条把那种情形直接判红。
    assert!(
        elapsed >= Duration::from_millis(400),
        "只花了 {elapsed:?} 就返回 —— 没等到 400ms deadline，走的是快速失败那一支，\
             不是看门狗那一支"
    );
}

#[tokio::test]
async fn race_single_endpoint_dead_reports_that_endpoint() {
    // 单地址退化路径：错误里报该地址（保留老实现的可诊断性）。
    let p = dead_port().await;
    let order = vec![ep("127.0.0.1", p)];
    let err =
        race_err(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
    assert!(err.contains(&p.to_string()), "单地址错误应含该地址: {err}");
}

#[tokio::test]
async fn race_empty_order_errors_cleanly() {
    let err =
        race_err(race_connect(test_config(), None, vec![], Duration::from_secs(1), None).await);
    assert!(err.contains("无可用地址"), "空 order: {err}");
}

// === F45 / D 审计 R-1：胜者 happy-path（live server 胜、慢地址被弃）===
// 起一个 mock russh server（run_stream 自动完成 KEX/握手,握手成功即客户端 Ok——race
// 只到握手,不需要真鉴权）。expected_fp=None 走 TOFU 接受该 mock key。

/// mock server 用的固定 ed25519 host key（ssh-keygen 生成）。
const MOCK_SERVER_KEY: &str = "\
-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACABVcXnVsSgWL3RAZE1r7ebLdEi510GsSqfaTYYzM26GwAAAJiEWV/KhFlf
ygAAAAtzc2gtZWQyNTUxOQAAACABVcXnVsSgWL3RAZE1r7ebLdEi510GsSqfaTYYzM26Gw
AAAEDRp5kloww4Jpr8K56RETPX0tLdId9XD8a+yNz5Tx0XOQFVxedWxKBYvdEBkTWvt5st
0SLnXQaxKp9pNhjMzbobAAAAD2Y0NS1tb2NrLXNlcnZlcgECAwQFBg==
-----END OPENSSH PRIVATE KEY-----";

struct MockServer;
impl russh::server::Handler for MockServer {
    type Error = russh::Error;
}

fn mock_server_config() -> Arc<russh::server::Config> {
    let key = russh::keys::PrivateKey::from_openssh(MOCK_SERVER_KEY).expect("parse mock key");
    Arc::new(russh::server::Config {
        keys: vec![key],
        ..Default::default()
    })
}

/// 起一个只接一条连接的 mock SSH server,返回其监听地址。
async fn spawn_mock_server() -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = russh::server::run_stream(mock_server_config(), stream, MockServer).await;
        }
    });
    ep("127.0.0.1", addr.port())
}

/// 从 race_connect 的 Ok 分支取胜者 Endpoint（Handle 不实现 Debug,丢弃即关连接）。
fn race_win(
    r: Result<
        (
            client::Handle<ClientHandler>,
            Arc<Mutex<Option<String>>>,
            Endpoint,
        ),
        String,
    >,
) -> Endpoint {
    match r {
        Ok((_h, _cell, ep)) => ep,
        Err(e) => panic!("expected a winner, got Err: {e}"),
    }
}

#[tokio::test]
async fn race_live_server_wins_when_first() {
    let live = spawn_mock_server().await;
    // live 排 i=0 立即拨、静默对端 i=1 延迟 → live 握手先成功即胜。
    // （i=1 那位通常根本没被拨到就被 abort 了 ⇒ 这里不断言它的前提，见下一条。）
    let (silent, _trace) = spawn_silent_peer().await;
    let order = vec![live.clone(), silent];
    let win =
        race_win(race_connect(test_config(), None, order, Duration::from_secs(5), None).await);
    assert_eq!(win, live, "live server 应胜出");
}

/// 静默对端排首(i=0 立即拨,卡在握手永不完成)、live 排 i=1(250ms 后拨)——慢地址被弃,live 仍胜。
/// 佐证 trap #8:首地址挂起不吊死整批,后位可达地址照样赢。
///
/// # 🔴 这条判据治的是「**绿说不清测没测到**」〔`K-R24` D1 边界 + 下一拍〕
///
/// 它的**判决从来不随环境翻转，翻转的是它有没有测到东西**：旧写法拨黑洞 IP，
/// 断网下那个地址**瞬间报错**而不是挂起 ⇒ live 照样胜 ⇒ **判决仍绿，
/// 而「首地址挂起也不吊死整批」这半再没被驱动过**。
///
/// ⚠⚠ **所以承重的不是那句 `assert_eq!(win, live)`** —— 首地址无论是挂住、
/// 瞬间 `ENETUNREACH`、还是被 RST 拒了，live 都赢。**只断言结果 = 什么都没断言。**
/// 要断言的是**「首地址真的挂住了」这件事本身**，下面两条腿各自独立地证它：
///
/// · **腿①（生产侧，不靠夹具记账）**：阶段事件流里首地址有 `dialing`、
///   **且自始至终没有 `failed`**，而 `won` 落在 live 上 ⇒ 首地址那一路**既没成也没败**，
///   是被 `abort_all` 收走的 —— **整批解决的那一刻它正挂着**。
///   （首地址若是快速失败，`race_connect:576` 会给它 emit 一条 `failed`。）
/// · **腿②（夹具侧痕迹，`PeerTrace`）**：对端记下它收到了客户端的 `SSH-2.0-` 标识
///   （⇒ TCP 早连上、卡的是握手那一段），且那条连接**一直攥到客户端被收走**才断 ——
///   而这个夹具自己从不主动关、从不回一个字节。
///
/// ★ 两条腿**不共用证据**：腿① 读的是被测函数自己吐的事件，腿② 读的是对端看到的字节。
#[tokio::test]
async fn race_live_server_wins_when_a_hung_peer_is_first() {
    let live = spawn_mock_server().await;
    let (hung, trace) = spawn_silent_peer().await;
    let hung_label = format!("{}:{}", hung.host, hung.port);
    let live_label = format!("{}:{}", live.host, live.port);
    let (ch, events) = collecting_channel();
    let order = vec![hung, live.clone()];
    let win = race_win(
        race_connect(test_config(), None, order, Duration::from_secs(5), Some(ch)).await,
    );
    let race_returned_at = std::time::Instant::now();

    // 结果那半 —— 它不随环境翻转，所以它**不是**承重的那条腿。
    assert_eq!(win, live, "首地址挂起时后位 live 仍应胜出");

    // ── 腿①：首地址那一路「既没成也没败」，是被整批 abort 收走的 ──
    let ev = events.lock().unwrap().clone();
    let (Some(i_dial), Some(i_won)) = (
        ev.iter()
            .position(|(k, e)| k == "dialing" && *e == hung_label),
        ev.iter().position(|(k, e)| k == "won" && *e == live_label),
    ) else {
        panic!(
            "前提不成立：首地址压根没被拨、或 live 没胜出 —— 「首地址挂起也不吊死整批」\
                 这半没被驱动，这一条此刻是**绿得没有意义**的。事件流：{ev:?}"
        );
    };
    assert!(
        i_dial < i_won,
        "首地址是在 live 胜出之后才被拨的 —— 竞速期间它并没挂在那儿：{ev:?}"
    );
    assert!(
        !ev.iter().any(|(k, e)| k == "failed" && *e == hung_label),
        "首地址那一路**自己失败了**（不是挂住）—— live 照样胜、这条照样绿，\
             但「首地址挂起也不吊死整批」这半没被驱动。事件流：{ev:?}"
    );

    // ── 腿②：夹具痕迹 —— 走到了握手，并且一直挂到客户端被收走 ──
    assert!(
        settled(|| trace.lock().unwrap().banner.is_some()).await,
        "前提不成立：首地址那个对端一个字节都没收到 —— 「首地址挂起也不吊死整批」\
             这半没被驱动，这一条此刻是**绿得没有意义**的"
    );
    let t = trace.lock().unwrap().clone();
    let banner = t.banner.clone().unwrap_or_default();
    assert!(
        banner.starts_with("SSH-2.0-"),
        "首地址那个对端收到的不是 SSH 标识而是 {banner:?} —— 卡住的不是握手那一段"
    );
    assert!(
        t.banner_at.is_some_and(|at| at < race_returned_at),
        "首地址那一路是在竞速**结束之后**才走到握手的 —— 竞速进行时它并没挂在那儿"
    );
    assert!(
        settled(|| trace.lock().unwrap().client_went_away_at.is_some()).await,
        "首地址那条连接**不是被整批 abort 收走的** —— 它没挂住（自己先断了 / 从没真连上）。\
             ⚠ 注意这一条与上面那句 `assert_eq!(win, live)` 的区别：live 照样胜、结果照样对，\
             但「首地址挂起也不吊死整批」这半没被驱动"
    );
}

// === F46：连接分阶段事件 ===

#[test]
fn classify_stage_buckets() {
    assert_eq!(classify_stage("Connection refused (os error 111)"), "tcp");
    assert_eq!(classify_stage("No route to host"), "tcp");
    assert_eq!(classify_stage("operation timed out"), "timeout");
    assert_eq!(classify_stage("握手超时"), "timeout");
    assert_eq!(classify_stage("host key mismatch"), "hostkey");
    assert_eq!(classify_stage("Unknown server key"), "hostkey");
    assert_eq!(classify_stage("something else entirely"), "other");
}

/// 收集 Channel emit 的阶段事件（send→on_message(InvokeResponseBody::Json)），
/// 逐条记 `(kind, endpoint)`、**保序**。
///
/// ⚠ 从前这里只记 `kind`。带上 `endpoint` 是 `K-R24` 下一拍要的：
/// 「首地址那一路怎么了」与「后位那一路怎么了」在只有 `kind` 的流上**分不开**，
/// 而那正是 `race_live_server_wins_when_a_hung_peer_is_first` 要断言的东西。
/// `endpoint` 那一格对 `auth` / `established` 这类不带地址的阶段是空串。
fn collecting_channel() -> (
    tauri::ipc::Channel<ConnectStage>,
    Arc<Mutex<Vec<(String, String)>>>,
) {
    let sink = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let s2 = Arc::clone(&sink);
    let ch = tauri::ipc::Channel::new(move |body: tauri::ipc::InvokeResponseBody| {
        if let tauri::ipc::InvokeResponseBody::Json(json) = body {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(k) = v.get("kind").and_then(|k| k.as_str()) {
                    let endpoint = v
                        .get("endpoint")
                        .and_then(|e| e.as_str())
                        .unwrap_or_default()
                        .to_string();
                    s2.lock().unwrap().push((k.to_string(), endpoint));
                }
            }
        }
        Ok(())
    });
    (ch, sink)
}

/// 只要 kind 那一维（给那两条不关心是哪个地址的判据用）。
fn stage_kinds(sink: &Arc<Mutex<Vec<(String, String)>>>) -> Vec<String> {
    sink.lock()
        .unwrap()
        .iter()
        .map(|(k, _)| k.clone())
        .collect()
}

#[tokio::test]
async fn race_emits_dialing_hostkey_won_for_live_server() {
    let live = spawn_mock_server().await;
    let (ch, sink) = collecting_channel();
    let _ = race_win(
        race_connect(
            test_config(),
            None,
            vec![live],
            Duration::from_secs(5),
            Some(ch),
        )
        .await,
    );
    let kinds = stage_kinds(&sink);
    assert!(
        kinds.contains(&"dialing".to_string()),
        "缺 dialing: {kinds:?}"
    );
    assert!(
        kinds.contains(&"hostKey".to_string()),
        "缺 hostKey: {kinds:?}"
    );
    assert!(kinds.contains(&"won".to_string()), "缺 won: {kinds:?}");
}

#[tokio::test]
async fn race_emits_dialing_and_failed_for_dead_address() {
    let p = dead_port().await;
    let (ch, sink) = collecting_channel();
    let _ = race_err(
        race_connect(
            test_config(),
            None,
            vec![ep("127.0.0.1", p)],
            Duration::from_secs(3),
            Some(ch),
        )
        .await,
    );
    let kinds = stage_kinds(&sink);
    assert!(
        kinds.contains(&"dialing".to_string()),
        "缺 dialing: {kinds:?}"
    );
    assert!(
        kinds.contains(&"failed".to_string()),
        "缺 failed: {kinds:?}"
    );
    assert!(
        !kinds.contains(&"won".to_string()),
        "死地址不应 won: {kinds:?}"
    );
}

#[tokio::test]
async fn race_emitter_none_still_works() {
    // emitter=None 路径不 panic、与 F45 行为等价（此处验死地址聚合）。
    let p = dead_port().await;
    let err = race_err(
        race_connect(
            test_config(),
            None,
            vec![ep("127.0.0.1", p)],
            Duration::from_secs(3),
            None,
        )
        .await,
    );
    assert!(err.contains(&p.to_string()));
}

// === F45：winner_address（喂 remote-launch 的拨号地址）===

fn cfg_with(label: &str, host: &str, port: u16, addresses: Vec<String>) -> RemoteConfig {
    RemoteConfig {
        host: host.into(),
        label: label.into(),
        port,
        user: "u".into(),
        key_path: None,
        daemon_path: "d".into(),
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
