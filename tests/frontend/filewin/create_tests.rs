//! [`super`] 的判据 —— 窗口上的**「新建空文件」**（老面板 7 项里写侧那一项）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`clicking_the_toolbar_button_puts_up_the_inline_row`] | 命令栏「新建 ▾ → 空文件」**真点一下**（合成指针事件喂生产那个 `frame_body`）⇒ 列表里冒出一行「新建文件」、名字是个输入框 | 按钮位置从这一帧画出来的字里现找 |
//! | [`a_new_file_speaks_files_create_with_root_and_rel_and_no_content`] | 线上发的是 `files-create`、`(root, rel)` 切对、**没有 `content`**（＝ 空文件），回来收掉那一格、重列一次目录 | 期望手写；实得是合成后端真收到的那几行（真回环口 ＋ 真钥匙） |
//! | [`a_taken_name_says_exists_under_the_cell_and_keeps_it`] | 后端回 `exists` ⇒ 那一格留着、下面说「x 已存在」（不是系统那句长话，不是「成功」） | 码是合成后端给的 |
//! | [`a_bad_name_is_judged_by_that_machine_and_said_under_the_cell`] | 名字合不合法由那台判：敲的原样交出去（`single`），回 `bad_name` ⇒ 那一格留着、照登那一句 | 线上那一行逐格比 |
//! | [`no_line_says_so_instead_of_doing_nothing`] | 没连上后端 ⇒ 出声、那一格留着 | — |
//!
//! ⚠ 买不到：真远端上建成过一份文件（合成后端只记账不落盘）；窗口真画在屏幕上。

use super::*;
use crate::find::testing::{frame_text, window_on, wire_up, Declared, FakeBackend};

/// 等就地那一趟回来（[`FileWindow::settle_inline`] 收到）。**带上限，绝不挂死**（同 `find::testing::settle`）。
async fn settle_inline(w: &mut FileWindow, who: &str) {
    for _ in 0..600 {
        if w.settle_inline() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("{who}：等了 3 秒那一趟还没回来");
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
async fn clicking_the_toolbar_button_puts_up_the_inline_row() {
    let wired = wire_up("create-click", FakeBackend::new(&[], Declared::default())).await;
    let mut w = window_on(&wired, "/srv/data");
    let ctx = egui::Context::default();
    let _ = frame_with_rects(&ctx, &mut w, Vec::new());
    let painted = frame_with_rects(&ctx, &mut w, Vec::new());
    // 命令栏「新建 ▾」⇒ 菜单里「空文件」。
    let menu = crate::copy::testing::rects_of(&painted, crate::chrome::NEW_LABEL.as_str());
    assert_eq!(menu.len(), 1, "命令栏上「新建」该恰好一颗：{painted:?}");
    assert!(w.new_file_prompt().is_none(), "还没点，那一格就冒出来了");
    let _ = frame_with_rects(
        &ctx,
        &mut w,
        crate::rows::testing::click_at(menu[0].center()),
    );
    let painted = frame_with_rects(&ctx, &mut w, Vec::new());
    let at = crate::copy::testing::rects_of(&painted, crate::chrome::NEW_FILE_ITEM.as_str());
    assert_eq!(
        at.len(),
        1,
        "「新建 ▾」菜单里「空文件」该恰好一项：{painted:?}"
    );
    let _ = frame_with_rects(&ctx, &mut w, crate::rows::testing::click_at(at[0].center()));
    let p = w
        .new_file_prompt()
        .expect("点了「新建空文件」，那一格没冒出来")
        .clone();
    assert_eq!(p.dir, "/srv/data", "那一格记下的目录不是当前目录");
    // 列表里真冒出了那一行：名字缺省「新建文件」、是一个输入框（不是对话框）。
    let painted = frame_text(&ctx, &mut w, Vec::new());
    let want = copy_core::copy_text("rsFilewinWriteops.inline.newFile", &[]);
    assert_eq!(p.text, want, "缺省名字不对");
    assert!(
        painted.iter().any(|t| *t == want),
        "那一行没画出来：{painted:?}"
    );
    // 阴性对照：点「新建 ▾ → 文件夹」冒的是另一种（两颗没接反）。
    w.cancel_new_file();
    let painted = frame_with_rects(&ctx, &mut w, Vec::new());
    let menu = crate::copy::testing::rects_of(&painted, crate::chrome::NEW_LABEL.as_str());
    let _ = frame_with_rects(
        &ctx,
        &mut w,
        crate::rows::testing::click_at(menu[0].center()),
    );
    let painted = frame_with_rects(&ctx, &mut w, Vec::new());
    let mk = crate::copy::testing::rects_of(&painted, crate::chrome::NEW_FOLDER_ITEM.as_str());
    let _ = frame_with_rects(&ctx, &mut w, crate::rows::testing::click_at(mk[0].center()));
    assert!(
        w.new_file_prompt().is_none(),
        "点「新建目录」却冒出了新建文件那一格"
    );
    assert!(
        matches!(
            w.write_prompt().map(|p| &p.kind),
            Some(crate::writeops::PromptKind::Mkdir)
        ),
        "点「新建目录」连它自己那一格都没冒"
    );
    assert!(
        wired.cmds().is_empty(),
        "只是冒出那一格，线上却有了请求：{:?}",
        wired.cmds()
    );
}

/// 🔴 线上那一行：`files-create`、`(root, rel)`、**没有 `content`**；回来收掉那一格、重列一次目录。
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
    assert!(!w.confirm_new_file(None), "在飞的那一趟还没回，又发了一趟");
    settle_inline(&mut w, "新建空文件").await;
    assert!(w.new_file_prompt().is_none(), "建成了，那一格却还摆着");
    assert!(
        w.prompt_error().is_none(),
        "建成了却说了一句：{:?}",
        w.prompt_error()
    );
    for _ in 0..600 {
        if wired.count("files-ls") >= 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let got: Vec<serde_json::Value> = wired.log.lock().unwrap().clone();
    assert_eq!(
        got.first(),
        Some(
            &serde_json::json!({ "cmd": "files-create", "args": { "root": "/srv/data", "rel": "笔记.md", "single": true } })
        ),
        "线上那一行与期望不等（带了 `content` 就不是「空文件」了）"
    );
    // 建成 ⇒ 重列当前目录（新文件要出现），选中它。
    assert_eq!(wired.cmds(), ["files-create", "files-ls"]);
    assert_eq!(w.reveal_name(), Some("笔记.md"), "新文件没被选中");
}

/// 🔴 名字被占了（后端 `exists`）⇒ 那一格留着、下面说「x 已存在」；不是系统那句长话，不是「成功」。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_taken_name_says_exists_under_the_cell_and_keeps_it() {
    let wired = wire_up(
        "create-exists",
        FakeBackend::new(&["files-create", "files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/srv/data");
    w.begin_new_file();
    *w.new_file_text_mut().unwrap() = "exists.md".to_string();
    assert!(w.confirm_new_file(None));
    settle_inline(&mut w, "同名已在").await;
    assert!(w.new_file_prompt().is_some(), "名字被占了，那一格却收掉了");
    assert_eq!(
        w.prompt_error(),
        Some(copy_core::copy_text(
            "rsFilewinWriteops.inline.exists",
            &[("name", "exists.md")]
        )),
        "那一格下面说的不是「已存在」"
    );
    let ctx = egui::Context::default();
    let _ = frame_text(&ctx, &mut w, Vec::new());
    let painted = frame_text(&ctx, &mut w, Vec::new());
    assert!(
        painted.iter().any(|t| t.contains(&copy_core::copy_text(
            "rsFilewinWriteops.inline.exists",
            &[("name", "exists.md")]
        ))),
        "那句话没画在窗口上：{painted:?}"
    );
    assert!(
        !painted.iter().any(|t| t.contains("目标已经在了")),
        "系统那句长话上了屏：{painted:?}"
    );
}

/// 🔴 名字合不合法由那台判：框里敲的原样交出去（`single`），回 `bad_name` ⇒ 那一格留着、下面照登那台那一句。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bad_name_is_judged_by_that_machine_and_said_under_the_cell() {
    let wired = wire_up(
        "create-bad",
        FakeBackend::new(&["files-create", "files-ls"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/srv/data");
    w.begin_new_file();
    *w.new_file_text_mut().unwrap() = " sub/x.md ".to_string();
    assert!(w.confirm_new_file(None), "名字交给那台判，窗口不拦");
    settle_inline(&mut w, "名字不合法").await;
    assert_eq!(
        wired.log.lock().unwrap().first(),
        Some(
            &serde_json::json!({ "cmd": "files-create", "args": { "root": "/srv/data", "rel": "sub/x.md", "single": true } })
        ),
        "敲的名字没有原样作 `rel` 交出去"
    );
    assert!(w.new_file_prompt().is_some(), "名字被拒了，那一格却收掉了");
    assert!(
        w.prompt_error()
            .is_some_and(|e| e.contains("名称不能是路径：sub/x.md")),
        "那一格下面说的不是那台那一句：{:?}",
        w.prompt_error()
    );
    assert_eq!(wired.cmds(), ["files-create"], "被拒了还重列了");
}

/// 没连上后端 ⇒ 出声、那一格留着（不静默、不退回 SFTP —— `D11`）。
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
        w.prompt_error().as_deref(),
        Some(crate::shell::NO_LINE.as_str()),
        "没连上后端，说的不是那一句"
    );
    assert!(w.new_file_prompt().is_some(), "发不出去，那一格却收掉了");
}
