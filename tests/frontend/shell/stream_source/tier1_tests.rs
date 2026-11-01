/// ★★ **本机 `~/.ssh` 的读面：恰好一处，且只读 `config`**〔audit-0805 08-08，Phase G 第 85 件〕。
///
/// # 「谁能读」这一侧此前只有半张表
///
/// 「谁能**写**」（壳的写盘只经 `platform::own_fs`）、「谁能**执行**」
/// （界面进程零拨号 `dial_move_judge` / 本机起进程只经 `spawn_managed` / 构建期执行面）都是默认拒绝，
/// 名单住 `src/frontend/clippy.toml`，家目录只在 `data_paths` 取；
/// 而**读**那一侧只有 backend 的 `readonly_guard`（整个 crate 不许写）。
///
/// ⇒ 用户机器上**别的**敏感目录不在任何人群里。08-08 实测：monitor 生产段里
/// 碰本机 `.ssh` 的只有**一处**（本文件读 `~/.ssh/config` 列别名给前端），
/// 而**没有任何判据钉住它保持一处** —— 加一句读 `~/.ssh/id_ed25519` 或
/// `known_hosts`，全仓判据一条不会红。
///
/// ⚠ 拼给**远端**执行的 `$HOME/.ssh` shell 串不在 monitor 里（公钥推送在本机后端）。
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
        // `stream_source/` 读 `~/.ssh/config` 那一行走了：导入搬进后端 `dial/ssh_config.rs`
        //   ⇒ monitor 生产段碰本机 `.ssh` 的从此是零处（下面只剩拼给远端的那一行）。
        // `pubkey.rs` 那一行（拼给远端的 `authorized_keys` 那一串）也走了：公钥推送进了本机后端（`pubkey-push`），
        //   读本机那份 `.pub` 也在那里 ⇒ monitor 生产段碰 `.ssh` 的**零处**。人群空了，下面那条正控保证尺子不是瞎的。
    ];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree!(&root, &["rs"]);
    assert!(
        files.len() >= 40,
        "只扫到 {} 个 .rs —— 遍历坏了，本条会零命中地绿",
        files.len()
    );
    // 被测的 `stream_source/` —— 它走普通遍历**本来就在**人群里，这一份是补的第二份。
    // ⚠ `scan_tree!` 按构造摘调用者那一刀**在这一处不生效**（判据由 `#[path]`
    //   挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中），而且调用者是本判据文件、
    //   不是 `stream_source/`。重复在这里无害（下面 `found` 排序后 `dedup`），
    //   而它**刻意不删**：它把「被测那一份一定在人群里」钉成一件不依赖扫描面的事。
    // ⚠ **变量名刻意不叫 `me`**：`needle_anchor` 棘轮按**文件文本**推断「语料变量」，
    // 而本文件另一条判据里有一句刻意演示旧近似的 `me.split("\n#[cfg(test)]")`（见 2890 行附近）。
    // 叫 `me` 会让那处旧 split 被算成「语料变量上的裸匹配」，棘轮当场红 —— 08-08 实测过。
    // 名字影响判据结果，这件事本身值得写下来。
    let self_src = guard_core::addr::stream_source_raw();
    let mut corpus: Vec<(String, String)> = files
        .iter()
        .map(|(p, s)| {
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                s.clone(),
            )
        })
        .collect();
    corpus.push(("stream_source/".to_string(), self_src));

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
    // 正控：同一把尺子在合成语料上认得这个目录名（人群为空时，这一条防它零命中地绿）。
    assert!(
        guard_core::contains_word("let p = home.join(\".ssh\");", &dot)
            && !guard_core::contains_word("let p = \".sshx\";", &dot),
        "`contains_word` 认不出 `.ssh` 了 —— 尺子瞎了"
    );
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

// 退避那几条（hello-then-die 不许把退避按在下界 · 翻倍封顶）随退避本体住 `resident-client`：`tests/comms/resident-client/backoff_tests.rs`；
//   「非 unix 远端不按退避空转」住同一条循环的判据 `feed_tests.rs::a_remote_that_is_not_unix_waits_for_a_hand`。

// 〔「一个判定一个家」〕地址四形态解析 · `endpoints` 去重保序 · `winner_order`（last-good 排首）三组判据
//   随那几个函数搬进后端 `ssh-dial/machine.rs`：`tests/backend/dial_machine_tests.rs` 的 `address_lines_read_the_four_shapes_and_refuse_garbage`
//   与 `the_preferred_winner_goes_first_while_it_is_still_one_of_the_addresses`。

// 远端开终端在本机后端（`terminal-ssh`，只交名字）。首选地址那一半判据都在后端：
//   「上次赢的那条排首 / 已不在这台地址里就不动」⇒ `dial_machine_tests::the_preferred_winner_goes_first_while_it_is_still_one_of_the_addresses`
//   ＋ 开终端那一行 `dial_terminal_tests::the_address_is_the_first_in_race_order`（后端今天不记上次赢的那条，照表里的次序）。

// 后端落点恒是 `relay_route_core::BACKEND_LANDING_SHELL`，没有外来值可判。
