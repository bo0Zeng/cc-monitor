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
