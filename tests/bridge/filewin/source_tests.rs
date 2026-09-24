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
/// 远端那一侧的序由 `sftp_pool::sort_entries` 产生，它是私有的；
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

    // 生产契约逐字（`sftp_pool.rs::sort_entries`）：目录在前，再名称小写升序。
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

/// 远端那条路上唯一有逻辑的一段：**六个字段一个都不许掉**。
/// **这是真行为判据**（不是判源码）—— 它不需要网络。
///
/// 🔴〔补齐五项 2026-09-23〕**这条判据此前漏了一格，而那一格正是一条真缺口。**
/// 它原来只对拍五格（判词逐字「五个字段一个都不许掉」），而 `SftpEntry`
/// 一直在送 `is_symlink` —— 于是「退路那一屏也看不出哪个是符号链接」
/// 在这条判据上**一点痕迹都没有**：夹具里 `is_symlink: true` 摆在那儿，
/// 没有任何一侧读它。⇒ 现在对拍的是整个 [`Listed`]（六格），
/// 那一格漂开当场红。
#[test]
fn the_sftp_entry_mapping_carries_every_field() {
    let e = crate::sftp_pool::SftpEntry {
        name: "\u{FFFD}odd".to_string(),
        path: "/remote/dir/\u{FFFD}odd".to_string(),
        is_dir: false,
        is_symlink: true,
        size: 4242,
        lossy_name: true,
    };
    let r = row_from_sftp_entry(e);
    assert_eq!(
        r,
        Listed {
            row: Row {
                name: "\u{FFFD}odd".to_string(),
                path: "/remote/dir/\u{FFFD}odd".to_string(),
                is_dir: false,
                size: 4242,
                // 🔴 有损名必须一路传到行上 —— 写操作要靠它灰置。
                lossy_name: true,
            },
            // 🔴 符号链接那一格：SFTP 这条**退路**也送得出它，别在这儿丢掉。
            link: true,
            // ⚠ `None` 是「SFTP 交不出这一格」，不是「这个文件没有时间」
            //    （`SftpEntry` 里压根没有 mtime）。
            mtime_secs: None,
        }
    );
}

/// ⚠ **判源码是代理，不是标的**（`tests/bridge/sftp_tests.rs` 同款如实标注）。
///
/// 买的是：远端列目录**走的是共用那条池**，没有人在本模块里另开一条。
/// 买不到：那条池今天真连得上、连上之后回的东西对不对。
#[test]
fn the_remote_path_delegates_to_the_shared_pool_instead_of_rolling_its_own() {
    let prod =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/source.rs"));
    let at = prod
        .find("pub async fn list_remote")
        .expect("生产段里找不到 `list_remote` —— 抽取器坏了，本条此刻无效");
    let body = &prod[at..];
    let end = body.find("\n}").map(|i| i + 2).unwrap_or(body.len());
    let body = &body[..end];
    assert!(
        body.contains("sftp_pool::sftp_list_dir"),
        "`list_remote` 不再调共用池的 `sftp_list_dir` 了 —— \
             那意味着这里长出了第二条列远端目录的路（也就是第二个连接池）"
    );
    assert!(
        !body.contains("connect_sftp") && !body.contains("read_dir"),
        "`list_remote` 里出现了自己建连接 / 自己 read_dir 的痕迹"
    );
}

/// 🔴 **第二刀把上面那条从「一个函数体」扩到「整棵 `filewin/`」。**
///
/// 上一版只看 `list_remote` 那一个函数体。它接不住的那一形，正是这一刀会长出来的：
/// 新加的入口（`entry.rs`）或传输层（`transfer.rs`）里**另开一条**列远端目录的路
/// —— 那个函数体一个字都不会变，而池被拆成了两份。
///
/// ⇒ 判「整棵树上列远端目录的路**恰好一条**」：
/// `sftp_list_dir` 的调用形状在整棵 `filewin/` 的生产段里出现**恰好 1 次**，
/// 而且那 1 次在 `source.rs` 里。
///
/// ⚠ 买的是「只有一条路，且那条路调共用池」；
/// 买不到那条池今天连得上（红线不许起真连接）。
#[test]
fn the_whole_filewin_tree_has_exactly_one_way_to_list_a_remote_directory() {
    let root = crate::guard_support::crate_src_root().join("filewin");
    let files = guard_core::files_by_extension(&root, "rs");
    // 反空真①：扫描面没塌。这棵树今天 **9** 份
    //（copy/corpus/entry/mod/rows/scale/shell/source/transfer）——
    //〔第三刀 09-20〕`copy.rs` 进来之后从 8 变 9，这个地板跟着抬。
    assert!(
        files.len() >= 9,
        "`filewin/` 下只扫到 {} 份 `.rs`（{files:?}）—— 扫描面塌了，下面几条在空转",
        files.len()
    );
    for must in [
        "source.rs",
        "shell.rs",
        "entry.rs",
        "transfer.rs",
        "copy.rs",
    ] {
        assert!(
            files.iter().any(|f| f == must),
            "扫描面里没有 `{must}` —— 抽取器坏了"
        );
    }

    let needle = format!("sftp_pool::sftp_{}(", "list_dir");
    let mut where_: Vec<(String, usize)> = Vec::new();
    let mut total_prod = 0usize;
    for f in &files {
        let src = std::fs::read_to_string(root.join(f)).expect("read filewin rs");
        let prod = guard_core::production_code(&src);
        total_prod += prod.len();
        let n = prod.matches(needle.as_str()).count();
        if n > 0 {
            where_.push((f.clone(), n));
        }
        // 自己开连接 / 自己 read_dir 远端，一处都不许有。
        for banned in ["connect_sftp", "russh_sftp", "SftpSession", "with_sftp("] {
            assert!(
                !prod.contains(banned),
                "`filewin/{f}` 的生产段里出现了 `{banned}` —— \
                 这棵树不许自己碰连接／通道，远端那一侧只能走 `sftp_pool` 那几条既有命令"
            );
        }
    }
    // 反空真②：剥法没把整棵树剥没。
    assert!(
        total_prod > 20_000,
        "整棵树的生产段只剩 {total_prod} 字节 —— 剥法坏了"
    );

    assert_eq!(
        where_,
        vec![("source.rs".to_string(), 1usize)],
        "列远端目录的路不再是「`source.rs` 里恰好一条」—— 实得 {where_:?}。\n\
         多一处就是多一个「怎么列远端」的答案；而这棵树上每一条自己开的路，\n\
         都会把 `设计/60 §5.4a` 那条「6 − 4 = 2 格永远留给浏览」的预算拆成两份。"
    );

    // 反空真③：这把尺子认得出「多一处」。
    let fake = format!("fn x() {{ {needle} }}\n{needle}");
    assert_eq!(
        guard_core::production_code(&fake)
            .matches(needle.as_str())
            .count(),
        2
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
    // 🔴 **阴性对照**：换成 `std::path` 的那一形在这几格上会给出不同的答案
    //    ⇒ 上面那一比不是恒真的。
    #[cfg(windows)]
    assert_ne!(
        std::path::Path::new("/srv/a\\b/c")
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        "/srv/a\\b".to_string(),
        "这台机器上 `std::path` 与本算法答得一样 —— 那上面那一比在这台机器上买不到东西"
    );
}

#[test]
fn a_missing_dir_is_an_error_not_an_empty_list() {
    let missing = std::env::temp_dir().join("ccm-filewin-does-not-exist-9d3f1a");
    let r = list_local(&missing);
    assert!(r.is_err(), "不存在的目录必须报错，不许静默返回空列表");
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第七刀 2026-09-21〕远端 home 那一跳 —— `sftp_realpath` 在窗口这侧的落点
// ════════════════════════════════════════════════════════════════════════
//
// `resolve_remote_home` 整条路要真远端（红线不许起真连接）⇒ 它自己**没有逻辑**，
// 有逻辑的那一段抽成了 `start_dir_from_realpath`，判据全落在它身上。
// 失败路径那一半（问不到就别开窗）住 `entry_tests` 那条。

/// 规矩的服务端回一条绝对路径 ⇒ 原样当起点。
#[test]
fn an_absolute_answer_becomes_the_start_directory() {
    assert_eq!(start_dir_from_realpath("/home/user").unwrap(), "/home/user");
    // 周围的空白不算内容（SFTP 的实现里见过带尾换行的）。
    assert_eq!(
        start_dir_from_realpath("  /srv/data\n").unwrap(),
        "/srv/data"
    );
    // 根自己是合法起点。
    assert_eq!(start_dir_from_realpath("/").unwrap(), "/");
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
    assert_eq!(start_dir_from_realpath("/srv/data/").unwrap(), "/srv/data");
    // 根那一格**不能**被剥成空串。
    assert_eq!(start_dir_from_realpath("///").unwrap(), "/");
    // 两种写法归一（这就是「一种写法」那句话的相等断言）。
    assert_eq!(
        start_dir_from_realpath("/srv/data/").unwrap(),
        start_dir_from_realpath("/srv/data").unwrap()
    );

    // 🔴 把那个假前提**钉成读数**：`parent_dir` 自己就吃得下尾斜杠
    //    ⇒ 哪天它不吃了，本条会红，而那时才轮到「剥这一步变承重了」这句话。
    assert_eq!(
        parent_dir("/srv/data/"),
        parent_dir("/srv/data"),
        "`parent_dir` 不再自己吃尾斜杠了 —— 那么 `start_dir_from_realpath` 里剥那一步\
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
        let e = start_dir_from_realpath(bad).expect_err(&format!("`{bad:?}` 竟然被当成了合法起点"));
        assert!(!e.trim().is_empty(), "`{bad:?}` 的报错是空串");
        // 报错里要带上对面那句原文（否则用户不知道是谁答错了）。
        if !bad.trim().is_empty() {
            assert!(e.contains(bad.trim()), "报错没带上对面答的那句原文：{e}");
        }
    }
}

/// 🔴〔第十刀〕远端 basename —— **只按 `/` 切**。
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
// 🔴〔第十二刀 2026-09-22〕后端做，前端拿结果 —— 只读那一侧
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
    assert!(!truncated);
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
    let e = rows_from_ls_data(&d, SortBy::default()).expect_err("有一条解不出来，整趟却成功了 —— 那一行被悄悄吞了");
    assert!(e.contains("第 1 条"), "报错没说是第几条：{e}");
    // 阴性对照：三条都好的时候它成得了（否则上面可以靠「什么都失败」全绿）。
    let ok = serde_json::json!({ "entries": [ { "path": "/a/x", "kind": "file" } ] });
    assert_eq!(rows_from_ls_data(&ok, SortBy::default()).unwrap().0.len(), 1);
}

/// `truncated` 带得回来（缺了就当没截断）。
#[test]
fn truncation_is_carried_back_not_dropped() {
    let d = serde_json::json!({ "entries": [], "truncated": true });
    assert!(rows_from_ls_data(&d, SortBy::default()).unwrap().1, "截断那一格被丢了");
    let d2 = serde_json::json!({ "entries": [] });
    assert!(!rows_from_ls_data(&d2, SortBy::default()).unwrap().1, "缺了就该当没截断");
}

/// 🔴 **退路在，而且排在问后端之后；而且它只有一支。**
///
/// ⚠ 同上，判源码是代理：`list_dir` 那条路要一条真后端通道才走得完，
/// 而判据不许在进程级登记表上种一个 `<local>` 通道
///（那张表是**进程内全局**的，`inbound_client` 自己的头注记着两条判据在同一个键上
/// 起真后端会互相看见对方登记的通道 ⇒ 种它就是给别的判据下毒）。
///
/// # 🔴〔2026-09-23 本机侧退役〕**退路从两支变一支，而本条的三个断言都换了写法**
///
/// 从前这里逐支钉（`list_local(` = 本机那条 · `list_remote(` = 远端那条），
/// 而三处匹配全是**裸的**（`prod.find("…")` / `body.contains("…")`）。
/// 本机那一侧不在了（`source.rs` 头注那块墓碑）⇒ 少一支；顺带把三处
/// 换成 `guard_core::pin_line`（**整行相等**）：
///
/// - 裸子串匹配的病形是「匹配单位比事实小」—— `body.find("list_remote(")` 在这份
///   文件上**现打命中 2 处**（`list_dir` 体内那一处 ＋ 下面 `pub async fn list_remote(`
///   那一行），于是「退路排在问后端之后」那句话可能锚在**函数声明**上，
///   而那是恒真的（声明永远在后面）。⚠ 这不是假想：换成 `find_pinned` 的那一趟
///   它当场以「命中 2 处，断言指不明是哪一处」红了。
/// - `needle_anchor_registry` 那条递减棘轮今天**零富余** ⇒ 新写一处裸匹配当场红。
///   本条这一换让那个数**往下走三格**，不是持平。
///
/// ⚠ 整行相等买到什么、买不到什么：买到「那一行逐字是这样」（缩进被 `trim` 掉）；
/// **买不到**「这一行在语义上属于 `list_dir`」—— 那靠它与 `fn` 那一行的**行序**。
#[test]
fn the_fallback_exists_and_comes_after_asking_the_backend() {
    let prod =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/source.rs"));
    let at_fn = guard_core::pin_line(
        &prod,
        // ⚠ 钉的是**函数头那一行**（签名被 rustfmt 折成多行之后，整条签名不再是「一行」）。
        //   `pub async fn list_dir(` 在这份文件里恰好一行，`pin_line` 自带那道自检。
        "pub async fn list_dir(",
    )
    .expect("`list_dir` 的签名不在生产段里（或者它换了形状）");
    let at_ask = guard_core::pin_line(
        &prod,
        "match list_via_backend(&source.origin(), dir, LS_LIMIT, by).await {",
    )
    .expect("`list_dir` 里没有问后端那一跳 —— 那它就不是主路了");
    let at_fb = guard_core::pin_line(
        &prod,
        "let mut rows = list_remote(source.cfg(), dir).await?;",
    )
    .expect("那条退路不在了（或者它换了形状 —— 那就把这一行一起改）");
    // 🔴 三行的**顺序**是承重的：`fn` → 问后端 → 退路。
    assert!(
        at_fn < at_ask && at_ask < at_fb,
        "三行的顺序不对（fn {at_fn} / 问后端 {at_ask} / 退路 {at_fb}）—— \
         退路排在问后端之前的话，后端那条主路永远走不到"
    );
    // 🔴 退路必须**交出一句话**（`ListVerdict` 是 `Some`）——
    //    静默降级与成功在屏幕上长得一样。
    let at_say = guard_core::pin_line(&prod, "Ok((rows, false, Some(why)))")
        .expect("退路没把原因交出去 —— 那就是一次静默降级");
    assert!(at_fb < at_say, "那句话不在退路后面（fn 体被重排过？）");
}
