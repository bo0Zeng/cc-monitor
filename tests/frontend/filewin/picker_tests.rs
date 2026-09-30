//! 〔W5-FILES · 第五波〕`filewin/picker.rs` 的判据 —— **原生选文件框**（上传 · 存到哪儿）。
//!
//! 要求住址：`设计/60 §6.2`「原生选文件框 —— 上传今天是问一句本机路径；本机无图形会话验不了」
//! ＋ 主会话 09-25 补件（WIN1 报备）：「文件窗口需要选本机文件 / 目录的地方（上传、下载到…）用原生选择框」。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`picked_paths_are_appended_one_per_line_without_duplicates`] | 选到的路径接在框里已有的字后面，一行一个、不重复 | 期望手写 |
//! | [`browse_on_the_upload_prompt_fills_the_box_through_the_injected_picker`] | 上传那一问真画出「选择…」、真点它 ⇒ 选择框被问「选文件」、结局填进框；没选 ⇒ 框不动、出声 | 假选择框记下被问了什么 |
//! | [`browse_on_the_save_prompt_fills_the_destination_with_the_suggested_name`] | 下载「存到哪儿」⇒ 选择框被问「存」且建议名 ＝ 那一行的名字；选到的位置填进那一问 | 同上 |
//!
//! ⚠ 买不到：真弹框（本机无图形会话）· 真 Windows（交叉编译到 `x86_64-pc-windows-gnu` 为止）。

use super::*;
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend};

#[test]
fn picked_paths_are_appended_one_per_line_without_duplicates() {
    let p = |s: &str| std::path::PathBuf::from(s);
    assert_eq!(append_lines("", &[p("/a"), p("/b")]), "/a\n/b");
    assert_eq!(append_lines("/a\n\n", &[p("/a"), p("/c")]), "/a\n/c");
    assert_eq!(append_lines("/x", &[]), "/x");
}

/// 假选择框：回一份定好的结局，并记下被问了什么。
struct Fake {
    answer: Option<Vec<std::path::PathBuf>>,
    asked: std::sync::Arc<std::sync::Mutex<Vec<PickKind>>>,
}

impl Picker for Fake {
    fn pick(&self, kind: PickKind) -> PickFuture {
        self.asked.lock().unwrap().push(kind);
        let a = self.answer.clone();
        Box::pin(async move { a })
    }
}

async fn settle(w: &mut crate::shell::FileWindow) {
    for _ in 0..600 {
        if w.settle_pick() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("等了 3 秒选择框的结局还没落下");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browse_on_the_upload_prompt_fills_the_box_through_the_injected_picker() {
    let wired = wire_up("w5-pick-up", FakeBackend::new(&[], Declared::default())).await;
    let mut w = window_on(&wired, "/srv");
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    w.picker = std::sync::Arc::new(Fake {
        answer: Some(vec!["/home/u/a.txt".into(), "/home/u/b.txt".into()]),
        asked: asked.clone(),
    });
    w.upload.open();
    // 真画一帧、真点「选择…」那颗按钮（合成指针）。
    let ctx = egui::Context::default();
    crate::find::testing::frame_text(&ctx, &mut w, Vec::new());
    let painted = painted_rects(&ctx, &mut w);
    let at = crate::copy::testing::rects_of(&painted, "选择…");
    assert!(
        painted.iter().any(|(t, _)| t == "选择…"),
        "上传那一问上没画「选择…」：{:?}",
        painted.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(at.len(), 1, "「选择…」不是恰好一颗");
    let pos = at[0].center();
    crate::find::testing::frame_text(&ctx, &mut w, vec![egui::Event::PointerMoved(pos)]);
    crate::find::testing::frame_text(
        &ctx,
        &mut w,
        vec![
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    settle(&mut w).await;
    assert_eq!(asked.lock().unwrap().clone(), vec![PickKind::OpenFiles]);
    assert_eq!(w.upload.text(), Some("/home/u/a.txt\n/home/u/b.txt"));
    // 没选到 ⇒ 框不动、说一句。
    w.picker = std::sync::Arc::new(Fake {
        answer: None,
        asked: asked.clone(),
    });
    assert!(w.start_pick(Purpose::Upload, None));
    settle(&mut w).await;
    assert_eq!(
        w.upload.text(),
        Some("/home/u/a.txt\n/home/u/b.txt"),
        "没选到却动了框"
    );
    assert!(
        w.upload
            .refused()
            .is_some_and(|r| r.starts_with("没有选到文件")),
        "没选到却没出声：{:?}",
        w.upload.refused()
    );
}

/// 帧上画出来的字连同矩形（给「按名字找按钮」用）。
fn painted_rects(
    ctx: &egui::Context,
    w: &mut crate::shell::FileWindow,
) -> Vec<crate::copy::testing::PaintedText> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| w.frame_body(ui));
    let painted = crate::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    painted
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn browse_on_the_save_prompt_fills_the_destination_with_the_suggested_name() {
    let wired = wire_up("w5-pick-down", FakeBackend::new(&[], Declared::default())).await;
    let mut w = window_on(&wired, "/srv");
    *w.listing.rows.lock().unwrap() = vec![crate::source::Row {
        name: "report.pdf".into(),
        path: "/srv/report.pdf".into(),
        is_dir: false,
        size: 9,
        lossy_name: false,
    }
    .into()];
    assert!(w.begin_pull(0));
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    w.picker = std::sync::Arc::new(Fake {
        answer: Some(vec!["/home/u/下载/report.pdf".into()]),
        asked: asked.clone(),
    });
    assert!(w.start_pick(Purpose::Download, None));
    settle(&mut w).await;
    assert_eq!(
        asked.lock().unwrap().clone(),
        vec![PickKind::SaveFile {
            suggested_name: "report.pdf".into()
        }]
    );
    assert_eq!(
        w.pull_dest_mut().map(|s| s.clone()),
        Some("/home/u/下载/report.pdf".to_string())
    );
}
