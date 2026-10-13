//! 暂存区里上传件的命名：造键的一侧（本机传输台 `control/transfer.rs::staging_key`，原生那一块）与
//! 提交的一侧（那台后端的 `control/files_commit.rs`，文件管理那一块）认同一个键长、同一个后缀。
//! 两块之间零互相依赖（`files/module_boundary_guard.rs`），共用的只许住 `common/`。
//! 暂存区本身的落点（家目录相对）住契约 crate `relay_route_core::STAGING_DIR_REL`。

/// 暂存件的键长（十六进制位数）。
pub const KEY_LEN: usize = 32;

/// 暂存件的后缀。
pub const PART_SUFFIX: &str = ".part";
