//! 能力与版本：期望的协议版本与后端 build、hello 的能力 token → 流模式 flag、已验 build 的自证记忆。

use crate::copy_table::copy_text;
use std::sync::Mutex;

/// 冷启动预检的自证记忆：远端已经是当前 build 时，每次重连都付的那两条预检连接（`byte_table::probe_key` 的 `uname -s -m` 一次性 exec ·
/// 读远端 `.build_id` 的那一趟）可以跳过。记的是 hello 自报的 build_id（「这台机器上一次真的跑起来的后端就是当前期望的那个」），不是预检算出来的结论。
/// 跳的条件只有一个：记忆里那台机器的 build_id 恰好等于「我这一版」（`byte_table::my_backend_id`）；其余一律照跑。miss 路径的 caps 决策本身是保守的
/// （`hello_confirmed` → 部署侧确认 → 空集全降级）。记忆过期（远端二进制被删 / 换旧）⇒ 本轮 exec 失败，失败路径清掉记忆 ⇒ 下一轮重新预检并部署。代价是多一次重连。
static VERIFIED_BUILD: Mutex<Option<std::collections::HashMap<String, String>>> = Mutex::new(None);

/// 纯函数：这一轮能不能跳过那两条预检连接。`verified` = [`VERIFIED_BUILD`] 里这台机器的记录（`None` = 没记过）；`mine` = 「我这一版」（`None` = 手上没带后端字节）。
/// 逐字相等，不是包含（前缀相等会把 `abc123` 与 `abc123-dirty` 混为一谈）；`mine` 是 `None` ⇒ 恒不跳。
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

/// 版本那条健康信息（`RemoteHealthPayload::kind`）的三类：界面按它挑标题（`remote-health.ts::headlineFor`），不自己判。
pub(super) const VERSION_KIND_OLDER: &str = "version-older";
pub(super) const VERSION_KIND_NEWER: &str = "version-newer";
pub(super) const VERSION_KIND_INCOMPARABLE: &str = "version-incomparable";

/// 版本关系 → 健康信息的类别（同 ⇒ 不说）。协议不兼容在 [`version_relation`] 里已归「那台旧」。
pub(super) fn version_health_kind(
    rel: crate::machine_state::VersionRelation,
) -> Option<&'static str> {
    use crate::machine_state::VersionRelation as R;
    match rel {
        R::Same => None,
        R::Older => Some(VERSION_KIND_OLDER),
        R::Newer => Some(VERSION_KIND_NEWER),
        R::Incomparable => Some(VERSION_KIND_INCOMPARABLE),
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
        // 按新旧分两句（部署只升不降）：那台旧 ⇒ 下次连上的部署预检会换掉它；那台不比这一版旧 ⇒ 这个 monitor 不会把它换回去。
        // 新旧不在这里比：本机常驻后端接上那一刻判过（`resident-verdict`，`control/deploy_plan.rs`），这里只按答案挑句子。
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
