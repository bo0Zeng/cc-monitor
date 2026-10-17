//! 帧面宿主：只读查询与几条薄壳（本体在 `observe/` / `control/`，`stream/inbound/` 不许出现 `observe::`）。

pub mod accounts_face; // 改账号库那几条命令的帧面宿主：本体在 accounts/manage/，接上 apikey 表与别名文件那两步
pub(crate) mod assets_sync_face; // 帧面 `assets-sync`（typed）：沿池里那条 SSH 同步资产目录，本体在 assets/asset_sync.rs
pub(crate) mod catalog_face; // 资产目录那一族四条（typed）：assets-catalog · assets-catalog-merge · ext-note-set · skill-install-record
pub(crate) mod cells_catalog; // 格目录：每件成品有哪些格（从成品的 Rust 类型序列化出来，不手写第二份；`view` 投影按它认成品名与路径）
pub(crate) mod ext_hub_face; // 两台之间「装」那一件的枢纽那一对（typed）：ext-hub-preview · ext-hub-apply
pub(crate) mod ext_list_face; // 设置「扩展」页那几条（typed）：ext-list · ext-uninstall-preview / -apply
pub mod feature_face; // 功能侧只读查询的帧面宿主（tasks-list …）—— 薄壳，本体在 observe/，与 read_face 分家的理由在它头注
pub(crate) mod first_run_face; // 帧面 `first-run`（typed）：「开始用」三步，本体在 footprint/readiness.rs
pub(crate) mod footprint_face; // 足迹那一族四条小命令（typed）：agent-home-check · chores-mark · last-seen-read / -write
pub(crate) mod footprint_report_face; // 「足迹」那两条成品（typed）：footprint-report · data-report（单住：data-report 问 tmux）
pub mod fork_face;
pub(crate) mod history_face; // 读一份会话记录的那几条（typed）：history-read · -page · -lines · -run · -tail · -branch · -record · sessions-needs
pub(crate) mod hub_inner_face; // 枢纽问各台的那 8 条内层命令（typed）：名字 · 入参 · 回包取 `assets::hub_wire` 那三样
pub mod launch_face; // 帧面 `launch-local` / `launch-render-cli` 的宿主壳：交起会话挑号要的事实（账号库 · 上次的号） // 帧面 `session-fork` 的宿主壳：找家目录、交 `control/fork_write`（本体与 CLI `--fork-session` 同一份）
pub mod login_face; // 在 app 里登录一个号（`login-*`）· 会话外的需手动清单（`manual-list`）
pub mod plan_face; // 帧面 `plan-list` · `plan-read` · `plan-cell-view`：计划读面要的这台事实（pb 装在哪 · 会话在哪些目录 · id 是哪个会话）
pub mod plan_return_face; // 帧面 `plan-return`：计划退回（拼那一行 · 判能不能送 · 走 `terminal-input` 的本体 · 记已退回）
pub mod plan_review_face; // 帧面 `plan-ack` · `plan-unack`：计划认可（记在后端自己的 `plan-review.json`）
pub(crate) mod profiles_face; // 配置文件那两条要问控制面的（typed）：profiles-read（有没有 tmux）· profiles-resolve（渲 ccm 那一行）；本体在 assets/aliases/page.rs
pub(crate) mod project; // 通用投影 `project(成品, 声明)`：出口在请求信封里交的 `view`（cells · omit）照格目录校验、挑格去格，只这一处
pub mod quota_probe_face; // 帧面 `quota-probe`：用某个号查一次额度（起官方客户端报用量、读法住适配层），记进同一本额度账
pub(crate) mod quota_read; // `quota-read` 的成品（有类型：每号 ＝ 原数 ＋ 显示态 ＋ 写好的几行 · 5h 那一格 · 开窗那一判；顶上 ＋ 号名表）
pub(crate) mod quota_rows; // `quota-read` 每号的 `rows`（额度每号几行，唯一的行模型）与开窗那一判 `warm`
pub mod read_face; // 只读查询的帧面宿主（8 条：history-* / accounts-*）—— 薄壳，本体在 observe/，住顶层的理由同 files/
pub(crate) mod remote_reach_face; // 帧面 `remote-reach`（typed）：可达表登记（dial）＋ 登记完交一次待交的偏好（control）—— 两边只在这里接上
pub(crate) mod rotation_edit_face; // 帧面：轮换的写（typed）：设置 · 存 · 换一份 · 拖出来的顺序 · 现在用 · 策略文件 · 24 小时条的算术
pub mod rotation_face; // 换号那一族的帧面宿主：账号库读成上游选择换号要的那几格（两边只在这里接上）
pub(crate) mod rotation_facts_face; // 帧面 `rotation-facts`（typed）：每个号此刻的事实，本体在 Hop::facts
pub(crate) mod rotation_try_face; // 帧面 `rotation-try`（typed）：拿一份策略原文判一次、推演，不写盘
pub(crate) mod rotation_view_face; // 帧面：轮换的读（typed）：策略目录 · 界面四块要的全部
pub(crate) mod search_face; // 全文搜索那两条（typed）：`history-search` · `history-search-merge`；本体在 observe/search_query.rs
pub mod session_batch_face;
pub(crate) mod session_commands_face; // 帧面 `session-commands`（typed）：一条会话能用的斜杠命令与 Skill
pub(crate) mod session_mode_face;
pub(crate) mod session_model_face; // 帧面 `session-model`（typed）：会话开着时换模型（送那一行、从记录读回） // 帧面 `session-permission`（typed）：会话开着时切权限档（按循环键、等重画、读回到了为止）
pub mod session_new_face; // 帧面 `session-new` · `session-new-facts` · `session-new-dir`：起新会话框要的这台事实 ＋ 起
pub(crate) mod session_open_face; // 帧面 `session-open`：起新的 · 恢复 · 接回一个请求（恢复那条的记录 · 分叉源在不在 tmux 由这层读）
pub mod session_restart_face;
pub(crate) mod session_say_face; // 帧面 `session-say`（typed）：往一条会话说一句，在等人 ⇒ 不送，不在等 ⇒ 照 `terminal-input` 送 // 帧面 `session-restart` 的宿主壳：每一步一份批量那份事实、停旧 ＋ 起新拿退出排空的票、两种等待住观测层 // 帧面 `sessions-stop` / `sessions-start` 的宿主壳：把 tmux 名单 · 记录在不在 · 杀 · 建会话那几样交给 control/session_batch
