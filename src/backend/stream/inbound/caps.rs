//! 阻塞档命令的总期限：后端只登记上限，发起方在请求信封里带它这一发愿意等多久（`within_ms`），
//! 分派那一层按它减余量收紧、装在执行这条命令的线程上。两个入口（帧面 · CLI 面）都经这里装。

use crate::control::{session_batch, session_open, terminals};
use crate::platform::child::{Budget, Deadline, Until};
use crate::wire::Request;

/// 发起方期限里留给回程与界面收尾的那一段：后端在发起方放手之前这么久答出那句准话。
pub(crate) const MARGIN_MS: u64 = 2_000;

/// 装总期限的命令 → 上限。没带发起方期限（CLI 面 · 不带这一格的前端）⇒ 按上限；不在表里的命令不装。
pub(crate) const CAPS: &[(&str, Deadline)] = &[
    ("terminals-list", terminals::TERMINALS_LIST_CAP),
    ("terminal-preview", terminals::TERMINAL_PREVIEW_CAP),
    ("terminal-input", terminals::TERMINAL_PREVIEW_CAP),
    ("session-interrupt", terminals::TERMINAL_PREVIEW_CAP),
    // 名单一趟 ＋ 结束 / 接管那一下（托管口每读一下 5 s 期限），同送字那一档。
    ("terminal-end", terminals::TERMINAL_PREVIEW_CAP),
    ("terminal-take-input", terminals::TERMINAL_PREVIEW_CAP),
    ("terminal-release-input", terminals::TERMINAL_PREVIEW_CAP),
    ("sessions-where", session_batch::SESSIONS_WHERE_CAP),
    ("sessions-stop", session_batch::BATCH_CAP),
    ("session-open", session_open::OPEN_CAP),
    ("ssh-config-import", crate::dial::ssh_config::SSH_IMPORT_CAP),
    ("quota-probe", crate::faces::quota_probe_face::PROBE_CAP),
    ("quota-warm", crate::accounts::quota::warm::WARM_CAP),
    ("aliases-read", crate::assets::aliases::ALIASES_READ_CAP),
    (
        "powershell-policy-set",
        crate::assets::aliases::POLICY_SET_CAP,
    ),
    // 开计划页那两问：起 pb 的总期限，发起方放手了 pb 跟着收（超时照读不成那一路写进回包 / 那一句）。
    ("plan-list", crate::faces::plan_face::PLAN_READ_CAP),
    ("plan-read", crate::faces::plan_face::PLAN_READ_CAP),
];

/// 发起方期限 − 余量，从此刻起换成截止时刻（没带 ⇒ `None`；不够余量 ⇒ 此刻就到）。
pub(crate) fn until_of(within_ms: Option<u64>) -> Option<Until> {
    within_ms.and_then(|ms| Until::after(Deadline::millis(ms.saturating_sub(MARGIN_MS))))
}

/// 这条命令登记的总期限上限（不在表里 ⇒ `None`：不装，发起方带的期限对它不起作用）。协议参考按它逐条写。
pub(crate) fn cap_of(cmd: &str) -> Option<Deadline> {
    CAPS.iter()
        .find(|(name, _)| *name == cmd)
        .map(|&(_, cap)| cap)
}

/// 给这条命令装总期限（不在表里 ⇒ 不装）。守卫活着期间有效：在执行这条命令的那根线程上调、跑完才放。
pub(crate) fn install(req: &Request) -> Option<Budget> {
    cap_of(&req.cmd).map(|cap| Budget::capped(cap, req.until))
}
