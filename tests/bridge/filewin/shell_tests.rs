use super::*;

/// 合成一份远端配置。**全字段合成**，不读任何真配置 ——
/// `host` 用 `.invalid`（RFC 2606 保留），确保就算有人不小心让它真去连，
/// DNS 也解不出来。
fn synth_cfg(label: &str) -> crate::ssh_source::RemoteConfig {
    crate::ssh_source::RemoteConfig {
        host: "example.invalid".into(),
        label: label.into(),
        port: 22,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/nonexistent/cc-monitor-backend".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// 骨架真的立得起来：本机源、列一个真目录、状态进得去。
/// ⚠ **这条不开窗**（本机无图形会话）—— 它买的是「窗口状态机与数据面接得上」。
#[test]
fn a_local_window_loads_its_directory_without_a_display() {
    let dir = std::env::temp_dir().join(format!("ccm-filewin-shell-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("f.txt"), b"xyz").unwrap();

    let w = FileWindow::new(Source::Local, dir.to_string_lossy().to_string(), None);
    let rows = w.rows.lock().unwrap().clone();
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["sub", "f.txt"]
    );
    assert!(w.error.lock().unwrap().is_none());
    std::fs::remove_dir_all(&dir).ok();
}

/// 远端源拿不到运行时就**出声**，不假装列了个空目录。
#[test]
fn a_remote_window_without_a_runtime_says_so_instead_of_showing_an_empty_dir() {
    let w = FileWindow::new(
        Source::Remote(Box::new(synth_cfg("synthetic-origin"))),
        "/tmp".into(),
        None,
    );
    assert!(w.rows.lock().unwrap().is_empty());
    let e = w.error.lock().unwrap().clone();
    assert!(e.is_some(), "没有运行时却没报错 —— 那是静默的空列表");
}

/// 反空真：错误目录必须留下错误，不是一个空列表。
#[test]
fn a_bad_local_path_surfaces_an_error() {
    let w = FileWindow::new(
        Source::Local,
        "/definitely/not/a/real/path/9f3a".into(),
        None,
    );
    assert!(w.rows.lock().unwrap().is_empty());
    assert!(w.error.lock().unwrap().is_some());
}

/// `Source::label()` 是窗口标题的来源，别让它回空串。
#[test]
fn every_source_has_a_non_empty_label() {
    assert_eq!(Source::Local.label(), "本机");
    assert_eq!(
        Source::Remote(Box::new(synth_cfg("tagged"))).label(),
        "tagged"
    );
    // `label` 为空时回退到 `host`（`RemoteConfig::origin_label` 的契约）。
    let mut anon = synth_cfg("");
    anon.label.clear();
    assert_eq!(Source::Remote(Box::new(anon)).label(), "example.invalid");
}
