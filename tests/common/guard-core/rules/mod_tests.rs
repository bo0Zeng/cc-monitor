//! 入口那几样内建的：合成样本上逐样看红。

use super::*;

fn red(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let e = std::panic::catch_unwind(f).expect_err("该红没红");
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

fn sample() -> Repo {
    let attr = format!("#[{}]", "test");
    Repo::synthetic(&[
        ("src/a/Cargo.toml", "[package]\nname = \"a\"\n\n[package.metadata.guard]\npopulation = [\"../b\"]\n"),
        ("src/a/src/lib.rs", "#[path = \"../../c/mod.rs\"]\nmod c;\n#[path = \"../../../tests/a/x_tests.rs\"]\nmod x_tests;\nfn a() {}\n"),
        ("src/b/Cargo.toml", "[package]\nname = \"b\"\n"),
        ("src/b/lib.rs", "fn b() {}\n"),
        ("src/c/mod.rs", "fn c() {}\n"),
        ("src/c/d.rs", "fn d() {}\n"),
        ("src/leak.rs", &format!("fn x() {{}}\n{attr}\nfn t() {{}}\n")),
        ("tests/a/x_tests.rs", &format!("{attr}\nfn t() {{}}\n")),
        ("tests/common/guard-core/rules/r.rs", "fn r() {}\n"),
        ("tests/common/guard-core/k.rs", "fn k() {}\n"),
    ])
}

fn rels(v: Vec<&File>) -> Vec<&str> {
    v.into_iter().map(|f| f.rel.as_str()).collect()
}

#[test]
fn an_empty_population_is_red() {
    let r = sample();
    assert!(red(move || {
        r.under(&["src/nowhere"], &["rs"]);
    })
    .contains("空人群"));
}

#[test]
fn the_rules_directory_is_out_of_every_population_unless_asked() {
    let r = sample();
    assert_eq!(
        rels(r.under(&["tests/common/guard-core"], &["rs"])),
        ["tests/common/guard-core/k.rs"]
    );
    assert_eq!(
        rels(r.with_rules().under(&["tests/common/guard-core"], &["rs"])),
        [
            "tests/common/guard-core/k.rs",
            "tests/common/guard-core/rules/r.rs"
        ]
    );
}

#[test]
fn a_named_file_that_is_not_there_is_red() {
    let r = sample();
    assert_eq!(r.file("src/b/lib.rs").text(), "fn b() {}\n");
    assert!(red(move || {
        r.file("src/gone.rs");
    })
    .contains("住址坏了"));
}

#[test]
fn a_crate_tree_takes_its_population_and_its_path_mounts_but_not_tests() {
    let r = sample();
    assert_eq!(
        rels(r.crate_tree("src/a", &["rs"])),
        [
            "src/a/src/lib.rs",
            "src/b/lib.rs",
            "src/c/d.rs",
            "src/c/mod.rs"
        ]
    );
}

#[test]
fn a_production_side_that_still_has_tests_is_red() {
    let r = sample();
    assert_eq!(r.file("src/c/d.rs").prod(), "fn d() {}");
    assert_eq!(r.file("tests/a/x_tests.rs").prod(), "");
    assert!(red(move || {
        r.file("src/leak.rs").prod();
    })
    .contains("src/leak.rs"));
}

#[test]
fn equal_is_two_way_and_names_both_sides() {
    let msg = red(|| equal("样", ["a", "b"], ["b", "c"]));
    assert!(msg.contains("\"a\"") && msg.contains("\"c\""), "{msg}");
    equal("样", ["b", "a"], ["a", "b"]);
    assert!(red(|| equal("样", ["a"], Vec::<&str>::new())).contains("登记表是空的"));
}

#[test]
fn none_needs_a_positive_control() {
    assert!(red(|| none("样", Vec::<&str>::new(), 0)).contains("正控"));
    assert!(red(|| none("样", ["x"], 1)).contains("\"x\""));
    none("样", Vec::<&str>::new(), 1);
}
