//! ⚠⚠ 🔴 **条 48之后，本文件里的字段名换了一遍，逐条对照**：
//!
//! | 先前 | 今天 | 它是什么 |
//! |---|---|---|
//! | `r.agent` | `r.key.seg1` | 路径第 1 段 —— 中转不解释它 |
//! | `r.account` | `r.key.seg2` | 路径第 2 段 —— 中转不解释它 |
//! | `r.key` | （退役） | 路径第 3 段 —— 先前是中转那条流的名字；今天流标签取自请求头，路径里没有这一段 |
//! | （没有） | `r.mode` | 哪个前缀进来的 |
//!
//! **断言的内容一条没改** —— 改的只是怎么称呼那几个槽位。下面散文里仍然出现
//! 「agent」「账号」这两个词：那是**在说槽位里装的是什么**（规格的语言），
//! 不是在说中转的类型里有这两个名字。〔条 61：`C1` 的人群不含注释。〕

use super::*;

#[test]
fn strips_the_prefix_and_keeps_the_rest_verbatim() {
    let r = parse("/s/agentA/acctA/v1/messages?beta=true").expect("应当解析成功");
    assert_eq!(r.mode, super::super::Mode::Substitute, "`/s/` 是代入模式");
    assert_eq!(r.key.seg1, "agentA");
    assert_eq!(r.key.seg2, "acctA");
    // ★ 期望值是**手写字面量**，不是拿被测函数算出来的（否则本断言自证、恒绿）。
    assert_eq!(r.rest, "/v1/messages?beta=true");
}

/// ★★★ 🔴 **两个前缀切出**同样的槽位（两段 ＋ 真路径），**只有模式不同**。
///
/// # 它钉的是哪一句
///
/// 「路由的线格式已经是对的，四个槽位不用动 —— 要动的只是中转怎么称呼
/// 它们」。⇒ 加 `/t/` **不许**顺手换段序、不许多剥一段、不许对某一段放宽白名单。
///
/// # ⚠ 分母：本条量的是**这两条**（`/s/` 与 `/t/`），不是「所有前缀」
///
/// 「别的前缀一律不认」由下面那条 `rejects_everything_that_is_not_the_shape` 的
/// `/x/…` 那一形兜。
#[test]
fn both_prefixes_cut_the_same_four_slots_and_differ_only_in_the_mode() {
    let s = parse("/s/agentA/acctA/v1/messages?beta=true").expect("`/s/` 应当解析成功");
    let t = parse("/t/agentA/acctA/v1/messages?beta=true").expect("`/t/` 应当解析成功");

    // ★ 模式那一格：期望值是**手写字面量**，两边不同。
    assert_eq!(s.mode, super::super::Mode::Substitute);
    assert_eq!(t.mode, super::super::Mode::Passthrough);
    assert_ne!(
        s.mode, t.mode,
        "两个前缀必须切出两种模式 —— 一样就等于只有一条路"
    );

    // ★ 槽位：**逐格相同**。
    assert_eq!(s.key, t.key, "前两段必须一模一样");
    assert_eq!(s.rest, t.rest, "真路径必须一模一样");
    // 手写字面量（不拿被测函数算）。
    assert_eq!(t.key.seg1, "agentA");
    assert_eq!(t.key.seg2, "acctA");
    assert_eq!(t.rest, "/v1/messages?beta=true");
}

#[test]
fn two_keys_do_not_collide() {
    let a = parse("/s/agentA/acctA/v1/messages").expect("A");
    let b = parse("/s/agentB/acctB/v1/messages").expect("B");
    assert_ne!(a.key.seg1, b.key.seg1);
    assert_ne!(a.key.seg2, b.key.seg2);
    assert_eq!(a.rest, b.rest, "两条路由的真路径相同，区别只在键");
}

/// ★★ **`K-H2`**：每段各装一件事 —— 同一个 agent，
/// **只有账号段不同**时，切出来的 `account` 必须不同，而 agent 段与真路径**一个字节不差**。
///
/// 它钉的是「账号身份**没有**被塞进别的段里」。
#[test]
fn the_account_segment_is_its_own_dimension() {
    let a = parse("/s/agentA/acct-one/v1/messages").expect("one");
    let b = parse("/s/agentA/acct-two/v1/messages").expect("two");
    assert_ne!(a.key.seg2, b.key.seg2, "账号段没被切出来");
    // 期望值是手写字面量。
    assert_eq!(a.key.seg2, "acct-one");
    assert_eq!(b.key.seg2, "acct-two");
    // agent 段与真路径**完全相同** —— 账号身份没有渗进它们。
    assert_eq!(a.key.seg1, b.key.seg1);
    assert_eq!(a.rest, b.rest);
    assert_eq!(a.rest, "/v1/messages");
}

#[test]
fn rejects_everything_that_is_not_the_shape() {
    // ★ **非空对照排最前**〔`D1` 一并修，08-28〕：先证明这把尺子认得**合法**的那一形，
    //   否则下面整个循环可能只是因为 `parse` 恒返回 `None` 而全绿。
    //   （先前它排在循环之后 —— 循环一红，它就一次都没被求值。）
    assert!(parse("/s/agentA/acctA/v1").is_some(), "这把尺子是瞎的");
    assert!(
        parse("/t/agentA/acctA/v1").is_some(),
        "这把尺子对 `/t/` 是瞎的 —— 下面那几条 `/t/` 的否定就成了空真"
    );

    // 分母 = 我列出的这 9 形；不是「所有不合法输入」。
    // 〔加了 `/t/` 少一段照样不认 ＋ **第三个前缀一律不认**；V141 路由剩两段。〕
    for bad in [
        "/v1/messages",
        "/s/agentA",
        "/s/agentA/",
        "/s/agentA/acctA",
        "/s//acctA/v1",
        "/s/agentA//v1",
        "/s/../../../etc/v1",
        "/t/agentA/acctA",
        "/t/agentA//v1",
        "/x/agentA/acctA/v1",
    ] {
        assert!(parse(bad).is_none(), "这一形不该被接受：{bad}");
    }
}

/// ★★★ **退役的会话段不会在本层被拒 —— 它被读成真路径的一截。**
///
/// 升级之前起的会话手里是 `/s|t/<agent>/<账号>/<会话段>` 那一形（env 起会话那一刻就定死了）。新解析器眼里
/// 它是 `seg1 · seg2 · rest=/<会话段>/v1/messages` —— 两段都过白名单 ⇒ **解析器拦不住它**，上游收到的
/// 真路径多了一截 ⇒ 上游自己 404。照「不为旧状态留兼容」不在这里认旧形；那几条会话要重起（报备主会话）。
#[test]
fn the_retired_session_segment_is_not_rejected_here_it_becomes_part_of_the_real_path() {
    let r = parse("/s/agentA/acctA/sid-AAA/v1/messages").expect("老形状在**本层**照样解析得了");
    // 期望值全是手写字面量。
    assert_eq!(r.key.seg1, "agentA");
    assert_eq!(r.key.seg2, "acctA");
    assert_eq!(r.rest, "/sid-AAA/v1/messages", "会话段成了真路径的一截");
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
    assert!(parse("/s/agentA/..%2f..%2fetc/v1").is_none());
    assert!(parse("/s/a.b/acctA/v1").is_none(), "点号不在白名单里");
    assert!(
        parse("/s/agentA/../v1").is_none(),
        "账号段也要过同一条白名单"
    );
}

/// ★ **同一条性质只许有一个实现**：装路由表时判「这个账号 id 当得了路由段吗」
/// 走的必须是本模块这个 [`segment_is_safe`]，不是另写一份。
///
/// 本条钉的是**那个谓词与 `parse` 的判断一致** —— 它俩要是漂开了，
/// 症状是「文件里配了一条账号，请求永远 404」，而两边各自看起来都没错。
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
        let parsed = parse(&format!("/s/agentA/{id}/v1")).is_some();
        assert_eq!(
            parsed, ok,
            "`segment_is_safe` 与 `parse` 对 {id:?} 的判断漂开了"
        );
    }

    // ⚠ **含 `/` 的 id 刻意不进上面那个循环**，如实说清为什么：
    //   它拼进 URL 之后**根本不是一段** —— `/s/agentA/has/slash/v1` 会被
    //   `parse` 读成 `account=has · rest=/slash/v1`，**解析成功**。
    //   ⇒ 「谓词说不行、`parse` 说行」在这一形上是**对的**，不是漂移：
    //     谓词回答的是「这个 id 当得了**一段**吗」，`parse` 回答的是「这条 URL 是那个形状吗」。
    //   真正兜住它的是**表里查不到 `has`** ⇒ 404。
    assert!(!segment_is_safe("has/slash"), "含 `/` 的 id 必须被谓词拒掉");
    assert!(
        parse("/s/agentA/has/slash/v1").is_some(),
        "这一形**确实**解析得了 —— 上面那段说明不是假设"
    );
}

/// ★★★ **上游选择拼给起会话那一发的 `/t/` 地址**（`accounts::upstream_select::endpoint::launch_relay_with`，
/// 路由语法住共享 crate `relay_route_core`）本解析器读成**直通模式**、各段各落各位；再交给**生产段那张决策表**
/// （`accounts::upstream_select::decide`）：那一家（登记过）⇒ 发到它自己的默认上游；同一条路由把第 1 段换成 `codex`（未登记，手写）⇒ 拒（404 ＋ 原因头，FIX3 之前是 502）。
///
/// ⇒ 「注入的那一形，中转真的会照直通处理」这一截从成品到决策表一路是真的。先前这里是三条跨半边对拍
/// （monitor `payload.rs` 的两份样例 · `APIKEY_TABLE_AGENT` · `AGENTS_WITH_DEFAULT_UPSTREAM`〔散文墓碑〕 现抠字面量），
/// 拼的那一侧搬进后端、语法进共享 crate 之后，两半之间没有第二份可对拍了（`cross_half_edge_registry` 那条边随之出列）。
/// 买不到的那一截（claude 拿到这个变量之后怎么走）同今天（`C7`）。
#[test]
fn the_passthrough_url_the_launch_answer_builds_parses_as_passthrough() {
    let answer = crate::accounts::upstream_select::endpoint::launch_relay_with(
        &serde_json::json!({"agent":"claude-code","account":{"kind":"named","configDir":"/h/.claude-alt/acct-a"},
            "allSessions":true}),
        &[],
        &|_| true,
    )
    .expect("成品");
    let url = answer.as_deref().expect("开关开、没行 ⇒ 该注入 `/t/`");
    let sample = url
        .strip_prefix(&format!("http://127.0.0.1:{}", relay_route_core::PORT))
        .expect("注入的不是回环那个口");
    assert!(sample.starts_with("/t/"), "不像直通路由键：{sample:?}");
    let r = parse(&format!("{sample}/v1/messages")).expect("monitor 拼的 `/t/` 那一形解析不了");
    // 期望值全是手写字面量。
    assert_eq!(
        r.mode,
        super::super::Mode::Passthrough,
        "`/t/` 没被读成直通"
    );
    assert_eq!(r.key.seg1, "claude-code");
    assert_eq!(r.key.seg2, "acct-a");
    assert_eq!(r.rest, "/v1/messages");

    // 交给生产段那张决策表（空表 = 这个号在 apikey 表里没有行，即订阅号）。
    let table = crate::accounts::upstream_select::table::RoutingTable::build(std::iter::empty());
    let ups = crate::accounts::upstream_select::Upstreams::from_env(&|_| None).expect("内置默认");
    let said = |k: &super::super::RouteKey| {
        let mut out = String::new();
        crate::accounts::upstream_select::decide(&table, &ups, r.mode, k, &mut |d| {
            out = match d {
                super::super::Destination::Passthrough { upstream } => {
                    format!("pass {}", upstream.host)
                }
                super::super::Destination::Refuse { status, reason, .. } => {
                    format!("refuse {status} {reason}")
                }
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
        "refuse 404 Not Found agent-not-registered",
        "🔴 codex 走 `/t/` 没被拒 ⇒ 它的请求会被发到别家的上游"
    );
}
