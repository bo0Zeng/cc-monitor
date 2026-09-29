//! 要求住址：主会话 09-28 裁 MIG-3b 报备 3 ——「全景三条：`panorama-edit` 进后端 · 界面直问 `panorama` · 回「没装 / 太旧」时 monitor 放字节再问一次」。
//!
//! # 帧命令 `panorama-edit {repo, op, args}`（〔RM1d〕V110「引擎只算、文件管理来写」，本机远端同一条）
//!
//! ① 问**这台**的全景小程序要一份计划（[`EDITS`] 第二列那个 `plan_*` op，经 [`super::panorama::answer`]：小程序读盘上现状、
//!    算出 `{value, edit: {rel, before, after, parents}}`，一个字节不写）→
//! ② `edit = null` ⇒ 盘上已经是想要的样子，原样回 `value` →
//! ③ `after` 是全文 ⇒ 这台文件管理面的 `files-put`（`root` = 仓、`expect = before`、`parents`）；
//!    `after = null` ⇒ `files-delete`（〔RM1e〕带 `expect = before`：盘上不是那一份就不删）→
//! ④ `stale`（盘上那份在算与写之间被别人改了）⇒ 回 ① 重算，最多 `assets::door::EDIT_ATTEMPTS` 趟 →
//! ⑤ 文档关联那两种写成之后 `refresh_doc_links`（让文档关联的查询跟上；只写索引）。
//!
//! 原住 monitor `panorama_call.rs`（问 · 交那一环 ＋ 那张六行表 ＋ 计划的线上形状，逐字搬来）：「算」与「写」本来就都在这台，
//! monitor 那一跳只是在中间转。写的规则（CAS · 暂存旁名换名上位 · 回读 · 回滚 · 围栏）一条都不在这里 —— 在文件管理面。
//!
//! 「算」那一步的码原样往外交（`not_installed` / `unsupported` 是界面「放字节再问一次」的触发条件）；撤单同 `panorama`
//! （可取消档：future 被丢 ⇒ 小程序那一组子进程被杀）。

use copy_core::copy_text;
use serde_json::Value;

use crate::assets::door::{self, Door, Refused};

type CmdErr = (String, String);

/// ★**写那六种 → 它的「算」op → 写成之后要不要刷文档关联** —— 唯一住址。
///
/// 「算」op 的集合 == 小程序自报的 op 表（生成物 `engine-contract.json`）里 `plan_` 开头的那几个（判据两向）；界面写入口发的 op 集合 == 本表第一列（vitest 读本文件，两向）。
pub(crate) const EDITS: &[(&str, &str, bool)] = &[
    ("add_annotation", "plan_add_annotation", false),
    ("propose_annotation", "plan_propose_annotation", false),
    ("approve_annotation", "plan_approve_annotation", false),
    ("remove_annotation", "plan_remove_annotation", false),
    ("write_doc_link", "plan_write_doc_link", true),
    ("remove_doc_link", "plan_remove_doc_link", true),
];

/// 写成之后让文档关联的查询跟上的那个 op。
pub(crate) const REFRESH_DOC_LINKS: &str = "refresh_doc_links";

/// 小程序交回的一份计划（上游 `edits::Planned` 的线上形状；字段名与上游 `FileEdit` 两向相等，判据读 vendored 源码）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Planned {
    pub value: Value,
    pub edit: Option<FileEdit>,
}

/// 一处要落盘的改动（上游 `edits::FileEdit`）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileEdit {
    /// 仓相对、`/` 分隔。
    pub rel: String,
    /// 算的那一刻盘上是什么（`None` = 不存在）—— CAS 期望。
    pub before: Option<String>,
    /// 要变成什么（`None` = 删）。
    pub after: Option<String>,
    /// 写的时候要不要建父目录。
    pub parents: bool,
}

/// 帧面入口（生产：「算」= 这台的 [`super::panorama::answer`]，写 = 这台的文件管理面）。
pub(crate) async fn answer<D>(door: D, args: &Value) -> Result<Value, CmdErr>
where
    D: Door + Clone + Send + 'static,
{
    answer_with(door, args, |a: Value| async move {
        super::panorama::answer(&a).await
    })
    .await
}

/// [`answer`] 的本体：`ask` 问一次小程序（`{op, repo, args}` → `{result}`）由调用方给（判据用替身数次数、造 `stale`）。
pub(crate) async fn answer_with<D, A, F>(door: D, args: &Value, ask: A) -> Result<Value, CmdErr>
where
    D: Door + Clone + Send + 'static,
    A: Fn(Value) -> F,
    F: std::future::Future<Output = Result<Value, CmdErr>>,
{
    let bad = |m: &str| {
        (
            "bad_args".to_string(),
            crate::common::contract::malformed(m),
        )
    };
    let op = args
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("missing `op` (a string)"))?;
    let repo = args
        .get("repo")
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| bad("missing `repo` (a string)"))?
        .to_string();
    let op_args = args.get("args").cloned().unwrap_or(Value::Null);
    // 〔PANO〕要的那一代由发起方带来，每次问小程序都原样转交（后端不存形状代号）。
    let shape = args.get("shape").cloned().unwrap_or(Value::Null);
    let Some((_, plan_op, refresh)) = EDITS.iter().find(|(n, ..)| *n == op) else {
        return Err((
            "bad_args".to_string(),
            copy_text(
                "rsPanoramaCall.edit.unknownOp",
                &[
                    ("op", op),
                    (
                        "edits",
                        &EDITS
                            .iter()
                            .map(|(n, ..)| *n)
                            .collect::<Vec<_>>()
                            .join(&copy_text("rsPanoramaCall.edit.opSep", &[])),
                    ),
                ],
            ),
        ));
    };
    let result_of = |v: Value| v.get("result").cloned().unwrap_or(Value::Null);
    let mut last = String::new();
    let mut written = None;
    for _ in 0..door::EDIT_ATTEMPTS {
        let raw = result_of(
            ask(
                serde_json::json!({ "op": plan_op, "repo": repo, "args": op_args, "shape": shape }),
            )
            .await?,
        );
        let planned: Planned = serde_json::from_value(raw.clone()).map_err(|e| {
            (
                "failed".to_string(),
                copy_text(
                    "rsPanoramaCall.edit.badPlan",
                    &[("e", &e.to_string()), ("raw", &raw.to_string())],
                ),
            )
        })?;
        let edit = match planned.edit {
            // 上游构造处已归一；这里再挡一道，免得把「没事可做」当成一次写交出去。
            Some(e) if e.before != e.after => e,
            _ => return Ok(planned.value),
        };
        let (d, root) = (door.clone(), repo.clone());
        let landed = tokio::task::spawn_blocking(move || match edit.after.as_deref() {
            Some(text) => door::put(
                &d,
                &root,
                &edit.rel,
                text,
                edit.before.as_deref(),
                false,
                edit.parents,
            )
            .map(|_| ()),
            // 〔RM1e〕删也带 CAS；`after = None` 而 `before = None` 上面已按「没事可做」回了 ⇒ 这里 `before` 恒在。
            None => match edit.before.as_deref() {
                Some(expect) => door::delete(&d, &root, &edit.rel, expect),
                None => Ok(()),
            },
        })
        .await
        .map_err(|e| ("failed".to_string(), e.to_string()))?;
        match landed {
            Ok(()) => {
                written = Some(planned.value);
                break;
            }
            Err(Refused::Stale(s)) => last = s,
            Err(Refused::Peer { code, said }) => return Err((code, said)),
        }
    }
    let Some(value) = written else {
        return Err((
            "stale".to_string(),
            copy_text(
                "rsPanoramaCall.edit.gaveUp",
                &[
                    ("last", &last),
                    ("attempts", &door::EDIT_ATTEMPTS.to_string()),
                ],
            ),
        ));
    };
    if *refresh {
        ask(serde_json::json!({ "op": REFRESH_DOC_LINKS, "repo": repo, "shape": shape }))
            .await
            .map_err(|(c, e)| {
                (
                    c,
                    copy_text("rsPanoramaCall.edit.notRefreshed", &[("e", &e)]),
                )
            })?;
    }
    Ok(value)
}

#[cfg(test)]
#[path = "../../../tests/backend/control/panorama_edit_tests.rs"]
mod tests;
