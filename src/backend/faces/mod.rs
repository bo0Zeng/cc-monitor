//! 帧面宿主：只读查询与几条薄壳（本体在 `observe/` / `control/`，`stream/inbound/` 不许出现 `observe::`）。

pub mod accounts_face; // 改账号库那几条命令的帧面宿主：本体在 accounts/manage/，接上 apikey 表与别名文件那两步
pub mod autostart_face; // 自动起算：每号开关与时段的读写 ＋ 判定要的那几样现读 ＋ 记结果
pub mod autostart_waker; // 自动起算的醒点：常驻后端里只醒在真期限上的那条线程（到点替号发那一句 · 卡住会话到点推一次）
pub mod feature_face; // 功能侧只读查询的帧面宿主（tasks-list …）—— 薄壳，本体在 observe/，与 read_face 分家的理由在它头注
pub mod fork_face;
pub mod launch_face; // 帧面 `launch-local` / `launch-render-cli` 的宿主壳：交起会话挑号要的事实（账号库 · 上次的号） // 帧面 `session-fork` 的宿主壳：找家目录、交 `control/fork_write`（本体与 CLI `--fork-session` 同一份）
pub mod read_face; // 只读查询的帧面宿主（8 条：history-* / accounts-*）—— 薄壳，本体在 observe/，住顶层的理由同 files/
pub mod resync_face;
pub mod rotation_face; // 换号那一族的帧面宿主：账号库读成上游选择换号要的那几格（两边只在这里接上）
pub mod rotation_switch_face; // 「现在就换」：不重启换交给 rotation_face、重启换交给 session-restart（只有这一条够得着 tmux）
pub mod session_batch_face;
pub mod session_restart_face; // 帧面 `session-restart` 的宿主壳：每一步一份批量那份事实、停旧 ＋ 起新拿退出排空的票、两种等待住观测层 // 帧面 `sessions-stop` / `sessions-start` 的宿主壳：把 tmux 名单 · 记录在不在 · 杀 · 建会话那几样交给 control/session_batch // 手动对齐 `resync` 的帧面宿主 —— 薄壳，本体在 observe/watcher.rs（住顶层的理由同 read_face）
