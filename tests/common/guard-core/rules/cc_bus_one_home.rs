//! cc-bus 的写面只有一条路（`cc-send`）：monitor 不拼 cc-bus 脚本的 shell 串（只经后端的 `bus-*` 读），
//! 界面说 `bus-*` 那几条只经 `src/frontend/ui/cc-bus-control.ts` 一处。

use super::{equal, none, rule};

/// cc-bus 写面的脚本名（现拼，免得本文件自己成了别的扫描的命中）。
fn verbs() -> Vec<String> {
    ["send"].iter().map(|v| format!("cc-{v}")).collect()
}

/// 那份生产段里提到的脚本名。
fn named(prod: &str) -> Vec<String> {
    let prod = crate::strip_comment_lines(prod);
    verbs()
        .into_iter()
        .filter(|v| crate::contains_word(&prod, v))
        .collect()
}

#[test]
fn the_monitor_has_no_cc_bus_write_path_any_more() {
    rule(|repo| {
        let found: Vec<String> = repo
            .crate_tree("src/frontend/shell", &["rs"])
            .into_iter()
            .flat_map(|f| {
                named(f.prod())
                    .into_iter()
                    .map(move |v| format!("{} ({v})", f.rel))
            })
            .collect();
        // 正控：发信那条脚本自己的用法行里有它的名字，同一个认法认得出。
        let control = named(repo.file("src/shared/cc-bus/scripts/cc-send").prod()).len();
        none(
            "monitor 生产段里提到 cc-bus 写面脚本名（monitor 不发信，只经后端的 `bus-*` 读）",
            found,
            control,
        );
    });
}

#[test]
fn the_front_end_speaks_the_bus_ops_only_through_one_module() {
    const HOME: &str = "src/frontend/ui/cc-bus-control.ts";
    const OPS: &[&str] = &["bus-list", "bus-inbox"];
    rule(|repo| {
        let mut found: Vec<(String, &str, usize)> = Vec::new();
        for f in repo.under(&["src"], &["ts"]) {
            let text = crate::strip_comment_lines(f.text());
            for op in OPS {
                let n: usize = [
                    format!("chan.call(origin, \"{op}\""),
                    format!("callTyped(origin, \"{op}\""),
                ]
                .iter()
                .map(|needle| text.matches(needle.as_str()).count())
                .sum();
                if n > 0 {
                    found.push((f.rel.clone(), op, n));
                }
            }
        }
        equal(
            "界面说 `bus-*` 的调用（`chan.call` / `callTyped`）各恰好一处、只在 cc-bus-control.ts",
            found,
            OPS.iter().map(|op| (HOME.to_string(), *op, 1)),
        );
    });
}
