//! 壳给界面的「本机能力」：编它的那个平台那一行与跨语言金样逐格相等；注入的那一句就是那一行的 JSON。

use super::*;

#[test]
fn the_facts_of_this_platform_match_the_golden_row() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../../__fixtures__/host-facts.golden.json")).unwrap();
    let mine = serde_json::to_value(host_facts()).unwrap();
    assert_eq!(
        g.get(host_core::OS),
        Some(&mine),
        "这个平台（{}）那一行与金样对不上：{mine}",
        host_core::OS
    );
    let s = init_script();
    let json = s
        .strip_prefix("window.__CCM_HOST__ = ")
        .and_then(|r| r.strip_suffix(';'))
        .unwrap_or_else(|| panic!("起页脚本不是那一形：{s}"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(json).unwrap(),
        mine
    );
}
