//! **大记录的骨架落盘**（11 §5.1 ③ ＋ §5.2 c）：一份记录扫过一遍得出的东西存在数据目录的 `cache/skeleton/` 底下，
//! 后端重启 · 重连 · 冷开时照它接着读，不再整份重来。
//!
//! - 两样各一份文件（同一个键、两个写者，不互相读改）：预读那一路（[`crate::observe::watcher`]：游标 · 行数 · 链索引 · 子运行那几行 ·
//!   最后一件说了的活动）· 冷开扫描图（[`crate::observe::record_scan`]：行起点 · 骨架索引 · 用户输入 · 轮次 ·
//!   还没收尾那一轮的扫描状态 · 事实 · 链索引）。
//! - 键 ＝ 路径 ＋ 长度 ＋ 修改时刻 ＋ 游标前 [`WITNESS`] 字节的见证（[`Mark`]）：
//!   长度与修改时刻都没变 ⇒ 不开文件（读 0 字节）；变长 ⇒ 读见证、对得上就从游标接着读（读 追加的 ＋ 见证）；其余 ⇒ 整份重扫。
//! - 格式带版本号（[`VERSION`]），认不出 · 解析不了 · 路径对不上 ⇒ 当没有，整份重扫（不迁移）。
//! - 只给大记录落（[`MIN_LEN`] 起）；目录里份数超了 [`MAX_FILES`] ⇒ 删最久没写的那些。它是从记录推出来的缓存：
//!   删了能从记录重建，足迹里归「能重建」一类（`relay_route_core::SKELETON_DIR_REL`）。
//! - 写只经 [`Own`]：建目录 · 拿目录锁 · 原子写 · 删文件。

use crate::platform::fs::{Own, OwnPath, Ro};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;

/// 格式版本：骨架里任何一样的形状变了就加一（旧的那几份读出来版本对不上 ⇒ 整份重扫、覆盖）。
pub(crate) const VERSION: u32 = 1;

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

/// 整台那一份（启动入口装：[`install`]）。冷开扫描图（全进程一份）经它找家；预读那一路由 watcher 自己带着。
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
    let fnv = |seed: u64| {
        b.iter().fold(seed, |h, &c| {
            (h ^ u64::from(c)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    };
    format!(
        "{:016x}{:016x}",
        fnv(0xcbf2_9ce4_8422_2325),
        fnv(0x6c62_272e_07bb_0142)
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

    /// 这么长的记录落不落。
    pub(crate) fn wants(&self, len: u64) -> bool {
        len >= self.min_len
    }

    fn dir(&self) -> Option<OwnPath> {
        self.own.path(dir_rel()).ok()
    }

    fn file(&self, path: &Path, what: &str) -> Option<OwnPath> {
        self.dir()?
            .join(&format!("{}.{what}.json", digest(path)))
            .ok()
    }

    /// 读 `path` 那一样（`what`：`prime` · `scan`）。没有 · 读不出 · 版本或路径对不上 ⇒ `None`。
    pub(crate) fn load<T: DeserializeOwned>(&self, path: &Path, what: &str) -> Option<T> {
        let f = self.file(path, what)?;
        // 不在 ⇒ 没落过（不出声）；别的读不出（超了上限 · 读错）· 解析不了 ⇒ 说一声是哪一份，当没有、照记录整份重扫。
        Ro::metadata(f.as_path()).ok()?;
        let raw = Ro::read_regular_capped(f.as_path(), MAX_READ)
            .map_err(|e| tracing::warn!("skeleton {what} for {} skipped: {e}", path.display()))
            .ok()?;
        let got: File<T> = serde_json::from_slice(&raw)
            .map_err(|e| tracing::debug!("skeleton {what} for {} unreadable: {e}", path.display()))
            .ok()?;
        (got.v == VERSION && Path::new(&got.path) == path).then_some(got.body)
    }

    /// 落 `path` 那一样（整份原子写）。写不成只记一笔调试日志：缓存，下一次照记录重扫。
    pub(crate) fn store<T: Serialize>(&self, path: &Path, what: &str, body: &T) {
        if let Err(e) = self.try_store(path, what, body) {
            tracing::debug!("skeleton {what} for {} not stored: {e}", path.display());
        }
    }

    fn try_store<T: Serialize>(&self, path: &Path, what: &str, body: &T) -> Result<(), String> {
        let dir = self.dir().ok_or("no data dir")?;
        let mut at = self.own.root().ok_or("no data dir")?;
        for seg in dir_rel().split('/') {
            at = at.join(seg).map_err(|e| e.to_string())?;
            self.own.ensure_dir(&at).map_err(|e| e.to_string())?;
        }
        let file = self.file(path, what).ok_or("bad name")?;
        let bytes = serde_json::to_vec(&File {
            v: VERSION,
            path: path.to_string_lossy().into_owned(),
            body,
        })
        .map_err(|e| e.to_string())?;
        let held = self.own.lock(&dir).map_err(|e| format!("{e:?}"))?;
        self.own
            .write(&file, &held, &bytes)
            .map_err(|e| e.to_string())?;
        self.prune(&dir);
        Ok(())
    }

    /// 份数超了 ⇒ 删最久没写的那些（只删本目录里的骨架文件）。
    fn prune(&self, dir: &OwnPath) {
        let Ok(rd) = Ro::read_dir(dir.as_path()) else {
            return;
        };
        let mut v: Vec<(std::time::SystemTime, String)> = rd
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let t = e.metadata().ok()?.modified().ok()?;
                name.ends_with(".json").then_some((t, name))
            })
            .collect();
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

#[cfg(test)]
#[path = "../../../tests/backend/observe/skeleton_tests.rs"]
pub(crate) mod tests;
