//! 〔F7c · 第三波 · 2026-09-24〕工具栏「上传」那一问的判据。
//!
//! # 买到什么
//!
//! - 判定两向（纯函数，`is_file` 注进来）：空框拒 · 有一行不是文件 ⇒ **整批**拒并点名 · 合法 ⇒ 远端落点
//!   照拖入那一条拼、覆盖标记一律 `false`（人还没被问过）。
//! - 状态机：被拒 ⇒ 框留着、字不丢；合法 ⇒ 收掉并交出那一摞；取消 ⇒ 什么都不起。
//! - 🔴 **挂载那一跳在执行链上**：真跑 `frame_body`，合成一次点击点在画出来的「上传」上 ⇒ 那一问开了；
//!   填好路径、点「确定」⇒ 走的是拖入那一条（真回环 ＋ 真钥匙 ＋ 合成对端上逐步读回：
//!   先问「那儿有没有东西」、人答覆盖、再开单 → 订阅 → 提交，提交带 `overwrite: true`）。
//!
//! # 买不到什么
//!
//! - 原生选文件框（没有，理由住 `upload.rs` 头注 ②）；真图形会话上的一次真点击（这里是合成事件）。

use super::*;
use crate::transfer::tests::{steps, xfer_rig, Ends};

fn yes(_: &str) -> bool {
    true
}

/// ★ 空框（或全是空白行）⇒ 拒，说「是空的」。
#[test]
fn an_empty_box_is_refused_not_filled_in() {
    for t in ["", "   ", "\n\n  \n"] {
        assert_eq!(
            judge_upload(t, "/srv", yes),
            UploadVerdict::Rejected("要传的文件是空的".to_string())
        );
    }
}

/// ★ 有一行不是本机上的一份文件 ⇒ **整批**拒并点名那一行（起一半拒一半，用户分不清哪几件走了）。
#[test]
fn one_bad_line_refuses_the_whole_batch_and_names_it() {
    let v = judge_upload("/home/u/a.bin\n/home/u/nope\n/home/u/b.bin", "/srv", |p| {
        p != "/home/u/nope"
    });
    assert_eq!(
        v,
        UploadVerdict::Rejected("/home/u/nope 不是本机上的一份文件".to_string())
    );
}

/// ★ 合法 ⇒ 每一行一件，远端落点照拖入那一条拼（恒用 `/`），覆盖标记一律 `false`，空行丢掉。
#[test]
fn a_good_box_becomes_the_same_pending_items_a_drop_would() {
    let v = judge_upload(" /home/u/a.bin \n\n/home/u/b.txt\n", "/srv/data/", yes);
    let want = vec![
        crate::transfer::Pending::into_remote_dir("/home/u/a.bin", "/srv/data/").unwrap(),
        crate::transfer::Pending::into_remote_dir("/home/u/b.txt", "/srv/data/").unwrap(),
    ];
    assert_eq!(v, UploadVerdict::Go(want.clone()));
    assert!(want.iter().all(|p| !p.overwrite));
    assert_eq!(want[0].remote_path, "/srv/data/a.bin");
}

/// ★ 状态机：被拒 ⇒ 框留着、字不丢、那句话记着；合法 ⇒ 收掉并交出；取消 ⇒ 什么都不起。
#[test]
fn the_prompt_keeps_the_text_on_refusal_and_closes_on_success() {
    let mut p = UploadPrompt::default();
    assert!(!p.is_open());
    assert_eq!(p.confirm("/srv", yes), None, "没开的时候确定不该起东西");
    p.open();
    *p.text_mut().unwrap() = "/home/u/nope".to_string();
    assert_eq!(p.confirm("/srv", |_| false), None);
    assert!(p.is_open(), "被拒了，框却收了");
    assert_eq!(p.text(), Some("/home/u/nope"), "被拒了，用户敲的字丢了");
    assert!(p.refused().is_some_and(|r| r.contains("/home/u/nope")));
    let got = p.confirm("/srv", yes).expect("合法该交出那一摞");
    assert_eq!(got.len(), 1);
    assert!(!p.is_open());
    p.open();
    p.cancel();
    assert!(!p.is_open() && p.refused().is_none());
}

/// 生产那一侧的「是不是本机文件」恰好一处（同 `download::dest_exists` 那条纪律）。
#[test]
fn the_is_file_check_has_exactly_one_production_address() {
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/filewin/src/upload.rs"));
    assert_eq!(prod.matches(".is_file()").count(), 1);
    // 判定本身不碰盘（注入式，判据才走得到两支）。
    let at = prod.find("pub fn judge_upload(").expect("判定不在生产段里");
    let end = prod[at..]
        .find("\npub fn local_is_file")
        .map(|i| at + i)
        .expect("`local_is_file` 没紧跟在判定后面 —— 本条的窗口口径要重定");
    assert!(!prod[at..end].contains(".is_file()"), "判定自己碰盘了");
}

/// 🔴🔴 **挂载那一跳在执行链上**：真 `frame_body` ＋ 合成点击 ⇒ 开那一问；填好 ＋ 点「确定」⇒ 走拖入那一条，
/// 在真回环 ＋ 真钥匙 ＋ 合成对端上逐步读回。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clicking_upload_then_ok_runs_the_drop_path_over_the_channel() {
    use crate::copy::testing::rects_of;
    let (line, origin, log) = xfer_rig(Ends::Done).await;
    let cfg = origin.0.clone();
    let mut w = crate::shell::FileWindow::seeded(
        crate::source::Source::remote(cfg),
        "/srv".to_string(),
        tokio::runtime::Handle::try_current().ok(),
        Vec::<crate::source::Row>::new(),
    );
    w.attach_line(line);
    let ctx = egui::Context::default();
    let painted = |w: &mut crate::shell::FileWindow, ev: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            events: ev,
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| w.frame_body(ui));
        let p = crate::copy::testing::text_in_frame(&out);
        out.drop_without_applying_deltas();
        p
    };
    // ① 第一帧：工具栏上那颗「上传」画出来了。
    let first = painted(&mut w, Vec::new());
    let at = rects_of(&first, UPLOAD_LABEL.as_str());
    assert_eq!(at.len(), 1, "工具栏上「上传」不是恰好一颗：{at:?}");
    // ② 点它 ⇒ 那一问开了。
    let _ = painted(&mut w, crate::rows::testing::click_at(at[0].center()));
    assert!(w.upload.is_open(), "点了「上传」，那一问没开 —— 按钮没挂上");
    // ③ 填一个真的本机文件（判定要它真是文件），点「确定」。
    let local = std::env::temp_dir().join(format!("ccm-f7c-upbtn-{}", std::process::id()));
    std::fs::write(&local, b"abc").unwrap();
    *w.upload.text_mut().unwrap() = local.to_string_lossy().to_string();
    let modal = painted(&mut w, Vec::new());
    let ok = rects_of(&modal, "确定");
    assert_eq!(ok.len(), 1, "那一问上「确定」不是恰好一颗");
    let _ = painted(&mut w, crate::rows::testing::click_at(ok[0].center()));
    assert!(
        !w.upload.is_open(),
        "点了确定，那一问没收（被拒了？{:?}）",
        w.upload.refused()
    );
    // ④ 走的是拖入那一条：先问「那儿有没有东西」（合成对端答有）⇒ 看板上那一问 ⇒ 人答覆盖。
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !w.board.is_asking() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("覆盖那一问没摆出来 —— 没走拖入那一条");
    assert!(w.board.settle(true));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !steps(&log).iter().any(|s| s == "files-commit-upload") {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("一直没等到提交");
    let got: Vec<(String, serde_json::Value)> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(s, _)| s != "dropped")
        .cloned()
        .collect();
    let names: Vec<&str> = got.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(
        names,
        // 〔FILES2 · Q5〕开单之前先问一次那台后端的 `$HOME`（开单带上，传输台连上之后比 SFTP 起始目录）。
        [
            "files-stat",
            "files-home",
            "transfer-upload",
            "subscribe",
            "files-commit-upload"
        ]
    );
    assert_eq!(
        got[4].1["overwrite"],
        serde_json::json!(true),
        "人答了覆盖，提交却没带"
    );
    assert_eq!(got[4].1["root"], serde_json::json!("/srv"));
    let _ = std::fs::remove_file(&local);
}
