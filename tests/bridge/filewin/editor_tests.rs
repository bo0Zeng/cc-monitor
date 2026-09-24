use super::*;

fn row(name: &str, is_dir: bool, lossy: bool, size: u64) -> Row {
    Row {
        name: name.to_string(),
        path: format!("/srv/data/{name}"),
        is_dir,
        size,
        lossy_name: lossy,
    }
}

// ═══════════════════════════════════════════════════════════════════
// 「超了怎么办」—— `设计/60 §5.4b` 指名留给这一刀的第二问
// ═══════════════════════════════════════════════════════════════════

/// 普通小文本 ⇒ 改得了，而且**一句话都不用说**。
#[test]
fn a_small_text_file_is_editable_with_nothing_to_explain() {
    let r = row("a.txt", false, false, 1024);
    assert_eq!(why_not_editable(&r), None);
    assert!(is_editable(&r));
}

/// 🔴 **太大那一档在本地就判得出来，而且那句话里带着两个真数。**
///
/// # 它买到什么（这一条是这一刀的正题）
///
/// 池子那条命令回的 `Option<String>` 把三件事压成一件（太大 / 含 NUL / 非 UTF-8）
/// ⇒ 谁拿到 `None` 都说不出为什么，老面板的做法是把那一行灰置。
/// 而**列目录回来的每一行都带着 `size`** ⇒ 最常见的那一档在这儿就答完了，
/// **连那趟往返都不发**。
#[test]
fn an_oversized_file_says_so_with_both_numbers_and_never_asks_the_remote() {
    let over = row(
        "big.log",
        false,
        false,
        crate::sftp_pool::MAX_EDIT_BYTES as u64 + 1,
    );
    let why = why_not_editable(&over).expect("超上限却说改得了");
    // 上限那个数要在那句话里。
    assert!(why.contains("256.0 K"), "那句话里没有上限那个数：{why}");
    // 🔴 **「多了多少」必须在，而且不许退化。**
    //
    // 第一版这里判的是「两个 `human_size` 都在」，而生产那一版正是那么写的
    // ⇒ 判据当场逮到它退化成「这份 **256.0 K** 超过 **256.0 K** 的编辑上限」
    //（`256 KiB + 1` 被四舍成 `256.0 K`）。那句话读出来什么都没说。
    // ⇒ 现在判的是「多了 1 字节」这个**永不退化**的数，
    //   它直接答「我该把文件弄小多少」。
    assert!(
        why.contains("多了 1 字节"),
        "那句话没说超出多少 —— 在边界附近两个 `human_size` 会一模一样：{why}"
    );
    // 精确字节数也要在（`human_size` 在边界上分不开两个值）。
    assert!(
        why.contains(&format!("{} 字节", crate::sftp_pool::MAX_EDIT_BYTES + 1)),
        "那句话里没有这份文件的精确字节数：{why}"
    );
    // 而「拒编而非截断」这条契约要说出来（`sftp_pool::MAX_EDIT_BYTES` 头注逐字）。
    assert!(
        why.contains("拒编"),
        "没说清为什么不是「截断给你看」：{why}"
    );
    assert!(!is_editable(&over));

    // 🔴 边界：**正好等于上限**的那一份是改得了的（`>` 不是 `>=`）。
    //    这一比不是洁癖 —— 池子那边 `decode_editable` 用的正是 `>`，
    //    两侧用不同的比较符会造出「这边说能改、那边拒」的静默态。
    let exact = row(
        "exact",
        false,
        false,
        crate::sftp_pool::MAX_EDIT_BYTES as u64,
    );
    assert_eq!(why_not_editable(&exact), None, "正好到上限那一份被拒了");
}

/// 目录与有损名各有自己那句话（**不是同一句**）。
#[test]
fn a_directory_and_a_lossy_name_each_get_their_own_sentence() {
    let d = why_not_editable(&row("sub", true, false, 0)).expect("目录竟然可编辑");
    let l = why_not_editable(&row("bad", false, true, 10)).expect("有损名竟然可编辑");
    assert!(d.contains("目录"), "{d}");
    assert!(l.contains("UTF-8"), "{l}");
    assert_ne!(d, l, "两种拒绝说的是同一句话 —— 那用户分不清是哪一种");
}

/// 🔴 「画不画那颗按钮」与「为什么不画」是**同一个判定**。
///
/// 两处各写一份条件时的失效形状是个静默态：按钮画出来了、点了没反应。
#[test]
fn the_button_and_the_reason_are_one_judgement() {
    let corpus = [
        row("a.txt", false, false, 10),
        row("sub", true, false, 0),
        row("bad", false, true, 10),
        row(
            "big",
            false,
            false,
            crate::sftp_pool::MAX_EDIT_BYTES as u64 + 1,
        ),
    ];
    // 反空真：两种答案都要出现。
    assert!(corpus.iter().any(|r| is_editable(r)));
    assert!(corpus.iter().any(|r| !is_editable(r)));
    for r in &corpus {
        assert_eq!(
            is_editable(r),
            why_not_editable(r).is_none(),
            "`{}` 上两个判定不一致 —— 那就是「按钮画了但点了没反应」那一形",
            r.name
        );
    }
}

/// 池子回 `None` 之后那句话里**没有**「太大」，而且提了那个竞态。
#[test]
fn the_not_text_notice_covers_what_is_left_after_the_size_check() {
    let n = not_text_notice("/srv/data/x.bin");
    assert!(n.contains("/srv/data/x.bin"), "没说是哪个文件：{n}");
    assert!(
        n.contains("NUL") && n.contains("UTF-8"),
        "没说清剩下那两种：{n}"
    );
    // 🔴 「太大」这一档**不在这句话里** —— 它在发往返之前就被挡掉了。
    //    写进来的话，用户会对着一个 1 KB 的二进制文件读到「超过编辑上限」。
    assert!(!n.contains("上限"), "把「太大」也塞进这句话了：{n}");
    // 竞态那一形要提 —— 否则用户会对着一个刚变大的文件反复点。
    assert!(n.contains("刷新"), "没给出下一步：{n}");
}

// ═══════════════════════════════════════════════════════════════════
// 改了没存就关掉 —— 这一刀的「不静默丢弃」
// ═══════════════════════════════════════════════════════════════════

/// 🔴 `dirty` 是**相等断言**，不是「敲过键」那个标志位。
///
/// 敲进去又改回来**不算改过** —— 一个标志位会把那一形报成「有未保存改动」，
/// 于是用户每次都要答一遍一个假问题，而假问题答多了他就不看了。
#[test]
fn dirty_is_an_equality_not_a_keystroke_flag() {
    let mut p = Pane::opened("/srv/a.txt", "a.txt", "hello".into());
    assert!(!p.dirty(), "刚读回来就说改过了");
    assert_eq!(judge_close(&p), Close::Now);

    p.text.push_str(" world");
    assert!(p.dirty());
    assert_eq!(judge_close(&p), Close::NeedsConfirm, "改了没存却直接关");

    // 改回去 ⇒ 不算改过（这一比正是「相等断言 vs 标志位」的分界）。
    p.text = "hello".into();
    assert!(!p.dirty(), "改回原样之后还说有未保存改动");
    assert_eq!(judge_close(&p), Close::Now);
}

/// 存成功 ⇒ 基准线跟上；存失败 ⇒ **一个字都不碰用户敲的东西**。
#[test]
fn a_failed_save_keeps_every_character_the_user_typed() {
    let mut p = Pane::opened("/srv/a.txt", "a.txt", "old".into());
    p.text = "new".into();

    // 失败：`text` 与 `dirty` 都不许变（那些字是用户唯一的一份）。
    let why = "拒绝写 Claude 数据源文件(/x/projects/p/s.jsonl)——管理会话文件请用历史浏览器";
    p.mark_failed(why.to_string());
    assert_eq!(p.text, "new", "存失败把用户敲的东西弄掉了");
    assert!(p.dirty(), "存失败之后却说已经存好了");
    match p.last_save.clone() {
        Some(Err(got)) => assert_eq!(got, why, "拒绝那句原话被改写了"),
        other => panic!("{other:?}"),
    }
    // 还要能再存一次（失败不是终态）。
    p.mark_saved();
    assert!(!p.dirty(), "存成功之后基准线没跟上");
    assert_eq!(p.last_save, Some(Ok(())));
    assert_eq!(p.text, "new");
}

/// 敲超上限 ⇒ **屏幕上先说**，不等存的时候才失败。
#[test]
fn typing_past_the_cap_is_visible_before_the_save_fails() {
    let cap = crate::sftp_pool::MAX_EDIT_BYTES;
    let mut p = Pane::opened("/srv/a.txt", "a.txt", "x".into());
    assert!(!p.over_cap());
    assert_eq!(p.headroom(), cap as i64 - 1);

    p.text = "y".repeat(cap);
    assert!(!p.over_cap(), "正好到上限就说超了（`>` 不是 `>=`）");
    assert_eq!(p.headroom(), 0);

    p.text.push('z');
    assert!(p.over_cap(), "超了却不出声 —— 那要等存的时候才被池子拒");
    assert_eq!(p.headroom(), -1);
}

// ═══════════════════════════════════════════════════════════════════
// 「这个量该多大」—— 本刀的答复是「保持 256 KiB」，而这一条是它的现打依据
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **一整个上限的文字，在一帧里排得动。**
///
/// # 它是「上限该多大」那一问在**原生文本控件语境**里的约束
///
/// `设计/60 §5.4b` 逐字把那一问留给「原生窗口的文本控件」这个语境。
/// 而在这个语境里，上限的真实约束不是内存（256 KiB 的 2× 是 512 KiB，
/// 那一节自己算过「不值一改」），是 **egui 的 `TextEdit` 每帧要把整段文字排一次版**
/// ⇒ 上限决定「打字卡不卡」。
///
/// ⇒ 本条把满上限的一段文字喂进**真 egui 帧**，量它排得出来。
/// 有读数之后，「保持 256 KiB」才是一个结论，而不是「没人动它」。
///
/// ⚠ 它**买不到**「在用户那台机器上手感如何」 —— 本机没有图形会话
/// （`XDG_SESSION_TYPE=tty`），量到的是 CPU 排版那一段，不含 GPU 上屏。
/// ⚠ 它**刻意不钉一个毫秒数**：那会变成一条随机器快慢红的判据（本仓那条
/// 「金标准把开发机烤进去只有它永远绿」的反面）。钉的是「它跑完了、
/// 而且真的排了那么多字」。
///
/// 🔴🔴 **〔第十四刀 2026-09-23 订正 —— 本条印出来的那个数被误读过〕**
/// 本条量的是**全新 `Context` 的第一帧**，而那一帧的大头是**一次性**的字体图谱
/// 与中文字形栅格化（现打：同样首帧、只放 3 字节，debug 4.6–5.6 ms /
/// release 0.30–0.47 ms），**不随文本长度长**；而且它在 **debug** 档上印出来是
/// 16–30 ms、在 **release** 档上是 **2.19 ms**。
/// ⇒ 上一刀拿这个数去跟 60 fps 的 16.67 ms 比，比错了两处（档位 · 冷首帧）。
/// ⇒ **要看「打字卡不卡」，看 [`the_readings_behind_only_laying_out_the_viewport`]**
///   —— 那一条烤热之后再量、连跑三趟、印明档位，而且它还量了本条量不到的那一维：
///   **同样 256 KiB，代价随「最长的一行有多少字节」差两三个数量级**。
/// ⇒ 本条**保留**（它钉的「跑完了 ＋ 真排了那么多字」仍然成立），
///   但它印出来那个毫秒数**不许**再被当成每帧代价。逐条住 `editor.rs` 头注 §四.0。
#[test]
fn a_full_cap_worth_of_text_still_lays_out_in_one_frame() {
    let cap = crate::sftp_pool::MAX_EDIT_BYTES;
    // 造一段**满上限**的文字，带换行（单行 256 KiB 与多行的排版代价不是一回事，
    // 而真实文本是多行的）。
    let line = "远端配置的一行 abcdefghijklmnopqrstuvwxyz 0123456789\n";
    let mut text = String::with_capacity(cap + line.len());
    while text.len() < cap {
        text.push_str(line);
    }
    // ⚠ `truncate` 要落在字符边界上（这一行里有中文，`cap` 未必是边界）——
    //    第一版直接 `truncate(cap)` 当场 panic（`is_char_boundary`）。
    //    往下找最近的边界：差几个字节不影响这一格量的是什么。
    let mut cut = cap;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
    assert!(
        text.len() > cap - 8,
        "夹具没造到满上限附近（实得 {} / {cap}）",
        text.len()
    );

    let ctx = egui::Context::default();
    let t0 = std::time::Instant::now();
    let painted = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.1,
        Vec::new(),
        |ui| {
            let mut t = text.clone();
            ui.add(egui::TextEdit::multiline(&mut t).desired_rows(30));
        },
    );
    let dt = t0.elapsed();

    // 反空真：那一帧**真的**排了字（否则这个读数是在量一个空框）。
    let drawn: usize = painted.iter().map(|(s, _)| s.len()).sum();
    assert!(
        drawn > 1000,
        "这一帧只画出了 {drawn} 字节的文字 —— 那不是在量满上限那一段"
    );
    // 只报读数，不钉毫秒（理由见头注）。
    println!(
        "〔现打〕{} 字节（上限 {cap}）的 `TextEdit::multiline` 排一帧：{:?}（这一帧画出 {drawn} 字节文字）",
        text.len(),
        dt
    );
    // 唯一的硬闸：它**跑完了**（不是挂住）。给一个极宽的上界，
    // 宽到任何一台能跑本仓门禁的机器都过得了，而「排到死」会撞它。
    assert!(
        dt < std::time::Duration::from_secs(10),
        "满上限那一段排一帧用了 {dt:?} —— 那不是「上限可以再大」，是**它已经太大了**，\
         回 `editor.rs` 头注重读「这个量该多大」那一节"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 〔第十四刀〕只排视口内的行 —— 头注 §四 那几条结论的判据
//
// 🔴 这一族**不钉毫秒数**（同上面那条的理由：钉了就是一条随机器快慢红的
//    判据）。它钉的全是**相等 / 零命中**，毫秒只当读数印出来。
// ═══════════════════════════════════════════════════════════════════

/// 造语料：`line_len == 0` ⇒ 整份压成一行（**没有 `\n`**）；否则每行 `line_len` 字节。
fn corpus(cap: usize, line_len: usize) -> String {
    let mut s = String::with_capacity(cap + 64);
    if line_len == 0 {
        while s.len() < cap {
            s.push_str("abcdefghij");
        }
        s.truncate(cap);
        return s;
    }
    let body: String = std::iter::repeat_n('a', line_len - 1).collect();
    while s.len() < cap {
        s.push_str(&body);
        s.push('\n');
    }
    s.truncate(cap);
    s
}

/// 跑一帧真 egui，回「这一帧交给文字排版的字节数」。
///
/// 🔴 它**不是**「应该是多少」的推算值 —— 是从 `FullOutput::shapes` 里
/// 把 galley 的文字抠回来数的（同 `rows::RenderTally::rows_materialized` 的理由）。
fn bytes_handed_to_layout(text: &str) -> usize {
    let ctx = egui::Context::default();
    let painted = crate::filewin::copy::testing::painted_text(
        &ctx,
        egui::vec2(1280.0, 800.0),
        0.0,
        Vec::new(),
        |ui| {
            let mut t = text.to_string();
            ui.add(
                egui::TextEdit::multiline(&mut t)
                    .desired_rows(24)
                    .desired_width(f32::INFINITY)
                    .code_editor(),
            );
        },
    );
    painted.iter().map(|(s, _)| s.len()).sum()
}

/// 🔴 [`LineIndex`] 的「行」与 **epaint 切段的单位**是同一个，而且这一条
/// 由一份**独立算出来**的答案对着钉（不是拿它自己算的数跟它自己比 ——
/// 那是本仓那条「恒等两侧同源会恒真」）。
#[test]
fn the_line_index_agrees_with_an_independently_computed_answer() {
    // 四形：不带末换行 · 带末换行 · 只有一行 · 含中文（字节 ≠ 字符）
    for text in [
        "ab\ncd\nef",
        "ab\ncd\n",
        "只有一行没有换行",
        "第一行\n第二行有点长一些\n\n第四行",
        "",
    ] {
        let idx = LineIndex::build(text);
        // 独立答案：`str::split('\n')` —— 与 epaint 的 `\n` 切段同一条规则。
        let want: Vec<&str> = text.split('\n').collect();
        assert_eq!(idx.lines(), want.len(), "行数不对：{text:?}");
        assert_eq!(idx.total_bytes(), text.len());
        for (i, w) in want.iter().enumerate() {
            let got = &text[idx.line_start(i)..idx.line_content_end(i)];
            assert_eq!(got, *w, "第 {i} 行切得不对：{text:?}");
            // 🔴 **不许把那个 `\n` 切进来** —— 切进来屏幕上会多一个全文里不存在的空行。
            assert!(!got.contains('\n'), "第 {i} 行里带着换行：{got:?}");
        }
        assert_eq!(
            idx.longest_line(),
            want.iter().map(|w| w.len()).max().unwrap_or(0),
            "最长一行不对：{text:?}"
        );
    }
}

/// 🔴 **把改过的窗口写回全文，逐字节可逆。**
///
/// 这是整条路上唯一会改用户数据的一步（[`splice_window`]）⇒ 四形都要钉：
/// 改长了 · 改短了 · 把几行删成一行 · 里头有中文。
#[test]
fn splicing_a_window_back_is_byte_exact() {
    let full = "第一行\nsecond\n第三行\nfourth\n第五行\nsixth\n";
    let idx = LineIndex::build(full);
    // 窗口 = 第 2..4 行（`第三行` / `fourth`）
    let w = window_of(&idx, 2..4);
    assert_eq!(window_text(full, &w), "第三行\nfourth", "窗口切错了");

    for (edited, want) in [
        // 原样写回 ⇒ 全文一个字节都不许变（这一条是恒等，也是「刀落在靶子上」的自检）
        ("第三行\nfourth", full.to_string()),
        // 改长
        (
            "第三行改长了\nfourth-also-longer",
            "第一行\nsecond\n第三行改长了\nfourth-also-longer\n第五行\nsixth\n".to_string(),
        ),
        // 改短
        ("x\ny", "第一行\nsecond\nx\ny\n第五行\nsixth\n".to_string()),
        // 两行删成一行（行数变了）
        ("合并", "第一行\nsecond\n合并\n第五行\nsixth\n".to_string()),
        // 删空（窗口那两行的内容全没了，但两行之间那个 `\n` 是窗口内的 ⇒ 一起没）
        ("", "第一行\nsecond\n\n第五行\nsixth\n".to_string()),
    ] {
        let mut t = full.to_string();
        splice_window(&mut t, &w, edited);
        assert_eq!(t, want, "把 {edited:?} 写回去之后全文不对");
        // 行结构确实被改了 ⇒ 调用方必须重建索引（[`splice_window`] 头注那一条）
        let re = LineIndex::build(&t);
        assert_eq!(re.total_bytes(), t.len());
    }

    // 🔴 头/尾两扇窗也要钉 —— 边界上 `line_content_end` 走的是另一支（哨兵那一支）。
    let head = window_of(&idx, 0..1);
    assert_eq!(window_text(full, &head), "第一行");
    let tail = window_of(&idx, idx.lines() - 1..idx.lines());
    assert_eq!(window_text(full, &tail), "", "末尾那个空行切错了");
    let mut t = full.to_string();
    splice_window(&mut t, &tail, "尾巴");
    assert_eq!(t, "第一行\nsecond\n第三行\nfourth\n第五行\nsixth\n尾巴");
}

/// 🔴 **「只排视口内」对一行特别长的文件买不到东西** —— 钉成一条**相等**断言。
///
/// # 这一条是头注 §四.2 第 10 条 / §四.5 的承重点
///
/// 一扇窗必须装得下**整行** ⇒ 一份压成一行的 256 KiB JSON，
/// 最坏那扇窗**就是全文**。散文里说这句话谁都能读宽成「那也能快一点」，
/// ⇒ 这里钉 `worst_window_bytes == total_bytes`。
///
/// ⚠ **反空真**：同一条判据里放一份行结构正常的语料，断言它
/// `buys_nothing() == false` 且最坏那扇窗**小两个数量级** ——
/// 证明这把尺子量得出差别，不是两边都恒真。
#[test]
fn windowing_buys_nothing_on_a_file_that_is_one_long_line() {
    let cap = crate::sftp_pool::MAX_EDIT_BYTES;

    const ROWS: usize = 40; // 一屏大约这么多行

    // ── 一行到底 ──────────────────────────────────────────────
    let one_liner = corpus(cap, 0);
    assert_eq!(
        one_liner.matches('\n').count(),
        0,
        "夹具里居然有换行 —— 那量的就不是这一档"
    );
    let one = windowing_payoff(&LineIndex::build(&one_liner), ROWS);
    assert_eq!(
        one.worst_window_bytes, one.total_bytes,
        "一行到底的文件上，最坏那扇窗竟然比全文小 —— 那 §四.2 第 10 条就写错了"
    );
    assert!(one.buys_nothing());

    // ── 行结构正常（每行 64 字节）──────────────────────────────
    let lined = corpus(cap, 64);
    let many = windowing_payoff(&LineIndex::build(&lined), ROWS);
    assert!(
        !many.buys_nothing(),
        "行结构正常的文件上竟然也说买不到 —— 那这把尺子恒真"
    );
    assert!(
        many.worst_window_bytes * 100 < many.total_bytes,
        "最坏那扇窗 {} 字节 / 全文 {} 字节 —— 差不到两个数量级",
        many.worst_window_bytes,
        many.total_bytes
    );

    // ── 🔴 **第二种「买不到」：行数比一屏还少。** ────────────────────
    //
    // 这一条是**判据自己逮出来的**：本条第一版写的是
    // `assert!(!chunky.buys_nothing())`（以为「每行 8 KiB、不是一行到底」
    // 就总省得下来），而 256 KiB / 每行 8 KiB 只有 **32 行** ——
    // **比一屏 40 行还少** ⇒ 那扇窗就是全文。
    // ⇒ 「买不到」**有两种**，不是一种，而散文里原本只写了一种。
    let few_lines = corpus(cap, 8192);
    assert!(
        few_lines.matches('\n').count() < ROWS,
        "夹具行数没少于一屏，这一档量的就不是那件事"
    );
    let few = windowing_payoff(&LineIndex::build(&few_lines), ROWS);
    assert_eq!(
        few.worst_window_bytes, few.total_bytes,
        "行数比一屏还少的文件上，最坏那扇窗竟然比全文小"
    );
    assert!(few.buys_nothing());

    // ── 中间那一档：行长 8 KiB **但行数够多** ⇒ 买得到，只是少 ────────
    let chunky = windowing_payoff(&LineIndex::build(&corpus(4 * 1024 * 1024, 8192)), ROWS);
    assert!(
        !chunky.buys_nothing(),
        "行数够多之后还说买不到 —— 那这把尺子恒真"
    );
    assert!(
        chunky.worst_window_bytes > many.worst_window_bytes,
        "行越长、窗越大这条单调性没了（{} vs {}）",
        chunky.worst_window_bytes,
        many.worst_window_bytes
    );
    // ⚠ 而它买到的只有一个数量级出头（对比 `many` 的两个数量级）——
    //    「只排视口内」**买到多少由行长决定，不由字节数决定**。
    assert!(
        chunky.worst_window_bytes * 8 < chunky.total_bytes
            && chunky.worst_window_bytes * 100 > chunky.total_bytes,
        "行长 8 KiB 那一档买到的倍数跑出了预期（窗 {} / 全文 {}）",
        chunky.worst_window_bytes,
        chunky.total_bytes
    );
}

/// 🔴 **交给排版的字节数，在 64 倍的文件大小跨度上「相等」。**
///
/// # 为什么必须是「相等」而不是「变少」
///
/// 本仓逐字：「地板在『变少』方向瞎」。一条 `assert!(windowed < full)` 在
/// 「开窗只砍掉一点」时照样绿，而那正是「假的只排视口内」过关的形状。
/// ⇒ 这里钉的是 `256 KiB 那一档` 与 `16 MiB 那一档` 交给排版的字节数**恰好相等**
///   —— 这条性质「整份排版」**结构上给不出来**（它必然是 O(n)）。
///
/// ⚠ **对照组就在同一条判据里**：同一份语料**不开窗**跑一趟，断言它交给排版的
/// 字节数**等于全文长度**、而且两个大小档上**不相等** —— 证明这把尺子量得出差别。
///
/// ⚠ 它买不到「屏幕上那几十行看起来对」—— 本机没有图形会话。
/// 它买到的是「**这一帧真的只把那么多字拿去排版了**」。
#[test]
fn the_bytes_handed_to_layout_are_equal_no_matter_how_big_the_file() {
    const ROWS: usize = 40;
    const LINE: usize = 64;
    let small = corpus(256 * 1024, LINE); // 今天的上限
    let huge = corpus(16 * 1024 * 1024, LINE); // 64 倍
    assert!(
        huge.len() / small.len() >= 60,
        "两档差得不够远，这条判据没力气"
    );

    let win = |t: &str| {
        let idx = LineIndex::build(t);
        let w = window_of(&idx, 0..ROWS);
        bytes_handed_to_layout(window_text(t, &w))
    };
    let (a, b) = (win(&small), win(&huge));

    // 反空真：真的排了字（否则这是在比两个空框）。
    assert!(a > 1000, "开窗那一趟只排了 {a} 字节 —— 那不是在量一扇窗");
    // 🔴 正题：**相等**。
    assert_eq!(
        a, b,
        "开窗之后交给排版的字节数随文件大小变了（{a} vs {b}）—— 那就不是「只排视口内」"
    );

    // ── 对照组：**故意不开窗**（就是今天生产那条路）──────────────
    let c = bytes_handed_to_layout(&small);
    let d = bytes_handed_to_layout(&corpus(1024 * 1024, LINE));
    assert_eq!(
        c,
        small.len(),
        "不开窗那一趟没把全文交给排版？那对照组失效了"
    );
    assert_ne!(
        c, d,
        "不开窗的两个大小档竟然交给排版一样多 —— 这把尺子量不出差别，上面那条相等断言就是恒真"
    );
    assert!(
        a * 50 < c,
        "开窗 {a} 字节 vs 不开窗 {c} 字节 —— 差不到 50 倍，读数与 §四.3(b) 那张表不符"
    );
}

/// 头注 §四 那几张表的**读数来源**。只印，不钉毫秒（理由见本族头注）。
///
/// 🔴 每档**连跑三趟**（本仓逐字：「一趟不算读数 …… 连跑三趟后差五倍」），
/// 并把**档位**印出来 —— 上一刀正是因为没印档位，把 debug 的数当成了
/// release 的数去跟 16.67 ms 比（§四.0）。
#[test]
fn the_readings_behind_only_laying_out_the_viewport() {
    const PROFILE: &str = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!("〔档位〕{PROFILE}（`debug_assertions` 现打）");

    // ── 打字帧：每帧改一个字 ⇒ galley 缓存必失，量的是「敲一个键要多久」──
    //
    // `wrap == false` 那一支要**自己给 layouter**：`.desired_width(f32::INFINITY)`
    // 被 `builder.rs:498` 的 `.at_most(available_width)` 夹回可见宽度，
    // multiline 上关不掉软换行（§四.1a）。
    let typing = |text: &str, wrap: bool| -> Vec<std::time::Duration> {
        let ctx = egui::Context::default();
        let mut t = text.to_string();
        let one = |ctx: &egui::Context, time: f64, t: &str| {
            let t0 = std::time::Instant::now();
            crate::filewin::copy::testing::painted_text(
                ctx,
                egui::vec2(1280.0, 800.0),
                time,
                Vec::new(),
                |ui| {
                    let mut s = t.to_string();
                    if wrap {
                        ui.add(
                            egui::TextEdit::multiline(&mut s)
                                .desired_rows(24)
                                .desired_width(f32::INFINITY)
                                .code_editor(),
                        );
                    } else {
                        let mut lay = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _w: f32| {
                            let mut job = egui::text::LayoutJob::simple(
                                buf.as_str().to_owned(),
                                egui::FontId::monospace(12.0),
                                egui::Color32::WHITE,
                                f32::INFINITY,
                            );
                            job.wrap.max_width = f32::INFINITY;
                            ui.fonts_mut(|f| f.layout_job(job))
                        };
                        ui.add(
                            egui::TextEdit::multiline(&mut s)
                                .desired_rows(24)
                                .desired_width(f32::INFINITY)
                                .layouter(&mut lay),
                        );
                    }
                },
            );
            t0.elapsed()
        };
        // 先烤热字体图谱与缓存（**不计入** —— §四.0 那条订正就是栽在这儿）
        for i in 0..2 {
            one(&ctx, i as f64 * 0.016, &t);
        }
        (0..3)
            .map(|i| {
                t.push('x');
                one(&ctx, (10 + i) as f64 * 0.016, &t)
            })
            .collect()
    };
    let band = |d: &[std::time::Duration]| {
        let mut v: Vec<f64> = d.iter().map(|x| x.as_secs_f64() * 1000.0).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        format!("{:.2}–{:.2} ms", v[0], v[v.len() - 1])
    };

    let cap = crate::sftp_pool::MAX_EDIT_BYTES;

    // ── 内存：排一帧要多少（现打，不是估）────────────────────────
    //
    // 🔴 **这把尺子先后错了两次，逐条记下来，因为两次都是「尺子没跑」而不是「没占内存」**：
    //    ① 第一版读 `/proc/self/statm` 的**当下** RSS，但读数取在 `Context`
    //       （连着 galley 缓存）被丢掉**之后** ⇒ 印出「净增 0 MiB」。
    //    ② 第二版换成 `VmHWM`（**峰值**）并让 `Context` 活着，**还是 0**——
    //       因为峰值是「进程开张以来」的，而同一个测试进程早就被别处的
    //       大语料把峰值顶上去了 ⇒ 差值恒 0。
    //    ⇒ 第三版：**当下 RSS ＋ galley 活着 ＋ 这一段摆在本测试最前面**
    //      （在任何大语料之前）。
    //    ⚠ 即便如此它仍是一个**下界**：分配器不把释放的页还给内核，
    //      同进程里先前的峰值会让后面的分配「白拿」到已有的页。
    let rss_kb = || -> u64 {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<u64>().ok())
            })
            .map(|pages| pages * 4)
            .unwrap_or(0)
    };
    for bytes in [cap, 1024 * 1024] {
        let text = corpus(bytes, 64);
        let before = rss_kb();
        // 🔴 galley 必须**在读数的时候还活着** ⇒ `Context` 留在作用域里。
        let ctx = egui::Context::default();
        let painted = crate::filewin::copy::testing::painted_text(
            &ctx,
            egui::vec2(1280.0, 800.0),
            0.0,
            Vec::new(),
            |ui| {
                let mut t = text.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut t)
                        .desired_rows(24)
                        .desired_width(f32::INFINITY)
                        .code_editor(),
                );
            },
        );
        let after = rss_kb();
        let drawn: usize = painted.iter().map(|(s, _)| s.len()).sum();
        println!(
            "〔现打·{PROFILE}〕{} 字节排一帧（真画出 {drawn} 字节）⇒ 进程常驻内存净增 {} MiB（**下界**）",
            text.len(),
            after.saturating_sub(before) / 1024
        );
        drop(ctx);
        drop(painted);
    }
    for (label, text) in [
        (
            "256 KiB / **压成一行**（§四.3a 那条现在就在犯的）",
            corpus(cap, 0),
        ),
        ("256 KiB / 4097 行（今天的常态）", corpus(cap, 64)),
        (
            "4 MiB / 65536 行（「上限调到这里」那一问）",
            corpus(4 * 1024 * 1024, 64),
        ),
    ] {
        let idx = LineIndex::build(&text);
        let w = window_of(&idx, 0..40);
        let payoff = windowing_payoff(&idx, 40);
        println!(
            "〔现打·{PROFILE}〕{label}：整份打字帧 {}｜只排 40 行（窗口 {} 字节）打字帧 {}｜开窗买到东西吗 {}",
            band(&typing(&text, true)),
            w.byte_len(),
            band(&typing(window_text(&text, &w), true)),
            if payoff.buys_nothing() { "**买不到**" } else { "买得到" },
        );
    }

    // ── §四.3(a) 那张表：**恒 256 KiB，只改行结构**，换行开/关并排 ────
    //
    // 🔴 这张表是本节最要紧的一条发现的来源：同样字节数，最快与最慢差两三个
    //    数量级 ⇒ **字节上限守错了维度**。
    for line_len in [0usize, 8192, 512, 64] {
        let text = corpus(cap, line_len);
        let paras = text.matches('\n').count() + 1;
        println!(
            "〔现打·{PROFILE}〕{} 字节 / {paras} 段（每段约 {} 字节）⇒ 打字帧：换行开 {}｜换行关 {}",
            text.len(),
            if line_len == 0 { text.len() } else { line_len },
            band(&typing(&text, true)),
            band(&typing(&text, false)),
        );
    }

    // ── §四.3(b) 那张表：**按字节扫**，整份 vs 只排 40 行 ──────────────
    //
    // 🔴 右边那一列在 64 倍跨度上恒定 —— 那条**相等**性质由
    //    [`the_bytes_handed_to_layout_are_equal_no_matter_how_big_the_file`]
    //    钉住（这里只印毫秒，不钉）。
    for bytes in [cap, 1024 * 1024, 4 * 1024 * 1024, 16 * 1024 * 1024] {
        let text = corpus(bytes, 64);
        let idx = LineIndex::build(&text);
        let w = window_of(&idx, 0..40);
        println!(
            "〔现打·{PROFILE}〕{} 字节 / {} 行 ⇒ 整份打字帧 {}｜只排 40 行（窗口 {} 字节）打字帧 {}",
            text.len(),
            idx.lines(),
            band(&typing(&text, true)),
            w.byte_len(),
            band(&typing(window_text(&text, &w), true)),
        );
    }

    // ── 排版之外那几笔 O(n)（§四.3b 下半那张表）──────────────────
    for bytes in [cap, 4 * 1024 * 1024, 64 * 1024 * 1024] {
        let t = corpus(bytes, 64);
        let mut clone2 = Vec::new();
        let mut cmp = Vec::new();
        for _ in 0..3 {
            let t0 = std::time::Instant::now();
            // egui 聚焦时每帧干这两下（`builder.rs:1085` / `:1375`）
            let (a, b) = (t.as_str().to_owned(), t.as_str().to_owned());
            clone2.push(t0.elapsed());
            std::hint::black_box((a.len(), b.len()));

            let other = t.clone();
            let t1 = std::time::Instant::now();
            let ne = t != other;
            cmp.push(t1.elapsed());
            std::hint::black_box(ne);
        }
        println!(
            "〔现打·{PROFILE}〕{} 字节 ⇒ egui 每帧 2× 全量克隆 {}｜`Pane::dirty` 全量比 {}｜撤销栈内存上界（**算的**，100 × 全文）{} MiB",
            t.len(),
            band(&clone2),
            band(&cmp),
            t.len() * 100 / 1024 / 1024,
        );
    }
}
