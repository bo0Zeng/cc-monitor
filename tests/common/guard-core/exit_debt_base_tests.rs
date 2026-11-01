//! 退出债名单（`tests/frontend/ui/exit-debt.json`）的上限只许变小：此刻的 `ceiling` 不比基线上那一版（[`crate::baseline_text`]，与 main 分叉点）大。
//!
//! 名单自身（每格的数 · 合计 == `ceiling` · 句名 · 文件在盘上）在 `tests/frontend/ui/exit-judgment-scan.vitest.ts` 里判；
//! 「对着基线只许变短」与别的存量名单一样只在这里判，基线读法只此一份。

use std::path::Path;

const DEBT_PATH: &str = "tests/frontend/ui/exit-debt.json";

/// 名单正文里顶层那一格 `"ceiling": <数>`；没有那一格（不是这一形的名单）⇒ `None`。
fn ceiling_of(text: &str) -> Option<u64> {
    let at = text.find("\"ceiling\"")? + "\"ceiling\"".len();
    let rest = text[at..].trim_start().strip_prefix(':')?.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// 此刻那一版比基线那一版的上限大 ⇒ 那一句。基线上没有这份 · 没有那一格 · 不是按格分组的名单（没有 `"groups"`，
/// 换成两句之前的旧形，不是同一张名单）⇒ 没有可比的上限，过。
fn grown(now: &str, base: Option<&str>) -> Option<String> {
    let now_c = ceiling_of(now).unwrap_or_else(|| panic!("{DEBT_PATH} 里读不出 ceiling"));
    let base_c = base
        .filter(|b| b.contains("\"groups\""))
        .and_then(ceiling_of)?;
    (now_c > base_c).then(|| format!("ceiling {now_c} 比基线上的 {base_c} 大 —— 名单只许删"))
}

#[test]
fn the_ceiling_never_grows_past_the_baseline() {
    let root: &Path = &crate::addr::repo_root();
    let now = std::fs::read_to_string(root.join(DEBT_PATH))
        .unwrap_or_else(|e| panic!("{DEBT_PATH} 读不动：{e}"));
    let base = crate::baseline_text(root, DEBT_PATH)
        .unwrap_or_else(|e| panic!("`{DEBT_PATH}` 读不到基线：{e}"));
    assert_eq!(grown(&now, base.as_deref()), None);
}

/// 判法自己：长了 ⇒ 红；持平 · 变小 ⇒ 过；基线上没有这份或没有那一格 ⇒ 过；读法认得出空白与别处的同名词。
#[test]
fn growing_is_red_and_shrinking_or_no_baseline_is_not() {
    let at = |n: u64| {
        format!(
            "{{\n  \"why\": \"ceiling = 各格之和\",\n  \"ceiling\" :  {n},\n  \"groups\": []\n}}\n"
        )
    };
    assert!(grown(&at(11), Some(&at(10))).is_some());
    assert_eq!(grown(&at(10), Some(&at(10))), None);
    assert_eq!(grown(&at(9), Some(&at(10))), None);
    assert_eq!(grown(&at(9), None), None);
    assert_eq!(grown(&at(9), Some("{\"groups\": []}")), None);
    assert_eq!(
        grown(&at(500), Some("{\"rows\": [], \"ceiling\": 121}")),
        None,
        "旧形的名单不是同一张"
    );
    assert_eq!(ceiling_of(&at(1712)), Some(1712));
}
