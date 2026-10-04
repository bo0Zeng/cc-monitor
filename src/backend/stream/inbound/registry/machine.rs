//! 命令表 · 机器：`ssh-*` · `remote-*` · `deploy-*` · `resident-*` · `place-verdict` · `drift-report` · `powershell-*` · `authorized-keys-add` · `pubkey-push` · `backend-log` · `exit-policy-*` · `relay-optin` · `forward-*` · `ccm-*`。

use crate::stream::inbound::spec::{CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // **别名预览**（「生成器旁边显示这条别名实际会执行什么，是真验证，
    //   不是前端拼串」）：与 `ccm --print` 同一个计划函数，环境是「这台机器家目录里的一个新终端」。
    //   只读（不起进程、不写盘），阻塞档（读账号库 manifest ＋ 问会话快照）。
    CommandSpec {
        name: "ccm-print",
        doc_anchor: Some("#### `ccm-print`"),
        codes: &["bad_args", "refused"],
        fields: &["args", "line"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::ccm::answer_print(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **这台的 `ccm` 会哪些**：`ccm` 就是这台后端本身，「PATH 上那个是谁」退化成问它自己。
    //   纯函数（拼 `--ccm-probe` 那几行），不起进程、不碰盘 ⇒ 不进阻塞档（同 `ping` 那一形）。
    CommandSpec {
        name: "ccm-probe",
        doc_anchor: Some("#### `ccm-probe`"),
        codes: &[],
        fields: &["agents", "build", "capabilities", "version"],
        takes_input: false,
        run: Run::Async(|_r| {
            Box::pin(async move { Ok(Some(crate::control::ccm::answer_probe())) })
        }),
    },
    // **部署计划**：`{dial, carried, machine}` → 换成哪一格 · 落点那一份是谁 · 该不该换 · 旧落点那份删不删。
    //   真异步（拨号 / 等远端）；一个字节都不写（放字节是 monitor 经 `files` 链路的事）。本体 `control/deploy_plan.rs`。
    CommandSpec {
        name: "deploy-plan",
        doc_anchor: Some("#### `deploy-plan`"),
        codes: &[
            "bad_args",
            "io_failed",
            "refused",
            "unreachable",
            "undecidable",
        ],
        fields: &[
            "ack",
            "action",
            "arch",
            "expected",
            "label",
            "leftovers",
            "legacy",
            "legacy_why",
            "os",
            "theirs",
            "why",
        ],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                let facing = crate::control::deploy_plan::DialFacing::new(
                    r.args
                        .get("dial")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
                crate::control::deploy_plan::answer(&r.args, &facing)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // **那台旧入口的去向**：`{dial}` → `{verdict, expect, why}`（沿池里那条 SSH 开只读 SFTP，stat ＋ 读回，真异步）。
    //   本体 `control/deploy_plan.rs::answer_retired`（与上传残件同一家：落点上该清的东西）。
    // 另一形 `{text}`：本机 PATH 上另一个 `ccm` 的开头一截，只判不读盘（monitor 本机探针拿来说话）。
    CommandSpec {
        name: "deploy-retired",
        doc_anchor: Some("#### `deploy-retired`"),
        codes: &["bad_args", "unreachable"],
        fields: &["dial", "expect", "text", "verdict", "why"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                let facing = crate::control::deploy_plan::DialFacing::new(
                    r.args
                        .get("dial")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                );
                crate::control::deploy_plan::answer_retired(&r.args, &facing)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // **远端常驻后端 hello 的新旧**：`{mine, theirs, replaced}` → `{action, older}`（纯判定，不碰盘不拨号 ⇒ 不进阻塞档）。
    //   本体 `control/deploy_plan.rs::answer_resident_verdict`（判定只在后端）。
    CommandSpec {
        name: "resident-verdict",
        doc_anchor: Some("#### `resident-verdict`"),
        codes: &["bad_args"],
        fields: &["action", "mine", "older", "replaced", "theirs"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::control::deploy_plan::answer_resident_verdict(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // **本机那一份放不放**：`{dest, machine}` → `{action: place|keep, why}`（只读落点那一个文件 ⇒ 阻塞档）。
    //   monitor 放本机后端之前还没有常驻后端 ⇒ 跑手上那份字节的 CLI 面问它（`--place-verdict`）。本体 `control/deploy_plan.rs::answer_place`。
    CommandSpec {
        name: "place-verdict",
        doc_anchor: Some("#### `place-verdict`"),
        codes: &["bad_args", "refused", "undecidable"],
        fields: &["action", "dest", "machine", "why"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::deploy_plan::answer_place(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // **公钥一键推送**（本机常驻后端答）：`{machine, saved?, jump?, pubKeyPath?}` → `{outcome, pubPath, via}`。
    //   那台在可达表里 ⇒ 问它 `authorized-keys-add`；不在 ⇒ 沿池里那条 SSH 一次 exec（只写这一件）。本体 `assets/pubkey.rs`。
    CommandSpec {
        name: "pubkey-push",
        doc_anchor: Some("#### `pubkey-push`"),
        codes: &["invalid_args", "bad_jump", "refused", "failed"],
        fields: &[
            "jump",
            "machine",
            "outcome",
            "pubKeyPath",
            "pubPath",
            "saved",
            "via",
        ],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::assets::pubkey::answer_push(
                    &r.args,
                    &crate::assets::pubkey::Wire,
                    &crate::assets::pubkey::read_local_pub,
                )
                .await
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // **公钥写进这台的 `authorized_keys`**（被写那台答）：算经 `assets/pubkey.rs`，写经 [`LocalFiles`]（CAS ＋ 建父目录 ＋ 700 / 600）。
    CommandSpec {
        name: "authorized-keys-add",
        doc_anchor: Some("#### `authorized-keys-add`"),
        codes: &["bad_args", "refused", "io_failed"],
        fields: &["key", "outcome"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::pubkey::answer_add(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 〔条 66〕「退出行为」那个值的两条命令 —— 值住**后端所在那台机器**
    //   （`~/.cc-monitor/backend.json`），前端要读要改都经这两条，**前端从不碰那个文件**。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断 ⇒ `cancel` 命中回 `not_cancellable`。
    //   ⚠ `exit-policy-read` **没有错误码**：「读不出来」是一个**状态**（`state: "unreadable"` ＋ `reason`），
    //     不是一次失败 —— 调用方要的就是那一句「读不出来，按默认办」（`§3.3b ⑤`）。
    //   ⚠ CLI 面同样是派生的必然（`cli_control::cli_exposed`），理由同 `files-create` 那一段（文件管理那一族开头）。
    CommandSpec {
        name: "exit-policy-read",
        doc_anchor: Some("#### `exit-policy-read`"),
        codes: &[],
        fields: &["killOnExit", "path", "reason", "said", "state"],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::control::exit_policy::answer_read()))),
    },
    CommandSpec {
        name: "exit-policy-set",
        doc_anchor: Some("#### `exit-policy-set`"),
        codes: &["bad_args", "io_failed"],
        fields: &["killOnExit", "path", "reason", "said", "state"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::control::exit_policy::answer_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 直接敲的那一家也走中转（可选、用户自己贴）：读这台那份用户级设置文件（同步文件 I/O ⇒ 阻塞档），出状态 ＋ 要贴的那一段。
    CommandSpec {
        name: "relay-optin",
        doc_anchor: Some("#### `relay-optin`"),
        codes: &["failed"],
        fields: &["listening", "missing", "note", "snippet", "source", "state"],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::accounts::upstream_select::endpoint::answer_optin(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 这里原是 `relay-status` / `relay-ensure`（这台机器上脱离的 `--relay` 在不在 · 起一个）：
    //   中转只住常驻后端进程里（本机远端同形），那一族随回落一形删了。
    // 漂移账出成品（`read_face.rs` 那一臂 ＋ 注册表 `RecordFace.drift`）。纯内存读一把锁，不进阻塞档（同 `forward-list`）。
    CommandSpec {
        name: "drift-report",
        doc_anchor: Some("#### `drift-report`"),
        codes: &["bad_args"],
        fields: &["faces"],
        takes_input: false,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::faces::read_face::answer(&r.cmd, &r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // 端口转发（F58）的账住本机常驻后端（`dial/forwards.rs`；monitor 那三条 Tauri 命令退役）。
    //   起 = 真异步（查可达表 · 池里那条 SSH 上开 `use: forward` 链路 · 等 ack），`cancel` 能在 await 点打断；
    //   停 / 列 = 纯内存（一把锁），同 `remote-reach` 不进阻塞档。三条都只在流面上有意义（`cli_control::STREAM_ONLY`）。
    CommandSpec {
        name: "forward-start",
        doc_anchor: Some("#### `forward-start`"),
        codes: &[
            "invalid_args",
            "bad_spec",
            "unreachable",
            "bad_jump",
            "full",
            "failed",
        ],
        fields: &[
            "id",
            "jump",
            "localPort",
            "machine",
            "origin",
            "remoteHost",
            "remotePort",
            "saved",
        ],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::dial::forwards::answer_start(&r.args)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // 〔「后端持有全部 SSH」〕测试连接（monitor 那条 Tauri 命令 `test_remote_connection` 退役）：
    //   真异步（拨号 · 读 hello · 控制通道往返；本后端零定时器，期限归发起方），`cancel` 能在 await 点打断；短命探活、不进连接池。
    // 进度边拨边推（`probe` 帧，走本连接的应答通道）⇒ `Run::Builtin`：只在帧面，分派在 `dispatch` 那条硬臂。
    CommandSpec {
        name: "remote-probe",
        doc_anchor: Some("#### `remote-probe`"),
        codes: &["invalid_args", "bad_jump", "failed"],
        fields: &[
            "backendHello",
            "backendOk",
            "end",
            "endpoint",
            "fingerprint",
            "jump",
            "machine",
            "message",
            "reached",
            "saved",
            "sshOk",
            "stage",
            "ticket",
        ],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "forward-stop",
        doc_anchor: Some("#### `forward-stop`"),
        codes: &["invalid_args", "not_found"],
        fields: &["id"],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::dial::forwards::answer_stop(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "forward-list",
        doc_anchor: Some("#### `forward-list`"),
        codes: &[],
        fields: &[
            "connCount",
            "forwards",
            "id",
            "localPort",
            "origin",
            "remoteHost",
            "remotePort",
            "state",
        ],
        takes_input: false,
        run: Run::Async(|_r| {
            Box::pin(async move { Ok(Some(crate::dial::forwards::answer_list())) })
        }),
    },
    // **可达表登记**：monitor（宿主，只交事实）在每台远端流握手成功那一刻交「怎么够到那台」
    //   （拨号请求 ＋ 那台后端的路径），本机后端记进内存可达表（`remote_ask`，后端重启就空）。**只登记，不拨号**：
    //   之后「本机后端问远端后端」的两路（资产目录同步 · 历史跨机 join）都查这张表。老远端也登记（历史问它的是老子命令）。
    CommandSpec {
        name: "remote-reach",
        doc_anchor: Some("#### `remote-reach`"),
        codes: &["bad_args"],
        fields: &["dial", "origin", "reach"],
        takes_input: true,
        // 纯内存（一把锁、插一行）⇒ 不进阻塞档，同 `ping` / `resolve`。
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::stream::remote_ask::answer_reach(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "backend-log",
        doc_anchor: Some("#### `backend-log`"),
        codes: &["bad_args", "failed"],
        fields: &["maxBytes", "path", "size", "text", "truncated"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 起一次那一代 PowerShell（同步子进程）⇒ 阻塞档。
    CommandSpec {
        name: "powershell-policy-set",
        doc_anchor: Some("#### `powershell-policy-set`"),
        codes: &["bad_args", "refused"],
        fields: &["host", "policy", "setError"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_policy_set(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // `~/.ssh/config` 的解读（`dial/ssh_config.rs`，从 monitor `stream_source/` 原样搬来）。
    //   阻塞档：读一份文件 ／ 起 `ssh -G`（只读配置、不建连接）并等它退出。
    CommandSpec {
        name: "ssh-config-aliases",
        doc_anchor: Some("#### `ssh-config-aliases`"),
        codes: &[],
        fields: &["aliases"],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::dial::ssh_config::answer_aliases()))),
    },
    CommandSpec {
        name: "ssh-config-resolve",
        doc_anchor: Some("#### `ssh-config-resolve`"),
        codes: &["invalid_args", "bad_alias", "failed", "child_timed_out"],
        fields: &["alias", "host", "keyPath", "port", "proxyJump", "user"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::dial::ssh_config::answer_resolve(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "ssh-config-import",
        doc_anchor: Some("#### `ssh-config-import`"),
        codes: &[],
        fields: &[
            "addresses",
            "alias",
            "groups",
            "host",
            "jump",
            "keyPath",
            "label",
            "members",
            "port",
            "proxyJump",
            "user",
        ],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::dial::ssh_config::answer_import()))),
    },
];
