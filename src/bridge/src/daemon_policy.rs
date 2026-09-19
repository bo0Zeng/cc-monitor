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
//! ⇒ 禁令换成**按状态分档**（`K-P1 KPY4`，由 `tests/settings/daemon-section.vitest.ts` 机检）：
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
pub const EXIT_SELF_DIES: &str = "monitor 不主动结束它；它仍会在 monitor 退出后很快自行退出";

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
// # 为什么这一档住在宿主侧，而不是预批的 `src/backend/death_ledger.rs`
//
// 三条现打，逐条给住址（PM 补充里预批了那个新文件 + `main.rs` 一行 `mod`，本件**都没用**）：
//
// 1. **daemon 写不了盘，而放宽写盘口是第二档的事。**
//    `src/backend/readonly_guard.rs` 的默认层禁掉本 crate 生产段里
//    全部 `fs::write` / `File::create` / `OpenOptions` …，白名单**恰好一个模块**
//    （`control/fork_write.rs`，`assert_eq!(whitelisted, 1)`）⇒ 在 daemon 侧开第二个
//    写盘口 = 动一条相等断言 = **放宽**，而 `§0-7` 那张表把「零断言放宽」写进了第一档。
// 2. **daemon 说出口的话在「前端没开时」落不到任何地方。**
//    `local_daemon.rs::spawn_detached` 现打 `.stdout(Stdio::null())` + `.stderr(Stdio::null())`
//    ⇒ 脱离那条路上 daemon 的 `tracing` 全部进 `/dev/null`。
//    ⚠ 这一条比第 1 条更硬：**它不是权限问题，是那句话没有听众。**
// 3. **宿主叫不动 daemon 那侧的代码。** `src/backend` 只有 `[[bin]]`、没有 `[lib]`
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
/// 形状照 `src/backend/readonly_guard.rs::ALLOWED` 那种
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
#[path = "../../../tests/bridge/daemon_policy_tests.rs"]
mod tests;
