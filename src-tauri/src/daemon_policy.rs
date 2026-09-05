//! P2s（定框 `C8`）：**每台机一份 daemon 策略**。
//!
//! 今天只有一条策略：**monitor 退出时要不要主动结束这台机的 daemon**，默认 **false（不主动结束）**。
//!
//! # 归属：为什么持久化不在这里
//!
//! `config.rs` 头注逐字「Rust 端**不解释配置内容**（schema 在前端定义）」。
//! 若这里也往 `config.json` 里读写，同一个文件就有了**两个写者** ——
//! 前端「读—改—写」整份的那一刻，会把 Rust 刚写进去的键按一份**陈旧副本**覆盖掉。
//! ⇒ 持久化归前端；本模块只持有**生效值**，由前端在改动时与启动时推进来。
//!
//! # ⚠ 「不主动结束」到底等不等于「继续跑」—— **今天要看它有没有真脱离**〔`K-P1` 08-26 翻面〕
//!
//! **翻面之前**（一直到 `K-P1`）：daemon 是**纯 stdio 子进程**，monitor 一退读端就断，
//! 它在 **153 毫秒**内自己 broken-pipe 退出（实测，08-11 P2s §0a）。所以那时本策略的真实语义是
//! **「立刻杀」与「让它自己死」之差**，不是「后台常驻」，而 `P2s-Y5` 据此立了一条
//! **无条件**禁令（原句已从这四处删干净，由
//! [`tests::the_unconditional_ban_is_gone_from_all_four_homes`] 钉着；要看原文去翻
//! `control-parity` 的 `P2s-Y5`）。
//!
//! ⚠ **这里刻意不把那句原文抄下来** —— 抄下来它就会命中那条判据自己，
//! 而「为了不命中判据把引号换成另一种」是**绕**，不是治。同族的自指陷阱本仓记过多次。
//!
//! **今天那条禁令的前提只在一半的情况下成立了。**`K-P1` 给了 daemon 一个监听口，
//! 并让它在 Linux 上**真脱离**（`process_group(0)` + stdio 全 null + 协议改走那个口）
//! ⇒ 「勾掉开关」在那一支上**真的**是「继续跑」。
//!
//! ⇒ 禁令换成**按状态分档**（`K-P1 KPY4`，由 `src/settings/daemon-section.vitest.ts` 机检）：
//! 用户可见的那四句话有**唯一一个家**（`src/daemon-policy.ts`），
//! Rust 这一侧把同样的四条字面量放在下面，由 [`tests::the_exit_copy_is_the_same_string_on_both_sides`]
//! 逐字对拍 —— 形状抄 `the_local_origin_is_the_same_string_on_both_sides`。
//!
//! ⚠⚠ **「无人监护」这半是用户裁定的一半，不许省**（`DECISIONS` `K14` 逐字：
//! 「第一档必须在 UI 上如实说『继续跑，无人监护』，这是本裁定的一半，
//! **不许只做常驻不做这句话**」）。脱离之后**没有任何东西在监护它** ——
//! 崩了不会自动重起（自愈整件摘出去成了 `K-P3`）。那是**如实登记的降级**，不是漏洞；
//! 而如实登记的意思就是**说出口**，不是写在一条源码注释里。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// ── `K-P1 KPY4`：退出行为的四句话。**用户可见文案的家在 TS 那侧**
/// （`src/daemon-policy.ts` 的 `EXIT_*`），这里这四条只为**逐字对拍**而存在。
///
/// ⚠ 别在这里加第五条而不动那边：对拍是**双向**的（两边条数与内容都比）。
///
/// ① 勾上「退出时结束它」。
///
/// ⚠ 它与那个复选框的**标签**刻意不是同一个串：标签说的是**这个开关是什么**，
/// 这一句说的是**接下来会发生什么**。写成同一个串的话，「那四句只许有一个家」那条判据
/// 会把复选框的标签算成第二个家 —— 而那**不是误报**：两处一模一样的串，
/// 下一次改文案时一定只会改到一处。
pub const EXIT_KILLS: &str = "monitor 退出时会结束它";
/// ② 勾掉 + **真脱离了**。★ 这一句里的「无人监护」是 `K14` 背书的那半。
pub const EXIT_UNATTENDED: &str =
    "monitor 退出后它继续跑，无人监护：崩了不会自动重起；下次开 monitor 会接上它，接不上才起一个新的";
/// ③ 勾掉 + **没脱离**（平台不支持 / 被关掉了 / 脱离失败）⇒ **保持今天那句，一字不改**。
pub const EXIT_SELF_DIES: &str =
    "monitor 不主动结束它；它仍会在 monitor 退出后很快自行退出";

/// 那三句的顺序**与 TS 那侧逐条对齐**。对拍判据按名字取、按内容比，条数也比。
///
/// ⚠ 曾经有过第四档（「已经脱离了 ⇒ 这个勾管不到它」）。它是**一个缺口的产物**：
/// 退出钩子当时只收被监护的那条路。缺口补上（`lib.rs` 的 `RunEvent::Exit` 现在两条都收）
/// 之后那一档成了假话 ⇒ 随缺口一起删掉。**留档是为了下一个人别把它当成「少了一档」。**
pub const EXIT_COPY: &[(&str, &str)] = &[
    ("EXIT_KILLS", EXIT_KILLS),
    ("EXIT_UNATTENDED", EXIT_UNATTENDED),
    ("EXIT_SELF_DIES", EXIT_SELF_DIES),
];

/// ── `K-P3 KP3C`：那句「无人监护」后面接的那个**读数**。用户可见文案的家同样在 TS 那侧
/// （`src/daemon-policy.ts` 的 `HEALTH_*`），这里这四条只为**逐字对拍**而存在。
///
/// ★★ 最要紧的是 [`HEALTH_UNKNOWN`]：它买的是 `K-P3` `§0-1` 那一格 ——
/// 今天不是「它没崩过」，是「**没有任何东西在记它崩没崩**」，而 `§0-1` 逐字写着
/// 「这两句话差得很远，件计划里不许混用」。⇒ 读数的默认档是**答不出来**，不是绿灯。
pub const HEALTH_UNKNOWN: &str =
    "上次崩没崩：答不出来 —— 今天没有任何东西在跨 monitor 进程地记它崩没崩，而「答不出来」不等于「没崩过」";
/// 记到过事、但一次崩溃都没有。`{misread}` 是**两侧共用的占位符**。
pub const HEALTH_CLEAN: &str =
    "这次 monitor 开着以来：它一次都没崩过（读坏了 {misread} 次不算它崩 —— 那是我们这一侧的读端）";
/// 崩过。`{crashed}` / `{last}` 同上。
pub const HEALTH_CRASHED: &str = "这次 monitor 开着以来：它崩过 {crashed} 次，最后一次是「{last}」";
/// 崩过但那一行没留住 —— 也要说出口，不许拿空串糊过去。
pub const HEALTH_LAST_MISSING: &str = "那一行没留下来";

/// `KP3C` 的那四条，顺序**与 TS 那侧逐条对齐**。
pub const HEALTH_COPY: &[(&str, &str)] = &[
    ("HEALTH_UNKNOWN", HEALTH_UNKNOWN),
    ("HEALTH_CLEAN", HEALTH_CLEAN),
    ("HEALTH_CRASHED", HEALTH_CRASHED),
    ("HEALTH_LAST_MISSING", HEALTH_LAST_MISSING),
];

/// ★ **跨语言逐字对拍的全部表** —— 一张表一件事，两张表不合并。
///
/// # `KP3C` 的题面说「加进 `EXIT_COPY` 那张表」，这里**没有照字面做**〔顶回来，已上报〕
///
/// `EXIT_COPY` 的头注逐字把自己定义成「**退出行为**的四句话」，而
/// 「上次崩没崩」不是一句退出行为 —— 塞进去就是**一个值装了两件事**
/// （本工作区最贵的那一类病，`main.rs::TmuxPlatform` 的头注为同一条病拆过一次）。
///
/// ⇒ 本件兑现的是那句话的**承重那半**：[`EXIT_COPY`] 的头注逐字要的是
/// 「别在这里加第五条而不动那边 —— 对拍是**双向**的（两边条数与内容都比）」。
/// 新常量照样受同一条对拍管着，只是走**第二张表**。
///
/// 这张清单本身守的是**第三件事**：`K-P1 KPY6` 那条判据只认 `EXIT_COPY` 一张表，
/// 所以「有人加了第三张跨语言表而没配对拍」它一个字都不会说。
/// [`tests::every_cross_language_table_is_compared_on_both_sides`] 按这张清单派生人群，
/// **表数也在断言里** ⇒ 加一张新表要同轮改两处：这里，和 TS 那份文件。
pub const CROSS_LANGUAGE_COPY: &[(&str, &[(&str, &str)])] =
    &[("EXIT_COPY", EXIT_COPY), ("HEALTH_COPY", HEALTH_COPY)];

fn table() -> &'static Mutex<HashMap<String, bool>> {
    static T: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缺省：**不主动结束**（`C8`③ 的前半句 —— 那半是站得住的）。
pub const DEFAULT_KILL_ON_EXIT: bool = false;

/// 这台机的 daemon，monitor 退出时要不要主动结束。
pub fn kill_on_exit(origin: &str) -> bool {
    table()
        .lock()
        .ok()
        .and_then(|t| t.get(origin).copied())
        .unwrap_or(DEFAULT_KILL_ON_EXIT)
}

/// 前端推进来的生效值（改动时 + 启动时各推一次）。
#[tauri::command]
pub fn set_daemon_kill_on_exit(origin: String, kill: bool) -> Result<(), String> {
    if origin.trim().is_empty() {
        return Err("origin 不许为空 —— 策略是 per-host 的，没有「全局」这一档".into());
    }
    let mut t = table().lock().map_err(|e| format!("锁毒化: {e}"))?;
    tracing::info!("daemon 策略：origin={origin} kill_on_exit={kill}");
    t.insert(origin, kill);
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════
// `K-P3` 第一档（09-04）：**一个分得清的死亡判据 + 一本真的会被写下来的账**
//
// `§0-7` 的分档：第一档**零 spawn 口、零断言放宽、不动 `SPAWN_SITES_TODAY`**。
// 理由是 `§0-2` 那两条真事故 —— 盘上唯一与「自动再来一次」有关的两条事故，
// **都是自愈把一次确定性失败放大了**，而两条同一根：
// **死亡判据分不清崩了 / 拒绝了 / 读坏了。** ⇒ 先买判据，重起是它的下游。
//
// # 为什么这一档住在宿主侧，而不是预批的 `remote-daemon-proto/src/death_ledger.rs`
//
// 三条现打，逐条给住址（PM 补充里预批了那个新文件 + `main.rs` 一行 `mod`，本件**都没用**）：
//
// 1. **daemon 写不了盘，而放宽写盘口是第二档的事。**
//    `remote-daemon-proto/src/readonly_guard.rs` 的默认层禁掉本 crate 生产段里
//    全部 `fs::write` / `File::create` / `OpenOptions` …，白名单**恰好一个模块**
//    （`control/fork_write.rs`，`assert_eq!(whitelisted, 1)`）⇒ 在 daemon 侧开第二个
//    写盘口 = 动一条相等断言 = **放宽**，而 `§0-7` 那张表把「零断言放宽」写进了第一档。
// 2. **daemon 说出口的话在「前端没开时」落不到任何地方。**
//    `local_daemon.rs::spawn_detached` 现打 `.stdout(Stdio::null())` + `.stderr(Stdio::null())`
//    ⇒ 脱离那条路上 daemon 的 `tracing` 全部进 `/dev/null`。
//    ⚠ 这一条比第 1 条更硬：**它不是权限问题，是那句话没有听众。**
// 3. **宿主叫不动 daemon 那侧的代码。** `remote-daemon-proto` 只有 `[[bin]]`、没有 `[lib]`
//    （`Cargo.toml` 那段 standalone 头注），⇒ 住在那个 crate 里的判据只有 daemon 自己用得上，
//    而**唯一观测得到 daemon 之死的位置是它的父进程 = 宿主**（`§0-4` 的结论）。
//
// ⇒ 账住宿主侧，**而它零新增写盘口**：真正碰盘的是 `logging.rs::build_rolling_appender`
//   （`write_site_registry::WRITE_SITES` 里已登记的那条「monitor 自己的滚动日志」），
//   本段一个 `fs::` 调用都没有 ⇒ 不进那张表的人群，也不用改它。
//
// # ⚠ 今天**还没接线**的那一格，如实登记（不写在注释里就等于埋掉）
//
// [`record_death`] 的生产调用点该在宿主观测到它死掉的那一刻 ——
// `local_daemon.rs::reap_detached`（`c.wait()` 返回那一拍）与
// `backend/control/local_backend.rs::supervise_with_stdio`（读到 EOF 那一拍）。
// 两处**都不在本件写区** ⇒ 交回里逐字点名，由 PM 落。
// 同理 [`describe_health`] 那句话要显示出来得在 `settings/daemon-section.ts` 加一行，
// 那份文件也不在本件写区。
// ⇒ **本段今天证的是「判据分得开、账写得下、写不进去会出声」，证不了「它已经被调用过」。**
// ══════════════════════════════════════════════════════════════════════════

/// 那个进程**怎么没的** —— 这一维只装这一件事。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// **从来没起来**：没内嵌 / 释放失败 / 口上有东西但接不上（`local_daemon.rs` 的
    /// `Adopt::Refused` 与 `resolve_bin` 失败那两支）。
    NeverSpawned,
    /// 自己退了，带退出码。
    Exited(i32),
    /// 被信号打死 —— 这一支**没有**「退出码」这回事。
    Signalled(i32),
}

/// 它**跟我们说过话没有**（hello 帧 / attach 应答）。
///
/// ⚠ 这一维是 2026-07-09 那次事故的**判别式**：未知 flag ⇒ daemon `exit 2`、
/// **一个字节都不输出、没有 hello** ⇒ monitor 看到的和「daemon 崩了」无法区分
/// ⇒ 重连 ⇒ 发同一个 flag ⇒ **死循环**（`doc/IPC-PROTOCOL.md` 与
/// `remote-daemon-proto/src/main.rs` 两处逐字）。
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
    /// 今天唯一的生产来源是「从来没起来」那一支（`local_daemon::start_local_backend`
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
    /// —— `local_daemon.rs` 那一族逐字的纪律：「两份措辞迟早对不上」。
    pub start_failure: Option<(String, Vec<std::path::PathBuf>)>,
}

/// 起不来而**连一句原因都没转过来**时说的话。
///
/// ⚠ 它是一句「我不知道」，不是一句解释 —— 那一格真的没有事实可说，
/// 编一个听起来合理的原因就是 `platform/fallback_guard.rs` 治的那一族。
pub const NO_START_REASON: &str = "起不来，而调用方一句原因都没转过来（这一格今天没有事实可说）";

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
pub fn death_kind(d: &Death) -> &'static str {
    match d {
        Death::NeverStarted { .. } => "从来没起来",
        Death::Refused { .. } => "被拒了",
        Death::Crashed { .. } => "崩了",
        Death::Misread { .. } => "读坏了",
    }
}

/// 那一行上的**退出状态**（`KP3A`① 要的两样之一）。
pub fn exit_status(d: &Death) -> String {
    match d {
        Death::NeverStarted { .. } => "没有退出状态（进程从来没存在过）".to_string(),
        Death::Refused { code } => format!("exit {code}"),
        Death::Crashed { how } => match how {
            Outcome::Exited(code) => format!("exit {code}"),
            Outcome::Signalled(sig) => format!("signal {sig}"),
            Outcome::NeverSpawned => "没有退出状态".to_string(),
        },
        Death::Misread { .. } => "退出状态未知（是我们的读端出错，不是它报的）".to_string(),
    }
}

/// 四条话。**两两不同**，且每一条各自点名自己那条证据与下一步。
///
/// ⚠ `KP3B` 的判定逐字是「四条文案**两两不同**」，不是「源码里出现了四个枚举名」。
/// 所以这里每一条都带**它自己那次事故的住址与处置**，不是四个同义的短语。
pub fn death_copy(d: &Death) -> String {
    match d {
        Death::NeverStarted { reason, looked_at } => format!(
            "从来没起来：{reason}（找过 {} 处：{}）。**下一步**在那句原因里，\
             它是从起法那一侧原样转来的，本处不另写一份。",
            looked_at.len(),
            looked_at
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" · ")
        ),
        Death::Refused { code } => format!(
            "被拒了：它自己 exit {code}，而且**一个字节都没说过** —— \
             这一形与「崩了」在线上无法区分，2026-07-09 就是它变成死循环的\
             （重连 ⇒ 发同一个参数 ⇒ 又 exit）。**下一步：别原样重连重发，先看它拒的是什么。**"
        ),
        Death::Crashed { how } => format!(
            "崩了：说过话之后异常终止（{}）。**下一步：这一格才是自愈要治的那一格**，\
             而重起归第二档 —— 判据不可信的时候重起是放大器。",
            match how {
                Outcome::Exited(code) => format!("exit {code}"),
                Outcome::Signalled(sig) => format!("被信号 {sig} 打死"),
                Outcome::NeverSpawned => "没有退出状态".to_string(),
            }
        ),
        Death::Misread { detail } => format!(
            "读坏了：{detail} —— 这是**我们这一侧**的读端出错，\
             **不算它崩了一次**。把它算进去就是「崩了 3 次」那条错误诊断的来历\
             （`backend/control/local_backend.rs` 逐字：「一个错误的诊断」）。\
             **下一步：重开这条读端，别去动那个进程。**"
        ),
    }
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
        death_copy(d)
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

/// ★ **今天这本账不跨 monitor 进程。**
///
/// 这个常量就是 `§0-1` 那一格变成的读数：
/// - `~/.cc-monitor/bin/wrap.log` 现打 **2 行**、末行 mtime **07-08**、两行都 `rc=0`；
/// - 而**仓里没有任何一处写它**（现打 `grep -rn "wrap\.log"` 全仓 **0 命中**）
///   ⇒ 它是一本**没有写者的孤账**。
///
/// ⇒ 「上次崩没崩」这句话今天答得出来的射程只有**这一个 monitor 进程活着的这段时间**，
/// 而「答不出来」与「没崩过」不是一句话（`§0-1` 逐字：「这两句话差得很远，不许混用」）。
/// 这就是 [`HEALTH_UNKNOWN`] 存在的全部理由，也是 `exit: 待摸底` 第一问要的那个读数的边界。
pub const LEDGER_IS_PROCESS_LOCAL: bool = true;

/// 一台机的死亡账读数。**四个计数分开装** —— 「读坏了」不许被加进「崩了」。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub crashed: u32,
    pub refused: u32,
    pub never_started: u32,
    pub misread: u32,
    /// 最后落在账上的那一行。
    pub last: Option<String>,
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

/// ★★ `K-P3b KP3W3`：[`record_death`] 的**生产调用点逐处点名**。
///
/// 形状照 `remote-daemon-proto/src/readonly_guard.rs::ALLOWED` 那种
/// 「**逐处点名 + 相等**」，不是地板 —— 地板在变大方向上是瞎的
/// （那张表的报错文案逐字：「不许改回地板」）。
///
/// # 它守的是什么
///
/// `K-P3` 第一档交付时这个数是 **0**：判据、账、文案全买了，**一个消费者都没有**
/// （`K-P3` `§3-5` 第一行如实登记）。本件把它接成 3 处；
/// [`tests::the_death_ledger_is_wired_at_exactly_these_sites`] 让「接了几处就是几处」
/// 变成一条相等断言 —— 摘掉任何一处**都会点名是哪一处少了**。
///
/// `(文件, 那一处的宿主函数头, 它记的是哪条路)`
///
/// ⚠ 第二列是**函数头整行的前缀**，用来把那一处的函数体切出来单独数 ——
/// 只数全局总数的话，「某一处塌了、另一处多了一次」会互相抵消（本仓 `daemon_control.rs`
/// 那条「逐口切体，不数全局」的头注为同一形栽过一次）。
pub const DEATH_RECORD_SITES: &[(&str, &str, &str)] = &[
    (
        "local_daemon.rs",
        "fn note_detached_death(",
        "脱离路：`attach_stream` 的流断了 ⇒ `reap_detached` 那条收尸线程 `wait()` 回来那一拍",
    ),
    (
        "local_daemon.rs",
        "fn daemon_supervise_events(",
        "监护路：daemon 那个 `on_event` **闭包**收到 `Exited` 那一拍。\
         它抽成一个返回闭包的函数，只为让 `KP3W3` 那三只假 daemon 能跑**同一个闭包**\
         —— 内联的闭包测试够不着，那条行为判据就只能退回读源码",
    ),
    (
        "local_daemon.rs",
        "fn note_never_started(",
        "起不来：`start_local_backend` 返回 `StartOutcome::Failed` 那一个出口",
    ),
];

/// 记一笔。`None` = 那不是一次死亡（正常收工），不上账。
///
/// 三件事按这个次序做，次序是承重的：
/// 1. **判**（纯函数，不碰世界）；
/// 2. **写落点**，拒收了**就地喊一声**（不许静默）；
/// 3. **更新进程内那张表** —— ⚠ 这一步在第 2 步失败时**照样做**：
///    落点坏了不该把界面上的读数一起带走。
pub fn record_death(origin: &str, ev: &DeathEvidence, sink: &mut dyn DeathSink) -> Option<Recorded> {
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

/// 那句读数 —— 三档，与 TS 那侧 `describeDaemonHealth` 逐格对应。
///
/// ⚠ 第一档的判准是「**这本账上一条记录都没有**」，不是 `crashed == 0`。
/// 写成后者的话，一台从来没被记过的机器会被说成「一次都没崩过」——
/// 那正是 `§0-1` 点名不许混用的那两句话。
pub fn describe_health(h: &Health) -> String {
    if h.seen() == 0 {
        return HEALTH_UNKNOWN.to_string();
    }
    if h.crashed == 0 {
        return HEALTH_CLEAN.replace("{misread}", &h.misread.to_string());
    }
    HEALTH_CRASHED
        .replace("{crashed}", &h.crashed.to_string())
        .replace("{last}", h.last.as_deref().unwrap_or(HEALTH_LAST_MISSING))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_origin_defaults_to_not_killing() {
        assert!(
            !kill_on_exit("这台机从来没被推过策略"),
            "缺省必须是「不主动结束」——`C8`③ 的前半句。\n\
             缺省若是 true，用户什么都没设就会被杀 daemon，而开关默认关着。"
        );
    }

    #[test]
    fn the_policy_is_per_origin_not_global() {
        set_daemon_kill_on_exit("甲机".into(), true).expect("设甲机");
        assert!(kill_on_exit("甲机"), "甲机设了 true 却读不回来");
        assert!(
            !kill_on_exit("乙机"),
            "改甲机把乙机也改了 —— 那就不是 per-host 而是全局一份（`C8`① 明说粒度是每台机各一个）"
        );
    }

    /// ★★ `K-P1 KPY6`：**那几句话不许只改一处** —— 跨语言逐字对拍。
    ///
    /// # 为什么是「逐字对拍」而不是 `grep -c`
    ///
    /// `brief` 第 11 条逐字：判「动没动某个闭集」**不许用 `grep` 数加行**（多行字面量会漏）。
    /// 这里两侧都是**具名常量**，所以量法是「按名字取那一行的字面量，整串相等」——
    /// 形状抄仓里已有的那条 `the_local_origin_is_the_same_string_on_both_sides`。
    ///
    /// # 它防的那个漂**不会有任何东西报错**
    ///
    /// 前端改了措辞而 Rust 这侧没跟：两边都编得过、都跑得起来，
    /// 只有「这一句到底是谁说了算」这件事悄悄没了 —— 下一个人只会看到两句不一样的话。
    /// 一张跨语言表的对拍本体。〔`K-P3`：**纯重构**从上面那条判据里抽出来的，
    /// 两条判红条件（找不到那个名字 / 两侧字面不等）一个字没动 ——
    /// 抽出来只为让第二张表（`HEALTH_COPY`）与第三张、第四张走**同一份**比法，
    /// 而不是各写一份便宜近似（本仓 `E3`：一个事实恰好一个权威源）。〕
    fn compare_one_table(ts: &str, what: &str, table: &[(&str, &str)]) {
        for (name, rust) in table {
            let head = format!("export const {name} =");
            // TS 那侧允许换行（prettier 会把长串折下来）⇒ 从声明处起取到第一个分号。
            let at = ts
                .find(&head)
                .unwrap_or_else(|| panic!("前端那份里找不到 `{head}` —— 名字改了就来改这条"));
            let decl = &ts[at..];
            let end = decl.find(';').expect("那一行不是 `export const X = …;` 的形状");
            let lit = decl[..end]
                .split('"')
                .nth(1)
                .unwrap_or_else(|| panic!("`{name}` 的值不是一个双引号字面量"));
            assert_eq!(
                lit, *rust,
                "{what} 那句话两侧漂了（`{name}`）：\n  前端 {lit:?}\n  后端 {rust:?}\n\
                 ⚠ 这种漂**不会有任何东西报错** —— 两边都编得过、都跑得起来，\n\
                 只是「这一句谁说了算」悄悄没了。⇒ 改文案要**同一拍改两处**。"
            );
        }
    }

    #[test]
    fn the_exit_copy_is_the_same_string_on_both_sides() {
        let ts = include_str!("../../src/daemon-policy.ts");
        compare_one_table(ts, "退出行为", EXIT_COPY);
        // 抽取器自检：条数变了也要红（少一条 = 上面的循环少跑一圈，那正是「空转」）。
        assert_eq!(EXIT_COPY.len(), 3, "退出行为的档数变了 —— 回来重判，别让本条在少数几档上绿着");
    }

    /// ★★ `K-P3 KP3C`：**每一张**跨语言表都有对拍 —— 人群从 [`CROSS_LANGUAGE_COPY`] 派生。
    ///
    /// 上面那条只认 `EXIT_COPY` 一张表 ⇒「有人加了第三张跨语言表而没配对拍」它一个字都不会说。
    /// 本条按清单派生，**表数与每张表的条数都在断言里**。
    #[test]
    fn every_cross_language_table_is_compared_on_both_sides() {
        let ts = include_str!("../../src/daemon-policy.ts");
        // 抽取器自检：读不到那份文件的话下面整条是空转的。
        assert!(
            ts.len() > 500,
            "`src/daemon-policy.ts` 只读到 {} 字节 —— 人群坏了",
            ts.len()
        );
        assert_eq!(
            CROSS_LANGUAGE_COPY.len(),
            2,
            "跨语言表的张数变成 {} 了 —— 加一张就在这里加一条，别让新表在没有对拍的情况下上线。\n\
             （`K-P1 KPY6` 那条判据只认 `EXIT_COPY`，第三张表它一个字都不会说。）",
            CROSS_LANGUAGE_COPY.len()
        );
        let mut compared = 0usize;
        for (what, table) in CROSS_LANGUAGE_COPY {
            assert!(
                !table.is_empty(),
                "`{what}` 是一张空表 —— 上面那个循环会零命中地绿"
            );
            compare_one_table(ts, what, table);
            compared += table.len();
        }
        // 反空真：两张表合起来今天恰好 7 条（EXIT 3 + HEALTH 4）。
        // 数变了就回来重判 —— 变小 = 有几条悄悄掉出了对拍面。
        assert_eq!(
            compared, 7,
            "两侧对拍的常量总数是 {compared}（今天应为 7 = EXIT 3 + HEALTH 4）"
        );
    }

    /// ★★ `KP3C`：那四条读数文案也**只许有一个家**。
    ///
    /// # 为什么这一格由 Rust 侧补
    ///
    /// TS 那侧 `settings/daemon-section.vitest.ts` 的 `KPY4②` 是**手写的三条 `EXIT_*`**，
    /// 本件新增的读数文案不在它的分母里 —— 而那份文件不在本件写区
    /// ⇒ 这一格落在这儿。形状抄同文件那条 `the_unconditional_ban_is_gone_from_all_four_homes`。
    ///
    /// ⚠ 人群里**刻意没有 `daemon_policy.rs` 自己**：那份 Rust 镜像是**对拍的对象**，
    /// 不是第二个家（`EXIT_*` 的头注对同一件事已经论证过一次）。
    #[test]
    fn the_health_copy_has_exactly_one_home() {
        const HOMES: &[(&str, &str)] = &[
            ("src/daemon-policy.ts", include_str!("../../src/daemon-policy.ts")),
            (
                "src/settings/daemon-section.ts",
                include_str!("../../src/settings/daemon-section.ts"),
            ),
            ("daemon_control.rs", include_str!("daemon_control.rs")),
        ];
        for (name, src) in HOMES {
            assert!(src.len() > 500, "{name} 只读到 {} 字节 —— 人群坏了", src.len());
        }
        for (name, lit) in HEALTH_COPY {
            let homes: Vec<&str> = HOMES
                .iter()
                .filter(|(_, src)| src.contains(*lit))
                .map(|(n, _)| *n)
                .collect();
            assert_eq!(
                homes,
                vec!["src/daemon-policy.ts"],
                "`{name}` 出现在 {} 个文件里：{homes:?}\n\
                 ★ 用户可见文案的唯一一个家是 `src/daemon-policy.ts`。\n\
                 **少了**（空表）= 那句话根本不在它该在的家，两侧对拍会先红；\n\
                 **多了** = 抄进了别处，而下一次改文案一定只会改到一处。",
                homes.len()
            );
        }
    }

    // ── `K-P3` `KP3B`：四件事分得开 ──────────────────────────────────────

    /// 四种死法各造一形。**用证据造，不用变体名造** —— 这是 `KP3B` 的承重点：
    /// 「不许是「源码里出现了四个枚举名」（有名字 ≠ 接上了）」。
    fn four_shapes() -> Vec<(&'static str, DeathEvidence)> {
        vec![
            (
                "从来没起来（Adopt::None / 起不来）",
                DeathEvidence {
                    outcome: Outcome::NeverSpawned,
                    handshake: Handshake::NeverSpoke,
                    reader: ReaderEnd::CleanEof,
                    start_failure: Some((
                        "没内嵌 daemon，exe 旁边也没有".to_string(),
                        vec![std::path::PathBuf::from("/甲/乙")],
                    )),
                },
            ),
            (
                "被拒了（2026-07-09 那个 exit 2：一个字节都没输出、没有 hello）",
                DeathEvidence {
                    outcome: Outcome::Exited(2),
                    handshake: Handshake::NeverSpoke,
                    reader: ReaderEnd::CleanEof,
                    start_failure: None,
                },
            ),
            (
                "崩了（说过话之后被信号打死）",
                DeathEvidence {
                    outcome: Outcome::Signalled(9),
                    handshake: Handshake::Spoke,
                    reader: ReaderEnd::CleanEof,
                    start_failure: None,
                },
            ),
            (
                "读坏了（B1：InvalidData 与 EOF 同路）",
                DeathEvidence {
                    outcome: Outcome::Exited(0),
                    handshake: Handshake::Spoke,
                    reader: ReaderEnd::Broken("stream did not contain valid UTF-8".to_string()),
                    start_failure: None,
                },
            ),
        ]
    }

    /// ★★ `KP3B` 正题：四形 ⇒ 四个**互不相同**的判定，四条**两两不同**的话。
    ///
    /// 死值验逐字要的是「把两种合成一种 ⇒ 红」。本条就是那一刀的落点：
    /// 让 `verdict` 把任意两形判成同一格，判定集合就从 4 掉到 3，当场红。
    #[test]
    fn the_four_deaths_are_told_apart_by_evidence_not_by_name() {
        let shapes = four_shapes();
        // 反空真：四形都得在，少一形下面的去重计数会靠人群变小而恒绿。
        assert_eq!(shapes.len(), 4, "夹具只剩 {} 形 —— 人群坏了", shapes.len());
        let mut kinds: Vec<&'static str> = Vec::new();
        let mut copies: Vec<String> = Vec::new();
        for (what, ev) in &shapes {
            let d = verdict(ev).unwrap_or_else(|| {
                panic!("「{what}」被判成了「不是一次死亡」—— 它不上账，也就没有任何一行记它")
            });
            kinds.push(death_kind(&d));
            copies.push(death_copy(&d));
        }
        let mut uniq = kinds.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(
            uniq.len(),
            4,
            "四形只判出 {} 种判定：{kinds:?}\n\
             ★ `§0-2` 那两条真事故同一根：**死亡判据分不清崩了 / 拒绝了 / 读坏了**。\n\
             合成一格 = 那条根原样复发，而它的下游是「重起把一次确定性失败放大」。",
            uniq.len()
        );
        // 四条话两两不同（`KP3B` 逐字：「四条文案两两不同」）。
        for i in 0..copies.len() {
            for j in (i + 1)..copies.len() {
                assert_ne!(
                    copies[i], copies[j],
                    "第 {i} 形与第 {j} 形说的是同一句话 —— 那两格在用户眼里就没分开"
                );
            }
        }
        // 每一条话都得说得下去（掏空成一个短语，上面那条靠「两两不同」照样绿）。
        for (n, c) in copies.iter().enumerate() {
            assert!(
                c.chars().count() >= 30,
                "第 {n} 条话只有 {} 个字 —— 一句说不出住址与下一步的话，等于只给了个名字",
                c.chars().count()
            );
        }
    }

    /// ★★ `B1` 那一刀：**读坏了不许被计成一次崩溃。**
    ///
    /// 来历（`local_backend.rs` 逐字）：一个坏字节让 `InvalidData` 与 EOF 走同一条路
    /// ⇒ 消费者返回 = 判死 ⇒ 记一次「崩溃」，三次之后整个进程周期不再起来，
    /// 日志写「崩了 3 次」——**一个错误的诊断**。
    #[test]
    fn a_broken_reader_is_never_counted_as_a_crash() {
        let ev = DeathEvidence {
            outcome: Outcome::Signalled(9), // ⚠ 连「它真的被打死了」都不改这一格的答案
            handshake: Handshake::Spoke,
            reader: ReaderEnd::Broken("invalid utf-8".to_string()),
            start_failure: None,
        };
        let d = verdict(&ev).expect("读坏了也是一件要上账的事");
        assert_eq!(
            death_kind(&d),
            "读坏了",
            "读端出错被判成了别的 —— 而这一维说的是**我们这一侧**，不是那个进程"
        );
        let origin = "kp3-读坏了不算崩-甲";
        let mut sink = CapturingSink::default();
        record_death(origin, &ev, &mut sink).expect("要记一笔");
        let h = health(origin);
        assert_eq!(
            (h.crashed, h.misread),
            (0, 1),
            "读坏了被加进了「崩了」那一格（crashed={}, misread={}）——\n\
             那正是「崩了 3 次」那条错误诊断的来历。",
            h.crashed,
            h.misread
        );
    }

    /// ★ **负例**：正常收工不上账。少了它，`verdict` 恒 `Some(..)` 也照样绿，
    /// 而那会让每一次干净退出都在账上留一行「崩了」。
    #[test]
    fn a_clean_exit_is_not_a_death() {
        let ev = DeathEvidence {
            outcome: Outcome::Exited(0),
            handshake: Handshake::Spoke,
            reader: ReaderEnd::CleanEof,
            start_failure: None,
        };
        assert_eq!(
            verdict(&ev),
            None,
            "`exit 0` + 说过话 + 干净 EOF 被判成了一次死亡 —— 那是一次正常收工"
        );
        let origin = "kp3-正常收工-甲";
        let mut sink = CapturingSink::default();
        assert!(
            record_death(origin, &ev, &mut sink).is_none(),
            "正常收工也上账了"
        );
        assert!(
            sink.lines.is_empty(),
            "正常收工往落点写了东西：{:?}",
            sink.lines
        );
        assert_eq!(health(origin).seen(), 0, "正常收工被记进了读数");
    }

    /// ★ 起不来那一支的原因**原样转来**，不另写一份
    /// （`local_daemon.rs` 那一族逐字的纪律：「两份措辞迟早对不上」）。
    #[test]
    fn the_start_failure_reason_is_passed_through_verbatim() {
        // 中性夹具串：**不取自任何路径或夹具名**（`brief` 第 12 条那一族 ——
        // 断言用的子串取自夹具名字时，判据会靠名字恒真）。
        let reason = "拿不到 attach token（那个文件是空的）⇒ 拒绝起一个不设防的口";
        let ev = DeathEvidence {
            outcome: Outcome::NeverSpawned,
            handshake: Handshake::NeverSpoke,
            reader: ReaderEnd::CleanEof,
            start_failure: Some((reason.to_string(), vec![std::path::PathBuf::from("/丙/丁")])),
        };
        let d = verdict(&ev).expect("起不来是一件要上账的事");
        match &d {
            Death::NeverStarted {
                reason: got,
                looked_at,
            } => {
                assert_eq!(got, reason, "那句原因被改写过了 —— 两份措辞迟早对不上");
                assert_eq!(looked_at.len(), 1, "找过的地方丢了");
            }
            other => panic!("判成了 {other:?}，而证据说它从来没起来"),
        }
        assert!(
            death_copy(&d).contains(reason),
            "那句原因没被带到账上那一行 —— 账上只剩一个分类名，读的人拿不到下一步"
        );
    }

    // ── `K-P3` `KP3A`：一本真的会被写下来的账 ────────────────────────────

    /// 记下每一行的落点（**只在测试里**）—— 让「那一行真的交给了落点」可判。
    #[derive(Default)]
    struct CapturingSink {
        lines: Vec<String>,
    }
    impl DeathSink for CapturingSink {
        fn write_line(&mut self, line: &str) -> Result<(), String> {
            self.lines.push(line.to_string());
            Ok(())
        }
    }

    /// 一个**拒收**的落点（目录不可写那一形的替身）。
    struct RefusingSink;
    impl DeathSink for RefusingSink {
        fn write_line(&mut self, _line: &str) -> Result<(), String> {
            Err("落点拒收：那个目录不可写".to_string())
        }
    }

    /// ★★ `KP3A`①：非零退出 / 异常终止**必须在账上留一行，带退出状态**。
    ///
    /// # 非空对照（`KP3A`② 那一刀的落点）
    ///
    /// 把 [`record_death`] 里 `sink.write_line(&line)` 那一行摘掉 ⇒ **本条当场红**
    /// （落点一行都没收到）。少了它，「有一本账」这句话只靠一个函数名成立。
    #[test]
    fn every_abnormal_exit_leaves_one_line_carrying_its_exit_status() {
        let cases: Vec<(&str, DeathEvidence, &str)> = vec![
            (
                "非零退出",
                DeathEvidence {
                    outcome: Outcome::Exited(2),
                    handshake: Handshake::NeverSpoke,
                    reader: ReaderEnd::CleanEof,
                    start_failure: None,
                },
                "exit 2",
            ),
            (
                "异常终止",
                DeathEvidence {
                    outcome: Outcome::Signalled(11),
                    handshake: Handshake::Spoke,
                    reader: ReaderEnd::CleanEof,
                    start_failure: None,
                },
                "signal 11",
            ),
        ];
        assert_eq!(cases.len(), 2, "夹具少了一形");
        for (what, ev, status) in &cases {
            let origin = format!("kp3-账上留一行-{what}");
            let mut sink = CapturingSink::default();
            let rec = record_death(&origin, ev, &mut sink)
                .unwrap_or_else(|| panic!("「{what}」没上账 —— 那就没有任何东西在记它"));
            assert_eq!(
                sink.lines.len(),
                1,
                "「{what}」交给落点的行数是 {}（应恰好 1）",
                sink.lines.len()
            );
            assert_eq!(sink.lines[0], rec.line, "交给落点的那一行与回给调用方的不是同一行");
            assert!(
                rec.line.contains(status),
                "「{what}」那一行里没有退出状态 `{status}`：{}\n\
                 ⇒ 账上只剩一个分类名，而 `KP3A`① 要的是「带退出状态与时刻」。",
                rec.line
            );
            assert!(
                rec.line.contains(&origin),
                "那一行没说是哪台机：{}",
                rec.line
            );
            assert!(rec.sink_error.is_none(), "落点好着却报了错：{:?}", rec.sink_error);
        }
    }

    /// ★★ `KP3A` 死值验：**账写不进去要出声，不许静默。**
    ///
    /// 两半缺一不可：① 那句拒收**原样**回到调用方手上（`#[must_use]` 让它吞不掉）；
    /// ② 落点坏了**不许把读数一起带走** —— 进程内那张表照样更新，
    /// 否则「写不进去」会顺带把界面上「崩过几次」清成 0，那是第二次静默。
    #[test]
    fn a_refusing_ledger_is_never_silent() {
        let origin = "kp3-写不进去-甲";
        let ev = DeathEvidence {
            outcome: Outcome::Signalled(9),
            handshake: Handshake::Spoke,
            reader: ReaderEnd::CleanEof,
            start_failure: None,
        };
        let rec = record_death(origin, &ev, &mut RefusingSink).expect("要记一笔");
        let why = rec.sink_error.as_deref().unwrap_or("");
        assert!(
            why.contains("拒收"),
            "落点拒收了，而调用方手上什么都没有（sink_error={:?}）——\n\
             那就是「没有任何东西在记它崩没崩」原地复发。",
            rec.sink_error
        );
        assert!(
            !rec.line.is_empty(),
            "拒收的时候连那一行本身都没给 —— 调用方连就地喊一声都做不到"
        );
        assert_eq!(
            health(origin).crashed,
            1,
            "落点坏了把读数也一起带走了 —— 那是同一件事上的第二次静默"
        );
    }

    /// ★★ `KP3C`：那句「无人监护」后面接得上一个**真读数**，而默认档是**答不出来**。
    ///
    /// ★ 这一条是 `§0-1` 那一格的落点：今天不是「它没崩过」，是「没有任何东西在记」。
    /// 把默认档写成 `HEALTH_CLEAN` ⇒ 本条当场红。
    #[test]
    fn the_reading_defaults_to_unknown_not_to_clean() {
        let never_touched = health("kp3-从来没被记过的一台机");
        assert_eq!(never_touched.seen(), 0, "这台机的夹具串被别的测试用过了");
        assert_eq!(
            describe_health(&never_touched),
            HEALTH_UNKNOWN,
            "一条记录都没有的机器被说成了别的 —— `§0-1` 逐字：\n\
             「今天不是『它没崩过』，是『没有任何东西在记它崩没崩』…… \
             这两句话差得很远，不许混用」。"
        );
        assert!(
            LEDGER_IS_PROCESS_LOCAL,
            "这本账变成跨进程的了 —— 那 `HEALTH_UNKNOWN` 那一档的理由就变了，回来重判"
        );
        // 崩过之后读数要跟着走，且带得出次数与最后那一行。
        let origin = "kp3-读数跟着走-甲";
        let ev = DeathEvidence {
            outcome: Outcome::Signalled(6),
            handshake: Handshake::Spoke,
            reader: ReaderEnd::CleanEof,
            start_failure: None,
        };
        let mut sink = CapturingSink::default();
        record_death(origin, &ev, &mut sink).expect("要记一笔");
        record_death(origin, &ev, &mut sink).expect("要记第二笔");
        let said = describe_health(&health(origin));
        assert!(
            said.contains("崩过 2 次") && said.contains("signal 6"),
            "读数没带出次数与最后那一行：{said}"
        );
        assert_ne!(
            said, HEALTH_UNKNOWN,
            "记了两笔，读数还说「答不出来」—— 那句话就永远只是一句静态承诺了"
        );
        // 占位符必须真的被填掉（漏一个 `replace` 会把 `{crashed}` 原样端到用户眼前）。
        for ph in ["{crashed}", "{last}", "{misread}"] {
            assert!(
                !said.contains(ph),
                "读数里还留着占位符 `{ph}`：{said}"
            );
        }
    }

    /// ★ **反空真**：生产落点真的造得出来、真的接一行。
    ///
    /// 少了这一条，下面那条源码判据在一棵**根本没有 `MonitorLog`** 的树上照样能红得对、
    /// 却证不了那个类型今天还在（`brief` 第 9 条那一族：报「有牙」要说清射程）。
    #[test]
    fn the_production_sink_takes_a_line_without_complaining() {
        let origin = "kp3-生产落点-甲";
        let ev = DeathEvidence {
            outcome: Outcome::Exited(3),
            handshake: Handshake::NeverSpoke,
            reader: ReaderEnd::CleanEof,
            start_failure: None,
        };
        let rec = record_death(origin, &ev, &mut MonitorLog).expect("要记一笔");
        assert!(
            rec.sink_error.is_none(),
            "生产落点拒收了：{:?}",
            rec.sink_error
        );
        assert!(
            rec.line.contains("exit 3"),
            "生产落点收到的那一行没带退出状态：{}",
            rec.line
        );
        assert_eq!(health(origin).refused, 1, "读数没跟上");
    }

    /// ★ 落点那一半**真的把行交给了日志**（否则 `MonitorLog` 是个 no-op，
    /// 而「真的会被写下来」这句话就只剩一个类型名）。
    ///
    /// ⚠ 这一条读的是**源码文本**：`tracing` 没有返回值，一次真调用与一次 no-op
    /// 在行为面上无法区分（要区分得装一个 capture subscriber，那是另一件事的成本）。
    /// **如实登记，不假装这是行为判据。**
    #[test]
    fn the_production_sink_really_hands_the_line_to_the_log() {
        let prod = guard_core::production_code(include_str!("daemon_policy.rs"));
        guard_core::pin_line(&prod, "tracing::error!(\"{line}\");").unwrap_or_else(|why| {
            panic!(
                "{why}\n\
                 ⇒ `MonitorLog` 不再把那一行交给日志了。\n\
                 那本账**真的会被写下来**靠的就是这一行：真正碰盘的是\n\
                 `logging.rs::build_rolling_appender`（`write_site_registry::WRITE_SITES` 里\n\
                 已登记的那条），而时刻由 `fmt` 的默认 timer 打（带日期）。\n\
                 换成一个不打时刻的落点，账上就再问不出「它上一次崩在哪一天」——\n\
                 `~/.cc-monitor/bin/wrap.log` 那两行就是那个样子。"
            )
        });
    }

    /// ★ `KP3C` 的另一半：那三支**真的写在 TS 那份文件里**。
    ///
    /// ⚠ **诚实边界**：本件够不着 vitest 的写区（`src/daemon-policy.vitest.ts` 不在写区，
    /// `settings/daemon-section.vitest.ts` 也不在）⇒ TS 那侧的**运行时**行为今天没有判据，
    /// 这一条只证「那三支写在那儿」。要真判它得在前端加一个测试文件，交回里点名了。
    #[test]
    fn the_health_reading_branches_are_wired_into_the_typescript() {
        let ts = include_str!("../../src/daemon-policy.ts");
        assert!(ts.len() > 500, "那份文件只读到 {} 字节 —— 人群坏了", ts.len());
        for line in [
            "export function describeDaemonHealth(h: DaemonHealth): string {",
            "if (seen === 0) return HEALTH_UNKNOWN;",
            "if (h.crashed === 0) return HEALTH_CLEAN.replace(\"{misread}\", String(h.misread));",
        ] {
            guard_core::pin_line(ts, line).unwrap_or_else(|why| {
                panic!(
                    "{why}\n\
                     ⇒ TS 那侧的读数分支变了。Rust 这侧 `describe_health` 与它\n\
                     **逐格对应**，而两边漂了不会有任何东西报错。"
                )
            });
        }
    }

    /// ★★ `K-P1 KPY6` 的另一半：**那条无条件禁令今天是假的，四处一处都不许留着。**
    ///
    /// 翻面之前那句话住四处（`P2s-Y5`）：`daemon_policy.rs` · `daemon_control.rs` ·
    /// `daemon-policy.ts` · `settings/daemon-section.ts`。它逐字是
    /// 「**UI 文案不许写「daemon 继续运行」**」，依据是「daemon 153ms 内自己走」。
    ///
    /// `K-P1` 之后那个依据**只在没脱离的那一支成立** ⇒ 这条**无条件**禁令是一句半假的全称。
    /// 留在四处中的任何一处，下一个人读到的就是「界面永远不许说继续跑」——
    /// 而那正好会把 `K14` 背书的那半（「如实说继续跑，无人监护」）读成违规。
    ///
    /// ⚠ 本条只查**那一句的字面**。它逮不到「换个说法写同一条无条件禁令」——
    /// 如实登记，不假装机检覆盖了它。
    #[test]
    fn the_unconditional_ban_is_gone_from_all_four_homes() {
        const HOMES: &[(&str, &str)] = &[
            ("daemon_policy.rs", include_str!("daemon_policy.rs")),
            ("daemon_control.rs", include_str!("daemon_control.rs")),
            ("src/daemon-policy.ts", include_str!("../../src/daemon-policy.ts")),
            (
                "src/settings/daemon-section.ts",
                include_str!("../../src/settings/daemon-section.ts"),
            ),
        ];
        // 针**运行时拼**：本条自己的散文里就有这几个字，写成一个整串的话它会命中自己
        //（同族自指陷阱本仓记过多次）。
        //
        // ⚠⚠ **08-26 死值验当场逮到这条针是错的**：第一版拼的是 `不许写daemon 继续运行`，
        //   而原句是 `不许写「daemon 继续运行」`——**中间隔着一对直角引号**。
        //   ⇒ 那一版**永远匹配不上原句**，本条是个安慰剂：把原句原样放回 `daemon_control.rs`，
        //   实测 `5 passed; 0 failed`（rc=0），**一点都不红**。
        //   ★ 这就是「acceptor 必须先证明它会失败」买到的东西：它红之前，我以为它有牙。
        let needle = format!(
            "{}{}{}daemon 继续运行{}",
            "不许",
            "写",
            '\u{300c}',
            '\u{300d}'
        );
        // ⚠ `.rs` 那两份**只看 `#[cfg(test)]` 之前那一段** —— 判据自己的解释性散文
        //   （包括本条的头注）住在测试段里，不切掉的话本条会**命中它自己、恒红**。
        //   ⚠ 这里不能用 `production_code`：它把 `//` 开头的行整行剥掉，
        //   而这条判据要查的**正是**那些头注散文（`//!` 也是 `//` 开头）。
        let before_tests = |s: &str| s.split("#[cfg(test)]").next().unwrap_or(s).to_string();
        let mut left: Vec<&str> = Vec::new();
        for (name, src) in HOMES {
            // 抽取器自检：四份都得真读到，切完也不能只剩个壳。
            assert!(src.len() > 500, "{name} 只读到 {} 字节 —— 人群坏了", src.len());
            let scan = if name.ends_with(".rs") {
                before_tests(src)
            } else {
                (*src).to_string()
            };
            assert!(
                scan.len() > 400,
                "{name} 切完只剩 {} 字节 —— 切法坏了，本条在空转",
                scan.len()
            );
            if scan.contains(&needle) {
                left.push(name);
            }
        }
        assert!(
            left.is_empty(),
            "这几处还留着那条**无条件**禁令：{left:?}\n\
             它的依据（「monitor 一退它 153ms 内自己走」）在 `K-P1` 之后**只对没脱离的那一支成立**。\n\
             ⇒ 留着它 = 下一个人会把 `K14` 背书的那半（如实说「继续跑，无人监护」）读成违规。\n\
             四处要**同一拍**改，这正是「不许只改一处」那条 DoD 的落点。"
        );
    }

    #[test]
    fn an_empty_origin_is_refused() {
        assert!(
            set_daemon_kill_on_exit("  ".into(), true).is_err(),
            "空 origin 必须拒。放过它等于悄悄造出一档「全局策略」，\n\
             而 `kill_on_exit(真 origin)` 永远读不到它 —— 设了没反应，且不报错。"
        );
    }

    // ── `K-P3b`：接了几处就是几处 ────────────────────────────────────────

    /// 把一段（函数体）从生产段里切出来：从 `head` 那一行的下一行起，
    /// 到第一行**恰好是右花括号**为止。形状抄 `local_daemon.rs::body_of`。
    ///
    /// ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段，
    /// 源码里多一个孤立的右花括号会让它提前闭合（`local_backend.rs` 那处逐字记过）。
    fn body_after(prod: &str, head: &str) -> String {
        let at = guard_core::find_pinned(prod, head).unwrap_or_else(|e| {
            panic!("切不出 `{head}`（{e}）—— 它改名或搬家了，来改 `DEATH_RECORD_SITES`")
        });
        prod[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// ★★ `KP3W3` 的「数」那一格：[`record_death`] 的生产调用点 == [`DEATH_RECORD_SITES`]。
    ///
    /// **相等，不是地板** —— 地板在变大方向上是瞎的（`readonly_guard::ALLOWED` 那张表的
    /// 报错文案逐字：「不许改回地板」）。
    ///
    /// 两格一起判，缺一格就漏一种：
    /// ① **总数**：整棵 `src-tauri/src` 的生产段里恰好这么多处；
    /// ② **逐处点名**：每一处的宿主函数体内**恰好一处** ——
    ///    只数总数的话，「某一处塌了、另一处多记了一次」会互相抵消
    ///    （`daemon_control.rs` 那条「逐口切体，不数全局」为同一形栽过一次）。
    ///
    /// ⚠ 人群里**没有本文件自己**：`scan_tree!` 按 `file!()` 摘掉调用者那一份，
    /// 而 [`record_death`] 的定义与它自己的单测都住这儿 —— 不摘就恒有命中。
    #[test]
    fn the_death_ledger_is_wired_at_exactly_these_sites() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"]);
        assert!(
            files.len() >= 20,
            "只扫到 {} 份 `.rs` —— 抽取坏了，本条会零命中地绿",
            files.len()
        );
        let mut scanned = 0usize;
        let mut hits: Vec<(String, usize)> = Vec::new();
        for (path, raw) in &files {
            let prod = guard_core::production_code(raw);
            scanned += prod.len();
            let n = prod.matches("record_death(").count();
            if n > 0 {
                hits.push((
                    path.file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    n,
                ));
            }
        }
        assert!(
            scanned > 200_000,
            "剥完只剩 {scanned} 字节可扫 —— 剥过头了，本条在空转"
        );
        // ⚠⚠ **逐处点名排在总数前面，这个次序是有意的**〔09-05 死值验当场逼出来的〕。
        //   先跑总数那一条时，摘掉任意一处 `record_death` 印出来的是
        //   「生产调用点是 2 处（登记 3 处）。实得：[("local_daemon.rs", 2)]」——
        //   它说得出**少了一处**，说不出**少的是哪一处**（三处都在同一个文件里）。
        //   ⇒ 先红的该是**说得出病在哪**的那句诊断，而不是要人再去查一遍的记账话。
        //   （形状抄 `local_daemon.rs::the_user_actionable_start_failures_all_reach_the_user`
        //   头注那一段：「②③ 排在 ④ 前面是有意的」。）
        for (file, head, why) in DEATH_RECORD_SITES {
            let raw: &str = match *file {
                "local_daemon.rs" => include_str!("local_daemon.rs"),
                other => panic!("`DEATH_RECORD_SITES` 里出现了没接语料的文件：{other}"),
            };
            let prod = guard_core::production_code(raw);
            let body = body_after(&prod, head);
            assert!(
                body.len() > 100,
                "`{file}` 的 `{head}` 切出来只有 {} 字节 —— 切错了，这一格在空转",
                body.len()
            );
            let n = body.matches("record_death(").count();
            assert_eq!(
                n, 1,
                "`{file}` 的 `{head}` 体内 `record_death(` 有 {n} 处（该恰好 1 处）。\n\
                 这一处记的是：{why}\n\
                 ★ 0 处 = **这条路的死亡从此没人记**；而只数总数的话，\n\
                 「这一处塌了、另一处多记了一次」会互相抵消，谁都不出声。"
            );
        }
        let total: usize = hits.iter().map(|(_, n)| *n).sum();
        assert_eq!(
            total,
            DEATH_RECORD_SITES.len(),
            "`record_death` 的生产调用点是 {total} 处（登记 {} 处）。实得：{hits:?}\n\
             ★ **接了几处就是几处**。变少 = 某一条观测路又回到了「看得见它没了、却没人记」，\n\
             那正是 `K-P3` `§3-5` 第一行登记的那一格（当时这个数是 0）；\n\
             变多 = 有第四条路开始记账（上面逐处那一圈只看登记过的三处，\n\
             **第四处它一个字都不会说**）⇒ 回来把它写进 `DEATH_RECORD_SITES`，\n\
             并说清它记的是哪条路。",
            DEATH_RECORD_SITES.len()
        );
    }

    /// ★★ `KP3W3`：**监护器自己一笔都不许记。**
    ///
    /// 同一个 `supervise_with_stdio` 今天有两种客户：daemon 与中转。
    /// 记账落进监护器体内 ⇒ 中转的死会记到「这台机的 daemon」头上，
    /// 而那本账的定义就是「这台机的 daemon」的账（`§0a` 逐字）。
    /// ⇒ 接线必须落在**客户这一侧**的 `on_event` 上。
    #[test]
    fn the_supervisor_itself_never_records_a_death() {
        let prod = guard_core::production_code(include_str!("backend/control/local_backend.rs"));
        assert!(
            prod.len() > 10_000,
            "剥完只剩 {} 字节 —— 本条在空转",
            prod.len()
        );
        let body = body_after(&prod, "pub fn supervise_with_stdio(");
        assert!(
            body.len() > 1_000,
            "`supervise_with_stdio` 切出来只有 {} 字节 —— 切错了",
            body.len()
        );
        assert_eq!(
            body.matches("record_death(").count(),
            0,
            "`supervise_with_stdio` 体内出现了 `record_death(` ——\n\
             ★ 它同时监护 daemon 与中转 ⇒ 中转的死会被记进「这台机的 daemon」那本账。\n\
             ⇒ 记账落在客户那一侧的 `on_event`（`local_daemon::daemon_supervise_events`）。"
        );
        // 整棵 `backend/` 也是 0 —— 判与记都不在那一半（`B1` 那次误诊正是判断落在 backend 层的产物）。
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/backend");
        let files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"]);
        assert!(
            files.len() >= 5,
            "只扫到 {} 份 backend 文件 —— 抽取坏了，本条在空转",
            files.len()
        );
        let mut offenders: Vec<String> = Vec::new();
        for (path, raw) in &files {
            if guard_core::production_code(raw).contains("record_death(") {
                offenders.push(path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "`backend/` 的生产段里出现了 `record_death(`：{offenders:?}\n\
             ⇒ 判与记该在宿主层。`backend/` 那半**只搬证据**（`SuperviseEvent::Exited` 的\n\
             `status` / `witness` 两个字段就是它搬的全部）。"
        );
    }

    /// ★ `K-P3b`：**「根本没有读端」不是一次干净 EOF，也永远不是「读坏了」。**
    ///
    /// 死值验：把 `verdict` 里 `ReaderEnd::NotObserved` 那一臂改成
    /// `return Some(Death::Misread { .. })` ⇒ 下面两格一起红。
    #[test]
    fn a_reader_that_never_existed_is_neither_a_clean_eof_nor_a_misread() {
        assert_ne!(
            ReaderEnd::NotObserved,
            ReaderEnd::CleanEof,
            "「根本没有读端」与「干净 EOF」变成同一个值了 —— \
             那句「管子关了 / 它走了」对一条从来没有过管子的路是假话"
        );
        // ① 从来没起来：这一维让开，判定由剩下两维给出。
        let never = DeathEvidence {
            outcome: Outcome::NeverSpawned,
            handshake: Handshake::NeverSpoke,
            reader: ReaderEnd::NotObserved,
            start_failure: Some(("拿不到那一份".to_string(), Vec::new())),
        };
        assert_eq!(
            death_kind(&verdict(&never).expect("起不来是一件要上账的事")),
            "从来没起来",
            "没有读端被判成了别的 —— 「读坏了」说的是我们这一侧读**出了错**，\
             而那条路上连读端都没有过"
        );
        // ② 非空对照：同一个 `NotObserved` 配一个真的异常终止 ⇒ 仍然判「崩了」，
        //    这一维不许把判定拽走。
        let signalled = DeathEvidence {
            outcome: Outcome::Signalled(9),
            handshake: Handshake::Spoke,
            reader: ReaderEnd::NotObserved,
            start_failure: None,
        };
        assert_eq!(
            death_kind(&verdict(&signalled).expect("被信号打死要上账")),
            "崩了",
            "`NotObserved` 把一次真的异常终止判成了别的格"
        );
    }
}
