//! issue #28：远端全文搜索（一次性查询子命令 `--search`）。
//!
//! cc-monitor 通过**独立 SSH 连接**一次性 exec `<backend> --search <query> [opts]`，
//! backend 在远端 CPU 上扫 `<claude_dir>/projects/**/*.jsonl`、做服务端搜索（避免拉
//! 整库回本地），输出**每命中会话一行** camelCase JSON（与 monitor `search::SessionHits`
//! 形状严格一致，可直接反序列化）：
//! `{agent,sessionId,projectPath,projectName,jsonlPath,title,updatedAt,hitCount,hits:[{uuid,tsMs,kind,before,matched,after}]}`
//!
//! 🔴 语义与本地 `../../bridge/src/search.rs` **不是「对齐」，是同一份**：
//! 抽取 / 匹配 / snippet 的 12 个助手、4 个口径常量、snippet 预算与预算顺序只有一个家 ——通用那一半住
//! [`super::search_rules`]，Claude 记录文本那一半（正文 / 工具内容怎么抽 · 注入怎么剥）住适配层 `agents/claudecode/text.rs`（经注册表够）。
//! 收口前本文件各写了一遍那 12 个（`K-R85` 实测逐字相同），而 monitor 的
//! `cross_half_edge_registry::CROSS_EDGES` 17 条跨轨边里 **search 零命中** ⇒
//! **没有任何判据在拦着它们漂开**。判据现在有了，住
//! `tests/backend/observe/search_rules_tests.rs::the_search_kou_jing_has_exactly_one_home`（随家从 monitor 那份守卫搬来）。
//! backend 无 `parse_line`，故仍直接在 `serde_json::Value` 上抽取 —— 那是**取数**的差别，
//! 不是**口径**的差别。
//!
//! 安全：路径严格限 `<claude_dir>/projects/`（〔审计 F 🔴-6〕经 observe 唯一那道围栏 `observe/fence.rs::Fence`；
//! 先前这里内联复刻了一份 history_query 的，点名的第二个家）；
//! 只读铁律（cc-monitor 不写远端）成立——本模块只 read_dir / read。

// U2/U3：这两个原来在本文件里各有一份逐字相同的副本。去向**不同**：
// `projects_root` 跨 observe/control 两层 ⇒ `common/`；`mtime_ms` 两个调用点同属 observe
// ⇒ U3 按 `common/` 自己的「≥2 层」门槛搬回 `observe/`。
use crate::agents::claudecode::paths::projects_root;
use crate::observe::fence::Fence;
use crate::observe::fs::mtime_ms;
use crate::observe::search_rules::{self, SnippetBudget, SnippetVerdict, MAIN_CAP, TOOL_CAP};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// 🔴 口径常量**一个都不在这里**（`K-R100`）——它们就是口径本身，本文件再写一个同样的
// 字面量 = 又开了第二份。`MAIN_CAP` / `TOOL_CAP` / `SNIPPET_CTX` / `PER_SESSION_CAP` /
// `DEFAULT_LIMIT` 住 `search_rules`（原共享 crate `search-core`）。

/// 解析后的查询选项。
struct SearchOpts {
    include_tools: bool,
    /// None=全部；Some("user")/Some("assistant")=只搜该类型。
    scope: Option<String>,
    after_ms: i64,
    /// 全局返回 snippet 上限（hitCount 仍报全量）。
    limit: usize,
    /// 只比会话标题与第一句（不搜内容）：命中的会话照样一行，`hitCount` 为 0、`hits` 空。
    titles: bool,
}

/// `--search <query> [--include-tools] [--scope user|assistant] [--after-ms N] [--limit N]`。
/// 返回进程退出码（0 ok / 2 err），与 history_query::run 同约定。
pub fn run(agent_home: &Path, args: &[String]) -> i32 {
    // args[0] == "--search"
    let query = match args.get(1) {
        Some(q) => q.as_str(),
        None => {
            eprintln!("cc-monitor-backend query error: --search requires <query> argument");
            return 2;
        }
    };
    let opts = parse_opts(&args[2.min(args.len())..]);
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match search(agent_home, query, &opts, &mut out) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cc-monitor-backend search error: {e}");
            2
        }
    }
}

/// 帧面那条（`history-search`）的入口：`rest` 是 `--search <query>` **之后**那一截，
/// 解析走**同一个** [`parse_opts`] —— 选项的口径只有一份，不在帧面另写一套 JSON 解析。
/// 回「读不动、没搜到」的会话数（帧面那条把它交给界面说出来）。
/// `titles` ＝ 只比标题与第一句（帧面那一格；CLI 面没有这个选项）。
pub(crate) fn search_into(
    agent_home: &Path,
    query: &str,
    rest: &[String],
    titles: bool,
    out: &mut impl Write,
) -> Result<usize, String> {
    let opts = SearchOpts {
        titles,
        ..parse_opts(rest)
    };
    let unreadable = search_counting(agent_home, query, &opts, out)?;
    if let Some(note) = unreadable_note(unreadable) {
        tracing::warn!("{note}");
    }
    Ok(unreadable)
}

/// 从 `--search <query>` 之后的参数解析选项（未知/缺值的容错忽略）。
fn parse_opts(rest: &[String]) -> SearchOpts {
    let mut opts = SearchOpts {
        include_tools: false,
        scope: None,
        after_ms: 0,
        limit: search_rules::DEFAULT_LIMIT,
        titles: false,
    };
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--include-tools" => opts.include_tools = true,
            "--scope" => {
                if let Some(v) = rest.get(i + 1) {
                    if v == "user" || v == "assistant" || v == "report" {
                        opts.scope = Some(v.clone());
                    }
                    i += 1;
                }
            }
            "--after-ms" => {
                if let Some(v) = rest.get(i + 1) {
                    opts.after_ms = v.parse::<i64>().unwrap_or(0).max(0);
                    i += 1;
                }
            }
            "--limit" => {
                if let Some(v) = rest.get(i + 1) {
                    opts.limit = search_rules::clamp_limit(
                        v.parse::<usize>().unwrap_or(search_rules::DEFAULT_LIMIT),
                    );
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    opts
}

/// 扫 projects/**/*.jsonl，搜索匹配，每命中会话输出一行 JSON。
///
/// ⚠ `out` 是参数而不是直接 `stdout()`：**输出的行序就是预算顺序**，
/// 而「预算按最近优先花」正是 `KR100D2` 要判的性质 —— 判据得看得见那个序
/// （`the_snippet_budget_goes_to_the_most_recent_sessions`）。直接写 stdout 就判不了。
fn search(
    agent_home: &Path,
    query: &str,
    opts: &SearchOpts,
    out: &mut impl Write,
) -> Result<(), String> {
    let unreadable = search_counting(agent_home, query, opts, out)?;
    if let Some(note) = unreadable_note(unreadable) {
        tracing::warn!("{note}");
    }
    Ok(())
}

/// 〔E 吞错普查点名〕扫完一趟，有几个会话文件**读不动**（权限 / IO 错 / 不是合法 UTF-8）、因此没搜到。
/// 原先读不动的那一份 `.ok()?` 折成「无命中」—— 从结果里**静默消失、不计数**。
/// 结果的行形状不动（界面那一半要改前端与协议，登记为买不到）；逐份在读的那一刻 `warn`，扫完再出一行总数。
pub(crate) fn unreadable_note(n: usize) -> Option<String> {
    (n > 0)
        .then(|| format!("全文搜索：{n} 个会话文件读不动，这一趟没搜到它们（逐份原因见上面几行）"))
}

/// [`search`] 的本体：回「读不动、没搜到」的会话数（判据直接看这个数，不看日志）。
/// 走进程级常驻索引（本机常驻后端 / 远端流后端同一条路）；只在内存、随进程死 ——。
pub(crate) fn search_counting(
    agent_home: &Path,
    query: &str,
    opts: &SearchOpts,
    out: &mut impl Write,
) -> Result<usize, String> {
    let q = query.trim().to_lowercase();
    let root = projects_root(agent_home);
    if q.is_empty() || !root.is_dir() {
        return Ok(0); // 空查询 / 无 projects → 无输出（exit 0）
    }
    // 路径白名单根（read 的文件必须在其下，挡 symlink 逃逸）：observe 唯一那道围栏（`observe/fence.rs`）。
    let fence = Fence::at(&root)?;
    let mut resident = resident();
    let index = resident.entry(fence.root().to_path_buf()).or_default();
    let live = crate::observe::accounts_query::live_session_ids(agent_home);
    let unreadable = index.search(&fence, &q, opts, &live, out)?;
    let r = index.last;
    tracing::debug!(
        "history-search index: full={} appended={} reused={} bytes={}",
        r.full,
        r.appended,
        r.reused,
        r.bytes
    );
    Ok(unreadable)
}

/// 会话内查找走同一份常驻索引：这一份 (mtime, 长度) 没变不读、变长只读尾巴。
/// 回 `None` = 这一份不归索引管（不在 projects 下 / 读不动）⇒ 调用方退回现扫（[`scan_session_find`]，口径同一套）。
pub(crate) fn find_indexed(
    agent_home: &Path,
    target: &Path,
    query: &str,
    include_tools: bool,
    page: FindPage,
    on_hit: impl FnMut(&Value) -> std::io::Result<()>,
) -> Option<std::io::Result<(u64, u64)>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(Ok((0, 0)));
    }
    let fence = Fence::at(&projects_root(agent_home)).ok()?;
    let path = fence.admit(target).ok()?;
    let mut resident = resident();
    let index = resident.entry(fence.root().to_path_buf()).or_default();
    index.find(&path, &q, include_tools, page, on_hit)
}

/// 进程级常驻索引：规范化的 projects 根 → 那一棵的索引。后端一起来就由 [`warm_in_background`] 后台建；
/// 第一问赶在它前面到了也不白等：谁先拿到锁谁读那一份，另一方见它没变就不再读。
static RESIDENT: std::sync::Mutex<BTreeMap<PathBuf, SearchIndex>> =
    std::sync::Mutex::new(BTreeMap::new());

/// 〔第二问〕常驻索引最多留这么多字节的可搜文本（[`FileEntry::weight`] 的和）。按最近优先留；
/// 留不下的那几份照样读、照样搜，只是不留（下一问再读）⇒ 答案与不设上界逐字相等，变的只是那几份的读盘。
/// 本机正文约 11 MB、勾过「含工具」约 40 MB（`SX1.md §4`）⇒ 本机整份都留得下；历史大一个数量级的远端封在这里。
pub(crate) const RESIDENT_MAX_BYTES: usize = 64 << 20;

/// 中毒（某一问 panic 在半路）⇒ 整张表丢掉重建，不带着半截状态答。
fn resident() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, SearchIndex>> {
    RESIDENT.lock().unwrap_or_else(|p| {
        let mut g = p.into_inner();
        g.clear();
        RESIDENT.clear_poison();
        g
    })
}

/// 〔第一问〕后端一起来就后台建索引（一条一次性线程，不是定时器；建完就退）。
/// 只在内存（理由同 `SX1.md §1`，不落盘）；远端流后端与本机常驻后端同一个调用点（`main.rs`）。
pub fn warm_in_background(agent_home: PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("search-warm".into())
        .spawn(move || {
            let (files, kept) = warm(&agent_home);
            tracing::info!(
                "全文搜索索引：后台建好 {files} 份，常驻 {kept} 字节（上界 {RESIDENT_MAX_BYTES}）"
            );
        });
    if let Err(e) = spawned {
        tracing::warn!("全文搜索索引：后台建索引的线程起不来（{e}），第一问会现读");
    }
}

/// [`warm_in_background`] 的本体（判据直接调它）：按最近优先把每份会话读进常驻表，留到上界为止就停（再读也留不下）。
/// 每一份单独拿一次锁 ⇒ 半路来的一问不必等整棵建完。回 `(这一趟读进来的份数, 常驻字节)`。
pub(crate) fn warm(agent_home: &Path) -> (usize, usize) {
    let root = projects_root(agent_home);
    let Ok(fence) = Fence::at(&root) else {
        return (0, 0);
    };
    let mut read = 0usize;
    for (path, _) in session_files(&fence) {
        if fence.admit(&path).is_err() {
            continue;
        }
        let mut guard = resident();
        let index = guard.entry(fence.root().to_path_buf()).or_default();
        let prev = index.files.remove(&path);
        let was_there = prev.is_some();
        let Ok(entry) = index.bring_up(&path, prev, false) else {
            continue; // 读不动的留给那一问去说（它会逐份出声、计数）
        };
        let others = index.kept();
        if others + entry.weight > index.budget {
            break;
        }
        read += usize::from(!was_there);
        index.files.insert(path, entry);
    }
    let kept = resident().get(fence.root()).map_or(0, SearchIndex::kept);
    (read, kept)
}

/// 一棵 projects 下的会话文件，按最近优先排（搜索与预热同一个 walk、同一个排序）。
fn session_files(fence: &Fence) -> Vec<(PathBuf, i64)> {
    let mut files: Vec<(PathBuf, i64)> = WalkDir::new(fence.root())
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().is_file() && crate::agents::claudecode::records::is_session_file(e.path())
        })
        .map(|e| {
            let p = e.into_path();
            let m = mtime_ms(&p);
            (p, m)
        })
        .collect();
    // 🔴 `K-R100`：**snippet 预算按最近优先花**（`search_rules::sort_by_recency`，理由在它的文档注释里）。
    search_rules::sort_by_recency(&mut files, |(_, m)| *m);
    files
}

/// 追加读之前核的那一段：`consumed` 之前最多这么多字节；对不上 ⇒ 被改写过、整份重读。
const WITNESS_BYTES: usize = 256;

/// 一棵 projects 的索引：每份会话文件一格，按 (mtime, 长度) 增量读。
pub(crate) struct SearchIndex {
    files: BTreeMap<PathBuf, FileEntry>,
    /// 上一问读盘的账（判据 J3 看它）。
    last: Refresh,
    /// 常驻上界（生产恒为 [`RESIDENT_MAX_BYTES`]；判据用小的量「留不下」那一格）。
    budget: usize,
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::with_budget(RESIDENT_MAX_BYTES)
    }
}

/// 一问里读盘的账：整份读几份 · 追加读几份 · 没读几份 · 一共读了多少字节。
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refresh {
    pub(crate) full: usize,
    pub(crate) appended: usize,
    pub(crate) reused: usize,
    pub(crate) bytes: u64,
}

/// 一份会话文件在索引里的样子。`[0, consumed)` 是已读进 `done` 的完整行；其后没写完的那截在 `tail`（旧现扫也把它当一行看）。
struct FileEntry {
    /// 上一次读完时的 (读之前 stat 的 mtime, 真读到的长度)。
    seen: (Option<std::time::SystemTime>, u64),
    consumed: u64,
    witness: Vec<u8>,
    done: Facts,
    tail: Facts,
    /// 读不动（不是合法 UTF-8）：坏在完整行里 ⇒ 下次变了就整份重读；坏在残尾里 ⇒ 追加读会重看它。
    bad: Option<(BadAt, String)>,
    /// 记录里抽没抽工具文本。
    tools: bool,
    /// 这一格常驻的字节（估算：可搜文本 ＋ 每条记录的定长开销 ＋ 见证）；每次 [`FileEntry::take`] 之后重算。
    weight: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BadAt {
    Done,
    Tail,
}

/// 一段完整行 / 残尾里抽出的会话事实：会话级三格（cwd 取第一个非空 · 标题取最后一个 · 首条 user 摘要取第一个）＋ 可搜的记录。
#[derive(Default)]
struct Facts {
    cwd: Option<String>,
    /// 有一条记录带 `sessionKind:"bg"`（后台分身会话；与历史清单同一个信号）。
    bg: bool,
    title: Option<String>,
    excerpt: String,
    records: Vec<Rec>,
}

/// 一条 user / assistant 记录。工具文本**按需**抽：第一次有 `include_tools` 的那一问才整份重读补上（它要把整块工具输入 / 输出串起来，贵）。
struct Rec {
    rt: RecordText,
    ts_ms: i64,
    uuid: String,
    /// 这一条是你说的一句（一轮的开头；口径同大纲 `user_inputs::user_input_of`）。
    opens_turn: bool,
}

impl Facts {
    /// 逐行吸收一段文本（与旧现扫逐行那段同一套分支，只是不看查询）。
    fn absorb(&mut self, text: &str, tools: bool) {
        for line in text.lines() {
            let trimmed = line.trim_start_matches('\u{feff}').trim();
            if trimmed.is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("sessionKind").and_then(Value::as_str) == Some("bg") {
                self.bg = true;
            }
            if self.cwd.is_none() {
                if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                    if !c.is_empty() {
                        self.cwd = Some(c.to_string());
                    }
                }
            }
            match v.get("type").and_then(Value::as_str).unwrap_or("") {
                "ai-title" => {
                    if let Some(t) = v.get("aiTitle").and_then(Value::as_str) {
                        self.title = Some(t.to_string());
                    }
                }
                "custom-title" => {
                    if let Some(t) = v.get("customTitle").and_then(Value::as_str) {
                        self.title = Some(t.to_string());
                    }
                }
                "user" | "assistant" => {
                    let opens_turn = super::user_inputs::user_input_of(&v).is_some();
                    let Some(rt) = record_text(&v, tools).or_else(|| {
                        opens_turn.then(|| RecordText {
                            is_assistant: false,
                            report: false,
                            main: String::new(),
                            tool: String::new(),
                        })
                    }) else {
                        continue;
                    };
                    if !rt.is_assistant
                        && !rt.report
                        && self.excerpt.is_empty()
                        && !rt.main.is_empty()
                    {
                        self.excerpt = search_rules::truncate_excerpt(&rt.main, 120);
                    }
                    let ts_ms = v
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .and_then(parse_iso8601_ms)
                        .unwrap_or(0);
                    let uuid = v
                        .get("uuid")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    self.records.push(Rec {
                        rt,
                        ts_ms,
                        uuid,
                        opens_turn,
                    });
                }
                _ => {}
            }
        }
    }

    fn weight(&self) -> usize {
        let text = |o: &Option<String>| o.as_ref().map_or(0, String::len);
        text(&self.cwd)
            + text(&self.title)
            + self.excerpt.len()
            + self
                .records
                .iter()
                .map(|r| {
                    std::mem::size_of::<Rec>() + r.rt.main.len() + r.rt.tool.len() + r.uuid.len()
                })
                .sum::<usize>()
    }

    /// 把**后面**那一段并进来：按段序合并与整份顺扫相等。
    fn extend(&mut self, later: Facts) {
        if self.cwd.is_none() {
            self.cwd = later.cwd;
        }
        if later.title.is_some() {
            self.title = later.title;
        }
        if self.excerpt.is_empty() {
            self.excerpt = later.excerpt;
        }
        self.records.extend(later.records);
    }
}

impl FileEntry {
    fn empty(mtime: Option<std::time::SystemTime>, tools: bool) -> Self {
        Self {
            seen: (mtime, 0),
            consumed: 0,
            witness: Vec::new(),
            done: Facts::default(),
            tail: Facts::default(),
            bad: None,
            tools,
            weight: 0,
        }
    }

    /// 从 `consumed` 起新读到的字节并进来：最后一个 `\n` 之前的完整行进 `done`，之后的残尾整个换掉 `tail`。
    /// 切点紧跟 `\n`（ASCII）⇒ 每段是不是合法 UTF-8 与整份是不是同一个答案。
    fn take(&mut self, mtime: Option<std::time::SystemTime>, new: &[u8]) {
        let end = self.consumed + new.len() as u64;
        let cut = new.iter().rposition(|&b| b == b'\n').map_or(0, |k| k + 1);
        let (whole, rest) = new.split_at(cut);
        self.bad = None;
        self.tail = Facts::default();
        if !whole.is_empty() {
            match std::str::from_utf8(whole) {
                Ok(s) => {
                    let mut seg = Facts::default();
                    seg.absorb(s, self.tools);
                    self.done.extend(seg);
                }
                Err(e) => {
                    *self = Self::empty(mtime, self.tools);
                    self.bad = Some((BadAt::Done, e.to_string()));
                }
            }
            if self.bad.is_none() {
                self.consumed += cut as u64;
                let mut w = std::mem::take(&mut self.witness);
                w.extend_from_slice(whole);
                self.witness = w.split_off(w.len() - w.len().min(WITNESS_BYTES));
            }
        }
        if self.bad.is_none() {
            match std::str::from_utf8(rest) {
                Ok(s) => self.tail.absorb(s, self.tools),
                Err(e) => self.bad = Some((BadAt::Tail, e.to_string())),
            }
        }
        self.seen = (mtime, end);
        self.weight = self.witness.len() + self.done.weight() + self.tail.weight();
    }
}

impl SearchIndex {
    pub(crate) fn with_budget(budget: usize) -> Self {
        Self {
            files: BTreeMap::new(),
            last: Refresh::default(),
            budget,
        }
    }

    /// 常驻的字节（每格 [`FileEntry::weight`] 的和）。
    pub(crate) fn kept(&self) -> usize {
        self.files.values().map(|e| e.weight).sum()
    }

    /// 留得下就留（会话内查找那一臂；全局搜索那一臂按最近优先整趟重排）。
    fn keep(&mut self, path: &Path, entry: FileEntry) {
        if self.kept() + entry.weight <= self.budget {
            self.files.insert(path.to_path_buf(), entry);
        }
    }

    /// 这一份带到这一问：(mtime, 长度) 没变不读 · 变长且见证对得上只读尾巴 · 其余整份重读。打不开 / 读不了 ⇒ `Err(原因)`。
    fn bring_up(
        &mut self,
        path: &Path,
        prev: Option<FileEntry>,
        tools: bool,
    ) -> Result<FileEntry, String> {
        use std::io::{Read, Seek, SeekFrom};
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        let seen = (meta.modified().ok(), meta.len());
        // 这一问要工具文本而这一格没抽过 ⇒ 整份重读。
        if let Some(mut e) = prev.filter(|e| e.tools || !tools) {
            if e.seen == seen {
                self.last.reused += 1;
                return Ok(e);
            }
            let grew =
                seen.1 > e.seen.1 && e.bad.as_ref().map_or(true, |(at, _)| *at == BadAt::Tail);
            if grew {
                let w = e.witness.len() as u64;
                let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
                let mut buf = Vec::new();
                f.seek(SeekFrom::Start(e.consumed - w))
                    .and_then(|_| f.read_to_end(&mut buf))
                    .map_err(|e| e.to_string())?;
                self.last.bytes += buf.len() as u64;
                if buf.len() as u64 >= w && buf[..w as usize] == e.witness[..] {
                    self.last.appended += 1;
                    e.take(seen.0, &buf[w as usize..]);
                    return Ok(e);
                }
            }
        }
        let mut buf = Vec::new();
        std::fs::File::open(path)
            .and_then(|mut f| f.read_to_end(&mut buf))
            .map_err(|e| e.to_string())?;
        self.last.full += 1;
        self.last.bytes += buf.len() as u64;
        let mut e = FileEntry::empty(seen.0, tools);
        e.take(seen.0, &buf);
        Ok(e)
    }

    /// 一份会话里按文件序出命中：只看完整行（`done`，与 [`scan_session_find`] 同）、跳过没 uuid 的。`q` 已小写、已 trim。
    fn find(
        &mut self,
        path: &Path,
        q: &str,
        include_tools: bool,
        page: FindPage,
        mut on_hit: impl FnMut(&Value) -> std::io::Result<()>,
    ) -> Option<std::io::Result<(u64, u64)>> {
        self.last = Refresh::default();
        let prev = self.files.remove(path);
        let entry = self.bring_up(path, prev, include_tools).ok()?;
        if entry.bad.is_some() {
            self.keep(path, entry);
            return None; // 读不动：现扫那一臂是 lossy 读，口径不许在这里变
        }
        let mut pager = FindPager::new(page);
        let mut result = Ok(());
        let mut turn: u64 = 0;
        // 会话内查找（主窗口与查看器的查找面板）不收 agent 回报：那一种只在全局搜索里单列（历史页「搜内容时」）。
        for rec in entry.done.records.iter() {
            if rec.opens_turn {
                turn += 1;
            }
            if rec.uuid.is_empty() || rec.rt.report {
                continue;
            }
            let Some((kind, hit)) = record_hit(&rec.rt, q, include_tools) else {
                continue;
            };
            if pager.take() && result.is_ok() {
                result = on_hit(&find_hit(&rec.uuid, kind, hit, q, turn, rec.ts_ms));
            }
        }
        let (count, total) = pager.finish();
        self.keep(path, entry);
        Some(result.map(|()| (count, total)))
    }

    /// 扫 projects/**/*.jsonl（与旧现扫同一个 walk、同一个排序、同一道围栏），每命中会话输出一行 JSON。`q` 已小写、已 trim。
    fn search(
        &mut self,
        fence: &Fence,
        q: &str,
        opts: &SearchOpts,
        live: &std::collections::BTreeSet<String>,
        out: &mut impl Write,
    ) -> Result<usize, String> {
        let files = session_files(fence);

        // 这一趟 walk 没走到的（删了 / 改名）随旧表一起丢掉。
        let mut prev = std::mem::take(&mut self.files);
        self.last = Refresh::default();
        let mut budget = SnippetBudget::new(opts.limit);
        let mut unreadable = 0usize;
        // 按最近优先留到上界（`files` 已是这个序）；留不下的这一问照样搜，只是不留。
        let mut kept = 0usize;
        for (path, updated_at) in files {
            // 防 symlink 逃逸：解开之后仍须在 projects/ 下；解不开 / 越界 ⇒ 跳过这一份（与先前同）。
            if fence.admit(&path).is_err() {
                continue;
            }
            // 读不动 ⇒ 说出来、记一笔（原先 `.ok()?` 折成「无命中」静默消失）。
            // 打不开 / 读不了 ⇒ 这一格丢掉；不是合法 UTF-8 ⇒ 这一格留着（没变就不再读），照样说、照样数。
            let (entry, bad) = match self.bring_up(&path, prev.remove(&path), opts.include_tools) {
                Ok(e) => {
                    let bad = e.bad.as_ref().map(|(_, why)| why.clone());
                    (Some(e), bad)
                }
                Err(why) => (None, Some(why)),
            };
            let session = match (&entry, &bad) {
                (Some(e), None) if opts.titles => session_title_row(&path, e, q, updated_at),
                (Some(e), None) => session_hits_in(&path, e, q, opts, &mut budget, updated_at),
                _ => None,
            };
            let bg = entry.as_ref().is_some_and(|e| e.done.bg || e.tail.bg);
            if let Some(e) = entry.filter(|e| kept + e.weight <= self.budget) {
                kept += e.weight;
                self.files.insert(path.clone(), e);
            }
            if let Some(e) = bad {
                unreadable += 1;
                tracing::warn!("全文搜索：读不动 {}（{e}），这一份没搜", path.display());
                continue;
            }
            if let Some(mut session) = session {
                // 能不能恢复由这台判（活不活看这台的 pidfile），与历史清单的行同一个函数。
                let sid = session["sessionId"].as_str().unwrap_or_default();
                let status = crate::observe::history_query::status_of(Some(live.contains(sid)));
                let kind = session["agent"].as_str().unwrap_or_default().to_string();
                session["isBg"] = serde_json::json!(bg);
                session["status"] = serde_json::json!(status);
                session["can"] = crate::observe::history_query::can_of(&kind, status, bg);
                writeln!(out, "{session}").map_err(|e| format!("stdout write failed: {e}"))?;
            }
        }
        Ok(unreadable)
    }
}

/// 一个会话（索引里那一格）的命中 JSON（无命中 → None）。`budget` 跨会话累计已构造
/// snippet 数，达到 `opts.limit` 后只计数不再构造 snippet（贵活封顶）——
/// 🔴 判定在 `search_rules::SnippetBudget`（只此一份），且它**分得清**
/// 「全局预算用完」与「单会话满 `PER_SESSION_CAP` 条」（收口前这两件事挤在一个
/// `if` 里，下游只看得到 `hitCount > hits.len()` 这一个信号）。
/// `updated_at` 由调用方传入（排序时已 stat 过一次，别再 stat 第二次）。
fn session_hits_in(
    path: &Path,
    entry: &FileEntry,
    q_lc: &str,
    opts: &SearchOpts,
    budget: &mut SnippetBudget,
    updated_at: i64,
) -> Option<Value> {
    let session_id = path.file_stem()?.to_str()?.to_string();

    let mut hits: Vec<Value> = Vec::new();
    let mut hit_count: u32 = 0;
    // 本会话有命中因**全局预算用完**而拿不到 snippet（≠ 单会话超 PER_SESSION_CAP）。
    let mut session_starved = false;
    for rec in entry.done.records.iter().chain(entry.tail.records.iter()) {
        // scope 过滤：想要 user 却是 assistant（或反之）→ 跳过。
        if let Some(s) = opts.scope.as_deref() {
            if !in_scope(s, &rec.rt) {
                continue;
            }
        }
        // 时间过滤。
        if opts.after_ms > 0 && rec.ts_ms < opts.after_ms {
            continue;
        }
        let Some((hkind, text)) = record_hit(&rec.rt, q_lc, opts.include_tools) else {
            continue;
        };
        hit_count += 1;
        match budget.take(hits.len()) {
            SnippetVerdict::Give => {
                let (before, matched, after) = search_rules::make_snippet(text, q_lc);
                hits.push(serde_json::json!({
                    "uuid": rec.uuid,
                    "tsMs": rec.ts_ms,
                    "kind": hkind,
                    "before": before,
                    "matched": matched,
                    "after": after,
                }));
            }
            SnippetVerdict::BudgetExhausted => session_starved = true,
            SnippetVerdict::SessionCapped => {}
        }
    }

    if hit_count == 0 {
        return None;
    }
    Some(session_row(
        path,
        entry,
        session_id,
        updated_at,
        hit_count,
        hits,
        session_starved,
    ))
}

/// 只比标题与第一句（帧面的 `titles`）：标题 ／ 第一句里有这几个字 ⇒ 一行（`hitCount` 0、`hits` 空）。
fn session_title_row(path: &Path, entry: &FileEntry, q_lc: &str, updated_at: i64) -> Option<Value> {
    let session_id = path.file_stem()?.to_str()?.to_string();
    let title = entry.tail.title.as_ref().or(entry.done.title.as_ref());
    let excerpt = if entry.done.excerpt.is_empty() {
        &entry.tail.excerpt
    } else {
        &entry.done.excerpt
    };
    let found = title.is_some_and(|t| t.to_lowercase().contains(q_lc))
        || excerpt.to_lowercase().contains(q_lc);
    found.then(|| session_row(path, entry, session_id, updated_at, 0, Vec::new(), false))
}

/// 一个会话的那一行（命中那几格由调用方给）。
fn session_row(
    path: &Path,
    entry: &FileEntry,
    session_id: String,
    updated_at: i64,
    hit_count: u32,
    hits: Vec<Value>,
    session_starved: bool,
) -> Value {
    let project_path = entry
        .done
        .cwd
        .clone()
        .or_else(|| entry.tail.cwd.clone())
        .unwrap_or_default();
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| project_path.clone());
    let ai_title = entry.tail.title.as_ref().or(entry.done.title.as_ref());
    let first_user_excerpt = if entry.done.excerpt.is_empty() {
        &entry.tail.excerpt
    } else {
        &entry.done.excerpt
    };
    let title = search_rules::session_title(
        ai_title.map(String::as_str),
        first_user_excerpt,
        &session_id,
    );
    serde_json::json!({
        // 全文搜索只扫记录树（`projects/`）⇒ 命中的会话都是记录树那一家的。
        "agent": crate::agents::record_tree_kind().unwrap_or_default(),
        "sessionId": session_id,
        "projectPath": project_path,
        "projectName": project_name,
        "jsonlPath": path.to_string_lossy(),
        "title": title,
        "updatedAt": updated_at,
        "hitCount": hit_count,
        "hits": hits,
        // 🔴 `K-R100`：**远端截断从此说得出话。** 收口前这一行不存在 ⇒ 一份
        // `hitCount: 12, hits: []` 与「这个会话没什么可看的」在 monitor 与前端眼里同形，
        // 而 `merge_search_results`〔散文墓碑〕逐字 `truncated: local.truncated` 把远端那一半整个丢掉。
        "hitsTruncated": session_starved,
    })
}

/// 一条 user / assistant 记录拿去搜的两段文本（从 `session_hits_in` 里拆出来）。
pub(crate) struct RecordText {
    pub(crate) is_assistant: bool,
    /// user 那侧的这一条是 agent 回报（同一会话里子 agent 交回 / 发来的话 · 另一个会话发来的话），不是人说的。
    pub(crate) report: bool,
    /// 正文：assistant 是文本块；user 那侧只有人说的话（`agents::human_speech`）。按 `MAIN_CAP` 截断。
    pub(crate) main: String,
    /// 工具内容（tool_use 入参 / tool_result 输出 / thinking），按 `TOOL_CAP` 截断；不搜工具时空串。
    pub(crate) tool: String,
}

/// 一条已解析的记录 → 拿去搜的文本；不是 user / assistant ⇒ `None`。
///
/// 🔴 **全局搜索（`--search`）与会话内查找（`--find-in-session`）的口径只有这一个住址**；
/// 抽取 / 「谁说的」住适配层（经注册表 `agents::main_text` · `tool_text` · `human_speech`），截断住 `search_rules`（原 `search-core`）。
pub(crate) fn record_text(v: &Value, include_tools: bool) -> Option<RecordText> {
    let is_assistant = match v.get("type").and_then(Value::as_str) {
        Some("assistant") => true,
        Some("user") => false,
        _ => return None,
    };
    let content_v = v.get("message").and_then(|m| m.get("content"));
    // 搜的是记录树那一家的记录。
    let kind = crate::agents::record_tree_kind().unwrap_or_default();
    // user 那侧：人说的话是 user 命中；agent 回报（子 agent 交回 / 发来的话 · 另一个会话发来的话）单列一种（`report`）；
    //   后台通知 · 系统注入都不搜。谁说的由适配层判（`agents::user_text_of` 的 `speaker`）。
    let (raw_main, report) = if is_assistant {
        (
            content_v
                .map(|c| crate::agents::main_text(kind, c))
                .unwrap_or_default(),
            false,
        )
    } else {
        match crate::agents::human_speech(kind, v) {
            Some(said) => (said, false),
            None => match crate::agents::user_text_of(kind, v).and_then(report_text) {
                Some(said) => (said, true),
                None => (String::new(), false),
            },
        }
    };
    let main = search_rules::truncate_plain(&raw_main, MAIN_CAP);
    let tool = if include_tools {
        content_v
            .map(|c| {
                search_rules::truncate_plain(
                    &crate::agents::tool_text(kind, c, is_assistant),
                    TOOL_CAP,
                )
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    Some(RecordText {
        is_assistant,
        report,
        main,
        tool,
    })
}

/// 这一条来话是 agent 回报（搜索里单列的那一种）⇒ 搜它的正文（框里那一段，适配层给的 `body`；没有就原文）。
fn report_text(u: crate::agents::UserText) -> Option<String> {
    match u.speaker {
        crate::agents::Speaker::AgentMessage { body, .. }
        | crate::agents::Speaker::PeerSession { body, .. } => Some(body.unwrap_or(u.text)),
        _ => None,
    }
}

/// `scope` 留不留这一条：`user` 人说的 · `assistant` 那一家说的 · `report` agent 回报。
pub(crate) fn in_scope(scope: &str, rt: &RecordText) -> bool {
    match scope {
        "user" => !rt.is_assistant && !rt.report,
        "assistant" => rt.is_assistant,
        "report" => rt.report,
        _ => true,
    }
}

/// 命中判定：先看正文、再看工具内容（大小写不敏感子串）。命中 ⇒ `(种类, 命中的那段文本)`，
/// 种类是 `"user"` / `"assistant"` / `"report"` / `"tool"`（与 `Hit.kind` 同一套词）。`q_lc` 已小写、已 trim。
/// 工具内容只在 `include_tools` 时看（索引里的那一格可能抽过工具文本，不看时当它是空串）。
pub(crate) fn record_hit<'a>(
    rt: &'a RecordText,
    q_lc: &str,
    include_tools: bool,
) -> Option<(&'static str, &'a str)> {
    if rt.main.to_lowercase().contains(q_lc) {
        let kind = if rt.is_assistant {
            "assistant"
        } else if rt.report {
            "report"
        } else {
            "user"
        };
        return Some((kind, &rt.main));
    }
    if include_tools && !rt.tool.is_empty() && rt.tool.to_lowercase().contains(q_lc) {
        return Some(("tool", &rt.tool));
    }
    None
}

/// 会话内查找一次最多列多少条（缺省）。尾行照报**全量**命中数。
pub(crate) const FIND_DEFAULT_LIMIT: usize = 500;
/// 会话内查找的上限封顶（调用方要得再多也只列这么多）。
pub(crate) const FIND_MAX_LIMIT: usize = 2000;

/// 会话内查找要哪一页：跳过前 `skip` 条命中，再列至多 `limit` 条（滚到底续下一页时 `skip` ＝ 已拿到的条数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FindPage {
    pub(crate) skip: usize,
    pub(crate) limit: usize,
}

impl FindPage {
    pub(crate) fn first(limit: usize) -> Self {
        Self { skip: 0, limit }
    }
}

/// 一页的账：每条命中都记进全量数，只有落在这一页里的才交出去。
struct FindPager {
    page: FindPage,
    count: u64,
    total: u64,
}

impl FindPager {
    fn new(page: FindPage) -> Self {
        Self {
            page,
            count: 0,
            total: 0,
        }
    }

    /// 又一条命中：落在这一页里 ⇒ `true`（调用方把它交出去）。
    fn take(&mut self) -> bool {
        self.total += 1;
        let inside =
            self.total as usize > self.page.skip && (self.count as usize) < self.page.limit;
        if inside {
            self.count += 1;
        }
        inside
    }

    fn finish(self) -> (u64, u64) {
        (self.count, self.total)
    }
}

/// 一条命中的成品：片段三段 ＋ 第几轮（你说的第几句之后；第一句之前 ＝ 0）＋ 那条记录的时刻（毫秒，读不出 ＝ 0）。
fn find_hit(uuid: &str, kind: &str, hit: &str, q: &str, turn: u64, ts_ms: i64) -> Value {
    let (before, matched, after) = search_rules::make_snippet(hit, q);
    serde_json::json!({
        "uuid": uuid,
        "kind": kind,
        "before": before,
        "matched": matched,
        "after": after,
        "turn": turn,
        "tsMs": ts_ms,
    })
}

/// **会话内查找**的内核：读 `r`（一份会话，从头）逐行找 `query`，
/// 出三段（形状登记 `IPC-PROTOCOL.md §10.5`）：
/// 1. 头 `{"kind":"session_find","v":1}`；
/// 2. 每条命中一行 `{"uuid","kind","before","matched","after","turn","tsMs"}`，**按文件序**（= 对话序），最多 `limit` 条；
/// 3. 尾 `{"kind":"session_find_end","count":N,"total":T}` —— `T` = 全量命中数（≥ N）。**没有尾行 ⇒ 截断**。
///
/// 与 `--search` 的差别只在「扫哪些文件、给多少条」：口径（[`record_text`] / [`record_hit`] ＋ `search_rules`
/// 的片段）同一份。没有 uuid 的记录不算（跳不过去 —— 列出来就是一条点了没反应的项）。
/// 只看**完整行**（torn 残尾下一次再看）。空查询 ⇒ 零条。返回 `(count, total)`。
pub(crate) fn write_session_find<R: std::io::BufRead, W: std::io::Write>(
    r: R,
    query: &str,
    include_tools: bool,
    limit: usize,
    out: &mut W,
) -> std::io::Result<(u64, u64)> {
    writeln!(out, "{{\"kind\":\"session_find\",\"v\":1}}")?;
    let (count, total) =
        scan_session_find(r, query, include_tools, FindPage::first(limit), |hit| {
            serde_json::to_writer(&mut *out, hit)?;
            out.write_all(b"\n")
        })?;
    writeln!(
        out,
        "{{\"kind\":\"session_find_end\",\"count\":{count},\"total\":{total}}}"
    )?;
    Ok((count, total))
}

/// [`write_session_find`] 的中段：**逐条命中交给 `on_hit`**（按文件序、只交 `page` 那一页），
/// 回 `(count, total)`。判定一行都不在这一层之外 —— CLI 那一臂（上面，写头尾三段：stdout 要分帧）与帧面那一臂
/// （`read_face.rs` 的 `history-find`，把同一串命中装成成品 `{total, hits}`）跑的是**同一个**扫描。
/// 第几轮：每遇到你说的一句（口径同大纲 `user_inputs::user_input_of`）加一，与常驻索引那一臂同一个数法。
/// `on_hit` 回错 ⇒ 扫描当场停、错原样上抛（帧面那一臂靠它在整份超上限时停下）。
pub(crate) fn scan_session_find<R: std::io::BufRead>(
    mut r: R,
    query: &str,
    include_tools: bool,
    page: FindPage,
    mut on_hit: impl FnMut(&Value) -> std::io::Result<()>,
) -> std::io::Result<(u64, u64)> {
    let q = query.trim().to_lowercase();
    let mut pager = FindPager::new(page);
    let mut turn: u64 = 0;
    let mut buf: Vec<u8> = Vec::new();
    while !q.is_empty() {
        buf.clear();
        let read = r.read_until(b'\n', &mut buf)?;
        if read == 0 || buf.last() != Some(&b'\n') {
            break;
        }
        let text = String::from_utf8_lossy(&buf[..buf.len() - 1]);
        let Ok(v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}').trim())
        else {
            continue;
        };
        if matches!(
            v.get("type").and_then(Value::as_str),
            Some("user" | "assistant")
        ) && super::user_inputs::user_input_of(&v).is_some()
        {
            turn += 1;
        }
        let Some(uuid) = v
            .get("uuid")
            .and_then(Value::as_str)
            .filter(|u| !u.is_empty())
        else {
            continue;
        };
        let Some(rt) = record_text(&v, include_tools).filter(|rt| !rt.report) else {
            continue;
        };
        let Some((kind, hit)) = record_hit(&rt, &q, include_tools) else {
            continue;
        };
        if pager.take() {
            let ts_ms = v
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_iso8601_ms)
                .unwrap_or(0);
            on_hit(&find_hit(uuid, kind, hit, &q, turn, ts_ms))?;
        }
    }
    Ok(pager.finish())
}

// === 文本抽取 / snippet / 截断：**一份都不在这里**（`K-R100`） ===
//
// 🔴 那 12 个助手（`extract_text_blocks` · `extract_tool_text` · `stringify_json` ·
// `user_text` · `make_snippet` · `find_ci` · `tail_chars` · `head_chars` ·
// `collapse_ws` · `collapse_ws_keep_ellipsis` · `truncate_plain` · `truncate_excerpt`）
// 与它们的单元测试只有一个家：通用的住 `search_rules.rs`，Claude 记录文本那三个住 `agents/claudecode/text.rs`
// —— **同一份**。别在这里「顺手再写一个小的」：那就是收口前的形状（两份、逐字同、零判据）。

/// 解析 Claude 的 ISO8601 时间戳 `YYYY-MM-DDTHH:MM:SS(.fff)?Z` → epoch ms。
/// 自带 civil-days 算法（Howard Hinnant），无需 chrono。
/// 开成 `pub(crate)`：历史会话清单那一行的开始时刻（`history_query::analyze_session`）用同一份。
pub(crate) fn parse_iso8601_ms(s: &str) -> Option<i64> {
    if s.len() < 19 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let mon: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;
    // 小数秒：扫 '.' 之后的数字串，归一到毫秒（取前 3 位、不足右补 0），对齐本地 utils
    // 口径——容忍 1/2/3+ 位小数（真实 Claude 总是 .fffZ，但稳健处理变体）。
    let millis = if s.as_bytes().get(19) == Some(&b'.') {
        let mut frac: String = s[20..]
            .chars()
            .take_while(char::is_ascii_digit)
            .take(3)
            .collect();
        while !frac.is_empty() && frac.len() < 3 {
            frac.push('0');
        }
        frac.parse::<i64>().unwrap_or(0)
    } else {
        0
    };
    let days = days_from_civil(year, mon, day);
    Some((days * 86_400 + hour * 3_600 + min * 60 + sec) * 1_000 + millis)
}

/// days since 1970-01-01 for a civil (proleptic Gregorian) date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 帧命令 `history-search-merge`：**把各台 `history-search` 的会话行合成一份**。
///
/// 界面照旧逐台经通道问那台常驻后端的 `history-search`（各台内存索引保热），拿回来的会话行（界面已补 `origin`）原样交到这里：
/// `{sessions: [<会话行>…]}` ⇒ `{totalHits, sessionCount, truncated, sessions}`。
/// - 排序：`updatedAt` 倒序、稳定（[`search_rules::sort_by_recency`]，与每台后端花 snippet 预算的顺序同一个函数）；
/// - `totalHits` = 各会话 `hitCount` 之和；任一会话 `hitsTruncated` ⇒ `truncated`（`K-R100`：每一台都被自己的 `limit` 砍过）；
/// - 会话行其余各格原样透传（形状由界面的解码器收，这里只读排序与计数要的那三格）。
///
/// 码：`bad_args`（不是 `{sessions: [...]}` / 某一行缺那三格或类型不对）。纯计算：不读盘、不起进程。
pub(crate) fn answer_merge(args: &Value) -> Result<Value, (&'static str, String)> {
    let bad = |d: &str| ("bad_args", crate::common::contract::malformed(d));
    let rows = args
        .get("sessions")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("missing `sessions` (an array)"))?;
    let mut sessions: Vec<(i64, Value)> = Vec::with_capacity(rows.len());
    let mut total_hits: u64 = 0;
    let mut truncated = false;
    for row in rows {
        let updated = row
            .get("updatedAt")
            .and_then(Value::as_i64)
            .ok_or_else(|| bad("a session without an integer `updatedAt`"))?;
        let hits = row
            .get("hitCount")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("a session without a non-negative integer `hitCount`"))?;
        let cut = row
            .get("hitsTruncated")
            .and_then(Value::as_bool)
            .ok_or_else(|| bad("a session without a boolean `hitsTruncated`"))?;
        total_hits = total_hits.saturating_add(hits);
        truncated |= cut;
        sessions.push((updated, row.clone()));
    }
    search_rules::sort_by_recency(&mut sessions, |(updated, _)| *updated);
    Ok(serde_json::json!({
        "totalHits": total_hits,
        "sessionCount": sessions.len(),
        "truncated": truncated,
        "sessions": sessions.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/search_query_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/backend/observe/search_query_find_tests.rs"]
mod find_tests;

// J1：应答 == 起步树现扫实现冻结下来的金样（逐问逐行逐字节）。
#[cfg(test)]
#[path = "../../../tests/backend/observe/search_query_golden_tests.rs"]
mod golden_tests;

// J2 增量 == 整份重读 · J3 帧面那一臂只读变了的字节。
#[cfg(test)]
#[path = "../../../tests/backend/observe/search_query_index_tests.rs"]
mod index_tests;

// 秤：真规模本机历史上的冷首趟 / 热态（`#[ignore]` 读数，不是判据）。
#[cfg(test)]
#[path = "../../../tests/backend/observe/search_query_reading.rs"]
mod reading;
