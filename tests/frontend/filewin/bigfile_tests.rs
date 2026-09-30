//! 〔F9〕大文件模式的判据。
//!
//! 🔴 这一族**不钉毫秒数**（钉了就是一条随机器快慢红的判据）。它钉的全是**相等 / 零命中**：
//!
//! | 判据 | 异源在哪 |
//! |---|---|
//! | 进模式两条各一格（刚好越线 / 刚好不越） | 语料按字节手造，两向都过生产入口 [`show`] |
//! | 阈值 == 推算式 | 推算式与常量是两处写下的数 |
//! | 排版量 == 视口量 | **epaint 自己的排版缓存条数** ＋ 这一帧真画出来的文字，对着 `split('\n')` 与 egui 的裁剪矩形 |
//! | 长行只排可见段 | 同上；可见段对着 `chars().skip(c0)` 逐字相等 |
//! | 编辑后只重排改到的行 | epaint 缓存条数的差（缓存未命中 ＝ 真排了） |
//! | 行结构增量 == 重建 | `str::split('\n')` |
//!
//! ⚠ 买不到：屏幕上那几十行**看起来**对 —— 本机没有图形会话（`XDG_SESSION_TYPE=tty`）。

use super::*;
use crate::editor::Pane;

// ═══════════════════════════════════════════════════════════════════
// 量具
// ═══════════════════════════════════════════════════════════════════

/// 这一帧画出来的一段文字：内容 · 它的矩形 · 它被裁在哪个矩形里。
#[derive(Clone, Debug)]
struct Seen {
    text: String,
    rect: egui::Rect,
    clip: egui::Rect,
}

fn collect(s: &egui::Shape, clip: egui::Rect, out: &mut Vec<Seen>) {
    match s {
        egui::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: egui::Rect::from_min_size(t.pos, t.galley.size()),
            clip,
        }),
        egui::Shape::Vec(v) => {
            for one in v {
                collect(one, clip, out);
            }
        }
        _ => {}
    }
}

const SCREEN: egui::Vec2 = egui::vec2(1280.0, 800.0);

/// 跑一帧生产入口 [`show`]，回这一帧画出来的全部文字。
fn frame(ctx: &egui::Context, time: f64, events: Vec<egui::Event>, pane: &mut Pane) -> Vec<Seen> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
        time: Some(time),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| show(ui, Some(pane)));
    let mut seen = Vec::new();
    for cs in &out.shapes {
        collect(&cs.shape, cs.clip_rect, &mut seen);
    }
    out.drop_without_applying_deltas();
    seen
}

/// epaint 排版缓存里现在有几条 —— **不是本模块数的**。
///
/// epaint 每一趟开头只留「上一趟用过的」条目 ⇒ 一趟跑完之后，缓存 ＝ 上一趟用过的 ∪ 这一趟用过的。
/// ⇒ 稳态（两趟排同一批）时它 ＝ 那一批的条数；某一趟新排了 k 条 ⇒ 那一趟之后多 k 条。
fn galleys(ctx: &egui::Context) -> usize {
    ctx.fonts(|f| f.num_galleys_in_cache())
}

fn mono(ctx: &egui::Context) -> (f32, f32) {
    let font = egui::TextStyle::Monospace.resolve(&ctx.global_style());
    ctx.fonts_mut(|f| (f.row_height(&font), f.glyph_width(&font, '0')))
}

/// 语料：每行 `line_len` 字节（含 `\n`），**每行内容都不同**（行号打头）——
/// 相同的两行在 epaint 缓存里只算一条，那会让「条数」这把尺子少数。
fn lines_corpus(total: usize, line_len: usize) -> String {
    let mut s = String::with_capacity(total + line_len);
    let mut i = 0usize;
    while s.len() < total {
        let head = format!("L{i:08}:");
        s.push_str(&head);
        for k in 0..line_len.saturating_sub(head.len() + 1) {
            s.push((b'a' + (k % 26) as u8) as char);
        }
        s.push('\n');
        i += 1;
    }
    s.truncate(total);
    s
}

/// 一整行、没有 `\n`：`0123456789` 循环，外加每 1000 字一个标记（让不同位置的段内容不同）。
fn one_line(n: usize) -> String {
    let mut s = String::with_capacity(n);
    let mut i = 0usize;
    while s.len() < n {
        if i.is_multiple_of(1000) {
            s.push_str(&format!("<{i}>"));
        } else {
            s.push((b'0' + (i % 10) as u8) as char);
        }
        i += 1;
    }
    s.truncate(n);
    s
}

fn is_file_text(t: &str) -> bool {
    !t.starts_with("大文件模式") && !t.starts_with("这一行共")
}

/// 本族量的是**编辑面自己**（排版 · 编辑 · 撤销）在大文本上的形状。
/// 〔F9c〕上一版这里要把「打开即只读」摘掉（1 MiB 以上存不回）；存盘改走暂存区之后那一档整个删了。
fn pane(text: String) -> Pane {
    Pane::opened(
        "/srv/big.txt",
        "big.txt",
        text,
        crate::find::testing::fake_sha256(""),
    )
}

/// 先在一个丢掉的 `Context` 上跑一帧，让 `Doc` 立起来（钉偏移要它先在）。
fn warmed(text: String) -> Pane {
    let mut p = pane(text);
    frame(&egui::Context::default(), 0.0, Vec::new(), &mut p);
    assert!(
        p.big.is_big(),
        "这份语料没进大文件模式 —— 下面量的就不是那条路"
    );
    p
}

fn key(k: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn click_at(pos: egui::Pos2) -> [Vec<egui::Event>; 2] {
    let b = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    [
        vec![egui::Event::PointerMoved(pos), b(true)],
        vec![b(false)],
    ]
}

// ═══════════════════════════════════════════════════════════════════
// 一、怎么判断
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **两条各一格，各自刚好越线 / 刚好不越** —— 判定与生产入口两边都过。
#[test]
fn each_of_the_two_lines_is_crossed_by_exactly_one_byte() {
    // ── 最长一行：恰好 BIG_LINE_BYTES 不进，+1 进；全文远小于总量线 ──
    let long = |n: usize| format!("head\n{}\ntail\n", "x".repeat(n));
    let under = long(BIG_LINE_BYTES);
    let over = long(BIG_LINE_BYTES + 1);
    assert!(under.len() < BIG_TOTAL_BYTES && over.len() < BIG_TOTAL_BYTES);
    assert_eq!(judge(&under), None, "最长一行恰好等于线就进了模式");
    let w = judge(&over).expect("最长一行越线一个字节却没进模式");
    assert!(w.line_over() && !w.total_over(), "进模式的理由不对：{w:?}");
    assert_eq!(w.longest_line, BIG_LINE_BYTES + 1);

    // 按**字节**判，不按字：一行中文，字数远不到线、字节刚好越线。
    let cjk = "字".repeat(BIG_LINE_BYTES / 3 + 1);
    assert!(cjk.chars().count() < BIG_LINE_BYTES && cjk.len() > BIG_LINE_BYTES);
    assert!(
        judge(&cjk).is_some_and(|w| w.line_over()),
        "中文长行按字数判了"
    );

    // ── 全文：每行 64 字节、恰好 BIG_TOTAL_BYTES 不进，+1 进 ──
    let total = lines_corpus(BIG_TOTAL_BYTES, 64);
    assert_eq!(total.len(), BIG_TOTAL_BYTES);
    assert_eq!(judge(&total), None, "全文恰好等于线就进了模式");
    let plus = format!("{total}y");
    let w = judge(&plus).expect("全文越线一个字节却没进模式");
    assert!(w.total_over() && !w.line_over(), "进模式的理由不对：{w:?}");

    // ── 生产入口两向：越线的进、不越的不进 ──
    for (text, want) in [(under, false), (over, true), (total, false), (plus, true)] {
        let mut p = pane(text);
        frame(&egui::Context::default(), 0.0, Vec::new(), &mut p);
        assert_eq!(p.big.is_big(), want, "生产入口与判定说的不一样");
    }
}

/// 🔴 **两个阈值 == 从读数推出来的数。** 改读数不改常量、或改常量不改读数 ⇒ 红。
#[test]
fn the_two_thresholds_are_what_the_readings_derive() {
    assert_eq!(derive_threshold(LINE_READING), BIG_LINE_BYTES);
    assert_eq!(derive_threshold(TOTAL_READING), BIG_TOTAL_BYTES);
    // 取整那一步自己也钉住（手算的独立答案：√2·1024 ≈ 1448.15）。
    for (x, want) in [
        (1u128, 1u128),
        (2, 2),
        (3, 4),
        (5, 4),
        (6, 8),
        (1024, 1024),
        (1448, 1024),
        (1449, 2048),
    ] {
        assert_eq!(nearest_pow2(x), want, "nearest_pow2({x})");
    }
}

// ═══════════════════════════════════════════════════════════════════
// 二、排版量 == 视口量
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **只排视口内的行：排了哪几行 == 视口盖住的那几行，逐行相等；而且与文件多大无关。**
///
/// 三把尺子，都不是本模块数的：
/// ① 这一帧真画出来的文件文字，逐行对着 `split('\n')[first..end]`；
/// ② `first..end` 由钉住的偏移与 **egui 给的裁剪矩形**高度算；
/// ③ **epaint 的排版缓存条数** == 画出来的段数 ＋ 顶上那一行说明 —— 「整份排版、只把
///    不可见行画空白」那种假的只排视口，缓存里会多出整份的条目，这一条当场红。
///
/// ⚠ 对照组就在同一条里：同一份 2 MiB 语料走**普通路径那个 `TextEdit`**，
/// 缓存条数随行数长（上游按段缓存）—— 证明这把尺子量得出差别。
#[test]
fn only_the_lines_under_the_viewport_are_laid_out_however_big_the_file() {
    let mut laid_per_size = Vec::new();
    for total in [2 * 1024 * 1024, 16 * 1024 * 1024] {
        let text = lines_corpus(total, 64);
        let want_lines: Vec<&str> = text.split('\n').collect();
        let mut p = warmed(text.clone());
        let ctx = egui::Context::default();
        // 字体要先有一帧才量得出行高 ⇒ 先空跑一帧，再钉偏移、换一个全新的 Context 量。
        frame(&ctx, 0.0, Vec::new(), &mut pane("x".into()));
        let (row_h, _) = mono(&ctx);
        let off_y = 1000.5 * row_h; // 上下两行都只露半截
        p.big.with(|d| d.pin_offset(egui::vec2(0.0, off_y)));
        let fresh = egui::Context::default();
        let seen = frame(&fresh, 0.0, Vec::new(), &mut p);
        let mut files: Vec<&Seen> = seen.iter().filter(|s| is_file_text(&s.text)).collect();
        files.sort_by(|a, b| a.rect.min.y.total_cmp(&b.rect.min.y));
        let clip = files.first().expect("一行文件文字都没画").clip;
        let first = (off_y / row_h).floor() as usize;
        let end = ((off_y + clip.height()) / row_h).ceil() as usize;
        let got: Vec<&str> = files.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            got,
            want_lines[first..end].to_vec(),
            "画出来的行不是视口盖住的那几行（{total} 字节）"
        );
        let notice = seen
            .iter()
            .filter(|s| s.text.starts_with("大文件模式"))
            .count();
        assert_eq!(notice, 1);
        assert_eq!(
            galleys(&fresh),
            got.len() + notice,
            "epaint 这一帧排的条数 ≠ 画出来的段数 ＋ 说明 —— 有不在视口里的东西被排了（{total} 字节）"
        );
        let t = p.big.tally().expect("进了模式却没有读数");
        assert_eq!(
            (t.first_line, t.end_line, t.lines_laid),
            (first, end, got.len())
        );
        laid_per_size.push(t.bytes_laid);
    }
    assert_eq!(
        laid_per_size[0], laid_per_size[1],
        "交给排版的字节数随文件大小变了 —— 那就不是「只排视口内」"
    );

    // ── 对照组：同一类语料走普通路径（不进模式的那条）──
    let ctx = egui::Context::default();
    let small = lines_corpus(256 * 1024, 64);
    let mut t = small.clone();
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            ..Default::default()
        },
        |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut t)
                    .desired_rows(VIEW_ROWS)
                    .desired_width(f32::INFINITY)
                    .code_editor(),
            );
        },
    )
    .drop_without_applying_deltas();
    // 上游按段缓存：每个非空段一条（外加拼起来那一条）⇒ 至少「非空行数」条。
    assert!(
        galleys(&ctx) >= small.lines().count(),
        "普通路径的缓存条数 {} 竟没到非空行数 {} —— 这把尺子量不出差别，上面的相等就是恒真",
        galleys(&ctx),
        small.lines().count()
    );
}

// ═══════════════════════════════════════════════════════════════════
// 三、长行只排可见段
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **一行特别长：只排横向可见那一截，逐字等于那一截；行尾标「这一行共 N 字」。**
///
/// 64 KiB 与 1 MiB 两档交给排版的字数**相等**；那一截盖满视口宽、多出不到两列；
/// 横向滚到中间，那一截**逐字**等于 `chars().skip(c0)`。
/// 中文长行同样（按字切，不按字节切出半个字）。
#[test]
fn a_long_line_lays_out_only_the_stretch_in_view() {
    let ctx0 = egui::Context::default();
    frame(&ctx0, 0.0, Vec::new(), &mut pane("x".into()));
    let (_, char_w) = mono(&ctx0);

    let mut laid = Vec::new();
    for (n, text) in [
        (64 * 1024, one_line(64 * 1024)),
        (1024 * 1024, one_line(1024 * 1024)),
        (40_000, "中文长行".repeat(10_000)),
    ] {
        let nchars = text.chars().count();
        for c0 in [0usize, 5_000] {
            let mut p = warmed(text.clone());
            p.big
                .with(|d| d.pin_offset(egui::vec2(c0 as f32 * char_w, 0.0)));
            let fresh = egui::Context::default();
            let seen = frame(&fresh, 0.0, Vec::new(), &mut p);
            let files: Vec<&Seen> = seen.iter().filter(|s| is_file_text(&s.text)).collect();
            assert_eq!(files.len(), 1, "一行的文件却画了 {} 段", files.len());
            let seg = &files[0];
            let k = seg.text.chars().count();
            let want: String = text.chars().skip(c0).take(k).collect();
            assert_eq!(
                seg.text, want,
                "画出来那一截不是第 {c0} 字起的那一截（{n}）"
            );
            // 盖满视口：左缘不在裁剪框里面，右缘够到裁剪框右缘。
            assert!(seg.rect.min.x <= seg.clip.min.x + 1.0, "那一截左缘露了缝");
            assert!(seg.rect.max.x >= seg.clip.max.x - 1.0, "那一截没盖满视口宽");
            // 多排的不到两列（中文一个字两列宽，按「列」算的截会多出半截视口 ⇒ 只对 ASCII 卡紧）。
            if text.is_ascii() {
                let cols = (seg.clip.width() / char_w).ceil() as usize;
                assert!(k <= cols + 2, "排了 {k} 字，视口只有 {cols} 列");
            }
            // 行尾标：说的是整行的字数。
            let tag = format!("这一行共 {nchars} 字");
            assert!(
                seen.iter().any(|s| s.text == tag),
                "没标「{tag}」（{n}，从第 {c0} 字起）"
            );
            let notice = seen
                .iter()
                .filter(|s| s.text.starts_with("大文件模式"))
                .count();
            assert_eq!(
                galleys(&fresh),
                1 + 1 + notice,
                "epaint 排了不止「一截 ＋ 标签 ＋ 说明」"
            );
            if text.is_ascii() && c0 == 0 {
                laid.push(p.big.tally().unwrap().chars_laid);
            }
        }
    }
    assert_eq!(
        laid[0], laid[1],
        "64 KiB 与 1 MiB 的一行交给排版的字数不相等"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 四、编辑后只重排改到的行
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **敲一个字只排那一行，回车只排拆出来的两行，滚一行只排新露出来的那一行。**
///
/// 尺子是 epaint 缓存条数的差：稳态两趟排同一批 ⇒ 条数不变；某一趟新排了 k 条 ⇒ 多 k 条。
/// 编辑走真事件（点一下拿焦点、再送字 / 回车），改动落在全文里对着独立算的答案核。
#[test]
fn an_edit_relays_only_the_lines_it_touched() {
    let text = lines_corpus(2 * 1024 * 1024, 64);
    let mut p = warmed(text.clone());
    let ctx = egui::Context::default();
    frame(&ctx, 0.0, Vec::new(), &mut p);
    let (row_h, _) = mono(&ctx);
    p.big.with(|d| d.pin_offset(egui::vec2(0.0, 100.0 * row_h)));
    let mut t = 0.0;
    let mut go = |ev: Vec<egui::Event>, p: &mut Pane| {
        t += 0.5;
        frame(&ctx, t, ev, p)
    };
    let seen = go(Vec::new(), &mut p);
    go(Vec::new(), &mut p);
    let steady = galleys(&ctx);
    // 点第 105 行（视口里第 6 行）的「L00000105:」后面一点。
    let target = seen
        .iter()
        .find(|s| s.text.starts_with("L00000105:"))
        .expect("第 105 行没画出来");
    let pos = egui::pos2(target.rect.min.x + 1.0, target.rect.center().y);
    for ev in click_at(pos) {
        go(ev, &mut p);
    }
    go(Vec::new(), &mut p);
    assert_eq!(galleys(&ctx), steady, "点一下（没改字）就重排了东西");

    // ── 敲一个字 ──
    go(vec![egui::Event::Text("Z".into())], &mut p);
    assert_eq!(galleys(&ctx), steady + 1, "敲一个字重排的不是恰好一行");
    let line: Vec<&str> = p.text.split('\n').collect();
    assert!(
        line[105].contains('Z') && line[105].len() == 64,
        "字没落在第 105 行：{}",
        line[105]
    );
    let mut want = text.clone();
    let at = want.find("L00000105:").unwrap() + line[105].find('Z').unwrap();
    want.insert(at, 'Z');
    assert_eq!(p.text, want, "全文里别处也被改了");
    go(Vec::new(), &mut p);
    assert_eq!(galleys(&ctx), steady, "稳态一帧里还在重排");

    // ── 回车：拆成两行 ⇒ 恰好两行新排（下面的行只是往下挪，内容没变 ⇒ 命中）──
    go(vec![key(egui::Key::Enter, egui::Modifiers::NONE)], &mut p);
    assert_eq!(galleys(&ctx), steady + 2, "回车重排的不是恰好两行");
    assert_eq!(p.text.split('\n').count(), text.split('\n').count() + 1);
    go(Vec::new(), &mut p);

    // ── 撤销两步 ⇒ 全文逐字节回到原样 ──
    let ctrl_z = key(egui::Key::Z, egui::Modifiers::COMMAND);
    go(vec![ctrl_z.clone(), ctrl_z], &mut p);
    assert_eq!(p.text, text, "撤销两步之后全文没回到原样");

    // ── 滚一行 ⇒ 只排新露出来那一行 ──
    go(Vec::new(), &mut p);
    let before = galleys(&ctx);
    p.big.with(|d| d.pin_offset(egui::vec2(0.0, 101.0 * row_h)));
    go(Vec::new(), &mut p);
    assert_eq!(galleys(&ctx), before + 1, "滚一行重排的不是恰好一行");
}

// ═══════════════════════════════════════════════════════════════════
// 五、行结构 · 字 → 字节 · 编辑与撤销
// ═══════════════════════════════════════════════════════════════════

/// 一个不依赖外部 crate 的伪随机数（判据要可复现）。
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// 独立答案：`split('\n')` 给出每行的起点与字数。
fn oracle(text: &str) -> (Vec<usize>, Vec<usize>) {
    let mut starts = Vec::new();
    let mut chars = Vec::new();
    let mut at = 0;
    for l in text.split('\n') {
        starts.push(at);
        chars.push(l.chars().count());
        at += l.len() + 1;
    }
    (starts, chars)
}

fn assert_lines_match(l: &mut Lines, text: &str, when: &str) {
    let (starts, chars) = oracle(text);
    assert_eq!(l.count(), starts.len(), "{when}：行数不对");
    assert_eq!(l.total_bytes(), text.len(), "{when}：总字节不对");
    for i in 0..starts.len() {
        assert_eq!(l.start(i), starts[i], "{when}：第 {i} 行起点不对");
        assert_eq!(l.chars(i), chars[i], "{when}：第 {i} 行字数不对");
        assert!(
            !l.line_str(text, i).contains('\n'),
            "{when}：第 {i} 行带着换行"
        );
    }
    assert_eq!(
        l.widest(),
        chars.iter().copied().max().unwrap_or(0),
        "{when}：最宽一行不对"
    );
}

/// 🔴 **行结构：从零建 == `split('\n')`；每一次增量编辑之后 == 重建。**
#[test]
fn incremental_lines_equal_a_rebuild_after_every_edit() {
    for text in [
        "",
        "ab\ncd\nef",
        "ab\ncd\n",
        "只有一行",
        "第一行\n第二行长一些\n\n第四行",
    ] {
        assert_lines_match(&mut Lines::build(text), text, &format!("从零建 {text:?}"));
    }
    let pieces = [
        "",
        "x",
        "\n",
        "中文",
        "a\nb",
        "\n\n",
        "长长长长长长长长",
        "é\n字",
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut text = String::from("第一行\nsecond\n第三行很长很长很长\nfourth\n");
    let mut l = Lines::build(&text);
    for step in 0..2000 {
        let mut a = rng.below(text.len() + 1);
        while !text.is_char_boundary(a) {
            a -= 1;
        }
        let mut b = (a + rng.below(12)).min(text.len());
        while !text.is_char_boundary(b) {
            b -= 1;
        }
        let ins = pieces[rng.below(pieces.len())];
        let removed = text[a..b].to_string();
        text.replace_range(a..b, ins);
        l.splice(&text, a, &removed, ins);
        assert_lines_match(
            &mut l,
            &text,
            &format!("第 {step} 步（[{a},{b}) → {ins:?}）"),
        );
        let mut rebuilt = Lines::build(&text);
        rebuilt.widest();
        assert_eq!(l, rebuilt, "第 {step} 步：增量那份与重建那份整体不相等");
    }
}

/// 字 → 字节：对着 `char_indices` 逐位置相等（跨 64 字节块、纯 ASCII 快路两支都走）。
#[test]
fn byte_at_char_agrees_with_char_indices() {
    for s in [
        String::new(),
        "abc".into(),
        "a".repeat(200),
        "中".repeat(100),
        format!("{}é{}字{}", "x".repeat(63), "y".repeat(64), "z".repeat(10)),
    ] {
        let n = s.chars().count();
        let want: Vec<usize> = s.char_indices().map(|(i, _)| i).chain([s.len()]).collect();
        for k in 0..=n + 3 {
            let w = want.get(k).copied().unwrap_or(s.len());
            assert_eq!(byte_at_char(&s, k, false), w, "慢路 {k} in {s:?}");
            if s.is_ascii() {
                assert_eq!(byte_at_char(&s, k, true), w, "快路 {k}");
            }
        }
    }
}

/// 🔴 **编辑 ＋ 撤销 ＋ 重做逐字节可逆**（全走真事件：选区 · 剪切 · 粘贴 · 退格 · 删除 · 回车）。
#[test]
fn edits_undo_and_redo_are_byte_exact() {
    let base = format!("{}\n第二行\n{}", "x".repeat(BIG_LINE_BYTES + 10), "tail");
    let mut p = warmed(base.clone());
    let ctx = egui::Context::default();
    frame(&ctx, 0.0, Vec::new(), &mut p);
    let mut t = 0.0;
    let mut go = |ev: Vec<egui::Event>, p: &mut Pane| {
        t += 0.5;
        frame(&ctx, t, ev, p)
    };
    let seen = go(Vec::new(), &mut p);
    let row2 = seen
        .iter()
        .find(|s| s.text == "第二行")
        .expect("第二行没画出来");
    let pos = egui::pos2(row2.rect.min.x + 0.5, row2.rect.center().y);
    for ev in click_at(pos) {
        go(ev, &mut p);
    }
    let none = egui::Modifiers::NONE;
    let shift = egui::Modifiers::SHIFT;
    let steps: Vec<Vec<egui::Event>> = vec![
        vec![egui::Event::Text("甲".into())],
        vec![
            key(egui::Key::ArrowRight, shift),
            key(egui::Key::ArrowRight, shift),
        ],
        vec![egui::Event::Cut],
        vec![egui::Event::Paste("粘\n贴".into())],
        vec![key(egui::Key::Backspace, none)],
        vec![key(egui::Key::End, none), key(egui::Key::Delete, none)],
        vec![key(egui::Key::Enter, none), egui::Event::Text("尾".into())],
    ];
    let mut history = vec![p.text.clone()];
    for ev in steps {
        go(ev, &mut p);
        history.push(p.text.clone());
    }
    let last = p.text.clone();
    assert_ne!(last, base, "一串编辑之后全文竟然没变 —— 事件没送到");
    assert!(last.contains("甲") && last.contains("粘\n") && last.contains("尾"));
    let z = || key(egui::Key::Z, egui::Modifiers::COMMAND);
    for _ in 0..20 {
        go(vec![z()], &mut p);
    }
    assert_eq!(p.text, base, "撤到底之后全文没逐字节回到打开时那一份");
    let y = || key(egui::Key::Y, egui::Modifiers::COMMAND);
    for _ in 0..20 {
        go(vec![y()], &mut p);
    }
    assert_eq!(p.text, last, "重做到底之后全文不是编辑完那一份");
    // 行结构一路跟着（增量那条路走的就是这些编辑）。
    p.big.with(|d| {
        let mut l = d.lines().clone();
        assert_lines_match(&mut l, &last, "编辑 ＋ 撤销 ＋ 重做之后");
    });
    assert!(history.len() > 5);
}

// ═══════════════════════════════════════════════════════════════════
// 六、生产挂载：窗口那条路真走到这里
// ═══════════════════════════════════════════════════════════════════

/// 🔴 **判据不在执行链上就等于不存在** ⇒ 从窗口的 `frame_body` 走一趟：
/// 一份一行 200 KiB 的文件打开之后进了模式，而且这一帧画出来的文件文字 ≪ 全文；
/// 一份小文件照旧走普通路径、整份画出来（对照组）。
#[test]
fn the_file_window_really_goes_through_the_big_mode() {
    let cfg = String::from("bigfile");
    for (text, big) in [
        (one_line(200 * 1024), true),
        ("a=1\nb=2\n".to_string(), false),
    ] {
        let mut w = crate::shell::FileWindow::seeded(
            crate::source::Source::remote(cfg.clone()),
            "/srv/data".to_string(),
            None,
            Vec::<crate::source::Row>::new(),
        );
        w.edits.deliver(crate::editor::Arrived::Text {
            path: "/srv/data/x.json".into(),
            name: "x.json".into(),
            text: text.clone(),
            sha256: crate::find::testing::fake_sha256(""),
        });
        let ctx = egui::Context::default();
        let mut painted = Vec::new();
        for _ in 0..3 {
            painted = crate::find::testing::frame_text(&ctx, &mut w, Vec::new());
        }
        let pane = w.editing().expect("编辑面没立起来");
        assert_eq!(pane.big.is_big(), big, "窗口那条路上进没进模式不对");
        if big {
            let t = pane.big.tally().expect("进了模式却没排过");
            assert_eq!(t.lines_laid, 1);
            let drawn: usize = painted
                .iter()
                .filter(|s| text.contains(s.as_str()) && s.len() > 20)
                .map(|s| s.len())
                .sum();
            assert_eq!(
                drawn, t.bytes_laid,
                "窗口里画出来的文件文字 ≠ 本模块说排了的"
            );
            assert!(
                drawn * 50 < text.len(),
                "窗口里画了 {drawn} 字节 —— 那不是只排可见段"
            );
        } else {
            assert!(painted.iter().any(|s| s == &text), "小文件没整份画出来");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// 七、读数（只印，不钉）
// ═══════════════════════════════════════════════════════════════════

/// 本线程到现在一共在 CPU 上跑了多少纳秒（`/proc/thread-self/schedstat` 第一栏）。
///
/// 🔴 为什么要它：本机常被别的几路编译吃满（负载 40–55 / 24 核），墙钟读数会被**排队**
/// 抬高一到两倍、趟与趟之间差十倍（现打过：同一档 2 MiB 16.81–183.17 ms）。
/// 上 CPU 的时间只数这一帧真干的活，不数等调度的那段 ⇒ 门槛按它推。
/// ⚠ 它仍会被缓存 / 超线程争用抬高一点，所以它是**偏保守**的数，不是下界。
/// ⚠ 现打：本机上这个数**按 1 ms 取整**（正在跑的线程只在时钟节拍上记账）⇒ 它只够当
///   「墙钟有没有被排队抬高」的粗对照，**门槛按低负载时的墙钟推**（`bigfile::LINE_READING` 头注）。
fn on_cpu_ns() -> u64 {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

/// 量一次：`(墙钟 ms, 上 CPU ms)`。
fn timed(f: impl FnOnce()) -> (f64, f64) {
    let (c0, t0) = (on_cpu_ns(), std::time::Instant::now());
    f();
    (
        t0.elapsed().as_secs_f64() * 1000.0,
        on_cpu_ns().saturating_sub(c0) as f64 / 1e6,
    )
}

/// 两个阈值的**读数来源**（`LINE_READING` / `TOTAL_READING`），以及大文件模式自己的打字帧。
///
/// 🔴 每档**三趟**，每趟一个全新 `Context`、先烤热两帧（不计）、再连打三帧取最坏；
/// 印出档位（`debug_assertions`）与当时的负载，墙钟与上 CPU 两个数并排（理由见 [`on_cpu_ns`]）。
/// 跑法：`cargo test --release -p monitor --lib filewin::bigfile::tests::the_readings -- --nocapture --ignored --test-threads=1`
#[test]
#[ignore = "读数，不是判据；要 release 档手动跑"]
fn the_readings_behind_the_two_thresholds() {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    println!("〔档位〕{profile}〔负载〕{}", load.trim());

    let screen = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
        ..Default::default()
    };
    let worst = |v: &mut (f64, f64), d: (f64, f64)| {
        v.0 = v.0.max(d.0);
        v.1 = v.1.max(d.1);
    };
    // 普通路径（不进模式时那个 `TextEdit`，与 `show` 里那一支逐项相同）每敲一个键那一帧。
    //
    // 🔴〔F9 续 · 09-24 订正〕**上一版量的不是「敲一个键」**：它在帧与帧之间 `s.push('x')`，控件那一帧
    //    只是「文本变了、重排一次」。真敲键走的是控件自己的事件路径（有焦点 ⇒ `Event::Text`）：
    //    撤销器前后各克隆一次全文、字下标换字节下标 O(n)、**改完再排一次**（一帧两次排版）。
    //    现打（debug，512 KiB / 每行 64 字节）：push 那一形 ~19 ms、真敲键那一形 **~61 ms**，差三倍多。
    //    ⇒ 两个门槛的读数要按真敲键重打（`设计/60 §9c 续`）；这里改成先给焦点、再每帧送一个字。
    let normal = |text: &str| -> (f64, f64) {
        let ctx = egui::Context::default();
        let mut s = text.to_string();
        let mut w = (0.0, 0.0);
        for i in 0..5 {
            let mut inp = screen();
            inp.time = Some(i as f64 * 0.016);
            if i >= 2 {
                inp.events = vec![egui::Event::Text("x".into())];
            }
            let mut out = None;
            let d = timed(|| {
                out = Some(ctx.run_ui(inp, |ui| {
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut s)
                            .id(egui::Id::new("filewin-editor-text"))
                            .desired_rows(VIEW_ROWS)
                            .desired_width(f32::INFINITY)
                            .code_editor(),
                    );
                    if i == 1 {
                        r.request_focus();
                    }
                }));
            });
            if let Some(o) = out {
                o.drop_without_applying_deltas();
            }
            if i >= 2 {
                worst(&mut w, d);
            }
        }
        assert_eq!(
            s.len(),
            text.len() + 3,
            "送的三个字没落进去 —— 量的不是敲键"
        );
        w
    };
    // 大文件模式：点一下拿焦点，再每帧送一个字。
    let big = |text: &str| -> (f64, f64) {
        let mut p = pane(text.to_string());
        let ctx = egui::Context::default();
        let seen = frame(&ctx, 0.0, Vec::new(), &mut p);
        let at = seen
            .iter()
            .find(|s| is_file_text(&s.text))
            .map(|s| s.rect.center())
            .unwrap_or(egui::pos2(100.0, 100.0));
        let mut tt = 1.0;
        for ev in click_at(at) {
            frame(&ctx, tt, ev, &mut p);
            tt += 0.5;
        }
        let mut w = (0.0, 0.0);
        for _ in 0..3 {
            tt += 0.5;
            let d = timed(|| {
                frame(&ctx, tt, vec![egui::Event::Text("x".into())], &mut p);
            });
            worst(&mut w, d);
        }
        w
    };
    let three = |f: &dyn Fn() -> (f64, f64)| -> String {
        let v: Vec<(f64, f64)> = (0..3).map(|_| f()).collect();
        let band = |g: &dyn Fn(&(f64, f64)) -> f64| {
            let lo = v.iter().map(g).fold(f64::INFINITY, f64::min);
            let hi = v.iter().map(g).fold(0.0, f64::max);
            format!("{lo:.2}–{hi:.2}")
        };
        format!(
            "上 CPU {} ms｜墙钟 {} ms（三趟上 CPU：{:.2} / {:.2} / {:.2}）",
            band(&|x| x.1),
            band(&|x| x.0),
            v[0].1,
            v[1].1,
            v[2].1
        )
    };
    for kib in [64usize, 128, 256, 512, 1024] {
        let text = lines_corpus(kib * 1024, 64);
        println!(
            "〔现打·{profile}〕全文 {kib} KiB / 每行 64 字节 ⇒ 普通路径打字帧 {}",
            three(&|| normal(&text))
        );
    }
    for kib in [4usize, 8, 16, 32, 64] {
        let text = one_line(kib * 1024);
        println!(
            "〔现打·{profile}〕一行 {kib} KiB ⇒ 普通路径打字帧 {}",
            three(&|| normal(&text))
        );
    }
    for (label, text) in [
        (
            "全文 2 MiB / 每行 64 字节",
            lines_corpus(2 * 1024 * 1024, 64),
        ),
        (
            "全文 16 MiB / 每行 64 字节",
            lines_corpus(16 * 1024 * 1024, 64),
        ),
        ("一行 64 KiB", one_line(64 * 1024)),
        ("一行 1 MiB", one_line(1024 * 1024)),
    ] {
        println!(
            "〔现打·{profile}〕{label} ⇒ 大文件模式打字帧 {}",
            three(&|| big(&text))
        );
    }

    // ── 〔F9 续 · 09-24 → F9c 第四波〕**存得回的最大那一份**（〔F9c〕编辑上限 ＝ 8 MiB，装不进一行的分块走暂存区）
    //    **经窗口的生产路径**：打开那一帧（到货 → 立编辑面 → 判模式 → 建行表 → 排第一屏，含 `shell.rs` 每帧那一次
    //    `Pane` 克隆）与打字帧（真点一下拿焦点、再送字）。⚠ 不含后端读盘与线上搬运那一段。
    //    ⚠ 这一段第一版量的是 8 MiB（后端 `files-read-text` 一趟的天花板），读数记在 `设计/60 §9c`；
    //    那一拍逮出「存不回去」之后上限一度定成一行的上限（1 MiB）；〔F9c〕存盘走暂存区之后回到 8 MiB，
    //    「存得回的最大」＝ 恰好 `MAX_EDIT_BYTES` 那么大（每一形都存得回，不再按转义削）。
    let window = |text: &str| -> ((f64, f64), (f64, f64), bool) {
        let mut w = crate::shell::FileWindow::seeded(
            crate::source::Source::remote(String::from("readings")),
            "/srv/data".to_string(),
            None,
            Vec::<crate::source::Row>::new(),
        );
        let ctx = egui::Context::default();
        let mut tt = 0.0;
        let mut run = |w: &mut crate::shell::FileWindow, ev: Vec<egui::Event>| {
            tt += 0.5;
            let mut inp = screen();
            inp.time = Some(tt);
            inp.events = ev;
            let out = ctx.run_ui(inp, |ui| w.frame_body(ui));
            let seen = crate::copy::testing::text_in_frame(&out);
            out.drop_without_applying_deltas();
            seen
        };
        run(&mut w, Vec::new()); // 字体图集那一帧不算
        w.edits.deliver(crate::editor::Arrived::Text {
            path: "/srv/data/big.txt".into(),
            name: "big.txt".into(),
            text: text.to_string(),
            sha256: crate::find::testing::fake_sha256(""),
        });
        // 打开 ＝ 到货之后、**第一屏文件文字真画出来**为止的那几帧之和
        //（egui 的模态框第一帧只量尺寸不画 ⇒ 通常是两帧）。
        let is_file = |t: &str| t.len() > 8 && text.contains(t);
        let mut seen = Vec::new();
        let mut frames = 0;
        let open = timed(|| {
            while frames < 5 && !seen.iter().any(|(t, _): &(String, egui::Rect)| is_file(t)) {
                seen = run(&mut w, Vec::new());
                frames += 1;
            }
        });
        let big = w.editing().is_some_and(|p| p.big.is_big());
        let at = seen
            .iter()
            .find(|(t, _)| is_file(t))
            .map(|(_, r)| r.center())
            .expect("五帧里第一屏一行文件文字都没画");
        for ev in click_at(at) {
            run(&mut w, ev);
        }
        let mut k = (0.0, 0.0);
        for _ in 0..3 {
            let d = timed(|| {
                run(&mut w, vec![egui::Event::Text("x".into())]);
            });
            worst(&mut k, d);
        }
        assert!(w.editing().unwrap().dirty(), "送的字没落进全文");
        (open, k, big)
    };
    // 存得回的最大那一份：〔F9c〕恰好 `MAX_EDIT_BYTES`（语料生成器按它切齐）。
    let largest = |make: &dyn Fn(usize) -> String| -> String {
        let mut t = make(crate::editor::MAX_EDIT_BYTES);
        let mut cut = t.len().min(crate::editor::MAX_EDIT_BYTES);
        while !t.is_char_boundary(cut) {
            cut -= 1;
        }
        t.truncate(cut);
        t
    };
    for (label, text) in [
        ("每行 64 字节", largest(&|n| lines_corpus(n, 64))),
        ("压成一行", largest(&one_line)),
        (
            "中文每行 30 字",
            largest(&|n| ("汉字".repeat(15) + "\n").repeat(n / 91)),
        ),
    ] {
        let v: Vec<_> = (0..3).map(|_| window(&text)).collect();
        println!(
            "〔现打·{profile}〕窗口生产路径 存得回的最大 · {label}（{} 字节，{}）⇒ 打开那一帧墙钟 {:.2} / {:.2} / {:.2} ms｜\
             打字帧（三帧最坏）墙钟 {:.2} / {:.2} / {:.2} ms",
            text.len(),
            if v[0].2 { "大文件模式" } else { "普通路径" },
            v[0].0 .0,
            v[1].0 .0,
            v[2].0 .0,
            v[0].1 .0,
            v[1].1 .0,
            v[2].1 .0,
        );
    }
}
