//! **`pub` 项要有产品读者** —— 后端 · 壳 · 共享 crate 里每一个 `pub` 项，除定义处之外在生产代码里至少被读一次；
//! 只被测试读的算零读者：要么删，要么收进 `#[cfg(test)]`（明说它只给测试用），要么登记进豁免表并写清为什么。
//!
//! 编译器的 `dead_code` 看不见这一族：库包里的 `pub` 项算对外导出，没人读也不出声。
//!
//! 人群与读者面、豁免表、正控都在测试文件里。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/pub_reader_registry_tests.rs"]
mod tests;
