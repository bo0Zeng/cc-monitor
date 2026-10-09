use super::*;

/// 造一棵**结构**上像样的临时目录树：名字全合成，不锚在任何活体上。
fn synth_tree() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ccm-filewin-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("Beta")).unwrap();
    std::fs::create_dir_all(root.join("alpha")).unwrap();
    std::fs::write(root.join("Zeta.txt"), b"0123456789").unwrap();
    std::fs::write(root.join("mid.txt"), b"01234").unwrap();
    root
}

#[test]
fn listing_a_local_dir_yields_dirs_first_then_case_insensitive_name() {
    let root = synth_tree();
    let rows = list_local(&root).unwrap();
    let got: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    // 相等断言，不是「包含」：目录在前（alpha < Beta，小写比），再文件（mid < Zeta）。
    assert_eq!(got, vec!["alpha", "Beta", "mid.txt", "Zeta.txt"]);
    assert_eq!(
        rows.iter().map(|r| r.is_dir).collect::<Vec<_>>(),
        vec![true, true, false, false]
    );
    // 大小走的是真 metadata，不是编出来的。
    assert_eq!(rows[2].size, 5);
    assert_eq!(rows[3].size, 10);
    std::fs::remove_dir_all(&root).ok();
}

/// 🔴 上面头注那个「两份实现」的缝 —— 这条把**两条路的真实输出**对拍成相等。
///
/// 远端那一侧的序**曾经**由池子里那个私有排序函数产生（`sort_entries`〔散文墓碑〕已随列目录命令删了，
/// 这条对拍今天钉的是「生产契约的定义」与 [`sort_rows`] 相等）；
/// 这里够不着它，但够得着**它排过的结果**在类型上等价的那个形状 ——
/// 于是拿同一组合成名字，一边喂 [`sort_rows`]，一边按生产契约的定义重算，
/// 断言**两个序列逐项相等**。任何一侧改了排序规则，这条当场红。
#[test]
fn the_two_orderings_agree_on_a_synthetic_set() {
    let names = [
        ("README", false),
        ("bin", true),
        ("Cargo.toml", false),
        ("Src", true),
        ("aux", true),
        ("build.rs", false),
    ];
    let mut mine: Vec<Listed> = names
        .iter()
        .map(|(n, d)| {
            Listed::plain(Row {
                name: (*n).to_string(),
                path: format!("/x/{n}"),
                is_dir: *d,
                size: 0,
                lossy_name: false,
            })
        })
        .collect();
    // ⚠ 缺省那一档 —— 它就是本刀之前那个写死的函数（逐字节相同那一条住下面）。
    sort_rows(&mut mine, SortBy::default());

    // 生产契约逐字（原住池子那个私有排序函数，已删）：目录在前，再名称小写升序。
    let mut theirs: Vec<(String, bool)> =
        names.iter().map(|(n, d)| ((*n).to_string(), *d)).collect();
    theirs.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });

    let mine_seq: Vec<(String, bool)> = mine.iter().map(|r| (r.name.clone(), r.is_dir)).collect();
    assert_eq!(mine_seq, theirs);
    // 反空真：这个集合真的会被重排（不是本来就有序）。
    assert_ne!(
        mine_seq.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        names.iter().map(|(n, _)| *n).collect::<Vec<_>>()
    );
}

// 这里原先有一条「`SftpEntry` → 行的映射六格一格不许掉」的判据。
// 窗口进程不再经 SFTP 列目录（没有退路，`D11`）⇒ 那个映射函数连同这条判据一起走了；
// 「后端送的每一格落到行上哪一格」由下面那张逐格表（`files.ls` 的声明现读）接着管。

/// 🔴 **整棵 `filewin/` 上「列一个远端目录」恰好一处，是问后端 `files-ls`；池子那条列目录命令零处。**
///
/// 两处 → 一处：`entry.rs` 那一处（monitor 开窗前替窗口列第一屏）退役，
/// 第一屏由窗口进程自己列（`proc::first_screen` → `source::list_dir`，落在 `source.rs` 那一处上）。下面「恰好两处」那段是历史。
///
/// 上一版这里是两条：「`list_remote` 调的是共用池」＋「池子那条列目录命令
/// 整棵树恰好一处（`source.rs`）」。窗口改成只经通道说 `call` 之后，那条路整条摘了 ⇒ 判据翻面：
///
/// - 问后端 `files-ls` 的地方（`CMD_LS` 这个名字被用到的地方）恰好两处：
///   `source.rs`（窗口进程里那一次，[`list_via_backend`]）与 `entry.rs`（开窗前那一屏，
///   monitor 进程里经通道宿主的同一个句柄）。**两向相等**，多一处 / 少一处都红。
/// - 池子那条列目录命令在整棵树的生产段里**零处**（零命中守卫，带反向自检）。
/// - 自己开连接 / 借会话的写法**零处**。
///
/// ⚠ 判源码是代理：买的是「只有这两处在问、而且问的是后端」；「后端答得对」由
/// [`listing_has_no_second_road_when_the_backend_refuses`] 那条行为判据买。
#[test]
fn the_whole_filewin_tree_lists_a_remote_directory_only_by_asking_the_backend() {
    // 「整棵 `filewin/`」今天是两棵：本包 ＋ monitor 那一侧 `src/frontend/shell/src/filewin/`（开窗入口 · 起进程），人群与搬家前逐份相同。
    let root = crate::guard_support::crate_src_root();
    let monitor_side = crate::guard_support::repo_root().join("src/frontend/shell/src/filewin");
    let files: Vec<std::path::PathBuf> = guard_core::files_by_extension(&root, "rs")
        .into_iter()
        .map(|f| root.join(f))
        .chain(
            guard_core::files_by_extension(&monitor_side, "rs")
                .into_iter()
                .map(|f| monitor_side.join(f)),
        )
        .collect();
    assert!(
        files.len() >= 16,
        "`filewin/` 下只扫到 {} 份 `.rs`（{files:?}）—— 扫描面塌了，下面几条在空转",
        files.len()
    );
    let ask_ls = "CMD_LS";
    let pool_ls = format!("sftp_pool::sftp_{}(", "list_dir");
    let mut asking: Vec<(String, bool)> = Vec::new();
    let mut total_prod = 0usize;
    for path in &files {
        let f = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let src = std::fs::read_to_string(path).expect("read filewin rs");
        let prod = guard_core::production_code(&src);
        total_prod += prod.len();
        // 定义那一行（`pub const CMD_LS`）不算「问」。
        let uses = prod
            .lines()
            .filter(|l| {
                guard_core::contains_word(l, ask_ls) && !guard_core::contains_word(l, "const")
            })
            .count();
        if uses > 0 {
            asking.push((f.clone(), true));
        }
        assert_eq!(
            prod.matches(pool_ls.as_str()).count(),
            0,
            "`filewin/{f}` 的生产段里又出现了池子那条列目录命令 —— `D11`：没有退路"
        );
        for banned in ["connect_sftp", "russh_sftp", "SftpSession", "with_sftp("] {
            assert!(
                !prod.contains(banned),
                "`filewin/{f}` 的生产段里出现了 `{banned}` —— 这棵树不许自己碰连接／通道"
            );
        }
    }
    assert!(
        total_prod > 20_000,
        "整棵树的生产段只剩 {total_prod} 字节 —— 剥法坏了"
    );
    asking.sort();
    assert_eq!(
        asking,
        // `entry.rs` 那一处（monitor 开窗前替窗口列第一屏）退役：第一屏由窗口进程经 `source::list_dir` 列。
        vec![("source.rs".to_string(), true)],
        "问后端列目录的地方不再是「`source.rs` 一处」"
    );
    // 反向自检：这把尺子认得出池子那条命令（否则上面那条零命中恒真）。
    let fake = format!("fn x() {{ {pool_ls} }}");
    assert_eq!(
        guard_core::production_code(&fake)
            .matches(pool_ls.as_str())
            .count(),
        1
    );
}

/// 上一级目录：**只有一个算法，而它只认 `/`。**
///
/// 🔴 它**不许**借 `std::path` —— 在 Windows 上 `Path` 把 `\` 也当分隔符，
/// 于是远端一个名字里含反斜杠的目录会被切成两级（而 SFTP 的路径分隔符只有 `/`）。
///
/// ⚠〔2026-09-23 本机侧退役〕本条原名是
/// 〔散文墓碑〕`walking_up_uses_slashes_on_the_remote_side_and_the_platform_on_the_local_side`，
/// 尾巴上那半判的是**本机**那一支（用 `Path` 造期望值、到顶回原值）。
/// `parent_dir` 今天只吃一条字符串、只有一个算法 ⇒ 那半判的东西不在了。
/// **反斜杠那一条一个字没动** —— 它才是这条判据承重的那一格。
#[test]
fn walking_up_uses_slashes_only_and_never_the_platform_separator() {
    // 相等断言，逐个：
    assert_eq!(parent_dir("/a/b/c"), "/a/b");
    assert_eq!(parent_dir("/a/b/c/"), "/a/b");
    assert_eq!(parent_dir("/a"), "/");
    assert_eq!(parent_dir("/"), "/", "到根了还往上走");
    assert_eq!(parent_dir(""), "/");
    // 🔴 承重的那一条：反斜杠是**名字的一部分**，不是分隔符。
    assert_eq!(
        parent_dir("/srv/a\\b/c"),
        "/srv/a\\b",
        "远端路径里的反斜杠被当成分隔符了 —— 那是 `std::path` 在 Windows 上的行为，\
         而这条路上不许用它"
    );
    // 🔴 反斜杠在**最后一段**里：这一格才分得开「只认 `/`」与「`\` 也算」（上面那格两种切法同答：
    //    `Path::parent` 只摘掉末段，`a\b` 在前缀里原样留着）。
    assert_eq!(
        parent_dir("/srv/a\\b"),
        "/srv",
        "末段里的反斜杠被当成分隔符了"
    );
    // 🔴 **阴性对照**：「`\` 也当分隔符」那一形（`std::path` 在 Windows 上就是它）在这一格上答得不同
    //    ⇒ 上面那一比不是恒真的。上一版只在 Windows 上比、且比的是上面那格（两种切法同答）⇒ Windows runner 上红、Linux 上从没跑过。
    let also_backslash = |p: &str| -> String {
        let t = p.trim_end_matches(['/', '\\']);
        match t.rfind(['/', '\\']) {
            Some(0) | None => "/".to_string(),
            Some(i) => t[..i].to_string(),
        }
    };
    assert_ne!(
        also_backslash("/srv/a\\b"),
        parent_dir("/srv/a\\b"),
        "「`\\` 也算分隔符」那一形与本算法答得一样 —— 上面那一比买不到东西"
    );
    // 那一形确是 Windows 上 `std::path` 的行为（不是立给自己打的靶子）。
    #[cfg(windows)]
    assert_eq!(
        std::path::Path::new("/srv/a\\b")
            .parent()
            .map(|p| p.to_string_lossy().to_string()),
        Some(also_backslash("/srv/a\\b")),
        "阴性对照那一形不是这台机器上 `std::path` 的答案"
    );
}

#[test]
fn a_missing_dir_is_an_error_not_an_empty_list() {
    let missing = std::env::temp_dir().join("ccm-filewin-does-not-exist-9d3f1a");
    let r = list_local(&missing);
    assert!(r.is_err(), "不存在的目录必须报错，不许静默返回空列表");
}

// ════════════════════════════════════════════════════════════════════════
// 🔴远端 home 那一跳 ——问的是后端 `files-home`
// ════════════════════════════════════════════════════════════════════════
//
// 问 home 那一趟要一个起着的后端（〔09-28 裁 3〕窗口进程的 `proc::first_screen`）⇒ 有逻辑的两段抽成了
// `home_from_reply`（解字节）与 `start_dir_from_home`（能不能当起点），判据全落在它们身上。
// 失败路径那一半（问不到就别开窗、带原话）住 `proc_tests::the_first_screen_asks_home_only_when_told_nothing`。
// ⚠ 第七刀那一版问的是 SFTP 的 `realpath(".")`；「服务端」「对面」这些字眼说的都是那台机器。

/// 后端那条应答 → 起点：字符串 · `b16`（合法 UTF-8 的）两形都收；
/// 不是合法 UTF-8 / 缺 `path` / 形状不对 ⇒ **报错**，不有损解码去开一个别处的窗。
#[test]
fn the_home_reply_is_decoded_as_bytes_and_refused_when_it_cannot_address_a_directory() {
    assert_eq!(
        home_from_reply(&serde_json::json!({ "path": "/home/u/" })).unwrap(),
        "/home/u"
    );
    // `/home/u` 的十六进制形 —— 后端对非 UTF-8 才出这一形，但合法的也得收。
    assert_eq!(
        home_from_reply(&serde_json::json!({ "path": { "b16": "2f686f6d652f75" } })).unwrap(),
        "/home/u"
    );
    for (bad, what) in [
        (
            serde_json::json!({ "path": { "b16": "2f686fff" } }),
            "不是合法 UTF-8",
        ),
        (serde_json::json!({}), "缺 `path`"),
        (serde_json::json!({ "path": 7 }), "形状不对"),
        (serde_json::json!({ "path": "relative/home" }), "相对路径"),
    ] {
        let e = home_from_reply(&bad).expect_err(&format!("{what} 竟然被当成了起点"));
        assert!(!e.trim().is_empty(), "{what} 的报错是空串");
    }
}

/// 规矩的服务端回一条绝对路径 ⇒ 原样当起点。
#[test]
fn an_absolute_answer_becomes_the_start_directory() {
    assert_eq!(start_dir_from_home("/home/user").unwrap(), "/home/user");
    // 周围的空白不算内容（SFTP 的实现里见过带尾换行的）。
    assert_eq!(start_dir_from_home("  /srv/data\n").unwrap(), "/srv/data");
    // 根自己是合法起点。
    assert_eq!(start_dir_from_home("/").unwrap(), "/");
}

/// 末尾那个 `/` 剥掉 —— **一个规范形**，而它**不是**承重的。
///
/// # 🔴〔散文墓碑 2026-09-21〕我第一版给的理由是**假的**
///
/// 我原先在这儿写着（逐字）：「`parent_dir` 靠「回来的和给出去的相等」判「已经在顶上了」，
/// 而 `/srv/` 与 `/srv` 在那个算法里是两个不同的输入 ⇒ 起点带尾斜杠时，
/// 「上一级」那颗按钮第一下会原地不动（看起来像卡住了）」。
///
/// **那是错的。** `parent_dir` 的远端那一支**第一句**就是 `cwd.trim_end_matches('/')`
/// ⇒ `/srv/data/` 与 `/srv/data` 都回 `/srv`，那颗按钮从来没有过这个毛病。
///
/// ⚠ 逮到它的是本条自己那半**阴性对照**（我顺手写的「不剥的话那一下真的不动」）——
/// 它红了，红的是**我的前提**，不是生产代码。⇒ **阴性对照也在守判据自己说的话。**
/// 墓碑不删：下一个人会想把这条判据「加强」成那个假理由。
///
/// ⇒ 那这一步还剥不剥？**剥** —— 但买到的只有「路径在窗口里只有一种写法」
/// （工具栏那行 `{label} : {cwd}` 与 `navigate_to` 的相等判定看同一个串），
/// **不是**「否则某个功能会坏」。本条断的就是这一件，不多说。
#[test]
fn a_trailing_slash_is_normalised_away_into_one_canonical_form() {
    assert_eq!(start_dir_from_home("/srv/data/").unwrap(), "/srv/data");
    // 根那一格**不能**被剥成空串。
    assert_eq!(start_dir_from_home("///").unwrap(), "/");
    // 两种写法归一（这就是「一种写法」那句话的相等断言）。
    assert_eq!(
        start_dir_from_home("/srv/data/").unwrap(),
        start_dir_from_home("/srv/data").unwrap()
    );

    // 🔴 把那个假前提**钉成读数**：`parent_dir` 自己就吃得下尾斜杠
    //    ⇒ 哪天它不吃了，本条会红，而那时才轮到「剥这一步变承重了」这句话。
    assert_eq!(
        parent_dir("/srv/data/"),
        parent_dir("/srv/data"),
        "`parent_dir` 不再自己吃尾斜杠了 —— 那么 `start_dir_from_home` 里剥那一步\
         就从「规范形」升级成「承重」，回头把上面那段墓碑重写"
    );
}

/// 🔴 对面答得不像话 ⇒ **报错，不拿它去开窗**。
///
/// 少了这一条，一个空串或相对路径会被原样当成起点 ⇒ 窗口出来了、里面是空的，
/// 而那正是 `entry.rs` 头注花一整节要避免的那一形。
#[test]
fn an_answer_we_cannot_use_as_a_start_is_an_error_not_a_blank_window() {
    for bad in ["", "   ", "\n", "home/user", "./x", "C:\\Users\\user"] {
        let e = start_dir_from_home(bad).expect_err(&format!("`{bad:?}` 竟然被当成了合法起点"));
        assert!(!e.trim().is_empty(), "`{bad:?}` 的报错是空串");
        // 报错里要带上对面那句原文（否则用户不知道是谁答错了）。
        if !bad.trim().is_empty() {
            assert!(e.contains(bad.trim()), "报错没带上对面答的那句原文：{e}");
        }
    }
}

/// 🔴远端 basename —— **只按 `/` 切**。
///
/// 盘上此前已有**三处** `rsplit('/')` 各写一份，这一条钉的是第四处不要再长出来
/// （逐条理由住 `remote_basename` 头注）。
#[test]
fn a_remote_basename_only_ever_splits_on_slashes() {
    assert_eq!(remote_basename("/a/b/c.txt"), "c.txt");
    assert_eq!(remote_basename("/a/b/"), "b", "尾斜杠该被先剥掉");
    assert_eq!(remote_basename("top.txt"), "top.txt", "没有分隔符就是整条");
    assert_eq!(remote_basename("/"), "", "根切不出名字");
    assert_eq!(remote_basename(""), "");
    // 🔴 **反斜杠不是分隔符** —— SFTP 协议恒用 `/`，对面是 Windows 也一样。
    //    拿 `std::path` 切的话，这一条在 Windows 上会回 `b.txt`。
    assert_eq!(
        remote_basename("/srv/a\\b.txt"),
        "a\\b.txt",
        "反斜杠被当成分隔符了 —— 那会把一个名字里含 `\\` 的文件切成两级"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴后端做，前端拿结果 —— 只读那一侧
// ════════════════════════════════════════════════════════════════════════

/// 一条正常的 `files-ls` entry → 一行。
#[test]
fn a_backend_entry_becomes_a_row() {
    let v = serde_json::json!({
        "path": "/srv/data/报表.csv",
        "kind": "file",
        "size": 4096u64,
        "mtime_secs": 1_700_000_000u64,
    });
    let r = row_from_ls_entry(&v).expect("这条 entry 应当解得出来");
    assert_eq!(r.path, "/srv/data/报表.csv");
    assert_eq!(r.name, "报表.csv", "名字是从路径尾段取的");
    assert!(!r.is_dir);
    assert_eq!(r.size, 4096);
    assert!(!r.lossy_name);
}

/// `kind` 是**四值**，而窗口今天只认 `dir` 一档。
///
/// ⚠ 后端分得出符号链接而窗口分不出 —— 那一格**本刀没接**，这里把它钉成读数：
/// 哪天窗口要在行上画链接标记，改的是这一条。
#[test]
fn only_dir_counts_as_a_directory_and_the_rest_are_flattened() {
    for (kind, want_dir) in [
        ("dir", true),
        ("file", false),
        ("symlink", false),
        ("other", false),
    ] {
        let v = serde_json::json!({ "path": format!("/a/{kind}"), "kind": kind });
        let r = row_from_ls_entry(&v).unwrap();
        assert_eq!(r.is_dir, want_dir, "`kind={kind}` 判错了");
    }
    // `size` 缺了不整条失败（同本机那条：读不到 stat 也照样给一行）。
    let v = serde_json::json!({ "path": "/a/x", "kind": "file" });
    assert_eq!(row_from_ls_entry(&v).unwrap().size, 0);
}

/// 🔴 **`lossy_name` 这一格当场变准了 —— 而旧那份是一个猜。**
///
/// 旧写法是 `name.contains('\u{FFFD}')`（`list_local` 里那一行，今天还在退路上）：
/// 它把「转换时产生了替换字符」与「这个文件真叫这个名字」混成一件事
/// ⇒ 一个**真叫** `\u{FFFD}` 的文件会被误判成有损，于是复制/改名/删除三颗按钮
/// 对它一颗都不画（`is_copyable` / `is_writable` 都看这一格）。
///
/// 新写法看的是**字节**：`files-ls` 的 `path` 走原始字节
/// （字符串 或 `{"b16":…}`）⇒ 有损与否是**事实**不是猜。
#[test]
fn lossiness_is_a_fact_about_the_bytes_not_a_guess_about_the_string() {
    // ① 真叫 U+FFFD 的文件：**合法 UTF-8**，不该判成有损。
    let v = serde_json::json!({ "path": "/a/\u{FFFD}怪名", "kind": "file" });
    let r = row_from_ls_entry(&v).unwrap();
    assert!(
        !r.lossy_name,
        "一个**真叫**替换字符的文件被判成了有损 —— 那正是旧那份 `contains('\\u{{FFFD}}')` 的错法。\n\
         后果具体：复制 / 改名 / 删除三颗按钮对它一颗都不画"
    );
    // 阴性对照：旧那份写法在同一条输入上**确实**会判错（证明上面那一比不是空真）。
    assert!(
        r.name.contains('\u{FFFD}'),
        "夹具没造出那个字符 —— 本条此刻在量别的东西"
    );

    // ② 真的非 UTF-8 字节（走 `{"b16":…}`）⇒ 判成有损。
    //    `/a/` ＋ 一个孤立的 0xFF（任何 UTF-8 序列里都不合法）。
    let hex = "2f612fff";
    let v2 = serde_json::json!({ "path": { "b16": hex }, "kind": "file" });
    let r2 = row_from_ls_entry(&v2).unwrap();
    assert!(r2.lossy_name, "非 UTF-8 字节没被判成有损");
    assert!(
        r2.name.contains('\u{FFFD}'),
        "画出来那一份该是有损的：{}",
        r2.name
    );
}

/// 形状不对的 entry ⇒ **报错，不悄悄跳过那一行**。
///
/// 悄悄跳过的后果：目录里少一个文件，而屏幕上没有任何提示 ——
/// 与「这个文件不存在」分不开。
#[test]
fn an_entry_we_cannot_read_is_an_error_not_a_skipped_row() {
    for bad in [
        serde_json::json!({ "kind": "file" }),
        serde_json::json!({ "path": 42, "kind": "file" }),
        serde_json::json!({ "path": { "b16": "zz" }, "kind": "file" }),
    ] {
        let e = row_from_ls_entry(&bad).expect_err(&format!("`{bad}` 竟然解出来了"));
        assert!(!e.trim().is_empty(), "报错是空串");
    }
    // 阴性对照：正常那条解得出来（否则上面可以靠「什么都报错」全绿）。
    assert!(row_from_ls_entry(&serde_json::json!({ "path": "/a", "kind": "dir" })).is_ok());
}

/// 🔴 **一屏回来就是排好序的** —— 而这是**行为**判据，不是源码代理。
///
/// 后端不排（它答的是目录项不是一屏）。「目录在前、名称小写排」这个**显示序**契约，
/// 主路上只经 `sort_rows` 这一处 —— 本条喂一份乱序的 `data`，看它出来是不是序对的。
#[test]
fn a_screenful_comes_back_already_sorted() {
    let d = serde_json::json!({
        "entries": [
            { "path": "/a/zebra.txt", "kind": "file" },
            { "path": "/a/Apple",     "kind": "dir"  },
            { "path": "/a/beta.txt",  "kind": "file" },
            { "path": "/a/yak",       "kind": "dir"  },
        ],
        "truncated": false,
    });
    let (rows, truncated) = rows_from_ls_data(&d, SortBy::default()).expect("这一份该解得出来");
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Apple", "yak", "beta.txt", "zebra.txt"],
        "序不对 —— 契约是「目录在前，再按名称**小写**排」（`Apple` 要排在 `yak` 前）"
    );
    assert!(!truncated.truncated);
    // 反空真：喂进去的就是乱序的（否则「出来有序」可能只是原样）。
    assert_ne!(
        names,
        vec!["zebra.txt", "Apple", "beta.txt", "yak"],
        "本条喂的语料没被重排过 —— 它此刻在量一个恒真"
    );
}

/// 🔴 **解不出的一行 ⇒ 整趟报错，不悄悄跳过。**
///
/// # 这一条是死值验逼出来的
///
/// 第一版只抽了 `row_from_ls_entry`，而 `list_via_backend` 里那个 `?` 没人钉
/// ⇒ 死值验把它换成 `if let Ok(r) = … { out.push(r) }`（悄悄跳过）时
/// **一条判据都不红**。而悄悄跳过的后果是：目录里少一个文件、屏幕上没有提示，
/// 与「这个文件不存在」分不开。
#[test]
fn one_unreadable_entry_fails_the_whole_screen_instead_of_vanishing() {
    let d = serde_json::json!({
        "entries": [
            { "path": "/a/good.txt", "kind": "file" },
            { "kind": "file" },                       // 没有 path
            { "path": "/a/also-good", "kind": "dir" },
        ],
    });
    let e = rows_from_ls_data(&d, SortBy::default())
        .expect_err("有一条解不出来，整趟却成功了 —— 那一行被悄悄吞了");
    assert!(
        copy_core::copy_matches("rsFilewinSource.ls.noPath", &e),
        "报错没说是哪一条：{e}"
    );
    // 阴性对照：三条都好的时候它成得了（否则上面可以靠「什么都失败」全绿）。
    let ok = serde_json::json!({ "entries": [ { "path": "/a/x", "kind": "file" } ] });
    assert_eq!(
        rows_from_ls_data(&ok, SortBy::default()).unwrap().0.len(),
        1
    );
}

/// `truncated` 带得回来（缺了就当没截断）。
#[test]
fn truncation_is_carried_back_not_dropped() {
    let d = serde_json::json!({ "entries": [], "truncated": true });
    assert!(
        rows_from_ls_data(&d, SortBy::default())
            .unwrap()
            .1
            .truncated,
        "截断那一格被丢了"
    );
    let d2 = serde_json::json!({ "entries": [] });
    assert!(
        !rows_from_ls_data(&d2, SortBy::default())
            .unwrap()
            .1
            .truncated,
        "缺了就该当没截断"
    );
}

/// 🔴**后端说不行 ⇒ 列目录就是失败，没有第二条路**（`D11`）。
///
/// 上一版这里是反面：「退路在、排在问后端之后、只有一支」（源码代理，钉三行的行序）。
/// 退路拿掉之后它换成**行为**判据 —— 挂一台合成后端（真回环、真钥匙、真 `dial`）：
/// ① 后端答得出 ⇒ 一屏回来，而且**是后端那一份**（名字与大小逐格相等）；
/// ② 后端拒（目录读不进去）⇒ `Err`，那句话里带着后端的码 —— 不是一屏空的、不是「退回去自己列」。
/// ③ 线上恰好一条 `files-ls`（没有第二趟、也没有别的命令被悄悄发出去）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listing_has_no_second_road_when_the_backend_refuses() {
    use crate::find::testing::{remote_form, wire_up, Declared, FakeBackend};
    let root = synth_tree();
    let wired = wire_up(
        "source-ls",
        FakeBackend::new(&[CMD_LS], Declared::default()),
    )
    .await;
    let cfg = wired.origin.clone();
    let src = Source::remote(cfg);
    // 窗口手里的是远端那一形（恒 `/`；Windows 上本机临时目录要换一下，见 `remote_form` 头注）。
    let dir = remote_form(&root.to_string_lossy());
    // ① 后端答得出。
    let (rows, cut) = list_dir(&wired.line, &src, &dir, SortBy::default())
        .await
        .expect("后端答得出，窗口这一侧却失败了");
    let want = list_local(&root).expect("合成树列得出来");
    assert_eq!(
        rows.iter()
            .map(|r| (r.name.clone(), r.size, r.is_dir))
            .collect::<Vec<_>>(),
        want.iter()
            .map(|r| (r.name.clone(), r.size, r.is_dir))
            .collect::<Vec<_>>(),
        "经通道回来的那一屏与后端那一侧看见的不等"
    );
    assert!(!rows.is_empty(), "合成树是空的 —— 上面那条相等在空集上成立");
    assert_eq!(cut, crate::source::Cut::default());
    // ② 后端拒 ⇒ 失败，带那台的原话（码不上屏）。
    let e = list_dir(&wired.line, &src, "/definitely/not/here", SortBy::default())
        .await
        .expect_err("后端拒了，窗口却交出了一屏");
    assert!(
        copy_core::copy_matches("rsFilewinSource.local.readDirFailed", &e)
            && !e.contains("unreadable"),
        "那句话里没有后端的原话，或错误码上了屏：{e}"
    );
    // ③ 线上恰好两条 `files-ls`（① 一条、② 一条），没有别的。
    assert_eq!(wired.cmds(), [CMD_LS, CMD_LS]);
    std::fs::remove_dir_all(&root).ok();
}

/// 要求：「只有远端，没有本机」（远端路径恒用 `/`）—— Windows runner 上那一形**在这台 Linux 上也跑一遍**。
///
/// 合成后端拿本机临时目录演远端，Windows 上那条目录是 `C:\…`。注入 `\` 当本机分隔符：
/// ① 原样交给窗口（上一版夹具就这样）⇒ 一行的名字是整条路径、面包屑里冒出整条根 —— 正是
///    `listing_has_no_second_road_when_the_backend_refuses` 与 `find` 那两条在 Windows 上的红；
/// ② 过夹具的 `remote_form_with` ⇒ 名字是末段、面包屑逐级、上一级回到根。
#[test]
fn a_windows_temp_dir_is_split_by_the_window_only_after_the_fixture_makes_it_posix() {
    use crate::find::testing::remote_form_with;
    let root = r"C:\Users\RUNNER~1\AppData\Local\Temp\ccm-filewin-1";
    let child = format!(r"{root}\alpha");
    let name_of = |p: &str| {
        row_from_ls_entry(&serde_json::json!({ "path": p, "kind": "dir" }))
            .expect("解得出来")
            .name
            .clone()
    };
    let crumbs_like_root = |r: &str| {
        breadcrumbs(r)
            .into_iter()
            .filter(|(t, _)| t.starts_with(r))
            .count()
    };
    // ① 不换：切不开。
    assert_eq!(name_of(&child), child);
    assert_eq!(crumbs_like_root(root), 1);
    // ② 换成远端那一形：切得开。
    let (r, c) = (remote_form_with(root, '\\'), remote_form_with(&child, '\\'));
    assert_eq!(r, "C:/Users/RUNNER~1/AppData/Local/Temp/ccm-filewin-1");
    assert_eq!(name_of(&c), "alpha");
    assert_eq!(parent_dir(&c), r);
    assert_eq!(crumbs_like_root(&r), 0);
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔补齐五项 2026-09-23〕排序 · 面包屑 · 修改时间 · 逐格表
// ════════════════════════════════════════════════════════════════════════

/// 一摞行的**序指纹**（FNV-1a 64，喂 `名字 ‖ 是不是目录 ‖ 0`）。
///
/// ⚠ 散列本身不是被测对象 —— 它只是把「两万行的序」压成一个能钉进判据的数。
/// 基线那一趟用的是**逐字同一段**散列代码（见下面那条判据的头注）。
fn order_fingerprint(v: &[Listed]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for r in v {
        for b in r.name.bytes().chain([u8::from(r.is_dir), 0]) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// 🔴 **缺省那一档（名称）与本刀之前那个写死的 `sort_rows` 逐字节同序。**
///
/// # 两侧不同源 —— 那个数是在**基线提交上真跑出来的**
///
/// 期望值 `0xb7ba462750a3cc6a` 不是由本文件里任何一行算出来的：它是在基线
/// `66f34e29`（补齐五项之前，`sort_rows(v: &mut [Row])` 还是写死的那一版）的一棵临时工作树里，
/// 对 `corpus::synth_rows(20_000, 0x5EED_0F0D)` 跑**那一版** `sort_rows` 印出来的
/// （同一段散列代码；语料的名字与是否目录两格在两版之间逐字节相同 ——
/// `corpus::synth_rows` 头注写着新那两格一格 rng 都不多抽）。
///
/// ⇒ `Name` 那一支里多出来的那个 `then_with(Equal)` 哪天被人改成真比较一格东西，
///   这条当场红；而 `sort_rows` 整个换一种写法、只要序不变，它照样绿（它判的是行为）。
#[test]
fn the_default_order_is_byte_for_byte_what_it_was_before() {
    let mut v = crate::corpus::synth_rows(20_000, 0x5EED_0F0D);
    let before = order_fingerprint(&v);
    sort_rows(&mut v, SortBy::default());
    assert_eq!(
        order_fingerprint(&v),
        0xb7ba_4627_50a3_cc6a,
        "缺省那一档的序与基线那一版不再逐字节相同 —— 「默认与从前一样」那句话失效了"
    );
    // 反空真：这份语料真的被重排了（不是本来就有序、排了等于没排）。
    assert_ne!(
        before, 0xb7ba_4627_50a3_cc6a,
        "语料本来就是有序的 —— 上面那条在空转"
    );
    // 反空真 ②：「大小」那一档真的给出**不同**的序（`by` 这一格不是被忽略掉的）。
    // ⚠ 「类型」那一档在这份语料上**恰好**与名称同序（现打）：合成名字里没有点，
    //   而旧面板那个怪处让「没有点」的扩展名就是整个名字 ⇒ 按扩展名排 ＝ 按名字排。
    //   那一档的区分力由 `sorting_by_type_follows_the_type_column` 单独喂。
    let mut w = v.clone();
    sort_rows(&mut w, SortBy::Size);
    assert_ne!(
        order_fingerprint(&w),
        0xb7ba_4627_50a3_cc6a,
        "「大小」那一档排出来和名称一模一样 —— 入参没进比较器"
    );
}

/// 🔴 **已经按名称排过的一屏，再按名称排一遍是恒等**（稳定排序 ＋ 同一个比较器）。
///
/// 这是 `list_dir` 那一步「池子排过一次、这里再排一次」不搅序的结构理由。
#[test]
fn sorting_an_already_sorted_screenful_by_name_changes_nothing() {
    let mut once = crate::corpus::synth_rows(5_000, 7);
    sort_rows(&mut once, SortBy::Name);
    let mut twice = once.clone();
    sort_rows(&mut twice, SortBy::Name);
    assert_eq!(once, twice);
    // 阴性对照：换一档**会**改（否则「恒等」可能来自比较器根本没在比）。
    let mut other = once.clone();
    sort_rows(&mut other, SortBy::Size);
    assert_ne!(once, other);
}

fn named(name: &str, is_dir: bool, size: u64) -> Listed {
    Listed::plain(Row {
        name: name.to_string(),
        path: format!("/x/{name}"),
        is_dir,
        size,
        lossy_name: false,
    })
}

fn order_of(v: &[Listed]) -> Vec<&str> {
    v.iter().map(|r| r.name.as_str()).collect()
}

/// 「大小」那一档：目录恒在前 · 大的在前 · 一样大按名称（同旧面板 `b.size - a.size || cmpName`）。
///
/// ⚠ 期望序是**照着 `src/sftp/paths.ts::sortEntries` 手推**的，不是拿本函数跑一遍抄下来的。
#[test]
fn sorting_by_size_matches_the_old_panel() {
    let mut v = vec![
        named("small", false, 1),
        named("Zdir", true, 0),
        named("big", false, 900),
        named("b-tie", false, 50),
        named("a-tie", false, 50),
        named("adir", true, 0),
    ];
    sort_rows(&mut v, SortBy::Size);
    assert_eq!(
        order_of(&v),
        ["adir", "Zdir", "big", "a-tie", "b-tie", "small"]
    );
}

/// 「类型」那一列：按种类（与那一列写的字同源）、再扩展名、相持按名称；没有扩展名的照「文件」那一类排。
#[test]
fn sorting_by_type_follows_the_type_column() {
    let mut v = vec![
        named("b.rs", false, 0),
        named("README", false, 0),
        named("a.md", false, 0),
        named("z.MD", false, 0),
        named("a.rs", false, 0),
        named("src", true, 0),
    ];
    sort_rows(&mut v, SortBy::Type);
    assert_eq!(
        order_of(&v),
        ["src", "a.md", "z.MD", "a.rs", "b.rs", "README"],
        "文本（md）在代码（rs）前，没有扩展名的 `README` 是「文件」那一类、排最后"
    );
}

/// 反过来的那一列只反它自己：目录照旧在前，相持照旧按名称（不分大小写）从小到大。期望序手推。
#[test]
fn a_descending_column_keeps_dirs_first_and_ties_by_name_ascending() {
    let at = |name: &str, is_dir: bool, mtime: u64| {
        let mut r = named(name, is_dir, 0);
        r.mtime_secs = Some(mtime);
        r
    };
    let mut v = vec![
        at("b-old", false, 1),
        at("Tie", false, 5),
        at("old-dir", true, 1),
        at("new", false, 9),
        at("tie2", false, 5),
        at("New-dir", true, 9),
    ];
    sort_rows(&mut v, SortBy::Mtime);
    assert_eq!(
        order_of(&v),
        ["New-dir", "old-dir", "new", "Tie", "tie2", "b-old"]
    );
}

/// 表头那四列是**闭集**，缺省按名称；时间与大小第一下从大到小，再点一下反过来，换一列从那一列的第一下起。
#[test]
fn the_four_columns_and_how_a_click_turns_the_order() {
    assert_eq!(SortBy::default(), SortBy::Name);
    let labels: Vec<String> = SortBy::ALL.iter().map(|b| b.label()).collect();
    assert_eq!(
        labels,
        [
            copy_core::copy_static!("rsFilewinSource.sort.name"),
            copy_core::copy_static!("rsFilewinSource.sort.mtime"),
            copy_core::copy_static!("rsFilewinSource.sort.type"),
            copy_core::copy_static!("rsFilewinSource.sort.size")
        ]
    );
    let s = Sort::default();
    assert!(!s.descending());
    let t = s.after_click(SortBy::Mtime);
    assert!(t.descending() && !t.reversed);
    let t2 = t.after_click(SortBy::Mtime);
    assert!(!t2.descending() && t2.reversed);
    assert_eq!(t2.after_click(SortBy::Name), Sort::default());
}

/// 面包屑：根在最前，每一段各是一个可点的前缀；`//` 与尾巴上的 `/` 不生出空段。
#[test]
fn breadcrumbs_are_every_prefix_with_the_root_first() {
    let own = |v: &[(&str, &str)]| -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
            .collect()
    };
    assert_eq!(
        breadcrumbs("/home/u/带空格 的目录/"),
        own(&[
            ("/", "/"),
            ("home", "/home"),
            ("u", "/home/u"),
            ("带空格 的目录", "/home/u/带空格 的目录"),
        ])
    );
    assert_eq!(
        breadcrumbs("/a//b"),
        own(&[("/", "/"), ("a", "/a"), ("b", "/a/b")])
    );
    // 到顶与空串：恰好一格（根自己），不是零格。
    assert_eq!(breadcrumbs("/"), own(&[("/", "/")]));
    assert_eq!(breadcrumbs(""), own(&[("/", "/")]));
    // 🔴 与 `parent_dir` 同一种切法：每一格的「去哪儿」再取一次上一级，正好是前一格。
    let crumbs = breadcrumbs("/srv/a/b/c");
    for w in crumbs.windows(2) {
        assert_eq!(
            parent_dir(&w[1].1),
            w[0].1,
            "面包屑与「上一级」两种切法漂开了"
        );
    }
    // ⚠ 反斜杠**不是**分隔符（远端路径恒用 `/`）。
    assert_eq!(breadcrumbs("/a\\b").len(), 2);
}

/// 修改时间按本机时区画：当天只写时分、今年写月日、往年写年月日；完整时间到秒；时区差跨日也对（期望手写）。
#[test]
fn a_modification_time_is_printed_in_local_time_short_this_year() {
    // 2023-11-14T22:13:20Z（`date -u -d @1700000000` 现打）。
    let t = mtime_text_at(1_700_000_000, 0, (2026, 10, 5));
    assert_eq!(
        (t.short.as_str(), t.full.as_str()),
        ("2023-11-14", "2023-11-14 22:13:20")
    );
    // 东八区：跨过午夜进了第二天；同一年 ⇒ 月日；就是今天 ⇒ 时分。
    let t = mtime_text_at(1_700_000_000, 8 * 3600, (2023, 1, 1));
    assert_eq!(
        (t.short.as_str(), t.full.as_str()),
        ("11-15", "2023-11-15 06:13:20")
    );
    assert_eq!(
        mtime_text_at(1_700_000_000, 8 * 3600, (2023, 11, 15)).short,
        "06:13"
    );
    assert_eq!(
        mtime_text_at(1_700_000_000, 0, (2023, 11, 15)).short,
        "11-14"
    );
    // 西五区：纪元零点往回退进 1969 年。
    assert_eq!(
        mtime_text_at(0, -5 * 3600, (2026, 1, 1)).short,
        "1969-12-31"
    );
    // 闰日。
    assert_eq!(mtime_text_at(951_782_400, 0, (2000, 3, 1)).short, "02-29");
}

/// `files-ls` 那份声明里的字段名 —— **现读后端源码**（`files.ls` 那条 `Capability` 的 `fields`）。
fn backend_ls_fields() -> std::collections::BTreeSet<String> {
    const CAP_LINE: &str = "name: \"files.ls\",";
    const FIELDS_HEAD: &str = "fields: &[";
    // ⚠ **运行期读**，不是 `include_str!`：后者是一条 monitor → backend 的**编译期**边，
    //   要进 `cross_half_edge_registry` 那张表（写区外）；这一条只要今天那份声明的文本。
    let raw =
        std::fs::read_to_string(crate::guard_support::repo_root().join("src/backend/files/mod.rs"))
            .expect("读不到后端那份 `files/mod.rs`");
    let prod = guard_core::production_code(&raw);
    let at = guard_core::pin_line(&prod, CAP_LINE).expect("后端那条 `files.ls` 能力声明不见了");
    // `fields: &[` 起、到收口的 `]` 止（rustfmt 把长的一摞折成一行一个）。
    let mut lines = prod.lines().skip(at).take(30).map(str::trim);
    let head = lines
        .find_map(|l| l.strip_prefix(FIELDS_HEAD))
        .expect("`files.ls` 那条声明后面 30 行里没有 `fields`");
    let mut line = head.to_string();
    if !head.contains(']') {
        for l in lines.by_ref() {
            line.push_str(l);
            if l.contains(']') {
                break;
            }
        }
    }
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// 🔴 **后端送的每一格 ↔ 窗口这一行的哪一格** —— `row_from_ls_entry` 头注那张表的可判形态。
///
/// 左栏的人群**现读后端声明**（[`backend_ls_fields`]），与下面这张表**两向相等**
/// ⇒ 后端多送一格而窗口没说它去哪 / 表里写着一格而后端早不送了，两种都红。
/// 右栏每一格各挂一条**行为**断言（不判源码）。
#[test]
fn the_field_by_field_table_between_backend_and_window_is_a_judge() {
    const TABLE: &[(&str, &str)] = &[
        ("entries", "那一屏有几行"),
        ("kind", "is_dir ＋ link"),
        ("link_dir", "link_dir（缺 ＝ 否）"),
        ("link_to", "link_broken（`missing` ⇒ 断了；缺 ＝ 否）"),
        ("total", "Cut::total（缺 ＝ 0）"),
        ("mtime_secs", "mtime_secs（缺 ＝ None）"),
        (
            "mtime_text",
            "mtime_text（后端写好的短写法，原样；缺 ＝ None）",
        ),
        (
            "mtime_full",
            "mtime_full（后端写好的完整写法，原样；缺 ＝ None）",
        ),
        ("path", "path · name · lossy_name"),
        ("size", "size（缺 ＝ 0）"),
        ("truncated", "界面上那句「只拿到了前 N 条」"),
        ("unreadable", "界面上那句「有 n 项读不出来」（缺 ＝ 0）"),
    ];
    let declared: std::collections::BTreeSet<String> =
        TABLE.iter().map(|(k, _)| (*k).to_string()).collect();
    assert_eq!(
        backend_ls_fields(),
        declared,
        "后端 `files.ls` 声明的字段与窗口这张逐格表对不上"
    );

    // ── 右栏：逐格行为 ──
    let one = |v: serde_json::Value| row_from_ls_entry(&v).expect("这一条该解得出来");
    // kind：两格，不是一格。
    let link = one(serde_json::json!({ "path": "/a/l", "kind": "symlink" }));
    assert!(link.link && !link.is_dir, "symlink 没落成 link");
    let dir = one(serde_json::json!({ "path": "/a/d", "kind": "dir" }));
    assert!(dir.is_dir && !dir.link);
    let file = one(serde_json::json!({ "path": "/a/f", "kind": "file" }));
    assert!(!file.is_dir && !file.link);
    // link_dir：链接且为真 ⇒ 点得进去（is_dir 仍为假：删 / 复制按链接本身算）；缺 ⇒ 否。
    let to_dir = one(serde_json::json!({ "path": "/a/ld", "kind": "symlink", "link_dir": true }));
    assert!(to_dir.link && to_dir.link_dir && !to_dir.is_dir && to_dir.opens_as_dir());
    assert!(!link.link_dir && !link.opens_as_dir());
    // link_to：`missing` ⇒ 断了的链接；指向得到的 ⇒ 不是。
    let broken =
        one(serde_json::json!({ "path": "/a/bl", "kind": "symlink", "link_to": "missing" }));
    assert!(broken.link_broken && !to_dir.link_broken && !link.link_broken);
    // mtime_secs：原样；缺了是 None，不是 0（1970）。
    let t =
        one(serde_json::json!({ "path": "/a/t", "kind": "file", "mtime_secs": 1_700_000_000u64 }));
    assert_eq!(t.mtime_secs, Some(1_700_000_000));
    assert_eq!(file.mtime_secs, None);
    // mtime_text · mtime_full：后端按那台本地钟写好的两格，原样（窗口不换算）；缺了是 None。
    let tx = one(
        serde_json::json!({ "path": "/a/x", "kind": "file", "mtime_secs": 1u64, "mtime_text": "10-02", "mtime_full": "2026-10-02 15:01:23" }),
    );
    assert_eq!(
        (tx.mtime_text.as_deref(), tx.mtime_full.as_deref()),
        (Some("10-02"), Some("2026-10-02 15:01:23"))
    );
    assert_eq!(
        (file.mtime_text.as_deref(), file.mtime_full.as_deref()),
        (None, None)
    );
    // size：缺 ⇒ 0。
    let s = one(serde_json::json!({ "path": "/a/s", "kind": "file", "size": 42 }));
    assert_eq!((s.size, file.size), (42, 0));
    // path：字节 → 名字，有损那一格跟着**名字那一段**的字节走（目录有损、名字干净 ⇒ 名字照样能用）。
    let lossy = one(serde_json::json!({ "path": { "b16": "2f612f66ff" }, "kind": "file" }));
    assert!(lossy.lossy_name && lossy.name.ends_with('\u{FFFD}'));
    let in_lossy_dir =
        one(serde_json::json!({ "path": { "b16": "2fff2f72656164" }, "kind": "file" }));
    assert_eq!(
        (
            in_lossy_dir.name.as_str(),
            in_lossy_dir.lossy_name,
            &in_lossy_dir.raw_name
        ),
        ("read", false, &None),
        "目录不是合法 UTF-8、名字是 —— 这一行被当成了名字有损"
    );
    assert_eq!(file.name, "f");
    // entries / truncated：由 `rows_from_ls_data` 摊开。
    let (rows, cut) = rows_from_ls_data(
        &serde_json::json!({ "entries": [ { "path": "/a/x", "kind": "file" } ], "truncated": true, "unreadable": 3, "total": 9 }),
        SortBy::default(),
    )
    .unwrap();
    assert_eq!(
        (rows.len(), cut),
        (
            1,
            crate::source::Cut {
                truncated: true,
                unreadable: 3,
                total: 9,
            }
        )
    );
}

/// 🔴 **盘上每一处按 `/` 切远端路径的地方都在这张表里，表里也没有死行。**
///
/// `parent_dir` / `remote_basename` / `breadcrumbs` 那一族的头注说「多一份切法就多一种
/// 『Windows 上 `\` 被当分隔符』的机会」—— 这一条把那句话从散文变成相等断言。
///
/// # 人群与口径
///
/// - 人群：`filewin/` 生产段（`guard_core::production_code` 剥过测试与注释）。窗口独立成包之后那是三棵：
///   窗口包本身 · monitor 那一侧 `shell/src/filewin/`（开窗入口也切「跳到这个文件」）· 契约 crate `filewin-contract`
///   （`parent_dir` / `remote_basename` 搬去那里、两边共用一份）；后两棵的键带前缀。
/// - 算一处「切」：一行里出现 `.split('/')` · `.rsplit('/')` · `.rfind('/')` ·
///   `.split_terminator('/')` 任一形；记成 `(文件, 所在函数)`。
/// - ⚠ **不算**：`trim_end_matches('/')`（剥尾巴，不切）· `push('/')`（拼，不切）·
///   `matches('/').count()`（数深度，不切）。它们不产生「哪一段是名字」的判断。
#[test]
fn every_place_that_splits_a_remote_path_is_declared() {
    const SPLITS: &[&str] = &[
        ".split('/')",
        ".rsplit('/')",
        ".rfind('/')",
        ".split_terminator('/')",
    ];
    const FN_WORD: &str = "fn ";
    /// `(文件, 函数, 为什么它可以切)`。
    const DECLARED: &[(&str, &str, &str)] = &[
        (
            "corpus.rs",
            "measure",
            "合成语料的统计（深度 / 段长），不是远端路径",
        ),
        ("corpus.rs", "synth_rows", "合成语料取名字，不是远端路径"),
        ("source.rs", "breadcrumbs", "面包屑那一摞前缀"),
        ("filewin-contract/lib.rs", "parent_dir", "上一级"),
        ("filewin-contract/lib.rs", "remote_basename", "尾段"),
        (
            "shell/filewin/proc.rs",
            "landing_dir",
            "自带那份窗口程序的本机落点：契约里 `/` 分隔的相对路径逐段拼进本机家目录，不是远端路径",
        ),
    ];
    let repo = crate::guard_support::repo_root();
    let mut found: std::collections::BTreeSet<(String, String)> = Default::default();
    let mut files = 0usize;
    let trees = [
        (crate::guard_support::crate_src_root(), ""),
        (
            repo.join("src/frontend/shell/src/filewin"),
            "shell/filewin/",
        ),
        (
            repo.join("src/common/filewin-contract/src"),
            "filewin-contract/",
        ),
    ];
    for (dir, label) in &trees {
        for (path, src) in guard_core::scan_tree_excluding(dir, &["rs"], &[]) {
            files += 1;
            let file = format!("{label}{}", path.file_name().unwrap().to_string_lossy());
            let prod = guard_core::production_code(&src);
            let mut current = String::new();
            for line in prod.lines() {
                if let Some(name) = fn_name_on(line, FN_WORD) {
                    current = name;
                }
                if SPLITS.iter().any(|n| guard_core::contains_word(line, n)) {
                    found.insert((file.clone(), current.clone()));
                }
            }
        }
    }
    assert!(
        files >= 16,
        "`filewin/` 只扫到 {files} 份 —— 遍历器坏了，下面那条会在空集上成立"
    );
    let want: std::collections::BTreeSet<(String, String)> = DECLARED
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    assert_eq!(
        found, want,
        "按 `/` 切远端路径的地方与登记表对不上。\n  多出来的（新写了一份切法）：{:?}\n  表里有、盘上没了的：{:?}",
        found.difference(&want).collect::<Vec<_>>(),
        want.difference(&found).collect::<Vec<_>>()
    );
}

/// 这一行是不是一个函数头；是的话回函数名。
fn fn_name_on(line: &str, fn_word: &str) -> Option<String> {
    let t = line.trim_start();
    let at = t.find(fn_word)?;
    // `fn ` 之前只许是可见性 / `async` / `const` 之类的词（不许是一段表达式）。
    let head = &t[..at];
    if !head.split_whitespace().all(|w| {
        matches!(
            w,
            "pub" | "pub(crate)" | "pub(super)" | "async" | "const" | "unsafe"
        )
    }) {
        return None;
    }
    let rest = &t[at + fn_word.len()..];
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// 🔴**通道那三层失败，每一层说的话都不一样** —— 而「发没发出去」那一格一定说出来。
///
/// 写面那几条据此决定要不要再点一次：`NotSent` ＝ 这一下没生效；`Sent` / `Unknown` ＝ 对面可能已经做了。
/// 压成同一句的话，用户在「删除」上再点一次就可能删两遍。
#[test]
fn each_layer_of_a_channel_failure_says_something_different() {
    use comms_inward::chan::wire::{Body, CallError, HopFault, HopId, OursFault, PeerFault, Reach};
    let hop = |idx, reach| CallError::Hop {
        at: HopId { idx, tag: "wait" },
        reach,
        why: HopFault::Overrun,
    };
    let not_sent = said("files-delete", &hop(1, Reach::NotSent));
    let unknown = said("files-delete", &hop(1, Reach::Unknown));
    let sent = said("files-delete", &hop(1, Reach::Sent));
    assert_ne!(not_sent, unknown, "「没发出去」与「拿不准」说成了同一句");
    assert_eq!(
        unknown, sent,
        "`Sent` 与 `Unknown` 对用户是同一件事（对面可能已经做了）"
    );
    assert_ne!(
        said("x", &hop(0, Reach::NotSent)),
        said("x", &hop(1, Reach::NotSent)),
        "断在哪一段没说出来"
    );
    // 对端拒绝：原话在、码不上屏（走 `find::refusal` 那一个翻译）。
    let refused = said(
        "files-mkdir",
        &CallError::Peer {
            why: PeerFault::Refused {
                body: Body(br#"{"code":"refused","message":"refuse write: fence"}"#.to_vec()),
            },
        },
    );
    assert!(
        refused.contains("refuse write: fence") && !refused.contains("refused"),
        "{refused}"
    );
    // 不认这条命令 ⇒ 说「版本旧了、不支持」那一条。
    // 按文案键断言，不按原文（CP1 裁掉了命令名，「是哪条命令」那一维不在句子里）。
    assert_eq!(
        said(
            "files-chmod",
            &CallError::Peer {
                why: PeerFault::Unsupported
            }
        ),
        copy_text("rsFilewinSource.said.unknownCmd", &[])
    );
    // 本侧三种互不相同。
    let ours: std::collections::BTreeSet<String> =
        [OursFault::Cancelled, OursFault::Misuse, OursFault::Broken]
            .into_iter()
            .map(|why| said("x", &why.into()))
            .collect();
    assert_eq!(ours.len(), 3, "本侧三种错说成了同一句");
}

/// 🔴 **那台送来的文件时间窗口不换算**：生产代码里叫本机钟的 [`mtime_text`] 的地方 == 下面这张登记表（逐文件计数，两向）。
/// 登记的四处都是**这台自己的事**：断线于（`chrome.rs`）· 一件传输收尾于（`progress.rs`）· 保存于（`shell.rs`）·
/// 上传撞名表「这台」那一格（`transfer.rs`，这台盘上的文件）。列表 · 搜索结果 · 属性 · 预览 · 撞名表「那台」那一格照抄后端写好的字，
/// 谁把它们改回 `mtime_text(…)`，这里多出一处就红。
#[test]
fn only_this_machines_own_moments_go_through_the_local_clock() {
    const REGISTERED: &[(&str, usize)] = &[
        ("chrome.rs", 1),
        ("progress.rs", 1),
        ("shell.rs", 1),
        ("transfer.rs", 1),
    ];
    let mut got: Vec<(String, usize)> =
        guard_core::scan_tree_excluding(&crate::guard_support::crate_src_root(), &["rs"], &[])
            .into_iter()
            .filter_map(|(path, raw)| {
                let prod = guard_core::production_code(&raw);
                // 定义那一份（`source.rs`）里的 `fn mtime_text(` 不算调用处。
                let n =
                    prod.matches("mtime_text(").count() - prod.matches("fn mtime_text(").count();
                (n > 0).then(|| (path.file_name().unwrap().to_string_lossy().to_string(), n))
            })
            .collect();
    got.sort();
    let want: Vec<(String, usize)> = REGISTERED
        .iter()
        .map(|(f, n)| ((*f).to_string(), *n))
        .collect();
    assert_eq!(
        got, want,
        "按本机钟画时间的地方与登记表对不上（那台送来的时间要照抄后端写好的字）"
    );
}

/// 复制详情（条带 §5.2）：对端拒了 ⇒ 那台后端写好的那份原样；通道这一跳没走通 ⇒ 窗口进程自己写（机器 · 命令 · 断在 · 码）。
#[test]
fn a_failure_carries_the_detail_the_peer_wrote_or_the_window_writes_its_own() {
    use comms_inward::chan::wire::{Body, CallError, HopFault, HopId, PeerFault, Reach};
    let label = |k: &str| copy_text(k, &[]);
    let wrote = format!("{}：io_failed", label("detail.label.code"));
    let refused = CallError::Peer {
        why: PeerFault::Refused {
            body: Body(
                serde_json::to_vec(
                    &serde_json::json!({"code": "io_failed", "message": "m", "detail": wrote}),
                )
                .unwrap(),
            ),
        },
    };
    assert_eq!(
        detail_of(&Origin("devbox".into()), "files-commit-upload", &refused),
        wrote
    );
    let hop = CallError::Hop {
        at: HopId {
            idx: 1,
            tag: "open",
        },
        reach: Reach::NotSent,
        why: HopFault::Unreachable,
    };
    let d = detail_of(&Origin("devbox".into()), "files-ls", &hop);
    for want in [
        format!(
            "{}：devbox（{}）",
            label("detail.label.machine"),
            label("detail.value.notConnected")
        ),
        format!("{}：files-ls", label("detail.label.command")),
        format!("{}：1:open NotSent", label("detail.label.hop")),
        format!("{}：Unreachable", label("detail.label.code")),
    ] {
        assert!(d.contains(&want), "缺「{want}」：\n{d}");
    }
    assert!(d.starts_with(&label("detail.label.at")), "{d}");
}

/// ［复制详情］只在有详情时出：复制出去的首行是屏上那一句；一行汇总底下几件各一段，没一件带详情 ⇒ 不出。
#[test]
fn the_copy_body_exists_only_when_there_is_a_detail() {
    assert_eq!(Failed::copy_body("那一句", ""), None);
    assert_eq!(Failed::copy_body("那一句", "  \n"), None);
    assert_eq!(
        Failed::copy_body("那一句", "码：x"),
        Some("那一句\n码：x".to_string())
    );
    let with = Failed {
        code: None,
        said: "句 A".into(),
        detail: "码：a".into(),
    };
    let without: Failed = "句 B".to_string().into();
    assert_eq!(
        Failed::copy_many("头", &[("b".into(), without.clone())]),
        None
    );
    let body = Failed::copy_many("头", &[("a".into(), with), ("b".into(), without)]).unwrap();
    assert!(body.starts_with("头\n\n"), "{body}");
    assert!(body.contains("码：a") && !body.contains("句 B"), "{body}");
}
