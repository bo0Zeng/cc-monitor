//! 格目录的判据：目录 ＝ 成品真序列化出来的格（两向）· 缺格表与目录不重 · 帧面那条命令出的就是它。

use super::*;
use serde_json::Value;
use std::collections::BTreeSet;

fn product(name: &str) -> &'static Product {
    PRODUCTS
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("目录里没有成品 {name}"))
}

fn catalog_of(name: &str) -> BTreeMap<String, Cell> {
    cells_of(product(name))
        .0
        .into_iter()
        .map(|c| (c.path.clone(), c))
        .collect()
}

/// 一份真成品（JSON）里出现的格路径：写法与目录同一种；以 id 为键的表 · 透传的一团 · 判别格按目录认。
/// 值是 `null` 的格算「出现了」（线上那一格在，只是这一次没有值）。
fn seen(
    v: &Value,
    path: &str,
    tags: &[(&str, &str)],
    cat: &BTreeMap<String, Cell>,
    out: &mut BTreeSet<String>,
) {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            out.insert(path.to_string());
        }
        Value::Array(items) => {
            let at = format!("{path}[]");
            // 空列表：那一格在线上（只是这一次没有项）—— 只在它本身是叶子格（值的列表）时算出现。
            if items.is_empty() && cat.contains_key(&at) {
                out.insert(at.clone());
            }
            for it in items {
                seen(it, &at, tags, cat, out);
            }
        }
        Value::Object(m) => {
            if cat.get(path).is_some_and(|c| c.ty == "object") {
                out.insert(path.to_string());
                return;
            }
            let star = join(path, "*");
            if cat.keys().any(|k| {
                k == &star
                    || k.starts_with(&format!("{star}."))
                    || k.starts_with(&format!("{star}["))
            }) {
                for x in m.values() {
                    seen(x, &star, tags, cat, out);
                }
                return;
            }
            let disc = m.iter().find_map(|(k, x)| match x {
                Value::String(s) if is_tag(tags, path, k) => Some((k.as_str(), s.as_str())),
                _ => None,
            });
            let base = disc.map_or_else(|| path.to_string(), |(k, s)| picked(path, k, s));
            for (k, x) in m {
                if disc.is_some_and(|(dk, _)| dk == k) {
                    out.insert(join(path, k));
                } else {
                    seen(x, &join(&base, k), tags, cat, out);
                }
            }
        }
    }
}

fn seen_all(name: &str, vals: &[Value]) -> BTreeSet<String> {
    let p = product(name);
    let cat = catalog_of(name);
    let mut out = BTreeSet::new();
    for v in vals {
        seen(v, "", p.tags, &cat, &mut out);
    }
    out
}

/// 一条真路径能读成的全部写法（去掉任意几处挑法）：目录把每一种都有的格提到挑法外面写，真路径要能对上它。
fn gens(p: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::from([p.to_string()]);
    let mut todo = vec![p.to_string()];
    while let Some(x) = todo.pop() {
        for (pre, list, _, _, rest) in picks(&x) {
            let g = unpicked(&pre, list, &rest);
            if out.insert(g.clone()) {
                todo.push(g);
            }
        }
    }
    out
}

fn golden(file: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/__fixtures__")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不了 {}: {e}", path.display()))
}

/// 各件成品的真样子：跨语言金样里由真代码写出来的那几份（＋ 金样里恰好缺的那一格，用真代码现造）。
fn corpus(name: &str) -> Vec<Value> {
    match name {
        "record" => golden("record.golden.jsonl")
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap()["record"].clone())
            .collect(),
        "facts" => {
            let g: Value = serde_json::from_str(&golden("session-reads.golden.json")).unwrap();
            let mut f = g["history-facts"].clone();
            // 金样那份会话不在等人（`needs` 是 null）⇒ 这一格用真判定现造一份。
            let pending = vec![crate::observe::facts_query::PendingCall {
                id: "c".into(),
                name: "Bash".into(),
                what: Some("ls".into()),
                at: None,
                state: crate::observe::facts_query::StepWait::Running,
                why: None,
            }];
            let wait = crate::observe::facts_query::PidWait {
                waiting_for: Some(crate::agents::WaitOn::Permission),
                since_ms: Some(1),
                read_at_ms: 0,
            };
            f["needs"] =
                serde_json::to_value(crate::observe::facts_query::needs_of(&pending, Some(&wait)))
                    .unwrap();
            // 金样那份会话不在后台任务运行中那一态（`background` 是 null）⇒ 同上，用真判定现造一份（金样里那几条没收场的后台命令）。
            let tasks: Vec<crate::observe::facts_query::BgTask> =
                serde_json::from_value(f["bgTasks"].clone()).unwrap();
            f["background"] = serde_json::to_value(crate::observe::facts_query::background_of(
                &tasks,
                None,
                u64::MAX / 2,
            ))
            .unwrap();
            vec![f]
        }
        // 清单那一行用真判定现造（金样生成那一处不起进程，答不出非空的清单）。
        "needs_row" => {
            let pending = vec![crate::observe::facts_query::PendingCall {
                id: "c".into(),
                name: "Bash".into(),
                what: Some("ls".into()),
                at: None,
                state: crate::observe::facts_query::StepWait::Running,
                why: None,
            }];
            let wait = crate::observe::facts_query::PidWait {
                waiting_for: Some(crate::agents::WaitOn::Permission),
                since_ms: Some(1),
                read_at_ms: 2,
            };
            let needs = crate::observe::facts_query::needs_of(&pending, Some(&wait)).unwrap();
            vec![
                serde_json::to_value(crate::observe::accounts_query::NeedsRow {
                    sid: "s".into(),
                    needs,
                })
                .unwrap(),
            ]
        }
        "index_row" => {
            let g: Value = serde_json::from_str(&golden("session-reads.golden.json")).unwrap();
            let mut rows = g["history-index"]["rows"].as_array().unwrap().clone();
            // 金样那份会话里没有子运行、没有代码块与全宽字 ⇒ 这一行用真函数现算。
            let line = r#"{"type":"assistant","uuid":"a-1","isSidechain":true,"agentId":"ag","message":{"content":[{"type":"text","text":"\u4e2d\n```\nx\n```"}]}}"#.as_bytes();
            rows.push(
                serde_json::to_value(crate::observe::history_query::index_row(
                    line,
                    0,
                    line.len() as u64,
                ))
                .unwrap(),
            );
            rows
        }
        // 行摘要：真走读核（Claude 那一家）读金样记录的原文，再加一行不进界面的（只有 `{end, hash}`）与一截残尾（`end` 是 null）。
        "read_row" => {
            let page = format!(
                "{}\n{}\n{{torn",
                r#"{"type":"user","uuid":"u1","timestamp":"2026-10-09T01:30:00.000Z","cwd":"/w","message":{"role":"user","content":"q"}}"#,
                r#"{"type":"permission-mode","permissionMode":"default","sessionId":"s"}"#,
            );
            let face = crate::agents::claudecode::RECORDS;
            let mut reader = crate::observe::record_page::Reader::new(&face, 0, &[]);
            crate::observe::record_page::rows_of(&mut reader, 0, page.as_bytes())
                .into_iter()
                .map(|r| serde_json::to_value(r).unwrap())
                .collect()
        }
        frame => golden("session-stream.golden.jsonl")
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|v| v["kind"] == frame)
            .collect(),
    }
}

/// ★ 样本把每一格都露出来了：没有 `None`、没有空列表（不然那一格在目录里就漏了）。
#[test]
fn every_specimen_fills_every_cell() {
    for p in PRODUCTS {
        let (cells, holes) = cells_of(p);
        assert!(holes.is_empty(), "成品 {} 的样本没写全：{holes:#?}", p.name);
        assert!(!cells.is_empty(), "成品 {} 一格都没有", p.name);
    }
}

/// ★★ 目录对真代码写出来的成品（跨语言金样），两向：金样里见到的每一格都在目录里；目录里的每一格在金样里都见得到。
#[test]
fn the_catalog_equals_the_cells_real_products_serialize() {
    for p in PRODUCTS {
        let cat = catalog_of(p.name);
        let real = seen_all(p.name, &corpus(p.name));
        assert!(
            !real.is_empty(),
            "成品 {} 的金样一份都没读到 —— 本条在空转",
            p.name
        );
        let missing: Vec<&String> = real
            .iter()
            .filter(|r| !gens(r).iter().any(|g| cat.contains_key(g)))
            .collect();
        let real: BTreeSet<String> = real.iter().flat_map(|r| gens(r)).collect();
        assert!(
            missing.is_empty(),
            "成品 {} 线上有、目录里没有的格：{missing:#?}",
            p.name
        );
        let unseen: Vec<&String> = cat.keys().filter(|k| !real.contains(*k)).collect();
        assert!(
            unseen.is_empty(),
            "成品 {} 目录里有、金样里见不到的格：{unseen:#?}",
            p.name
        );
    }
}

/// ★★ 走查器没走歪：目录 ＝ 同一批样本经 `serde_json` 真写出来的格，两向相等（判别格 · 表 · 透传的一团按目录认）。
#[test]
fn the_catalog_is_what_serde_writes_for_the_specimens() {
    for p in PRODUCTS {
        let cat = catalog_of(p.name);
        let wire: Vec<Value> = (p.specimens)().into_iter().map(|s| s.unwrap().1).collect();
        let real = seen_all(p.name, &wire);
        let missing: Vec<&String> = real
            .iter()
            .filter(|r| !gens(r).iter().any(|g| cat.contains_key(g)))
            .collect();
        assert!(
            missing.is_empty(),
            "成品 {} 线上写了、目录里没有：{missing:#?}",
            p.name
        );
        let real: BTreeSet<String> = real.iter().flat_map(|r| gens(r)).collect();
        let extra: Vec<&String> = cat.keys().filter(|k| !real.contains(*k)).collect();
        assert!(
            extra.is_empty(),
            "成品 {} 目录里有、线上没写：{extra:#?}",
            p.name
        );
    }
}

/// 写好的字与语气由类型说，不按名字猜：同叫 `text`，原文是值、核心写的是字。
#[test]
fn a_cell_is_text_only_when_its_type_says_so() {
    let kind = |p: &str, c: &str| {
        catalog_of(p)
            .get(c)
            .unwrap_or_else(|| panic!("{p} 里没有 {c}"))
            .kind
    };
    assert_eq!(kind("facts", "tokens.text"), "text");
    assert_eq!(kind("facts", "cost.text"), "text");
    assert_eq!(kind("record", "timeText"), "text");
    assert_eq!(kind("record", "{t=said}.blocks[type=text].text"), "value");
    assert_eq!(kind("record", "{t=title}.text"), "value");
    assert_eq!(kind("facts", "lastSay.text"), "value");
    assert_eq!(catalog_of("facts")["usage.limitFrom"].ty, "enum");
}

/// 缺格表：每一行的成品在目录里、那一格还不在（落地了就删那一行）。表空着也是真话（今天一格都不缺）。
#[test]
fn pending_cells_are_not_in_the_catalog_yet() {
    for c in PENDING {
        assert!(
            PRODUCTS.iter().any(|p| p.name == c.product),
            "缺格表里的成品 {} 不在目录里",
            c.product
        );
        assert!(
            !catalog_of(c.product).contains_key(c.path),
            "{}.{} 已经落地 —— 从 PENDING 里删掉那一行",
            c.product,
            c.path
        );
    }
}

/// 帧面那条命令出的就是目录。
#[test]
fn the_frame_command_answers_the_catalog() {
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "cells-catalog")
        .expect("注册表里没有 cells-catalog");
    assert!(!spec.takes_input);
    let v = catalog();
    let names: Vec<&str> = v["products"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, PRODUCTS.iter().map(|p| p.name).collect::<Vec<_>>());
    assert_eq!(v["pending"].as_array().unwrap().len(), PENDING.len());
}

/// 金样重写开关（人读过「现打」再落盘）。
const REGEN: &str = "CCM_REGEN_CELLS_CATALOG";

/// ★ 格目录的金样 ＝ 帧命令真写出来的那一份（出口不起后端也能读；注册表出参对拍也用它）。改了成品就重写：
/// `CCM_REGEN_CELLS_CATALOG=1 cargo test --lib -- cells_catalog`。
///
/// 冻结的成品（[`Product::frozen`]）只许加格：金样里它有的每一格，目录里还得在、类别与类型不变 —— 先于重写判，
/// 所以删 / 改名 / 换类型重写金样也不放行（要删得先手改金样，那是一次有意的协议变化）。
#[test]
fn the_golden_is_what_the_command_writes() {
    let now = catalog();
    let got = format!("{}\n", serde_json::to_string_pretty(&now).unwrap());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/__fixtures__/cells-catalog.golden.json");
    let had: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
        .unwrap_or(Value::Null);
    let cells = |v: &Value, name: &str| -> Vec<Value> {
        v["products"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["name"] == name)
            .and_then(|p| p["cells"].as_array().cloned())
            .unwrap_or_default()
    };
    let frozen: Vec<&Product> = PRODUCTS.iter().filter(|p| p.frozen).collect();
    assert!(!frozen.is_empty(), "一件冻结的成品都没有 —— 本条在空转");
    for p in frozen {
        let kept = cells(&had, p.name);
        assert!(
            !kept.is_empty(),
            "冻结的成品 {} 在落盘的金样里一格都没有 —— 先把它连同格写进金样",
            p.name
        );
        let now = cells(&now, p.name);
        let lost: Vec<&Value> = kept.iter().filter(|c| !now.contains(c)).collect();
        assert!(
            lost.is_empty(),
            "冻结的成品 {} 少了格或格换了样（两个前端在读，只许加）：{lost:#?}",
            p.name
        );
    }
    if std::env::var_os(REGEN).is_some() {
        std::fs::write(&path, &got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(got == want, "格目录与金样不一致（{REGEN}=1 重写）");
}
