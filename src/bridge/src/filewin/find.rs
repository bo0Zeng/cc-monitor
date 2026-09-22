//! 〔步 `24f` **第四刀** · 2026-09-21〕**把搜索接到那一个原生窗口上** ——
//! `files-read` 这一族在**客户端侧**的命令面与消费面。
//!
//! 设计住 `调研/设计/60 §3.5`（搜索是唯一必须走后端的那一件）· `§3.5.2a`
//! （机制／偏好／节拍那条裁决）· `§3.5.3`（边界）· `调研/设计/96 §2.9`（那张能力表）。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 一、这一刀之前那半边是什么状态
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 后端那半**做完了**（六条能力 ＋ 两个命令面 ＋ 契约文档），而且**零调用方** ——
//! `src/backend/lib.rs` 与 `src/backend/files/mod.rs` 的头注各逐字记着一句：
//! 「零生产调用方 ⇒ 真机上 `files-find` 恒回 `index_missing: true`」。
//! 本模块就是那句话的对侧：**这里是第一个发那几条命令的人。**
//!
//! ⚠ 现打复核过（2026-09-21，本刀开工前）：这一刀之前，`files-find` /
//! `files-index-status` / `files-index-rebuild` / `files-browse` 四个线上名
//! 在 `src/bridge/` 与 `src/*.ts` 里**一处都没有**，全部命中都在 `src/backend/`、
//! `src/doc/IPC-PROTOCOL.md` 与 `tests/backend/` 里。**题面与现打一致。**
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 二、名字：线上用连字符，**这里一个点都不许出现**
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 能力名是 `files.find` 那一套，线上名是 `files-find` 那一套。两者刻意不同形，
//! 理由不是排版（`设计/96 §2.9` 逐字）：本仓三个取词器的字符集都是
//! 「字母数字 / `_` / `-`」，带 `.` 的字面量会被它们**静默丢弃** ——
//! 那等于把一条命令从判据底下抽走，而判据照常报绿。
//!
//! ⇒ 本模块只持有**线上名**（[`COMMANDS`]），而且
//! [`tests::the_wire_names_never_carry_a_dot`] 钉住这四个串里一个 `.` 都没有。
//! 能力名那一套在客户端侧**压根不出现** —— 这一侧不需要它。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 三、`§3.5.2a` 那条节奏缺口：本刀**把它补上了**，补法与射程逐条写清
//! ═══════════════════════════════════════════════════════════════════════
//!
//! `设计/60 §3.5.2a` 的裁决是三层：**机制**在后端（`rebuild_once`）·
//! **偏好**由后端声明（`REWALK_INTERVAL_SECS`，摆在 `files-index-status` 的回答里）·
//! **节拍**归调用方。而它同拍登记了一个缺口：
//!
//! > 调用方不发那条重走命令，索引就永远不会自己变新 —— 而「调用方到底发不发」
//! > 后端那棵树的判据钉不住（它在另一棵树上）。⇒ 欠一条判据，住址在 `src/bridge` 那一侧。
//!
//! **那条判据就住本模块的 [`tests`]**，两侧都钉：
//!
//! | 判据 | 钉什么 |
//! |---|---|
//! | [`tests::the_window_sends_a_rebuild_when_the_backend_says_the_index_is_missing`] | 后端说「没建过」⇒ 线上**真的**多出一条 `files-index-rebuild`（数出来**恰好 1 条**） |
//! | [`tests::a_fresh_index_is_never_rebuilt_behind_the_users_back`] | 后端说「建过、不 stale」⇒ 那条命令**一条都没发**（阴性对照） |
//! | [`tests::many_keystrokes_in_flight_still_only_trigger_one_rebuild`] | 连打多趟查询时重走**只发一趟**（`files-index-rebuild` 没有并发保护，`src/doc/IPC-PROTOCOL.md` 逐字） |
//!
//! ## 🔴 节拍是什么、以及**我为什么没有定那个周期**
//!
//! `设计/99 §2 Q4` 逐字记着「周期该定多少」**还没拍板** ⇒ 本模块**不定那个数**。
//! 它定的是**在什么事件上问**：
//!
//! - 事件源 = **用户在搜索框里打字**（外加那颗「重建索引」按钮）。**没有定时器**
//!   （`rust_timer_registry` 的人群里本模块生产段一个 `sleep` / `interval` 都没有）；
//! - 判「要不要重走」用的是**后端自己算出来的那两个布尔**：`index_missing` 与 `stale`。
//!   而 `stale` 逐字是 `age_secs > rewalk_interval_secs`，**两个数都是后端的**
//!   ⇒ 周期换了值，这一侧一个字节都不用改，也不会有第二份「300」躺在客户端。
//!
//! ⇒ 这正是 `设计/01 §5 D3` 那条「诚实的默认 ＝ 沿用调用者已有状态」在这一格的样子。
//!
//! ## ⚠ 补上了的是哪一格、**没**补上的是哪一格（别读宽）
//!
//! - ✅ **「调用方真的会发那条命令」有判据了。** 那是 `§3.5.2a` 登记的缺口本身。
//! - ❌ **「不看这个窗口的时候索引也会变新」没有，而且本刀刻意不做。**
//!   窗口关着 / 用户不搜的时候，一条命令都不发 ⇒ 索引就停在那儿。
//!   要那一格得有一个**与用户动作无关的节拍**，而 monitor 侧的每一个节拍都要进
//!   `rust_timer_registry` 并说清「谁退役它」—— 那是一件独立的活，**如实登记为未做**。
//! - ❌ **`browse_watch::BrowseWatcher` 仍然零生产调用方**（那在后端那棵树上）
//!   ⇒ [`CMD_BROWSE`] 买到的是「发命令那一刻那几个目录的子项是新的」，
//!   **不是**「此后一有动静就跟着新」。`设计/96 §2.9` 逐字登记过，这一侧照抄一遍以免读宽。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 四、新鲜度：显示的**必须**是后端报的那个数
//! ═══════════════════════════════════════════════════════════════════════
//!
//! `设计/60 §3.5.3` 逐字要求那个延迟**写在界面上**，而那一条此前是 ⬜
//! （「界面那一侧还没消费它」）。本刀的兑现处是 [`freshness_line`] ——
//! 一个**纯函数**，把 [`IndexStatus`] 那几个字段摆成一行字。
//!
//! 🔴 **判据买的是「显示出来了」，不是「盘上有」**：
//! [`tests::the_freshness_numbers_the_backend_reports_really_reach_the_frame`]
//! 真跑一帧 [`super::shell::FileWindow::frame_body`]，从 egui 这一帧交出去的 galley 里
//! 把文字读回来，断言后端报的那几个数**在那一帧上**。
//! 而且它喂**两组不同的数**，断言画出来的跟着变 ——
//! 一个写死的 `300` 会在第二组上当场红（`真相源/99 §9.1`：
//! 「调了那个看起来对的 API 只证明盘上有」）。
//!
//! ⇒ 本模块生产段里**没有任何一个重走周期的字面量**，由
//! [`tests::no_rewalk_period_literal_lives_on_this_side`] 钉住。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 五、一个字节都不许写
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 这一族六条**纯读**（`设计/96 §2.9` 边界①），`tests/backend/readonly_guard.rs`
//! 因此一行都不用改。本模块同样**一个写动词都没有**：它只发命令、读应答、画字。
//! 由 [`tests::this_module_never_touches_the_disk`] 钉住（零命中型扫描）。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # ⚠ 六、这一刀**买不到**什么（逐条，别读宽）
//! ═══════════════════════════════════════════════════════════════════════
//!
//! - **真后端上的读数一趟都没有。** 判据喂的是一台**合成后端**
//!   （[`testing::FakeBackend`]，住 `cfg(test)`），它按
//!   `src/doc/IPC-PROTOCOL.md §10` 那份冻结契约答话，索引由它**真的走一趟磁盘**建出来。
//!   ⇒ 本摞判据买到的是**客户端侧那条链**（发命令 · 解析回参 · 画到帧上）真的通，
//!   **不是**后端那份真索引的正确性 —— 后者的判据住 `tests/backend/files/`。
//! - **「真机上鼠标点得到、眼睛看得见」买不到**：本机 `XDG_SESSION_TYPE=tty`，
//!   没有图形会话（同 `super::shell` 头注）。判据喂的是合成事件，读的是这一帧的 galley。
//! - **时延一个读数都没有。** `设计/60 §3.5.3` 逐字「『打字即出结果』在 64 万条量纲上
//!   仍然没有被证明」—— 本刀**没有改变那一句**，而且**又加了一条**：
//!   **没有去抖**（debounce 要一个定时器，见 §三）⇒ 每敲一个字就是一趟往返。
//!   在 64 万条量纲上那个成本**没量过**。
//! - **索引的根是「窗口现在在看的那个目录」，不是整个 home。** 这是一个**取舍**，
//!   写在 [`one_round`] 那里，不是设计里定的。
//! - **命中行是只读的文字**：点不开、没有「复制」。理由住 [`super::rows::show_hit_rows`]。
//! - **回程那一跳在判据里走的是 `InboundClient::route_reply`**（生产里由
//!   `ssh_source` 的收帧侧调的同一个函数）⇒ **帧的反序列化那一小段不在本摞的射程里**，
//!   它有自己的判据（同 `tests/bridge/backend/control/inbound_client_tests.rs` 那一摞的形状）。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::backend::control::backend_route::{no_channel, route_call_error, Routed};
use crate::backend::control::inbound_client::client_for;
use crate::origin::Origin;

// ═══════════════════════════════════════════════════════════════════
// 线上命令名
// ═══════════════════════════════════════════════════════════════════

/// 在常驻索引里查。**整族的存在理由**（`设计/96 §2.9`）。
pub const CMD_FIND: &str = "files-find";
/// 索引的新鲜度 ／ 条目数 ／ 常驻字节 ／ **后端声明的重走周期**。
pub const CMD_INDEX_STATUS: &str = "files-index-status";
/// 走一遍，就一遍，做完返回（`设计/60 §3.5.2a` 那条裁决里的「机制」那一格）。
pub const CMD_INDEX_REBUILD: &str = "files-index-rebuild";
/// 告诉后端「用户现在在看哪几个目录」（保鲜的另一半）。
pub const CMD_BROWSE: &str = "files-browse";

/// 这一侧用到的**全部**线上名。**唯一住址** —— 判据按它对拍协议文档。
pub const COMMANDS: &[&str] = &[CMD_FIND, CMD_INDEX_STATUS, CMD_INDEX_REBUILD, CMD_BROWSE];

/// 非 UTF-8 路径在线上的那个键名（`src/doc/IPC-PROTOCOL.md §10` 逐字）。
pub const HEX_KEY: &str = "b16";

/// 一趟查询 ／ 一趟状态 ／ 一趟浏览名单的往返上限。
///
/// 同 `backend_kill::CALL_TIMEOUT_SECS` 的理由：后端那条零定时器铁律管的是
/// **后端侧不许等**，客户端侧的等待本来就归客户端（`设计/60 §3.5.2a` 的「节拍」那一层）。
/// ⚠ 它**不是**重走周期（那个数不在这一侧，见模块头注 §四）。
const CALL_TIMEOUT_SECS: u64 = 10;

/// 一趟**重走**的往返上限 —— 它要走一整棵树。
///
/// 为什么比上面那个大一个数量级：`真相源/98 §3.3` 现打 64 万条 **0.99 秒**，
/// 而那是**热缓存**；冷缓存没量过（`drop_caches` 要 root）。
/// ⇒ 这是一条**上界**，不是一个期望值；调小它的后果是「大树永远搜不了」。
const REBUILD_TIMEOUT_SECS: u64 = 120;

fn call_timeout() -> Duration {
    Duration::from_secs(CALL_TIMEOUT_SECS)
}

fn rebuild_timeout() -> Duration {
    Duration::from_secs(REBUILD_TIMEOUT_SECS)
}

// ═══════════════════════════════════════════════════════════════════
// 出方向那几个字段：解析器 ＋ 字段名的唯一住址
// ═══════════════════════════════════════════════════════════════════

/// `files-find` 出方向的字段名。**唯一住址**，判据按它对拍
/// `src/doc/IPC-PROTOCOL.md §10` 那张表（两侧不同源）。
pub const FIND_FIELDS: &[&str] = &[
    "hits",
    "total_hits",
    "truncated",
    "scanned",
    "index_age_secs",
    "index_missing",
];

/// `files-index-status` 出方向的字段名。同上。
pub const STATUS_FIELDS: &[&str] = &[
    "index_missing",
    "entries",
    "resident_bytes",
    "unreadable_dirs",
    "truncated",
    "age_secs",
    "rewalk_interval_secs",
    "stale",
    "browse_watches",
    "browse_watch_cap",
];

/// 一条命中。**持有原始字节，不持有字符串**。
///
/// 🔴 `设计/60 §2 档②` 逐字：文件名在某一跳被有损解码过之后，拿着那串替换字符
/// 回去找那个文件，**找的是一个不存在的名字**。所以线上那一侧一路走字节
/// （`files-find` 的 `hits` 要么是字符串、要么是 `{"b16":…}`），
/// 而这一侧把字节**留着** —— 只在**画到屏幕上**那一刻才有损地转成人话。
///
/// ⚠ 同 [`super::source::Row::lossy_name`] 的先例：有损与不有损分得开
/// （[`Self::lossy`]），界面上要出声。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    pub path: Vec<u8>,
}

impl Hit {
    /// 画到屏幕上的那一份。**有损** —— 见本类型头注。
    pub fn display(&self) -> String {
        String::from_utf8_lossy(&self.path).to_string()
    }

    /// 这条命中的字节不是有效 UTF-8 ⇒ 上面那一份是有损的。
    pub fn lossy(&self) -> bool {
        std::str::from_utf8(&self.path).is_err()
    }
}

/// 一趟 `files-find` 的答案。字段与 [`FIND_FIELDS`] 一一对应。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindOutcome {
    pub hits: Vec<Hit>,
    pub total_hits: usize,
    pub truncated: bool,
    /// 这一趟扫了几条（＝索引条目数）。🔴 **反空真用**：扫到 0 条的「没命中」
    /// 与「索引是空的」在界面上一模一样（`src/doc/IPC-PROTOCOL.md §10` 逐字）。
    pub scanned: usize,
    pub index_age_secs: u64,
    /// 索引还没建过 ⇒ 上面几个数全是 0，而那**不是**「没搜到」。
    pub index_missing: bool,
}

/// 一趟 `files-index-status` 的答案。字段与 [`STATUS_FIELDS`] 一一对应。
///
/// 🔴 **这里的每一个数都是后端报的。** 本结构体不派生、不换算、不补默认值 ——
/// 缺字段就是解析失败（见 [`decode_status`]），不是悄悄当 0。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexStatus {
    pub index_missing: bool,
    pub entries: u64,
    pub resident_bytes: u64,
    pub unreadable_dirs: u64,
    pub truncated: bool,
    pub age_secs: u64,
    /// 🔴 **后端声明的重走周期。** 这一侧**只显示它**，不定它、不校验它
    /// （`设计/99 §2 Q4`：那个数还没拍板）。
    pub rewalk_interval_secs: u64,
    /// `age_secs > rewalk_interval_secs` —— 后端自己算的那句判断。
    pub stale: bool,
    pub browse_watches: u64,
    pub browse_watch_cap: u64,
}

fn field(d: &Value, k: &str) -> Result<Value, String> {
    d.get(k)
        .cloned()
        .ok_or_else(|| format!("应答里少了 `{k}` —— 后端与这一侧的契约漂了"))
}

fn need_u64(d: &Value, k: &str) -> Result<u64, String> {
    field(d, k)?
        .as_u64()
        .ok_or_else(|| format!("`{k}` 不是一个非负整数"))
}

fn need_bool(d: &Value, k: &str) -> Result<bool, String> {
    field(d, k)?
        .as_bool()
        .ok_or_else(|| format!("`{k}` 不是一个布尔"))
}

/// 十六进制（大小写都认）→ 字节。奇数长度 / 非十六进制字符 ⇒ `None`。
///
/// 🔴 **不许在这里「尽力而为」**：猜错一个字节就是指到另一个文件
/// （同 `src/backend/files/raw.rs::from_json` 的头注那一条）。
fn from_hex(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return None;
    }
    let nib = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(b.len() / 2);
    let mut i = 0usize;
    while i < b.len() {
        out.push((nib(b[i])? << 4) | nib(b[i + 1])?);
        i += 2;
    }
    Some(out)
}

/// 线上那两种路径形状 → 原始字节。`None` = 形状不对。
pub fn decode_path(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::String(s) => Some(s.as_bytes().to_vec()),
        Value::Object(m) => match m.get(HEX_KEY)? {
            Value::String(h) => from_hex(h),
            _ => None,
        },
        _ => None,
    }
}

/// `files-find` 的 `data` → [`FindOutcome`]。
pub fn decode_find(d: &Value) -> Result<FindOutcome, String> {
    let raw = field(d, "hits")?;
    let arr = raw
        .as_array()
        .ok_or_else(|| "`hits` 不是一个数组".to_string())?;
    let mut hits = Vec::with_capacity(arr.len());
    for (i, one) in arr.iter().enumerate() {
        let bytes = decode_path(one).ok_or_else(|| {
            format!("第 {i} 条命中的形状不对 —— 只认字符串或 `{{\"{HEX_KEY}\": …}}`")
        })?;
        hits.push(Hit { path: bytes });
    }
    Ok(FindOutcome {
        hits,
        total_hits: need_u64(d, "total_hits")? as usize,
        truncated: need_bool(d, "truncated")?,
        scanned: need_u64(d, "scanned")? as usize,
        index_age_secs: need_u64(d, "index_age_secs")?,
        index_missing: need_bool(d, "index_missing")?,
    })
}

/// `files-index-status` 的 `data` → [`IndexStatus`]。
pub fn decode_status(d: &Value) -> Result<IndexStatus, String> {
    Ok(IndexStatus {
        index_missing: need_bool(d, "index_missing")?,
        entries: need_u64(d, "entries")?,
        resident_bytes: need_u64(d, "resident_bytes")?,
        unreadable_dirs: need_u64(d, "unreadable_dirs")?,
        truncated: need_bool(d, "truncated")?,
        age_secs: need_u64(d, "age_secs")?,
        rewalk_interval_secs: need_u64(d, "rewalk_interval_secs")?,
        stale: need_bool(d, "stale")?,
        browse_watches: need_u64(d, "browse_watches")?,
        browse_watch_cap: need_u64(d, "browse_watch_cap")?,
    })
}

/// `files-index-rebuild` 的 `data` → 它真的走了哪个根（原样回送的那一份）。
pub fn decode_rebuilt_root(d: &Value) -> Result<String, String> {
    let bytes = decode_path(&field(d, "path")?).ok_or_else(|| "`path` 的形状不对".to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

// ═══════════════════════════════════════════════════════════════════
// 入方向那几个 `args`
// ═══════════════════════════════════════════════════════════════════

/// `files-find` 的 `args`。
///
/// ⚠ **`limit` 与 `ignore_ascii_case` 刻意都不发** —— 那两个默认值住后端
/// （`src/doc/IPC-PROTOCOL.md §10`：不给 `limit` ⇒ 1000 · 不给 `ignore_ascii_case` ⇒ `false`）。
/// 在这一侧写一份等于给那两个数造第二个家，而 `设计/01 §5 D3` 的诚实默认
/// 逐字是「沿用调用者已有状态」。回参里的 `truncated` 要**照实画出来**，
/// 用户才知道自己看到的不是全部。
pub fn find_args(needle: &str) -> Value {
    serde_json::json!({ "needle": needle })
}

/// `files-index-rebuild` 的 `args`。
pub fn rebuild_args(root: &str) -> Value {
    serde_json::json!({ "path": root })
}

/// `files-browse` 的 `args` —— **此刻的整份名单**（后端自己算差分）。
pub fn browse_args(dirs: &[String]) -> Value {
    serde_json::json!({ "dirs": dirs })
}

// ═══════════════════════════════════════════════════════════════════
// 新鲜度那一行
// ═══════════════════════════════════════════════════════════════════

/// 还没问过后端时那一行。
pub const FRESHNESS_UNKNOWN: &str = "索引：还没问过这台机器";

/// 🔴 **把后端报的那几个数摆成一行字。纯函数。**
///
/// 逐字要求住 `设计/60 §3.5.3`：那个延迟**必须显示在界面上**，不许让用户猜
/// 为什么刚建的文件搜不到。
///
/// ⚠ **秒数原样画出去，不换算成「几分钟前」**：换算是这一侧编的，而这一格
/// 要的恰好是「后端报的那个数」。人话好看那一档是文案的事，不是本函数的事。
pub fn freshness_line(s: &IndexStatus) -> String {
    if s.index_missing {
        return format!(
            "索引还没建过（后端声明的重走周期 {} 秒）",
            s.rewalk_interval_secs
        );
    }
    let mut t = format!(
        "索引 {} 条 · {} 字节 · {} 秒前走完 · 后端声明的重走周期 {} 秒",
        s.entries, s.resident_bytes, s.age_secs, s.rewalk_interval_secs
    );
    if s.stale {
        t.push_str(" · 后端说该重走了");
    }
    if s.unreadable_dirs > 0 {
        t.push_str(&format!(
            " · 有 {} 个目录读不进去，这份索引有洞",
            s.unreadable_dirs
        ));
    }
    if s.truncated {
        t.push_str(" · 上一趟撞到条目上限没走完，这份索引不完整");
    }
    t.push_str(&format!(
        " · 浏览中的目录挂着 {} 个监听（上限 {}）",
        s.browse_watches, s.browse_watch_cap
    ));
    t
}

/// 命中那一摞上面那一行。**`scanned` 一定画出来** ——
/// 「没命中」与「索引是空的」在屏幕上本来一模一样。
pub fn hits_line(o: &FindOutcome) -> String {
    if o.index_missing {
        return "这台机器上的索引还没建过 —— 那不是「没搜到」".to_string();
    }
    let mut t = format!(
        "命中 {} 条（这一趟扫了 {} 条，索引 {} 秒前建的）",
        o.total_hits, o.scanned, o.index_age_secs
    );
    if o.truncated {
        t.push_str(&format!(" · 只回送了 {} 条，还有更多", o.hits.len()));
    }
    t
}

// ═══════════════════════════════════════════════════════════════════
// 一趟搜索的状态：**两条线程看同一份**
// ═══════════════════════════════════════════════════════════════════

/// 界面这一刻该画什么。**一趟搜索的全部产出。**
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// 这一份是给哪个子串的答案。
    pub needle: String,
    pub outcome: Option<FindOutcome>,
    pub status: Option<IndexStatus>,
    /// 上一趟重走真的走了哪个根（后端原样回送的那一份）。
    pub indexed_root: Option<String>,
    /// 出了事那句话。**画在窗口上**，不是 `tracing`。
    pub notice: Option<String>,
}

/// 搜索这件事的**共享落点** —— 形状照 [`super::shell::Listing`] 与
/// [`super::copy::CopyBoard`] 办（UI 线程读，tokio 那条写）。
///
/// 🔴 `epoch` 那一条与 `Listing` 同理、同必要：用户打字比往返快，
/// 「t」的答案可能在「te」的答案之后才到 —— 号对不上就整份丢掉，
/// 不然搜索框里写着 `te`、屏幕上摆的是 `t` 的命中。
#[derive(Clone, Default)]
pub struct SearchBoard {
    inner: Arc<Mutex<Shown>>,
    epoch: Arc<AtomicU64>,
    inflight: Arc<AtomicU64>,
    /// 手上有没有一趟重走在飞。🔴 **它不是优化** ——
    /// `files-index-rebuild` 逐字「没有并发保护：两个调用方同时发，后完成的那一趟胜出」
    /// （`src/doc/IPC-PROTOCOL.md §10`），而它是一整棵树的遍历。
    /// 连打五个字就发五趟全树遍历，那是客户端自己造的雪崩。
    rebuilding: Arc<AtomicBool>,
    /// 一共发出去过几趟重走。**给判据一个可观测的数**（`§3.5.2a` 那条判据要它）。
    rebuilds: Arc<AtomicU64>,
    /// 落地过几份答案。判据靠它等（不靠睡一个猜出来的时长）。
    rounds: Arc<AtomicU64>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl SearchBoard {
    /// 把窗口交给它，好让它在答案到了的时候敲一下（同 [`super::copy::CopyBoard::attach`]）。
    ///
    /// ⚠ **`None` 不会把已经交过的那个窗口摘掉** —— 与 `CopyBoard::attach` 刻意不同形。
    /// 理由：本板子的发起口 [`super::shell::FileWindow::fire_search`] 可以在**没有 `Ui`
    /// 在手**的地方被调（判据就是这么用的），那时它只能交一个 `None`；
    /// 让 `None` 覆盖掉真窗口 ⇒ 答案回来时**敲不动窗口**，而 egui 只在有事发生时才画
    /// ⇒ 结果要等用户再动一下鼠标才出现，那和「搜不出来」在屏幕上分不开。
    pub fn attach(&self, ctx: Option<egui::Context>) {
        if ctx.is_some() {
            *self.ctx.lock().unwrap() = ctx;
        }
    }

    /// 敲一下窗口：「有新东西了，画下一帧」。
    ///
    /// egui 只在有事发生时才画 ⇒ 不敲的话答案要等用户再动一下鼠标才出现，
    /// 而那和「搜不出来」在屏幕上分不开（同 `真相源/99 §9.6`）。
    pub fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    /// 开一趟。回的是这一趟的号。
    pub fn start(&self) -> u64 {
        self.inflight.fetch_add(1, Ordering::SeqCst);
        self.epoch.load(Ordering::SeqCst)
    }

    /// 换子串：号 +1（在飞的那些从此全部作废）。
    ///
    /// ⚠ **不清 `status`**：新鲜度那一行讲的是**这台机器的索引**，与查哪个子串无关；
    /// 清掉它就会在每次打字时闪一下「还没问过」。清的是**命中**那一半。
    pub fn invalidate(&self, needle: &str) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let mut s = self.inner.lock().unwrap();
        s.needle = needle.to_string();
        s.outcome = None;
        s.notice = None;
    }

    /// 还有几趟在飞。
    pub fn is_running(&self) -> bool {
        self.inflight.load(Ordering::SeqCst) > 0
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    /// 一共发出去过几趟重走 —— `§3.5.2a` 那条判据数的就是它。
    pub fn rebuilds_sent(&self) -> u64 {
        self.rebuilds.load(Ordering::SeqCst)
    }

    pub fn shown(&self) -> Shown {
        self.inner.lock().unwrap().clone()
    }

    /// 摆一句话上去（不经网络的那几档失败走这条）。
    pub fn say(&self, notice: &str) {
        self.inner.lock().unwrap().notice = Some(notice.to_string());
        self.rounds.fetch_add(1, Ordering::SeqCst);
        self.poke();
    }

    /// 抢下「这一趟由我发重走」。`false` = 已经有人在发了，别发第二趟。
    fn claim_rebuild(&self) -> bool {
        self.rebuilding
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    fn release_rebuild(&self) {
        self.rebuilding.store(false, Ordering::SeqCst);
    }
}

/// 把一趟搜索的结果落进 [`SearchBoard`] —— **号对不上就丢掉**。
///
/// 回值 = 真的落盘了。**自由函数**（不吃窗口）⇒ 不开窗、不联网就判得动，
/// 而它是生产那条路上唯一写那份 [`Shown`] 的地方（同 [`super::shell::store_if_current`]）。
pub fn store_if_current(b: &SearchBoard, mine: u64, needle: &str, round: Round) -> bool {
    b.inflight.fetch_sub(1, Ordering::SeqCst);
    if b.epoch.load(Ordering::SeqCst) != mine {
        return false;
    }
    {
        let mut s = b.inner.lock().unwrap();
        s.needle = needle.to_string();
        s.outcome = round.outcome;
        if let Some(st) = round.status {
            s.status = Some(st);
        }
        if let Some(r) = round.indexed_root {
            s.indexed_root = Some(r);
        }
        s.notice = round.notice;
    }
    b.rounds.fetch_add(1, Ordering::SeqCst);
    b.poke();
    true
}

/// 一趟往返下来拿到的东西。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Round {
    pub status: Option<IndexStatus>,
    pub outcome: Option<FindOutcome>,
    pub indexed_root: Option<String>,
    pub notice: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
// 往返
// ═══════════════════════════════════════════════════════════════════

/// 把三态里那两种「没拿到」摊成一句给用户的话。
///
/// ⚠ 搜索**没有第二条路可回落** —— `设计/60 §2 档①` 逐字：SFTP 给不了搜索
/// （协议只能递归 `READDIR`，而且没地方放索引）。
/// ⇒ 这里不做回落决策，但**照样走共用那个分流器**（同
/// `backend::control::cc_bus` 与 `tmux.rs` 那两处的理由）：
/// 分流规则一有第二份实现，「被门拒绝」就会在某一份里被洗成「换条路重做」。
pub(super) fn routed_text(r: Routed) -> String {
    match r {
        Routed::Done => "后端说做完了，但没给答案 —— 那不是这条命令的契约".to_string(),
        Routed::NoChannel(why) | Routed::Refused(why) => why,
    }
}

/// 发一条命令、拿它的 `data`。
///
/// 🔴〔2026-09-22〕**它现在有两个消费者**：搜索那一摞（本模块）与
/// **列目录**（[`super::source::list_via_backend`]）。
/// ⚠ 「怎么跟后端说话」这件事因此横跨两个模块了 —— 如实登记：
/// **再出现第三个消费者就把这三样（本函数 · [`refusal`] · [`routed_text`]）
/// 抽到一个自己的落点去**，别让它一直挂在「搜索」这个名字下面。
///
/// ⚠ 吃的是 [`Origin`]（全仓那一个「哪台机器」的类型），不是一个裸字符串 ——
/// `origin_tests::no_new_raw_string_origin_parameters` 那条递减棘轮逐字要求新代码这么写。
pub(super) async fn call_one(
    origin: &Origin,
    cmd: &str,
    args: Value,
    t: Duration,
) -> Result<Value, Routed> {
    let wire = origin.as_wire_str();
    let Some(client) = client_for(wire) else {
        return Err(no_channel(wire));
    };
    match client.call(cmd, args, t).await {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(Routed::Refused(format!(
            "`{cmd}` 回了一条没有 data 的应答 —— 与 `src/doc/IPC-PROTOCOL.md §10` 那份契约不符"
        ))),
        Err(e) => Err(route_call_error(&e, |code, message| {
            refusal(cmd, code, message)
        })),
    }
}

/// 后端拒绝时那句话。**逐档对着 `src/doc/IPC-PROTOCOL.md §10` 的错误码写。**
pub(super) fn refusal(cmd: &str, code: &str, message: &str) -> String {
    let hint = match code {
        "bad_args" => "这条命令的参数形状不对（多半是两侧契约漂了）",
        "bad_path" => "这个路径后端不认",
        "unreadable" => "这个目录后端打不开（权限，或者它不在那台机器上）",
        _ => "后端拒了这一趟",
    };
    format!("`{cmd}` 被拒（{code}）：{hint}。后端原话：{message}")
}

/// 🔴 **一趟搜索的全部编排。** `§3.5.2a` 那三层在这个函数里各占一段。
///
/// ```text
/// ① files-index-status   ← 新鲜度那几个数（**后端报的**）＋ index_missing / stale
/// ② files-browse         ← 保鲜的另一半：用户现在在看这个目录（**每趟都发**）
/// ③ 后端说「没建过」或「该重走了」（或用户按了那颗按钮）
///      ├── files-index-rebuild   ← **机制**（走一遍，就一遍），节拍在这里
///      └── files-index-status    ← 重走完那几个数变了，再问一趟
/// ④ files-find           ← 查。子串是空的就跳过（空串在后端语义里是「匹配一切」）
/// ```
///
/// # 🔴 根取的是「窗口现在在看的那个目录」—— 这是一个**取舍**，设计里没定
///
/// `files-index-rebuild` 会把常驻那一份**整份换掉**（不是并集）
/// ⇒ 根一定，搜索的射程就定了。三个候选各自的代价：
///
/// | 候选 | 代价 |
/// |---|---|
/// | 用户的 home | 窗口可能在看远端的 `/var/log`，而索引是 home 的 ⇒ **搜不到眼前的东西** |
/// | 远端的 `/` | 一趟遍历没有上界（`设计/60 §3.5.3`：冷缓存没量过），而且大半跟用户无关 |
/// | ⭐ 窗口现在在看的那个目录 | 射程 = 你看得见的那棵树；换目录之后要重走一趟 |
///
/// ⇒ 选第三个，并且把「它到底走了哪个根」画在界面上（[`Shown::indexed_root`]，
/// 值来自后端**原样回送**的 `path`）—— 让射程可见，而不是让用户猜。
/// ⚠ **换目录不会自动重走**：换完之后第一趟搜索会看到后端手上那份旧根的索引。
/// 这是一条真实的边界，如实登记；补它要么每次换目录就重走一整棵树（贵），
/// 要么后端那侧支持多根（不在本刀的写区）。
async fn one_round(
    board: &SearchBoard,
    origin: &Origin,
    root: &str,
    needle: &str,
    force_rebuild: bool,
) -> Round {
    let mut round = Round::default();

    // ① 状态。拿不到就到此为止 —— 后面两步都要它来判。
    match call_one(
        origin,
        CMD_INDEX_STATUS,
        serde_json::json!({}),
        call_timeout(),
    )
    .await
    {
        Ok(v) => match decode_status(&v) {
            Ok(s) => round.status = Some(s),
            Err(e) => {
                round.notice = Some(format!("`{CMD_INDEX_STATUS}` 的应答读不动：{e}"));
                return round;
            }
        },
        Err(r) => {
            round.notice = Some(routed_text(r));
            return round;
        }
    }

    // ② 保鲜的另一半：**每一趟都**告诉后端「用户现在在看这个目录」。
    //
    // 🔴 **刻意不放在下面重走那一支里面。** 重走难得发生一次（后端说没建过 / 说该重走了），
    // 而「眼前是哪个目录」用户每换一次目录就变一次 —— 放进去等于这条命令只在
    // 重走那一刻有效，之后再换目录后端就不知道了，而它买到的那点新鲜度**正好是
    // 「眼前那个目录」的**（`设计/96 §2.9`：`set_browsing` ＝ 登记名单 ＋ 当场重列一遍）。
    // ⚠ 它失败**不致命** —— 少的是「这个目录此刻新不新」，不是整趟搜索
    //   ⇒ 只记一句话，继续往下走。
    // ⚠ 代价如实记：每一趟查询多一次往返 ＋ 后端那侧多 `read_dir` 一个目录。
    let dirs = vec![root.to_string()];
    if let Err(r) = call_one(origin, CMD_BROWSE, browse_args(&dirs), call_timeout()).await {
        round.notice = Some(format!("浏览名单没送到（{}）", routed_text(r)));
    }

    // ③ 要不要重走 —— **判据是后端自己算的那两个布尔**，不是这一侧的一个周期。
    let want = force_rebuild
        || round
            .status
            .as_ref()
            .is_some_and(|s| s.index_missing || s.stale);
    if want && board.claim_rebuild() {
        board.rebuilds.fetch_add(1, Ordering::SeqCst);
        match call_one(
            origin,
            CMD_INDEX_REBUILD,
            rebuild_args(root),
            rebuild_timeout(),
        )
        .await
        {
            Ok(v) => {
                match decode_rebuilt_root(&v) {
                    Ok(p) => round.indexed_root = Some(p),
                    Err(e) => {
                        round.notice = Some(format!("`{CMD_INDEX_REBUILD}` 的应答读不动：{e}"))
                    }
                }
                // 重走完那几个数变了 —— 界面上那一行要是新的。
                if let Ok(v2) = call_one(
                    origin,
                    CMD_INDEX_STATUS,
                    serde_json::json!({}),
                    call_timeout(),
                )
                .await
                {
                    if let Ok(s) = decode_status(&v2) {
                        round.status = Some(s);
                    }
                }
            }
            Err(r) => round.notice = Some(routed_text(r)),
        }
        board.release_rebuild();
    }

    // ④ 查。⚠ 空子串在后端语义里是「匹配一切」⇒ 这里**不发**，
    //    否则按一下「重建索引」就会顺手拉回一千条路径。
    if !needle.is_empty() {
        match call_one(origin, CMD_FIND, find_args(needle), call_timeout()).await {
            Ok(v) => match decode_find(&v) {
                Ok(o) => round.outcome = Some(o),
                Err(e) => round.notice = Some(format!("`{CMD_FIND}` 的应答读不动：{e}")),
            },
            Err(r) => round.notice = Some(routed_text(r)),
        }
    }
    round
}

/// 跑一趟搜索并把结果落进那块板子。**窗口那一侧 `spawn` 的就是它。**
pub async fn run_search(
    board: SearchBoard,
    origin: Origin,
    root: String,
    needle: String,
    mine: u64,
    force_rebuild: bool,
) {
    let round = one_round(&board, &origin, &root, &needle, force_rebuild).await;
    store_if_current(&board, mine, &needle, round);
}

// ═══════════════════════════════════════════════════════════════════
// 画出来
// ═══════════════════════════════════════════════════════════════════

impl SearchBoard {
    /// 新鲜度那一行 ＋ 上一趟走的是哪个根 ＋ 出了事那句话。
    ///
    /// 🔴 这一段是本刀的承重墙：`设计/60 §3.5.3` 的 ⬜「还欠写在界面上那一半」
    /// 就是这几行。删掉它 ⇒ 用户看着一份五分钟前的索引，以为自己刚建的文件不存在。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let s = self.shown();
        match &s.status {
            Some(st) => {
                if st.index_missing || st.stale {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xE0, 0x9A, 0x20),
                        freshness_line(st),
                    );
                } else {
                    ui.label(freshness_line(st));
                }
            }
            None => {
                ui.label(FRESHNESS_UNKNOWN);
            }
        }
        if let Some(root) = &s.indexed_root {
            ui.label(format!("索引走的那个根：{root}"));
        }
        if let Some(n) = &s.notice {
            ui.colored_label(egui::Color32::RED, n);
        }
        if let Some(o) = &s.outcome {
            ui.label(hits_line(o));
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/find_testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/find_tests.rs"]
mod tests;
