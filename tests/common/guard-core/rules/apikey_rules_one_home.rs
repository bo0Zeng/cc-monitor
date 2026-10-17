//! 「configDir → 表里的 id」与「哪几个号在表里有行」两条规则只有一个家：`acct-core`。

use super::{equal, rule};

const DEFS: &[&str] = &["fn apikey_account_id_of_dir", "fn apikey_routed_subset"];

#[test]
fn the_two_apikey_rules_are_defined_only_in_acct_core() {
    rule(|repo| {
        let mut found = Vec::new();
        for f in repo.under(
            &["src/frontend/shell/src", "src/backend", "src/common"],
            &["rs"],
        ) {
            for d in DEFS {
                if crate::contains_word(f.prod(), d) {
                    found.push((f.rel.clone(), *d));
                }
            }
        }
        equal(
            "两条 apikey 规则的定义",
            found,
            DEFS.iter()
                .map(|d| ("src/common/acct-core/src/lib.rs".to_string(), *d)),
        );
    });
}
