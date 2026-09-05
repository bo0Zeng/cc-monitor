//! 那份文件**长什么样**，以及「读一份 / 改一个键 / 再写回去」这三步的**纯逻辑**。
//!
//! # ⚠ 本模块**一个文件系统调用都没有**，那是刻意的（不是「还没写」）
//!
//! 三条理由，每条都指向本仓一条真实存在的判据：
//!
//! 1. **daemon 只许读，不许写**〔`K-H2a` 裁四〕：`readonly_guard.rs` 扫 daemon 生产段
//!    （剥掉 `#[cfg(test)]` 块之后）断言**不含任何文件系统变更调用**，白名单**恰好一个模块**
//!    `control/fork_write.rs`。本 crate 被 daemon depend ⇒ 写调用放进来是在给那道护栏挖洞。
//! 2. **写盘落点必须被登记表看见**：`src-tauri/src/write_site_registry.rs` 与
//!    `atomic_replace_registry.rs` 的扫描根**都是 `src-tauri/src`**
//!    （两者的 `src_root()` 逐字是 `CARGO_MANIFEST_DIR/src`）——
//!    `src-tauri/crates/` **不在它们的人群里**。
//!    ⇒ 把写盘搬进本 crate，等于**让它从两张登记表底下溜出去**，而那正是本工作区在治的那族病
//!    （守卫的人群对不上它守的性质）。⇒ 真正的写盘留在 `src-tauri/src`，在人群里、要申报。
//! 3. 纯函数好测：`KS10` 要测的是**交错**（人改了 A，程序写 B，A 还在不在），
//!    而交错的关键是「程序在**写的那一刻**才去读」——那条纪律由调用方兑现，
//!    本模块只保证「给我旧内容 + 新 key，我还你一份没吃掉任何东西的新内容」。
//!
//! # 格式（`KS9`：人能读能改，改完就生效）
//!
//! 一份**明文 JSON 对象**。今天本件只认一个键 [`KEY_FIELD`]，**其余键一律原样留着**。
//! 文件不存在时给 [`TEMPLATE`] —— 一个「能手编但没人知道格式」的文件等于不能手编。

use crate::SecretKey;
use serde_json::{Map, Value};

/// key 住在哪个字段。**这个名字是契约**：人手编的时候写的就是它。
///
/// ⚠ 它在**两个深度**上都是这个名字：顶层（`K-H2a` 交付时那一把）与
/// [`ACCOUNTS_FIELD`] 里每一条账号内部。**刻意同名** —— 人手编时不必记两套词。
pub const KEY_FIELD: &str = "api_key";

/// 多账号那张表住哪个字段。**这个名字是契约**〔`K-H2` `KH5`〕。
///
/// 形状：`{"accounts": {"<账号 id>": {"api_key": "…", "base_url": "…"}}}`。
/// `<账号 id>` 会**原样**变成路由键里那一段（`/s/<agent>/<账号 id>/<key>/…`）
/// ⇒ 它必须是路由段放得下的字符；放不下的那一条**永远匹配不上**，
/// 由中转起来时出声（`relay::table::build` 那条「这一行进不了表」）。
pub const ACCOUNTS_FIELD: &str = "accounts";

/// 一条账号的**上游端点**住哪个字段。缺席 / 空串 ⇒ 用中转启动时那个默认上游。
///
/// ⚠ 它**不是**「回落」：`base_url` 缺席说的是「这一行用默认端点」，
/// 而「这一行根本不在表里」说的是**404**。两件事不许混 —— 见 `K-H2` `KH2`。
pub const BASE_URL_FIELD: &str = "base_url";

/// 一条账号**怎么把 key 交给上游**住哪个字段〔`K-R1`〕。缺席 / 空串 ⇒ [`AuthStyle::DEFAULT`]。
///
/// ⚠⚠ **它说的只有「鉴权头怎么写」这一件事，不是「上游说哪种方言」。**
/// 这两件事读起来像，混成一个字段就是本工作区最贵的那族病（一个值装了两件事）：
/// 中转对请求体**一个字节都不解析**（`relay/` 生产段零 `serde_json`，`K-R1 §0c` 乙路现打），
/// ⇒ 它没有资格声称自己知道上游要哪种 body。方言那一格若将来真要买，
/// 是**另一个字段**（`K-R1` 的 `KR12`/`KR13`），不是给本字段多加几个值。
pub const AUTH_STYLE_FIELD: &str = "auth_style";

/// 顶层那把 key（`K-H2a` 交付时的形状）在表里**叫什么名字**。
///
/// # ⚠⚠ 它是一个**有名字的行**，不是「默认行」
///
/// 这两件事读起来像，差别却正是 `K-H2` `KH2` 要守的全部：
/// - **有名字的行**：只有路由键里账号段**逐字**是 `default` 的请求才用它；别的账号段查不到 ⇒ **404**。
/// - **默认行**（本件明令不做）：查不到就拿它顶上 ⇒ **拿 A 的 key 发 B 的请求**。
///
/// 它存在的理由只有一个：`K-H2a` 已经落地的那份文件（顶层一个 `api_key`）
/// **升级之后要照常能用**，而不是变成「一份读不懂的旧文件」。
///
/// # ⚠⚠ `K-H2c` `KH2C3` 逐字：**它是读得出来的一行，但不再是写进去的地方**
///
/// 这两句必须一起读，只读一句都会读错一格：
/// - **读**：[`read_accounts`] 今天仍然把顶层那一把折成一条 id 为本常量的行 ——
///   老用户手上那份文件、以及 `KS9` 那条「脱离这个前端也能配」的手编路，都照常能用。
///   **谁都不许顺手删它**（删掉就是打掉老用户手上那份文件），
///   由 `the_legacy_top_level_key_becomes_one_named_row_not_a_default_row` 与
///   `an_unconfigured_file_yields_no_rows_at_all` 两条钉着。
/// - **写**：界面那条路（`creds_store::write_key_at`）落的是 `accounts.<id>`，
///   **一个字节都不再往顶层那一格写**。机检住 monitor 侧的
///   `the_write_side_no_longer_targets_the_legacy_top_level_slot`。
///
/// ★ 为什么写侧非换不可（这不是洁癖）：写顶层那一格 ⇒ 读回来 id 逐字是本常量，
/// 而起会话那一侧按**账号目录末段名**索引 ⇒ **从界面配的 key 永远匹配不上任何账号**，
/// 中转按 `KL7` 第 2 条给 404、一个字节不发上游。
/// 〔`K-H2c` `§0` 的立件读数，09-02 在写侧接上之前仍然属实。〕
///
/// ⚠ 而 [`merge_key`]（写顶层那一格的那个纯函数）**没有被删**：
/// 它仍是 [`merge_account_key`] 内部改「某一条里那个 `api_key`」的实现。
/// **「不再往顶层写」是调用面的事实，不是这个模块少了一个函数。**
pub const LEGACY_ACCOUNT_ID: &str = "default";

/// 那份文件相对 claude 家目录的位置。**两侧共用的唯一契约。**
///
/// # 为什么住这里，而不是两边各写一份字面量
///
/// monitor 与 daemon 各有自己的「家目录」解析（前者 `paths::resolve_monitor_data_dir`，
/// 后者 `agents::claudecode::paths::resolve_home`），但**落点的相对路径必须是同一个** ——
/// 两边各写一份字符串，漂开的那天没有任何东西会说，而症状是
/// 「界面上配好了，中转说没配」这种**查不出来**的形状。
///
/// ⚠ 它**不**跟随 `claudeDir` 覆盖（monitor 自己的数据目录本来就不跟随，见
/// `src-tauri/src/config.rs` 头注逐字：「monitor 自己的设置永远在默认
/// `~/.claude/claudecode-frontend/` 下，不跟随 `claudeDir` 字段变化」）。
pub const FILE_NAME: &str = "relay-credentials.json";

/// `<claude 家目录>/claudecode-frontend/relay-credentials.json`。
pub fn path_under_claude_home(home: &std::path::Path) -> std::path::PathBuf {
    home.join("claudecode-frontend").join(FILE_NAME)
}

/// 文件不存在 / 是空的时候给出去的模板。
///
/// ⚠ 它是**注释 + 一个空字段**，不是一份能直接用的配置 —— 目的是让人一眼看出该填哪儿。
/// JSON 没有注释语法，所以说明写成一个**未知键**（`_note`），而
/// 「未知键原样保留」正是 [`merge_key`] 的性质 ⇒ 这份模板**自己就是那条性质的用例**。
///
/// # ⚠ 它刻意**不列举** `auth_style` 的合法值〔`K-R1`，`brief` 13b〕
///
/// 那个闭集只有一个住址（[`AuthStyle::ALL`]）。在这里再抄一份，加第四个成员的那天
/// 这份模板会**静默变旧**，而它是随产物发到用户机器上的那一份。
/// ⇒ 模板只点名字段，合法值由中转启动时**现算**印出来（`relay::creds::announce`）。
pub const TEMPLATE: &str = r#"{
  "_note": "把第三方 API key 填进 api_key。这份文件可以直接用编辑器改，改完下次读就生效；也可以整份换成另一份 JSON（导入）。本文件之外的键不会被程序动。",
  "_note_accounts": "多账号写进 accounts：每条一个 id（会原样出现在中转的路由键里，只许字母数字与 - _），每条可带 api_key、base_url 与 auth_style。base_url 留空就用中转启动时那个默认上游，写全路径（含网关前缀）也认；api_key 留空就原样转发客户端自己那份鉴权头。例：\"accounts\": { \"my-account\": { \"api_key\": \"sk-...\", \"base_url\": \"https://api.example.com\" } }",
  "_note_auth_style": "auth_style 说的是「这一把 key 用哪种鉴权头交给上游」，不是「上游说哪种方言」—— 中转对请求体一个字节都不解析。留空就用今天的默认。写了一个认不出的词不会被悄悄当默认：中转起来时会逐条说出来，并把它认得的那几个值现算着印在同一屏。本地部署（不校验凭据的那种）要的就是「一个鉴权头都不发」那一档。",
  "accounts": {},
  "api_key": ""
}
"#;

/// 读不动那份文件时的说法。**三态，不是两态** —— 「不存在」与「读坏了」必须分开：
/// 前者是正常的（还没配），后者要出声（人手编时打错了一个逗号，不该被当成「没配」）。
#[derive(Debug, PartialEq, Eq)]
pub enum StoreError {
    /// 不是合法 JSON。带上解析器的话，人手编打错时能看懂。
    NotJson(String),
    /// 是合法 JSON，但顶层不是对象（例如整份是个数组）。
    NotAnObject,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotJson(e) => write!(
                f,
                "凭据文件不是合法 JSON（{e}）。它是人能手编的明文 JSON —— \
                 多半是少了逗号或引号。**没有动它**，修好再试。"
            ),
            StoreError::NotAnObject => write!(
                f,
                "凭据文件的顶层不是一个 JSON 对象。正确形状见模板：{TEMPLATE}"
            ),
        }
    }
}

/// 解析一份盘上的内容。**空内容 = 空对象**（还没配，不是错）。
pub fn parse(raw: &str) -> Result<Map<String, Value>, StoreError> {
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    let v: Value = serde_json::from_str(raw).map_err(|e| StoreError::NotJson(e.to_string()))?;
    match v {
        Value::Object(m) => Ok(m),
        _ => Err(StoreError::NotAnObject),
    }
}

/// 从一份已解析的文档里取 key。没有 / 不是字符串 / 是空串 ⇒ `None`（都当成「还没配」）。
///
/// ⚠ **不许在这里对 key 做任何「顺手清理」**（去引号、剥前缀…）：
/// 人手编写进去什么，上游就该收到什么。猜错一次的代价是一条查不出来的 401。
/// 只 `trim` 首尾空白 —— 那是编辑器留下的，不是人的意思。
pub fn read_key(doc: &Map<String, Value>) -> Option<SecretKey> {
    let s = doc.get(KEY_FIELD)?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    Some(SecretKey::new(s))
}

/// 从一份已解析的文档里取上游端点。缺席 / 不是字符串 / 空串 ⇒ `None`（用默认上游）。
///
/// ⚠ 与 [`read_key`] 同一条纪律：**只 `trim` 首尾空白，别的一个字节都不动**。
/// 「顺手补个 `https://`」这种清理猜错一次的代价是**连到另一个地方去**。
fn read_base_url(doc: &Map<String, Value>) -> Option<String> {
    let s = doc.get(BASE_URL_FIELD)?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    Some(s.to_string())
}

/// 这一行的 key **用哪种鉴权头交给上游**〔`K-R1`〕。
///
/// # ★ 为什么这三个值是「头怎么写」，而不是「哪一家供应商」
///
/// 〔用 09-04〕逐字点了「供应商deepseek\kimi\qwen\等等」，并要「api做成通用的,
/// 还可以接本地部署的」。**按供应商枚举做不到「通用」** —— 每加一家就得加一条路，
/// 而那张表永远落后于现实。⇒ 本枚举按**协议形状**切：今天现实里只有两种鉴权头形状
/// （一个 `Authorization: Bearer`、一个 `x-api-key`）加一种「不发」。
/// 供应商是谁**不进代码**：DeepSeek / Kimi / Qwen 用哪一种，是那一行 `auth_style`
/// 里写着的，不是本 crate 里判的。
///
/// # ⚠ 它**不**说什么（射程如实写）
///
/// 不说上游要哪种请求体、不说路径长什么样、不说用哪个模型名。
/// 中转对 body 零解析 ⇒ 这三样今天全由客户端决定，本字段一个字都管不到。
///
/// # 值的**闭集只有一个住址** —— [`AuthStyle::ALL`]
///
/// 散文里不复述成员（`brief` 13b）：要印出来就现算 [`AuthStyle::field_value`] 再 `join`。
/// [`AuthStyle::from_field_value`] 也是从 `ALL` 派生的 ⇒ **没有第二份字面量会漂**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStyle {
    /// `Authorization: Bearer <key>`。**这是今天盘上的行为**，也是缺席时的默认
    /// ⇒ 一份没写 `auth_style` 的旧文件，上游收到的字节与本件之前**逐字节相同**。
    Bearer,
    /// `x-api-key: <key>`。
    XApiKey,
    /// **一个鉴权头都不发**，并且把客户端自带的那几个也丢掉。
    ///
    /// ★ 这一档是**本地部署那一格**（〔用 09-04〕「还可以接本地部署的」）：
    /// 本机跑的推理服务默认不校验凭据，而「把一把真 key 发给一个不校验的本地端点」
    /// 是把凭据白送出去。⇒ 无鉴权要能**显式表示**，不是靠「不配 key 恰好也能用」。
    ///
    /// ⚠ 它与「这一行不配 key」**不是一回事**，两者的区别是承重的：
    /// 不配 key ⇒ **原样转发客户端那份鉴权头**（订阅登录那一档要的正是这个）；
    /// 本档 ⇒ **连客户端那份也不转发**。
    NoAuth,
}

impl AuthStyle {
    /// 缺席 / 空串时用哪一个。**取今天的行为**，不是取「最安全的那个」——
    /// 换默认值等于给每一份既有文件改行为，那是另一次决定，要另一件去做。
    pub const DEFAULT: AuthStyle = AuthStyle::Bearer;

    /// 这个闭集的**唯一住址**。判据与日志都从这里派生，不许再写第二份字面量。
    pub const ALL: &'static [AuthStyle] = &[AuthStyle::Bearer, AuthStyle::XApiKey, AuthStyle::NoAuth];

    /// 人在文件里写的那个词。**这是契约**（手编时写的就是它）。
    ///
    /// ⚠ 刻意不叫 `"anthropic"` / `"openai"`：那两个词说的是**方言**，
    /// 而本枚举一个字节的 body 都不看 —— 用方言名给它命名，会让读的人以为
    /// 配了它中转就会替他翻译。**它不会。**
    pub fn field_value(self) -> &'static str {
        match self {
            AuthStyle::Bearer => "bearer",
            AuthStyle::XApiKey => "x-api-key",
            AuthStyle::NoAuth => "none",
        }
    }

    /// 反过来：文件里那个词 → 这个值。认不出就是 `None`（调用方要**出声**，不许回落）。
    ///
    /// ★ 它**从 [`AuthStyle::ALL`] 派生**，不是第二个 `match`：加一个成员，
    /// 这一头自动跟上；写成第二个 `match` 的话，漏一支就是一次静默回落。
    pub fn from_field_value(s: &str) -> Option<AuthStyle> {
        let s = s.trim();
        AuthStyle::ALL
            .iter()
            .copied()
            .find(|v| v.field_value().eq_ignore_ascii_case(s))
    }
}

/// 文件里那一格 `auth_style` 的**三种状态**〔`K-R1`〕。
///
/// # ⚠ 为什么是三态而不是 `Option<AuthStyle>`
///
/// 「缺席」与「写了一个认不出的词」**必须分开**：合成一态就只能回落成默认，
/// 而一个打错字母的 `auth_style` 静默走 Bearer 的症状是**一条查不出来的 401**
/// ——与「key 打错了」同形。〔`K-R21` 那一族逐字：一个 `None` 装了三件事。〕
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStyleSetting {
    /// 字段缺席 / 空串 ⇒ 用 [`AuthStyle::DEFAULT`]。
    Absent,
    /// 认得的那几个之一。
    Known(AuthStyle),
    /// 字段在、但不是认得的那几个之一。
    ///
    /// ⚠ **它刻意什么都不带** —— 不留那个原字符串。理由：它下一跳的落点是
    /// 中转的启动日志，而 `creds_guard` 那张白名单要的正是「进日志的东西不含文件内容」。
    /// 认不出的那个词长什么样，人自己看那份文件；日志只说「这一条的 auth_style 认不出」。
    Unknown,
}

/// 从一份已解析的（子）文档里取 `auth_style`。
///
/// ⚠ 与 [`read_key`] / `read_base_url` 同一条纪律：只 `trim` 首尾空白。
/// 值不是字符串（有人写成数字 / 对象）⇒ 与「写了个认不出的词」**同归 `Unknown`**：
/// 两者都是「人写了东西但我读不懂」，都该出声。
pub fn read_auth_style(doc: &Map<String, Value>) -> AuthStyleSetting {
    let Some(v) = doc.get(AUTH_STYLE_FIELD) else {
        return AuthStyleSetting::Absent;
    };
    let Some(s) = v.as_str() else {
        return AuthStyleSetting::Unknown;
    };
    if s.trim().is_empty() {
        return AuthStyleSetting::Absent;
    }
    match AuthStyle::from_field_value(s) {
        Some(k) => AuthStyleSetting::Known(k),
        None => AuthStyleSetting::Unknown,
    }
}

/// 表里的一条。**上游与 key 在这里还是分开的两个值** ——
/// 把它们焊成一个不可分解的值是**中转那一侧**的活（`relay::table` 的 `Row`）。
///
/// ⚠ **刻意不 `derive(Debug)`**：同 `relay::server::Relay` 那条（`KS1` 的第二道）。
/// `SecretKey` 自己的 `Debug` 是遮蔽形，但**少一个能顺手印整条的入口就少一个出口**。
pub struct AccountEntry {
    /// 路由键里那一段账号 id。
    pub id: String,
    /// 这一行的上游端点；`None` = 用中转启动时那个默认上游。
    pub base_url: Option<String>,
    /// 这一行的 key；`None` = **原样转发下游那份鉴权头**（订阅制那一档是合法状态）。
    pub key: Option<SecretKey>,
    /// 这一行的鉴权头风格〔`K-R1`〕。**三态原样带出去，本模块不替它做决定** ——
    /// 同 `base_url`：把 `Unknown` 折成默认值是一次静默回落，而出声那一步在中转那侧
    /// （它才有日志出口）。
    pub auth_style: AuthStyleSetting,
}

/// 把一份文档读成**一张表**〔`K-H2` `KH5`〕。
///
/// # 两个来源，合成一张表，**顺序由键名定**
///
/// 1. [`ACCOUNTS_FIELD`] 那张表 —— 逐条读。遍历走 [`ordered_keys`]，
///    ⇒ 输出顺序与 `Map` 今天是 `BTreeMap` 还是 `IndexMap` **无关**。
/// 2. 顶层那把 key / 那个 `base_url` ⇒ 一条 id 逐字是 [`LEGACY_ACCOUNT_ID`] 的行。
///
/// # ⚠ 三条判断都要说清（每一条都对应一种「读起来像另一件事」的形状）
///
/// - **`accounts` 里每一个对象都是一条**，两个字段一个都没填也算 ——
///   那是**合法且有用**的一条：`base_url` 空 = 用默认上游，`api_key` 空 = **原样转发
///   客户端自己那份鉴权头**（`K-H1` 甲半那条「不配凭据也能用的透传路」，
///   在多账号之后它从「隐式的全局行为」变成「**显式的一条路**」）。
///   ⚠ 值**不是对象**的那一条跳过（比如有人写成 `"acct": "sk-..."`）。
/// - **`accounts` 里已经有同名的那一条 ⇒ 顶层那半不再加**。理由：人手写的那条优先，
///   程序不许拿一份「历史形状」去盖掉人明确写下的东西。
/// - **顶层完全没有 `api_key` / `base_url` ⇒ 一条都不加**（不是加一条空的）。
///   ⚠⚠ **这一条是承重的**：加一条「什么都没配的默认行」等于给「查不到就用它」
///   开了门，而那正是 `K-H2` `KH2` 逐字禁的**回落**。**没配就是零条，零条就是全部 404。**
///
/// # 它**不**做什么
///
/// 不判 `id` 能不能当路由段用（那要 `route::segment_is_safe`，住 daemon 那一侧，
/// 本 crate 刻意不认识 HTTP）· 不解析 `base_url`（那要 `upstream::Base`，同上）。
/// ⇒ **这两格由中转在装表那一刻判并出声**，本函数只负责「文件里写了什么」。
pub fn read_accounts(doc: &Map<String, Value>) -> Vec<AccountEntry> {
    let mut out: Vec<AccountEntry> = Vec::new();

    if let Some(Value::Object(m)) = doc.get(ACCOUNTS_FIELD) {
        for id in ordered_keys(m.keys()) {
            let Some(obj) = m[id].as_object() else { continue };
            out.push(AccountEntry {
                id: id.clone(),
                base_url: read_base_url(obj),
                key: read_key(obj),
                auth_style: read_auth_style(obj),
            });
        }
    }

    if !out.iter().any(|e| e.id == LEGACY_ACCOUNT_ID) {
        let key = read_key(doc);
        let base_url = read_base_url(doc);
        // ⚠⚠ **顶层的 `auth_style` 单独在，不足以造出这一行**〔`K-R1`〕：
        //   它进不了上面那个 `if` 的条件，是刻意的 —— 加进去就等于「只写了
        //   `auth_style` 的文件也有一条什么都没配的 default 行」，而那正是本函数头注
        //   逐字禁的**回落**。⇒ 它只在这一行**因为别的字段已经存在**时被读进来。
        if key.is_some() || base_url.is_some() {
            out.push(AccountEntry {
                id: LEGACY_ACCOUNT_ID.to_string(),
                base_url,
                key,
                auth_style: read_auth_style(doc),
            });
        }
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// **`KS10` 在多条形状下的正主**〔`K-H2` `KH5`〕：只改 `accounts.<id>.api_key` 那一格，
/// **别的条一个字节都不动**，两层的未知键都留着。
///
/// # 为什么它不收「整张 accounts」
///
/// 件计划 `§0d 三㈣` 那条：**公开面上不许有「整份替换 accounts 子对象」的路**。
/// 有那条路，「只改一条」就退化成「调用方记得只改一条」——而那正是本仓反复栽的形状。
/// ⇒ 签名逼着调用方说清**改哪一条**，改别的条这件事在本模块的公开面上**不可表示**。
///
/// ⚠ **如实说它的分母**：它挡住的是**走本模块的写者**。盘上那份文件仍然可以被别的代码
/// 整份覆盖（原子替换那一步就在 `src-tauri/src/creds_store.rs` 里）⇒
/// 这一格守的是「走 `store` 的写者」，**不是「所有写者」**。
///
/// # 落盘出口仍然只有一处
///
/// 本函数**不自己碰明文** —— 它把那一格转交给 [`merge_key`]，
/// 而 `expose_for_persisting` 的调用点仍然**恰好 1 处**（在 `merge_key` 里）。
/// ⇒ `KS2` 那条相等断言一个字节都不用动（`K-H2` `KH3`）。
pub fn merge_account_key(
    current: &Map<String, Value>,
    id: &str,
    key: &SecretKey,
) -> Map<String, Value> {
    let mut out = current.clone();

    // ★ 两层都是 clone-then-replace：顶层的未知键、这一条之外的每一条、
    //   以及**这一条自己**的未知键，三样都留着。
    let mut accounts = match out.get(ACCOUNTS_FIELD).and_then(Value::as_object) {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    let entry = match accounts.get(id).and_then(Value::as_object) {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    accounts.insert(id.to_string(), Value::Object(merge_key(&entry, key)));
    out.insert(ACCOUNTS_FIELD.to_string(), Value::Object(accounts));
    out
}

/// **`KS10` 的正主**：把 key 并进一份**刚从盘上读回来的**文档，其余键一个不动。
///
/// # 它为什么收 `current` 而不是收一个 `&mut self`
///
/// `daemon_policy.rs` 头注逐字记着本仓踩过的那个形状：同一个文件有两个写者时，
/// 「前端『读—改—写』整份的那一刻，会把 Rust 刚写进去的键按一份**陈旧副本**覆盖掉」。
/// **本件是同一个形状换了两个当事人（程序 vs 人手）。**
/// ⇒ 签名逼着调用方在**写的那一刻**把盘上的当前内容递进来，
/// 而不是递一份「界面打开时读的那一份」。这条约束写在类型上，不写在注释里。
pub fn merge_key(current: &Map<String, Value>, key: &SecretKey) -> Map<String, Value> {
    let mut out = current.clone();
    out.insert(
        KEY_FIELD.to_string(),
        // ★ 用的是**落盘那个出口**，不是换头那个。两个出口的人群刻意不相交，
        //   见 `SecretKey::expose_for_persisting` 的头注。
        Value::String(key.expose_for_persisting().to_string()),
    );
    out
}

/// 键的落盘顺序。**抽成一个纯函数，理由是一次实测证伪。**
///
/// # ⚠ 它为什么不能留在 `to_pretty_json` 里（`MU9`，08-27 实测）
///
/// 排序那一步原来内联在 `to_pretty_json` 里，判据是
/// `the_field_order_does_not_depend_on_the_map_implementation`：
/// 造两份**内容相同、插入顺序相反**的文档，断言落盘文本相同。
/// **变异实测：把 `keys.sort()` 整条删掉，19 条判据全绿。**
///
/// 根子是**判据的人群与它守的性质对不上**：`serde_json::Map` 在**这台机器今天这份构建**里
/// 是 `BTreeMap`（默认 feature），插进去就已经有序 ⇒ 「两份插入顺序相反的文档」这个夹具
/// **根本造不出来**，两边喂进去的本来就是同一个有序结构。
/// 而这条判据要守的恰恰是「**哪天 `preserve_order` 被依赖图里任何一个 crate 打开、
/// `Map` 变成 `IndexMap`（插入序）**，输出仍然稳定」——那一天今天造不出来。
///
/// ⇒ 把排序抽成一个**收迭代器**的纯函数：判据可以直接喂它一个乱序的序列，
/// 与 `Map` 今天是哪种实现**无关**。`MU9` 重打后被 `ordering_is_by_name_not_by_arrival` 逮住。
///
/// # ⚠⚠ 订正〔`K-H2` `D1` 阻-3 回修，08-28〕：上面那句「那一天今天造不出来」**已被证伪**
///
/// 上一段推理**前半对、后半错**。对的是「人群与性质对不上」；
/// 错的是把「**这台机器今天这份构建**是 `BTreeMap`」滑成「**那个夹具造不出来**」——
/// 后者是一个**关于分母的全称句**，而它**没有被打过**。
/// 打它只要一行：给本 crate 的 `[dev-dependencies]` 加 `serde_json`
/// 并开 `preserve_order` ⇒ **判据构建里 `Map` 就是 `IndexMap`**，夹具当场造得出来。
/// 实测：删掉 [`ordered_value`] 的递归 ⇒ **30 passed / 2 failed**（两份 `Cargo.lock` 零变化）。
/// ⇒ 抽出纯函数那一步**仍然是对的**（它不依赖任何 feature），
/// 但「端到端那条只能是安慰剂」这个结论**当年就下早了**。
/// ★ 这条经过值一句纪律：**「造不出来」要先被打一次才算数。**
pub fn ordered_keys<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let mut v: Vec<&'a String> = keys.collect();
    v.sort();
    v
}

/// 递归地把每一层对象的键按名字排好〔`K-H2` `KH5c`〕。
///
/// # 它补的是一个**今天就已经漏着**的洞（`K-H2` `Bx` 摸底发现）
///
/// `to_pretty_json` 先前只排**顶层**（`ordered_keys(doc.keys())`），嵌套值是
/// `doc[k].clone()` 原样塞回去、由 `serde_json::to_string_pretty` 按那个**内层 `Map`
/// 自己的顺序**输出。而 [`ordered_keys`] 存在的**全部理由**（见它的头注）就是
/// 「哪天 `preserve_order` 被依赖图里任何一个 crate 打开、`Map` 变成 `IndexMap`」——
/// **那条理由在深度 ≥2 上原样复现，而先前的实现够不到。**
/// 多账号把每条账号做成一个嵌套对象 ⇒ 不补这一格，`KS10②` 在多条形状下**静默失效**。
///
/// # ⚠⚠ 数组**不排**，但要**递归进去**
///
/// 数组的顺序是**数据**（换了就是改内容），对象的键序不是。这两件事不许混。
///
/// # ★★★ 它**有牙** —— 而买到这颗牙的过程本身是一课，逐字记下来
///
/// `K-H2` `C` 阶段我在这里写下过一句**错话**，逐字是：
/// 「`serde_json::Map` 在今天这份构建里是 `BTreeMap`（插进去就有序）⇒
/// **造不出一个「嵌套层乱序」的夹具**」，并据此把本函数登记成
/// 「今天没有牙、是给明天用的绊线」。**`D1` 审计实测证伪了它。**
///
/// 证伪的办法只有一行：给本 crate 的 `[dev-dependencies]` 加
/// `serde_json = { features = ["preserve_order"] }` —— **判据构建里 `Map` 就成了 `IndexMap`**，
/// 那个「造不出来」的夹具当场造得出来。读数（我自己复打，与 `D1` 逐字相同）：
/// **原码 32 passed / 0 failed；把下面 `Value::Object` 那一支删成 `other.clone()`
/// ⇒ 30 passed / 2 failed**（本函数那条 + 隔壁 `the_field_order_does_not_depend_on_the_map_implementation`）。
/// 两份 `Cargo.lock` **零变化**，全量门禁三个数**逐个不变**。
///
/// ⚠⚠ **这一课是 `MU9` 那一课的第二次，而且更贵**：`MU9` 的结论是
/// 「这台机器上造不出反例」，本轮**证明了造得出**。
/// ⇒ **「造不出来」这个判断本身要先被打一次才算数** —— 它是一个**关于分母的全称句**，
/// 而全称句不许当前提直接写进头注。写下它的那一刻我没有去打它，这就是那次的病灶。
///
/// ⚠ 它仍然**不**保什么：本函数只管**对象的键序**。数组那一格由
/// `arrays_keep_their_order_because_that_order_is_data` 钉，不由本条钉。
pub fn ordered_value(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut out = Map::new();
            for k in ordered_keys(m.keys()) {
                out.insert(k.clone(), ordered_value(&m[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(ordered_value).collect()),
        other => other.clone(),
    }
}

/// 序列化成盘上那份文本。
///
/// # `KS10②` 字段顺序稳定 —— 而且**不靠 `serde_json` 的默认行为**
///
/// `serde_json::Map` 默认是 `BTreeMap`（有序），但开了 `preserve_order` feature 之后
/// 它变成 `IndexMap`（插入序）。**那个 feature 由依赖图里任何一个 crate 打开都算数**，
/// 而 monitor 那棵树很大 ⇒ 「今天是有序的」不是一条能靠的性质。
/// ⇒ 这里**自己排一次序**，两种情况下输出都一样。
/// 由 `the_field_order_does_not_depend_on_the_map_implementation` 钉住。
/// ⚠ **订正〔`K-H2` `KH5c`，08-28〕**：这里先前只排**顶层**一层。
/// 多账号把每条账号做成嵌套对象之后，那条「不靠 `Map` 的默认行为」的承诺在深度 ≥2 上就断了。
/// ⇒ 今天整份走 [`ordered_value`]（**递归**），它的诚实边界写在那个函数的头注里。
pub fn to_pretty_json(doc: &Map<String, Value>) -> String {
    let ordered = ordered_value(&Value::Object(doc.clone()));
    let mut s = serde_json::to_string_pretty(&ordered).unwrap_or_else(|_| TEMPLATE.to_string());
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `KS9①`：格式是**明文 JSON**，不是二进制、不是密文块。
    ///
    /// 量法：把一份带 key 的文档序列化出来，断言**用一个通用 JSON 解析器读得回来**，
    /// 且 key 的明文**逐字**出现在文本里（这一条正是「明文」的定义，不是缺陷）。
    #[test]
    fn what_lands_on_disk_is_plain_json_a_human_can_read() {
        let doc = merge_key(&Map::new(), &SecretKey::new("sk-ant-PLAINTEXT-ON-PURPOSE"));
        let text = to_pretty_json(&doc);
        let back: Value = serde_json::from_str(&text).expect("落盘的东西必须是合法 JSON");
        assert_eq!(back[KEY_FIELD], "sk-ant-PLAINTEXT-ON-PURPOSE");
        // 明文就是明文 —— 这一档**刻意**不加密（理由见 crate 头注：单机加密是安全剧场）。
        assert!(text.contains("sk-ant-PLAINTEXT-ON-PURPOSE"));
        // 非空对照：它确实是一份**对象**，不是被塞进一个字符串里的转义 JSON。
        assert!(back.is_object());
    }

    /// `KS9③`：**导入** —— 把一份现成 JSON 放进去就算配好，不需要任何迁移步骤。
    #[test]
    fn a_hand_authored_json_is_accepted_as_is_with_no_migration_step() {
        // 一份**人手写**的文件：有未知键、有注释性字段、键的顺序是人的顺序。
        let hand = r#"{
            "_note": "这是我自己写的",
            "api_key": "sk-ant-HAND-WRITTEN",
            "endpoint_hint": "https://example.invalid"
        }"#;
        let doc = parse(hand).expect("人手写的 JSON 应当直接被接受");
        let k = read_key(&doc).expect("应当读得到 key");
        assert_eq!(k.expose_for_auth_header(), "sk-ant-HAND-WRITTEN");
        // 没有任何「版本号 / schema 迁移」这一步：读出来就能用。
        assert!(doc.get("_schema_version").is_none());
    }

    /// `KS9`：文件不存在时要给一份模板 —— 否则「导入」这条要靠猜。
    #[test]
    fn the_template_is_itself_a_valid_store_and_names_the_field() {
        let doc = parse(TEMPLATE).expect("模板自己必须是合法的一份 store");
        assert!(doc.contains_key(KEY_FIELD), "模板里没点名那个字段");
        // 模板里的 key 是空的 ⇒ 读出来是「还没配」，不是一把假 key。
        assert!(read_key(&doc).is_none(), "模板不该看起来像已经配好了");
        // 模板自己就带一个未知键（那句说明），它是「未知键会被保留」的活用例。
        assert!(doc.contains_key("_note"));
    }

    /// `KS10①`：保留未知键。**人加的字段不许被抹掉。**
    #[test]
    fn merging_a_key_never_swallows_anything_the_human_wrote() {
        let hand = parse(r#"{"_note":"别删我","api_key":"OLD","zzz_last":1,"aaa_first":[1,2]}"#)
            .expect("夹具应当可解析");
        let merged = merge_key(&hand, &SecretKey::new("NEW"));
        assert_eq!(merged[KEY_FIELD], "NEW", "key 应当被换成新的");
        // 分母 = 夹具里除 key 外的这 3 个键，逐个查。
        assert_eq!(merged["_note"], "别删我");
        assert_eq!(merged["zzz_last"], 1);
        assert_eq!(merged["aaa_first"], serde_json::json!([1, 2]));
        assert_eq!(merged.len(), 4, "键数变了 —— 有东西被吃掉或凭空多出来了");
    }

    /// ★★ `KS10②` 的**正主**：顺序按**名字**定，不按**到达先后**定。
    ///
    /// 它直接喂 `ordered_keys` 一个乱序序列 ⇒ 与 `serde_json::Map` 今天是
    /// `BTreeMap` 还是 `IndexMap` **无关**。
    /// 〔立项理由：`MU9` 实测把排序删掉，隔壁那条端到端判据 19 全绿 —— 详见 `ordered_keys` 头注。〕
    #[test]
    fn ordering_is_by_name_not_by_arrival() {
        let arrival: Vec<String> = ["zzz", "mmm", "aaa", "bbb"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let got: Vec<&str> = ordered_keys(arrival.iter())
            .into_iter()
            .map(|s| s.as_str())
            .collect();
        // 期望值是**手写字面量**，不是拿被测函数算出来的（否则本断言自证、恒绿）。
        assert_eq!(got, vec!["aaa", "bbb", "mmm", "zzz"]);
        // 非空对照：到达顺序**确实**不是这个顺序（夹具真的是乱的）。
        let as_arrived: Vec<&str> = arrival.iter().map(|s| s.as_str()).collect();
        assert_ne!(
            as_arrived, got,
            "夹具的到达顺序本来就等于排序结果 —— 这条判据在空转"
        );
    }

    /// `KS10②` 的**端到端那一半**（整份进、整份出）。
    ///
    /// ⚠ **订正〔`K-H2` `D1` 阻-3 回修，08-28〕：本段那句「它单独存在时是安慰剂」今天不成立了。**
    /// 先前逐字写的是：「`serde_json::Map` 在今天这份构建里是 `BTreeMap`（插进去就有序）
    /// ⇒『两份插入顺序相反的文档』这个夹具**造不出来**，把排序整条删掉它照样绿（`MU9` 实测 19/19）」。
    /// `K-H2` 给本 crate 的 `[dev-dependencies]` 加了 `serde_json` 的 `preserve_order`
    /// ⇒ **判据构建里 `Map` 就是 `IndexMap`**，那个夹具造得出来了。
    /// 实测：删掉 [`ordered_value`] 的递归 ⇒ **本条与 `nested_objects_…` 两条一起红**（30 passed / 2 failed）。
    /// ⇒ **`MU9` 当年那条「造不出来」是对当时那份构建说的，不是一条永久事实。**
    /// ⚠ 但**牙的来源变了要说清**：本条今天有牙靠的是那一行 dev-dependency；删了它就退回安慰剂。
    /// `ordering_is_by_name_not_by_arrival` 仍然是**不依赖任何 feature** 的那一格，两条配着用。
    ///
    /// ⚠⚠ **而「那一行被删掉」这件事，先前没有任何东西会说**〔`D2` `§五-3`，`C-补` 08-28 补上〕。
    /// `D2` `P6` 实测：删掉那行 dev-dependency、**生产码一个字不动** ⇒ 全量门禁 `GATE: OK`、
    /// 三个数逐个不变 —— **本条与 `nested_objects_…` 一起悄悄退回安慰剂，而没人会知道。**
    /// ⇒ 判据体第一段加了**反空真自检**（直接量「`Map` 是不是插入序」）：
    /// 那一行没了 ⇒ **本条当场红**，报文逐字点出是哪一行依赖没了，不再是「悄悄退回」。
    #[test]
    fn the_field_order_does_not_depend_on_the_map_implementation() {
        // ★★ **反空真自检排最前**〔`C-补` 08-28〕：本条的牙**整个**架在
        //    `[dev-dependencies]` 里 `serde_json` 那行 `preserve_order` 上 —— 没有它
        //    `Map` 是 `BTreeMap`，下面那两份「插入顺序相反」的夹具**根本造不出来**
        //    （两边喂进去的本来就是同一个有序结构，删掉排序也照绿）。
        //    这两行就是「那一行没了会说话的东西」。
        let mut probe = Map::new();
        probe.insert("z".into(), Value::from(1));
        probe.insert("a".into(), Value::from(2));
        assert_eq!(
            probe.keys().next().map(String::as_str),
            Some("z"),
            "判据构建里 Map 不是插入序 —— `preserve_order` 那行 dev-dependency 没了，本条已退回安慰剂"
        );

        // 两份**内容相同、插入顺序相反**的文档。
        let a = parse(r#"{"aaa":1,"mmm":2,"zzz":3}"#).expect("a");
        let b = parse(r#"{"zzz":3,"mmm":2,"aaa":1}"#).expect("b");
        assert_eq!(
            to_pretty_json(&a),
            to_pretty_json(&b),
            "同样的内容按不同顺序插入，落盘文本就不一样 —— \
             那样每次界面一动整份文件的 diff 全变，人就不敢手编了"
        );
        // 非空对照：输出确实有多行、确实含那三个键（不是空串恒等）。
        let t = to_pretty_json(&a);
        assert!(t.lines().count() >= 5, "输出只有 {} 行", t.lines().count());
        for k in ["aaa", "mmm", "zzz"] {
            assert!(t.contains(k), "输出里没有 {k}");
        }
        // 顺序就是排好的那一个（不是「碰巧一样」）。
        let ia = guard_core::find_pinned(&t, "aaa").expect("aaa 应当恰好出现一处");
        let im = guard_core::find_pinned(&t, "mmm").expect("mmm 应当恰好出现一处");
        let iz = guard_core::find_pinned(&t, "zzz").expect("zzz 应当恰好出现一处");
        assert!(ia < im && im < iz, "输出不是按键名排序的：{t}");
    }

    /// ★★ **`KS10` 的交错那一格**（PM 08-27 点名：别测成「写完能读回来」，那测不到覆盖）。
    ///
    /// 场景逐字：**人改了 A，程序写 B，A 还在不在。**
    /// 这里模拟的是那条真实的时间线 ——
    /// ① 界面打开时程序读了一份（`stale`）；② 人在编辑器里改了另一个键；
    /// ③ 程序这时候才去写 key。
    /// **`merge_key` 收的是「写的那一刻盘上的内容」**，所以第三步喂的必须是 `fresh` 而不是 `stale`。
    #[test]
    fn a_human_edit_between_read_and_write_survives_the_program_write() {
        // ① 界面打开时的样子
        let stale = parse(r#"{"_note":"旧的","api_key":"OLD"}"#).expect("stale");
        // ② 人在编辑器里把 `_note` 改了，并且自己加了一个新键
        let fresh =
            parse(r#"{"_note":"人刚改成这样","my_own":"别动我","api_key":"OLD"}"#).expect("fresh");
        // ③ 程序写 key —— 喂的是**写的那一刻**的内容
        let written = merge_key(&fresh, &SecretKey::new("NEW"));

        assert_eq!(written["_note"], "人刚改成这样", "人的编辑被陈旧副本盖掉了");
        assert_eq!(written["my_own"], "别动我", "人新加的键被吃掉了");
        assert_eq!(written[KEY_FIELD], "NEW");

        // ★ **非空对照：喂陈旧副本就是会盖掉** —— 没有这一格，上面三条可能是恒真的。
        let wrong = merge_key(&stale, &SecretKey::new("NEW"));
        assert_eq!(
            wrong["_note"], "旧的",
            "拿陈旧副本合并**应当**丢掉人的编辑；它没丢 ⇒ 上面那三条断言证不了什么"
        );
        assert!(
            wrong.get("my_own").is_none(),
            "拿陈旧副本合并**应当**丢掉人新加的键；没丢 ⇒ 本条在空转"
        );
    }

    /// 三态：不存在（空）· 读坏了 · 顶层不是对象。**「读坏了」不许被当成「没配」。**
    #[test]
    fn a_broken_file_is_reported_instead_of_being_read_as_not_configured() {
        assert_eq!(parse(""), Ok(Map::new()));
        assert_eq!(parse("   \n "), Ok(Map::new()));
        assert!(matches!(
            parse(r#"{"api_key": }"#),
            Err(StoreError::NotJson(_))
        ));
        assert_eq!(parse("[1,2,3]"), Err(StoreError::NotAnObject));
        // 说法里要带得走「怎么修」——人手编打错时看得懂。
        let msg = parse(r#"{"a":}"#).unwrap_err().to_string();
        assert!(
            msg.contains("手编"),
            "错误说法没告诉人这文件是手编的：{msg}"
        );
        assert!(
            msg.contains("没有动它"),
            "错误说法没说清程序没破坏文件：{msg}"
        );
    }

    /// key 字段的几种「等于没配」的写法。
    #[test]
    fn several_shapes_all_mean_not_configured() {
        // 分母 = 我列出的这 4 形，**不是**「所有写法」。
        for raw in [
            r#"{}"#,
            r#"{"api_key":""}"#,
            r#"{"api_key":"   "}"#,
            r#"{"api_key":null}"#,
        ] {
            let doc = parse(raw).expect(raw);
            assert!(read_key(&doc).is_none(), "这一形该被当成没配：{raw}");
        }
        // 非空对照：真配了的读得出来。
        let doc = parse(r#"{"api_key":" sk-x "}"#).expect("ok");
        assert_eq!(
            read_key(&doc).expect("应当读得到").expose_for_auth_header(),
            "sk-x",
            "首尾空白该被 trim（那是编辑器留下的），中间的内容一个字节都不许动"
        );
    }

    // ================================================================ `K-H2` `KH5`：多条

    /// ★★ **`KH5a`**：`KS9③`（导入）在**多条**形状下重验 —— 一份人手写的多账号 JSON
    /// 放进去就算配好，**两层的未知键都还在**，不需要任何迁移步骤。
    ///
    /// ⚠ `KS9` 那几条原来只在**一条**的形状上验过（件计划 `§0c` 第 3 问逐字）。
    #[test]
    fn a_hand_authored_multi_account_file_is_read_as_a_table_with_both_layers_intact() {
        let hand = r#"{
            "_note": "顶层未知键：别删我",
            "accounts": {
                "zzz-last": { "api_key": "KEY-Z", "why": "我自己加的注释" },
                "aaa-first": { "api_key": "KEY-A", "base_url": "https://api.example.invalid" }
            }
        }"#;
        let doc = parse(hand).expect("人手写的多账号 JSON 应当直接被接受");
        let table = read_accounts(&doc);

        // 分母 = 夹具里这 2 条，逐条查。
        assert_eq!(table.len(), 2, "读出来的条数不对");
        // ★ 顺序按**键名**，不按人写的先后（`aaa-first` 写在后面）。
        assert_eq!(table[0].id, "aaa-first");
        assert_eq!(table[1].id, "zzz-last");
        assert_eq!(
            table[0].key.as_ref().expect("A 应当有 key").expose_for_auth_header(),
            "KEY-A"
        );
        assert_eq!(table[0].base_url.as_deref(), Some("https://api.example.invalid"));
        assert_eq!(
            table[1].key.as_ref().expect("Z 应当有 key").expose_for_auth_header(),
            "KEY-Z"
        );
        // 没写 `base_url` 的那条是 `None`（= 用默认上游），**不是空串**。
        assert_eq!(table[1].base_url, None);

        // 两层的未知键都还在（这是「导入不吃东西」的另一半）。
        assert_eq!(doc["_note"], "顶层未知键：别删我");
        assert_eq!(doc["accounts"]["zzz-last"]["why"], "我自己加的注释");
        // 没有任何「版本号 / schema 迁移」这一步。
        assert!(doc.get("_schema_version").is_none());
    }

    /// **`KH5a` 的另一半**：`K-H2a` 交付的那份**旧文件**（顶层一把 key）升级之后照常能用，
    /// 它变成一条 id 逐字是 [`LEGACY_ACCOUNT_ID`] 的**有名字的行**。
    ///
    /// ⚠⚠ 「有名字的行」与「默认行」的分界就在这一条判据上：
    /// 本条只证「`default` 查得到」，**不证也不许证**「别的 id 也能拿到它」——
    /// 那一半由 `K-H2` `KH2` 的 404 判据反向钉住。
    #[test]
    fn the_legacy_top_level_key_becomes_one_named_row_not_a_default_row() {
        let doc = parse(r#"{"_note":"旧文件","api_key":"LEGACY-KEY"}"#).expect("旧形状");
        let table = read_accounts(&doc);
        assert_eq!(table.len(), 1, "旧文件应当读成**恰好一条**");
        assert_eq!(table[0].id, LEGACY_ACCOUNT_ID);
        assert_eq!(
            table[0].key.as_ref().expect("应当有 key").expose_for_auth_header(),
            "LEGACY-KEY"
        );

        // ★ `accounts` 里已经有同名的那一条 ⇒ **人写的那条赢**，顶层那半不再加。
        let both = parse(
            r#"{"api_key":"LEGACY-KEY","accounts":{"default":{"api_key":"HAND-WRITTEN"}}}"#,
        )
        .expect("两处都有");
        let t2 = read_accounts(&both);
        assert_eq!(t2.len(), 1, "同名的两处应当合成一条，不是两条");
        assert_eq!(
            t2[0].key.as_ref().expect("key").expose_for_auth_header(),
            "HAND-WRITTEN",
            "程序拿历史形状盖掉了人明确写下的那条"
        );

        // 非空对照：顶层**没有**这两个字段时，一条都不加（不是加一条空的）。
        let empty = parse(r#"{"_note":"什么都没配"}"#).expect("空");
        assert!(read_accounts(&empty).is_empty());
    }

    /// ★★★ **「没配」就是零条 —— 一条「默认行」都不许有。**
    ///
    /// 这是 `K-H2` `KH2` 在存储那一层的那一半：**零条 ⇒ 中转全部 404**。
    /// 只要这里凭空多出一条什么都没配的行，「查不到就用它」马上就写得出来了，
    /// 而那正是件计划逐字禁的**回落**。
    ///
    /// ⚠ 模板里 `accounts` 是**空对象**、`api_key` 是**空串** ——
    /// 例子写在 `_note_accounts` 那句**散文**里，不是一条活的行。
    /// 这是刻意的：模板里放一条活的示例行，等于每个刚装好的人都白得一条路。
    #[test]
    fn an_unconfigured_file_yields_no_rows_at_all() {
        // ★ **非空对照排最前**〔`D1` 一并修，08-28〕：先证明这把尺子读得出行，
        //   否则下面整个循环可能只是因为 `read_accounts` 恒返回空而全绿。
        let filled = parse(r#"{"accounts":{"my-account":{"api_key":"K"}}}"#).expect("填上");
        assert_eq!(read_accounts(&filled).len(), 1, "这把尺子是瞎的");

        // 分母 = 我列出的这 4 种「没配」的写法。
        for raw in [
            TEMPLATE,
            r#"{}"#,
            r#"{"_note":"什么都没写"}"#,
            r#"{"accounts":{},"api_key":""}"#,
        ] {
            let doc = parse(raw).expect("夹具应当可解析");
            assert!(
                read_accounts(&doc).is_empty(),
                "这一形凭空多出了行 —— 那就是一条默认行：{raw}"
            );
        }
    }

    /// ★★ **一条什么都没填的账号是合法的一条**：`base_url` 空 = 用默认上游，
    /// `api_key` 空 = 原样转发客户端自己那份鉴权头。
    ///
    /// # 它买回来的是 `K-H1` 甲半那条被多账号「顺手弄没了」的性质
    ///
    /// 先前「不配凭据 ⇒ 中转仍是一条能用的透传路」是一条**隐式的全局行为**
    /// （`render_upstream_request` 的头注逐字写着它）。改成按表路由之后，
    /// 那条隐式行为**必然消失** —— 因为不再有「全局」这个东西。
    /// ⇒ 它没有被删掉，是被**改成显式的一条路**：写一条空账号就有了。
    /// **这两句话差得很远，所以要有一条判据钉着后一句。**
    #[test]
    fn an_account_with_nothing_filled_in_is_still_a_row() {
        let doc = parse(r#"{"accounts":{"passthrough":{}}}"#).expect("夹具");
        let rows = read_accounts(&doc);
        assert_eq!(rows.len(), 1, "空账号应当算一条");
        assert_eq!(rows[0].id, "passthrough");
        assert!(rows[0].key.is_none(), "它不该有 key");
        assert!(rows[0].base_url.is_none(), "它不该有自己的上游");

        // ⚠ 但值**不是对象**的那一条要跳过（有人写成 `"acct": "sk-..."`）。
        let wrong = parse(r#"{"accounts":{"acct":"sk-not-an-object"}}"#).expect("夹具");
        assert!(read_accounts(&wrong).is_empty(), "非对象的那一条不该进表");
        // ★ `K-R1`：没写 `auth_style` 的那一条读回来是 `Absent`，**不是**默认值 ——
        //   把它折成默认值是调用方的事，本模块只报「文件里写了什么」。
        assert_eq!(rows[0].auth_style, AuthStyleSetting::Absent);
    }

    /// ★★ `K-R1`：那个闭集**只有一个住址**，而且两头对得上。
    ///
    /// # 它买的是什么
    ///
    /// [`AuthStyle::from_field_value`] 刻意从 [`AuthStyle::ALL`] 派生，不是第二个 `match`。
    /// 本条把这句话变成读数：`ALL` 里每一个成员，`field_value` 出去再进得回来，
    /// **而且回到的是同一个成员**。写成第二个 `match` 而漏一支的话，那一支在这里当场红。
    ///
    /// ⚠ 它**不**证明「这三个词就是现实里所有的鉴权头形状」—— 那是一个没人给得出分母的
    /// 全称句。它证的是「我登记的这几个，代码两头一致」。
    #[test]
    fn every_registered_auth_style_survives_the_round_trip_through_its_field_value() {
        // 反空真：`ALL` 不是空的（空的话下面整个循环恒真）。
        assert!(
            AuthStyle::ALL.len() >= 2,
            "闭集只有 {} 个成员 —— 少于两个的话「按形状切」这句话没有内容",
            AuthStyle::ALL.len()
        );
        // 每个成员的词**互不相同**（重了的话 `from_field_value` 会把两个折成一个）。
        let mut words: Vec<&str> = AuthStyle::ALL.iter().map(|s| s.field_value()).collect();
        let n = words.len();
        words.sort();
        words.dedup();
        assert_eq!(words.len(), n, "有两个成员的 field_value 撞了：{words:?}");

        for want in AuthStyle::ALL {
            let w = want.field_value();
            assert_eq!(
                AuthStyle::from_field_value(w),
                Some(*want),
                "`{w}` 进不回它自己 —— 两头漂了"
            );
            // 大小写与首尾空白都要认（人手编时会带）。
            assert_eq!(
                AuthStyle::from_field_value(&format!("  {}  ", w.to_ascii_uppercase())),
                Some(*want)
            );
        }
        // ★ 非空对照：认不出的词就是认不出，**不许**回落到默认值。
        //   分母 = 我列出的这 3 形（不是「所有不合法输入」）。
        for bad in ["Bearer token", "x_api_key", "openai"] {
            assert_eq!(
                AuthStyle::from_field_value(bad),
                None,
                "`{bad}` 被认成了某个合法值 —— 静默回落正是本件在治的那一形"
            );
        }
    }

    /// ★★★ `K-R1`：`auth_style` 那一格读出来是**三态**，
    /// 「缺席」与「写了个认不出的词」**不许挤进同一态**。
    ///
    /// 合成一态的唯一去处是回落成默认 ⇒ 一个打错字母的 `auth_style` 静默走默认头，
    /// 症状是**一条查不出来的 401**，与「key 打错了」同形。〔`K-R21` 那一族〕
    #[test]
    fn a_typo_in_auth_style_is_not_the_same_state_as_leaving_it_out() {
        // 分母 = 下面这 6 形，逐形手写期望值。
        let cases: &[(&str, AuthStyleSetting)] = &[
            (r#"{"accounts":{"a":{}}}"#, AuthStyleSetting::Absent),
            (
                r#"{"accounts":{"a":{"auth_style":""}}}"#,
                AuthStyleSetting::Absent,
            ),
            (
                r#"{"accounts":{"a":{"auth_style":"   "}}}"#,
                AuthStyleSetting::Absent,
            ),
            (
                r#"{"accounts":{"a":{"auth_style":"bearerr"}}}"#,
                AuthStyleSetting::Unknown,
            ),
            // 值不是字符串（有人写成数字）也归 `Unknown` —— 同样是「写了但读不懂」。
            (
                r#"{"accounts":{"a":{"auth_style":7}}}"#,
                AuthStyleSetting::Unknown,
            ),
            (
                r#"{"accounts":{"a":{"auth_style":"none"}}}"#,
                AuthStyleSetting::Known(AuthStyle::NoAuth),
            ),
        ];
        for (raw, want) in cases {
            let doc = parse(raw).expect("夹具应当可解析");
            let rows = read_accounts(&doc);
            assert_eq!(rows.len(), 1, "夹具该读出恰好一条：{raw}");
            assert_eq!(&rows[0].auth_style, want, "这一形读错了：{raw}");
        }
        // ★ 非空对照：这把尺子分得出三态里的**两两不同**（不是恒答一张脸）。
        assert_ne!(AuthStyleSetting::Absent, AuthStyleSetting::Unknown);
        assert_ne!(
            AuthStyleSetting::Known(AuthStyle::Bearer),
            AuthStyleSetting::Known(AuthStyle::NoAuth)
        );
    }

    /// ★★ `K-R1`：顶层那一格 `auth_style` **单独在，造不出一条行**。
    ///
    /// 加进「要不要造这一行」的条件里就等于「只写了 `auth_style` 的文件也有一条
    /// 什么都没配的 `default` 行」，而那正是 [`read_accounts`] 头注逐字禁的**回落**。
    #[test]
    fn a_top_level_auth_style_on_its_own_does_not_conjure_a_row() {
        let alone = parse(r#"{"auth_style":"none"}"#).expect("夹具");
        assert!(
            read_accounts(&alone).is_empty(),
            "只写了 auth_style 就凭空多出一条行 —— 那是一条默认行"
        );
        // ★ 非空对照：同一格与 `api_key` 一起写时，它**被读进那一行**（不是被忽略）。
        let with_key = parse(r#"{"api_key":"K","auth_style":"none"}"#).expect("夹具");
        let rows = read_accounts(&with_key);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, LEGACY_ACCOUNT_ID);
        assert_eq!(
            rows[0].auth_style,
            AuthStyleSetting::Known(AuthStyle::NoAuth),
            "顶层那一格没被读进这一行"
        );
    }

    /// ★★★ **`KH5b` 的正主**〔件计划 `§0c` 第 3 问逐字点名的那个**新**形状〕：
    /// **程序只改其中一条，别的条不许被动。**`K-H2a` 没验过这一格。
    ///
    /// # 量法：**B 那一条序列化出来的文本逐字节相等**
    ///
    /// ⚠ 刻意**不**逐字段查 —— 逐字段查的分母是「我列出的这几个字段」，
    /// 而这里要的是「**整条**没被动」，那个全称句只有逐字节比得起。
    #[test]
    fn writing_one_account_leaves_every_other_account_byte_for_byte_untouched() {
        let hand = parse(
            r#"{
                "_note": "顶层未知键",
                "accounts": {
                    "A": { "api_key": "A-OLD", "note_a": "A 自己的注释", "base_url": "https://a.invalid" },
                    "B": { "api_key": "B-KEY", "note_b": "别动我", "nested": { "z": 1, "a": 2 } }
                }
            }"#,
        )
        .expect("夹具应当可解析");
        let before_b = serde_json::to_string(&hand["accounts"]["B"]).expect("序列化 B");

        let merged = merge_account_key(&hand, "A", &SecretKey::new("A-NEW"));
        let merged_b = merge_account_key(&hand, "B", &SecretKey::new("B-NEW"));

        // ★★ **非空对照排最前**〔`D1` 一并修，08-28〕：同一把尺子，改 **B** 的时候 B **应当**变。
        //    没有这一格，下面 ㈡ 那条可能只是因为这把尺子看不见任何变化。
        //    ⚠ 先前它排在整条判据的**最后** —— 与 `D1-M6` 逮到的那一形同族：
        //    前面任何一条先炸，它就一次都没被求值。
        let b_after_writing_b =
            serde_json::to_string(&merged_b["accounts"]["B"]).expect("序列化 B");
        assert_ne!(
            b_after_writing_b, before_b,
            "改 B 的时候 B 没变 —— 这把尺子是瞎的，下面那条「没被动」证不了什么"
        );

        // ㈡ **别的条逐字节没动**（本条判据的正题，排在对照之后、别的之前）。
        let after_b = serde_json::to_string(&merged["accounts"]["B"]).expect("序列化 B");
        assert_eq!(after_b, before_b, "改 A 的时候 B 那一条被动了");

        // ㈠ 被改的那一条：key 换了，**它自己的未知键**还在。
        assert_eq!(merged["accounts"]["A"][KEY_FIELD], "A-NEW");
        assert_eq!(merged["accounts"]["A"]["note_a"], "A 自己的注释");
        assert_eq!(merged["accounts"]["A"][BASE_URL_FIELD], "https://a.invalid");
        // ㈢ 顶层的未知键也还在。
        assert_eq!(merged["_note"], "顶层未知键");
        // 改 B 的时候，B 自己的未知键与嵌套结构也都留着。
        assert_eq!(merged_b["accounts"]["B"]["note_b"], "别动我");
        assert_eq!(merged_b["accounts"]["B"]["nested"]["z"], 1);
    }

    /// **`KH5b` 的第二半**：往一份**还没有 `accounts` 段**的文件里写一条账号，
    /// 顶层原有的东西（含 `K-H2a` 那把顶层 key）一个都不许被吃掉。
    #[test]
    fn adding_the_first_account_to_a_legacy_file_keeps_the_legacy_half() {
        let legacy = parse(r#"{"_note":"旧的","api_key":"LEGACY"}"#).expect("旧文件");
        let merged = merge_account_key(&legacy, "newone", &SecretKey::new("NEW"));
        assert_eq!(merged["accounts"]["newone"][KEY_FIELD], "NEW");
        assert_eq!(merged[KEY_FIELD], "LEGACY", "顶层那把 key 被吃掉了");
        assert_eq!(merged["_note"], "旧的");
        // 读回来是**两条**（`default` + `newone`）。
        let rows = read_accounts(&merged);
        let ids: Vec<&str> = rows.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec![LEGACY_ACCOUNT_ID, "newone"]);
    }

    /// **`KH5c`**：落盘文本在**深度 ≥2** 上也按键名排。
    ///
    /// # ★★ 它**有牙**（`D1` 阻-3 回修之后）
    ///
    /// ⚠⚠ **本段先前写的是「这一条今天没有牙，它是一条给明天用的绊线」，那句话已被实测证伪。**
    /// 病灶是我把一个**关于分母的全称句**（「`BTreeMap` ⇒ 嵌套层乱序的夹具造不出来」）
    /// 当成前提直接写进了头注，而**没有去打它**。
    /// `D1` 只加了一行 dev-dependency（`serde_json` 开 `preserve_order`）就把夹具造了出来。
    ///
    /// 今天的读数（我自己复打）：原码 **32 passed / 0 failed**；
    /// 把 [`ordered_value`] 的 `Value::Object` 那一支删成 `other.clone()`
    /// ⇒ **30 passed / 2 failed**，红的是本条 + 隔壁 `the_field_order_does_not_depend_on_the_map_implementation`。
    /// ⇒ **深度 ≥2 今天真有人守着。**
    ///
    /// ⚠ 它仍然要配着 `ordering_is_by_name_not_by_arrival` 读：那一条收迭代器，
    /// 与 `Map` 是哪种实现**无关**；本条则是**靠 `preserve_order` 把 `Map` 换成 `IndexMap`** 才有牙的
    /// —— 哪天那一行 dev-dependency 被删掉，本条**先前会悄悄退回**成安慰剂。
    /// 那一行的理由与读数写在 `crates/creds-core/Cargo.toml` 里，**别当成可有可无的依赖**。
    ///
    /// ⚠⚠ **「悄悄」那一格已经补上了**〔`D2` `§五-3`，`C-补` 08-28〕：判据体第一段是
    /// **反空真自检**（直接量「`Map` 是不是插入序」）⇒ 那一行没了本条**当场红**。
    /// 立项读数：`D2` `P6` 删掉那行、生产码不动 ⇒ 全量门禁 `GATE: OK`、三个数逐个不变，
    /// **没有任何东西说话**；`P6b`（再删递归）⇒ 32 passed / 0 failed ⇒ 确实退回了安慰剂。
    #[test]
    fn nested_objects_are_also_ordered_by_name_in_what_lands_on_disk() {
        // ★★ **反空真自检排最前**〔`C-补` 08-28〕：见本条头注最后一段。
        //    没有 `preserve_order`，下面这个「嵌套层乱序」的夹具造不出来（`parse` 出来就已经排好），
        //    整条判据会变成「排过的东西还是排过的」——恒真。
        let mut probe = Map::new();
        probe.insert("z".into(), Value::from(1));
        probe.insert("a".into(), Value::from(2));
        assert_eq!(
            probe.keys().next().map(String::as_str),
            Some("z"),
            "判据构建里 Map 不是插入序 —— `preserve_order` 那行 dev-dependency 没了，本条已退回安慰剂"
        );

        let doc = parse(r#"{"accounts":{"b":{"zzz":1,"aaa":2},"a":{"mmm":3}}}"#).expect("夹具");
        let text = to_pretty_json(&doc);

        // ★ **反空真排最前**〔`D1` 一并修〕：真的落到了嵌套那一层（不是整份被压成一行），
        //   且落盘的仍然是合法 JSON。先前这两条排在最后。
        assert!(text.lines().count() >= 8, "输出只有 {} 行", text.lines().count());
        let back: Value = serde_json::from_str(&text).expect("落盘的东西必须是合法 JSON");
        assert_eq!(back["accounts"]["b"]["aaa"], 2);

        // 深度 1：`a` 排在 `b` 前面。
        let ia = guard_core::find_pinned(&text, "\"a\": {").expect("`a` 应当恰好出现一处");
        let ib = guard_core::find_pinned(&text, "\"b\": {").expect("`b` 应当恰好出现一处");
        assert!(ia < ib, "深度 1 没按键名排：{text}");
        // 深度 2：`b` 那条里 `aaa` 排在 `zzz` 前面。
        let iaaa = guard_core::find_pinned(&text, "\"aaa\"").expect("`aaa` 应当恰好出现一处");
        let izzz = guard_core::find_pinned(&text, "\"zzz\"").expect("`zzz` 应当恰好出现一处");
        assert!(iaaa < izzz, "深度 2 没按键名排：{text}");
    }

    /// **数组的顺序是数据，不许排；但数组里的对象要递归进去。**
    ///
    /// 这一条守的是**方向**：把 [`ordered_value`] 的 `Value::Array` 那一支改成「排数组」会当场红。
    ///
    /// # ⚠⚠ 「递归进去」那一半先前**没有牙**，本轮取到了〔`D2` `§五-4`，`C-补` 08-28〕
    ///
    /// 本条先前逐字承认：「把那一支改成 `other.clone()`（不递归进数组里的对象）本条**看不出来**
    /// —— 夹具是一个**标量数组**，人群里没有『数组里套对象』那一形」。**那句自陈属实**
    /// （`D2` `P7`：不递归 + 旧夹具 ⇒ **32 passed / 0 failed**，一声不吭）。
    /// ⇒ 本轮把缺的那一形**加进夹具**：`{"list":[{"zzz":1,"aaa":2}]}`，断 `aaa` 排在 `zzz` 前。
    /// 今天这一支**真有人守**：不递归 ⇒ 本条红，报文逐字说「数组里那个对象没被递归排序」。
    ///
    /// ⚠ **牙的来源要说清**：新那一段与 `nested_objects_…` 同源 —— 它靠
    /// `[dev-dependencies]` 里 `serde_json` 那行 `preserve_order` 把 `Map` 换成 `IndexMap`，
    /// 否则 `parse` 出来的内层对象**本来就已经排好**，「有没有递归进去」看不出来。
    /// ⇒ 那一段前面同样立着**反空真自检**。**数组本身那一半（顺序是数据、不许排）不依赖它。**
    #[test]
    fn arrays_keep_their_order_because_that_order_is_data() {
        let doc = parse(r#"{"list":["zzz","aaa","mmm"]}"#).expect("夹具");
        let text = to_pretty_json(&doc);
        let back: Value = serde_json::from_str(&text).expect("合法 JSON");
        // 期望值是**手写字面量**，不是拿被测函数算出来的。
        assert_eq!(back["list"], serde_json::json!(["zzz", "aaa", "mmm"]));
        // 非空对照：同一把尺子看得见「排过」的样子长什么样（证明它不是恒等）。
        assert_ne!(back["list"], serde_json::json!(["aaa", "mmm", "zzz"]));

        // ── 另一半：**数组里套的对象要递归进去** ──────────────────
        // ★★ 反空真自检排在这一段最前（同 `nested_objects_…`）：没有 `preserve_order`
        //    下面那个内层对象 `parse` 出来就已经排好，这一段会恒真。
        let mut probe = Map::new();
        probe.insert("z".into(), Value::from(1));
        probe.insert("a".into(), Value::from(2));
        assert_eq!(
            probe.keys().next().map(String::as_str),
            Some("z"),
            "判据构建里 Map 不是插入序 —— `preserve_order` 那行 dev-dependency 没了，下面这一段已退回安慰剂"
        );

        let nested = parse(r#"{"list":[{"zzz":1,"aaa":2}]}"#).expect("嵌套夹具");
        let nested_text = to_pretty_json(&nested);
        // 非空对照：落盘的仍是合法 JSON，而且数组那一层**还在**（不是被压没了）。
        let nested_back: Value = serde_json::from_str(&nested_text).expect("合法 JSON");
        assert_eq!(nested_back["list"][0]["aaa"], 2);
        let iaaa = guard_core::find_pinned(&nested_text, "\"aaa\"").expect("`aaa` 应当恰好出现一处");
        let izzz = guard_core::find_pinned(&nested_text, "\"zzz\"").expect("`zzz` 应当恰好出现一处");
        assert!(
            iaaa < izzz,
            "数组里那个对象没被递归排序 —— `ordered_value` 的 `Value::Array` 那一支没有往下走：{nested_text}"
        );
    }
}
