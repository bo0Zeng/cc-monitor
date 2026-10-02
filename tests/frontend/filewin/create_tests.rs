//! [`super`] 的判据 —— 窗口上的**「新建空文件」**（老面板 7 项里写侧那一项）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`clicking_the_toolbar_button_puts_up_the_box`] | 工具栏上真画了那颗按钮，**真点一下**（合成指针事件喂生产那个 `frame_body`）框就摆出来 | 按钮位置从这一帧画出来的字里现找 |
//! | [`a_new_file_speaks_files_create_with_root_and_rel_and_no_content`] | 线上发的是 `files-create`、`(root, rel)` 切对、**没有 `content`**（＝ 空文件），跑完重列一次目录 | 期望手写；实得是合成后端真收到的那几行（真回环口 ＋ 真钥匙） |
//! | [`an_existing_name_comes_back_as_the_backends_sentence_and_is_painted`] | 同名已在 ⇒ 后端那句原话回到窗口上、**画出来**，不是「成功」 | 那句话是合成后端给的，界面上读回来 |
//! | [`an_impossible_name_keeps_the_box_up_and_sends_nothing`] | 名字不合法 ⇒ 框留着、出声、**线上零条**（带正控：合法名字恰好一条） | 零命中读的是线上那本账 |
//! | [`no_line_says_so_instead_of_doing_nothing`] | 没连上后端 ⇒ 出声、框留着 | — |
//!
//! ⚠ 买不到：真远端上建成过一份文件（合成后端只记账不落盘）；窗口真画在屏幕上。

use super::*;
use crate::find::testing::{frame_text, window_on, wire_up, Declared, FakeBackend};

/// 等写操作那块结果板跑完 `want` 趟。**带上限，绝不挂死**（同 `find::testing::settle`）。
async fn settle_writes(w: &FileWindow, want: u64, who: &str) {
    for _ in 0..600 {
        if w.write_board.rounds() >= want {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!(
        "{who}：等了 3 秒结果板还是只跑完 {} 趟（要 {want} 趟）",
        w.write_board.rounds()
    );
}

/// 一帧生产那个 `frame_body` 连同窗口的框（命令栏上那几颗在框里），交回这一帧画出来的字 ＋ 位置。
fn frame_with_rects(
    ctx: &egui::Context,
    w: &mut FileWindow,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(input, |ui| crate::chrome::testing::pane_with_chrome(ui, w));
    let painted = crate::copy::testing::text_in_frame(&out);
    out.drop_without_applying_deltas();
    painted
}

/// 🔴 **真点一下**：按钮位置从这一帧画出来的字里找（不是源码里有没有那个字面量）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clicking_the_toolbar_button_puts_up_the_box() {
    let wired = wire_up("create-click", FakeBackend::new(&[], Declared::default())).await;
    let mut w = window_on(&wired, "/srv/data");
    let ctx = egui::Context::default();
    let _ = frame_with_rects(&ctx, &mut w, Vec::new());
    let painted = frame_with_rects(&ctx, &mut w, Vec::new());
    let at = crate::copy::testing::rects_of(&painted, NEW_FILE_LABEL.as_str());
    assert_eq!(
        at.len(),
        1,
        "工具栏上「{NEW_FILE_LABEL}」该恰好一颗：{painted:?}",
        NEW_FILE_LABEL = NEW_FILE_LABEL.as_str()
    );
    assert!(w.new_file_prompt().is_none(), "还没点，框就摆出来了");
    let _ = frame_with_rects(&ctx, &mut w, crate::rows::testing::click_at(at[0].center()));
    let p = w
        .new_file_prompt()
        .expect("点了「新建空文件」，框没摆出来")
        .clone();
    assert_eq!(p.dir, "/srv/data", "框记下的目录不是当前目录");
    // 框真的画出来了（它是一个模态，画在这一帧上）。
    let painted = frame_text(&ctx, &mut w, Vec::new());
    assert!(
        painted.iter().any(|t| *t == NewFilePrompt::heading()),
        "框那一行提示没画出来：{painted:?}"
    );
    // 阴性对照：点「新建目录」摆的是另一个框，不是这一个（两颗按钮没接反）。
    w.cancel_new_file();
    let painted = frame_with_rects(&ctx, &mut w, Vec::new());
    let mk = crate::copy::testing::rects_of(&painted, &crate::writeops::MKDIR_LABEL);
    let _ = frame_with_rects(&ctx, &mut w, crate::rows::testing::click_at(mk[0].center()));
    assert!(
        w.new_file_prompt().is_none(),
        "点「新建目录」却摆出了新建文件的框"
    );
    assert!(
        w.write_prompt().is_some(),
        "点「新建目录」连它自己的框都没摆"
    );
    assert!(
        wired.cmds().is_empty(),
        "只是摆框，线上却有了请求：{:?}",
        wired.cmds()
    );
}

/// 🔴 线上那一行：`files-create`、`(root, rel)`、**没有 `content`**；跑完重列一次目录。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_file_speaks_files_create_with_root_and_rel_and_no_content() {
    let wired = wire_up(
        "create-face",
        FakeBackend::new(&["files-create", "files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/srv/data");
    assert!(w.begin_new_file());
    *w.new_file_text_mut().unwrap() = "  笔记.md ".to_string();
    assert!(w.confirm_new_file(None), "合法名字却没发出去");
    assert!(w.new_file_prompt().is_none(), "发出去了，框却还摆着");
    settle_writes(&w, 1, "新建空文件").await;
    let got: Vec<serde_json::Value> = wired.log.lock().unwrap().clone();
    assert_eq!(
        got,
        vec![
            serde_json::json!({ "cmd": "files-create", "args": { "root": "/srv/data", "rel": "笔记.md" } })
        ],
        "线上那一行与期望不等（带了 `content` 就不是「空文件」了）"
    );
    let out = w.write_board.last().expect("跑完了却没有结果");
    assert_eq!((out.ok, out.failed.len()), (1, 0), "{out:?}");
    // 跑完 ⇒ 重列当前目录，恰好一次（新文件要出现）。
    assert!(w.settle_finished_writes(), "跑完了却没重列目录");
    assert!(!w.settle_finished_writes(), "同一趟重列了两次");
    for _ in 0..600 {
        if wired.count("files-ls") >= 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(wired.cmds(), ["files-create", "files-ls"]);
}

/// 🔴 同名已在 ⇒ 后端那句原话回到窗口上、**画出来**；不是「成功」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_existing_name_comes_back_as_the_backends_sentence_and_is_painted() {
    let wired = wire_up(
        "create-exists",
        FakeBackend::new(&["files-create", "files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/srv/data");
    w.begin_new_file();
    *w.new_file_text_mut().unwrap() = "exists.md".to_string();
    assert!(w.confirm_new_file(None));
    settle_writes(&w, 1, "同名已在").await;
    let out = w.write_board.last().unwrap();
    assert_eq!(out.ok, 0, "同名已在却报成了成功：{out:?}");
    assert_eq!(out.failed.len(), 1, "{out:?}");
    let (what, why) = &out.failed[0];
    assert_eq!(what, &label("/srv/data/exists.md"));
    assert!(
        why.contains("目标已经在了") && !why.contains("io_failed"),
        "后端的原话没带回来，或错误码上了屏：`{why}`"
    );
    let ctx = egui::Context::default();
    let _ = frame_text(&ctx, &mut w, Vec::new());
    let painted = frame_text(&ctx, &mut w, Vec::new());
    assert!(
        painted
            .iter()
            .any(|t| t.contains("没做成") && t.contains("目标已经在了")),
        "那句话没画在窗口上：{painted:?}"
    );
}

/// 🔴 名字不合法 ⇒ 框留着、出声、**线上零条**；正控：换个合法名字恰好一条。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_impossible_name_keeps_the_box_up_and_sends_nothing() {
    let wired = wire_up(
        "create-bad",
        FakeBackend::new(&["files-create", "files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/srv/data");
    w.begin_new_file();
    for bad in ["", "   ", "sub/x.md", ".", ".."] {
        *w.listing.error.lock().unwrap() = None;
        *w.new_file_text_mut().unwrap() = bad.to_string();
        assert!(!w.confirm_new_file(None), "「{bad}」竟然发得出去");
        assert!(w.new_file_prompt().is_some(), "「{bad}」被拒了，框却收掉了");
        assert!(
            w.listing.error.lock().unwrap().is_some(),
            "「{bad}」被拒了却一句话都没说"
        );
    }
    // 给它点时间：真有请求在飞的话，这时候已经落账了。
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        wired.cmds().is_empty(),
        "不合法的名字上了线：{:?}",
        wired.cmds()
    );
    // 正控：同一个框换一个合法名字 ⇒ 恰好一条。
    *w.new_file_text_mut().unwrap() = "ok.txt".to_string();
    assert!(w.confirm_new_file(None));
    settle_writes(&w, 1, "正控").await;
    assert_eq!(wired.count("files-create"), 1);
}

/// 没连上后端 ⇒ 出声、框留着（不静默、不退回 SFTP —— `D11`）。
#[test]
fn no_line_says_so_instead_of_doing_nothing() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("起得来一个运行时");
    let mut w = FileWindow::seeded(
        super::super::source::Source::remote(String::from("create-noline")),
        "/srv/data".to_string(),
        Some(rt.handle().clone()),
        Vec::<super::super::source::Row>::new(),
    );
    w.begin_new_file();
    *w.new_file_text_mut().unwrap() = "a.md".to_string();
    assert!(!w.confirm_new_file(None), "没连上后端却说发出去了");
    assert_eq!(
        w.listing.error.lock().unwrap().as_deref(),
        Some(NO_LINE.as_str()),
        "没连上后端，说的不是那一句"
    );
    assert!(w.new_file_prompt().is_some(), "发不出去，框却收掉了");
}
