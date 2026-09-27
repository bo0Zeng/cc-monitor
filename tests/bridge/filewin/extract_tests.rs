//! 〔FILES2 · 第四波 · 2026-09-27〕`filewin/extract.rs` 的判据 —— **窗口上的「解压到这里」**。
//!
//! 要求住址：`设计/60 §6.2`「解压」· `§7` 第 9 条 Q3；主会话 09-27 按通行做法裁：「右键『解压到这里』……**撞名就问**；
//! 支持 zip · tar · tar.gz · tgz，其余格式说『不认这种包』」。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`extract_on_the_menu_asks_once_when_the_name_is_taken_and_only_then_sends_fresh`] | 撞名 ⇒ 摆一问；答「另起一个名字」⇒ 线上恰两行（第二行带 `fresh: true`）；答「不解了」⇒ 线上恰一行 | 实得是合成后端真收到的 |
//! | [`an_unknown_archive_says_so_and_a_directory_is_not_offered`] | 后端 `unsupported` 的原话进结局；目录那一行菜单上没有这一项 | 期望手写 |

use super::*;
use crate::filewin::find::testing::{window_on, wire_up, Declared, FakeBackend};

fn file_row(name: &str) -> crate::filewin::source::Listed {
    crate::filewin::source::Row {
        name: name.into(),
        path: format!("/srv/{name}"),
        is_dir: false,
        size: 0,
        lossy_name: false,
    }
    .into()
}

async fn settle(w: &crate::filewin::shell::FileWindow, want: u64) {
    for _ in 0..600 {
        if w.extract_board.rounds() >= want {
            return;
        }
        if w.extract_board.is_asking() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

fn sent(wired: &crate::filewin::find::testing::Wired) -> Vec<serde_json::Value> {
    wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == CMD_EXTRACT)
        .map(|r| r["args"].clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extract_on_the_menu_asks_once_when_the_name_is_taken_and_only_then_sends_fresh() {
    use crate::filewin::select::Action;
    for (answer, want_lines, want_path) in [(true, 2, Some("/srv/taken (2)")), (false, 1, None)] {
        let wired = wire_up(
            "fx-extract",
            FakeBackend::new(&[CMD_EXTRACT], Declared::default()),
        )
        .await;
        let mut w = window_on(&wired, "/srv");
        *w.listing.rows.lock().unwrap() = vec![file_row("taken.tar.gz")];
        w.apply_intent(crate::filewin::select::Intent::SelectAll, 0.0, None);
        assert!(
            w.perform(Action::Extract, None),
            "解压没起来：{:?}",
            w.key_notice()
        );
        settle(&w, 1).await;
        assert!(w.extract_board.is_asking(), "撞名却没问");
        assert!(w.busy_reason().is_some(), "摆着一问的标签却说没活");
        assert!(w.extract_board.settle(answer), "那一问收不了");
        settle(&w, 1).await;
        assert_eq!(w.extract_board.rounds(), 1, "答完之后那一趟没回话");
        let lines = sent(&wired);
        assert_eq!(lines.len(), want_lines, "线上行数不对：{lines:?}");
        assert_eq!(
            lines[0],
            serde_json::json!({"root": "/srv", "rel": "taken.tar.gz"})
        );
        if answer {
            assert_eq!(
                lines[1],
                serde_json::json!({"root": "/srv", "rel": "taken.tar.gz", "fresh": true})
            );
        }
        let (name, o) = w.extract_board.last().expect("没有结局");
        match (want_path, &o) {
            (Some(p), Outcome::Done(e)) => assert_eq!(e.path, p),
            (None, Outcome::Skipped(said)) => assert!(said.contains("已经在了"), "{said}"),
            _ => panic!("结局不对：{o:?}"),
        }
        assert_eq!(name, "taken.tar.gz");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_archive_says_so_and_a_directory_is_not_offered() {
    use crate::filewin::select::{actions_for, Action};
    let wired = wire_up(
        "fx-extract-bad",
        FakeBackend::new(&[CMD_EXTRACT], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/srv");
    *w.listing.rows.lock().unwrap() = vec![file_row("bad.rar")];
    w.apply_intent(crate::filewin::select::Intent::SelectAll, 0.0, None);
    assert!(w.perform(Action::Extract, None));
    settle(&w, 1).await;
    let (name, o) = w.extract_board.last().expect("没有结局");
    let said = outcome_text(&name, &o);
    assert!(
        said.starts_with("bad.rar 没解成：") && said.contains("不认这种包"),
        "后端原话没进结局：{said}"
    );
    let dir: crate::filewin::source::Listed = crate::filewin::source::Row {
        name: "d".into(),
        path: "/srv/d".into(),
        is_dir: true,
        size: 0,
        lossy_name: false,
    }
    .into();
    assert!(
        !actions_for(&[&dir]).contains(&Action::Extract),
        "目录上也给了解压"
    );
    assert!(
        actions_for(&[&file_row("f.zip")]).contains(&Action::Extract),
        "文件上没给解压"
    );
}
