const BIND_RS_RAW: &str = include_str!("../../../src/frontend/shell/src/bind.rs");
/// 模板的渲染住后端（`src/backend/assets/aliases/block.rs`）；握手那一半的对账在 monitor（`bind.rs` 在这边）⇒ 直接读模板原文。
const CC_TEMPLATE: &str = include_str!("../../../src/shared/cc.ps1.tpl");

/// ★ **判据一律看剥掉注释之后的代码**。
///
/// D 审计把这三条护栏**全部攻破**，手法都一样：把值改坏，再在旁边加一行
/// 「沿革：以前是 …3000…」的注释 —— 护栏读的是整份原文，注释就把它喂饱了。
/// 实测 deadline 3000→800、轮询 30→250、debouncer 50→500、
/// 重试 12×50→3×10（旧模板用户唯一的活路缩成 30ms），**四条全绿**。
///
/// backend 那边的护栏早就走 `guard_support::production_code` 剥注释，
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
    strip_comments(CC_TEMPLATE, "#")
}

/// v2 竞态修复的核心：**先设标题、再写 await 文件**。
///
/// 反过来的话，monitor 的 notify 在文件落地瞬间就 EnumWindows 找 marker，
/// **扫得越快越容易找不到窗口** ⇒ 删 await 走失败路径 ⇒ 绑定成败全凭时序运气。
#[test]
fn ps_template_sets_the_window_title_before_writing_the_await_file() {
    let t = &tpl();
    let title = t
        .find("[System.Console]::Title = $marker")
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
/// 要改就两处一起改：实现 · 本 pin（文档的时序图不写这些数，只指向实现）。
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
        g(&t, "[System.Threading.Thread]::Sleep(", ")"),
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

/// 抽第一处 `a`…`b` 之间的内容。抽不到返回 None —— 调用方一律 expect，
/// 免得夹具悄悄退化成空转（本仓有先例：抽取器改坏后抽到 0 个、断言全绿）。
fn between<'a>(hay: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let s = hay.find(a)? + a.len();
    let e = hay[s..].find(b)? + s;
    Some(hay[s..e].trim())
}
