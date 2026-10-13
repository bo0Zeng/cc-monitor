//! 每条 typed 命令的形状指纹：入参 · 回包的 TS 声明（连同它们引到的每个类型）＋ 错误码，合起来的 FNV-1a 32 位，8 位十六进制。
//! 🔴 由后端判据 `stream::inbound::typed::tests::the_shapes_table_is_what_the_types_say` 从类型现算写出（不一致就重写这份并红）：别手改。

/// `(命令, 形状指纹)`，按命令名排。
pub const SHAPES: &[(&str, &str)] = &[
    ("history-search", "84a3f656"),
    ("history-search-merge", "32be9716"),
    ("kill", "6de86725"),
    ("launch", "59041b9e"),
    ("launch-local", "d359d78c"),
    ("launch-render-cli", "7fc9d2a8"),
    ("rotation-day-paint", "02a0b275"),
    ("rotation-facts", "978176bb"),
    ("rotation-for-new", "ff386b52"),
    ("rotation-revert", "508e5fa9"),
    ("rotation-save", "730685de"),
    ("rotation-settings", "9874d7a8"),
    ("rotation-strategies", "b0cb062e"),
    ("rotation-strategy-add", "393f19ea"),
    ("rotation-strategy-copy", "1f006aae"),
    ("rotation-strategy-delete", "2a779c1a"),
    ("rotation-strategy-rename", "55c18f72"),
    ("rotation-strategy-tz", "68a1b5cf"),
    ("rotation-temp", "89510195"),
    ("rotation-try", "251fe90a"),
    ("rotation-use", "671580e6"),
    ("rotation-use-now", "c3bb1f44"),
    ("rotation-view", "cfd5c25f"),
    ("session-interrupts", "6b0c632f"),
    ("session-new", "7fbad542"),
    ("session-new-dir", "2c86c180"),
    ("session-new-facts", "aed6893e"),
    ("session-restart", "d223b9ce"),
    ("session-terminals", "7c85cbeb"),
    ("sessions-start", "264d59fb"),
    ("sessions-stop", "0a1aa1a6"),
    ("sessions-where", "8c20eb86"),
    ("terminal-input", "b2a383a2"),
    ("terminal-name-mint", "f0db66f2"),
    ("terminal-preview", "d60b74d2"),
    ("terminal-processes", "abba2221"),
    ("terminals-list", "09490ec0"),
];

/// 这条命令在本份编进来的形状指纹；不是 typed 命令 ⇒ `None`。
pub fn shape_of(cmd: &str) -> Option<&'static str> {
    SHAPES.iter().find(|(c, _)| *c == cmd).map(|(_, h)| *h)
}
