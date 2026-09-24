//! 路由表：路由键里 **agent ＋ 账号** 那两段 → **上游与 key 焊在一起的一个值**〔`K-H2` `KH1`/`KH2`；键的第二维是条 49〕。
//!
//! # 🔴 表的键是 `(agent, 账号)`，不是账号〔条 49 · `设计/20 §3.1` 拍板 (b) 甲〕
//!
//! 先前 `lookup` 只收一个账号 ⇒ **claude 的 3 号账号与 codex 的 3 号账号是同一行**。
//! 那一形今天不疼只因为 codex 那边没有注入；「让所有会话都过中转」那一刀一落地，
//! codex 会话一发 `/s/codex/3/…` 就会拿到 claude 那一行的上游与 key ⇒ **每一发都错发到 Anthropic**。
//! ⇒ 今天键是两段，一段都不许省：查 `(codex, 3)` 查不到 ⇒ `/s/` 回 404，一个字节不发上游。
//!
//! ⚠ **凭据文件本身今天没有 agent 这一维**（`creds-core` 那份格式只有账号 id）。
//! 装表时每一行挂在谁名下由调用方显式交进来（[`build`] 的 `agent` 入参），
//! 那个值今天恒是 `super::CREDENTIALS_FILE_AGENT` —— 住址与理由见它的头注。
//! **本模块自己不认识任何一个 agent 的名字**。
//!
//! # 它治的是一个**今天就已经存在**的形状，不是一个将来的风险
//!
//! `K-H2` `Bx` 现打（`7116e59`）：`Relay` 有两个**各自独立**的进程级字段
//! `base: Base` 与 `key: Option<SecretKey>`，`handle` 里
//! `upstream::connect(&relay.base)`（`:363`）与 `relay.key.as_ref()`（`:375`）**各取各的**，
//! 而路由键**一格都不参与**这个决定（它只喂 tee）。
//! ⇒ 那等于「**回落到默认上游 + 默认 key**」，而且是它当时**唯一**的行为。
//! 一把 key 时无害；多账号之后，同一条代码路径就是 `KH2` 逐字点名的**最坏失效形态**：
//! **拿 A 账号的 key 去发 B 账号的请求，而两边看起来都成功了。**
//!
//! ⇒ 所以本模块的形状不是「加一张表」，是**把那两个字段删掉**。
//!
//! # ★★ 买法：往「写不出来」那边挪一格 —— ⚠ **但没有买断，这一段被实测改写过**
//!
//! ⚠⚠ **订正〔`D1` 阻-1 回修，08-28〕**：本节先前那张表把三条性质里的**两条**记成
//! 「**编译器**买的」。`D1` 逐条编出反例，**本模块那三条腿 + 那条棘轮一条都没红**。
//! 下面是**重打之后**的表 —— 每一格要么给读数，要么写「判不了」。
//!
//! ⚠⚠ 🔴 **〔`设计/20 §7` 步 1–2〕下面这张表是 `D1` 那一拍的读数，三行今天已经过期了，
//! 逐行标着「今天」。旧读数**刻意不删** —— 它记着「读着像买断 ≠ 买断了」那一课是怎么打出来的。**
//!
//! | 性质 | 今天真正由谁守 | 实测出来的洞（**分母 = `D1` 编出来的这几形**） |
//! |---|---|---|
//! | **从一个 `Row` 里拿不到 `&Base`** | ⚠ **这条今天不成立了**：`Row::base()` 存在（层间契约要它），但**收成了 `pub(super)`** ⇒ 层 1 够不到整个 `Row`。旧话：⭐ 编译器（字段私有 · 住 `mod sealed` · 无 `base()` 访问器） | 这是编译器**真正**买到的**唯一**一条 |
//! | ⚠ **另一半（`&SecretKey`）根本不在这张表的保护面里** | **没有人** —— [`Row::key`] 就是一个 `pub(crate)` 访问器 | 〔`D2` `§五-2`，`C-补` 08-28 补的话，**不是新缺陷**〕`Row` 两个半边**不对称**：`base` 那半没访问器（要「有意重建」，见 `D1-M1`），`key` 那半**一个方法调用**就够 ⇒ 「拿 B 的 key」不需要重建任何东西。⚠ 它下游那一跳另有人守：把 `&SecretKey` 变成明文的地方由 `creds_guard` ㈢（`expose_for_auth_header(` 恰好 1 处）钉着 —— 但那守的是**明文出口**，**不是**「谁拿得到这个值」 |
//! | 上游与 key「只能同源」 | **只有行为判据** | `D1-M2`：两行同时在作用域、A 连 B 渲染 ⇒ **编译通过**（我复打过）。`D1-M1`：换签名收 `host: &str` + 用 `row.host_header()` **重建 `Base`** 去连 ⇒ **488 passed / 0 failed** |
//! | 进程里「没有默认上游可回落」 | ⚠ **今天换人守了**：那个进程级常量先搬进层 2、条 59 又把它**整删**成每 agent 一行的表（`super::AGENT_UPSTREAMS`），`table_guard::layer_one_has_no_default_upstream_to_fall_back_to` 两向相等断言钉着「层 1 零处 · 层 2 恰好登记那几处」。旧话：只有一条文本棘轮（禁 `Relay` 里出现 `base:` / `key:` 字面） | **假**（那一拍）：那个默认上游常量当时是**层 1 的** crate 常量（当时的住址见本表下面那一段），`Base` 三个字段全 `pub(crate)`、`Base::parse` 也是 ⇒ 一行就能造一个。⚠ **后半句今天仍成立** —— 层 1 有意去 `Base::parse` 现造一个，没人拦得住 |
//! | 装表**只有一处**做 | **文本判据**（`table_guard.rs` 那几条相等断言；⚠ 「`sealed` 里面」那一格今天改成「`accounts/` 里面」） | `D1-M4`：㈠ 那根针在它自称「真正的人群」（`sealed` 里面）**恰恰最弱** —— **一个字面量能焊出任意多行**，数字面量数不出「焊了几行、每行装了什么」。实测多行焊接 + 把回落整个加回来 ⇒ `table_guard` 单跑 **10 passed / 0 failed** |
//!
//! ⚠⚠ **上表第 4 行那个「当时的住址」是 `server.rs:24`** —— 逐条说清它为什么还写着行号：
//! ① 它是 `D1` 那一拍的**引文**，不是指路（今天那个常量住层 2）；
//! ② 它**没被改成符号地址**，是因为 `tests/bridge/structural_scan_tests.rs::INVENTORY`
//!    里有它一行**存量登记**，而那张表**不在本拍的写区**——
//!    把这句话删干净会让「登记表保鲜」那条断言红在一行我删不掉的登记上；
//! ③ 它也**不能写成 `server.rs::<那个常量名>`**：那个符号今天住 `accounts/mod.rs`，
//!    写成符号地址会让 `every_symbol_address_in_the_sources_still_resolves` 当场红。
//! ⇒ 正确的改法是**同拍两处**：删那一行存量登记 ＋ 把这里换成符号地址。留给下一件。
//!
//! ⇒ **准确的说法只有一句**：换成这个形状**比先前那版强**（它把「顺手拆开」变成要**有意重建**），
//! 但它**不是买断**。真正拦住上面那几形的是**行为判据**（`KH2`/`KH4` 那几条走真子进程、真转发的），
//! 不是类型。
//!
//! ★★ **这一课要逐字记住**：`K-H2a` 七轮打出来的是「**有判据 ≠ 保证了**」；
//! 本件在它的下一件里犯了它的孪生形 ——「**读着像买断 ≠ 买断了**」，差的那一格叫**量过没有**。
//! 我提这个形状时**没有去打它**，PM 采纳时也没有；`D1` 打了，一刀就穿。
//!
//! # ⚠ 「这一行的 `base_url` 缺席」与「这一行不在表里」是两件事
//!
//! - 缺席 ⇒ 用**这一行所属那个 agent** 的默认上游（每 agent 一行，`super::AGENT_UPSTREAMS`）。
//!   它是**一行已经存在**的行的一个字段取默认值，**不是**回落。
//! - 不在表里 ⇒ **404，一个字节都不发上游**。
//!
//! 这两句读起来像，差别正是 `KH2` 要守的全部。

use super::super::route::segment_is_safe;
use super::super::upstream::Base;
use creds_core::store::{AccountEntry, AuthStyle, AuthStyleSetting};

// ★★ 🔴 〔`设计/20 §7` 步 2〕**`mod sealed` 删掉了** —— 换来的东西写在这里
//
// 先前 `Row`/`RoutingTable` 住一个私有 `mod sealed`，买的是「`table.rs` 自己那半
// 也够不到 `Row` 的字段」。`20 §6` 逐字判过这一格：那道墙挡的只是「顺手」，
// 而本模块头注那张表里两条实测（`D1-M1`/`D1-M2`）记着它挡不住「有意」。
//
// **换到手里的是两样更硬的**：
// 1. **一次请求只拿到一个 `Destination`**（`§6` 第 1 行）—— 跨行拼装在这条路上凑不出来；
// 2. **访问器从 `pub(crate)` 收成 `pub(super)`** —— `Row` 的那几个方法今天
//    **只有 `accounts/` 里面看得见**。层 1 连 `Row` 这个类型都点不到，
//    那是**编译器**买的，不是一条文本判据。⚠ 先前是 `pub(crate)`：整个 crate 都能调。
//
// ⇒ 净账是**收紧**：少了一道只挡「顺手」的墙，多了一道挡住整层的墙。
use creds_core::SecretKey;
use std::collections::BTreeMap;

/// 表里的一行：**这条路由发到哪儿 + 用哪把 key**。
///
/// # ⚠ 谁拿得到它 —— 这一段 `设计/20 §7` 步 2 重写过，**旧话与新话都留着**
///
/// **今天**：字段私有；三个访问器（[`Row::base`] · [`Row::key`] · [`Row::auth_style`]）
/// 全是 `pub(super)` ⇒ **只有 `accounts/` 里面够得到这一行的任何一格**。
/// 层 1（`server.rs` / `listen.rs` / `http1.rs` / `tee.rs`）连 `Row` 这个类型都点不到，
/// 它手里只有 `resolve` 递过来的**一个** `Destination`。**这一格是编译器买的。**
///
/// **先前**（`K-H2` 到 `20 §7` 步 1 之间）：`Row` 住一个私有 `mod sealed`，
/// 没有 `base()` 访问器，而三个方法是 `pub(crate)`。那一版的读数逐条留着，
/// 因为它是本仓最值钱的一课（「读着像买断 ≠ 买断了」）：
/// - `D1-M2`：**根本不用 `base()`** —— 两行同时在作用域里，`a.connect()` 配
///   `render_upstream_request(&head, …, b, …)`，**编译通过、跑得通**。
/// - `D1-M1`：连 `connect()` 都能绕 —— `host_header()` 返回的那个 `String`
///   足够把 `Base` **重建**出来 ⇒ 实测 **488 passed / 0 failed**。
/// - `D2 §五-2`：两个半边**不对称** —— `base` 那半要「有意重建」，
///   而 `key` 那半**一个方法调用**就够。
///
/// ⇒ 那一版挡的只是「顺手」。今天挡住「A 的端点配 B 的 key」的是
/// **一次请求只拿到一个 `Destination`**（`20 §6` 第 1 行）＋ 上面那条可见性，
/// 而**量它的**仍然是 `KH2`/`KH4` 那几条走真转发的行为判据 ＋ `wire_golden` 的字节金标准。
///
/// ⚠ **刻意没有 `derive(Debug)`**：同 `Relay`（`KS1` 的第二道）。
/// `SecretKey` 自己的 `Debug` 是遮蔽形，但少一个能顺手把整行印出来的入口就少一个出口。
pub(crate) struct Row {
    base: Base,
    key: Option<SecretKey>,
    /// 这一把 key **用哪种鉴权头**交给上游〔`K-R1`〕。
    ///
    /// ★ 它焊在这里而**不是**一个进程级设置，理由与 `base`/`key` 逐字同一条：
    /// 上游、key、鉴权头形状是**同一个决定的三个面**。分开取就写得出
    /// 「A 的端点 + B 的 key + C 的头风格」，而那一形的症状是**401**
    /// ——与「key 打错了」同形，查不出来。
    ///
    /// ⚠ 它**不是**方言（见 `creds_core::store::AUTH_STYLE_FIELD` 的头注）：
    /// 中转对 body 零解析，这一格一个字节的请求体都管不到。
    auth_style: AuthStyle,
}

impl Row {
    /// 这一行发到哪儿。
    ///
    /// # ⚠⚠ 🔴 **它是 `设计/20 §7` 步 1 新开的一个口，代价要认下来**
    ///
    /// 先前这里**刻意没有** `base()`，而外面拿得到的只有 `connect()` 与 `host_header()`
    /// ⇒ `&Base` 这个值不出这个边界。今天它出得去了，因为**层间契约要求它出去**：
    /// `Destination::{Passthrough,Substitute}` 逐字带着 `upstream`，而「连上游」
    /// 是层 1 的活（`20 §4`：`exchange` 那一行是「resolve → 连上游 → pump → tee」）。
    ///
    /// **换到手里的是 `20 §6` 第 1 行那一格**：先前那道墙挡的只是「顺手」
    /// （本模块头注两条实测 `D1-M1`/`D1-M2` 逐字记着它挡不住「有意」）；
    /// 今天挡住「A 的端点配 B 的 key」的是**一次请求只拿到一个 `Destination`**
    /// —— 上游与 key 是同一个变体的两个字段，要拼错得先有两个 `Destination`
    /// 同时在作用域里，而 `resolve` 只给一个。
    ///
    /// ⚠ 步 1 那一拍它是 `pub(crate)`（整个 crate 都能调）；**步 2 收成了 `pub(super)`**
    /// —— 只有 `accounts/` 看得见。「层 1 够不到 `Row`」从此是编译器买的。
    pub(super) fn base(&self) -> &Base {
        &self.base
    }

    /// 这一行的鉴权头形状。**不是**方言。
    pub(super) fn auth_style(&self) -> AuthStyle {
        self.auth_style
    }

    /// 这一行的 key。`None` = **原样转发下游那份鉴权头**。
    ///
    /// ⚠⚠ **`None` 不是「这一行不存在」** —— 它是一个**合法状态**：
    /// 订阅登录那一档（`acct_core::AUTH_KIND_SUBSCRIPTION`）本来就不该换头。
    /// 「行不在表里」与「行在但没 key」要两条判据，**不许合成一条**
    /// （合了会把一个合法状态判成错误）。
    pub(super) fn key(&self) -> Option<&SecretKey> {
        self.key.as_ref()
    }
}

/// `(agent, 账号 id)` → [`Row`]。**一个进程一张**，跨连接共享。
///
/// ⚠ 键是一个**二元组**而不是拼接串（`format!("{agent}/{id}")` 那一种）：
/// 拼接串要靠「两段里都不许出现分隔符」这条外部约束才不撞，二元组不靠任何约束。
pub(crate) struct RoutingTable {
    rows: BTreeMap<(String, String), Row>,
}

impl RoutingTable {
    /// **整个后端生产段里唯一一处造 `Row` 的地方**
    /// （`table_guard::the_only_place_that_welds_an_upstream_to_a_key_is_inside_layer_two`
    /// 那条相等断言钉着）。
    ///
    /// 收的是**分开的五样**（agent / id / 上游 / key / 鉴权头形状），出的是**焊死的一样**。
    /// 焊接这一步只此一次 ⇒ 「哪个上游配哪把 key、用哪种头」这个决定只有一个地方做得了。
    ///
    /// ⚠ `K-R1` 把鉴权头形状加进来，形状照前几样：**跟着行走，不做进程级设置**。
    /// ⚠ 条 49 把 agent 加进来：它只进**键**，不进 [`Row`] —— 「这一行属于谁」是索引，
    ///   不是这一行要发出去的任何一个字节。
    pub(crate) fn build(
        entries: impl IntoIterator<Item = (String, String, Base, Option<SecretKey>, AuthStyle)>,
    ) -> Self {
        let mut rows = BTreeMap::new();
        for (agent, id, base, key, auth_style) in entries {
            // ⚠ **排版随便拆，形状不能改。** `table_guard` 那条相等断言 09-09 起走
            //   `sites_layout_blind`：**先把生产段的空白全删干净**，再找无空白形的针
            //   `Row{base`（`table_guard::WELD`）⇒ 换行 / 缩进 / `cargo fmt` 一格都不影响它。
            //   仍然会让它数出 0 处、当场红的是**内容**上的改法：`base` 不再紧跟 `Row {`
            //   （字段反序，或中间插进别的字段）、`base` 改名、改用 `Row::new(…)` 或 `..`
            //   更新语法，以及把这处焊接搬出 `accounts/table.rs`。反方向也钉着：
            //   生产段里再出现第二处同形字面量 ⇒ 2 处 ⇒ 红（要求**恰好 1**）。
            //   ⇒ 真要写成那些形状，**改的是判据里那根针**，不是把这里的排版扭回去迁就它。
            //   〔原注写着「这一行必须留在一行上」：那是 09-09 `cargo fmt` 拆开它、老那把
            //   逐行尺子数出 0 处留下的化石，今天双重失效 —— 而它真正的害处，是在教
            //   下一个人拿生产代码的排版去迁就判据。〕
            rows.insert(
                (agent, id),
                Row {
                    base,
                    key,
                    auth_style,
                },
            );
        }
        Self { rows }
    }

    /// 查一条。**查不到就是 `None`** —— 调用方回 404，不许拿别的行顶上。
    ///
    /// 🔴 **两段都是键**（条 49）：同一个账号 id 挂在另一个 agent 名下 ⇒ 查不到。
    /// 不许「agent 查不到就只按账号再查一次」—— 那正是本次要拆掉的那一形。
    pub(super) fn lookup(&self, agent: &str, account: &str) -> Option<&Row> {
        self.rows.get(&(agent.to_string(), account.to_string()))
    }

    /// 表里有几行。只给日志与判据用。
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }
}

/// 一条**进不了表**的账号，以及它为什么进不了。
///
/// ⚠ 它必须被**说出去** —— 静默丢掉一行的症状是「我明明配了，中转永远 404」，
/// 而那查起来要人去读源码。出声那一步在 `creds::announce`。
pub(crate) struct Rejected {
    pub(crate) id: String,
    /// 固定文案（**不含文件内容**）⇒ 它进日志是安全的，理由登记在 `creds_guard::ALLOWED_LOG_ARGS`。
    pub(crate) why: &'static str,
}

/// 一条**进了表、但有一件事必须让人知道**的账号〔`K-R1`〕。
///
/// # ⚠ 它为什么是一个**新类型**，不是 `Rejected` 多一个字段
///
/// 「这一行不能用」与「这一行能用，但它的行为与默认不同」是**两件事**：
/// 前者的后果是 404，后者的后果是**字节变了但仍然发出去**。
/// 合成一个类型（比如加一个 `severity`）就是本工作区最贵的那族病
/// —— 一个值装了两件事，而下一个读它的人只会读到方便的那一半。
///
/// ⚠ `what` 与 `Rejected::why` 同为 `&'static str`，同一条理由：
/// 它进日志是安全的**由类型兜着**，不是由「记得别把文件内容塞进来」兜着。
pub(crate) struct Note {
    pub(crate) id: String,
    pub(crate) what: &'static str,
}

/// 账号 id 当不了路由段。
pub(crate) const WHY_ID_UNUSABLE: &str =
    "账号 id 当不了路由段（只许字母数字与 - _，最长 128 字节）";

/// 🔴 **明文 http 只许连回环**〔`K-R1`；`裁-1` 08-25「只准 TLS」的那一格例外〕。
///
/// 本地部署（`http://127.0.0.1:11434/...` 这一类）是〔用 09-04〕点名要的一等公民
/// ⇒ 回环上的明文放行。而**非回环 + 明文** = 那一行的 key 明着过网线
/// ⇒ 拒掉并出声，不是「出声之后照发」：出声照发这一档在这里等于
/// 「我告诉过你了」，而代价由用户付。
pub(crate) const WHY_PLAINTEXT_OFF_LOOPBACK: &str =
    "base_url 是明文 http 而主机不是本机回环 —— 那会把这一行的 key 明着发上网线。要么换 https，要么把上游放到本机回环上";

/// `auth_style` 写了一个认不出的词。**刻意不回落成默认值**。
pub(crate) const WHY_AUTH_STYLE_UNKNOWN: &str =
    "auth_style 写的不是账号层认得的值之一（认得的那几个见下面那行现算的清单）";

/// `auth_style` 说「一个鉴权头都不发」，而同一行又配了一把 key。
///
/// 两句话说的是相反的事 ⇒ **不猜哪一句是他的意思**：猜「用 key」就把一把真 key
/// 发给一个声明了不要鉴权的端点；猜「不发」就让人以为配好的 key 在生效。
/// ⇒ 拒掉并出声，让人自己删掉其中一句。
pub(crate) const WHY_NO_AUTH_WITH_KEY: &str =
    "这一条的 auth_style 说不发任何鉴权头，同一条却配了 api_key —— 两句话说的是相反的事，删掉其中一句";

/// 这一行的 `base_url` 带了路径前缀。
pub(crate) const NOTE_PATH_PREFIX: &str =
    "base_url 带路径前缀 ⇒ 上游收到的是「那个前缀 + 客户端自己的真路径」。若前缀与客户端的路径头一段重了，上游会看到重复的那一段（中转不替你合并）";

/// 这一行用 `Authorization: Bearer` 换头。
///
/// ⚠ 今天它**印不出来** —— 那正是 `AuthStyle::DEFAULT`，而默认那个不出声。
/// 留着它不是仪式：`note_for_auth_style` 里那个穷尽 `match` 要求每个成员都有话说，
/// 而把这一支写成「借用隔壁那句」的话，默认值哪天换了，它就开始报一句**假话**。
pub(crate) const NOTE_AUTH_STYLE_BEARER: &str =
    "这一条用 Authorization: Bearer 头把 key 交给上游，而客户端自带的鉴权头会被丢掉";

/// 这一行用 `x-api-key` 换头。
pub(crate) const NOTE_AUTH_STYLE_X_API_KEY: &str =
    "这一条用 x-api-key 头把 key 交给上游（不是默认那种），而客户端自带的鉴权头会被丢掉";

/// 这一行一个鉴权头都不发。
pub(crate) const NOTE_AUTH_STYLE_NO_AUTH: &str =
    "这一条一个鉴权头都不发，客户端自带的那份也不转发 —— 这是给不校验凭据的本地部署用的那一档";

/// 一个**非默认**的鉴权头形状该报哪一句；默认那个 ⇒ `None`（每次都印等于噪音）。
///
/// ★ 「哪个是默认」不写死在这里，问 `AuthStyle::DEFAULT` ——
/// 默认值哪天换了，本函数自动跟着换，而 `every_auth_style_other_than_the_default_gets_announced`
/// 会去核「非默认的每一个都有话说」。
fn note_for_auth_style(style: AuthStyle) -> Option<&'static str> {
    if style == AuthStyle::DEFAULT {
        return None;
    }
    // ⚠ 穷尽 `match`：加一个成员**编译不过**，而不是静默地不出声。
    Some(match style {
        AuthStyle::Bearer => NOTE_AUTH_STYLE_BEARER,
        AuthStyle::XApiKey => NOTE_AUTH_STYLE_X_API_KEY,
        AuthStyle::NoAuth => NOTE_AUTH_STYLE_NO_AUTH,
    })
}

/// 把文件里读出来的那些条，装成一张表。**每一条都挂在 `agent` 名下**（条 49）。
///
/// `default_base` 是 **`agent` 那一家**的默认上游（每 agent 一行，住 `super::AGENT_UPSTREAMS`），
/// 不是进程级的某一个 —— 它只给「这一行没写 `base_url`」那一格取值，**不是**回落。
///
/// # 「装不进去」的判断都在这里，都出声 —— **条数别写死，数下面那几条**
///
/// ⚠ 本节的标题先前逐字是「**两条**『装不进去』的判断」，而 `K-R1` 之后是四条。
/// 「报一个基数也是复述」（`brief` 13b）：标题里那个数会在下一次加判断时**自动变成假话**。
///
/// 1. **账号 id 当不了路由段** —— 走 [`segment_is_safe`]，
///    与 `route::parse` 用的是**同一个谓词**（`route.rs` 头注逐字论证过为什么不许各写一份）。
///    装不下的那条**永远匹配不上**任何请求，进表只会变成一行死行。
/// 2. **`base_url` 解析不了** —— 走 `Base::parse`。
///    ⚠ 这一条**刻意不回落到默认上游**：一个打错的端点回落到官方端点，
///    症状是「我配了第三方 API，它却在用官方的」——**那比 404 坏得多**。
///    ⚠⚠ **订正一句盘上的旧话〔`K-R1`，09-04〕**：这一行先前括号里逐字写着
///    「`裁-1` 只准 `https`，`http` 只给本机夹具」。前半仍然对，**后半今天不准确了**：
///    `http` 现在也给**本机部署**（〔用 09-04〕要的那一格），而「只给夹具」这个说法
///    读起来像「生产里配 `http` 是不该的」。今天准确的分界线是**回环**，见下面第 3 条。
///
/// 3. 🔴 **明文 http 指向非回环**〔`K-R1`〕—— 见 [`WHY_PLAINTEXT_OFF_LOOPBACK`]。
/// 4. 🔴 **`auth_style` 认不出** / **`auth_style` 说不发头却又配了 key**〔`K-R1`〕——
///    见 [`WHY_AUTH_STYLE_UNKNOWN`] / [`WHY_NO_AUTH_WITH_KEY`]。两条都**刻意不回落**。
///
/// # ⚠ 判断的**次序**是有意的，写下来（几条同时成立时报哪一句）
///
/// id → `base_url` 形状 → 明文/回环 → `auth_style` 认不认得 → 「不发头」与 key 的矛盾。
/// 从「这一行根本到不了」排到「这一行到得了但配法自相矛盾」：
/// 越前面的越是**这一行整个用不了**，报它更有用。
/// ⚠ 一条只报**第一个**理由（`continue`）—— 别把这读成「它只有一个毛病」。
///
/// # ⚠ 它**不**判什么
///
/// 不判 key 像不像一把 key（`store::read_key` 那条纪律逐字：人写进去什么，上游就该收到什么）。
/// 不判那个路径前缀「对不对」——**判不了**：对不对取决于上游怎么挂它的端点，
/// 而那要打真网才知道。⇒ 它换来的是一条 [`Note`]，不是一条判断。
pub(crate) fn build(
    entries: Vec<AccountEntry>,
    agent: &str,
    default_base: &Base,
) -> (RoutingTable, Vec<Rejected>, Vec<Note>) {
    let mut rows: Vec<(
        String,
        String,
        Base,
        Option<creds_core::SecretKey>,
        AuthStyle,
    )> = Vec::new();
    let mut rejected: Vec<Rejected> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();

    for e in entries {
        if !segment_is_safe(&e.id) {
            rejected.push(Rejected {
                id: e.id,
                why: WHY_ID_UNUSABLE,
            });
            continue;
        }
        let base = match e.base_url.as_deref() {
            None => default_base.clone(),
            Some(u) => match Base::parse(u) {
                Ok(b) => b,
                // ★ `K-R1`：理由**来自解析器自己**，不是调用方现编一句万能的话。
                //   先前这里写死「base_url 解析不了（要 https:// 或 http://）」，
                //   而那句话在「端口读不懂」「带了查询串」这几形上是**假的指引**。
                Err(issue) => {
                    rejected.push(Rejected {
                        id: e.id,
                        why: issue.0,
                    });
                    continue;
                }
            },
        };
        // 🔴 明文只许回环。⚠ 它判的是**装表这一刻的字面**，不是「连出去之后落到哪」——
        //    一个解析到回环的域名本条照样拒（`Base::host_is_loopback` 的头注写清了分母）。
        if !base.tls && !base.host_is_loopback() {
            rejected.push(Rejected {
                id: e.id,
                why: WHY_PLAINTEXT_OFF_LOOPBACK,
            });
            continue;
        }
        let auth_style = match e.auth_style {
            AuthStyleSetting::Absent => AuthStyle::DEFAULT,
            AuthStyleSetting::Known(s) => s,
            AuthStyleSetting::Unknown => {
                rejected.push(Rejected {
                    id: e.id,
                    why: WHY_AUTH_STYLE_UNKNOWN,
                });
                continue;
            }
        };
        if auth_style == AuthStyle::NoAuth && e.key.is_some() {
            rejected.push(Rejected {
                id: e.id,
                why: WHY_NO_AUTH_WITH_KEY,
            });
            continue;
        }
        // ⇒ 到这里这一行是**能用的**；下面两条只是「它的行为与默认不同，得让人知道」。
        if !base.path.is_empty() {
            notes.push(Note {
                id: e.id.clone(),
                what: NOTE_PATH_PREFIX,
            });
        }
        if let Some(what) = note_for_auth_style(auth_style) {
            notes.push(Note {
                id: e.id.clone(),
                what,
            });
        }
        rows.push((agent.to_string(), e.id, base, e.key, auth_style));
    }

    (RoutingTable::build(rows), rejected, notes)
}

#[cfg(test)]
#[path = "../../../../tests/backend/relay/table_tests.rs"]
mod tests;
