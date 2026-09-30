//! 要求住址：`设计/00 §1.2`「共享 crate 只放两边必须对上的契约（路径 · 端口 · 文件格式 · 文案表 · 令牌形状 · 字节表键），不放判定；判定只在后端」＋ `99 §2.3` THIN 第 5 件「monitor 生产段只许依赖契约类 crate」。
//!
//! 三道：
//! ① `src/common/` 下每个 crate 恰好登记一类、逐个写理由（两向：目录 == [`CRATES`]）；
//! ② monitor 生产段（`[dependencies]` · `[build-dependencies]` · `[target.*.dependencies]`）点名的每个 path 依赖、连同它们自己生产段的
//!    path 依赖闭包，只许住 `src/common/` 且是契约类；判定类出现即红 —— 唯一例外是 ③ 按符号钉住的 `deploy-core`；
//! ③ `deploy-core` 按符号：monitor 生产源码（壳 `src/**` ＋ 它经 `#[path]` 收进来的通信层文件）里每个 `deploy_core::X`
//!    要么在契约名单 [`DEPLOY_CONTRACT`] 里，要么是 [`DEPLOY_RESIDUAL`] 登记的自举残留（符号 × 住址两向相等：搬走一处，登记当场红）。
//!
//! 买不到：契约类 crate **里面**长出一个判定（这条只按 crate / 符号分类，不读函数体；那是 `judgment-single-home.vitest.ts` 的 CORE_ITEMS）；
//! `use deploy_core::*` 之外的别名路（`extern crate … as`）今天全仓零处，下面那道「`deploy_core` 只以 `deploy_core::` 出现」顺带挡住。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// 两侧必须对上的形状：monitor 生产段可以依赖。
    Contract,
    /// 裁决：只许后端用；monitor 生产段依赖即红（`deploy-core` 按 ③ 的符号例外）。
    Decision,
    /// 判据原语：两侧只在 dev 侧。
    TestInfra,
}

/// `src/common/` 每个 crate 的类与理由（`00 §1.2` 表「类」一列的判据版）。
const CRATES: &[(&str, Class, &str)] = &[
    ("acct-core", Class::Decision, "账号能不能用 · 走哪一支（`auth_ready` · `apikey_routed_subset`）是裁决；界面那份 `AUTH_KINDS` 由测试档的生成器现生成（monitor 只在 dev 侧链它）"),
    ("copy-core", Class::Contract, "文案表：两侧按同一个键取同一句（`copy_text` / `copy_static`），不裁决"),
    ("creds-core", Class::Contract, "文件格式 · 路径：凭据落盘格式与文件名、数据目录（monitor 只算路径；写半边在 `harden` 后面，monitor 不开）"),
    ("deploy-core", Class::Decision, "那台要哪一格 · 换不换是裁决（`judge` · `identity_decision` · `is_newer`）；它同时带字节表键与身份戳格式 ⇒ monitor 只按 ③ 的符号用"),
    ("guard-core", Class::TestInfra, "源码扫描判据的原语，两侧都只在 dev 侧"),
    ("relay-route-core", Class::Contract, "端口 · 路径 · 路由语法：中转与常驻监听口的门牌、后端落点，两侧拼 / 拆同一份"),
    ("search-core", Class::Decision, "搜索口径与多机合并排序是裁决，只后端用"),
    ("shell-quote-core", Class::Contract, "令牌形状：POSIX 单引号 quote 与标识符放行形状（session id · 启动令牌 · cc-bus id · 路径），两侧拼进 shell 前对上同一份"),
    ("upstream-url-core", Class::Decision, "上游 URL 能不能用是裁决；界面读的是生成器现生成的式子（monitor 只在 dev 侧链它）"),
];

/// `deploy-core` 里 monitor 生产段可以用的契约符号：字节表键 · 线上答话的形状 · 身份戳格式 · 落点路径。
const DEPLOY_CONTRACT: &[(&str, &str)] = &[
    ("Arch", "字节表键"),
    ("Os", "字节表键"),
    ("Key", "字节表键"),
    ("Product", "字节表键（`wire` / `of_wire` 是线上两个词）"),
    ("Route", "字节表键（本机 / 远端）"),
    ("LINES", "字节表的行（取字节口按它把键映到内嵌字节）"),
    ("UNAME_CMD", "问那台是什么机器的那一行命令"),
    ("key_of", "两个线上词 → 字节表键（解析，不裁决）"),
    ("key_from_uname", "`uname -s -m` 的答话 → 字节表键（解析）"),
    ("Refusal", "拒绝的形状（后端答话与自举同一套词）"),
    (
        "DeployAction",
        "`deploy-plan` 答话的形状（monitor 解码后照做）",
    ),
    ("LegacyVerdict", "`deploy-plan` 答话里旧落点那一格的形状"),
    ("LEGACY_BACKEND_REL", "旧落点路径"),
    (
        "LEGACY_ENTRY_REL",
        "旧入口路径（`~/.local/bin/ccm`；去向后端判，monitor 照删）",
    ),
    ("Marks", "身份戳的两个界标（文件格式）"),
    ("RemoteIdentity", "读身份戳的结果形状"),
    ("identity_of_bytes", "从字节里读身份戳（文件格式，不跑它）"),
];

/// `deploy-core` 判定符号在 monitor 生产段的**自举残留**：(符号, 住址 `文件::函数`, 为什么搬不进后端)。
/// 两向：登记的每一条恰好在它说的函数里出现；没登记的判定符号出现即红。
const DEPLOY_RESIDUAL: &[(&str, &str, &str)] = &[
    (
        "judge",
        "src/frontend/shell/src/byte_table.rs::choose",
        "本机常驻后端自己的那一份字节：放它的时候还没有后端可问（取字节口的拒绝链）",
    ),
    (
        "identity_decision",
        "src/frontend/shell/src/local_backend.rs::extract_embedded_to",
        "同上：本机那一份换不换，放完它才有后端",
    ),
];

fn root() -> PathBuf {
    crate::guard_support::repo_root()
}

fn class_of(name: &str) -> Option<Class> {
    CRATES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, c, _)| *c)
}

/// 生产段：`[dependencies]` · `[build-dependencies]` · `[target.<cfg>.dependencies]` · `[target.<cfg>.build-dependencies]`。
fn is_production_section(header: &str) -> bool {
    let h = header
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    let last = h.rsplit('.').next().unwrap_or(h);
    (h == "dependencies" || h == "build-dependencies" || h.starts_with("target."))
        && (last == "dependencies" || last == "build-dependencies")
}

/// 一份 manifest 生产段里的 path 依赖 → (依赖名, 解析后的目录)。
fn production_path_deps(manifest: &Path) -> Vec<(String, PathBuf)> {
    let raw = std::fs::read_to_string(manifest)
        .unwrap_or_else(|e| panic!("读 {}: {e}", manifest.display()));
    let text = guard_core::strip_hash_comment_lines(&raw);
    let dir = manifest.parent().expect("manifest 有父目录");
    let mut section = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            section = t.to_string();
            continue;
        }
        if !is_production_section(&section) {
            continue;
        }
        for p in guard_core::inline_table_paths(line) {
            let name = t.split('=').next().unwrap_or_default().trim().to_string();
            out.push((name, normalize(&dir.join(p))));
        }
    }
    out
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

fn common_dir() -> PathBuf {
    root().join("src/common")
}

/// monitor 生产段依赖闭包里每个 path 依赖：(谁带进来的, 依赖名, 目录)。
fn monitor_production_closure() -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut todo = vec![(
        "monitor".to_string(),
        root().join("src/frontend/shell/Cargo.toml"),
    )];
    while let Some((who, manifest)) = todo.pop() {
        for (name, dir) in production_path_deps(&manifest) {
            out.push((who.clone(), name.clone(), dir.clone()));
            if seen.insert(dir.clone()) {
                todo.push((name, dir.join("Cargo.toml")));
            }
        }
    }
    out
}

/// monitor 生产源码：壳 `src/**/*.rs` ＋ 其中生产段经 `#[path = "…"]` 收进来、不在 `tests/` 下的文件。→ (仓内相对路径, 生产段正文)。
fn monitor_production_sources() -> Vec<(String, String)> {
    let root = root();
    let mut files: BTreeMap<PathBuf, String> =
        guard_core::scan_tree_excluding(&root.join("src/frontend/shell/src"), &["rs"], &[])
            .into_iter()
            .map(|(p, raw)| (normalize(&p), guard_core::production_code(&raw)))
            .collect();
    let mut todo: Vec<PathBuf> = files.keys().cloned().collect();
    while let Some(p) = todo.pop() {
        let body = files[&p].clone();
        for rest in body.split("#[path = \"").skip(1) {
            let rel = rest.split('"').next().unwrap_or_default();
            let target = normalize(&p.parent().expect("文件有父目录").join(rel));
            if target.starts_with(root.join("tests")) || files.contains_key(&target) {
                continue;
            }
            let raw = std::fs::read_to_string(&target)
                .unwrap_or_else(|e| panic!("`#[path]` 指向 {} 读不出：{e}", target.display()));
            files.insert(target.clone(), guard_core::production_code(&raw));
            todo.push(target);
        }
    }
    files
        .into_iter()
        .map(|(p, body)| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, body)
        })
        .collect()
}

/// 一段生产正文里每个 `deploy_core::X`（花括号组展开）→ (符号, 它所在的函数名；模块级为空串)。
fn deploy_symbols(body: &str) -> Vec<(String, String)> {
    let needle = "deploy_core::";
    let mut out = Vec::new();
    for (at, _) in body.match_indices(needle) {
        let rest = &body[at + needle.len()..];
        let names: Vec<String> = if let Some(group) = rest.strip_prefix('{') {
            let inner = group.split('}').next().unwrap_or_default();
            inner
                .split(',')
                .map(|s| {
                    s.split("::")
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string()
                })
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '*')
                .collect()]
        };
        let before = &body[..at];
        let func = before
            .rmatch_indices("fn ")
            .find(|(i, _)| {
                *i == 0
                    || !before.as_bytes()[i - 1].is_ascii_alphanumeric()
                        && before.as_bytes()[i - 1] != b'_'
            })
            .map(|(i, _)| {
                before[i + 3..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect::<String>()
            })
            .unwrap_or_default();
        for n in names {
            out.push((n, func.clone()));
        }
    }
    out
}

#[test]
fn every_common_crate_has_exactly_one_class_with_a_reason() {
    // 遍历口径只有一份（`guard_core`）：`src/common/<crate>/Cargo.toml` 那一层。
    let common = common_dir();
    let on_disk: BTreeSet<String> = guard_core::scan_tree_excluding(&common, &["toml"], &[])
        .into_iter()
        .filter_map(|(p, _)| {
            let rel = p
                .strip_prefix(&common)
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            rel.strip_suffix("/Cargo.toml")
                .filter(|c| !c.contains('/'))
                .map(str::to_string)
        })
        .collect();
    let listed: BTreeSet<String> = CRATES.iter().map(|(n, _, _)| n.to_string()).collect();
    assert_eq!(listed.len(), CRATES.len(), "登记表里有重名");
    assert_eq!(
        on_disk, listed,
        "`src/common/` 与登记表两向不等 —— 新共享 crate 先想清楚它是契约还是判定（判定的家在后端），再登记"
    );
    for (n, _, why) in CRATES {
        assert!(why.chars().count() >= 12, "`{n}` 的理由写得太短：{why:?}");
    }
}

#[test]
fn the_monitor_production_segment_links_only_contract_crates() {
    let closure = monitor_production_closure();
    assert!(
        closure.iter().any(|(_, n, _)| n == "copy-core"),
        "monitor 生产段一个 path 依赖都没量到（`copy-core` 明明在）—— 量法瞎了"
    );
    let common = common_dir();
    let mut bad = Vec::new();
    for (who, name, dir) in &closure {
        let Ok(rel) = dir.strip_prefix(&common) else {
            bad.push(format!(
                "{who} → {name}：path 依赖不在 `src/common/` 下（{}）",
                dir.display()
            ));
            continue;
        };
        let crate_dir = rel.to_string_lossy().replace('\\', "/");
        match class_of(&crate_dir) {
            Some(Class::Contract) => {}
            Some(Class::Decision) if crate_dir == "deploy-core" => {} // ③ 按符号管
            Some(c) => bad.push(format!(
                "{who} → {crate_dir}：{c:?} 类，monitor 生产段不许链它（判定的家在后端）"
            )),
            None => bad.push(format!("{who} → {crate_dir}：没登记类")),
        }
    }
    assert!(
        bad.is_empty(),
        "monitor 生产段依赖了非契约类 crate：\n{}",
        bad.join("\n")
    );
}

#[test]
fn deploy_core_is_used_only_through_its_contract_and_the_pinned_bootstrap_residual() {
    let contract: BTreeSet<&str> = DEPLOY_CONTRACT.iter().map(|(s, _)| *s).collect();
    let mut residual_hits: BTreeMap<(&str, &str), usize> = DEPLOY_RESIDUAL
        .iter()
        .map(|(s, at, _)| ((*s, *at), 0))
        .collect();
    let mut bad = Vec::new();
    let sources = monitor_production_sources();
    assert!(
        sources
            .iter()
            .any(|(r, _)| r == "src/comms/inward/backend_route.rs"),
        "经 `#[path]` 收进来的通信层文件没进扫描面 —— 量法瞎了"
    );
    let mut total = 0;
    for (rel, body) in &sources {
        let bare = body.matches("deploy_core").count();
        let qualified = body.matches("deploy_core::").count();
        if bare != qualified {
            bad.push(format!("{rel}：`deploy_core` 不以 `deploy_core::` 出现（别名 / `use deploy_core;` 会绕过逐符号判）"));
        }
        for (sym, func) in deploy_symbols(body) {
            total += 1;
            if contract.contains(sym.as_str()) {
                continue;
            }
            let at = format!("{rel}::{func}");
            match residual_hits.iter_mut().find(|((s, a), _)| *s == sym && *a == at) {
                Some((_, n)) => *n += 1,
                None => bad.push(format!("`deploy_core::{sym}` 在 `{at}`：判定符号，monitor 生产段不许用（问后端；自举残留要登记住址与理由）")),
            }
        }
    }
    assert!(
        total > 0,
        "monitor 生产段一个 `deploy_core::` 都没量到 —— 量法瞎了"
    );
    for ((sym, at), n) in &residual_hits {
        if *n == 0 {
            bad.push(format!(
                "残留登记 `{sym}` @ `{at}` 已经不在那里了 —— 搬走了就删这一行"
            ));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
