//! 路由表：路由键里那一段账号 → **上游与 key 焊在一起的一个值**〔`K-H2` `KH1`/`KH2`〕。
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
//! | 性质 | 今天真正由谁守 | 实测出来的洞（**分母 = `D1` 编出来的这几形**） |
//! |---|---|---|
//! | **从一个 `Row` 里拿不到 `&Base`** | ⭐ **编译器**（字段私有 · 住 `mod sealed` · 无 `base()` 访问器） | 这是编译器**真正**买到的**唯一**一条 |
//! | ⚠ **另一半（`&SecretKey`）根本不在这张表的保护面里** | **没有人** —— [`Row::key`] 就是一个 `pub(crate)` 访问器 | 〔`D2` `§五-2`，`C-补` 08-28 补的话，**不是新缺陷**〕`Row` 两个半边**不对称**：`base` 那半没访问器（要「有意重建」，见 `D1-M1`），`key` 那半**一个方法调用**就够 ⇒ 「拿 B 的 key」不需要重建任何东西。⚠ 它下游那一跳另有人守：把 `&SecretKey` 变成明文的地方由 `creds_guard` ㈢（`expose_for_auth_header(` 恰好 1 处）钉着 —— 但那守的是**明文出口**，**不是**「谁拿得到这个值」 |
//! | 上游与 key「只能同源」 | **只有行为判据** | `D1-M2`：两行同时在作用域、A 连 B 渲染 ⇒ **编译通过**（我复打过）。`D1-M1`：换签名收 `host: &str` + 用 `row.host_header()` **重建 `Base`** 去连 ⇒ **488 passed / 0 failed** |
//! | 进程里「没有默认上游可回落」 | **只有一条文本棘轮**（禁 `Relay` 里出现 `base:` / `key:` 字面） | **假**：`DEFAULT_UPSTREAM` 是 `server.rs:24` 的 crate 常量，`Base` 三个字段全 `pub(crate)`、`Base::parse` 也是 ⇒ 一行就能造一个 |
//! | 装表**只有一处**做 | **文本判据**（`table_guard.rs` 两条相等断言） | `D1-M4`：㈠ 那根针在它自称「真正的人群」（`sealed` 里面）**恰恰最弱** —— **一个字面量能焊出任意多行**，数字面量数不出「焊了几行、每行装了什么」。实测多行焊接 + 把回落整个加回来 ⇒ `table_guard` 单跑 **10 passed / 0 failed** |
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
//! - 缺席 ⇒ 用中转启动时那个**默认上游**（`CCM_RELAY_UPSTREAM` / 内置默认）。
//!   它是**一行已经存在**的行的一个字段取默认值，**不是**回落。
//! - 不在表里 ⇒ **404，一个字节都不发上游**。
//!
//! 这两句读起来像，差别正是 `KH2` 要守的全部。

use super::route;
use super::upstream::Base;
use creds_core::store::{AccountEntry, AuthStyle, AuthStyleSetting};

/// `Row` 与整张表都住这里。**外面够不到它的字段** —— 这是本模块全部意义所在。
///
/// 形状抄 `creds_core` 的 `mod sealed`（`K-H2a` 七轮之后落定的那一版）：
/// 那一件的经过逐字记着，同一条性质**换了四版人群**（某个字面 → 返回类型 →
/// 函数体碰没碰字段 → **换形状交给编译器**），**前三版逐版被打穿**。
/// ⇒ 这里直接上第四版。
mod sealed {
    use super::super::upstream::{self, Base, Conn};
    use creds_core::SecretKey;
    use std::collections::BTreeMap;

    /// 表里的一行：**这条路由发到哪儿 + 用哪把 key**。
    ///
    /// # ⚠ 字段是私有的，而且**刻意不给 `base()` 访问器** —— 它买到的**比读起来少**
    ///
    /// 外面拿得到的只有 [`Row::connect`]（这一行自己连自己的上游）与
    /// [`Row::host_header`]（这一行自己的 `Host:`）⇒ **`&Base` 这个值不出这个边界**。
    ///
    /// ⚠⚠ **订正〔`D1` 阻-1 回修，08-28〕**：先前这里逐字写着「给了 `base()`，
    /// **跨行拼装就又写得出来了**」—— 那句话的**逆否**读起来像「不给就写不出来」，
    /// 而那是假的，两条实测：
    /// - `D1-M2`：**根本不用 `base()`** —— 两行同时在作用域里，`a.connect()` 配
    ///   `render_upstream_request(&head, …, b, …)`，**编译通过、跑得通**（我自己复打过）。
    /// - `D1-M1`：连 `connect()` 都能绕 —— [`Row::host_header`] 返回的那个 `String`
    ///   足够把 `Base` **重建**出来（`Base::parse` 是 `pub(crate)`，三个字段也是），
    ///   换个签名收 `host: &str` 就行 ⇒ 实测 **488 passed / 0 failed**。
    ///
    /// ⇒ 准确的说法：**不给 `base()` 挡住的是「顺手」，挡不住「有意」。**
    /// 真正拦住那两形的是 `KH2`/`KH4` 那几条**走真转发的行为判据**（它们量的是
    /// 「哪个端点收到了哪把 key」，与这里写不写得出来无关）。
    ///
    /// ⚠⚠ **而上面那句只对 `base` 那一侧成立 —— `key` 那半连「有意」都不用**
    /// 〔`D2` `§五-2`，`C-补` 08-28 补的话；**不是新缺陷**，`D1-M2` 的读数里本来就含着它〕。
    /// 本结构体两个半边**不对称**：`base` 没有访问器，**`key` 有**（[`Row::key`]，`pub(crate)`）。
    /// ⇒ 「拿 B 的 key」是**一个方法调用**，不需要重建任何东西。
    /// 别把上面那句读成「两半都要重建才拿得到」—— 那会把这里说得比它实际强。
    /// （`&SecretKey` 那一侧今天由谁守、守到哪一格，见**模块头注**那张表新增的那一格。）
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
        auth_style: super::AuthStyle,
    }

    impl Row {
        /// 这一行自己连自己的上游。
        ///
        /// ★ 它是**整个后端生产段里唯一一处** `upstream::connect(` 的调用点
        /// （`table_guard::the_only_place_that_opens_an_upstream_connection_is_a_table_row`
        /// 那条相等断言钉着）⇒ **没有第二条路能绕过表把请求发出去**。
        pub(crate) fn connect(&self) -> std::io::Result<Conn> {
            upstream::connect(&self.base)
        }

        /// 这一行的 `Host:` 头该写什么。
        pub(crate) fn host_header(&self) -> String {
            self.base.host_header()
        }

        /// 这一行的**上游请求行目标** = 这一行的路径前缀 + 客户端那份真路径〔`K-R1`〕。
        ///
        /// # ⚠⚠ 它是本件**两处**会改变发出去的字节的地方之一，射程逐条写清
        ///
        /// 另一处是 `server.rs` 换头那一行（`auth_header_of` 那一支）。
        /// ⚠ **这一句是订正**：本节初稿逐字写的是「本件**唯一**一处会改变发出去的字节的地方」，
        /// 而那是一句**假的全称** —— 换头那一处也在改字节，而且就是本件另一半的正主。
        /// 〔`brief` 12：写「唯一 / 全部」这类话也是在报一个数，同句给分母。自查逮到，09-04。〕
        ///
        /// - `rest` 是**下游原样**的「真路径 + 查询串」（`route::parse` 保证它以 `/` 打头）。
        ///   本函数**一个字节都不改它**，只在**前面**接上这一行自己的前缀。
        /// - 前缀是 `""` 时，返回值与 `rest` **逐字节相同** ⇒ 没配前缀的那一路**零字节改动**。
        ///   ⚠ **这里刻意不写「盘上有几条字面量是这一路」那个数**：初稿抄了摸底那一拍的
        ///   「全部 9 条」，而**本件自己新加的判据里就有带路径的字面量** ⇒ 那个数在
        ///   写下它的同一个 commit 里就馊了。要现打就跑
        ///   `evidence/K-R1-B1-auth-style-and-base-path-census.py` 的第 ⑦c/⑦d 格
        ///   （它自己印两个分母：生产段 / 含测试段）。〔`brief` 13：别抄快照，指住址。〕
        /// - 🔴 **它不查重、不合并重复的段**：配 `https://h/v1` 而客户端发 `/v1/messages`
        ///   的人会得到 `/v1/v1/messages`。**这是有意的** —— 「顺手把重复的段合掉」
        ///   要先猜出「哪一段是重复」，而猜错的症状是**静默打到另一个地方**。
        ///   ⇒ 处置是**出声**（装表时给这一行记一条 `Note`，见 `NOTE_PATH_PREFIX`），
        ///   不是替人重写他写下的东西。
        /// - ⚠ **改前那一版这个函数不存在**，前缀在 `Base::parse` 里就被丢掉了
        ///   ⇒ `https://h/v1` **碰巧是对的**。本件把「碰巧对」换成「照写的做 + 出声」。
        ///
        /// ★ 它是 `Row` 的方法而不是一个收 `&Base` 的自由函数：`&Base` 这个值
        /// **不出这个边界**（本模块头注那条编译器真正买到的性质），拼接也不许把它带出去。
        pub(crate) fn upstream_target(&self, rest: &str) -> String {
            format!("{}{}", self.base.path, rest)
        }

        /// 这一行的鉴权头形状。**不是**方言。
        pub(crate) fn auth_style(&self) -> super::AuthStyle {
            self.auth_style
        }

        /// 这一行的 key。`None` = **原样转发下游那份鉴权头**。
        ///
        /// ⚠⚠ **`None` 不是「这一行不存在」** —— 它是一个**合法状态**：
        /// 订阅登录那一档（`acct_core::AUTH_KIND_SUBSCRIPTION`）本来就不该换头。
        /// 「行不在表里」与「行在但没 key」要两条判据，**不许合成一条**
        /// （合了会把一个合法状态判成错误）。
        pub(crate) fn key(&self) -> Option<&SecretKey> {
            self.key.as_ref()
        }
    }

    /// 账号 id → [`Row`]。**一个进程一张**，跨连接共享。
    pub(crate) struct RoutingTable {
        rows: BTreeMap<String, Row>,
    }

    impl RoutingTable {
        /// **整个后端生产段里唯一一处造 `Row` 的地方**
        /// （`table_guard::the_only_place_that_welds_an_upstream_to_a_key_is_inside_the_sealed_module`
        /// 那条相等断言钉着）。
        ///
        /// 收的是**分开的四样**（id / 上游 / key / 鉴权头形状），出的是**焊死的一样**。
        /// 焊接这一步只此一次 ⇒ 「哪个上游配哪把 key、用哪种头」这个决定只有一个地方做得了。
        ///
        /// ⚠ `K-R1` 把第四样加进来，形状照前三样：**跟着行走，不做进程级设置**。
        pub(crate) fn build(
            entries: impl IntoIterator<Item = (String, Base, Option<SecretKey>, super::AuthStyle)>,
        ) -> Self {
            let mut rows = BTreeMap::new();
            for (id, base, key, auth_style) in entries {
                // ⚠ 这一行**必须留在一行上**：`table_guard` 那条相等断言的针是逐行的
                //   `Row { base` ⇒ 换行拆开会让它数出 0 处、当场红。〔现打确认过〕
                rows.insert(
                    id,
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
        pub(crate) fn lookup(&self, account: &str) -> Option<&Row> {
            self.rows.get(account)
        }

        /// 表里有几行。只给日志与判据用。
        pub(crate) fn len(&self) -> usize {
            self.rows.len()
        }
    }
}

pub(crate) use sealed::{RoutingTable, Row};

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
    "auth_style 写的不是这个中转认得的值之一（认得的那几个见下面那行现算的清单）";

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

/// 把文件里读出来的那些条，装成一张表。
///
/// # 「装不进去」的判断都在这里，都出声 —— **条数别写死，数下面那几条**
///
/// ⚠ 本节的标题先前逐字是「**两条**『装不进去』的判断」，而 `K-R1` 之后是四条。
/// 「报一个基数也是复述」（`brief` 13b）：标题里那个数会在下一次加判断时**自动变成假话**。
///
/// 1. **账号 id 当不了路由段** —— 走 [`route::segment_is_safe`]，
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
    default_base: &Base,
) -> (RoutingTable, Vec<Rejected>, Vec<Note>) {
    let mut rows: Vec<(String, Base, Option<creds_core::SecretKey>, AuthStyle)> = Vec::new();
    let mut rejected: Vec<Rejected> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();

    for e in entries {
        if !route::segment_is_safe(&e.id) {
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
        rows.push((e.id, base, e.key, auth_style));
    }

    (RoutingTable::build(rows), rejected, notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use creds_core::store;

    fn base(u: &str) -> Base {
        Base::parse(u).expect("夹具的 base 应当解析得了")
    }

    fn entries(raw: &str) -> Vec<AccountEntry> {
        store::read_accounts(&store::parse(raw).expect("夹具应当可解析"))
    }

    /// ★★ 装表这一步**认得两条账号，各带各的上游与各自的 key**。
    #[test]
    fn two_accounts_land_in_two_rows_each_with_its_own_upstream_and_key() {
        let (t, bad, _) = build(
            entries(
                r#"{"accounts":{
                    "acct-a":{"api_key":"KEY-A","base_url":"https://a.invalid"},
                    "acct-b":{"api_key":"KEY-B"}
                }}"#,
            ),
            &base("https://default.invalid"),
        );
        assert!(bad.is_empty(), "不该有装不进去的行：{:?}", bad.len());
        assert_eq!(t.len(), 2);

        let a = t.lookup("acct-a").expect("A 应当查得到");
        let b = t.lookup("acct-b").expect("B 应当查得到");
        // ★★ **反空真排最前**〔`D1` 一并修〕：两行的 key 必须**不同**，
        //    否则下面「各用各的」整族断言恒真。先前它排在最后。
        assert_ne!(
            a.key().expect("A").expose_for_auth_header(),
            b.key().expect("B").expose_for_auth_header(),
            "两行的 key 一样 —— 下面整族断言在空转"
        );
        // ★ 同理：两行的**端点**也必须不同（本件题目就是每行端点不一样）。
        assert_ne!(
            a.host_header(),
            b.host_header(),
            "两行的端点一样 —— 「端点跟着行走」那一向量不到"
        );
        // 期望值全是**手写字面量**。
        assert_eq!(a.host_header(), "a.invalid");
        assert_eq!(a.key().expect("A 有 key").expose_for_auth_header(), "KEY-A");
        // 没写 `base_url` 的那条用**默认上游** —— 这不是回落，是这一行的字段取默认值。
        assert_eq!(b.host_header(), "default.invalid");
        assert_eq!(b.key().expect("B 有 key").expose_for_auth_header(), "KEY-B");
    }

    /// ★★★ **`KH2` 在这一层的那一半**：查不到就是查不到，**没有任何一行会被顶上来**。
    #[test]
    fn an_account_that_is_not_in_the_table_resolves_to_nothing() {
        let (t, _, _) = build(
            entries(r#"{"accounts":{"acct-a":{"api_key":"KEY-A"}}}"#),
            &base("https://default.invalid"),
        );
        // 分母 = 我列出的这 4 个不存在的 id。
        for miss in ["acct-b", "default", "", "ACCT-A"] {
            assert!(
                t.lookup(miss).is_none(),
                "{miss:?} 不在表里，却查到了一行 —— 那就是回落"
            );
        }
        // ★ 非空对照承重：同一张表、同一把尺子，**存在**的那一条查得到。
        assert!(t.lookup("acct-a").is_some(), "这把尺子是瞎的");
    }

    /// **进不了表的两形，都要出声**，而且不许静默回落。
    #[test]
    fn a_row_that_cannot_be_reached_or_cannot_be_parsed_is_rejected_out_loud() {
        let (t, bad, _) = build(
            entries(
                r#"{"accounts":{
                    "bad.id":{"api_key":"K1"},
                    "ok-id":{"api_key":"K2","base_url":"ftp://nope"},
                    "good":{"api_key":"K3"}
                }}"#,
            ),
            &base("https://default.invalid"),
        );
        // ★★ **承重的那条排最前**〔`D1` 一并修，08-28〕：
        //    先前它排在整条判据的**最后**，被前面 `assert_eq!(t.len(), 1)` 挡着
        //    ⇒ `D1-M6` 实测红在前面那条，**这一句一次都没被求值**。
        //    本件这是**第二次**同形（`KH2` 那两条已经修过一次）⇒ 全件扫过一遍。
        assert!(
            t.lookup("ok-id").is_none(),
            "打错的端点回落到了默认上游 —— 那比 404 坏得多"
        );
        // 同族的另一半：id 装不下的那条也不许在表里。
        assert!(t.lookup("bad.id").is_none(), "id 当不了路由段的那条进了表");

        assert_eq!(t.len(), 1, "只有 `good` 该进表");
        assert!(t.lookup("good").is_some());
        assert_eq!(bad.len(), 2, "两条都该被说出去：{}", bad.len());
        let ids: Vec<&str> = bad.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"bad.id"), "id 装不下那条没被说出去");
        assert!(ids.contains(&"ok-id"), "base_url 坏那条没被说出去");
    }

    /// `K-H2a` 那份**旧文件**（顶层一把 key）装出来是一条 id 逐字是 `default` 的行，
    /// 用默认上游 —— **别的 id 一律查不到**。
    #[test]
    fn the_legacy_single_key_file_becomes_exactly_one_reachable_row() {
        let (t, bad, _) = build(
            entries(r#"{"api_key":"LEGACY"}"#),
            &base("https://api.example.invalid"),
        );
        // ★★ **承重的那条排最前**〔`D1` 一并修〕：它是「**有名字的行**，不是默认行」
        //    这句话的全部内容 —— 先前它排在最后。
        assert!(
            t.lookup("anything-else").is_none(),
            "别的账号段也查到了那一行 —— 那它就成了「默认行」，正是 `KH2` 禁的回落"
        );
        assert!(bad.is_empty());
        assert_eq!(t.len(), 1);
        let row = t
            .lookup(store::LEGACY_ACCOUNT_ID)
            .expect("default 应当查得到");
        assert_eq!(row.host_header(), "api.example.invalid");
        assert_eq!(
            row.key().expect("有 key").expose_for_auth_header(),
            "LEGACY"
        );
    }

    /// **行在、但这一行没配 key** —— 合法状态（订阅那一档），`lookup` 查得到、`key()` 是 `None`。
    ///
    /// 这一条与上面那条 `an_account_that_is_not_in_the_table_resolves_to_nothing`
    /// **是两件事**，件计划逐字要求它们分开。
    #[test]
    fn a_row_can_exist_and_still_have_no_key_of_its_own() {
        let (t, bad, _) = build(
            entries(r#"{"accounts":{"sub-only":{"base_url":"https://sub.invalid"}}}"#),
            &base("https://default.invalid"),
        );
        assert!(bad.is_empty());
        let row = t.lookup("sub-only").expect("行应当在");
        assert!(row.key().is_none(), "这一行不该有 key");
        assert_eq!(row.host_header(), "sub.invalid");
        // ★ `K-R1`：没写 `auth_style` 的那一行落到默认 ⇒ 一份旧文件的行为一个字节不变。
        assert_eq!(row.auth_style(), AuthStyle::DEFAULT);
    }

    /// ★★★ **`K-R1` 的正主之一**：鉴权头形状**跟着行走**，两行各是各的。
    ///
    /// # 反空真排最前
    ///
    /// 两行的形状必须**不同** —— 一样的话「跟着行走」这一向根本量不到。
    #[test]
    fn each_row_carries_its_own_auth_style_and_a_missing_one_means_the_default() {
        let (t, bad, _) = build(
            entries(
                r#"{"accounts":{
                    "as-xapikey":{"api_key":"K1","auth_style":"x-api-key"},
                    "as-default":{"api_key":"K2"},
                    "as-local":{"base_url":"http://127.0.0.1:11434/v1","auth_style":"none"}
                }}"#,
            ),
            &base("https://default.invalid"),
        );
        assert!(bad.is_empty(), "不该有装不进去的行：{}", bad.len());
        assert_eq!(t.len(), 3);

        let x = t.lookup("as-xapikey").expect("x-api-key 那一行");
        let d = t.lookup("as-default").expect("没写的那一行");
        let l = t.lookup("as-local").expect("本地那一行");
        // ★★ 反空真：三行的形状两两不同（否则下面整族断言恒真）。
        assert_ne!(x.auth_style(), d.auth_style());
        assert_ne!(x.auth_style(), l.auth_style());
        assert_ne!(d.auth_style(), l.auth_style());
        // 期望值是**手写字面量**。
        assert_eq!(x.auth_style(), AuthStyle::XApiKey);
        assert_eq!(d.auth_style(), AuthStyle::DEFAULT);
        assert_eq!(l.auth_style(), AuthStyle::NoAuth);
        // ★ 本地那一行的三样**同时**成立：无鉴权 · 自定义 host:port/path · 明文回环。
        assert!(l.key().is_none(), "本地那一行不该有 key");
        assert_eq!(l.host_header(), "127.0.0.1:11434");
        assert_eq!(l.upstream_target("/v1/messages"), "/v1/v1/messages");
    }

    /// ★★★ `K-R1`：**认不出的 `auth_style` 不许被悄悄当成默认**，
    /// 「说不发头又配了 key」这条自相矛盾也一样 —— 两条各自一格，各自有话说。
    ///
    /// # 单断在这里是承重的
    ///
    /// 三条判断（认不出 / 矛盾 / 明文非回环）各有一格**只有它红**的夹具行，
    /// 而不是一格「全都坏」的行 —— 后者只买得到目录级塌陷。〔`brief` 9〕
    #[test]
    fn a_row_whose_auth_style_makes_no_sense_is_rejected_out_loud() {
        let (t, bad, _) = build(
            entries(
                r#"{"accounts":{
                    "typo":{"api_key":"K1","auth_style":"bearerr"},
                    "contradiction":{"api_key":"K2","auth_style":"none"},
                    "good":{"api_key":"K3","auth_style":"bearer"}
                }}"#,
            ),
            &base("https://default.invalid"),
        );
        // ★★ **承重的那两条排最前**：它们不许静默回落进表。
        assert!(
            t.lookup("typo").is_none(),
            "认不出的 auth_style 被回落成默认了 —— 症状是一条查不出来的 401"
        );
        assert!(
            t.lookup("contradiction").is_none(),
            "「不发任何头」与「配了 key」同时在，却被挑了一句执行 —— 那是替人猜意思"
        );
        // ★ 非空对照：好的那条进得去（不是恒拒）。
        assert_eq!(t.len(), 1, "只有 `good` 该进表");
        assert!(t.lookup("good").is_some(), "这把尺子是瞎的");

        // 两条的**理由不同** —— 合成一句的话，其中一句在另一形上是假的指引。
        let why = |id: &str| {
            bad.iter()
                .find(|r| r.id == id)
                .map(|r| r.why)
                .unwrap_or_else(|| panic!("{id} 没被说出去"))
        };
        assert_eq!(why("typo"), WHY_AUTH_STYLE_UNKNOWN);
        assert_eq!(why("contradiction"), WHY_NO_AUTH_WITH_KEY);
        assert_ne!(why("typo"), why("contradiction"));
    }

    /// ★★★ `K-R1`：**明文 http 只许连回环**。
    ///
    /// ⚠ 分母 = 我列出的这 4 形（2 形该拒、2 形该放）。它**不是**「所有明文写法」。
    #[test]
    fn a_plaintext_upstream_is_only_allowed_on_loopback() {
        let (t, bad, _) = build(
            entries(
                r#"{"accounts":{
                    "off-loopback":{"api_key":"K1","base_url":"http://1.2.3.4/v1"},
                    "private-lan":{"api_key":"K2","base_url":"http://10.0.0.1:8000/v1"},
                    "on-loopback":{"api_key":"K3","base_url":"http://127.0.0.1:11434/v1"},
                    "tls-anywhere":{"api_key":"K4","base_url":"https://1.2.3.4/v1"}
                }}"#,
            ),
            &base("https://default.invalid"),
        );
        // ★★ 承重的排最前：明文 + 非回环的两条**一条都不许进表**。
        assert!(
            t.lookup("off-loopback").is_none(),
            "明文 http 打到公网地址进了表 —— 那一行的 key 会明着过网线"
        );
        assert!(
            t.lookup("private-lan").is_none(),
            "内网也不是回环：那把 key 仍然明着过网线，只是网线短一点"
        );
        // ★ 非空对照（两格，各自单断）：回环上的明文放行，非回环上的 TLS 也放行
        //   ⇒ 本条判的是「明文 **且** 非回环」这个合，不是其中任一个。
        assert!(
            t.lookup("on-loopback").is_some(),
            "回环上的明文被拒了 —— 那把本地部署这一格整个关掉了"
        );
        assert!(
            t.lookup("tls-anywhere").is_some(),
            "TLS 打到公网被拒了 —— 本条把 `裁-1` 反过来读了"
        );
        assert_eq!(t.len(), 2);
        for id in ["off-loopback", "private-lan"] {
            let r = bad
                .iter()
                .find(|r| r.id == id)
                .unwrap_or_else(|| panic!("{id} 没被说出去"));
            assert_eq!(r.why, WHY_PLAINTEXT_OFF_LOOPBACK);
        }
    }

    /// ★★ `K-R1`：**能用但行为与默认不同的行要出声** —— 那是 [`Note`] 这个类型的全部意义。
    ///
    /// # 它治的是本件最阴的那一格
    ///
    /// 路径前缀是**承重的、会改变字节**，而它错了的时候（前缀与客户端路径头一段重了）
    /// 上游给的是一个 404 ——「配错了」与「上游挂了」同形。
    /// ⇒ 装表那一刻就把这件事说出来，别等它变成一条查不出来的 404。
    #[test]
    fn a_row_that_still_works_but_behaves_differently_gets_a_note() {
        let (t, bad, notes) = build(
            entries(
                r#"{"accounts":{
                    "plain":{"api_key":"K1"},
                    "prefixed":{"api_key":"K2","base_url":"https://gw.invalid/anthropic"},
                    "xapikey":{"api_key":"K3","auth_style":"x-api-key"}
                }}"#,
            ),
            &base("https://default.invalid"),
        );
        assert!(bad.is_empty(), "这三条都该进表：{}", bad.len());
        assert_eq!(t.len(), 3);

        let of = |id: &str| -> Vec<&'static str> {
            notes
                .iter()
                .filter(|n| n.id == id)
                .map(|n| n.what)
                .collect()
        };
        // ★★ **反空真排最前**：什么都没配特别的那一行**一条 note 都不该有**
        //    —— 没有这一格，「出声了」可能只是因为它对每一行都出声。
        assert!(
            of("plain").is_empty(),
            "默认那一行也出声了 ⇒ 全是噪音：{:?}",
            of("plain")
        );
        assert_eq!(of("prefixed"), vec![NOTE_PATH_PREFIX]);
        assert_eq!(of("xapikey"), vec![NOTE_AUTH_STYLE_X_API_KEY]);
        assert_eq!(notes.len(), 2, "note 的条数不对：{}", notes.len());
    }

    /// ★★ `K-R1`：**非默认的每一个鉴权头形状都有话说**，默认那个不说。
    ///
    /// 它把「哪个是默认」这件事的住址钉在 `AuthStyle::DEFAULT` 上：
    /// 默认值哪天换了，本条会去核新的那个不出声、旧的那个开始出声。
    /// ⚠ 它**不**证明那几句话是对的（散文没人机检），只证明「一个都没漏、也没有两个共用一句」。
    #[test]
    fn every_auth_style_other_than_the_default_gets_announced() {
        assert!(
            note_for_auth_style(AuthStyle::DEFAULT).is_none(),
            "默认那个形状也出声 ⇒ 每一行都会印一句，那是噪音不是信号"
        );
        let mut said: Vec<&'static str> = Vec::new();
        for s in AuthStyle::ALL.iter().copied() {
            if s == AuthStyle::DEFAULT {
                continue;
            }
            let what = note_for_auth_style(s)
                .unwrap_or_else(|| panic!("{s:?} 是非默认形状，却一个字都不说"));
            said.push(what);
        }
        // 反空真：真的走过至少一个非默认成员。
        assert!(!said.is_empty(), "闭集里只有默认那一个 —— 本条在空转");
        let n = said.len();
        said.sort();
        said.dedup();
        assert_eq!(said.len(), n, "有两个形状共用了同一句话：{said:?}");
    }
}
