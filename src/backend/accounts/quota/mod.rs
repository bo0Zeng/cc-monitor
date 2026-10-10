//! 额度与轮换：账号域里用量的唯一住址（额度账）＋ 满了换号的规则（轮换）。
//!
//! 用量只记在额度账这一处；同一种事实两个来源：中转经手的回包头（读法住适配层，[`crate::agents::quota_read_of`]）·
//! 官方客户端报的用量（帧命令 `quota-probe`，读法同住适配层，[`crate::agents::usage_of`]）。

pub(crate) mod decide;
pub(crate) mod ledger;
pub(crate) mod name_words;
pub(crate) mod rotation;
pub(crate) mod rule_text;
pub(crate) mod show;

/// ★ 派生事实「已重置、未计时」的唯一判法：这个窗口（或卡着的那一处）说的重置时刻已经过了 ⇒ 之后没再看到它，
/// 上次的数不再作数（用量当 0、窗口没开）。说不出几点重置 ⇒ 不算（还在那一期里）。
pub(crate) fn reset_since_seen(resets_at: Option<u64>, now: u64) -> bool {
    resets_at.is_some_and(|t| t <= now)
}

/// 此刻（unix 秒）。
pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
