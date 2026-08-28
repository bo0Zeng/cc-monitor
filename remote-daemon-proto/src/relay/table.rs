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
//! # ★★ 买法：让它**写不出来**，而不是再检测一遍〔`K-H2a` 七轮之后学到的那条〕
//!
//! 三件事分别由谁买，逐条说清（**别把它们读成一件事**）：
//!
//! | 性质 | 谁买的 | 诚实边界 |
//! |---|---|---|
//! | 进程里**没有「默认上游」可回落** | **编译器**：`Relay` 里不再有那个字段，没有那个值 | — |
//! | **上游与 key 只能同源** | **编译器**：它俩是 [`Row`] 的**私有字段**，`Base` 一次都不出 `Row` 的边界（外面拿不到 `&Base`，只拿得到 `connect()` / `host_header()`） | ⚠ **没买到**「两行同时在作用域里时，有人拿 A 连、拿 B 渲染」—— 今天 `handle` 里**只有一行**在作用域，这一格靠的是**那个事实**，不是类型 |
//! | 装表这件事**只有一处**做 | **判据**（`table_guard.rs` 两条相等断言） | 人群与分母写在那两条判据的头注里 |
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
use creds_core::store::AccountEntry;

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
    /// # ⚠ 字段是私有的，而且**刻意不给 `base()` 访问器**
    ///
    /// 给了 `base()`，`upstream::connect(row_a.base())` 与
    /// `render(row_b)` 这种**跨行拼装**就又写得出来了 —— 而那正是要防的那一形。
    /// ⇒ 外面拿得到的只有 [`Row::connect`]（这一行自己连自己的上游）与
    /// [`Row::host_header`]（这一行自己的 `Host:`）。`Base` 一次都不出这个边界。
    ///
    /// ⚠ **刻意没有 `derive(Debug)`**：同 `Relay`（`KS1` 的第二道）。
    /// `SecretKey` 自己的 `Debug` 是遮蔽形，但少一个能顺手把整行印出来的入口就少一个出口。
    pub(crate) struct Row {
        base: Base,
        key: Option<SecretKey>,
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
        /// 收的是**分开的三样**（id / 上游 / key），出的是**焊死的一样**。
        /// 焊接这一步只此一次 ⇒ 「哪个上游配哪把 key」这个决定只有一个地方做得了。
        pub(crate) fn build(
            entries: impl IntoIterator<Item = (String, Base, Option<SecretKey>)>,
        ) -> Self {
            let mut rows = BTreeMap::new();
            for (id, base, key) in entries {
                rows.insert(id, Row { base, key });
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

pub(crate) use sealed::{Row, RoutingTable};

/// 一条**进不了表**的账号，以及它为什么进不了。
///
/// ⚠ 它必须被**说出去** —— 静默丢掉一行的症状是「我明明配了，中转永远 404」，
/// 而那查起来要人去读源码。出声那一步在 `creds::announce`。
pub(crate) struct Rejected {
    pub(crate) id: String,
    /// 固定文案（**不含文件内容**）⇒ 它进日志是安全的，理由登记在 `creds_guard::ALLOWED_LOG_ARGS`。
    pub(crate) why: &'static str,
}

/// 把文件里读出来的那些条，装成一张表。
///
/// # 两条「装不进去」的判断，都在这里，都出声
///
/// 1. **账号 id 当不了路由段** —— 走 [`route::segment_is_safe`]，
///    与 `route::parse` 用的是**同一个谓词**（`route.rs` 头注逐字论证过为什么不许各写一份）。
///    装不下的那条**永远匹配不上**任何请求，进表只会变成一行死行。
/// 2. **`base_url` 解析不了** —— 走 `Base::parse`（`裁-1` 只准 `https`，`http` 只给本机夹具）。
///    ⚠ 这一条**刻意不回落到默认上游**：一个打错的端点回落到官方端点，
///    症状是「我配了第三方 API，它却在用官方的」——**那比 404 坏得多**。
///
/// # ⚠ 它**不**判什么
///
/// 不判 key 像不像一把 key（`store::read_key` 那条纪律逐字：人写进去什么，上游就该收到什么）。
pub(crate) fn build(entries: Vec<AccountEntry>, default_base: &Base) -> (RoutingTable, Vec<Rejected>) {
    let mut rows: Vec<(String, Base, Option<creds_core::SecretKey>)> = Vec::new();
    let mut rejected: Vec<Rejected> = Vec::new();

    for e in entries {
        if !route::segment_is_safe(&e.id) {
            rejected.push(Rejected {
                id: e.id,
                why: "账号 id 当不了路由段（只许字母数字与 - _，最长 128 字节）",
            });
            continue;
        }
        let base = match e.base_url.as_deref() {
            None => default_base.clone(),
            Some(u) => match Base::parse(u) {
                Some(b) => b,
                None => {
                    rejected.push(Rejected {
                        id: e.id,
                        why: "base_url 解析不了（要 https:// 或 http://）",
                    });
                    continue;
                }
            },
        };
        rows.push((e.id, base, e.key));
    }

    (RoutingTable::build(rows), rejected)
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
        let (t, bad) = build(
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
        // 期望值全是**手写字面量**。
        assert_eq!(a.host_header(), "a.invalid");
        assert_eq!(
            a.key().expect("A 有 key").expose_for_auth_header(),
            "KEY-A"
        );
        // 没写 `base_url` 的那条用**默认上游** —— 这不是回落，是这一行的字段取默认值。
        assert_eq!(b.host_header(), "default.invalid");
        assert_eq!(
            b.key().expect("B 有 key").expose_for_auth_header(),
            "KEY-B"
        );
        // ★★ 两行的 key **不同**（否则下面「各用各的」那族判据全是恒真）。
        assert_ne!(
            a.key().expect("A").expose_for_auth_header(),
            b.key().expect("B").expose_for_auth_header()
        );
    }

    /// ★★★ **`KH2` 在这一层的那一半**：查不到就是查不到，**没有任何一行会被顶上来**。
    #[test]
    fn an_account_that_is_not_in_the_table_resolves_to_nothing() {
        let (t, _) = build(
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
        let (t, bad) = build(
            entries(
                r#"{"accounts":{
                    "bad.id":{"api_key":"K1"},
                    "ok-id":{"api_key":"K2","base_url":"ftp://nope"},
                    "good":{"api_key":"K3"}
                }}"#,
            ),
            &base("https://default.invalid"),
        );
        assert_eq!(t.len(), 1, "只有 `good` 该进表");
        assert!(t.lookup("good").is_some());
        assert_eq!(bad.len(), 2, "两条都该被说出去：{}", bad.len());
        let ids: Vec<&str> = bad.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"bad.id"), "id 装不下那条没被说出去");
        assert!(ids.contains(&"ok-id"), "base_url 坏那条没被说出去");
        // ★★ **base_url 坏的那条没有回落到默认上游** —— 它根本不在表里。
        assert!(
            t.lookup("ok-id").is_none(),
            "打错的端点回落到了默认上游 —— 那比 404 坏得多"
        );
    }

    /// `K-H2a` 那份**旧文件**（顶层一把 key）装出来是一条 id 逐字是 `default` 的行，
    /// 用默认上游 —— **别的 id 一律查不到**。
    #[test]
    fn the_legacy_single_key_file_becomes_exactly_one_reachable_row() {
        let (t, bad) = build(
            entries(r#"{"api_key":"LEGACY"}"#),
            &base("https://api.example.invalid"),
        );
        assert!(bad.is_empty());
        assert_eq!(t.len(), 1);
        let row = t.lookup(store::LEGACY_ACCOUNT_ID).expect("default 应当查得到");
        assert_eq!(row.host_header(), "api.example.invalid");
        assert_eq!(
            row.key().expect("有 key").expose_for_auth_header(),
            "LEGACY"
        );
        // ★ 它是**有名字的一行**，不是「默认行」：别的账号段查不到。
        assert!(t.lookup("anything-else").is_none());
    }

    /// **行在、但这一行没配 key** —— 合法状态（订阅那一档），`lookup` 查得到、`key()` 是 `None`。
    ///
    /// 这一条与上面那条 `an_account_that_is_not_in_the_table_resolves_to_nothing`
    /// **是两件事**，件计划逐字要求它们分开。
    #[test]
    fn a_row_can_exist_and_still_have_no_key_of_its_own() {
        let (t, bad) = build(
            entries(r#"{"accounts":{"sub-only":{"base_url":"https://sub.invalid"}}}"#),
            &base("https://default.invalid"),
        );
        assert!(bad.is_empty());
        let row = t.lookup("sub-only").expect("行应当在");
        assert!(row.key().is_none(), "这一行不该有 key");
        assert_eq!(row.host_header(), "sub.invalid");
    }
}
