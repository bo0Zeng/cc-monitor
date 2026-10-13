//! 帧面 `session-fork` 的宿主 —— 与 `read_face` / `footprint` 同形的一层壳：
//! 找家目录（与帧面其余几条同一个出处 `observe::history_query::agent_home`）、交给本体；
//! 再收齐「分叉之后起」要的源会话事实（记录里的工作目录 · 进程名单 · 终端名单），交推断（`control/fork_launch.rs`）出 `launch` 那一格。
//!
//! # 为什么要这一层壳（而不是 `control/fork_write.rs` 自己找家）
//!
//! 本体住 `control/`，而 `control → observe` 是反向边（`layering_guard` 零容忍）；本体直接问适配层要配置根
//! 又会让「加一家 agent 要回来改的地方」多一处（`agent_locality_guard` 那张只许降的表）。CLI 那一面的家目录
//! 由 `main.rs` 解析好传进去；帧面没有那一跳 ⇒ 由这层顶层壳补上，与 `read_face` 拿家目录是同一句。
//!
//! **写盘只在本体**：本文件领一只只会新建的手（`NewFile`）交给 `control/fork_write.rs`（那一处 `O_EXCL` 新建），自己只转交与读。

use crate::control::fork_launch;
use crate::control::fork_write::{ForkErr, ForkReq, Forked};
use crate::stream::inbound::typed::{Blocking, Command, Cx};
use std::path::Path;

/// `session-fork`：从某条消息处分叉出一个新会话。读整份 jsonl ⇒ 阻塞档。
pub(crate) struct SessionFork;

impl Command for SessionFork {
    const NAME: &'static str = "session-fork";
    type Req = ForkReq;
    type Reply = Forked;
    type Err = ForkErr;
}

impl Blocking for SessionFork {
    fn run(req: ForkReq, _cx: &Cx) -> Result<Forked, ForkErr> {
        let home = crate::observe::history_query::agent_home();
        // 分叉写分支记录：这一条在这里领一只只会新建的手（别的命令拿不到它）。
        let nf = crate::platform::fs::NewFile::issue();
        crate::control::fork_write::fork_at(&nf, &home, &req, &|source, sid| {
            launch_of(&home, source, sid)
        })
    }
}

/// 源会话的三格事实。名单读不出 ⇒ 那一半落「不知道 / 不在任何终端里」（同推断的规矩），不挡分叉。
pub(crate) fn launch_of(home: &Path, source: &Path, sid: &str) -> fork_launch::Launch {
    let cwd = crate::agents::project_dir_of(source);
    let processes: Vec<fork_launch::ProcessRow> =
        crate::observe::accounts_query::session_lines_for_frame(home)
            .iter()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter_map(|v| fork_launch::process_row_of(&v))
            .collect();
    let terminals = super::session_batch_face::tmux_rows().ok().flatten();
    fork_launch::infer(&fork_launch::source_of(
        &processes,
        terminals.as_deref(),
        sid,
        cwd,
    ))
}
