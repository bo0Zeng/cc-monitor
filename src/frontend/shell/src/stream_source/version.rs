//! 能力与版本：期望的协议版本与后端 build、hello 的能力 token → 流模式 flag、已验 build 的自证记忆。

use crate::copy_table::copy_text;
use std::sync::Mutex;

/// 冷启动预检的**自证记忆**〔audit-0805 F05 下半，报告「可选 12」〕。
///
/// # 冷启动今天付三条 SSH 连接，其中两条常常是白付的
///
/// 实测（08-06 读码逐条对上）：
///
/// | # | 连接 | 谁发起 |
/// |---|---|---|
/// | ① | `uname -m` 一次性 exec（选内嵌二进制的 arch；今天问 `uname -s -m`） | `byte_table::probe_key` |
/// | ② | SFTP 连接（读远端 `.build_id` marker） | `sftp::connect_sftp` |
/// | ③ | 接那台的常驻后端（`--resident-ensure` 一次 capture ＋ 隧道） | `remote_resident::attach` |
///
/// ①② 同属 `ensure_backend_deployed`。**即使远端已经是当前 build、什么都不用部署，
/// 每次重连也照付这两条**（各含一次 TCP + 握手 + 指纹校验 + auth）。
///
/// # 记什么：hello **自报**的 build_id，不是预检算出来的结论
///
/// 报告的原提法是「把 `arch`/`build_id` 记进 memo」。**记预检结论是猜，记 hello 是自证** ——
/// 只有后端自己说「我是 D」才写进来。于是这份记忆的含义是
/// 「**这台机器上一次真的跑起来的后端就是当前期望的那个**」，
/// 而不是「上一次我们检查时它看起来是对的」。
///
/// # 为什么这样跳预检是保守的
///
/// 跳的条件**只有一个**：记忆里那台机器的 build_id **恰好等于**「我这一版」（`byte_table::my_backend_id`）。
/// 其余一律照跑（无记忆 / 记的是别的 build / 手上没带后端字节）—— 那些情况本来就**可能需要部署**，或根本比不了，不能跳。
///
/// ⚠ 功能件 §8 把「缓存 miss 时 caps 决策必须保守」写成了这件事的阻塞。
/// 逐字复核之后：那句话约束的是 **miss 路径**，而 miss 路径的答案**早就在代码里** ——
/// `caps` 的三级阶梯（`hello_confirmed` → 部署侧确认 → **空集全降级**）本身就是保守的。
/// ⇒ 它挡住的是一部分（miss 怎么办），**不是整件**（hit 能不能跳）。
///
/// # 记忆过期怎么办（**如实写在这里**）
///
/// 远端二进制被人删掉/换旧、而记忆还说「是期望 build」时，本轮会跳过预检直接起流 ⇒
/// **exec 失败**。失败路径清掉记忆 ⇒ **下一轮重新预检并重新部署**。
/// 代价是**多一次重连**，不是永久坏掉。这是本设计唯一的退化，写下来不藏着。
static VERIFIED_BUILD: Mutex<Option<std::collections::HashMap<String, String>>> = Mutex::new(None);

/// 纯函数：这一轮**能不能跳过**那两条预检连接。
///
/// `verified` = [`VERIFIED_BUILD`] 里这台机器的记录（`None` = 没记过）；`mine` = 「我这一版」（`None` = 手上没带后端字节）。
/// 判据是**逐字相等**，不是包含 —— 前缀相等会让 `abc123` 与 `abc123-dirty` 混为一谈
/// （本区 F24 那一族）。`mine` 是 `None` ⇒ 恒不跳（没有对照物，「那台就是这一版」无从说起）。
pub(super) fn preflight_can_be_skipped(verified: Option<&str>, mine: Option<&str>) -> bool {
    matches!((verified, mine), (Some(v), Some(m)) if v == m)
}

/// 读这台机器的自证记录。
pub(super) fn verified_build_of(origin: &str) -> Option<String> {
    VERIFIED_BUILD.lock().ok()?.as_ref()?.get(origin).cloned()
}

/// 记下「这台机器上一次真的跑起来的后端是 `build_id`」。
///
/// ⚠ **只许在收到 hello 的那一处调**（backend 自报）。别处调就把「自证」变回了「猜」。
pub(super) fn record_verified_build(origin: &str, build_id: &str) {
    if let Ok(mut g) = VERIFIED_BUILD.lock() {
        g.get_or_insert_with(std::collections::HashMap::new)
            .insert(origin.to_string(), build_id.to_string());
    }
}

/// 抹掉这台机器的自证记录（连接没起来 / backend 换了身份）。
pub(super) fn forget_verified_build(origin: &str) {
    if let Ok(mut g) = VERIFIED_BUILD.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(origin);
        }
    }
}

/// 连接远端、鉴权、开 session channel、exec [`BACKEND_CMD`]，
/// 返回 channel 的双向流（`AsyncRead + AsyncWrite`）——读端即后端的 stdout 数据。
///
/// 鉴权委托给 [`connect_session`]（publickey 或 ssh-agent）。
/// 错误统一 map 成 `String`（本 crate 未直接依赖 anyhow，不为骨架引入新依赖）。
/// F66（#58③）流模式门控决策（纯函数，矩阵单测）：**从后端声明的能力 token 决定
/// 发哪些 flag**，不再靠 build_id 精确匹配（Batch7-F24/Batch8-F26 的旧机制）。
///
/// - `capabilities` = backend hello 自报的能力集（旧后端无声明 → 空集）。
/// - 空集（旧 backend / 尚未确认）→ `(false, false)`：全降级 = 2.18.0 行为，功能退化但
///   连接正常。
/// - `tail_only`（历史改走旁路快照，拥塞根除）需后端声明 `"tail-only"`。
/// - `with_bg`（放行 bg 会话）需后端声明 `"bg"` **且**用户开了 `show_bg`。
///
/// **§26 死循环护栏靠声明本身保住**：旧后端把未知 flag 当一次性查询 → 退出 → 无
/// hello → 重连死循环。而只有**会先剥离该 flag** 的后端才声明对应能力（见 backend
/// `CAPABILITIES` 注释），故「声明了 = 发该 flag 安全」——比 build_id 精确匹配更强更干净，
/// 且直接闭合 2026-07-09「身份确认不了就全降级」事故（能力由后端自报，不靠脆弱身份链）。
/// monitor **认识**的能力 token：契约 crate 那一份（后端 `CAPABILITIES` 取的也是它）。
///
/// U-CC1：它与 [`decide_stream_flags`] 是同一份事实 —— 由
/// `known_capability_tokens_match_decide_stream_flags` 钉住。
/// 有它才能回答「backend 声明了一个我们不认识的能力」这个问题（漂移记账的第四个面）。
const KNOWN_CAPABILITY_TOKENS: &[&str] = deploy_contract::STREAM_CAPABILITIES;

/// U-CC1 第四个面的写点：hello 里**不认识的**能力 token 记一笔。记在 `origin`（那台远端）名下。
/// 只记账，行为一字不改（不认识的 token 本来就按保守缺省忽略）。
pub(super) fn note_unknown_capabilities(
    origin: &crate::origin::Origin,
    capabilities: &[String],
    build_id: &str,
) {
    for t in capabilities {
        if !KNOWN_CAPABILITY_TOKENS.contains(&t.as_str()) {
            crate::drift_ledger::record(
                origin,
                crate::drift_ledger::DriftFace::UnknownBackendToken,
                &format!("capabilities:{t}"),
                Some(&format!("build_id={build_id}")),
            );
        }
    }
}

/// 两位流模式 flag：`(with_bg, tail_only)`。改它会连带 [`should_upgrade_reconnect`]（防无限重连的收敛判据）。
pub(super) fn decide_stream_flags(capabilities: &[String], show_bg: bool) -> (bool, bool) {
    let has = |c: &str| capabilities.iter().any(|t| t == c);
    (show_bg && has("bg"), has("tail-only"))
}

/// F66（#58③）★ 防无限重连的收敛判据（纯函数，穷举单测）：收到后端能力声明后，
/// **是否值得重连一轮升级流模式**。`cur` = 本轮实际发的 `(with_bg, tail_only)`；`next` =
/// 据后端自报能力算出的下一轮 flag。
///
/// **仅当下一轮会开一个本轮关着的 flag** 才重连——每次重连严格增开 flag，flag 数有限
/// ⟹ 最多 2 轮收敛，绝不无限重连。**关键定理**：一旦记账
/// `hello_confirmed=Some(D)`，下一轮 `caps=D` ⟹ `next==cur` ⟹ 本函数两项皆自相矛盾
/// （`next_x && !cur_x` 在 next==cur 时恒 false）⟹ 恒 `false`，不再重连。
///
/// 两项都写全 `&& !cur_*`（不靠调用点的外层 guard），使收敛不变式在函数内自洽、
/// 可独立穷举测试（审计：原 `next_tail` 裸项隐含依赖外层 guard，读者需回连才懂）。
///
pub(super) fn should_upgrade_reconnect(cur: (bool, bool), next: (bool, bool)) -> bool {
    let ((cur_bg, cur_tail), (next_bg, next_tail)) = (cur, next);
    (next_tail && !cur_tail) || (next_bg && !cur_bg)
}

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/coldstart_preflight_guard.rs"]
mod coldstart_preflight_guard;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/coldstart_perf_guard.rs"]
mod coldstart_perf_guard;

#[cfg(test)]
#[path = "../../../../../tests/frontend/shell/stream_source/stream_flag_gate_tests.rs"]
mod stream_flag_gate_tests;

// ============================================================================
// 版本协商（issue #33）：连接时比对后端的 hello.v / hello.build_id。
// ============================================================================

/// 本 monitor 期望的流式 wire 协议大版本。与后端的 `PROTO_VERSION` 对齐（语义同值）。
/// 类型用 `u64` 而非后端侧的 `u32`：JSON 数字无符号宽度之分，`parse_frame` 用
/// `as_u64()` 读 `v`，这里与之同宽以便直接比较，无需转换。
pub(super) const EXPECTED_PROTO_V: u64 = 1;

/// F66（#58③）：monitor **内嵌** backend 声明的能力 token（契约 crate `STREAM_CAPABILITIES`，后端 hello 交的是同一份）。
///
/// 用途：部署侧确认「那台装的就是手上这一版」时，第一次连接还没收到 hello，用这份常量预知后端能力、
/// 直接发对应 flag —— 省一轮「降级→收 hello→重连升级」的往返。收到真实 hello 后一律以 backend
/// **自报**的 `capabilities` 为准（见 `hello_confirmed`）。
pub(super) fn embedded_backend_capabilities() -> Vec<String> {
    deploy_contract::STREAM_CAPABILITIES
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// 版本协商结论（纯函数 [`negotiate_version`] 的产物）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionVerdict {
    /// 协议版本 + build_id 都匹配 —— 无需提示。
    Ok,
    /// 协议版本相同，但手上没带后端字节（「我这一版」是 `None`）—— 版本不可比：不判旧、不发起换装，只按那台报的接。
    Incomparable { reported: String },
    /// 协议版本相同，但 build_id 与「我这一版」（`mine`）不同 —— backend 偏旧/偏新（非阻断；旧的下次连上换掉）。
    StaleBuild { reported: String, mine: String },
    /// 协议大版本不符 —— 渲染可能异常，醒目提示需更新后端（仍不 hard-disconnect：
    /// 解析器向前兼容，能解析的仍照常呈现）。
    Incompatible { reported_v: u64 },
}

/// 纯函数版本协商：协议版本优先于 build_id（协议不兼容是更严重的问题）。`mine` = 「我这一版」（`byte_table::my_backend_id`）。
///
/// - `reported_v != EXPECTED_PROTO_V` → `Incompatible`（无论 build_id）。
/// - 协议同、`mine` 是 `None` → `Incomparable`（手上没带后端字节，没有对照物）。
/// - 协议同、`reported_build_id != mine` → `StaleBuild`。
/// - 全同 → `Ok`。
pub(super) fn negotiate_version(
    reported_v: u64,
    reported_build_id: &str,
    mine: Option<&str>,
) -> VersionVerdict {
    match mine {
        _ if reported_v != EXPECTED_PROTO_V => VersionVerdict::Incompatible { reported_v },
        None => VersionVerdict::Incomparable {
            reported: reported_build_id.to_string(),
        },
        Some(m) if reported_build_id != m => VersionVerdict::StaleBuild {
            reported: reported_build_id.to_string(),
            mine: m.to_string(),
        },
        Some(_) => VersionVerdict::Ok,
    }
}

/// 版本关系判一处（机器状态成品那一格）：同 · 那台旧（含协议大版本不符）· 那台新 · 不可比。
pub(super) fn version_relation(
    reported_v: u64,
    reported_build_id: &str,
    remote_older: bool,
    mine: Option<&str>,
) -> crate::machine_state::VersionRelation {
    use crate::machine_state::VersionRelation as R;
    match negotiate_version(reported_v, reported_build_id, mine) {
        VersionVerdict::Ok => R::Same,
        VersionVerdict::Incomparable { .. } => R::Incomparable,
        VersionVerdict::StaleBuild { .. } if remote_older => R::Older,
        VersionVerdict::StaleBuild { .. } => R::Newer,
        VersionVerdict::Incompatible { .. } => R::Older,
    }
}

/// 把协商结论变成给用户看的提示文案（`None` = 兼容、无需提示）。`label` 是出问题的远端机器。
/// `remote_older` = 接上那一刻本机常驻后端答的「那台比手上这一版旧」（`remote_resident::Replayed::remote_is_older`）。
pub(super) fn version_warning(
    reported_v: u64,
    reported_build_id: &str,
    label: &str,
    remote_older: bool,
    mine: Option<&str>,
) -> Option<String> {
    match negotiate_version(reported_v, reported_build_id, mine) {
        VersionVerdict::Ok => None,
        VersionVerdict::Incomparable { reported } => Some(copy_text(
            "rsSshSource.version.noOwnBytes",
            &[
                ("label", &label.to_string()),
                ("reported", &reported.to_string()),
            ],
        )),
        // 按新旧分两句（部署只升不降）：
        //   那台旧 ⇒ 下次连上的部署预检会换掉它；那台不比这一版旧 ⇒ 这个 monitor 不会把它换回去。
        // 新旧不在这里比（从前调共享判定 `is_newer`，今天住后端 `control/deploy_plan.rs`）：本机常驻后端接上那一刻判过（`resident-verdict`），这里只按答挑句子。
        //   〔墓碑 —— 从前一句话不分新旧（`rsSshSource.version.buildMismatch`：「…建议更新后端（后续将支持自动部署）」），自动部署早已落地。〕
        VersionVerdict::StaleBuild { reported, mine } if remote_older => Some(copy_text(
            "rsSshSource.version.remoteOlder",
            &[
                ("label", &label.to_string()),
                ("reported", &reported.to_string()),
                ("mine", &mine.to_string()),
            ],
        )),
        VersionVerdict::StaleBuild { reported, mine } => Some(copy_text(
            "rsSshSource.version.remoteNotOlder",
            &[
                ("label", &label.to_string()),
                ("reported", &reported.to_string()),
                ("mine", &mine.to_string()),
            ],
        )),
        VersionVerdict::Incompatible { .. } => Some(copy_text(
            "rsSshSource.version.protoMismatch",
            &[("label", &label.to_string())],
        )),
    }
}
