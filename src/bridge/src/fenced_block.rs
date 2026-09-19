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
            "{what} 第 {} 行有 cc-monitor BEGIN 标记，但其后找不到配对的 END\
             （可能被手动改坏 / 上次安装中断）。为避免误删你的内容，已中止\
             ——请手动修好该文件后重试。",
            b + 1
        )),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// `KR62D3`：**同一件事今天有几套形状 —— 一条有住址的账**
// ═══════════════════════════════════════════════════════════════════════════

/// 「把 cc-monitor 的一块东西装进一份 shell 配置」这件事的**一个形状**。
///
/// # 为什么要有这张表（而不是把它写进某段注释里）
///
/// `K-R62 §0c` 现打到的那条：同一件事按宿主分了三套形状，判定那一半 `fenced_block`
/// 已经收了（三套全走 [`find_pair`]），**装与卸那一半没收**。
/// 那段话本来只活在件文件的正文里 —— 而 `K-R60` / `K-R61` 两件已经连着证明：
/// **写在注释里而字段 / 判据看不见，等于没写**（一句真话摆错了格，和假话一样是假举证，`K29`）。
///
/// ⇒ `K-R62` **不收敛它们**（那是另一个量级，见件文件 `§0e`），只把它**登记成一条有住址的账**，
/// 然后由 PM 裁收不收、什么时候收。这张表买到三件下面各有判据看着的东西：
///   ① 每一行都指得出**代码住址**（`<文件>.rs::<符号>`），`structural_scan` 会去验它解析得到；
///   ② 围栏标记**指**各自那个常量，不在这里抄字面量（抄一份就是第二个住址）；
///   ③ 「判定已收 / 装卸未收」这句话是**数出来的**，不是形容出来的。
pub struct FenceShape {
    /// 稳定 id。
    pub id: &'static str,
    /// 哪台机器上的哪份文件。
    pub host: &'static str,
    /// 往里放什么。
    pub what_goes_in: &'static str,
    /// 这一套认哪一对围栏的 BEGIN。**指常量，不抄字面量。**
    pub begin_marker: &'static str,
    /// 装那一半住哪（`<文件>.rs::<符号>`）。
    pub install_site: &'static str,
    /// 卸那一半住哪；`None` = **今天没有卸口**（那本身就是一条账）。
    pub uninstall_site: Option<&'static str>,
    /// 配对判定走哪一份。三套今天是同一份 —— 这一格就是「已经收了哪一半」的读数。
    pub pairing: &'static str,
    /// 它与别的形状**差在哪**。
    pub differs_in: &'static str,
}

/// 🔴 **那三套形状 + `K-R62` 新加的那一条，唯一一份账。**
///
/// ⚠ 它**不判对错**，只记「今天是什么样」。要不要收敛由 PM 裁。
pub const FENCE_SHAPES: &[FenceShape] = &[
    FenceShape {
        id: "remote-posix-block",
        host: "远端 POSIX 的 ~/<用户选的那份 rc>",
        what_goes_in: "整块别名 snippet（src/shared/ccm-aliases.sh）",
        begin_marker: crate::sftp::CCM_PROFILE_BEGIN,
        install_site: "sftp.rs::install_remote_ccm_helper",
        uninstall_site: Some("sftp.rs::uninstall_remote_ccm_helper"),
        pairing: "fenced_block.rs::find_pair",
        differs_in: "落盘走 SFTP（upload_atomic + 远端备份），本机那两套走本地原子替换",
    },
    FenceShape {
        id: "local-windows-ps",
        host: "本机 Windows 的 PowerShell profile",
        what_goes_in: "整块 PowerShell 代码（scripts/cc.ps1.tpl 渲染）",
        begin_marker: crate::profile_installer::BEGIN_MARKER,
        install_site: "profile_installer.rs::install_to_profile",
        uninstall_site: Some("profile_installer.rs::uninstall_from_profile"),
        pairing: "fenced_block.rs::find_pair",
        differs_in: "内容是**现渲**的（命令名与要不要带 cc 函数由界面给），另两套写的是仓里那份文件本身；\
                     而且它要保住 CRLF（profile_installer.rs::detect_eol）",
    },
    FenceShape {
        id: "local-posix-source-line",
        host: "本机 POSIX 的 ~/<用户选的那份 rc>",
        what_goes_in: "**一行** source，指向 ~/.cc-monitor/account-aliases.sh（内容住在那份生成文件里）",
        begin_marker: crate::account_aliases::RC_BEGIN,
        install_site: "account_aliases.rs::ensure_rc_source_line",
        uninstall_site: None,
        pairing: "fenced_block.rs::find_pair",
        differs_in: "🔴 唯一**没有卸口**的一套（K-R62 §0b 那张表的「卸」一栏逐字「部分」）；\
                     也是唯一「围栏里只有一行、真内容在别处」的一套 —— 它刻意用另一对标记，\
                     与别的套共用标记就会「装一个把另一个整块替换掉」",
    },
    // ★★ 〔`K-R62` 09-11〕**本件新加的那条路，就是这一行。**
    FenceShape {
        id: "local-posix-block",
        host: "本机 POSIX 的 ~/<用户选的那份 rc>",
        what_goes_in: "整块别名 snippet —— **与 `remote-posix-block` 同一个常量**（sftp.rs::CCM_WRAPPER_SNIPPET）",
        // 与远端那一套**同一个常量**：本机与远端装进 rc 的是同一个东西（`K15` / `K36`）。
        begin_marker: crate::sftp::CCM_PROFILE_BEGIN,
        // 🔴 **落盘那一跳与 `local-windows-ps` 是同一处** —— 这一行的 `install_site`
        //    与它逐字相同，不是笔误：补这一格没有多出第四台安装器，多出来的只是
        //    那一台安装器的第二种方言（分岔在 profile_installer.rs::plan_install）。
        install_site: "profile_installer.rs::install_to_profile",
        uninstall_site: Some("profile_installer.rs::uninstall_from_profile"),
        pairing: "fenced_block.rs::find_pair",
        differs_in: "与 `remote-posix-block` **只差落盘那一跳**（本地原子替换 vs SFTP）：\
                     内容、围栏、合块与剥块的实现全共用；与 `local-windows-ps` 只差**方言**\
                     （分岔在 profile_installer.rs::plan_install / \
                     profile_installer.rs::plan_uninstall，落盘与备份回滚那一整套共用）。\
                     ⇒ 补这一格没有把三套变成四套。判据 \
                     profile_installer.rs::the_local_posix_port_is_byte_for_byte_the_remote_one",
    },
];

#[cfg(test)]
#[path = "../../../tests/bridge/fenced_block_tests.rs"]
mod tests;
