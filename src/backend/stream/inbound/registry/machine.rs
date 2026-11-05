//! 命令表 · 机器：`ssh-*` · `remote-*` · `deploy-*` · `resident-*` · `place-verdict` · `drift-report` · `powershell-*` · `authorized-keys-add` · `pubkey-push` · `backend-log` · `exit-policy-*` · `forward-*` · `ccm-*`。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // **别名预览**（「生成器旁边显示这条别名实际会执行什么，是真验证，
    //   不是前端拼串」）：与 `ccm --print` 同一个计划函数，环境是「这台机器家目录里的一个新终端」。
    //   只读（不起进程、不写盘），阻塞档（读账号库 manifest ＋ 问会话快照）。
    super::typed::blocking::<crate::faces::profiles_face::CcmPrint>("一条别名实际会执行什么"),
    // **这台的 `ccm` 会哪些 ＋ 终端里敲它走到哪**：`ccm` 就是这台后端本身；那一格起一次登录 shell 问 `PATH`
    //   （在 `spawn_blocking` 里等，异步档 ⇒ 撤得动：撤了不等它，那个 shell 到期限自己收）。
    super::typed::asynchronous::<crate::control::ccm::CcmProbe>(
        "这台的 `ccm` 会哪些、终端里敲它走到哪",
    ),
    // **用户级 `PATH` 那一格**（只 Windows）：`{op: read|add|remove}` → 做完现问的状态 ＋ 那一句 ＋ 那段命令（起 PowerShell ⇒ 阻塞档）。
    super::typed::blocking::<crate::control::ccm::user_path::CcmUserPath>(
        "`ccm` 所在目录在不在用户级 `PATH` 上 · 加 · 撤",
    ),
    // **给那台装 / 升级后端**：`{dial, slots, machine}` → `{action, said, build, os, ack}`；每走一步记进那台的状态成品（推 `changed {machine}`）。
    //   本体 `control/deploy_plan.rs`（编排住 `deploy` crate，手机同一份）。
    super::typed::asynchronous::<crate::control::deploy_plan::DeployInstall>("给那台装 / 升级后端"),
    // **卸那台的后端**：入参同上 → `{removed, said}`（传一份暂存件、由它删落点那一份；拨号 · 等远端 ⇒ 异步档）。
    super::typed::asynchronous::<crate::control::deploy_plan::DeployUninstall>("卸那台的后端"),
    // **那台远端此刻的状态**：机器表里每台的成品（`machine-read`）。那台的常驻流归本后端（`control/feeds.rs`），每一轮自己记；
    //   判定在 `deploy::machine`，表在 `control/machine_state.rs`（进程里的状态 ⇒ 只在流上答）。
    super::typed::blocking::<crate::control::machine_state::MachineReadCmd>(
        "机器表里每台此刻的状态",
    ),
    // **命令面板打机器名**（`control/machine_menu.rs`）：认那台 · 那台能做的几样（头是上面那份成品）。只读机器表与 `~/.ssh/config` ⇒ 阻塞档。
    super::typed::blocking::<crate::control::machine_menu::MachineFindCmd>(
        "打的字是不是恰好一台（名字 · 本机 · ssh 配置的 Host · 名字后跟第几条）",
    ),
    super::typed::blocking::<crate::control::machine_menu::MachineMenuCmd>(
        "那台此刻能做的几样（打开终端 · 新建会话 · 文件 · 机器状态 · 加入机器表）",
    ),
    // **机器页「起」·［重试］/「停」**：那台的常驻流住本后端 ⇒ 叫醒它 / 先停它、再在那台跑 `--resident-stop`（拨号 ⇒ 异步档）。
    super::typed::asynchronous::<crate::control::feeds::MachineWake>(
        "叫醒那台的常驻流（不再停着、断着就马上再接）",
    ),
    super::typed::asynchronous::<crate::control::feeds::MachineStop>(
        "停那台的常驻流、再停那台的常驻后端",
    ),
    // **本机那一份放不放**：`{dest, machine}` → `{action: place|keep, why}`（只读落点那一个文件 ⇒ 阻塞档）。
    //   monitor 放本机后端之前还没有常驻后端 ⇒ 跑手上那份字节的 CLI 面问它（`--place-verdict`）。本体 `control/deploy_plan.rs::answer_place`。
    super::typed::blocking::<crate::control::deploy_plan::PlaceVerdict>("本机那一份放不放"),
    // **公钥一键推送**（本机常驻后端答）：`{machine, saved?, jump?, pubKeyPath?}` → `{said}`（那一句后端写好）。
    //   那台在可达表里 ⇒ 问它 `authorized-keys-add`；不在 ⇒ 沿池里那条 SSH 一次 exec（只写这一件）。本体 `assets/pubkey.rs`。
    super::typed::asynchronous::<crate::assets::pubkey::PubkeyPush>(
        "把本机公钥推进那台的 `authorized_keys`",
    ),
    // **公钥写进这台的 `authorized_keys`**（被写那台答）：算经 `assets/pubkey.rs`，写经 [`LocalFiles`]（CAS ＋ 建父目录 ＋ 700 / 600）。
    super::typed::blocking::<crate::assets::pubkey::AuthorizedKeysAdd>(
        "把一行公钥并进这台的 `authorized_keys`",
    ),
    // 「退出行为」那个值的两条命令 —— 值住**后端所在那台机器**
    //   （`~/.cc-monitor/backend.json`），前端要读要改都经这两条，**前端从不碰那个文件**。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断 ⇒ `cancel` 命中回 `not_cancellable`。
    //   ⚠ `exit-policy-read` **没有错误码**：「读不出来」是一个**状态**（`state: "unreadable"` ＋ `reason`），
    //     不是一次失败 —— 调用方要的就是那一句「读不出来，按默认办」（`§3.3b ⑤`）。
    //   ⚠ CLI 面同样是派生的必然（`stream::cli::cli_exposed`），理由同 `files-create` 那一段（文件管理那一族开头）。
    super::typed::blocking::<crate::control::exit_policy::ExitPolicyRead>(
        "读「退出行为」那个值（值住后端所在那台）",
    ),
    super::typed::blocking::<crate::control::exit_policy::ExitPolicySetCmd>(
        "写「退出行为」那个值，写完读回",
    ),
    // 这台的偏好（`~/.cc-monitor/prefs.json`：启动命令 · 恢复进不进 tmux · 每号模型 · 上下文上限表）：住这台，前端只经这两条读写。
    super::typed::blocking::<crate::control::machine_prefs::PrefsRead>("读这台的偏好"),
    super::typed::blocking::<crate::control::machine_prefs::PrefsSetCmd>("改这台的偏好，写完读回"),
    // 这一端的机器表（`~/.cc-monitor/machines.json`：连各台用的名字 · 地址 · 钥匙 · 跳板 · 连不连）：住拨号那一端，monitor 只经这几条读写。
    super::typed::blocking::<crate::control::machines::MachinesRead>("读这一端的机器表"),
    super::typed::blocking::<crate::control::machines::MachinesTry>(
        "这串补丁若现在落盘，机器表那一道过不过（盘上不动）",
    ),
    super::typed::blocking::<crate::control::machines::MachinesSetCmd>(
        "改机器表（按键认那一台），写完读回、推 `changed {machines}`",
    ),
    super::typed::blocking::<crate::control::machines::HostKeysForget>(
        "忘掉一台记下的主机指纹，读回机器表、推 `changed {machines}`",
    ),
    // 这里原是 `relay-status` / `relay-ensure`（这台机器上脱离的 `--relay` 在不在 · 起一个）：
    //   中转只住常驻后端进程里（本机远端同形），那一族随回落一形删了。
    // 漂移账出成品（`read_face.rs` 那一臂 ＋ 注册表 `RecordFace.drift`）。纯内存读一把锁，不进阻塞档（同 `forward-list`）。
    super::typed::asynchronous::<crate::faces::read_face::DriftReportCmd>("这台后端的漂移账"),
    // 本机与机器表里每台的漂移账（诊断信息那一份）：界面只问本机后端这一条，别的台由它经那台的常驻流问 `drift-report`、
    //   并好再答（`read_face::drift_hub`）⇒ 异步档（读机器表进阻塞池）。只读。
    super::typed::asynchronous::<crate::faces::read_face::DriftMergeCmd>("本机与各台的漂移账"),
    // 端口转发（F58）的账住本机常驻后端（`dial/forwards.rs`；monitor 那三条 Tauri 命令退役）。
    //   起 = 真异步（查可达表 · 池里那条 SSH 上开 `use: forward` 链路 · 等 ack），`cancel` 能在 await 点打断；
    //   停 / 列 = 纯内存（一把锁），不进阻塞档。三条都只在流面上有意义（`stream::cli::STREAM_ONLY`）。
    super::typed::asynchronous::<crate::dial::forwards::ForwardStart>("起一条本地端口转发"),
    // 〔「后端持有全部 SSH」〕测试连接：
    //   真异步（拨号 · 读 hello · 控制通道往返；本后端零定时器，期限归发起方），`cancel` 能在 await 点打断；短命探活、不进连接池。
    // 进度边拨边推（`probe` 帧，走本连接的应答通道）⇒ `Run::Builtin`：只在帧面，分派在 `dispatch` 那条硬臂。
    super::typed::frame::<crate::dial::probe::RemoteProbe>("测试连接"),
    super::typed::asynchronous::<crate::dial::forwards::ForwardStop>("停一条转发"),
    super::typed::asynchronous::<crate::dial::forwards::ForwardList>("列转发"),
    super::typed::blocking::<crate::faces::read_face::BackendLog>("这台后端的 stderr 诊断文件尾部"),
    // 起一次那一代 PowerShell（同步子进程）⇒ 阻塞档。
    super::typed::blocking::<crate::assets::aliases::PowershellPolicySet>(
        "那一代 PowerShell 的执行策略设成当前用户 `RemoteSigned`",
    ),
    // `~/.ssh/config` 的解读（`dial/ssh_config.rs`）：读一份文件 ／ 逐个起 `ssh -G`（只读配置、不建连接）并等它退出 ⇒ 阻塞档。
    super::typed::blocking::<crate::dial::ssh_config::SshConfigImport>("批量导入预览"),
];

#[cfg(test)]
#[path = "../../../../../tests/backend/control/machines_replay_tests.rs"]
mod machines_replay_tests;
