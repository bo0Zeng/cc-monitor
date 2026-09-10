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
//! 🔴 **`N-F1c`（09-05）把这个「二选一」解掉了，本节留作来历，别当现行**：
//! 〔旧文逐字：「硬把后两个合了，只能二选一：要么 monitor 失去 Windows 路径，
//! 要么 daemon 失去对 `\` 的拒绝。」〕
//! 那个两难的前提是「`daemon` 只跑在 Linux 上，所以它可以把 `\` 当危险字符」。
//! `N-F1c` 起 **monitor 的本机账号清单也来问这个二进制**（`--list-accounts`），而 monitor 要在
//! Windows 上跑 ⇒ 那个前提没了。
//! 解法**不是**「合」，是把判据按它防的东西拆开（逐字照 `local_accounts.rs` 那段头注）：
//!   ① shell 元字符与视觉欺骗字符 = **平台无关的安全性质**，两侧同一套；
//!   ② 「是绝对路径」= **平台相关的形式**，各写各的。
//! `\` 从两侧的危险字符表里一起拿掉，由**下游那一层**（`config_dir_command_safe`，拼 POSIX 命令
//! 之前那一道）继续拒它 —— **分层校验**，不是放弃防守。
//! ⇒ 今天 `is_safe_config_dir` 两侧仍未合并，但**理由变了**：不是「合不了」，是
//! 「形式那一半本来就该各写各的」。

/// 账号库目录名（`$HOME` 下）。
pub const ACCTS_DIR_NAME: &str = ".claude-accts";
/// manifest 文件名（账号库目录下）。
pub const MANIFEST_NAME: &str = "accounts.json";
/// 凭据文件名（每个账号的 config dir 下）；只 stat 存在性，**绝不读内容**。
pub const CREDENTIALS_NAME: &str = ".credentials.json";
/// 本仓支持的 manifest schema 版本。**不支持的版本不是错误**，是「未启用多账号」。
///
/// # K-A1（08-24）：加 `authKind` 这一维**刻意不 bump** —— 裁决与排除见件计划 `KA6d`
///
/// 一句话理由：`RawAccount` 两侧都是 `#[serde(default)]` + 忽略未知键 ⇒ 加一个**可选键**
/// 是**双向兼容**的（新代码读旧 manifest、旧代码读新 manifest 都不炸）⇒ 按定义不是
/// breaking change。而 bump 会立刻造成真伤害：本常量的语义逐字是
/// 「不支持的版本 = 未启用多账号」⇒ 任何还没升级 `cc-acct-iso` 写侧的机器（manifest 仍写
/// `version: 1`）会被新读侧判成「没启用多账号」，**整张账号列表消失**。
pub const SUPPORTED_SCHEMA: u64 = 1;

// ---------------------------------------------------------------- 鉴权方式（K-A1）

/// 鉴权方式 = 订阅登录（`~/.claude` 那套 `.credentials.json`）。
pub const AUTH_KIND_SUBSCRIPTION: &str = "subscription";
/// 鉴权方式 = 第三方 / 官方 API key（**不**看订阅凭据文件）。
pub const AUTH_KIND_API_KEY: &str = "api-key";

/// 鉴权方式的**闭集**。
///
/// # 为什么是闭集而不是 `bool`
///
/// `unified-backend` 记着一次同形状的伤口：pidfile 的 `kind` 被写成「非 `interactive`
/// 即隐藏」，第三档来的时候那个布尔装不下。这一维今天就看得见第三档
/// （`apiKeyHelper` / bedrock / vertex 各是一种鉴权方式），所以从第一天就是**枚举**。
///
/// ⚠ **本数组只是「今天认识哪些字面量」，不是「将来只会有这两个」。**
/// 加第三档的步骤：这里加一个常量 + `auth_ready` 里给它一条规则 +
/// `src-tauri/src/accounts.rs::AuthKind` 加一个 variant（生成物会跟着变，TS 侧的
/// `switch` 少一支就 `tsc` 红）。
pub const AUTH_KINDS: [&str; 2] = [AUTH_KIND_SUBSCRIPTION, AUTH_KIND_API_KEY];

/// manifest 的 `authKind` 键 → 鉴权方式。**这是这条分类规则的唯一住址。**
///
/// - 缺席（旧 manifest / 写侧还没加这个键）⇒ [`AUTH_KIND_SUBSCRIPTION`]。
///   这不是猜：K-A1 Bx 复量过，`ANTHROPIC_API_KEY` / `ANTHROPIC_AUTH_TOKEN` /
///   `ANTHROPIC_BASE_URL` / `apiKeyHelper` 四个针在 `src/` + `src-tauri/src/` +
///   `shared/` + `remote-daemon-proto/src/` 下**全为零命中** ⇒ 今天存量账号**全部**是订阅号。
/// - 认不出的值（比如将来写侧先加了 `bedrock` 而读侧还没升）⇒ 也落
///   [`AUTH_KIND_SUBSCRIPTION`]，**这是刻意选的保守方向**：订阅这一档**保留**
///   「缺凭据 ⇒ 不可选」那道保护，用户看到的是「未登录」（看得见、可修），
///   而落到 api-key 那一档会让它**变成可选**却连不上（看不见的坏）。
pub fn auth_kind_from_manifest(raw: Option<&str>) -> &'static str {
    match raw {
        Some(s) if s == AUTH_KIND_API_KEY => AUTH_KIND_API_KEY,
        _ => AUTH_KIND_SUBSCRIPTION,
    }
}

/// 「鉴权方式这一维**不再阻塞**这个号被选中」。**这是这条规则的唯一住址** ——
/// 两个生产者（daemon `observe/accounts_query.rs` · monitor `local_accounts.rs`）都调它，
/// 所以「三个生产者各填一个不同默认值」那种漂移在结构上不可表示。
///
/// - 订阅号：凭据文件在不在（**逐字节旧行为** —— `logged_in` 原来就是这一格）。
/// - api-key 号：**恒真**，因为它压根不用那个文件。
///
/// ⚠ **`true` 不等于「真能连上」**（件计划 `KA6a`）：api-key 号今天还没有配端点的路，
/// 起会话会在 claude 那边报鉴权失败。⇒ UI **必须**把这个状态说出来
/// （徽章写「api-key（未配置端点）」而不是「已登录」，落点
/// `src/accounts.ts::accountStatusBadge`），否则这一维交付出去就是一个新的坏体验。
///
/// ⚠ 也不等于「凭据有效」（`KA6b`）：订阅那一支仍然只 stat 存在性，过期/吊销看不出来。
pub fn auth_ready(auth_kind: &str, credentials_present: bool) -> bool {
    match auth_kind {
        k if k == AUTH_KIND_API_KEY => true,
        _ => credentials_present,
    }
}

/// 跨生产者对拍夹具的**一格**：喂什么、该出什么。
///
/// 字段分两半：`manifest_auth_kind` / `credentials_present` 是**输入**，
/// `expect_auth_kind` / `expect_auth_ready` 是**期望**（写成字面量，**不是**由
/// [`auth_ready`] 算出来的 —— 算出来就成了循环自证）。两者的一致性由
/// `tests::the_golden_matches_the_rule` 单独钉住：改了规则不改金样、或反过来，都红。
pub struct AuthKindParityCase {
    /// 账号名，同时也是它在夹具根下的目录名。
    pub name: &'static str,
    /// manifest 里 `authKind` 键写什么；`None` = 这个键**不出现**。
    pub manifest_auth_kind: Option<&'static str>,
    /// 要不要在它的目录里放一个 [`CREDENTIALS_NAME`]。
    pub credentials_present: bool,
    /// 期望产出的 `authKind`（金样）。
    pub expect_auth_kind: &'static str,
    /// 期望产出的 `authReady`（金样）。
    pub expect_auth_ready: bool,
}

/// 跨生产者对拍的**唯一一份夹具**（K-A1 `KAY1` 的那条 acceptor）。
///
/// # 为什么住在生产 crate 里而不是一个 `fixtures/` 文件
///
/// 两个生产者住在**两个不同的 crate**（`remote-daemon-proto` 是 bin-only、刻意不进
/// workspace），它们唯一共享的东西就是本 crate。夹具放文件里要各写一份读法与各自的路径，
/// 那正是本 crate 存在的理由所反对的（「双写点必须有守卫」不如「让双写不可表示」）。
/// 代价如实写：这张表会编进两个二进制（约 300 字节的静态数据）。
///
/// # 六格各自在守什么
///
/// | 格 | 它在守什么 |
/// |---|---|
/// | `sub-cred` | 正常订阅号 —— 旧行为一格没变 |
/// | `sub-nocred` | ★ `KAY3` 阴性对照：订阅号缺凭据**仍然**不可选 |
/// | `api-nocred` | ★ `KAY2` 正题：api-key 号缺凭据**不再**被判不可用 |
/// | `api-cred` | api-key 号**有**凭据文件也一样 —— 它不看那个文件（防「其实还是在看文件」） |
/// | `legacy-nokind` | 旧 manifest（键缺席）⇒ 订阅（`KA6d` 的裁决落到判据上） |
/// | `bogus-kind` | 认不出的值 ⇒ 保守落订阅，**不是**落 api-key |
pub const AUTH_KIND_PARITY_CASES: [AuthKindParityCase; 6] = [
    AuthKindParityCase {
        name: "sub-cred",
        manifest_auth_kind: Some(AUTH_KIND_SUBSCRIPTION),
        credentials_present: true,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: true,
    },
    AuthKindParityCase {
        name: "sub-nocred",
        manifest_auth_kind: Some(AUTH_KIND_SUBSCRIPTION),
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: false,
    },
    AuthKindParityCase {
        name: "api-nocred",
        manifest_auth_kind: Some(AUTH_KIND_API_KEY),
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_API_KEY,
        expect_auth_ready: true,
    },
    AuthKindParityCase {
        name: "api-cred",
        manifest_auth_kind: Some(AUTH_KIND_API_KEY),
        credentials_present: true,
        expect_auth_kind: AUTH_KIND_API_KEY,
        expect_auth_ready: true,
    },
    AuthKindParityCase {
        name: "legacy-nokind",
        manifest_auth_kind: None,
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: false,
    },
    AuthKindParityCase {
        name: "bogus-kind",
        manifest_auth_kind: Some("oauth-device"),
        credentials_present: false,
        expect_auth_kind: AUTH_KIND_SUBSCRIPTION,
        expect_auth_ready: false,
    },
];

/// 把 [`AUTH_KIND_PARITY_CASES`] 渲染成一份 schema v1 的 manifest（`root` = 夹具根的绝对路径）。
///
/// 两侧的测试都调它 ⇒ **喂进去的那份 JSON 逐字节相同**，不然「三者产出相同」就成了
/// 「三者读的不是同一份输入」。目录与凭据文件由调用方按同一张表创建（见
/// [`AuthKindParityCase::credentials_present`]）。
pub fn auth_kind_parity_manifest(root: &str) -> String {
    let mut s =
        String::from("{\"version\":1,\"updatedAt\":\"2026-08-24T00:00:00Z\",\"sharedStore\":\"");
    s.push_str(root);
    s.push_str("/shared\",\"accounts\":[");
    for (i, c) in AUTH_KIND_PARITY_CASES.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str("{\"name\":\"");
        s.push_str(c.name);
        s.push_str("\",\"email\":\"");
        s.push_str(c.name);
        s.push_str("@x.test\",\"configDir\":\"");
        s.push_str(root);
        s.push('/');
        s.push_str(c.name);
        s.push_str("\",\"isDefault\":false,\"mode\":\"isolated\"");
        if let Some(k) = c.manifest_auth_kind {
            s.push_str(",\"authKind\":\"");
            s.push_str(k);
            s.push('"');
        }
        s.push('}');
    }
    s.push_str("]}");
    s
}

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

    // ---- K-A1：鉴权方式这一维 ----

    /// ★ **金样与规则互钉。** 改了 [`auth_ready`] 而没改金样、或反过来，这条红。
    ///
    /// 金样里的 `expect_auth_ready` 是**手写字面量**（不是算出来的），所以本条不是恒真：
    /// 把 `auth_ready` 的 api-key 那支改成 `credentials_present`，本条当场红两格。
    #[test]
    fn the_golden_matches_the_rule() {
        assert_eq!(
            AUTH_KIND_PARITY_CASES.len(),
            6,
            "金样格数变了 —— 先确认新格是不是也纳进了两侧的对拍"
        );
        for c in &AUTH_KIND_PARITY_CASES {
            assert_eq!(
                auth_kind_from_manifest(c.manifest_auth_kind),
                c.expect_auth_kind,
                "{}：分类规则与金样不一致",
                c.name
            );
            assert_eq!(
                auth_ready(c.expect_auth_kind, c.credentials_present),
                c.expect_auth_ready,
                "{}：就绪规则与金样不一致",
                c.name
            );
        }
    }

    /// 闭集里每一个字面量都要能被分类函数认出来 —— 否则「闭集」是装饰。
    #[test]
    fn every_declared_auth_kind_round_trips() {
        assert_eq!(
            AUTH_KINDS.len(),
            2,
            "加了第三档 ⇒ 先给它一条 auth_ready 规则"
        );
        for k in AUTH_KINDS {
            assert_eq!(
                auth_kind_from_manifest(Some(k)),
                k,
                "{k} 在闭集里却分类不出来"
            );
        }
    }

    /// ★ 缺席与认不出**都**落订阅，而且**不许**落 api-key。
    ///
    /// 反向也断一次：只有逐字 `api-key` 才是 api-key（大小写/空格/近似写法一律不算），
    /// 否则一个手抄错的 manifest 会静默把订阅号变成「可选但连不上」。
    #[test]
    fn unknown_and_absent_auth_kinds_fall_back_to_subscription_not_api_key() {
        for raw in [
            None,
            Some(""),
            Some("api_key"),
            Some("API-KEY"),
            Some(" api-key"),
            Some("apikey"),
            Some("bedrock"),
        ] {
            let got = auth_kind_from_manifest(raw);
            assert_eq!(
                got, AUTH_KIND_SUBSCRIPTION,
                "{raw:?} 被认成了 {got} —— 保守方向是订阅（缺凭据看得见），不是 api-key（连不上看不见）"
            );
        }
        assert_eq!(
            auth_kind_from_manifest(Some(AUTH_KIND_API_KEY)),
            AUTH_KIND_API_KEY,
            "逐字 api-key 反而没被认出来 —— 上面那批断言就成了空真"
        );
    }

    /// 订阅那一支**逐字节旧行为**：就绪 == 凭据文件在。
    #[test]
    fn subscription_readiness_is_still_exactly_the_credential_file() {
        assert!(auth_ready(AUTH_KIND_SUBSCRIPTION, true));
        assert!(!auth_ready(AUTH_KIND_SUBSCRIPTION, false));
        // api-key 那一支不看那个文件 —— 两种取值都真。
        assert!(auth_ready(AUTH_KIND_API_KEY, false));
        assert!(auth_ready(AUTH_KIND_API_KEY, true));
    }

    /// 夹具渲染器：路径真的被替换进去了，且 `manifest_auth_kind: None` 那格**没有**这个键
    /// （不是写成 `"authKind":null` —— 缺席与 null 在 serde 上是两件事）。
    #[test]
    fn the_parity_manifest_renders_absent_keys_as_absent() {
        let m = auth_kind_parity_manifest("/tmp/fixture-root");
        assert!(m.starts_with("{\"version\":1,"), "schema 版本得是 1：{m}");
        assert!(
            m.contains("\"configDir\":\"/tmp/fixture-root/sub-cred\""),
            "{m}"
        );
        assert!(
            m.contains("\"sharedStore\":\"/tmp/fixture-root/shared\""),
            "{m}"
        );
        assert!(!m.contains("null"), "缺席的键不许渲染成 null：{m}");
        // 六格里有五格带 authKind（`legacy-nokind` 那格不带）。
        assert_eq!(m.matches("\"authKind\"").count(), 5, "{m}");
        // 认不出的那格照原样写进去 —— 分类规则由读侧负责，夹具不许提前替它决定。
        assert!(m.contains("\"authKind\":\"oauth-device\""), "{m}");
    }

    /// 正常字符不许被误杀 —— 否则合法账号名会被拒。
    #[test]
    fn ordinary_characters_are_not_rejected() {
        for c in ['a', 'Z', '0', '_', '-', '.', '/', ' ', '中', 'é'] {
            assert!(!is_deceptive_char(c), "{c:?} 被误判成欺骗字符");
        }
    }
}
