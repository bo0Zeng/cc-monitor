//! 〔AS2 · 第四波 4B · V113〕**资产目录的自动同步** —— 本机常驻后端沿它已有的那条 SSH 连接，拉远端的目录、合并、回写。
//!
//! # 用户裁决（2026-09-25，`99 §1` V113，逐字）
//!
//! 「比如本机后端在本机看见一个skill并记录下来, 就会和远端后端同步, 这样远端后端也能在远端装skill或者mcp」·
//! 「目录自动同步，装要你点」。`设计/01 §3.5`：「观测方沿它本来就拥有的那条连接去拉被观测方」（零新通道）。
//!
//! # 形状（一趟 = 对一台远端）
//!
//! ```text
//!  本机常驻后端（池里那条 SSH 连接上多开一个 exec 通道，不是新连接）
//!   ① 拉：capture `<远端后端> --assets-catalog`            ──▶ 远端现扫、记下、回它的整份
//!   ② 并：本机 `assets-catalog-merge {catalog: 远端那份}`   （经门递进来的写口；同一台取 gen 大的整份）
//!   ③ 推：远端缺的 / 比远端新的那几台快照（不含远端自己那格）
//!          capture `printf '%s\n' '<json>' | <远端后端> --assets-catalog-merge` ──▶ 远端并进它自己的文件（一台一个写者）
//!   ④ 本机目录因这一趟变了 ⇒ 对可达表里**其余**各台各做一趟 ①–③（只一层，不递归）
//! ```
//!
//! # 为什么是 capture ＋ 管道，不是长流、不是 CLI 读 stdin
//!
//! - **不起远端的流模式**：流模式一起来就往 tmux server 装全局 hook（`control/tmux_hook.rs::install_hooks`，
//!   载荷里烤着**那个进程**的 pid）—— 一个用完就退的流会把 monitor 那条真流的 hook 盖成一个死 pid。
//! - **CLI 面读 stdin 读到 EOF**，而 capture 不关远端的 stdin、`stream` 用法「上行一结束就收工」（`dial/uses.rs`）
//!   ⇒ 入参只能在命令里交：`printf … |` 管进去。⚠ 这假设远端登录 shell 认 POSIX 单引号与管道 ——
//!   monitor 起远端后端那条命令（`ssh_source::shell_quote`）早就是同一个假设。
//! - **一趟命令的大小有上限**（`sh -c` 的那一个参数，Linux `MAX_ARG_STRLEN` = 128 KiB）⇒ 推的载荷按台切块，
//!   一块不超过 [`PUSH_MAX_BYTES`]；**单独一台就超了 ⇒ 那一台不推、说出来**（不截断）。
//!
//! # 事件，不是定时（`no_timer_guard`）
//!
//! 触发只有两种：monitor 在远端那条流握手成功那一刻交一次 `assets-sync {origin, dial, backend}`（连上）；
//! 界面看机器页前交一次 `assets-sync {}`（对可达表里每一台各一趟）。本模块一个会自己醒的构件都没有；
//! 一趟的期限归调用方（monitor 那一侧的调用预算），同本后端其余异步命令。
//!
//! # 可达表（内存）与「问远端」那一跳
//!
//! 〔C4d · 第四波 4B〕两样都**不住这里了**：主会话 09-25 裁「一路造、两路用」—— `DialRemote`（capture 那一跳）
//! 与可达表（`origin → {拨号请求, 远端后端路径, 对面的 id}`）原样提到中立住址 `crate::remote_ask`，逻辑一字不改；
//! 本模块只剩资产目录那一套（拉什么、并什么、推什么、扇不扇出）。历史跨机 join 用的是同一张表、同一个对面。

use std::collections::BTreeMap;

use serde_json::{json, Value};

// 〔C4d〕问远端那一跳与可达表住 `remote_ask`（原样搬过去的）；这里只取用，不再导出。
use crate::remote_ask::{lock, Reach, Remote, Table, REACH};

/// 一块推的载荷（JSON 本身，引号转义之前）的上限。`sh -c` 那一个参数 128 KiB，留出引号转义与命令本身的余量。
pub const PUSH_MAX_BYTES: usize = 96 * 1024;

/// 远端后端的两条一次性子命令（与 `lib.rs::SUBCOMMANDS` 同名，判据钉）。
pub const PULL_FLAG: &str = "--assets-catalog";
pub const PUSH_FLAG: &str = "--assets-catalog-merge";

/// 本机写口（`asset_catalog::answer_merge`）—— **由门（`inbound.rs`）递进来**，本模块不直呼它（第四层判据 ④）。
pub type Fold =
    std::sync::Arc<dyn Fn(&Value) -> Result<Value, (&'static str, String)> + Send + Sync>;

/// 远端上那两条命令的完整字面（**只此一处拼**）。
pub fn pull_command(backend: &str) -> String {
    format!("{} {PULL_FLAG}", shell_quote_core::posix_quote(backend))
}

pub fn push_command(backend: &str, payload: &str) -> String {
    format!(
        "printf '%s\\n' {} | {} {PUSH_FLAG}",
        shell_quote_core::posix_quote(payload),
        shell_quote_core::posix_quote(backend)
    )
}

/// 一趟对一台的结局（线上 `synced[]` 的一行）。
fn outcome(
    key: &str,
    peer: Option<&str>,
    changed: bool,
    pushed: usize,
    errors: &[String],
) -> Value {
    json!({
        "origin": key,
        "peer": peer,
        "changed": changed,
        "pushed": pushed,
        "error": if errors.is_empty() { Value::Null } else { json!(errors.join("；")) },
    })
}

/// 远端缺的 / 比远端新的那几台快照（不含远端自己那格），切成不超过 [`PUSH_MAX_BYTES`] 的块。
/// 单独一台就超了的 ⇒ 进第二个返回值（说出来，不推、不截断）。**纯函数**。
pub fn push_plan(mine: &Value, theirs: &Value) -> (Vec<Vec<Value>>, Vec<String>) {
    let their_self = theirs.get("self").and_then(Value::as_str).unwrap_or("");
    let their_gen: BTreeMap<&str, u64> = theirs
        .get("machines")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|m| Some((m.get("id")?.as_str()?, m.get("gen")?.as_u64()?)))
                .collect()
        })
        .unwrap_or_default();
    let mut chunks: Vec<Vec<Value>> = Vec::new();
    let mut too_big: Vec<String> = Vec::new();
    let (mut cur, mut cur_len): (Vec<Value>, usize) = (Vec::new(), 0);
    for m in mine
        .get("machines")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(id), Some(gen)) = (
            m.get("id").and_then(Value::as_str),
            m.get("gen").and_then(Value::as_u64),
        ) else {
            continue;
        };
        if id == their_self || their_gen.get(id).is_some_and(|g| *g >= gen) {
            continue;
        }
        let len = m.to_string().len();
        if cur_len + len > PUSH_MAX_BYTES && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            cur_len = 0;
        }
        if len > PUSH_MAX_BYTES {
            let why = format!(
                "机器 {id} 的目录太大（{len} 字节，一趟最多 {PUSH_MAX_BYTES}），这一台没推过去"
            );
            tracing::warn!("资产目录：{why}");
            too_big.push(why);
            continue;
        }
        cur_len += len + 1;
        cur.push(m.clone());
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    (chunks, too_big)
}

/// 经写口并一次（同步文件 I/O ⇒ 挪到阻塞线程池，不占 tokio worker）。
async fn fold_blocking(fold: &Fold, args: Value) -> Result<Value, String> {
    let f = fold.clone();
    tokio::task::spawn_blocking(move || f(&args))
        .await
        .map_err(|e| format!("本机目录那一趟没跑完：{e}"))?
        .map_err(|(c, m)| format!("本机目录（{c}）：{m}"))
}

/// 对一台做一趟 ①–③。`key` 是可达表的键（monitor 交来的 origin 串，本后端只当不透明的键用）。
/// 回（本机目录因此变了没有, 结局那一行）。
async fn sync_one(
    key: &str,
    r: &Reach,
    fold: &Fold,
    remote: &dyn Remote,
    table: &Table,
) -> (bool, Value) {
    let mut errors = Vec::new();
    // ① 拉
    let theirs: Value = match remote.run(&r.dial, pull_command(&r.backend)).await {
        Ok(out) => match serde_json::from_str(out.trim()) {
            Ok(v) => v,
            Err(e) => {
                return (
                    false,
                    outcome(key, None, false, 0, &[format!("对面答的目录认不出来：{e}")]),
                )
            }
        },
        Err(e) => return (false, outcome(key, None, false, 0, &[e])),
    };
    let peer = theirs
        .get("self")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(row) = lock(table).get_mut(key) {
        row.peer = peer.clone();
    }
    // ② 并
    let mine = match fold_blocking(fold, json!({ "catalog": theirs })).await {
        Ok(v) => v,
        Err(e) => return (false, outcome(key, peer.as_deref(), false, 0, &[e])),
    };
    let changed = mine
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // ③ 推
    let (chunks, too_big) = push_plan(&mine, &theirs);
    errors.extend(too_big);
    let mut pushed = 0usize;
    for chunk in chunks {
        let n = chunk.len();
        let payload = json!({ "catalog": { "machines": chunk } }).to_string();
        match remote
            .run(&r.dial, push_command(&r.backend, &payload))
            .await
        {
            Ok(_) => pushed += n,
            Err(e) => errors.push(e),
        }
    }
    (
        changed,
        outcome(key, peer.as_deref(), changed, pushed, &errors),
    )
}

/// `assets-sync`：给了 `origin`（＋ `dial` ＋ `backend`）⇒ 记进可达表、对它做一趟，本机因此变了再对其余各台各一趟；
/// 什么都没给 ⇒ 对可达表里每一台各一趟。开头先让本机现扫一次：本机自己那份变了也算「目录变了」（扇出到每一台）。
/// 回 `{self, synced, reach}`（`self` = 本机目录的 id，界面据它把目录里本机那一格对回 `<local>`）。
pub async fn answer(
    args: &Value,
    fold: Fold,
    remote: &dyn Remote,
) -> Result<Value, (&'static str, String)> {
    answer_with(args, fold, remote, &REACH).await
}

/// [`answer`] 的可喂夹具那一半：可达表由调用方给（判据各用各的表，不共享进程里那一张）。
pub async fn answer_with(
    args: &Value,
    fold: Fold,
    remote: &dyn Remote,
    table: &Table,
) -> Result<Value, (&'static str, String)> {
    let origin = args.get("origin").and_then(Value::as_str);
    let mut first: Option<String> = None;
    if origin.is_some() {
        // 〔C4d〕登记那一段原样搬进 `remote_ask::register`（可达表唯一的写口；`remote-reach` 也经它）。
        first = Some(crate::remote_ask::register(table, args)?);
    } else if args.get("dial").is_some() || args.get("backend").is_some() {
        return Err(("bad_args", "给了 `dial` / `backend` 却没给 `origin`".into()));
    }
    let rows: Vec<(String, Reach)> = lock(table)
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    // 这一趟先让本机现扫一次（并一份空的进来 = 只刷新自己那一格）：拿到本机目录的 id，
    // 并且本机自己那份变了（刚装了 / 删了一个 skill）就算「目录变了」—— 下面对可达表里每一台都做一趟。
    let own = fold_blocking(&fold, json!({ "catalog": { "machines": [] } }))
        .await
        .map_err(|e| ("io_failed", e))?;
    let own_id = own.get("self").cloned().unwrap_or(Value::Null);
    let own_changed = own.get("changed").and_then(Value::as_bool).unwrap_or(false);
    let mut synced = Vec::new();
    match &first {
        Some(o) => {
            let r = rows
                .iter()
                .find(|(k, _)| k == o)
                .map(|(_, r)| r.clone())
                .expect("刚插进去的那一行");
            let (changed, row) = sync_one(o, &r, &fold, remote, table).await;
            synced.push(row);
            if changed || own_changed {
                for (other, r) in rows.iter().filter(|(k, _)| k != o) {
                    synced.push(sync_one(other, r, &fold, remote, table).await.1);
                }
            }
        }
        None => {
            for (o, r) in &rows {
                synced.push(sync_one(o, r, &fold, remote, table).await.1);
            }
        }
    }
    let reach_rows = crate::remote_ask::reach_rows(table);
    Ok(json!({ "self": own_id, "synced": synced, "reach": reach_rows }))
}

#[cfg(test)]
#[path = "../../tests/backend/asset_sync_tests.rs"]
mod tests;
