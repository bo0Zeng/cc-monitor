//! 第三方 API key 的明文只往下传登记过的那几跳；monitor 一侧零读零写（本机那一份的写者是本机常驻后端）。
//!
//! 调用面数的是两个具名方法的调用点，不是「明文」这个概念：经别的路径带出去（自定义类型、`Deref`）这里看不见，
//! 那一格由 `creds-core` 定义面的两条判据兜（有几个出口 · 类型没有第二个交出内层串的 impl）。

use super::{equal, none, rule};

/// 明文出口：（方法名，唯一调用点住的文件）。每个恰好一处调用。
const PLAINTEXT_EXIT_SITES: &[(&str, &str)] = &[
    (
        "expose_for_auth_header(",
        "src/backend/accounts/upstream_select/mod.rs",
    ),
    (
        "expose_for_persisting(",
        "src/common/creds-core/src/store.rs",
    ),
    (
        "expose_for_token_request(",
        "src/common/creds-core/src/token.rs",
    ),
];

/// 扫哪几棵树：这就是「每个出口恰好一处调用」的分母。
const PLAINTEXT_SCAN_TREES: &[&str] = &["src/frontend/shell/src", "src/common", "src/backend"];

#[test]
fn the_plaintext_exits_are_called_from_exactly_one_place_each_across_all_three_trees() {
    rule(|repo| {
        let mut found: Vec<(&str, String)> = Vec::new();
        for f in repo.under(PLAINTEXT_SCAN_TREES, &["rs"]) {
            let prod = f.prod();
            for (needle, _) in PLAINTEXT_EXIT_SITES {
                for (i, _) in prod.match_indices(needle) {
                    // 定义不是出口，调用才是。
                    if !prod[..i].trim_end().ends_with("fn") {
                        found.push((needle, f.rel.clone()));
                    }
                }
            }
        }
        let n = found.len();
        equal(
            "明文出口的调用点（每个恰好一处，住登记的那份）",
            found,
            PLAINTEXT_EXIT_SITES
                .iter()
                .map(|(n, home)| (*n, home.to_string())),
        );
        assert_eq!(n, PLAINTEXT_EXIT_SITES.len(), "有出口的调用点不止一处");
    });
}

/// 定义面（`creds-core` 判据表 `INNER_FIELD_USERS` 里标 `HandsOut` 的）与调用面（上面那张）是同一组名字：
/// 新出口要绿，两张表得一起改，而调用面一改，上一条就要它恰好一处调用。
#[test]
fn the_definition_table_and_the_call_site_table_name_the_same_exits() {
    rule(|repo| {
        let core = repo.file("tests/common/creds-core/lib_tests.rs").text();
        let at = crate::find_pinned(core, "const INNER_FIELD_USERS:").expect("切不出定义面那张表");
        let table = core[at..]
            .find("\n];")
            .map(|i| &core[at..at + i])
            .expect("切不出表体（找不到 `];` 收尾）");
        let mut hands_out: Vec<String> = Vec::new();
        for (i, _) in table.match_indices("Handling::HandsOut") {
            let before = &table[..i];
            let Some(q_end) = before.rfind('"') else {
                continue;
            };
            let Some(q_start) = before[..q_end].rfind('"') else {
                continue;
            };
            hands_out.push(before[q_start + 1..q_end].to_string());
        }
        equal(
            "定义面标 `HandsOut` 的 · 调用面登记的",
            hands_out,
            PLAINTEXT_EXIT_SITES
                .iter()
                .map(|(n, _)| n.trim_end_matches('(').to_string()),
        );
    });
}

/// monitor 开着 `harden`（Windows 上读 DACL 要它），编译器兜不住「monitor 写不了这份文件」—— 由本条兜。
#[test]
fn gp1_the_monitor_never_reaches_the_credentials_write_half() {
    let needle = format!("create_{}(", "private");
    rule(|repo| {
        let hits: Vec<&str> = repo
            .crate_tree("src/frontend/shell", &["rs"])
            .into_iter()
            .filter(|f| crate::strip_comment_lines(f.prod()).contains(&needle))
            .map(|f| f.rel.as_str())
            .collect();
        // 正控：后端自有状态文件的原子写那一处真在用它。
        let control = repo
            .file("src/backend/platform/fs/own.rs")
            .prod()
            .matches(&needle)
            .count();
        none("monitor 生产段够到了凭据文件的写半边", hits, control);
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
