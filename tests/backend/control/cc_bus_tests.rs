use super::*;
use serde_json::json;

// 判据照线上那一份比：入参经 typed 那一处收（同分派）再交本体，回包写成线上那一份（盖住 `super::*` 里的同名口）。
fn take<R: serde::de::DeserializeOwned>(
    v: &serde_json::Value,
) -> Result<R, (&'static str, String)> {
    crate::wire::typed::take(v).map_err(|f| ("bad_args", f.message))
}
fn owned<T>(r: Result<T, (&'static str, String)>) -> Result<T, (String, String)> {
    r.map_err(|(c, m)| (c.to_string(), m))
}
fn val<T: serde::Serialize>(v: T) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}
fn parse_send(v: &serde_json::Value) -> Result<(String, String, Option<String>), CmdErr> {
    super::parse_send(&take(v)?)
}
fn parse_inbox(v: &serde_json::Value) -> Result<(String, u64), (String, String)> {
    super::parse_inbox(&owned(take(v))?)
}
fn agent(v: &serde_json::Value) -> BusAgent {
    BusAgent {
        id: v["id"].as_str().unwrap_or_default().to_string(),
        target: v["target"].as_str().unwrap_or_default().to_string(),
        unread: v["unread"].as_i64().unwrap_or_default(),
        live: v["live"].as_bool(),
        ccm_sid: v["ccm_sid"].as_str().map(str::to_string),
    }
}
fn inbox_reply(text: &str) -> Result<serde_json::Value, (String, String)> {
    super::inbox_reply(text).map(val)
}
fn list_reply(agents: Vec<serde_json::Value>) -> serde_json::Value {
    val(super::list_reply(agents.iter().map(agent).collect()))
}
fn send_reply(
    to: &str,
    registered: bool,
    live: serde_json::Value,
    from: Option<&str>,
) -> serde_json::Value {
    val(super::send_reply(to, registered, live.as_bool(), from))
}

/// **本模块每一条「转调 shell 脚本」的裁定，逐条写成数据。**
///
/// `(被调命令, 写面, 这一趟为什么不收进后端, 解锁条件)`
///
/// # 为什么有这张表
///
/// 定框 `K33` 逐字「**不要有什么 bash 脚本**」，而本模块今天**每一条**都在转调
/// cc-bus 的脚本。`K-R111` 摸底把这件事登记为「后端那一侧也还有活」，
/// `KR113D2` 逼出一次裁定：**收，还是不收，都要落成盘上看得见的东西**。
/// 裁的是**乙（不收）** —— 理由逐条写在下面第三栏，**不是一句「以后再说」**。
///
/// 形状抄 `readonly_guard::spawn_registry::ALLOWED` 与 `stream::cli::NOT_ON_CLI`
///（逐字：「把『为什么这条不上』写成**数据**，好让机检对着它比 —— 散文里说一遍，
/// 下一个人加命令时看不见」）。
///
/// # ⚠ 它与 `readonly_guard::g6_reach` 那一格**不是同一条规矩的两处住址**
///
/// 那一格钉的是「只读铁律的豁免理由今天覆盖了哪几条被调命令」（`ALLOWED` 的键
/// 分不出被调命令，那是它唯一的机器提醒）；本表钉的是「`K33` 对每一条各裁了什么」。
/// 两者今天的成员集相同，**而都不许手抄** —— 各自从生产段现算。
const TRANSCALLS: &[(&str, &str, &str, &str)] = &[
    (
        "cc-list",
        "只读",
        "cc-bus 的**名册与已读位置是它自己的私有格式**（`agents.tsv` ＋ `inbox/*.jsonl` \
             ＋ `state/*.pos`，`cc-list` 现打就是在做这个 join）。收进后端 = 把那份格式\
             再实现一遍 ⇒ 一处改、一处漏，而且错得静悄悄。「后面我可能要改ccbus」\
             ⇒ 今天焊进来的每一个字段，都是他改那天的一笔返工。",
        "cc-bus 那份文件格式**稳下来**，或者 cc-bus 本身被收成后端的一部分（那时它就不是\
             『外部插件』了，本表整张作废）。⚠ 解锁的判据不是『过了多久』，是**格式契约有没有定**。",
    ),
    (
        "cc-send",
        "写：收件人的收件箱（`inbox/<id>.jsonl`）",
        "投递这条路上住着**路由层**（ACL / 限流）与 `flock`，\
             而它们是用户自己在改的东西（`CCBUS_RATE_*` / `CCBUS_POLICY_MODE` …）。\
             后端重实现一份 = 两套路由规则同时在跑，而**被拦的那一条不会报错、只会不见**。",
        "同 `cc-list`。⚠ 若哪天要收，**先收读面再收写面** —— 写面出错是不可见的。",
    ),
    (
        "cc-log",
        "只读（共享锁，不推已读位置）",
        "驾驶舱读收件箱的尾巴：收件箱与它的锁是 cc-bus 的私有格式（`inbox/<id>.jsonl` ＋ `<inbox>.lock`），\
把命令当接口 ⇒ 后端转调它、不读那份文件；消费性读仍只走 `cc-peek` / `cc-commit`。",
        "同 `cc-list`：cc-bus 本身被收成后端的一部分的那天。",
    ),
];

/// 从**生产段**现算：本模块今天经 [`crate::plugin::invoke`] 转调了哪几条 cc-bus 命令。
///
/// 取法与 `readonly_guard::g6_reach` 那一格**刻意相同**（`run("` / `run_as("` 之后
/// 那一个字符串字面量）—— 两处认的是同一件事实，取法不同才会各说各话。
fn transcalled_today() -> Vec<String> {
    let prod = guard_core::production_code(include_str!("../../../src/backend/control/cc_bus.rs"));
    let mut out: Vec<String> = Vec::new();
    for opener in ["run(\"", "run_as(\"", "read_via(\""] {
        let mut from = 0usize;
        while let Some(k) = prod[from..].find(opener) {
            let at = from + k + opener.len();
            let end = prod[at..]
                .find('"')
                .expect("被调命令的字面量没有闭合 —— 抽取坏了");
            out.push(prod[at..at + end].to_string());
            from = at + end;
        }
    }
    out.sort();
    out.dedup();
    out
}

/// ★★ `KR113D2`〔09-13 裁**乙**：这一趟不收〕：**每一条转调，都要有一条写在盘上的裁定。**
///
/// # 它逮哪三形
///
/// ① **登记被删掉** ⇒ 那条转调没有裁定了 —— 下一个人只会看见一句「这里在调 shell」，
///    看不见「这是裁过的」，于是要么当成待办去动它，要么当成默认继续躺着。
/// ② **新加一条转调而不裁** ⇒ `K33` 那条定框上又多一笔债，而没有任何人被通知。
/// ③ **裁定过期**（表里那条命令今天已经不转调了）⇒ 幽灵条目
///    （`spawn_registry` 那张表里躺过一条 13 天的幽灵，形状逐字相同）。
///
/// # ⚠ 它买不到什么
///
/// **不判那条理由说得对不对**（住址级判得了，语义判不了）。
/// 它买到的是「这一族里没有一条是**没人裁过**就躺在那儿的」。
#[test]
fn every_shelled_out_command_carries_a_written_ruling() {
    let today = transcalled_today();
    // 反空真地板：抽取塌了的话，下面两个差集都会是空的，本条会零命中地绿。
    assert!(
        today.len() >= 3,
        "生产段只抠到 {} 条转调（地板 3，实测 3）—— 抽取塌了，本条此刻在空转：{today:?}",
        today.len()
    );
    let ruled: Vec<&str> = TRANSCALLS.iter().map(|(c, ..)| *c).collect();
    let unruled: Vec<&String> = today
        .iter()
        .filter(|c| !ruled.contains(&c.as_str()))
        .collect();
    assert!(
        unruled.is_empty(),
        "这几条转调 cc-bus 脚本的命令**没有一条写在盘上的裁定**：{unruled:?}\n\
             定框 `K33` 逐字「不要有什么 bash 脚本」⇒ 每一条要么收进后端，要么在 `TRANSCALLS` \n\
             里写明「写面 · 这一趟为什么不收 · 解锁条件」。写「以后再说」不算理由。"
    );
    let ghosts: Vec<&&str> = ruled
        .iter()
        .filter(|c| !today.contains(&c.to_string()))
        .collect();
    assert!(
        ghosts.is_empty(),
        "`TRANSCALLS` 里这几条**今天已经不转调了**：{ghosts:?} —— 裁定过期了，\n\
             收进后端时要**同时**把它这一行删掉（不删就是一条幽灵，\n\
             下一个人会照着一份不成立的理由做判断）。"
    );
    for (cmd, face, _, unlock) in TRANSCALLS {
        assert!(
            !face.trim().is_empty(),
            "`{cmd}` 没写写面 —— 那一栏正是只读铁律那条豁免理由要引用的东西"
        );
        assert!(
            unlock.chars().count() >= 20,
            "`{cmd}` 没写解锁条件 —— 没有解锁条件的裁定会一直躺着，\
                 而『躺着』与『裁过』在盘上长得一模一样"
        );
    }
}

/// **跨语言金样**：驾驶舱读收件箱的成品，两侧读同一份 `tests/__fixtures__/cc-bus-read.golden.json`。
/// 要求：「成品的两侧对拍 … 后端测试产出 == 金样 · TS 解码器读同一份」· 收件箱只看尾巴。
/// 异源：输入样例经**生产**解析器（`parse_inbox` / `inbox_reply`）现算，reply 是手写期望；码集合 == `inbound::REGISTRY`。
#[test]
fn the_cockpit_read_products_match_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/cc-bus-read.golden.json"))
            .expect("金样读不出来");
    let ib = &g["inbox"];
    assert_eq!(
        parse_inbox(&ib["request"]).expect("金样请求过不了生产解析器"),
        ("alpha_cc".to_string(), 200)
    );
    assert_eq!(
        parse_inbox(&ib["badRequest"])
            .expect_err("`--help` 当 id 必须在起进程之前拒")
            .0,
        "bad_id"
    );
    assert_eq!(
        inbox_reply(ib["log"].as_str().expect("缺 log")).expect("`cc-log` 样例解析失败"),
        ib["reply"],
        "`bus-inbox` 成品与金样不相等"
    );
    for (op, key) in [("bus-inbox", "inbox")] {
        let mut want: Vec<&str> = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == op)
            .unwrap_or_else(|| panic!("后端登记表里没有 `{op}`"))
            .codes
            .to_vec();
        want.sort_unstable();
        let mut got: Vec<&str> = g[key]["codes"]
            .as_array()
            .expect("缺 codes")
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        got.sort_unstable();
        assert_eq!(got, want, "金样 `{op}` 的拒绝码与后端登记的不相等");
    }
}

/// 老 cc-bus（不认 `--tsv` / 没有 `cc-log`）与半份输出都**明说**，不猜着解成一份空名单。
#[test]
fn an_old_cc_bus_or_a_half_read_is_said_not_read_as_empty() {
    let human = "ID           TMUX               待读\nx_cc         x_cc:0.0           2\n";
    for (what, got) in [
        ("名册", parse_roster_tsv(human).map(|_| ())),
        (
            "收件箱",
            inbox_reply("{\"from\":\"a\",\"text\":\"b\"}\n").map(|_| ()),
        ),
    ] {
        let e = got.expect_err(what);
        assert!(
            copy_core::copy_matches("beCcBus.read.tooOld", &e.1),
            "{what}：老 cc-bus 那一句没说清下一步：{e:?}"
        );
    }
    let half = parse_roster_tsv("#cc-list-tsv\t1\nalpha_cc\ta:0.0\tts\t1\t0\n")
        .expect_err("缺末行的名册必须回错");
    assert!(
        copy_core::copy_matches("beCcBus.read.truncated", &half.1),
        "{half:?}"
    );
    // 正控：首尾齐的空名单是「真的一个都没有」，不是错。
    assert_eq!(
        parse_roster_tsv("#cc-list-tsv\t1\n#skipped\t0\n").unwrap(),
        (Vec::new(), 0)
    );
}

/// `P4f-Y4`：找不到时要说**查过哪儿**。
///
/// ⚠ 拼那句话的活搬去通用口了，**这一格没跟着搬**：它核的是 cc-bus 自己那三样
///（两处固定位置的形状 + [`NOT_INSTALLED_HINT`] 这句尾巴），而 `tests/e2e/backend-cc-bus.sh` 的
/// 第 6 组逐字 `grep` 的正是这三条。搬走它等于把那三条 e2e 的单测对位丢掉。
#[test]
fn the_not_installed_message_names_the_places_it_looked() {
    let home = PathBuf::from("/home/u");
    let fixed = fixed_candidates(None, Some(&home), "cc-list");
    assert_eq!(fixed.len(), 2, "固定位置应当是两处：{fixed:?}");
    let msg =
        crate::plugin::discover::not_installed_message("cc-list", &fixed, 9, &NOT_INSTALLED_HINT);
    assert!(msg.contains("/home/u/.local/bin/cc-list"), "{msg}");
    assert!(
        msg.contains(".claude/skills/cc-bus/scripts/cc-list"),
        "{msg}"
    );
    assert!(
        copy_core::copy_matches_with(
            "beDiscover.notInstalledMessage.notFound",
            &[("pathDirs", "9")],
            &msg
        ),
        "PATH 那半没说：{msg}"
    );
    assert!(msg.contains("CC_BUS_BIN_DIR"), "没告诉人怎么指过去：{msg}");
}

#[test]
fn the_override_dir_wins_over_the_fixed_places() {
    let over = PathBuf::from("/opt/ccbus");
    let home = PathBuf::from("/home/u");
    let fixed = fixed_candidates(Some(&over), Some(&home), "cc-send");
    assert_eq!(fixed[0], PathBuf::from("/opt/ccbus/cc-send"));
    assert_eq!(fixed.len(), 3);
}

/// `P4f-Y5`：三档退出码各自映射成**不同**的语义码。
///
/// ⚠ 判据钉「互不相同」而不是逐条对字符串 —— 前者才是这条 DoD 的内容
/// （把两档并成一个码，用户就分不出「名字写错了」和「被 ACL 拦了」）。
#[test]
fn the_three_exit_codes_map_to_three_different_meanings() {
    assert!(classify_send(Some(0), "", 10).is_ok());
    let bad = classify_send(Some(2), "x", 10).unwrap_err().0;
    let rej = classify_send(Some(3), "x", 10).unwrap_err().0;
    let other = classify_send(Some(9), "x", 10).unwrap_err().0;
    let killed = classify_send(None, "x", 10).unwrap_err().0;
    assert_ne!(bad, rej, "「收件人非法」与「被路由层拦」必须分得开");
    assert_ne!(bad, other);
    assert_ne!(rej, other);
    assert_eq!(killed, other, "被信号打断与其它失败同档（都是 failed）");
}

/// 〔`INVARIANTS §47` ①〕**id 的形状在交给 `cc-send` / `cc-log` 之前先判**，
/// 规则是共享那一份（`shell_quote_core::bus_id_ok`，全仓唯一；monitor 读收件箱用的是同一个函数）。
///
/// 这一条替掉了 `P4f-Y5` 那一格原来的判据（「backend 不重复校验收件人合法性，放行到 cc-send 那一步由它拒」）：
/// `§47` 逐字「**『对端会校验』不是理由**」—— 界面那一道按删了之后，「本侧」就是真把 id 交出去的这一侧。
/// ⚠ 判的是**形状**不是**成员资格**：一个形状合法但没登记过的名字照样发（`registered:false` 如实回），不在这里拒。
///
/// 样本取跨语言金样 `cc-bus-control.golden.json` 的 `ids`（手写：`--help` 在盘上真出现过）：两个入口正反各一格。
#[test]
fn bus_ids_are_judged_here_before_they_reach_cc_bus() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/cc-bus-control.golden.json"
    ))
    .expect("金样读不出来");
    let take = |k: &str| -> Vec<String> {
        g["ids"][k]
            .as_array()
            .unwrap_or_else(|| panic!("金样缺 ids.{k}"))
            .iter()
            .map(|v| v.as_str().expect("id 不是字符串").to_string())
            .collect()
    };
    let (ok, bad) = (take("ok"), take("bad"));
    assert!(
        !ok.is_empty() && !bad.is_empty(),
        "金样的 ids 空了 —— 下面是空转"
    );
    for id in &ok {
        assert!(
            parse_send(&json!({ "to": id, "text": "x" })).is_ok(),
            "{id:?} 该发得出去"
        );
        assert_eq!(
            parse_inbox(&json!({ "id": id }))
                .map(|(id, _)| id)
                .as_deref(),
            Ok(id.as_str()),
            "{id:?} 的收件箱该读得到"
        );
    }
    for id in bad.iter().filter(|id| !id.trim().is_empty()) {
        let e = parse_send(&json!({ "to": id, "text": "x" })).expect_err(id);
        assert_eq!(e.0, "bad_id", "{id:?}：发消息的拒码不对（{e:?}）");
        assert!(
            e.1.contains(&format!("{id:?}")),
            "{id:?}：那一句没点出是哪个值：{}",
            e.1
        );
        let e = parse_inbox(&json!({ "id": id })).expect_err(id);
        assert_eq!(e.0, "bad_id", "{id:?}：读收件箱的拒码不对（{e:?}）");
    }
    // 空 / 纯空白是「缺」（`bad_args`），不是「形状不对」—— 两件事的话不一样。
    assert_eq!(
        parse_send(&json!({ "to": "  ", "text": "x" }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    assert_eq!(parse_inbox(&json!({ "id": "" })).unwrap_err().0, "bad_args");
    // 形状之外的缺格照旧判（「能不能构成一次有意义的调用」）。
    for bad in [
        json!({}),
        json!({ "to": "x" }),
        json!({ "text": "x" }),
        json!({ "to": "", "text": "x" }),
        json!({ "to": 1, "text": "x" }),
    ] {
        assert!(parse_send(&bad).is_err(), "形状不对却放行了：{bad:?}");
    }
    // 「规则只有一份、后端入口用的就是共享那一个」由 `tests/frontend/ui/judgment-single-home.vitest.ts` 的 J12 行钉（`rustNeedles`）。
}

/// ★★**跨语言金样**：界面直接收的 cc-bus 那几份成品，两侧读同一份 `tests/__fixtures__/cc-bus-control.golden.json`。
///
/// 守的要求：「**成品的两侧对拍**：界面按形状严格收……线上形状由一份跨语言金样钉住
/// （后端测试产出 == 金样 · TS 解码器读同一份）」。查在线 · 发消息两件由界面经通道直接说
/// （`src/frontend/ui/cc-bus-control.ts`），monitor 那一跳只搬字节。
///
/// 各格异源：请求样例过**生产**解析器（`parse_send`）·
/// 成品 == **生产**构造器（`bus-list` 由金样里那份 `cc-list --tsv` 输出样例经生产的 `parse_roster_tsv` ＋ `roster_agents`（挂身份空间）现算）·
/// 码集合 == `inbound::REGISTRY` 那一块。
#[test]
fn the_bus_products_match_the_cross_language_golden() {
    use serde_json::{json, Value};
    let g: Value = serde_json::from_str(include_str!(
        "../../__fixtures__/cc-bus-control.golden.json"
    ))
    .expect("金样读不出来");
    let codes_of = |op: &str| -> Vec<String> {
        let mut v: Vec<String> = crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == op)
            .unwrap_or_else(|| panic!("后端登记表里没有 `{op}`"))
            .codes
            .iter()
            .map(|c| c.to_string())
            .collect();
        v.sort();
        v
    };
    let golden_codes = |op: &str| -> Vec<String> {
        let mut v: Vec<String> = g[op]["codes"]
            .as_array()
            .unwrap_or_else(|| panic!("金样 `{op}` 缺 `codes`"))
            .iter()
            .map(|c| c.as_str().expect("码不是字符串").to_string())
            .collect();
        v.sort();
        v
    };
    for op in ["bus-list", "bus-send"] {
        assert_eq!(
            golden_codes(op),
            codes_of(op),
            "金样里 `{op}` 的拒绝码与后端登记的不相等 —— 界面那张「码 → 一句话」的表就会漏一档或多一档"
        );
    }
    // bus-list：名单由输入样例现算。
    let sessions: Vec<(String, String)> = g["input"]["sessions"]
        .as_array()
        .expect("输入样例缺 `sessions`")
        .iter()
        .map(|p| {
            (
                p[0].as_str().expect("会话名").to_string(),
                p[1].as_str().expect("@ccm_sid").to_string(),
            )
        })
        .collect();
    let agents: Vec<Value> = roster_agents(
        &parse_roster_tsv(
            g["input"]["ccListTsv"]
                .as_str()
                .expect("输入样例缺 `ccListTsv`"),
        )
        .expect("名册样例读得动")
        .0,
        Some(&sessions),
    )
    .into_iter()
    .map(val)
    .collect();
    assert_eq!(
        list_reply(agents),
        g["bus-list"]["reply"],
        "`bus-list` 成品与金样不相等"
    );
    // bus-send：请求过解析器；成品 == 构造器。
    let s = &g["bus-send"];
    let (to, _text, from) = parse_send(&s["request"]).expect("金样的发消息请求过不了生产解析器");
    assert_eq!(
        send_reply(&to, true, json!(true), from.as_deref()),
        s["reply"],
        "`bus-send` 成品与金样不相等"
    );
}

/// 〔`INVARIANTS §47`「交给对端之前本侧先判」〕`from`（作 `CC_BUS_ID` 交给 `cc-send`）过同一个 `bus_id_ok`，
/// 正反各一格（样本取跨语言金样的 `ids`）：判不过 ⇒ 整条 `bad_id`、那一句点出是哪个值（解析器在起进程之前）；
/// 空 / 纯空白的 `from` 仍是「没给」，不是「形状不对」。
#[test]
fn senders_are_judged_before_they_reach_cc_send() {
    let g: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/cc-bus-control.golden.json"
    ))
    .expect("金样读不出来");
    let take = |k: &str| -> Vec<String> {
        g["ids"][k]
            .as_array()
            .unwrap_or_else(|| panic!("金样缺 ids.{k}"))
            .iter()
            .map(|v| v.as_str().expect("id 不是字符串").to_string())
            .collect()
    };
    let (ok, bad) = (take("ok"), take("bad"));
    assert!(
        !ok.is_empty() && !bad.is_empty(),
        "金样的 ids 空了 —— 下面是空转"
    );
    for id in &ok {
        let (_, _, from) = parse_send(&json!({ "to": "alpha_cc", "text": "x", "from": id }))
            .unwrap_or_else(|e| panic!("{id:?} 当发件身份该放行：{e:?}"));
        assert_eq!(from.as_deref(), Some(id.as_str()));
    }
    for id in bad.iter().filter(|id| !id.trim().is_empty()) {
        let e = parse_send(&json!({ "to": "alpha_cc", "text": "x", "from": id })).expect_err(id);
        assert_eq!(e.0, "bad_id", "{id:?}：发消息的发件身份拒码不对（{e:?}）");
        assert!(
            e.1.contains(&format!("{id:?}")),
            "{id:?}：那一句没点出是哪个值：{}",
            e.1
        );
    }
    for blank in ["", "  "] {
        assert_eq!(
            parse_send(&json!({ "to": "alpha_cc", "text": "x", "from": blank }))
                .expect("空 from = 没给")
                .2,
            None
        );
    }
}

/// 超时那句说的秒数 ＝ 实际等了多久：命令总期限把一发截短到 1 s（cc-bus 自己的期限是 10 s）⇒ 话里是 1，不是 10。
/// 真起一个睡着的子进程走同一个原语（`plugin::invoke::run`），再经 cc-bus 这一侧的分档出那句话。
#[cfg(unix)]
#[test]
fn a_timeout_says_how_long_it_actually_waited() {
    use crate::platform::child::{Budget, Deadline};
    let _total = Budget::capped(Deadline::secs(1), None);
    let out = crate::plugin::invoke::run(std::path::Path::new("sleep"), &["30"], 10, &[], None)
        .unwrap_or_else(|_| panic!("起不来 sleep"));
    assert!(out.timed_out());
    assert_eq!(out.waited_secs, Some(1), "原语交回的不是截短后的那个时长");
    let (code, said) = classify_send(out.code, "", waited_secs(&out)).unwrap_err();
    assert_eq!(code, "child_timed_out");
    assert!(
        copy_core::copy_matches_with("beCcBus.timedOut.say", &[("secs", "1")], &said)
            && !said.contains("10"),
        "{said}"
    );
}

/// 总线的帧命令只剩收发三条：起停会话走 `session-open` · 血缘，广播删了（A13-1 减法）。
#[test]
fn the_bus_commands_are_list_send_and_inbox_only() {
    let mut got: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| n.starts_with("bus-"))
        .collect();
    got.sort_unstable();
    assert_eq!(got, ["bus-inbox", "bus-list", "bus-send"]);
}
