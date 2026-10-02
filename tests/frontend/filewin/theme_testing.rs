//! 判据用的主题：照主界面 `tokens.css` 里那几格的缺省值解出来（与 monitor 开窗那一刻在缺省主题下交过去的同一份）。

use filewin_contract::{Theme, THEME_TOKENS};
use std::collections::BTreeMap;

/// 主界面的令牌表（两个前端包都住在 `src/frontend/` 下，按包目录取同一份）。
pub fn tokens_css() -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../ui/styles/tokens.css");
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("读不到 {p}：{e}"))
}

/// `tokens.css` 里名单上每一格的声明值（`  --name: value;`，取第一处）。
pub fn default_tokens() -> BTreeMap<String, String> {
    let css = tokens_css();
    THEME_TOKENS
        .iter()
        .map(|t| {
            let at = css
                .lines()
                .find_map(|l| l.trim().strip_prefix(&format!("{t}:")))
                .unwrap_or_else(|| panic!("tokens.css 里没有 {t}"));
            let v = at.split(';').next().unwrap_or("").trim().to_string();
            (t.to_string(), v)
        })
        .collect()
}

/// 缺省主题。
pub fn default_theme() -> Theme {
    Theme::from_tokens(&default_tokens()).expect("缺省令牌该解得出来")
}
