//! 判据共用的帧夹具：同一版后端的 hello 与一条最少格的活会话宣告。

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

/// 判据用的一条活会话宣告：后端会写的最少那几格（字 · 语气 · 序 · 能不能换 · 名字 · pid），`extra` 里的格盖上去。
pub(crate) fn session_added(sid: &str, extra: serde_json::Value) -> cmd_wire::frame::SessionAdded {
    let mut v = serde_json::json!({
        "sid": sid,
        "agent_kind": "claude",
        "activity_text": "T",
        "activity_tone": "now",
        "activity_order": 1,
        "switch": {"model": {"can": "yes"}, "permission": {"can": "yes"}},
        "label": "L",
        "pid": 1,
    });
    if let (Some(o), serde_json::Value::Object(e)) = (v.as_object_mut(), extra) {
        o.extend(e);
    }
    serde_json::from_value(v).expect("判据造的宣告解不开")
}
