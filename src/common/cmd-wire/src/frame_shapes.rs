//! 每种帧（`hello` 除外）的形状指纹：那种帧结构的 TS 声明连同它引到的每个类型，FNV-1a 32 位，8 位十六进制（[`crate::fingerprint`]）。
//! 🔴 由判据 `frame::tests::the_frames_table_is_what_the_types_say` 从类型现算写出（不一致就重写这份并红）：别手改。

/// `(kind, 形状指纹)`，按 kind 排。
pub const FRAMES: &[(&str, &str)] = &[
    ("cancelled", "59eb66d7"),
    ("changed", "0668a219"),
    ("files_batch", "b343f828"),
    ("line", "2a715d57"),
    ("live_card", "d72cedd2"),
    ("overflow", "6c57a5a8"),
    ("probe", "9b42ff05"),
    ("relayed", "a34c89c4"),
    ("reply", "96e8c938"),
    ("session_added", "05dd309c"),
    ("session_branch", "8f2c5c0a"),
    ("session_file_gone", "0284f2c7"),
    ("session_file_reread", "bdab1d4a"),
    ("session_left", "f9746125"),
    ("session_primed", "84796fd7"),
    ("session_runs", "05c759bd"),
    ("session_status", "0a9c0bc7"),
    ("sessions_replayed", "039f74b7"),
    ("terminal_follow_end", "59702041"),
    ("terminal_screen", "8dc76c06"),
    ("transfer", "25dfe029"),
];
