//! cc-acct-iso 账号库契约的**唯一定义**，外加平台无关的名字安全判据。
//!
//! # 这份数据有三个读者
//!
//! bash 写侧（`cc-acct-iso`）· 远端 daemon（`observe/accounts_query.rs`）·
//! 本机 monitor（`local_accounts.rs`）。daemon crate 是 bin-only、刻意不进 workspace，
//! 所以此前只能靠一条**读对面源文件**的守卫（`contract_matches_the_daemon_implementation`）
//! 把四个常量钉住。
//!
//! 那条守卫是**真的**（它剥注释、剥测试段、有字节地板与锚点自检，注释里还记着
//! 第一版是安慰剂、被变异证伪后修好）—— 但守卫只能**发现**漂移。
//! 常量放进这里之后，漂移变成**不可表示**：两侧 import 同一个 `const`，
//! 想不一致得先把 import 删掉。⇒ 那条守卫可以退役。
//!
//! # 为什么只搬这些
//!
//! `local_accounts.rs` 与 `accounts_query.rs` 有三个同名函数，但**只有一个该合**：
//!
//! | 函数 | 判定 |
//! |---|---|
//! | [`is_deceptive_char`] | **平台无关**，而且两侧**双向漂了**（见下）⇒ 合，取并集 |
//! | `is_safe_config_dir` | monitor 用 `looks_absolute`（认 Windows 盘符）、**允许 `\`** 作分隔符故改拒 `\..\`；daemon 直接把 `\` 当危险字符拒掉。**是刻意的平台特化，不是漂移** ⇒ 不合 |
//! | `norm_dir` | 同上（monitor 多剥一层 `\`）⇒ 不合 |
//!
//! 硬把后两个合了，只能二选一：要么 monitor 失去 Windows 路径，要么 daemon 失去对 `\` 的拒绝。

/// 账号库目录名（`$HOME` 下）。
pub const ACCTS_DIR_NAME: &str = ".claude-accts";
/// manifest 文件名（账号库目录下）。
pub const MANIFEST_NAME: &str = "accounts.json";
/// 凭据文件名（每个账号的 config dir 下）；只 stat 存在性，**绝不读内容**。
pub const CREDENTIALS_NAME: &str = ".credentials.json";
/// 本仓支持的 manifest schema 版本。**不支持的版本不是错误**，是「未启用多账号」。
pub const SUPPORTED_SCHEMA: u64 = 1;

/// 视觉欺骗字符：看不见或会改变渲染方向/边界的码位。
///
/// 账号名与 config dir 会进 UI、也会进命令串；一个夹带 RLO 的名字能在界面上
/// 显示成另一个账号。两侧读的是**同一份 manifest**，所以「什么算欺骗」必须一致。
///
/// # 这里是两侧的并集 —— 因为它们各自都有洞
///
/// U7-3 实测，同名函数两侧**双向漂移**：
///
/// | 缺在哪 | 码位 | 是不是真洞 |
/// |---|---|---|
/// | daemon 缺 | `U+2060..=U+2064`（word joiner / 不可见运算符）· `U+1680` · `U+2000..=U+200A` · `U+202F` · `U+205F` · `U+3000`（各类空白） | **是**。这些 `char::is_control()` 全是 `false`，daemon 侧真的会放行 |
/// | monitor 缺 | `U+0085`（NEL） | **不是**。U7-3 我把它当安全洞报了出来，**那是错的**：Rust 里 `'\u{0085}'.is_control() == true`（NEL 属 Cc 类），monitor 的 `is_safe_config_dir` 本来就靠 `is_control()` 拒了它。**集合差了一项，可观察行为没差。**<br>daemon 源码里那句「NEL 不在 `char::is_control` 里」是**事实错误**，我照抄了它 —— U7-4 实测证伪 |
///
/// NEL 仍然留在本集合里：让集合**自足** —— 调用方即使没有另外查 `is_control()` 也有完整保护。
/// 但**理由要说对**，不能靠一句错的断言撑着。
///
/// 那条既有守卫只钉四个字符串常量，**看不见这个**。
/// 一个能骗过其中一侧的名字就是能骗人的名字，与哪一侧在读无关 ⇒ 取并集。
pub fn is_deceptive_char(c: char) -> bool {
    matches!(c,
        '\u{0085}'                  // NEL（C1 换行；is_control 已覆盖，留此为让集合自足）
        | '\u{00A0}'                // NBSP
        | '\u{1680}'                // Ogham space mark
        | '\u{2000}'..='\u{200A}'   // en/em 等各类空格
        | '\u{200B}'..='\u{200F}'   // 零宽空格/连接符 + LRM/RLM
        | '\u{2028}' | '\u{2029}'   // 行分隔 / 段分隔
        | '\u{202A}'..='\u{202E}'   // 双向嵌入/覆盖
        | '\u{202F}'                // narrow NBSP
        | '\u{205F}'                // medium mathematical space
        | '\u{2060}'..='\u{2064}'   // word joiner / 不可见运算符
        | '\u{2066}'..='\u{2069}'   // 双向隔离
        | '\u{3000}'                // ideographic space
        | '\u{FEFF}'                // ZWNBSP / BOM
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 并集必须**同时**覆盖两侧此前各自独有的那些码位。
    ///
    /// 这条是集合漂移的回归钉：任一侧的集合被"还原"回去，本测试红。
    ///
    /// ⚠ 但要分清**哪一半是真洞**：daemon 缺的那些 `is_control()` 全是 `false`，
    /// 它真的会放行；monitor 缺的 NEL 属 Cc 类、`is_control()` 本来就挡着 ——
    /// U7-3 我把后者也当成安全洞报了，U7-4 实测证伪。集合差过，行为没差。
    #[test]
    fn the_union_covers_what_each_side_used_to_miss() {
        // NEL：集合里确实缺过，但**不是安全洞**（`is_control()` 覆盖）。
        // 留在集合里是为了让本集合自足，不是因为它此前漏防了什么。
        assert!(is_deceptive_char('\u{0085}'), "NEL 从集合里掉了");
        // daemon 此前**真的**漏防的（这些 `is_control()` 全是 false）
        for c in [
            '\u{2060}', '\u{2064}', '\u{1680}', '\u{2000}', '\u{200A}', '\u{202F}', '\u{205F}',
            '\u{3000}',
        ] {
            assert!(
                is_deceptive_char(c),
                "U+{:04X} —— daemon 侧此前真的会放行",
                c as u32
            );
        }
    }

    /// 两侧本来就都有的那些，一个都不能丢。
    #[test]
    fn the_union_keeps_everything_both_sides_already_had() {
        for c in [
            '\u{00A0}', '\u{200B}', '\u{200F}', '\u{2028}', '\u{2029}', '\u{202A}', '\u{202E}',
            '\u{2066}', '\u{2069}', '\u{FEFF}',
        ] {
            assert!(is_deceptive_char(c), "U+{:04X} 丢了", c as u32);
        }
    }

    /// ★ 凭据文件名必须与 `cc-acct-iso` 的 `NATIVE_IDENTITY` 声明一致。
    ///
    /// 这是**唯一还需要守卫的那一半**：bash 侧是另一门语言，共享不了常量。
    /// Claude Code 哪天改了凭据文件名，改一边漏一边的表现是**静默错** ——
    /// `loggedIn` 恒 false，UI 上看不出来。
    ///
    /// 守卫搬到这里而不是留在两个调用方：常量住在这儿，检查就该住在这儿，
    /// 否则又是两份。原先 daemon 侧那条还带了「本文件真的在用这个字面量」的第二半 ——
    /// 常量共享之后那半**结构上不可能不成立**（只有一个定义处），已随之删掉。
    #[test]
    fn the_credential_filename_matches_the_cc_acct_iso_declaration() {
        let lib_sh = include_str!("../../../vendor/cc-acct-iso/scripts/lib.sh");
        assert!(
            lib_sh.len() > 1000,
            "只读到 {} 字节的 lib.sh —— include_str! 没读到，本断言在空转",
            lib_sh.len()
        );
        // 声明里那一行的精确形状：`<项名>:<原生根>:<类别>`，凭据项必须是 secret。
        let expected = format!("{CREDENTIALS_NAME}:cfg:secret");
        assert!(
            lib_sh.contains(&expected),
            "Z06 双写点漂移：cc-acct-iso 的 NATIVE_IDENTITY 声明里找不到 {expected:?}。\n\
             两侧判「已登录」用的都是 {CREDENTIALS_NAME:?}（本 crate 的常量），\n\
             Claude Code 改了凭据文件名就要**同时**改这里与 bash 声明。"
        );
    }

    /// 正常字符不许被误杀 —— 否则合法账号名会被拒。
    #[test]
    fn ordinary_characters_are_not_rejected() {
        for c in ['a', 'Z', '0', '_', '-', '.', '/', ' ', '中', 'é'] {
            assert!(!is_deceptive_char(c), "{c:?} 被误判成欺骗字符");
        }
    }
}
