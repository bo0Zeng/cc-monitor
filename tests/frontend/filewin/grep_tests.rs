//! 要求：「文件管理器**做**按内容搜（那台后端执行、有字节与条数上界、可撤）」＋
//! 「窗口里一个搜索框出结果列表、点了跳到文件」。`filewin/grep.rs` 与 `shell.rs` 那几处接线的判据。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`the_window_decodes_the_backend_golden`] | 后端那一侧对拍的同一份金样 ⇒ 解出来的命中 · 计数 · 停在哪逐格等于手写；缺格 / 怪值 ⇒ 读不懂 | 金样由后端 `grep_tests::the_product_matches_the_cross_half_golden` 写着 |
//! | [`a_search_asks_the_backend_about_this_directory_and_puts_the_hits_on_the_frame`] | 按了「搜内容」⇒ 线上恰好一条 `files-grep`（根 = 当前目录、串 = 框里的字）；帧上画出那几条命中与总述 | 帧上文字从 galley 读回 |
//! | [`clicking_a_hit_goes_to_its_directory_and_highlights_it`] | 点第 2 条 ⇒ 进 `<root>/sub`、高亮 `c.txt`、屏幕换回目录列表；越界 ⇒ 不动 | 期望手写 |
//! | [`stop_withdraws_the_search_in_flight`] | 后端扣着不回时按「停」⇒ 这一趟当场收（不在飞了、说出来被撤了）；正控：不停 ⇒ 放行后收到命中 | 真通道、真撤单手柄 |

use super::*;
use crate::find::testing::{self, Declared, FakeBackend};

fn golden() -> Value {
    serde_json::from_str(include_str!("../../__fixtures__/files-grep.golden.json")).unwrap()
}

fn ctx_ready() -> egui::Context {
    let ctx = egui::Context::default();
    let out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.drop_without_applying_deltas();
    ctx
}

async fn settle(board: &GrepBoard, before: u64, who: &str) {
    for _ in 0..600 {
        if board.rounds() > before {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("{who}：等了 3 秒那块板子还是没有新答案");
}

#[test]
fn the_window_decodes_the_backend_golden() {
    let o = decode_grep(&golden()).unwrap();
    assert_eq!(
        o.hits,
        vec![
            GrepHit {
                path: b"<root>/a.txt".to_vec(),
                rel: b"a.txt".to_vec(),
                line: 2,
                text: b"the Needle here".to_vec(),
                matches: 2,
                lines: vec![
                    GrepLine {
                        line: 2,
                        text: b"the Needle here".to_vec(),
                        marks: vec![(4, 10)]
                    },
                    GrepLine {
                        line: 3,
                        text: b"needle again".to_vec(),
                        marks: vec![(0, 6)]
                    },
                ],
            },
            GrepHit {
                path: b"<root>/sub/c.txt".to_vec(),
                rel: b"sub/c.txt".to_vec(),
                line: 3,
                text: b"find the needle".to_vec(),
                matches: 1,
                lines: vec![GrepLine {
                    line: 3,
                    text: b"find the needle".to_vec(),
                    marks: vec![(9, 15)]
                }],
            },
        ]
    );
    assert_eq!(
        (
            o.files,
            o.bytes,
            o.skipped_binary,
            o.truncated,
            o.stopped.clone()
        ),
        (4, 92, 1, false, None)
    );
    for broken in [
        {
            let mut g = golden();
            g.as_object_mut().unwrap().remove("files");
            g
        },
        {
            let mut g = golden();
            g["stopped"] = serde_json::json!("sideways");
            g
        },
        {
            let mut g = golden();
            g["hits"][0]["line"] = serde_json::json!("2");
            g
        },
    ] {
        assert!(decode_grep(&broken).is_err(), "该读不懂的读懂了：{broken}");
    }
    // 上界到了 ⇒ 总述里说出来（停在哪一道）。
    let mut cut = o.clone();
    cut.truncated = true;
    cut.stopped = Some("bytes".into());
    assert_eq!(
        stopped_line(&cut),
        Some(copy_text("rsFilewinGrep.summary.stoppedBytes", &[]))
    );
    assert_eq!(stopped_line(&o), None);
    // 总述：几处 · 几个文件（处数是每份命中行数之和）；跳过的另起一段，详情分行。
    assert_eq!(
        summary_line(&o),
        copy_core::copy_text("rsFilewinGrep.summary.line", &[("m", "3"), ("n", "2")])
    );
    assert_eq!(
        skipped_line(&o),
        Some(copy_core::copy_text("rsFilewinGrep.summary.skipped", &[("k", "1")]).to_string())
    );
    assert_eq!(
        skipped_detail(&o),
        vec![copy_core::copy_text("rsFilewinGrep.detail.binary", &[("n", "1")]).to_string()]
    );
}

#[tokio::test]
async fn a_search_asks_the_backend_about_this_directory_and_puts_the_hits_on_the_frame() {
    let mut be = FakeBackend::new(&[CMD_GREP, "files-ls"], Declared::default());
    be.grep_reply = Some(golden());
    let wired = testing::wire_up("files3-grep-frame", be).await;
    let ctx = ctx_ready();
    let mut w = testing::window_on(&wired, "<root>");
    w.set_grep_query("needle");
    let before = w.grep.rounds();
    assert!(w.fire_grep(None), "没发出去");
    settle(&w.grep, before, "按内容搜那一趟").await;
    assert_eq!(wired.count(CMD_GREP), 1);
    let log = wired.log.lock().unwrap().clone();
    let asked = log.iter().find(|r| r["cmd"] == CMD_GREP).unwrap();
    assert_eq!(
        asked["args"],
        serde_json::json!({ "path": "<root>", "needle": "needle" })
    );
    let painted = testing::frame_text(&ctx, &mut w, Vec::new());
    let want_rows: Vec<String> = decode_grep(&golden())
        .unwrap()
        .hits
        .iter()
        .map(GrepHit::display)
        .collect();
    for row in &want_rows {
        assert!(
            painted.contains(row),
            "帧上没有这一条命中：{row}\n帧上：{painted:?}"
        );
    }
    let summary = summary_line(&decode_grep(&golden()).unwrap());
    assert!(painted.contains(&summary), "帧上没有总述那一行：{summary}");
}

#[test]
fn clicking_a_hit_goes_to_its_directory_and_highlights_it() {
    let mut w = crate::shell::FileWindow::seeded(
        crate::source::Source::remote(String::from("files3-grep-jump")),
        "<root>".to_string(),
        None,
        Vec::<crate::source::Row>::new(),
    );
    w.set_grep_query("needle");
    let (mine, _t) = w.grep.start("needle");
    assert!(w.grep.store_if_current(mine, decode_grep(&golden())));
    assert!(w.showing_grep());
    assert!(!w.jump_to_grep_hit(9), "越界那一下不许动");
    assert_eq!(w.cwd_path().shown, "<root>");
    assert!(w.jump_to_grep_hit(1));
    assert_eq!(w.cwd_path().shown, "<root>/sub");
    assert_eq!(w.reveal_name(), Some("c.txt"));
    assert!(
        !w.showing_grep(),
        "跳过去之后屏幕该换回目录列表（那一行亮着）"
    );
}

#[tokio::test]
async fn stop_withdraws_the_search_in_flight() {
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    let mut be = FakeBackend::new(&[CMD_GREP], Declared::default()).holding_grep(gate.clone());
    be.grep_reply = Some(golden());
    let wired = testing::wire_up("files3-grep-stop", be).await;
    let mut w = testing::window_on(&wired, "<root>");
    w.set_grep_query("needle");
    let before = w.grep.rounds();
    assert!(w.fire_grep(None));
    // 等它真的到了后端那一侧（扣着不回）。
    for _ in 0..600 {
        if wired.count(CMD_GREP) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(w.grep.is_running());
    assert!(w.grep.stop(), "在飞那一趟没有撤单手柄");
    settle(&w.grep, before, "撤掉那一趟").await;
    assert!(!w.grep.is_running());
    let s = w.grep.shown();
    assert!(s.outcome.is_none(), "撤了还收到了成品");
    assert!(s.notice.is_some(), "撤了却一句话都没说");
    gate.notify_one();
    // 正控：同一台、不撤 ⇒ 放行之后收到命中。
    let before = w.grep.rounds();
    assert!(w.fire_grep(None));
    for _ in 0..600 {
        if wired.count(CMD_GREP) == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    gate.notify_one();
    settle(&w.grep, before, "不撤那一趟").await;
    assert_eq!(w.grep.shown().outcome.map(|o| o.hits.len()), Some(2));
}
