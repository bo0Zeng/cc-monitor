//! ★ 后端的一判：叫策略来判，照它交的先后做动作。后端没有任何换号的想法 ——
//! 上限、软硬、启用延迟、耗尽后启用都在策略里（策略调积木库 `src/shared/rotation/lib.rhai`）。
//! 后端只认事实上发不发得出去（[`standing::sendable`]）、用户拖出来的顺序、停发、出错兜底。
//! 纯函数：不读钟、不写盘；真用（中转发之前）与推演调的都是 [`turn`]。

use super::standing;
use super::Acct;
use serde_json::Value;

/// 某号某窗读数到 n 再来问（按读数叫醒）。
#[derive(Debug, Clone, PartialEq)]
pub struct WakeWhen {
    pub(crate) acct: String,
    /// 0 ＝ 5 小时 · 1 ＝ 7 天。
    pub(crate) w: usize,
    pub(crate) used_at_least: f64,
}

/// 策略这一判交出的。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Verdict {
    /// 此刻愿意用的号，先后即想用的先后；不在里面 ＝ 此刻不用。
    pub(crate) order: Vec<String>,
    /// 每个号一句为什么。
    pub(crate) why: Vec<(String, String)>,
    /// `order` 空时停到几点（不给 ⇒ 停到最早有号事实上发得出去的时刻）。
    pub(crate) hold_until: Option<u64>,
    /// 几点再来问（按时刻叫醒）。
    pub(crate) wake_at: Option<u64>,
    /// 读数到哪再来问（按读数叫醒）。
    pub(crate) wake_when: Vec<WakeWhen>,
    /// 策略要跨判记的小东西（后端原样存回、下一判原样交回）。
    pub(crate) memo: Option<Value>,
    /// 要开 5 小时窗的号（后端照 [`warmable`] 收、交给开窗那一处发一句）。
    pub(crate) warm: Vec<String>,
}

/// 策略那一侧（沙箱里的脚本，或判据里的假策略）。
pub trait Strategy {
    /// 出错 · 超时 · 形状不对 ⇒ `Err`（这一判走兜底）。
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, super::super::strategy::Fail>;
    /// 声明了「结论会随时间变」：没约时刻时后端每 [`FALLBACK_STEP`] 来问一次。
    fn time_varying(&self) -> bool {
        false
    }
    /// 这一趟推演给策略的耗时用完了（推演停在那里）。
    fn exhausted(&self) -> bool {
        false
    }
}

/// 交给策略的那一份（算法 §2）。
#[derive(Debug, Clone)]
pub struct Ctx<'a> {
    pub(crate) now: u64,
    pub(crate) clock: u16,
    pub(crate) start: &'a str,
    pub(crate) current: &'a str,
    pub(crate) settings: &'a Value,
    pub(crate) memo: Option<&'a Value>,
    pub(crate) accounts: Vec<CtxAcct>,
    /// 本会话量出来的（[`Me`]）。
    pub(crate) me: &'a Me,
    /// 在用的号是用户手动选的（「现在用」· 拖到第一格 ⇒ 它排在拖出来的顺序最前）。
    pub(crate) pinned: bool,
    /// 用户拖出来的「接下来先用谁」。
    pub(crate) temp: &'a [String],
}

/// 本会话此刻量出来的（交给策略的 `session`；量不到的空着）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Me {
    /// 用速：折合输入 token / 分（最近半小时的平均）。
    pub(crate) pace: Option<f64>,
    /// 短窗用速：同一口径，最近 5 分钟（会话记满 2 分钟才给）。
    pub(crate) pace_short: Option<f64>,
    /// 在途：最近 5 分钟平均几发同时在途（子 agent · 工作流的请求带本会话标识，一并算）。
    pub(crate) inflight: Option<f64>,
    /// 最近 6 小时每 10 分钟用了多少（折合输入 token），36 格、旧的在前；会话记到之前的格空着。
    pub(crate) bins: Vec<Option<f64>>,
    /// 起伏：一段一段比的 σ/μ。
    pub(crate) spread: Option<f64>,
    /// 上下文：最近一发的 输入 ＋ 写缓存 ＋ 读缓存（token）。
    pub(crate) context: Option<u64>,
    /// 写缓存比读缓存多的倍数（写缓存那一档的）。
    pub(crate) write_extra: Option<f64>,
    /// 缓存留多久（秒，同一档）。
    pub(crate) ttl: Option<u64>,
    /// 上下文每分钟长多少（token / 分）。
    pub(crate) grow: Option<f64>,
    /// 本会话在各号上的缓存（还没过期的；照号名排）。
    pub(crate) cached: Vec<PromptCache>,
}

/// 本会话在一个号上的缓存：留到几点 · 当时的上下文多长。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PromptCache {
    pub(crate) account: String,
    pub(crate) until: u64,
    pub(crate) tokens: u64,
}

/// ctx 里的一个号（只有事实）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CtxAcct {
    /// 这个号是哪一家的号（同 [`Acct::agent`]；ctx 里只有会话那一家的号）。
    pub(crate) agent: String,
    pub(crate) id: String,
    pub(crate) pos: usize,
    pub(crate) api: bool,
    pub(crate) sendable: bool,
    pub(crate) used: [f64; 2],
    pub(crate) resets: [Option<u64>; 2],
    pub(crate) refused_until: Option<u64>,
    pub(crate) rate: [Option<f64>; 2],
    pub(crate) per_pct: [Option<u64>; 2],
    /// 有周上限（读数里有周用量那一格）。
    pub(crate) weekly: bool,
    /// 这个会话先不用它（界面上去掉的勾）。
    pub(crate) off: bool,
    /// 别的会话压在它 5 小时窗上的用速（点 / 分）；说不出 ⇒ 空。
    pub(crate) others: Option<f64>,
    /// 换窗时额度账记下的那两样（周重置时剩几点 · 开窗取整往哪边）。
    pub(crate) past: super::Past,
}

/// 会话状态 Σ（后端存 ＋ 本会话量出来的）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sigma {
    /// 会话是哪一家的（路由名）：只在这一家的号里判（号按（家, 号名）认，[`Acct::agent`]）。
    pub agent: String,
    /// 开会话用的号。
    pub start: String,
    pub current: String,
    /// 用户拖出来的「接下来先用谁」。
    pub temp: Vec<String>,
    /// 这个会话先不用的号（当用不了）。
    pub off: Vec<String>,
    /// 上一判策略约的读数（记账那一下比对）。
    pub wake_when: Vec<WakeWhen>,
    pub memo: Option<Value>,
    /// 本会话量出来的（不存，判的时候现量）。
    pub me: Me,
}

/// 结论。
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    Use(String),
    /// 停发到 `until`；`account` 是到时最早恢复的那个号（说得出才有）。
    Hold {
        until: Option<u64>,
        account: Option<String>,
    },
}

/// 一判的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub pick: Pick,
    pub sigma: Sigma,
    /// 这一判丢掉的拖出来的条目（轮到时用不了 ⇒ 跳过）。
    pub skipped: Vec<String>,
    /// 策略交的先后（显示用）与每号一句。
    pub order: Vec<String>,
    pub why: Vec<(String, String)>,
    /// 这一号来自用户拖出来的顺序。
    pub from_temp: bool,
    /// 下一个已知节点（重置 · 被拒到期 · 挡着的到期 · 策略约的时刻 · 停发到期）。
    pub tn: Option<u64>,
    /// 策略约的时刻（原样存进会话，推演与下一判从它接着）。
    pub(crate) wake_at: Option<u64>,
    pub strategy_error: Option<super::super::strategy::Fail>,
    /// 策略要开窗、后端收下的号（[`warmable`]）；没收的另列（只记一句，不发）。
    pub(crate) warm: Vec<String>,
    pub(crate) warm_dropped: Vec<String>,
}

/// 策略声明了结论随时间变、又没约时刻 ⇒ 每隔这么久来问一次。
pub(crate) const FALLBACK_STEP: u64 = 15 * 60;

/// ★ `agent` 那一家号名 `a` 的那一行在事实里第几个（号按（家, 号名）认的唯一一处：两家可以有同名的号）。
/// 判 · 推演 · 中转那一侧 · 出口找号都照它（[`fact`] · [`fact_mut`]）。
pub fn fact_at(f: &[Acct], agent: &str, a: &str) -> Option<usize> {
    f.iter().position(|x| x.agent == agent && x.id == a)
}

/// `agent` 那一家的号 `a`（[`fact_at`]）。
pub fn fact<'a>(f: &'a [Acct], agent: &str, a: &str) -> Option<&'a Acct> {
    fact_at(f, agent, a).map(|i| &f[i])
}

/// [`fact`] 那一行，改得动。
pub fn fact_mut<'a>(f: &'a mut [Acct], agent: &str, a: &str) -> Option<&'a mut Acct> {
    fact_at(f, agent, a).map(move |i| &mut f[i])
}

/// 号表那一侧：号表里一个号是哪一家（路由名，同会话那一条与事实的 `agent`）：清单那一格（缺 ⇒ 缺省那一家，
/// [`crate::agents::account_kind_among`]）那一家起会话那一面的路由名；说不出 ⇒ `None`。注册表照参数交（判据交替身家）。
pub fn family_among(
    reg: &[crate::agents::Adapter],
    m: &acct_core::ManifestAccount,
) -> Option<&'static str> {
    let kind = crate::agents::account_kind_among(reg, m.agent.as_deref())?;
    crate::agents::launch_face_among(reg, kind).map(|f| f.adapter_id)
}

/// ★ 号表那一侧的（家, 号名）找号（唯一一处；探测 · 换号那一侧都照它）：`agent` 那一家号名 `id` 的那一条。
/// 账号 0（不设号目录 ＝ 每一家各自本来那一份登录）哪一家都算（同账号清单那一条）。
pub fn in_family_among<'a>(
    reg: &[crate::agents::Adapter],
    lib: &'a acct_core::AccountBook,
    agent: &str,
    id: &str,
) -> Option<&'a acct_core::ManifestAccount> {
    lib.accounts.iter().find(|m| {
        (m.config_dir.is_none() || family_among(reg, m) == Some(agent))
            && m.route_id().as_deref() == Some(id)
    })
}

/// `agent` 那一家的号（照事实的先后）：一个会话只在这几个里判。
pub fn mine<'a>(f: &'a [Acct], agent: &'a str) -> impl Iterator<Item = &'a Acct> + Clone + 'a {
    f.iter().filter(move |x| x.agent == agent)
}

/// 策略交的先后里头一个只属别家的号（本家没有这个号名、别家有）。
fn foreign<'a>(order: &'a [String], f: &[Acct], agent: &str) -> Option<&'a String> {
    order
        .iter()
        .find(|a| fact(f, agent, a).is_none() && f.iter().any(|x| x.id == **a))
}

/// 开窗那一判为什么这样（码；那一句由出口照码写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmWhy {
    /// 按量号：没有 5 小时窗可开。
    Api,
    /// 被拒着。
    Refused,
    /// 要重新登录（续令牌一时没成也算，那时带几点再试）。
    Login,
    /// 缺 key。
    Key,
    /// 那一家不归我方续的账号 0、令牌快过期 / 过期了（等那一家自己续）。
    BaseUntouched,
    /// 在超额上兜着。
    Overage,
    /// 用满了：0 ＝ 5 小时 · 1 ＝ 7 天。
    Full(usize),
    /// 5 小时窗在计时。
    Ticking,
    /// 5 小时窗没在计时（没见过 · 已过重置时刻；事实里两者都没有重置时刻，那一句由出口照显示态挑）。
    NoWindow,
}

/// 开窗那一判：发一句 · 等（`until` ＝ 几点又该看；说不出 ⇒ 空，出口自己定过多久再看）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmVerdict {
    Send(WarmWhy),
    Wait { until: Option<u64>, why: WarmWhy },
}

/// ★ 一个号此刻要不要发一句让 5 小时窗开始计时（唯一一处：策略交的 `warm` 照它收，`quota-read` 每号的 `warm` 照它写）。
/// 只发这样的号：订阅号 · 此刻发得出去 · 5 小时窗没在计时（没见过重置时刻 · 重置时刻已过）。
/// 号的 5 小时窗由第一发触发：在计时的窗不会因多发一句提前或推后，发了白花。
/// 发不出去的等到它又发得出去（[`standing::sendable_again`]）。
pub fn warm_verdict(x: &Acct, t: u64) -> WarmVerdict {
    use super::super::facts::FactWhy;
    if !standing::sendable(x, t) {
        let barred = x
            .barred
            .filter(|b| b.until.is_none_or(|u| u > t))
            .map(|b| b.why);
        let why = if x.refused_until.is_some_and(|u| u > t) {
            WarmWhy::Refused
        } else {
            match barred {
                Some(FactWhy::Login) => WarmWhy::Login,
                Some(FactWhy::Key) => WarmWhy::Key,
                Some(FactWhy::BaseUntouched) => WarmWhy::BaseUntouched,
                Some(FactWhy::Overage) => WarmWhy::Overage,
                Some(FactWhy::Refused) => WarmWhy::Refused,
                Some(FactWhy::Used) | None => {
                    WarmWhy::Full(usize::from(standing::used_up(standing::used_at(x, 1, t))))
                }
            }
        };
        return WarmVerdict::Wait {
            until: standing::sendable_again(x, t),
            why,
        };
    }
    if x.api {
        return WarmVerdict::Wait {
            until: None,
            why: WarmWhy::Api,
        };
    }
    match x.resets[0] {
        Some(r) if r > t => WarmVerdict::Wait {
            until: Some(r),
            why: WarmWhy::Ticking,
        },
        _ => WarmVerdict::Send(WarmWhy::NoWindow),
    }
}

/// 策略交的 `warm` 收不收这个号：[`warm_verdict`] 说发。
pub(crate) fn warmable(x: &Acct, t: u64) -> bool {
    matches!(warm_verdict(x, t), WarmVerdict::Send(_))
}

/// ★ `agent` 那一家的会话此刻用得了 `a`（唯一一处：判的候选与会话顺序都照它）：会话没把它先去掉 · 本家的号里有它 · 事实上发得出去。
pub(crate) fn usable(agent: &str, off: &[String], facts: &[Acct], a: &str, t: u64) -> bool {
    !off.iter().any(|x| x == a) && fact(facts, agent, a).is_some_and(|x| standing::sendable(x, t))
}

/// 会话的顺序：第一格 ＝ 在用的号，往下照拖出来的 · 策略交的先后，此刻用不了的与先不用的沉底（各照号的先后）；只排会话那一家的号。
pub fn lineup_of(
    current: &str,
    off: &[String],
    d: &Decision,
    facts: &[Acct],
    t: u64,
) -> Vec<String> {
    lineup_by(
        &d.sigma.agent,
        current,
        off,
        d.sigma.temp.iter().chain(&d.order),
        facts,
        t,
    )
}

/// 不轮换的会话的顺序：第一格 ＝ 在用的号，往下照号的先后，此刻用不了的与先不用的沉底（没有策略交的先后）；只排 `agent` 那一家的号。
pub fn plain_lineup(
    agent: &str,
    current: &str,
    off: &[String],
    facts: &[Acct],
    t: u64,
) -> Vec<String> {
    lineup_by(agent, current, off, std::iter::empty(), facts, t)
}

/// 第一格在用的号，往下 `prefer` 里用得了的，再照本家号的先后：用得了的在前，余下的沉底。
fn lineup_by<'a>(
    agent: &'a str,
    current: &str,
    off: &[String],
    prefer: impl Iterator<Item = &'a String>,
    facts: &'a [Acct],
    t: u64,
) -> Vec<String> {
    let mut out = vec![current.to_string()];
    let ids: Vec<&String> = mine(facts, agent).map(|x| &x.id).collect();
    let ok = prefer
        .chain(ids.iter().copied())
        .filter(|a| usable(agent, off, facts, a, t));
    for a in ok.chain(ids.iter().copied()) {
        if !out.contains(a) {
            out.push(a.clone());
        }
    }
    out
}

/// 策略交的 `warm` 照事实分两份：收下的 · 丢掉的（不在本家号里的也丢）。同一个号只算一次。
fn warm_of(want: &[String], facts: &[Acct], agent: &str, t: u64) -> (Vec<String>, Vec<String>) {
    let (mut keep, mut drop) = (Vec::new(), Vec::new());
    for a in want {
        if keep.contains(a) || drop.contains(a) {
            continue;
        }
        if fact(facts, agent, a).is_some_and(|x| warmable(x, t)) {
            keep.push(a.clone());
        } else {
            drop.push(a.clone());
        }
    }
    (keep, drop)
}

/// 在用的号是用户手动选的：拖出来的顺序排第一的就是它（「现在用」· 拖到第一格）。
/// 交给策略的 `pinned` 与 [`choose`] 留不留在用的号都照这一处。
fn pinned(sigma: &Sigma) -> bool {
    sigma.temp.first() == Some(&sigma.current)
}

/// 拼交给策略的 ctx（只交会话那一家的号，照事实的先后；`pos` 是在本家号里的第几个）。
pub(crate) fn ctx_of<'a>(
    sigma: &'a Sigma,
    facts: &[Acct],
    settings: &'a Value,
    clock: u16,
    t: u64,
) -> Ctx<'a> {
    Ctx {
        now: t,
        clock,
        start: &sigma.start,
        current: &sigma.current,
        settings,
        memo: sigma.memo.as_ref(),
        me: &sigma.me,
        pinned: pinned(sigma),
        temp: &sigma.temp,
        accounts: mine(facts, &sigma.agent)
            .enumerate()
            .map(|(pos, x)| CtxAcct {
                agent: x.agent.clone(),
                id: x.id.clone(),
                pos,
                api: x.api,
                off: sigma.off.contains(&x.id),
                weekly: x.weekly,
                others: x.others,
                past: x.past.clone(),
                sendable: !sigma.off.contains(&x.id) && standing::sendable(x, t),
                used: [standing::used_at(x, 0, t), standing::used_at(x, 1, t)],
                resets: x.resets.map(|r| r.filter(|r| *r > t)),
                refused_until: x.refused_until.filter(|u| *u > t),
                rate: x.rate,
                per_pct: x.per_pct,
            })
            .collect(),
    }
}

/// 照策略交的先后定这一判用谁（算法 §3 第 2–5 步）：结论 · 新的会话状态（叫醒与 memo 取这一份 Verdict 的）· 跳过的拖出来的条目 · 是不是拖出来的那个。
struct Chosen {
    pick: Pick,
    sigma: Sigma,
    skipped: Vec<String>,
    from_temp: bool,
}

/// `agent` 那一家（去掉 `off` 的）里最早又发得出去的号与时刻（停发 · 派活拒都照它）；说不出 ⇒ 空。
fn earliest<'a>(
    facts: &'a [Acct],
    agent: &'a str,
    off: &[String],
    t: u64,
) -> Option<(u64, &'a str)> {
    let mut best: Option<(u64, &str)> = None;
    for x in mine(facts, agent).filter(|x| !off.contains(&x.id)) {
        if let Some(at) = standing::sendable_again(x, t) {
            if best.is_none_or(|(b, _)| at < b) {
                best = Some((at, &x.id));
            }
        }
    }
    best
}

fn choose(
    sigma: &Sigma,
    v: &Verdict,
    facts: &[Acct],
    t: u64,
    send: &dyn Fn(&str) -> bool,
) -> Chosen {
    // 2：候选 ＝ order 里事实上发得出去的。
    let cands: Vec<&String> = v.order.iter().filter(|a| send(a)).collect();
    let c = sigma.current.as_str();
    let mut next = Sigma {
        agent: sigma.agent.clone(),
        start: sigma.start.clone(),
        current: sigma.current.clone(),
        temp: sigma.temp.clone(),
        off: sigma.off.clone(),
        wake_when: v.wake_when.clone(),
        memo: v.memo.clone(),
        me: sigma.me.clone(),
    };
    let mut skipped = Vec::new();
    // 3：在用的号是手动选的、事实上发得出去 ⇒ 留着（策略列不列它、排第几都不看）。
    // 该换了（手动选的发不出去了 · 不是手动选的又不在候选里）⇒ 先照用户拖出来的顺序：第一个事实上发得出去的被选中（策略列不列它都算），排在它前面的丢掉。
    let keep = pinned(sigma) && send(c);
    let mut chosen: Option<String> = keep.then(|| c.to_string());
    if !keep && !cands.iter().any(|a| *a == c) {
        next.temp.retain(|a| a != c);
        if let Some(i) = next.temp.iter().position(|a| send(a)) {
            skipped = next.temp[..i].to_vec();
            next.temp.drain(..i);
            chosen = Some(next.temp[0].clone());
        }
    }
    let from_temp = chosen.is_some();
    // 4：否则取候选第一个（当前号排第一 ⇒ 不换）。
    let pick = match chosen.or_else(|| cands.first().map(|a| (*a).clone())) {
        Some(a) => Pick::Use(a),
        None => {
            // 5：候选空 ⇒ 停发。
            let best = earliest(facts, &sigma.agent, &sigma.off, t);
            Pick::Hold {
                until: v.hold_until.or(best.map(|b| b.0)),
                account: best.map(|b| b.1.to_string()),
            }
        }
    };
    // 留着拖出来的那个号（它排在拖出来的顺序最前）也算来自拖出来的顺序。
    let from_temp = from_temp
        || (pick == Pick::Use(c.to_string()) && next.temp.first().map(String::as_str) == Some(c));
    if let Pick::Use(a) = &pick {
        next.current = a.clone();
    }
    Chosen {
        pick,
        sigma: next,
        skipped,
        from_temp,
    }
}

/// ★ 一判。`tried` ＝ 这一发已经试过的号（429 同一发重判时排除），平时空。`clock` ＝ 策略时区里 t 是一天的第几分钟。
pub fn turn(
    sigma: &Sigma,
    facts: &[Acct],
    settings: &Value,
    p: &dyn Strategy,
    clock: u16,
    t: u64,
    tried: &[String],
) -> Decision {
    let agent = sigma.agent.as_str();
    let ctx = ctx_of(sigma, facts, settings, clock, t);
    let send = |a: &str| !tried.iter().any(|x| x == a) && usable(agent, &sigma.off, facts, a, t);
    // 策略交的先后里有别家的号 ⇒ 当策略出错（一个会话只用本家的号）；错带着「是不是交了别家的号」。
    let ask = |c: &Ctx<'_>| -> Result<Verdict, (super::super::strategy::Fail, bool)> {
        let v = p.decide(c).map_err(|e| (e, false))?;
        match foreign(&v.order, facts, agent) {
            Some(a) => Err((super::super::strategy::Fail::foreign(a), true)),
            None => Ok(v),
        }
    };
    // 兜底那一份：此刻的号发得出去就照用，否则照本家号的先后取第一个发得出去的。
    let fallback = || {
        let mut order: Vec<String> = vec![sigma.current.clone()];
        order.extend(
            mine(facts, agent)
                .map(|x| x.id.clone())
                .filter(|a| *a != sigma.current),
        );
        Verdict {
            order,
            memo: sigma.memo.clone(),
            ..Verdict::default()
        }
    };
    // 1：叫策略；出错 ⇒ 兜底。
    let (v, mut strategy_error) = match ask(&ctx) {
        Ok(v) => (v, None),
        Err((e, _)) => (fallback(), Some(e)),
    };
    let mut r = choose(sigma, &v, facts, t, &send);
    let (mut order, mut why, mut wake_at, mut warm) = (v.order, v.why, v.wake_at, v.warm);
    // 换了号 ⇒ 在用的号变了，这是一件事实：照新的在用号再问策略一次（它约的叫醒跟着新在用号走，不然要等到别的节点才知道几点再判）。
    // 它此刻就又要换（例：策略的先后跟着在用的是谁变，对着新换上的号要另一个）⇒ 同一刻接着换（第 4 步照新的在用号再走一遍），
    // 直到它留下；同一刻里一个号只换到一次（策略来回要 ⇒ 停在这一个），至多本家号数那么多次。
    // 再问的那一下交了别家的号 ⇒ 同第一问：当策略出错、照兜底；别的出错 ⇒ 停在已换到的这一个。
    if strategy_error.is_none() {
        let mut seen = vec![sigma.current.clone()];
        let n = mine(facts, agent).count();
        while let Pick::Use(a) = &r.pick {
            if seen.contains(a) || seen.len() > n {
                break;
            }
            seen.push(a.clone());
            let v2 = match ask(&ctx_of(&r.sigma, facts, settings, clock, t)) {
                Ok(v2) => v2,
                Err((e, true)) => {
                    let v = fallback();
                    r = choose(sigma, &v, facts, t, &send);
                    (order, why, wake_at, warm) = (v.order, v.why, v.wake_at, v.warm);
                    strategy_error = Some(e);
                    break;
                }
                Err((_, false)) => break,
            };
            let r2 = choose(&r.sigma, &v2, facts, t, &send);
            wake_at = v2.wake_at;
            warm = v2.warm;
            r.sigma.wake_when = v2.wake_when;
            r.sigma.memo = v2.memo;
            match &r2.pick {
                Pick::Use(b) if b != a && !seen.contains(b) => {
                    let mut skipped = std::mem::take(&mut r.skipped);
                    skipped.extend(r2.skipped.iter().cloned());
                    r = Chosen { skipped, ..r2 };
                    (order, why) = (v2.order, v2.why);
                }
                _ => break,
            }
        }
    }
    let Chosen {
        pick,
        sigma: next,
        skipped,
        from_temp,
    } = r;
    // 6：下一个已知节点。
    let mut tn: Vec<u64> = Vec::new();
    for x in mine(facts, agent) {
        tn.extend(x.resets.iter().flatten().copied());
        tn.extend(x.refused_until);
        tn.extend(x.barred.and_then(|b| b.until));
    }
    match wake_at {
        Some(n) if n > t => tn.push(n.max(t + 60)),
        _ if p.time_varying() => tn.push(t + FALLBACK_STEP),
        _ => {}
    }
    if let Pick::Hold { until: Some(u), .. } = &pick {
        tn.push(*u);
    }
    let (warm, warm_dropped) = warm_of(&warm, facts, agent, t);
    Decision {
        warm,
        warm_dropped,
        pick,
        sigma: next,
        skipped,
        order,
        why,
        from_temp,
        tn: tn.into_iter().filter(|x| *x > t).min(),
        wake_at,
        strategy_error,
    }
}

/// 「现在用 a」（用户的手动操作）：事实上发得出去 ⇒ 放到拖出来的顺序最前、换过去；发不出去 · 这个会话先不用它 ⇒ 不改，回几点恢复。
/// 策略的上限不拦它。
pub fn use_now(sigma: &Sigma, facts: &[Acct], t: u64, a: &str) -> Result<Sigma, Option<u64>> {
    let x = fact(facts, &sigma.agent, a).ok_or(None)?;
    if sigma.off.iter().any(|b| b == a) {
        return Err(None);
    }
    if !standing::sendable(x, t) {
        return Err(standing::sendable_again(x, t));
    }
    let mut s = sigma.clone();
    s.temp.retain(|b| b != a);
    s.temp.insert(0, a.to_string());
    s.current = a.to_string();
    Ok(s)
}

/// 派一件活落到哪一家的哪个号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPick {
    pub agent: String,
    pub account: String,
}

/// 派不了：那一家不能派活 · 那一家此刻没有用得上的号（`until` ＝ 最早几点恢复，说不出 ⇒ 空）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRefuse {
    NoTask { agent: String },
    NoAccount { agent: String, until: Option<u64> },
}

/// ★ 派一件活选家选号（唯一一处；纯判）。家缺省是派活那条会话 `sigma` 那一家，`to` 指名别的家才跨家；
/// 活着的会话不换家，跨家只对这一件活。`can_task` ＝ 那一家能不能派活（有没有一次性跑一件活那一形），调用方当事实交进来；
/// 不能 ⇒ 照实拒，不改派。号照那一家的顺序挑第一个用得上的：本家照会话的顺序（在用的号 · 拖出来的 · 号的先后；
/// 会话先去掉的不算；策略交的先后在这一判之外，不看），别家照那一家号的先后（本会话的勾与拖出来的都管不到别家）；
/// 一个都用不了 ⇒ 拒，带那一家最早几点又发得出去。
pub fn task_pick<'a>(
    sigma: &'a Sigma,
    to: Option<&'a str>,
    can_task: &dyn Fn(&str) -> bool,
    facts: &'a [Acct],
    t: u64,
) -> Result<TaskPick, TaskRefuse> {
    let agent = to.unwrap_or(&sigma.agent);
    if !can_task(agent) {
        return Err(TaskRefuse::NoTask {
            agent: agent.to_string(),
        });
    }
    let off: &[String] = if agent == sigma.agent {
        &sigma.off
    } else {
        &[]
    };
    let order: Vec<String> = if agent == sigma.agent {
        lineup_by(agent, &sigma.current, off, sigma.temp.iter(), facts, t)
    } else {
        mine(facts, agent).map(|x| x.id.clone()).collect()
    };
    match order.into_iter().find(|a| usable(agent, off, facts, a, t)) {
        Some(account) => Ok(TaskPick {
            agent: agent.to_string(),
            account,
        }),
        None => Err(TaskRefuse::NoAccount {
            agent: agent.to_string(),
            until: earliest(facts, agent, off, t).map(|b| b.0),
        }),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/backend/accounts/quota/rules/decide_tests.rs"]
mod tests;
