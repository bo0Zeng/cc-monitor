//! ⚠⚠ 🔴 **条 48（`设计/20 §7` 步 1）之后，本文件里的字段名换了一遍，逐条对照**：
//!
//! | 先前 | 今天 | 它是什么 |
//! |---|---|---|
//! | `r.agent` | `r.key.seg1` | 路径第 1 段 —— 层 1 不解释它 |
//! | `r.account` | `r.key.seg2` | 路径第 2 段 —— 层 1 不解释它 |
//! | `r.key` | `r.stream` | 路径第 3 段 —— 层 1 那条流的名字 |
//! | （没有） | `r.mode` | 哪个前缀进来的 |
//!
//! **断言的内容一条没改** —— 改的只是怎么称呼那几个槽位。下面散文里仍然出现
//! 「agent」「账号」这两个词：那是**在说槽位里装的是什么**（规格的语言），
//! 不是在说层 1 的类型里有这两个名字。〔条 61：`C1` 的人群不含注释。〕

use super::*;

#[test]
fn strips_the_prefix_and_keeps_the_rest_verbatim() {
    let r = parse("/s/agentA/acctA/sid-AAA/v1/messages?beta=true").expect("应当解析成功");
    assert_eq!(r.mode, super::super::Mode::Substitute, "`/s/` 是代入模式");
    assert_eq!(r.key.seg1, "agentA");
    assert_eq!(r.key.seg2, "acctA");
    assert_eq!(r.stream, "sid-AAA");
    // ★ 期望值是**手写字面量**，不是拿被测函数算出来的（否则本断言自证、恒绿）。
    assert_eq!(r.rest, "/v1/messages?beta=true");
}

/// ★★★ 🔴 〔`设计/20 §7` 步 3〕**两个前缀切出**同样的四个槽位，**只有模式不同**。
///
/// # 它钉的是哪一句
///
/// `20 §0` 逐字：「路由的线格式已经是对的，四个槽位不用动 —— 要动的只是层 1 怎么称呼
/// 它们」。⇒ 加 `/t/` **不许**顺手换段序、不许多剥一段、不许对某一段放宽白名单。
///
/// # ⚠ 分母：本条量的是**这两条**（`/s/` 与 `/t/`），不是「所有前缀」
///
/// 「别的前缀一律不认」由下面那条 `rejects_everything_that_is_not_the_shape` 的
/// `/x/…` 那一形兜。
#[test]
fn both_prefixes_cut_the_same_four_slots_and_differ_only_in_the_mode() {
    let s = parse("/s/agentA/acctA/sid-AAA/v1/messages?beta=true").expect("`/s/` 应当解析成功");
    let t = parse("/t/agentA/acctA/sid-AAA/v1/messages?beta=true").expect("`/t/` 应当解析成功");

    // ★ 模式那一格：期望值是**手写字面量**，两边不同。
    assert_eq!(s.mode, super::super::Mode::Substitute);
    assert_eq!(t.mode, super::super::Mode::Passthrough);
    assert_ne!(
        s.mode, t.mode,
        "两个前缀必须切出两种模式 —— 一样就等于只有一条路"
    );

    // ★ 四个槽位：**逐格相同**。
    assert_eq!(s.key, t.key, "前两段必须一模一样");
    assert_eq!(s.stream, t.stream, "流标签必须一模一样");
    assert_eq!(s.rest, t.rest, "真路径必须一模一样");
    // 手写字面量（不拿被测函数算）。
    assert_eq!(t.key.seg1, "agentA");
    assert_eq!(t.key.seg2, "acctA");
    assert_eq!(t.stream, "sid-AAA");
    assert_eq!(t.rest, "/v1/messages?beta=true");
}

#[test]
fn two_keys_do_not_collide() {
    let a = parse("/s/agentA/acctA/sid-AAA/v1/messages").expect("A");
    let b = parse("/s/agentB/acctB/sid-BBB/v1/messages").expect("B");
    assert_ne!(a.stream, b.stream);
    assert_ne!(a.key.seg1, b.key.seg1);
    assert_ne!(a.key.seg2, b.key.seg2);
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
    assert_ne!(a.key.seg2, b.key.seg2, "账号段没被切出来");
    // 期望值是手写字面量。
    assert_eq!(a.key.seg2, "acct-one");
    assert_eq!(b.key.seg2, "acct-two");
    // 另外两段与真路径**完全相同** —— 账号身份没有渗进它们。
    assert_eq!(a.key.seg1, b.key.seg1);
    assert_eq!(a.stream, b.stream);
    assert_eq!(a.rest, b.rest);
    assert_eq!(a.rest, "/v1/messages");
}

#[test]
fn rejects_everything_that_is_not_the_shape() {
    // ★ **非空对照排最前**〔`D1` 一并修，08-28〕：先证明这把尺子认得**合法**的那一形，
    //   否则下面整个循环可能只是因为 `parse` 恒返回 `None` 而全绿。
    //   （先前它排在循环之后 —— 循环一红，它就一次都没被求值。）
    assert!(
        parse("/s/agentA/acctA/sid-AAA/v1").is_some(),
        "这把尺子是瞎的"
    );
    assert!(
        parse("/t/agentA/acctA/sid-AAA/v1").is_some(),
        "这把尺子对 `/t/` 是瞎的 —— 下面那几条 `/t/` 的否定就成了空真"
    );

    // 分母 = 我列出的这 12 形；不是「所有不合法输入」。
    // 〔`设计/20 §7` 步 3 加了后 3 形：`/t/` 少一段照样不认 ＋ **第三个前缀一律不认**。〕
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
        "/t/agentA/acctA/sid-AAA",
        "/t/agentA//sid-AAA/v1",
        "/x/agentA/acctA/sid-AAA/v1",
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
    assert_eq!(r.key.seg1, "agentA");
    assert_eq!(r.key.seg2, "sid-AAA", "老形状的会话 id 被读成了账号段");
    assert_eq!(r.stream, "v1");
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
/// ⚠ 这条**跨半边编译期边已登记**在 `src/bridge/src/cross_half_edge_registry.rs`
/// （那张表默认拒绝：不登记就红），并且它**只长在判据里** ——
/// `no_cross_half_edge_lives_in_production_code` 钉着「一条都不许长到生产段」。
#[test]
fn the_shape_the_injection_side_builds_lands_in_the_slots_this_parser_expects() {
    // ★ 跨半边：读 monitor 那一侧的**源码**，把它的样例常量抠出来。
    const MONITOR_PAYLOAD_RS: &str =
        include_str!("../../../src/bridge/src/backend/control/payload.rs");
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
    assert_eq!(
        r.key.seg1, "claude-code",
        "第 2 段不是 agent 了 —— 两半漂开"
    );
    assert_eq!(r.key.seg2, "acct-a", "账号段没落在第 3 段 —— 表就查错行了");
    assert_eq!(
        r.stream, "k-0123456789abcdef",
        "第 4 段不是 key 了 —— 两半漂开"
    );
    assert_eq!(r.rest, "/v1/messages", "真路径没被原样透传");

    // ★ 同一条样例，把**账号段**换掉 ⇒ 切出来的 account 必须跟着变（它是自己一维）。
    let other = parse("/s/claude-code/acct-b/k-0123456789abcdef/v1/messages").expect("另一行");
    assert_ne!(r.key.seg2, other.key.seg2);
    assert_eq!(other.key.seg2, "acct-b");
    // ★ 段序对调（`<account>` 与 `<key>` 换位）**照样解析得了** ——
    //   这正是「拼错一段只表现成 404」的机制，本断言把它钉成明文。
    let swapped =
        parse("/s/claude-code/k-0123456789abcdef/acct-a/v1/messages").expect("对调也解析得了");
    assert_eq!(
        swapped.key.seg2, "k-0123456789abcdef",
        "对调之后被当成账号的是那个 key —— 解析器拦不住它，只有表能"
    );
    assert_ne!(swapped.key.seg2, r.key.seg2);
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

/// ★★★ **跨半边对拍**：「凭据文件那些行属于哪一家」在后端（层 2 的
/// `accounts::CREDENTIALS_FILE_AGENT`）与 monitor（起会话时判「要不要改写 apikey 端点」那一格，
/// `payload.rs::APIKEY_TABLE_AGENT`）**是同一个值**〔条 49〕。
///
/// 两侧**异源**：本侧是后端那个常量，那一侧是 `payload.rs` 源码里那一行的**字面量**
/// （`include_str!` 现抠，不是 `use`）。漂开的症状：monitor 给 A 家注入 `/s/A/…`，
/// 而后端把那些行挂在 B 家名下 ⇒ **每一发 404**。
///
/// # ⚠ 它为什么住在**层 1** 的判据文件里（照实写）
///
/// 它钉的是层 2 的一个事实，本该住 `table_tests.rs`。挪过来只为一件事：
/// **跨半边的编译期边**有一张默认拒绝的登记表（`src/bridge/src/cross_half_edge_registry.rs`），
/// 而 `route_tests.rs → payload.rs` 这条边**已经登记着**（上面那条对拍用的就是它）。
/// 在 `table_tests.rs` 里再开一条 = 新登记一条边，而那张表不在本拍的写区。
/// ⇒ 复用这一条，**边的集合一格不变**。它只读层 2 那一个常量，层 1 的生产段不因此多认识层 2 一个字
/// （`account_layer_guard` 只扫生产段，判据段不在它的人群里）。
///
/// ⚠ 买不到：monitor 那一侧**真的拿它去判了**。那一格由 monitor 自己的判据量
/// （`payload_tests::another_agent_with_the_same_account_id_is_not_routed_to_that_row`）。
#[test]
fn the_credentials_file_agent_is_the_same_on_both_halves() {
    const MONITOR_PAYLOAD_RS: &str =
        include_str!("../../../src/bridge/src/backend/control/payload.rs");
    let needle = "pub const APIKEY_TABLE_AGENT: &str = \"";
    let at = guard_core::find_pinned(MONITOR_PAYLOAD_RS, needle).unwrap_or_else(|e| {
        panic!("在 monitor 侧 `payload.rs` 里钉不住 `APIKEY_TABLE_AGENT` 那一行：{e}")
    });
    let tail = &MONITOR_PAYLOAD_RS[at + needle.len()..];
    let theirs = &tail[..tail.find('"').expect("那个字面量没有收尾的引号")];
    assert!(!theirs.is_empty(), "抠出来的是空串 —— 抽取器坏了");
    let ours = crate::accounts::CREDENTIALS_FILE_AGENT;
    assert_eq!(
        theirs, ours,
        "monitor 认为凭据文件的行属于 `{theirs}`，后端把它们挂在 `{ours}` 名下"
    );
}

/// monitor 侧 `payload.rs` 的源码（跨半边对拍用；这条边已在 `cross_half_edge_registry` 登记，见上一条头注）。
const MONITOR_PAYLOAD_SRC: &str =
    include_str!("../../../src/bridge/src/backend/control/payload.rs");

/// ★★★ 🔴 **跨半边对拍**〔`设计/20 §7` 步 4 · 条 59〕：monitor 注入闸认为「登记了默认上游」的那几家
/// （`payload.rs::AGENTS_WITH_DEFAULT_UPSTREAM`）⇔ 后端 `accounts::AGENT_UPSTREAMS` 的 agent 列，**两向集合相等**。
///
/// 两侧**异源**：本侧是后端运行期那张表，那一侧是 monitor 源码里那一行的**字面量**（现抠，不是 `use`）。
/// 漂开的两个方向各有各的症状：
/// - monitor 多一家（比如有人把 codex 加进注入闸而后端没登记）⇒ 那一家注入 `/t/` ⇒ **每一发 502**；
/// - 后端多一家而 monitor 没跟 ⇒ 那一家永远不注入（「有它更好」白白丢了，不坏事，但与后端说的不一致）。
#[test]
fn the_agents_with_a_default_upstream_are_the_same_on_both_halves() {
    let needle = "pub const AGENTS_WITH_DEFAULT_UPSTREAM: &[&str] = &[";
    let at = guard_core::find_pinned(MONITOR_PAYLOAD_SRC, needle).unwrap_or_else(|e| {
        panic!("在 monitor 侧 `payload.rs` 里钉不住 `AGENTS_WITH_DEFAULT_UPSTREAM` 那一行：{e}")
    });
    let tail = &MONITOR_PAYLOAD_SRC[at + needle.len()..];
    let list = &tail[..tail.find(']').expect("那个数组没有收尾的 `]`")];
    let theirs: std::collections::BTreeSet<String> = list
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // 抽取器自检：抠出来的每一项都得像一个 agent 名（不是半截、不是空）。
    assert!(!theirs.is_empty(), "抠出来是空集 —— 抽取器坏了");
    for a in &theirs {
        assert!(
            segment_is_safe(a),
            "抠出来的 {a:?} 不像一个路由段 —— 抽取器坏了"
        );
    }
    let ours: std::collections::BTreeSet<String> = crate::accounts::AGENT_UPSTREAMS
        .iter()
        .map(|a| a.agent.to_string())
        .collect();
    assert_eq!(
        theirs, ours,
        "monitor 注入闸认为登记了默认上游的是 {theirs:?}，后端那张表登记的是 {ours:?}"
    );
}

/// ★★★ **跨半边对拍**：monitor 真的拼出来的 `/t/` 样例（`payload.rs::RELAY_PASSTHROUGH_SAMPLE`），
/// 本解析器读成**直通模式**、四段各落各位；再交给**生产段那张决策表**（`accounts::decide`）：
/// 那一家（登记过）⇒ 发到它自己的默认上游；同一条路由把第 1 段换成 `codex`（未登记，手写）⇒ 502。
///
/// ⇒ 「monitor 注入的那一形，后端真的会照直通处理」这一截从源码到决策表一路是真的；
/// 买不到的那一截（claude 拿到这个变量之后怎么走）见 `payload.rs` 那一段的头注（`C7`）。
#[test]
fn the_passthrough_sample_the_monitor_side_builds_parses_as_passthrough() {
    let key = "pub const RELAY_PASSTHROUGH_SAMPLE: &str = \"";
    let at = guard_core::find_pinned(MONITOR_PAYLOAD_SRC, key)
        .unwrap_or_else(|e| panic!("monitor 侧钉不住 `RELAY_PASSTHROUGH_SAMPLE`：{e}"));
    let rest = &MONITOR_PAYLOAD_SRC[at + key.len()..];
    let sample = &rest[..rest.find('"').expect("那个字面量没收尾")];
    assert!(
        sample.starts_with("/t/") && sample.len() > 10,
        "从 monitor 抠出来的样例不像直通路由键：{sample:?}"
    );
    let r = parse(&format!("{sample}/v1/messages")).expect("monitor 拼的 `/t/` 那一形解析不了");
    // 期望值全是手写字面量。
    assert_eq!(
        r.mode,
        super::super::Mode::Passthrough,
        "`/t/` 没被读成直通"
    );
    assert_eq!(r.key.seg1, "claude-code");
    assert_eq!(r.key.seg2, "acct-a");
    assert_eq!(r.stream, "k-0123456789abcdef");
    assert_eq!(r.rest, "/v1/messages");

    // 交给生产段那张决策表（空表 = 这个号在 apikey 表里没有行，即订阅号）。
    let table = crate::accounts::table::RoutingTable::build(std::iter::empty());
    let ups = crate::accounts::Upstreams::from_env(&|_| None).expect("内置默认");
    let said = |k: &super::super::RouteKey| {
        let mut out = String::new();
        crate::accounts::decide(&table, &ups, r.mode, k, &mut |d| {
            out = match d {
                super::super::Destination::Passthrough { upstream } => {
                    format!("pass {}", upstream.host)
                }
                super::super::Destination::Refuse { status, .. } => format!("refuse {status}"),
                super::super::Destination::Substitute { .. } => "substitute".to_string(),
            }
        });
        out
    };
    assert_eq!(
        said(&r.key),
        "pass api.anthropic.com",
        "登记过的那家没被直通到它自己的默认上游"
    );
    let codex = super::super::RouteKey {
        seg1: "codex".to_string(),
        seg2: r.key.seg2.clone(),
    };
    assert_eq!(
        said(&codex),
        "refuse 502 Bad Gateway",
        "🔴 codex 走 `/t/` 没被拒 ⇒ 它的请求会被发到别家的上游"
    );
}
