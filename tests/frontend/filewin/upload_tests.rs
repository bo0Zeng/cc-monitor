//! 命令栏「上传」那颗按钮的判据（照稿 10-05：直接弹系统选文件框）。
//!
//! - 🔴 **挂载那一跳在执行链上**：真跑 `frame_body` ＋ 命令栏，合成一次点击点在画出来的「上传」上 ⇒ 选择框被问；
//!   选到的那一份走拖入那一条（真回环 ＋ 真钥匙 ＋ 合成对端上逐步读回：先问「那儿有没有东西」、人答覆盖、
//!   再开单 → 订阅 → 提交，提交带 `overwrite: true`）。
//!
//! ⚠ 买不到：真弹框（选择框是注入的假的）；真图形会话上的一次真点击（这里是合成事件）。

use super::*;
use crate::transfer::tests::{steps, xfer_rig, Ends};

/// 假选择框：恒答那一份本机文件。
struct One(std::path::PathBuf);

impl crate::picker::Picker for One {
    fn pick(&self, _kind: crate::picker::PickKind) -> crate::picker::PickFuture {
        let p = self.0.clone();
        Box::pin(async move { Some(vec![p]) })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clicking_upload_picks_a_file_and_runs_the_drop_path_over_the_channel() {
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
    let local = std::env::temp_dir().join(format!("ccm-f7c-upbtn-{}", std::process::id()));
    std::fs::write(&local, b"abc").unwrap();
    w.picker = std::sync::Arc::new(One(local.clone()));
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
        let out = ctx.run_ui(input, |ui| crate::chrome::testing::pane_with_chrome(ui, w));
        let p = crate::copy::testing::text_in_frame(&out);
        out.drop_without_applying_deltas();
        p
    };
    // ① 第一帧：命令栏上那颗「上传」画出来了。
    let first = painted(&mut w, Vec::new());
    let at = rects_of(&first, UPLOAD_LABEL.as_str());
    assert_eq!(at.len(), 1, "命令栏上「上传」不是恰好一颗：{at:?}");
    // ② 点它 ⇒ 系统选文件框（注入的）答那一份 ⇒ 下一帧接上、走拖入那一条。
    let _ = painted(&mut w, crate::rows::testing::click_at(at[0].center()));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !w.board.is_asking() {
            let _ = painted(&mut w, Vec::new());
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("同名那一问没摆出来 —— 没走拖入那一条");
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
