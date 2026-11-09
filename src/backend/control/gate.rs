//! Gate 2（identity）在后端侧：「这个 tmux 会话是不是本工具管的」—— 送键 / 杀会话之前过这一道。
//!
//! 判定本身在 [`super::gate_rules`]；本模块只负责承载：怎么把 `@ccm_sid` 从本机 tmux 取回来。
//!
//! # 用 `#{session_id}` 当句柄，关掉 TOCTOU 窗口
//!
//! argv 直传、没有 shell，「探测」与「动作」折不进一条 tmux 调用。所以探测时连 `#{session_id}` 一起取回
//! （tmux 的 `$N`，server 生命周期内唯一、不复用），之后一律对那个句柄下命令：名字在窗口期内被重新绑定到
//! 别的会话，句柄仍指着核验过的那一个；那个会话已消失 ⇒ `send-keys` 失败、回 `typed_unconfirmed`，不会打到别人身上。
//!
//! # 用 `display-message` 而不是 `show-options`
//!
//! `show-options` 对未设置的 option 是 `rc=1` + stderr；`display-message -p` 静默展开成空串。
//! 目标不存在时整条输出为空但 `rc=0` ⇒「目标在不在」看输出为空，不看退出码。

use crate::platform::child::Child;
use copy_core::copy_text;

/// 命令级错误：`(code, message)`。与 [`super::launch`] 同型。
use crate::common::said::Said;

// UTF-8 客户端旗与段数下溢的口径住 `crate::common::tmux_utf8` 头注，这里只记本调用点的两句：
// ① 本机 argv 直传 ⇒ 用旗，而且必须排在子命令之前。
// ② 这一处刻意不看退出码（没有会话时 `tmux ls` 就是非零 + 空输出），放错位置的 `-u` 会被压成「探不到」⇒
//    判据钉的是旗的位置，不只是有没有。
use crate::common::tmux_utf8::{tab_underflow, UTF8_CLIENT_FLAG};

/// 探测回来的四格。字段私有、不能复制 ⇒ 只有 [`probe_with`] 造得出，门放不放只按探回来的判。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Probed {
    /// tmux 的 `#{session_id}`（形如 `$0`）。**后续一律对它下命令**，见模块头注。
    session_id: String,
    /// `@ccm_sid` 的值。未设置 ⇒ 空串。
    ///
    /// 是 `@ccm_sid`，不是 `@ccm_sid_expect`：通道 A（`shared/ccm`）只写意图，通道 B 才写事实，破坏性动作只认事实。
    /// 通道 B 的写者是后端自己（[`super::identity_tag`]，由 pidfile inotify 驱动，打标前过 `procStart` 冒名检查）。
    ccm_sid: String,
    /// `#{session_windows}`。**Gate 3 只给破坏性动作用**（见 [`admit_destructive`]）。
    /// 解析不出来 ⇒ `0`，而 Gate 3 要求恰好 `1` ⇒ **fail closed**（不会误杀）。
    windows: u32,
    /// `@ccm_client`：起这个会话的那一方声明的名字（没设 ⇒ 空串）。「哪个前端的会话」那一维（`gate_rules::gate_client`）。
    client: String,
}

impl Probed {
    /// `#{session_id}` 句柄。
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    /// `@ccm_sid`（没设 ⇒ 空串）。
    pub(crate) fn ccm_sid(&self) -> &str {
        &self.ccm_sid
    }
}

/// 探测格式串。三个字段用 TAB 分隔：`session_id` 恒是 `$<数字>`，`@ccm_sid` 是会话 id（`cmd_wire::Sid`，字符集 `[A-Za-z0-9-]`）
/// （`launch::parse_request`），`session_windows` 对存在的会话恒为正整数。
///
/// `#{session_windows}`（Gate 3 用）放在同一次探测里：多一次 `display-message` 就多一个 TOCTOU 窗口。
const PROBE_FMT: &str = "#{session_id}\t#{@ccm_sid}\t#{session_windows}\t#{@ccm_client}";

/// `PROBE_FMT` 的列数 —— [`tab_underflow`] 的 N。三列的取值域都排除了真 TAB，所以过溢只可能来自
/// 有人把 `@ccm_sid` 手工设成含 TAB 的值、或格式串被改 —— 两种都拒（fail-closed）。
const PROBE_FMT_FIELDS: usize = 4;

/// 本模块起 tmux 的那一处（经宿主口 `host::tmux::child`）。`socket`：`None` = 让 tmux 按环境解析默认 socket；
/// `Some(p)` = 显式 `-S <p>`（问的是列名单的那一台：终端名单 · 判据的隔离 server）。
#[expect(
    clippy::disallowed_methods,
    reason = "三道门里身份那一探（取 `@ccm_sid`）：只读 tmux，不改任何状态"
)]
fn tmux_on(socket: Option<&str>) -> Child {
    crate::host::tmux::child(socket)
}

/// **探一次**：跑 `tmux display-message -p -t <target> '<fmt>'` 并把 stdout 取回来；`tmux` 由调用方造（门问它那一台，
/// [`super::identity_tag`] 经它自己那一个口递进来，测试构建里那个口是注入的假 tmux，`INVARIANTS §48.3`）。
///
/// `Ok(None)` = 目标不存在（输出为空，见模块头注：不看退出码）。
///
/// `target` 不限于会话：pane id（`%N`）也会被归到它所属的会话上，[`super::identity_tag`] 就拿 `TMUX_PANE` 当 target 调它。
/// 空串 target 会被 tmux 静默解析成「某个会话」，调用方必须自己挡（`host::tmux::pane_id_ok`）。
pub(crate) fn probe_with(tmux: Child, target: &str) -> Result<Option<Probed>, Said> {
    let out = tmux
        // `-u` 必须在子命令之前（`display-message -u -p` 是 rc=1 的响错）。
        .args([
            UTF8_CLIENT_FLAG,
            "display-message",
            "-p",
            "-t",
            target,
            PROBE_FMT,
        ])
        .run(crate::host::tmux::TMUX_WITHIN)
        // 句子只说原因词；系统原话进原话位（复制详情）。
        .map_err(crate::host::tmux::tmux_unavailable)?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.trim_end_matches(['\n', '\r']);
    if line.is_empty() {
        return Ok(None);
    }
    // 段数下溢 ⇒ 通道被改写：出声，并拒绝把这行当好数据。不然 `ccm_sid` 拿到空串，`admit` 报出来的话
    // 与「远端真的没设」逐字相同。处置沿用 `Ok(None)`（fail-closed，`admit` 回 `no_such_session`），warn 里带原样回包。
    if tab_underflow(line, PROBE_FMT_FIELDS) {
        tracing::warn!(
            "CCM_TMUX_UNPARSABLE display-message 切出 {} 段 < {PROBE_FMT_FIELDS} —— \
             tmux 打印通道被改写（客户端不是 UTF-8 ⇒ TAB 变 `_`），\
             判成「探不到」而不是「@ccm_sid 没设」。原样回包：{line:?}",
            line.split('\t').count()
        );
        return Ok(None);
    }
    // 逐段切。**多出一段就是格式串被人动过了**，宁可判不存在也不猜。
    let mut it = line.split('\t');
    let session_id = it.next().unwrap_or_default().to_string();
    let ccm_sid = it.next().unwrap_or_default().to_string();
    // 解析不出来 ⇒ 0。Gate 3 要求恰好 1 ⇒ **fail closed**（拿不到窗口数就不许杀）。
    let windows = it
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u32>()
        .unwrap_or(0);
    let client = it.next().unwrap_or_default().to_string();
    if session_id.is_empty() || it.next().is_some() {
        return Ok(None);
    }
    Ok(Some(Probed {
        session_id,
        ccm_sid,
        windows,
        client,
    }))
}

/// 请求里自报的前端（入参的 `client`，typed 命令已按类型解好）：没给 ⇒ `None`；给了就得是一个合形状的名字（`gate_rules::client_name_ok`）。
pub fn requester(client: Option<&str>) -> Result<Option<String>, Said> {
    match client {
        None => Ok(None),
        Some(c) if crate::control::gate_rules::client_name_ok(c) => Ok(Some(c.to_string())),
        Some(c) => Err(crate::common::said::Said::new(
            "bad_args",
            crate::common::contract::malformed(&format!(
                "`client` must be 1-32 chars of [a-z0-9-]: {c:?}"
            )),
        )),
    }
}

/// 身份那一道（Gate 2 ＋「哪个前端的会话」那一维）的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Who {
    /// 放行。
    Pass,
    /// 名字规则没过（没声明哪个前端、名字不像我们铸的、`@ccm_sid` 也没设）。
    NotOurs,
    /// 声明了别的前端：只读。
    OtherClient,
}

/// 声明了别的前端 ⇒ 只读；声明归这次请求自报的前端 ⇒ 放；没声明 / 用户终端起的 ⇒ 照名字规则（`gate2`）。
/// `client` ＝ 会话上的 `@ccm_client`，`ccm_sid` ＝ 按它判身份的那个 `@ccm_sid`（都没设 ⇒ 空串）。只答放不放，不发令牌。
pub(crate) fn identity(name: &str, client: &str, ccm_sid: &str, requester: Option<&str>) -> Who {
    use crate::control::gate_rules::{gate_client, ClientVerdict};
    match gate_client(client, requester) {
        ClientVerdict::Own => Who::Pass,
        ClientVerdict::NotYours => Who::OtherClient,
        ClientVerdict::Open if crate::control::gate_rules::gate2(name, Some(ccm_sid)).allowed() => {
            Who::Pass
        }
        ClientVerdict::Open => Who::NotOurs,
    }
}

/// **按名单那一行的事实判身份**（谁起的 · sid · 宿主）：托管的全是我们起的（只有 ccm 起得了托管进程）⇒ 只看「哪个前端的」那一维、
/// 不跑名字规则；tmux 那一行照 [`identity`]（名字 · `@ccm_client` · `@ccm_sid`）。只答放不放；tmux 那一形真下手之前仍经 [`admit`] 再探一次。
pub(crate) fn identity_of(row: &crate::host::TermRow, requester: Option<&str>) -> Who {
    use crate::control::gate_rules::{gate_client, ClientVerdict};
    match row.host {
        crate::wire::TerminalHost::Managed => match gate_client(&row.client, requester) {
            ClientVerdict::NotYours => Who::OtherClient,
            ClientVerdict::Own | ClientVerdict::Open => Who::Pass,
        },
        crate::wire::TerminalHost::Tmux => identity(&row.name, &row.client, &row.sid, requester),
    }
}

/// 过了门的目标（`#{session_id}` 或 `#{pane_id}` 句柄）：字段私有、不能复制 ⇒ 只有本模块造得出，送键只收它。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Admitted(String);

impl Admitted {
    /// 对它下命令用的 tmux 句柄。
    pub(crate) fn handle(&self) -> &str {
        &self.0
    }
}

/// 送键那道门不放的几种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Barred {
    /// 目标不存在。
    Gone,
    /// 给了 `sid`，哪个窗格都不挂它（此刻跑的已经不是那条会话）。
    OtherSid,
    /// 身份没过（[`identity`] 的那两种）。
    Not(Who),
}

/// **过门。** 通过则返回令牌：后续该用的目标句柄（`#{session_id}`；给了 `sid` ⇒ 挂着它的那个窗格的 `#{pane_id}`）。
///
/// `name` 是用户/调用方给的会话名（Gate 2 的本地半支按它判）；
/// `target` 是 `common::tmux_utf8::exact_target(name)` 产出的精确目标（`=name:`）。
/// `sid` ＝ 要的是这个会话里挂着它的那个窗格（一个 tmux 会话里可以有几个 claude 窗格）。
///
/// 三种结局：
/// - 目标不存在 ⇒ `no_such_session`（**语义不变** —— 新门不许把这一档吞掉）；
/// - Gate 2 不通过 / 哪个窗格都不挂那个 sid ⇒ `wrong_owner`；
/// - 通过 ⇒ `Ok(句柄)`。
///
/// `requester` ＝ 请求自报的前端（「哪个前端的会话」那一维，见 [`identity`]）；声明了别的前端 ⇒ 也是 `wrong_owner`，话不同。
pub(crate) fn admit(
    name: &str,
    target: &str,
    sid: Option<&str>,
    requester: Option<&str>,
) -> Result<Admitted, Said> {
    let said = match admit_with(
        crate::host::tmux::On::default(),
        name,
        target,
        sid,
        requester,
    )? {
        Ok(a) => return Ok(a),
        Err(Barred::Gone) => {
            return Err(crate::common::said::Said::new(
                "no_such_session",
                copy_text("beGate.admit.noSession", &[("name", name)]),
            ))
        }
        Err(Barred::OtherSid) => copy_text("beGate.admit.otherSession", &[("name", name)]),
        Err(Barred::Not(Who::OtherClient)) => {
            copy_text("beGate.admit.otherClient", &[("name", name)])
        }
        Err(Barred::Not(_)) => copy_text("beGate.admit.notOurs", &[("name", name)]),
    };
    Err(crate::common::said::Said::new("wrong_owner", said))
}

/// [`admit`] 的本体，问哪台 tmux 由调用方给（终端送字那一路问它自己那台）：探 → 按 `sid` 找窗格 → 判身份 → 令牌。
/// 给了 sid ⇒ 落在挂着它的那个窗格、身份也按它判（活动窗格是谁不算数）。
pub(crate) fn admit_with(
    on: crate::host::tmux::On<'_>,
    name: &str,
    target: &str,
    sid: Option<&str>,
    requester: Option<&str>,
) -> Result<Result<Admitted, Barred>, Said> {
    let judge = || -> Result<Result<Admitted, Barred>, Said> {
        let Some(p) = probe_with(tmux_on(on.socket), target)? else {
            return Ok(Err(Barred::Gone));
        };
        let (ccm_sid, at) = match sid {
            None => (p.ccm_sid, p.session_id),
            Some(s) => {
                let panes = crate::host::tmux::panes_on(on, &p.session_id);
                let Some(c) = crate::host::tmux::carrier(&panes, s) else {
                    return Ok(Err(Barred::OtherSid));
                };
                (c.sid.clone(), c.pane.clone())
            }
        };
        Ok(match identity(name, &p.client, &ccm_sid, requester) {
            Who::Pass => Ok(Admitted(at)),
            no => Err(Barred::Not(no)),
        })
    };
    retagged(judge)
}

/// 破坏性动作的门：Gate 2（身份）再加 Gate 3（`windows == 1`）。
///
/// 本工具建的会话也可能被用户扩出了别的窗口，整个杀掉会连带毁掉用户的活 ⇒ 只杀干净的单窗口会话，
/// 多窗口 ⇒ 拒绝（`CCM_GUARD_REJECTED windows=<n>`），让用户自己去那个 tmux 里处理。
/// Gate 3 只给破坏性动作：`send-keys` 不删东西，给它加 Gate 3 会误拒「往多窗口会话里打字」—— 所以与 [`admit`] 是两个入口。
///
/// **按名单那一行的句柄探**（`row.id`，tmux 的 `#{session_id}`，不复用），在列名单的同一台 tmux（`on`）上：列名单之后那个会话没了 ⇒
/// `no_such_session`，同名又起的那一个不会被认成它（不按名字再认一次）。名字只拿来说话与判名字规则（Gate 2）。
///
/// 这一行挂着 sid 的窗格（`row.pane`）⇒ 按挂着那个 sid 的窗格认（`host::tmux::panes_on`）：哪个窗格都不挂它 ⇒ 认完换了人（`wrong_owner`）；身份按那个窗格判；
/// 会话里还有窗格挂着别的 sid（别的 claude）⇒ 只结束挂着它的窗格（不关整个会话，Gate 3 不适用）；没有 ⇒ 整个会话（照常过 Gate 3）。
pub(crate) fn admit_destructive(
    on: crate::host::tmux::On<'_>,
    row: &crate::host::TermRow,
    requester: Option<&str>,
) -> Result<Doomed, Said> {
    let name = row.name.as_str();
    let sid = row.pane.as_ref().map(|_| row.sid.as_str());
    let judge = || -> Result<Result<Doomed, Barred>, Said> {
        let Some(p) = probe_with(tmux_on(on.socket), &row.id)? else {
            return Ok(Err(Barred::Gone));
        };
        let (who, only) = match sid {
            None => (p.ccm_sid.clone(), Vec::new()),
            Some(s) => {
                let tags = crate::host::tmux::panes_on(on, &p.session_id);
                let carrying: Vec<String> = tags
                    .iter()
                    .filter(|t| t.sid == s)
                    .map(|t| t.pane.clone())
                    .collect();
                if carrying.is_empty() {
                    return Ok(Err(Barred::OtherSid));
                }
                let others = tags.iter().any(|t| !t.sid.is_empty() && t.sid != s);
                (s.to_string(), if others { carrying } else { Vec::new() })
            }
        };
        match identity(name, &p.client, &who, requester) {
            Who::Pass => {}
            no => return Ok(Err(Barred::Not(no))),
        }
        if !only.is_empty() {
            return Ok(Ok(Doomed(EndAt::Panes(p.session_id, only))));
        }
        if p.windows != 1 {
            return Err(crate::common::said::Said::new(
                "too_many_windows",
                copy_text(
                    "beGate.admitDestructive.manyWindows",
                    &[("name", name), ("n", &p.windows.to_string())],
                ),
            ));
        }
        Ok(Ok(Doomed(EndAt::Session(p.session_id))))
    };
    let barred = match retagged(judge)? {
        Ok(d) => return Ok(d),
        Err(b) => b,
    };
    Err(match barred {
        Barred::Gone => crate::common::said::Said::new(
            "no_such_session",
            copy_text("beGate.admitDestructive.noSession", &[("name", name)]),
        ),
        Barred::OtherSid => crate::common::said::Said::new(
            "wrong_owner",
            copy_text("beGate.admitDestructive.otherSession", &[("name", name)]),
        ),
        Barred::Not(Who::OtherClient) => crate::common::said::Said::new(
            "wrong_owner",
            copy_text("beGate.admitDestructive.otherClient", &[("name", name)]),
        ),
        Barred::Not(_) => crate::common::said::Said::new(
            "wrong_owner",
            copy_text("beGate.admitDestructive.notOurs", &[("name", name)]),
        ),
    })
}

/// 判一次；身份那一道说「不是我们的」（没设 `@ccm_sid` · 哪个窗格都不挂那个 sid）⇒ 先补打上一回没打上的身份标
/// （`identity_tag::retag_pending`，等正在打的那一趟打完），再探一次重判。别的结局（没了 · 别的前端的 · 放行）原样回。
fn retagged<T>(
    judge: impl Fn() -> Result<Result<T, Barred>, Said>,
) -> Result<Result<T, Barred>, Said> {
    match judge()? {
        Err(Barred::OtherSid | Barred::Not(Who::NotOurs)) => {
            super::identity_tag::retag_pending();
            judge()
        }
        r => Ok(r),
    }
}

/// 破坏性动作的令牌：字段私有、不能复制 ⇒ 只有本模块造得出，结束会话只收它。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Doomed(EndAt);

impl Doomed {
    /// 放行之后落在哪。
    pub(crate) fn at(&self) -> &EndAt {
        &self.0
    }
}

/// 破坏性动作放行之后落在哪。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EndAt {
    /// 整个会话（`#{session_id}` 句柄）。
    Session(String),
    /// 只这几个窗格（`#{pane_id}` 句柄；同会话的 `#{session_id}` 在前）：会话里还跑着挂别的 sid 的 claude。
    Panes(String, Vec<String>),
}

#[cfg(test)]
#[path = "../../../tests/backend/control/gate_tests.rs"]
mod tests;
