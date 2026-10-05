//! 额度与轮换：账号域里用量的唯一住址（额度账）＋ 满了换号的规则（轮换）＋ 空闲的号替它起算 5h 窗口（自动起算）。
//!
//! 数据只有一个来源：中转经手的回包头（读法住适配层，[`crate::agents::quota_read_of`]）。

pub(crate) mod autostart;
pub(crate) mod autostart_send;
pub(crate) mod decide;
pub(crate) mod ledger;
pub(crate) mod rotation;
pub(crate) mod show;

/// 此刻（unix 秒）。
pub(crate) fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
