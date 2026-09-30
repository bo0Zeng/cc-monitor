//! 帧面宿主：只读查询与几条薄壳（`00 §1.6.2`：本体在 `observe/` / `control/`，`inbound.rs` 不许出现 `observe::`）。

pub mod accounts_face; // 改账号库那几条命令的帧面宿主：本体在 accounts/manage/，接上 apikey 表与别名文件那两步
pub mod feature_face; // 〔RM1b · 第四波〕功能侧只读查询的帧面宿主（tasks-list …）—— 薄壳，本体在 observe/，与 read_face 分家的理由在它头注
pub mod fork_face; // 〔LOC1a · 第四波 4D〕帧面 `session-fork` 的宿主壳：找家目录、交 `control/fork_write`（本体与 CLI `--fork-session` 同一份）
pub mod read_face; // 〔C1 · 09-24〕只读查询的帧面宿主（8 条：history-* / accounts-*）—— 薄壳，本体在 observe/，住顶层的理由同 files/
pub mod resync_face; // 〔RESYNC · V149〕手动对齐 `resync` 的帧面宿主 —— 薄壳，本体在 observe/watcher.rs（住顶层的理由同 read_face）
