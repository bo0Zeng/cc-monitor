//! 〔C4d · 第四波 4B〕**本机后端问远端后端**的那一跳 —— 全后端**只此一处**。
//!
//! # 出处与裁决
//!
//! `设计/01 §3.5`：「观测方沿它本来就拥有的那条连接去拉被观测方」（零新通道）。
//! AS2 用这一形造出了资产目录的自动同步（`调研/第四波记录/AS2.md §3.4`）；C4c 的历史跨机 join 要同一跳
//! （`C4c.md §3.3`）。主会话 09-25 裁：「**把 `DialRemote` ＋ 可达表从 `asset_sync.rs` 提到中立住址
//! `src/backend/remote_ask.rs`（逻辑一字不改），`asset_sync` 改调它**」—— 一路造、两路用，不各写一份。
//!
//! # 形状（乙：池里那条连接上多开一个 capture exec）
//!
//! ```text
//!  monitor（宿主，只交事实） ── remote-reach {origin, dial, backend} ──▶ 可达表（内存）
//!  调用方 ── ask_with(machine, argv, 表, 对面) ──▶ 查表 ──▶ DialRemote.run(dial, "<backend> <argv…>")
//!                                          └─ dial::uses::run（池里那条 SSH，多一个 exec 通道，不是新连接）
//! ```
//!
//! # 为什么是 capture 一次性子命令，不起远端流模式
//!
//! 流模式一起来就往 tmux server 装**全局** hook（`control/tmux_hook.rs::install_hooks`，载荷里烤着
//! **那个进程**的 pid）—— 一个用完就退的流会把 monitor 那条真流的 hook 盖成一个死 pid。
//! 老后端不认子命令会进流模式、第一行是 hello ⇒ capture 带 `abort_marker`，见到就收工、报「太旧」。
//!
//! # 可达表（内存）
//!
//! `origin → {拨号请求, 远端后端路径, 对面的资产目录 id}`：只在本进程里，后端重启就空（下次那台连上再填）。
//! 拨号请求里只有路径（`key_path`），没有私钥本体（凭据面 `K11` 同 `dial_host::request`）。
//! **写口只有 [`register`] 一个**：`remote-reach`（每台远端流握手时 monitor 无条件交）与 `assets-sync`
//! （顺手登记）都调它。

use copy_core::copy_text;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// 拉回来那一份（远端 stdout）的上限。目录文件本身 16 MiB 封顶（`asset_catalog::CATALOG_MAX_BYTES`），这里取同一个数。
pub const PULL_MAX_BYTES: usize = 16 * 1024 * 1024;

/// 可达表的条数上限（有界资源；一个人配不出这么多台远端）。
pub const MAX_REACH: usize = 256;

/// 老后端不认一次性子命令会进流模式、第一行是 hello —— capture 看见它就收工（不让它装 hook、不挂住）。
pub(crate) const HELLO_MARKER: &str = "\"kind\":\"hello\"";

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
    pub(crate) dial: Value,
    pub(crate) backend: String,
    pub(crate) peer: Option<String>,
}

/// 可达表（本进程一张）。
pub type Table = Mutex<BTreeMap<String, Reach>>;

pub(crate) static REACH: Table = Mutex::new(BTreeMap::new());

pub(crate) fn lock(t: &Table) -> std::sync::MutexGuard<'_, BTreeMap<String, Reach>> {
    t.lock().unwrap_or_else(|e| e.into_inner())
}

/// 可达表**唯一的写口**：`{origin, dial, backend}` 三格齐了才记（`origin` 是 monitor 交来的名字，本后端只当不透明的键用）。
/// 同一台再记一次 ⇒ 换新的拨号请求与路径，`peer`（对面的资产目录 id）留着。
/// 〔C4d〕这一段原样搬自 `asset_sync::answer_with`（AS2），逻辑一字不改；`remote-reach` 与 `assets-sync` 都经它。
pub(crate) fn register(table: &Table, args: &Value) -> Result<String, (&'static str, String)> {
    let o = args.get("origin").and_then(Value::as_str).ok_or((
        "bad_args",
        crate::common::contract::malformed("missing `origin` (the machine name)"),
    ))?;
    if o.is_empty() {
        return Err((
            "bad_args",
            crate::common::contract::malformed("`origin` is empty").into(),
        ));
    }
    let dial = args.get("dial").filter(|d| d.is_object()).ok_or((
        "bad_args",
        crate::common::contract::malformed("`origin` given without `dial` (a dial request)"),
    ))?;
    let backend = args
        .get("backend")
        .and_then(Value::as_str)
        .filter(|b| !b.is_empty())
        .ok_or((
            "bad_args",
            crate::common::contract::malformed(
                "`origin` given without `backend` (the backend path on that machine)",
            ),
        ))?;
    // 〔TL3 · `INVARIANTS §47` ②〕可达表是这台后端往远端拼命令时「那台后端在哪」的唯一出处（`command_line` ·
    //   `asset_sync::{pull_command, push_command}` 都读它）⇒ 在这扇唯一的写口先过放行判定，判不过一条都不登记。
    //   规则是本后端里那一份同族判定（形式 ＋ 拒绝集，`accounts_query::is_safe_config_dir`），不另写。
    if !crate::observe::accounts_query::is_safe_config_dir(backend) {
        return Err((
            "bad_args",
            copy_text(
                "beRemoteAsk.register.backendPathRefused",
                &[("origin", &o.to_string()), ("path", &format!("{backend:?}"))],
            ),
        ));
    }
    let mut t = lock(table);
    if !t.contains_key(o) && t.len() >= MAX_REACH {
        return Err((
            "bad_args",
            copy_text(
                "beRemoteAsk.register.full",
                &[("max", &MAX_REACH.to_string())],
            ),
        ));
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
    Ok(o.to_string())
}

/// 可达表现状的线上形状：`[{origin, machine}]`（`machine` = 对面的资产目录 id，没拉过 ⇒ `null`）。
pub(crate) fn reach_rows(table: &Table) -> Vec<Value> {
    lock(table)
        .iter()
        .map(|(o, r)| json!({ "origin": o, "machine": r.peer }))
        .collect()
}

/// `remote-reach`：只登记、不做别的（monitor 在每台远端流握手那一刻交；老远端也登记 ——
/// 历史清单问它的是 `--list-projects` 这种老子命令）。回 `{origin, reach}`。
pub fn answer_reach(args: &Value) -> Result<Value, (&'static str, String)> {
    answer_reach_with(args, &REACH)
}

/// [`answer_reach`] 的可喂夹具那一半：可达表由调用方给。
pub fn answer_reach_with(args: &Value, table: &Table) -> Result<Value, (&'static str, String)> {
    let o = register(table, args)?;
    Ok(json!({ "origin": o, "reach": reach_rows(table) }))
}

/// 远端上那条一次性命令的完整字面：`<后端> <argv…>`，**每一格都过 POSIX 单引号**（路径、项目目录名都是自由文本）。
pub fn command_line(backend: &str, argv: &[&str]) -> String {
    let mut s = shell_quote_core::posix_quote(backend);
    for a in argv {
        s.push(' ');
        s.push_str(&shell_quote_core::posix_quote(a));
    }
    s
}

/// 够不到那台时给的那句话（不猜、不回落）。
pub(crate) fn unreachable_message(machine: &str) -> String {
    copy_text(
        "beRemoteAsk.unreachableMessage.say",
        &[("machine", &machine.to_string())],
    )
}

/// 问可达表里的那一台跑一条一次性子命令，交回它的 stdout。表与对面由调用方给 —— 生产调用方交进程里那张表（[`REACH`]）
/// ＋ 真拨号（[`DialRemote`]），判据交自己的表与替身（数得出「一台问了几次」）。
/// `machine` = 可达表的键（monitor 交来的那台的名字；本后端只当不透明的键用 —— 参数名刻意不叫 origin：
/// 那个概念在 monitor 里有自己的类型，这里只是一张表的键）。
pub async fn ask_with(
    machine: &str,
    argv: &[&str],
    table: &Table,
    remote: &dyn Remote,
) -> Result<String, String> {
    let r = lock(table)
        .get(machine)
        .cloned()
        .ok_or_else(|| unreachable_message(machine))?;
    remote.run(&r.dial, command_line(&r.backend, argv)).await
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
        .map_err(|e| {
            copy_text(
                "beRemoteAsk.cappedLine.readFailed",
                &[("e", &e.to_string())],
            )
        })?;
    if n == 0 {
        return Ok(None);
    }
    if buf.len() as u64 > cap {
        return Err(copy_text(
            "beRemoteAsk.cappedLine.tooLong",
            &[("cap", &cap.to_string())],
        ));
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
            let obj = v.as_object_mut().ok_or(crate::common::contract::malformed(
                "dial request is not an object",
            ))?;
            obj.insert("use".into(), json!("capture"));
            obj.insert("command".into(), json!(command));
            obj.insert(
                "capture".into(),
                json!({ "max_bytes": PULL_MAX_BYTES, "abort_marker": HELLO_MARKER }),
            );
            obj.insert("stages".into(), json!(false));
            obj.insert("probe".into(), json!(false));
            let req = crate::dial::parse_request_value(&v).map_err(|e| {
                crate::common::contract::malformed(&format!("dial request unreadable: {e}"))
            })?;
            pull_over(move |up_r, mut down_w| async move {
                let stages = crate::dial::StageSink::new(false);
                crate::dial::uses::run(&req, &stages, up_r, &mut down_w).await;
            })
            .await
        })
    }
}

/// 〔NT2 · A4〕一个 `spawn` 出去的任务，**句柄被丢 ⇒ 任务被收**。
///
/// tokio 的 `JoinHandle` 被丢是**脱钩**（任务照跑），不是收。本模块的内层任务手里攥着池里那条 SSH 连接的一格通道：
/// 调用方帧期限到点 ⇒ monitor 补发 `cancel` ⇒ 后端打断的是**外层** future（`Run::Async` 那一档）——
/// 句柄若只是被丢，内层那一格就**永远占着**（远端不答的那一形），`设计/15 §3.2` 第 4 条红线说的正是这个。
pub(crate) struct AbortOnDrop(pub(crate) tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// 经一条内存链路跑一趟 capture：`serve` 拿上行读端与下行写端（生产 = `dial::uses::run`），这里读 ack 与结果那一行。
/// **抽出来是为了判据**：「外层被丢 ⇒ 内层一起收」不需要真 SSH 就验得动（`remote_ask_tests` 喂一个永不答的 `serve`）。
pub(crate) async fn pull_over<F, Fut>(serve: F) -> Result<String, String>
where
    F: FnOnce(tokio::io::DuplexStream, tokio::io::DuplexStream) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    // 上行那根管子我们一个字节都不写（capture 不读上行）；留着不关，直到拿到结果。
    let (_up_w, up_r) = tokio::io::duplex(1024);
    let (down_w, down_r) = tokio::io::duplex(64 * 1024);
    // 〔NT2〕随本函数（的 future）一起死：正常答完 / 出错返回 / 被 `cancel` 打断，三种收法同一个 `Drop`。
    let _task = AbortOnDrop(tokio::spawn(serve(up_r, down_w)));
    let mut rd = BufReader::new(down_r);
    let ack = capped_line(&mut rd, 64 * 1024)
        .await?
        .ok_or_else(|| copy_text("beRemoteAsk.run.droppedBeforeReady", &[]))?;
    let ack: Value = serde_json::from_str(&ack)
        .map_err(|e| crate::common::contract::malformed(&format!("unreadable ack: {e}")))?;
    if ack.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(copy_text(
            "beRemoteAsk.run.unreachable",
            &[(
                "why",
                &(ack
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or(&copy_text("beRemoteAsk.run.noReason", &[])))
                .to_string(),
            )],
        ));
    }
    let got = capped_line(&mut rd, (PULL_MAX_BYTES as u64) * 8)
        .await?
        .ok_or_else(|| copy_text("beRemoteAsk.run.droppedBeforeResult", &[]))?;
    let got: Value = serde_json::from_str(&got)
        .map_err(|e| copy_text("beRemoteAsk.run.resultUnreadable", &[("e", &e.to_string())]))?;
    let stdout = got.get("stdout").and_then(Value::as_str).unwrap_or("");
    if stdout.contains(HELLO_MARKER) {
        return Err(copy_text("beRemoteAsk.run.tooOld", &[]));
    }
    if got.get("exit_status").and_then(Value::as_u64) != Some(0) {
        let stderr = got.get("stderr").and_then(Value::as_str).unwrap_or("");
        let said = serde_json::from_str::<Value>(stderr.trim())
            .ok()
            .and_then(|e| e.get("message").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_else(|| stderr.trim().to_string());
        return Err(copy_text(
            "beRemoteAsk.run.failed",
            &[("said", &said.to_string())],
        ));
    }
    if stdout.len() >= PULL_MAX_BYTES {
        return Err(copy_text(
            "beRemoteAsk.run.tooLarge",
            &[("max", &PULL_MAX_BYTES.to_string())],
        ));
    }
    Ok(stdout.to_string())
}

#[cfg(test)]
#[path = "../../tests/backend/remote_ask_tests.rs"]
mod tests;
