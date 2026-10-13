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
    ("session-interrupts", "6b0c632f"),
    ("session-new", "af0ce8f1"),
    ("session-new-dir", "2c86c180"),
    ("session-new-facts", "eb214f62"),
    ("session-restart", "d223b9ce"),
    ("session-say", "461750a1"),
    ("session-terminals", "7c85cbeb"),
    ("sessions-start", "264d59fb"),
    ("sessions-stop", "0a1aa1a6"),
    ("sessions-where", "8c20eb86"),
    ("terminal-input", "60ebb8d3"),
    ("terminal-name-mint", "f0db66f2"),
    ("terminal-preview", "d60b74d2"),
    ("terminal-processes", "abba2221"),
    ("terminals-list", "09490ec0"),
];

/// 这条命令在本份编进来的形状指纹；不是 typed 命令 ⇒ `None`。
pub fn shape_of(cmd: &str) -> Option<&'static str> {
    SHAPES.iter().find(|(c, _)| *c == cmd).map(|(_, h)| *h)
}
