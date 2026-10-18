//! 一份会话记录一张扫描图：冷开一条会话时界面问的那几样（尾段 · 骨架索引 · 用户输入 · 轮次 · 事实）一遍扫出来，
//! 按（路径 · 长度 · 修改时刻）留着，那几条帧命令（`history-tail` · `-index` · `-user-inputs` · `-turns` · `-facts`）从字节 0 问时共用。
//!
//! - 一遍：每个完整行只解析一次，同一个值喂给五样各自的那一步（各自的口径仍只住各自的模块：
//!   [`super::recognize::index_row::index_row_of`] · [`super::recognize::user_inputs::user_input_of`] · [`super::recognize::turns::TurnScan`] ·
//!   [`super::recognize::facts::note_record`] · [`super::lines::counts`]）。
//! - 留着：没变 ⇒ 那一张；变长了 ⇒ 核游标前那几个字节（见证），对得上就从那一张接着扫追加的那一截（还没收尾那一轮的扫描状态
//!   也留着）、对不上 ⇒ 整份重扫；条数与估算字节各有上限，超了先淘汰最久没用的；单张比字节上限还大 ⇒ 照留、别的全挤掉
//!   （问的时候它本来就整张在内存里；不留就每问回盘重读）。常驻上界 = 字节上限与眼下最大那一张取大。
//! - 落盘：大记录扫完落一份骨架（[`super::skeleton`]）；进程里没有那一张时照骨架起步 —— 没变 ⇒ 不读，变长了 ⇒ 只读追加的那一截。
//! - 同时冷开同一份（界面开一条会话几问是一齐发的）：先到的那一个扫，后到的等它扫完拿同一张 —— 那一张是它们发问之后才扫完的，
//!   每样都带着 `end`，续取从那里接。
//! - 只扫到问的那一刻的长度：之后写进来的不在这一张里（键对得上内容）。
//! - 事实尾巴：一张图被挤掉时按路径留下它的事实累加器（游标 · 见证 · 累加器），至多 [`TAIL_KEEP`] 条；
//!   之后问事实（[`RecordScans::facts`]）图不在、尾巴在且见证对得上 ⇒ 只认事实地接着读追加的那一截，不读骨架、不整份重扫。

use super::recognize::facts::{ContextLimits, SessionFacts};
use super::recognize::index_row::IndexRow;
use super::recognize::turns::{TurnRow, TurnScan};
use super::recognize::user_inputs::UserInputRow;
use super::skeleton::{Look, Mark, Skeletons};
use crate::agents::mainline::Chain;
use crate::platform::fs::{Ro, RoFile};
use cmd_wire::session::TailPlan;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::SystemTime;

/// 一遍扫出来的那几样（都是从字节 0 起）。
#[derive(Debug, Default)]
pub(crate) struct ScanMap {
    /// 最后一个完整行之后的字节位置（残尾不算）。
    pub(crate) end: u64,
    /// 扫的那一刻的文件长度（进程里留着的那张续扫时认「变长了」用）。
    seen: u64,
    /// 每个可计行的字节起点（尾段按 `n` 现切）。
    starts: Vec<u64>,
    pub(crate) index: Vec<IndexRow>,
    pub(crate) inputs: Vec<UserInputRow>,
    pub(crate) turns: Vec<TurnRow>,
    /// 主线外清单（回退掉的那几条，文件序；`history-branch`）。用户输入与轮次已经不含它们。
    pub(crate) off: Vec<String>,
    /// 定上下文上限之前的事实（上限按每次问的上限表现定，[`RecordScans::facts`]）。
    pub(crate) facts: SessionFacts,
    /// 接着扫要的那几样（骨架也落它）。
    rest: Rest,
    /// 估算的常驻字节（字节上限按它算）。
    bytes: usize,
    /// 盘上骨架照的是读到哪的这一份（没落过 ⇒ 0）：接着扫过了门槛才再落（[`super::skeleton::Skeletons::rewrite_after`]）。
    stored: u64,
}

/// 接着扫要的：游标前的见证 · 没滤主线外的用户输入与收了尾的轮次 · 还没收尾那一轮 · 链索引；
/// 外加实时跟读起步要的两样：说到子运行的那几行原文（过了预筛的）· 最后一件说了的「此刻在干什么」。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Rest {
    witness: Vec<u8>,
    inputs: Vec<UserInputRow>,
    #[serde(with = "super::recognize::turns::skel::rows")]
    turns: Vec<TurnRow>,
    open: TurnScan,
    #[serde(with = "crate::agents::mainline::skel")]
    chain: Chain,
    runs: Vec<String>,
    activity: Option<crate::agents::SessionActivity>,
}

/// 扫到一半的样子：从它接着读（[`Scan::feed`]），读完收成一张图（[`Scan::done`]）。骨架落的也是它（[`ScanView`] 是借着写的那一形）。
#[derive(Debug, Default, Deserialize)]
struct Scan {
    end: u64,
    starts: Vec<u64>,
    #[serde(deserialize_with = "index_back")]
    index: Vec<IndexRow>,
    #[serde(deserialize_with = "super::recognize::facts::skel::deserialize")]
    facts: SessionFacts,
    rest: Rest,
}

/// 骨架里那一份（借着写）。字段名与 [`Scan`] 相同。
#[derive(Serialize)]
struct ScanView<'a> {
    end: u64,
    starts: &'a [u64],
    index: &'a [IndexRow],
    #[serde(serialize_with = "super::recognize::facts::skel::serialize")]
    facts: &'a SessionFacts,
    rest: &'a Rest,
}

/// 盘上那一份：读到哪了 ＋ 扫到一半的样子。
#[derive(Deserialize)]
struct Skel {
    mark: Mark,
    scan: Scan,
}

#[derive(Serialize)]
struct SkelView<'a> {
    mark: Mark,
    scan: ScanView<'a>,
}

/// 骨架里的一行骨架索引读回来（线上那一形：省略的格缺 ⇒ 零 / 空；`sp` 只认 `Speaker::kind` 那一份闭集里的词）。
#[derive(Deserialize)]
#[serde(default)]
#[derive(Default)]
struct IndexBack {
    o: u64,
    n: u64,
    t: Option<crate::agents::record::RecordClass>,
    u: Option<String>,
    sc: bool,
    sp: Option<String>,
    ch: u32,
    cj: u32,
    pl: u32,
    cb: u32,
    cl: u32,
    fd: u32,
    x: Option<String>,
    ts: Option<String>,
}

fn index_back<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<IndexRow>, D::Error> {
    let rows = Vec::<IndexBack>::deserialize(d)?;
    Ok(rows
        .into_iter()
        .map(|r| IndexRow {
            o: r.o,
            n: r.n,
            t: r.t,
            u: r.u,
            sc: r.sc,
            sp: r.sp.and_then(|s| {
                crate::agents::SPEAKER_KINDS
                    .iter()
                    .position(|k| *k == s)
                    .map(|i| crate::agents::SPEAKER_KINDS[i])
            }),
            ch: r.ch,
            cj: r.cj,
            pl: r.pl,
            cb: r.cb,
            cl: r.cl,
            fd: r.fd,
            x: r.x,
            ts: r.ts,
        })
        .collect())
}

impl Scan {
    /// 从 `self.end` 接着读 `r`（`r` 已定位在那里），读到 `upto` 为止（之后的不读；残尾不计）。
    fn feed<R: std::io::BufRead>(
        &mut self,
        r: R,
        upto: u64,
        kind: &str,
        face: Option<crate::agents::RecordFace>,
    ) -> std::io::Result<()> {
        let chain = face.and_then(|f| f.chain);
        let said = face.and_then(|f| f.activity);
        let run_of = face.and_then(|f| f.run_of);
        let runs = crate::agents::run_faces(kind);
        let mut lines = super::lines::Lines::new(r, self.end).upto(upto);
        while let Some(row) = lines.next()? {
            let (start, buf) = (row.at, row.whole);
            let read = buf.len();
            self.end = start + read as u64;
            slide(&mut self.rest.witness, buf);
            let body = row.body();
            // 一行只解一次：链事实与下面那四样吃同一份（[`super::lines::parse`] 同一口径）。
            let text = String::from_utf8_lossy(body);
            let line = crate::agents::Line::new(&text, start);
            let v = line.value();
            if let Some(f) = chain.and_then(|c| c(&line)) {
                self.rest.chain.load(f);
            }
            if runs.hint(line.text()) {
                self.rest.runs.push(line.text().to_string());
            }
            // 子运行的记录不说主运行此刻在干什么（同实时那一路 `watcher::send_line`）。
            if let Some(a) = said.and_then(|f| f(&line)) {
                if run_of.is_none_or(|f| line.value().and_then(f).is_none()) {
                    self.rest.activity = Some(a);
                }
            }
            // 一拍也只算一次：骨架索引 · 用户输入 · 轮次 · 事实吃同一拍。
            let beat = v.and_then(|v| crate::agents::beat_of(kind, v));
            // 这一行的行号（不算一行的那几行与下一行同号，它们解析不出记录、用不上）。
            let line = self.starts.len() as u64;
            let counted = super::lines::counts(body);
            if counted {
                self.starts.push(start);
                self.index.push(super::recognize::index_row::index_row_of(
                    kind,
                    v,
                    beat.as_ref(),
                    start,
                    read as u64,
                ));
            }
            let (Some(v), Some(beat)) = (v, beat) else {
                continue;
            };
            if let Some(mut row) = super::recognize::user_inputs::user_input_given(kind, v, &beat) {
                row.at = start;
                row.line = line;
                self.rest.inputs.push(row);
            }
            if let Some(row) = self.rest.open.line(start, line, kind, v, &beat) {
                self.rest.turns.push(row);
            }
            if super::recognize::facts::could_matter(kind, body, &self.facts) {
                super::recognize::facts::note_record(&mut self.facts, kind, v, &beat);
            }
        }
        Ok(())
    }

    /// 收成一张图：还开着的那一轮照收尾交出（扫描状态留着、续扫接着用）· 链索引算一次 · 用户输入与轮次照主线外清单滤。
    fn done(mut self, seen: u64) -> ScanMap {
        self.rest.chain.settle();
        let off = self.rest.chain.off().to_vec();
        let mut inputs = self.rest.inputs.clone();
        let mut turns = self.rest.turns.clone();
        if let Some(row) = self.rest.open.clone().finish() {
            turns.push(row);
        }
        if !off.is_empty() {
            let gone: std::collections::HashSet<&str> = off.iter().map(String::as_str).collect();
            inputs.retain(|r| !gone.contains(r.uuid.as_str()));
            turns.retain(|t| !gone.contains(t.uuid.as_str()));
        }
        let mut map = ScanMap {
            end: self.end,
            seen,
            starts: self.starts,
            index: self.index,
            inputs,
            turns,
            off,
            facts: self.facts,
            rest: self.rest,
            bytes: 0,
            stored: 0,
        };
        map.bytes = map.estimate();
        map
    }
}

impl ScanMap {
    /// 实时跟读从这一张起步要的那几样：读到哪（游标 · 当时的长度 · 游标前的见证）· 几个可计行 · 链索引 ·
    /// 说到子运行的那几行原文 · 最后一件说了的「此刻在干什么」。
    pub(crate) fn seen(&self) -> u64 {
        self.seen
    }

    pub(crate) fn witness(&self) -> &[u8] {
        &self.rest.witness
    }

    pub(crate) fn lines(&self) -> u64 {
        self.starts.len() as u64
    }

    pub(crate) fn chain(&self) -> &Chain {
        &self.rest.chain
    }

    pub(crate) fn runs(&self) -> &[String] {
        &self.rest.runs
    }

    pub(crate) fn activity(&self) -> Option<crate::agents::SessionActivity> {
        self.rest.activity
    }

    /// 某一条记录（`uuid`）落在主线的第几轮、那一轮从何时起（分叉框顶上那一句）：数它之前起了几轮（轮次同 `history-turns`）。
    /// 找不到那一条 / 它在第一句之前 ⇒ `None`。
    pub(crate) fn turn_at(&self, uuid: &str) -> Option<(u64, String)> {
        let at = self.index.iter().find(|r| r.u.as_deref() == Some(uuid))?.o;
        let n = self.turns.partition_point(|t| t.at <= at);
        let t = self.turns.get(n.checked_sub(1)?)?;
        Some((n as u64, t.start.clone()))
    }

    /// 接着扫：这一张的样子抄一份，从 `end` 往后读。
    fn resume(&self) -> Scan {
        Scan {
            end: self.end,
            starts: self.starts.clone(),
            index: self.index.clone(),
            facts: self.facts.clone(),
            rest: self.rest.clone(),
        }
    }

    fn view(&self) -> ScanView<'_> {
        ScanView {
            end: self.end,
            starts: &self.starts,
            index: &self.index,
            facts: &self.facts,
            rest: &self.rest,
        }
    }

    /// 尾段那张图：最后 `n` 个可计行从哪开始（与查询那一层现数一遍行的那一份同口径）。
    pub(crate) fn tail(&self, n: usize) -> super::lines::Tail {
        let total = self.starts.len();
        let from = total - n.max(1).min(total);
        super::lines::Tail {
            plan: TailPlan {
                total: total as u64,
                tail_from: from as u64,
                split_at: self.starts.get(from).copied().unwrap_or(self.end),
                end: self.end,
            },
            last: self.starts.last().copied(),
        }
    }

    /// 粗估常驻字节：每行一份定长 ＋ 里面的串长。
    fn estimate(&self) -> usize {
        let opt = |s: &Option<String>| s.as_ref().map_or(0, String::len);
        let index: usize = self
            .index
            .iter()
            .map(|r| 96 + opt(&r.u) + opt(&r.x) + opt(&r.ts))
            .sum();
        let off: usize = self.off.iter().map(|o| 24 + o.len()).sum();
        let inputs: usize = off
            + self
                .inputs
                .iter()
                .map(|r| 72 + r.uuid.len() + r.timestamp.len() + r.excerpt.len())
                .sum::<usize>();
        let turns: usize = self
            .turns
            .iter()
            .map(|t| {
                160 + t.uuid.len()
                    + t.start.len()
                    + t.end.len()
                    + t.said.len()
                    + t.reply.len()
                    + t.shown.iter().map(|c| 24 + c.len()).sum::<usize>()
                    + t.folds
                        .iter()
                        .map(|f| {
                            120 + f.after.len()
                                + f.until.as_ref().map_or(0, String::len)
                                + f.span.text.len()
                                + f.parts.iter().map(|p| 24 + p.text.len()).sum::<usize>()
                        })
                        .sum::<usize>()
                    + t.pending
                        .iter()
                        .map(|p| {
                            64 + p.id.len() + p.name.len() + p.what.as_ref().map_or(0, String::len)
                        })
                        .sum::<usize>()
            })
            .sum();
        // 接着扫要的那几样：没滤的用户输入与轮次（与滤过的同量级）· 链索引每个节点几个短串。
        let chain = self.rest.chain.len() * 160
            + self.rest.runs.iter().map(|r| 24 + r.len()).sum::<usize>();
        self.starts.len() * 8 + index + 2 * (inputs + turns) + chain + 1024
    }
}

/// 一份记录的键：长度 ＋ 修改时刻（路径在槽上）。
type Stamp = (u64, Option<SystemTime>);

struct Slot {
    path: PathBuf,
    stamp: Stamp,
    map: Arc<ScanMap>,
    used: u64,
}

/// 正在扫的那一份：后到的等它。
#[derive(Default)]
struct Flight {
    done: Mutex<Option<Result<Arc<ScanMap>, String>>>,
    cv: Condvar,
}

/// 一张图被挤掉时留下的事实累加器：从 `end` 接着读（读之前核游标前的见证）。
#[derive(Clone)]
struct Tail {
    path: PathBuf,
    end: u64,
    witness: Vec<u8>,
    facts: SessionFacts,
    used: u64,
}

struct Inner {
    slots: Vec<Slot>,
    flights: Vec<(PathBuf, Arc<Flight>)>,
    tails: Vec<Tail>,
    tick: u64,
}

impl Inner {
    /// 按路径留下这一张的事实尾巴（同一份旧的那条换掉；满了挤掉最久没用的）。
    fn leave_tail(&mut self, path: &Path, map: &ScanMap) {
        self.tails.retain(|t| t.path != path);
        self.tails.push(Tail {
            path: path.to_path_buf(),
            end: map.end,
            witness: map.rest.witness.clone(),
            facts: map.facts.clone(),
            used: self.tick,
        });
        if self.tails.len() > TAIL_KEEP {
            if let Some(i) = (0..self.tails.len()).min_by_key(|&i| self.tails[i].used) {
                self.tails.remove(i);
            }
        }
    }
}

/// 扫描图的缓存。
pub(crate) struct RecordScans {
    max_entries: usize,
    max_bytes: usize,
    inner: Mutex<Inner>,
    /// 骨架的家（全进程那一张图用启动时装的那一份；`None` ⇒ 不读不落）。
    home: Option<Skeletons>,
    /// 到此刻为扫描图从记录里读了多少字节（判据数它，不按墙钟）。
    read: AtomicU64,
    /// 旁线程上正在落的骨架（哪一份 · 那条线程）。
    writes: Mutex<Vec<(PathBuf, std::thread::JoinHandle<()>)>>,
}

/// 全进程那一份的上限：条数（同时开着的大会话）与估算字节（单张超了照留、独占，见 [`RecordScans::evict`]）。
const MAX_ENTRIES: usize = 16;
const MAX_BYTES: usize = 64 << 20;

/// 事实尾巴至多留几条：每个开着的 tab（桌面）/ 会话页（手机）占一条；一条最大约 140 KB（改动文件 1000 条封顶），64 条封顶约 9 MB。
pub(crate) const TAIL_KEEP: usize = 64;

/// 全进程那一份。
pub(crate) fn global() -> &'static RecordScans {
    static G: std::sync::OnceLock<RecordScans> = std::sync::OnceLock::new();
    G.get_or_init(|| {
        let scans = RecordScans::new(MAX_ENTRIES, MAX_BYTES);
        match super::skeleton::installed() {
            Some(home) => scans.with_skeletons(home.clone()),
            None => scans,
        }
    })
}

/// 读过一行之后，见证换成游标前那 [`super::skeleton::WITNESS`] 个字节。
fn slide(w: &mut Vec<u8>, line: &[u8]) {
    let probe = super::skeleton::WITNESS as usize;
    if line.len() >= probe {
        w.clear();
        w.extend_from_slice(&line[line.len() - probe..]);
    } else {
        w.extend_from_slice(line);
        let drop = w.len().saturating_sub(probe);
        w.drain(..drop);
    }
}

/// 读游标前那几个字节、跟见证比（`file` 读完停在 `end`）。回：对得上没有 · 读了几个字节。
fn witness_holds(file: &mut RoFile, end: u64, witness: &[u8]) -> std::io::Result<(bool, u64)> {
    use std::io::{Read, Seek, SeekFrom};
    let k = super::skeleton::WITNESS.min(end);
    let mut got = vec![0u8; k as usize];
    file.seek(SeekFrom::Start(end - k))?;
    file.read_exact(&mut got)?;
    Ok((got == witness, k))
}

impl RecordScans {
    pub(crate) fn new(max_entries: usize, max_bytes: usize) -> Self {
        RecordScans {
            max_entries,
            max_bytes,
            inner: Mutex::new(Inner {
                slots: Vec::new(),
                flights: Vec::new(),
                tails: Vec::new(),
                tick: 0,
            }),
            home: None,
            read: AtomicU64::new(0),
            writes: Mutex::new(Vec::new()),
        }
    }

    /// 骨架落在这个家里（大记录扫完落一份；进程里没有那一张时照它起步）。
    pub(crate) fn with_skeletons(mut self, s: Skeletons) -> Self {
        self.home = Some(s);
        self
    }

    /// 到此刻为扫描图从记录里读了多少字节（实时跟读起步那一趟照它记账；判据也数它）。
    pub(crate) fn bytes_read(&self) -> u64 {
        self.read.load(Ordering::Relaxed)
    }

    /// 这么长的记录扫完会不会落骨架（有家 ＋ 够大）。
    pub(crate) fn would_keep(&self, len: u64) -> bool {
        self.skeletons().is_some_and(|h| h.wants(len))
    }

    fn skeletons(&self) -> Option<&Skeletons> {
        self.home.as_ref()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 只看不扫：留着、且 `path` 此刻没变 ⇒ 那一张；否则 `None`。
    pub(crate) fn peek(&self, path: &Path) -> Option<Arc<ScanMap>> {
        let meta = Ro::metadata(path).ok()?;
        let stamp: Stamp = (meta.len(), meta.modified().ok());
        let mut g = self.lock();
        g.tick += 1;
        let tick = g.tick;
        let s = g
            .slots
            .iter_mut()
            .find(|s| s.path == path && s.stamp == stamp)?;
        s.used = tick;
        Some(s.map.clone())
    }

    /// 实时跟读起步用：这一份的图留着、或盘上落过骨架 ⇒ 照它接着扫到此刻的那一张（同 [`Self::get`]：没变不读，变长了只读追加的那一截）；
    /// 都没有 · 正有人在从头扫它 ⇒ `None`（调用方自己轻读一遍行数与链，不在这里等一整遍解析）。
    pub(crate) fn kept(&self, path: &Path) -> Option<Arc<ScanMap>> {
        {
            let g = self.lock();
            if g.flights.iter().any(|(p, _)| p == path) {
                return None;
            }
            let held = g.slots.iter().any(|s| s.path == path);
            if !held && !self.skeletons().is_some_and(|h| h.has(path)) {
                return None;
            }
        }
        self.get(path).ok()
    }

    /// `path` 那一份此刻的扫描图：没变 ⇒ 留着的那张；变长了 ⇒ 从留着的那张（或盘上的骨架）接着扫追加的那一截；
    /// 别的 ⇒ 现扫整份（同一份正在扫 ⇒ 等它）。
    pub(crate) fn get(&self, path: &Path) -> std::io::Result<Arc<ScanMap>> {
        let file = Ro::open(path)?;
        let meta = file.metadata()?;
        let stamp: Stamp = (meta.len(), meta.modified().ok());
        let (flight, base) = {
            let mut g = self.lock();
            g.tick += 1;
            let tick = g.tick;
            if let Some(s) = g
                .slots
                .iter_mut()
                .find(|s| s.path == path && s.stamp == stamp)
            {
                s.used = tick;
                return Ok(s.map.clone());
            }
            if let Some((_, f)) = g.flights.iter().find(|(p, _)| p == path) {
                let f = f.clone();
                drop(g);
                return wait(&f);
            }
            let base = g
                .slots
                .iter()
                .find(|s| s.path == path)
                .map(|s| s.map.clone());
            let f = Arc::new(Flight::default());
            g.flights.push((path.to_path_buf(), f.clone()));
            (f, base)
        };
        let scanned = self.build(path, file, &meta, base).map(|(map, mark)| {
            let map = Arc::new(map);
            if let Some(mark) = mark {
                self.store_later(path, mark, map.clone());
            }
            map
        });
        {
            let mut g = self.lock();
            g.flights.retain(|(p, _)| p != path);
            g.slots.retain(|s| s.path != path);
            if let Ok(map) = &scanned {
                let tick = g.tick;
                g.tails.retain(|t| t.path != path);
                g.slots.push(Slot {
                    path: path.to_path_buf(),
                    stamp,
                    map: map.clone(),
                    used: tick,
                });
                self.evict(&mut g, path);
            }
        }
        let shared = scanned.as_ref().map(Arc::clone).map_err(|e| e.to_string());
        *flight.done.lock().unwrap_or_else(|e| e.into_inner()) = Some(shared);
        flight.cv.notify_all();
        scanned
    }

    /// 造这一张（大记录另回一份要落的「读到哪了」）：起点按先后挑 —— 进程里留着的那张（文件变长了）· 盘上的骨架（没变 ⇒ 不读；变长了 ⇒ 接着读）· 从 0。
    /// 接着读之前先核游标前那几个字节，对不上 ⇒ 从 0。照骨架原样出的（没变）不再落。
    fn build(
        &self,
        path: &Path,
        mut file: RoFile,
        meta: &crate::platform::fs::Metadata,
        base: Option<Arc<ScanMap>>,
    ) -> std::io::Result<(ScanMap, Option<Mark>)> {
        use std::io::{Seek, SeekFrom};
        let len = meta.len();
        let mtime = super::skeleton::mtime_of(meta);
        let home = self.skeletons().filter(|s| s.wants(len));
        let mut stored = base.as_ref().map_or(0, |b| b.stored);
        let mut from = base.filter(|b| len > b.seen).map(|b| b.resume());
        if from.is_none() {
            stored = 0;
            if let Some(k) = home.and_then(|h| h.load::<Skel>(path)) {
                match k.mark.look(len, mtime) {
                    Look::Same => {
                        let mut map = k.scan.done(len);
                        map.stored = map.end;
                        return Ok((map, None));
                    }
                    Look::Grown => {
                        stored = k.mark.cursor;
                        from = Some(k.scan);
                    }
                    Look::Changed => {}
                }
            }
        }
        let mut scan = Scan::default();
        if let Some(s) = from {
            let (holds, k) = witness_holds(&mut file, s.end, &s.rest.witness)?;
            self.read.fetch_add(k, Ordering::Relaxed);
            if holds {
                scan = s;
            } else {
                stored = 0;
            }
        }
        file.seek(SeekFrom::Start(scan.end))?;
        self.read
            .fetch_add(len.saturating_sub(scan.end), Ordering::Relaxed);
        let face = crate::agents::record_face_of(path);
        let kind = crate::agents::record_kind_of(path).unwrap_or_default();
        scan.feed(std::io::BufReader::new(file), len, kind, face)?;
        let mut map = scan.done(len);
        map.stored = stored;
        // 盘上那份照的是 `stored`：没落过 · 接着扫过了门槛 ⇒ 落一份新的（落了就记成照到此刻）；没过门槛 ⇒ 不重写几十 MB。
        let due = home.filter(|h| stored == 0 || map.end >= stored + h.rewrite_after(stored));
        let mark = due.map(|_| Mark {
            len,
            mtime,
            cursor: map.end,
            witness: map.rest.witness.clone(),
        });
        if mark.is_some() {
            map.stored = map.end;
        }
        Ok((map, mark))
    }

    /// 落骨架放到一条旁线程上（几十 MB 的写不挡这一问的应答）。同一份上一次还没落完 ⇒ 这一次不落：缓存，晚一步只是下一次多读一截。
    fn store_later(&self, path: &Path, mark: Mark, map: Arc<ScanMap>) {
        let Some(home) = self.skeletons().cloned() else {
            return;
        };
        let mut w = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        w.retain(|(_, h)| !h.is_finished());
        if w.iter().any(|(p, _)| p == path) {
            return;
        }
        let p = path.to_path_buf();
        let h = std::thread::spawn(move || {
            home.store(
                &p,
                &SkelView {
                    mark,
                    scan: map.view(),
                },
            );
        });
        w.push((path.to_path_buf(), h));
    }

    /// 条数或字节超了 ⇒ 从最久没用的开始扔；刚放进来的 `kept` 那张不扔。它一张就比字节上限大 ⇒ 别的全扔、它独留，说一声。
    fn evict(&self, g: &mut Inner, kept: &Path) {
        loop {
            let bytes: usize = g.slots.iter().map(|s| s.map.bytes).sum();
            if g.slots.len() <= self.max_entries && bytes <= self.max_bytes {
                return;
            }
            let Some(oldest) = g
                .slots
                .iter()
                .enumerate()
                .filter(|(_, s)| s.path != kept)
                .min_by_key(|(_, s)| s.used)
                .map(|(i, _)| i)
            else {
                tracing::warn!(
                    "扫描图 {} MiB 超过缓存上限 {} MiB：照留，别的扫描图全挤掉 —— {}",
                    bytes >> 20,
                    self.max_bytes >> 20,
                    kept.display()
                );
                return;
            };
            let gone = g.slots.remove(oldest);
            g.leave_tail(&gone.path, &gone.map);
        }
    }

    /// `path` 此刻的会话事实（上下文上限按这一次的上限表与中转标记定）：图留着 · 正在扫 ⇒ 照图（[`Self::get`]）；
    /// 图不在、事实尾巴在且见证对得上 ⇒ 只认事实地接着读追加的那一截；都不成 ⇒ 现扫整份（[`Self::get`]）。
    pub(crate) fn facts(
        &self,
        path: &Path,
        limits: &ContextLimits,
        relay: Option<bool>,
    ) -> std::io::Result<SessionFacts> {
        let tail = {
            let g = self.lock();
            let held =
                g.slots.iter().any(|s| s.path == path) || g.flights.iter().any(|(p, _)| p == path);
            (!held)
                .then(|| g.tails.iter().find(|t| t.path == path).cloned())
                .flatten()
        };
        let mut facts = match tail {
            Some(t) => match self.follow(t)? {
                Some(f) => f,
                None => self.get(path)?.facts.clone(),
            },
            None => self.get(path)?.facts.clone(),
        };
        super::recognize::facts::settle_limit(&mut facts, limits, relay);
        Ok(facts)
    }

    /// 从尾巴的游标接着读到此刻的长度（只认事实），读完换下那一条；变短了 · 见证对不上 ⇒ `None`。
    fn follow(&self, mut t: Tail) -> std::io::Result<Option<SessionFacts>> {
        let mut file = Ro::open(&t.path)?;
        let kind = crate::agents::record_kind_of(&t.path).unwrap_or_default();
        let len = file.metadata()?.len();
        if len < t.end {
            return Ok(None);
        }
        let (holds, k) = witness_holds(&mut file, t.end, &t.witness)?;
        self.read.fetch_add(k, Ordering::Relaxed);
        if !holds {
            return Ok(None);
        }
        self.read.fetch_add(len - t.end, Ordering::Relaxed);
        let mut lines = super::lines::Lines::new(std::io::BufReader::new(file), t.end).upto(len);
        while let Some(row) = lines.next()? {
            slide(&mut t.witness, row.whole);
            super::recognize::facts::note_line(&mut t.facts, kind, row.body());
        }
        t.end = lines.end();
        let facts = t.facts.clone();
        let mut g = self.lock();
        if !g.slots.iter().any(|s| s.path == t.path) {
            g.tick += 1;
            t.used = g.tick;
            g.tails.retain(|x| x.path != t.path);
            g.tails.push(t);
        }
        Ok(Some(facts))
    }
}

/// 等正在扫的那一份扫完（阻塞等它的结果，不是定时醒）。
fn wait(f: &Flight) -> std::io::Result<Arc<ScanMap>> {
    let mut done = f.done.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        match done.as_ref() {
            Some(Ok(m)) => return Ok(m.clone()),
            Some(Err(e)) => return Err(std::io::Error::other(e.clone())),
            None => done = f.cv.wait(done).unwrap_or_else(|e| e.into_inner()),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/record_scan_tests.rs"]
mod tests;
