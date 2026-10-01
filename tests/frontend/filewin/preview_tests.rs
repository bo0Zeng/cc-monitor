//! 本族最重的那一刀：光标连挪时同一时刻最多一趟 `files-read-text` 在飞、最新的赢、没有去抖定时器。
//! 旁边两格：读不了的形状说一句话且线上零条；超上限整趟拒、不截断。
//!
//! [`super`] 的判据 —— **预览**。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`the_latest_pick_wins_and_the_middle_ones_are_never_read`] | 连挪三下光标：线上恰好两趟 `files-read-text`（第一项 ＋ 最后一项），面板上摆的是最后一项 | 实得是合成后端真收到的那几行 |
//! | [`what_is_not_read_says_why_and_sends_nothing`] | 目录 / 两项 / 超上限 ⇒ 一句话、线上零条；不是文本 ⇒ 那句话 | 零命中读的是线上那本账 |
//! | [`the_text_on_the_panel_is_the_text_the_backend_sent`] | 打开预览、选中一个文件、跑生产那个 `Workspace::frame` ⇒ 后端给的那几行真画在这一帧上 | galley 读回；文本由合成后端给 |
//! | [`line_starts_cut_where_the_newlines_are`] | 行起点逐格相等 | 期望手写 |
//!
//! ⚠ 买不到：真远端上一趟的时延；图片 / 二进制预览（没做）。

use super::*;
use crate::copy::testing::text_in_frame;
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend, Wired};
use crate::select::Intent;
use crate::source::Row;
use crate::workspace::Workspace;

fn row(name: &str, size: u64, is_dir: bool) -> Row {
    Row {
        name: name.into(),
        path: format!("/srv/{name}"),
        is_dir,
        size,
        lossy_name: false,
    }
}

async fn rig(tag: &str, rows: Vec<Row>) -> (Wired, FileWindow) {
    let wired = wire_up(
        tag,
        FakeBackend::new(&["files-read-text"], Declared::default()),
    )
    .await;
    let w = window_on(&wired, "/srv");
    *w.listing.rows.lock().unwrap() = rows.into_iter().map(Into::into).collect();
    (wired, w)
}

fn reads(wired: &Wired) -> Vec<String> {
    wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-read-text")
        .map(|r| r["args"]["path"].as_str().unwrap_or("").to_string())
        .collect()
}

/// 跟到面板上摆出 `Text` / `Said` 为止（带上限，绝不挂死）。
async fn follow_until_settled(p: &mut Preview, w: &FileWindow) {
    for _ in 0..600 {
        p.follow(w, None);
        if matches!(p.view(), View::Text { .. } | View::Said(_)) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("等了 3 秒预览还在 {:?}", p.view());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_latest_pick_wins_and_the_middle_ones_are_never_read() {
    let (wired, mut w) = rig(
        "fw34-pv-latest",
        vec![
            row("a.txt", 5, false),
            row("b.txt", 5, false),
            row("c.txt", 5, false),
        ],
    )
    .await;
    let mut p = Preview::default();
    let step = |w: &mut FileWindow| {
        w.apply_intent(
            Intent::Step {
                by: 1,
                extend: false,
            },
            0.0,
            None,
        );
    };
    // 第一下 ⇒ 发 a；a 还在飞时连挪两下（b、c）⇒ 只记下「要 c」。
    step(&mut w);
    p.follow(&w, None);
    step(&mut w);
    p.follow(&w, None);
    step(&mut w);
    p.follow(&w, None);
    assert_eq!(p.fired(), 1, "a 还在飞时就又发了");
    follow_until_settled(&mut p, &w).await;
    assert_eq!(
        reads(&wired),
        vec!["/srv/a.txt".to_string(), "/srv/c.txt".to_string()],
        "线上那几趟不等于「第一项 ＋ 最后一项」"
    );
    assert_eq!(p.fired(), 2);
    // 每一趟送的上限都是预览自己那个数（不是编辑上限）。
    let caps: Vec<u64> = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-read-text")
        .map(|r| r["args"]["max_bytes"].as_u64().unwrap_or(0))
        .collect();
    assert_eq!(
        caps,
        vec![PREVIEW_MAX_BYTES; 2],
        "线上那几趟的 max_bytes 不是预览的上限"
    );
    match p.view() {
        View::Text { path, text, .. } => {
            assert_eq!(path, "/srv/c.txt");
            assert_eq!(text, "text of /srv/c.txt", "摆出来的不是后端给的那份");
        }
        other => panic!("面板上摆的是 {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_is_not_read_says_why_and_sends_nothing() {
    let over = PREVIEW_MAX_BYTES + 1;
    let (wired, mut w) = rig(
        "fw34-pv-idle",
        vec![
            row("dir", 0, true),
            row("big.log", over, false),
            row("binary.bin", 5, false),
            row("ok.txt", PREVIEW_MAX_BYTES, false),
        ],
    )
    .await;
    let mut p = Preview::default();
    p.follow(&w, None);
    assert_eq!(p.view(), &View::Idle(PICK_ONE.to_string()), "没选时那一句");
    let step = |w: &mut FileWindow, p: &mut Preview| {
        w.apply_intent(
            Intent::Step {
                by: 1,
                extend: false,
            },
            0.0,
            None,
        );
        p.follow(w, None);
    };
    step(&mut w, &mut p); // dir
    assert!(
        matches!(p.view(), View::Idle(s) if s.contains("目录")),
        "{:?}",
        p.view()
    );
    step(&mut w, &mut p); // big.log
    assert!(
        matches!(p.view(), View::Idle(s) if s.contains("big.log") && s.contains("以内")),
        "{:?}",
        p.view()
    );
    w.apply_intent(Intent::SelectAll, 0.0, None);
    p.follow(&w, None);
    assert_eq!(
        p.view(),
        &View::Idle("选中了 4 项，预览只看一个文件".into())
    );
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        reads(&wired).is_empty(),
        "不该读的那几形上了线：{:?}",
        reads(&wired)
    );
    // 不是文本：发了、后端说 not_text ⇒ 那句话。
    w.apply_intent(
        Intent::Step {
            by: 1,
            extend: false,
        },
        0.0,
        None,
    ); // binary.bin
    follow_until_settled(&mut p, &w).await;
    assert!(
        matches!(p.view(), View::Said(s) if s.contains("不是文本")),
        "{:?}",
        p.view()
    );
    // 正控：恰好在上限上的那一份照读。
    w.apply_intent(
        Intent::Step {
            by: 1,
            extend: false,
        },
        0.0,
        None,
    ); // ok.txt
    follow_until_settled(&mut p, &w).await;
    assert!(matches!(p.view(), View::Text { .. }), "{:?}", p.view());
    assert_eq!(
        reads(&wired),
        vec!["/srv/binary.bin".to_string(), "/srv/ok.txt".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_on_the_panel_is_the_text_the_backend_sent() {
    let (_wired, w) = rig("fw34-pv-paint", vec![row("note.md", 5, false)]).await;
    let mut ws = Workspace::new(w);
    ws.set_preview(true);
    ws.pane_on_mut(0).apply_intent(
        Intent::Step {
            by: 1,
            extend: false,
        },
        0.0,
        None,
    );
    let ctx = egui::Context::default();
    let mut seen = false;
    for _ in 0..600 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| ws.frame(ui));
        let painted = text_in_frame(&out);
        out.drop_without_applying_deltas();
        if painted.iter().any(|(t, _)| t == "text of /srv/note.md") {
            seen = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(seen, "后端给的那一行没画在预览面板上");
}

#[test]
fn line_starts_cut_where_the_newlines_are() {
    assert_eq!(line_starts(""), vec![0]);
    assert_eq!(line_starts("a"), vec![0]);
    assert_eq!(line_starts("a\nbc\n"), vec![0, 2, 5]);
    assert_eq!(line_starts("\n\n"), vec![0, 1, 2]);
}
