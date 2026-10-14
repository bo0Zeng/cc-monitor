//! 命令表 · 账号：`accounts-*` · `apikey-*` · `quota-read` · `quota-probe` · `rotation-*` · `login-*` · `manual-list`。

use crate::wire::spec::{arg, both, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "quota-read",
        summary: "这台的额度账",
        codes: &[],
        fields: &[out("accounts", "每个号一条，按 `(agent, account)` 排：`agent` 路由第 1 段（哪一家）· `account` 路由第 2 段（哪个号"), out("candidates", "换号 · 加号那几处的候选 `{agent, account, kind}`：出过数的在前、没出过的在后，起会话时没说是哪个号的 `_` 不在里面（判断只住 `name_words::is_candidate`）；界面照它画，不另拼、不另滤"), out("detail", "只在 `unreadable` 时有：复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答），`reason` 那一句不带原话"), out("earliestReturn", "被拒 / 超额在兜的号里最早回来的那个 `{account, at}`；没有、或都说不出时刻 ⇒ `null`"), out("fiveHour", "「5h 那一格」写好的字：顶上那一格只在 `unreadable` 时有（`5h 读不到`）、否则 `null`；`accounts[]` 每个出过数的订阅号一格（`5h 41%` · 卡着的照语义位的字），按量号 ⇒ `null`；`unseen[]` 恒 `null`"), out("names", "号名与语义位名的字（核心一处写，出口照它画、不认码）：`{accounts: {码: 字}, slots: {码: 字}}`；`accounts` 只列与原名不同的号（起会话时没说是哪个号的 `_`），不在表里的号就叫它自己的名字；`slots` 列 `5h` · `7d`"), out("now", "这台此刻的 unix 秒（界面算「几分钟前看到的」「还有多久重置」都按这台的钟）；回包里每个时刻（`at` · `seenAt` · `resetsAt` · `fromResetsAt` · `since`）旁边有一格 `…Text`：出口按看的那一台的时区（请求的 `tz`）写好的字（当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年），还没到的再有一格 `…RelText`（距今 `+1h50m` · `+3d`），界面照抄、不换算；`slots[]` 每格带写好的 `text` 与 `tone`（plain · fail · warn）"), out("path", "那份文件的绝对路径（家推不出来时 `null`）"), out("reason", "只在 `unreadable` 时有：为什么读不出来；其余 `null`"), out("state", "`\"present\"`（读得懂）· `\"absent\"`（还没看到过任何回包）· `\"unreadable\"`（文件读不出来 / 家推不出来）"), out("unseen", "账号库里有、额度账上从没出过数的号：`{agent, account, kind, login, subId?, rows, warm}`（几格同下）。出过数的与没出过的每号都带 `rows`：那一段写好的几行 `[[{text, tone}]]`（首行 名 · 类型 · 标签；其余每行 键 · 值 · ↻ · 距今；采样那一格只写几点，哪台采的由出口写）· `warm`：开窗那一判 `{act: send|wait, at?, text}`（quota-warm 照它发一句 / 睡到 `at`）"), out("text", "读不出（`unreadable`）· 一个号都没有 ⇒ 那一句（`--text` 拼字时放最前）；否则 `null`"), out("usableNow", "此刻发得出去的号（路由第 2 段）：登录拿得到、不是被拒 / 超额在兜（快满 · 数旧 · 没采样 · 上一窗已过都算）")],
        takes_input: false,
        run: Run::BlockingData(|r| crate::faces::rotation_face::answer_quota_read(&r.tz).map(Some)),
    },
    // 用某个号查一次额度（帧面宿主 `faces/quota_probe_face.rs`）：起官方客户端、等它 ⇒ 阻塞档，总期限登记在 `caps.rs`。
    crate::wire::typed::blocking::<crate::faces::quota_probe_face::QuotaProbe>(
        "用某个号查一次额度",
    ),
    // 轮换第五版：每个号此刻的事实（typed，`轮换-v5-分层.md` §1.1）。只读。
    crate::wire::typed::blocking::<crate::faces::rotation_facts_face::RotationFacts>(
        "每个号此刻的事实",
    ),
    // 轮换第五版：拿一份策略原文照此刻的事实判一次、往后推演，不写盘不换号（skill 试跑 · 测试台）。
    crate::wire::typed::blocking::<crate::faces::rotation_try_face::RotationTry>(
        "试跑一份策略",
    ),
    // 轮换第五版（`轮换-v5-分层.md` §6.6 第 10 条）：读 —— 策略目录 · 界面四块要的全部。
    crate::wire::typed::blocking::<crate::faces::rotation_view_face::RotationStrategies>(
        "策略目录",
    ),
    crate::wire::typed::blocking::<crate::faces::rotation_view_face::RotationView>(
        "一个会话或一份策略此刻的轮换",
    ),
    // 写 —— 设置 · 存 · 还原 · 换一份 · 新会话用哪份 · 拖出来的顺序 · 现在用 · 策略文件加 / 复制 / 改名 / 删。
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationSettings>("改一份设置"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationSave>("存设置"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationRevert>("丢掉会话里没存的设置"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationUse>("会话换一份策略"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationForNew>("新会话用哪份策略"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationTemp>("排接下来先用谁"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationUseNow>("现在用这个号"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyAdd>("加一份策略"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyCopy>("复制一份策略"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyRename>("给策略改名"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyDelete>("删一份策略"),
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationStrategyTz>("给策略设时区"),
    // 算术 —— 24 小时条上拖出一段（分段的算术，不是换号的想法）。
    crate::wire::typed::blocking::<crate::faces::rotation_edit_face::RotationDayPaint>(
        "在 24 小时条上刷一段",
    ),
    // **上游选择**那份凭据文件在**这台机器上**的读写口 —— 上游选择自己的状态，
    //   不是用户文件（判清全文）⇒ 写经 `Own`（启动时造、经分派递进来），
    //   **只从这里一扇门进来**。远端账号页配的 key 从此落在会话跑的那台机器上。
    //   ⚠ 明文只在 `apikey-key-set` 的 `args.key` 里（帧面：长连接入方向；派生 CLI 面：stdin），
    //     **不进 argv / env / 日志**；两条的应答都只有掩码。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断。
    //   ⚠ 它们**不起中转**、中转那几条也**不碰凭据**（「账号就账号, 中转就中转」）。
    crate::wire::typed::blocking::<crate::accounts::upstream_select::file_face::ApikeyKeySet>(
        "给一个账号写 key，写完读回",
    ),
    // 出成品：`{meta, accounts, notice}`，并上这台机器自己那份 apikey 表；`agent` 随请求带（必填）。
    CommandSpec {
        name: "accounts-list",
        summary: "账号清单",
        codes: &["bad_args", "too_large"],
        fields: &[out("accounts", "每账号一个对象 `{name, email, configDir, isDefault, mode, exists, loggedIn, authKind, authReady, selectable, keyMasked, baseUrl, badge, color}`；`selectable` ＝ 能拿来起会话 / 选为默认（`acct_core::account_selectable`）；`color` ＝ 号色（`--acct-c0..c7` 第几格，账号库里的事实：加号时分第一个空格、存在清单里，`Manifest::with_colors`），界面只画它"), arg("agent", "必填：这次起会话的是哪一家（适配器 id；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）"), out("meta", "`{enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error, unsupported, nextDefault, effectiveDefault, home}`；`effectiveDefault` ＝ 这台的默认号（标了的第一个，没标 ⇒ 第一个；`acct_core::effective_default`）"), out("notice", "「能用但有缺」的那一句（几件都有 ⇒ 连成一句）：启用了却一个账号 0 都没有（写清单的那一侧旧到不认账号 0）· 这台的 API key 凭据文件不是只给本人（宽在哪 ＋ 怎么修；查不出 ⇒ 为什么）；都没有 ⇒ `null`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么（界面问那台 ＋ 问本机数通往那台的转发，并排画）。只读。
    crate::wire::typed::blocking::<crate::faces::read_face::MachineInterrupts>(
        "停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前，会打断什么",
    ),
    crate::wire::typed::blocking::<crate::faces::read_face::AccountsSessions>(
        "正在跑的会话各属哪个账号",
    ),
    // 换号前的信任预检：`configDir`（缺席 / null = 账号 0）＋ `cwd` → `{trusted, known}`。
    //   同族同档（读一份 manifest ＋ 一份 `.claude.json` ⇒ 阻塞档）、同一个只读宿主。
    crate::wire::typed::blocking::<crate::faces::read_face::AccountsTrust>("换号前的信任预检"),
    // 改账号库的那几条（typed：入参 · 成品 · 码表都从 `cmd_wire::accounts` 与 `faces::accounts_face` 的类型来）：本体 `accounts/manage/`，
    //   帧面宿主 `faces/accounts_face.rs`（建 API 号写 key · 账号表变了改配置文件、重生成别名）。
    //   写经这台的文件管理面（[`LocalFiles`]）与分派那一层递的写句柄（key 表）；同步文件 I/O ⇒ 阻塞档。`accounts-verify` / `accounts-login-cmd` 只读。
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsInit>("建账号库"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsAdd>("新建一个号"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsRemove>("删一个号"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsSetDefault>("设默认号"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsRepair>("修复账号库（幂等）"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsRollback>("按一份备份还原"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsVerify>("核对账号库"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsLoginCmd>("在终端里登录一个号的那一行"),
    // 各号共用的用户级 MCP（本体 `accounts/manage/mcp_share_exec.rs`）：读各号的配置文件、只改那一个键、写回共享集合 ⇒ 阻塞档。
    //   成品只有名字与号名，不带定义里的任何值。
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsMcpRead>("这台各账号共用的用户级 MCP 此刻的样子"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsMcpRemove>("从各账号共用的用户级 MCP 里删一条"),
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsMcpPick>("两边都改了的那一条用哪一版"),
    // 停 / 开各号之间的同步（只改共享集合那份文件里那一格；开回来那一刻同步一趟）。
    crate::wire::typed::blocking::<crate::faces::accounts_face::AccountsMcpSync>("停 / 开各账号之间同步用户级 MCP（定义与登录同一个开关）"),
    // 在 app 里登录一个号 · 一个号里的一个 MCP 服务器（本体 `accounts/sign_in.rs`，帧面宿主 `faces/login_face.rs`）：核心起那一家的登录子命令、转链接、递码，
    //   令牌那一家自己写。`login-start` 要跟着发起它的那条连接走（连接断了作业就收）⇒ 硬臂（`dispatch` 带本连接的号调它）；
    //   作业自己收了（本机浏览器回调登完 · 那一家先退了）推一帧 `changed {login, key: 作业号}` 给那条连接。
    CommandSpec {
        name: "login-start",
        summary: "在 app 里登录一个号或一个号里的一个 MCP 服务器：起那一家的登录子命令，交回登录链接（本连接断了 ⇒ 作业收）",
        codes: crate::faces::login_face::LOGIN_START_CODES,
        fields: &[arg("account", "账号库里的号（账号 0 是 `0`；`mcp` 那一种可为 `null`：那一家的默认配置目录）"), arg("agent", "哪一家（同 `accounts-list` 的 `agent`）"), arg("kind", "登录什么：`account`（一个号）· `mcp`（一个号里的一个 MCP 服务器）"), arg("projectDir", "`mcp` 那一种：那一条配置所属的项目目录（用户级 ⇒ `null`）"), arg("server", "`mcp` 那一种：服务器名（`mcp-read` 里那一条）"), out("auto", "会不会自己登完（本机浏览器回调到那一家自己开的本机端口；登完推 `changed {login}`，粘码只是后备）"), out("job", "作业号（`login-finish` · `login-cancel` 交它）"), out("paste", "粘回来的是什么：`code`（登录页上给的那一段）· `redirectUrl`（浏览器回跳到本机的那一整条地址）"), out("url", "登录链接（在浏览器里打开，不用内嵌网页）")],
        takes_input: true,
        // 本体 `faces::login_face::answer_start`（`dispatch` 的硬臂带着本连接的号调它）。
        run: Run::Builtin,
    },
    crate::wire::typed::blocking::<crate::faces::login_face::LoginFinish>(
        "把粘回来的那一段（登录页上的码 · 浏览器回跳的地址）交给那份登录作业，等那一家换完",
    ),
    crate::wire::typed::blocking::<crate::faces::login_face::LoginCancel>(
        "收一份登录作业",
    ),
    // 会话外的需手动清单（需登录的号 · 需登录的 MCP）：每一项带写好的那一句和怎么解。读账号清单 ＋ MCP 缓存 ⇒ 阻塞档。
    crate::wire::typed::blocking::<crate::faces::login_face::ManualListCmd>(
        "会话外的需手动清单：需登录的号 · 需登录的 MCP",
    ),
];
