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

/// 空路径 ⇒ 立刻报错，**而且一个窗口都不开**。
///
/// 🔴 「一个窗口都不开」是这条的第二半，也是更要紧的那一半：
/// 早退的实现很容易先 `spawn` 了线程再检查参数，那样用户会看到一个空窗 ＋ 一条报错。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_path_is_refused_without_opening_a_window() {
    let before = crate::filewin::shell::open_requested();
    let e = open_file_window(synth_cfg(), "   ".into())
        .await
        .expect_err("空路径竟然过了");
    assert!(e.contains("路径是空的"), "报错没说清是什么问题：{e}");
    assert_eq!(
        crate::filewin::shell::open_requested(),
        before,
        "参数不合法却已经请求开窗了"
    );
}

/// 🔴 **列不出来就别开窗** —— 而且要把下层那句原文带回去。
///
/// ⚠ 这条走的是真 `list_remote` ⇒ 真 `sftp_pool`，而 `host` 是 `.invalid`
/// （DNS 保留域）⇒ 它**连不上**，那正是本条要的那一形：失败路径。
/// **不是**「起了一条真连接」—— 解析就失败了，一个 TCP 包都没出去。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_directory_we_cannot_list_is_an_error_not_a_blank_window() {
    let before = crate::filewin::shell::open_requested();
    let e = open_file_window(synth_cfg(), "/srv/whatever".into())
        .await
        .expect_err("连不上的远端竟然列出了目录");
    assert!(
        !e.trim().is_empty(),
        "报错是空串 —— webview 那侧会弹一个没有内容的失败提示"
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
    let mine = guard_core::production_code(include_str!(
        "../../../src/bridge/src/filewin/entry.rs"
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
