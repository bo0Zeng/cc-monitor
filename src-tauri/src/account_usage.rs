//! F10（unify-launch，剩余账号 UX）：每账号 Claude 订阅计划用量窗口百分比（"plan 窗口%"）——
//! 不是 context window 用量（那是 `usage-hud.ts` 的事），不是本地 token 累计（那是
//! `usage.rs`/`views/usage-view.ts` 的事），是 Anthropic 服务端权威的 5h/周额度窗口剩余%，
//! 必须真的起一个已登录的 claude 会话跑 `/usage` 斜杠命令、抓屏。
//!
//! **本模块只负责编排一次性探针会话本身**（起/送键/等/抓屏/清理），完全不理解 `/usage`
//! 输出的语义。
//!
//! 🔴 **〔`K-R101`/`R59` 09-13 订正〕上面那句话后半截原写「——那是 TS 侧
//! `src/account-usage-parse.ts` 纯函数的职责」，今天它是假的**：`R59`〔用 09-13〕逐字
//! 「解析层代码保留, 但是功能先退役」⇒ **生产路上没有解析这一层了**。抓回去的那一屏
//! 由 `src/account-usage.ts` **原样**交给界面（`usageScreenEl`），用户自己看。
//! 解析器与它的 vitest、冻结夹具仍在盘上（退役≠删除），墓碑住那份文件头部。
//! ⇒ 本模块的产出从此**只有一个消费者语义**：那一屏字节。
//!
//! # ★★ `K-R104`（09-13）：**编排整条搬上帧面** —— 本模块从此一个 shell 字符都不渲染
//!
//! 在它之前，本模块把整条编排渲染成**一条 shell 串**交给远端的登录 shell 跑
//! （`tmux kill-session … new-session … setsid sh -c 'sleep N; …' … send-keys … capture-pane`），
//! 那是 `K-R54` 裁定表第 7 行点名的「同一件事盘上有两份实现」。
//!
//! 🔴 **搬它的理由是结构性的，不是「更干净」**：`K-R101` 现打的阻断 ——
//! `K-R86` 的 `--capture-pane` 与 `K-R87` 的 `--oneshot-session` 当时**只有 CLI 面**，
//! 而走 CLI 面**每抓一屏一次 SSH 握手** ⇒ 两段轮询上限 12+20 轮
//! ⇒ 单次探测最多 **36 次握手** vs [`EXEC_TIMEOUT_SECS`] = 25 ⇒ **结构上超时**。
//! ⇒ `K-R104` 先把那两条原语搬上**帧面**（`inbound::REGISTRY` 8 → 10），
//! 本模块再改成在**一条已经建立的控制通道**上发 N 次命令 —— **握手恒 1 次**。
//!
//! 今天这条编排是**既有命令的组合**，一步一条，逐步登记在 [`PROBE_ORCHESTRATION_STEPS`]：
//! `oneshot-session`（起会话 ＋ 挂看门狗 ＋ 定几何）→ `launch send-into`（送启动载荷）
//! → `capture-pane` × N（**轮询在调用方**）→ `launch send-into`（送 `/usage`）
//! → `capture-pane` × N → `kill`（收尾）。
//!
//! ⚠ **轮询没有消失，它换了住址**：daemon 侧零定时器铁律不许「隔 N 毫秒再抓一次」
//! （`K37`〔用 09-11〕逐字「后端只给机制，不给偏好」，而「等画面稳定多久算稳」是偏好），
//! 所以那一半留在本模块，登记在 `rust_timer_registry::REGISTERED`
//! （**从 shell 那张表搬到 Rust 那张表** —— 两张表都是双向的，搬家两侧都会红一次）。
//!
//! **命名与识别**：探针会话名由 **daemon 铸**（`control/oneshot_session.rs::mint_name`），
//! 形状 `ccm-oneshot-<slug>-cc`。本模块**给不了完整名字**，只给 slug ——
//! 撞名由 daemon 当场拒（`name_taken`），不再像老串那样「先无条件 `kill-session` 清场」。
//! ⚠ 那条 `-cc` 尾巴不是装饰：没有它，daemon 铸出来的会话过不了 §34 Gate 2 的名字半支
//! ⇒ `launch send-into` 与 `kill` **都进不去自己刚建的那个会话**。理由全文住那个常量的头注。
//!
//! **孤儿防护**：看门狗由 daemon 挂（`setsid` + `sleep N` + `kill-session <句柄>`，
//! 对**句柄**下手不对名字）。它**独立于这条控制通道**：连接断了、daemon 没了，它照样到点清场。
//! ⇒ 本模块的超时（[`EXEC_TIMEOUT_SECS`]）只负责「别让界面无限等」，清场归它。
//!
//! **不新增周期性后台负载**：探针只在前端按需调用时触发（面板"查看用量"按钮/chip 菜单展开），
//! 没有任何 `setInterval`/定时任务。

use crate::inbound_client::{capture_pane_args, launch_args, oneshot_session_args, CallError};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

/// 探针会话固定几何尺寸——较宽的列数减少 `/usage` 表格换行/裁切风险（真机验证前的保守选择，
/// 见 F10 计划 §7 真机验证清单第 5 条）。
const PROBE_COLS: u32 = 200;
const PROBE_ROWS: u32 = 50;
/// 看门狗自毁超时（秒）——正常路径下探针几秒内就该跑完+清理，这是"万一跑不完"的保险丝。
const WATCHDOG_TIMEOUT_SECS: u32 = 30;
/// 画面稳定轮询：用"抓屏内容连续多久没变"代替固定 sleep 猜测冷启动/网络查询耗时
/// （真机耗时未知，且这个判据格式无关、版本无关，见 F10 计划 §1 设计说明）。
const QUIESCENCE_POLL_INTERVAL_MS: u64 = 500;
/// 判定"画面稳定"所需的**连续无变化次数**（× 间隔 = 静止时长）。6 × 0.5s = 3s。
///
/// ★ **这是 E42 的真正修复点**，且是唯一有实测支撑的那一半
/// （`e2e/usage-probe-acceptance.sh` 场景 4：把它调回 1 就红，其余场景全绿）。
/// 之所以是"静止时长"而不是"画面变过没有"：`send-keys '/usage'` 打进去的字符**会被终端
/// 回显**，屏幕在毫秒级就变了 —— 任何"变过就算数"的判据都会被回显自己满足
/// （我第一版修法就是这么错的，已在真 tmux 上证伪）。**能区分"渲染完了"和"还在等"的
/// 只有：静止得够久。**
///
/// 3s 是**预算，不是测量值**：本仓库不允许起真实已认证的 claude 去测真实渲染耗时
/// （消耗真实订阅额度、且与用户当前会话交互不可控）。取 3s 的依据是它显著大于回显与
/// TUI 重绘的时间尺度（~10ms 级），又装得进下面的时间预算。**残余风险如实说**：真 claude
/// 若在渲染途中静止超过 3s（如网络请求卡顿），仍会抓早 —— 🔴 **〔`K-R101`/`R58` 09-13
/// 订正〕原话接着写「那种情况下解析器返回 `unrecognized` 并把原始屏带回 UI」，
/// 那半句今天不成立（生产路零解析）。今天的兜底更硬：抓早了 ⇒ **用户直接看见那半截屏**，
/// 按刷新再抓一次。★ `R58` 真正买到的是这个 —— 把一个**证不了**的判据（解析得出）
/// 换成一个**能证**的判据（画面两次抓一样）＋ 一个**看得见**的兜底（人）。
/// 仍然是**可见失败，不是静默错值**。
const QUIESCENCE_STILL_POLLS: u32 = 6;
/// 两段等待的轮询上限**分开给**——它们等的不是一回事，预算也不该平摊。
///
/// 第一段等 claude 的 REPL 起来（本地进程启动，快）；第二段等 `/usage` 的面板渲染出来
/// （要拉一次用量数据，可能走网络，慢）。总和受 [`EXEC_TIMEOUT_SECS`] 约束，
/// 由 `time_budget_ordering_holds` 钉住。
const STARTUP_MAX_POLLS: u32 = 12; // 6s
const RENDER_MAX_POLLS: u32 = 20; // 10s
/// **整条编排**的硬超时——防控制通道本身卡死导致 `account_usage` 永久挂起。
///
/// ⚠ `K-R104` 起它盖的面变了：从前它盖「一条 SSH exec」，今天盖「一条通道上的 N 次往返」。
/// 数值不动 —— 它守的是同一件事（别让界面无限等），而 `K-R101` 现打的那个阻断
/// （36 次握手撑破它）正是靠**握手从 36 降到 1** 解掉的，不是靠把这个数调大。
const EXEC_TIMEOUT_SECS: u64 = 25;
/// 单条帧命令的应答期限。整条编排的总闸仍是 [`EXEC_TIMEOUT_SECS`]，这一条只防
/// 「某一条命令自己吊着」把总预算一次吃光 —— 抓一屏 / 起会话都是亚秒级动作。
const CALL_TIMEOUT_SECS: u64 = 10;
/// 送进 TUI 的那条斜杠命令。**唯一住址**（判据也引它，不写第二份字面量）。
const USAGE_SLASH_COMMAND: &str = "/usage";
/// 会话名后缀的前半段 —— daemon 铸名时会在它前面加 `ccm-oneshot-`、后面加 `-cc`。
const PROBE_SLUG_PREFIX: &str = "usage-";

/// `e2e/usage-probe-acceptance.sh` 里那个**由脚本替换成真名字**的占位 token。
///
/// 会话名由 daemon 在运行期铸，编译期给不出 ⇒ 那套 e2e 的输入源印这个 token，
/// 脚本拿到 `oneshot-session` 的应答之后原样替换。
/// **它是常量而不是脚本里的一个字面量**：两侧各写一份的话，改一侧不会红。
pub(crate) const E2E_SESSION_PLACEHOLDER: &str = "CCM-E2E-SESSION";

#[derive(serde::Serialize, Debug, Clone, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct AccountUsageProbeResult {
    /// true = 拿到了屏幕文本。
    ///
    /// ⚠ **它不说那屏上是什么** —— `R59` 之后生产路上没有解析层，
    /// `captured=true` 的唯一含义是「抓到了」，**包括抓到一片空白**
    /// （`KR101D1` ③：空屏是成功，把它判成失败是明令禁止的那一形）。
    pub captured: bool,
    /// `captured=true` 时的抓屏原始文本。
    pub raw: Option<String>,
    /// `captured=false` 时的人话原因（没通道 / 后端太旧 / 远端拒绝 / 超时）。
    pub error: Option<String>,
}

/// 把账号名安全化成可以嵌进 tmux 会话名的 slug——只留 `[A-Za-z0-9_-]`，其余字符丢弃；
/// 结果为空（如账号名全是非常规字符,理论上 `validateAcctName` 早已在创建时拒绝这类名字,
/// 这里是纵深防御,不信任调用方）时兜底成 `x`,保证候选名恒非空、恒安全。
fn slugify_account_name(name: &str) -> String {
    let s: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(32)
        .collect();
    if s.is_empty() {
        "x".to_string()
    } else {
        s
    }
}

/// 一次帧命令失败的两档。**它们不是本模块自己分的** —— 是
/// `backend::control::daemon_route::route_call_error` 那唯一一份分流规则的两个出口。
///
/// 🔴 **本模块一个字都不许自己 `match CallError`**：那是分流规则的第二份实现，
/// 而它一旦与那一份漂开，一次「被门拒绝」就可能在某条路上被洗成「换条路重做」。
/// `cc_bus::broadcast_via_daemon` 在同一处栽过一次，逐字记在它那儿。
/// 由 `daemon_route::every_daemon_sender_is_registered_and_uses_the_one_router` 钉着
/// （本文件登记为 `Verdict::UsesRouter`）。
///
/// ⚠ **名字里刻意不含 `CallError` 那几个字**：上面那条守卫按**子串**判
/// （`prod.contains("CallError::")`），而 `ProbeCallError::X` 里正好含着它
/// ⇒ 叫那个名字会让本文件被误判成「自己在 match `CallError`」。
/// **这是那道守卫的一处假阳**（如实登记在这里，不是在替它遮丑）——
/// 它按子串判是刻意的粗，收窄成「词边界」会放过 `let e = CallError :: X` 这类写法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProbeStepError {
    /// 分流器判「**能证明一个字节都没发出去**」（`Routed::NoChannel`）。
    ///
    /// 对探针来说这一档**恒是同一件事**：这台机器的后端给不了这条命令。
    /// 最常见的成因就是 `KR104D3` ③ 说的那一形 —— 老 daemon 的 `hello.commands` 里
    /// 没有 `capture-pane` / `oneshot-session`（`K-R104` 才上帧面的两条原语）
    /// ⇒ `InboundClient::accepts` 在发之前就拒了。
    /// **不静默失败、不挂住**，而且分流器的原话里带着「多半是旧版本」。
    NothingWasSent(String),
    /// 分流器判「daemon 说过话了，或者证不了它没执行」（`Routed::Refused`）。原话带出来。
    Refused(String),
}

impl ProbeStepError {
    fn message(&self) -> &str {
        match self {
            ProbeStepError::NothingWasSent(m) | ProbeStepError::Refused(m) => m,
        }
    }
}

/// 一次帧命令调用返回的 future。
pub(crate) type ProbeStepFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Option<Value>, ProbeStepError>> + Send + 'a>,
>;

/// 一条**已经建立**的控制通道：发一条命令、等一条应答。
///
/// # 为什么要这一层抽象（它买的是判据，不是灵活性）
///
/// `KR104D2` ③ 逐字要求「**握手次数有判据在数**，不是靠『跑得快了』这种观感」。
/// 「几次连接」这件事在真 `InboundClient` 上量不出来（那是一条早就建好的长连接），
/// 所以把「拨号」与「在通道上说话」拆成两个可注入的口：
/// 判据喂一个**会数拨号次数**的假通道，就能按数据断言「整条编排恰好拨一次号、往返 N 次」。
///
/// ⚠ **它不是配置口**（同 daemon 侧 `capture_pane::spawn_capture` 那个 `socket` 参数的理由）：
/// 生产只有一个实现 [`DaemonChannel`]，由 [`dial_origin`] 造。
pub(crate) trait ProbeChannel: Send + Sync {
    fn call<'a>(&'a self, cmd: &'a str, args: Value) -> ProbeStepFuture<'a>;
}

/// 生产实现：走 `inbound_client` 那条长连接。
struct DaemonChannel(Arc<crate::inbound_client::InboundClient>);

impl ProbeChannel for DaemonChannel {
    fn call<'a>(&'a self, cmd: &'a str, args: Value) -> ProbeStepFuture<'a> {
        Box::pin(async move {
            self.0
                .call(cmd, args, Duration::from_secs(CALL_TIMEOUT_SECS))
                .await
                .map_err(|e| route(&e))
        })
    }
}

/// 把一次调用失败交给**那唯一一份分流规则**，再落到本模块的两档上。
///
/// ⚠ 这里**没有**任何关于 `CallError` 的判断 —— 判断全在 `route_call_error` 里。
/// 本函数只做一次「`Routed` → 本模块的两档」的搬运（同 `cc_bus` 那条 `BroadcastRoute`）。
fn route(e: &CallError) -> ProbeStepError {
    use crate::backend::control::daemon_route::{route_call_error, Routed};
    match route_call_error(e, |code, message| {
        format!("远端后端拒绝了这一步（{code}）：{message}")
    }) {
        Routed::NoChannel(why) => ProbeStepError::NothingWasSent(why),
        Routed::Refused(why) => ProbeStepError::Refused(why),
        Routed::Done => ProbeStepError::Refused("分流器判成已完成，这不该发生".to_string()),
    }
}

/// 拨号：拿这台机器的控制通道。**本机与远端同一条路** —— `<local>` 也是一个 origin。
///
/// 「没有通道」那句话也只有一个家（`daemon_route::no_channel`），本模块不另写一份。
fn dial_origin(origin: &str) -> Result<Arc<dyn ProbeChannel>, String> {
    use crate::backend::control::daemon_route::{no_channel, Routed};
    match crate::inbound_client::client_for(origin) {
        Some(c) => Ok(Arc::new(DaemonChannel(c)) as Arc<dyn ProbeChannel>),
        None => Err(match no_channel(origin) {
            Routed::NoChannel(why) => {
                format!("{why} —— 用量探针整条走后端，通道不在就是明确失败，不换条路悄悄做掉")
            }
            other => format!("没有控制通道，而分流器给了 {other:?} —— 这不该发生"),
        }),
    }
}

/// 编排的时间参数。**生产恒 [`ProbeTiming::PRODUCTION`]**。
///
/// 可注入的理由与 daemon 侧那两个 `socket` 参数逐字同一条：让「整条编排真的只拨一次号、
/// 真的在一条通道上往返多次」这件事**测得出来**，而不是让判据去等 16 秒真 sleep。
/// **它不是配置口** —— 生产路径上没有任何地方读配置来填它。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeTiming {
    pub(crate) interval_ms: u64,
    pub(crate) still_polls: u32,
    pub(crate) startup_max_polls: u32,
    pub(crate) render_max_polls: u32,
    pub(crate) watchdog_secs: u32,
    pub(crate) cols: u32,
    pub(crate) rows: u32,
}

impl ProbeTiming {
    pub(crate) const PRODUCTION: Self = Self {
        interval_ms: QUIESCENCE_POLL_INTERVAL_MS,
        still_polls: QUIESCENCE_STILL_POLLS,
        startup_max_polls: STARTUP_MAX_POLLS,
        render_max_polls: RENDER_MAX_POLLS,
        watchdog_secs: WATCHDOG_TIMEOUT_SECS,
        cols: PROBE_COLS,
        rows: PROBE_ROWS,
    };
}

/// 只产**载荷**那一段（不含任何编排）。
///
/// 与编排分开是为了**可断言**：载荷是逐字节钉住的（`probe_payload_is_byte_exact_for_both_account_states`），
/// 而编排是一串命令。两件事，两条判据。
fn probe_payload_for(config_dir: Option<&str>) -> Result<String, String> {
    // 键表与启动器都走活跃适配器 —— 它们各自已有 TS↔Rust 对拍守卫。
    let agent = crate::adapter::active();
    crate::backend::control::payload::usage_probe_payload(
        config_dir,
        agent.nested_env_to_scrub(),
        agent.default_launcher(),
    )
}

/// 抓一屏。回的是那一屏的**原文**（空屏是合法的成功）。
async fn capture_screen(ch: &dyn ProbeChannel, session: &str) -> Result<String, ProbeStepError> {
    let data = ch.call("capture-pane", capture_pane_args(session)).await?;
    data.as_ref()
        .and_then(|v| v.get("screen"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            ProbeStepError::Refused(
                "抓屏应答里没有 `screen` 字段 —— 这一端与那一端的契约漂开了".to_string(),
            )
        })
}

/// 往探针会话里送一段文本（附 `Enter`）。
async fn send_into(
    ch: &dyn ProbeChannel,
    session: &str,
    payload: &str,
) -> Result<(), ProbeStepError> {
    ch.call(
        "launch",
        launch_args(
            "send-into",
            session,
            payload,
            None,
            None,
            Default::default(),
        ),
    )
    .await
    .map(|_| ())
}

/// 等画面稳定 —— **轮询住在这里，daemon 里一个定时器都没有**。
///
/// 判据两半，**证据强度不同，别当成一回事**：
///
/// 1. 连续 `still_polls` 次无变化 —— **修复 E42 的就是这一半**，
///    有 e2e 场景 4（慢速 stand-in）红/绿两向实测。
/// 2. `cur != base` —— 排除"什么都没发生"（键没送到 / 会话没起来）。
///    **这一半没有实测支撑，是推理**：拿掉它 e2e 仍全绿（实测过）。留着的理由是它守
///    第 1 半守不住的那个形态 —— 「渲染前的画面本身就静止 ≥3s」。
///    代价有界（最多多等到上限），所以按 fail-safe 留下，但不谎称它被验证过。
///
/// `base` 由调用点在每次送键**之前**取；只取一次不行（第二段会拿"claude 已起来"
/// 的屏当基线，第 2 半退化成恒真）。
async fn settle(
    ch: &dyn ProbeChannel,
    session: &str,
    base: &str,
    max_polls: u32,
    t: ProbeTiming,
) -> Result<String, ProbeStepError> {
    let mut prev = String::new();
    let mut same = 0u32;
    let mut i = 0u32;
    while i < max_polls {
        tokio::time::sleep(Duration::from_millis(t.interval_ms)).await;
        let cur = capture_screen(ch, session).await?;
        if !cur.is_empty() && cur == prev {
            same += 1;
        } else {
            same = 0;
        }
        prev = cur;
        if !prev.is_empty() && prev != base && same >= t.still_polls {
            break;
        }
        i += 1;
    }
    Ok(prev)
}

/// 会话起来之后的那几步。抽出来是为了让**收尾杀会话**在成败两条路上都跑得到。
async fn drive_probe(
    ch: &dyn ProbeChannel,
    session: &str,
    payload: &str,
    t: ProbeTiming,
) -> Result<String, ProbeStepError> {
    let base = capture_screen(ch, session).await?;
    send_into(ch, session, payload).await?;
    settle(ch, session, &base, t.startup_max_polls, t).await?;
    let base = capture_screen(ch, session).await?;
    send_into(ch, session, USAGE_SLASH_COMMAND).await?;
    settle(ch, session, &base, t.render_max_polls, t).await
}

/// **整条编排**：一次拨号 ＋ 一条通道上的 N 次往返。
///
/// 🔴 `dial` **只许被调一次** —— 那一行就是 `KR104D2` 的正题
/// （`§0a` 的阻断：走 CLI 面每抓一屏一次握手 ⇒ 最多 36 次 ⇒ 结构上超时）。
/// 由 `the_whole_probe_dials_once_and_talks_many_times` 按**数据**断言（数拨号次数，
/// 不是数耗时 —— 后者是环境噪声）。
pub(crate) async fn probe_over_frames(
    dial: &(dyn Fn() -> Result<Arc<dyn ProbeChannel>, String> + Sync),
    slug: &str,
    payload: &str,
    t: ProbeTiming,
) -> AccountUsageProbeResult {
    let ch = match dial() {
        Ok(c) => c,
        Err(e) => return probe_failed(e),
    };
    // ① 起会话：daemon 铸名 ＋ 挂看门狗 ＋ 定几何，一次往返办完。
    let started = ch
        .call(
            "oneshot-session",
            oneshot_session_args(
                &format!("{PROBE_SLUG_PREFIX}{slug}"),
                t.watchdog_secs,
                Some((t.cols, t.rows)),
            ),
        )
        .await;
    let session = match started {
        Ok(data) => match data
            .as_ref()
            .and_then(|v| v.get("session"))
            .and_then(|v| v.as_str())
        {
            Some(name) => name.to_string(),
            None => {
                return probe_failed(
                    "起探针会话的应答里没有 `session` 字段 —— 这一端与那一端的契约漂开了"
                        .to_string(),
                )
            }
        },
        Err(e) => return probe_failed(describe(&e)),
    };
    let out = drive_probe(ch.as_ref(), &session, payload, t).await;
    // ② 收尾一律杀会话（成败都杀）。看门狗是**保险丝**，不是清场机制 ——
    //    指望它清场等于让每个探针会话在远端多活到 ttl 秒。
    let _ = ch
        .call("kill", serde_json::json!({ "name": session }))
        .await;
    match out {
        Ok(screen) => AccountUsageProbeResult {
            captured: true,
            raw: Some(screen),
            error: None,
        },
        Err(e) => probe_failed(describe(&e)),
    }
}

/// 失败那一档的**唯一造句处**。
fn probe_failed(why: String) -> AccountUsageProbeResult {
    AccountUsageProbeResult {
        captured: false,
        raw: None,
        error: Some(why),
    }
}

/// 把一次调用失败讲成人话。**两档分开说**（`KR104D3` ③）。
fn describe(e: &ProbeStepError) -> String {
    match e {
        ProbeStepError::NothingWasSent(m) => format!(
            "用量探针一个字节都没发出去：{m}\n\
             ⇒ 最常见的成因是这台机器的后端太旧 —— 抓一屏（`capture-pane`）与\
             起一次性会话（`oneshot-session`）是后来才上帧面的两条原语，\
             **重装那台机器的后端**就有了。"
        ),
        ProbeStepError::Refused(m) => m.clone(),
    }
}

/// 两个 tauri 命令共用的那一段：安全化账号名 → 编载荷 → 跑编排。
///
/// # ★ 本机与远端**逐字同一条路**（定框 C1「一份代码两种承载」）
///
/// `K-R104` 之前这两条各有一个执行面（远端 SSH exec / 本机 `sh -c`），
/// 靠一条比**命令串**的判据证同源。
/// 今天连那半都不存在了：两条**是同一个函数**，只差一个 `origin`
/// （`<local>` 也是一个 origin，`client_for` 两侧都答得出）。
async fn probe_account_usage(
    origin: &str,
    account_name: &str,
    config_dir: Option<&str>,
) -> AccountUsageProbeResult {
    let slug = slugify_account_name(account_name);
    // 载荷由内核编译（P4b 起在 `backend::control::payload`）：账号前缀 + 嵌套 env 清理 + 启动器，无 cd。
    // 构造失败（载荷非法）→ 诚实回报，**不拨号、不发一个字节**。
    let payload = match probe_payload_for(config_dir) {
        Ok(p) => p,
        Err(e) => return probe_failed(e),
    };
    let dial = move || dial_origin(origin);
    match tokio::time::timeout(
        Duration::from_secs(EXEC_TIMEOUT_SECS),
        probe_over_frames(&dial, &slug, &payload, ProbeTiming::PRODUCTION),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => probe_failed(format!(
            "探测超时（{EXEC_TIMEOUT_SECS}s）—— 探针会话由 daemon 侧的看门狗到点清场，\
             不会留下孤儿；稍后重试"
        )),
    }
}

/// F10：per-account 探测 Claude 订阅计划用量窗口%（"plan 窗口%"）。
///
/// `account_name` 只用于探针会话名 slug + 错误文案，不参与鉴权（鉴权/账号存在性由 TS 侧调用
/// 前已经确认过）。
///
/// `config_dir` **两态，没有第三态**（探针恒是 per-account）：
/// `Some(路径)` = 具名账号 · `None` = **账号 0**（产出 `unset CLAUDE_CONFIG_DIR; `，
/// 绝不是「什么都不加」——那会让探针落到远端 rc 的默认号上 = 静默串号）·
/// `Some("")` = 坏数据 ⇒ 诚实回报 probe-failed。
#[tauri::command]
pub async fn account_usage(
    origin: String,
    account_name: String,
    config_dir: Option<String>,
) -> Result<AccountUsageProbeResult, String> {
    Ok(probe_account_usage(&origin, &account_name, config_dir.as_deref()).await)
}

/// F08：**本机** per-account 用量探针 —— 补平 `parity_ledger` 那条 `usage.per-account`。
///
/// # 它与远端那条的关系：**同一个函数，两种 origin**（定框 C1）
///
/// `K-R104` 之前这里还有一条独立的本机执行面（`sh -c <载荷>` 并收 stdout），
/// 以及一条 `#[cfg(not(unix))]` 的诚实降级（Windows 没有 tmux + POSIX shell）。
/// **今天两样都没有了，而这不是把它们删掉，是它们的前提消失了**：
/// 执行不再发生在界面进程里，而是发生在**那台机器的后端**里 ——
/// Windows 上本机后端答的是 `no_tmux`（daemon 侧 `capture_pane` 那一档的真实原因），
/// 比这一侧猜一句「那条路在那个平台上不存在」更硬。
#[tauri::command]
pub async fn account_usage_local(
    account_name: String,
    config_dir: Option<String>,
) -> Result<AccountUsageProbeResult, String> {
    Ok(probe_account_usage(
        crate::inbound_client::LOCAL_ORIGIN,
        &account_name,
        config_dir.as_deref(),
    )
    .await)
}

/// 🔴 **`KR104D2`（`K33` 逐字「所有命令只许有一处，其他都是根据传参来调用」）的登记表。**
///
/// 一行 = 探针编排里的**一步**：`(这一步干什么, 今天由谁做, 它走 daemon 帧面的哪一条命令)`。
///
/// # ★★ `K-R104` 把「今天由谁做」那一列整列翻了面
///
/// `K-R101` 交回时这一列**全是 `monitor`**，头注逐字写着「也就是说 **编排还没有搬走**。
/// 本表是那笔欠账的**住址**，不是它的兑现」。**今天它兑现了**：每一步都是本模块在
/// 一条已经建立的控制通道上发一条 daemon 命令，本模块**一个 shell 字符都不渲染**。
///
/// # 它买到什么、买不到什么 —— **两句都要读**
///
/// **买到的**（由 [`tests::the_probe_is_a_composition_of_frame_commands_the_daemon_already_has`] 钉）：
/// ① 表里点名的每一条命令**今天真的在 daemon 的帧面上**（现打 `inbound::COMMANDS`，
///    不是抄一份名单）；② daemon 的两个命令面上**都没有**一条「整条探针」式的大动作；
/// ③ 这一步是**组合**：owner 列至少四条互不相同的命令，塌成一条就红。
///
/// **买不到的（如实登记，别读大）**：本表不证明「这几步真的按这个顺序被发出去了」——
/// 那一格由 [`tests::the_whole_probe_dials_once_and_talks_many_times`] 用一条**假通道**
/// 记录真实发出的命令序列来断。两条合起来才是 `KR104D2`。
const PROBE_ORCHESTRATION_STEPS: &[(&str, &str, &str)] = &[
    (
        "起会话（daemon 铸名 ＋ 固定几何 -x/-y）",
        "daemon（帧面 `oneshot-session`，monitor 只给 slug 与尺寸）",
        "oneshot-session",
    ),
    (
        "挂自毁看门狗（到点自己死）",
        "daemon（帧面 `oneshot-session` 同一次往返里办完）",
        "oneshot-session",
    ),
    (
        "送启动载荷 / 送 `/usage`",
        "daemon（帧面 `launch` 的 `send-into`，monitor 只给那段文本）",
        "launch",
    ),
    (
        "抓一屏",
        "daemon（帧面 `capture-pane`；**抓几次**由本模块的 `settle` 决定）",
        "capture-pane",
    ),
    (
        "收尾杀会话",
        "daemon（帧面 `kill`，过 §34 Gate 2 + Gate 3）",
        "kill",
    ),
];

/// 「一条命令吃下整条探针」长什么样 —— **`K-R101#§0c` 点名的失效方向的针**。
///
/// 判据不是「名字里有 usage」（`--usage` 今天就在 daemon 上，那是读本地 token 累计的，
/// 与探针无关），而是「**探针**」这件事本身被做成一条命令。
/// ⚠ `K-R104` 起它盖**两个面**（CLI 的 `--x` 与帧面的裸名），因为编排搬上帧面之后，
/// 「顺手做一条大命令」最省事的地方就是帧面。
const WHOLE_PROBE_SUBCOMMAND_NEEDLES: &[&str] = &[
    "--usage-probe",
    "--probe-usage",
    "--account-usage",
    "usage-probe",
    "probe-usage",
    "account-usage",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    // ════════════════════ 假通道（`KR104D2` ③ 的量具）════════════════════

    /// 一条**记账用的**假通道：记下这条通道上真实发出的每一条命令。
    ///
    /// ⚠ 它**不模拟 daemon 的行为** —— 只按命令名回一份形状对的应答，
    /// 好让编排跑得下去。行为归 daemon 侧那几条真 tmux 判据。
    #[derive(Default)]
    struct RecordingChannel {
        /// `(cmd, args)` 按发出的顺序。
        sent: Mutex<Vec<(String, Value)>>,
        /// 抓屏依次回这些内容；用完之后重复最后一条。
        screens: Mutex<Vec<String>>,
        /// 哪条命令要回 `Unsupported`（模拟老 daemon）。
        unsupported: Option<&'static str>,
    }

    impl RecordingChannel {
        fn with_screens(screens: &[&str]) -> Self {
            Self {
                screens: Mutex::new(screens.iter().rev().map(|s| (*s).to_string()).collect()),
                ..Default::default()
            }
        }
        fn cmds(&self) -> Vec<String> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .map(|(c, _)| c.clone())
                .collect()
        }
        fn args_of(&self, cmd: &str) -> Vec<Value> {
            self.sent
                .lock()
                .unwrap()
                .iter()
                .filter(|(c, _)| c == cmd)
                .map(|(_, a)| a.clone())
                .collect()
        }
        fn next_screen(&self) -> String {
            let mut g = self.screens.lock().unwrap();
            if g.len() > 1 {
                g.pop().unwrap_or_default()
            } else {
                g.last().cloned().unwrap_or_default()
            }
        }
    }

    impl ProbeChannel for RecordingChannel {
        fn call<'a>(&'a self, cmd: &'a str, args: Value) -> ProbeStepFuture<'a> {
            self.sent
                .lock()
                .unwrap()
                .push((cmd.to_string(), args.clone()));
            Box::pin(async move {
                if self.unsupported == Some(cmd) {
                    // ★ **走真的分流器造这个错**，不是手搓一个长得像的：
                    //   本条要证的正是「`CallError::Unsupported` 经那唯一一份分流规则之后，
                    //   到用户面前是哪句话」。手搓等于把被测的那一段绕过去。
                    return Err(super::route(
                        &crate::inbound_client::CallError::Unsupported {
                            cmd: cmd.to_string(),
                            offered: vec!["ping".to_string(), "launch".to_string()],
                        },
                    ));
                }
                Ok(match cmd {
                    "oneshot-session" => Some(serde_json::json!({
                        "session": "ccm-oneshot-usage-z-cc",
                        "handle": "$7",
                        "ttlSecs": "30",
                    })),
                    "capture-pane" => Some(serde_json::json!({
                        "name": "ccm-oneshot-usage-z-cc",
                        "screen": self.next_screen(),
                    })),
                    "launch" => Some(serde_json::json!({ "typed": true, "created": false })),
                    "kill" => Some(serde_json::json!({ "killed": true })),
                    _ => None,
                })
            })
        }
    }

    /// 从 `at` 起取一段**字符边界安全**的窗口。
    ///
    /// ⚠ 直接 `&s[at..at+N]` 在这份文件上会 panic —— 它满是中文（`'面'` 占 3 字节）。
    /// 这不是风格问题：panic 出来的报错说的是「不是字符边界」，
    /// 而判据要报的是「那个函数里没有转发参数」，**两句话指的修法完全不同**。
    fn window_from(s: &str, at: usize, len: usize) -> &str {
        let mut end = (at + len).min(s.len());
        while end > at && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[at..end]
    }

    /// 判据用的时间参数：**间隔 0**，别让判据去等 16 秒真 sleep。
    fn fast() -> ProbeTiming {
        ProbeTiming {
            interval_ms: 0,
            still_polls: 2,
            startup_max_polls: 4,
            render_max_polls: 4,
            ..ProbeTiming::PRODUCTION
        }
    }

    // ════════════════════ `KR104D2` ════════════════════

    /// ★★ `KR104D2` 的正题：**整条编排恰好拨一次号，而在那一条通道上往返很多次。**
    ///
    /// # 🔴 判的是「几次连接」，不是「花了多久」
    ///
    /// 件文件逐字点名了这一条：耗时是环境噪声。这里数的是两个**整数**：
    /// 拨号次数（必须恰好 1）与往返次数（必须显著大于 1）。
    ///
    /// # 它逮的那一形
    ///
    /// `§0a` 的阻断：走 CLI 面**每抓一屏一次 SSH 握手** ⇒ 上限 12+20 轮 ⇒ 最多 36 次握手
    /// vs `EXEC_TIMEOUT_SECS = 25` ⇒ 结构上超时。把 `dial()` 挪进抓屏那个循环里
    /// （= 退回 CLI 面的形状），本条第一格当场红。
    #[tokio::test]
    async fn the_whole_probe_dials_once_and_talks_many_times() {
        let dials = Arc::new(AtomicUsize::new(0));
        // 前两屏相同但等于基线 ⇒ 不算稳定；之后换一屏并静止 ⇒ 稳定。
        let ch: Arc<RecordingChannel> =
            Arc::new(RecordingChannel::with_screens(&["", "", "面板出来了"]));
        let mk = ch.clone();
        let counter = dials.clone();
        let dial = move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(mk.clone() as Arc<dyn ProbeChannel>)
        };
        let r = probe_over_frames(&dial, "z", "unset X; claude", fast()).await;

        assert_eq!(
            dials.load(Ordering::SeqCst),
            1,
            "整条编排拨了 {} 次号 —— `KR104D2` 要的正是「一条连接上多次往返」。\n\
             每抓一屏拨一次 = 退回 CLI 面那个形状（现打上限 12+20 轮 ⇒ 最多 36 次握手\n\
             vs EXEC_TIMEOUT_SECS = 25 ⇒ 结构上超时）。",
            dials.load(Ordering::SeqCst)
        );
        let cmds = ch.cmds();
        assert!(
            cmds.len() > 6,
            "一条通道上只往返了 {} 次 —— 编排塌了，本条下面几格在空转：{cmds:?}",
            cmds.len()
        );
        // ★ 顺序按**数据**断（真实发出的命令序列），不是扫源码。
        assert_eq!(
            cmds.first().map(String::as_str),
            Some("oneshot-session"),
            "第一条不是起会话：{cmds:?}"
        );
        assert_eq!(
            cmds.last().map(String::as_str),
            Some("kill"),
            "最后一条不是收尾杀会话 —— 看门狗是保险丝不是清场机制：{cmds:?}"
        );
        assert_eq!(
            cmds.iter().filter(|c| *c == "launch").count(),
            2,
            "送键不是恰好两次（启动载荷 ＋ `/usage`）：{cmds:?}"
        );
        assert!(
            cmds.iter().filter(|c| *c == "capture-pane").count() >= 4,
            "抓屏次数太少 —— 稳定轮询没跑起来，本条量不到「一条连接上多次往返」：{cmds:?}"
        );
        // 载荷与 `/usage` 都真的送出去了，而且顺序不能反。
        let sent: Vec<String> = ch
            .args_of("launch")
            .iter()
            .map(|a| {
                a.get("payload")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert_eq!(sent.len(), 2, "送键实参取不到：{sent:?}");
        assert!(
            sent[0].contains("claude"),
            "第一次送的不是启动载荷：{sent:?}"
        );
        assert_eq!(
            sent[1], USAGE_SLASH_COMMAND,
            "第二次送的不是 `/usage`：{sent:?}"
        );
        // 几何真的跟着起会话那一次发出去了（不给就是 80x24，`/usage` 那张表会被折断）。
        let one = ch.args_of("oneshot-session");
        assert_eq!(one.len(), 1, "起会话不是恰好一次：{one:?}");
        assert_eq!(one[0].get("width").and_then(|v| v.as_str()), Some("200"));
        assert_eq!(one[0].get("height").and_then(|v| v.as_str()), Some("50"));
        assert_eq!(
            one[0].get("slug").and_then(|v| v.as_str()),
            Some("usage-z"),
            "slug 形状变了 —— daemon 会在它前后加 `ccm-oneshot-` 与 `-cc`"
        );
        // 抓回来的那一屏原样交出去（零解析，`R59`）。
        assert!(r.captured, "应当抓到了：{r:?}");
        assert_eq!(
            r.raw.as_deref(),
            Some("面板出来了"),
            "原文没被原样带出来：{r:?}"
        );
        assert!(r.error.is_none());
    }

    /// ★ `KR104D2` ①「编排退回 CLI 面 ⇒ 红」的**零命中守卫**。
    ///
    /// 本模块的生产段里**不许再出现**任何渲染 shell 的痕迹。
    /// 这与上面那条互补：那条断「今天真的走帧面」，这条断「盘上没有第二条路」——
    /// 同 `tmux_daemon_gate_guard` 的回潮闸（`K-R72` 那两条）的形状。
    ///
    /// ⚠ **针在测试段、扫的是生产段**（`production_code` 剥掉测试段）⇒ 本条不会
    /// 在自己的文本里找到自己。
    #[test]
    fn the_probe_never_falls_back_to_rendering_a_shell_string() {
        let prod = guard_core::production_code(include_str!("account_usage.rs"));
        assert!(
            prod.len() > 3_000,
            "剥完只剩 {} 字节 —— 剥法坏了，本条在空转",
            prod.len()
        );
        // 运行时拼，免得命中本文件自己的说明文字。
        let banned = [
            (
                format!("connect_and_exec{}", "_cmd"),
                "一次性 SSH exec —— 那就是每抓一屏一次握手",
            ),
            (
                format!("shell{}", "_quote"),
                "渲染 shell 串才需要引用；帧面 argv 直传",
            ),
            (
                format!("tmux {}", "new-session"),
                "建会话归 daemon 的 `oneshot-session`",
            ),
            (
                format!("tmux {}", "send-keys"),
                "送键归 daemon 的 `launch send-into`",
            ),
            (
                format!("tmux {}", "capture-pane"),
                "抓屏归 daemon 的 `capture-pane`",
            ),
            (
                format!("tmux {}", "kill-session"),
                "杀会话归 daemon 的 `kill`",
            ),
            (
                format!("set{}", "sid"),
                "看门狗归 daemon 的 `oneshot-session`",
            ),
            (
                format!("NO{}", "_TMUX"),
                "那是 shell 串时代的哨兵；今天 daemon 回 `no_tmux` 码",
            ),
        ];
        let hits: Vec<&str> = banned
            .iter()
            .filter(|(n, _)| prod.contains(n.as_str()))
            .map(|(_, why)| *why)
            .collect();
        assert!(
            hits.is_empty(),
            "本模块的生产段里又长出了渲染 shell 的痕迹：{hits:?}\n\
             `KR104D2` ①：编排退回 CLI 面就是红。整条编排今天只许由帧面命令组合而成。"
        );
        // ★ 反向自检：这把尺子真的会咬人 —— 不然上面那一格是空真。
        let synthetic = format!("let c = format!(\"tmux {} -t x\");", "capture-pane");
        assert!(
            banned.iter().any(|(n, _)| synthetic.contains(n.as_str())),
            "针认不出一条摆在面前的抓屏 shell 串 —— 上面那一格是摆设"
        );
    }

    // ════════════════════ `KR104D3` ════════════════════

    /// ★★ `KR104D3` ③：**老 daemon 说得出「我不认得」** —— 不静默失败、不挂住。
    ///
    /// 三格：① 失败（不是假装成功）· ② 话里指出「重装这台机器的后端」
    /// · ③ **当场返回**（假通道在这一档不 await 任何东西；挂住的话本条会超时红）。
    #[tokio::test]
    async fn an_old_backend_says_it_does_not_know_the_command() {
        for (missing, step) in [("oneshot-session", "起会话"), ("capture-pane", "抓屏")] {
            let ch = Arc::new(RecordingChannel {
                unsupported: Some(missing),
                screens: Mutex::new(vec!["x".to_string()]),
                ..Default::default()
            });
            let mk = ch.clone();
            let dial = move || Ok(mk.clone() as Arc<dyn ProbeChannel>);
            let r = probe_over_frames(&dial, "z", "claude", fast()).await;
            assert!(
                !r.captured,
                "`{missing}`（{step}）不被支持时居然回了 captured=true —— 那是假装成功：{r:?}"
            );
            let msg = r.error.clone().unwrap_or_default();
            assert!(
                msg.contains("重装"),
                "`{missing}` 不被支持时那句话没告诉用户该怎么办（重装这台机器的后端）：{msg}"
            );
            assert!(
                msg.contains(missing),
                "那句话没点名是哪条命令不被支持：{msg}"
            );
            assert!(
                msg.contains("旧版本"),
                "那句话不是从分流器来的 —— 分流器对这一档的原话里带「多半是旧版本」：{msg}"
            );
        }
    }

    /// ★ `KR104D3` ③ 的另一半：**发之前就拒**，一个字节都不发。
    ///
    /// 这一格由 `inbound_client` 的 `accepts` 买（`hello.commands` 里没有 ⇒ 直接
    /// `CallError::Unsupported`）。本条钉的是**本模块把它接住并分成单独一档**——
    /// 压进 `Other` 那一档，用户看到的就只是一句「远端拒绝」，而真相是「后端太旧」。
    #[test]
    fn the_unsupported_case_is_a_bucket_of_its_own() {
        let old = describe(&ProbeStepError::NothingWasSent("原话".into()));
        let other = describe(&ProbeStepError::Refused("原话".into()));
        assert_ne!(old, other, "两档讲出来的话一样 —— 那就等于没分档");
        assert!(old.contains("重装"), "老后端那一档没说该怎么办：{old}");
        assert_eq!(other, "原话", "其它失败那一档不该被加工");
        // 反向：`ProbeStepError` 只有这两档，加第三档时本条会红（编译期）。
        for e in [
            ProbeStepError::NothingWasSent("m".into()),
            ProbeStepError::Refused("m".into()),
        ] {
            assert_eq!(e.message(), "m");
        }
        // ★★ **两档真的由那唯一一份分流规则分的**，不是本模块自己判的。
        //    换一条 `CallError` 进去，档位必须跟着分流器的裁定走。
        use crate::inbound_client::CallError;
        assert!(
            matches!(
                super::route(&CallError::Unsupported {
                    cmd: "capture-pane".into(),
                    offered: vec![],
                }),
                ProbeStepError::NothingWasSent(_)
            ),
            "`Unsupported` 没落进「一个字节都没发出去」那一档"
        );
        assert!(
            matches!(
                super::route(&CallError::Remote {
                    code: "wrong_owner".into(),
                    message: "sid=".into(),
                }),
                ProbeStepError::Refused(_)
            ),
            "`Remote` 落错档了 —— 那是「daemon 说过话了」，不许被当成没发出去"
        );
    }

    // ════════════════════ `KR104D1` / `K33`：编排是组合 ════════════════════

    /// daemon 那棵树的 `inbound.rs` 原文（现打，不抄名单）。
    fn daemon_inbound_rs() -> String {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .join("remote-daemon-proto/src/inbound.rs");
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}: {e}", p.display()))
    }

    /// daemon `main.rs` 的 `SUBCOMMANDS`（CLI 那一面，现算）。
    fn daemon_subcommands() -> Vec<String> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .join("remote-daemon-proto/src/main.rs");
        let src =
            std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}: {e}", p.display()));
        let head = "const SUBCOMMANDS: &[&str] = &[";
        let at = src
            .find(head)
            .expect("daemon main.rs 里找不到 SUBCOMMANDS —— 尺子的作用域没了");
        let body_start = at + head.len();
        let body_len = src[body_start..]
            .find("];")
            .expect("SUBCOMMANDS 数组没有收尾 `];`");
        let body = &src[body_start..body_start + body_len];
        let mut out = Vec::new();
        for line in body.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    out.push(rest[..end].to_string());
                }
            }
        }
        out
    }

    /// daemon 帧面今天认哪几条命令（现算自 `inbound::COMMANDS` 那个数组体）。
    fn daemon_frame_commands() -> Vec<String> {
        let src = daemon_inbound_rs();
        let head = "pub const COMMANDS: &[&str] = &[";
        let at = src
            .find(head)
            .expect("daemon inbound.rs 里找不到 COMMANDS —— 尺子的作用域没了");
        let body_start = at + head.len();
        let body_len = src[body_start..]
            .find("];")
            .expect("COMMANDS 数组没有收尾 `];`");
        let body = &src[body_start..body_start + body_len];
        body.split('"')
            .skip(1)
            .step_by(2)
            .map(|s| s.to_string())
            .collect()
    }

    /// ★★ **`KR104D2`：探针编排是既有帧面命令的组合，而 daemon 上没有一条「整条探针」。**
    ///
    /// 🔴 与 `K-R101` 那一版的差别**不是措辞**：那一版对的是 CLI 那一面
    /// （`SUBCOMMANDS`，`--x`），因为那时编排根本没走帧面；今天对的是**帧面**
    /// （`inbound::COMMANDS`），因为编排真的在那上面跑。
    /// ⚠ 「整条探针」那根针**两个面都扫** —— 编排搬上帧面之后，
    /// 「顺手做一条大命令」最省事的地方就是帧面。
    #[test]
    fn the_probe_is_a_composition_of_frame_commands_the_daemon_already_has() {
        let frame = daemon_frame_commands();
        // 抽取器自检：取不到就拒跑，别零命中地绿。
        assert!(
            frame.len() >= 8,
            "只从 daemon 帧面取到 {} 条命令 —— 取数坏了，下面几条会零命中地绿：{frame:?}",
            frame.len()
        );
        assert!(
            frame.iter().any(|s| s == "launch") && frame.iter().any(|s| s == "kill"),
            "取到的帧面命令里连 `launch`/`kill` 都没有 —— 取数落在了别的数组上：{frame:?}"
        );

        // ① 每一步点名的命令都得在 daemon 的**帧面**上。
        for (step, who, owner) in PROBE_ORCHESTRATION_STEPS {
            assert!(
                frame.iter().any(|s| s == owner),
                "探针这一步「{step}」（{who}）点名的帧面命令 `{owner}` \
                 不在 `inbound::COMMANDS` 上 —— 要么它被删/改名了，要么本表抄错了。\
                 daemon 帧面今天认的：{frame:?}"
            );
        }

        // ② 组合，不是一个大动作。
        let mut owners: Vec<&str> = PROBE_ORCHESTRATION_STEPS
            .iter()
            .map(|(_, _, o)| *o)
            .collect();
        owners.sort_unstable();
        owners.dedup();
        assert!(
            owners.len() >= 4,
            "探针编排塌成了 {n} 条命令（应 ≥4：起会话 · 送键 · 抓屏 · 杀会话）—— \
             `K33` 逐字「所有命令只许有一处，其他都是根据传参来调用」，\
             一条命令吃下整条编排就是 `K-R101#§0c` 点名的那个失效方向。今天的 owner：{owners:?}",
            n = owners.len()
        );

        // ③ 🔴 **「今天由谁做」那一列整列必须是 daemon** —— `K-R101` 交回时它全是 monitor。
        for (step, who, _) in PROBE_ORCHESTRATION_STEPS {
            assert!(
                who.starts_with("daemon"),
                "「{step}」这一步的做事方又变回 `{who}` 了 —— 编排退回 monitor 就是 `KR104D2` ①"
            );
        }

        // ④ 两个面都不许长出一条「整条探针」。
        let mut both = frame.clone();
        both.extend(daemon_subcommands());
        for needle in WHOLE_PROBE_SUBCOMMAND_NEEDLES {
            assert!(
                !both.iter().any(|s| s == needle),
                "daemon 的命令面上出现了 `{needle}` —— 那是把一份 shell 串换成一份 Rust 串：\
                 命令是少了一处，**编排仍然只有一份实现在替调用方做决定**。\
                 要新增**小原语**没问题，要新增「一条命令吃下整条编排」得回 `K33` 重裁。"
            );
        }
    }

    /// ★ 反向自检：上面那条真的逮得住 —— 合成一份长出 `usage-probe` 的命令面必须被认出来。
    ///
    /// ⚠ 没有这一条，`④` 那个循环在「取数坏了 ⇒ 集合为空」时同样全过（空真）。
    #[test]
    fn the_whole_probe_needle_actually_catches_one() {
        for synthetic in ["--usage-probe", "usage-probe", "account-usage"] {
            assert!(
                WHOLE_PROBE_SUBCOMMAND_NEEDLES.contains(&synthetic),
                "针认不出一条摆在面前的 `{synthetic}` —— 那条判据是摆设"
            );
        }
        // 反方向：今天这几条正当的小原语不许被针误伤。
        for ok in [
            "capture-pane",
            "oneshot-session",
            "kill",
            "launch",
            "--capture-pane",
            "--oneshot-session",
            "--usage",
        ] {
            assert!(
                !WHOLE_PROBE_SUBCOMMAND_NEEDLES.contains(&ok),
                "`{ok}` 被针误伤了 —— 它是既有的小原语，本条不禁止小原语"
            );
        }
    }

    /// `e2e/usage-probe-acceptance.sh` 的**输入源**（`K-R104` 起换成帧行）。
    ///
    /// 惯例来自 `e2e/inbound-daemon-frames.sh` 那条逐字：喂进去的行必须是
    /// **monitor 自己的编码器的产物**，否则那套 e2e 只证明「daemon 认得我手写的 JSON」，
    /// 证明不了「monitor 真会发的那种 JSON」。
    ///
    /// 🔴 上一版这条叫「打印真实 `build_usage_probe_cmd` 产出的命令串」——
    /// **那条 shell 串今天不存在了**，所以输入源跟着换成这四类帧行。
    ///
    /// ⚠ 会话名由 daemon 在运行期铸，脚本拿不到编译期的值 ⇒ 这里印
    /// [`E2E_SESSION_PLACEHOLDER`]，由脚本原样替换。**只有那一个 token 是占位的**，
    /// 其余每个字节都是真编码器产的。
    /// `#[ignore]` —— 只由该脚本用
    /// `cargo test --lib -- --ignored --nocapture emit_usage_probe_frames_for_e2e` 触发。
    #[test]
    #[ignore]
    fn emit_usage_probe_frames_for_e2e() {
        use crate::inbound_client::encode_request;
        let t = ProbeTiming::PRODUCTION;
        let sess = E2E_SESSION_PLACEHOLDER;
        // ⚠ **看门狗按场景分开**：正常路径必须用生产值（`t.watchdog_secs`），
        //   短看门狗只给专门测看门狗的那个场景（同上一版那条纪律，逐字保留理由）：
        //   让正常路径的看门狗短于自然完成时间，会把一个本该测编排的场景变成在测竞态。
        const E2E_SHORT_WATCHDOG_SECS: u32 = 2;
        let lines: &[(&str, String)] = &[
            (
                "oneshot",
                encode_request(
                    "e2e-up-1",
                    "oneshot-session",
                    &oneshot_session_args("usage-e2e", t.watchdog_secs, Some((t.cols, t.rows))),
                ),
            ),
            (
                "oneshot-shortdog",
                encode_request(
                    "e2e-up-2",
                    "oneshot-session",
                    &oneshot_session_args(
                        "usage-dog",
                        E2E_SHORT_WATCHDOG_SECS,
                        Some((t.cols, t.rows)),
                    ),
                ),
            ),
            (
                "send-payload",
                encode_request(
                    "e2e-up-3",
                    "launch",
                    &launch_args(
                        "send-into",
                        sess,
                        "unset CLAUDECODE; FAKECLAUDE",
                        None,
                        None,
                        Default::default(),
                    ),
                ),
            ),
            (
                "send-usage",
                encode_request(
                    "e2e-up-4",
                    "launch",
                    &launch_args(
                        "send-into",
                        sess,
                        USAGE_SLASH_COMMAND,
                        None,
                        None,
                        Default::default(),
                    ),
                ),
            ),
            (
                "capture",
                encode_request("e2e-up-5", "capture-pane", &capture_pane_args(sess)),
            ),
            (
                "kill",
                encode_request("e2e-up-6", "kill", &serde_json::json!({ "name": sess })),
            ),
        ];
        for (tag, line) in lines {
            // `encode_request` 自带行尾换行 —— 这里去掉，由脚本按 TSV 逐行读。
            println!("{tag}\t{}", line.trim_end());
        }
    }

    // ════════════════════ C13 / D3：monitor 不许自己建 tmux ════════════════

    /// ★★ **monitor 的 Rust 生产段里，一处都不许自己建 tmux 会话**〔`K-R104` 翻面〕。
    ///
    /// # 它翻了面，而翻面本身就是本件的交付
    ///
    /// 上一版是**登记制**：`D3_EXCEPTIONS` 里逐条写「为什么这一处可以绕开 C13/D3」，
    /// 而表里**只有一条** —— `account_usage.rs`（它自己拼 `tmux new-session -d -s
    /// ccm-usage-<slug>`，理由写的是「无头测量而非用户会话」）。
    /// `K-R104` 把那条编排整条搬上帧面之后，**那一处没有了** ⇒ 例外表空了
    /// ⇒ 登记制退化成空真（`[] == []` 照样绿）。
    ///
    /// ⇒ 换成**零命中守卫**：这条性质今天是「一处都没有」，那就直接钉它。
    /// **这比登记制强一格**，而且它是 `D3` 逐字要的那句话
    /// （「D3 的例外只有一条 —— daemon 在**自己管的** tmux 容器里起会话」）。
    ///
    /// ⚠ **发现口径不在这里各写一份**（E3）：问唯一那个家
    /// `daemon_kill::creation_detect::creates_a_session`。
    /// 08-08 实测过两张表口径不一致的后果，那一次的修法就是这条纪律。
    #[test]
    fn monitor_never_creates_a_tmux_session_of_its_own() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0usize;
        let mut found: Vec<String> = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir)
                .expect("读不到 monitor src")
                .flatten()
            {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    scanned += 1;
                    let src = std::fs::read_to_string(&p).unwrap_or_default();
                    let prod = guard_core::production_code(&src);
                    if crate::backend::control::daemon_kill::creation_detect::creates_a_session(
                        &prod,
                    ) {
                        found.push(p.file_name().unwrap().to_string_lossy().into_owned());
                    }
                }
            }
        }
        // 人群自检：遍历真的走到了东西（不然「零命中」是遍历坏了）。
        assert!(
            scanned >= 60,
            "只扫到 {scanned} 个 `.rs` —— 遍历坏了，下面那格是零命中地绿"
        );
        assert!(
            found.is_empty(),
            "monitor 的 Rust 生产段里又有人自己建 tmux 会话了：{found:?}\n\
             ★ **C13**：最后那次 `exec` 必须在用户那个终端进程里；\n\
             **D3 的例外只有一条** —— daemon 在**自己管的**容器里起会话。\n\
             要建会话就发帧面的 `oneshot-session` / `launch`，别在界面进程里拼 tmux。\n\
             真要开例外，回来把这条零命中守卫改回登记制，并写下「为什么它正当」。"
        );
        // ★ 反向自检：这把尺子真的会咬人 —— 不然上面那一格是空真。
        let synthetic = format!("    let c = \"tmux new-{} -d -s x\";", "session");
        assert!(
            crate::backend::control::daemon_kill::creation_detect::creates_a_session(&synthetic),
            "口径认不出一条摆在面前的建会话语句 —— 上面那一格是摆设"
        );
    }

    // ════════════════════ 载荷（`K-R104` 一个字节没动）════════════════════

    #[test]
    fn slugify_keeps_only_safe_chars() {
        assert_eq!(slugify_account_name("z"), "z");
        assert_eq!(slugify_account_name("my-account_2"), "my-account_2");
        assert_eq!(slugify_account_name("a b;rm -rf"), "abrm-rf");
        assert_eq!(slugify_account_name("日本語"), "x"); // 全非 ASCII 字母数字 → 兜底
        assert_eq!(slugify_account_name(""), "x");
        // 长度截断：防止一个异常长的账号名把会话名撑得过长。
        let long = "a".repeat(100);
        assert_eq!(slugify_account_name(&long).len(), 32);
    }

    /// ★ U8c-2a（代码审计 R1–R4 的收口）：**两态各自的载荷逐字节钉住**。
    ///
    /// 这条杀掉的是「载荷编译搬进 Rust 之后接线没人管」那一类：审计实测
    /// 「只清一个嵌套 env 键」「换掉启动器」两个变异在全绿门禁下存活过。
    ///
    /// ⚠ **键序与 TS `AGENT_PROFILE.nestedEnvVars` 同序是刻意的**（见
    /// `adapter/claude_code.rs::CLAUDE_NESTED_ENV` 头注）。
    #[test]
    fn probe_payload_is_byte_exact_for_both_account_states() {
        const NESTED: &str =
            "unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; ";
        assert_eq!(
            probe_payload_for(Some("/h/.claude-accts/z")).unwrap(),
            format!("export CLAUDE_CONFIG_DIR='/h/.claude-accts/z'; {NESTED}claude")
        );
        let zero = probe_payload_for(None).unwrap();
        assert_eq!(zero, format!("unset CLAUDE_CONFIG_DIR; {NESTED}claude"));
        // ★ 最要紧的一条：账号 0 **绝不**退化成裸载荷（那会继承远端 rc 里的默认号 = 静默串号）。
        assert!(
            !zero.contains("export CLAUDE_CONFIG_DIR="),
            "账号 0 竟带上了 export：\n{zero}"
        );
    }

    /// 空 / 坏 configDir 是坏数据 ⇒ **一个字节都不发**（连号都不拨）。
    #[tokio::test]
    async fn bad_config_dir_never_dials_and_never_sends() {
        let dials = AtomicUsize::new(0);
        assert!(probe_payload_for(Some("")).is_err());
        assert!(probe_payload_for(Some("/h/a;rm -rf /")).is_err());
        // 反向自检：合法输入必须构造得出来，否则上面两条是空转。
        assert!(probe_payload_for(Some("/h/.claude-accts/z")).is_ok());
        // 接线：坏数据经 tauri 命令进来时，`probe_account_usage` 在拨号之前就返回。
        let r = probe_account_usage("<no-such-origin>", "z", Some("")).await;
        assert!(!r.captured, "坏 configDir 竟报了成功：{r:?}");
        assert_eq!(dials.load(Ordering::SeqCst), 0);
    }

    /// ★ **本机与远端是同一条路** —— 定框 C1「一份代码两种承载」的直接判据。
    ///
    /// `K-R104` 之前这条比的是**两条命令串**（两个执行面各渲一份）。
    /// 今天连那半都不存在：两个 tauri 命令**调的是同一个函数**，只差一个 origin。
    /// ⇒ 判据跟着变形：钉「两条路都只经 `probe_account_usage`，而它只有一处」。
    #[test]
    fn the_local_and_remote_probes_are_the_same_code_path() {
        let prod = guard_core::production_code(include_str!("account_usage.rs"));
        // 调用点（排掉定义那一处）。
        let calls = prod
            .match_indices("probe_account_usage(")
            .filter(|(i, _)| !prod[..*i].trim_end().ends_with("async fn"))
            .count();
        assert_eq!(
            calls, 2,
            "`probe_account_usage` 的生产调用点不是恰好两个（实得 {calls}）—— \
             今天只有两条合法承载：`account_usage`（给 origin）与 `account_usage_local`\
             （给 `<local>`）。多一个 = 有第三条路；少一个 = 有人绕过了这个入口。"
        );
        for owner in [
            "pub async fn account_usage(",
            "pub async fn account_usage_local(",
        ] {
            let at = prod
                .find(owner)
                .unwrap_or_else(|| panic!("生产段里找不到 `{owner}` —— 承载少了一个"));
            let body = window_from(&prod, at, 900);
            assert!(
                body.contains("probe_account_usage("),
                "`{owner}` 里没有调 `probe_account_usage` —— 它自己搓了一条编排？\n\
                 那就破了 C1「一份代码两种承载」：两侧会各自漂。"
            );
        }
        // 🔴 本机那条必须显式给 `<local>`，不许悄悄退化成「不给 origin」。
        let at = prod
            .find("pub async fn account_usage_local(")
            .expect("找不到本机那条");
        assert!(
            window_from(&prod, at, 900).contains("LOCAL_ORIGIN"),
            "本机那条没有显式给 `<local>` origin"
        );
    }

    /// ★ 接线：`account_usage` 真的把它收到的 `config_dir` 转发下去。
    ///
    /// 恒当账号 0（`None`）或写死别的号 = **静默串号**，而其余全部判据都会保持绿。
    /// ⚠ 它是**约定不是事实**（源码形态守卫）：挡得住「顺手把参数换成常量」，
    /// 挡不住「换个名字继续错」。**比没有强，别读成证明。**
    #[test]
    fn account_usage_actually_forwards_the_config_dir_it_received() {
        let prod = guard_core::production_code(include_str!("account_usage.rs"));
        for owner in [
            "pub async fn account_usage(",
            "pub async fn account_usage_local(",
        ] {
            let at = prod
                .find(owner)
                .unwrap_or_else(|| panic!("找不到 `{owner}`"));
            let body = window_from(&prod, at, 900);
            assert!(
                body.contains("config_dir.as_deref()"),
                "`{owner}` 没有把它收到的 `config_dir` 转发下去 —— \
                 恒当账号 0 或写死别的号 = 静默串号"
            );
        }
    }

    // ════════════════════ 时间预算 ════════════════════

    #[test]
    fn time_budget_ordering_holds() {
        // 三层超时必须**严格套娃**，否则外层会把内层掐死、让被保护的逻辑根本跑不完。
        // ★ 这条测试是被真事逼出来的：E42 之前那条 e2e 输入源给三个场景
        // 统一发 3s 看门狗，旧判据下自然完成约 2s 侥幸躲过；判据一改成"静止 3s"，
        // 正常路径的会话就在轮询跑完前被自己的看门狗杀掉。
        let t = ProbeTiming::PRODUCTION;
        let poll_budget_ms = t.interval_ms * u64::from(t.startup_max_polls + t.render_max_polls);
        let exec_ms = EXEC_TIMEOUT_SECS * 1000;
        let watchdog_ms = u64::from(t.watchdog_secs) * 1000;

        // ① 两段轮询跑满也要装得进整条编排的硬超时，且留出余量给往返 + 建/清会话。
        assert!(
            poll_budget_ms + poll_budget_ms / 4 < exec_ms,
            "轮询预算 {poll_budget_ms}ms(+25% 余量) 撑破了硬超时 {exec_ms}ms"
        );
        // ② 外层先放弃，看门狗后收尸——反过来会留下无人清理的探针会话。
        assert!(
            exec_ms < watchdog_ms,
            "硬超时 {exec_ms}ms 必须早于看门狗 {watchdog_ms}ms，否则会话会被提前杀掉"
        );
        // ③ 判定"静止"所需的时长必须显著短于**单段**上限。
        let still_ms = t.interval_ms * u64::from(t.still_polls);
        for (name, cap) in [
            ("startup", t.startup_max_polls),
            ("render", t.render_max_polls),
        ] {
            let cap_ms = t.interval_ms * u64::from(cap);
            assert!(
                still_ms * 2 <= cap_ms,
                "{name} 段上限 {cap_ms}ms 容不下两倍静止时长 {still_ms}ms：判据几乎必然打不中"
            );
        }
        // ④ 🔴 **单条命令的期限必须装得进整条预算**，否则一条卡住的命令就把总闸吃光。
        assert!(
            CALL_TIMEOUT_SECS < EXEC_TIMEOUT_SECS,
            "单条命令期限 {CALL_TIMEOUT_SECS}s 不小于整条编排的 {EXEC_TIMEOUT_SECS}s"
        );
    }

    /// ★ 稳定判据的**行为**：回显自己不算数，画面要真的变过 ＋ 静止够久。
    ///
    /// `K-R104` 之前这一格只钉**结构**（扫那条 shell 串里有没有那两半）。
    /// 编排搬进 Rust 之后它变成一个纯粹的行为判据：喂一串屏，看它在第几屏收手。
    #[tokio::test]
    async fn settling_needs_the_screen_to_change_and_then_hold_still() {
        // 基线 = "base"。前三屏还是 base（= 什么都没发生）⇒ 不许收手。
        let ch = RecordingChannel::with_screens(&["base", "base", "新面板", "新面板", "新面板"]);
        let t = ProbeTiming {
            interval_ms: 0,
            still_polls: 2,
            ..ProbeTiming::PRODUCTION
        };
        let got = settle(&ch, "s", "base", 20, t).await.expect("不该失败");
        assert_eq!(got, "新面板", "收手时拿到的不是稳定之后那一屏：{got:?}");
        // 抓了几次：base·base·新·新·新 —— 第 5 次时 same=2 且 != base ⇒ 收手。
        assert_eq!(
            ch.cmds().iter().filter(|c| *c == "capture-pane").count(),
            5,
            "收手的时机不对：{:?}",
            ch.cmds()
        );

        // ★ 反向：画面**一直是基线**（键没送到）⇒ 必须一路等到上限，不许早收手。
        let ch2 = RecordingChannel::with_screens(&["base"]);
        let got2 = settle(&ch2, "s", "base", 3, t).await.expect("不该失败");
        assert_eq!(got2, "base");
        assert_eq!(
            ch2.cmds().iter().filter(|c| *c == "capture-pane").count(),
            3,
            "画面没变过却提前收手了 —— 「相对基线变过」那一半失效了"
        );
    }

    /// ★ **空屏是合法的成功**（`KR101D1` ③ 明令禁止把它判成失败）。
    #[tokio::test]
    async fn an_empty_screen_is_still_a_successful_capture() {
        let ch = Arc::new(RecordingChannel::with_screens(&[""]));
        let mk = ch.clone();
        let dial = move || Ok(mk.clone() as Arc<dyn ProbeChannel>);
        let r = probe_over_frames(&dial, "z", "claude", fast()).await;
        assert!(r.captured, "空屏被判成了失败：{r:?}");
        assert_eq!(r.raw.as_deref(), Some(""));
        assert!(r.error.is_none());
    }

    /// ★ 没通道 ⇒ **明确失败**，不换条路悄悄做掉（同 `tmux.rs::no_channel_message` 那条纪律）。
    #[tokio::test]
    async fn no_channel_is_an_explicit_failure() {
        let dial = || Err("[<local>] 没有可用的控制通道".to_string());
        let r = probe_over_frames(&dial, "z", "claude", fast()).await;
        assert!(!r.captured);
        assert!(
            r.error.as_deref().is_some_and(|e| e.contains("控制通道")),
            "没通道时的话没说清是通道不在：{r:?}"
        );
    }
}
