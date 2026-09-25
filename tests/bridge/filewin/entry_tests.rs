//! # 要求住址：`设计/60 §2.3`（开窗前那一屏：列不出来就报错、不开窗）＋ `设计/60 §2.5`（入口与三种落点）
//!
//! 核原文：`设计/60 §2.3` 逐字「开窗前那一屏：monitor 侧经宿主注入的同一个句柄问 `files-home`（不给落点时）与 `files-ls`；
//! 列不出来就带原文报错」；`设计/60 §2.5` 逐字「三种落点与 `filewin/entry.rs::plan_target` 三支一一对应」·
//! 「判据：入口人群两向相等、`open_file_window` 在包装层外恰好一处」—— 本族判的正是先问后开、三支落点、命令真接到前端。
//! 〔乙 · 迁移底账〕老面板书签搬家那两格（`entry.rs::carry_legacy`）钉的是一次性搬家，家在 `设计/99 §4.1` 的 FW34 行，
//! 搬完连同判据一起删，不该升格成条。〔JA1 点址 2026-09-24〕

use super::*;

fn synth_cfg() -> RemoteConfig {
    RemoteConfig {
        host: "example.invalid".into(), // RFC 2606 保留 ⇒ DNS 解不出来
        label: "synthetic-origin".into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/nonexistent/cc-monitor-backend".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// 空路径 ⇒ **去问远端 home**；问不到就报错，**而且一个窗口都不开**。
///
/// # 🔴〔第七刀 2026-09-21〕这一条改过措辞，性质没松
///
/// 上一版它叫〔散文墓碑〕`an_empty_path_is_refused_without_opening_a_window`，断的是
/// 报错里含「路径是空的」。**那钉的是机制，不是性质** —— 真性质是
/// 「**说不出要看哪儿就别开一个空窗**」，而「空路径一律回错」只是当时唯一可选的实现
/// （见 `entry.rs` 那一节：在入口里猜一个默认值 vs 去问那个说得上话的）。
///
/// ⇒ 现在空路径的意思是「开在远端 home」，而本条断的换成**更强**的一件：
/// 报错里要出现 `realpath` ——**那证明我们真的去问了**。
/// 只断「报错非空」的话，一个把空路径原样丢下去列的实现也能全绿。
///
/// 🔴 「一个窗口都不开」照旧是这条的第二半，也是更要紧的那一半：
/// 早退的实现很容易先 `spawn` 了线程再检查参数，那样用户会看到一个空窗 ＋ 一条报错。
///
/// ⚠ 本条走的是**失败路径**（`host` 是 `.invalid`，DNS 保留域 ⇒ 解析就失败了，
/// 一个 TCP 包都没出去）。「问得到 home 时它真的开在那儿」本机买不到 —— 要真远端。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_path_asks_the_remote_for_home_and_opens_nothing_when_it_cannot() {
    let before = crate::filewin::shell::open_requested();
    let e = open_file_window(synth_cfg(), "   ".into(), None, None)
        .await
        .expect_err("问不到 home 竟然过了");
    // 🔴〔订正 2026-09-21〕这里原先断的是「报错里含 `realpath`」，**那买不到**：
    //    `.invalid` 上失败发生在**连接**阶段，`canonicalize` 一次都没跑到
    //    ⇒ `sftp_realpath` 那句 `realpath 失败:` 不会出现，而问 home 与列目录
    //      在这台机器上回的是**同一句**池错误（现打逐字「所有地址连接失败: …」）。
    //    ⇒ 「是哪一跳失败的」在失败路径上**文本分不开**。如实降级成断得住的两件，
    //      「真的去问了」那一件交给下面那条源码代理。
    assert!(
        !e.trim().is_empty(),
        "报错是空串 —— webview 那侧会弹一个没有内容的失败提示"
    );
    // 🔴〔F7a · 第三波 09-24〕**「真的去问了」这一件又断得住了**：问的是后端，
    //    判据进程里没有那台机器的控制通道 ⇒ 那一跳就地失败，而失败那句话带着**命令名**
    //    （`source::said` 逐字「`files-home` 没走通…」）⇒ 是哪一跳失败的，文本分得开了。
    assert!(
        e.contains(crate::filewin::source::CMD_HOME),
        "空路径的报错里没有 `files-home` —— 那就不是在问 home（原文：{e}）"
    );
    assert_eq!(
        crate::filewin::shell::open_requested(),
        before,
        "说不出要看哪儿，却已经请求开窗了 —— 那就是个空窗"
    );
}

/// 🔴 **列不出来就别开窗** —— 而且要把下层那句原文带回去。
///
/// 〔F2 · 2026-09-24〕列那一趟改成问后端之后，判据进程里**通道口没起来**
/// （`chan::host::start` 只在 app 起来时调）⇒ 它在「够不着后端」那一步就回错 ——
/// 同一个性质（列不出来就别开窗）的另一个失败点，一个包都不出去。
/// ⚠ 「后端说列不出来 ⇒ 不开窗」那一形要一个起着的通道口，本条不买；
/// 那句话本身的翻译与窗口里那一次同一个函数（`source::said`），由 `source_tests` 那几条判。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_directory_we_cannot_list_is_an_error_not_a_blank_window() {
    let before = crate::filewin::shell::open_requested();
    let e = open_file_window(synth_cfg(), "/srv/whatever".into(), None, None)
        .await
        .expect_err("连不上的远端竟然列出了目录");
    assert!(
        !e.trim().is_empty(),
        "报错是空串 —— webview 那侧会弹一个没有内容的失败提示"
    );
    // 〔F7a〕阴性对照：给了路径 ⇒ **不问 home**（问了就是多一趟往返，而且它会先失败、盖掉真原因）。
    assert!(
        !e.contains(crate::filewin::source::CMD_HOME),
        "给了路径却去问了 home：{e}"
    );
    assert_eq!(
        crate::filewin::shell::open_requested(),
        before,
        "目录列不出来却还是开了窗 —— 那就是个空窗，用户不知道发生了什么"
    );
}

/// 🔴 **这条命令真的在命令面上。**
///
/// 判的是三样，少一样那条「用户点得开」的链就断在某一处：
/// ① Rust 侧有 `#[tauri::command]` 这个属性（否则它只是一个普通函数）；
/// ② `lib.rs` 的 `generate_handler!` 里注册了它（否则 `invoke` 直接 reject）；
/// ③ 前端包装层 `src/ipc/commands.ts` 里有它，**键名与线上那个串都在**。
///
/// ⚠ 第 ③ 条与 `tests/ipc/commands.vitest.ts` 的 `C04a` 同族但**不是重复**：
/// 那边判的是「两侧的集合相等」（整面），这边判的是「**这一条**在不在」（点名）。
/// 集合判据在两侧同时漏掉同一条时是静默的；点名的不会。
#[test]
fn this_command_is_wired_all_the_way_to_the_frontend_wrapper() {
    let name = "open_file_window";

    // ① 属性在盘上（needle 运行时拼，免得命中本文件自己的说明）。
    let mine =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/entry.rs"));
    let attr = format!("#[{}::command]", "tauri");
    assert_eq!(
        mine.matches(attr.as_str()).count(),
        1,
        "`entry.rs` 的生产段里 `#[tauri::command]` 出现 {} 次（应当恰好 1 次）",
        mine.matches(attr.as_str()).count()
    );
    assert!(mine.contains(&format!("pub async fn {name}(")));

    // ② 注册表里有它。
    let lib = guard_core::production_code(include_str!("../../../src/bridge/src/lib.rs"));
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
    let wrapper = include_str!("../../../src/ipc/commands.ts");
    assert!(
        wrapper.contains(&format!("{name}: (")),
        "`src/ipc/commands.ts` 里没有 `{name}` 这个键"
    );
    assert!(
        wrapper.contains(&format!("\"{name}\"")),
        "`src/ipc/commands.ts` 里那个键没把 `{name}` 这个串传给 `invoke`"
    );

    // 反空真：这把尺子认得出「不在」。
    let bogus = "open_file_window_that_does_not_exist";
    assert!(!registry.contains(bogus) && !wrapper.contains(bogus));
}

/// 🔴 **界面上点得到它。** 这一条是「用户第一次真能打开那个窗口」的**落点判据**。
///
/// 上一刀（`设计/60 §5.4c` 的 `sftp_chmod`）留下的欠账逐字是「**前端没有入口**」——
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
         那就又是一条「命令有了、前端没有入口」的欠账（`设计/60 §5.4c` 补记那一形）"
    );
}

/// 🔴 **空路径那一支真的走 `ask_home`（问后端 `files-home`），而且排在列目录前面。**
///
/// # 为什么要这条源码代理
///
/// 上面那条行为判据**买不到**「是哪一跳失败的」：在 `.invalid` 这台合成远端上，
/// 问 home 与列目录回的是同一句池错误（连接阶段就失败了）⇒ 文本分不开。
/// 而「问得到 home 时它真的开在那儿」要真远端，本机永远量不到。
///
/// ⇒ 剩下能确定地钉住的是**结构**：那一跳在不在、在不在前面。
/// 同族先例：`transfer_tests::the_real_adapters_speak_only_through_the_channel`
/// （那条头注逐字「判源码是代理」）。
///
/// ⚠ **它买不到那一跳是对的**，只买到它在。别读宽。
#[test]
fn the_empty_path_branch_goes_through_the_one_home_resolver_before_listing() {
    let prod =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/entry.rs"));
    assert_eq!(
        prod.matches("ask_home(&").count(),
        1,
        "`entry.rs` 生产段里 `ask_home(&` 不是恰好一处 —— \
         少了就是空路径又被原样丢下去，多了就是这件事长出了第二个住址"
    );
    let at_home = prod
        .find("ask_home(&")
        .expect("上一比已经保证它在，这里拿不到位置说明抽取器坏了");
    // 〔F2 · 2026-09-24〕列那一趟从池子（SFTP）换成了问后端（`list_first_screen`）。
    let at_list = prod
        .find("list_first_screen(")
        .expect("`list_first_screen(` 不在生产段里 —— 那条「先列一趟再开窗」的纪律没了");
    assert!(
        at_home < at_list,
        "问 home 那一跳排在列目录后面 —— 那就是先拿空路径去列了一趟"
    );
    // 反空真：这把尺子认得出「不在」。
    assert!(!prod.contains("ask_home_that_does_not_exist"));
    // 〔F7a · 第三波 09-24〕那一跳问的是后端：`entry.rs` 生产段里一处 SFTP 都不许有
    //   （monitor 这一侧开窗此前唯一碰 SFTP 的就是问 home 那一下）。针拼出来，免得命中本文件。
    let pool = format!("sftp_{}::", "pool");
    assert!(
        !prod.contains(pool.as_str()),
        "`entry.rs` 又够到了 SFTP 那个池子 —— 开窗前那两问都该走后端"
    );
    assert!(
        prod.contains("CMD_HOME"),
        "问 home 那一跳没用 `files-home` 那个常量"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 🔴〔第十刀 2026-09-22〕三者优先级 —— `P3` 的最后一格功能前置
// ════════════════════════════════════════════════════════════════════════
//
// 老面板 `open()` 有三种入口模式，窗口此前只覆盖两种。差的那一格是 **F54**：
// 远端**文件**路径 ⇒ 进它父目录 ＋ 高亮那一行。而那条路是**活的**
//（`src/cards/index.ts::openRemoteFileInSftp` 里那个可点元素）
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

// ═══════════════ 〔FW34〕〔待退役〕老面板书签的搬家那一跳 ═══════════════

/// 🔴 搬家落到书签**唯一那个写口**上：盘上那一份逐格相等；搬两次与搬一次逐字节相同；
/// 数据目录解不出来 ⇒ 报错且那句话说清旧书签还在原处（webview 那侧据此不删旧键）。
#[test]
fn carrying_the_old_panels_bookmarks_lands_in_the_one_bookmark_file() {
    let dir = std::env::temp_dir().join(format!(
        "ccm-fw34-carry-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    let file = crate::filewin::bookmarks::file_in(&dir);
    let legacy: crate::filewin::bookmarks::Book = [
        (
            "aya".to_string(),
            vec!["/home/zbl/".to_string(), "/srv".to_string()],
        ),
        ("gpd".to_string(), vec!["/data".to_string()]),
    ]
    .into_iter()
    .collect();
    assert_eq!(carry_legacy(Some(&file), &legacy).unwrap(), 3);
    let first = std::fs::read(&file).unwrap();
    let want: crate::filewin::bookmarks::Book = [
        (
            "aya".to_string(),
            vec!["/home/zbl".to_string(), "/srv".to_string()],
        ),
        ("gpd".to_string(), vec!["/data".to_string()]),
    ]
    .into_iter()
    .collect();
    assert_eq!(crate::filewin::bookmarks::read_book(&file).unwrap(), want);
    assert_eq!(carry_legacy(Some(&file), &legacy).unwrap(), 0);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        first,
        "搬两次与搬一次盘上逐字节相同"
    );
    let e = carry_legacy(None, &legacy).unwrap_err();
    assert!(e.contains("旧书签还留在原处"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 🔴 入口那条命令里**先搬、再列、再开窗**；搬不成就在开窗之前回错（`?` 就在那一行上）。
#[test]
fn the_entry_command_carries_before_it_lists_or_opens() {
    let prod =
        guard_core::production_code(include_str!("../../../src/bridge/src/filewin/entry.rs"));
    let carry = guard_core::find_pinned(&prod, "carry_legacy(bookmarks.as_deref(), &legacy)?;")
        .expect("入口里没有搬家那一下（或者它不再在失败时回错）");
    let list = guard_core::find_pinned(&prod, "list_first_screen(&source, &path).await?")
        .expect("入口里找不到列第一屏那一下 —— 锚漂了");
    let open = guard_core::find_pinned(&prod, "open_in_new_process(&OpenRequest {")
        .expect("入口里找不到开窗那一下 —— 锚漂了");
    assert!(carry < list && list < open, "顺序不是「搬 → 列 → 开」");
}
