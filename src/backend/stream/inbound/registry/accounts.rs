//! 命令表 · 账号：`accounts-*` · `apikey-*` · `quota-read` · `quota-probe` · `rotation-*`。

use crate::stream::inbound::spec::{CommandSpec, Run};
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
        doc_anchor: Some("#### `quota-read`"),
        codes: &[],
        fields: &[
            "accounts",
            "earliestReturn",
            "now",
            "path",
            "reason",
            "state",
            "unseen",
            "usableNow",
        ],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::faces::rotation_face::answer_quota_read()))),
    },
    // 用某个号查一次额度（帧面宿主 `faces/quota_probe_face.rs`）：起官方客户端、等它 ⇒ 阻塞档，总期限登记在 `caps.rs`。
    CommandSpec {
        name: "quota-probe",
        doc_anchor: Some("#### `quota-probe`"),
        codes: &[
            "bad_args",
            "child_timed_out",
            "failed",
            "io_failed",
            "not_found",
            "unsupported",
        ],
        fields: &[
            "account", "agent", "from", "now", "path", "reason", "state", "windows",
        ],
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
        doc_anchor: Some("#### `rotation-read`"),
        codes: &[],
        fields: &["followers", "path", "reason", "rotation", "state"],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::faces::rotation_face::answer_read()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-set",
        doc_anchor: Some("#### `rotation-set`"),
        codes: &["bad_args", "io_failed"],
        fields: &["followers", "path", "reason", "rotation", "state"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::rotation_face::answer_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-session-read",
        doc_anchor: Some("#### `rotation-session-read`"),
        codes: &["bad_args", "failed"],
        fields: &["now", "reason", "sessions", "sids", "state"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::rotation_face::answer_session_read(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-session-set",
        doc_anchor: Some("#### `rotation-session-set`"),
        codes: &["bad_args", "failed", "io_failed"],
        fields: &["agent", "rotation", "sessions", "sids", "start"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::rotation_face::answer_session_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "rotation-switch",
        doc_anchor: Some("#### `rotation-switch`"),
        codes: &["bad_args", "failed"],
        fields: &["mode", "sessions", "target"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::faces::rotation_switch_face::answer_switch(r.args, r.until)
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
        doc_anchor: Some("#### `apikey-key-set`"),
        codes: &["bad_args", "bad_file", "io_failed"],
        // 入 `configDir`（账号 id 由后端推）· 出 `account`（推出来的那个）。
        fields: &["account", "baseUrl", "configDir", "key", "masked", "path"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::accounts::upstream_select::file_face::answer_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "apikey-read",
        doc_anchor: Some("#### `apikey-read`"),
        codes: &[],
        // `rows` 退出线上：「表里有哪几行」只在这台后端里用（`file_face::rows_at`，三处读者同一份）。
        fields: &["configured", "masked", "notice", "path", "problem"],
        takes_input: false,
        run: Run::Blocking(|_r| {
            crate::accounts::upstream_select::file_face::answer_read()
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "apikey-routing",
        doc_anchor: Some("#### `apikey-routing`"),
        codes: &["bad_args"],
        fields: &["routed", "running"],
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
        doc_anchor: Some("#### `accounts-list`"),
        codes: &["bad_args", "too_large"],
        fields: &["accounts", "agent", "meta", "notice"],
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
        doc_anchor: Some("#### `machine-interrupts`"),
        codes: &["bad_args"],
        fields: &[
            "forwards",
            "liveStreams",
            "machine",
            "relayedMaybe",
            "relayedSessions",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-sessions",
        doc_anchor: Some("#### `accounts-sessions`"),
        codes: &["too_large"],
        fields: &["lines"],
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
        doc_anchor: Some("#### `accounts-trust`"),
        codes: &[
            "bad_args",
            "failed",
            "manifest_unavailable",
            "no_home",
            "unknown_config_dir",
            "unsafe_config_dir",
        ],
        fields: &["configDir", "cwd", "known", "trusted"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 改账号库的那几条：本体 `accounts/manage/`，帧面宿主 `faces/accounts_face.rs`（建 API 号写 key · 账号表变了重写别名文件）。
    //   写经这台的文件管理面（[`LocalFiles`]）；同步文件 I/O ⇒ 阻塞档。`accounts-verify` / `accounts-login-cmd` 只读。
    CommandSpec {
        name: "accounts-init",
        doc_anchor: Some("#### `accounts-init`"),
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[
            "aliasNames",
            "aliases",
            "applied",
            "backup",
            "dryRun",
            "name",
            "notes",
            "steps",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-add",
        doc_anchor: Some("#### `accounts-add`"),
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[
            "account",
            "aliasNames",
            "aliases",
            "applied",
            "backup",
            "baseUrl",
            "configDir",
            "credFile",
            "dryRun",
            "isDefault",
            "key",
            "keyMasked",
            "keyProblem",
            "kind",
            "loginCmd",
            "name",
            "notes",
            "steps",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-remove",
        doc_anchor: Some("#### `accounts-remove`"),
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[
            "aliases", "applied", "backup", "dryRun", "force", "name", "notes", "steps",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-set-default",
        doc_anchor: Some("#### `accounts-set-default`"),
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[
            "aliases", "applied", "backup", "dryRun", "name", "notes", "steps",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-repair",
        doc_anchor: Some("#### `accounts-repair`"),
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &["aliases", "applied", "backup", "dryRun", "notes", "steps"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-isolate",
        doc_anchor: Some("#### `accounts-isolate`"),
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[
            "aliases", "applied", "backup", "dryRun", "item", "notes", "steps",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-rollback",
        doc_anchor: Some("#### `accounts-rollback`"),
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &["aliases", "applied", "backup", "dryRun", "notes", "steps"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-verify",
        doc_anchor: Some("#### `accounts-verify`"),
        codes: &["bad_args", "io_failed", "unsupported"],
        fields: &[
            "account", "checks", "fails", "level", "pass", "text", "warns",
        ],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-login-cmd",
        doc_anchor: Some("#### `accounts-login-cmd`"),
        codes: &["bad_args", "io_failed", "refused", "unsupported"],
        fields: &["cmd", "name"],
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
        doc_anchor: Some("#### `accounts-mcp-read`"),
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[
            "changed",
            "choices",
            "conflicts",
            "enabled",
            "from",
            "gone",
            "holders",
            "name",
            "notes",
            "servers",
        ],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-mcp-remove",
        doc_anchor: Some("#### `accounts-mcp-remove`"),
        codes: &["bad_args", "io_failed", "not_found", "refused"],
        fields: &[
            "changed",
            "choices",
            "conflicts",
            "enabled",
            "from",
            "gone",
            "holders",
            "name",
            "notes",
            "servers",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-mcp-pick",
        doc_anchor: Some("#### `accounts-mcp-pick`"),
        codes: &["bad_args", "io_failed", "not_found", "refused"],
        fields: &[
            "changed",
            "choices",
            "conflicts",
            "enabled",
            "from",
            "gone",
            "holders",
            "name",
            "notes",
            "servers",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, &ACCOUNT_KEYS)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
