//! 帧面宿主：只读查询与几条薄壳（本体在 `observe/` / `control/`，`inbound.rs` 不许出现 `observe::`）。

pub mod accounts_face; // 改账号库那几条命令的帧面宿主：本体在 accounts/manage/，接上 apikey 表与别名文件那两步
pub mod feature_face; // 功能侧只读查询的帧面宿主（tasks-list …）—— 薄壳，本体在 observe/，与 read_face 分家的理由在它头注
pub mod fork_face; // 帧面 `session-fork` 的宿主壳：找家目录、交 `control/fork_write`（本体与 CLI `--fork-session` 同一份）
pub mod read_face; // 只读查询的帧面宿主（8 条：history-* / accounts-*）—— 薄壳，本体在 observe/，住顶层的理由同 files/
pub mod resync_face;
pub mod session_batch_face; // 帧面 `sessions-stop` / `sessions-start` 的宿主壳：把 tmux 名单 · 记录在不在 · 杀 · 建会话那几样交给 control/session_batch // 手动对齐 `resync` 的帧面宿主 —— 薄壳，本体在 observe/watcher.rs（住顶层的理由同 read_face）
