//! 查询层：每条帧命令的本体（只读扫描图切页 · 拼成品）；对外的面（`faces/`）只留入参 · 码与接线。

pub(crate) mod body;
pub(crate) mod branch;
pub(crate) mod index;
pub(crate) mod inputs;
pub(crate) mod locate;
pub(crate) mod tail;
pub(crate) mod turns;

// ───────────────────────── 分页（index · user-inputs · turns · branch）─────────────────────────

/// 一条应答那一行里信封那一截的余量（`{"kind":"reply","id":…,"ok":true,"data":` 与收尾；`id` 是客户端起的短号）。
pub(crate) const ENVELOPE_BYTES: usize = 256;

/// **一页的装法**（四条分页命令共用这一份）：先占住回包里不随条目变的那一截（空回包按最长的续点序列化）＋ 信封，
/// 再一项一项加它序列化后的字节数（上界）；装不下 · 到了 `limit` 项 ⇒ 停，下一项就是下一页的头。
/// ⇒ 一条应答一行 ≤ 上限。一页至少装一项：一项单独就比上限大 ⇒ 这一页只有它（不切一项）。
pub(crate) struct Pager {
    room: usize,
    used: usize,
    limit: usize,
    taken: usize,
}

impl Pager {
    pub(crate) fn new(cap: usize, fixed: usize, limit: Option<u64>) -> Self {
        Pager {
            room: cap.saturating_sub(fixed + ENVELOPE_BYTES),
            used: 0,
            limit: limit.map_or(usize::MAX, |n| {
                usize::try_from(n.max(1)).unwrap_or(usize::MAX)
            }),
            taken: 0,
        }
    }

    /// 这一项（`cost` 字节）装得下 ⇒ 记上、回 `true`；装不下 ⇒ `false`（这一页到此为止）。
    pub(crate) fn admit(&mut self, cost: usize) -> bool {
        if self.taken >= self.limit || (self.taken > 0 && self.used + cost > self.room) {
            return false;
        }
        self.used += cost;
        self.taken += 1;
        true
    }

    /// 最后装进去的那一项换了个样子（还没收尾那一轮按此刻的会话事实补了字）：照新的字节数改账；超了 ⇒ `false`（调用方把它挪到下一页）。
    pub(crate) fn redo_last(&mut self, old: usize, new: usize) -> bool {
        self.used = self.used - old + new;
        self.taken <= 1 || self.used <= self.room
    }
}

/// 一个值序列化后几个字节。
pub(crate) fn jlen<T: serde::Serialize + ?Sized>(v: &T) -> usize {
    serde_json::to_vec(v).map_or(0, |b| b.len())
}

/// 一个数写成十进制几个字。
pub(crate) fn digits(n: u64) -> usize {
    n.checked_ilog10().map_or(1, |d| d as usize + 1)
}
