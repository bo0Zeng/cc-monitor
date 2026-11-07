//! 帧面宿主：只读查询与几条薄壳（本体在 `observe/` / `control/`，`stream/inbound/` 不许出现 `observe::`）。

// 测试段可以直接问系统钟（量具、夹具的时刻）；生产段只许纯工具那一层的 `time::now`（`clippy.toml`）。
// 测试段收尾时丢结果不写理由；生产段丢一个 `Result` 写 `.discard(Why::…)`。
#![cfg_attr(
    test,
    allow(
        clippy::allow_attributes,
        clippy::allow_attributes_without_reason,
        clippy::disallowed_methods,
        clippy::let_underscore_must_use,
        clippy::unused_result_ok,
        reason = "测试段放开禁用名单与吞结果那两条：夹具自己定期限、读写盘、起进程、收尾时丢结果；用不用得到随判据增减，写成 `expect` 会在用不到时报 ⇒ 整包唯一许写 `allow` 的一行"
    )
)]

// 包里照旧写 `crate::faces::…` · `crate::accounts::…` · `crate::agents::…` · `crate::assets::…` · `crate::control::…` ·
// `crate::dial::…` · `crate::footprint::…` · `crate::history::…` · `crate::host::…` · `crate::observe::…` · `crate::plan::…` ·
// `crate::relay::…` · `crate::wire::…` · `crate::common::…` · `crate::platform::…`。
extern crate self as faces;
use backend_accounts as accounts;
use backend_agents as agents;
use backend_assets as assets;
use backend_common as common;
use backend_control as control;
use backend_dial as dial;
use backend_footprint as footprint;
use backend_history as history;
use backend_host as host;
use backend_observe as observe;
use backend_plan as plan;
use backend_platform as platform;
use backend_relay as relay;
use backend_wire as wire;

// 测试档的家一开始就是沙箱（`src/common/test-home`）：这一行让链接器把它的启动段带进本包的测试二进制。
#[cfg(test)]
#[used]
static TEST_HOME: fn() = test_home::linked;

pub mod accounts_face; // 改账号库那几条命令的帧面宿主：本体在 accounts/manage/，接上 apikey 表与别名文件那两步
pub mod assets_sync_face; // 帧面 `assets-sync`（typed）：沿池里那条 SSH 同步资产目录，本体在 assets/asset_sync.rs
pub mod bookmarks_face; // 书签那两条（typed）：bookmarks-read · bookmarks-edit，本体在 control/bookmarks.rs
pub mod bus_list_face; // 帧面 `bus-list`（typed）：名单问 cc-bus，叫它什么 · 在不在问观测层那一处
pub mod catalog_face; // 资产目录那一族四条（typed）：assets-catalog · assets-catalog-merge · ext-note-set · ext-list-here
pub mod cells_catalog; // 格目录：每件成品有哪些格（从成品的 Rust 类型序列化出来，不手写第二份；`view` 投影按它认成品名与路径）
pub mod diagnostics_face; // 帧面 `diagnostics-report`（typed）：日志页「复制诊断信息」整段成品（壳只交它自己的三样事实）
pub mod ext_hub_face; // 两台之间「装」那一件的枢纽那一对（typed）：ext-hub-preview · ext-hub-apply
pub mod ext_list_face; // 设置「扩展」页那几条（typed）：ext-list · ext-uninstall-preview / -apply
pub mod feature_face; // 功能侧只读查询的帧面宿主（tasks-list …）—— 薄壳，本体在 observe/，与 read_face 分家的理由在它头注
pub mod first_run_face; // 帧面 `first-run`（typed）：「开始用」三步，本体在 footprint/readiness.rs
pub mod footprint_face; // 足迹那一族四条小命令（typed）：agent-home-check · chores-mark · last-seen-read / -write
pub mod footprint_report_face; // 「足迹」那两条成品（typed）：data-report · chores-merge（单住：data-report 问 tmux）
pub mod fork_face;
pub mod history_face; // 读一份会话记录的那几条（typed）：history-read · -page · -lines · -run · -tail · -branch · -record · sessions-needs
pub mod launch_account_face; // 起会话挑号要的事实（账号库 · 上次的号）与起之前预标信任：`session-open` · 换号重启 · 会话里切号同一份
pub mod login_face; // 在 app 里登录一个号（`login-*`）· 会话外的需手动清单（进 `sessions-needs`）
pub mod md_doc_face; // 帧面 `md-doc`（typed）：一段 Markdown 原文 ⇒ 块树（思考展开 · 工具结果切到 Markdown · 计划格正文）
pub mod monitor_board_face; // 监控板那两条（typed）：monitor-board-here（这台那几条的序 · 活不活 · 在跑几个）· monitor-board（问到各台、合成一份）
pub mod plan_ask_face; // 帧面 `plan-ask`：问图开框前那一问（拼开口那句 · 记这台问图用的别名）
pub mod plan_face; // 帧面 `plan-list` · `plan-read` · `plan-cell-view`：计划读面要的这台事实（pb 装在哪 · 会话在哪些目录 · id 是哪个会话）
pub mod plan_return_face; // 帧面 `plan-return`：计划退回（拼那一行 · 判能不能送 · 走 `terminal-input` 的本体 · 记已退回）
pub mod plan_review_face; // 帧面 `plan-ack` · `plan-unack`：计划认可（记在后端自己的 `plan-review.json`）
pub mod profiles_face; // 配置文件那两条要问控制面的（typed）：profiles-read（有没有 tmux）· profiles-resolve（渲 ccm 那一行）；本体在 assets/aliases/page.rs
pub mod project; // 通用投影 `project(成品, 声明)`：出口在请求信封里交的 `view`（cells · omit）照格目录校验、挑格去格，只这一处
pub mod prose_hl_face; // 帧面 `prose-hl`（typed）：拉一段正文的代码高亮，本体在 md-core（进程里常驻一份高亮器、按内容缓存）
pub mod quota_probe_face; // 帧面 `quota-probe`：用某个号查一次额度（起官方客户端报用量、读法住适配层），记进同一本额度账
pub mod quota_read; // `quota-read` 的成品（有类型：每号 ＝ 原数 ＋ 显示态 ＋ 写好的几行 · 5h 那一格 · 开窗那一判；顶上 ＋ 号名表）
pub(crate) mod quota_rows; // `quota-read` 每号的 `rows`（额度每号几行，唯一的行模型）与开窗那一判 `warm`
pub mod read_face; // 只读查询的帧面宿主（8 条：history-* / accounts-*）—— 薄壳，本体在 observe/，住顶层的理由同 files/
pub mod rotation_act_face; // 帧面：轮换的切换（typed，基础功能层）：拖出来的先后 · 现在用
pub mod rotation_edit_face; // 帧面：轮换配置的写（typed，配置编辑层）：设置 · 存 · 换一份 · 新会话用哪份 · 策略文件 · 起之前那一笔
pub mod rotation_face; // 换号那一族的帧面宿主：账号库读成上游选择换号要的那几格（两边只在这里接上）
pub mod rotation_facts_face; // 帧面 `rotation-facts`（typed）：每个号此刻的事实，本体在 Hop::facts
pub mod rotation_try_face; // 帧面 `rotation-try`（typed）：拿一份策略原文判一次、推演，不写盘
pub mod rotation_view_face; // 帧面：轮换的读（typed）：策略目录 · 界面四块要的全部
pub mod search_face; // 全文搜索那两条（typed）：`history-search` · `history-search-merge`；本体在 observe/search_query.rs
pub mod session_account_face; // 帧面 `session-account`（typed）：一条会话的号那一份成品（chip · 徽章 · 手机照画）
pub mod session_arrive_face; // 帧面 `session-arrive`：等起好的会话报到（认法住观测层）
pub mod session_batch_face;
pub mod session_commands_face; // 帧面 `session-commands`（typed）：一条会话能用的斜杠命令与 Skill
pub mod session_mode_face;
pub mod session_model_face; // 帧面 `session-model`（typed）：换模型（送那一行、从记录读回；终端没了记下等恢复） // 帧面 `session-permission`（typed）：切权限档（按循环键、等重画、读回到了为止；终端没了记下等恢复）
pub mod session_open_face; // 帧面 `session-open` · `-facts` · `-dir`：起新的 · 恢复 · 接回一个请求，起新会话框要的这台事实与目录那一问
pub mod session_queue_deliver; // 会话输入队的投递：主人这台一条线程，到「能送」才送一条（送字本体在 session_send.rs）
pub mod session_queue_face; // 会话输入队那三条（typed）：session-queue-put（往会话说一句的唯一入口）· -cancel · -read，本体在 history/session_queue.rs
pub(crate) mod session_queue_hop; // 会话输入队那三条的跨台那一跳：会话不在这台 ⇒ 交给那台的常驻，够不到 ⇒ offline
pub mod session_rewind_face; // 编辑已发的一句、发送那一下的选项（session-rewind-options）：照那一家申报的交出，会话不在这台 ⇒ 交给那台
pub(crate) mod session_send; // 往一条会话送进一句的本体（在等人不送 · 只剩 shell 不送 · 终端没了恢复再交 · 附件照那一家拼）；只由投递调
pub mod session_set_face; // 帧面 `set-plan` · `set-launch`：补足一组会话要的这台事实与手（配置文件 · 活会话表 · 启动台账的别名 · 起会话 · 组注解）；判定住 control/session_set.rs
pub mod session_window_face; // 帧面 `session-window`（typed）：↗ 那一问，会话在这台电脑上的哪儿（本机 · 远端同一个形）
pub mod sessions_batch_face; // 帧面 `sessions-batch`（typed）：一批会话跨几台，本机同进程做、别的台经常驻问它自己那一组，合好那一句
pub mod terminals_board_face; // 「终端」视图那两条（typed）：terminals-board-here（这台那几行）· terminals-board（问到各台、合成一份）
pub mod terminals_end_face; // 帧面 `terminals-end`（typed）：终端名单「结束所选」那一批跨几台，本机同进程结束、别的台经常驻问它自己那一组，合好那一句

/// 拨号层要的「这一端的机器表怎么读」由这里装（拨号层不认控制面）：进程起来那一下调一次。
/// 之后 `link-open` · 问那台 · 传输 · 部署都只按名字查它组请求，线上不交拨号。
pub fn install_machines() {
    crate::dial::install_machines(crate::control::machines::here);
}
