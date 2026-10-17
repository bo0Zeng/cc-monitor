//! 每种帧（`hello` 除外）的形状指纹：那种帧结构的 TS 声明连同它引到的每个类型，FNV-1a 32 位，8 位十六进制（[`crate::fingerprint`]）。
//! 🔴 由判据 `frame::tests::the_frames_table_is_what_the_types_say` 从类型现算写出（不一致就重写这份并红）：别手改。

/// `(kind, 形状指纹)`，按 kind 排。
pub const FRAMES: &[(&str, &str)] = &[
    ("cancelled", "59eb66d7"),
    ("changed", "11192005"),
    ("files_batch", "b343f828"),
    ("line", "0fc3f2f2"),
    ("link_data", "78319744"),
    ("link_end", "4d744cd6"),
    ("live_card", "4320b506"),
    ("overflow", "6c57a5a8"),
    ("probe", "f36b8ba6"),
    ("reply", "561634f0"),
    ("session_added", "4264b543"),
    ("session_branch", "8f2c5c0a"),
    ("session_file_gone", "0284f2c7"),
    ("session_file_reread", "661b822e"),
    ("session_primed", "84796fd7"),
    ("session_removed", "3b6c9126"),
    ("session_runs", "a72e3930"),
    ("session_state", "367bbea7"),
    ("session_status", "064c5537"),
    ("sessions_replayed", "039f74b7"),
    ("terminal_follow_end", "59702041"),
    ("terminal_screen", "a0f7a825"),
    ("transfer", "c408c696"),
];
