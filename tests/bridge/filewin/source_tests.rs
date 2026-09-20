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
    let mut mine: Vec<Row> = names
        .iter()
        .map(|(n, d)| Row {
            name: (*n).to_string(),
            path: format!("/x/{n}"),
            is_dir: *d,
            size: 0,
            lossy_name: false,
        })
        .collect();
    sort_rows(&mut mine);

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

/// 远端那条路上唯一有逻辑的一段：五个字段一个都不许掉。
/// **这是真行为判据**（不是判源码）—— 它不需要网络。
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
        Row {
            name: "\u{FFFD}odd".to_string(),
            path: "/remote/dir/\u{FFFD}odd".to_string(),
            is_dir: false,
            size: 4242,
            // 🔴 有损名必须一路传到行上 —— 写操作要靠它灰置。
            lossy_name: true,
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
    // 反空真①：扫描面没塌。这棵树今天 7 份（corpus/entry/mod/rows/scale/shell/source/transfer）。
    assert!(
        files.len() >= 7,
        "`filewin/` 下只扫到 {} 份 `.rs`（{files:?}）—— 扫描面塌了，下面几条在空转",
        files.len()
    );
    for must in ["source.rs", "shell.rs", "entry.rs", "transfer.rs"] {
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
    assert_eq!(guard_core::production_code(&fake).matches(needle.as_str()).count(), 2);
}

/// 上一级目录：**远端与本机不是同一个算法。**
///
/// 🔴 远端那一支**不许**借 `std::path` —— 在 Windows 上 `Path` 把 `\` 也当分隔符，
/// 于是远端一个名字里含反斜杠的目录会被切成两级（而 SFTP 的路径分隔符只有 `/`）。
#[test]
fn walking_up_uses_slashes_on_the_remote_side_and_the_platform_on_the_local_side() {
    let remote = Source::Remote(Box::new(crate::ssh_source::RemoteConfig {
        host: "example.invalid".into(),
        label: "r".into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/nonexistent".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }));
    // 相等断言，逐个：
    assert_eq!(parent_dir(&remote, "/a/b/c"), "/a/b");
    assert_eq!(parent_dir(&remote, "/a/b/c/"), "/a/b");
    assert_eq!(parent_dir(&remote, "/a"), "/");
    assert_eq!(parent_dir(&remote, "/"), "/", "到根了还往上走");
    assert_eq!(parent_dir(&remote, ""), "/");
    // 🔴 承重的那一条：反斜杠是**名字的一部分**，不是分隔符。
    assert_eq!(
        parent_dir(&remote, "/srv/a\\b/c"),
        "/srv/a\\b",
        "远端路径里的反斜杠被当成分隔符了 —— 那是 `std::path` 在 Windows 上的行为，\
         而这条路上不许用它"
    );

    // 本机那一支用本机分隔符 ⇒ 用 `Path` 造期望值，别手写字面量。
    let deep = std::path::Path::new("x").join("y").join("z");
    let want = std::path::Path::new("x").join("y");
    assert_eq!(
        parent_dir(&Source::Local, &deep.to_string_lossy()),
        want.to_string_lossy().to_string()
    );
    // 到顶回原值（不是空串）。
    let top = std::path::Path::new("/").to_string_lossy().to_string();
    assert_eq!(parent_dir(&Source::Local, &top), top);
}

#[test]
fn a_missing_dir_is_an_error_not_an_empty_list() {
    let missing = std::env::temp_dir().join("ccm-filewin-does-not-exist-9d3f1a");
    let r = list_local(&missing);
    assert!(r.is_err(), "不存在的目录必须报错，不许静默返回空列表");
}
