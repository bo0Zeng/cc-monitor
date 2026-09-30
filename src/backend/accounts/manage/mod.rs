//! **账号库管理**：建账号库 · 加号 / 删号 / 设默认 · 核对 · 修复 · 隔离 · 回滚 · 账号别名 —— 这台后端自己做，不起外部工具。
//!
//! 账号库就是盘上那一份（`~/.claude-accts/accounts.json` ＋ 每个号一个配置目录），原样接着用、不迁移。
//!
//! | 模块 | 管什么 | 碰不碰盘 |
//! |---|---|---|
//! | [`model`] | 清单的模型与读写（认不得的格原样留着）| 纯 |
//! | [`scan`] | 把账号库此刻的样子读成一份快照 | 只读 |
//! | [`layout`] | 快照 ＋ 意图 ⇒ 计划（建哪些目录 · 哪些链接 · 改哪些权限 · 清单改成什么）| 纯 |
//! | [`exec`] | 按计划落盘、先备份再改、按备份回滚 —— 每一步经文件管理面（`assets::door::Door`）| 写 |
//! | [`verify`] | 快照 ⇒ 核对报告（链接 · 权限 · 邮箱）| 纯 |
//! | [`aliases`] | 账号表 ⇒ 每个号一条 `<名>cc` 别名，并进用户那份别名清单 | 纯 |
//! | [`wire`] | 入参严格收 → 快照 → 计划 → 预演或执行 | 经上面几块 |
//!
//! 平台差异（符号链接 · 权限位 · 这台做不做得了多账号）只在 `platform::acct_view`。
//! 与上游选择（`accounts::upstream_select`）零引用：建 API 号时写 key 那一步、改完账号表后重写别名文件那一步，
//! 由帧面宿主 `faces/accounts_face.rs` 接起来。

pub(crate) mod aliases;
pub(crate) mod exec;
pub(crate) mod layout;
pub(crate) mod model;
pub(crate) mod scan;
pub(crate) mod verify;
pub(crate) mod wire;
