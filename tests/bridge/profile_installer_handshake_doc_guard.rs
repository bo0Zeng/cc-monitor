const IPC_DOC: &str = include_str!("../../src/doc/IPC-PROTOCOL.md");
const BIND_RS_RAW: &str = include_str!("../../src/bridge/src/bind.rs");
const ARCH_DOC: &str = include_str!("../../src/doc/ARCHITECTURE.md");

/// ★ **判据一律看剥掉注释之后的代码**。
///
/// D 审计把这三条护栏**全部攻破**，手法都一样：把值改坏，再在旁边加一行
/// 「沿革：以前是 …3000…」的注释 —— 护栏读的是整份原文，注释就把它喂饱了。
/// 实测 deadline 3000→800、轮询 30→250、debouncer 50→500、
/// 重试 12×50→3×10（旧模板用户唯一的活路缩成 30ms），**四条全绿**。
///
/// daemon 那边的护栏早就走 `guard_support::production_code` 剥注释，
/// 那个模块的注释里逐字写着「不剥的话守卫会被解释它自己的那段散文喂饱」。
/// **同一个坑，隔一个 crate 又踩了一遍。**
fn strip_comments(src: &str, line_comment: &str) -> String {
    src.lines()
        .map(|l| match l.find(line_comment) {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn bind_rs() -> String {
    strip_comments(BIND_RS_RAW, "//")
}

fn tpl() -> String {
    strip_comments(super::CC_TEMPLATE, "#")
}

/// v2 竞态修复的核心：**先设标题、再写 await 文件**。
///
/// 反过来的话，monitor 的 notify 在文件落地瞬间就 EnumWindows 找 marker，
/// **扫得越快越容易找不到窗口** ⇒ 删 await 走失败路径 ⇒ 绑定成败全凭时序运气。
#[test]
fn ps_template_sets_the_window_title_before_writing_the_await_file() {
    let t = &tpl();
    let title = t
        .find("$Host.UI.RawUI.WindowTitle = $marker")
        .expect("模板里找不到设标题那行 —— 抽取坏了还是握手改了？");
    let write = t
        .find("WriteAllText($awaitFile")
        .expect("模板里找不到写 await 文件那行 —— 抽取坏了还是握手改了？");
    assert!(
        title < write,
        "cc.ps1.tpl 把顺序换回了「先写文件、后设标题」—— 那正是 v2.21 \
             『每个新 shell 首次 cc 固定烧满超时』的成因（src/doc/IPC-PROTOCOL.md \
             § 跨进程握手时序图 有整段说明）。设标题@{title} 写文件@{write}"
    );
}

/// 模板里的 deadline / 轮询步长必须在协议文档里出现（防「改了代码忘了改图」）。
#[test]
fn handshake_timings_in_the_template_appear_in_the_protocol_doc() {
    let t = &tpl();
    let deadline = between(t, "AddMilliseconds(", ")").expect("模板里没有 deadline");
    let poll = between(t, "Start-Sleep -Milliseconds ", "\n").expect("模板里没有轮询步长");

    // 这两个数曾双双漂移：文档停在 800ms，实现早已 3000ms。
    assert!(
        IPC_DOC.contains(&format!("{deadline}ms")),
        "PS 握手 deadline 是 {deadline}ms，但 src/doc/IPC-PROTOCOL.md 里没有 `{deadline}ms` \
             —— 文档还停在旧数字上（上一次是 800ms）"
    );
    assert!(
        IPC_DOC.contains(&format!("{poll}ms")),
        "PS 轮询步长是 {poll}ms，但 src/doc/IPC-PROTOCOL.md 里没有 `{poll}ms`"
    );
}

/// monitor 侧同理：debouncer 窗口 + 「找不到窗口就重试」的总时长。
///
/// 重试那一条是**旧模板用户唯一的活路**（老 profile 不会自动更新），
/// 图上却整个没画 —— 少画它等于把兼容性保障说成不存在。
/// ★ **每一份描述这个握手的文档**都必须写当前顺序，不只 IPC-PROTOCOL.md。
///
/// U6a 实测：同一个顺序被写在**五处** —— `cc.ps1.tpl`（真相）· `bind.rs` 模块头 ·
/// `src/doc/IPC-PROTOCOL.md` 时序图 · `src/doc/ARCHITECTURE.md` · `cc_integration.ts` 的 UI 文案。
/// v2 反转顺序时**只改了真相那一处**，另外四处全停在旧顺序上。
/// 只钉一份文档，剩下几份照样把人教回旧写法。
#[test]
fn every_doc_that_describes_the_handshake_states_the_current_order() {
    // 判据：一段文字若同时提到 `ps-await` 与 `WindowTitle`，就算「在描述这个握手」，
    // 那它必须把 `WindowTitle` 写在 `ps-await` 前面（= v2 的真实顺序）。
    // 必须从**描述握手那一节**起算，不能拿全文首次出现比 —— IPC-PROTOCOL.md 有一整节
    // 就叫 `ps-await/<PID>.json`，排在时序图**之前**，全文首现比法会恒红（实测踩到）。
    for (name, doc, anchor) in [
        ("src/doc/IPC-PROTOCOL.md", IPC_DOC, "## 跨进程握手时序图"),
        ("src/doc/ARCHITECTURE.md", ARCH_DOC, "### marker 握手"),
    ] {
        let at = doc
            .find(anchor)
            .unwrap_or_else(|| panic!("{name} 里找不到锚点 {anchor:?} —— 文档被大改了？"));
        let doc = &doc[at..];
        let (Some(title), Some(await_file)) = (doc.find("WindowTitle"), doc.find("ps-await"))
        else {
            panic!("{name} 的握手节里找不到那两个关键词 —— 抽取坏了还是文档被大改了？");
        };
        assert!(
            title < await_file,
            "{name} 把握手顺序写成了「先写 ps-await、后设 WindowTitle」—— 那是 v2 之前的旧顺序，\n\
                 照它实现会复刻 v2.21『每个新 shell 首次 cc 固定烧满超时』。\n\
                 真相源是 src/bridge/scripts/cc.ps1.tpl（先设标题、后写文件）。"
        );
    }
}

/// ★ 把这四个数**直接钉死**。
///
/// # 为什么「数字出现在文档里」不够
///
/// D 审计实测：把 deadline 从 3000 退回 **800**，上面那条护栏**不红** ——
/// 因为 `src/doc/IPC-PROTOCOL.md` 自己的沿革括号里就写着「v2 之前 deadline 是 800ms」。
/// **文档的 changelog 把旧值供着，判据就被它喂饱了。**
///
/// 那条护栏的立项理由是「文档停在 800、实现早已 3000」—— 它管的是**文档滞后**。
/// 反方向（**实现退回旧值**）得靠这条钉死。两条一起才闭合。
///
/// # 改这些数怎么办
///
/// 它们是**协议的一部分**（PS 与 monitor 两侧必须对齐，且旧模板用户靠重试兜底）。
/// 要改就三处一起改：实现 · 本 pin · `src/doc/IPC-PROTOCOL.md` 的时序图。
/// 本 pin 红了不是"更新一下数字"，是提醒你**这是一次协议变更**。
#[test]
fn handshake_timings_match_their_pinned_values() {
    let t = tpl();
    let bind = bind_rs();
    let g = |src: &str, a: &str, b: &str| -> u32 {
        between(src, a, b)
            .unwrap_or_else(|| panic!("抽不到 {a:?} —— 抽取坏了，本断言在空转"))
            .parse()
            .unwrap_or_else(|e| panic!("{a:?} 抽到的不是整数：{e}"))
    };
    assert_eq!(
        g(&t, "AddMilliseconds(", ")"),
        3000,
        "PS 握手 deadline 变了。v2 从 800 抬到 3000 是为了覆盖 monitor 冷启动 ——\n\
             退回去会让「monitor 没在跑时第一次 cc」重新烧满超时。"
    );
    assert_eq!(
        g(&t, "Start-Sleep -Milliseconds ", "\n"),
        30,
        "PS 轮询步长变了"
    );
    assert_eq!(
        g(&bind, "new_debouncer(Duration::from_millis(", ")"),
        50,
        "notify debouncer 变了"
    );
    let n = g(&bind, "for _ in 0..", " {");
    let step = g(
        &bind,
        "std::thread::sleep(std::time::Duration::from_millis(",
        ")",
    );
    assert_eq!(
        (n, step),
        (12, 50),
        "找不到窗口时的重试节奏变了。**那是旧模板用户唯一的活路** ——\n\
             老 profile 不会自动更新，它们靠这 600ms 兜住「标题还没设上」的窗口。\n\
             D 审计把它缩成 3×10ms=30ms，四条护栏当时全绿。"
    );
}

#[test]
fn monitor_side_timings_appear_in_the_protocol_doc() {
    let bind = bind_rs();
    let debounce =
        between(&bind, "new_debouncer(Duration::from_millis(", ")").expect("找不到 debouncer");
    assert!(
        IPC_DOC.contains(&format!("{debounce}ms")),
        "notify debouncer 是 {debounce}ms，src/doc/IPC-PROTOCOL.md 里没有 —— \
             图上曾长期写着 100ms"
    );

    // 重试：`for _ in 0..12 { sleep(50ms) }` ⇒ 总 600ms。两个数都得对得上。
    let n: u32 = between(&bind, "for _ in 0..", " {")
        .expect("找不到重试次数")
        .parse()
        .expect("重试次数不是整数");
    let step: u32 = between(
        &bind,
        "std::thread::sleep(std::time::Duration::from_millis(",
        ")",
    )
    .expect("找不到重试步长")
    .parse()
    .expect("重试步长不是整数");
    let total = n * step;
    assert!(
        IPC_DOC.contains(&format!("{total}ms")) && IPC_DOC.contains(&format!("{n} × {step}")),
        "monitor 找不到窗口时重试 {n} × {step}ms = {total}ms，\
             src/doc/IPC-PROTOCOL.md 必须同时写出总时长 `{total}ms` 和拆分 `{n} × {step}`（当前缺其一）"
    );
}

/// 抽第一处 `a`…`b` 之间的内容。抽不到返回 None —— 调用方一律 expect，
/// 免得夹具悄悄退化成空转（本仓有先例：抽取器改坏后抽到 0 个、断言全绿）。
fn between<'a>(hay: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let s = hay.find(a)? + a.len();
    let e = hay[s..].find(b)? + s;
    Some(hay[s..e].trim())
}
