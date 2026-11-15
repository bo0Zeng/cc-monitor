//! 命令栏「上传」那颗按钮的判据（直接弹系统选文件框）。
//!
//! - 🔴 **挂载那一跳在执行链上**：真跑 `frame_body` ＋ 命令栏，合成一次点击点在画出来的「上传」上 ⇒ 选择框被问；
//!   选到的那一份走拖入那一条（真回环 ＋ 合成对端上逐步读回：一组寻址本机的 `transfer-copy`、同名由对端探了回 `exists` ⇒
//!   冲突框问一次、人答覆盖 ⇒ 同一组带 `decide` 再发；窗口自己一次 `files-stat` 都不问、一次提交都不发）。
//!
//! ⚠ 买不到：真弹框（选择框是注入的假的）；真图形会话上的一次真点击（这里是合成事件）。

use super::*;

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
    let (mut w, log) = crate::transfer::tests::one_line_window_with(Default::default(), true);
    let machine = w.source.label();
    let local = std::env::temp_dir().join(format!("ccm-f7c-upbtn-{}", std::process::id()));
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
    let asking = tokio::time::timeout(guard_core::wall::CEILING, async {
        loop {
            let ask = w.my_trips().into_iter().find_map(|j| match j.trip {
                crate::progress::Trip::Batch { board, .. } if board.is_asking() => Some(board),
                _ => None,
            });
            if let Some(b) = ask {
                return b;
            }
            let _ = painted(&mut w, Vec::new());
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("同名那一问没摆出来 —— 没走拖入那一条");
    assert_eq!(asking.conflicts().len(), 1, "冲突框照对端的同名清单画");
    assert!(asking.answer(Some(cmd_wire::batch::Decide::Overwrite), false));
    let sent = |log: &std::sync::Mutex<Vec<(String, String, serde_json::Value)>>| -> Vec<(String, serde_json::Value)> {
        log.lock()
            .unwrap()
            .iter()
            .filter(|(_, s, _)| s == crate::transfer::CMD_TRANSFER || s.starts_with("files-commit") || s == "files-stat")
            .map(|(o, s, v)| (format!("{o} {s}"), v.clone()))
            .collect()
    };
    tokio::time::timeout(guard_core::wall::CEILING, async {
        while sent(&log).len() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("答完冲突框没再发");
    let got = sent(&log);
    let local_s = local.to_string_lossy().into_owned();
    let want = |decide: Option<serde_json::Value>| {
        let mut v = serde_json::json!({
            "from": cmd_wire::LOCAL, "to": machine, "items": [{"path": local_s}], "to_dir": "/srv",
        });
        if let Some(d) = decide {
            v["decide"] = d;
        }
        v
    };
    let strip = |mut v: serde_json::Value| {
        assert!(v["ticket"].is_string(), "没带票：{v}");
        v.as_object_mut().unwrap().remove("ticket");
        v
    };
    let addressed = format!("{} {}", cmd_wire::LOCAL, crate::transfer::CMD_TRANSFER);
    assert_eq!(
        got.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
        [addressed.clone(), addressed],
        "窗口不再自己探同名、不提交：一组一条，答过冲突框带 `decide` 再发一次"
    );
    assert_eq!(got[0].1["ticket"], got[1].1["ticket"], "再发换了票");
    assert_eq!(strip(got[0].1.clone()), want(None));
    assert_eq!(
        strip(got[1].1.clone()),
        want(Some(serde_json::json!({"0": "overwrite"})))
    );
}
