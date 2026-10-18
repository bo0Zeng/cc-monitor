//! **每份落盘文件一处 [`Doc`](super::Doc) 声明** —— 扫描型判据（读三态 · 状态 / 缓存两种的行为本身由 `own_state_tests` 的 S6 · S7 钉）。
//!
//! | # | 判据 | 正控 |
//! |---|---|---|
//! | D1 | 经 `Own` 写（`own.write(` · `own.write_streamed(`，空白不计）的生产文件里都有 `Doc::state(` / `Doc::cache(` 声明 | 写了没声明的一份 ⇒ 红 |
//! | D2 | 声明只住 `const` 条目（`const X: Doc<…> = Doc::…(`），落点 `rel` 全仓不重名 | 声明写在调用里 · 两份同名 ⇒ 红 |
//!
//! 盲区：写句柄不叫 `own` 的那一处 D1 看不见（今天全仓写句柄都叫 `own` / `self.own`）。

fn production_files() -> Vec<(String, String)> {
    let root = guard_core::addr::backend_src_root();
    guard_core::scan_tree_excluding(&root, &["rs"], &[])
        .into_iter()
        .map(|(p, src)| {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (rel, guard_core::production_code(&src))
        })
        .collect()
}

fn squeezed(src: &str) -> String {
    src.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 经 `Own` 写、却没有 `Doc` 声明的那几份。
fn writers_without_doc(files: &[(String, String)]) -> Vec<String> {
    files
        .iter()
        .filter(|(rel, _)| rel != "platform/fs/own.rs")
        .filter_map(|(rel, src)| {
            let s = squeezed(src);
            let writes = s.contains("own.write(") || s.contains("own.write_streamed(");
            let declares = s.contains("Doc::state(") || s.contains("Doc::cache(");
            (writes && !declares).then(|| rel.clone())
        })
        .collect()
}

/// 每一处声明：（文件, 落点那一格的原文）；不在 `const` 条目里的那几处进第二张表。
fn declarations(files: &[(String, String)]) -> (Vec<(String, String)>, Vec<String>) {
    let mut decls = vec![];
    let mut loose = vec![];
    for (rel, src) in files {
        if rel == "common/own_state.rs" {
            continue;
        }
        let s = squeezed(src);
        for ctor in ["Doc::state(", "Doc::cache("] {
            let mut from = 0;
            while let Some(i) = s[from..].find(ctor).map(|i| i + from) {
                from = i + ctor.len();
                let head = &s[..i];
                let item = head.rfind([';', '}', '{']).map_or(head, |j| &head[j + 1..]);
                let item = item.trim_start_matches(|c| c != 'c');
                if !(item.starts_with("const") || item.contains(")const"))
                    || !item.contains(":Doc<")
                {
                    loose.push(format!(
                        "{rel}：…{}",
                        &item[item.len().saturating_sub(60)..]
                    ));
                    continue;
                }
                let arg = s[from..].split(',').next().unwrap_or("").to_string();
                decls.push((rel.clone(), arg));
            }
        }
    }
    (decls, loose)
}

fn duplicated(decls: &[(String, String)]) -> Vec<String> {
    let mut seen = std::collections::BTreeMap::<&str, &str>::new();
    let mut dup = vec![];
    for (rel, arg) in decls {
        if let Some(first) = seen.insert(arg, rel) {
            dup.push(format!("{arg}：{first} · {rel}"));
        }
    }
    dup
}

#[test]
fn d1_every_file_written_through_own_declares_its_doc() {
    let files = production_files();
    let writers = files
        .iter()
        .filter(|(_, s)| squeezed(s).contains("own.write"))
        .count();
    assert!(
        writers >= 19,
        "经 Own 写的文件只扫到 {writers} 份：扫法坏了"
    );
    let missing = writers_without_doc(&files);
    assert!(
        missing.is_empty(),
        "经 Own 写、没有 Doc 声明（读上限 · 状态还是缓存说不清）：{missing:#?}"
    );
}

#[test]
fn d1_positive_control_a_writer_without_doc_is_caught() {
    let fake = |src: &str| vec![("x/y.rs".to_string(), src.to_string())];
    let bare = "fn f(own: &Own) { self.own\n    .write_streamed(&file, &held, |f| Ok(())); }";
    assert_eq!(writers_without_doc(&fake(bare)), vec!["x/y.rs".to_string()]);
    let declared = "const D: Doc<X> = Doc::cache(R, 1);\nfn f() { own.write(&f, &h, &b); }";
    assert!(writers_without_doc(&fake(declared)).is_empty());
}

#[test]
fn d2_docs_are_const_items_and_their_places_are_unique() {
    let (decls, loose) = declarations(&production_files());
    assert!(decls.len() >= 19, "声明只扫到 {} 份：扫法坏了", decls.len());
    assert!(loose.is_empty(), "Doc 声明不在 const 条目里：{loose:#?}");
    let dup = duplicated(&decls);
    assert!(dup.is_empty(), "两份声明落点同名：{dup:#?}");
}

#[test]
fn d2_positive_control_loose_and_duplicated_docs_are_caught() {
    let fake = |src: &str| vec![("x/y.rs".to_string(), src.to_string())];
    let (_, loose) = declarations(&fake("fn f() { let d: Doc<X> = Doc::state(R, 1); }"));
    assert_eq!(loose.len(), 1, "写在调用里的那一份没认出来");
    let (decls, loose) = declarations(&fake(
        "pub(crate) const A: Doc<X> = Doc::state(R, 1);\nconst B: Doc<Y> = Doc::cache(R, 2);",
    ));
    assert!(loose.is_empty(), "{loose:?}");
    assert_eq!(duplicated(&decls).len(), 1, "同名落点没认出来");
}
