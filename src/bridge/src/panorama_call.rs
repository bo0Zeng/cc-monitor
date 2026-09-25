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
//!    `after = null` ⇒ 同一台后端的 `files-delete`（〔RM1e〕带 `expect = before`：盘上不是那一份就不删）→
//! ④ `stale`（盘上那份在算与写之间被别人改了）⇒ 回 ① 重算，最多 `user_files::EDIT_ATTEMPTS` 趟 →
//! ⑤ 文档关联那两种写成之后 `refresh_doc_links`（让文档关联的查询跟上；只写索引）。
//! 落盘只经 `user_files::Door`（monitor 碰用户文件的唯一开口）；**写的规则只住后端**，这里只「问 · 交」。
//! 本机与远端同一条路，只差 origin。
//!
//! # 〔RM1e〕那台机器上没有小程序 / 装的那份太旧 ⇒ 推过去再问一次（用户 09-24 V108「只传给开过远端全景的机器」）
//!
//! 远端 `panorama` 回 [`PUSH_ON`] 里的码（`not_installed`：找不到或不是它；`unsupported`：装的那份缺这个 op）
//! ⇒ 由 `panorama_bytes::push_to` 按那台的 `uname -s -m` 取内嵌字节、经本机常驻后端那条 `files` 链路
//! （与 F08 部署后端同一条路、同一道「只许两根」围栏）推到 `~/.cc-monitor/bin/cc-monitor-panorama`，**再问一次**；
//! 仍缺 ⇒ 如实说，不循环（[`ask_or_push`]）。每台机器一把锁：同时几问都撞上「没装」时只推一份。
//! **本机不推**（本机的字节怎么到位是「本机对称」那一拍的事）。
//! 「是不是同一代」只按 `--probe` 的能力表判（缺 op ⇒ `unsupported`）；为什么不比字节指纹写在
//! `调研/第四波记录/RM1e.md §1.1`。
//!
//! 〔RM1e〕为了拿到对端的**码**，这里自己是发送端（`client_for` ＋ 共用分流器 `route_call_error`，
//! 码从分流器递回的 `(code, message)` 里认，不自己 match 错误枚举 —— 与 `user_files.rs::BackendDoor::ask` 同形；
//! 登记在 `backend_route_tests::SENDERS`）。〔墓碑 —— RM1c 那一版经 `frame_query::call` 发，它把码压进了一句话。〕
//!
//! # 〔RM1f〕可取消（建索引点下去可以撤）
//!
//! `panorama_call` 带一张 `ticket`（界面每一问现造一张）；界面点「取消」⇒ [`panorama_cancel`] 拉那张票的铃 ⇒
//! 等那一问的 future 被**丢掉** ⇒ `inbound_client::call` 的放弃守卫补发 `cancel{target}` ⇒ 后端 `panorama`
//! （可取消档）撤掉处理器、小程序连同 `timeout` 前缀那一组子进程一起被杀。界面收到 [`CANCELLED_SAID`]。
//! ⚠ 撤单是 best-effort 的那一格与超时同一格（`cancel` 那一行入不了写队列就不发）；撤得掉的只有**在后端跑着**
//! 的那一段 —— 推字节那一段（`push_to`）被丢时上传中途停下，远端留一份暂存旁名，下一次推照常覆盖。
//!
//! # 诚实边界
//!
//! - 〔墓碑 —— RM1c 那一版这里写着「打不断：后端那一侧是阻塞档（`cancel` 回 `not_cancellable`）；这边等到期限为止」。〕
//! - 〔RM1e〕删批注的 CAS 闭合在后端那一侧（核与删在同一个函数里紧挨着，窗只剩那两行之间 —— 后端写面头注那条 TOCTOU）。
//!   〔墓碑 —— RM1d 那一版这里写着「删批注**没有 CAS**（`files-delete` 不收 `expect`）：删前 `peek` 核一遍，核与删之间仍有一个窗口」。〕

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client::client_for;
use crate::origin::Origin;
use serde_json::{json, Value};
use std::time::Duration;

/// 后端那条帧命令的名字。
const FRAME_CMD: &str = "panorama";

/// ★〔RM1e〕对端回这几个码 ⇒ 那台机器上缺小程序 / 装的那份太旧 ⇒ 推字节再问一次。
///
/// == 后端适配层把「找不到 · 不是它 · 缺能力」映射出来的码（判据运行时读后端源码，两向）。
pub(crate) const PUSH_ON: &[&str] = &["not_installed", "unsupported"];

/// 〔RM1e〕问了一次没问成：对端说了话时带着它的码（没通道 / 没发出去 ⇒ `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Asked {
    pub code: Option<String>,
    pub said: String,
}

impl Asked {
    fn plain(said: String) -> Self {
        Asked { code: None, said }
    }
    /// 这一次失败是不是「那台缺小程序 / 太旧」。
    pub(crate) fn wants_bytes(&self) -> bool {
        self.code.as_deref().is_some_and(|c| PUSH_ON.contains(&c))
    }
}

/// 〔RM1e〕问一次；撞上「缺 / 旧」⇒ 推一次、再问一次。**推最多一次，问最多两次。**
///
/// `ask` / `push` 由调用方给（生产侧 = 那台后端 ＋ `panorama_bytes::push_to`；判据用替身数次数）。
/// 〔RM1f〕`announce`：**真要推之前**说一声（生产侧 = 远端健康通道上那句「正在把代码全景组件装到 <机器>」）——
/// 推是同步等的（发版 20 MB，慢链路上几十秒起），不说的话界面上只有那一问在转圈。恰在推之前、恰一次；不推就不说。
pub(crate) async fn ask_or_push<A, FA, N, P, FP>(
    mut ask: A,
    announce: N,
    push: P,
) -> Result<Value, String>
where
    A: FnMut() -> FA,
    FA: std::future::Future<Output = Result<Value, Asked>>,
    N: FnOnce(),
    P: FnOnce() -> FP,
    FP: std::future::Future<Output = Result<(), String>>,
{
    let first = match ask().await {
        Err(a) if a.wants_bytes() => a,
        other => return other.map_err(|a| a.said),
    };
    announce();
    push().await.map_err(|e| {
        format!(
            "{}\n—— 试着把代码全景组件推到那台机器上，没成：{e}",
            first.said
        )
    })?;
    match ask().await {
        Err(again) if again.wants_bytes() => Err(format!(
            "已经把这一版的代码全景组件推过去了，那台机器仍然说：{}",
            again.said
        )),
        other => other.map_err(|a| a.said),
    }
}

/// 每台机器一把「正在推」的锁（同一台同时几问都撞上「没装」时只推一份）。
fn push_lock(origin: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let mut m = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    m.entry(origin.to_string()).or_default().clone()
}

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
                // 〔RM1e〕删也带 CAS（`files-delete` 的 `expect`）：盘上不再是算的那一份 ⇒ 后端回 `stale` ⇒ 重算。
                // 〔墓碑 —— RM1d 那一版先 `door.peek` 核、再删，核与删之间整整一趟往返的窗。〕
                // `after = None` 而 `before = None` 上面已按「没事可做」回了 ⇒ 这里 `before` 恒在。
                let Some(expect) = edit.before.as_deref() else {
                    return Ok(planned.value);
                };
                match door.delete(repo, &edit.rel, expect).await {
                    Ok(()) => return Ok(planned.value),
                    Err(Refused::Stale(s)) => last = s,
                    Err(e @ (Refused::Other(_) | Refused::Peer { .. })) => return Err(e.said()),
                }
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
///
/// 〔RM1f〕`ticket`：给了 ⇒ 这一问能被 [`panorama_cancel`] 撤掉（头注「可取消」一节）；不给 ⇒ 等到结局为止。
#[tauri::command]
pub async fn panorama_call(
    app: tauri::AppHandle,
    origin: Origin,
    op: String,
    repo: Option<String>,
    args: Option<Value>,
    ticket: Option<String>,
) -> Result<Value, String> {
    let _ = origin.route("panorama_call")?;
    let asked = ask(&app, &origin, &op, repo.as_deref(), args);
    match ticket {
        None => asked.await,
        Some(t) => with_ticket(t, asked).await,
    }
}

/// 〔RM1f〕撤掉一张票（界面上「取消」那一下）。回这张票此刻在不在飞（不在 ⇒ 已经有结局了，撤了也没用）。
#[tauri::command]
pub fn panorama_cancel(ticket: String) -> bool {
    let bell = lock_tickets().get(&ticket).cloned();
    match bell {
        Some(b) => {
            // `notify_one` 在还没人等时也会留一张许可 ⇒ 撤单先到、等待后到也撤得掉。
            b.notify_one();
            true
        }
        None => false,
    }
}

/// 〔RM1f〕被撤掉的那一问交给界面的那句话（界面按「是我撤的」认它，不当失败弹）。
pub(crate) const CANCELLED_SAID: &str = "已取消（后端那一趟已经停下）";

/// 〔RM1f〕在飞的票：票 → 撤单铃。进程内、按票登记、有结局即摘（[`TicketGuard`]）。
fn lock_tickets() -> std::sync::MutexGuard<
    'static,
    std::collections::HashMap<String, std::sync::Arc<tokio::sync::Notify>>,
> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static TICKETS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Notify>>>> = OnceLock::new();
    TICKETS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 〔RM1f〕这张票的登记随它一起走（有结局 / 被撤 / 调用方自己被丢，都摘掉）。
struct TicketGuard(String);

impl Drop for TicketGuard {
    fn drop(&mut self) {
        lock_tickets().remove(&self.0);
    }
}

/// 〔RM1f〕带票等一问：票被撤 ⇒ `asked` 被**丢掉**（⇒ `inbound_client::call` 的放弃守卫补发 `cancel`
/// ⇒ 后端撤掉处理器、杀掉小程序那一组子进程），回 [`CANCELLED_SAID`]。
/// 同一张票同时只许一问（前一问的撤单铃被后一问顶掉，前一问就撤不掉了 ⇒ 当场拒）。
pub(crate) async fn with_ticket<F>(ticket: String, asked: F) -> Result<Value, String>
where
    F: std::future::Future<Output = Result<Value, String>>,
{
    let bell = std::sync::Arc::new(tokio::sync::Notify::new());
    {
        let mut t = lock_tickets();
        if t.contains_key(&ticket) {
            return Err(format!(
                "代码全景：取消票 `{ticket}` 还有一问在飞，这一问没有发出去（界面应当每一问一张新票）"
            ));
        }
        t.insert(ticket.clone(), bell.clone());
    }
    let _registered = TicketGuard(ticket);
    tokio::select! {
        r = asked => r,
        () = bell.notified() => Err(CANCELLED_SAID.to_string()),
    }
}

/// 分流器那三态里给人看的那句话。
fn routed_text(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
        Routed::Done => "代码全景这一问出了内部错误，没有拿到结果".to_string(),
    }
}

/// 发一次 `panorama` 帧命令、拿 `result`；失败带着对端的码（〔RM1e〕推字节要按码判）。
async fn ask_once(
    origin: &Origin,
    op: &str,
    repo: Option<&str>,
    args: Option<Value>,
) -> Result<Value, Asked> {
    let wire = origin.as_wire_str();
    let who = crate::backend::control::cc_bus::machine_label(wire);
    let Some(client) = client_for(wire) else {
        return Err(Asked::plain(routed_text(no_channel(wire))));
    };
    if !client.accepts(FRAME_CMD) {
        return Err(Asked::plain(
            crate::backend::control::cc_bus::describe_backend_too_old_for(
                wire,
                FRAME_CMD,
                "代码全景在这台上用不了",
            ),
        ));
    }
    // 对端说了话时它给的那个码（分流器递回来的 `(code, message)` 里认，不自己 match 错误枚举）。
    let peer_code: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
    match client
        .call(FRAME_CMD, frame_args(op, repo, args), budget_for(op))
        .await
    {
        Ok(Some(data)) => data.get("result").cloned().ok_or_else(|| {
            Asked::plain(format!(
                "{who} 的 `panorama` 应答没有 `result` —— 两端契约对不上"
            ))
        }),
        Ok(None) => Err(Asked::plain(format!(
            "{who} 的 `panorama` 回了一条空应答 —— 两端契约对不上"
        ))),
        Err(e) => {
            let said = routed_text(route_call_error(&e, |code, message| {
                *peer_code.borrow_mut() = Some(code.to_string());
                format!("{who} 的代码全景没答上来（{code}）：{message}")
            }));
            Err(Asked {
                code: peer_code.into_inner(),
                said,
            })
        }
    }
}

/// 问一次（两条命令共用）；远端回「缺 / 旧」⇒ 推字节再问一次（头注〔RM1e〕那一节）。
/// 〔RM1f〕`app`：推之前在远端健康通道上说一声（[`INSTALL_NOTICE_KIND`]）。
async fn ask(
    app: &tauri::AppHandle,
    origin: &Origin,
    op: &str,
    repo: Option<&str>,
    args: Option<Value>,
) -> Result<Value, String> {
    // 〔RM1f〕本机也走这一格：本机后端答「没装 / 装的太旧」⇒ 把这一份产物带着的小程序放到本机后端找得到的地方、再问一次。
    // 〔墓碑 —— RM1e 那一版这里是 `a.wants_bytes() && !origin.is_local()`：本机不推。〕
    match ask_once(origin, op, repo, args.clone()).await {
        Err(a) if a.wants_bytes() => {}
        other => return other.map_err(|a| a.said),
    }
    // 拿到锁先**再问一次**（前一个人可能刚推完）—— `ask_or_push` 的第一问就是它。
    let lock = push_lock(origin.as_wire_str());
    let _pushing = lock.lock().await;
    ask_or_push(
        || ask_once(origin, op, repo, args.clone()),
        || announce_install(app, origin.as_wire_str()),
        || crate::panorama_bytes::push_to(origin),
    )
    .await
}

/// 〔RM1f〕远端健康通道上那一句的 `kind`（前端 `remote-health.ts` 的标题表认它）。
pub(crate) const INSTALL_NOTICE_KIND: &str = "panorama-install";

/// 〔RM1f〕那一句说什么（纯函数，判据直接比）。
pub(crate) fn install_notice(origin: &str) -> crate::bridge::RemoteHealthPayload {
    let message = if origin == crate::backend::control::inbound_client::LOCAL_ORIGIN {
        // 〔RM1f〕本机那一台不经网络：放到 `~/.cc-monitor/bin/`，一两秒的事。
        "本机还没有这一版的代码全景组件，正在把它放好（一两秒）。放好之后这一问会自己接着答。"
            .to_string()
    } else {
        let who = crate::backend::control::cc_bus::machine_label(origin);
        format!(
            "{who} 上还没有这一版的代码全景组件，正在把它装上去（约 20 MB，慢链路上要等一会儿）。装好之后这一问会自己接着答。"
        )
    };
    crate::bridge::RemoteHealthPayload {
        origin: origin.to_string(),
        kind: INSTALL_NOTICE_KIND.to_string(),
        message,
    }
}

/// 〔RM1f〕真要推之前说一声（远端健康通道，一条 toast）。发不出去只记日志 —— 推照推。
fn announce_install(app: &tauri::AppHandle, origin: &str) {
    use tauri::Emitter;
    if let Err(e) = app.emit(crate::bridge::events::REMOTE_HEALTH, install_notice(origin)) {
        tracing::warn!("[{origin}] 「正在装代码全景组件」那一句没发出去：{e}");
    }
}

/// 〔RM1d〕写批注 / 文档关联：问那台机器要计划、经那台机器后端的文件管理落盘（头注 ①–⑤）。
/// `op` 只许 [`EDITS`] 第一列；回的 `value` 与本机进程内那几条写命令的返回值同形（id / 在不在 / `null`）。
#[tauri::command]
pub async fn panorama_edit(
    app: tauri::AppHandle,
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
        let (app, origin, repo, args) = (app.clone(), origin.clone(), repo.clone(), args.clone());
        async move {
            if local {
                crate::panorama::plan_local(repo, plan_op, args).await
            } else {
                ask(&app, &origin, plan_op, Some(&repo), Some(args)).await
            }
        }
    })
    .await?;
    if *refresh {
        let refreshed = if local {
            crate::panorama::refresh_doc_links_local(repo.clone()).await
        } else {
            ask(&app, &origin, REFRESH_DOC_LINKS, Some(&repo), None)
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
