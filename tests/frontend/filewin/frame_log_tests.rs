//! 帧日志的判据：关着时不写一个字节、不读画面；开着时每帧一行，画面不动就说没变、动了就说变了。

use super::*;

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "filewin-frame-log-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建临时目录");
    d
}

fn input(pointer: Option<egui::Pos2>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 200.0),
        )),
        events: pointer
            .map(|p| vec![egui::Event::PointerMoved(p)])
            .unwrap_or_default(),
        ..Default::default()
    }
}

/// 一帧：一行字 ＋ 指针在左半边时多一块底色（「悬停高亮」那一形，只改画面不改布局）。
fn frame(ctx: &egui::Context, log: &mut FrameLog, pointer: Option<egui::Pos2>, word: &str) {
    let out = ctx.run_ui(input(pointer), |ui| {
        let r = ui.label(word);
        if pointer.is_some_and(|p| p.x < 200.0) {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(50.0, 20.0)),
                0.0,
                egui::Color32::RED,
            );
        }
        let _ = r;
        log.record(ui.ctx());
    });
    out.drop_without_applying_deltas();
}

#[test]
fn off_by_default_writes_nothing() {
    let ctx = egui::Context::default();
    let mut log = FrameLog::off();
    assert!(!log.is_on());
    for _ in 0..3 {
        frame(&ctx, &mut log, None, "a");
    }
    assert_eq!(log.frames(), 0, "关着的帧日志不该数帧（也就没读画面）");
}

#[test]
fn each_frame_is_one_line_saying_whether_the_picture_and_the_text_layout_moved() {
    let dir = scratch("lines");
    let file = dir.join("frames.log");
    let ctx = egui::Context::default();
    let mut log = FrameLog::to_file(&file).expect("开得了日志文件");
    frame(&ctx, &mut log, Some(egui::pos2(300.0, 50.0)), "a"); // 第 1 帧：没有上一帧可比
    frame(&ctx, &mut log, Some(egui::pos2(300.0, 50.0)), "a"); // 同样子
    frame(&ctx, &mut log, Some(egui::pos2(100.0, 50.0)), "a"); // 多一块底色：画面变、字的布局不变
    frame(&ctx, &mut log, Some(egui::pos2(100.0, 50.0)), "ab"); // 字变了：布局变
    drop(log);
    let text = std::fs::read_to_string(&file).expect("读回日志");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "四帧该是四行：{text}");
    let field = |l: &str, k: &str| -> String {
        l.split(' ')
            .find_map(|w| w.strip_prefix(&format!("{k}=")))
            .unwrap_or_else(|| panic!("这一行没有 {k}=：{l}"))
            .to_string()
    };
    assert_eq!(field(lines[0], "paint"), "first");
    assert_eq!(field(lines[1], "paint"), "same");
    assert_eq!(field(lines[1], "layout"), "same");
    assert_eq!(field(lines[2], "paint"), "changed");
    assert_eq!(field(lines[2], "layout"), "same");
    assert_eq!(field(lines[3], "layout"), "changed");
    assert_eq!(field(lines[2], "pointer"), "100.0,50.0");
    for l in &lines {
        for k in ["frame", "t_ms", "pass", "events", "ppp", "why"] {
            let _ = field(l, k);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
