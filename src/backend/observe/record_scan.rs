//! 一份会话记录一张扫描图：冷开一条会话时界面问的那几样（尾段 · 骨架索引 · 用户输入 · 轮次 · 事实）一遍扫出来，
//! 按（路径 · 长度 · 修改时刻）留着，那几条帧命令（`history-tail` · `-index` · `-user-inputs` · `-turns` · `-facts`）从字节 0 问时共用。
//!
//! - 一遍：每个完整行只解析一次，同一个值喂给五样各自的那一步（各自的口径仍只住各自的模块：
//!   [`super::history_query::index_row_of`] · [`super::user_inputs::user_input_of`] · [`super::turns::TurnScan`] ·
//!   [`super::facts_query::note_record`] · [`super::history_query::line_counts`]）。
//! - 留着：长度或修改时刻变了就作废重扫；条数与估算字节各有上限，超了先淘汰最久没用的；单张比字节上限还大就不留。
//! - 同时冷开同一份（界面开一条会话几问是一齐发的）：先到的那一个扫，后到的等它扫完拿同一张 —— 那一张是它们发问之后才扫完的，
//!   每样都带着 `end`，续取从那里接。
//! - 只扫到问的那一刻的长度：之后写进来的不在这一张里（键对得上内容）。

use super::facts_query::{ContextLimits, SessionFacts};
use super::history_query::{IndexRow, TailPlan};
use super::turns::{TurnRow, TurnScan};
use super::user_inputs::UserInputRow;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::SystemTime;

/// 一行记录的解析（去 BOM 与两头空白）。解析不了 ⇒ `None`。各扫描都经它，口径只有这一份。
pub(crate) fn parse_record(line: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(line);
    serde_json::from_str(text.trim_start_matches('\u{feff}').trim()).ok()
}

/// 一遍扫出来的那几样（都是从字节 0 起）。
#[derive(Debug, Default)]
pub(crate) struct ScanMap {
    /// 最后一个完整行之后的字节位置（残尾不算）。
    pub(crate) end: u64,
    /// 每个可计行的字节起点（尾段按 `n` 现切）。
    starts: Vec<u64>,
    pub(crate) index: Vec<IndexRow>,
    pub(crate) inputs: Vec<UserInputRow>,
    pub(crate) turns: Vec<TurnRow>,
    /// 主线外清单（回退掉的那几条，文件序；`history-branch`）。用户输入与轮次已经不含它们。
    pub(crate) off: Vec<String>,
    /// 定上下文上限之前的事实（上限按每次问的上限表现定）。
    facts: SessionFacts,
    /// 估算的常驻字节（字节上限按它算）。
    bytes: usize,
}

impl ScanMap {
    /// 从字节 0 读 `r`，至多读 `len` 字节；`chain` 是这份记录那一家的链事实面（注册表 `RecordFace.chain`）：给了就顺手算主线外清单。
    pub(crate) fn scan_with<R: std::io::BufRead>(
        r: R,
        len: u64,
        chain: Option<fn(&str) -> Option<crate::agents::mainline::ChainFact>>,
    ) -> std::io::Result<ScanMap> {
        use std::io::BufRead as _;
        let mut facts = Vec::new();
        let mut r = r.take(len);
        let mut map = ScanMap::default();
        let mut turns = TurnScan::default();
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            let read = r.read_until(b'\n', &mut buf)?;
            if read == 0 || buf.last() != Some(&b'\n') {
                break; // EOF / 残尾不计
            }
            let start = map.end;
            map.end += read as u64;
            map.facts.end = map.end;
            let body = &buf[..buf.len() - 1];
            if let Some(f) = chain.and_then(|c| c(&String::from_utf8_lossy(body))) {
                facts.push(f);
            }
            let counted = super::history_query::line_counts(body);
            let v = parse_record(body);
            if counted {
                map.starts.push(start);
                map.index.push(super::history_query::index_row_of(
                    v.as_ref(),
                    start,
                    read as u64,
                ));
            }
            let Some(v) = v else {
                continue;
            };
            if let Some(row) = super::user_inputs::user_input_of(&v) {
                map.inputs.push(row);
            }
            if let Some(row) = turns.line(start, &v) {
                map.turns.push(row);
            }
            if super::facts_query::could_matter(body, &map.facts) {
                super::facts_query::note_record(&mut map.facts, &v);
            }
        }
        if let Some(row) = turns.finish() {
            map.turns.push(row);
        }
        map.off = crate::agents::mainline::off_of(facts);
        if !map.off.is_empty() {
            let off: std::collections::HashSet<&str> = map.off.iter().map(String::as_str).collect();
            map.inputs.retain(|r| !off.contains(r.uuid.as_str()));
            map.turns.retain(|t| !off.contains(t.uuid.as_str()));
        }
        map.bytes = map.estimate();
        Ok(map)
    }

    /// 尾段那张图：最后 `n` 个可计行从哪开始（同 [`super::history_query::tail_plan_of`]）。
    pub(crate) fn tail(&self, n: usize) -> TailPlan {
        let total = self.starts.len();
        let from = total - n.max(1).min(total);
        TailPlan {
            total: total as u64,
            tail_from: from as u64,
            split_at: self.starts.get(from).copied().unwrap_or(self.end),
            end: self.end,
        }
    }

    /// 事实：按这一次的上限表与中转标记定上下文上限。
    pub(crate) fn facts(&self, limits: &ContextLimits, relay: Option<bool>) -> SessionFacts {
        let mut f = self.facts.clone();
        super::facts_query::settle_limit(&mut f, limits, relay);
        f
    }

    /// 还没结果的工具调用（文件序；种类判「需手动」用）。
    pub(crate) fn pending(&self) -> &[super::facts_query::PendingCall] {
        &self.facts.pending
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
                    + t.ending.iter().map(|c| 24 + c.len()).sum::<usize>()
                    + t.parts.iter().map(|p| 24 + p.text.len()).sum::<usize>()
                    + t.span.text.len()
                    + t.pending
                        .iter()
                        .map(|p| {
                            64 + p.id.len() + p.name.len() + p.what.as_ref().map_or(0, String::len)
                        })
                        .sum::<usize>()
            })
            .sum();
        self.starts.len() * 8 + index + inputs + turns + 1024
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

struct Inner {
    slots: Vec<Slot>,
    flights: Vec<(PathBuf, Arc<Flight>)>,
    tick: u64,
}

/// 扫描图的缓存。
pub(crate) struct RecordScans {
    max_entries: usize,
    max_bytes: usize,
    inner: Mutex<Inner>,
}

/// 全进程那一份的上限：条数（同时开着的大会话）与估算字节。
const MAX_ENTRIES: usize = 16;
const MAX_BYTES: usize = 64 << 20;

/// 全进程那一份。
pub(crate) fn global() -> &'static RecordScans {
    static G: RecordScans = RecordScans::new(MAX_ENTRIES, MAX_BYTES);
    &G
}

impl RecordScans {
    pub(crate) const fn new(max_entries: usize, max_bytes: usize) -> Self {
        RecordScans {
            max_entries,
            max_bytes,
            inner: Mutex::new(Inner {
                slots: Vec::new(),
                flights: Vec::new(),
                tick: 0,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 只看不扫：留着、且 `path` 此刻没变 ⇒ 那一张；否则 `None`。
    pub(crate) fn peek(&self, path: &Path) -> Option<Arc<ScanMap>> {
        let meta = std::fs::metadata(path).ok()?;
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

    /// `path` 那一份此刻的扫描图：没变 ⇒ 留着的那张；变了 / 没有 ⇒ 现扫（同一份正在扫 ⇒ 等它）。
    pub(crate) fn get(&self, path: &Path) -> std::io::Result<Arc<ScanMap>> {
        let file = std::fs::File::open(path)?;
        let meta = file.metadata()?;
        let stamp: Stamp = (meta.len(), meta.modified().ok());
        let flight = {
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
            let f = Arc::new(Flight::default());
            g.flights.push((path.to_path_buf(), f.clone()));
            f
        };
        let chain = crate::agents::record_face_of(path).and_then(|f| f.chain);
        let scanned =
            ScanMap::scan_with(std::io::BufReader::new(file), stamp.0, chain).map(Arc::new);
        {
            let mut g = self.lock();
            g.flights.retain(|(p, _)| p != path);
            g.slots.retain(|s| s.path != path);
            if let Ok(map) = &scanned {
                if map.bytes <= self.max_bytes {
                    let tick = g.tick;
                    g.slots.push(Slot {
                        path: path.to_path_buf(),
                        stamp,
                        map: map.clone(),
                        used: tick,
                    });
                    self.evict(&mut g);
                }
            }
        }
        let shared = scanned.as_ref().map(Arc::clone).map_err(|e| e.to_string());
        *flight.done.lock().unwrap_or_else(|e| e.into_inner()) = Some(shared);
        flight.cv.notify_all();
        scanned
    }

    /// 条数或字节超了 ⇒ 从最久没用的开始扔。
    fn evict(&self, g: &mut Inner) {
        loop {
            let bytes: usize = g.slots.iter().map(|s| s.map.bytes).sum();
            if g.slots.len() <= self.max_entries && bytes <= self.max_bytes {
                return;
            }
            let Some(oldest) = g
                .slots
                .iter()
                .enumerate()
                .min_by_key(|(_, s)| s.used)
                .map(|(i, _)| i)
            else {
                return;
            };
            g.slots.remove(oldest);
        }
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
