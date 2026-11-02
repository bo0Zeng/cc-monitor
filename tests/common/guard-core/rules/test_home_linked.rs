//! 每个有测试的包都链测试沙箱：`[dev-dependencies]` 里有 `test-home`，crate 根的测试段里有
//! `static TEST_HOME: fn() = test_home::linked;`。缺了哪一样 ⇒ `cargo test -p <那个包>` 用这台用户的真家。
//!
//! 人群：两棵 Cargo 工作区（`src/backend` · `src/frontend/shell`）的成员，`cargo metadata --no-deps --offline` 现取。
//! 有测试 ＝ 有 lib / bin 目标，且包目录（摘掉嵌在里面的别的成员）的源码里有 `#[test]` 或挂进 `tests/` 的 `#[path]`。
//! crate 根 ＝ lib 目标的 `src_path`（没有 lib ⇒ bin 的）。那一句只认测试段里的，注释里写的不算。没有豁免名单。

use super::{rule, Repo};
use std::path::Path;

/// 两棵工作区的根 manifest（仓根相对）。
const WORKSPACES: [&str; 2] = ["src/backend/Cargo.toml", "src/frontend/shell/Cargo.toml"];

/// crate 根测试段里要有的那一句（去掉空白比）。
const LINE: &str = "static TEST_HOME: fn() = test_home::linked;";

/// 一个成员：包名 · 包目录（仓根相对）· crate 根（仓根相对）· `[dev-dependencies]` 有没有 `test-home`。
#[derive(Debug)]
struct Member {
    name: String,
    dir: String,
    root_file: String,
    dev_test_home: bool,
}

fn rel_of(root: &Path, abs: &str) -> String {
    Path::new(abs)
        .strip_prefix(root)
        .unwrap_or_else(|_| panic!("`{abs}` 不在仓里"))
        .to_string_lossy()
        .replace('\\', "/")
}

/// 一棵工作区的成员（`cargo metadata` 现取）。
fn members(root: &Path, manifest: &str) -> Vec<Member> {
    let out = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(root.join(manifest))
        .output()
        .unwrap_or_else(|e| panic!("起不来 `cargo metadata`（{e}）—— 判不了"));
    assert!(
        out.status.success(),
        "`cargo metadata` 读 {manifest} 非零退出：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("`cargo metadata` 吐的不是 JSON");
    let packages = v["packages"]
        .as_array()
        .expect("`cargo metadata` 里没有 packages");
    let mut found = Vec::new();
    for p in packages {
        let name = p["name"].as_str().expect("包没有名字").to_string();
        let targets = p["targets"].as_array().expect("包没有 targets");
        // lib 目标的 kind 可能写成 `lib` · `rlib` · `cdylib` …（都以 `lib` 收尾）。
        let src_of = |is: fn(&str) -> bool| {
            targets
                .iter()
                .find(|t| {
                    t["kind"]
                        .as_array()
                        .is_some_and(|ks| ks.iter().filter_map(|k| k.as_str()).any(is))
                })
                .and_then(|t| t["src_path"].as_str())
                .map(str::to_string)
        };
        let Some(root_abs) = src_of(|k| k.ends_with("lib")).or_else(|| src_of(|k| k == "bin"))
        else {
            continue;
        };
        let manifest_abs = p["manifest_path"].as_str().expect("包没有 manifest_path");
        let dir = rel_of(root, manifest_abs)
            .trim_end_matches("Cargo.toml")
            .trim_end_matches('/')
            .to_string();
        let dev_test_home = p["dependencies"].as_array().is_some_and(|ds| {
            ds.iter()
                .any(|d| d["name"] == "test-home" && d["kind"] == "dev")
        });
        found.push(Member {
            name,
            dir,
            root_file: rel_of(root, &root_abs),
            dev_test_home,
        });
    }
    found
}

/// `dir` 这个包自己的 `.rs`（摘掉嵌在里面的别的成员）里有没有测试。
fn has_tests(repo: &Repo, dir: &str, nested: &[&str]) -> bool {
    repo.under(&[dir], &["rs"])
        .into_iter()
        .filter(|f| !nested.iter().any(|n| f.rel.starts_with(&format!("{n}/"))))
        .any(|f| {
            let code = crate::strip_comment_lines(f.text());
            code.contains("#[test]")
                || code
                    .lines()
                    .filter_map(crate::path_attr)
                    .any(crate::mounts_tests)
        })
}

/// 一段源码的测试段里有没有那一句：注释剥掉、空白不计；挂在 `#[cfg(test)]` 底下（它上面那几行属性里有），
/// 或住在测试模块里。生产段里那一句（没有 `cfg(test)`）不算。
fn linked_in(src: &str) -> bool {
    let squash = |s: &str| s.split_whitespace().collect::<String>();
    let want = squash(LINE);
    let code = crate::strip_comment_lines(src);
    if squash(&crate::test_source(&code)).contains(&want) {
        return true;
    }
    let lines: Vec<&str> = code.lines().collect();
    lines.iter().enumerate().any(|(i, l)| {
        squash(l) == want
            && lines[..i]
                .iter()
                .rev()
                .map(|a| a.trim())
                .take_while(|a| a.starts_with("#["))
                .any(|a| squash(a) == "#[cfg(test)]")
    })
}

/// 缺的那几样：`<包名>: <缺什么>`，一样一行。
fn missing(repo: &Repo, all: &[Member]) -> Vec<String> {
    let dirs: Vec<&str> = all.iter().map(|m| m.dir.as_str()).collect();
    let mut out = Vec::new();
    for m in all {
        let nested: Vec<&str> = dirs
            .iter()
            .copied()
            .filter(|d| *d != m.dir && d.starts_with(&format!("{}/", m.dir)))
            .collect();
        if !has_tests(repo, &m.dir, &nested) {
            continue;
        }
        if !m.dev_test_home {
            out.push(format!("{}: `[dev-dependencies]` 没有 `test-home`", m.name));
        }
        if !linked_in(repo.file(&m.root_file).text()) {
            out.push(format!(
                "{}: {} 的测试段里没有 `{LINE}`",
                m.name, m.root_file
            ));
        }
    }
    out
}

#[test]
fn every_member_with_tests_links_the_test_home() {
    rule(|repo| {
        let root = crate::addr::repo_root();
        let mut all: Vec<Member> = Vec::new();
        for w in WORKSPACES {
            for m in members(&root, w) {
                if !all.iter().any(|x| x.name == m.name) {
                    all.push(m);
                }
            }
        }
        // 人群自检：两棵工作区都读到了。
        for want in ["cc-monitor-backend", "monitor", "backend-common"] {
            assert!(
                all.iter().any(|m| m.name == want),
                "成员里没有 `{want}` —— 读 metadata 的口径坏了，判不了"
            );
        }
        let bad = missing(repo, &all);
        assert!(
            bad.is_empty(),
            "这几个包的测试会用这台用户的真家（缺测试沙箱）：\n  {}",
            bad.join("\n  ")
        );
    });
}

/// 正控：认法分得出「链了」与「只在注释里 · 只在生产段」。
#[test]
fn the_line_is_seen_only_on_the_test_side() {
    let linked = "fn a() {}\n#[cfg(test)]\n#[used]\nstatic TEST_HOME: fn() = test_home::linked;\n";
    let in_mod =
        "fn a() {}\n#[cfg(test)]\nmod t {\n    static TEST_HOME: fn() = test_home::linked;\n}\n";
    let commented = "fn a() {}\n#[cfg(test)]\n// static TEST_HOME: fn() = test_home::linked;\n";
    let prod = "fn a() {}\n#[used]\nstatic TEST_HOME: fn() = test_home::linked;\n";
    assert!(linked_in(linked), "`#[cfg(test)]` 底下那一句没认出来");
    assert!(linked_in(in_mod), "测试模块里那一句没认出来");
    assert!(!linked_in(commented), "注释里那一句被当成了链上");
    assert!(!linked_in(prod), "生产段里那一句被当成了链上");
}
