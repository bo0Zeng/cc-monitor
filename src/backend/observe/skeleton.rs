//! **大记录的骨架落盘**：一份记录扫过一遍得出的东西存在数据目录的 `cache/skeleton/` 底下，
//! 后端重启 · 重连 · 冷开时照它接着读，不再整份重来。
//!
//! - 一份记录一个文件，写者只有冷开扫描图（[`crate::observe::record_scan`]：行起点 · 骨架索引 · 用户输入 · 轮次 ·
//!   还没收尾那一轮的扫描状态 · 事实 · 链索引 · 子运行那几行 · 最后一件说了的活动）；实时跟读（[`crate::observe::watcher`]）
//!   起步时照它接着读，不另落一份。
//! - 键 ＝ 路径 ＋ 长度 ＋ 修改时刻 ＋ 游标前 [`WITNESS`] 字节的见证（[`Mark`]）：
//!   长度与修改时刻都没变 ⇒ 不开文件（读 0 字节）；变长 ⇒ 读见证、对得上就从游标接着读（读 追加的 ＋ 见证）；其余 ⇒ 整份重扫。
//! - 格式带版本号（[`VERSION`]），认不出 · 解析不了 · 路径对不上 ⇒ 当没有，整份重扫（不迁移）。
//! - 只给大记录落（[`MIN_LEN`] 起）；目录里份数超了 [`MAX_FILES`] ⇒ 删最久没写的那些。它是从记录推出来的缓存：
//!   删了能从记录重建，足迹里归「能重建」一类（`relay_route_core::SKELETON_DIR_REL`）。
//! - 写只经 [`Own`]：建目录 · 拿目录锁 · 原子写（流式）· 删文件。
//! - **按块读写**：写的时候边序列化边往盘上流（缓冲一块 [`READ_BLOCK`]），每过一块在两项之间插一个换行（JSON 允许）⇒ 文件是一行行
//!   不长于约一块的；读的时候经 [`Blocks`] 一块一块读、边读边解析 ⇒ 手里不再有整份原文，峰值 ≈ 一块 ＋ 解出来的那份结构。
//! - **键是规范化之后的路径**（[`crate::common::fence::real_path`]）：预读那一路拿的是走出来的路径（可能经符号链接），冷开扫描图拿的是
//!   围栏放行之后的路径 —— 同一份记录两种写法落到同一个键上，实时跟读起步时按这个键找得到冷开扫描图落的那一份。

use crate::common::append_read::{Blocks, READ_BLOCK};
use crate::platform::fs::{Own, OwnPath, Ro};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;

/// 格式版本：骨架里任何一样的形状变了就加一（旧的那几份读出来版本对不上 ⇒ 整份重扫、覆盖）。
pub(crate) const VERSION: u32 = 4;

/// 见证多长：游标前这么多字节（续读核「游标之前没被改写」的那一段，watcher 的续读指纹也是这个数）。
pub const WITNESS: u64 = 64;

/// 多大的记录才落骨架（小记录整份读一遍比读骨架还快）。
pub(crate) const MIN_LEN: u64 = 1 << 20;

/// 目录里至多留几份（超了删最久没写的）。
pub(crate) const MAX_FILES: usize = 256;

/// 读一份骨架的上限。
const MAX_READ: u64 = 1 << 30;

/// 一份骨架记的「读到哪了」：那一刻的长度 · 修改时刻 · 游标（最后一个整行之后）· 游标前那几个字节。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Mark {
    pub(crate) len: u64,
    /// 修改时刻（自纪元起的纳秒）；问不到 ⇒ 缺（下一次就不算「没变」）。
    pub(crate) mtime: Option<u128>,
    pub(crate) cursor: u64,
    pub(crate) witness: Vec<u8>,
}

/// 此刻那份文件跟骨架比。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// 长度与修改时刻都没变：照骨架，不读。
    Same,
    /// 变长了：读见证，对得上就从游标接着读。
    Grown,
    /// 变短 · 同长改写 · 问不到修改时刻：整份重扫。
    Changed,
}

/// 修改时刻（纳秒）。
pub(crate) fn mtime_of(m: &crate::platform::fs::Metadata) -> Option<u128> {
    m.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_nanos())
}

impl Mark {
    pub(crate) fn look(&self, len: u64, mtime: Option<u128>) -> Look {
        if len == self.len && mtime.is_some() && mtime == self.mtime {
            Look::Same
        } else if len > self.len && self.cursor <= self.len {
            Look::Grown
        } else {
            Look::Changed
        }
    }
}

/// 盘上一份：版本 ＋ 路径（摘要撞了也认得出）＋ 那一样。
#[derive(Serialize, Deserialize)]
struct File<T> {
    v: u32,
    path: String,
    body: T,
}

/// 骨架的家（数据目录底下 `cache/skeleton/`）＋ 多大才落。克隆便宜。
#[derive(Clone, Debug)]
pub(crate) struct Skeletons {
    own: Own,
    min_len: u64,
}

/// 整台那一份（启动入口装：[`install`]）。冷开扫描图（全进程一份）经它找家。
static INSTALLED: OnceLock<Skeletons> = OnceLock::new();

/// 启动时装一次（拿启动时造的写句柄）。
pub fn install(own: &Own) {
    if INSTALLED.set(Skeletons::new(own.clone())).is_err() {
        tracing::debug!("skeleton home already installed; keeping the first one");
    }
}

pub(crate) fn installed() -> Option<&'static Skeletons> {
    INSTALLED.get()
}

/// 目录相对数据目录的那一截（契约常量相对家目录、以 `.cc-monitor/` 打头）。
fn dir_rel() -> &'static str {
    relay_route_core::SKELETON_DIR_REL
        .strip_prefix(".cc-monitor/")
        .unwrap_or(relay_route_core::SKELETON_DIR_REL)
}

/// 路径的摘要（两遍 FNV-1a 64，不同起点）：文件名用，跨版本稳定。
fn digest(path: &Path) -> String {
    let b = path.as_os_str().as_encoded_bytes();
    format!(
        "{:016x}{:016x}",
        cmd_wire::fnv::fnv1a64(b),
        cmd_wire::fnv::fnv1a64_from(0x6c62_272e_07bb_0142, b)
    )
}

impl Skeletons {
    pub(crate) fn new(own: Own) -> Self {
        Self::at(own, MIN_LEN)
    }

    /// 判据用：门槛另给（夹具的记录小）。
    pub(crate) fn at(own: Own, min_len: u64) -> Self {
        Skeletons { own, min_len }
    }

    /// 上一次落盘时读到 `stored`，之后读进来多少才再落一份（实时回写的门槛：落一份是整份重写，按长度的四分之一摊开 ⇒ 写的总量是读的常数倍）。
    pub(crate) fn rewrite_after(&self, stored: u64) -> u64 {
        self.min_len.max(stored / 4)
    }

    /// 这么长的记录落不落。
    pub(crate) fn wants(&self, len: u64) -> bool {
        len >= self.min_len
    }

    fn dir(&self) -> Option<OwnPath> {
        self.own.path(dir_rel()).ok()
    }

    fn file(&self, key: &Path) -> Option<OwnPath> {
        self.dir()?.join(&format!("{}.json", digest(key))).ok()
    }

    /// `path` 那一份落过没有（只看在不在，不读）。
    pub(crate) fn has(&self, path: &Path) -> bool {
        self.file(&key_of(path))
            .is_some_and(|f| Ro::metadata(f.as_path()).is_ok_and(|m| m.is_file()))
    }

    /// 读 `path` 那一份。没有 · 读不出 · 版本或路径对不上 ⇒ `None`。
    pub(crate) fn load<T: DeserializeOwned>(&self, path: &Path) -> Option<T> {
        let key = key_of(path);
        let f = self.file(&key)?;
        // 不在 ⇒ 没落过（不出声）；别的读不出（超了上限 · 不是文件 · 读错）· 解析不了 ⇒ 说一声是哪一份，当没有、照记录整份重扫。
        let meta = Ro::metadata(f.as_path()).ok()?;
        if !meta.is_file() || meta.len() > MAX_READ {
            tracing::warn!(
                "skeleton for {} skipped: not a file or over the cap",
                path.display()
            );
            return None;
        }
        let blocks = Blocks::open(f.as_path())
            .map_err(|e| tracing::warn!("skeleton for {} skipped: {e}", path.display()))
            .ok()?;
        let got: File<T> = serde_json::from_reader(std::io::BufReader::new(BlockRead::new(blocks)))
            .map_err(|e| tracing::debug!("skeleton for {} unreadable: {e}", path.display()))
            .ok()?;
        (got.v == VERSION && Path::new(&got.path) == key).then_some(got.body)
    }

    /// 落 `path` 那一份（整份原子写）。写不成只记一笔调试日志：缓存，下一次照记录重扫。
    pub(crate) fn store<T: Serialize>(&self, path: &Path, body: &T) {
        if let Err(e) = self.try_store(path, body) {
            tracing::debug!("skeleton for {} not stored: {e}", path.display());
        }
    }

    fn try_store<T: Serialize>(&self, path: &Path, body: &T) -> Result<(), String> {
        let dir = self.dir().ok_or("no data dir")?;
        let mut at = self.own.root().ok_or("no data dir")?;
        for seg in dir_rel().split('/') {
            at = at.join(seg).map_err(|e| e.to_string())?;
            self.own.ensure_dir(&at).map_err(|e| e.to_string())?;
        }
        let key = key_of(path);
        let file = self.file(&key).ok_or("bad name")?;
        let held = self.own.lock(&dir).map_err(|e| format!("{e:?}"))?;
        self.own
            .write_streamed(&file, &held, |f| {
                write_lined(
                    f,
                    &File {
                        v: VERSION,
                        path: key.to_string_lossy().into_owned(),
                        body,
                    },
                )
            })
            .map_err(|e| e.to_string())?;
        self.prune(&dir);
        Ok(())
    }

    /// 拿着目录锁时收拾：写到一半就没了的旁名（进程在原子写当中被杀留下的，`.tmp` 结尾 —— 锁在手里 ⇒ 没有别人正在写）一律删；
    /// 份数超了 ⇒ 删最久没写的那些（只删本目录里的骨架文件）。
    fn prune(&self, dir: &OwnPath) {
        let Ok(rd) = Ro::read_dir(dir.as_path()) else {
            return;
        };
        let mut v: Vec<(std::time::SystemTime, String)> = Vec::new();
        for e in rd.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".tmp") {
                if let Ok(p) = dir.join(&name) {
                    if let Err(e) = self.own.delete_file(&p) {
                        tracing::debug!("stale skeleton temp {name} not removed: {e}");
                    }
                }
                continue;
            }
            if let Some(t) = e.metadata().ok().and_then(|m| m.modified().ok()) {
                if name.ends_with(".json") {
                    v.push((t, name));
                }
            }
        }
        if v.len() <= MAX_FILES {
            return;
        }
        v.sort();
        let extra = v.len() - MAX_FILES;
        for (_, name) in v.into_iter().take(extra) {
            if let Ok(p) = dir.join(&name) {
                if let Err(e) = self.own.delete_file(&p) {
                    tracing::debug!("skeleton {name} not pruned: {e}");
                }
            }
        }
    }
}

/// 一份骨架的键：规范化之后的路径（解不开 ⇒ 原样）。
fn key_of(path: &Path) -> std::path::PathBuf {
    crate::common::fence::real_path(path).unwrap_or_else(|| path.to_path_buf())
}

/// 边序列化边写：缓冲一块，写出去的字节每过一块就在下一个数组元素 / 对象键之前插一个换行 ⇒ 文件一行行不长于约一块（[`Blocks`] 读得动）。
fn write_lined<T: Serialize>(f: &mut dyn std::io::Write, v: &T) -> std::io::Result<()> {
    use std::io::Write as _;
    let since = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let mut w = Counting {
        inner: std::io::BufWriter::with_capacity(READ_BLOCK, f),
        since: since.clone(),
    };
    let mut ser = serde_json::Serializer::with_formatter(&mut w, Lined { since });
    v.serialize(&mut ser).map_err(std::io::Error::other)?;
    w.inner.flush()
}

/// 数着自上一个换行起写了多少字节的写者（[`Lined`] 看它决定插不插换行）。
struct Counting<W> {
    inner: W,
    since: std::rc::Rc<std::cell::Cell<usize>>,
}

impl<W: std::io::Write> std::io::Write for Counting<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        let at = buf[..n].iter().rposition(|&b| b == b'\n');
        self.since.set(match at {
            Some(i) => n - i - 1,
            None => self.since.get() + n,
        });
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// 紧凑写法，只在两项之间、自上一个换行起过了一块时插一个换行。
struct Lined {
    since: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Lined {
    fn gap<W: ?Sized + std::io::Write>(&self, w: &mut W, first: bool) -> std::io::Result<()> {
        if !first {
            w.write_all(b",")?;
        }
        if self.since.get() >= READ_BLOCK {
            w.write_all(b"\n")?;
        }
        Ok(())
    }
}

impl serde_json::ser::Formatter for Lined {
    fn begin_array_value<W: ?Sized + std::io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        self.gap(w, first)
    }

    fn begin_object_key<W: ?Sized + std::io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        self.gap(w, first)
    }
}

/// [`Blocks`] 当一个 `Read` 用：一块一块往外给，读到尾再给没有换行的那一截。
struct BlockRead {
    blocks: Blocks<crate::platform::fs::RoFile>,
    cur: Vec<u8>,
    pos: usize,
    done: bool,
}

impl BlockRead {
    fn new(blocks: Blocks<crate::platform::fs::RoFile>) -> Self {
        BlockRead {
            blocks,
            cur: Vec::new(),
            pos: 0,
            done: false,
        }
    }
}

impl std::io::Read for BlockRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        while self.pos >= self.cur.len() {
            if self.done {
                return Ok(0);
            }
            self.cur = match self.blocks.next()? {
                Some(b) => b,
                None => {
                    self.done = true;
                    self.blocks.rest()
                }
            };
            self.pos = 0;
        }
        let n = buf.len().min(self.cur.len() - self.pos);
        buf[..n].copy_from_slice(&self.cur[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/skeleton_tests.rs"]
pub(crate) mod tests;
