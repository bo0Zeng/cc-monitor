//! 命令表 · 账号：`accounts-*` · `apikey-*` · `quota-read` · `quota-probe` · `quota-warm` · `rotation-*` · `login-*`。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    super::typed::blocking::<crate::faces::rotation_face::QuotaReadCmd>("这台的额度账"),
    // 用某个号查一次额度（帧面宿主 `faces/quota_probe_face.rs`）：起官方客户端、等它 ⇒ 阻塞档，总期限登记在 `caps.rs`。
    super::typed::blocking::<crate::faces::quota_probe_face::QuotaProbe>(
        "用某个号查一次额度",
    ),
    // 用某个号发一句、让它的 5 小时窗开始计时（宿主 `accounts/quota/warm.rs`，与轮换判完开窗同一处）：等那一发说完 ⇒ 阻塞档，总期限登记在 `caps.rs`。
    super::typed::blocking::<crate::accounts::quota::warm::QuotaWarm>("给一个号开窗"),
    // 轮换第五版：每个号此刻的事实（typed）。只读。
    super::typed::blocking::<crate::faces::rotation_facts_face::RotationFacts>(
        "每个号此刻的事实",
    ),
    // 一条会话的号那一份成品（帧面宿主 `faces/session_account_face.rs`）：状态栏按钮 · 号徽章 · 手机照画。只读。
    super::typed::blocking::<crate::faces::session_account_face::SessionAccountCmd>(
        "一条会话的号",
    ),
    // 轮换第五版：拿一份策略原文照此刻的事实判一次、往后推演，不写盘不换号（skill 试跑 · 测试台）。
    super::typed::blocking::<crate::faces::rotation_try_face::RotationTry>(
        "试跑一份策略",
    ),
    // 轮换第五版：读 —— 策略目录 · 界面四块要的全部。
    super::typed::blocking::<crate::faces::rotation_view_face::RotationStrategies>(
        "策略目录",
    ),
    super::typed::blocking::<crate::faces::rotation_view_face::RotationView>(
        "一个会话或一份策略此刻的轮换",
    ),
    // 写 —— 设置 · 载入一份 · 都载入新版 · 新会话用哪份 · 会话侧的临时动作 · 现在用 · 策略文件加 / 复制 / 改名 / 删。
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationSettings>("改一份策略的设置"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationUse>("会话载入一份策略"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationReload>("在用旧版的会话都载入新版"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationForNew>("新会话用哪份策略"),
    super::typed::blocking::<crate::faces::rotation_act_face::RotationTemp>("会话侧的临时动作"),
    super::typed::blocking::<crate::faces::rotation_act_face::RotationUseNow>("现在用这个号"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyAdd>("加一份策略"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyCopy>("复制一份策略"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyRename>("给策略改名"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyDelete>("删一份策略"),
    super::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyTz>("给策略设时区"),
    // **上游选择**那份凭据文件在**这台机器上**的读写口 —— 上游选择自己的状态，
    //   不是用户文件（判清全文）⇒ 写经 `Own`（启动时造、经分派递进来），
    //   **只从这里一扇门进来**。远端账号页配的 key 从此落在会话跑的那台机器上。
    //   ⚠ 明文只在 `apikey-key-set` 的 `args.key` 里（帧面：长连接入方向；派生 CLI 面：stdin），
    //     **不进 argv / env / 日志**；两条的应答都只有掩码。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断。
    //   ⚠ 它们**不起中转**、中转那几条也**不碰凭据**（「账号就账号, 中转就中转」）。
    super::typed::blocking::<crate::faces::accounts_face::ApikeyKeySet>(
        "给一个账号写 key，写完读回",
    ),
    // 出成品：`{meta, accounts, notice}`，并上这台机器自己那份 apikey 表；`agent` 随请求带（必填）。
    super::typed::blocking::<crate::faces::read_face::AccountsListCmd>("账号清单"),
    // 停 / 重启 / 更新 / 卸载一台的 cc-monitor 之前会打断什么：界面只问本机后端这一条，别的台那一份由它经那台的常驻流问到、
    //   并好再答（`read_face::interrupts_hub`）⇒ 异步档（这台自己那一份读 pidfile，在阻塞池里出）。只读。
    super::typed::asynchronous::<crate::faces::read_face::MachineInterrupts>(
        "停 / 重启 / 更新 / 卸载一台的 cc-monitor 之前，会打断什么",
    ),
    // 改账号库的那几条（typed：入参 · 成品 · 码表都从 `cmd_wire::accounts` 与 `faces::accounts_face` 的类型来）：本体 `accounts/manage/`，
    //   帧面宿主 `faces/accounts_face.rs`（建 API 号写 key · 账号表变了改配置文件、重生成别名）。
    //   写经这台的文件管理面（[`LocalFiles`]）与分派那一层递的写句柄（key 表）；同步文件 I/O ⇒ 阻塞档。`accounts-verify` 只先补缺的共享链接（账号库对齐）；`accounts-login-cmd` 只读。
    super::typed::blocking::<crate::faces::accounts_face::AccountsInit>("建账号库"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsAdd>("新建一个号"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsRemove>("删一个号"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsSetDefault>("设默认号"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsRepair>("修复账号库（幂等）"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsRollback>("按一份备份还原"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsVerify>("核对账号库"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsLoginCmd>("在终端里登录一个号的那一行"),
    // 各号共用的用户级 MCP（本体 `accounts/manage/mcp_share_exec.rs`）：读各号的配置文件、只改那一个键、写回共享集合 ⇒ 阻塞档。
    //   成品只有名字与号名，不带定义里的任何值。
    super::typed::blocking::<crate::faces::accounts_face::AccountsMcpRead>("这台各账号共用的用户级 MCP 此刻的样子"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsMcpRemove>("从各账号共用的用户级 MCP 里删一条"),
    super::typed::blocking::<crate::faces::accounts_face::AccountsMcpPick>("两边都改了的那一条用哪一版"),
    // 停 / 开各号之间的同步（只改共享集合那份文件里那一格；开回来那一刻同步一趟）。
    super::typed::blocking::<crate::faces::accounts_face::AccountsMcpSync>("停 / 开各账号之间同步用户级 MCP（定义与登录同一个开关）"),
    // 在 app 里登录一个号 · 一个号里的一个 MCP 服务器（本体 `accounts/sign_in.rs`，帧面宿主 `faces/login_face.rs`）：核心起那一家的登录子命令、转链接、递码，
    //   令牌那一家自己写。`login-start` 要跟着发起它的那条连接走（连接断了作业就收）⇒ 硬臂（`dispatch` 带本连接的号调它）；
    //   作业自己收了（本机浏览器回调登完 · 那一家先退了）推一帧 `changed {login, key: 作业号}` 给那条连接。
    super::typed::frame_blocking::<crate::faces::login_face::LoginStart>(
        "在 app 里登录一个号或一个号里的一个 MCP 服务器：起那一家的登录子命令，交回登录链接（本连接断了 ⇒ 作业收）",
    ),
    super::typed::blocking::<crate::faces::login_face::LoginFinish>(
        "把粘回来的那一段（登录页上的码 · 浏览器回跳的地址）交给那份登录作业，等那一家换完",
    ),
    super::typed::blocking::<crate::faces::login_face::LoginCancel>(
        "收一份登录作业",
    ),
];
