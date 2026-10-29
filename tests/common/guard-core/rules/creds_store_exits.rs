//! 第三方 API key 的明文只往下传登记过的那几跳；monitor 一侧零读零写（本机那一份的写者是本机常驻后端）。
//!
//! 调用面数的是具名出口方法的调用点，不是「明文」这个概念：经别的路径带出去（自定义类型、`Deref`）这里看不见，
//! 那一格由 `creds-core` 定义面的判据兜（有几个出口 · 类型没有第二个交出内层串的 impl）。
//! monitor 够不到凭据文件的写半边由编译器挡：写半边在 `harden` feature 后面，monitor 不开（`creds_guard.rs` 钉 manifest）。

use super::{equal, none, rule};

/// 定义面那张表（`creds-core` 判据 `INNER_FIELD_USERS`）住在哪。
const DEFINITION_TABLE: &str = "tests/common/creds-core/lib_tests.rs";

/// 扫哪几棵树：这就是「每个出口恰好一处调用」的分母。
const PLAINTEXT_SCAN_TREES: &[&str] = &["src/frontend/shell/src", "src/common", "src/backend"];

/// 定义面标 `Handling::HandsOut("<唯一调用点住的文件>")` 的每一行 ⇒ `(方法名, 那份文件)`。
fn plaintext_exit_sites(core: &str) -> Vec<(String, String)> {
    let at = crate::find_pinned(core, "const INNER_FIELD_USERS:").expect("切不出定义面那张表");
    let table = core[at..]
        .find("\n];")
        .map(|i| &core[at..at + i])
        .expect("切不出表体（找不到 `];` 收尾）");
    let mark = "Handling::HandsOut(\"";
    let mut out = Vec::new();
    for (i, _) in table.match_indices(mark) {
        let before = &table[..i];
        let name = before
            .rfind('"')
            .and_then(|q_end| {
                before[..q_end]
                    .rfind('"')
                    .map(|q_start| &before[q_start + 1..q_end])
            })
            .expect("`HandsOut` 前面切不出方法名");
        let rest = &table[i + mark.len()..];
        let home = &rest[..rest.find('"').expect("`HandsOut(\"…` 没有收尾引号")];
        out.push((name.to_string(), home.to_string()));
    }
    out
}

#[test]
fn the_plaintext_exits_are_called_from_exactly_one_place_each_across_all_three_trees() {
    rule(|repo| {
        let sites = plaintext_exit_sites(repo.file(DEFINITION_TABLE).text());
        assert!(
            sites.len() >= 2,
            "定义面里只读出 {} 个 `HandsOut` —— 取法坏了，本条会零命中地绿：{sites:?}",
            sites.len()
        );
        let mut found: Vec<(String, String)> = Vec::new();
        for f in repo.under(PLAINTEXT_SCAN_TREES, &["rs"]) {
            let prod = f.prod();
            for (name, _) in &sites {
                let needle = format!("{name}(");
                for (i, _) in prod.match_indices(&needle) {
                    // 定义不是出口，调用才是。
                    if !prod[..i].trim_end().ends_with("fn") {
                        found.push((name.clone(), f.rel.clone()));
                    }
                }
            }
        }
        let n = found.len();
        let want = sites.len();
        equal(
            "明文出口的调用点（每个恰好一处，住定义面登记的那份）",
            found,
            sites,
        );
        assert_eq!(n, want, "有出口的调用点不止一处");
    });
}

/// 界面经通道 `apikey-key-set` 交那台的后端；monitor 只转不透明字节，一处都不许叫出那条写口。
#[test]
fn hx2_the_monitor_names_no_plaintext_key_on_the_way_to_the_backend() {
    let needles = [
        format!("write_apikey_{}_key", "credentials"),
        format!("\"apikey-{}-set\"", "key"),
    ];
    let named = |prod: &str| -> usize {
        prod.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .map(|l| needles.iter().filter(|n| l.contains(n.as_str())).count())
            .sum()
    };
    rule(|repo| {
        let hits: Vec<&str> = repo
            .crate_tree("src/frontend/shell", &["rs"])
            .into_iter()
            .filter(|f| named(f.prod()) > 0)
            .map(|f| f.rel.as_str())
            .collect();
        // 正控：后端那条帧命令的宿主里认得出。
        let control = named(repo.file("src/backend/faces/accounts_face.rs").prod());
        none("monitor 生产段又能叫出明文 key 的写口", hits, control);
        equal(
            "前端发 apikey-key-set 的地方",
            repo.under(&["src/frontend/ui"], &["ts"])
                .into_iter()
                .filter(|f| !f.rel.starts_with("src/frontend/ui/generated/"))
                .flat_map(|f| {
                    let n = f.text().matches(needles[1].as_str()).count();
                    (n > 0).then(|| (f.rel.clone(), n))
                }),
            [("src/frontend/ui/apikey-reads.ts".to_string(), 1)],
        );
    });
}
