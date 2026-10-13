//! 文件管理那一族（`files-*`）的线上类型：后端照它解入参、写回包，文件窗口照它发入参、解回包，两边是同一份。
//!
//! 路径一律是原始字节（[`WirePath`]）：非 UTF-8 的文件名在哪一跳被有损解码，回程就找不到那个名字了。

use serde::de::{Deserializer, Error as DeError, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

/// `{"b16": …}` 那一形的键。
pub const HEX_KEY: &str = "b16";

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let nib = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        }
    };
    s.as_bytes()
        .chunks(2)
        .map(|p| Some((nib(p[0])? << 4) | nib(p[1])?))
        .collect()
}

/// 一个路径（或一段名字、一行字）的**原始字节**。线上：是 UTF-8 ⇒ 字符串；不是 ⇒ `{"b16": "<十六进制小写>"}`。两形都双向无损。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        type = "string | { b16: string }",
        export_to = "../../../frontend/ui/generated/"
    )
)]
pub struct WirePath(pub Vec<u8>);

impl WirePath {
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<&[u8]> for WirePath {
    fn from(b: &[u8]) -> Self {
        WirePath(b.to_vec())
    }
}

impl From<&str> for WirePath {
    fn from(s: &str) -> Self {
        WirePath(s.as_bytes().to_vec())
    }
}

impl Serialize for WirePath {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(t) => s.serialize_str(t),
            Err(_) => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry(HEX_KEY, &to_hex(&self.0))?;
                m.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for WirePath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = WirePath;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a string or {{\"b16\": \"<hex>\"}}")
            }
            fn visit_str<E: DeError>(self, s: &str) -> Result<WirePath, E> {
                Ok(WirePath(s.as_bytes().to_vec()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<WirePath, A::Error> {
                let mut got: Option<Vec<u8>> = None;
                while let Some(k) = m.next_key::<String>()? {
                    if k != HEX_KEY || got.is_some() {
                        return Err(A::Error::custom(format!("unexpected key `{k}`")));
                    }
                    let h: String = m.next_value()?;
                    got = Some(
                        from_hex(&h)
                            .ok_or_else(|| A::Error::custom("`b16` is not lowercase hex"))?,
                    );
                }
                got.map(WirePath)
                    .ok_or_else(|| A::Error::custom("missing `b16`"))
            }
        }
        d.deserialize_any(V)
    }
}

/// 恒以 `{"b16": …}` 出的那一形（分块读回的那一块：内容不是名字，是 UTF-8 也不出字符串）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        type = "{ b16: string }",
        export_to = "../../../frontend/ui/generated/"
    )
)]
pub struct HexBytes(pub Vec<u8>);

impl Serialize for HexBytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(1))?;
        m.serialize_entry(HEX_KEY, &to_hex(&self.0))?;
        m.end()
    }
}

impl<'de> Deserialize<'de> for HexBytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct B16 {
            b16: String,
        }
        let b = B16::deserialize(d)?;
        from_hex(&b.b16)
            .map(HexBytes)
            .ok_or_else(|| D::Error::custom("`b16` is not lowercase hex"))
    }
}

/// 一个条目是什么。闭集四个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Dir,
    File,
    Symlink,
    Other,
}

/// 链接指向的那一头：目录 · 文件 · 跟不过去（断链）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(rename_all = "lowercase")]
pub enum LinkTo {
    Dir,
    File,
    Missing,
}

// ── files-ls ─────────────────────────────────────────

/// `files-ls`：列一个目录的直接子项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct LsReq {
    pub path: WirePath,
    /// 至多回几条（缺 · 0 ⇒ 后端默认 1000）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub limit: Option<u64>,
    /// `true` ⇒ 只列文件夹（含指向目录的链接）；截断与计数都只数文件夹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub dirs_only: Option<bool>,
}

/// `files-ls` 的一行。读不到元数据的那几格缺席（不是 0）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct LsEntry {
    pub path: WirePath,
    pub kind: FileKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub mtime_secs: Option<u64>,
    /// 那台按本地钟写好的短写法。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub mtime_text: Option<String>,
    /// 那台按本地钟写好的完整写法。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub mtime_full: Option<String>,
    /// 链接指向的是不是目录（只在跟得过去的链接上）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub link_dir: Option<bool>,
    /// 链接指向哪一种（只在链接上）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub link_to: Option<LinkTo>,
}

/// `files-ls` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct LsReply {
    pub entries: Vec<LsEntry>,
    /// 回送的比读到的少（被 `limit` 截了）。
    pub truncated: bool,
    /// 一共读到几项（含没回送的）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub total: u64,
    /// 有几项读不出来（照数，不静默少一项）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub unreadable: u64,
}

// ── files-stat ───────────────────────────────────────

/// `files-stat`：一个路径的元数据（跟链接）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct StatReq {
    pub path: WirePath,
}

/// `files-size`：算一个目录（或文件）有多大。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct SizeReq {
    pub path: WirePath,
}

/// `files-stat` 的回包（跟链接）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct StatReply {
    pub path: WirePath,
    pub kind: FileKind,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub size: u64,
    pub readonly: bool,
    /// unix 权限位低 12 位；非 unix 缺席。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub mode: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub mtime_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub mtime_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub mtime_full: Option<String>,
    /// 属主名（查不到名字给 uid 数字串）；非 unix ⇒ `null`。
    pub owner: Option<String>,
    /// 路径本身是链接 ⇒ 目标原文；否 ⇒ `null`。
    pub link_target: Option<WirePath>,
}

// ── files-home ───────────────────────────────────────

/// 只回一个路径的回包（home）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct FilesPathReply {
    pub path: WirePath,
}

// ── files-size ───────────────────────────────────────

/// `files-size` 的回包（不跟链接、不进别的文件系统）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct SizeReply {
    pub path: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub files: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub dirs: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub links: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub other: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub skipped_mounts: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub unreadable_dirs: u64,
}

// ── files-read-text ──────────────────────────────────

/// `files-read-text`：读一份文本（超上限整趟拒、不截断）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct ReadTextReq {
    pub path: WirePath,
    /// 这一趟许读的上限（每次都要给，1..=8 MiB）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub max_bytes: u64,
}

/// `files-read-text` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct ReadTextReply {
    pub path: WirePath,
    pub text: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    /// 读到的那一份字节的 SHA-256（小写十六进制）：存回时交回去当 `expect`。
    pub sha256: String,
}

// ── files-read-chunk ─────────────────────────────────

/// `files-read-chunk`：从 `offset` 起读至多 `len` 字节（1..=256 KiB）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct ReadChunkReq {
    pub path: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub offset: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub len: u64,
}

/// `files-read-chunk` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct ReadChunkReply {
    pub path: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub offset: u64,
    /// 整份文件多大。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub size: u64,
    pub eof: bool,
    pub content: HexBytes,
}

// ── files-grep ───────────────────────────────────────

/// `files-grep`：在一个目录（或一份文件）底下按内容找一串字节。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct GrepReq {
    pub path: WirePath,
    /// 要找的那一串（1..=256 字节）。
    pub needle: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ignore_ascii_case: Option<bool>,
    /// 至多几条命中（缺 · 0 ⇒ 后端默认；超上限按上限）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub limit: Option<u64>,
}

/// 命中那一行前后的一行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct GrepLine {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub line: u64,
    pub text: WirePath,
    /// 命中那一段的字节区间（至多一段）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<[number, number]>"))]
    pub marks: Vec<[u64; 2]>,
}

/// 一份文件里的第一处命中。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct GrepHit {
    pub path: WirePath,
    /// 相对搜索起点的那一段。
    pub rel: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub line: u64,
    pub text: WirePath,
    /// 这份文件里一共命中几处。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub matches: u64,
    pub lines: Vec<GrepLine>,
}

/// 搜到一半为什么停了：命中够数 · 读的字节到上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(rename_all = "lowercase")]
pub enum GrepStopped {
    Hits,
    Bytes,
}

/// `files-grep` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct GrepReply {
    pub path: WirePath,
    pub hits: Vec<GrepHit>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub files: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub links: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub skipped_binary: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub skipped_large: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub skipped_mounts: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub unreadable: u64,
    /// 这一趟用的命中上限。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub limit: u64,
    pub truncated: bool,
    /// 没搜完就停了的原因；搜完了（或被撤）⇒ `null`。
    pub stopped: Option<GrepStopped>,
}

// ── files-find ───────────────────────────────────────

/// 结果按什么排。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(rename_all = "lowercase")]
pub enum FindSort {
    Relevance,
    Name,
    Location,
    Mtime,
    Size,
}

/// 搜哪儿：`under` 那个目录底下（缺 ⇒ 家目录）· 整台机器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(rename_all = "lowercase")]
pub enum FindScope {
    Under,
    Machine,
}

/// `files-find`：在常驻索引里查。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct FindReq {
    /// 窗口原样发来的搜索词。
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub under: Option<WirePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub scope: Option<FindScope>,
    /// 同一个搜索框的第几趟（新号到了，旧号收手）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub seq: Option<u64>,
    /// 哪一个搜索框。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub stream: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sort: Option<FindSort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub desc: Option<bool>,
}

/// 一条命中。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct FilesFindHit {
    pub path: WirePath,
    pub kind: FileKind,
    /// 相对搜索起点的那一段（所在目录）。
    pub location: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub size: Option<u64>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub mtime_secs: Option<u64>,
    pub mtime_text: Option<String>,
    /// 命中段的字节区间。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "Array<[number, number]>"))]
    pub marks: Vec<[u64; 2]>,
}

/// `files-find` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct FindReply {
    pub hits: Vec<FilesFindHit>,
    /// 这一趟从哪儿搜起（缺 `under` ⇒ 家目录；都说不出 ⇒ `null`）。
    pub start: Option<WirePath>,
    pub sort: FindSort,
    pub desc: bool,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub total_hits: u64,
    pub truncated: bool,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub scanned: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub index_age_secs: u64,
    pub index_missing: bool,
    pub stale: bool,
    pub index_root: Option<WirePath>,
    /// 搜的那一处在索引覆盖之外。
    pub out_of_index: bool,
    pub cover_root: Option<WirePath>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number | null"))]
    pub seq: Option<u64>,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub offset: u64,
}

// ── files-index-status · files-index-rebuild ─────────

/// `files-index-status` 的回包：索引的新鲜度 · 条目数 · 常驻字节 · 后端声明的重走周期。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct IndexStatusReply {
    pub index_missing: bool,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub entries: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub resident_bytes: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub unreadable_dirs: u64,
    /// 读不进去的那几个目录（前 20 个）。
    pub unreadable_paths: Vec<WirePath>,
    pub truncated: bool,
    /// 没走进去的挂载点个数。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub skipped_mounts: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub age_secs: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub rewalk_interval_secs: u64,
    pub stale: bool,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub browse_watches: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub browse_watch_cap: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub cold_first_build_secs: u64,
}

/// `files-index-rebuild`：走一遍、做完返回。缺 `path` ⇒ 家目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct RebuildReq {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub path: Option<WirePath>,
}

/// `files-index-rebuild` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct RebuildReply {
    pub path: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub entries: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub resident_bytes: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub unreadable_dirs: u64,
    pub unreadable_paths: Vec<WirePath>,
    pub truncated: bool,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub skipped_mounts: u64,
}

// ── files-browse ─────────────────────────────────────

/// `files-browse`：用户此刻在看哪几个目录（整份名单，不是增量；空 ⇒ 都卸掉）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct BrowseReq {
    pub dirs: Vec<WirePath>,
}

/// `files-browse` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct BrowseReply {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub added: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub removed: u64,
    /// 超过名单上限没收的。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub rejected: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub browse_watch_cap: u64,
    /// 监听器真挂上了几个。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub watching: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub watch_failed: u64,
    /// 没挂上的那一句原话。
    pub watch_error: Option<String>,
}

// ── 不收入参的那两条 ─────────────────────────────────

/// `files-home`：不收入参（缺 · `null` · `{}` 都认，多一个键拒）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct HomeReq {}

/// `files-index-status`：不收入参（同 [`HomeReq`]）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = "../../../frontend/ui/generated/")
)]
#[serde(deny_unknown_fields)]
pub struct IndexStatusReq {}

// ── 命令名 · 入参 · 回包连成一张表 ───────────────────

/// 一条命令的入参类型就选中了那条命令：名字与回包类型都挂在它上面（后端登记那一行照它取名，文件窗口照它发、照它解）。
pub trait Call: Serialize {
    /// 线上命令名。
    const NAME: &'static str;
    type Reply: Serialize + serde::de::DeserializeOwned;
}

macro_rules! calls {
    ($($req:ty => $name:literal, $reply:ty;)*) => {
        $(impl Call for $req {
            const NAME: &'static str = $name;
            type Reply = $reply;
        })*
    };
}

calls! {
    LsReq => "files-ls", LsReply;
    StatReq => "files-stat", StatReply;
    HomeReq => "files-home", FilesPathReply;
    SizeReq => "files-size", SizeReply;
    ReadTextReq => "files-read-text", ReadTextReply;
    ReadChunkReq => "files-read-chunk", ReadChunkReply;
    GrepReq => "files-grep", GrepReply;
    FindReq => "files-find", FindReply;
    IndexStatusReq => "files-index-status", IndexStatusReply;
    RebuildReq => "files-index-rebuild", RebuildReply;
    BrowseReq => "files-browse", BrowseReply;
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/files_tests.rs"]
mod tests;
