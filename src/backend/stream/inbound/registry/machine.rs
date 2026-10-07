//! 命令表 · 机器：`ssh-*` · `remote-*` · `deploy-*` · `resident-*` · `place-verdict` · `drift-report` · `powershell-*` · `authorized-keys-add` · `pubkey-push` · `backend-log` · `exit-policy-*` · `relay-optin` · `forward-*` · `ccm-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // **别名预览**（「生成器旁边显示这条别名实际会执行什么，是真验证，
    //   不是前端拼串」）：与 `ccm --print` 同一个计划函数，环境是「这台机器家目录里的一个新终端」。
    //   只读（不起进程、不写盘），阻塞档（读账号库 manifest ＋ 问会话快照）。
    CommandSpec {
        name: "ccm-print",
        summary: "一条别名实际会执行什么",
        codes: &["bad_args", "refused"],
        fields: &[arg("args", "一条别名的预置参数（原样 ccm argv，`<交给 agent 的…> -- <ccm 的…>` 那一形；最多 64 个、每个最长 4096 字节）"), out("line", "`ccm --print` 那一行")],
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
        summary: "这台的 `ccm` 会哪些",
        codes: &[],
        fields: &[out("agents", "认得的 agent 种类"), out("build", "这一份的 `BUILD_ID`"), out("capabilities", "这台 `ccm` 认得的能力（与名片的 `capabilities=` 行同一份）"), out("version", "CLI 契约版本（`CCM_VERSION`）")],
        takes_input: false,
        run: Run::Async(|_r| {
            Box::pin(async move { Ok(Some(crate::control::ccm::answer_probe())) })
        }),
    },
    // **部署计划**：`{dial, carried, machine}` → 换成哪一格 · 落点那一份是谁 · 该不该换 · 旧落点那份删不删。
    //   真异步（拨号 / 等远端）；一个字节都不写（放字节是 monitor 经 `files` 链路的事）。本体 `control/deploy_plan.rs`。
    CommandSpec {
        name: "deploy-plan",
        summary: "那台的后端要不要换、换成哪一格",
        codes: &[
            "bad_args",
            "io_failed",
            "refused",
            "unreachable",
            "undecidable",
        ],
        fields: &[out("ack", "问 `uname` 那一趟拨号的 `DialAck` 原样（逐地址指纹 · 严格与否）：拨号在本机后端里，monitor 按它固化指纹（与自己开链路那几条同一个判定）"), out("action", "`skip`（已是这一版）· `deploy`（没装 / 0 字节 / 更旧 / 从前的三行入口）· `keep`（另一版、不比这一版旧 ⇒ 不动它）"), out("arch", "那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`）"), out("expected", "那一格这一版带着的字节自报的身份（对照物）"), out("label", "那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`）"), out("leftovers", "落点目录里没人要的上传残件（家目录相对，排序）；列不出那个目录 ⇒ `[]`（下次连上再问）"), out("os", "那台是表 A 的哪一格（`label` 说给人听：`Linux / x86_64`）"), out("theirs", "`keep` 时那台上那一份自报的身份，否则 `null`"), out("why", "人读原因（`skip` 时空串）")],
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
    // **远端常驻后端 hello 的新旧**：`{mine, theirs, replaced}` → `{action, older}`（纯判定，不碰盘不拨号 ⇒ 不进阻塞档）。
    //   本体 `control/deploy_plan.rs::answer_resident_verdict`（判定只在后端）。
    CommandSpec {
        name: "resident-verdict",
        summary: "远端常驻后端要不要换一次",
        codes: &["bad_args"],
        fields: &[out("action", "`replace`（换掉再接）· `attach`（接上它）"), arg("mine", "monitor 手上这一版后端自报的身份（非空）"), out("older", "那台是不是比手上这一版旧（与 `replaced` 无关；版本提示那句话按它挑「会换掉」还是「不会换回去」）"), arg("replaced", "这一趟是不是已经换过一次"), arg("theirs", "那台 hello 报的 `build_id`（缺 ⇒ 空串）")],
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
        summary: "本机那一份放不放",
        codes: &["bad_args", "refused", "undecidable"],
        fields: &[out("action", "`place`（放 / 换上去）· `keep`（盘上那一份不比这一份旧 ⇒ 不动、用它）"), arg("dest", "落点的绝对路径（不在 ⇒ 没装 ⇒ 放；读不了 ⇒ `undecidable`，不当成没装）"), arg("machine", "对人说话时这台叫什么（monitor 交「本机」）"), out("why", "人读原因（`keep` 时点名两边各是哪一版）")],
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
        summary: "把本机公钥推进那台的 `authorized_keys`",
        codes: &["bad_args", "bad_jump", "refused", "failed"],
        fields: &[arg("jump", "同 `remote-probe`"), arg("machine", "同 `remote-probe`"), out("outcome", "`added`（新加的）· `already`（本就有整行相等的一行，没写）"), arg("pubKeyPath", "本机那份 `.pub` 的路径；缺席 / 空 ⇒ 私钥同名 `.pub`（两样都没有 ⇒ `refused`，界面让用户挑文件）"), out("pubPath", "实际推的是哪一份（给人看）"), arg("saved", "同 `remote-probe`"), out("via", "走了哪条：`backend`（那台后端的文件管理面）· `exec`（那一次 exec）")],
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
        summary: "把一行公钥并进这台的 `authorized_keys`",
        codes: &["bad_args", "refused", "io_failed"],
        fields: &[arg("key", "一行公钥（本条自己也校验一遍）"), out("outcome", "`added` · `already`")],
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
        summary: "读「退出行为」那个值（值住后端所在那台）",
        codes: &[],
        fields: &[out("killOnExit", "monitor 退出时结束本机常驻后端"), out("path", "那份文件的路径（在 `~/.cc-monitor/` 下）"), out("reason", "`unreadable` 时的原因"), out("said", "这个值意味着什么的一句话"), out("state", "`absent`（没设过）· `chosen` · `unreadable`（读不出：也是 `ok:true`）")],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::control::exit_policy::answer_read()))),
    },
    CommandSpec {
        name: "exit-policy-set",
        summary: "写「退出行为」那个值，写完读回",
        codes: &["bad_args", "io_failed"],
        fields: &[both("killOnExit", "要写的值（布尔）"), out("path", "同 `exit-policy-read`"), out("reason", "同 `exit-policy-read`"), out("said", "同 `exit-policy-read`"), out("state", "写完再读一遍的状态")],
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
        summary: "直接敲的 agent 也走中转",
        codes: &["failed"],
        fields: &[out("listening", "这台我们的中转此刻在不在听（与 `apikey-routing.running` 同一个判准）"), out("missing", "那一段为什么生成不了（这台的中转还没起来过、没有钥匙 · 决策表不给这一条）；已装 / 生成得了 ⇒ 空串"), out("note", "那份文件为什么读不了（`unreadable` 才有，其余空串）"), out("snippet", "要合并进 `env` 的那一段（**带钥匙**：设置文件里写不了 `$(cat …)`）；`installed` 或生成不了 ⇒ `null`"), out("source", "读的是哪份文件（这台后端看到的路径）"), out("state", "`installed`（写着的就是现在那一条）· `stale`（是我们那一形")],
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
        summary: "这台后端的漂移账",
        codes: &["bad_args"],
        fields: &[out("faces", "各家记录解释面记下的「看不懂的记录」：`face`")],
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
        summary: "起一条本地端口转发",
        codes: &[
            "bad_args",
            "bad_spec",
            "unreachable",
            "bad_jump",
            "full",
            "failed",
        ],
        fields: &[out("id", "这条转发的号（`fwd-<n>`，本进程内单调）"), arg("jump", "跳板那一台的配置（可缺）"), arg("localPort", "绑本机 `127.0.0.1:localPort`，每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip"), arg("machine", "那台的配置（界面 `remote-config` 那一格，camelCase）· 已保存的那一份 · 跳板那一台：可达表里**没有**那台（流没起来）时按它自己组请求去拨（`dial/machine.rs`，同 `remote-probe`），不拒"), arg("origin", "那台的名字（可达表 `remote-reach` 的键：那台的流握手过 ⇒ 用那一份拨号请求）"), arg("remoteHost", "绑本机 `127.0.0.1:localPort`，每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip"), arg("remotePort", "绑本机 `127.0.0.1:localPort`，每接进一条连接开一条到那台 `remoteHost:remotePort` 的 direct-tcpip"), arg("saved", "已保存的那一份机器配置（可缺）")],
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
        summary: "测试连接",
        codes: &["bad_args", "bad_jump", "failed"],
        fields: &[out("backendHello", "那台后端的一句（往返毫秒 · 版本 · 做不到几项）"), out("backendOk", "那台后端答没答（hello ＋ `ping` 往返）"), out("end", "结局，**最后一格**"), out("endpoint", "实际连上的地址"), out("fingerprint", "那台的主机指纹（`sshOk:false` 时不给）"), arg("jump", "跳板那一台的配置（可缺）"), arg("machine", "那台的配置（可能还没保存）：`host` · `port` · `user` · `keyPath` · `addresses` · `jump` …"), out("message", "结局那一句"), out("reached", "`ssh` 握手过了 · `hello` 那台后端回了 hello · `control` ping 往返了"), arg("saved", "已保存的那一份（可缺）"), out("sshOk", "SSH 握手 ＋ 鉴权过没过（结局 `end` 里）"), out("stage", "拨号阶段行，与界面 `ConnectStage` 同形"), both("ticket", "界面交来的票（1..=64 个 `[A-Za-z0-9-]`），进度帧 `probe` 原样回填")],
        takes_input: true,
        run: Run::Builtin,
    },
    CommandSpec {
        name: "forward-stop",
        summary: "停一条转发",
        codes: &["bad_args", "not_found"],
        fields: &[both("id", "转发号（`fwd-<n>`）")],
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
        summary: "列转发",
        codes: &[],
        fields: &[out("connCount", "累计接进的连接数"), out("forwards", "转发清单，每项 `{id, origin, localPort, remoteHost, remotePort, state, connCount}`"), both("id", "转发号"), out("localPort", "本机口"), out("origin", "那台的名字"), out("remoteHost", "那台上的目标 host"), out("remotePort", "那台上的目标口"), out("state", "`running`（链路还在）· `error`（链路自己收工了，留在账上等用户停）")],
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
        summary: "本机后端的可达表登记",
        codes: &["bad_args"],
        fields: &[arg("dial", "那台的拨号请求（同 `link-open` 的 `dial`；只有路径，没有私钥本体）"), arg("origin", "那台的名字（monitor 的 origin 名，本后端只当不透明的键用）"), out("reach", "登记之后的可达表 `[{origin, machine}]`（同 `assets-sync` 的那一格）")],
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
        summary: "这台后端的 stderr 诊断文件尾部",
        codes: &["bad_args", "failed"],
        fields: &[arg("maxBytes", "可选，缺省 = 封顶 256 KiB：只回尾部这么多字节"), out("path", "本进程 stderr 此刻落在的那份文件；没装（stdio 载体 · 没被交路径）⇒ `null`"), out("size", "那份文件的总字节数"), out("text", "尾部正文（lossy UTF-8）；截断时从截点后第一个换行起，不给半行"), out("truncated", "前面还有没回的字节")],
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
        summary: "那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`",
        codes: &["bad_args", "refused"],
        fields: &[arg("host", "哪一代：`powershell`（5.1）· `pwsh`（7）"), out("policy", "设完现问的那一份（形状同 `aliases-read` 候选里的 `policy`）"), out("setError", "设的那一下 PowerShell 的原话（组策略压着时它会报）；`null` = 没报")],
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
        summary: "这台 `~/.ssh/config` 里可点的别名",
        codes: &[],
        fields: &[out("aliases", "`Host` 行里非通配的别名（去重保序）；没有 config ⇒ `[]`")],
        takes_input: false,
        run: Run::Blocking(|_r| Ok(Some(crate::dial::ssh_config::answer_aliases()))),
    },
    CommandSpec {
        name: "ssh-config-resolve",
        summary: "一个别名的有效连接参数",
        codes: &["bad_args", "bad_alias", "failed", "child_timed_out"],
        fields: &[arg("alias", "必填"), out("host", "`ssh -G` 的 `hostname`（缺省回退别名）"), out("keyPath", "第一个**展开后存在**的 `identityfile`（只问在不在，不读内容）；都不存在 ⇒ `null`"), out("port", "`port`（缺省 22）"), out("proxyJump", "`proxyjump`（`none` ⇒ `null`）"), out("user", "`user`（缺省空串）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::dial::ssh_config::answer_resolve(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "ssh-config-import",
        summary: "批量导入预览",
        codes: &["bad_args"],
        fields: &[out("addresses", "其余地址，端口不同则 `host:port`"), out("alias", "成员的别名"), out("groups", "聚成组的机器，每组 `{label, host, port, user, keyPath, addresses, jump, members, inList}`"), both("host", "组首的 host"), out("inList", "组首或任一成员的地址 ＋ 组的用户 ＋ 端口与 `known` 里某台相同，去首尾空白比 ⇒ 已在列表里，界面照它灰、不自己比"), out("jump", "组内首个非空 proxyjump"), out("keyPath", "组首"), arg("known", "机器列表里已有的那几台，`[{host, user, port}]`；缺 / 形状不对 ⇒ `bad_args`"), out("label", "单成员组 = 完整别名，多成员 = 基名"), out("members", "`alias` / `host` / `port` / `proxyJump`，界面「拆分」时据此还原"), both("port", "组首的端口"), out("proxyJump", "成员的跳板"), both("user", "组首的用户")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::dial::ssh_config::answer_import(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
