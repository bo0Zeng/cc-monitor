//! 〔RM1c · 第四波〕代码全景**经那台机器的后端**走（用户 09-24 V108 选 B）。
//!
//! 一条命令 `panorama_call(origin, op, repo, args)`：按 origin 找那台的长连接，发帧命令 `panorama`，
//! 拿回 `data.result` 原样交给前端。后端那一侧经插件通用调用口起只装引擎的独立小程序
//! （`cc-monitor-panorama`），解析在那台机器上、那个进程里；线上只走查询语义与结构化结果。
//!
//! # 这一拍接谁
//!
//! 前端今天只在**远端** origin 上用它（`src/panorama/api.ts`）；本机那一条仍是进程内那 23 条
//! （`panorama.rs`），第二拍（本机对称、monitor 摘内嵌引擎）才改走这里 —— 那一拍要先答
//! 「本机的批注写走哪」（记录在 `调研/第四波记录/RM1c.md`）。命令本身对 origin 不做假设。
//!
//! # 期限
//!
//! 帧那一跳的期限必须**长于**后端给子进程的期限（后端那一侧先说 `timed_out`，这边才听得到原因，
//! 而不是自己先超时、说一句「没回来」）。两档与后端适配层那两档对拍
//! （`tests::the_budgets_outlast_the_backend_deadlines`，运行时读后端源码，异源）。
//!
//! # 〔RM1d〕批注 / 文档关联的写：引擎只算、文件管理来写（用户 09-24 V110）
//!
//! 第二条命令 `panorama_edit(origin, repo, op, args)`（[`EDITS`] 里的六种）：
//! ① 问那台后端 `panorama` 要一份**计划**（`plan_*`：小程序读盘上现状、调上游 `edits::plan_*` 算出
//!    `{value, edit: {rel, before, after, parents}}`，一个字节不写）→
//! ② `edit = null` ⇒ 盘上已经是想要的样子，原样回 `value` →
//! ③ `after` 是全文 ⇒ 同一台后端的 `files-put`（`root` = 仓、`expect = before`、`parents`）；
//!    `after = null` ⇒ 先 `files-peek` 核「盘上还是 `before`」再 `files-delete` →
//! ④ `stale`（盘上那份在算与写之间被别人改了）⇒ 回 ① 重算，最多 `user_files::EDIT_ATTEMPTS` 趟 →
//! ⑤ 文档关联那两种写成之后 `refresh_doc_links`（让文档关联的查询跟上；只写索引）。
//! 落盘只经 `user_files::Door`（monitor 碰用户文件的唯一开口）；**写的规则只住后端**，这里只「问 · 交」。
//! 本机与远端同一条路，只差 origin。
//!
//! # 诚实边界
//!
//! - 打不断：后端那一侧是阻塞档（`cancel` 回 `not_cancellable`）；这边等到期限为止。
//! - 删批注**没有 CAS**（`files-delete` 不收 `expect`）：删前 `peek` 核一遍，核与删之间仍有一个窗口。

use crate::origin::Origin;
use serde_json::{json, Value};
use std::time::Duration;

/// 建索引那一档的 op（后端给子进程 900 s）。〔RM1d〕`refresh_doc_links` 写的也是索引。
pub(crate) const BUILD_OPS: &[&str] = &["index", "reindex", "refresh_doc_links"];

/// 建索引那一档：后端给子进程 900 s，再留 60 s 给回程。
const BUILD_BUDGET: Duration = Duration::from_secs(960);
/// 其余查询：后端给子进程 60 s（另有 10 s 探测），再留 30 s。
const QUERY_BUDGET: Duration = Duration::from_secs(100);

/// 这个 op 等多久。
pub(crate) fn budget_for(op: &str) -> Duration {
    if BUILD_OPS.contains(&op) {
        BUILD_BUDGET
    } else {
        QUERY_BUDGET
    }
}

/// ★〔RM1d〕**写那六种 → 它的「算」op → 写成之后要不要刷文档关联** —— 唯一住址。
///
/// 「算」op 的集合 == 后端适配层 `OPS` 里 `plan_` 开头的那几个（判据运行时读后端源码，两向）；
/// 前端写入口发的 op 集合 == 本表第一列（vitest 读本文件，两向）。
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

/// 🔴 **问 · 交**：`plan` 每调一次问那台机器要一份新计划；落盘经 `door`（那台机器后端的文件管理）。
/// 写的规则（CAS · 暂存旁名换名上位 · 回读 · 回滚 · 围栏）一条都不在这里。
pub(crate) async fn edit_via<D, P, F>(door: &D, repo: &str, mut plan: P) -> Result<Value, String>
where
    D: crate::user_files::Door,
    P: FnMut() -> F,
    F: std::future::Future<Output = Result<Value, String>>,
{
    use crate::user_files::{Refused, EDIT_ATTEMPTS};
    let mut last = String::new();
    for _ in 0..EDIT_ATTEMPTS {
        let raw = plan().await?;
        let planned: Planned = serde_json::from_value(raw.clone()).map_err(|e| {
            format!("代码全景组件交回的编辑计划形状不对（{e}）—— 两端版本对不上：{raw}")
        })?;
        let Some(edit) = planned.edit else {
            return Ok(planned.value);
        };
        if edit.before == edit.after {
            // 上游构造处已归一；这里再挡一道，免得把「没事可做」当成一次写交出去。
            return Ok(planned.value);
        }
        match edit.after.as_deref() {
            Some(text) => {
                match door
                    .put(
                        repo,
                        &edit.rel,
                        text,
                        edit.before.as_deref(),
                        false,
                        edit.parents,
                    )
                    .await
                {
                    Ok(_) => return Ok(planned.value),
                    Err(Refused::Stale(s)) => last = s,
                    Err(e @ (Refused::Other(_) | Refused::Peer { .. })) => return Err(e.said()),
                }
            }
            None => {
                // 删没有 CAS ⇒ 先核盘上还是算的那一份；不是 ⇒ 当 `stale` 重算。
                let now = door.peek(repo, &edit.rel).await?;
                if now.text != edit.before {
                    last = format!("{} 在算完之后变了", now.path);
                    continue;
                }
                door.delete(repo, &edit.rel).await?;
                return Ok(planned.value);
            }
        }
    }
    Err(format!(
        "{last}（连着 {EDIT_ATTEMPTS} 趟都是算完之后盘上那份又变了，先停下 —— 过一会儿再点一次）"
    ))
}

/// 拼帧载荷（纯函数）。`repo` / `args` 缺席就不带这一格（后端对缺席与 `null` 同一口径）。
pub(crate) fn frame_args(op: &str, repo: Option<&str>, args: Option<Value>) -> Value {
    let mut a = json!({ "op": op });
    if let Some(r) = repo {
        a["repo"] = json!(r);
    }
    if let Some(x) = args {
        a["args"] = x;
    }
    a
}

/// 问 `origin` 那台机器的后端做一次全景查询，拿回 `result`。
#[tauri::command]
pub async fn panorama_call(
    origin: Origin,
    op: String,
    repo: Option<String>,
    args: Option<Value>,
) -> Result<Value, String> {
    let _ = origin.route("panorama_call")?;
    ask(&origin, &op, repo.as_deref(), args).await
}

/// 发一次 `panorama` 帧命令、拿 `result`（两条命令共用）。
async fn ask(
    origin: &Origin,
    op: &str,
    repo: Option<&str>,
    args: Option<Value>,
) -> Result<Value, String> {
    let data = crate::backend::control::frame_query::call(
        origin,
        "panorama",
        frame_args(op, repo, args),
        budget_for(op),
    )
    .await?;
    data.get("result").cloned().ok_or_else(|| {
        format!(
            "远端 [{}] `panorama` 的应答没有 `result` —— 两端契约对不上",
            origin.as_wire_str()
        )
    })
}

/// 〔RM1d〕写批注 / 文档关联：问那台机器要计划、经那台机器后端的文件管理落盘（头注 ①–⑤）。
/// `op` 只许 [`EDITS`] 第一列；回的 `value` 与本机进程内那几条写命令的返回值同形（id / 在不在 / `null`）。
#[tauri::command]
pub async fn panorama_edit(
    origin: Origin,
    repo: String,
    op: String,
    args: Value,
) -> Result<Value, String> {
    let _ = origin.route("panorama_edit")?;
    let Some((_, plan_op, refresh)) = EDITS.iter().find(|(n, ..)| *n == op) else {
        return Err(format!(
            "不认识的全景写入 `{op}`（认得的：{}）",
            EDITS
                .iter()
                .map(|(n, ..)| *n)
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    };
    let door = crate::user_files::BackendDoor::new(origin.clone());
    // 「算」在哪：远端 = 那台机器的全景小程序；本机 = 进程内引擎的只读那一层（本机对称那一拍之前的过渡，
    // `panorama.rs::plan_local`）。**落盘两边同一扇门**（那台机器后端的文件管理）。
    let local = origin.is_local();
    let value = edit_via(&door, &repo, || {
        let (origin, repo, args) = (origin.clone(), repo.clone(), args.clone());
        async move {
            if local {
                crate::panorama::plan_local(repo, plan_op, args).await
            } else {
                ask(&origin, plan_op, Some(&repo), Some(args)).await
            }
        }
    })
    .await?;
    if *refresh {
        let refreshed = if local {
            crate::panorama::refresh_doc_links_local(repo.clone()).await
        } else {
            ask(&origin, REFRESH_DOC_LINKS, Some(&repo), None)
                .await
                .map(|_| ())
        };
        refreshed.map_err(|e| {
                format!("文档关联已经写进去了，但全景里的关联没跟着刷新（{e}）—— 点「刷新」重建一次就能看到")
            })?;
    }
    Ok(value)
}

#[cfg(test)]
#[path = "../../../tests/bridge/panorama_call_tests.rs"]
mod tests;
