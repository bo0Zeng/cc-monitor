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
//! # 可达表（内存）
//!
//! `origin → {拨号请求, 远端后端路径, 对面的 id}`：只在本进程里，后端重启就空（下次连上再填）。
//! 拨号请求里只有路径（`key_path`），没有私钥本体（凭据面 `K11` 同 `dial_host::request`）。

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// 一块推的载荷（JSON 本身，引号转义之前）的上限。`sh -c` 那一个参数 128 KiB，留出引号转义与命令本身的余量。
pub const PUSH_MAX_BYTES: usize = 96 * 1024;

/// 拉回来那一份（远端 stdout）的上限。目录文件本身 16 MiB 封顶（`asset_catalog::CATALOG_MAX_BYTES`），这里取同一个数。
pub const PULL_MAX_BYTES: usize = 16 * 1024 * 1024;

/// 可达表的条数上限（有界资源；一个人配不出这么多台远端）。
pub const MAX_REACH: usize = 256;

/// 老后端不认一次性子命令会进流模式、第一行是 hello —— capture 看见它就收工（不让它装 hook、不挂住）。
const HELLO_MARKER: &str = "\"kind\":\"hello\"";

/// 远端后端的两条一次性子命令（与 `lib.rs::SUBCOMMANDS` 同名，判据钉）。
pub const PULL_FLAG: &str = "--assets-catalog";
pub const PUSH_FLAG: &str = "--assets-catalog-merge";

/// 本机写口（`asset_catalog::answer_merge`）—— **由门（`inbound.rs`）递进来**，本模块不直呼它（第四层判据 ④）。
pub type Fold =
    std::sync::Arc<dyn Fn(&Value) -> Result<Value, (&'static str, String)> + Send + Sync>;

/// 对面：在那台上跑一条一次性命令，交回它的 stdout。生产 = [`DialRemote`]（经 `dial` 的 capture）；判据用替身。
pub trait Remote: Send + Sync {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;
}

/// 可达表的一行。
#[derive(Clone, Debug, PartialEq)]
pub struct Reach {
    dial: Value,
    backend: String,
    peer: Option<String>,
}

/// 可达表（本进程一张）。
pub type Table = Mutex<BTreeMap<String, Reach>>;

static REACH: Table = Mutex::new(BTreeMap::new());

fn lock(t: &Table) -> std::sync::MutexGuard<'_, BTreeMap<String, Reach>> {
    t.lock().unwrap_or_else(|e| e.into_inner())
}

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
    origin: &str,
    peer: Option<&str>,
    changed: bool,
    pushed: usize,
    errors: &[String],
) -> Value {
    json!({
        "origin": origin,
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
        if len > PUSH_MAX_BYTES {
            too_big.push(format!(
                "机器 {id} 的目录有 {len} 字节，超过一趟能推的 {PUSH_MAX_BYTES} —— 这一台没推过去"
            ));
            continue;
        }
        if cur_len + len > PUSH_MAX_BYTES && !cur.is_empty() {
            chunks.push(std::mem::take(&mut cur));
            cur_len = 0;
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

/// 对一台做一趟 ①–③。回（本机目录因此变了没有, 结局那一行）。
async fn sync_one(
    origin: &str,
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
                    outcome(
                        origin,
                        None,
                        false,
                        0,
                        &[format!("对面答的目录认不出来：{e}")],
                    ),
                )
            }
        },
        Err(e) => return (false, outcome(origin, None, false, 0, &[e])),
    };
    let peer = theirs
        .get("self")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(row) = lock(table).get_mut(origin) {
        row.peer = peer.clone();
    }
    // ② 并
    let mine = match fold_blocking(fold, json!({ "catalog": theirs })).await {
        Ok(v) => v,
        Err(e) => return (false, outcome(origin, peer.as_deref(), false, 0, &[e])),
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
        outcome(origin, peer.as_deref(), changed, pushed, &errors),
    )
}

/// `assets-sync`：给了 `origin`（＋ `dial` ＋ `backend`）⇒ 记进可达表、对它做一趟，本机因此变了再对其余各台各一趟；
/// 什么都没给 ⇒ 对可达表里每一台各一趟。回 `{synced, reach}`。
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
    if let Some(o) = origin {
        if o.is_empty() {
            return Err(("bad_args", "`origin` 是空串".into()));
        }
        let dial = args.get("dial").filter(|d| d.is_object()).ok_or((
            "bad_args",
            "给了 `origin` 就要给 `dial`（一份拨号请求）".to_string(),
        ))?;
        let backend = args
            .get("backend")
            .and_then(Value::as_str)
            .filter(|b| !b.is_empty())
            .ok_or((
                "bad_args",
                "给了 `origin` 就要给 `backend`（那台上后端的路径）".to_string(),
            ))?;
        let mut t = lock(table);
        if !t.contains_key(o) && t.len() >= MAX_REACH {
            return Err(("bad_args", format!("可达表已满（{MAX_REACH} 台）")));
        }
        let peer = t.get(o).and_then(|r| r.peer.clone());
        t.insert(
            o.to_string(),
            Reach {
                dial: dial.clone(),
                backend: backend.to_string(),
                peer,
            },
        );
        first = Some(o.to_string());
    } else if args.get("dial").is_some() || args.get("backend").is_some() {
        return Err(("bad_args", "给了 `dial` / `backend` 却没给 `origin`".into()));
    }
    let rows: Vec<(String, Reach)> = lock(table)
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
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
            if changed {
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
    let reach_rows: Vec<Value> = lock(table)
        .iter()
        .map(|(o, r)| json!({ "origin": o, "machine": r.peer }))
        .collect();
    Ok(json!({ "synced": synced, "reach": reach_rows }))
}

// ───────────────────────── 生产那一个对面：经 dial 的 capture ─────────────────────────

/// 经本机常驻后端池里那条 SSH 连接跑 capture（`dial::uses::run`，与 monitor 开的链路同一条路，零新连接）。
pub struct DialRemote;

/// 读一行（带上限；超了是错，不截断）。
async fn capped_line<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    cap: u64,
) -> Result<Option<String>, String> {
    let mut buf = Vec::new();
    let n = r
        .take(cap + 1)
        .read_until(b'\n', &mut buf)
        .await
        .map_err(|e| format!("读拨号链路失败：{e}"))?;
    if n == 0 {
        return Ok(None);
    }
    if buf.len() as u64 > cap {
        return Err(format!("拨号链路上一行超过 {cap} 字节 —— 拒收"));
    }
    Ok(Some(String::from_utf8_lossy(&buf).trim_end().to_string()))
}

impl Remote for DialRemote {
    fn run<'a>(
        &'a self,
        dial: &'a Value,
        command: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            let mut v = dial.clone();
            let obj = v.as_object_mut().ok_or("拨号请求不是对象".to_string())?;
            obj.insert("use".into(), json!("capture"));
            obj.insert("command".into(), json!(command));
            obj.insert(
                "capture".into(),
                json!({ "max_bytes": PULL_MAX_BYTES, "abort_marker": HELLO_MARKER }),
            );
            obj.insert("stages".into(), json!(false));
            obj.insert("probe".into(), json!(false));
            let req = crate::dial::parse_request_value(&v)
                .map_err(|e| format!("拨号请求认不出来：{e}"))?;
            // 上行那根管子我们一个字节都不写（capture 不读上行）；留着不关，直到拿到结果。
            let (_up_w, up_r) = tokio::io::duplex(1024);
            let (mut down_w, down_r) = tokio::io::duplex(64 * 1024);
            let task = tokio::spawn(async move {
                let stages = crate::dial::StageSink::new(false);
                crate::dial::uses::run(&req, &stages, up_r, &mut down_w).await;
            });
            let mut rd = BufReader::new(down_r);
            let result = async {
                let ack = capped_line(&mut rd, 64 * 1024)
                    .await?
                    .ok_or("拨号链路没回 ack 就断了")?;
                let ack: Value =
                    serde_json::from_str(&ack).map_err(|e| format!("ack 认不出来：{e}"))?;
                if ack.get("ok").and_then(Value::as_bool) != Some(true) {
                    return Err(format!(
                        "连不上那台：{}",
                        ack.get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("（没说为什么）")
                    ));
                }
                let got = capped_line(&mut rd, (PULL_MAX_BYTES as u64) * 8)
                    .await?
                    .ok_or("那台跑完没交结果就断了")?;
                let got: Value =
                    serde_json::from_str(&got).map_err(|e| format!("结果认不出来：{e}"))?;
                let stdout = got.get("stdout").and_then(Value::as_str).unwrap_or("");
                if stdout.contains(HELLO_MARKER) {
                    return Err(
                        "那台的后端太旧，不认资产目录（一次性子命令进了流模式）".to_string()
                    );
                }
                if got.get("exit_status").and_then(Value::as_u64) != Some(0) {
                    let stderr = got.get("stderr").and_then(Value::as_str).unwrap_or("");
                    let said = serde_json::from_str::<Value>(stderr.trim())
                        .ok()
                        .and_then(|e| e.get("message").and_then(Value::as_str).map(str::to_string))
                        .unwrap_or_else(|| stderr.trim().to_string());
                    return Err(format!("那台的后端没办成：{said}"));
                }
                if stdout.len() >= PULL_MAX_BYTES {
                    return Err(format!(
                        "那台的目录超过 {PULL_MAX_BYTES} 字节 —— 拒收，不拿截断的用"
                    ));
                }
                Ok(stdout.to_string())
            }
            .await;
            task.abort();
            result
        })
    }
}

#[cfg(test)]
#[path = "../../tests/backend/asset_sync_tests.rs"]
mod tests;
