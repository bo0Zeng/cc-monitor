//! 足迹面：`FootprintFace` 一家落在自己布局里的申报 ＋ 用户级设置住哪。

use super::*;

/// 见 [`Adapter::footprint`]。
#[derive(Clone, Copy)]
pub struct FootprintFace {
    pub tools: &'static [footprint_decl::ToolSpec],
    pub env: &'static [footprint_decl::UnmanagedEnv],
    pub under_agent_home: fn(&str) -> Option<&str>,
    pub user_settings: fn(&Path) -> [PathBuf; 2],
    /// 用户级设置文件里「会话停下时」那一种钩子：解析出来的那一份 ⇒ 登记在下面的每条命令（cc-bus 钩子诊断按它认装没装）。
    pub stop_hook_commands: fn(&serde_json::Value) -> Vec<String>,
    /// 一条命令 ⇒ 要并进那份文件的（键路径, 那一项），并法是往那条路径上的数组末尾加一项
    /// （只算不写：诊断给的要加的内容 · 待办的 diff 都照它）。
    pub stop_hook_slots: fn(&str) -> Vec<(Vec<&'static str>, serde_json::Value)>,
    /// 收信钩子那一条要不要信任、怎么算（`None` ⇒ 这一家的钩子不要信任）。只读、只算，谁都不替用户写。
    pub hook_trust: Option<HookTrust>,
}

/// 一家收信钩子的信任：记在哪份文件 · 每一条的键与照声明算好的值 · 读回此刻记着的值 · 把一对键值并进那份文件。
#[derive(Clone, Copy)]
pub struct HookTrust {
    /// 家目录 ⇒ 信任记在哪份文件。
    pub file: fn(&Path) -> PathBuf,
    /// 钩子那份文件的路径 · 它解析出来的样子 ⇒ 「停下时」底下登记的每一条（命令, 信任键, 照那一家的算法算好的值）。
    pub expected: fn(&Path, &serde_json::Value) -> Vec<(String, String, String)>,
    /// 信任那份文件的原文 · 键 ⇒ 此刻记着的值（没有 · 读不懂 ⇒ `None`）。
    pub recorded: fn(&str, &str) -> Option<String>,
    /// 信任那份文件现在的内容 · 键 · 值 ⇒ 记上这一对之后的整份（读不懂 ⇒ `None`）；并进空文件的那一份就是要加的那一段。
    pub merge: fn(&str, &str, &str) -> Option<String>,
}

/// 给定注册表里 `kind` 那一家的足迹面（没有 ⇒ `None`）。
pub fn footprint_face_among(registry: &[Adapter], kind: &str) -> Option<FootprintFace> {
    adapter_among(registry, kind).and_then(|a| a.footprint)
}

/// 给定注册表里带足迹面的那几家（按注册表顺序；生产交 `REGISTRY`，判据拿替身家喂）。
pub fn footprint_faces_among(registry: &[Adapter]) -> impl Iterator<Item = FootprintFace> + '_ {
    registry.iter().filter_map(|a| a.footprint)
}

/// 一个号（`kind` 那一家的号，配置目录 `dir`）里账号面碰的那几份：那一家账号面身份表的每一项，各在那个号的配置目录底下
/// （设了账号环境变量，身份表那几项都跟着配置目录走）。那一家没有账号面 ⇒ 空。
pub fn account_files_among(registry: &[Adapter], kind: &str, dir: &Path) -> Vec<PathBuf> {
    accounts_face_among(registry, kind)
        .map(|f| f.identity.iter().map(|(n, _, _)| dir.join(n)).collect())
        .unwrap_or_default()
}
