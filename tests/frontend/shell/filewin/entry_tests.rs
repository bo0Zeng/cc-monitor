//! # （开窗前那一屏：列不出来就报错、不开窗）＋（入口与三种落点）
//!
//! 核原文：「开窗前那一屏：monitor 侧经宿主注入的同一个句柄问 `files-home`（不给落点时）与 `files-ls`；
//! 列不出来就带原文报错」（〔主会话 09-28 裁 3〕「谁去问」改成窗口进程自己，那句待设计侧改写）；「三种落点与 `filewin/entry.rs::plan_target` 三支一一对应」·
//! 「判据：入口人群两向相等、`open_file_window` 在包装层外恰好一处」—— 本族判的正是先问后开、三支落点、命令真接到前端。

use super::*;

fn synth_cfg() -> RemoteConfig {
    RemoteConfig {
        host: "example.invalid".into(), // RFC 2606 保留 ⇒ DNS 解不出来
        label: "synthetic-origin".into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

// 〔主会话 09-28 裁 3〕上一版这里两条行为判据判的是 monitor 这一侧先问 home / 先列一屏：
//   `an_empty_path_asks_the_remote_for_home_and_opens_nothing_when_it_cannot`〔散文墓碑〕
//   `a_directory_we_cannot_list_is_an_error_not_a_blank_window`〔散文墓碑〕
//   那两问进了窗口进程，性质（没给目录才问 home · 列不出来带原话、不开窗）搬到
//   `proc_tests::the_first_screen_asks_home_only_when_told_nothing`（真通道口 ＋ 替身后端）与
//   `proc_tests::the_window_process_lists_first_and_the_parent_carries_its_words`（替身窗口进程说那一行）。

/// 🔴 **通道口没起来 ⇒ 一个窗口进程都不起**（`D11`：窗口只有这一条路够后端），而且话不是空的。
///
/// 判据进程里 `chan::host::start` 没调过 ⇒ 交接件拿不到。⚠ 本条买的是入口这一格的早退，
/// 不是「列不出来」那一形（那一形住 `proc_tests`）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_channel_no_window_process_is_started() {
    let before = cc_monitor_filewin::shell::open_requested();
    for (path, reveal) in [
        ("   ", None),
        ("/srv/whatever", None),
        ("", Some("/a/b.txt")),
    ] {
        let e = open_with(
            synth_cfg(),
            path.into(),
            reveal.map(str::to_string),
            None,
            Box::new(|_| {}),
        )
        .await
        .expect_err("没有通道口竟然开了窗");
        assert_eq!(
            e,
            copy_text("rsFilewinEntry.open.noHost", &[]),
            "早退的话不对：{e}"
        );
    }
    assert_eq!(
        cc_monitor_filewin::shell::open_requested(),
        before,
        "没有通道口却请求开窗了"
    );
}

/// 🔴〔09-28 裁 3〕**窗口进程列不出来的那句原话原样到 webview**；进程层的错才套「文件窗口没起来」。
#[test]
fn the_window_process_words_reach_the_webview_verbatim() {
    let said = "那台说：没有这个目录 /srv/不在";
    assert_eq!(unopened_said(Unopened::Said(said.into())), said);
    let wrapped = unopened_said(Unopened::Process("退出码 1".into()));
    assert_eq!(
        wrapped,
        copy_text("rsFilewinEntry.open.failed", &[("why", "退出码 1")])
    );
    assert_ne!(wrapped, "退出码 1", "进程层的错没套上「文件窗口没起来」");
}

/// 🔴 **这条命令真的在命令面上。**
///
/// 判的是三样，少一样那条「用户点得开」的链就断在某一处：
/// ① Rust 侧有 `#[tauri::command]` 这个属性（否则它只是一个普通函数）；
/// ② `lib.rs` 的 `generate_handler!` 里注册了它（否则 `invoke` 直接 reject）；
/// ③ 前端包装层 `src/frontend/ui/ipc/commands.ts` 里有它，**键名与线上那个串都在**。
///
/// ⚠ 第 ③ 条与 `tests/frontend/ui/ipc/commands.vitest.ts` 的 `C04a` 同族但**不是重复**：
/// 那边判的是「两侧的集合相等」（整面），这边判的是「**这一条**在不在」（点名）。
/// 集合判据在两侧同时漏掉同一条时是静默的；点名的不会。
#[test]
fn this_command_is_wired_all_the_way_to_the_frontend_wrapper() {
    let name = "open_file_window";

    // ① 属性在盘上（needle 运行时拼，免得命中本文件自己的说明）。
    let mine = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/filewin/entry.rs"
    ));
    let attr = format!("#[{}::command]", "tauri");
    assert_eq!(
        mine.matches(attr.as_str()).count(),
        1,
        "`entry.rs` 的生产段里 `#[tauri::command]` 出现 {} 次（应当恰好 1 次）",
        mine.matches(attr.as_str()).count()
    );
    assert!(mine.contains(&format!("pub async fn {name}(")));

    // ② 注册表里有它。
    let lib =
        guard_core::production_code(include_str!("../../../../src/frontend/shell/src/lib.rs"));
    let at = lib
        .find("generate_handler![")
        .expect("`lib.rs` 里找不到 `generate_handler![` —— 抽取器坏了，本条此刻无效");
    let end = lib[at..]
        .find("])")
        .map(|i| at + i)
        .expect("`generate_handler![` 没有收尾 —— 抽取器坏了");
    let registry = &lib[at..end];
    assert_eq!(
        registry.matches(&format!("filewin::entry::{name}")).count(),
        1,
        "`generate_handler!` 里 `filewin::entry::{name}` 出现 {} 次（应当恰好 1 次）",
        registry.matches(&format!("filewin::entry::{name}")).count()
    );

    // ③ 前端包装层两侧都在。
    let wrapper = include_str!("../../../../src/frontend/ui/ipc/commands.ts");
    assert!(
        wrapper.contains(&format!("{name}: (")),
        "`src/frontend/ui/ipc/commands.ts` 里没有 `{name}` 这个键"
    );
    assert!(
        wrapper.contains(&format!("\"{name}\"")),
        "`src/frontend/ui/ipc/commands.ts` 里那个键没把 `{name}` 这个串传给 `invoke`"
    );

    // 反空真：这把尺子认得出「不在」。
    let bogus = "open_file_window_that_does_not_exist";
    assert!(!registry.contains(bogus) && !wrapper.contains(bogus));
}

/// 🔴 **界面上点得到它。** 这一条是「用户第一次真能打开那个窗口」的**落点判据**。
///
/// 上一刀（`sftp_chmod`）留下的欠账逐字是「**前端没有入口**」——
/// 命令到包装层为止，没有任何界面调它。⇒ 这一条就是不让同一件事再发生一次：
/// 包装层**之外**必须有至少一处**调用形状**（`.open_file_window(`）。
///
/// ⚠ 只认调用形状，注释／散文里提到命令名**不算**（照 `installface` 那一格的口径，
/// 否则这把尺子可以靠删一条注释变绿）。
/// ⚠ 买不到「那颗按钮长在用户找得到的地方」—— 那要人看。行为那一半的判据住
/// `tests/sftp/panel.vitest.ts`（真点一下那颗按钮、看它带着什么参数调下去）。
#[test]
fn some_ui_file_other_than_the_wrapper_actually_calls_it() {
    let root = crate::guard_support::repo_src_root();
    let mut sites: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for f in guard_core::files_by_extension(&root, "ts") {
        let path = root.join(&f);
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        scanned += 1;
        if f.replace('\\', "/").ends_with("ipc/commands.ts") {
            continue; // 包装层自己不算落点
        }
        if src.contains(".open_file_window(") {
            sites.push(f);
        }
    }
    // 反空真：扫描面没塌。
    assert!(
        scanned > 100,
        "只扫到 {scanned} 份 `.ts` —— 扫描面塌了，下面那一比会空真地红或绿"
    );
    assert!(
        !sites.is_empty(),
        "整个 `src/` 里没有一处**调用** `open_file_window` —— \
         那就又是一条「命令有了、前端没有入口」的欠账（补记那一形）"
    );
}

/// 🔴 **空路径那一支真的去问 home，而且排在列目录前面；monitor 这一侧一问都不问。**
///
/// 〔主会话 09-28 裁 3〕射程从 `entry.rs` 换到 `proc.rs::first_screen`：那两问进了窗口进程。
/// 窗口独立成包：`first_screen` 随窗口进程那一半住窗口包的 `proc.rs`；monitor 那一侧的 `proc.rs`（起进程）同 `entry.rs` 一起判零 SFTP。
/// 行为那一半（没给目录才问 · 问的顺序）住 `proc_tests::the_first_screen_asks_home_only_when_told_nothing`；
/// 本条钉结构：① `first_screen` 里 home 那一问排在列目录前面、用的是 `files-home` 那个常量；
/// ② `entry.rs` 生产段里**没有**问后端的写法（宿主句柄 · 两问的命令常量）—— 那正是待迁那一行删掉的理由。
///
/// ⚠ **它买不到那一跳是对的**，只买到它在、在前面、monitor 这一侧不在。别读宽。
#[test]
fn the_empty_path_branch_goes_through_the_one_home_resolver_before_listing() {
    let proc =
        guard_core::production_code(include_str!("../../../../src/frontend/filewin/src/proc.rs"));
    let spawner = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/filewin/proc.rs"
    ));
    let at_fn = guard_core::find_pinned(&proc, "pub async fn first_screen(")
        .expect("`first_screen` 不在 proc.rs 生产段里");
    let body = &proc[at_fn..];
    let body = &body[..body.find("\n}\n").expect("`first_screen` 的花括号没收口")];
    let at_home = body
        .find("super::source::CMD_HOME")
        .expect("`first_screen` 里没有问 home 那一问（`files-home` 那个常量）");
    let at_list = body
        .find("super::source::list_dir(")
        .expect("`first_screen` 里没有列目录那一下");
    assert!(
        at_home < at_list,
        "问 home 那一跳排在列目录后面 —— 那就是先拿空路径去列了一趟"
    );
    let entry = guard_core::production_code(include_str!(
        "../../../../src/frontend/shell/src/filewin/entry.rs"
    ));
    assert!(
        guard_core::find_pinned(&entry, "pub async fn open_file_window").is_ok(),
        "剥生产段把入口剥没了 —— 下面几条零命中此刻恒真"
    );
    for needle in [
        "InboundBackends",
        "CMD_HOME",
        "CMD_LS",
        "list_dir(",
        "source::ask(",
    ] {
        assert!(
            !entry.contains(needle),
            "`entry.rs` 生产段里又出现了 `{needle}` —— monitor 这一侧又替窗口问后端了"
        );
    }
    // 一处 SFTP 都不许有。针拼出来，免得命中本文件。
    let pool = format!("sftp_{}::", "pool");
    assert!(
        !entry.contains(pool.as_str())
            && !proc.contains(pool.as_str())
            && !spawner.contains(pool.as_str()),
        "开窗那条路又够到了 SFTP 那个池子"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴三者优先级 —— `P3` 的最后一格功能前置
// ════════════════════════════════════════════════════════════════════════
//
// 老面板 `open()` 有三种入口模式，窗口此前只覆盖两种。差的那一格是 **F54**：
// 远端**文件**路径 ⇒ 进它父目录 ＋ 高亮那一行。而那条路是**活的**
//（`src/frontend/ui/cards/index.ts::openRemoteFileInSftp` 里那个可点元素）
// ⇒ 先退役老面板 = 那颗「跳到这个文件」当场失效。

/// `path` 非空 ⇒ 它赢，**`reveal_file` 一起给也不管用**。
///
/// ⚠ 那个优先级是**照抄老面板**的（`initialDir > revealPath > home`），
/// 理由住 `plan_target` 头注：前端那几条调用点今天就是按它写的。
#[test]
fn an_explicit_directory_beats_a_reveal_request() {
    assert_eq!(
        plan_target("/srv/data", None).unwrap(),
        Target::Dir("/srv/data".into())
    );
    // 两个都给 ⇒ 目录赢。
    assert_eq!(
        plan_target("/srv/data", Some("/other/place/x.txt")).unwrap(),
        Target::Dir("/srv/data".into())
    );
}

/// 🔴 只给文件路径 ⇒ **进父目录 ＋ 高亮尾段**，而那两半都在这一侧算。
#[test]
fn a_file_path_becomes_its_parent_plus_the_name_to_highlight() {
    assert_eq!(
        plan_target("", Some("/srv/data/2026/报表.csv")).unwrap(),
        Target::Reveal {
            dir: "/srv/data/2026".into(),
            name: "报表.csv".into()
        }
    );
    // 周围空白不算内容。
    assert_eq!(
        plan_target("  ", Some("  /a/b.txt ")).unwrap(),
        Target::Reveal {
            dir: "/a".into(),
            name: "b.txt".into()
        }
    );
    // 根下那一层：父目录是 `/`。
    assert_eq!(
        plan_target("", Some("/top.txt")).unwrap(),
        Target::Reveal {
            dir: "/".into(),
            name: "top.txt".into()
        }
    );
}

/// 都没说 ⇒ 问 home（第七刀那一支，**只有它要 IO**）。
#[test]
fn nothing_given_falls_through_to_home() {
    assert_eq!(plan_target("", None).unwrap(), Target::Home);
    assert_eq!(plan_target("   ", Some("")).unwrap(), Target::Home);
    assert_eq!(plan_target("", Some("   ")).unwrap(), Target::Home);
}

/// 🔴 切不出名字 ⇒ **报错，不静默退回 home**。
///
/// 静默退回的后果具体：用户点了「跳到这个文件」，窗口开在他 home、
/// 什么都没高亮、**而且一句话都没有** —— 与「那个文件不见了」分不开。
#[test]
fn a_reveal_request_we_cannot_split_is_an_error_not_a_silent_home() {
    for bad in ["/", "//", "///"] {
        let e = plan_target("", Some(bad)).expect_err(&format!("`{bad}` 竟然切出了名字"));
        assert!(e.contains(bad), "报错没带上是哪条路径：{e}");
    }
    // 阴性对照：一条**正常**的文件路径不会走这一支
    //（少了这一半，上面那一比可以靠「什么都报错」全绿）。
    assert!(plan_target("", Some("/a/b.txt")).is_ok());
}

/// 开窗种子里那台的名字 ＝ `RemoteConfig::origin_label`（`label` 为空时回退到 `host`）——
/// 原住 `shell_tests::every_source_has_a_non_empty_label` 的后一半：窗口只拿名字之后，「名字怎么从配置来」是开窗入口这一侧的事。
#[test]
fn the_seed_names_the_machine_by_its_origin_label() {
    assert_eq!(synth_cfg().origin_label(), "synthetic-origin");
    let mut anon = synth_cfg();
    anon.label.clear();
    assert_eq!(anon.origin_label(), "example.invalid");
}
