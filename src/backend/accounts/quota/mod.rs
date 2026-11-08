//! 额度与轮换：账号域里用量的唯一住址（额度账）＋ 满了换号的规则（轮换）。
//!
//! 用量只记在额度账这一处；同一种事实两个来源：中转经手的回包头（读法住适配层，[`crate::agents::quota_read_of`]）·
//! 官方客户端报的用量（帧命令 `quota-probe`，本体 [`probe`]，读法同住适配层，[`crate::agents::usage_of`]）。

pub mod facts;
pub mod ledger;
pub mod name_words;
pub mod nodes;
pub mod probe;
pub mod rotation;
pub mod rules;
pub mod setup;
pub mod show;
pub mod spend;
pub mod strategy;
pub mod warm;

/// ★ 派生事实「已重置、未计时」的唯一判法：这个窗口（或卡着的那一处）说的重置时刻已经过了 ⇒ 之后没再看到它，
/// 上次的数不再作数（用量当 0、窗口没开）。说不出几点重置 ⇒ 不算（还在那一期里）。
pub(crate) fn reset_since_seen(resets_at: Option<u64>, now: u64) -> bool {
    resets_at.is_some_and(|t| t <= now)
}

/// 基础层要的「会话用哪份策略问谁」：跟着那份 `rotation.json` 的配置编辑层（两层的接线只在这里与 `faces/rotation_face.rs`）。
pub fn adoptions_at() -> rules::AdoptionsAt {
    std::sync::Arc::new(|store| {
        std::sync::Arc::new(setup::uses::Setup::of(std::sync::Arc::clone(store)))
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/quota/layer_guard.rs"]
mod layer_guard;
