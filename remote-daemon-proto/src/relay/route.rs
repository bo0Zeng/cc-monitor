//! 路由：从请求路径里切出**路由键**，把其余部分原样交给上游。
//!
//! 形状 `/s/<agent>/<account>/<key>/<真路径>` —— 三段都是**不透明串**。
//! 中转不认识任何 agent 叫什么，也不解释 `<key>` 是会话 id 还是别的什么。
//!
//! # ⚠ `<account>` 那一段是 `K-H2` 加的，理由与代价逐条记这里
//!
//! `K11 裁定一`（现行版）那张表逐字写着上游端点归「中转的路由表
//! （**路由键带账号那一段**）」——**而在 `K-H2` 之前，路由键里没有那一段**：
//! `<agent>` 是 `K9` 裁定二要的「agent-aware 的路由键」，`<key>` 是会话 id 那一类。
//! ⇒ 加这一段是**照裁定做**，不是发明一个新维度。
//!
//! **为什么不复用 `<key>` 段**（`K-H2` `Bx` 判的，PM 08-28 采纳）：
//! 那会让 `<key>` 同时装会话 id 与账号 id ——**一个值装了两件事**，
//! 本工作区最贵的那族病。它今天不疼（tee 那条流零消费者），
//! 而它的代价是**将来才发作、发作时找不回原因**的一次回退。
//! 加一段的代价是**今天可数的**：本文件的 `parse` · 本文件的判据 ·
//! `doc/IPC-PROTOCOL.md` 那一行 · tee 的行契约。
//!
//! ⚠ **`<account>` 段本身不保证那个账号存在** —— 它只保证「形状对得上」。
//! 「表里有没有这一行」是 `relay::table` 的活，查不到就是 **404**
//! （`K-H2` `KH2`：不许回落到别的账号的 key，也不许回落到默认上游）。

/// 切出来的路由。`rest` 逐字保留原请求的 `路径 + 查询串`，中转不重写它。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Route {
    pub(crate) agent: String,
    /// 路由表的**索引键**（`K-H2`）。中转不解释它，只拿它去查表。
    pub(crate) account: String,
    pub(crate) key: String,
    pub(crate) rest: String,
}

/// 一段路由键里允许的字符 —— 白名单，不是黑名单。
///
/// 收窄到这几类是**故意的**：路由键要参与日志与 tee 行，放开任意字节等于给
/// 「把控制字符 / 换行塞进 tee 流」开一条路。`.` 与 `/` **不在**白名单里 ⇒
/// `..` 这种段根本构造不出来。
///
/// ★ 它是 `pub(crate)` 的，理由是**同一条性质只许有一个实现**〔`K-H2`〕：
/// 装路由表的时候要判「这个账号 id 当得了路由段吗」，那与本函数问的是
/// **同一个问题**。两边各写一份，漂开的那天没有任何东西会说，
/// 而症状是「文件里配了一条账号，中转永远 404」这种查不出来的形状。
pub(crate) fn segment_is_safe(seg: &str) -> bool {
    !seg.is_empty()
        && seg.len() <= 128
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 解析 `/s/<agent>/<account>/<key>/<rest>`。不是这个形状就返回 `None`（调用方回 404）。
pub(crate) fn parse(target: &str) -> Option<Route> {
    let after = target.strip_prefix("/s/")?;
    let (agent, after) = after.split_once('/')?;
    let (account, after) = after.split_once('/')?;
    let (key, rest) = after.split_once('/')?;
    if !segment_is_safe(agent) || !segment_is_safe(account) || !segment_is_safe(key) {
        return None;
    }
    Some(Route {
        agent: agent.to_string(),
        account: account.to_string(),
        key: key.to_string(),
        rest: format!("/{rest}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_prefix_and_keeps_the_rest_verbatim() {
        let r = parse("/s/agentA/acctA/sid-AAA/v1/messages?beta=true").expect("应当解析成功");
        assert_eq!(r.agent, "agentA");
        assert_eq!(r.account, "acctA");
        assert_eq!(r.key, "sid-AAA");
        // ★ 期望值是**手写字面量**，不是拿被测函数算出来的（否则本断言自证、恒绿）。
        assert_eq!(r.rest, "/v1/messages?beta=true");
    }

    #[test]
    fn two_keys_do_not_collide() {
        let a = parse("/s/agentA/acctA/sid-AAA/v1/messages").expect("A");
        let b = parse("/s/agentB/acctB/sid-BBB/v1/messages").expect("B");
        assert_ne!(a.key, b.key);
        assert_ne!(a.agent, b.agent);
        assert_ne!(a.account, b.account);
        assert_eq!(a.rest, b.rest, "两条路由的真路径相同，区别只在键");
    }

    /// ★★ **`K-H2`**：三段各装一件事 —— 同一个 agent、同一条会话 id，
    /// **只有账号段不同**时，切出来的 `account` 必须不同，而别的两段与真路径**一个字节不差**。
    ///
    /// 它钉的是「账号身份**没有**被塞进别的段里」。
    #[test]
    fn the_account_segment_is_its_own_dimension() {
        let a = parse("/s/agentA/acct-one/sid-SAME/v1/messages").expect("one");
        let b = parse("/s/agentA/acct-two/sid-SAME/v1/messages").expect("two");
        assert_ne!(a.account, b.account, "账号段没被切出来");
        // 期望值是手写字面量。
        assert_eq!(a.account, "acct-one");
        assert_eq!(b.account, "acct-two");
        // 另外两段与真路径**完全相同** —— 账号身份没有渗进它们。
        assert_eq!(a.agent, b.agent);
        assert_eq!(a.key, b.key);
        assert_eq!(a.rest, b.rest);
        assert_eq!(a.rest, "/v1/messages");
    }

    #[test]
    fn rejects_everything_that_is_not_the_shape() {
        // ★ **非空对照排最前**〔`D1` 一并修，08-28〕：先证明这把尺子认得**合法**的那一形，
        //   否则下面整个循环可能只是因为 `parse` 恒返回 `None` 而全绿。
        //   （先前它排在循环之后 —— 循环一红，它就一次都没被求值。）
        assert!(parse("/s/agentA/acctA/sid-AAA/v1").is_some(), "这把尺子是瞎的");

        // 分母 = 我列出的这 9 形；不是「所有不合法输入」。
        for bad in [
            "/v1/messages",
            "/s/agentA",
            "/s/agentA/",
            "/s/agentA/acctA",
            "/s/agentA/acctA/sid-AAA",
            "/s//acctA/sid-AAA/v1",
            "/s/agentA//sid-AAA/v1",
            "/s/agentA/acctA//v1",
            "/s/../../../etc/v1",
        ] {
            assert!(parse(bad).is_none(), "这一形不该被接受：{bad}");
        }
    }

    /// ★★★ **写这条判据的第一版是错的，经过记下来** —— 它本来写在上面那张「不该被接受」
    /// 的表里，逐字是 `"/s/agentA/sid-AAA/v1/messages"`（`K-H2` 之前的**老三段形状**），
    /// 想当然以为「少一段 ⇒ 解析失败」。**实测不是。**
    ///
    /// # 老形状**不会**变成非法，它会被**重读成另一条四段路由**
    ///
    /// `/s/agentA/sid-AAA/v1/messages` 在新解析器眼里是
    /// `agent=agentA · account=sid-AAA · key=v1 · rest=/messages` —— 三段都过白名单。
    /// ⇒ **解析器拦不住它**，这一条必须由**表查不到 ⇒ 404** 兜（`K-H2` `KH2`）。
    ///
    /// # 为什么这值得单立一条判据
    ///
    /// 它把一句容易被读成「已经安全了」的话钉成它真正的样子：
    /// **「加了一段」买到的是「多一个维度」，不是「老客户端会被挡下来」。**
    /// 老客户端被挡下来靠的是另一格，而那一格在另一个文件里。
    #[test]
    fn the_old_three_segment_shape_is_not_rejected_here_it_is_reread_as_a_different_route() {
        let r = parse("/s/agentA/sid-AAA/v1/messages").expect("老形状在**本层**照样解析得了");
        // 期望值全是手写字面量。
        assert_eq!(r.agent, "agentA");
        assert_eq!(r.account, "sid-AAA", "老形状的会话 id 被读成了账号段");
        assert_eq!(r.key, "v1");
        assert_eq!(r.rest, "/messages");
        // ⇒ 它与真正想访问的那条路**不是同一条**：真路径被切掉了一截。
        assert_ne!(r.rest, "/v1/messages");
    }

    /// ⚠ **分母**〔回修轮之四 08-25 复扫补的〕：名字是个**全称**句（「cannot be smuggled」），
    /// 而它量的是**我列出的这 2 形**，**不是**「所有穿越写法」。真正承重的是白名单本身
    /// （段里只许出现白名单字符）—— 这两形是那条白名单的**样例**，不是它的证明。
    /// 〔隔壁 `rejects_everything_that_is_not_the_shape` 逐字写了「分母 = 我列出的这 7 形」，
    ///  本条先前一个字都没写 —— 同一族的话，同一份纪律。〕
    #[test]
    fn path_traversal_cannot_be_smuggled_through_a_segment() {
        // 分母 = 我列出的这 3 形（`K-H2` 加了账号段那一形）。
        assert!(parse("/s/agentA/acctA/..%2f..%2fetc/v1").is_none());
        assert!(parse("/s/a.b/acctA/sid/v1").is_none(), "点号不在白名单里");
        assert!(
            parse("/s/agentA/../sid/v1").is_none(),
            "账号段也要过同一条白名单"
        );
    }

    /// ★★★ `K-H2b` `KH2B4`：**注入侧真的拼出来的那一形，正是本解析器认的那一形。**
    ///
    /// # 这一条为什么必须存在（`LEDGER.md#KL7` 第 1 条）
    ///
    /// 注入侧拼错一段的症状**不是**「拼错了」，是「一个查不出来的 404」——
    /// 因为老三段形状**不会被本解析器拒掉**（隔壁那条判据实测过：它被重读成另一条四段路由、
    /// 解析成功），挡它的是**表里查不到**，而那在另一个文件里。
    /// ⇒ 两端各写各的，就会各自答错同一个问题，而症状指不向原因。
    ///
    /// # ★ 它是**跨半边对拍**，不是「再抄一遍」
    ///
    /// 样例**不是**手抄的常量，是**从 monitor 的源码里抠出来的**
    /// （`include_str!` 那份 `payload.rs`，取 `RELAY_ROUTE_SAMPLE` 那一行的字面量）。
    /// 两侧因此被焊成一条链：
    ///
    /// | 环 | 谁钉的 |
    /// |---|---|
    /// | 构造口的产物 == `RELAY_ROUTE_SAMPLE` | monitor 侧 `the_relay_route_sample_is_what_the_builder_really_produces` |
    /// | `RELAY_ROUTE_SAMPLE` 落进本解析器的哪几段 | **本条** |
    ///
    /// ⇒ monitor 那边改了段序或前缀 ⇒ 抠出来的样例跟着变 ⇒ **本条的槽位断言当场红**；
    /// 本解析器改了切法 ⇒ 同一条样例切出别的段 ⇒ **也是本条红**。
    /// 「两半漂开而两边都不红」这一形，从这一刀起不成立。
    ///
    /// ⚠ 这条**跨半边编译期边已登记**在 `src-tauri/src/cross_half_edge_registry.rs`
    /// （那张表默认拒绝：不登记就红），并且它**只长在判据里** ——
    /// `no_cross_half_edge_lives_in_production_code` 钉着「一条都不许长到生产段」。
    #[test]
    fn the_shape_the_injection_side_builds_lands_in_the_slots_this_parser_expects() {
        // ★ 跨半边：读 monitor 那一侧的**源码**，把它的样例常量抠出来。
        const MONITOR_PAYLOAD_RS: &str =
            include_str!("../../../src-tauri/src/backend/control/payload.rs");
        let key = "pub const RELAY_ROUTE_SAMPLE: &str = \"";
        let at = MONITOR_PAYLOAD_RS.find(key).expect(
            "monitor 侧找不到 `RELAY_ROUTE_SAMPLE` —— 抽取器坏了或那个常量被改名了，\n\
             本条会在一个空串上自问自答（抽取器自检就是为了不让它悄悄变成那样）",
        );
        let rest = &MONITOR_PAYLOAD_RS[at + key.len()..];
        let sample = &rest[..rest.find('"').expect("那个字面量没收尾")];
        // 抽取器自检：抠出来的必须像一条路由键，不是空串、不是半截。
        assert!(
            sample.starts_with("/s/") && sample.len() > 10,
            "从 monitor 抠出来的样例不像路由键：{sample:?}"
        );

        // 注入侧给的是 base URL（不带真路径），agent 自己往后接 `/v1/messages`。
        let target = format!("{sample}/v1/messages");
        let r = parse(&target).unwrap_or_else(|| {
            panic!(
                "monitor 侧真的拼出来的那一形，本解析器解析不了：{target:?}\n\
                 ⇒ 两半的路由键形状漂开了。注入侧拼错一段在生产上只表现成\n\
                 「一个查不出来的 404」，指不向原因 —— 所以要在这里当场红。"
            )
        });
        // 期望值全是**手写字面量**（不是拿被测函数算的，否则自证恒绿）。
        // ⚠ 它们同时是「monitor 那边不许偷偷换段序」的那道闸：换了，下面三条里必有一条红。
        assert_eq!(r.agent, "claude-code", "第 2 段不是 agent 了 —— 两半漂开");
        assert_eq!(r.account, "acct-a", "账号段没落在第 3 段 —— 表就查错行了");
        assert_eq!(r.key, "k-0123456789abcdef", "第 4 段不是 key 了 —— 两半漂开");
        assert_eq!(r.rest, "/v1/messages", "真路径没被原样透传");

        // ★ 同一条样例，把**账号段**换掉 ⇒ 切出来的 account 必须跟着变（它是自己一维）。
        let other = parse("/s/claude-code/acct-b/k-0123456789abcdef/v1/messages").expect("另一行");
        assert_ne!(r.account, other.account);
        assert_eq!(other.account, "acct-b");
        // ★ 段序对调（`<account>` 与 `<key>` 换位）**照样解析得了** ——
        //   这正是「拼错一段只表现成 404」的机制，本断言把它钉成明文。
        let swapped =
            parse("/s/claude-code/k-0123456789abcdef/acct-a/v1/messages").expect("对调也解析得了");
        assert_eq!(
            swapped.account, "k-0123456789abcdef",
            "对调之后被当成账号的是那个 key —— 解析器拦不住它，只有表能"
        );
        assert_ne!(swapped.account, r.account);
    }

    /// ★ **同一条性质只许有一个实现**：装路由表时判「这个账号 id 当得了路由段吗」
    /// 走的必须是本模块这个 [`segment_is_safe`]，不是另写一份。
    ///
    /// 本条钉的是**那个谓词与 `parse` 的判断一致** —— 它俩要是漂开了，
    /// 症状是「文件里配了一条账号，中转永远 404」，而两边各自看起来都没错。
    #[test]
    fn the_exported_predicate_agrees_with_what_parse_accepts() {
        // 分母 = 我列出的这 5 个 id（**都不含 `/`**，理由见下面那一段）。
        for (id, ok) in [
            ("acctA", true),
            ("my-account", true),
            ("my_account_2", true),
            ("has.dot", false),
            ("", false),
        ] {
            assert_eq!(segment_is_safe(id), ok, "谓词对 {id:?} 的判断不对");
            // ★ 同一个 id 走 `parse` 那条真实的路，两边必须给同一个答案。
            let parsed = parse(&format!("/s/agentA/{id}/sid/v1")).is_some();
            assert_eq!(
                parsed, ok,
                "`segment_is_safe` 与 `parse` 对 {id:?} 的判断漂开了"
            );
        }

        // ⚠ **含 `/` 的 id 刻意不进上面那个循环**，如实说清为什么：
        //   它拼进 URL 之后**根本不是一段** —— `/s/agentA/has/slash/sid/v1` 会被
        //   `parse` 读成 `account=has · key=slash`，**解析成功**。
        //   ⇒ 「谓词说不行、`parse` 说行」在这一形上是**对的**，不是漂移：
        //     谓词回答的是「这个 id 当得了**一段**吗」，`parse` 回答的是「这条 URL 是那个形状吗」。
        //   真正兜住它的是**表里查不到 `has`** ⇒ 404。
        assert!(!segment_is_safe("has/slash"), "含 `/` 的 id 必须被谓词拒掉");
        assert!(
            parse("/s/agentA/has/slash/sid/v1").is_some(),
            "这一形**确实**解析得了 —— 上面那段说明不是假设"
        );
    }
}
