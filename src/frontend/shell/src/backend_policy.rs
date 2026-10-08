//! P2s（定框 `C8`）：**每台机一份后端策略**。
//!
//! 今天只有一条策略：**monitor 退出时要不要主动结束这台机的 backend**，默认 **false（不主动结束）**。
//!
//! # 〔条 66〕那个值**不住 monitor** —— 本模块只是去问、去交写
//!
//! 搬家前：持久化归前端（monitor 的 `config.json`），本模块持有一张进程内的**生效值表**，
//! 由前端在启动时与改动时推进来，退出臂读那张表。那张表正是 `E2` 点名的「启动时快照」，
//! 而「值住看客这一侧」正是 `§3.3b ①` 那个反例的成因（C 拿自己那份默认值悄悄改掉 B 的行为）。
//! ⇒ 值搬到**后端所在那台机器**上（后端 `control/exit_policy.rs`，只有后端写）。本模块今天只剩三件：
//! ① [`backend_exit_policy`] —— 问那台机器（界面画勾用）；
//! ② [`set_backend_exit_policy`] —— 交那台机器写（界面改勾用），回写完读回的那一份；
//! ③ [`kill_on_exit_now`] —— monitor 退出臂**在决定那一刻现问**本机后端一次，按答案收它自己起的两个子进程。
//! 三件都经同一个发送口 [`exit_policy_call`]（走分流器，登记在 `backend_route_tests::SENDERS`）。
//! 进程内**没有**这个值的任何副本 —— 那张表连同推送它的那条 tauri 命令一起删了。
//!
//! # ⚠ 「不主动结束」到底等不等于「继续跑」—— **今天要看它有没有真脱离**〔`K-P1` 08-26 翻面〕
//!
//! **翻面之前**（一直到 `K-P1`）：backend 是**纯 stdio 子进程**，monitor 一退读端就断，
//! 它在 **153 毫秒**内自己 broken-pipe 退出（实测，08-11 P2s §0a）。所以那时本策略的真实语义是
//! **「立刻杀」与「让它自己死」之差**，不是「后台常驻」，而 `P2s-Y5` 据此立了一条
//! **无条件**禁令（原句已从这四处删干净，由
//! [`tests::the_unconditional_ban_is_gone_from_all_four_homes`] 钉着；要看原文去翻
//! `control-parity` 的 `P2s-Y5`）。
//!
//! ⚠ **这里刻意不把那句原文抄下来** —— 抄下来它就会命中那条判据自己，
//! 而「为了不命中判据把引号换成另一种」是**绕**，不是治。同族的自指陷阱本仓记过多次。
//!
//! **今天那条禁令的前提只在一半的情况下成立了。**`K-P1` 给了后端一个监听口，
//! 并让它在 Linux 上**真脱离**（`process_group(0)` + stdio 全 null + 协议改走那个口）
//! ⇒ 「勾掉开关」在那一支上**真的**是「继续跑」。
//!
//! ⇒ 禁令换成**按状态分档**（`K-P1 KPY4`，由 `tests/frontend/ui/settings/backend-section.vitest.ts` 机检）：
//! 用户可见的那四句话有**唯一一个家**（文案表 `backendPolicy.exit.*`；TS 那侧 `src/frontend/ui/backend-policy.ts` 按名字取）。
//!
//! ⚠⚠ **「无人监护」这半不许省**（原话：
//! 「第一档必须在 UI 上如实说『继续跑，无人监护』，这是本裁定的一半，
//! **不许只做常驻不做这句话**」）。脱离之后**没有任何东西在监护它** ——
//! 崩了不会自动重起（自愈整件摘出去成了 `K-P3`）。那是**如实登记的降级**，不是漏洞；
//! 而如实登记的意思就是**说出口**，不是写在一条源码注释里。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::backend_route::{no_channel, route_call_error, Routed};
use crate::copy_table::copy_text;
use crate::inbound_client;
use crate::origin::Origin;
use serde_json::{json, Value};

// 这里原来有退出行为的四句（`EXIT_*`）、崩溃读数的四句（`HEALTH_*`）与它们的两张对拍表
//   （`EXIT_COPY` / `HEALTH_COPY`）和表的清单（`CROSS_LANGUAGE_COPY`）—— Rust 这一份「只为与 TS 那份逐字对拍而存在」，
//   非 test 构建里没有读者（gate `deadcode` 那 11 条）。文案表立起来之后两侧读**同一条表项**，第二份副本与逐字对拍一起删。
// 崩溃读数那三档的**判定**也搬到了这里（[`health_face`]，唯一一份）：
//   后端出「健康」那一格的成品（状态 ＋ 格子里那一句 ＋ ⓘ ＋ `[详情]`），界面只排版。TS 那份三档与这里原来那份
//   同逻辑的第二份一起并掉；那几句的 key 随之归本文件的面（`rsBackendPolicy.health.*`）。

// 界面那两条（问 / 改那台机器上的值）的期限 `EXIT_POLICY_BUDGET`〔散文墓碑〕随那两条 Tauri 命令一起走了：
//   设置页经通道直接问后端（期限同值 10 秒，住 `settings/backend-section.ts`）。

/// monitor 退出臂那一问的期限。**monitor 正在退**：问不到就按缺省办，不许把退出拖住。
/// 本机后端读一份小文件就回，这个数是上界不是节拍（问的是本机那条已经连着的流）。
const EXIT_ASK_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

/// 发送口（形状照 `frame_query::call`）：没通道 / 旧后端不认 / 调用失败，各说各的话。
/// 今天只剩一个调用方 —— 退出臂那一问（[`kill_on_exit_now`]，**monitor 自己的事**，不是替界面转）；
/// 界面那两条（问 / 改）改走通道了。
async fn exit_policy_call(
    origin: &Origin,
    cmd: &str,
    args: Value,
    budget: std::time::Duration,
) -> Result<Value, String> {
    let origin = origin.as_wire_str();
    let Some(client) = inbound_client::client_for(origin) else {
        return Err(said(no_channel(origin)));
    };
    if !client.accepts(cmd) {
        return Err(copy_core::backend_old(&crate::cc_bus::machine_label(
            origin,
        )));
    }
    let data = client.call(cmd, args, budget).await.map_err(|e| {
        said(route_call_error(&e, |_code, message| {
            copy_text(
                "rsBackendPolicy.call.failed",
                &[
                    ("machine", &origin.to_string()),
                    ("message", &message.to_string()),
                ],
            )
        }))
    })?;
    data.ok_or_else(|| copy_core::reply_unreadable(&crate::cc_bus::machine_label(origin)))
}

/// 三态里给人看的那句话（同 `frame_query::said`）。`Done` 在本族走不到。
fn said(r: Routed) -> String {
    match r {
        Routed::NoChannel(s) | Routed::Refused(s) => s,
    }
}

// 那两条 Tauri 命令（问「退出行为」那个值 · 交那台机器写它：`backend_exit_policy` /
//   `set_backend_exit_policy`〔散文墓碑〕）退役：它们只在「拦空白名 ＋ 转一条 `exit-policy-read` / `exit-policy-set` ＋ 原样交回」，
//   解释本来就在界面那一侧（`settings/backend-section.ts::readExitAnswer`）⇒ 设置页经通道直接问，本机与远端同一条路。

/// 从后端那份应答里取生效值。形状不对 ⇒ `None`（调用方按缺省办并出声）。
pub(crate) fn kill_from_answer(data: &Value) -> Option<bool> {
    data.get("killOnExit").and_then(Value::as_bool)
}

/// 〔`§3.3b ④`〕**monitor 退出臂在决定那一刻现问一次**：这台机器要不要跟着结束。
///
/// 只问、不记：每次调用都真发一条 `exit-policy-read`，本模块没有任何地方存它的答案。
/// 问不到（没通道 / 超时 / 旧后端）⇒ **按缺省（不结束）办，并出声** —— 那不等于有人这么选过。
pub fn kill_on_exit_now(origin: &Origin) -> bool {
    let asked = tauri::async_runtime::block_on(exit_policy_call(
        origin,
        "exit-policy-read",
        json!({}),
        EXIT_ASK_BUDGET,
    ));
    match asked.as_ref().map(kill_from_answer) {
        Ok(Some(k)) => k,
        Ok(None) => {
            tracing::warn!(
                "退出：[{}] 的退出策略应答形状不对 ⇒ 按缺省（不结束）办",
                origin.as_wire_str()
            );
            false
        }
        Err(e) => {
            tracing::warn!(
                "退出：问不到 [{}] 的退出策略（{e}）⇒ 按缺省（不结束）办 —— 这不等于有人这么选过",
                origin.as_wire_str()
            );
            false
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// `K-P3` 第一档（09-04）：**一个分得清的死亡判据 + 一本真的会被写下来的账**
//
// `§0-7` 的分档：第一档**零 spawn 口、零断言放宽、不动 `SPAWN_SITES_TODAY`**。
// 理由是 `§0-2` 那两条真事故 —— 盘上唯一与「自动再来一次」有关的两条事故，
// **都是自愈把一次确定性失败放大了**，而两条同一根：
// **死亡判据分不清崩了 / 拒绝了 / 读坏了。** ⇒ 先买判据，重起是它的下游。
//
// # 为什么这一档住在宿主侧，而不是预批的 `src/backend/death_ledger.rs`
//
// 三条现打，逐条给住址（PM 补充里预批了那个新文件 + `main.rs` 一行 `mod`，本件**都没用**）：
//
// 1. **backend 写不了盘，而放宽写盘口是第二档的事。**
//    `src/backend/readonly_guard.rs` 的默认层禁掉本 crate 生产段里
//    全部 `fs::write` / `File::create` / `OpenOptions` …，白名单**恰好一个模块**
//    （`control/fork_write.rs`，`assert_eq!(whitelisted, 1)`）⇒ 在后端侧开第二个
//    写盘口 = 动一条相等断言 = **放宽**，而 `§0-7` 那张表把「零断言放宽」写进了第一档。
// 2. **backend 说出口的话在「前端没开时」落不到任何地方。**
//    `local_backend_host.rs::spawn_detached` 现打 `.stdout(Stdio::null())` + `.stderr(Stdio::null())`
//    ⇒ 脱离那条路上后端的 `tracing` 全部进 `/dev/null`。
//    ⚠ 这一条比第 1 条更硬：**它不是权限问题，是那句话没有听众。**
// 3. **宿主叫不动后端那侧的代码。** `src/backend` 只有 `[[bin]]`、没有 `[lib]`
//    （`Cargo.toml` 那段 standalone 头注），⇒ 住在那个 crate 里的判据只有后端自己用得上，
//    而**唯一观测得到后端之死的位置是它的父进程 = 宿主**（`§0-4` 的结论）。
//
// ⇒ 账住宿主侧，**而它零新增写盘口**：真正碰盘的是 `logging.rs::build_rolling_appender`
//   （`write_site_registry::WRITE_SITES` 里已登记的那条「monitor 自己的滚动日志」），
//   本段一个 `fs::` 调用都没有 ⇒ 不进那张表的人群，也不用改它。
//
// # ⚠ 今天**还没接线**的那一格，如实登记（不写在注释里就等于埋掉）
//
// [`record_death`] 的生产调用点该在宿主观测到它死掉的那一刻 ——
// `local_backend_host.rs::reap_detached`（`c.wait()` 返回那一拍）与
// `local_backend.rs::supervise_with_stdio`（读到 EOF 那一拍）。
// 两处**都不在本件写区** ⇒ 交回里逐字点名，由 PM 落。
// ⇒ **本段今天证的是「判据分得开、账写得下、写不进去会出声」，证不了「它已经被调用过」。**
// 上面那两处后来接上了（判据 `backend_policy_tests.rs` 的 `DEATH_RECORD_SITES` 逐处点名）；读数经 `backend_status` 上界面，
// 那一格的成品由 [`health_face`] 出。
// ══════════════════════════════════════════════════════════════════════════

/// 那个进程**怎么没的** —— 这一维只装这一件事。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// **从来没起来**：没内嵌 / 释放失败 / 口上有东西但接不上（`local_backend_host.rs` 的
    /// `Adopt::Refused` 与 `resolve_bin` 失败那两支）。
    NeverSpawned,
    /// 自己退了，带退出码。
    Exited(i32),
    /// 被信号打死 —— 这一支**没有**「退出码」这回事。
    Signalled(i32),
}

/// 它**跟我们说过话没有**（hello 帧 / attach 应答）。
///
/// ⚠ 这一维是 2026-07-09 那次事故的**判别式**：未知 flag ⇒ backend `exit 2`、
/// **一个字节都不输出、没有 hello** ⇒ monitor 看到的和「backend 崩了」无法区分
/// ⇒ 重连 ⇒ 发同一个 flag ⇒ **死循环**（`src/doc/IPC-PROTOCOL.md` 与
/// `src/backend/main.rs` 两处逐字）。
/// **分开这两件事的就是它。**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handshake {
    /// 一个字节都没说过。
    NeverSpoke,
    /// 说过（至少 hello 到手了）。
    Spoke,
}

/// **我们这一侧**的读端怎么结束的。⚠ 这一维说的是我们，**不是它**。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReaderEnd {
    /// 干净 EOF：管子关了 / 它走了。
    CleanEof,
    /// 读出错（`B1` 那一形：`InvalidData` 与 EOF 走同一条路），带**原样**的那句错。
    Broken(String),
    /// ★ `K-P3b`：**这条路上根本没有读端**。
    ///
    /// 今天唯一的生产来源是「从来没起来」那一支（`local_backend_host::start_local_backend`
    /// 返回 `Failed` 那一个出口）：进程一次都没存在过 ⇒ 也就没有过一根管子。
    ///
    /// ⚠ **它刻意不与 [`ReaderEnd::CleanEof`] 合并**：后者逐字的意思是
    /// 「管子关了 / 它走了」，而那条路上从来没有过管子 ⇒ 写 `CleanEof`
    /// 就是给一格**没有事实可说**的维度填一个看起来合理的值，
    /// 正是 `platform/fallback_guard.rs` 治的那一族。
    NotObserved,
}

/// 一次「它没了」的全部可观测证据。**三维分开装，一个字段一件事。**
///
/// ⚠ 刻意不做成一个「死因」枚举让调用方直接填：那样四件事分不分得开，
/// 取决于调用方当时怎么想 —— 而 `B1` 那次事故的全部内容正是**调用方把两件事想成了一件**
/// （`InvalidData` 与 EOF 同路 ⇒ 记一次「崩溃」）。
/// ⇒ 判定必须由**证据**驱动，不由名字驱动。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathEvidence {
    pub outcome: Outcome,
    pub handshake: Handshake,
    pub reader: ReaderEnd,
    /// 「从来没起来」那一支的失败原因与找过的地方。
    ///
    /// ⚠ **`StartOutcome::Failed { reason, looked_at }` 的两个字段原样转来，不另写一份**
    /// —— `local_backend_host.rs` 那一族逐字的纪律：「两份措辞迟早对不上」。
    pub start_failure: Option<(String, Vec<std::path::PathBuf>)>,
}

/// 起不来而**连一句原因都没转过来**时说的话。
///
/// ⚠ 它是一句「我不知道」，不是一句解释 —— 那一格真的没有事实可说，
/// 编一个听起来合理的原因就是 `platform/fallback_guard.rs` 治的那一族。
pub static NO_START_REASON: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsBackendPolicy.death.noStartReason", &[]));

/// 四件事，两两分得开。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Death {
    /// **从来没起来**。
    NeverStarted {
        reason: String,
        looked_at: Vec<std::path::PathBuf>,
    },
    /// **被拒了** —— 它自己 `exit` 非 0，且一个字节都没说过（2026-07-09 那个 `exit 2`）。
    Refused { code: i32 },
    /// **崩了** —— 说过话之后异常终止，或被信号打死。
    Crashed { how: Outcome },
    /// **读坏了** —— 我们这一侧读出错。⚠ **它不算它崩了一次。**
    Misread { detail: String },
}

/// 从证据判出那**一**件事。`None` = 它是一次正常收工，不上账。
///
/// # 四支的次序是承重的，逐条给理由
///
/// 1. **读坏了先判。** 这一维说的是我们这一侧，与那个进程死没死是两件事；
///    `B1` 的病根正是把这一格算成了一次崩溃 —— 而那条错误诊断的下游是
///    「崩了 3 次 ⇒ 整个进程周期不再起来」。
/// 2. **从来没起来**：连进程都没有 ⇒ 没有退出状态可谈。
/// 3. **被拒了**：非 0 退出码 **且** 从来没说过话。两个条件缺一不可 ——
///    只看退出码会把「说过话之后崩了」也算成被拒。
/// 4. 剩下的都是**崩了**。
///
/// ⚠ **诚实边界，如实登记**：`exit 0` 而**从来没说过话**这一形（起来了、干净地退了、
/// 但一句 hello 都没发）今天**判成不上账**。它在这三维上与「正常收工」不可区分，
/// 而给它单开第五档就是发明一件今天没有证据支持的事（本件的 DoD 说的是**四件**）。
/// 真撞上它 ⇒ 那是第二档的题目，回来重判，别在这里悄悄加一档。
pub fn verdict(ev: &DeathEvidence) -> Option<Death> {
    match &ev.reader {
        ReaderEnd::Broken(detail) => {
            return Some(Death::Misread {
                detail: detail.clone(),
            })
        }
        // ★ `K-P3b` 新增的那一臂：**没有读端**这件事在这里**让开**，不冒充一次干净 EOF。
        //
        // 它既不是「读坏了」（那说的是我们这一侧读出了错），也不是「干净 EOF」
        // （那说的是管子关了 —— 而那条路上从来没有过管子）。⇒ 这一维不参与判定，
        // 剩下两维（`Outcome::NeverSpawned` 那一支）足够判出「从来没起来」。
        //
        // ⚠ **既有四条臂的结论一个字没变**：这一臂只让 `NotObserved` 落到与
        //   `CleanEof` 相同的下游，而它存在的全部理由是**别在证据里编一个值**。
        ReaderEnd::NotObserved | ReaderEnd::CleanEof => {}
    }
    if matches!(ev.outcome, Outcome::NeverSpawned) {
        let (reason, looked_at) = ev
            .start_failure
            .clone()
            .unwrap_or_else(|| (NO_START_REASON.to_string(), Vec::new()));
        return Some(Death::NeverStarted { reason, looked_at });
    }
    if let Outcome::Exited(code) = &ev.outcome {
        if *code == 0 {
            return None;
        }
        if ev.handshake == Handshake::NeverSpoke {
            return Some(Death::Refused { code: *code });
        }
    }
    Some(Death::Crashed {
        how: ev.outcome.clone(),
    })
}

/// 四条**判定词** —— 账上那一行靠它分类，界面靠它认这是哪一格。
///
/// ⚠ 它不是四个变体名的 `Debug`：`K-H2b` 那条主线逐字禁掉的形状是「有名字 ≠ 接上了」，
/// 而 `Debug` 输出会跟着重命名漂，账上的历史行就此对不上。
pub fn death_kind(d: &Death) -> String {
    match d {
        Death::NeverStarted { .. } => copy_text("rsBackendPolicy.death.neverStarted", &[]),
        Death::Refused { .. } => copy_text("rsBackendPolicy.death.refused", &[]),
        Death::Crashed { .. } => copy_text("rsBackendPolicy.death.crashed", &[]),
        Death::Misread { .. } => copy_text("rsBackendPolicy.death.misread", &[]),
    }
}

/// Windows 的 `STATUS_CONTROL_C_EXIT`：进程被**控制台事件**（Ctrl+C / Ctrl+Break / 关控制台窗口）
/// 打死时的退出码。按 `i32` 读（`ExitStatus::code()` 的视角）是 `-1073741510`。
///
/// 〔死亡账说人话〕平台无关地比：POSIX 上的退出码只有 0–255，
/// 这个值在构造上不会出现 ⇒ 不需要 `cfg`。
pub const STATUS_CONTROL_C_EXIT: u32 = 0xC000013A;

/// 上面那个码的人话。进界面（`last_brief`）也进日志（`ledger_line`），同一个来源。
pub static CONSOLE_CTRL_EXIT_SAID: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsBackendPolicy.death.consoleCtrl", &[]));

/// 一个退出码的说法：认得的码说人话，其余是「退出码 N」。**判定只住这里**（`Refused` 与 `Crashed` 两臂共用）。
fn exit_code_said(code: i32) -> String {
    // `as u32` 是按位重解释，不是数值换算：-1073741510_i32 的位型就是 0xC000013A。
    if code as u32 == STATUS_CONTROL_C_EXIT {
        CONSOLE_CTRL_EXIT_SAID.to_string()
    } else {
        copy_text(
            "rsBackendPolicy.status.exit",
            &[("status", &code.to_string())],
        )
    }
}

/// 那一行上的**退出状态**（`KP3A`① 要的两样之一）。
pub fn exit_status(d: &Death) -> String {
    match d {
        Death::NeverStarted { .. } => copy_text("rsBackendPolicy.status.neverExisted", &[]),
        Death::Refused { code } => exit_code_said(*code),
        Death::Crashed { how } => match how {
            Outcome::Exited(code) => exit_code_said(*code),
            Outcome::Signalled(sig) => copy_text(
                "rsBackendPolicy.status.signal",
                &[("sig", &sig.to_string())],
            ),
            Outcome::NeverSpawned => copy_text("rsBackendPolicy.status.none", &[]),
        },
        Death::Misread { .. } => copy_text("rsBackendPolicy.status.misread", &[]),
    }
}

/// 账行破折号后面那一截：**只摆证据的详情**（短摘要 = 判定 ＋ 退出状态，见 [`last_brief`]）。四条两两不同。
///
/// 原来每条还带处方与论证（「先看它拒的是什么」「重起帮不上忙」…），删了。只进日志。
pub fn death_detail(d: &Death) -> String {
    match d {
        Death::NeverStarted { reason, looked_at } => format!(
            "{reason}；找过 {} 处：{}",
            looked_at.len(),
            looked_at
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(&copy_text("rsBackendPolicy.death.listSep", &[]))
        ),
        Death::Refused { .. } => "它一个字节都没说就退出了".to_string(),
        Death::Crashed { .. } => "说过话之后异常终止".to_string(),
        Death::Misread { detail } => format!("monitor 这一侧读出错：{detail}"),
    }
}

/// 界面上「最后一次」那一格：**判定 ＋ 退出状态**，一句短话（如「崩了，exit -1073741819」；
/// 认得的码说人话，见 [`exit_code_said`]）。
///
/// 它替掉的是原来直接进界面的整条 [`ledger_line`]：那是**日志行格式**
///（`[死亡账] origin=… 判定=… 退出状态=… —— …`），禁它进界面。
/// 细节（证据、该怎么办）留在日志那一行里，界面上要看就去「日志」。
pub fn last_brief(d: &Death) -> String {
    copy_text(
        "rsBackendPolicy.death.brief",
        &[("kind", &death_kind(d)), ("status", &exit_status(d))],
    )
}

/// 账上那一行。
///
/// ⚠ **这一行自己不带时刻，是刻意的。** 落点是 monitor 自己的滚动日志，
/// 而 `tracing_subscriber::fmt` 的默认 timer 已经给每一行打了带日期的时刻，
/// 文件名本身还按天滚（`logging.rs`）。在这里再打一份就是同一个事实的第二份表示。
///
/// ⚠ 反过来，**日期这件事必须有人打**：今天唯一那本持久账
/// `~/.cc-monitor/bin/wrap.log` 的两行**只有时刻、没有日期**
/// （现打逐字 `[21:14:36.212230477] exit rc=0 argv=[--with-bg --tail-only]`）
/// ⇒ 「它上一次崩在哪一天」这句话在那本账上问不出来。**别把落点换成一个不打时刻的。**
pub fn ledger_line(origin: &str, d: &Death) -> String {
    format!(
        "[死亡账] origin={origin} 判定={} 退出状态={} —— {}",
        death_kind(d),
        exit_status(d),
        death_detail(d)
    )
}

/// 账的**落点**。抽成 trait 只为一件事：让「写不进去要出声」那一格**可以被真的打一刀**。
pub trait DeathSink {
    /// 写一行。`Err` = 落点拒收，那句拒收的话**原样**回来。
    fn write_line(&mut self, line: &str) -> Result<(), String>;
}

/// 生产落点：**monitor 自己的滚动日志**。
///
/// ★ 这是本件「真的会被写下来」那一半的全部内容，而它**零新增写盘口**：
/// 真正碰盘的是 `logging.rs::build_rolling_appender` —— 一个已经登记在
/// `write_site_registry::WRITE_SITES` 里的落点。本模块一个 `fs::` 调用都没有。
///
/// ⚠ **诚实边界**：`tracing` 的语义是「没有 subscriber 就丢掉」。
/// `logging.rs` 在 setup 那一拍才装 subscriber ⇒ **在那之前记的一笔会被静默丢掉**，
/// 而本落点看不见这件事（`tracing::error!` 没有返回值）。
/// ⇒ 所以 [`record_death`] **不把账只交给落点**：那张进程内的表照样更新，
/// 界面上的读数不跟着落点一起消失。
pub struct MonitorLog;

impl DeathSink for MonitorLog {
    fn write_line(&mut self, line: &str) -> Result<(), String> {
        tracing::error!("{line}");
        Ok(())
    }
}

/// 记一笔之后手里剩下的东西。
///
/// `#[must_use]` 是**刻意的**：吞掉它就等于把「写不进去」这件事静默掉，
/// 而 `§0-1` 那一格的全部内容就是「没有任何东西在记」——
/// 一个被吞掉的错误会让本件原地退回那个状态。
#[must_use]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    /// 落在账上的那一行（落点拒收时也给，好让调用方至少能就地喊一声）。
    pub line: String,
    /// 落点拒收时**原样**转来的那句话。`None` = 写进去了。
    pub sink_error: Option<String>,
}

/// 一台机的死亡账读数。**四个计数分开装** —— 「读坏了」不许被加进「崩了」。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub crashed: u32,
    pub refused: u32,
    pub never_started: u32,
    pub misread: u32,
    /// 最后落在账上的那一行（**日志行格式**，只给日志与判据用）。
    pub last: Option<String>,
    /// 最后那一次的**短摘要**（[`last_brief`]）—— 进界面的只有这一格。
    pub last_brief: Option<String>,
}

impl Health {
    /// 这本账上一共记到过几件事。**0 = 一条都没有**，那一格说的是「答不出来」。
    pub fn seen(&self) -> u32 {
        self.crashed + self.refused + self.never_started + self.misread
    }
}

fn ledger() -> &'static Mutex<HashMap<String, Health>> {
    static L: OnceLock<Mutex<HashMap<String, Health>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 记一笔。`None` = 那不是一次死亡（正常收工），不上账。
///
/// 三件事按这个次序做，次序是承重的：
/// 1. **判**（纯函数，不碰世界）；
/// 2. **写落点**，拒收了**就地喊一声**（不许静默）；
/// 3. **更新进程内那张表** —— ⚠ 这一步在第 2 步失败时**照样做**：
///    落点坏了不该把界面上的读数一起带走。
pub fn record_death(
    origin: &str,
    ev: &DeathEvidence,
    sink: &mut dyn DeathSink,
) -> Option<Recorded> {
    let d = verdict(ev)?;
    let line = ledger_line(origin, &d);
    let sink_error = sink.write_line(&line).err();
    if let Some(why) = &sink_error {
        tracing::error!(
            "这台机（{origin}）的死亡账写不进去（{why}）—— 那一行没有落点，只能就地喊一声：{line}"
        );
    }
    if let Ok(mut t) = ledger().lock() {
        let h = t.entry(origin.to_string()).or_default();
        match &d {
            Death::NeverStarted { .. } => h.never_started += 1,
            Death::Refused { .. } => h.refused += 1,
            Death::Crashed { .. } => h.crashed += 1,
            Death::Misread { .. } => h.misread += 1,
        }
        h.last = Some(line.clone());
        h.last_brief = Some(last_brief(&d));
    }
    Some(Recorded { line, sink_error })
}

/// 这台机的读数。**未登记 ⇒ 一条都没有**（而那一格说「答不出来」，不说「没崩过」）。
pub fn health(origin: &str) -> Health {
    ledger()
        .lock()
        .ok()
        .and_then(|t| t.get(origin).cloned())
        .unwrap_or_default()
}

/// 「健康」那一格的三档。线上就是这三个词（`data-health`），界面原样挂、不枚举。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    /// 账上一条都没有 —— 答不出来（**不是**「没崩过」）。
    Unknown,
    /// 记到过事，一次崩溃都没有。
    Clean,
    /// 崩过。
    Crashed,
}

/// 设置页「健康」那一格的**成品** —— 界面只排版，不判。
///
/// 线上就是这四个键（`backend_status` 的 `health`；形状由金样 `tests/__fixtures__/backend-health.golden.json`
/// 两侧同读钉住）。四个计数与账行**不上线**：界面拿不到原料，也就没法再判一遍（「前端不做判定」）。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HealthFace {
    /// 哪一档。「区分保留，用界面状态表达，不用散文表达」—— 界面挂到 DOM 状态上。
    pub state: HealthState,
    /// 格子里那一句（「— 无记录」/「⚠ 崩过 N 次 · 最后一次：<判定，退出状态>」）。
    pub summary: String,
    /// ⓘ 里那条完整区分（「无记录 ≠ 没崩过」）。只有「无记录」有。
    pub why: Option<String>,
    /// `[详情]`：四个计数分开列 ＋ 完整记录在日志里。「无记录」没有（没东西可展开）。
    pub detail: Option<String>,
}

/// ★ 那条区分的**唯一一处判定**：这台机的读数落在哪一档、每一档给界面什么。
///
/// ⚠ 第一档的判准是「**这本账上一条记录都没有**」（[`Health::seen`] `== 0`），不是 `crashed == 0`。
/// 写成后者的话，一台从来没被记过的机器会被说成「一次都没崩过」——
/// 那正是 `§0-1` 点名不许混用的那两句话。
/// 「崩过」那一句接的是**短摘要**（[`last_brief`]），不是账行（日志行格式只落日志）。
pub fn health_face(h: &Health) -> HealthFace {
    if h.seen() == 0 {
        return HealthFace {
            state: HealthState::Unknown,
            summary: copy_text("rsBackendPolicy.health.unknown", &[]),
            why: Some(copy_text("rsBackendPolicy.health.unknownWhy", &[])),
            detail: None,
        };
    }
    let detail = Some(copy_text(
        "rsBackendPolicy.health.detail",
        &[
            ("crashed", &h.crashed.to_string()),
            ("refused", &h.refused.to_string()),
            ("neverStarted", &h.never_started.to_string()),
            ("misread", &h.misread.to_string()),
        ],
    ));
    // 没异常退出过 ⇒ 一句就够，不挂［详情］（被拒 / 没起来 / 读坏的计数只在异常退出那一档展开）。
    if h.crashed == 0 {
        return HealthFace {
            state: HealthState::Clean,
            summary: copy_text("rsBackendPolicy.health.clean", &[]),
            why: None,
            detail: None,
        };
    }
    let missing = copy_text("rsBackendPolicy.health.lastMissing", &[]);
    HealthFace {
        state: HealthState::Crashed,
        summary: copy_text(
            "rsBackendPolicy.health.crashed",
            &[
                ("crashed", &h.crashed.to_string()),
                ("last", h.last_brief.as_deref().unwrap_or(&missing)),
            ],
        ),
        why: None,
        detail,
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/backend_policy_tests.rs"]
mod tests;
