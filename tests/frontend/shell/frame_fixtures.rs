//! 判据用的几帧（壳按 `cmd-wire` 那一份类型解的形）：同一版后端的 hello · 一帧去向 · 一条活会话宣告。

/// 同一版的后端发的那一帧 hello：typed 命令与帧的形状指纹照本侧编进来的那两张表交（帧门全放行）。
pub(crate) fn our_hello(commands: &[&str]) -> crate::stream_source::Frame {
    cmd_wire::frame::Frame::Hello(cmd_wire::frame::Hello {
        build_id: "test".into(),
        host_arch: "x86_64".into(),
        commands: commands.iter().map(|s| s.to_string()).collect(),
        shapes: cmd_wire::shapes::SHAPES
            .iter()
            .map(|(c, h)| (c.to_string(), h.to_string()))
            .collect(),
        frames: cmd_wire::frame_shapes::FRAMES
            .iter()
            .map(|(k, h)| (k.to_string(), h.to_string()))
            .collect(),
        ..Default::default()
    })
}

/// 判据用的一份整脸（点与短名由调用方给，别的格取最少那一形）。
fn face(dot: &str, name: &str) -> serde_json::Value {
    let yes = serde_json::json!({"can": "yes"});
    serde_json::json!({
        "dot": dot, "live": {"is": "no"}, "settled": false, "name": name, "tip": "T", "line": "L", "tone": "plain", "order": 5,
        "can": {"close": {"can": "quiet"}, "resume": yes, "term": {"can": "yes", "tmux": false}, "reattach": yes, "model": yes, "permission": yes},
    })
}

/// 判据用的一帧去向（`state` 取线上那一形：`reconnectable` / `ended`），`text` 是那台写的短名。
pub(crate) fn session_state(sid: &str, state: &str, text: &str) -> cmd_wire::frame::SessionState {
    serde_json::from_value(serde_json::json!({
        "sid": sid,
        "state": state,
        "face": face("ended", text),
    }))
    .expect("判据造的去向解不开")
}

/// 判据用的一条活会话宣告：后端会写的最少那几格（整份脸 · 名字 · pid），`extra` 里的格盖上去。
pub(crate) fn session_added(sid: &str, extra: serde_json::Value) -> cmd_wire::frame::SessionAdded {
    let mut v = serde_json::json!({
        "sid": sid,
        "agent_kind": "claude",
        "label": "L",
        "face": face("running", "T"),
        "pid": 1,
    });
    if let (Some(o), serde_json::Value::Object(e)) = (v.as_object_mut(), extra) {
        o.extend(e);
    }
    serde_json::from_value(v).expect("判据造的宣告解不开")
}
