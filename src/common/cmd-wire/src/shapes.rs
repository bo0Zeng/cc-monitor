//! 每条 typed 命令的形状指纹：入参 · 回包的 TS 声明（连同它们引到的每个类型）＋ 错误码，合起来的 FNV-1a 32 位，8 位十六进制。
//! 🔴 由后端判据 `stream::inbound::typed::tests::the_shapes_table_is_what_the_types_say` 从类型现算写出（不一致就重写这份并红）：别手改。

/// `(命令, 形状指纹)`，按命令名排。
pub const SHAPES: &[(&str, &str)] = &[
    ("history-search", "84a3f656"),
    ("history-search-merge", "32be9716"),
    ("rotation-facts", "978176bb"),
    ("rotation-try", "5ab86978"),
    ("terminal-name-mint", "f0db66f2"),
];

/// 这条命令在本份编进来的形状指纹；不是 typed 命令 ⇒ `None`。
pub fn shape_of(cmd: &str) -> Option<&'static str> {
    SHAPES.iter().find(|(c, _)| *c == cmd).map(|(_, h)| *h)
}
