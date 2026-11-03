//! 资产域判据里引到上层的那几条：那一层是独立的包（`backend-assets`）、够不到上层 ⇒ 挂在主 crate。

// 命令表 · 分派那一层的手 · 子命令闸门 · 远端 CLI 面的 stdin 上限 · 金样里的码集合 == 登记表。
#[path = "../assets/on_upper_tests.rs"]
mod on_upper_tests;
// 设置窗配置文件那一页的五口（「等于」那一行要帧面宿主那一份 · 金样里的码集合要命令表）。
#[path = "../assets/aliases/page_tests.rs"]
mod aliases_page_tests;
