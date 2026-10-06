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
