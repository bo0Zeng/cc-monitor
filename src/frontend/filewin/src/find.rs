//! **窗口里的文件名搜索** —— `files-read` 这一族在客户端侧的命令面与消费面。
//!
//! # 形状：Everything 式，一敲就出
//!
//! - 搜索框每变一次就发一趟 `files-find`：**原样的搜索词** ＋ 这一趟的号（[`SearchBoard`] 的 epoch）＋
//!   这个搜索框的名字（`stream`，每块板子一个）。语法这一侧一个字都不读 —— 解析、匹配、分页、
//!   丢旧号全在后端。
//! - 范围：默认这台机器的家目录（后端定）；「只搜当前目录」开着 ⇒ 带 `under` = 当前目录。
//! - 只要一屏（[`PAGE`]）；命中那一摞滚到底、后端说还有 ⇒ 同号再要下一屏，接在后面（[`fetch_more`]）。
//! - 一趟里 `files-find` 与 `files-index-status` 并发发（一个往返）；`files-browse` 只在当前目录换了时发。
//!
//! # 节拍：要不要重走，后端说了算
//!
//! 机制在后端（`files-index-rebuild` 走一遍就返回）、周期由后端声明（`files-index-status` 的
//! `rewalk_interval_secs`）、**发不发由调用方**。这一侧只照 `files-find` 回来的三个判断办：
//! `index_missing` / `stale` / `out_of_index` 任一为真 ⇒ 照它给的 `cover_root` 发一趟重走（同一时刻只发一趟），
//! 回来再查一遍。这一侧没有重走周期的字面量（[`tests::no_rewalk_period_literal_lives_on_this_side`]）。
//! 没有去抖、没有定时器：每敲一个字一趟往返，过期的答案靠号丢掉。
//!
//! # 新鲜度与首建
//!
//! [`freshness_line`] 原样画后端报的数（秒数不换算，`stale` / 没建过变橙）。
//! 后端说 `index_missing`（后端起来之后还没建过 ＝ 冷启动首建）且本窗口发了重走 ⇒
//! 那一行换成 [`first_build_line`]（「正在建索引（首次约 N 秒）」），重走回来就摘。
//!
//! # 买不到的
//!
//! - 判据喂的是一台按 `src/doc/IPC-PROTOCOL.md §10` 答话的合成后端（[`testing::FakeBackend`]），
//!   买到的是客户端这条链（发什么 · 解析 · 画到帧上）；后端真索引与语法的正确性判在 `tests/backend/files/`。
//! - 不看这个窗口时索引不会变新（要一个与用户动作无关的节拍，刻意不做）。
//! - 命中没有排序（后端按索引顺序给）。

use copy_core::copy_text;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use super::source::{Line, Origin};

// ═══════════════════════════════════════════════════════════════════
// 线上命令名
// ═══════════════════════════════════════════════════════════════════

/// 在常驻索引里查。**整族的存在理由**。
pub const CMD_FIND: &str = "files-find";
/// 索引的新鲜度 ／ 条目数 ／ 常驻字节 ／ **后端声明的重走周期**。
pub const CMD_INDEX_STATUS: &str = "files-index-status";
/// 走一遍，就一遍，做完返回（那条裁决里的「机制」那一格）。
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
/// **后端侧不许等**，客户端侧的等待本来就归客户端（「节拍」那一层）。
/// ⚠ 它**不是**重走周期（那个数不在这一侧，见模块头注 §四）。
const CALL_TIMEOUT_SECS: u64 = 10;

/// 一趟**重走**的往返上限 —— 它要走一整棵树。
///
/// 为什么比上面那个大一个数量级：现打 64 万条 **0.99 秒**，
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

/// 一屏要几条（`files-find` 的 `limit`）。
pub const PAGE: usize = 100;

/// `files-find` 出方向的字段名。**唯一住址**，判据按它对拍
/// `src/doc/IPC-PROTOCOL.md §10` 那张表（两侧不同源）。
pub const FIND_FIELDS: &[&str] = &[
    "hits",
    "total_hits",
    "truncated",
    "scanned",
    "index_age_secs",
    "index_missing",
    "stale",
    "index_root",
    "out_of_index",
    "cover_root",
    "seq",
    "offset",
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
    "cold_first_build_secs",
    // 后端没走进去的挂载点个数。
    "skipped_mounts",
];

/// 一条命中。**持有原始字节，不持有字符串** —— 有损解码之后拿着替换字符回去找，找的是一个不存在的名字；
/// 只在画到屏幕上那一刻才有损地转成人话（[`Self::display`]），有损与否分得开（[`Self::lossy`]）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    pub path: Vec<u8>,
    /// 后端说它是目录（`kind == "dir"`，不跟链接）。
    pub dir: bool,
}

impl Hit {
    /// 画到屏幕上的那一份。**有损** —— 见本类型头注。
    pub fn display(&self) -> String {
        String::from_utf8_lossy(&self.path).to_string()
    }

    /// 命中那一摞上的一行：目录后面带一个 `/`。
    pub fn row_text(&self) -> String {
        let mut t = self.display();
        if self.dir {
            t.push('/');
        }
        t
    }

    /// 这条命中的字节不是有效 UTF-8 ⇒ 上面那一份是有损的。
    pub fn lossy(&self) -> bool {
        std::str::from_utf8(&self.path).is_err()
    }
}

/// 一趟 `files-find` 的答案（翻页时接上的几屏合在 `hits` 里）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindOutcome {
    pub hits: Vec<Hit>,
    pub total_hits: usize,
    /// 这一屏之后还有。
    pub truncated: bool,
    /// 这一趟扫了几条（＝索引条目数）。🔴 **反空真用**：扫到 0 条的「没命中」
    /// 与「索引是空的」在界面上一模一样。
    pub scanned: usize,
    pub index_age_secs: u64,
    /// 索引还没建过 ⇒ 上面几个数全是 0，而那**不是**「没搜到」。
    pub index_missing: bool,
    /// 后端说该重走了。
    pub stale: bool,
    /// 后端手上那份索引的根。
    pub index_root: Option<Vec<u8>>,
    /// 这一趟的范围不在那份索引里。
    pub out_of_index: bool,
    /// 要搜全这一趟、重走该走哪个根（后端判的）。
    pub cover_root: Option<Vec<u8>>,
    /// 这一屏是从第几条起的。
    pub offset: usize,
}

impl FindOutcome {
    /// 后端说要重走（没建过 · 该重走了 · 范围不在索引里）。
    pub fn wants_rebuild(&self) -> bool {
        self.index_missing || self.stale || self.out_of_index
    }
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
    /// （那个数还没定）。
    pub rewalk_interval_secs: u64,
    /// `age_secs > rewalk_interval_secs` —— 后端自己算的那句判断。
    pub stale: bool,
    pub browse_watches: u64,
    pub browse_watch_cap: u64,
    /// 后端**声明**的冷启动首建大约要几秒。
    /// 这一侧只在「首建那一趟」里把它画出来（[`first_build_line`]），不定它、不换算它。
    pub cold_first_build_secs: u64,
    /// 根底下挂着的别的文件系统，后端没走进去的个数（那几个目录底下的搜不到）。
    pub skipped_mounts: u64,
}

fn field(d: &Value, k: &str) -> Result<Value, String> {
    d.get(k)
        .cloned()
        .ok_or_else(|| copy_text("rsFilewinFind.reply.missingField", &[("k", &k.to_string())]))
}

fn need_u64(d: &Value, k: &str) -> Result<u64, String> {
    field(d, k)?
        .as_u64()
        .ok_or_else(|| copy_text("rsFilewinFind.reply.notU64", &[("k", &k.to_string())]))
}

fn need_bool(d: &Value, k: &str) -> Result<bool, String> {
    field(d, k)?
        .as_bool()
        .ok_or_else(|| copy_text("rsFilewinFind.reply.notBool", &[("k", &k.to_string())]))
}

/// 十六进制（大小写都认）→ 字节。奇数长度 / 非十六进制字符 ⇒ `None`。
///
/// 🔴 **不许在这里「尽力而为」**：猜错一个字节就是指到另一个文件
/// （同 `src/backend/common/path_wire.rs::from_json` 的头注那一条）。
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

/// 可以是 `null` 的路径字段。
fn opt_path(d: &Value, k: &str) -> Result<Option<Vec<u8>>, String> {
    match field(d, k)? {
        Value::Null => Ok(None),
        v => decode_path(&v)
            .map(Some)
            .ok_or_else(|| copy_text("rsFilewinFind.reply.badPath", &[("k", &k.to_string())])),
    }
}

/// `files-find` 的 `data` → [`FindOutcome`]。
pub fn decode_find(d: &Value) -> Result<FindOutcome, String> {
    let raw = field(d, "hits")?;
    let arr = raw
        .as_array()
        .ok_or_else(|| copy_text("rsFilewinFind.hits.notArray", &[]))?;
    let mut hits = Vec::with_capacity(arr.len());
    for (i, one) in arr.iter().enumerate() {
        let bad = || copy_text("rsFilewinFind.decodeFind.badHit", &[("i", &i.to_string())]);
        let path = one.get("path").and_then(decode_path).ok_or_else(bad)?;
        let kind = one.get("kind").and_then(Value::as_str).ok_or_else(bad)?;
        hits.push(Hit {
            path,
            dir: kind == "dir",
        });
    }
    Ok(FindOutcome {
        hits,
        total_hits: need_u64(d, "total_hits")? as usize,
        truncated: need_bool(d, "truncated")?,
        scanned: need_u64(d, "scanned")? as usize,
        index_age_secs: need_u64(d, "index_age_secs")?,
        index_missing: need_bool(d, "index_missing")?,
        stale: need_bool(d, "stale")?,
        index_root: opt_path(d, "index_root")?,
        out_of_index: need_bool(d, "out_of_index")?,
        cover_root: opt_path(d, "cover_root")?,
        offset: need_u64(d, "offset")? as usize,
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
        cold_first_build_secs: need_u64(d, "cold_first_build_secs")?,
        skipped_mounts: need_u64(d, "skipped_mounts")?,
    })
}

// ═══════════════════════════════════════════════════════════════════
// 入方向那几个 `args`
// ═══════════════════════════════════════════════════════════════════

/// 一趟 `files-find` 问的是什么（翻页时原样再问一遍，只换 `offset`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asked {
    /// 原样的搜索词。
    pub query: String,
    /// 「只搜当前目录」开着时的那个目录；`None` ⇒ 后端的默认范围（家目录）。
    pub under: Option<super::source::RemotePath>,
}

/// `files-find` 的 `args`：原样的搜索词 ＋ 号 ＋ 搜索框名 ＋ 范围 ＋ 这一屏从哪起。
pub fn find_args(asked: &Asked, seq: u64, stream: &str, offset: usize) -> Value {
    let mut v = serde_json::json!({
        "query": asked.query,
        "seq": seq,
        "stream": stream,
        "offset": offset,
        "limit": PAGE,
    });
    if let Some(u) = &asked.under {
        v["under"] = u.wire();
    }
    v
}

/// `files-index-rebuild` 的 `args`：后端给的那个根；没给 ⇒ 不带（后端走家目录）。
pub fn rebuild_args_at(root: Option<&[u8]>) -> Value {
    match root {
        Some(b) => serde_json::json!({ "path": super::source::wire_bytes(b) }),
        None => serde_json::json!({}),
    }
}

/// `files-browse` 的 `args` —— **此刻的整份名单**（后端自己算差分）。
pub fn browse_args_at(dirs: &[super::source::RemotePath]) -> Value {
    let v: Vec<Value> = dirs.iter().map(super::source::RemotePath::wire).collect();
    serde_json::json!({ "dirs": v })
}

// ═══════════════════════════════════════════════════════════════════
// 新鲜度那一行
// ═══════════════════════════════════════════════════════════════════

/// 还没问过后端时那一行。
pub static FRESHNESS_UNKNOWN: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| copy_text("rsFilewinFind.freshness.unknown", &[]));

/// 🔴 **把后端报的那几个数摆成一行字。纯函数。**
///
/// 逐字要求住：那个延迟**必须显示在界面上**，不许让用户猜
/// 为什么刚建的文件搜不到。
///
/// ⚠ **秒数原样画出去，不换算成「几分钟前」**：换算是这一侧编的，而这一格
/// 要的恰好是「后端报的那个数」。人话好看那一档是文案的事，不是本函数的事。
pub fn freshness_line(s: &IndexStatus) -> String {
    if s.index_missing {
        return copy_text("rsFilewinFind.freshness.notBuilt", &[]);
    }
    let mut t = copy_text(
        "rsFilewinFind.freshness.line",
        &[
            ("entries", &s.entries.to_string()),
            ("residentBytes", &s.resident_bytes.to_string()),
            ("ageSecs", &s.age_secs.to_string()),
            ("interval", &s.rewalk_interval_secs.to_string()),
        ],
    );
    if s.stale {
        t.push_str(&copy_text("rsFilewinFind.freshness.stale", &[]));
    }
    if s.unreadable_dirs > 0 {
        t.push_str(&copy_text(
            "rsFilewinFind.freshness.holes",
            &[("unreadableDirs", &s.unreadable_dirs.to_string())],
        ));
    }
    if s.truncated {
        t.push_str(&copy_text("rsFilewinFind.freshness.truncated", &[]));
    }
    // 没走进去的挂载点：那几个目录底下的东西搜不到 ⇒ 说出来（不静默少走）。
    if s.skipped_mounts > 0 {
        t.push_str(&copy_text(
            "rsFilewinFind.freshness.skippedMounts",
            &[("n", &s.skipped_mounts.to_string())],
        ));
    }
    // 〔CP1 裁「改·§2.1」〕原先这里还接一段「浏览中的目录挂着 N 个监听（上限 M）」—— 监听数 / 上限是
    //   内部资源读数，用户用不上 ⇒ 删去（裁词原话「删去这段」）。
    t
}

/// **冷启动首建那一趟正在走**时画的那一行。
///
/// 数是后端报的（[`IndexStatus::cold_first_build_secs`]），这一侧只摆字。
/// 「首次」二字是承重的：只有后端说「还没建过」（`index_missing`）的那一趟才画它 ——
/// 周期性重走是热的，那个数不适用（[`one_round`] 那一段）。
pub fn first_build_line(cold_first_build_secs: u64) -> String {
    copy_text(
        "rsFilewinFind.firstBuild.line",
        &[("coldFirstBuildSecs", &cold_first_build_secs.to_string())],
    )
}

/// 命中那一摞上面那一行。**`scanned` 一定画出来** ——
/// 「没命中」与「索引是空的」在屏幕上本来一模一样。
pub fn hits_line(o: &FindOutcome) -> String {
    if o.index_missing {
        return copy_text("rsFilewinFind.hits.noIndex", &[]);
    }
    let mut t = copy_text(
        "rsFilewinFind.hits.line",
        &[
            ("totalHits", &o.total_hits.to_string()),
            ("scanned", &o.scanned.to_string()),
            ("indexAgeSecs", &o.index_age_secs.to_string()),
        ],
    );
    if o.truncated {
        t.push_str(&copy_text(
            "rsFilewinFind.hits.more",
            &[("hitsCount", &(o.hits.len()).to_string())],
        ));
    }
    t
}

// ═══════════════════════════════════════════════════════════════════
// 一趟搜索的状态：**两条线程看同一份**
// ═══════════════════════════════════════════════════════════════════

/// 界面这一刻该画什么。**一趟搜索的全部产出。**
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// 这一份是给哪一问的答案。
    pub asked: Asked,
    pub outcome: Option<FindOutcome>,
    pub status: Option<IndexStatus>,
    /// 后端手上那份索引的根（`files-find` 回的 `index_root`）。
    pub indexed_root: Option<String>,
    /// 出了事那句话。**画在窗口上**，不是 `tracing`。
    pub notice: Option<String>,
}

/// 每块板子一个搜索框名（同一进程里不重）。
static NEXT_STREAM: AtomicU64 = AtomicU64::new(1);

/// 搜索这件事的**共享落点** —— 形状照 [`super::shell::Listing`] 与
/// [`super::copy::CopyBoard`] 办（UI 线程读，tokio 那条写）。
///
/// 🔴 `epoch` 既是这一侧丢旧答案的号，也是发给后端的 `seq`：
/// 用户打字比往返快，「t」的答案可能在「te」的答案之后才到 —— 号对不上就整份丢掉；
/// 后端那一侧同一个号让在飞的旧那一趟收手。
#[derive(Clone)]
pub struct SearchBoard {
    inner: Arc<Mutex<Shown>>,
    epoch: Arc<AtomicU64>,
    inflight: Arc<AtomicU64>,
    /// 这个搜索框在后端那一侧的名字（`stream`）。
    stream: Arc<str>,
    /// 手上有没有一趟重走在飞。🔴 **它不是优化** —— 重走是一整棵树的遍历，
    /// 连打五个字就发五趟，那是客户端自己造的雪崩（后端那一侧另有互斥，抢不到回 `already_rebuilding`）。
    rebuilding: Arc<AtomicBool>,
    /// 一共发出去过几趟重走。**给判据一个可观测的数**。
    rebuilds: Arc<AtomicU64>,
    /// 落地过几份答案。判据靠它等（不靠睡一个猜出来的时长）。
    rounds: Arc<AtomicU64>,
    /// 往下翻那一趟在飞。
    paging: Arc<AtomicBool>,
    /// 上一次告诉后端「用户在看哪个目录」时的那个目录（换了才再发）。
    browsed: Arc<Mutex<Option<super::source::RemotePath>>>,
    /// **冷启动首建正在走**：`Some(后端声明的秒数)`。
    ///
    /// 🔴 它**不跟 `epoch` 走**，跟那一趟重走走：用户在首建期间接着打字，号就换了、
    /// 发起重走的那一趟的答案会被整份丢掉 —— 而首建照样在走，那一行不许跟着没了。
    first_build: Arc<Mutex<Option<u64>>>,
    ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl Default for SearchBoard {
    fn default() -> Self {
        Self {
            inner: Default::default(),
            epoch: Default::default(),
            inflight: Default::default(),
            stream: format!(
                "{}-{}",
                std::process::id(),
                NEXT_STREAM.fetch_add(1, Ordering::Relaxed)
            )
            .into(),
            rebuilding: Default::default(),
            rebuilds: Default::default(),
            rounds: Default::default(),
            paging: Default::default(),
            browsed: Default::default(),
            first_build: Default::default(),
            ctx: Default::default(),
        }
    }
}

impl SearchBoard {
    /// 把窗口交给它，好让它在答案到了的时候敲一下（同 [`super::copy::CopyBoard::attach`]）。
    ///
    /// ⚠ **`None` 不会把已经交过的那个窗口摘掉**：发起口可以在没有 `Ui` 在手的地方被调（判据就是这么用的），
    /// 让 `None` 覆盖掉真窗口 ⇒ 答案回来时敲不动窗口，结果要等用户再动一下鼠标才出现。
    pub fn attach(&self, ctx: Option<egui::Context>) {
        if ctx.is_some() {
            *self.ctx.lock().unwrap() = ctx;
        }
    }

    /// 敲一下窗口：「有新东西了，画下一帧」。
    pub fn poke(&self) {
        if let Some(c) = self.ctx.lock().unwrap().as_ref() {
            c.request_repaint();
        }
    }

    /// 这个搜索框在后端那一侧的名字。
    pub fn stream(&self) -> &str {
        &self.stream
    }

    /// 开一趟。回的是这一趟的号。
    pub fn start(&self) -> u64 {
        self.inflight.fetch_add(1, Ordering::SeqCst);
        self.epoch.load(Ordering::SeqCst)
    }

    /// 换一问：号 +1（在飞的那些从此全部作废）。
    ///
    /// ⚠ **不清 `status`**：新鲜度那一行讲的是这台机器的索引，与问什么无关。清的是**命中**那一半。
    pub fn invalidate(&self, asked: &Asked) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let mut s = self.inner.lock().unwrap();
        s.asked = asked.clone();
        s.outcome = None;
        s.notice = None;
    }

    /// 眼下这一问的号（翻页用同一个号）。
    pub fn current(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// 还有几趟在飞。
    pub fn is_running(&self) -> bool {
        self.inflight.load(Ordering::SeqCst) > 0
    }

    pub fn rounds(&self) -> u64 {
        self.rounds.load(Ordering::SeqCst)
    }

    /// 一共发出去过几趟重走。
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

    /// 冷启动首建正在走 ⇒ 后端声明的那个秒数；否则 `None`。
    pub fn first_build(&self) -> Option<u64> {
        *self.first_build.lock().unwrap()
    }

    fn mark_first_build(&self, secs: Option<u64>) {
        *self.first_build.lock().unwrap() = secs;
        self.poke();
    }

    fn release_rebuild(&self) {
        self.rebuilding.store(false, Ordering::SeqCst);
    }

    /// 要不要再告诉后端「用户在看这个目录」：换了才要（并记下这一次）。
    fn note_browsing(&self, dir: &super::source::RemotePath) -> bool {
        let mut g = self.browsed.lock().unwrap();
        if g.as_ref() == Some(dir) {
            return false;
        }
        *g = Some(dir.clone());
        true
    }

    /// 该不该往下再要一屏：手上这一问有答案、后端说后面还有、没有一趟翻页在飞。
    /// 抢到 ⇒ 回 `(号, 这一问, 从哪起)`，并把「在飞」那一位按住。
    pub fn claim_more(&self) -> Option<(u64, Asked, usize)> {
        let s = self.inner.lock().unwrap();
        let o = s.outcome.as_ref()?;
        if !o.truncated {
            return None;
        }
        if self
            .paging
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }
        Some((self.current(), s.asked.clone(), o.hits.len()))
    }
}

/// 把一趟搜索的结果落进 [`SearchBoard`] —— **号对不上就丢掉**。
///
/// 回值 = 真的落盘了。**自由函数**（不吃窗口）⇒ 不开窗、不联网就判得动。
pub fn store_if_current(b: &SearchBoard, mine: u64, asked: &Asked, round: Round) -> bool {
    b.inflight.fetch_sub(1, Ordering::SeqCst);
    if b.epoch.load(Ordering::SeqCst) != mine {
        return false;
    }
    {
        let mut s = b.inner.lock().unwrap();
        s.asked = asked.clone();
        if let Some(root) = round.outcome.as_ref().and_then(|o| o.index_root.as_ref()) {
            s.indexed_root = Some(String::from_utf8_lossy(root).to_string());
        }
        s.outcome = round.outcome;
        if let Some(st) = round.status {
            s.status = Some(st);
        }
        s.notice = round.notice;
    }
    b.rounds.fetch_add(1, Ordering::SeqCst);
    b.poke();
    true
}

/// 往下翻回来的那一屏接到后面 —— **号对不上、或手上的条数已经不是发问时那么多 ⇒ 丢掉**。
pub fn append_if_current(b: &SearchBoard, mine: u64, page: Result<FindOutcome, String>) -> bool {
    b.paging.store(false, Ordering::SeqCst);
    if b.epoch.load(Ordering::SeqCst) != mine {
        return false;
    }
    let mut s = b.inner.lock().unwrap();
    let landed = match page {
        Ok(p) => match s.outcome.as_mut() {
            Some(o) if o.hits.len() == p.offset => {
                o.hits.extend(p.hits);
                o.total_hits = p.total_hits;
                o.truncated = p.truncated;
                o.scanned = p.scanned;
                o.index_age_secs = p.index_age_secs;
                true
            }
            _ => false,
        },
        Err(e) => {
            s.notice = Some(e);
            true
        }
    };
    drop(s);
    if landed {
        b.rounds.fetch_add(1, Ordering::SeqCst);
        b.poke();
    }
    landed
}

/// 一趟往返下来拿到的东西。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Round {
    pub status: Option<IndexStatus>,
    pub outcome: Option<FindOutcome>,
    pub notice: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════
// 往返
// ═══════════════════════════════════════════════════════════════════

/// 发一条命令、拿它的 `data`。
///
/// 🔴**它只是 [`super::source::ask`] 的一层转交** —— 窗口进程里说 `call`
/// 的唯一一处住那边（列目录与搜索两个消费者、写面四条都经它）。从前这里直接问进程级
/// 登记表（`inbound_client`）并走共用分流器翻成三态；窗口成了独立进程之后那张表在这个
/// 进程里是空的，而通道那一侧已经按分好了层（宿主调的就是那个分流器的
/// 分层出口）⇒ 这里只剩「翻成一句人话」，翻译住 [`super::source::said`]。
///
/// ⚠ 搜索**没有第二条路可回落** ——：SFTP 给不了搜索。
pub(super) async fn call_one(
    line: &Line,
    origin: &Origin,
    cmd: &str,
    args: Value,
    t: Duration,
) -> Result<Value, String> {
    super::source::ask(line, origin, cmd, &args, t).await
}

/// 后端拒绝时那句话。**逐档对着 `src/doc/IPC-PROTOCOL.md §10` 的错误码写。**
/// 对外那句只说哪一步没成 ＋ 那台的原话；命令名与错误码不上屏，只进日志（排查用）。
pub(super) fn refusal(cmd: &str, code: &str, message: &str) -> String {
    tracing::warn!("filewin: {cmd} refused ({code}): {message}");
    let hint = match code {
        "bad_args" => &copy_text("rsFilewinFind.refusal.badArgs", &[]),
        "bad_path" => &copy_text("rsFilewinFind.refusal.badPath", &[]),
        "bad_query" => &copy_text("rsFilewinFind.refusal.badQuery", &[]),
        "unreadable" => &copy_text("rsFilewinFind.refusal.cannotOpen", &[]),
        _ => &copy_text("rsFilewinFind.refusal.other", &[]),
    };
    if message.trim().is_empty() {
        return hint.to_string();
    }
    copy_text(
        "rsFilewinFind.refusal.line",
        &[
            ("hint", &hint.to_string()),
            ("message", &message.to_string()),
        ],
    )
}

/// 问一趟 `files-find`。`Ok(None)` = 后端说这一趟已被同一个搜索框更新的一趟顶掉（不出声）。
async fn ask_find(
    line: &Line,
    origin: &Origin,
    args: &Value,
) -> Result<Option<FindOutcome>, String> {
    match super::source::ask_coded(line, origin, CMD_FIND, args, call_timeout()).await {
        Ok(v) => decode_find(&v)
            .map(Some)
            .map_err(|e| copy_text("rsFilewinFind.round.findFailed", &[("e", &e.to_string())])),
        Err(f) if f.code.as_deref() == Some("superseded") => Ok(None),
        Err(f) => Err(f.said),
    }
}

async fn ask_status(line: &Line, origin: &Origin) -> Result<IndexStatus, String> {
    let v = call_one(
        line,
        origin,
        CMD_INDEX_STATUS,
        serde_json::json!({}),
        call_timeout(),
    )
    .await?;
    decode_status(&v)
        .map_err(|e| copy_text("rsFilewinFind.round.statusFailed", &[("e", &e.to_string())]))
}

/// 🔴 **一趟搜索的全部编排。**
///
/// ```text
/// ① files-browse          ← 当前目录换了才发（保鲜的另一半：后端给眼前这个目录挂监听）
/// ② files-find ∥ files-index-status   ← 并发，一个往返
/// ③ 后端说要重走（没建过 / 该重走了 / 范围不在索引里），或用户按了那颗按钮
///      ├── files-index-rebuild（根 ＝ 后端给的 cover_root）  ← 同一时刻只发一趟
///      └── files-find ∥ files-index-status   ← 重走完再问一遍
/// ```
async fn one_round(
    board: &SearchBoard,
    line: &Line,
    origin: &Origin,
    cwd: &super::source::RemotePath,
    asked: &Asked,
    mine: u64,
    force_rebuild: bool,
) -> Round {
    let mut round = Round::default();

    if board.note_browsing(cwd) {
        if let Err(r) = call_one(
            line,
            origin,
            CMD_BROWSE,
            browse_args_at(std::slice::from_ref(cwd)),
            call_timeout(),
        )
        .await
        {
            // 少的是「这个目录此刻新不新」，不是整趟搜索 ⇒ 只进日志。
            tracing::warn!("filewin: browse list not delivered: {r}");
        }
    }

    let args = find_args(asked, mine, board.stream(), 0);
    let (found, status) = tokio::join!(ask_find(line, origin, &args), ask_status(line, origin));
    match status {
        Ok(s) => round.status = Some(s),
        Err(e) => round.notice = Some(e),
    }
    let outcome = match found {
        Ok(Some(o)) => o,
        // 被更新的一趟顶掉了：这一份不落（号也已经对不上）。
        Ok(None) => return round,
        Err(e) => {
            round.notice = Some(e);
            return round;
        }
    };

    let want = force_rebuild || outcome.wants_rebuild();
    if want && board.claim_rebuild() {
        board.rebuilds.fetch_add(1, Ordering::SeqCst);
        // 后端说「还没建过」⇒ 这一趟就是冷启动首建，用户看得见 ⇒ 先挂上后端声明的那个秒数，回来再摘。
        //   该重走了 / 换根 / 按按钮那几种是热的，那个数不适用 ⇒ 不挂。
        let first = round
            .status
            .as_ref()
            .filter(|_| outcome.index_missing)
            .map(|s| s.cold_first_build_secs);
        if first.is_some() {
            board.mark_first_build(first);
        }
        let rebuilt = call_one(
            line,
            origin,
            CMD_INDEX_REBUILD,
            rebuild_args_at(outcome.cover_root.as_deref()),
            rebuild_timeout(),
        )
        .await;
        if first.is_some() {
            board.mark_first_build(None);
        }
        board.release_rebuild();
        match rebuilt {
            Ok(_) => {
                let (found, status) =
                    tokio::join!(ask_find(line, origin, &args), ask_status(line, origin));
                if let Ok(s) = status {
                    round.status = Some(s);
                }
                match found {
                    Ok(Some(o)) => round.outcome = Some(o),
                    Ok(None) => return round,
                    Err(e) => {
                        round.notice = Some(e);
                        round.outcome = Some(outcome);
                    }
                }
                return round;
            }
            Err(r) => round.notice = Some(r),
        }
    }
    round.outcome = Some(outcome);
    round
}

/// 跑一趟搜索并把结果落进那块板子。**窗口那一侧 `spawn` 的就是它。**
pub async fn run_search_at(
    board: SearchBoard,
    line: Line,
    origin: Origin,
    cwd: super::source::RemotePath,
    asked: Asked,
    mine: u64,
    force_rebuild: bool,
) {
    let round = one_round(&board, &line, &origin, &cwd, &asked, mine, force_rebuild).await;
    store_if_current(&board, mine, &asked, round);
}

/// 往下再要一屏（同号、同一问，`offset` ＝ 手上已有的条数），回来接在后面。
pub async fn fetch_more(
    board: SearchBoard,
    line: Line,
    origin: Origin,
    mine: u64,
    asked: Asked,
    offset: usize,
) {
    let args = find_args(&asked, mine, board.stream(), offset);
    let page = match ask_find(&line, &origin, &args).await {
        Ok(Some(o)) => Ok(o),
        // 被顶掉了 ⇒ 号也已经换了，下面那一步会丢掉它。
        Ok(None) => Err(String::new()),
        Err(e) => Err(e),
    };
    append_if_current(&board, mine, page);
}

// ═══════════════════════════════════════════════════════════════════
// 画出来
// ═══════════════════════════════════════════════════════════════════

impl SearchBoard {
    /// 新鲜度那一行 ＋ 搜的是哪个根 ＋ 出了事那句话 ＋ 命中那一行。
    pub fn ui(&self, ui: &mut egui::Ui) {
        let s = self.shown();
        // 冷启动首建正在走 ⇒ 这一行顶替新鲜度那一行（那一行此刻只会说「还没建过」）。
        if let Some(secs) = self.first_build() {
            ui.colored_label(ui.visuals().warn_fg_color, first_build_line(secs));
        } else {
            match &s.status {
                Some(st) => {
                    if st.index_missing || st.stale {
                        ui.colored_label(ui.visuals().warn_fg_color, freshness_line(st));
                    } else {
                        ui.label(freshness_line(st));
                    }
                }
                None => {
                    ui.label(FRESHNESS_UNKNOWN.as_str());
                }
            }
        }
        if let Some(root) = &s.indexed_root {
            ui.label(copy_text(
                "rsFilewinFind.ui.root",
                &[("root", &root.to_string())],
            ));
        }
        if let Some(n) = s.notice.as_ref().filter(|n| !n.is_empty()) {
            ui.colored_label(ui.visuals().error_fg_color, n);
        }
        if let Some(o) = &s.outcome {
            ui.label(hits_line(o));
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/find_testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "../../../../tests/frontend/filewin/find_tests.rs"]
mod tests;
