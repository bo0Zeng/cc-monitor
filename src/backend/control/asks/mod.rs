//! **待答登记表**（常驻后端进程里一份，通用、不认是哪一家）：会话里弹批准 / 提问 / 批计划的框时，那一家的答题 hook
//! 经 `needs-ask` 把请求挂进来、阻塞等；任一出口（桌面 · 手机）经 `needs-answer` 答，**第一个答案落下**，其余回「别处已答」（`gone` ＋ 那一句）；
//! hook 那条连接断了（终端那边先答了 · 被打断 · 到点）⇒ 撤掉，之后再答同样回 `gone`。挂上 · 撤掉 · 答掉都推 `changed {needs, key: sid}`。
//!
//! **挡着终端的那一种**（`AskIn::blocks_terminal`：那一家先跑 hook、hook 没给决定才弹框，如后台子 agent）：hook 交完就走、框当场在终端里出；
//! 这里照样记下（不跟那条连接走），记成「终端里有框」：那一家认得出那个框（[`crate::agents::AskFace::on_screen`]）⇒ 成品只给「允许 · 拒绝」，
//! app 里答经读屏驱动认准终端上正是这一条的框才按、按完复核（[`answer_needs_through`]）；认不得 ⇒ 一项都不给选、写「在终端里答」。终端那边答没答只由 [`hook::closed_now`]（同 hook 挂着等的那一种） 判
//! （盯这条会话的状态文件）；那一家没有状态文件 ⇒ 这条会话一轮的那几种来了才收（[`fold_unwatched`]）。收起照「别处已答」那一句。
//! 同一组 hook 交来的一轮的那几种（提交了一句 · 一轮结束，[`crate::agents::AskFace::heard`] 认）也经 `needs-ask` 来：`needs-ask` 收的是
//! 「那一家的会话 hook 交来的一份」，不止要人答的那一种（不另开命令：入参回包同形，另开要多一条命令面）；它们记进活会话表、当场回不给决定。
//!
//! 成品挂在会话的 `needs` 上（`observe::recognize::facts::dress_ask`）：`needs.ask` 是「这一条可以怎么答」—— 选项的字、语气、要不要附一句话都由这里写好，出口不按工具名取字；
//! pidfile 说在等、却没有挂着的请求（用户自己起的会话 · 只能本人答的框 · 工作区信任框 …）⇒ `ask: null`，`manualText` 写好「只能在终端里答」。
//!
//! **无人答时自动答**（只管提问；批准工具 · 批计划是安全闸，一律不自动答 —— 只此一处判，[`auto_of`]）：这条会话的注解开着
//! `autoAnswer` ⇒ 挂上的提问自己带期限（`needs-ask` 那条在等的，零定时器：期限跟着这一次挂着的走），等满了还没人答就替人答 ——
//! 每一题答成「其他」＋ 那一句。
//!
//! **上一条请求怎么收的场**（答上了 · 替人答了 · hook 那头先走了）都在这里写成那一句（[`Settled::end`]），记在会话上
//! （`sessions-needs.ended` · 会话事实 `askEnd`），这条会话下一次挂上请求才换掉；两端照摆，不各自兜底。
//!
//! **那一家答不了的那一种**（[`crate::agents::AskFace::cannot`] 认，比如某一家的提问）不挂：当场不给决定（hook 即刻退、那一家照常在终端里问），
//! 这条会话记一条需手动、原因码 `HookOff::NoAnswer`（[`refused`]；会话事实照它说那一句）；这条会话又挂上请求、或一轮的那几种来了就摘掉。
//!
//! 那一家的字段名不进这里：读 hook 输入 · 写决定都经注册表 `Adapter.asks`（[`crate::agents::AskFace`]）；挂着的那一条带着那一家的答题口，
//! 答的时候照它的 `decide` 写那一整份，这里不拼。

mod answer;
pub mod hook;
pub use answer::answer_needs;

use crate::agents::asks::HookOff;
use crate::agents::asks::{product_of, Ask, Asked, NoteNeed};
use crate::agents::{AskFace, AskIn, PlanMode, Verdict};
use crate::common::cells::Words;
use crate::common::said::Said;
/// 挂上 · 撤掉 · 答掉都报这一声（`history::needs_changed`）。
use crate::history::needs_changed::moved;
pub(crate) use cmd_wire::needs::{
    AnswerPick as Pick, AnswerReq, AnswerResult, Answered, AskEnd, AutoAnswer, AutoChoice,
};
use copy_core::copy_text;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// `needs-ask` 的应答：人的答案写成那一家要的那一整份（hook 原样打出去）；`null` ＝ 不给决定。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct AskReply {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub decision: Option<Value>,
}

/// `needs-ask` 的入参：哪一家（线上的 kind）＋ 那一家的 hook 交来的那一份原样。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct AskReq {
    pub agent: crate::agents::AgentKind,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "unknown"))]
    pub hook: Value,
    /// 经环境来的那一发（换了启动器那一支，子进程也继承得到）里发问那个进程的 pid：常驻照它认是不是 ccm 起的那一个会话
    /// （[`hook::vouched`]）。不是经环境来的 ⇒ 不带。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub asker: Option<u32>,
}

crate::wire::typed::codes! {
    /// `needs-ask` 没有别的码（不是要人答的那一种 ⇒ `{decision: null}`，不是失败）。
    pub enum AskErr {}
}

/// `needs-ask`（帧面硬臂：挂的那条跟着本连接走；本体真异步，`cancel` 打断 ＝ 不给决定）。
pub struct NeedsAsk;

impl crate::wire::typed::Command for NeedsAsk {
    const NAME: &'static str = "needs-ask";
    type Req = AskReq;
    type Reply = AskReply;
    type Err = AskErr;
}

/// 一条挂着的请求。
struct Open {
    /// 那一家的答题口（答的时候照它写决定）。
    face: AskFace,
    ask: AskIn,
    product: Ask,
    /// 何时挂上的（epoch ms，这台的钟）：算已等多久（[`pending`] 交 [`Asked`]）；先后照号，不照它。
    since_ms: u64,
    waits: Waits,
}

/// 这一条在等谁。
enum Waits {
    /// hook 挂着等（不挡终端的那一种）：它那条连接 ＋ 交回那一头（`Some(决定)` ＝ 打出去 · `None` ＝ 空手退）。
    Hook {
        conn: u64,
        tx: tokio::sync::oneshot::Sender<Option<Value>>,
    },
    /// 终端里有框（挡着终端的那一种：hook 交完就走了）：app 里答经读屏按（[`answer_needs_through`]），终端那边答没答只由 [`hook::closed_now`] 判。
    /// `Some` ＝ 盯着这条会话的状态文件；`None` ＝ 那一家没有状态文件（或挂不上），这条会话一轮的那几种来了才收（[`fold_unwatched`]）。
    Terminal {
        _watch: Option<crate::platform::watch_file::Watching>,
    },
}

/// 一条请求怎么收的场（收场那一句只在 [`Settled::end`] 写）。
enum Settled {
    /// 某个出口答上了。
    Answered,
    /// 没人答、替人答了：那一句（「{dur}无人答 · 已自动答：…」）。
    Auto(Words),
    /// hook 那头先走了（终端那边答了 · 被打断 · 到点）。
    Closed,
}

impl Settled {
    /// **收场那一句的唯一一处**：会话事实 `askEnd` · `sessions-needs.ended` · 之后再答回的 `gone` 那一句都照它。
    fn end(self) -> AskEnd {
        match self {
            Settled::Answered => AskEnd {
                said: Words(copy_text("beAsk.answer.already", &[])),
                stays: false,
            },
            Settled::Auto(said) => AskEnd { said, stays: true },
            Settled::Closed => AskEnd {
                said: Words(copy_text("askBlock.gone.elsewhere", &[])),
                stays: false,
            },
        }
    }
}

/// 登记表（进程里一份，[`book`]；判据另起一份照样挂、照样读，不响铃）。
#[derive(Default)]
struct Book {
    /// 上一条挂上的号（下一条 ＝ 它 ＋ 1）。
    last: u64,
    /// 挂着的那几条，键是号的数值（`ask-<n>` 的 n）：存的序就是挂上的先后。
    open: BTreeMap<u64, Open>,
    /// 收了场的那几条（请求 id ⇒ 那一格），之后再答照它回 `gone`。
    settled: VecDeque<(String, AskEnd)>,
    /// 会话 id ⇒ 上一条请求收场的那一格（这条会话下一次挂上请求就摘掉）。
    ended: BTreeMap<String, AskEnd>,
    /// 那一家答不了、没挂上的那一问还在的会话（需手动，码 `NoAnswer`）：又挂上请求 · 一轮的那几种来了就摘掉。
    refused: BTreeSet<String>,
    /// 撤过的连接（[`withdraw_conn`]，最近 [`SETTLED_KEEP`] 条）：处理器过了闸门才挂，读循环可能先读到 EOF、先撤 ——
    ///   撤了之后才来挂的那一条当场收场（[`Book::hang`]），不留着握住那条连接的应答那一路（流读到 EOF 后等它交应答才收场）。
    withdrawn: VecDeque<u64>,
}

/// 第 `n` 条挂上的那一条的号（出口答的时候照它交回）。
fn id_of(n: u64) -> String {
    format!("ask-{n}")
}

/// 号 ⇒ 登记表里的键（不是 [`id_of`] 写出的那一形 ⇒ `None`，当没这一条）。
fn key_of(id: &str) -> Option<u64> {
    let n = id.strip_prefix("ask-")?.parse().ok()?;
    (id_of(n) == id).then_some(n)
}

/// 收了场的记几条（再早的答回 `not_found`）；收场那一格也只留这么多条会话。
const SETTLED_KEEP: usize = 256;

impl Book {
    /// 挂上一条（`face` ＝ 那一家的答题口，`label` ＝ 子运行派出它时的标签，`waits` ＝ 在等谁）：回它的号。
    /// 这条会话上一条请求收场的那一格、记着的需手动跟着摘掉。
    /// 挂它的那条连接已经撤了 ⇒ 不挂、只记它收了场（之后再答回 `gone`；等答的那一头随 `waits` 落下收到撤了）——
    /// 界面从没见过它，这条会话「上一条请求怎么了」那一格不动。
    fn hang(&mut self, face: AskFace, asked: AskIn, label: Option<&str>, waits: Waits) -> String {
        self.last += 1;
        let id = id_of(self.last);
        if matches!(&waits, Waits::Hook { conn, .. } if self.withdrawn.contains(conn)) {
            self.settled.push_back((id.clone(), Settled::Closed.end()));
            while self.settled.len() > SETTLED_KEEP {
                self.settled.pop_front();
            }
            return id;
        }
        let product = product_of(
            &id,
            &asked,
            face.stop_on_deny,
            face.answers_on_screen(&asked),
            label,
        );
        self.ended.remove(&asked.sid);
        self.refused.remove(&asked.sid);
        self.open.insert(
            self.last,
            Open {
                face,
                ask: asked,
                product,
                since_ms: crate::common::time::now_ms(),
                waits,
            },
        );
        id
    }

    /// 收起终端里答掉的那几条（`which` 认哪几条），每条记「别处已答」那一句；回收起的那几条（锁外丢：里头可能有监听）。
    fn fold(&mut self, which: impl Fn(u64, &Open) -> bool) -> Vec<Open> {
        let keys: Vec<u64> = self
            .open
            .iter()
            .filter(|(k, o)| which(**k, o))
            .map(|(k, _)| *k)
            .collect();
        let mut out = Vec::new();
        for k in keys {
            if let Some(o) = self.open.remove(&k) {
                self.settle(o.product.id.clone(), o.ask.sid.clone(), Settled::Closed);
                out.push(o);
            }
        }
        out
    }

    /// 某条会话此刻挂着的请求：界面答得了的（hook 挂着等的）在前、终端里有框的（挡着终端的那一种）在后，各自照挂上的先后
    /// （会话事实只配最前那一条：终端里有框的那一条不盖住后挂上、答得了的那一条）。
    fn pending(&self, sid: &str) -> Vec<Asked> {
        let mut out: Vec<(bool, Asked)> = self
            .open
            .values()
            .filter(|o| o.ask.sid == sid)
            .map(|o| {
                (
                    matches!(o.waits, Waits::Terminal { .. }),
                    Asked {
                        ask: o.product.clone(),
                        since_ms: o.since_ms,
                    },
                )
            })
            .collect();
        out.sort_by_key(|(terminal, _)| *terminal);
        out.into_iter().map(|(_, a)| a).collect()
    }

    /// 收场：那条请求记下那一格、那条会话的「上一条请求怎么了」换成它；回那一格。
    fn settle(&mut self, id: String, sid: String, how: Settled) -> AskEnd {
        let end = how.end();
        self.settled.push_back((id, end.clone()));
        while self.settled.len() > SETTLED_KEEP {
            self.settled.pop_front();
        }
        self.ended.insert(sid, end.clone());
        while self.ended.len() > SETTLED_KEEP {
            self.ended.pop_first();
        }
        end
    }
    fn settled(&self, id: &str) -> Option<&AskEnd> {
        self.settled
            .iter()
            .rev()
            .find(|(i, _)| i == id)
            .map(|(_, e)| e)
    }
}

fn book() -> std::sync::MutexGuard<'static, Book> {
    static BOOK: std::sync::OnceLock<Mutex<Book>> = std::sync::OnceLock::new();
    BOOK.get_or_init(|| {
        crate::history::needs_changed::answer_asked(sids, pending);
        Mutex::default()
    })
    .lock()
    .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 一条连接（hook 那一头）挂的请求跟着它走：它一丢 ⇒ 它挂的全撤掉（入方向读循环每条连接一份）。
pub struct Desk {
    conn: u64,
}

impl Default for Desk {
    fn default() -> Self {
        Self::new()
    }
}

impl Desk {
    pub fn new() -> Desk {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Desk {
            conn: NEXT.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// 这条连接的号（交给 `needs-ask` 的处理器：处理器不拿着 [`Desk`] 本身，否则它丢不掉）。
    pub fn conn(&self) -> u64 {
        self.conn
    }
}

impl Drop for Desk {
    fn drop(&mut self) {
        withdraw_conn(self.conn);
    }
}

/// `needs-ask` 的本体：`{agent, hook}` ⇒ 挂上、等人答 ⇒ `{decision}`（`null` ＝ 不给决定，hook 空手退）。
/// 那一家没有答题口 · hook 输入不是要人答的那一种 ⇒ 立刻 `{decision: null}`（不挂）。
/// 那一家答不了的那一种（[`AskFace::cannot`]）⇒ 不挂、立刻 `{decision: null}`，这条会话记一条需手动（[`refused`]）。
/// `setting` ＝ 这条会话「无人答时自动答」那一格（注解里那一份，调用方现读交进来）。
pub async fn ask(
    conn: u64,
    req: AskReq,
    setting: impl FnOnce(&str) -> Option<AutoAnswer>,
) -> AskReply {
    ask_among(
        crate::agents::REGISTRY,
        conn,
        req,
        setting,
        crate::history::live_table::states_in,
    )
    .await
}

/// 同 [`ask`]，那一家从 `registry` 里找、会话各进程的态照 `states` 读（判据拿替身家跑）。
pub async fn ask_among(
    registry: &'static [crate::agents::Adapter],
    conn: u64,
    req: AskReq,
    setting: impl FnOnce(&str) -> Option<AutoAnswer>,
    states: hook::StatesOf,
) -> AskReply {
    let Some(face) = crate::agents::ask_face_among(registry, &req.agent) else {
        return AskReply { decision: None };
    };
    // 同 hook 客户端那一道的先后（[`hook::route`]）：要人答的那一种挂上（挡着终端的记成终端里有框）；那一家答不了的不挂、记需手动；
    // 一轮的那几种记进活会话表（会话在干什么先照它）。除了挂上等的那一种，当场不给决定。
    let route = hook::route(&face, &req.hook);
    // 经环境来的那一发：认不出是 ccm 起的那一个会话 ⇒ 不挂、不记（[`hook::vouched`]，托管名单的家装在这一侧）。
    if req
        .asker
        .is_some_and(|pid| !hook::vouched(&face, &route, pid))
    {
        return AskReply { decision: None };
    }
    match route {
        hook::Route::Hang(asked) if asked.blocks_terminal => {
            note_terminal(registry, &req.agent, face, *asked, states);
            AskReply { decision: None }
        }
        hook::Route::Hang(asked) => {
            let auto = auto_of(&asked, setting(&asked.sid).as_ref());
            wait_answer(face, *asked, conn, auto).await
        }
        hook::Route::Refused(sid) => {
            book().refused.insert(sid.clone());
            moved(&sid);
            AskReply { decision: None }
        }
        hook::Route::Heard(sid, event) => {
            unrefuse(&sid);
            fold_unwatched(&sid);
            crate::history::live_table::hear(&sid, event);
            AskReply { decision: None }
        }
        hook::Route::Skip => AskReply { decision: None },
    }
}

/// 挂上、等人答（或到点替人答）⇒ 那一家要的那一份。
async fn wait_answer(face: AskFace, asked: AskIn, conn: u64, auto: Option<AutoAnswer>) -> AskReply {
    let (id, mut rx) = open_id(face, asked, conn);
    // 撤掉（连接断了 / 别处关了）⇒ 发端丢了 ⇒ 不给决定。
    let decision = match auto {
        None => rx.await.ok().flatten(),
        Some(auto) => tokio::select! {
            r = &mut rx => r.ok().flatten(),
            () = due(auto.after_min) => {
                match answer_for_nobody(&id, &auto) {
                    Some(d) => Some(d),
                    // 刚好有人先答了 / 撤了：照那一头交来的。
                    None => rx.await.ok().flatten(),
                }
            }
        },
    };
    AskReply { decision }
}

/// 挂着的这一条自己带的期限（等满 `min` 分钟醒一次）。
#[expect(
    clippy::disallowed_methods,
    reason = "会话设了「无人答时自动答」的那一条提问自己带的一次性期限，不是节拍：随这一条收场一起没，那一格关着就不起"
)]
async fn due(min: u32) {
    tokio::time::sleep(std::time::Duration::from_secs(u64::from(min) * 60)).await;
}

/// 子运行发的那一条：派出它时的标签（会话记录照 sid 在这台的记录树里找，[`crate::agents::asks::run_label`]）；主会话发的 · 找不到 ⇒ `None`。
fn label_of(asked: &AskIn) -> Option<String> {
    let run = asked.agent_id.as_deref()?;
    let at = crate::agents::find_session_in(&crate::agents::local_homes(), &asked.sid).ok()?;
    crate::agents::asks::run_label(&at.file, run)
}

/// 挂上一条（`face` ＝ 那一家的答题口，`conn` ＝ 它那条连接）：回它的号与等答的那一头。
pub fn open_id(
    face: AskFace,
    asked: AskIn,
    conn: u64,
) -> (String, tokio::sync::oneshot::Receiver<Option<Value>>) {
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<Value>>();
    let id = hang(face, asked, Waits::Hook { conn, tx });
    (id, rx)
}

/// 挂进登记表、敲这条会话的铃：回它的号。
fn hang(face: AskFace, asked: AskIn, waits: Waits) -> String {
    let label = label_of(&asked);
    let sid = asked.sid.clone();
    let id = book().hang(face, asked, label.as_deref(), waits);
    moved(&sid);
    id
}

/// 挡着终端的那一种：记成「终端里有框」（不跟 hook 那条连接走；app 里答经读屏按），盯这条会话的状态文件等终端那边答掉。
/// 盯的是这条会话自己那一家的（[`hook::status_home`]：号的 pidfile 目录不链到常驻自己那一家的也认得）。
/// 那一家没有状态文件 · 各号的家里都找不到这条会话的进程 · 挂不上监听 ⇒ 不盯，这条会话一轮的那几种来了才收（[`fold_unwatched`]）。
fn note_terminal(
    registry: &[crate::agents::Adapter],
    agent: &str,
    face: AskFace,
    asked: AskIn,
    states: hook::StatesOf,
) {
    let label = label_of(&asked);
    let sid = asked.sid.clone();
    let started = crate::common::time::now_ms();
    // 这条会话自己那一家的家（不是常驻自己环境里那一家的）；在拿登记表那把锁之前找（读活会话表要问登记表）。
    let home = hook::status_home(registry, agent, &sid, states);
    {
        // 挂上与把监听存进那一条在同一把锁里：监听那一头头一次判要等这把锁，看到的一定是存好了监听的那一条
        // （不会在这中间被一轮的那几种当成没盯的收掉）。
        let mut b = book();
        let id = b.hang(
            face,
            asked,
            label.as_deref(),
            Waits::Terminal { _watch: None },
        );
        let w = home.and_then(|h| watch_terminal(&h, &sid, &id, started, states));
        if let Some(o) = key_of(&id).and_then(|k| b.open.get_mut(&k)) {
            o.waits = Waits::Terminal { _watch: w };
        }
    }
    moved(&sid);
}

/// 盯这条会话的状态文件（它自己那一家 `home` 的 pidfile 目录，[`hook::watch_closed`]），判出终端那边答了就收起 `id` 那一条。
/// `started` ＝ 记下它的那一刻。挂不上 ⇒ `None`。
fn watch_terminal(
    home: &hook::SessionHome,
    sid: &str,
    id: &str,
    started: u64,
    states: hook::StatesOf,
) -> Option<crate::platform::watch_file::Watching> {
    let (sid_, id) = (sid.to_string(), id.to_string());
    hook::watch_closed(
        home,
        sid,
        started,
        states,
        "ask-terminal-status",
        move || {
            let key = key_of(&id);
            let folded = book().fold(|k, _| Some(k) == key);
            if !folded.is_empty() {
                moved(&sid_);
            }
        },
    )
}

/// 这条会话一轮的那几种来了：没盯状态文件的那几条挡着终端的（判不了框在不在）当终端那边答掉了、收起（那一问已经过去了）。
fn fold_unwatched(sid: &str) {
    let folded =
        book().fold(|_, o| o.ask.sid == sid && matches!(o.waits, Waits::Terminal { _watch: None }));
    if !folded.is_empty() {
        moved(sid);
    }
}

/// 替人答的那一句的缺省（交给那一家的话）。
pub fn auto_say() -> String {
    copy_text("beAsk.auto.say", &[])
}

/// 「无人答时自动答」可选的几档（分钟；`0` ＝ 关）。上限同注解那一侧的值域（`history::history_annotations::AUTO_MAX_MIN`）。
const AUTO_CHOICES: [u32; 4] = [0, 5, 15, 30];

/// 自动答可选的那几档 —— **唯一一处**：几档 ＋ 此刻那一格（`now`；关 ⇒ `None`）不在几档里 ⇒ 多列一项，按分钟排好；字都写好。
pub fn auto_choices(now: Option<u32>) -> Vec<AutoChoice> {
    let mut mins = AUTO_CHOICES.to_vec();
    if let Some(m) = now.filter(|m| !mins.contains(m)) {
        mins.push(m);
        mins.sort_unstable();
    }
    mins.into_iter()
        .map(|min| AutoChoice {
            min,
            text: Words(if min == 0 {
                copy_text("beAsk.auto.off", &[])
            } else {
                copy_core::short_duration(u64::from(min) * 60_000)
            }),
        })
        .collect()
}

/// 这台挂上的这一条要不要、多久后替人答 —— **唯一的判定**：只有提问那一种；批准工具 · 批计划永远不（安全闸）。
/// 那一格的值域在注解那一侧（`history::history_annotations::auto_checked`）。
pub(crate) fn auto_of(a: &AskIn, setting: Option<&AutoAnswer>) -> Option<AutoAnswer> {
    if a.kind != crate::agents::AskKind::Answer {
        return None;
    }
    setting.filter(|s| s.after_min > 0).cloned()
}

/// 等满了：还挂着 ⇒ 每一题答成「其他」＋ 那一句（没读出题的那一种 ⇒ 不回答、附那一句），收场、留记录 ⇒ 那一家要的那一份；
/// 已经不在（有人先答了 · 撤了）/ 那一家写不出 ⇒ `None`。
fn answer_for_nobody(id: &str, auto: &AutoAnswer) -> Option<Value> {
    let say = auto.say.clone().unwrap_or_else(auto_say);
    let mut b = book();
    let key = key_of(id)?;
    let o = b.open.get(&key)?;
    let verdict = if o.product.questions.is_empty() {
        Verdict::Deny {
            note: Some(say.clone()),
            stop: false,
        }
    } else {
        Verdict::Allow {
            rule: None,
            mode: None,
            answers: o
                .product
                .questions
                .iter()
                .map(|q| (q.text.clone(), say.clone()))
                .collect(),
        }
    };
    let decision = (o.face.decide)(&o.ask, &verdict)?;
    let o = b.open.remove(&key).expect("刚找到的那一条");
    let sid = o.ask.sid;
    let said = Words(copy_text(
        "beAsk.auto.answered",
        &[
            (
                "dur",
                &copy_core::format_duration(u64::from(auto.after_min) * 60_000),
            ),
            ("say", &say),
        ],
    ));
    b.settle(id.to_string(), sid.clone(), Settled::Auto(said));
    drop(b);
    moved(&sid);
    Some(decision)
}

/// 这条会话记着的需手动（那一家答不了、没挂上的那一问）⇒ 它的原因码；没有 ⇒ `None`。
pub fn refused(sid: &str) -> Option<HookOff> {
    book().refused.contains(sid).then_some(HookOff::NoAnswer)
}

/// 这条会话往下走了（一轮的那几种来了）⇒ 摘掉记着的那一条需手动。
fn unrefuse(sid: &str) {
    let was = book().refused.remove(sid);
    if was {
        moved(sid);
    }
}

/// 这条会话上一条请求收场的那一格（会话事实 `askEnd`；下一次挂上请求前一直在）；还没收过场 ⇒ `None`。
pub fn ended(sid: &str) -> Option<AskEnd> {
    book().ended.get(sid).cloned()
}

/// 收了场的那几条（`sessions-needs.ended`），按会话 id 排。
pub fn ended_rows() -> Vec<cmd_wire::needs::AskEndRow> {
    book()
        .ended
        .iter()
        .map(|(sid, end)| cmd_wire::needs::AskEndRow {
            sid: sid.clone(),
            end: end.clone(),
        })
        .collect()
}

/// 这条连接挂的全撤掉（之后再答 ⇒ `gone`）。
/// 记下它撤过：之后才来挂的当场收场（[`Book::hang`]）。
pub(crate) fn withdraw_conn(conn: u64) {
    let gone = {
        let mut b = book();
        b.withdrawn.push_back(conn);
        while b.withdrawn.len() > SETTLED_KEEP {
            b.withdrawn.pop_front();
        }
        b.fold(|_, o| matches!(o.waits, Waits::Hook { conn: c, .. } if c == conn))
    };
    for o in gone {
        moved(&o.ask.sid);
    }
}

/// 某条会话此刻挂着的请求（界面答得了的在前，各自照挂上的先后；见 `Book::pending`）。
pub fn pending(sid: &str) -> Vec<Asked> {
    book().pending(sid)
}

/// 此刻有请求挂着的会话。
pub(crate) fn sids() -> Vec<String> {
    let mut v: Vec<String> = book().open.values().map(|o| o.ask.sid.clone()).collect();
    v.sort();
    v.dedup();
    v
}

/// 选的那一项 ⇒ 通用答案。闭集：approve 那几项 `allow` · `always-<i>` · `deny` · `deny-note` · `deny-stop`；
/// plan `plan-auto` · `plan-edits` · `plan-manual` · `plan-keep`；question `answer` · `dismiss`（挡着终端的那一种不经这里：[`answer_needs_through`] 只认 `allow` · `deny`）。
pub(crate) fn verdict_of(
    a: &AskIn,
    product: &Ask,
    choice: &str,
    note: Option<&str>,
    picks: Option<&BTreeMap<String, Pick>>,
) -> Result<Verdict, Said> {
    let spec = product
        .choices
        .iter()
        .find(|c| c.id == choice)
        .ok_or_else(|| Said::new("bad_choice", copy_text("beAsk.answer.badChoice", &[])))?;
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if spec.note == NoteNeed::Required && note.is_none() {
        return Err(Said::new(
            "note_required",
            copy_text("beAsk.answer.noteRequired", &[]),
        ));
    }
    let allow = |rule, mode| Verdict::Allow {
        rule,
        mode,
        answers: Vec::new(),
    };
    Ok(match choice {
        "allow" => allow(None, None),
        "deny" | "dismiss" => Verdict::Deny {
            note: None,
            stop: false,
        },
        "deny-note" => Verdict::Deny {
            note: note.map(str::to_string),
            stop: false,
        },
        "deny-stop" => Verdict::Deny {
            note: None,
            stop: true,
        },
        "plan-auto" => allow(None, Some(PlanMode::Auto)),
        "plan-edits" => allow(None, Some(PlanMode::AcceptEdits)),
        "plan-manual" => allow(None, Some(PlanMode::Manual)),
        "plan-keep" => Verdict::Deny {
            note: Some(note.map_or_else(|| copy_text("beAsk.planKeep.note", &[]), str::to_string)),
            stop: false,
        },
        "answer" => Verdict::Allow {
            rule: None,
            mode: None,
            answers: answers_of(product, picks)?,
        },
        c => match c
            .strip_prefix("always-")
            .and_then(|i| i.parse::<usize>().ok())
        {
            Some(i) if i < a.suggestions.len() => allow(Some(i), None),
            _ => {
                return Err(Said::new(
                    "bad_choice",
                    copy_text("beAsk.answer.badChoice", &[]),
                ))
            }
        },
    })
}

/// `picks` ＝ `{<题 id>: {options: [<选项 id>…], text: <自由文本>}}`；每题都要有答案，单选至多一项；
/// 多选几项用「, 」连（那一家框里多选的写法），自由文本接在后面。
fn answers_of(
    product: &Ask,
    picks: Option<&BTreeMap<String, Pick>>,
) -> Result<Vec<(String, String)>, Said> {
    let bad = || Said::new("bad_args", copy_text("beAsk.answer.picks", &[]));
    let picks = picks.ok_or_else(bad)?;
    let mut out = Vec::new();
    for q in &product.questions {
        let p = picks.get(&q.id).ok_or_else(bad)?;
        let mut parts: Vec<String> = Vec::new();
        for o in &p.options {
            let opt = q.options.iter().find(|x| x.id == *o).ok_or_else(bad)?;
            parts.push(opt.text.clone());
        }
        if let Some(t) = p.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            parts.push(t.to_string());
        }
        if parts.is_empty() || (!q.multi && parts.len() > 1) {
            return Err(bad());
        }
        out.push((q.text.clone(), parts.join(", ")));
    }
    Ok(out)
}

crate::wire::typed::codes! {
    /// `needs-answer` 的码。
    pub enum AnswerErr {
        /// 没这一条（或不是这个会话的）。
        NotFound = "not_found",
        /// 选项不在这一条的选项里 / 那一家写不出这个决定。
        BadChoice = "bad_choice",
        /// 选项要附一句话，没给。
        NoteRequired = "note_required",
        /// 挡着终端的那一条：终端里没认出这一条的框（或终端不收），一个键没按。
        NotSeen = "not_seen",
        /// 挡着终端的那一条：按了，框是否收起未能确认。
        Unsure = "unsure",
        /// 挡着终端的那一条：同一条会话挂着两条相同的（同 agent · 同工具 · 同命令），屏上分不出，一个键没按。
        Twin = "twin",
        /// 挡着终端的那一条：终端里排着的比登记表记下的多（后端重启前挂上的那几条），分不出正显示的是哪一条，一个键没按。
        UnknownQueue = "unknown_queue",
    }
}

/// `needs-answer`：答一条挂着的待答请求，第一个答案落下。hook 挂着等的那一种：锁一下登记表、交给 hook 那一头的通道；
/// 挡着终端的那一种：经读屏驱动在终端里按（取屏 · 按键 · 等重画），在阻塞线程池里做 ⇒ 异步档。
pub struct NeedsAnswerCmd;

impl crate::wire::typed::Command for NeedsAnswerCmd {
    crate::wire::typed::by_call!(AnswerReq);
    type Err = AnswerErr;
}

/// 别处已答那一种结局 ＋ 那一句。
fn gone(said: Words) -> Answered {
    Answered {
        result: AnswerResult::Gone,
        said: Some(said),
    }
}

/// 锁着登记表答一下 ⇒ `{result: "answered"}`；别处先答了（别的出口 · 替人答过）· hook 那头已经走了（终端那边答了 · 被打断）
/// ⇒ `{result: "gone", said}`（不是错：这一下没答进去，也不用再答）；没这一条 ⇒ `not_found`；挡着终端的那一条 ⇒ 去终端里按哪一下。
fn needs_step(req: &AnswerReq) -> Result<Step, Said> {
    let (sid, id, choice) = (req.sid.as_str(), req.ask.as_str(), req.choice.as_str());
    let mut b = book();
    let key = key_of(id);
    let Some(o) = key
        .and_then(|k| b.open.get(&k))
        .filter(|o| o.ask.sid == sid)
    else {
        return match b.settled(id) {
            Some(end) => Ok(Step::Done(gone(end.said.clone()))),
            None => Err(Said::new(
                "not_found",
                copy_text("beAsk.answer.notFound", &[]),
            )),
        };
    };
    if matches!(o.waits, Waits::Terminal { .. }) {
        let press = on_screen(&b, o, choice)?;
        drop(b);
        return Ok(Step::Screen(press));
    }
    let verdict = verdict_of(
        &o.ask,
        &o.product,
        choice,
        req.note.as_deref(),
        req.picks.as_ref(),
    )?;
    let Some(decision) = (o.face.decide)(&o.ask, &verdict) else {
        return Err(bad_choice());
    };
    let o = key.and_then(|k| b.open.remove(&k)).expect("刚找到的那一条");
    let delivered = match o.waits {
        Waits::Hook { tx, .. } => tx.send(Some(decision)).is_ok(),
        // 挡着终端的那一种在上面就交给读屏那一路了，走不到这里。
        Waits::Terminal { .. } => false,
    };
    let how = if delivered {
        Settled::Answered
    } else {
        Settled::Closed
    };
    let end = b.settle(id.to_string(), o.ask.sid, how);
    drop(b);
    moved(sid);
    Ok(Step::Done(if delivered {
        answered()
    } else {
        gone(end.said)
    }))
}

fn answered() -> Answered {
    Answered {
        result: AnswerResult::Answered,
        said: None,
    }
}

fn bad_choice() -> Said {
    Said::new("bad_choice", copy_text("beAsk.answer.badChoice", &[]))
}

/// 答一下走到哪：已有结局 · 要去终端里按那个框（锁外按）。
enum Step {
    Done(Answered),
    Screen(Press),
}

/// 去终端里按的那一下要的：哪一条（号 · 会话）· 那一家的认框 · 那一条请求 · 按哪一下。
struct Press {
    id: String,
    sid: String,
    face: crate::agents::OnScreen,
    ask: AskIn,
    key: crate::agents::Keystroke,
    /// 登记表里这条会话此刻挂着几条（哪一种都算：主会话那条 hook 挂着等的也画在同一个框队列里）。
    known: usize,
}

/// 挡着终端的那一条 ⇒ 去终端里按哪一下。只认「允许 · 拒绝」两项（附话 · 停下这一轮终端里按不出来）；
/// 那一家没有认框 · 不认这一种 ⇒ `bad_choice`。同一条会话还挂着一条同 agent · 同工具 · 同命令的 ⇒ 屏上分不出是哪一条，
/// 一个键都不按（`twin`）。
fn on_screen(b: &Book, o: &Open, choice: &str) -> Result<Press, Said> {
    let face = o
        .face
        .on_screen
        .filter(|_| o.face.answers_on_screen(&o.ask))
        .ok_or_else(bad_choice)?;
    let key = match choice {
        "allow" => face.allow,
        "deny" => face.deny,
        _ => return Err(bad_choice()),
    };
    let alike = b
        .open
        .values()
        .filter(|x| {
            matches!(x.waits, Waits::Terminal { .. })
                && x.ask.sid == o.ask.sid
                && x.ask.agent_type == o.ask.agent_type
                && x.ask.tool == o.ask.tool
                && x.ask.detail == o.ask.detail
        })
        .count();
    if alike > 1 {
        return Err(Said::new("twin", copy_text("beAsk.screen.twin", &[])));
    }
    Ok(Press {
        id: o.product.id.clone(),
        sid: o.ask.sid.clone(),
        face,
        ask: o.ask.clone(),
        key,
        known: b.open.values().filter(|x| x.ask.sid == o.ask.sid).count(),
    })
}

/// 同一时刻只一路在终端里按（两个出口同时答同一个框：后一路等前一路按完、再查那一条还在不在）。
fn screen_turn() -> std::sync::MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 同 [`answer_needs`]，挡着终端的那一条交 `drive` 去按（入参：会话 · 认框 · 按哪一下；判据交替身屏幕）。
/// 认准是这一条的框才按，按完复核框没了才回「已答」、那一张收起；认不出 · 终端不收 ⇒ `not_seen`（一个键没按）；
/// 按了框还在 · 判不了 ⇒ `unsure`；两条分不出 ⇒ `twin`。不看这条会话在不在等人（这一路恰恰在等人时才按）。
pub fn answer_needs_through(
    req: AnswerReq,
    drive: impl FnOnce(
        &str,
        &dyn Fn(&[String]) -> bool,
        &dyn Fn(&[String]) -> bool,
        crate::agents::Keystroke,
    ) -> Result<crate::control::screen_drive::OnBox, Said>,
) -> Result<Answered, Said> {
    use crate::control::screen_drive::OnBox;
    if let Step::Done(a) = needs_step(&req)? {
        return Ok(a);
    }
    let _turn = screen_turn();
    // 等这一把的时候别的出口可能已经按掉了：那一条不在了 ⇒ 照它收场的那一句回；还在 ⇒ 照此刻的登记表重认一遍。
    let p = match needs_step(&req)? {
        Step::Done(a) => return Ok(a),
        Step::Screen(p) => p,
    };
    let seen = |l: &[String]| (p.face.seen)(l, &p.ask);
    // 屏上说排着的比登记表里这条会话挂着的多 ⇒ 正显示的可能是认不得的那一条（后端重启前挂上的）。窄窗不画条数 ⇒ 只照登记表（twin）判。
    let doubt = |l: &[String]| (p.face.queued)(l).is_some_and(|n| n as usize > p.known);
    match drive(&p.sid, &seen, &doubt, p.key)? {
        OnBox::Closed(said) => Err(Said::new("not_seen", said)),
        OnBox::Doubt => Err(Said::new(
            "unknown_queue",
            copy_text("beAsk.screen.unknownQueue", &[]),
        )),
        OnBox::NotSeen => Err(Said::new(
            "not_seen",
            copy_text("beAsk.screen.notSeen", &[]),
        )),
        OnBox::Stays => Err(Said::new("unsure", copy_text("beAsk.screen.unsure", &[]))),
        OnBox::Gone => {
            pressed_gone(&p);
            Ok(answered())
        }
    }
}

/// 按完框没了：还挂着 ⇒ 摘下、记「已答」；盯状态文件那一侧先收了（记成别处已答）、这条会话又没挂上新的 ⇒ 那一格改记「已答」。
/// 摘下的那一条锁外丢（里头可能有监听）。
fn pressed_gone(p: &Press) {
    let mut b = book();
    let taken = key_of(&p.id).and_then(|k| b.open.remove(&k));
    let fresh = b.open.values().any(|o| o.ask.sid == p.sid);
    if taken.is_some() || !fresh {
        b.settle(p.id.clone(), p.sid.clone(), Settled::Answered);
    }
    drop(b);
    drop(taken);
    moved(&p.sid);
}

#[cfg(test)]
#[path = "../../../../tests/backend/control/asks_book_tests.rs"]
mod book_tests;
