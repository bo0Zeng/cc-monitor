//! 要求住址：主会话 09-28 裁 MIG-3b 报备 3 ——「全景三条：`panorama-edit` 进后端 · 界面直问 `panorama` · 回「没装 / 太旧」时 monitor 放字节再问一次」。
//!
//! # 帧命令 `panorama-edit {repo, op, args, shape}`（「引擎只算、文件管理来写」，本机远端同一条）
//!
//! ① 问**这台**的全景小程序要一份计划（`op` 是它自报的写表 `plans=` 里的「算」op，经 [`super::panorama::answer_plan`]：
//!    小程序读盘上现状、算出 `{value, edit: {rel, before, after, parents}}`，一个字节不写；不在写表里 ⇒ `bad_args`）→
//! ② `edit = null` ⇒ 盘上已经是想要的样子，原样回 `value` →
//! ③ `after` 是全文 ⇒ 这台文件管理面的 `files-put`（`root` = 仓、`expect = before`、`parents`）；
//!    `after = null` ⇒ `files-delete`（带 `expect = before`：盘上不是那一份就不删）→
//! ④ `stale`（盘上那份在算与写之间被别人改了）⇒ 回 ① 重算，最多 `assets::door::EDIT_ATTEMPTS` 趟 →
//! ⑤ 写成之后，小程序在写表里说了还要跑哪一个（今天是文档关联那两种 → `refresh_doc_links`，只写索引）就跑它。
//!
//! 〔「后端不带引擎知识」〕「写哪几种 · 各自的算 op · 写完要不要刷」原住本文件一张六行表，
//! 今天只住小程序（`src/panorama-engine/main.rs` 里那张写表，经 `--probe` 的 `plans=` 自报）；本文件只照它走。
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

/// 帧面入口（生产：「算」= 这台的 [`super::panorama::answer_plan`]，写完要跑的 = [`super::panorama::answer`]，写 = 这台的文件管理面）。
pub(crate) async fn answer<D>(door: D, args: &Value) -> Result<Value, CmdErr>
where
    D: Door + Clone + Send + 'static,
{
    answer_with(door, args, |a: Value, plan: bool| async move {
        if plan {
            super::panorama::answer_plan(&a).await
        } else {
            // 写成之后那一步（刷文档关联）不报进度：写那一问没有进度流（不交票）。
            super::panorama::answer(&a, &|_| {}).await
        }
    })
    .await
}

/// [`answer`] 的本体：`ask(载荷, 是不是「算」)` 问一次小程序由调用方给（判据用替身数次数、造 `stale`）。
/// 「算」那一问回 `{result, then}`（`then` = 写成之后要跑的 op），别的回 `{result}`。
pub(crate) async fn answer_with<D, A, F>(door: D, args: &Value, ask: A) -> Result<Value, CmdErr>
where
    D: Door + Clone + Send + 'static,
    A: Fn(Value, bool) -> F,
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
    // 要的那一代由发起方带来，每次问小程序都原样转交（后端不存形状代号）。
    let shape = args.get("shape").cloned().unwrap_or(Value::Null);
    let mut last = String::new();
    let mut written = None;
    // 写成之后要跑的 op：小程序在写表里说的（「算」那一问的应答带回来），本文件不认识它是什么。
    let mut then: Option<String> = None;
    for _ in 0..door::EDIT_ATTEMPTS {
        let got = ask(
            serde_json::json!({ "op": op, "repo": repo, "args": op_args, "shape": shape }),
            true,
        )
        .await?;
        then = got.get("then").and_then(Value::as_str).map(str::to_string);
        let raw = got.get("result").cloned().unwrap_or(Value::Null);
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
            // 删也带 CAS；`after = None` 而 `before = None` 上面已按「没事可做」回了 ⇒ 这里 `before` 恒在。
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
    if let Some(then) = then {
        ask(
            serde_json::json!({ "op": then, "repo": repo, "shape": shape }),
            false,
        )
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
