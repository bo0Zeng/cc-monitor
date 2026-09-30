//! 〔W5-FILES · 第五波〕`filewin/size.rs` 的判据 —— **窗口上的「算大小」**。
//!
//! 要求住址：`设计/60 §6.2`「复制目录 · 批量复制 · 跨机复制 · **算目录大小** · 解压 ——『在那台机器上就地做』的这一类还没有命令」
//! ＋ 用户 V45「我能连 ssh 对机器文件进行什么操作，后端就应该能进行什么操作」＋「我觉得要零流量」。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`a_reply_missing_any_field_is_a_loud_failure`] | 应答少任何一格 ⇒ 失败且点名那一格（不补 0） | 字段表手写 |
//! | [`the_outcome_line_says_the_numbers_as_reported`] | 一句话逐字等于手写期望（链接 / 挂载点 / 读不进 非零才说） | 期望手写 |
//! | [`size_on_the_menu_walks_to_the_wire_for_each_picked_item`] | 选两项点「算大小」⇒ 线上恰两行 `files-size`、路径逐格相等；一项被拒 ⇒ 结局里带原话、另一项照算 | 实得是合成后端真收到的 |

use super::*;
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend};

fn full() -> serde_json::Value {
    serde_json::json!({
        "path": "/srv/a", "bytes": 2048, "files": 3, "dirs": 2, "links": 1,
        "other": 0, "skipped_mounts": 4, "unreadable_dirs": 5,
    })
}

#[test]
fn a_reply_missing_any_field_is_a_loud_failure() {
    assert!(sized_from_reply("a", &full()).is_ok());
    for k in [
        "bytes",
        "files",
        "dirs",
        "links",
        "skipped_mounts",
        "unreadable_dirs",
    ] {
        let mut m = full();
        m.as_object_mut().unwrap().remove(k);
        let e = sized_from_reply("a", &m).expect_err("少了一格竟然解析成功");
        assert!(e.contains(k), "报错没点名 `{k}`：{e}");
    }
}

#[test]
fn the_outcome_line_says_the_numbers_as_reported() {
    let s = sized_from_reply("proj", &full()).unwrap();
    assert_eq!(
        size_line(&s),
        "proj：2.0 K（2048 字节），3 个文件、2 个目录，1 条链接没算进去，4 个目录在另一个盘上，没进去，5 个目录读不进去"
    );
    let quiet = Sized {
        links: 0,
        skipped_mounts: 0,
        unreadable_dirs: 0,
        ..s.clone()
    };
    assert_eq!(
        size_line(&quiet),
        "proj：2.0 K（2048 字节），3 个文件、2 个目录"
    );
    assert_eq!(
        outcome_text(&[Ok(quiet), Err(("x".into(), "原话".into()))]),
        "proj：2.0 K（2048 字节），3 个文件、2 个目录；x 算不出来：原话"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn size_on_the_menu_walks_to_the_wire_for_each_picked_item() {
    use crate::select::{Action, Intent};
    let wired = wire_up(
        "w5-size",
        FakeBackend::new(&["files-size"], Declared::default()),
    )
    .await;
    let mut w = window_on(&wired, "/srv");
    let row = |name: &str, is_dir: bool| -> crate::source::Listed {
        crate::source::Row {
            name: name.into(),
            path: format!("/srv/{name}"),
            is_dir,
            size: 0,
            lossy_name: false,
        }
        .into()
    };
    *w.listing.rows.lock().unwrap() = vec![row("data", true), row("refuse-me", true)];
    w.apply_intent(Intent::SelectAll, 0.0, None);
    assert!(
        w.perform(Action::Size, None),
        "算大小没起来：{:?}",
        w.key_notice()
    );
    for _ in 0..600 {
        if w.size_board.rounds() > 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(w.size_board.rounds(), 1, "等了 3 秒那一摞还没回话");
    let sent: Vec<serde_json::Value> = wired
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["cmd"] == "files-size")
        .map(|r| r["args"].clone())
        .collect();
    assert_eq!(
        sent,
        vec![
            serde_json::json!({"path": "/srv/data"}),
            serde_json::json!({"path": "/srv/refuse-me"}),
        ]
    );
    let said = outcome_text(&w.size_board.last().expect("没有结局"));
    assert!(
        said.starts_with("data：2.0 K（2048 字节），3 个文件、2 个目录，1 条链接没算进去"),
        "{said}"
    );
    assert!(
        said.contains("refuse-me 算不出来：") && said.contains("读不到"),
        "被拒那一项没带原话：{said}"
    );
}
