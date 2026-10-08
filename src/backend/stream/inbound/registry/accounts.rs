//! 命令表 · 账号：`accounts-*` · `apikey-*` · `quota-read` · `quota-probe` · `rotation-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

/// 改账号库那几条（`accounts-*`）递给执行器的 key 表几口：写 key · 删号清那一行 · 回滚放回去 · 表在哪。
/// 那份表（上游选择自己的状态）的写口只从这里递出去。
const ACCOUNT_KEYS: crate::faces::accounts_face::KeyDoor<'static> =
    crate::faces::accounts_face::KeyDoor {
        set: &crate::accounts::upstream_select::file_face::answer_set,
        drop: &crate::accounts::upstream_select::file_face::answer_drop,
        restore: &crate::accounts::upstream_select::file_face::answer_restore,
        path: &crate::accounts::upstream_select::file_face::machine_path,
    };

pub(super) const SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "quota-read",
        summary: "这台的额度账",
        codes: &[],
        fields: &[out("accounts", "每个号一条，按 `(agent, account)` 排：`agent` 路由第 1 段（哪一家）· `account` 路由第 2 段（哪个号"), out("earliestReturn", "被拒 / 超额在兜的号里最早回来的那个 `{account, at}`；没有、或都说不出时刻 ⇒ `null`"), out("now", "这台此刻的 unix 秒（界面算「几分钟前看到的」「还有多久重置」都按这台的钟）"), out("path", "那份文件的绝对路径（家推不出来时 `null`）"), out("reason", "只在 `unreadable` 时有：为什么读不出来；其余 `null`"), out("state", "`\"present\"`（读得懂）· `\"absent\"`（还没看到过任何回包）· `\"unreadable\"`（文件读不出来 / 家推不出来）"), out("unseen", "账号库里有、额度账上从没出过数的号：`{agent, account, kind, login, subId?}`（几格同下）"), out("usableNow", "此刻发得出去的号（路由第 2 段）：登录拿得到、不是被拒 / 超额在兜（快满 · 数旧 · 没采样 · 上一窗已过都算）")],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::faces::rotation_face::answer_quota_read()))),
    },
    // 用某个号查一次额度（帧面宿主 `faces/quota_probe_face.rs`）：起官方客户端、等它 ⇒ 阻塞档，总期限登记在 `caps.rs`。
    CommandSpec {
        name: "quota-probe",
        summary: "用某个号查一次额度",
        codes: &[
            "bad_args",
            "child_timed_out",
            "failed",
            "io_failed",
            "not_found",
            "unsupported",
        ],
        fields: &[both("account", "账号库里的号（配置目录末段；账号 0 是 `0`）"), both("agent", "路由第 1 段：哪一家"), out("from", "恒 `\"usage\"`"), out("now", "这台此刻的 unix 秒"), out("path", "额度账那份文件的绝对路径"), out("reason", "只在 `unreadable` 时有：哪一处读不懂（英文诊断）"), out("state", "`\"read\"`（读得懂、已记进额度账）· `\"unreadable\"`（输出对不上：**不猜、不写账**）"), out("windows", "读到的窗口（形状同 `reading.windows`）；`unreadable` ⇒ `[]`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::quota_probe_face::answer_probe(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 换号那一族（帧面宿主 `faces/rotation_face.rs`）：默认轮换读 / 写 · 一批会话的轮换与「账号」格读 / 写 · 现在就换。
    //   同步文件 I/O ⇒ 阻塞档；「现在就换」里重启换那一半要等 `session-restart` ⇒ 异步、失败可带码。
    CommandSpec {
        name: "rotation-read",
        summary: "这台的默认轮换",
        codes: &[],
        fields: &[out("followers", "跟随默认轮换、此刻活着的会话有几个（判活同历史清单：pidfile 里的会话 id ＋ 进程还是同一个）"), out("path", "那份文件的绝对路径（家推不出 ⇒ `null`）"), out("reason", "只在 `unreadable` 时有"), out("rotation", "默认轮换；没动过 / 读不出 ⇒ 缺省那一份（只有占位、`\"full\"`、`\"continue\"`）"), out("state", "`\"present\"` · `\"absent\"`（没动过）· `\"unreadable\"`（读不出 / 家推不出）")],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::faces::rotation_face::answer_read()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-set",
        summary: "写这台的默认轮换",
        codes: &["bad_args", "io_failed"],
        fields: &[out("followers", "跟随默认轮换的会话"), out("path", "同 `rotation-read`"), out("reason", "同 `rotation-read`"), arg("rotation", "整份默认轮换 `{order, enabled, when, atLimit?, cap?, stint?, preempt?}`"), out("state", "应答同 `rotation-read`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::rotation_face::answer_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-session-read",
        summary: "一批会话的轮换与「账号」格",
        codes: &["bad_args", "failed"],
        fields: &[out("now", "那份文件的三态（同 `rotation-read`）· 这台此刻的 unix 秒"), out("reason", "那份文件的三态（同 `rotation-read`）· 这台此刻的 unix 秒"), out("sessions", "每个 sid 一份"), arg("sids", "会话 id 的数组"), out("state", "那份文件的三态（同 `rotation-read`）· 这台此刻的 unix 秒")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::rotation_face::answer_session_read(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-session-set",
        summary: "改一批会话的轮换",
        codes: &["bad_args", "failed", "io_failed"],
        fields: &[arg("agent", "这台没见过的会话要另给：哪一家"), arg("rotation", "`\"follow\"` · `\"custom\"` · `{\"custom\":{…}}`"), out("sessions", "逐个结果 `{sid: {state:\"done\"} | {state:\"skipped\", code}}`"), arg("sids", "要改的会话"), arg("start", "起它的号")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::rotation_face::answer_session_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-switch",
        summary: "现在就换",
        codes: &["bad_args", "failed"],
        fields: &[arg("mode", "`hot`（不重启，下一发起就走它）· `restart`（换号重启）"), arg("sessions", "`hot`：会话 id；`restart`：每项是 `session-restart` 的入参（不带 `account`）；应答里是逐个结果"), arg("target", "换到哪个号")],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::faces::rotation_switch_face::answer_switch(r.args, r.until, &LocalFiles)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // **上游选择**那份凭据文件在**这台机器上**的读写口 —— 上游选择自己的状态，
    //   不是用户文件（判清全文）⇒ 写口登记在 `readonly_guard` 第四层，
    //   **只从这里一扇门进来**。远端账号页配的 key 从此落在会话跑的那台机器上。
    //   ⚠ 明文只在 `apikey-key-set` 的 `args.key` 里（帧面：长连接入方向；派生 CLI 面：stdin），
    //     **不进 argv / env / 日志**；两条的应答都只有掩码。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断。
    //   ⚠ 它们**不起中转**、中转那几条也**不碰凭据**（「账号就账号, 中转就中转」）。
    CommandSpec {
        name: "apikey-key-set",
        summary: "给一个账号写 key，写完读回",
        codes: &["bad_args", "bad_file", "io_failed"],
        // 入 `configDir`（账号 id 由后端推）· 出 `account`（推出来的那个）。
        fields: &[out("account", "推出来的账号 id"), both("baseUrl", "入（可选）：这个账号的第三方端点"), arg("configDir", "这个号的账号目录"), arg("key", "明文"), out("masked", "写完**再读一遍**、这一行 key 的掩码（盘上的事实）"), out("path", "那份文件的绝对路径")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::accounts::upstream_select::file_face::answer_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "apikey-read",
        summary: "上游选择凭据文件在这台的状态",
        codes: &[],
        // `rows` 退出线上：「表里有哪几行」只在这台后端里用（`file_face::rows_at`，三处读者同一份）。
        fields: &[out("configured", "**顶层那一把**（历史格式那一行）配没配、掩码"), out("masked", "**顶层那一把**（历史格式那一行）配没配、掩码"), out("notice", "权限过宽 / 查不出来时的一句话（文件不在时 `null`）"), out("path", "那份文件的绝对路径"), out("problem", "读不动 / 解析不了时的一句话")],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::accounts::upstream_select::file_face::answer_read()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "apikey-routing",
        summary: "这几个号在这台的表里有没有行 · 这台的中转在不在",
        codes: &["bad_args"],
        fields: &[out("routed", "传进来的里面、**表里有对应行**的那几个（原样回）"), out("running", "这台机器上**我们的**中转在不在听（读常驻后端进程内的监听状态；中转住这里）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::accounts::upstream_select::endpoint::answer_routing(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 出成品：`{meta, accounts, notice}`，并上这台机器自己那份 apikey 表；`agent` 随请求带（必填）。
    CommandSpec {
        name: "accounts-list",
        summary: "账号清单",
        codes: &["bad_args", "too_large"],
        fields: &[out("accounts", "每账号一个对象，字段同 `--list-accounts` 的账号行"), arg("agent", "必填：这次起会话的是哪一家（适配器 id；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）"), out("meta", "`{enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error, unsupported, nextDefault, home}`"), out("notice", "「能用但有缺」：启用了却一个账号 0 都没有（写清单的那一侧旧到不认账号 0）时的一句话；否则 `null`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么（界面问那台 ＋ 问本机数通往那台的转发，并排画）。只读。
    CommandSpec {
        name: "machine-interrupts",
        summary: "停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前，会打断什么",
        codes: &["bad_args"],
        fields: &[arg("appExit", "可缺；`true` ⇒ 问的是「重启 / 退出 cc-monitor」：这台后端选了随它退出一起停才数会话与账上全部转发，否则全零"), out("forwards", "见 `machine`"), out("liveStreams", "这台活着的会话数（停的那几秒 cc-monitor 里它们不更新）"), arg("machine", "可缺"), out("relayedMaybe", "活着、说不清走不走中转的几个（环境这一刻读不出 / agent 自己的设置可能压过它）"), out("relayedSessions", "这台的活会话里经本机中转走请求的几个（停了就断，直到再启动）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-sessions",
        summary: "正在跑的会话各属哪个账号",
        codes: &["too_large"],
        fields: &[out("lines", "同 `--session-accounts`：每条运行中会话一行")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 换号前的信任预检：`configDir`（缺席 / null = 账号 0）＋ `cwd` → `{trusted, known}`。
    //   同族同档（读一份 manifest ＋ 一份 `.claude.json` ⇒ 阻塞档）、同一个只读宿主。
    CommandSpec {
        name: "accounts-trust",
        summary: "换号前的信任预检",
        codes: &[
            "bad_args",
            "failed",
            "manifest_unavailable",
            "no_home",
            "unknown_config_dir",
            "unsafe_config_dir",
        ],
        fields: &[arg("configDir", "目标账号的 config dir（必须逐字 ∈ manifest，否则 `unknown_config_dir`）；**缺席或 `null` = 账号 0**（读 agent 自己那份用户级配置，不收路径）"), arg("cwd", "要预检的工作目录（必填）"), out("known", "这个账号的用户级配置里有该目录的记录（`false` ⇒ 首次进入，大概率会弹确认）"), out("trusted", "这个账号接受过该目录的信任对话框")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 改账号库的那几条：本体 `accounts/manage/`，帧面宿主 `faces/accounts_face.rs`（建 API 号写 key · 账号表变了改配置文件、重生成别名）。
    //   写经这台的文件管理面（[`LocalFiles`]）；同步文件 I/O ⇒ 阻塞档。`accounts-verify` / `accounts-login-cmd` 只读。
    CommandSpec {
        name: "accounts-init",
        summary: "建账号库",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliasNames", "这个号会拿到的别名名字（`accounts-init` / `accounts-add`，预演时也给；别的命令 ⇒ `[]`）"), out("aliases", "改配置文件（`profiles.toml`）的结局，恰一条 `{path, changed, added, removed, skipped, note}`：加了（基于 `cc` / `cct`、只写自己的号）/ 删了（合下来用这个号的全部段）/ 名字被占跳过的那几段；配置文件有写错的地方 ⇒ 不动、`note` 说一句"), out("applied", "这一趟真改了盘没有"), out("backup", "这一趟留的备份（`~/.cc-monitor/accounts/.backup-<这一段>`，回滚用它）；没改动 ⇒ `null`"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("name", "默认号的名字：过 `shell_quote_core::account_name_ok`（与 `ccm … --account` 同一条），`0` 是保留名"), out("notes", "提示（不挡这一趟），比如共享库里还没有可共享的项"), out("steps", "做了（预演时：将要做）的每一步，一句一行")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-add",
        summary: "新建一个号",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("account", "建出来的那个号：`{name, configDir}`"), out("aliasNames", "同 `accounts-init`"), out("aliases", "同 `accounts-init`"), out("applied", "同 `accounts-init`"), out("backup", "同 `accounts-init`"), arg("baseUrl", "只 API 号：上游地址（缺席 = 默认上游）与 key 明文"), out("configDir", "那个号的配置目录（`account` 里）"), arg("credFile", "只订阅号：导入哪一份凭据（家目录底下的绝对路径或 `~/…`；是链接 / 空文件 / 不在 ⇒ `refused`）"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("isDefault", "可缺席的布尔：真 ⇒ 建好后它是默认号（`ccm` 不带 `--account` 时用它）"), arg("key", "只 API 号：上游地址（缺席 = 默认上游）与 key 明文"), out("keyMasked", "API 号：写进 apikey 表之后的掩码；号建好了 key 却没写进去时那一句（界面据此让人在那一行重填）"), out("keyProblem", "API 号：写进 apikey 表之后的掩码；号建好了 key 却没写进去时那一句（界面据此让人在那一行重填）"), arg("kind", "`\"subscription\"`（订阅号）或 `\"api-key\"`（API 号，清单里写 `authKind: \"api-key\"`）"), out("loginCmd", "订阅号没导入凭据时：在终端里跑这一行登录（`'<家>/.cc-monitor/bin/ccm' -- --account '<名>'`，agent 自己的登录界面）；否则 `null`"), arg("name", "同 `accounts-init`；已有同名号 / 同名目录 ⇒ `refused`"), out("notes", "同 `accounts-init`"), out("steps", "同 `accounts-init`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-remove",
        summary: "删一个号",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), out("applied", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), out("backup", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("force", "可缺席的布尔：删的是默认号时必须给真（剩下的第一个号接着当默认）"), arg("name", "要删的号；`0` · 不认识的号 ⇒ `refused`"), out("notes", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), out("steps", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-set-default",
        summary: "设默认号",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), out("applied", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), out("backup", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("name", "要当默认的号（不认识 ⇒ `refused`）"), out("notes", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), out("steps", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-repair",
        summary: "修复账号库（幂等）",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), out("applied", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), out("backup", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), out("notes", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), out("steps", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-isolate",
        summary: "把一个共享项变成每个号各一份",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`（`aliases` 恒 `[]`：这一条不动账号表）；它不在身份表里 ⇒ `notes` 提示之后「核对」会报它不是共享链接"), out("applied", "同 `accounts-init`（`aliases` 恒 `[]`：这一条不动账号表）；它不在身份表里 ⇒ `notes` 提示之后「核对」会报它不是共享链接"), out("backup", "同 `accounts-init`（`aliases` 恒 `[]`：这一条不动账号表）；它不在身份表里 ⇒ `notes` 提示之后「核对」会报它不是共享链接"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("item", "共享库顶层的一个名字（含 `/` · `.` · `..` · 共享库里没有 · 是后端看会话用的那几项（常驻后端只看共享库那一份）⇒ `refused`）"), out("notes", "同 `accounts-init`（`aliases` 恒 `[]`：这一条不动账号表）；它不在身份表里 ⇒ `notes` 提示之后「核对」会报它不是共享链接"), out("steps", "同 `accounts-init`（`aliases` 恒 `[]`：这一条不动账号表）；它不在身份表里 ⇒ `notes` 提示之后「核对」会报它不是共享链接")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-rollback",
        summary: "按一份备份还原",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名"), out("applied", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名"), both("backup", "入：用哪一份（`.backup-` 后面那一段，只许 `[0-9A-Za-z._-]`、不含 `..`）；缺席 ⇒ 最近一份还没还原过的"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), out("notes", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名"), out("steps", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-verify",
        summary: "核对账号库",
        codes: &["bad_args", "io_failed", "unsupported"],
        fields: &[out("account", "说的是哪个号；全局那几条 ⇒ `null`"), out("checks", "每条 `{level, account, text}`"), out("fails", "`fail`"), out("level", "`ok` · `warn` · `fail` · `skip`"), out("pass", "没有一条 `fail`"), out("text", "给人看的那一句"), out("warns", "`warn` 各几条")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-login-cmd",
        summary: "在终端里登录一个号的那一行",
        codes: &["bad_args", "io_failed", "refused", "unsupported"],
        fields: &[out("cmd", "那一行：这台的 `ccm` 带 `--account` 起 agent（它自己的登录界面）；值一律经唯一的 quote（`shell_quote_core::posix_quote`）"), arg("name", "清单里的一个号（不认识 ⇒ `refused`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 各号共用的用户级 MCP（本体 `accounts/manage/mcp_share_exec.rs`）：读各号的配置文件、只改那一个键、写回共享集合 ⇒ 阻塞档。
    //   成品只有名字与号名，不带定义里的任何值。
    CommandSpec {
        name: "accounts-mcp-read",
        summary: "这台各账号共用的用户级 MCP 此刻的样子",
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[out("changed", "这一趟改写了哪几个号（这一条恒空）"), out("choices", "每一版 `{from, holders, gone}`"), out("conflicts", "两边都改了、等用户挑的那几条：每条 `{name, choices}`"), out("enabled", "这台有没有账号库（没有 ⇒ 不做同步，其余几格为空）"), out("from", "挑这一版时交回 `accounts-mcp-pick` 的 `from`：`null` = 共享的那一版；号名 = 那个号里的那一版"), out("gone", "这一版是「没有这一条」（在 cc-monitor 里删过）"), out("holders", "此刻是这一版的那几个号"), out("name", "那一条的名字"), out("notes", "提示：某个号的配置读不出来（这一趟不同步它）· 没写进去"), out("servers", "共享集合里的名字（排好序）"), out("sync", "各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的、`conflicts` 恒空；见 `accounts-mcp-sync`）")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-mcp-remove",
        summary: "从各账号共用的用户级 MCP 里删一条",
        codes: &["bad_args", "io_failed", "not_found", "refused"],
        fields: &[out("changed", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("choices", "同 `accounts-mcp-read`"), out("conflicts", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("enabled", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("from", "同 `accounts-mcp-read`"), out("gone", "同 `accounts-mcp-read`"), out("holders", "同 `accounts-mcp-read`"), arg("name", "要删的那一条（共享集合里与哪个号里都没有 ⇒ `not_found`）"), out("notes", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("servers", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("sync", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-mcp-pick",
        summary: "两边都改了的那一条用哪一版",
        codes: &["bad_args", "io_failed", "not_found", "refused"],
        fields: &[out("changed", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("choices", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("conflicts", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("enabled", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), arg("from", "→ 那个号里此刻的那一版；缺席 / `null` = 共享的那一版（共享集合里已经删了 ⇒ 删）"), out("gone", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("holders", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), arg("name", "那一条"), out("notes", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("servers", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("sync", "同 `accounts-mcp-read`；停着同步时拒（`refused`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 停 / 开各号之间的同步（只改共享集合那份文件里那一格；开回来那一刻同步一趟）。
    CommandSpec {
        name: "accounts-mcp-sync",
        summary: "停 / 开各账号之间同步用户级 MCP",
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[out("changed", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("choices", "同 `accounts-mcp-read`"), out("conflicts", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("enabled", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("from", "同 `accounts-mcp-read`"), out("gone", "同 `accounts-mcp-read`"), out("holders", "同 `accounts-mcp-read`"), out("name", "同 `accounts-mcp-read`"), out("notes", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), arg("on", "`false` = 停；`true` = 开回来"), out("servers", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("sync", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
