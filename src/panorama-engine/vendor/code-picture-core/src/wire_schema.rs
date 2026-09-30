//! **线上类型的 TypeScript 声明** —— 生成物 `src/wire.ts` 的写法与它的判据(只在测试构建里有)。
//!
//! 为什么要它:别的语言的消费方(cc-monitor 前端)此前照着本 crate 的结构体**手抄**一份类型,
//! 靠逐键对拍兜底 —— 可选性(`Option` · `skip_serializing_if`)对拍不出来,上游加一格就静默漂。
//! 现在由 ts-rs 从 `#[cfg_attr(test, derive(ts_rs::TS))]` 读同一份 serde 属性写出,消费方据它生成。
//!
//! 住在 `src/` 下是刻意的:别人 vendor 本 crate 时拷的就是 `src/` + `Cargo.toml`,生成物随之同到。
//! 整数一律写成 `number`(线上是 JSON 数;ts-rs 默认把 64 位整数写成 `bigint`,那与 `JSON.parse` 的结果不符)。

use crate::diagram::{
    ArchLink, ArchNode, CallEdge, CallNode, Diagram, DiagramBody, DiagramError, DiagramKind,
    DiagramKindInfo, DiagramParam, DiagramRequest, DiagramShape, Honesty, Omitted, TypeField,
    TypeMethod, TypeNode, TypeRelKind, TypeRelation,
};
use crate::edits::{FileEdit, Planned};
use crate::model::*;
use std::collections::BTreeSet;
use std::path::Path;
use ts_rs::{Config, TS};

/// 一个类型的那一段:文档注释 + `export type …`。
fn one<T: TS + ?Sized>(cfg: &Config) -> String {
    let mut s = T::docs().unwrap_or_default();
    s.push_str("export ");
    s.push_str(&T::decl(cfg));
    s.push('\n');
    s
}

/// ★ 全部线上类型(按模块、按源码顺序)。**= 本 crate 里全部实现了 `Serialize` 的类型**(下面那条判据两向钉)。
fn all(cfg: &Config) -> Vec<String> {
    vec![
        // model
        one::<SymbolRef>(cfg),
        one::<SymKind>(cfg),
        one::<Lang>(cfg),
        one::<Symbol>(cfg),
        one::<LineRange>(cfg),
        one::<Anchor>(cfg),
        one::<AnchorState>(cfg),
        one::<Location>(cfg),
        one::<Resolution>(cfg),
        one::<IndexStats>(cfg),
        one::<IndexProgress>(cfg),
        one::<IndexPhase>(cfg),
        one::<IndexDelta>(cfg),
        one::<EdgeKind>(cfg),
        one::<Confidence>(cfg),
        one::<Edge>(cfg),
        one::<RankedFile>(cfg),
        one::<Subsystem>(cfg),
        one::<Overview>(cfg),
        one::<AffectedSymbol>(cfg),
        one::<ImpactSet>(cfg),
        one::<CfgBlock>(cfg),
        one::<CfgDef>(cfg),
        one::<Cfg>(cfg),
        one::<FlowStep>(cfg),
        one::<FlowSet>(cfg),
        one::<CallPath>(cfg),
        one::<PathSet>(cfg),
        one::<LinkSource>(cfg),
        one::<DocLink>(cfg),
        one::<DriftItem>(cfg),
        one::<AnnotationStatus>(cfg),
        one::<Annotation>(cfg),
        one::<AnnotationOrigin>(cfg),
        one::<NodeView>(cfg),
        one::<SubGraph>(cfg),
        one::<Neighborhood>(cfg),
        one::<Reached>(cfg),
        // edits
        one::<FileEdit>(cfg),
        one::<Planned<String>>(cfg),
        // diagram
        one::<ArchNode>(cfg),
        one::<ArchLink>(cfg),
        one::<DiagramKind>(cfg),
        one::<DiagramShape>(cfg),
        one::<DiagramParam>(cfg),
        one::<DiagramKindInfo>(cfg),
        one::<DiagramRequest>(cfg),
        one::<DiagramError>(cfg),
        one::<Diagram>(cfg),
        one::<DiagramBody>(cfg),
        one::<CallNode>(cfg),
        one::<CallEdge>(cfg),
        one::<Honesty>(cfg),
        one::<Omitted>(cfg),
        one::<TypeNode>(cfg),
        one::<TypeField>(cfg),
        one::<TypeMethod>(cfg),
        one::<TypeRelation>(cfg),
        one::<TypeRelKind>(cfg),
    ]
}

/// 生成物全文。
fn render() -> String {
    let cfg = Config::new().with_large_int("number");
    let mut out = String::from(
        "// 生成物 —— code-picture-core 的线上类型(serde 序列化出来的样子),由 `src/wire_schema.rs` 从\n\
         // `#[cfg_attr(test, derive(ts_rs::TS))]` 写出(`cargo test -p code-picture-core wire_schema`)。不许手改。\n\n",
    );
    for s in all(&cfg) {
        out.push_str(&s);
        out.push('\n');
    }
    out
}

fn src_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// ★ 提交的 `src/wire.ts` == 由类型现写出来的那一份。漂了 ⇒ 当场重写并红一次(重跑即绿,把它一起提交)。
#[test]
fn the_committed_schema_is_what_the_types_say() {
    let p = src_dir().join("wire.ts");
    let want = render();
    let have = std::fs::read_to_string(&p).unwrap_or_default();
    if have != want {
        std::fs::write(&p, &want).unwrap();
        panic!("{p:?} 与类型对不上,已按类型重写 —— 重跑即绿,把它一起提交");
    }
}

/// 源码里实现了 `Serialize` 的类型名:`#[derive(…Serialize…)]` 之后那个 `pub struct|enum`,
/// 以及手写的 `impl Serialize for X`(本判据自己这个文件除外)。
fn serialize_types_in_source() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![src_dir()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|x| x != "rs") || p.ends_with("wire_schema.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                let t = l.trim();
                if let Some(rest) = t.strip_prefix("impl Serialize for ") {
                    out.insert(
                        rest.split(|c: char| !c.is_alphanumeric())
                            .next()
                            .unwrap()
                            .into(),
                    );
                }
                if !(t.starts_with("#[derive(") && t.contains("Serialize")) {
                    continue;
                }
                let item = lines[i + 1..]
                    .iter()
                    .map(|x| x.trim())
                    .find(|x| !x.starts_with('#') && !x.starts_with("//"))
                    .unwrap_or_else(|| panic!("{p:?} 第 {i} 行的 derive 后面没有类型"));
                let name = item
                    .strip_prefix("pub struct ")
                    .or_else(|| item.strip_prefix("pub enum "))
                    .unwrap_or_else(|| {
                        panic!("{p:?}:`{item}` 不是 pub struct / enum —— 线上类型都该是公开的")
                    });
                out.insert(
                    name.split(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap()
                        .into(),
                );
            }
        }
    }
    out
}

/// ★ 生成物里的类型 == 源码里全部 `Serialize` 类型(两向):加了一个线上类型没进 [`all`] ⇒ 红;
/// [`all`] 里多一个不上线的 ⇒ 也红。
#[test]
fn every_serialize_type_is_in_the_schema_and_nothing_else() {
    let exported: BTreeSet<String> = render()
        .lines()
        .filter_map(|l| l.strip_prefix("export type "))
        .map(|r| {
            r.split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap()
                .to_string()
        })
        .collect();
    let in_source = serialize_types_in_source();
    assert!(
        in_source.contains("Symbol") && in_source.contains("DiagramKind"),
        "正控:{in_source:?}"
    );
    assert_eq!(exported, in_source);
}
