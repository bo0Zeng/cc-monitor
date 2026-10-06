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
