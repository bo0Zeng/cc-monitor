//! **格目录**（`cells-catalog`）：核心出的每件成品有哪些格、各是值还是写好的字。
//!
//! 出口据它写自己的声明（要哪几格），不读核心代码就知道「有没有那一格」（`src/doc/ARCHITECTURE.md` §「核心与适配层」）。
//!
//! # 格从哪来：从成品的 Rust 类型序列化出来，不手写第二份
//!
//! 每件成品登记一组**样本**（[`Product::specimens`]）：用成品自己的 Rust 类型造的值，每个可缺的格都填上、每个列表都不空、
//! 每种变体各一个。目录 ＝ 把样本经 serde 交给本文件的走查序列化器（[`NodeSer`]）走一遍得到的格路径 ——
//! 与线上 `serde_json` 写出来的是同一套 `Serialize` 实现，所以目录里不会有线上没有的格。
//! 反方向（线上有、目录漏了）由判据钉：样本里出现 `None` / 空列表 ⇒ 红；各份跨语言金样（真代码写出来的成品）里见到的每一格都得在目录里。
//!
//! # 一格是什么
//!
//! - `kind`：`value`（机器要的值）· `text`（核心写好的字，类型是 [`Words`](crate::common::cells::Words)）· `tone`（语气，[`Tone`](crate::common::cells::Tone)）。
//!   由这一格的 Rust 类型说，不按格名猜（`blocks[].text` 是原文、`cost.text` 是写好的字）。
//! - `type`：`string` · `number` · `bool` · `enum`（闭集的一个词：Rust 枚举的单元变体）· `object`（原样透传的一团，不再往里分格）。
//! - 路径写法（声明里点格用同一种写法）：`a.b` 嵌套 · `a[]` 列表的每一项 · `a.*` 以 id 为键的表的每一项 ·
//!   `a[type=x]` 按判别格挑的那一种（只在成品登记了判别格时出现，见 [`Product::tags`]）。
//!
//! # 还缺的格
//!
//! [`PENDING`] 登记已经判了要补、还没落地的格（手机端 10-09 缺格清单里的那几条）：落地那一刻它们进了目录，
//! 判据就要求把这里那一行删掉（目录与缺格表不许同时有同一格）。

use crate::common::cells::{TONE_TYPE, WORDS_TYPE};
use serde::ser::{self, Serialize};
use std::collections::BTreeMap;

/// 走查序列化器产出的一棵树：与 `serde_json::Value` 同构，多带「这一格的 Rust 类型是什么标记」。
#[derive(Debug, Clone)]
pub(crate) enum Node {
    Leaf {
        ty: &'static str,
        mark: Mark,
        s: Option<String>,
    },
    Seq(Vec<Node>),
    /// `data` ＝ 以 id 为键的表（`BTreeMap` · 透传的 JSON 对象）；否则是结构体 / 拍平的结构体。
    Map {
        data: bool,
        entries: Vec<(String, Node)>,
    },
    /// 可缺的格在样本里是缺的（`None`）。
    Absent,
}

/// 一个叶子格的类型标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Plain,
    Words,
    Tone,
    Code,
}

/// 走查失败（只在样本写坏时出现：键不是字符串）。
#[derive(Debug)]
pub(crate) struct Fault(String);

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Fault {}
impl ser::Error for Fault {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Fault(msg.to_string())
    }
}

/// 一份样本：走查出来的树 ＋ 同一个值经 `serde_json` 写出来的样子（判据拿两者对：走查器没走歪）。
pub(crate) type Specimen = Result<(Node, serde_json::Value), Fault>;

/// 把一个值走成 [`Node`]，并照线上那样写一遍。
pub(crate) fn node_of<T: Serialize + ?Sized>(v: &T) -> Specimen {
    let wire = serde_json::to_value(v).map_err(|e| Fault(e.to_string()))?;
    Ok((v.serialize(NodeSer)?, wire))
}

fn leaf(ty: &'static str) -> Node {
    Node::Leaf {
        ty,
        mark: Mark::Plain,
        s: None,
    }
}

/// 走查序列化器：每种 serde 数据形变成 [`Node`] 的一种。
pub(crate) struct NodeSer;

pub(crate) struct SeqSer(Vec<Node>);
pub(crate) struct MapSer {
    data: bool,
    entries: Vec<(String, Node)>,
    key: Option<String>,
}
/// 外部标注的变体（`{"变体": …}`）。
pub(crate) struct VariantSer<S> {
    variant: &'static str,
    inner: S,
}

impl ser::Serializer for NodeSer {
    type Ok = Node;
    type Error = Fault;
    type SerializeSeq = SeqSer;
    type SerializeTuple = SeqSer;
    type SerializeTupleStruct = SeqSer;
    type SerializeTupleVariant = VariantSer<SeqSer>;
    type SerializeMap = MapSer;
    type SerializeStruct = MapSer;
    type SerializeStructVariant = VariantSer<MapSer>;

    fn serialize_bool(self, _: bool) -> Result<Node, Fault> {
        Ok(leaf("bool"))
    }
    fn serialize_i8(self, _: i8) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_i16(self, _: i16) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_i32(self, _: i32) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_i64(self, _: i64) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_u8(self, _: u8) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_u16(self, _: u16) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_u32(self, _: u32) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_u64(self, _: u64) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_f32(self, _: f32) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_f64(self, _: f64) -> Result<Node, Fault> {
        Ok(leaf("number"))
    }
    fn serialize_char(self, c: char) -> Result<Node, Fault> {
        self.serialize_str(&c.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<Node, Fault> {
        Ok(Node::Leaf {
            ty: "string",
            mark: Mark::Plain,
            s: Some(v.to_string()),
        })
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<Node, Fault> {
        Ok(Node::Seq(vec![leaf("number")]))
    }
    fn serialize_none(self) -> Result<Node, Fault> {
        Ok(Node::Absent)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Node, Fault> {
        v.serialize(NodeSer)
    }
    fn serialize_unit(self) -> Result<Node, Fault> {
        Ok(Node::Absent)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Node, Fault> {
        Ok(Node::Absent)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Node, Fault> {
        Ok(Node::Leaf {
            ty: "enum",
            mark: if name == TONE_TYPE {
                Mark::Tone
            } else {
                Mark::Code
            },
            s: Some(variant.to_string()),
        })
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        v: &T,
    ) -> Result<Node, Fault> {
        let n = v.serialize(NodeSer)?;
        Ok(match n {
            Node::Leaf { ty, s, .. } if name == WORDS_TYPE => Node::Leaf {
                ty,
                mark: Mark::Words,
                s,
            },
            other => other,
        })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Node, Fault> {
        Ok(Node::Map {
            data: false,
            entries: vec![(variant.to_string(), v.serialize(NodeSer)?)],
        })
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<SeqSer, Fault> {
        Ok(SeqSer(Vec::with_capacity(len.unwrap_or(0))))
    }
    fn serialize_tuple(self, len: usize) -> Result<SeqSer, Fault> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<SeqSer, Fault> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<VariantSer<SeqSer>, Fault> {
        Ok(VariantSer {
            variant,
            inner: SeqSer(Vec::with_capacity(len)),
        })
    }
    /// 长度给了 ⇒ 是一张按 id 的表（`BTreeMap` / 透传的 JSON 对象）；没给 ⇒ 拍平的结构体（serde 的 `flatten` 走这里）或自写的形。
    fn serialize_map(self, len: Option<usize>) -> Result<MapSer, Fault> {
        Ok(MapSer {
            data: len.is_some(),
            entries: Vec::new(),
            key: None,
        })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<MapSer, Fault> {
        Ok(MapSer {
            data: false,
            entries: Vec::new(),
            key: None,
        })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<VariantSer<MapSer>, Fault> {
        Ok(VariantSer {
            variant,
            inner: MapSer {
                data: false,
                entries: Vec::new(),
                key: None,
            },
        })
    }
}

impl ser::SerializeSeq for SeqSer {
    type Ok = Node;
    type Error = Fault;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Fault> {
        self.0.push(v.serialize(NodeSer)?);
        Ok(())
    }
    fn end(self) -> Result<Node, Fault> {
        Ok(Node::Seq(self.0))
    }
}
impl ser::SerializeTuple for SeqSer {
    type Ok = Node;
    type Error = Fault;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Fault> {
        ser::SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> Result<Node, Fault> {
        ser::SerializeSeq::end(self)
    }
}
impl ser::SerializeTupleStruct for SeqSer {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Fault> {
        ser::SerializeSeq::serialize_element(self, v)
    }
    fn end(self) -> Result<Node, Fault> {
        ser::SerializeSeq::end(self)
    }
}
impl ser::SerializeTupleVariant for VariantSer<SeqSer> {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Fault> {
        ser::SerializeSeq::serialize_element(&mut self.inner, v)
    }
    fn end(self) -> Result<Node, Fault> {
        Ok(Node::Map {
            data: false,
            entries: vec![(self.variant.to_string(), Node::Seq(self.inner.0))],
        })
    }
}
impl ser::SerializeMap for MapSer {
    type Ok = Node;
    type Error = Fault;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, k: &T) -> Result<(), Fault> {
        match k.serialize(NodeSer)? {
            Node::Leaf { s: Some(s), .. } => {
                self.key = Some(s);
                Ok(())
            }
            _ => Err(Fault("map key is not a string".into())),
        }
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Fault> {
        let k = self
            .key
            .take()
            .ok_or_else(|| Fault("value without key".into()))?;
        self.entries.push((k, v.serialize(NodeSer)?));
        Ok(())
    }
    fn end(self) -> Result<Node, Fault> {
        Ok(Node::Map {
            data: self.data,
            entries: self.entries,
        })
    }
}
impl ser::SerializeStruct for MapSer {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Fault> {
        self.entries.push((key.to_string(), v.serialize(NodeSer)?));
        Ok(())
    }
    /// `skip_serializing_if` 跳过的格：样本里它是缺的（判据要它为空 —— 跳过的格在目录里就漏了）。
    fn skip_field(&mut self, key: &'static str) -> Result<(), Fault> {
        self.entries.push((key.to_string(), Node::Absent));
        Ok(())
    }
    fn end(self) -> Result<Node, Fault> {
        ser::SerializeMap::end(self)
    }
}
impl ser::SerializeStructVariant for VariantSer<MapSer> {
    type Ok = Node;
    type Error = Fault;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), Fault> {
        ser::SerializeStruct::serialize_field(&mut self.inner, key, v)
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), Fault> {
        ser::SerializeStruct::skip_field(&mut self.inner, key)
    }
    fn end(self) -> Result<Node, Fault> {
        Ok(Node::Map {
            data: false,
            entries: vec![(
                self.variant.to_string(),
                ser::SerializeMap::end(self.inner)?,
            )],
        })
    }
}

/// 目录里的一格。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Cell {
    pub(crate) path: String,
    pub(crate) kind: &'static str,
    #[serde(rename = "type")]
    pub(crate) ty: &'static str,
}

fn join(a: &str, b: &str) -> String {
    if a.is_empty() {
        b.to_string()
    } else {
        format!("{a}.{b}")
    }
}

/// 判别格挑出的那一种：列表项 `a[]` ⇒ `a[k=v]`；别处 ⇒ 在后面接 `{k=v}`（根上就是 `{k=v}`）。
pub(crate) fn picked(path: &str, k: &str, v: &str) -> String {
    match path.strip_suffix("[]") {
        Some(base) => format!("{base}[{k}={v}]"),
        None => format!("{path}{{{k}={v}}}"),
    }
}

/// 一条路径里的每一处挑法：（挑法前那一截 · 是不是列表 · 键 · 值 · 挑法后那一截）。
pub(crate) fn picks(path: &str) -> Vec<(String, bool, String, String, String)> {
    let mut out = Vec::new();
    for (i, open) in path.char_indices() {
        let close = match open {
            '[' => ']',
            '{' => '}',
            _ => continue,
        };
        let Some(len) = path[i + 1..].find(close) else {
            continue;
        };
        let inner = &path[i + 1..i + 1 + len];
        if let Some((k, v)) = inner.split_once('=') {
            out.push((
                path[..i].to_string(),
                open == '[',
                k.to_string(),
                v.to_string(),
                path[i + 2 + len..].to_string(),
            ));
        }
    }
    out
}

/// 去掉一处挑法后的路径（列表的挑法退回 `[]`）。
pub(crate) fn unpicked(prefix: &str, list: bool, rest: &str) -> String {
    if list {
        format!("{prefix}[]{rest}")
    } else if prefix.is_empty() {
        rest.trim_start_matches('.').to_string()
    } else {
        format!("{prefix}{rest}")
    }
}

/// 每一种都有的格提到挑法外面：`{t=said}.agent` … `{t=title}.agent` 五种全有 ⇒ 写一格 `agent`。
/// 只在这一处挑法的每一种（目录里见过的全部值）都有这一格、且格的类别与类型都一样时才提。
fn hoist(mut cells: BTreeMap<String, Cell>) -> BTreeMap<String, Cell> {
    loop {
        type Group = BTreeMap<(String, bool, String, String), Vec<(String, String)>>;
        let mut all: BTreeMap<(String, bool, String), std::collections::BTreeSet<String>> =
            BTreeMap::new();
        let mut groups: Group = BTreeMap::new();
        for path in cells.keys() {
            for (pre, list, k, v, rest) in picks(path) {
                all.entry((pre.clone(), list, k.clone()))
                    .or_default()
                    .insert(v.clone());
                groups
                    .entry((pre, list, k, rest))
                    .or_default()
                    .push((v, path.clone()));
            }
        }
        let hit = groups.into_iter().find(|((pre, list, k, _), members)| {
            let vs: std::collections::BTreeSet<&String> = members.iter().map(|(v, _)| v).collect();
            let want = &all[&(pre.clone(), *list, k.clone())];
            vs.len() == want.len()
                && want.len() > 1
                && members.windows(2).all(|w| {
                    cells[&w[0].1].kind == cells[&w[1].1].kind
                        && cells[&w[0].1].ty == cells[&w[1].1].ty
                })
        });
        let Some(((pre, list, _, rest), members)) = hit else {
            return cells;
        };
        let mut cell = cells[&members[0].1].clone();
        for (_, p) in &members {
            cells.remove(p);
        }
        cell.path = unpicked(&pre, list, &rest);
        cells.insert(cell.path.clone(), cell);
    }
}

/// 一条路径的最后一段（列表的挑法一律写成 `[]`）：判别格按「它在谁底下」认。
pub(crate) fn last_seg(path: &str) -> String {
    let seg = path.rsplit('.').next().unwrap_or("");
    match seg.find('[') {
        Some(i) => format!("{}[]", &seg[..i]),
        None => seg.to_string(),
    }
}

/// 这一格是不是判别格：成品登记了（它所在的那一层 · 键）。
pub(crate) fn is_tag(tags: &[(&str, &str)], at: &str, key: &str) -> bool {
    let seg = last_seg(at);
    tags.iter().any(|(p, k)| *p == seg && *k == key)
}

/// 一棵 [`Node`] 走成格。样本写得不全（`None` · 空列表）⇒ 记进 `holes`（判据要它为空）。
pub(crate) fn walk(
    n: &Node,
    path: &str,
    tags: &[(&str, &str)],
    out: &mut BTreeMap<String, Cell>,
    holes: &mut Vec<String>,
) {
    match n {
        Node::Absent => holes.push(format!("{path}: absent")),
        Node::Leaf { ty, mark, .. } => {
            let kind = match mark {
                Mark::Words => "text",
                Mark::Tone => "tone",
                Mark::Plain | Mark::Code => "value",
            };
            put(out, holes, path, kind, ty);
        }
        Node::Seq(items) => {
            if items.is_empty() {
                holes.push(format!("{path}: empty list"));
            }
            let at = format!("{path}[]");
            for it in items {
                walk(it, &at, tags, out, holes);
            }
        }
        Node::Map {
            data: true,
            entries,
        } => {
            if entries.is_empty() {
                put(out, holes, path, "value", "object");
            }
            let at = join(path, "*");
            for (_, v) in entries {
                walk(v, &at, tags, out, holes);
            }
        }
        Node::Map {
            data: false,
            entries,
        } => {
            let disc = entries.iter().find_map(|(k, v)| match v {
                Node::Leaf { s: Some(s), .. } if is_tag(tags, path, k) => {
                    Some((k.as_str(), s.as_str()))
                }
                _ => None,
            });
            let base = disc.map_or_else(|| path.to_string(), |(k, v)| picked(path, k, v));
            for (k, v) in entries {
                if disc.is_some_and(|(dk, _)| dk == k) {
                    put(out, holes, &join(path, k), "value", "enum");
                } else {
                    walk(v, &join(&base, k), tags, out, holes);
                }
            }
        }
    }
}

fn put(
    out: &mut BTreeMap<String, Cell>,
    holes: &mut Vec<String>,
    path: &str,
    kind: &'static str,
    ty: &'static str,
) {
    let cell = Cell {
        path: path.to_string(),
        kind,
        ty,
    };
    match out.get(path) {
        Some(had) if *had != cell => holes.push(format!("{path}: {had:?} vs {cell:?}")),
        Some(_) => {}
        None => {
            out.insert(path.to_string(), cell);
        }
    }
}

/// 一件成品。
pub(crate) struct Product {
    /// 成品名（声明里 `cells` 的键）。帧是帧的 `kind`。
    pub(crate) name: &'static str,
    /// 判别格：（它所在那一层的最后一段 · 键）—— 这件成品里按哪几格分种类。只认这里登记的：
    /// 同叫 `kind`，`who.speaker.kind` 是判别格、`runs.*.kind` 是普通的值。根上那一层的段是空串。
    pub(crate) tags: &'static [(&'static str, &'static str)],
    /// 样本：每个可缺的格都填上、每个列表都不空、每种变体各一个。
    pub(crate) specimens: fn() -> Vec<Specimen>,
    /// **冻结**：两个前端（桌面界面 · 手机）都照它读的成品面 —— 格只许加，删 / 改名 / 换类型 ⇒ 判据红
    /// （对的是落盘的格目录金样：冻结成品的格在金样里有、目录里没了或换了样，重写金样也不放行）。
    pub(crate) frozen: bool,
}

/// 一件成品的格（按路径排）＋ 样本里的洞。
pub(crate) fn cells_of(p: &Product) -> (Vec<Cell>, Vec<String>) {
    let mut out = BTreeMap::new();
    let mut holes = Vec::new();
    for s in (p.specimens)() {
        match s {
            Ok((n, _)) => walk(&n, "", p.tags, &mut out, &mut holes),
            Err(e) => holes.push(format!("specimen: {e}")),
        }
    }
    (hoist(out).into_values().collect(), holes)
}

/// 已经判了要补、还没落地的一格。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub(crate) struct Pending {
    pub(crate) product: &'static str,
    pub(crate) path: &'static str,
    pub(crate) kind: &'static str,
}

/// 还缺的格（手机端 10-09 缺格清单）。落地那一刻删掉这里那一行（判据两向钉）。今天一条都不缺；机制留着，下一张缺格清单进这里。
///
/// 已落地 / 不补的（这里不再登记）：
/// - G1 会话灯「后台任务运行中」那一档 ⇒ `activity` 的闭集多了 `backgroundWork`（不是新格），字与语气在 `activity_text` · `activity_tone`。
/// - G2 会话状态的字与语气 ⇒ `session_*.activity_text` · `activity_tone` · `session_state.state_text` …
/// - G3 「在等什么」的字与先答哪个 ⇒ `facts.needs.text` · `facts.needs.rank`。那台 pidfile 的 `waitingFor` 六个原值由适配层翻成
///   [`WaitOn`](crate::agents::WaitOn)，再与记录里没结果的那一步一起在 `facts_query::needs_of` 判成八种 `needs.kind`（映射只在这两处）；
///   字与序跟着种类走，不另挂在 `session_status` 上。
/// - G4 上下文的字 ⇒ `facts.usage.contextText`（判得出上限写百分比、判不出只写用了多少）· `limitText` · `promptTokensText` ·
///   `percent` · `contextTone` · `limitFromText`；上限只有核心一套判定（`facts_query::context_limit`），出口不再按型号猜。
/// - G5 骨架行的时刻：**不补**（10-09 定走法甲）。时刻在骨架上唯一的用处是算「最新的那一支」，手机改吃 `history-branch.off`
///   拿主线之后这一用处没了；要画时刻的地方读正文时记录本身带 `at` · `timeText`。
pub(crate) const PENDING: &[Pending] = &[];

/// 全部成品（目录的次序）。
pub(crate) const PRODUCTS: &[Product] = &[
    Product {
        name: "record",
        tags: &[
            ("", "t"),
            ("blocks[]", "type"),
            ("content[]", "type"),
            ("speaker", "kind"),
            ("answer", "kind"),
        ],
        specimens: specimens::record,
        frozen: true,
    },
    Product {
        name: "facts",
        tags: &[],
        specimens: specimens::facts,
        frozen: false,
    },
    Product {
        name: "needs_row",
        tags: &[],
        specimens: specimens::needs_row,
        frozen: false,
    },
    Product {
        name: "index_row",
        tags: &[],
        specimens: specimens::index_row,
        frozen: false,
    },
    Product {
        name: "read_row",
        tags: &[],
        specimens: specimens::read_row,
        frozen: true,
    },
    Product {
        name: "session_added",
        tags: &[],
        specimens: specimens::session_added,
        frozen: false,
    },
    Product {
        name: "session_status",
        tags: &[],
        specimens: specimens::session_status,
        frozen: false,
    },
    Product {
        name: "session_state",
        tags: &[],
        specimens: specimens::session_state,
        frozen: false,
    },
    Product {
        name: "session_removed",
        tags: &[],
        specimens: specimens::session_removed,
        frozen: false,
    },
];

/// 帧面 `cells-catalog` 的应答（纯计算，不读盘）：`{products: [{name, frozen, cells: [{path, kind, type}]}], pending: [{product, path, kind}]}`。
pub(crate) fn catalog() -> serde_json::Value {
    let products: Vec<serde_json::Value> = PRODUCTS
        .iter()
        .map(|p| serde_json::json!({"name": p.name, "frozen": p.frozen, "cells": cells_of(p).0}))
        .collect();
    serde_json::json!({"products": products, "pending": PENDING})
}

/// 各件成品的样本。全用成品自己的类型造（编译器管住字段齐全；`None` / 空列表由判据管）。
mod specimens {
    use super::{node_of, Specimen};
    use crate::agents::record::{Block, Body, Record, ReplyError, TitleBy};
    use crate::agents::{
        Answer, ApiReason, ChildRunTag, McpStatus, Pasted, PatchHunk, SessionActivity, Speaker,
        StepResult, ToolCard, ToolStep, UserText,
    };
    use crate::common::cells::Words;
    use crate::observe::facts_query::{
        Cost, LastRequest, LastSay, LimitFrom, McpTrouble, Needs, NeedsKind, PendingCall,
        RetryOutcome, RetryRun, SessionFacts, StepWait, TokenUse, UnclearWhy, UsageFact,
    };
    use crate::observe::history_query::IndexRow;
    use crate::stream::wire::{
        activity_cells, Frame, RemovalCause, SessionContainer, SessionFate, TerminalHost,
    };
    use std::collections::BTreeMap;

    fn s(v: &str) -> String {
        v.to_string()
    }
    fn some(v: &str) -> Option<String> {
        Some(v.to_string())
    }
    fn one<T>(k: &str, v: T) -> BTreeMap<String, T> {
        BTreeMap::from([(k.to_string(), v)])
    }

    fn speakers() -> Vec<Speaker> {
        vec![
            Speaker::Human,
            Speaker::SlashCommand {
                name: s("n"),
                args: s("a"),
            },
            Speaker::BashInput { command: s("c") },
            Speaker::BashOutput {
                stdout: s("o"),
                stderr: s("e"),
            },
            Speaker::CommandOutput,
            Speaker::TaskNotification {
                task_id: some("t"),
                status: some("s"),
                summary: some("m"),
                tool_use_id: some("u"),
            },
            Speaker::AgentMessage {
                from: some("f"),
                name: some("n"),
                handback: true,
                body: some("b"),
            },
            Speaker::PeerSession {
                from: some("f"),
                body: some("b"),
            },
            Speaker::Coordinator { body: some("b") },
            Speaker::AgentTask,
            Speaker::System { body: some("b") },
            Speaker::CompactSummary,
            Speaker::Interrupt,
            Speaker::ToolResult,
        ]
    }

    fn who(speaker: Speaker) -> UserText {
        UserText {
            speaker,
            text: s("x"),
            pasted: vec![Pasted {
                id: some("p"),
                start: 0,
                end: 1,
                body_start: 0,
                body_end: 1,
                lines: 1,
            }],
        }
    }

    fn result(answer: Answer) -> StepResult {
        StepResult {
            ok: true,
            rejected: true,
            lines: Some(1),
            added: Some(1),
            removed: Some(1),
            files: Some(1),
            answer: Some(answer),
            exit_code: Some(0),
            file: some("/f"),
            patch: Some(vec![PatchHunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                lines: vec![s("+a")],
            }]),
            patch_truncated: true,
        }
    }

    fn rec(body: Body) -> Record {
        Record {
            agent: s("agent"),
            id: s("r"),
            at: some("2026-10-09T00:00:00Z"),
            time_text: Some(Words(s("08:00"))),
            body,
        }
    }

    pub(super) fn record() -> Vec<Specimen> {
        let image = Block::Image {
            source: serde_json::json!({}),
        };
        let mut out: Vec<Record> = vec![
            rec(Body::Said {
                who: who(Speaker::Human),
                blocks: vec![
                    Block::Text { text: s("t") },
                    Block::ToolResult {
                        of: s("c"),
                        content: vec![Block::Text { text: s("t") }, image.clone()],
                        is_error: false,
                    },
                    image,
                ],
                results: one("c", result(Answer::Approved)),
                cwd: some("/w"),
            }),
            rec(Body::Said {
                who: who(Speaker::Human),
                blocks: vec![Block::Text { text: s("t") }],
                results: one(
                    "c",
                    result(Answer::Picked {
                        options: vec![s("o")],
                    }),
                ),
                cwd: some("/w"),
            }),
            rec(Body::Reply {
                blocks: vec![
                    Block::Text { text: s("t") },
                    Block::Thinking { text: s("t") },
                    Block::ToolUse {
                        id: s("c"),
                        name: s("Bash"),
                        input: serde_json::json!({}),
                    },
                ],
                model: some("m"),
                auto_reply: false,
                ends_turn: true,
                cards: one("c", ToolCard::Command),
                steps: one(
                    "c",
                    ToolStep {
                        tool: s("Bash"),
                        arg: some("ls"),
                        path: true,
                        note: some("n"),
                        known: true,
                    },
                ),
                runs: one(
                    "c",
                    ChildRunTag {
                        label: s("l"),
                        kind: some("k"),
                    },
                ),
                error: Some(ReplyError {
                    reason: ApiReason::Overloaded,
                    status: Some(529),
                }),
            }),
            rec(Body::Retry {
                reason: ApiReason::Network,
                attempt: Some(1),
                max: Some(3),
            }),
            rec(Body::Title {
                text: s("t"),
                by: TitleBy::Agent,
            }),
        ];
        for sp in speakers() {
            out.push(rec(Body::Said {
                who: who(sp),
                blocks: vec![Block::Text { text: s("t") }],
                results: one("c", result(Answer::Approved)),
                cwd: some("/w"),
            }));
        }
        // 排队的那一条只出人说的（`record_of`：别的来源排了队不进界面）。
        out.push(rec(Body::Queued {
            who: who(Speaker::Human),
        }));
        out.iter().map(node_of).collect()
    }

    pub(super) fn facts() -> Vec<Specimen> {
        let kinds = [
            NeedsKind::Approve,
            NeedsKind::Answer,
            NeedsKind::Plan,
            NeedsKind::Unknown,
        ];
        kinds
            .into_iter()
            .map(|kind| {
                let f = SessionFacts {
                    end: 1,
                    forked_from: some("src"),
                    touched_files: vec![s("/f")],
                    usage: Some(UsageFact::new(
                        1,
                        some("m"),
                        1,
                        (200_000, LimitFrom::Observed),
                    )),
                    project_dir: some("/w"),
                    agent: some("agent"),
                    writers: vec![1],
                    pending: vec![PendingCall {
                        id: s("c"),
                        name: s("Bash"),
                        what: some("ls"),
                        at: some("t"),
                        state: StepWait::Unclear,
                        why: Some(UnclearWhy::NoWriter),
                    }],
                    last_say: Some(LastSay {
                        text: s("t"),
                        at: some("t"),
                    }),
                    needs: Some(Needs {
                        kind,
                        tool: some("Bash"),
                        call: some("c"),
                        what: some("ls"),
                        since_ms: Some(1),
                        text: Words(s("t")),
                        tone: crate::common::cells::Tone::Need,
                        rank: 1,
                        waited_ms: Some(1),
                        waited_text: Some(crate::common::cells::Words(copy_core::short_duration(
                            1,
                        ))),
                    }),
                    handed_back: vec![s("a")],
                    retries: vec![RetryRun {
                        id: s("r"),
                        outcome: RetryOutcome::Recovered,
                    }],
                    permission_mode: some("default"),
                    tokens: Some(TokenUse {
                        input: 1,
                        output: 1,
                        cache_read: 1,
                        cache_write5m: 1,
                        cache_write1h: 1,
                        requests: 1,
                        text: Words(s("1")),
                        last: Some(LastRequest {
                            id: s("q"),
                            tokens: [1; 5],
                        }),
                    }),
                    cost: Some(Cost {
                        micros: 1,
                        partial: false,
                        text: Words(s("$0.01")),
                    }),
                    bg_tasks: vec![crate::observe::facts_query::BgTask {
                        call: s("c"),
                        task: some("b"),
                        cmd: some("make"),
                        at: some("t"),
                    }],
                    background: Some(crate::observe::facts_query::Background {
                        text: Words(s("t")),
                        clock: Some(crate::observe::facts_query::Clock {
                            text: Words(s("t")),
                            from: 1,
                        }),
                        what: Some(Words(s("make"))),
                        count: 1,
                        tone: crate::common::cells::Tone::Busy,
                    }),
                    mcp: [McpStatus::NeedsLogin, McpStatus::Failed, McpStatus::Pending]
                        .into_iter()
                        .map(|status| McpTrouble {
                            name: s("m"),
                            status,
                            detail: some("e"),
                            at: some("t"),
                        })
                        .collect(),
                };
                node_of(&f)
            })
            .collect()
    }

    pub(super) fn read_row() -> Vec<Specimen> {
        vec![node_of(&crate::observe::record_page::ReadRow {
            end: Some(1),
            hash: 1,
            record: Some(serde_json::json!({})),
            cwd: some("/w"),
        })]
    }

    /// `sessions-needs` 的一行：会话 id ＋ 那一份 `needs`（与 `facts.needs` 同一个类型）。
    pub(super) fn needs_row() -> Vec<Specimen> {
        vec![node_of(&crate::observe::accounts_query::NeedsRow {
            sid: s("s"),
            needs: Needs {
                kind: NeedsKind::Approve,
                tool: some("Bash"),
                call: some("c"),
                what: some("ls"),
                since_ms: Some(1),
                text: Words(s("t")),
                tone: crate::common::cells::Tone::Need,
                rank: 1,
                waited_ms: Some(1),
                waited_text: Some(Words(s("1"))),
            },
        })]
    }

    pub(super) fn index_row() -> Vec<Specimen> {
        let row = IndexRow {
            o: 0,
            n: 1,
            t: Some(crate::agents::record::RecordClass::Said),
            u: some("u"),
            sc: true,
            sp: Some("system"),
            ch: 1,
            cj: 1,
            pl: 1,
            cb: 1,
            cl: 1,
            fd: 1,
            x: some("x"),
            ts: some("t"),
        };
        vec![node_of(&row)]
    }

    fn added(activity: SessionActivity) -> Frame {
        Frame::SessionAdded {
            sid: s("s"),
            agent_kind: some("agent"),
            liveness_confidence: some("heuristic"),
            background: true,
            attachable: Some(true),
            cwd: some("/w"),
            project_dir: some("/w"),
            name: some("n"),
            path: some("/p"),
            lines: Some(1),
            activity: Some(activity),
            activity_text: activity_cells(Some(activity)).0,
            activity_tone: activity_cells(Some(activity)).1,
            waiting_for: some("w"),
            container: Some(SessionContainer::Hosted {
                host: TerminalHost::Tmux,
                terminal: some("tmux-1-1"),
            }),
            pid: Some(1),
        }
    }

    const ACTIVITIES: [SessionActivity; 4] = [
        SessionActivity::Working,
        SessionActivity::NeedsYou,
        SessionActivity::Idle,
        SessionActivity::BackgroundWork,
    ];

    pub(super) fn session_added() -> Vec<Specimen> {
        ACTIVITIES.into_iter().map(|a| node_of(&added(a))).collect()
    }

    pub(super) fn session_status() -> Vec<Specimen> {
        ACTIVITIES
            .into_iter()
            .map(|a| {
                node_of(&Frame::SessionStatus {
                    sid: s("s"),
                    activity: Some(a),
                    activity_text: activity_cells(Some(a)).0,
                    activity_tone: activity_cells(Some(a)).1,
                    waiting_for: some("w"),
                    liveness_confidence: some("heuristic"),
                })
            })
            .collect()
    }

    pub(super) fn session_state() -> Vec<Specimen> {
        [SessionFate::Reconnectable, SessionFate::Ended]
            .into_iter()
            .map(|f| node_of(&Frame::session_state(s("s"), f)))
            .collect()
    }

    pub(super) fn session_removed() -> Vec<Specimen> {
        vec![node_of(&Frame::SessionRemoved {
            sid: s("s"),
            cause: RemovalCause::Superseded,
        })]
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/cells_catalog_tests.rs"]
mod tests;
