//! `filewin/picker.rs` 的判据 —— **系统的选文件框 / 存盘框**（照稿 10-05：上传 · 下载都直接弹系统框，窗口自己不再问）。
//!
//! | 判据 | 钉的那一形 | 两侧异源在哪 |
//! |---|---|---|
//! | [`upload_opens_the_file_picker_and_the_picked_files_go_the_drop_way`] | 命令栏「上传」⇒ 选择框被问「选文件」；选到的那几份走拖入那一条（「进度」表里多一行上传）；没选 ⇒ 什么都不起、不出声 | 假选择框记下被问了什么 |
//! | [`download_opens_the_save_box_with_a_legal_name_and_starts_the_pull_there`] | 「下载」⇒ 选择框被问「存」、建议名 ＝ 那一行的名字；选到落点 ⇒ 那一趟下载起在那个落点；没选 ⇒ 什么都不起 | 同上 |
//! | [`a_pick_that_lands_before_the_next_frame_is_taken_by_that_frame`] | 结局先落下（敲了窗口）⇒ 下一帧生产那一处把它接上、看板清空 | 「落下」认生产那一下 `request_repaint` |
//!
//! ⚠ 买不到：真弹框（本机无图形会话；测试构建里缺省的选择框一律答「没选」）· 真 Windows。

use super::*;
use crate::find::testing::{window_on, wire_up, Declared, FakeBackend};

/// 假选择框：回一份定好的结局，并记下被问了什么。
struct Fake {
    answer: Option<Vec<std::path::PathBuf>>,
    asked: std::sync::Arc<std::sync::Mutex<Vec<PickKind>>>,
    /// 给了 ⇒ 结局扣到它被 `notify_one` 为止（把「结局在点下去那一帧之后才落下」那一序钉死）。
    gate: Option<std::sync::Arc<tokio::sync::Notify>>,
}

impl Picker for Fake {
    fn pick(&self, kind: PickKind) -> PickFuture {
        self.asked.lock().unwrap().push(kind);
        let (a, gate) = (self.answer.clone(), self.gate.clone());
        Box::pin(async move {
            if let Some(g) = gate {
                g.notified().await;
            }
            a
        })
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

fn uploads(w: &crate::shell::FileWindow) -> usize {
    w.progress
        .jobs()
        .iter()
        .filter(|j| matches!(j.trip, crate::progress::Trip::Upload { .. }))
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upload_opens_the_file_picker_and_the_picked_files_go_the_drop_way() {
    let wired = wire_up("w5-pick-up", FakeBackend::new(&[], Declared::default())).await;
    let mut w = window_on(&wired, "/srv");
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    w.picker = std::sync::Arc::new(Fake {
        answer: Some(vec!["/home/u/a.txt".into(), "/home/u/b.txt".into()]),
        asked: asked.clone(),
        gate: None,
    });
    w.run_command(crate::chrome::Cmd::Upload, None);
    settle(&mut w).await;
    assert_eq!(asked.lock().unwrap().clone(), vec![PickKind::OpenFiles]);
    assert_eq!(uploads(&w), 1, "选到了两份，却没走拖入那一条");
    // 没选（取消）⇒ 什么都不起、不出声。
    w.picker = std::sync::Arc::new(Fake {
        answer: None,
        asked: asked.clone(),
        gate: None,
    });
    assert!(w.start_pick(Purpose::Upload, None));
    settle(&mut w).await;
    assert_eq!(uploads(&w), 1, "没选却又起了一趟");
    assert!(w.listing.error.lock().unwrap().is_none(), "没选却出了声");
}

/// 结局在下一帧**之前**就落下 ⇒ 那一帧自己把它接上（Windows runner 上的时序）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pick_that_lands_before_the_next_frame_is_taken_by_that_frame() {
    let wired = wire_up("w5-pick-early", FakeBackend::new(&[], Declared::default())).await;
    let mut w = window_on(&wired, "/srv");
    w.picker = std::sync::Arc::new(Fake {
        answer: Some(vec!["/home/u/a.txt".into()]),
        asked: Default::default(),
        gate: None,
    });
    let knock = egui::Context::default();
    let landed = std::sync::Arc::new(tokio::sync::Notify::new());
    let l = landed.clone();
    knock.set_request_repaint_callback(move |_| l.notify_one());
    assert!(w.start_pick(Purpose::Upload, Some(knock)));
    tokio::time::timeout(std::time::Duration::from_secs(3), landed.notified())
        .await
        .expect("等了 3 秒选择框的结局还没落下");
    assert_eq!(uploads(&w), 0, "还没画帧就起了上传");
    crate::find::testing::frame_text(&egui::Context::default(), &mut w, Vec::new());
    assert_eq!(uploads(&w), 1, "结局落下之后的那一帧没把它接上");
    assert!(!w.settle_pick(), "那一帧取走之后看板上还留着一份");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn download_opens_the_save_box_with_a_legal_name_and_starts_the_pull_there() {
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
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    w.picker = std::sync::Arc::new(Fake {
        answer: Some(vec!["/home/u/下载/report.pdf".into()]),
        asked: asked.clone(),
        gate: None,
    });
    assert!(w.begin_pull(0), "「下载」没起系统存盘框");
    assert_eq!(
        w.pull_want().cloned(),
        Some(("/srv/report.pdf".to_string(), "report.pdf".to_string()))
    );
    settle(&mut w).await;
    assert_eq!(
        asked.lock().unwrap().clone(),
        vec![PickKind::SaveFile {
            suggested_name: "report.pdf".into()
        }]
    );
    let dests: Vec<String> = w
        .progress
        .jobs()
        .iter()
        .filter_map(|j| match &j.trip {
            crate::progress::Trip::Download { dest, .. } => Some(dest.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        dests,
        vec!["/home/u/下载/report.pdf".to_string()],
        "那一趟没起在选到的落点上"
    );
    assert!(w.pull_want().is_none());
    // 没选 ⇒ 什么都不起。
    w.picker = std::sync::Arc::new(Fake {
        answer: None,
        asked,
        gate: None,
    });
    assert!(w.begin_pull(0));
    settle(&mut w).await;
    assert_eq!(w.progress.jobs().len(), 1, "没选却又起了一趟");
}
