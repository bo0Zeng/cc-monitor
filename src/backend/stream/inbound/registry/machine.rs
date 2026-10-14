//! 命令表 · 机器：`ssh-*` · `remote-*` · `deploy-*` · `resident-*` · `place-verdict` · `drift-report` · `powershell-*` · `authorized-keys-add` · `pubkey-push` · `backend-log` · `exit-policy-*` · `relay-optin` · `forward-*` · `ccm-*`。

use crate::stream::inbound::LocalFiles;
use crate::wire::spec::{arg, both, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // **别名预览**（「生成器旁边显示这条别名实际会执行什么，是真验证，
    //   不是前端拼串」）：与 `ccm --print` 同一个计划函数，环境是「这台机器家目录里的一个新终端」。
    //   只读（不起进程、不写盘），阻塞档（读账号库 manifest ＋ 问会话快照）。
    crate::wire::typed::blocking::<crate::control::ccm::CcmPrint>("一条别名实际会执行什么"),
    // **这台的 `ccm` 会哪些**：`ccm` 就是这台后端本身，「PATH 上那个是谁」退化成问它自己。
    //   纯函数（拼 `--ccm-probe` 那几行），不起进程、不碰盘 ⇒ 不进阻塞档（同 `ping` 那一形）。
    crate::wire::typed::asynchronous::<crate::control::ccm::CcmProbe>("这台的 `ccm` 会哪些"),
    // **部署计划**：`{dial, carried, machine}` → 换成哪一格 · 落点那一份是谁 · 该不该换 · 旧落点那份删不删。
    //   真异步（拨号 / 等远端）；一个字节都不写（放字节是 monitor 经 `files` 链路的事）。本体 `control/deploy_plan.rs`。
    crate::wire::typed::asynchronous::<crate::control::deploy_plan::DeployPlan>(
        "那台的后端要不要换、换成哪一格",
    ),
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
    crate::wire::typed::blocking::<crate::control::deploy_plan::PlaceVerdict>("本机那一份放不放"),
    // **公钥一键推送**（本机常驻后端答）：`{machine, saved?, jump?, pubKeyPath?}` → `{outcome, pubPath, via}`。
    //   那台在可达表里 ⇒ 问它 `authorized-keys-add`；不在 ⇒ 沿池里那条 SSH 一次 exec（只写这一件）。本体 `assets/pubkey.rs`。
    crate::wire::typed::asynchronous::<crate::assets::pubkey::PubkeyPush>(
        "把本机公钥推进那台的 `authorized_keys`",
    ),
    // **公钥写进这台的 `authorized_keys`**（被写那台答）：算经 `assets/pubkey.rs`，写经 [`LocalFiles`]（CAS ＋ 建父目录 ＋ 700 / 600）。
    crate::wire::typed::with_files::<crate::assets::pubkey::AuthorizedKeysAdd>(
        "把一行公钥并进这台的 `authorized_keys`",
        &LocalFiles,
    ),
    // 「退出行为」那个值的两条命令 —— 值住**后端所在那台机器**
    //   （`~/.cc-monitor/backend.json`），前端要读要改都经这两条，**前端从不碰那个文件**。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断 ⇒ `cancel` 命中回 `not_cancellable`。
    //   ⚠ `exit-policy-read` **没有错误码**：「读不出来」是一个**状态**（`state: "unreadable"` ＋ `reason`），
    //     不是一次失败 —— 调用方要的就是那一句「读不出来，按默认办」（`§3.3b ⑤`）。
    //   ⚠ CLI 面同样是派生的必然（`stream::cli::cli_exposed`），理由同 `files-create` 那一段（文件管理那一族开头）。
    crate::wire::typed::blocking::<crate::control::exit_policy::ExitPolicyRead>(
        "读「退出行为」那个值（值住后端所在那台）",
    ),
    crate::wire::typed::owning::<crate::control::exit_policy::ExitPolicySetCmd>("写「退出行为」那个值，写完读回"),
    // 这台的偏好（`~/.cc-monitor/prefs.json`：启动命令 · 恢复进不进 tmux · 每号模型 · 上下文上限表）：住这台，前端只经这两条读写。
    crate::wire::typed::blocking::<crate::control::machine_prefs::PrefsRead>("读这台的偏好"),
    crate::wire::typed::owning::<crate::control::machine_prefs::PrefsSetCmd>("改这台的偏好，写完读回"),
    // 这一端的机器表（`~/.cc-monitor/machines.json`：连各台用的名字 · 地址 · 钥匙 · 指纹 · 跳板 · 连不连）：住拨号那一端，monitor 只经这三条读写。
    crate::wire::typed::blocking::<crate::control::machines::MachinesRead>("读这一端的机器表"),
    crate::wire::typed::blocking::<crate::control::machines::MachinesTry>("这串补丁若现在落盘，机器表那一道过不过（盘上不动）"),
    crate::wire::typed::owning::<crate::control::machines::MachinesSetCmd>("改机器表（按键认那一台），写完读回、推 `changed {machines}`"),
    // 直接敲的那一家也走中转（可选、用户自己贴）：读这台那份用户级设置文件（同步文件 I/O ⇒ 阻塞档），出状态 ＋ 要贴的那一段。
    crate::wire::typed::blocking::<crate::faces::relay_optin_face::RelayOptin>(
        "直接敲的 agent 也走中转",
    ),
    // 这里原是 `relay-status` / `relay-ensure`（这台机器上脱离的 `--relay` 在不在 · 起一个）：
    //   中转只住常驻后端进程里（本机远端同形），那一族随回落一形删了。
    // 漂移账出成品（`read_face.rs` 那一臂 ＋ 注册表 `RecordFace.drift`）。纯内存读一把锁，不进阻塞档（同 `forward-list`）。
    crate::wire::typed::asynchronous::<crate::faces::read_face::DriftReportCmd>("这台后端的漂移账"),
    // 端口转发（F58）的账住本机常驻后端（`dial/forwards.rs`；monitor 那三条 Tauri 命令退役）。
    //   起 = 真异步（查可达表 · 池里那条 SSH 上开 `use: forward` 链路 · 等 ack），`cancel` 能在 await 点打断；
    //   停 / 列 = 纯内存（一把锁），同 `remote-reach` 不进阻塞档。三条都只在流面上有意义（`stream::cli::STREAM_ONLY`）。
    crate::wire::typed::asynchronous::<crate::dial::forwards::ForwardStart>("起一条本地端口转发"),
    // 〔「后端持有全部 SSH」〕测试连接：
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
    crate::wire::typed::asynchronous::<crate::dial::forwards::ForwardStop>("停一条转发"),
    crate::wire::typed::asynchronous::<crate::dial::forwards::ForwardList>("列转发"),
    // **可达表登记**：monitor（宿主，只交事实）在每台远端流握手成功那一刻交「怎么够到那台」
    //   （拨号请求 ＋ 那台后端的路径），本机后端记进内存可达表（`remote_ask`，后端重启就空）。**只登记，不拨号**：
    //   之后「本机后端问远端后端」的两路（资产目录同步 · 历史跨机 join）都查这张表。老远端也登记（历史问它的是老子命令）。
    crate::wire::typed::asynchronous::<crate::faces::remote_reach_face::RemoteReach>("本机后端的可达表登记"),
    crate::wire::typed::blocking::<crate::faces::read_face::BackendLog>("这台后端的 stderr 诊断文件尾部"),
    // 起一次那一代 PowerShell（同步子进程）⇒ 阻塞档。
    crate::wire::typed::blocking::<crate::assets::aliases::PowershellPolicySet>(
        "那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`",
    ),
    // `~/.ssh/config` 的解读（`dial/ssh_config.rs`）：读一份文件 ／ 逐个起 `ssh -G`（只读配置、不建连接）并等它退出 ⇒ 阻塞档。
    crate::wire::typed::blocking::<crate::dial::ssh_config::SshConfigImport>("批量导入预览"),
];
