//! `P8a`：Claude Code **插件面**的只读枚举。
//!
//! 〔RM1b · 第四波〕**读这件事归后端了** —— 本机与远端同一条路：按 `origin` 问那台机器的后端
//! `plugins-marketplaces`（本体 `src/backend/observe/plugins_query.rs`，从本文件原样搬过去，
//! 三条出口与上限一字未改）。本文件只剩两件：命令入口 ＋ **线上形状的收口**
//! （[`parse_survey_lines`]：拒收未知字段、每个字段必填 ⇒ 两端一漂当场报错，不静默少一格）。
//! 下面「为什么不回答装了哪些」那段仍然成立（搬家不改口径），留着给读界面文案的人。
//!
//! # ★★ 本模块**不回答「装了哪些插件」**
//!
//! 摸底实测（08-12，本机）：`settings.json` 与 `~/.claude.json` 里 `plugin` / `marketplace`
//! **零命中**；`~/.claude` 下唯一提 marketplace 的是 `plugins/known_marketplaces.json`，
//! 而它记的是 **marketplace**（哪来的、装哪了、何时更新），**不是插件**。
//!
//! 而 `marketplaces/<id>/plugins/` 底下那些目录**不是用户安装的** —— 那个 marketplace 目录
//! 带 `.gcs-sha`、**不是 git 仓**，是一份**下载的快照**，`plugins/` 就是快照的内容。
//! 本机实测：manifest **声明 276 个**、快照目录里躺着 **39 个**。
//! ⇒ **一个显示「已装 39 个插件」的界面会在说假话。**
//!
//! 所以这里只回答三件能诚实读到的事：**有哪些 marketplace、从哪来、它声明了多少个插件**。
//! 「哪些插件真的在生效」是待决 `U10d`，本模块**不猜**。
//!
//! # 三条出口刻意分开（住后端那一份；本文件只把它们原样交给界面）
//!
//! | 情形 | 后端出口 | 界面 |
//! |---|---|---|
//! | `known_marketplaces.json` 不存在 | `file_absent: true` + 空表 | 「这台机器没有」 |
//! | 读/解析失败 | 整条命令 `failed` ⇒ 本命令回 `Err` | 「读不到……（这不等于「没有」）」 |
//! | 某条的插件数读不出 | 那条 `declared_plugins: null` + 理由 | 「读不到插件数：<理由>」，**不是 0** |
//!
//! # 只读
//!
//! 本进程一个字节都不读 `<claude_dir>`，更不写（`src/doc/INVARIANTS.md` 的只读铁律）。

use crate::origin::Origin;

/// 一个 marketplace。**每个字段读不出就是 `null`，不编默认值。**
///
/// 〔RM1b〕线上契约：**拒收未知字段、每个字段必填**（`Option` 也必须以 `null` 显式出现）——
/// 后端 `plugins_query::MarketplaceEntry` 多一格、少一格、改一个名，这里当场反序列化失败。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct MarketplaceEntry {
    pub id: String,
    /// 形如 `github:anthropics/claude-plugins-official`。
    #[serde(deserialize_with = "required")]
    pub source: Option<String>,
    #[serde(deserialize_with = "required")]
    pub install_location: Option<String>,
    #[serde(deserialize_with = "required")]
    pub last_updated: Option<String>,
    /// 这个 marketplace **声明**的插件数。
    ///
    /// ★★ `null` 的意思是**读不到**，**不是 0**。两者在界面上都容易被渲染成「0 个」，
    /// 所以类型上就分开，别指望渲染层记得住。
    #[serde(deserialize_with = "required")]
    pub declared_plugins: Option<u32>,
    /// `declared_plugins` 为 `null` 时**为什么**读不到 —— 装的就是那条错误原文。
    /// 一个不说原因的 `null` 等于没说：与 `byte_cap_registry::ALLOWED_SEMANTICS` 里
    /// 「降级+说清」那条的「说清」是同一个要求，**说清的通道就是这个字段**。
    #[serde(deserialize_with = "required")]
    pub declared_error: Option<String>,
}

/// 一次枚举的结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct MarketplaceSurvey {
    pub entries: Vec<MarketplaceEntry>,
    /// `known_marketplaces.json` **不存在** ⇒ 这台机器一个 marketplace 都没有（**诚实的空**）。
    /// 「读不到」走的是 `Err`，**到不了这里** —— 两条出口刻意不合并。
    pub file_absent: bool,
}

/// `Option<T>` 字段**必须在场**（可以是 `null`，不许缺席）。
///
/// serde 对 `Option` 的默认是「缺席 ⇒ `None`」—— 那会让后端少发一格时这里静默补成 `null`，
/// 界面上变成「读不到」而不是「两端契约对不上」。用了 `deserialize_with` 之后 serde 不再替它兜缺席。
fn required<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    <Option<T> as serde::Deserialize>::deserialize(d)
}

/// 后端那几行 → 一份 survey。**恰一行**；多一行、少一行、形状不对都是 `Err`（说清是契约对不上）。
pub fn parse_survey_lines(lines: &[String]) -> Result<MarketplaceSurvey, String> {
    match lines {
        [one] => serde_json::from_str(one)
            .map_err(|e| format!("插件市场清单的形状对不上（后端与界面版本不一致？）：{e}")),
        other => Err(format!(
            "插件市场清单应当恰好一行，收到 {} 行（后端与界面版本不一致？）",
            other.len()
        )),
    }
}

/// `P8a`：列出**那台机器**登记的 marketplace（**只读**）。
///
/// 〔RM1b〕收 `origin`：本机逐字 `"<local>"`，远端是那台的 label —— 两侧同一条路
/// （那台机器的后端 `plugins-marketplaces`）。`route` 只用来拦空白名。
#[tauri::command]
pub async fn list_plugin_marketplaces(origin: Origin) -> Result<MarketplaceSurvey, String> {
    let _ = origin.route("list_plugin_marketplaces")?;
    let lines = crate::backend::control::frame_query::lines(
        &origin,
        "plugins-marketplaces",
        serde_json::json!({}),
    )
    .await?;
    parse_survey_lines(&lines)
}

#[cfg(test)]
#[path = "../../../tests/bridge/plugins_tests.rs"]
mod tests;
