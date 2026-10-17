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
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
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
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
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

/// 存盘的一块：发的那一方定哪一形，收的那一方两形都认（同 [`WirePath`]）。
/// 编辑器存文本按字切块、按 JSON 转义之后的长度装满一行 ⇒ 发字符串；上传按字节切 ⇒ 恒发 `{"b16": …}`
/// （任意字节是 UTF-8 也不发字符串：控制字符转义成 `\u0000` 一个字节要占六个，一块装不装得进一行就算不准了）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        type = "string | { b16: string }",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum ChunkBytes {
    Text(String),
    Hex(Vec<u8>),
}

impl ChunkBytes {
    pub fn bytes(&self) -> &[u8] {
        match self {
            ChunkBytes::Text(t) => t.as_bytes(),
            ChunkBytes::Hex(b) => b,
        }
    }
}

impl Serialize for ChunkBytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            ChunkBytes::Text(t) => s.serialize_str(t),
            ChunkBytes::Hex(b) => HexBytes(b.clone()).serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for ChunkBytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = WirePath::deserialize(d)?;
        Ok(match String::from_utf8(w.0) {
            Ok(t) => ChunkBytes::Text(t),
            Err(e) => ChunkBytes::Hex(e.into_bytes()),
        })
    }
}

/// 一个条目是什么。闭集四个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    /// 按哪一列排（缺 ⇒ 名称）。目录（含指向目录的链接）恒在前；截断在排好之后。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub sort: Option<LsSort>,
}

/// `files-ls` 按哪一列排：`by` 那一列的第一下（名称 A 在前 · 大小大的在前 · 时间新的在前），`reverse` 只反那一列；
/// 相持按名称小写、再按原字节。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct LsSort {
    pub by: LsSortBy,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<bool>"))]
    pub reverse: bool,
}

/// [`LsSort::by`] 的四列。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum LsSortBy {
    #[default]
    Name,
    Size,
    Mtime,
    /// 「类型」那一列：[`FileClass`]（链接自成一种），再扩展名。
    Type,
}

impl LsSort {
    /// 这一屏那一列是不是从大到小（第一下：大小 · 时间大的在前，名称 · 类型 A 在前；`reverse` 反过来）。
    pub fn descending(self) -> bool {
        matches!(self.by, LsSortBy::Size | LsSortBy::Mtime) != self.reverse
    }
}

/// 一项按名字看是哪一种：图标 · 「类型」那一列 · 按类型排都看它（那台判）。
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "lowercase")]
pub enum FileClass {
    Folder,
    Text,
    Code,
    Image,
    Archive,
    Pdf,
    Link,
    #[default]
    Other,
}

/// 一项读得起来的那几道门（只在文件上有；那台按名字与大小判好，两端照画）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Gates {
    pub preview: PreviewGate,
    /// 查看页：只在两端解得开的那几种图上有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub view: Option<Gate>,
    pub edit: Gate,
}

/// 预览：当文本读（读多少以内 · 按哪种语法上色）· 当图读（读多少以内）· 不预览（那一句）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum PreviewGate {
    Text(TextPreview),
    Image(#[cfg_attr(any(test, feature = "ts"), ts(type = "number"))] u64),
    No(String),
}

/// [`PreviewGate::Text`]：读多少以内 · 认得的代码 / 标记文本按哪种语法上色（扩展名；认不得 ⇒ 缺）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TextPreview {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub cap: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub lang: Option<String>,
}

/// 一道门：许读多少以内 · 不许（那一句）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    Cap(#[cfg_attr(any(test, feature = "ts"), ts(type = "number"))] u64),
    No(String),
}

/// 面包屑的一段：这一段叫什么 · 点它去哪（[`LsReply::crumbs`]）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Crumb {
    pub name: WirePath,
    pub path: WirePath,
}

/// `files-ls` 的一行。读不到元数据的那几格缺席（不是 0）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct LsEntry {
    pub path: WirePath,
    /// 末段名（那台按自己的分隔符切好）。
    pub name: WirePath,
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
    /// 画哪种图标（链接画它指向的那一种：指向目录 ⇒ 文件夹，其余按名字）。
    pub class: FileClass,
    /// 「类型」那一列的字（那台写好）。
    pub type_text: String,
    /// 读得起来的那几道门；目录（含指向目录的链接）缺。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub gates: Option<Gates>,
}

/// `files-ls` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct LsReply {
    /// 排好的（[`LsReq::sort`]）。
    pub entries: Vec<LsEntry>,
    /// 上一级；到顶 ⇒ `null`。
    pub up: Option<WirePath>,
    /// 根到这一级的每一段，根在最前（那台按自己的路径规则切：Windows 的盘符那一段是 `C:\`）。
    pub crumbs: Vec<Crumb>,
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct StatReply {
    pub path: WirePath,
    /// 上一级（那台按自己的路径规则切）；到顶 ⇒ `null`。
    pub up: Option<WirePath>,
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesFindHit {
    pub path: WirePath,
    pub kind: FileKind,
    /// 画哪种图标（索引不跟链接 ⇒ 链接画 [`FileClass::Link`]）。
    pub class: FileClass,
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
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
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct HomeReq {}

/// `files-index-status`：不收入参（同 [`HomeReq`]）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct IndexStatusReq {}

// ── 写的那几条（写面 · 上传提交 · 解压）─────────────

/// 可缺的一格：不给 ⇒ `None`；给了就必须是那一型（`null` 也拒 —— 写面这几格从来不收 `null`）。
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// 必给、可为 `null` 的一格（`Door::put` 的 `expect`：`null` ＝「我读的时候它不在」）。缺席 ⇒ 拒。
fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

/// 64 位小写十六进制的内容摘要（读的那一趟交出去的那个，调用方当不透明令牌原样交回）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, type = "string", export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Sha256Hex(pub String);

/// 摘要的长度（十六进制位数）。
const SHA256_HEX_LEN: usize = 64;

impl<'de> Deserialize<'de> for Sha256Hex {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() != SHA256_HEX_LEN
            || !s
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(D::Error::custom(format!(
                "`sha256` must be {SHA256_HEX_LEN} lowercase hex digits"
            )));
        }
        Ok(Sha256Hex(s))
    }
}

/// 摘要形的 CAS：恰好 `{"sha256": "<64 位小写十六进制>"}`（多一个键、少一个键都拒）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct Sha256Expect {
    pub sha256: Sha256Hex,
}

/// `files-delete` 的 `expect`：`{"empty_dir": true}` ⇒ 只删一个空目录；字符串 / `{"b16"}` ⇒ 盘上逐字节等于它才删。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(
        export,
        type = "{ empty_dir: true } | string | { b16: string }",
        export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")
    )
)]
pub enum DeleteExpect {
    EmptyDir,
    Bytes(WirePath),
}

impl Serialize for DeleteExpect {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            DeleteExpect::EmptyDir => {
                let mut m = s.serialize_map(Some(1))?;
                m.serialize_entry("empty_dir", &true)?;
                m.end()
            }
            DeleteExpect::Bytes(b) => b.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for DeleteExpect {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = DeleteExpect;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(
                    f,
                    "{{\"empty_dir\": true}}, a string or {{\"b16\": \"<hex>\"}}"
                )
            }
            fn visit_str<E: DeError>(self, s: &str) -> Result<DeleteExpect, E> {
                Ok(DeleteExpect::Bytes(WirePath(s.as_bytes().to_vec())))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<DeleteExpect, A::Error> {
                let got = match m.next_key::<String>()?.as_deref() {
                    Some("empty_dir") if m.next_value::<bool>()? => DeleteExpect::EmptyDir,
                    Some(HEX_KEY) => {
                        let h: String = m.next_value()?;
                        DeleteExpect::Bytes(WirePath(
                            from_hex(&h)
                                .ok_or_else(|| A::Error::custom("`b16` is not lowercase hex"))?,
                        ))
                    }
                    _ => return Err(A::Error::custom(
                        "`expect` must be {\"empty_dir\": true}, a string or {\"b16\": \"<hex>\"}",
                    )),
                };
                if let Some(k) = m.next_key::<String>()? {
                    return Err(A::Error::custom(format!("unexpected key `{k}`")));
                }
                Ok(got)
            }
        }
        d.deserialize_any(V)
    }
}

/// `files-create`：在目标根底下新建一份此前不存在的文件（不给 `content` ⇒ 空文件）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesCreateReq {
    pub root: WirePath,
    pub rel: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub content: Option<WirePath>,
    /// `true` ⇒ `rel` 只许是一段名字（界面就地新建 / 改名敲的那一格）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub single: Option<bool>,
}

/// 落点 ＋ 字节数（新建 · 上传提交）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesPathBytesReply {
    pub path: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
}

/// `files-mkdir`：新建一个目录（只建最后那一段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesMkdirReq {
    pub root: WirePath,
    pub rel: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub single: Option<bool>,
}

/// `files-rename`：改名 / 同根内移动（两段各过一遍路径解析）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesRenameReq {
    pub root: WirePath,
    pub from: WirePath,
    pub to: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub single: Option<bool>,
}

/// `files-delete`：删一组（同一个根底下）；单个就是一组里的一项。回包 [`crate::batch::BatchReply`]。
/// `ticket` 给了 ⇒ 这台按票记这一组（连接断了照常做完，带同一张票再问回同一份结局）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesDeleteReq {
    pub root: WirePath,
    pub items: Vec<DeleteItem>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ticket: Option<String>,
}

/// 删的一项：一个文件或一个空目录；`recursive` 显式才删整棵树；`expect` 给了 ⇒ CAS（不等 ⇒ 那一项 `stale`，一个字节不动）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct DeleteItem {
    pub rel: WirePath,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub recursive: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub expect: Option<DeleteExpect>,
}

/// `files-chmod`：一组改成同一个 unix 权限位。回包 [`crate::batch::BatchReply`]（每项带 `before`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesChmodReq {
    pub root: WirePath,
    pub items: Vec<ChmodItem>,
    /// 八进制写法原样（`644` · `0755`，框里敲的就交这几个字）；由那台解（空 · 不是八进制 · 超出 7777 ⇒ 整组 `refused`、那一句那台写）。
    pub mode: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ticket: Option<String>,
}

/// 改权限的一项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ChmodItem {
    pub rel: WirePath,
}

/// `files-copy`：同根内复制一组；目录整棵复制（不合并）。同名照 `on_exists`（缺 ⇒ `ask`），`decide` 逐项答。回包 [`crate::batch::BatchReply`]。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesCopyReq {
    pub root: WirePath,
    pub items: Vec<CopyItem>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub decide: Option<std::collections::BTreeMap<String, crate::batch::Decide>>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub ticket: Option<String>,
    /// `true` ⇒ 每项的 `to` 只许是一段名字、且不同于 `from`（界面「复制为」那一格敲的就是一个名字）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub single: Option<bool>,
}

/// 复制的一项：`from` 复制成 `to`（都是根底下的相对段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct CopyItem {
    pub from: WirePath,
    pub to: WirePath,
}

/// 落了一棵（复制 · 解压）：落点 ＋ 几份文件 · 几个目录 · 几条链接 · 字节之和。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesTreeReply {
    pub path: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub files: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub dirs: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub links: u64,
}

/// `files-extract`：解压到一个新目录（`fresh` ⇒ 撞名往后编号）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesExtractReq {
    pub root: WirePath,
    pub rel: WirePath,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub fresh: Option<bool>,
}

/// 建一条符号链接（只走后端内部的 `Door::link`，不上线）（`target` 原样写进去）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesLinkReq {
    pub root: WirePath,
    pub rel: WirePath,
    pub target: WirePath,
}

/// `files-write-text`：覆盖写一份已经在的普通文件（摘要形 CAS 必给）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesWriteTextReq {
    pub root: WirePath,
    pub rel: WirePath,
    pub content: WirePath,
    pub expect: Sha256Expect,
}

/// 覆盖写成了（`files-write-text` · `files-commit-text`）：新摘要是下一次存的 `expect`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesWrittenReply {
    pub path: WirePath,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    pub sha256: Sha256Hex,
}

/// 读改写的读那一半（只走后端内部的 `Door::peek`，不上线）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesPeekReq {
    pub root: WirePath,
    pub rel: WirePath,
}

/// `Door::peek` 的回包：`exists: false` ⇒ 确定不在（`text` 为 `null`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesPeekReply {
    pub path: WirePath,
    pub exists: bool,
    pub text: Option<String>,
}

/// 整份替换一份文本文件（只走后端内部的 `Door::put`，不上线）（`expect` 必给：`null` ＝ 读的时候它不在）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesPutReq {
    pub root: WirePath,
    pub rel: WirePath,
    pub content: WirePath,
    #[serde(deserialize_with = "nullable")]
    pub expect: Option<WirePath>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub backup: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub parents: Option<bool>,
}

/// `Door::put` 的回包。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilesPutReply {
    pub path: WirePath,
    pub bytes: u64,
    pub changed: bool,
    pub created: bool,
    pub backup: Option<WirePath>,
}

/// `files-delete-session`：删一份历史会话 —— 只收 sid。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesDeleteSessionReq {
    pub sid: String,
}

/// `files-stage-chunk`：存盘的一块进暂存区（至少 1 字节）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesStageChunkReq {
    pub key: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seq: u64,
    pub content: ChunkBytes,
}

/// `files-stage-chunk` 的回包：这一块写进去几个字节。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesStageChunkReply {
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
}

/// `files-commit-text`：按块读回、拼起来、覆盖写（语义同 `files-write-text`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesCommitTextReq {
    pub root: WirePath,
    pub rel: WirePath,
    pub key: String,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub chunks: u64,
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub bytes: u64,
    pub expect: Sha256Expect,
}

/// `files-commit-upload`：把暂存区里一份传完的上传件挪进目标（`chunks` · `bytes` 给了 ⇒ 先按块拼）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct FilesCommitUploadReq {
    pub root: WirePath,
    pub rel: WirePath,
    pub key: String,
    /// 落点已有同名：同成组复制那一条（缺 ⇒ `ask`：码 `exists`、`data` 是同名清单，一个字节不动、块留着，改个答原样再交）。
    /// 只有一件 ⇒ `skip` 不收（不放就别提交）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub on_exists: Option<crate::batch::OnExists>,
    pub expect: Sha256Expect,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub chunks: Option<u64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub bytes: Option<u64>,
    /// `true` ⇒ `rel` 只许是一段名字（界面上传一份文件：名字是本机那份的名字，或别的 App 交来的显示名）。不论给没给，每一段都过这台的名字规则。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub single: Option<bool>,
}

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
    FilesCreateReq => "files-create", FilesPathBytesReply;
    FilesMkdirReq => "files-mkdir", FilesPathReply;
    FilesRenameReq => "files-rename", FilesPathReply;
    FilesDeleteReq => "files-delete", crate::batch::BatchReply;
    FilesChmodReq => "files-chmod", crate::batch::BatchReply;
    FilesCopyReq => "files-copy", crate::batch::BatchReply;
    FilesExtractReq => "files-extract", FilesTreeReply;
    FilesWriteTextReq => "files-write-text", FilesWrittenReply;
    FilesDeleteSessionReq => "files-delete-session", FilesPathReply;
    FilesStageChunkReq => "files-stage-chunk", FilesStageChunkReply;
    FilesCommitTextReq => "files-commit-text", FilesWrittenReply;
    FilesCommitUploadReq => "files-commit-upload", FilesPathBytesReply;
}

#[cfg(test)]
#[path = "../../../../tests/common/cmd-wire/files_tests.rs"]
mod tests;
