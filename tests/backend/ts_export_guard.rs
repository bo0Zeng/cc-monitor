//! 🔴 ts-rs 生成物的落点与出处（第 8 刀收尾）。
//!
//! ts-rs 的相对落点（`export_to = "../.."`）相对的是**跑导出的那个 crate**（`<它的目录>/bindings/`），不是类型住的那个 crate：
//! 别的 crate 的测试档把一个类型当依赖导出时，相对落点就落到别处（`cmd_wire::Machine` 曾经因此往仓根多导出一个 `frontend/`）。
//! 生成物也按「落点目录 ＋ TS 名」一份文件：两个类型同名就悄悄互相盖掉（`ForkFrom` 那一次）。
//!
//! 这里读全仓 `src/` 里每个 `derive(ts_rs::TS)`，现算它落在哪、叫什么：
//! - 落点只许是 [`REGISTERED`] 那几处；没写落点（落进 `bindings/`）· 认不出的写法 ⇒ 红；
//! - 住在会被别的 crate 经 feature 引用的 crate 里（Cargo.toml 里 ts-rs 是 feature 开的 `dep:ts-rs`）⇒ 落点必须写成
//!   `concat!(env!("CARGO_MANIFEST_DIR"), "…")`，相对的直接红；
//! - 同一个落点目录里同一个 TS 名只许一个出处。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// 登记的生成物目录（仓根相对）。都叫 `generated`：`tests/scripts/generated-drift.sh` 按这个名字找没提交的生成物（判据在 `flow_scripts_guard`）。
pub(crate) const REGISTERED: &[&str] = &["src/frontend/ui/generated", "src/comms/ui/generated"];

/// 一个 `derive(ts_rs::TS)` 的类型。
#[derive(Debug)]
struct Derived {
    /// 仓根相对的源文件 ＋ 类型名（报错用）。
    at: String,
    /// TS 名（`ts(rename)` ＞ `serde(rename)` ＞ 类型名）。
    ts_name: String,
    /// 落点：`Ok(仓根相对目录)` 或 `Err(为什么不行)`。
    dir: Result<String, String>,
}

/// 把 `a/b/../c/./d/` 规整成 `a/c/d`（只做字面规整，不碰盘）。
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 一份源文件所在的 crate 目录（往上找第一个带 `[package]` 的 Cargo.toml）。
fn crate_dir_of(file: &Path) -> Option<PathBuf> {
    let mut d = file.parent()?;
    loop {
        let toml = d.join("Cargo.toml");
        if std::fs::read_to_string(&toml).is_ok_and(|t| t.contains("[package]")) {
            return Some(d.to_path_buf());
        }
        d = d.parent()?;
    }
}

/// 这个 crate 的 TS 派生会不会被别的 crate 经 feature 引用（那边导出时把它当依赖，相对落点就跟着那边走）。
fn exported_by_others(crate_dir: &Path) -> bool {
    std::fs::read_to_string(crate_dir.join("Cargo.toml")).is_ok_and(|t| t.contains("\"dep:ts-rs\""))
}

/// 一段文本里 `key = "…"`（`key` 前后是词边界、`=` 之后是字符串字面量）的值。
fn str_value(window: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(off) = window[from..].find(key) {
        let at = from + off;
        from = at + key.len();
        let before_ok = at == 0 || {
            let c = window.as_bytes()[at - 1];
            !(c.is_ascii_alphanumeric() || c == b'_')
        };
        let rest = window[at + key.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        if let Some(body) = rest.strip_prefix('"') {
            return body.find('"').map(|e| body[..e].to_string());
        }
    }
    None
}

/// 落点那一格：`export_to = <这里>`，交回原样那一段（到下一个顶层逗号或右括号为止）。
fn export_to_expr(window: &str) -> Option<String> {
    let at = window.find("export_to")?;
    let rest = window[at + "export_to".len()..]
        .trim_start()
        .strip_prefix('=')?;
    let (mut depth, mut out) = (0i32, String::new());
    for c in rest.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => break,
            ')' => depth -= 1,
            ',' if depth == 0 => break,
            _ => {}
        }
        out.push(c);
    }
    Some(out.chars().filter(|c| !c.is_whitespace()).collect())
}

/// 算一个派生落在哪（仓根相对）。
fn resolve(repo: &Path, crate_dir: &Path, window: &str) -> Result<String, String> {
    let expr = export_to_expr(window)
        .ok_or_else(|| "没写 `export_to`（会落进跑导出那个 crate 的 `bindings/`）".to_string())?;
    let abs = if let Some(lit) = expr
        .strip_prefix("concat!(env!(\"CARGO_MANIFEST_DIR\"),\"")
        .and_then(|r| r.strip_suffix("\")"))
    {
        crate_dir.join(lit.trim_start_matches('/'))
    } else if let Some(lit) = expr.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        if exported_by_others(crate_dir) {
            return Err(format!(
                "相对落点 `{lit}`：这个 crate 的类型会被别的 crate 当依赖导出，相对落点就跟着那边走 ⇒ 写成 `concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/…\")`"
            ));
        }
        crate_dir.join("bindings").join(lit)
    } else {
        return Err(format!("认不出的落点写法 `{expr}`"));
    };
    let abs = normalize(&abs);
    let rel = abs
        .strip_prefix(repo)
        .map_err(|_| format!("落到仓外：{}", abs.display()))?
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    if REGISTERED.contains(&rel.as_str()) {
        Ok(rel)
    } else {
        Err(format!("落到没登记的 `{rel}`（登记的：{REGISTERED:?}）"))
    }
}

/// 一份源文件（生产段）里的每个 `derive(ts_rs::TS)`。
fn derived_in(repo: &Path, file: &Path, src: &str) -> Vec<Derived> {
    let rel = file
        .strip_prefix(repo)
        .unwrap_or(file)
        .display()
        .to_string();
    let Some(crate_dir) = crate_dir_of(file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (at, _) in src.match_indices("derive(ts_rs::TS)") {
        // 窗口：从这一处到类型名（容器那几格属性都在里面；字段上的不在）。
        let rest = &src[at..];
        let item = ["struct ", "enum "]
            .iter()
            .filter_map(|k| {
                rest.match_indices(k)
                    .find(|(i, _)| *i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphanumeric())
                    .map(|(i, _)| (i, k.len()))
            })
            .min();
        let Some((i, klen)) = item else {
            out.push(Derived {
                at: rel.clone(),
                ts_name: String::new(),
                dir: Err("找不到它派生在哪个类型上".into()),
            });
            continue;
        };
        let ident: String = rest[i + klen..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let window = &rest[..i];
        let ts_attr = window.find("ts(").map(|t| &window[t..]).unwrap_or("");
        let ts_name = str_value(ts_attr, "rename")
            .or_else(|| {
                window
                    .find("serde(")
                    .and_then(|s| str_value(&window[s..], "rename"))
            })
            .unwrap_or_else(|| ident.clone());
        out.push(Derived {
            at: format!("{rel} · {ident}"),
            ts_name,
            dir: resolve(repo, &crate_dir, window),
        });
    }
    out
}

fn all_derived() -> Vec<Derived> {
    let repo = guard_core::addr::repo_root();
    let src = repo.join("src");
    let files = guard_core::scan_tree!(&src, &["rs"]);
    let out: Vec<Derived> = files
        .iter()
        .filter(|(p, _)| !p.starts_with(src.join("vendor")))
        .flat_map(|(p, raw)| derived_in(&repo, p, &guard_core::production_code(raw)))
        .collect();
    // 防空转：全仓几百个导出类型，后端 · 壳 · cmd-wire 三处都得扫到。
    assert!(
        out.len() > 300,
        "只扫到 {} 个 TS 派生 —— 遍历坏了",
        out.len()
    );
    for c in [
        "src/backend/",
        "src/frontend/shell/",
        "src/common/cmd-wire/",
    ] {
        assert!(
            out.iter().any(|d| d.at.starts_with(c)),
            "{c} 一个 TS 派生都没扫到"
        );
    }
    out
}

/// 🔴 每个 TS 派生的落点都是登记的那几处（相对落点只许用在不会被别的 crate 当依赖导出的 crate 里）。
#[test]
fn every_ts_export_lands_in_a_registered_directory() {
    let bad: Vec<String> = all_derived()
        .into_iter()
        .filter_map(|d| d.dir.err().map(|why| format!("  {}：{why}", d.at)))
        .collect();
    assert!(
        bad.is_empty(),
        "这些 TS 派生的落点不对（{} 处）：\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// 🔴 同一个生成物目录里，同一个 TS 名只许一个出处（不然后写的那份悄悄盖掉先写的）。
#[test]
fn every_generated_type_name_has_one_source() {
    let mut by: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for d in all_derived() {
        if let Ok(dir) = d.dir {
            by.entry((dir, d.ts_name)).or_default().push(d.at);
        }
    }
    let dup: Vec<String> = by
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|((dir, n), v)| format!("  {dir}/{n}.ts ⇐ {}", v.join(" ｜ ")))
        .collect();
    assert!(
        dup.is_empty(),
        "同名的生成物不止一个出处：\n{}",
        dup.join("\n")
    );
}

/// 正控：落点 · 名字的现算在合成文本上对。
#[test]
fn the_export_ruler_works_on_synthetic_text() {
    let repo = guard_core::addr::repo_root();
    let backend = repo.join("src/backend");
    let wire = repo.join("src/common/cmd-wire");
    let w = |s: &str| format!("#[cfg_attr(test, {s})]\n");
    assert_eq!(
        resolve(
            &repo,
            &backend,
            &w("ts(export, export_to = \"../../frontend/ui/generated/\")")
        ),
        Ok("src/frontend/ui/generated".into())
    );
    assert!(resolve(
        &repo,
        &backend,
        &w("ts(export, export_to = \"../../../frontend/ui/generated/\")")
    )
    .is_err());
    assert!(resolve(&repo, &backend, &w("ts(export)")).is_err());
    // 会被别的 crate 当依赖导出的那一个：相对的红、绝对的过。
    assert!(resolve(
        &repo,
        &wire,
        &w("ts(export, export_to = \"../../../frontend/ui/generated/\")")
    )
    .is_err());
    assert_eq!(
        resolve(
            &repo,
            &wire,
            &w("ts(export, export_to = concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../frontend/ui/generated/\"))")
        ),
        Ok("src/frontend/ui/generated".into())
    );
    let file = backend.join("x.rs");
    let got = derived_in(
        &repo,
        &file,
        "#[derive(ts_rs::TS)]\n#[ts(export, rename = \"Y\", export_to = \"../../frontend/ui/generated/\")]\npub(crate) struct X { #[ts(rename = \"z\")] a: u8 }\n\
         #[derive(ts_rs::TS)]\n#[serde(rename_all = \"camelCase\")]\n#[ts(export, export_to = \"../../frontend/ui/generated/\")]\nenum Q { A }\n",
    );
    let names: Vec<&str> = got.iter().map(|d| d.ts_name.as_str()).collect();
    assert_eq!(names, ["Y", "Q"]);
}
