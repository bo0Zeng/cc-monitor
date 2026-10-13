//! 安装包带的策略文件（打包那一层）：原样带上、播种时原样写进策略目录。这里只是一张文件清单 —— 后端别处不认它们的名字或意思，
//! 选哪份、新会话用哪份都只经 `defaults.json`（第三版编号 `"1" "2" "3"` 对哪几份也记在那里）。

/// 随包的策略文件：（文件 id, 原文）。
pub(crate) const FILES: [(&str, &str); 3] = [
    (
        "list-order",
        include_str!("../../../../shared/rotation/strategies/list-order.rhai"),
    ),
    (
        "reset-first",
        include_str!("../../../../shared/rotation/strategies/reset-first.rhai"),
    ),
    (
        "most-left",
        include_str!("../../../../shared/rotation/strategies/most-left.rhai"),
    ),
];

/// `defaults.json`：`forNew`（没有 `rotation.json` 时新会话用哪份）· `fromV3`（第三版编号 1 2 3 依次对哪几份）。
const DEFAULTS: &str = include_str!("../../../../shared/rotation/strategies/defaults.json");

/// 读 `defaults.json`（随包、编进来的，读不懂就是包坏了 ⇒ 空）。
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Defaults {
    #[serde(default)]
    pub(crate) for_new: String,
    #[serde(default)]
    pub(crate) from_v3: Vec<String>,
}

pub(crate) fn defaults() -> Defaults {
    serde_json::from_str(DEFAULTS).unwrap_or_default()
}

/// 随包的那几份：（id, 原文）拥有的形。
pub(crate) fn files() -> Vec<(String, String)> {
    FILES
        .iter()
        .map(|(id, src)| ((*id).to_string(), (*src).to_string()))
        .collect()
}

/// 升级要的那一排：照 `fromV3` 的先后（第三版编号 1 2 3）。
pub(crate) fn for_upgrade() -> Vec<(String, String)> {
    let all = files();
    defaults()
        .from_v3
        .iter()
        .filter_map(|id| all.iter().find(|(x, _)| x == id).cloned())
        .collect()
}
