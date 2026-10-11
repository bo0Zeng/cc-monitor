//! 窗口通用件的判据。

use super::*;

/// 命中底色只加在给的那几段上；区间对不上字的边界 ⇒ 整段不加（不画出半个字）。
#[test]
fn marked_text_backs_exactly_the_given_spans() {
    let ctx = egui::Context::default();
    let mut job = None;
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        job = Some((
            marked(ui, "with_retry.rs", &[(5, 10)], Color32::WHITE),
            marked(ui, "重试.rs", &[(0, 1)], Color32::WHITE),
        ));
    });
    out.drop_without_applying_deltas();
    let (a, b) = job.unwrap();
    let hit = super::palette(&ctx).hit;
    let backed: Vec<&str> = a
        .sections
        .iter()
        .filter(|s| s.format.background == hit)
        .map(|s| &a.text[s.byte_range.start.0..s.byte_range.end.0])
        .collect();
    assert_eq!(backed, vec!["retry"]);
    assert!(
        b.sections.iter().all(|s| s.format.background != hit),
        "半个字也加了底色"
    );
    assert_eq!(b.text, "重试.rs");
}

/// 对话框底下那一排按钮的间距归对话框（一处）：正文把 `item_spacing` 改成 0 也漏不到按钮行。
#[test]
fn dialog_footer_buttons_keep_their_gap_whatever_the_body_does() {
    let ctx = egui::Context::default();
    let mut rects = Vec::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            dialog(
                ui.ctx(),
                "kit-footer-gap",
                "T",
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label("body");
                },
                &[
                    ("A".to_string(), Btn::Plain),
                    ("B".to_string(), Btn::Plain),
                    ("C".to_string(), Btn::Danger),
                ],
                1,
                0,
            );
        });
        rects = crate::copy::testing::text_in_frame(&out)
            .into_iter()
            .filter(|(t, _)| ["A", "B", "C"].contains(&t.as_str()))
            .map(|(_, r)| r)
            .collect();
        out.drop_without_applying_deltas();
    }
    rects.sort_by(|a, b| a.left().total_cmp(&b.left()));
    assert_eq!(rects.len(), 3);
    for w in rects.windows(2) {
        // 字与字之间至少隔着两颗按钮的内边距 ＋ 间距；间距被吞掉时只剩内边距。
        let pad = ctx.global_style().spacing.button_padding.x * 2.0;
        assert!(
            w[1].left() - w[0].right() > pad + 1.0,
            "按钮挨在一起：{rects:?}"
        );
    }
}

/// ★ 指针停在**没选中的那个标签页**右端 × 那一格上，每一拍画出来的样子都一样（「鼠标移动时很快地闪」那一形）。
///
/// egui 的命中按**上一拍**的控件位置判：× 只在标签页悬停时才登记，而 × 一登记、它就盖在标签页上面 ⇒ 下一拍标签页
/// 不再算悬停 ⇒ × 不登记 ⇒ 再下一拍标签页又悬停 ⇒ … 每拍翻一次。鼠标一动就出一拍，看上去就是快闪。
/// 量法：同一个指针位置连跑几拍，比每拍里有没有画 × 那个字形、标签页报不报悬停。
#[test]
fn hovering_the_close_spot_of_an_inactive_tab_does_not_flip_every_frame() {
    let ctx = egui::Context::default();
    let input = |pos: Option<egui::Pos2>| egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 200.0),
        )),
        events: pos
            .map(|p| vec![egui::Event::PointerMoved(p)])
            .unwrap_or_default(),
        ..Default::default()
    };
    let mut tab_rect = egui::Rect::NOTHING;
    let mut seen: Vec<(bool, usize)> = Vec::new();
    let x_glyph = egui_phosphor::regular::X;
    for frame in 0..8 {
        let at = (frame > 0).then(|| egui::pos2(tab_rect.right() - 14.0, tab_rect.center().y));
        let mut hovered = false;
        let out = ctx.run_ui(input(at), |ui| {
            ui.horizontal(|ui| {
                let _ = tab(ui, "A", "first", true, None);
                let (r, _) = tab(ui, "B", "second", false, None);
                tab_rect = r.rect;
                hovered = r.hovered() || r.contains_pointer();
            });
        });
        let crosses = crate::copy::testing::text_in_frame(&out)
            .into_iter()
            .filter(|(t, _)| t == x_glyph)
            .count();
        out.drop_without_applying_deltas();
        if frame >= 2 {
            seen.push((hovered, crosses));
        }
    }
    assert!(
        seen.iter().all(|s| *s == seen[0]),
        "指针不动，没选中那个标签页的 × 与悬停却逐拍翻（(悬停, 画出的 × 个数) 每拍一格）：{seen:?}"
    );
    assert_eq!(
        seen[0].1, 2,
        "指针在没选中那个标签页的 × 上，它的 × 应当画着（选中那个一直有一个）：{seen:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 悬停才露的控件只许走 `hover_reveal`（「鼠标一动就闪」那一类按写法根治）
// ═══════════════════════════════════════════════════════════════════

/// 登记控件 / 点击区的调用（在一段「悬停才走」的代码里出现任何一个 ⇒ 这段代码按悬停加控件）。
const ADDS_A_CONTROL: [&str; 24] = [
    ".interact(",
    ".add(",
    ".add_sized(",
    ".add_enabled(",
    ".put(",
    "allocate_",
    ".button(",
    ".small_button(",
    ".selectable_label(",
    ".selectable_value(",
    ".checkbox(",
    ".radio(",
    ".radio_value(",
    ".toggle_value(",
    ".link(",
    ".hyperlink",
    ".menu_button(",
    ".text_edit_singleline(",
    ".text_edit_multiline(",
    ".label(",
    "Area::new(",
    "Button::new(",
    "kit::",
    "side_item(",
];

/// 悬停读数。
const HOVER_READS: [&str; 2] = [".hovered()", ".contains_pointer()"];

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `s[at..]` 起、配平的那一对括号（`open` / `close`）里的内容末尾（不含右括号）。配不上 ⇒ 到文末。
fn balanced_end(s: &str, at: usize, open: char, close: char) -> usize {
    let mut depth = 0i32;
    for (k, c) in s[at..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return at + k;
            }
        }
    }
    s.len()
}

/// 一段生产代码里「按悬停加控件」的那几处（每处回那一段条件）。
///
/// 认四种形：`if <悬停> { 加控件 }`（连它的 `else` 那一支）· 先 `let x = <悬停>;` 再 `if x { … }` ·
/// match 守卫 `… if <悬停> => 加控件` · `<悬停>.then(|| 加控件)`。条件里只要有悬停读数（或从它派生的变量）就算。
fn adds_controls_by_hover(code: &str) -> Vec<String> {
    let reads_hover = |t: &str, tainted: &[String]| {
        HOVER_READS.iter().any(|h| t.contains(h))
            || tainted.iter().any(|v| {
                t.match_indices(v.as_str()).any(|(k, _)| {
                    let before = t[..k].chars().next_back().is_none_or(|c| !is_ident(c));
                    let after = t[k + v.len()..].chars().next().is_none_or(|c| !is_ident(c));
                    before && after
                })
            })
    };
    let adds = |t: &str| ADDS_A_CONTROL.iter().any(|n| t.contains(n));
    // 从悬停派生的变量：`let x = …hovered()…;`
    let mut tainted: Vec<String> = Vec::new();
    for (k, _) in code.match_indices("let ") {
        let rest = &code[k + 4..];
        let name: String = rest
            .trim_start_matches("mut ")
            .chars()
            .take_while(|c| is_ident(*c))
            .collect();
        // 只认 `let 名字 = …;` 这一形（`if let Some(..)` 那种模式不算），式子到深度 0 的 `;`。
        let after_name = rest.trim_start_matches("mut ")[name.len()..].trim_start();
        if name.is_empty() || !after_name.starts_with('=') || after_name.starts_with("==") {
            continue;
        }
        let mut d = 0i32;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                match c {
                    '(' | '[' | '{' => d += 1,
                    ')' | ']' | '}' => d -= 1,
                    _ => {}
                }
                d < 0 || (d == 0 && c == ';')
            })
            .map_or(rest.len(), |(e, _)| e);
        if HOVER_READS.iter().any(|h| rest[..end].contains(h)) {
            tainted.push(name);
        }
    }
    let mut hits = Vec::new();
    for (k, _) in code.match_indices("if ") {
        if code[..k].chars().next_back().is_some_and(is_ident) {
            continue;
        }
        // 条件到哪：先碰到深度 0 的 `{`（if）还是 `=>`（match 守卫）。
        let start = k + 3;
        let mut depth = 0i32;
        let mut stop = None;
        let bytes: Vec<(usize, char)> = code[start..].char_indices().collect();
        for (j, &(off, c)) in bytes.iter().enumerate() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                '{' if depth == 0 => {
                    stop = Some((start + off, false));
                    break;
                }
                '=' if depth == 0 && bytes.get(j + 1).is_some_and(|n| n.1 == '>') => {
                    stop = Some((start + off, true));
                    break;
                }
                ';' if depth == 0 => break,
                _ => {}
            }
        }
        let Some((at, guard)) = stop else { continue };
        let cond = &code[start..at];
        if !reads_hover(cond, &tainted) {
            continue;
        }
        let body = if guard {
            // 守卫那一支：`=>` 之后到深度 0 的 `,`（或一整块）。
            let arm = &code[at + 2..];
            let t = arm.trim_start();
            if t.starts_with('{') {
                let s = at + 2 + (arm.len() - t.len());
                &code[s..balanced_end(code, s, '{', '}')]
            } else {
                let mut d = 0i32;
                let end = arm
                    .char_indices()
                    .find(|&(_, c)| {
                        match c {
                            '(' | '[' | '{' => d += 1,
                            ')' | ']' | '}' => d -= 1,
                            _ => {}
                        }
                        d < 0 || (d == 0 && c == ',')
                    })
                    .map_or(arm.len(), |(e, _)| e);
                &arm[..end]
            }
        } else {
            let then_end = balanced_end(code, at, '{', '}');
            // 连它的 `else { … }`（`else if` 是另一个 `if`，自己会被扫到）。
            let after = code[then_end + 1..].trim_start();
            if let Some(e) = after.strip_prefix("else") {
                let e = e.trim_start();
                if e.starts_with('{') {
                    let s = code.len() - e.len();
                    &code[at..balanced_end(code, s, '{', '}')]
                } else {
                    &code[at..then_end]
                }
            } else {
                &code[at..then_end]
            }
        };
        if adds(body) {
            hits.push(cond.trim().to_string());
        }
    }
    for h in HOVER_READS {
        for (k, _) in code.match_indices(h) {
            let rest = &code[k + h.len()..];
            for t in [".then(", ".then_some("] {
                if let Some(r) = rest.strip_prefix(t) {
                    let s = code.len() - r.len() - 1;
                    if adds(&code[s..balanced_end(code, s, '(', ')')]) {
                        hits.push(format!("{h}{t}"));
                    }
                }
            }
        }
    }
    hits
}

/// 🔴 **悬停才露的控件只许走 [`hover_reveal`]**：整个包（生产段）里不许拿 `hovered()` / `contains_pointer()`（或从它们派生的变量）决定加不加控件、登不登记点击区。
///
/// 这是「鼠标一动就闪」那一类的写法闸：只在悬停时登记的控件一登记就盖住让它出来的那一颗，下一帧就不悬停了，每帧翻一次。
/// 悬停只画东西（铺底色 · 换字色 · 画一颗图标）不算，那不登记点击区；`hover_reveal` 自己先登记、后看悬停，也不算。行为那一半由
/// `workspace::hover_sweep_tests` 扫全窗钉着（指针停在哪都不翻帧）。
#[test]
fn only_hover_reveal_decides_by_hover_whether_a_control_is_added() {
    // 量具自检：四种形各认得出、只画东西的认不出。
    let planted = [
        "if r.hovered() { let x = ui.interact(rect, id, sense); }",
        "let hot = r.hovered() || r.contains_pointer(); if active || hot { ui.add(egui::Button::new(\"x\")); }",
        "if on { ui.painter().rect_filled(rect, 0.0, c); } else if r.hovered() { ui.label(\"x\"); }",
        "if !r.hovered() { paint(); } else { ui.button(\"x\"); }",
        "match k { Some(_) if row.hovered() => { ui.small_button(\"x\"); } _ => {} }",
        "r.contains_pointer().then(|| ui.interact(rect, id, sense));",
    ];
    for p in planted {
        assert_eq!(adds_controls_by_hover(p).len(), 1, "量具没认出这一形：{p}");
    }
    let paint_only = [
        "if resp.hovered() { ui.painter().rect_filled(rect, 4.0, p.hover); }",
        "let hot = resp.hovered() || resp.dragged(); if hot { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); }",
        "egui::Stroke::new(1.0, if bg.hovered() { p.border } else { p.border_soft }); let b = ui.button(\"y\");",
        "let r = ui.interact(rect, id, sense); if r.clicked() { go(); }",
    ];
    for p in paint_only {
        assert_eq!(
            adds_controls_by_hover(p),
            Vec::<String>::new(),
            "量具把只画东西的认成了加控件：{p}"
        );
    }

    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut files = 0;
    let mut hits = Vec::new();
    for (p, src) in guard_core::scan_tree!(std::path::Path::new(dir), &["rs"]) {
        files += 1;
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let code = guard_core::production_code(&src);
        for h in adds_controls_by_hover(&code) {
            hits.push(format!("{name}: if {h}"));
        }
    }
    assert!(files > 30, "只扫到 {files} 份：路径不对");
    assert_eq!(
        hits,
        Vec::<String>::new(),
        "这几处拿悬停决定加不加控件（鼠标一动就会逐帧翻）：改用 kit::hover_reveal（点击区一直登记，悬停只决定画不画）"
    );
}

/// 一份生产代码里不经 kit 画菜单项的那几处（`name` = 文件名；kit.rs 自己另查：菜单那几口都落到 `menu_body`、它不画按钮）。
fn menus_not_through_the_kit(name: &str, code: &str) -> Vec<String> {
    let mut hits = Vec::new();
    if name == "kit.rs" {
        let body_of = |f: &str| {
            let at = code
                .find(&format!("pub fn {f}("))
                .or_else(|| code.find(&format!("pub fn {f}<")))?;
            let open = at + code[at..].find('{')?;
            Some(code[open..balanced_end(code, open, '{', '}')].to_string())
        };
        match body_of("menu_body") {
            None => hits.push("kit 里没有 menu_body（菜单项那一种写法）".to_string()),
            Some(b) => {
                if let Some(bad) = ["Button", "button(", "selectable"]
                    .into_iter()
                    .find(|x| b.contains(x))
                {
                    hits.push(format!("menu_body 里用了 {bad}：菜单项画成了按钮"));
                }
            }
        }
        for f in ["menu", "context_menu", "select_field"] {
            match body_of(f) {
                Some(b) if b.contains("menu_body(") => {}
                Some(_) => hits.push(format!("kit::{f} 的内容不经 menu_body")),
                None => hits.push(format!("kit 里没有 {f}")),
            }
        }
        return hits;
    }
    for pat in [
        "MenuButton",
        ".menu_button(",
        ".context_menu(",
        "Popup::",
        "PopupKind::Menu",
        "ComboBox",
    ] {
        if code.contains(pat) {
            hits.push(format!("自己开了一层菜单（{pat}）"));
        }
    }
    for call in ["kit::menu(", "kit::context_menu(", "kit::select_field("] {
        for (at, _) in code.match_indices(call) {
            let open = at + call.len() - 1;
            let args = &code[open..balanced_end(code, open, '(', ')')];
            if args.contains("|ui|") {
                hits.push(format!("{call}…) 的内容是一个往里画控件的闭包"));
            }
        }
    }
    hits
}

/// 🔴 **菜单项只许经 kit 那一种写法画**（图标 ＋ 字 ＋ 右端快捷键，悬停淡底、不画框）：
/// 别处不许自己开菜单层（Popup · MenuButton · ComboBox …），也不许给 `kit::menu` 塞一个画按钮的闭包；
/// kit 里那几口都落到 `menu_body`，而它不用按钮。
#[test]
fn menu_items_are_only_drawn_by_the_kit() {
    // 量具自检：每一形认得出、正常写法认不出。
    let planted = [
        ("a.rs", "super::kit::menu(ui, \"x\", |ui| { if ui.button(\"y\").clicked() {} });"),
        ("a.rs", "egui::Popup::new(id, ctx, anchor, layer).show(|ui| ui.button(\"y\"));"),
        ("a.rs", "egui::ComboBox::from_id_salt(\"m\").show_ui(ui, |ui| {});"),
        ("a.rs", "r.context_menu(|ui| { ui.button(\"x\"); });"),
        ("kit.rs", "pub fn menu_body(ui: &mut Ui) { ui.button(\"x\"); } pub fn menu() { menu_body(); } pub fn context_menu() { menu_body(); } pub fn select_field() { menu_body(); }"),
        ("kit.rs", "pub fn menu_body(ui: &mut Ui) { paint(); } pub fn menu() { other(); } pub fn context_menu() { menu_body(); } pub fn select_field() { menu_body(); }"),
    ];
    for (n, p) in planted {
        assert_eq!(
            menus_not_through_the_kit(n, p).len(),
            1,
            "量具没认出这一形：{p}"
        );
    }
    let fine = [
        ("a.rs", "let (r, hit) = super::kit::menu(ui, (ph::PLUS, label.as_str()), &entries);"),
        ("kit.rs", "pub fn menu_body(ui: &mut Ui) { paint(); } pub fn menu() { menu_body(); } pub fn context_menu() { menu_body(); } pub fn select_field() { menu_body(); }"),
    ];
    for (n, p) in fine {
        assert_eq!(
            menus_not_through_the_kit(n, p),
            Vec::<String>::new(),
            "量具把正常写法认成了违规：{p}"
        );
    }

    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut files = 0;
    let mut hits = Vec::new();
    for (p, src) in guard_core::scan_tree!(std::path::Path::new(dir), &["rs"]) {
        files += 1;
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let code = guard_core::production_code(&src);
        for h in menus_not_through_the_kit(&name, &code) {
            hits.push(format!("{name}: {h}"));
        }
    }
    assert!(files > 30, "只扫到 {files} 份：路径不对");
    assert_eq!(
        hits,
        Vec::<String>::new(),
        "这几处的菜单没经 kit 画（菜单项会画成一颗颗带框的按钮）：改成 kit::menu / kit::context_menu / kit::select_field 交一串 kit::Entry"
    );
}

/// 菜单项那一种写法画出来的样子（稿 `.mi`）：不画框 · 悬停一整条淡底 · 快捷键贴右端对成一列 · 危险项 `--error-text` ·
/// 挡着的那一项点不动 · 组与组之间一道线。
#[test]
fn a_menu_row_is_icon_text_and_a_right_aligned_key_with_no_border() {
    let ctx = egui::Context::default();
    let entries = vec![
        Entry::Row(MenuRow::new(egui_phosphor::regular::EYE, "View").key(Some("Enter".into()))),
        Entry::Row(MenuRow::new(egui_phosphor::regular::COPY, "Copy")),
        Entry::Row(MenuRow::new(egui_phosphor::regular::INFO, "Info").blocked(Some("不行".into()))),
        Entry::Sep,
        Entry::Row(
            MenuRow::new(egui_phosphor::regular::TRASH, "Delete…")
                .key(Some("Delete".into()))
                .danger(true),
        ),
    ];
    let first_row = egui::pos2(60.0, 14.0 + 10.0);
    let blocked_row = egui::pos2(60.0, 14.0 + 2.0 * MENU_ROW_H + 10.0);
    let mut run = |events: Vec<egui::Event>| {
        let mut hit = None;
        let out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| hit = menu_body(ui, &entries),
        );
        let shapes: Vec<egui::Shape> = out.shapes.iter().map(|c| c.shape.clone()).collect();
        out.drop_without_applying_deltas();
        (shapes, hit)
    };
    run(vec![]);
    let (out, _) = run(vec![egui::Event::PointerMoved(first_row)]);
    let p = palette(&ctx);
    let mut texts = Vec::new();
    let mut rects = Vec::new();
    fn walk(
        s: &egui::Shape,
        texts: &mut Vec<(String, egui::Rect, egui::Color32)>,
        rects: &mut Vec<egui::epaint::RectShape>,
    ) {
        match s {
            egui::Shape::Text(t) => texts.push((
                t.galley.text().to_string(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
                t.galley
                    .job
                    .sections
                    .first()
                    .map_or(t.fallback_color, |s| s.format.color),
            )),
            egui::Shape::Rect(r) => rects.push(r.clone()),
            egui::Shape::Vec(v) => v.iter().for_each(|x| walk(x, texts, rects)),
            _ => {}
        }
    }
    for sh in &out {
        walk(sh, &mut texts, &mut rects);
    }
    let find = |t: &str| {
        texts
            .iter()
            .find(|(s, _, _)| s == t)
            .unwrap_or_else(|| panic!("没画「{t}」：{texts:?}"))
            .clone()
    };
    assert!(
        rects.iter().all(|r| r.stroke.width == 0.0),
        "菜单项画了框：{rects:?}"
    );
    let hover: Vec<_> = rects.iter().filter(|r| r.fill == p.hover).collect();
    assert_eq!(
        hover.len(),
        1,
        "悬停那一行没有（或不止一条）淡底：{rects:?}"
    );
    assert!(
        hover[0].rect.width() >= MENU_MIN_W,
        "悬停底不是一整条：{:?}",
        hover[0].rect
    );
    let (_, enter, _) = find("Enter");
    let (_, del, del_c) = find("Delete");
    let (_, view, view_c) = find("View");
    assert!(
        (enter.right() - del.right()).abs() < 0.5,
        "快捷键没贴右端对成一列：{enter:?} {del:?}"
    );
    assert!(
        enter.left() > view.right() + 8.0,
        "快捷键挨着字：{enter:?} {view:?}"
    );
    assert_eq!(find("Delete…").2, p.error_text, "危险项不是 --error-text");
    assert_ne!(del_c, p.text);
    assert_eq!(view_c, p.text);
    assert!(find("Info").2 != p.text, "挡着的那一项没灰");
    // 点挡着的那一项：不回；点能做的那一项：回它的下标。
    let click = |pos| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]
    };
    run(vec![egui::Event::PointerMoved(blocked_row)]);
    assert_eq!(run(click(blocked_row)).1, None, "挡着的那一项点得动");
    run(vec![egui::Event::PointerMoved(first_row)]);
    assert_eq!(run(click(first_row)).1, Some(0));
    // 分组：两组之间一道线。
    let (e, at) = grouped(vec![
        vec![MenuRow::new("", "a"), MenuRow::new("", "b")],
        vec![],
        vec![MenuRow::new("", "c")],
    ]);
    assert_eq!(
        e.iter().filter(|x| **x == Entry::Sep).count(),
        1,
        "空组也画了线，或者组间没线"
    );
    assert_eq!(at[3], (2, 0));
}
