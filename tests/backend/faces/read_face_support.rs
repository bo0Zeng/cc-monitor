//! 只读查询帧面宿主判据共用的夹具：按命令名问本族（[`answer_at`]）· 本族点名的那几条（[`FAMILY`]）。
//! 本宿主的判据与主 crate 里引到命令表 · 出口投影的那几条（`tests/backend/faces/on_upper_tests.rs`，经 `test-support`）都读它。

use serde_json::Value;
use std::path::Path;

/// 本族的应答：`data` 或失败（码 ＋ 那一句）（判据这一层按登记表那一层的做法走一遍之后的样子）。
pub type Answer = Result<Value, crate::common::said::Said>;

/// 读一份会话记录的那几条换成了 typed（`faces/history_face.rs`）：判据照旧按命令名问，这里照登记表那一层的做法走一遍
///   （入参经 typed 那一处解、回包过一遍真序列化器、失败落成码 ＋ 那一句），盖住 `super::*` 里的同名口；别的命令照旧交只读宿主。
pub fn answer_at(home: &Path, cmd: &str, args: &Value, tz: &crate::common::time::Tz) -> Answer {
    use crate::faces::history_face as h;
    use crate::wire::typed::{take, Codes, Command};
    fn via<C: Command>(args: &Value, f: impl FnOnce(C::Req) -> Result<C::Reply, C::Err>) -> Answer {
        let code = |c: &str| -> &'static str {
            <C::Err as Codes>::ALL
                .iter()
                .find(|k| **k == c)
                .copied()
                .unwrap_or_else(|| panic!("码表外的码 `{c}`"))
        };
        let req = take::<C::Req>(args)
            .map_err(|e| crate::common::said::Said::new(code(e.code_or_fallback()), e.said))?;
        let got = f(req).map_err(|e| {
            let fail = e.into_said();
            crate::common::said::Said::new(code(fail.code_or_fallback()), fail.said)
        })?;
        Ok(serde_json::to_value(got).expect("回包序列化"))
    }
    match cmd {
        "history-read" => via::<h::HistoryReadCmd>(args, |r| h::read_at(home, r, tz)),
        "history-page" => via::<h::HistoryPageCmd>(args, |r| h::page_at(home, r, tz)),
        "history-lines" => via::<h::HistoryLinesCmd>(args, |r| h::lines_at(home, r, tz)),
        "history-image" => via::<h::HistoryImageCmd>(args, |r| h::image_at(home, r)),
        "history-run" => via::<h::HistoryRunCmd>(args, |r| h::run_at(home, r, tz)),
        "history-branch" => via::<h::HistoryBranchCmd>(args, |r| h::branch_at(home, r)),
        "history-tail" => via::<h::HistoryTailCmd>(args, |r| h::tail_at(home, r, tz)),
        "history-record" => via::<h::HistoryRecordCmd>(args, |r| h::record_at(home, r)),
        "sessions-needs" => {
            via::<h::SessionsNeedsCmd>(args, |_| Ok(h::needs_at(home, &[], Ok(Vec::new()))))
        }
        "history-index" => via::<h::HistoryIndexCmd>(args, |r| h::index_at(home, r)),
        "history-user-inputs" => via::<h::UserInputsCmd>(args, |r| h::user_inputs_at(home, r)),
        "history-turns" => via::<h::HistoryTurnsCmd>(args, |r| h::turns_at(home, r, tz)),
        "history-find" => via::<h::HistoryFindCmd>(args, |r| h::find_at(home, r, tz)),
        "history-facts" => via::<h::HistoryFactsCmd>(args, |r| {
            // 上限表住这台的偏好：夹具家目录里那份 `prefs.json`（没有 ⇒ 空表）。
            let limits =
                crate::common::machine_prefs::read_at(&crate::common::machine_prefs::path_in(home))
                    .prefs()
                    .limits();
            h::facts_at(home, r, &limits, tz)
        }),
        "backend-log" => via::<super::BackendLog>(args, |r| {
            crate::wire::spec::finish(<super::BackendLog as crate::wire::typed::Handle>::run(
                r,
                crate::wire::typed::Cx {
                    tz: tz.clone(),
                    until: None,
                    needs: (),
                },
            ))
        }),
        "machine-interrupts" => {
            via::<super::MachineInterrupts>(args, |r| Ok(super::interrupts_at(home, &r)))
        }
        "accounts-list" => via::<super::AccountsListCmd>(args, |r| {
            crate::wire::spec::finish(<super::AccountsListCmd as crate::wire::typed::Handle>::run(
                r,
                crate::wire::typed::Cx {
                    tz: tz.clone(),
                    until: None,
                    needs: (),
                },
            ))
        }),
        other => Err(crate::common::said::Said::new(
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 本族的八条帧命令 —— **要求点名的那八条**，
/// 写成帧面名。它是判据的**异源**那一侧：判据从 `stream/inbound/` 源码里数「谁把活交给了
/// `read_face::answer`」，两边必须相等。
/// +2：`history-index` / `history-user-inputs`（要求「`--list-user-inputs` 与骨架
/// `--read-session-from-offset --index` 上帧面」那一句 —— 异源仍是手抄的要求，不是 `stream/inbound/`）。
pub const FAMILY: &[&str] = &[
    "accounts-list",
    "machine-interrupts",
    "history-index",
    "history-user-inputs",
    // 按行号取回（异源是手抄的要求「后端给『从第 N 行起 k 行』的读口」，不是 `stream/inbound/`）。
    "history-lines",
    // 按引用取回一张图（记录行上线只带引用，画到那一格才问）。
    "history-image",
    // 会话内查找。
    "history-find",
    // 会话事实出成品（异源是手抄的要求「三样由后端出成品」，不是 `stream/inbound/`）。
    "history-facts",
    // 这台上需手动的会话清单（异源是手机端的要求「一次问一台，不是一条一问」，不是 `stream/inbound/`）。
    "sessions-needs",
    // 主线外清单的冷读（共用扫描图）。
    "history-branch",
    "history-turns",
    "history-read",
    // 记录还在不在（resume 一跳先问；异源是手抄的要求，不是 `stream/inbound/`）。
    "history-record",
    // 按字节分页出记录行（异源是手抄的要求，不是 `stream/inbound/`）。
    // 按运行读一个子运行的记录（异源是「子 agent 的流归各自的运行、通用层按运行读」那条要求，不是 `stream/inbound/`）。
    "history-run",
    "history-page",
    "history-tail",
];
