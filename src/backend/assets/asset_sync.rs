//! 资产目录的自动同步：本机常驻后端沿它已有的那条 SSH 连接，拉远端的目录、合并、回写（零新通道）。目录自动同步，装要用户点。
//!
//! # 形状（一趟 = 对一台远端）
//!
//! ```text
//! 本机常驻后端（池里那条 SSH 连接上多开一个 exec 通道，不是新连接）
//! ① 拉：capture `<远端后端> --assets-catalog`            ──▶ 远端现扫、记下、回它的整份
//! ② 并：本机 `assets-catalog-merge {catalog: 远端那份}`   （经门递进来的写口；同一台取 gen 大的整份）
//! ③ 推：远端缺的 / 比远端新的那几台快照（不含远端自己那格）
//! capture `<远端后端> --assets-catalog-merge --stdin-line`，载荷一行写进它的 stdin ──▶ 远端并进它自己的文件（一台一个写者）
//! ④ 本机目录因这一趟变了 ⇒ 对机器表里「连这台」的其余各台各做一趟 ①–③（只一层，不递归）
//! ```
//!
//! # capture ＋「只读一行 stdin」，不是长流
//!
//! - 不起远端的流模式：流模式一起来就往 tmux server 装全局 hook（载荷里烤着那个进程的 pid），一个用完就退的流会把 monitor 那条真流的 hook 盖成一个死 pid。
//! - capture 不关远端的 stdin ⇒ 推那一趟走 CLI 面的「只读一行」入口（`wire/cli_words.rs::STDIN_LINE_FLAG`，`control/cli_args.rs::read_input` 收）：
//!   命令行里只有后端路径与两个旗标，载荷不进命令行（不要求远端登录 shell 认 POSIX 单引号与管道）。
//!   后端路径那一格仍过 POSIX 单引号（`remote_ask::command_line` · `shell_quote_core::posix_quote`）：路径里没有 `'` / `\` 时 fish 也认。
//! - 一趟的大小有上限：远端 CLI 面 stdin 的上限（`cli_args::MAX_CLI_STDIN`，1 MiB，超了拒）⇒ 推的载荷按台切块，
//!   一块不超过 [`PUSH_MAX_BYTES`]；单独一台就超了 ⇒ 那一台不推、说出来。
//!
//! # 事件，不是定时（`no_timer_guard`）
//!
//! 触发只有两种：monitor 在远端那条流握手成功那一刻交一次 `assets-sync {origin, dial}`；界面看机器页前交一次 `assets-sync {}`
//! （对机器表里「连这台」的每一台各一趟）。一趟的期限归调用方。
//! 「问远端」那一跳（`DialRemote`）与「那几台」（[`Table`]）住 `crate::dial::remote_ask`；本模块只管资产目录拉什么、并什么、推什么、扇不扇出。

use crate::assets::asset_catalog::{
    AssetsCatalogMergeReq, CatalogIn, CatalogMachine, CatalogReply,
};
use crate::common::said::IntoNote as _;
use crate::common::said::Said;
use copy_core::copy_text;
use std::collections::BTreeMap;

use serde_json::{json, Value};

// 问远端那一跳与「那几台」住 `remote_ask`；这里只取用。
use crate::dial::remote_ask::{Remote, Table, REACH};

/// 一块推的载荷（JSON 本身）的上限。
/// 原理由是「`sh -c` 那一个参数 128 KiB，留出引号转义的余量」—— 载荷改走 stdin 之后那条上限不在了，
/// 今天管它的是远端 CLI 面 stdin 的上限（1 MiB，判据钉「本值不超过它」）。值**没动**。
pub const PUSH_MAX_BYTES: usize = 96 * 1024;

/// 远端后端的两条一次性子命令（子命令闸门 `stream::cli::is_subcommand` 认的同名那两条，判据钉）。
pub const PULL_FLAG: &str = "--assets-catalog";
pub const PUSH_FLAG: &str = "--assets-catalog-merge";

/// 本机写口（`asset_catalog::answer_merge`）—— **由门（`stream/inbound/`）递进来**，本模块不直呼它（`Own` 由分派那一层递）。
pub type Fold =
    std::sync::Arc<dyn Fn(AssetsCatalogMergeReq) -> Result<CatalogReply, Said> + Send + Sync>;

/// `assets-sync` 的入参与回包住 cmd-wire（壳连上一台时照同一份交、照同一份收）。
pub(crate) use cmd_wire::assets::{AssetsSyncReq, AssetsSyncRow, AssetsSynced};

/// 远端上那两条命令的完整字面（**只此一处拼**）。
pub fn pull_command() -> String {
    crate::dial::remote_ask::command_line(&[PULL_FLAG])
}

/// 推那一趟的命令行：**只有后端路径与两个旗标，不含载荷**；载荷由 [`push_stdin`] 经 capture 写进 stdin。
pub fn push_command() -> String {
    crate::dial::remote_ask::command_line(&[PUSH_FLAG, crate::wire::cli_words::STDIN_LINE_FLAG])
}

/// 推那一趟写进远端 stdin 的那一行（载荷本身是紧凑 JSON、没有换行 ⇒ 恰好一行）。
pub fn push_stdin(payload: &str) -> String {
    format!("{payload}\n")
}

/// 一趟对一台的结局（线上 `synced[]` 的一行）。
fn outcome(
    key: &str,
    peer: Option<&str>,
    changed: bool,
    pushed: usize,
    errors: &[String],
) -> AssetsSyncRow {
    AssetsSyncRow {
        origin: cmd_wire::Machine::named(key.to_string()),
        peer: peer.map(str::to_string),
        changed,
        pushed,
        error: (!errors.is_empty())
            .then(|| errors.join(&copy_text("beAssetSync.report.errorSep", &[]))),
    }
}

/// 单独一台就超过一趟上限时那一句（`push_plan` 说出口、记进 `warn!`）。
fn too_big_to_push(id: &str, len: usize, cap: usize) -> String {
    let (len, cap) = (len.to_string(), cap.to_string());
    copy_text(
        "beAssetSync.pushPlan.tooLarge",
        &[("id", id), ("len", &len), ("max", &cap)],
    )
}

/// 远端缺的 / 比远端新的那几台快照（不含远端自己那格：`their_self` ＝ 对面目录的 id），切成不超过 [`PUSH_MAX_BYTES`] 的块。
/// 单独一台就超了的 ⇒ 进第二个返回值（说出来，不推、不截断）。**纯函数**。
pub fn push_plan(
    mine: &CatalogReply,
    theirs: &CatalogIn,
    their_self: Option<&str>,
) -> (Vec<Vec<CatalogMachine>>, Vec<String>) {
    let their_gen: BTreeMap<&str, u64> = theirs
        .machines
        .iter()
        .map(|m| (m.id.as_str(), m.gen))
        .collect();
    let mut chunks: Vec<Vec<CatalogMachine>> = Vec::new();
    let mut too_big: Vec<String> = Vec::new();
    let (mut cur, mut cur_len): (Vec<CatalogMachine>, usize) = (Vec::new(), 0);
    for m in &mine.machines {
        let (id, gen) = (m.id.as_str(), m.gen);
        if Some(id) == their_self || their_gen.get(id).is_some_and(|g| *g >= gen) {
            continue;
        }
        let len = json!(m).to_string().len();
        if cur_len + len > PUSH_MAX_BYTES && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            cur_len = 0;
        }
        if len > PUSH_MAX_BYTES {
            let why = too_big_to_push(id, len, PUSH_MAX_BYTES);
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

/// 写口回的失败（或对面那一份解不成并的入参）⇒ 那一趟的那一句。
fn fold_failed(f: Said) -> String {
    let c = f.code_or_fallback().to_string();
    copy_text(
        "beAssetSync.foldBlocking.failed",
        &[("c", &c), ("m", &f.into_note())],
    )
}

/// 经写口并一次（同步文件 I/O ⇒ 挪到阻塞线程池，不占 tokio worker）。
async fn fold_blocking(fold: &Fold, req: AssetsCatalogMergeReq) -> Result<CatalogReply, String> {
    let f = fold.clone();
    tokio::task::spawn_blocking(move || f(req))
        .await
        .map_err(|e| {
            Said::with_raw(copy_text("beAssetSync.foldBlocking.unfinished", &[]), &e).into_note()
        })?
        .map_err(fold_failed)
}

/// 对一台做一趟 ①–③。`key` 是那台的名字，`dial` 是照机器表组好的拨号请求。
/// 回（本机目录因此变了没有, 结局那一行）。
async fn sync_one(
    key: &str,
    dial: &Value,
    fold: &Fold,
    remote: &dyn Remote,
    table: &Table,
) -> (bool, AssetsSyncRow) {
    let mut errors = Vec::new();
    // ① 拉
    let theirs: Value = match remote.run(dial, pull_command(), None).await {
        Ok(out) => match serde_json::from_str(out.trim()) {
            Ok(v) => v,
            Err(e) => {
                return (
                    false,
                    outcome(
                        key,
                        None,
                        false,
                        0,
                        &[
                            Said::with_raw(copy_text("beAssetSync.syncOne.unparsable", &[]), &e)
                                .into_note(),
                        ],
                    ),
                )
            }
        },
        Err(e) => return (false, outcome(key, None, false, 0, &[e])),
    };
    let peer = theirs
        .get("self")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(id) = &peer {
        table.set_peer(key, id);
    }
    // ② 并（对面那一份按并的入参收：形状不对 ⇒ 那一句同写口拒的那一形）
    let req: AssetsCatalogMergeReq = match crate::wire::typed::take(&json!({ "catalog": theirs })) {
        Ok(r) => r,
        Err(f) => {
            return (
                false,
                outcome(key, peer.as_deref(), false, 0, &[fold_failed(f)]),
            )
        }
    };
    let theirs = req.catalog.clone();
    let mine = match fold_blocking(fold, req).await {
        Ok(v) => v,
        Err(e) => return (false, outcome(key, peer.as_deref(), false, 0, &[e])),
    };
    let changed = mine.changed;
    // ③ 推
    let (chunks, too_big) = push_plan(&mine, &theirs, peer.as_deref());
    errors.extend(too_big);
    let mut pushed = 0usize;
    for chunk in chunks {
        let n = chunk.len();
        let payload = json!({ "catalog": { "machines": chunk } }).to_string();
        match remote
            .run(dial, push_command(), Some(push_stdin(&payload)))
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

/// 按名字对一台做一趟：怎么够到它照机器表组（组不成 ⇒ 那一行就是那一句）。
async fn sync_named(
    key: &str,
    fold: &Fold,
    remote: &dyn Remote,
    table: &Table,
) -> (bool, AssetsSyncRow) {
    match table.dial(key) {
        Ok(dial) => sync_one(key, &dial, fold, remote, table).await,
        Err(e) => (false, outcome(key, None, false, 0, &[e.said])),
    }
}

/// `assets-sync`：给了 `origin` ⇒ 对它做一趟，本机因此变了再对其余各台各一趟；
/// 什么都没给 ⇒ 对机器表里「连这台」的每一台各一趟。开头先让本机现扫一次：本机自己那份变了也算「目录变了」（扇出到每一台）。
/// 回 `{self, synced, reach}`（`self` = 本机目录的 id，界面据它把目录里本机那一格对回 `<local>`）。
pub(crate) async fn answer(
    req: &AssetsSyncReq,
    fold: Fold,
    remote: &dyn Remote,
) -> Result<AssetsSynced, Said> {
    answer_with(req, fold, remote, &REACH).await
}

/// [`answer`] 的可喂夹具那一半：那几台由调用方给（判据各用各的表，不共享进程里那一张）。
pub(crate) async fn answer_with(
    req: &AssetsSyncReq,
    fold: Fold,
    remote: &dyn Remote,
    table: &Table,
) -> Result<AssetsSynced, Said> {
    let origin = req.origin.as_ref().map(ToString::to_string);
    let origin = origin.as_deref();
    if origin == Some("") {
        return Err(Said::new(
            "bad_args",
            crate::common::contract::malformed("`origin` is empty (the machine name)"),
        ));
    }
    // 点名的那一台：机器表里要有（怎么够到它只看这一端的机器表）—— 没有就明说，不猜。
    if let Some(o) = origin {
        if !table.has(o) {
            return Err(Said::new(
                "unreachable",
                crate::dial::remote_ask::unreachable_message(o),
            ));
        }
    }
    let first = origin.map(str::to_string);
    let rows: Vec<String> = table.connected();
    // 这一趟先让本机现扫一次（并一份空的进来 = 只刷新自己那一格）：拿到本机目录的 id，
    // 并且本机自己那份变了（刚装了 / 删了一个 skill）就算「目录变了」—— 下面对「连这台」的每一台都做一趟。
    let nothing = AssetsCatalogMergeReq {
        catalog: CatalogIn { machines: vec![] },
    };
    let own = fold_blocking(&fold, nothing)
        .await
        .map_err(|e| Said::new("io_failed", e))?;
    let own_changed = own.changed;
    let mut synced = Vec::new();
    match &first {
        Some(o) => {
            let (changed, row) = sync_named(o, &fold, remote, table).await;
            synced.push(row);
            if changed || own_changed {
                for other in rows.iter().filter(|k| *k != o) {
                    synced.push(sync_named(other, &fold, remote, table).await.1);
                }
            }
        }
        None => {
            for o in &rows {
                synced.push(sync_named(o, &fold, remote, table).await.1);
            }
        }
    }
    Ok(AssetsSynced {
        self_id: own.self_id,
        synced,
        reach: crate::dial::remote_ask::reach_rows(table),
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/assets/asset_sync_tests.rs"]
mod tests;
