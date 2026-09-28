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
        // 〔MIG-1 · `99 §2.1 ⑯`〕`ssh_source.rs` 读 `~/.ssh/config` 那一行走了：导入搬进后端 `dial/ssh_config.rs`
        //   ⇒ monitor 生产段碰本机 `.ssh` 的从此是零处（下面只剩拼给远端的那一行）。
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
             ⚠ 少了的：登记的那一处被改写/挪走了 —— 先看它是不是换了形状、还在不在远端那一侧。\n\
             ⚠ 人群按**目录名**取，本机/远端由登记的第二列回答 —— \n\
             别再退回按拼法取样（那是 `needle_anchor` 棘轮 08-08 当场拦下的写法）。"
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

/// 〔DEL 续〕守的要求：主会话裁「非 unix 远端：那一类失败按『永久不支持』记在那台的连接状态里，**不再自动按退避重连**」。
/// 记下了 ⇒ `Stop`（带那句话，任何退避值都一样）；没记 ⇒ 照当前退避再连。
#[test]
fn a_permanently_unsupported_remote_stops_instead_of_backing_off() {
    for b in [RECONNECT_MIN, RECONNECT_MAX] {
        assert_eq!(
            after_round(Some("不是 Unix".into()), b),
            AfterRound::Stop("不是 Unix".into()),
            "非 unix 远端还在按退避 {b:?} 空转"
        );
        assert_eq!(after_round(None, b), AfterRound::RetryIn(b));
    }
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

// 〔MIG-1 收尾 · 主会话裁「一个判定一个家」〕地址四形态解析 · `endpoints` 去重保序 · `winner_order`（last-good 排首）三组判据
//   随那几个函数搬进后端 `dial/machine.rs`：`tests/backend/dial_machine_tests.rs` 的 `address_lines_read_the_four_shapes_and_refuse_garbage`
//   与 `a_wire_dial_is_composed_here_and_the_preferred_winner_goes_first`（后者同拍新加）。

fn ep(host: &str, port: u16) -> Endpoint {
    Endpoint {
        host: host.into(),
        port,
    }
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
    record_last_good(&cfg, &ep("10.0.0.9", 22));
    assert_eq!(
        winner_address(&cfg),
        ep("10.0.0.9", 22),
        "已连过 → last-good 胜者"
    );
    // 配置改掉备用地址 → 记下 last-good 那一刻的地址配置对不上了 → 回退 host（〔MIG-1 收尾〕不再在界面进程里解析地址来判）。
    let cfg2 = cfg_with("wa-lg", "h.example", 22, vec![]);
    assert_eq!(
        winner_address(&cfg2),
        ep("h.example", 22),
        "配置变更失效 last-good"
    );
}

// 〔E2 · V28〕`backendPath` 那一格删了（落点恒是 `relay_route_core::BACKEND_LANDING_SHELL`）⇒ 从前那道放行判定
//   `backend_path_for_shell`〔散文墓碑〕与它的两条判据没有外来值可判，一起删了。
