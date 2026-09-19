//! `P8a`：Claude Code **插件面**的只读枚举。
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
//! # 三条出口刻意分开
//!
//! | 情形 | 出口 | 为什么不能合并 |
//! |---|---|---|
//! | `known_marketplaces.json` 不存在 | `file_absent: true` + 空表 | 这是**诚实的空**：这台机器确实一个都没有 |
//! | 读/解析失败 | 整条命令回 `Err` | 回空表 ⇒ 与上一行**长得一模一样**，用户看到「没有」其实是「读不到」 |
//! | 某条的插件数读不出 | 那条 `declared_plugins: null` + 理由 | 回 `0` ⇒ 同上，而且**只坏一条不该毁掉整张表** |
//!
//! 中间那条正是本仓一路在收的那一族（`U3` 的「那个 0 是瞎的」· `P6b` 的「读失败说成没用过」·
//! `P4d-Y5` 的「未找到远端配置」）。
//!
//! # 只读
//!
//! 全程只 `read`，一个字节都不往 `<claude_dir>` 写（`src/doc/INVARIANTS.md` 的只读铁律）。
//! 连 `.gcs-sha` 都不解释、不校验 —— 那是上游下载器的账本，不是我们的。

use std::path::Path;

/// 读 `known_marketplaces.json` 的上限。**硬报错**：这份文件本该只有几百字节
/// （本机实测 **206 字节**），大到 4 MB 说明盘上的东西已经不是我们以为的那个了，
/// 与其猜着解析不如停下来说清。
pub(crate) const KNOWN_MARKETPLACES_CAP: u64 = 4 * 1024 * 1024;

/// 读单个 marketplace manifest 的上限。本机实测那份 **161 KB / 声明 276 个插件**，
/// 32 MB 留了两个数量级的余量。超了走**降级+说清**（那一行的插件数变 `null` + 理由），
/// **不让一份坏 manifest 毁掉整张表** —— 同 `local_accounts::MANIFEST_CAP` 的取舍。
pub(crate) const MARKETPLACE_MANIFEST_CAP: u64 = 32 * 1024 * 1024;

/// 一个 marketplace。**每个字段读不出就是 `null`，不编默认值。**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct MarketplaceEntry {
    pub id: String,
    /// 形如 `github:anthropics/claude-plugins-official`。
    pub source: Option<String>,
    pub install_location: Option<String>,
    pub last_updated: Option<String>,
    /// 这个 marketplace **声明**的插件数。
    ///
    /// ★★ `null` 的意思是**读不到**，**不是 0**。两者在界面上都容易被渲染成「0 个」，
    /// 所以类型上就分开，别指望渲染层记得住。
    pub declared_plugins: Option<u32>,
    /// `declared_plugins` 为 `null` 时**为什么**读不到 —— 装的就是那条错误原文。
    /// 一个不说原因的 `null` 等于没说：与 `byte_cap_registry::ALLOWED_SEMANTICS` 里
    /// 「降级+说清」那条的「说清」是同一个要求，**说清的通道就是这个字段**。
    pub declared_error: Option<String>,
}

/// 一次枚举的结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct MarketplaceSurvey {
    pub entries: Vec<MarketplaceEntry>,
    /// `known_marketplaces.json` **不存在** ⇒ 这台机器一个 marketplace 都没有（**诚实的空**）。
    /// 「读不到」走的是 `Err`，**到不了这里** —— 两条出口刻意不合并。
    pub file_absent: bool,
}

/// 带上限地读一个文件。`Ok(None)` = **文件不存在**（调用方自己决定那算不算错）。
fn read_capped(path: &Path, cap: u64) -> Result<Option<String>, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读不到 `{}`：{e}", path.display())),
    };
    if meta.len() > cap {
        return Err(format!(
            "`{}` 有 {} 字节，超过上限 {cap} ⇒ 不读",
            path.display(),
            meta.len()
        ));
    }
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) => Err(format!("读不到 `{}`：{e}", path.display())),
    }
}

/// 解析 `known_marketplaces.json`。形状：`{ "<id>": { source:{source,repo}, installLocation, lastUpdated } }`。
///
/// ⚠ 解析失败**回 `Err`**，不回空表 —— 见模块头注那张三出口表。
fn parse_known_marketplaces(raw: &str) -> Result<Vec<MarketplaceEntry>, String> {
    let v: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| format!("`known_marketplaces.json` 解析失败：{e}"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| "`known_marketplaces.json` 的顶层不是一个对象".to_string())?;
    let mut out: Vec<MarketplaceEntry> = obj
        .iter()
        .map(|(id, val)| MarketplaceEntry {
            id: id.clone(),
            source: describe_source(val.get("source")),
            install_location: val
                .get("installLocation")
                .and_then(|x| x.as_str())
                .map(str::to_string),
            last_updated: val
                .get("lastUpdated")
                .and_then(|x| x.as_str())
                .map(str::to_string),
            declared_plugins: None,
            declared_error: None,
        })
        .collect();
    // 顺序稳定，界面才不会每次读都跳。
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// `{"source":"github","repo":"a/b"}` → `github:a/b`。读不出就是 `None`，**不编**。
fn describe_source(v: Option<&serde_json::Value>) -> Option<String> {
    let v = v?;
    let kind = v.get("source").and_then(|x| x.as_str());
    let repo = v
        .get("repo")
        .or_else(|| v.get("url"))
        .and_then(|x| x.as_str());
    match (kind, repo) {
        (Some(k), Some(r)) => Some(format!("{k}:{r}")),
        (Some(k), None) => Some(k.to_string()),
        (None, Some(r)) => Some(r.to_string()),
        (None, None) => None,
    }
}

/// 数一个 marketplace **声明**了多少插件 —— 读 `<落点>/.claude-plugin/marketplace.json`
/// 里 `plugins[]` 的长度。
///
/// ⚠ 数的是**声明**，不是安装。这个函数**不看** `plugins/` 那个目录里有几个文件夹：
/// 那些是下载快照的内容（模块头注），把它们当成「装了几个」正是本件要避免的假话。
fn count_declared_plugins(install_location: &Path, cap: u64) -> Result<u32, String> {
    let manifest = install_location
        .join(".claude-plugin")
        .join("marketplace.json");
    let raw = read_capped(&manifest, cap)?
        .ok_or_else(|| format!("落点里没有 `{}`", manifest.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("manifest 解析失败：{e}"))?;
    let arr = v
        .get("plugins")
        .and_then(|x| x.as_array())
        .ok_or_else(|| "manifest 里没有 `plugins` 数组".to_string())?;
    Ok(arr.len() as u32)
}

/// 枚举 `<claude_dir>/plugins/` 下登记的 marketplace。
pub fn survey_marketplaces_in(claude_dir: &Path) -> Result<MarketplaceSurvey, String> {
    let path = claude_dir.join("plugins").join("known_marketplaces.json");
    let raw = match read_capped(&path, KNOWN_MARKETPLACES_CAP)? {
        Some(raw) => raw,
        // ★ 文件不存在**不是错**：这台机器一个 marketplace 都没有，如实说。
        None => {
            return Ok(MarketplaceSurvey {
                entries: Vec::new(),
                file_absent: true,
            })
        }
    };
    let mut entries = parse_known_marketplaces(&raw)?;
    for e in &mut entries {
        match e.install_location.as_deref() {
            // ★ 上限**当参数传到这里**，不是藏在被调函数里：这样「超限之后怎么办」
            // 与常量本身**长在同一屏**上 —— 降级的那一支就在下一行，谁都赖不掉。
            Some(loc) => match count_declared_plugins(Path::new(loc), MARKETPLACE_MANIFEST_CAP) {
                Ok(n) => e.declared_plugins = Some(n),
                // 只坏这一行，整张表照出 —— 且**带着理由**出去。
                Err(why) => e.declared_error = Some(why),
            },
            None => {
                e.declared_error =
                    Some("`known_marketplaces.json` 里没记 `installLocation` ⇒ 无从去数".into())
            }
        }
    }
    Ok(MarketplaceSurvey {
        entries,
        file_absent: false,
    })
}

/// `P8a`：列出本机登记的 marketplace（**只读**）。
///
/// ⚠ **本机专属**：远端同一个问题今天答不出（要 daemon 补一条子命令），
/// 已在 `parity_ledger` 里记成 `ParityDebt` 并写明欠的是什么 —— 不假装两侧都有。
#[tauri::command]
pub async fn list_plugin_marketplaces() -> Result<MarketplaceSurvey, String> {
    let claude_dir = crate::paths::resolve_claude_dir().ok_or("找不到 claude 目录")?;
    survey_marketplaces_in(&claude_dir)
}

#[cfg(test)]
#[path = "../../../tests/bridge/plugins_tests.rs"]
mod tests;
