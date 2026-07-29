//! T04 第二步：**围栏块配对判定**——本机 profile 与远端 profile 共用同一条规则。
//!
//! ## 为什么只抽这一条，而不是"统一部署器"
//!
//! T04 第二步原计划是"五套机制的装/升/卸真正走注册表"。**数下来那个抽象不该建**：
//!
//! | 范式 | 使用者 | 现状 |
//! |---|---|---|
//! | 指纹判过期 → 决定装/升/跳过 | daemon + cc-acct-iso（2） | **已共享** `sftp::deploy_decision` |
//! | 备份 → 写 → 读回比对 → 回滚 | 5 处 | **已共享** `verified_write::verify_readback` |
//! | 围栏块插入/替换/剥离 | ccm 远端 profile + PowerShell 本机 profile（2） | **两套独立实现** ← 本模块 |
//! | 整份 JSON 覆写 | 项目 MCP（1） | 单例，不抽 |
//!
//! 三个范式里两个早就共享了，第四个只有一个使用者。**再套一层 `install(tool_id)` 分派器
//! 只会把五件形状不同的事装进一个盒子**——正是本工作区反复拒绝的形状。
//! 真正剩下的重复只有围栏块这一族，而它藏着一个**真的数据丢失 bug**（见下）。
//!
//! ## 这里修的是一个会吃掉用户内容的 bug
//!
//! 两侧对「有 BEGIN 但找不到配对的 END」（上次安装中断 / 用户手改坏）处置**不一致**：
//!
//! - 远端（`sftp::merge_profile_block`）：**Err 中止**。这是 F10 审计 B1 专门加的——
//!   原话「绝不用独立 `find` 误配前面的 END 而吞掉用户内容；宁可报错让用户手修，
//!   也不破坏文件」。
//! - 本机（`profile_installer::find_block_range`）：返回 `None` → 走**追加**分支。
//!
//! 本机那条的后果我实测过（`profile_installer` 里留着那条复现测试）：
//!
//! ```text
//! 原始：  # my stuff / # === cc-monitor BEGIN v1 === / function cc { }        ← 损坏 + 用户代码
//! 装一次：…BEGIN… / function cc { } / …BEGIN… / NEW / …END…                  ← 追加，用户代码还在
//! 装两次：# my stuff / …BEGIN… / NEW / …END…                                 ← **function cc { } 没了**
//! ```
//!
//! 第二次安装时，**损坏的那个 BEGIN 与新块的 END 配上了对**，于是两者之间的东西
//! ——包含用户自己的代码——被整段替换掉。写的是用户的 PowerShell `$PROFILE`，
//! 和远端 `.bashrc` 同性质：写坏了下次开终端就炸。
//!
//! 所以本模块取**两者中最强的那一档**（同 T01 对 `verified_write` 的做法：
//! 四处实现里本机侧只比长度，统一到内容级比对）。

/// 找配对的围栏块，返回**行下标**区间（含两端）。
///
/// - `Ok(None)`：没有 BEGIN → 调用方追加。
/// - `Ok(Some((b, e)))`：找到配对 → 调用方整块替换。
/// - `Err(_)`：**有 BEGIN 但其后没有 END** → 调用方必须中止，绝不猜。
///
/// 匹配用 `trim_start().starts_with(..)`：两侧的标记都允许行内缩进，
/// 且本机侧的 BEGIN 带版本后缀（`# === cc-monitor BEGIN v1 ===`）所以只能前缀匹配。
///
/// **只找 BEGIN 之后的 END**——独立 `find` 会误配 BEGIN 前面的 END。
pub fn find_pair(
    text: &str,
    begin_marker: &str,
    end_marker: &str,
    what: &str,
) -> Result<Option<(usize, usize)>, String> {
    let mut begin: Option<usize> = None;
    for (idx, line) in text.lines().enumerate() {
        let l = line.trim_start();
        if begin.is_none() && l.starts_with(begin_marker) {
            begin = Some(idx);
            continue;
        }
        if begin.is_some() && l.starts_with(end_marker) {
            return Ok(Some((begin.unwrap(), idx)));
        }
    }
    match begin {
        None => Ok(None),
        Some(b) => Err(format!(
            "{what} 第 {} 行有 cc-monitor BEGIN 标记，但**其后找不到配对的 END**\
             （可能被手动改坏 / 上次安装中断）。为避免误删你的内容，已中止\
             ——请手动修好该文件后重试。",
            b + 1
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: &str = "# === cc-monitor BEGIN";
    const E: &str = "# === cc-monitor END";

    #[test]
    fn no_begin_means_append() {
        assert_eq!(find_pair("a\nb\n", B, E, "x").unwrap(), None);
        assert_eq!(find_pair("", B, E, "x").unwrap(), None);
    }

    #[test]
    fn paired_block_is_found_by_line_index() {
        let t = "a\n# === cc-monitor BEGIN v1 ===\nbody\n# === cc-monitor END ===\nz\n";
        assert_eq!(find_pair(t, B, E, "x").unwrap(), Some((1, 3)));
    }

    /// **这一条是本模块存在的理由。** 本机侧原先在这种输入上返回 `None` → 追加 →
    /// 第二次安装时损坏的 BEGIN 与新块的 END 配对 → **吃掉两者之间的用户代码**。
    #[test]
    fn begin_without_end_is_an_error_not_an_append() {
        let t = "# my stuff\n# === cc-monitor BEGIN v1 ===\nfunction cc { }\n";
        let e = find_pair(t, B, E, "PowerShell profile").unwrap_err();
        assert!(e.contains("第 2 行"), "要报出是哪一行：{e}");
        assert!(e.contains("找不到配对的 END"), "{e}");
        assert!(e.contains("已中止"), "措辞要让用户知道我们没动文件：{e}");
        assert!(e.contains("PowerShell profile"), "要说清是哪个文件：{e}");
    }

    /// **只找 BEGIN 之后的 END**：BEGIN 前面的 END 不算（独立 `find` 会误配它）。
    #[test]
    fn end_before_begin_does_not_pair() {
        let t = "# === cc-monitor END ===\nuser stuff\n# === cc-monitor BEGIN v1 ===\nbody\n";
        assert!(
            find_pair(t, B, E, "x").is_err(),
            "前面那个 END 不该被误配成配对"
        );
    }

    /// 缩进的标记也要认（两侧都用 `trim_start`）。
    #[test]
    fn indented_markers_are_recognised() {
        let t = "a\n  # === cc-monitor BEGIN v1 ===\nb\n\t# === cc-monitor END ===\n";
        assert_eq!(find_pair(t, B, E, "x").unwrap(), Some((1, 3)));
    }

    /// 只取**第一个** BEGIN（幂等：重复安装不会因为多个 BEGIN 而漂移）。
    #[test]
    fn first_begin_wins() {
        let t = "# === cc-monitor BEGIN v1 ===\nx\n# === cc-monitor BEGIN v2 ===\ny\n# === cc-monitor END ===\n";
        assert_eq!(find_pair(t, B, E, "x").unwrap(), Some((0, 4)));
    }
}
