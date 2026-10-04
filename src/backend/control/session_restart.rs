//! **换号重启**：查号 → 找这条会话此刻在哪个终端 → （可选）先请求压缩、等记录里出现压缩摘要 → 停旧 → 同一终端名里用新号起 → 等它报出。
//! 全在会话所在那台做完；界面只发起、只按回复说一句。
//!
//! - 步与步之间是 await 点：撤单（或界面关了不再等）在那里生效。**停旧 ＋ 起新是不可分的一步**：拿退出排空的票、在阻塞线程上一口气做完，
//!   开跑之后撤单也照样做完（只停不起会把会话丢在半路）。
//! - 停失败 ⇒ `stop_failed`，**一定不起新的**（新旧两个进程会抢同一条会话）。起失败（旧的已停）⇒ `start_failed`，`data` 带终端名。
//! - 压缩：问适配层「请求压缩那一句」；不支持 ⇒ `unsupported` 照常往下；到期限没见摘要 ⇒ `timed_out` 照常往下。
//!
//! 要动 tmux / 读观测的几样由入口经 [`Host`] 交进来（control 不引用 observe），判据交替身。

use super::launch::LAUNCH_TOTAL;
use super::launch_account::{self as la, AccountAsk, Settled};
use super::launch_render::Failed;
use super::session_batch::{self as batch, Batch, Deps, Item, Standing, SESSIONS_WHERE_TOTAL};
use crate::platform::child::{Budget, Deadline};
use serde_json::{json, Map, Value};
use std::future::Future;

/// 命令级失败（码 ＋ 那一句 ＋ 按码定形的 `data`），全是自有值：跨阻塞线程交回来。
pub(crate) type Fault = (String, String, Option<Value>);

fn owned((c, m, d): Failed) -> Fault {
    (c.to_string(), m, d)
}

/// 两个期限的上界（兜坏输入；界面今天给的是几分钟）。
pub(crate) const MAX_WAIT_MS: u64 = 3_600_000;

/// 停旧 ＋ 起新那一步的总期限：停一次（同 `kill` 的总期限）＋ 起一个（同一个的批量起）。界面等这条命令的是两个等待 ＋ `RESTART_BASE_MS`。
pub(crate) const SWAP_TOTAL: Deadline =
    Deadline::secs(8 + batch::BATCH_TOTAL_BASE_SECS + batch::BATCH_TOTAL_EACH_SECS);

/// 一次装好的等待：交期限（毫秒）⇒ 等到没有。
pub(crate) trait Wait: Send + 'static {
    fn within(self, ms: u64) -> impl Future<Output = bool> + Send;
}

/// 入口交进来的几样（生产那一份在 `faces/session_restart_face.rs`）。
pub(crate) trait Host: Send + Sync + 'static {
    type Ears: Wait;
    /// 在阻塞线程上、带这台的生产事实做一步。
    fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Deps<'_>) -> T + Send + 'static,
    ) -> impl Future<Output = T> + Send;
    /// 不可分的那一步：拿退出排空的票再做；进程正在收场 ⇒ `None`（一个字节不动）。外层被撤也做完。
    fn critical<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Deps<'_>) -> T + Send + 'static,
    ) -> impl Future<Output = Option<T>> + Send;
    /// 这一家请求压缩用的那一句（适配层）。
    fn compact_request(&self, agent: &str) -> Option<&'static str>;
    /// 装好「等这条会话的记录里出现压缩摘要」（送那一句之前装）。
    fn watch_compact(&self, sid: &str) -> impl Future<Output = Result<Self::Ears, String>> + Send;
    /// 装好「等这条会话由一个新进程报出」（停旧之前装）。
    fn watch_arrival(&self, sid: &str) -> impl Future<Output = Result<Self::Ears, String>> + Send;
}

/// 已过形状关的一次请求。
pub(crate) struct Req {
    item: Item,
    compact_first: bool,
    compact_within_ms: u64,
    arrive_within_ms: u64,
    local: bool,
    batch: Batch,
}

fn bad(why: &str) -> Failed {
    let (c, m) = batch::bad(why);
    (c, m, None)
}

fn ms_of(o: &Map<String, Value>, k: &str) -> Result<u64, Failed> {
    o.get(k)
        .and_then(Value::as_u64)
        .filter(|n| *n <= MAX_WAIT_MS)
        .ok_or_else(|| bad(&format!("`{k}` must be 0..={MAX_WAIT_MS} (ms)")))
}

/// `{sid, cwd, account, compact_first, compact_within_ms, arrive_within_ms, local, agent, launcher, defaultLauncher, models?}`。
pub(crate) fn parse(args: &Value) -> Result<Req, Failed> {
    let o = args
        .as_object()
        .ok_or_else(|| bad("args must be an object"))?;
    let s = |k: &str| batch::str_of(o, k).map_err(|(c, m)| (c, m, None));
    let sid = s("sid")?;
    if !shell_quote_core::session_id_ok(sid) {
        return Err(bad(&format!("not a session id: {sid:?}")));
    }
    let account = s("account")?;
    if account.is_empty() {
        return Err(bad("`account` must name an account"));
    }
    let flag = |k: &str| {
        o.get(k)
            .and_then(Value::as_bool)
            .ok_or_else(|| bad(&format!("missing bool `{k}`")))
    };
    Ok(Req {
        item: Item {
            sid: sid.to_string(),
            cwd: s("cwd")?.to_string(),
            account: AccountAsk::Named {
                name: Some(account.to_string()),
                config_dir: None,
            },
        },
        compact_first: flag("compact_first")?,
        compact_within_ms: ms_of(o, "compact_within_ms")?,
        arrive_within_ms: ms_of(o, "arrive_within_ms")?,
        local: flag("local")?,
        batch: batch::batch_of(o).map_err(|(c, m)| (c, m, None))?,
    })
}

/// 开动之前那一步：点名的号选得了 ＋ 这条会话恰好在一个终端里跑着 ⇒ `(号, 终端名)`。
fn prepare(req: &Req, deps: &Deps) -> Result<(Settled, String), Failed> {
    let _total = Budget::start(SESSIONS_WHERE_TOTAL);
    // 模型偏好只用在远端那一行上（本机那一行不带模型，同批量起）。
    let none = Default::default();
    let models = if req.local { &none } else { &req.batch.models };
    let account = la::settle(
        &req.item.account,
        Some(&req.item.sid),
        models,
        deps.accounts,
    )
    .map_err(super::launch_render::unavailable)?;
    let rows = (deps.list)().map_err(|m| ("unobservable", m, None))?;
    let not_here = || {
        (
            "not_in_terminal",
            copy_core::copy_text(
                "beSessionRestart.locate.notInTerminal",
                &[("sid", &req.item.sid)],
            ),
            None,
        )
    };
    match rows.as_deref().map(|r| batch::standing(r, &req.item.sid)) {
        Some(Standing::Running(name)) => Ok((account, name)),
        Some(Standing::Ambiguous(names)) => Err((
            "ambiguous",
            copy_core::copy_text(
                "beSessionRestart.locate.ambiguous",
                &[("names", &names.join(", "))],
            ),
            Some(json!({ "names": names })),
        )),
        Some(Standing::Idle(_) | Standing::None) | None => Err(not_here()),
    }
}

/// 不可分的那一步：停旧（失败 ⇒ 不起）→ 同一个名字用新号起。
fn swap(req: &Req, account: &Settled, name: &str, deps: &Deps) -> Result<(), Failed> {
    let _total = Budget::start(SWAP_TOTAL);
    if let Err((why, said)) = (deps.kill)(name, &req.item.sid) {
        return Err(("stop_failed", said, Some(json!({ "why": why }))));
    }
    let a = batch::start_named(
        &req.item,
        &req.batch,
        account,
        name.to_string(),
        req.local,
        deps,
    );
    if a.outcome == "done" {
        return Ok(());
    }
    Err((
        "start_failed",
        a.detail,
        Some(json!({ "terminal": name, "why": a.why })),
    ))
}

/// 先压缩那一步的结局（不论哪种都照常往下）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compact {
    Done,
    TimedOut,
    Skipped,
    Unsupported,
    Failed,
}

impl Compact {
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            Compact::Done => "done",
            Compact::TimedOut => "timed_out",
            Compact::Skipped => "skipped",
            Compact::Unsupported => "unsupported",
            Compact::Failed => "failed",
        }
    }
}

async fn compact<H: Host>(req: &Req, name: &str, host: &H) -> Compact {
    if !req.compact_first {
        return Compact::Skipped;
    }
    let Some(line) = host.compact_request(&req.batch.agent).map(str::to_string) else {
        return Compact::Unsupported;
    };
    // 先装耳朵再送那一句：摘要来得比装耳朵快也漏不掉。
    let Ok(ears) = host.watch_compact(&req.item.sid).await else {
        return Compact::Failed;
    };
    let (name, sid) = (name.to_string(), req.item.sid.clone());
    let sent = host
        .blocking(move |d| {
            let _total = Budget::start(LAUNCH_TOTAL);
            (d.send_into)(&name, &sid, &line).is_ok()
        })
        .await;
    if !sent {
        return Compact::Failed;
    }
    if ears.within(req.compact_within_ms).await {
        Compact::Done
    } else {
        Compact::TimedOut
    }
}

/// `session-restart` 本体 ⇒ `{compact, started, terminal, account}`。
pub(crate) async fn run<H: Host>(args: Value, host: H) -> Result<Value, Fault> {
    let req = std::sync::Arc::new(parse(&args).map_err(owned)?);
    let r = req.clone();
    let (account, name) = host
        .blocking(move |d| prepare(&r, d).map_err(owned))
        .await?;
    let compact = compact(&req, &name, &host).await;
    // 停之前装好「等它报出」：此刻在跑的那个进程不算。
    let arrival = host.watch_arrival(&req.item.sid).await;
    let (r, a, n) = (req.clone(), account.clone(), name.clone());
    match host
        .critical(move |d| swap(&r, &a, &n, d).map_err(owned))
        .await
    {
        None => {
            return Err((
                "shutting_down".to_string(),
                copy_core::copy_text("beInbound.drain.shuttingDown", &[]),
                None,
            ))
        }
        Some(done) => done?,
    }
    let started = match arrival {
        Ok(ears) => ears.within(req.arrive_within_ms).await,
        Err(_) => false,
    };
    Ok(json!({
        "compact": compact.as_wire(),
        "started": if started { "arrived" } else { "missed" },
        "terminal": name,
        "account": super::launch_render::launched(&account),
    }))
}

#[cfg(test)]
#[path = "../../../tests/backend/control/session_restart_tests.rs"]
mod tests;
