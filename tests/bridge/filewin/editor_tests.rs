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
        crate::filewin::editor::MAX_EDIT_BYTES as u64 + 1,
    );
    let why = why_not_editable(&over).expect("超上限却说改得了");
    // 上限那个数要在那句话里。
    // 〔F9 续〕上限 256 KiB → 1 MiB（`MAX_EDIT_BYTES` 头注），`human_size` 印出来是 `1.0 M`。
    assert!(
        why.contains(&crate::filewin::rows::human_size(
            crate::filewin::editor::MAX_EDIT_BYTES as u64
        )),
        "那句话里没有上限那个数：{why}"
    );
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
        why.contains(&format!(
            "{} 字节",
            crate::filewin::editor::MAX_EDIT_BYTES + 1
        )),
        "那句话里没有这份文件的精确字节数：{why}"
    );
    // 而「拒编而非截断」这条契约要说出来（`editor::MAX_EDIT_BYTES` 头注逐字；第九刀时那句话住池子那份同名常量上）。
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
        crate::filewin::editor::MAX_EDIT_BYTES as u64,
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
            crate::filewin::editor::MAX_EDIT_BYTES as u64 + 1,
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
// 〔F7a · 第三波 09-24〕读那一半经通道问后端 `files-read-text`
// ═══════════════════════════════════════════════════════════════════

/// 后端那一趟的结局 → 三形（纯函数）：`too_large` / `not_text` 是「不可编辑」（`None`），
/// 别的拒与没走通是 `Err`（原话），回了却没有 `text` 是 `Err`（**不当成空文本**）。
#[test]
fn the_reply_maps_to_text_not_text_or_failure_by_the_peers_code() {
    use crate::filewin::source::Failed;
    let failed = |code: Option<&str>, said: &str| Failed {
        code: code.map(str::to_string),
        said: said.to_string(),
    };
    assert_eq!(
        text_from_reply(Ok(
            serde_json::json!({ "text": "hi\n", "path": "/a", "bytes": 3 })
        )),
        Ok(Some("hi\n".to_string()))
    );
    for code in ["too_large", "not_text"] {
        assert_eq!(
            text_from_reply(Err(failed(Some(code), "x"))),
            Ok(None),
            "`{code}` 没落成「不可编辑」"
        );
    }
    // 阴性对照：别的码 / 没有码（没走通）⇒ 原话，**不许**也压成「不可编辑」。
    for code in [Some("unreadable"), Some("bad_args"), None] {
        assert_eq!(
            text_from_reply(Err(failed(code, "那句原话"))),
            Err("那句原话".to_string()),
            "`{code:?}` 被压成了「不可编辑」—— 连不上与不是文本在屏幕上就分不开了"
        );
    }
    assert!(
        text_from_reply(Ok(serde_json::json!({ "path": "/a" }))).is_err(),
        "回了却没有 `text`，竟然当成了一份文本（空编辑框存回去就是把文件清空）"
    );
}

/// ★ 经真回环口、真钥匙到合成后端：那几形各落各的；线上每一趟都带着窗口那个上限。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reading_goes_through_the_channel_and_each_refusal_lands_on_its_own_shape() {
    use crate::filewin::find::testing::{wire_up, Declared, FakeBackend};
    let wired = wire_up(
        "edit-read",
        FakeBackend::new(&[CMD_READ_TEXT], Declared::default()),
    )
    .await;
    let origin = crate::filewin::source::Origin(wired.origin.clone());
    let got = |p: &'static str| {
        let line = wired.line.clone();
        let origin = origin.clone();
        async move { read_text(&line, &origin, p).await }
    };
    assert_eq!(
        got("/srv/a.md").await,
        Ok(Some("text of /srv/a.md".to_string()))
    );
    assert_eq!(got("/srv/binary.bin").await, Ok(None));
    assert_eq!(got("/srv/huge.log").await, Ok(None));
    let e = got("/srv/gone.txt").await.expect_err("读不到竟然成了");
    assert!(e.contains(CMD_READ_TEXT), "那句原话没说是哪条命令：{e}");
    let log = wired.log.lock().unwrap().clone();
    assert_eq!(log.len(), 4, "线上该恰好四趟：{log:?}");
    for r in &log {
        assert_eq!(r["cmd"], CMD_READ_TEXT);
        assert_eq!(
            r["args"]["max_bytes"].as_u64(),
            Some(MAX_EDIT_BYTES as u64),
            "有一趟没带窗口的编辑上限：{r}"
        );
    }
    // 对端不认这条命令（旧后端）⇒ 原话，而且**一个字节都不发**（能力协商）。
    let old = wire_up("edit-read-old", FakeBackend::new(&[], Declared::default())).await;
    let e = read_text(
        &old.line,
        &crate::filewin::source::Origin(old.origin.clone()),
        "/srv/a.md",
    )
    .await
    .expect_err("旧后端不认这条命令，竟然读到了");
    assert!(e.contains("版本旧"), "旧后端那一形没说清：{e}");
    assert_eq!(old.count(CMD_READ_TEXT), 0);
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
    let cap = crate::filewin::editor::MAX_EDIT_BYTES;
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
/// ⇒ **要看「打字卡不卡」，看 `bigfile_tests::the_readings_behind_the_two_thresholds`**
///   〔F9：原先指的第十四刀那条读数判据随窗口化内核一起删了，读数改由这一条重打〕
///   —— 那一条烤热之后再量、连跑三趟、印明档位，而且它还量了本条量不到的那一维：
///   **同样 256 KiB，代价随「最长的一行有多少字节」差两三个数量级**。
/// ⇒ 本条**保留**（它钉的「跑完了 ＋ 真排了那么多字」仍然成立），
///   但它印出来那个毫秒数**不许**再被当成每帧代价。逐条住 `editor.rs` 头注 §四.0。
#[test]
fn a_full_cap_worth_of_text_still_lays_out_in_one_frame() {
    let cap = crate::filewin::editor::MAX_EDIT_BYTES;
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

// 〔F9 2026-09-24〕第十四刀那一族判据（行索引 · 窗口写回 · 「开窗买不到」· 交给排版的字节相等 ·
//   读数）随那组内核一起删了：它们钉的是「窗口化 `TextEdit`」那条没被选的路。
//   落地那条路的判据住 `bigfile_tests.rs`（理由见 `editor.rs` 头注 §四）。

// ═══════════════════════════════════════════════════════════════════
// 〔F9c · 第四波〕存盘：装得进一行的一条 `files-write-text`；装不进的分块走暂存区（`调研/第四波记录/F9c.md`）
// ═══════════════════════════════════════════════════════════════════

const SAVE_PATH: &str = "/srv/data/app.conf";

/// 🔴 **窗口量的那一行 == monitor 真发出去的那一行**（按最长的 id 算）。
///
/// 异源：另一侧是 monitor 那一侧真正编请求行的纯函数 `inbound_client::encode_request`，
/// 不是本模块的 [`request_line_len`]。语料覆盖会被转义变长的每一类
/// （引号 · 反斜杠 · 控制字符 · 换行 · 中文不转义）；三条命令（整份一行 · 一块 · 提交）各量一遍。
/// 另一格：`id` 的最长形状（u128 毫秒十六进制 ＋ 两个 u64）装得进 [`REQUEST_ID_ROOM`]。
#[test]
fn the_measured_save_line_is_byte_for_byte_the_line_that_is_sent() {
    let key = "0123456789abcdef0123456789abcdef";
    for content in [
        String::new(),
        "a=1\n".into(),
        "引号\"反斜杠\\制表\t换行\n回车\r".into(),
        "\u{1}\u{2}\u{1f}".repeat(100),
        "中文不转义".repeat(1000),
        "x".repeat(70_000),
    ] {
        for (cmd, args) in [
            (CMD_WRITE_TEXT, save_args(SAVE_PATH, &content)),
            (CMD_STAGE_CHUNK, stage_args(key, u64::MAX, &content)),
            (
                CMD_COMMIT_TEXT,
                commit_args(SAVE_PATH, key, 17, content.len()),
            ),
        ] {
            let sent = crate::backend::control::inbound_client::encode_request(
                &"0".repeat(REQUEST_ID_ROOM),
                cmd,
                &args,
            );
            assert!(sent.ends_with('\n'));
            assert_eq!(
                request_line_len(cmd, &args),
                sent.len() - 1,
                "`{cmd}`：窗口量的那一行与真编出来的那一行不一样长（内容 {} 字节）",
                content.len()
            );
        }
    }
    let longest_id = format!("m{:x}.{}-{}", u128::MAX, u64::MAX, u64::MAX);
    assert_eq!(
        longest_id.len(),
        REQUEST_ID_ROOM,
        "id 的最长形状与留的位子对不上"
    );
}

/// 🔴 **逐字转义长度 == `serde_json` 的**，对**全部** Unicode 标量值（异源：`serde_json::to_string`）。
#[test]
fn the_per_char_escape_length_matches_serde_json_for_every_scalar_value() {
    let mut checked = 0u32;
    for c in (0u32..=0x10FFFF).filter_map(char::from_u32) {
        let mut buf = [0u8; 4];
        let real = serde_json::to_string(c.encode_utf8(&mut buf) as &str)
            .unwrap()
            .len()
            - 2;
        assert_eq!(escaped_len(c), real, "U+{:04X}", c as u32);
        checked += 1;
    }
    assert_eq!(
        checked,
        0x110000 - 0x800,
        "码位没走全（应当是全部标量值：去掉代理区 2048 个）"
    );
}

/// 🔴 **切块三条性质一起钉，合起来恰好刻画「贪心取满」那一种切法**（不是切成一字一块的空真）：
/// ① 拼回来逐字节 == 原文；② 每块真编出来的请求行（`encode_request`，最长 id、块号按最大）≤ 一行上限；
/// ③ 除最后一块，每块再添下一块的第一个字，那一行就越线。语料覆盖每一种转义长度（1 · 2 · 3 · 4 · 6 字节）。
#[test]
fn chunks_reassemble_exactly_each_fits_one_line_and_each_is_filled() {
    let key = "0123456789abcdef0123456789abcdef";
    let line_of = |chunk: &str| {
        crate::backend::control::inbound_client::encode_request(
            &"0".repeat(REQUEST_ID_ROOM),
            CMD_STAGE_CHUNK,
            &stage_args(key, u64::MAX, chunk),
        )
        .len()
            - 1
    };
    let mixed: String = (0..400_000u32)
        .map(|i| {
            [
                'a', '"', '\\', '\n', '\u{1}', '中', '🦀', 'é', '\u{7f}', '\u{2028}',
            ][(i * 7 % 10) as usize]
        })
        .collect();
    for (label, text) in [
        ("纯 ASCII 3 MiB", "abcdefgh".repeat(3 * 1024 * 1024 / 8)),
        ("满控制字符 1 MiB（×6）", "\u{1}".repeat(1024 * 1024)),
        ("中文 2 MiB", "汉".repeat(2 * 1024 * 1024 / 3)),
        ("混合", mixed),
        ("刚好一块", "x".repeat(chunk_budget())),
    ] {
        let chunks = plan_chunks(&text, chunk_budget());
        assert_eq!(chunks.concat(), text, "{label}：拼回来不是原文");
        assert!(
            chunks.iter().all(|c| !c.is_empty()),
            "{label}：切出了空块（后端拒空块）"
        );
        for (i, c) in chunks.iter().enumerate() {
            assert!(
                line_of(c) <= SAVE_LINE_CAP,
                "{label}：第 {i} 块那一行 {} 字节，越过一行上限 {SAVE_LINE_CAP}",
                line_of(c)
            );
            if let Some(next) = chunks.get(i + 1) {
                let first = next.chars().next().unwrap();
                assert!(
                    line_of(&format!("{c}{first}")) > SAVE_LINE_CAP,
                    "{label}：第 {i} 块没取满（再添一个字还装得下）"
                );
            }
        }
    }
    assert_eq!(
        plan_chunks(&"x".repeat(chunk_budget()), chunk_budget()).len(),
        1
    );
    assert_eq!(
        plan_chunks(&"x".repeat(chunk_budget() + 1), chunk_budget()).len(),
        2
    );
}

/// 起一台合成后端（认得存盘那三条命令），交回线、`origin`、以及它最近一次提交拼出来的那一份。
async fn save_rig(
    tag: &str,
    refuse_stage_at: Option<u64>,
) -> (
    crate::filewin::find::testing::Wired,
    crate::filewin::source::Origin,
    std::sync::Arc<std::sync::Mutex<Option<(String, String)>>>,
) {
    let mut be = crate::filewin::find::testing::FakeBackend::new(
        &[CMD_WRITE_TEXT, CMD_STAGE_CHUNK, CMD_COMMIT_TEXT],
        crate::filewin::find::testing::Declared::default(),
    );
    be.refuse_stage_at = refuse_stage_at;
    let committed = be.committed.clone();
    let wired = crate::filewin::find::testing::wire_up(tag, be).await;
    let origin = crate::filewin::source::Origin(wired.origin.clone());
    (wired, origin, committed)
}

/// 🔴 **刚好装得进一行的一条 `files-write-text`、零块；多一个字节的整份分块走暂存区，合成后端拼回 == 原文。**
///
/// 走真通道（生产那个 `chan::client::Client` 拨真回环口）到一台合成后端，数线上出现过的命令序列。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_save_that_fits_one_line_goes_as_one_write_and_one_byte_more_goes_in_chunks() {
    let (wired, origin, committed) = save_rig("save-split", None).await;
    let base = request_line_len(CMD_WRITE_TEXT, &save_args(SAVE_PATH, ""));
    let fits = "a".repeat(SAVE_LINE_CAP - base);
    assert!(fits_one_line(SAVE_PATH, &fits), "夹具没造到刚好装满");
    let over = format!("{fits}a");
    assert!(!fits_one_line(SAVE_PATH, &over));

    write_text(&wired.line, &origin, SAVE_PATH, &fits)
        .await
        .expect("刚好装得进一行的那一份没存成");
    assert_eq!(
        wired.cmds(),
        vec![CMD_WRITE_TEXT],
        "刚好装得进一行的那一份没走一条写"
    );

    write_text(&wired.line, &origin, SAVE_PATH, &over)
        .await
        .expect("多一个字节的那一份没存成");
    let n = plan_chunks(&over, chunk_budget()).len();
    let mut want = vec![CMD_WRITE_TEXT.to_string()];
    want.extend(std::iter::repeat_n(CMD_STAGE_CHUNK.to_string(), n));
    want.push(CMD_COMMIT_TEXT.to_string());
    assert_eq!(
        wired.cmds(),
        want,
        "多一个字节的那一份不是「逐块 ＋ 一次提交」"
    );
    assert_eq!(n, 2, "多一个字节只该多出一块");
    let got = committed
        .lock()
        .unwrap()
        .clone()
        .expect("合成后端没收到一次成功的提交");
    assert_eq!(got.0, SAVE_PATH);
    assert!(
        got.1 == over,
        "拼回来的不是原文（{} vs {} 字节）",
        got.1.len(),
        over.len()
    );
}

/// 🔴 **最坏的那一形存得回：满上限的控制字符**（一个字节转义成六个 ⇒ 上一版打开即只读）。
/// 经真通道逐块送、提交，合成后端拼回 == 原文；一个编辑面照样可改（只读那一档随它的前提一起删了）。
/// 阴性对照：多一个字节 ⇒ 本地拒、线上零新增，那句话带着两个数。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_worst_case_full_cap_file_saves_back_through_the_staging_area() {
    let (wired, origin, committed) = save_rig("save-worst", None).await;
    let text = "\u{1}".repeat(MAX_EDIT_BYTES);
    let mut p = Pane::opened(SAVE_PATH, "app.conf", text.clone());
    assert!(!p.over_cap());
    write_text(&wired.line, &origin, SAVE_PATH, &p.text)
        .await
        .expect("满上限的控制字符没存成");
    let n = wired.count(CMD_STAGE_CHUNK);
    assert_eq!(n, plan_chunks(&text, chunk_budget()).len());
    assert_eq!(wired.count(CMD_COMMIT_TEXT), 1);
    assert_eq!(wired.count(CMD_WRITE_TEXT), 0);
    assert!(
        committed
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(_, t)| *t == text),
        "拼回来的不是原文"
    );
    p.mark_saved();
    assert!(!p.dirty());

    let over = format!("{text}\u{1}");
    let before = wired.cmds().len();
    let why = write_text(&wired.line, &origin, SAVE_PATH, &over)
        .await
        .expect_err("超上限的那一份竟然存成了");
    assert_eq!(wired.cmds().len(), before, "超上限的那一份还是上了线");
    for want in [
        (MAX_EDIT_BYTES + 1).to_string(),
        MAX_EDIT_BYTES.to_string(),
        "多了 1 字节".to_string(),
        "没有发出去".to_string(),
    ] {
        assert!(why.contains(&want), "那句话没说出「{want}」：{why}");
    }
}

/// 🔴 **送到一半断了 ⇒ 不发提交，那句话说第几段、共几段、原话**；编辑框的字一个不丢。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chunk_that_fails_stops_the_save_before_the_commit() {
    let (wired, origin, committed) = save_rig("save-broken", Some(2)).await;
    let text = "\u{1}".repeat(2 * 1024 * 1024);
    let total = plan_chunks(&text, chunk_budget()).len();
    assert!(total > 3, "夹具切不出第三块");
    let mut p = Pane::opened(SAVE_PATH, "app.conf", "old".into());
    p.text = text.clone();
    let why = write_text(&wired.line, &origin, SAVE_PATH, &p.text)
        .await
        .expect_err("第三块断了却存成了");
    p.mark_failed(why.clone());
    assert_eq!(
        wired.cmds(),
        vec![CMD_STAGE_CHUNK; 3],
        "断在第三块之后还在发（或发了提交）"
    );
    assert!(committed.lock().unwrap().is_none());
    assert!(
        why.contains(&format!("第 3 段（共 {total} 段）")) && why.contains("盘满了"),
        "那句话没说清断在哪、为什么：{why}"
    );
    assert_eq!(p.text, text, "存失败清掉了编辑框");
    assert!(p.dirty());
}
