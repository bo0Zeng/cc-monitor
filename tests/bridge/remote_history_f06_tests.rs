use super::*;

/// ★ **超限必须说话，而且要说清「少了什么」**〔audit-0805 F06，定框 E4/E5〕。
///
/// 此前是 `take(MAX)` + `if n == 0 { break; }` —— 到限与正常 EOF **完全同形**，
/// 前端拿到一份「看起来完整」的历史而后面的内容无声消失。
/// 而同一份数据走 daemon 的 `--fork-session` 会**硬报错**（`common/fs.rs`）：
/// **同一份数据走两条路得到两个答案**，正是 E5 要消灭的。
#[test]
fn the_truncation_message_says_what_is_missing_and_where_it_still_is() {
    let m = session_truncated_message(MAX_SESSION_BYTES + 1, 92_967);
    assert!(m.contains("上限"), "要说清是撞了上限：{m}");
    assert!(
        m.contains("92967") || m.contains("92_967"),
        "★ 要报出**已显示多少行** —— 用户得知道自己看到的是哪一截：{m}"
    );
    assert!(
        m.contains("没有显示"),
        "★ 要明说后面的内容没显示 —— 不说这句就等于还是在静默：{m}"
    );
    assert!(
        m.contains("仍在远端"),
        "★ 要告诉用户完整历史还在（这条与丢帧不同：数据没丢，是没读完）：{m}"
    );
}

/// ★ **上限检查还接在路径上，不只是「登记过」**〔audit-0805 §5 1g，08-06 补〕。
///
/// # 先量后写：1g 那句话对，但它给的理由只挡住一半
///
/// §5 1g 逐字写着「把检测拆成 `if false` 全仓没有任何判据会红」。08-06 真做了这次变异
/// （把下面那个条件换成 `if false {`）：monitor 全量 **passed=989 failed=0** ——
/// 那句话在**行为层**成立。
///
/// 但它给的**理由**是「住在吃真 SSH 流的 async 函数里，红线内造不出 >256 MiB 的远端流」。
/// 这次变异与流有多大**毫无关系** —— 那个理由挡住的只是「撞到上限时运行起来真的会 Err」，
/// 挡不住「判断被整个摘掉」。**阻塞四问 ①「挡住的是整件还是一部分」又一次命中。**
///
/// 顺带说清另一件容易误读的事：`byte_cap_registry` 里**登记了**这处上限，
/// 但它管的是「上限有没有被登记 + 语义有没有声明」，**不是「检查会不会触发」** ——
/// 「有个登记表覆盖着」不等于「这条路上有人守着」。
///
/// # 钉什么、不钉什么
///
/// 钉**源码形态的三段链**，每一段单独被摘掉，静默截断都会回来：
///
/// | 段 | 摘掉它会怎样 |
/// |---|---|
/// | `take(…+ 1)` 里的 **`+ 1`** | 到限时 `read_line` 返回 0，与正常 EOF **完全同形** ⇒ 无声截断 |
/// | 那个 `read_bytes >` 条件 | 判断没了，读到 cap 就当读完了 |
/// | 那一支的 `return Err(…)` | 换成 `break` 就是「读完了」，前端拿到一份看起来完整的历史 |
///
/// **不钉**「撞到上限时运行起来真的会 Err」—— 那要把读循环从这个吃真 SSH 流的
/// async fn 里抽出来（连 `tauri::ipc::Channel` 那个出口一起抽象）。本轮不做，
/// §5 1g **保留**，但范围缩小到行为层那一半。
///
/// # 「判据不会读到自己」今天由什么保障〔搬树 2026-09-18 订正〕
///
/// **原文（已作废）**：「本判据的 needle 在未剥测试段的源码里有两处：生产一处 ＋ 本测试的
/// 字面量一处。下面第一条断言的就是『剥完之后它变少了』。」——那条对照组的前提是
/// 判据与被测代码同住一份文件，剖分之后不成立（`16 §4.1` 预言的恒等变换）。
/// 今天的保障是**结构性**的：本条住 `tests/bridge/`，语料住 `src/bridge/src/` ——
/// 两份文件物理不同，读到自己**不可能发生**；而「有人把测试搬回去」那一形
/// 由体内那条 `assert_no_test_code` 钉着。
#[test]
fn the_cap_check_is_still_wired_not_just_declared() {
    let raw = include_str!("../../src/bridge/src/remote_history.rs");
    let prod = guard_core::production_source(raw);

    // 🔴 〔搬树 2026-09-18 · `设计/99 §2.5 P9` / `16 §4.1`〕**对照组退役，换成它今天真正的那道保障。**
    //
    // 上一版那条对照组的前提是「**判据与被测代码同住一份文件**」：needle 在未剥的源码里
    // 有两处（生产一处 ＋ 本测试的字面量一处），所以「剥完之后变少 / 归零」能证明
    // `production_source` 真在起作用。剖分之后那个前提**不成立了** ——
    // 本条住 `tests/bridge/`，语料是 `src/bridge/src/remote_history.rs`，
    // 两份文件物理不同 ⇒ `raw` 与 `prod` 逐字节相同，对照组**恒假**。
    // 这正是 `16 §4.1` 早就预言的那一格：`production_source` 在 `src/` 上是恒等变换。
    //
    // ⇒ 不是删掉一条检查，是把它换成**今天真的成立、而且仍然会红**的那一条：
    //   ① 语料真的读进来了（空串会让下面几条一起「找不到」—— 假红与假绿同形）；
    //   ② 被测那份文件里**一个 `#[test]` 都没有** —— 那就是「判据不会读到自己」
    //      在剖分之后的**结构性**保障。有人把测试搬回 `remote_history.rs`，这一格当场红。
    assert!(
        raw.len() > 5_000,
        "只读到 {} 字节的 `remote_history.rs` —— 本条在空转",
        raw.len()
    );
    guard_core::assert_no_test_code("remote_history.rs", raw);

    const COND: &str = "if read_bytes > MAX_SESSION_BYTES {";

    // ① `+ 1`：它是「到限」与「正好读完」唯一的区分手段。
    guard_core::pin_line(
        &prod,
        "let mut reader = BufReader::new(stream.take(MAX_SESSION_BYTES + 1));",
    )
    .expect(
        "★ `take(MAX_SESSION_BYTES + 1)` 这一行不在了（或写法变了）。\n\
             只 take(MAX) 的话，到限时 read_line 返回 0，与正常 EOF **完全同形** ——\n\
             下面两条即使都在，也再没有任何东西能分辨「读完了」和「读到上限」。",
    );

    // ② 条件本身：这一条正是 08-06 那次 `if false` 变异摘掉的东西。
    let at = guard_core::pin_line(&prod, COND).expect(
        "★ 上限判断不在生产段里了。08-06 实测：把它换成 `if false {`，\n\
             monitor 全量 989 条**一条都不会红** —— 本条就是为那个洞补的。",
    );

    // ③ 那一支必须**报错**，不能是 break/continue：后者等于「读完了」。
    let body = prod
        .lines()
        .skip(at + 1)
        .find(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with("//")
        })
        .unwrap_or("");
    assert!(
        body.trim()
            .starts_with("return Err(session_truncated_message("),
        "★ 撞上限那一支的第一句是 `{}`，不是 `return Err(session_truncated_message(…))`。\n\
             换成 break/continue 就是「读完了」：前端拿到一份**看起来完整**的历史，\n\
             而同一份数据走 daemon 的 `--fork-session` 会硬报错 —— 同一份数据两条路两个答案，\n\
             正是定框 E5 要消灭的那种不一致。",
        body.trim()
    );
}
