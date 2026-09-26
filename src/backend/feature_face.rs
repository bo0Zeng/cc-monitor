//! 〔RM1b · 第四波〕**功能侧只读查询的帧面宿主** —— 任务列表 · 插件市场。
//!
//! # 它补的是哪一格
//!
//! 这几样东西此前只有 monitor 进程**直读本机**那一条路（`parity_ledger` 的
//! `session.tasks` / `plugins.marketplaces` 两笔 `ParityDebt`）：远端机器上的同一份数据
//! 答不出来。本机后端与远端后端是**同一个二进制** ⇒ 读法搬进后端，monitor 按 origin
//! 问那一台（本机也走这里），直读那一份随之退役。
//!
//! # 为什么不并进 `read_face`
//!
//! `read_face` 是 `C1` 那八条的宿主，而 monitor 侧有一条两向相等判据
//! （`frame_query_tests::the_moved_table_matches_the_design_list_and_the_backend_registry`）
//! 数的正是「把活交给 `read_face::answer` 的帧命令」== 题面那八条。
//! 本族不在那八条里 —— 并进去就是让那条判据替两件事作证。
//!
//! # 形状与纪律（与 `read_face` 同）
//!
//! - 住顶层、不住 `observe/`：`inbound.rs` 不许出现 `observe::`；本文件只做换壳，
//!   读的本体在 `observe/`（那一层今天就是 Claude 专属的）。
//! - `tasks-list` 的应答**按行**：`{"lines": [...]}` —— monitor 侧用既有的 `frame_query::lines` 收，
//!   **不新增发送端**。〔C4b〕`plugins-marketplaces` 的应答是**成品**（整份 survey），界面经通道直接问。
//!   整份超过 [`crate::read_face::LINES_CAP_BYTES`] ⇒ `too_large`（不截断）。
//! - 不拨号、不起进程、不写盘。

use copy_core::copy_text;
use serde_json::{json, Value};

/// 本族的应答：`data` 或 `(code, message)`。
pub(crate) type Answer = Result<Value, (&'static str, String)>;

/// 帧面入口：命令名从 `r.cmd` 来（与 `read_face::answer` 同一形）。
pub(crate) fn answer(cmd: &str, args: &Value) -> Answer {
    answer_at(&crate::observe::history_query::agent_home(), cmd, args)
}

/// [`answer`] 的本体，家目录是参数（判据拿夹具喂它，不去动进程级环境变量）。
fn answer_at(home: &std::path::Path, cmd: &str, args: &Value) -> Answer {
    match cmd {
        "tasks-list" => {
            let sid = args.get("sid").and_then(Value::as_str).ok_or((
                "bad_args",
                crate::common::contract::malformed("missing `sid` (a string)"),
            ))?;
            lines(crate::observe::tasks_query::session_task_lines(home, sid)?)
        }
        // 〔C4b · 第四波 4B〕应答**就是成品**：整份 survey `{entries, file_absent}`（此前是「恰一行」的 `lines`，
        //   monitor 那一侧再核一遍「恰一行 ＋ 拒收未知字段 ＋ 必填」—— 那一份解释删了，界面经通道直接问、按形状收，
        //   `settings/plugins-section.ts::decodeSurvey`）。
        // 三条出口：文件不在 ⇒ `file_absent: true`（诚实的空）；读 / 解析失败 ⇒ `failed`；
        // 某一条数不出 ⇒ 那一条 `declared_plugins: null` ＋ 理由（整张表照出）。
        "plugins-marketplaces" => {
            let survey = crate::observe::plugins_query::survey_marketplaces_in(home)
                .map_err(|e| ("failed", e))?;
            let v = serde_json::to_value(&survey).map_err(|e| {
                (
                    "failed",
                    crate::common::contract::malformed(&format!(
                        "serializing the marketplace list failed: {e}"
                    )),
                )
            })?;
            let size = v.to_string().len();
            if size > crate::read_face::LINES_CAP_BYTES {
                return Err((
                    "too_large",
                    copy_text(
                        "beFeatureFace.answerAt.tooLarge",
                        &[
                            ("size", &size.to_string()),
                            ("cap", &(crate::read_face::LINES_CAP_BYTES).to_string()),
                        ],
                    ),
                ));
            }
            Ok(v)
        }
        other => Err((
            "bad_args",
            crate::common::contract::malformed(&format!("this face has no command `{other}`")),
        )),
    }
}

/// 收成 `{"lines": [...]}`，过同一个整份上限。
fn lines(rows: Vec<String>) -> Answer {
    let size: usize = rows.iter().map(|l| l.len() + 1).sum();
    if size > crate::read_face::LINES_CAP_BYTES {
        return Err((
            "too_large",
            copy_text(
                "beFeatureFace.answerAt.tooLarge",
                &[
                    ("size", &size.to_string()),
                    ("cap", &(crate::read_face::LINES_CAP_BYTES).to_string()),
                ],
            ),
        ));
    }
    Ok(json!({ "lines": rows }))
}

#[cfg(test)]
#[path = "../../tests/backend/feature_face_tests.rs"]
mod tests;
