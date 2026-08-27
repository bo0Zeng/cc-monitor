//! **第三方 API key 的唯一住址** —— 装它的类型（本文件）· 它落盘的格式（[`store`]）·
//! 它的权限够不够窄（[`perm`]）。monitor 与远端 daemon 共用同一份。
//!
//! # 这一档保什么、不保什么（`K-H2a §0a`〔用 08-26〕选的第三档，逐字抬进来）
//!
//! 三档里选的是「**单独一个文件 · 只给本人 · 不进前端那份配置**」。另两档为什么没选：
//! 系统钥匙串**撞用户逐字「不强依赖本机环境」**（每平台一套 API，目标机还要有对应服务在跑）；
//! 自己加密存文件在单机场景下是**安全剧场**（解密密钥还得放在同一台机器上），
//! 而且它给「已经加密了」的错觉，**会让人放松对回显与日志的警惕，而那两个才是真实泄漏的主要来源**。
//!
//! - **保**：同机器上别的用户读不到 · 顺手打开配置文件不会看见 ·
//!   前端每次读写整份配置时它不在里面。
//! - **不保**：**已经能以你的身份运行程序的人**。这一档挡不住他，**别写成挡得住**。
//!
//! ## ⚠ DPAPI 那条「拷到另一台机器也解不开」的性质，**今天没有**
//!
//! 初稿在 Windows 那半写的是 DPAPI，被〔用 08-26〕同一天的另一条要求推翻：
//! 「**这个 key 能不能直接导入 json, 要求脱离这个前端也能配**」「**即能直接在文件里改**」——
//! 而 DPAPI 存的是一个**二进制密文块，记事本打开是乱码，改不了**。
//! ⇒ 换来手编，付出的就是那条性质。**它从今天起没有了，别在别处再声称它。**
//! （留这段是为了防止下一个人重新捡起 DPAPI —— 治过的病要留住址，不然会复发。）
//!
//! ## ⚠ 远端那一侧：**不许从 SFTP 的 mode 参数拿机密性**〔实@08-27〕
//!
//! `src-tauri/src/sftp.rs::upload_atomic` 收一个 mode 参数，看起来像是「推上去就是 0600」。
//! 对面是 Windows 时**那不是「不生效」，是「静默地不生效」**，三条读数（分母都在，量于 08-27）：
//! 1. 那个 mode 只以 SFTP v3 的 `SSH_FILEXFER_ATTR_PERMISSIONS` 属性搭在 `SSH_FXP_OPEN` 上
//!    （`russh-sftp-2.3.0/src/protocol/file_attrs.rs:29`）——**顺不顺是服务端的事，协议不回执**；
//! 2. **没有第二次机会**：`sftp.rs:141-147` 头注逐字禁掉了兜底 `set_metadata`
//!    （理由是 OpenSSH sftp-server 上 setstat 会把文件截成 0 字节 —— 而那条理由是在一台
//!    **POSIX** 服务端上量的）；
//! 3. **本仓没有任何一处回读权限**：`upload_atomic_verified` 只走 `verify_uploaded_bytes`，
//!    那函数三条分支逐字只比**字节与长度**。
//!    再加一条：全仓唯一那条 OS 判定 `src/settings/host-os.ts` 头注逐字说它量的是
//!    **monitor 自己**跑在哪个 OS 上，**不是远端** ⇒ 代码里根本没有「对面是什么 OS」这个量。
//!
//! ⇒ 结论：**机密性只能由「真正拿着那份文件的那台机器」自己检查**（这就是 [`perm`] 存在的理由），
//! **不能由写它的那一跳「设一下就当保住了」**。
//! ⚠ 我**没有**对着一台 Windows sftp-server 实打过（那要真机）；上面 1–3 是从本仓代码 +
//! `russh-sftp` 源码读出来的，「服务端可以忽略」这一句是 SFTP v3 的协议性质，**我没量**。
//!
//! # 分工（别把两件事混成一件）
//!
//! - **写**这份文件：**只有 monitor 那一侧**（本 crate 的 `harden` feature 打开时）。
//! - **读**这份文件：两侧都读。daemon 那侧**只许读** —— `K-H2a` 裁四：`readonly_guard.rs`
//!   扫 daemon 生产段断言不含任何文件系统变更调用，白名单恰好一个模块 `control/fork_write.rs`。

pub mod perm;
pub mod store;

use std::fmt;

/// 一把 key 的明文。
///
/// # `KS1`：这个类型**存在的唯一理由**是让「顺手打印」这条路不存在
///
/// - **手写 [`fmt::Debug`]**，印出来恒为遮蔽形（见下面那个 impl）；
/// - **不许** `derive(Debug)`（会把内层 `String` 原样印出来）；
/// - **不许** `derive(Serialize)`（会把它写进任何一帧 JSON）；
/// - **不许** `impl Display`（`{}` 是最容易被顺手写出来的那一个）。
///
/// 这三条由 `secret_key_has_no_printing_shortcut` 扫本文件的定义面钉住，命中任一即红。
///
/// # ⚠ 它**认不出**什么（诚实边界 —— 别把这条判据读成「明文不可能泄漏」）
///
/// 1. **别处再定义第二个装 key 的类型**（本判据只扫本文件的定义面）；
/// 2. **把明文 `clone()` 进一个普通 `String` 再打印** ——
///    一旦出了 [`SecretKey::expose_for_auth_header`] 那道门，本类型就管不着了。
///
/// 这两形由 `KS2`（取明文的地方**恰好一处**）兜：出口只有一个，那一处被逐字钉住。
/// **两条判据缺一都不成立**：只有本条 ⇒ 换个类型就绕过；只有 `KS2` ⇒ 出口是一处，
/// 但那一处拿到的东西照样能被 `{:?}` 印出来。
pub struct SecretKey(String);

impl SecretKey {
    /// 从明文建一把 key。**唯一入口**（内层字段是私有的）。
    pub fn new(raw: impl Into<String>) -> Self {
        SecretKey(raw.into())
    }

    /// 有没有配。**不碰内容** —— 这是判「配没配」的正确问法。
    pub fn is_configured(&self) -> bool {
        !self.0.trim().is_empty()
    }

    /// 明文的字节数。给诊断用；它**不泄漏内容**，但泄漏长度，所以只在本地日志里用。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// `len() == 0`。clippy 要求有 `len` 就得有它。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// **`KS6` 的后端那一半**：给前端看的东西**只有这一个形状**。
    ///
    /// 短到看不出前后缀的（`<= MASK_KEEP * 2`）**整条遮掉**——
    /// 「前后各留几位」在一把 8 字符的 key 上等于把它交出去。
    ///
    /// ⚠ 它**不是** `KS6` 的全部：本函数只保证「这条路上出去的是掩码」，
    /// 不保证「没有别的路把明文送出去」。那一格归 `KS2`（出口恰好一处）。
    pub fn masked(&self) -> String {
        let s = self.0.trim();
        if s.is_empty() {
            return String::new();
        }
        let n = s.chars().count();
        if n <= MASK_KEEP * 2 {
            return "*".repeat(n.max(MASK_KEEP));
        }
        let head: String = s.chars().take(MASK_KEEP).collect();
        let tail: String = s.chars().skip(n - MASK_KEEP).collect();
        format!("{head}{}{tail}", "*".repeat(n - MASK_KEEP * 2))
    }

    /// ★★ **`KS2`：取明文的唯一出口。**
    ///
    /// 名字里写死了它唯一的用途（**往上游请求写鉴权头**），因为
    /// 「这个函数是干什么的」正是将来有人多加一处调用时唯一能被读出来的约束。
    ///
    /// ⚠ **加一处调用是收紧、动那条相等断言是放宽**：
    /// 将来真要多一处，**必须在件计划里单独说清那一处是什么**，
    /// 不许在实现里顺手把 `EXPECTED_EXPOSE_SITES` 改大。
    pub fn expose_for_auth_header(&self) -> &str {
        &self.0
    }

    /// ⚠⚠ **明文的第二个出口：把它写回那份文件。**
    ///
    /// # 这一处是**新开的**，不是我顺手加的 —— 经过写在这里
    ///
    /// `KS2` 的字面是「取明文的地方**恰好一处**：只有『往上游请求写鉴权头』那一行」。
    /// 实现时撞上一件绕不过去的事：**落盘也必须碰明文**（`store::merge_key` 要把它写进 JSON）。
    /// 「一处」在字面上做不到，而把落盘伪装成别的东西（让 `store.rs` 直接摸私有字段）
    /// 只是**把第二个出口藏起来**，不是消掉它 —— 那正是本工作区在治的病。
    ///
    /// ⇒ 处置：**两个出口各自具名、各自钉死次数、各自只在一个 crate 里出现**，
    /// 再加一条「明文出口总数恰好 2」的全断。`KS2` 那句「不许在实现里顺手把断言改大」
    /// 我照做了：**没有改任何既有断言**，而是把这一处**逐字写进件计划 `§0f`**交 PM 裁。
    ///
    /// # 两个出口的人群是**不相交**的（这一条是承重的）
    ///
    /// - [`SecretKey::expose_for_auth_header`]：只出现在 **daemon** 的生产段（中转换头那一行）。
    ///   monitor 那侧**不换头**，所以它在 `src-tauri` 生产段里应当是 **0** 次。
    /// - `expose_for_persisting`（本方法）：只出现在 **`creds-core`** 的生产段（`store::merge_key`）。
    ///   daemon 只读不写（`K-H2a` 裁四）⇒ 它在 daemon 生产段里应当是 **0** 次。
    ///
    /// ⇒ 「N 个独立源 ⇒ N 格单断 + 1 格全断」：三条断言各自守一格，见
    /// `creds-core` 的 `the_plaintext_has_exactly_two_named_exits` 与
    /// daemon 的 `relay::creds_guard`。
    pub fn expose_for_persisting(&self) -> &str {
        &self.0
    }
}

/// 掩码前后各留几位。
pub const MASK_KEEP: usize = 4;

/// **手写的** `Debug` —— 印出来恒为遮蔽形，且**不带长度**（长度也是信息）。
///
/// ⚠ 这个 impl 是 `KS1` 的正主：`derive(Debug)` 会把内层 `String` 原样印出来，
/// 而 `{:?}` 出现在 `format!("上游拒绝了：{resp:?}")` 这类错误路径里是**真实泄漏的大头**。
impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(<已遮蔽>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本文件的生产段。**剥掉 `#[cfg(test)]`** —— 下面那些 needle 的字面量就住在本模块里，
    /// 不剥的话判据会在自己的测试代码里找到它们（`scanning_guard_registry` 记着这一族实测五次、
    /// 五次都不是被判据变红发现的）。剥生产段是那张表里「**按构造读不到自己**」那一类。
    fn production() -> String {
        guard_core::production_code(include_str!("lib.rs"))
    }

    /// 从 `at` 之后的第一个 `{` 起，按花括号配平切出一整块（含两端花括号）。
    ///
    /// ★ 它替掉的是 `.find("\n}")` 那种「找收尾」的写法 —— 那种写法有两个毛病：
    /// ① 它是**语料变量上的裸匹配**，撞本仓 `needle_anchor_registry` 那条递减棘轮；
    /// ② 更实质的：它会在**块里第一个顶格 `}`** 上停住，而不是这一块真正的收尾。
    fn brace_block(src: &str, at: usize) -> Option<&str> {
        let open = src[at..].find('{')? + at;
        let b = src.as_bytes();
        let (mut depth, mut i) = (0i32, open);
        while i < src.len() {
            match b[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&src[open..=i]);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// `KS1` 机检：明文不许有「顺手打印」的路。
    #[test]
    fn secret_key_has_no_printing_shortcut() {
        let prod = production();
        // 反空真自检：剥完还得剩下真东西，且靶子确实在里面。
        assert!(
            prod.contains("pub struct SecretKey"),
            "剥生产段之后连类型定义都没了 —— 取法坏了，下面的断言在空转"
        );
        assert!(
            prod.len() > 500,
            "生产段只剩 {} 字节 —— 取法坏了",
            prod.len()
        );

        // ① 三条禁止形态，命中任一即红。分母 = 我登记的这 3 形，**不是**「所有打印写法」。
        for bad in [
            "derive(Debug)",
            "derive(Serialize)",
            "impl fmt::Display for SecretKey",
        ] {
            assert!(
                !prod.contains(bad),
                "`SecretKey` 的定义面出现了 `{bad}` —— \
                 KS1 要的是手写 Debug、恒为遮蔽形；这三形任一都会把明文送上一条顺手打印的路"
            );
        }

        // ② **正向**（黑名单必漏，所以同时要有白名单那一半）：手写 Debug 必须真在，且恰好一处。
        let n = prod.matches("impl fmt::Debug for SecretKey").count();
        assert_eq!(
            n, 1,
            "手写 `Debug` 应当恰好一处，实得 {n} —— 0 处 = 类型可以被别的方式印出来；\
             多处 = 编不过，说明这条判据量错了对象"
        );
        // ③ 遮蔽形的字面必须真在那个 impl 里（不是随便一句 `write_str`）。
        // ⚠ 用 `find_pinned` 而不是裸 `.find("…")`：本仓 `needle_anchor_registry` 立着一条
        //   **递减棘轮**（语料变量上的裸匹配「与 `contains` 同族同险：**needle 被撑大时照样绿**」）。
        //   `find_pinned` 额外买两样：**恰好一处**（多一处/零处都报错，不是悄悄取第一处）+ 两侧有边界。
        //   〔08-27 实测：我第一版写裸 `.find` 撞红了那条棘轮，读数「11 处 > 上限 8」。〕
        let at = guard_core::find_pinned(&prod, "impl fmt::Debug for SecretKey")
            .expect("切不出 Debug impl 的锚点 —— 本条按红处理，不是绿");
        let window = brace_block(&prod, at).expect("Debug impl 的花括号没配平 —— 按红处理");
        assert!(
            window.contains("<已遮蔽>"),
            "手写的 Debug 里没有遮蔽形字面 —— 它可能又把内容印回去了。窗口：{window}"
        );
        // 反空真自检之二（`KP4` 那两个价钱之一：窗口不许跨进下一个 item）。
        assert!(
            !window.contains("pub struct") && !window.contains("pub fn"),
            "Debug impl 的窗口跨进了下一个 item —— 窗口无界，上面那条断言不算数"
        );
    }

    /// ★★ **`KS2` 的「全断」那一格**：明文的出口**恰好两个**，而且都得具名。
    ///
    /// 上面 `secret_key_has_no_printing_shortcut` 守的是「不许有顺手打印的路」，
    /// 本条守的是另一件事：**不许有第三个出口**。两条缺一都不成立 ——
    /// 只有前者 ⇒ 加一个 `pub fn raw(&self) -> &str` 照样绿；
    /// 只有后者 ⇒ `derive(Debug)` 照样把明文印出去。
    ///
    /// 量法：在 `impl SecretKey` 的**函数体窗口**里数「返回内层字段」的写法。
    /// ⚠ 窗口是**花括号配平切出来的 impl 块**，不是 `[\s\S]*?`（`KP4` 那两个价钱之一）。
    #[test]
    fn the_plaintext_has_exactly_two_named_exits() {
        let prod = production();
        let at = guard_core::find_pinned(&prod, "impl SecretKey {")
            .expect("切不出 `impl SecretKey` —— 本条按红处理，不是绿");
        let block = brace_block(&prod, at).expect("impl 块的花括号没配平 —— 按红处理");
        // 反空真自检：窗口不许跨进下一个 item。
        assert!(
            !block.contains("impl fmt::Debug"),
            "impl 块的窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
        );
        assert!(block.len() > 400, "窗口只有 {} 字节 —— 切法坏了", block.len());

        // 「把内层字段原样交出去」的唯一写法。
        let exits = block.matches("&self.0").count();
        assert_eq!(
            exits, 2,
            "`SecretKey` 交出明文的地方有 {exits} 处，登记的是 **2** 处：\n\
             · `expose_for_auth_header` —— 中转往上游请求写鉴权头（只在 daemon 生产段）\n\
             · `expose_for_persisting`  —— 把它写回那份文件（只在 creds-core 生产段）\n\
             多一处 ⇒ **必须先在件计划里说清那一处是什么**（`KS2` 逐字：\
             加行是收紧、动断言是放宽，不许在实现里顺手把断言改大）"
        );
        // 两个名字都得真在（否则上面那个 2 可能来自两处匿名写法）。
        for name in ["fn expose_for_auth_header", "fn expose_for_persisting"] {
            assert_eq!(
                block.matches(name).count(),
                1,
                "`{name}` 在 impl 块里应当恰好定义一次"
            );
        }
    }

    #[test]
    fn a_debug_print_never_carries_the_plaintext_or_its_length() {
        let k = SecretKey::new("sk-ant-THIS-MUST-NOT-BE-PRINTED");
        let printed = format!("{k:?}");
        assert!(
            !printed.contains("THIS-MUST-NOT-BE-PRINTED"),
            "`{{:?}}` 把明文印出来了：{printed}"
        );
        // 非空对照：它确实印了**点什么**（不是空串恒过）。
        assert_eq!(printed, "SecretKey(<已遮蔽>)");
        // 长度也不许漏 —— 一把 key 的长度能把候选集缩小一个量级。
        assert!(
            !printed.contains(&k.len().to_string()),
            "`{{:?}}` 把长度印出来了：{printed}"
        );
    }

    #[test]
    fn masking_keeps_only_the_two_ends_and_swallows_short_keys_whole() {
        let long = SecretKey::new("sk-ant-api03-ABCDEFGHIJKLMNOP");
        let m = long.masked();
        assert!(m.starts_with("sk-a"), "前 4 位应当留着，实得 {m}");
        assert!(m.ends_with("MNOP"), "后 4 位应当留着，实得 {m}");
        assert!(
            !m.contains("api03-ABCDEFGHIJKL"),
            "中段没被遮住：{m}"
        );
        assert_eq!(m.chars().count(), "sk-ant-api03-ABCDEFGHIJKLMNOP".chars().count());

        // ★ 短 key **整条遮掉**：留前后各 4 位等于把一把 8 字符的 key 交出去。
        let short = SecretKey::new("abcdefgh");
        assert_eq!(short.masked(), "********");
        assert!(
            !short.masked().contains("abcd"),
            "短 key 的前缀漏出去了：{}",
            short.masked()
        );
        // 非空对照：空的就是空的，不是一串星号。
        assert_eq!(SecretKey::new("   ").masked(), "");
    }

    #[test]
    fn is_configured_looks_at_presence_not_at_content() {
        assert!(!SecretKey::new("").is_configured());
        assert!(!SecretKey::new("  \t\n ").is_configured());
        assert!(SecretKey::new("x").is_configured());
    }
}
