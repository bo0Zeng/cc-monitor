use super::*;
use serde_json::json;

/// 〔`KR113D2` 09-13〕**本模块每一条「转调 shell 脚本」的裁定，逐条写成数据。**
///
/// `(被调命令, 写面, 这一趟为什么不收进后端, 解锁条件)`
///
/// # 为什么有这张表
///
/// 定框 `K33` 逐字「**不要有什么 bash 脚本**」，而本模块今天**每一条**都在转调
/// cc-bus 的脚本。`K-R111` 摸底把这件事登记为「后端那一侧也还有活」，
/// `KR113D2` 逼出一次裁定：**收，还是不收，都要落成盘上看得见的东西**。
/// 本轮裁的是**乙（不收）** —— 理由逐条写在下面第三栏，**不是一句「以后再说」**。
///
/// 形状抄 `readonly_guard::spawn_registry::ALLOWED` 与 `cli_control::NOT_ON_CLI`
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
             再实现一遍 ⇒ 一处改、一处漏，而且错得静悄悄。用户 08-13 逐字「后面我可能要改ccbus」\
             ⇒ 今天焊进来的每一个字段，都是他改那天的一笔返工。",
        "cc-bus 那份文件格式**稳下来**，或者 cc-bus 本身被收成后端的一部分（那时它就不是\
             『外部插件』了，本表整张作废）。⚠ 解锁的判据不是『过了多久』，是**格式契约有没有定**。",
    ),
    (
        "cc-agents",
        "只读",
        "同 `cc-list`：它读的是 `spawned.tsv` ＋ 回头去 `agents.tsv` 借 pid 核身份。\
             **那次核身份正是不能重写的那一段** —— 08-13 的事故逐字记在它的头注里\
             （只按名字判活 ⇒ 会话名被重用 ⇒ 把占了同名的无辜进程当成 agent）。\
             强证据（登记时记下的 pane 根进程 pid）只在 cc-bus 那份名册的第 4 列里，\
             **命令面看不见它** ⇒ 后端重实现这一段只能退回弱证据，那是把已修的事故改回去。",
        "同上；另加一条**更便宜的中间路**：给 cc-bus 加一条机器可读的输出\
             （`cc_bus_boundary_guard` 的诊断逐字「要拿的东西命令给不出来时，正确做法是\
             **给 cc-bus 加一条命令**」）。那条路能一并治掉本模块 `parse_spawned` 那处\
             『目录名含空格就切不准』与『答不出 spawn 时间』。",
    ),
    (
        "cc-send",
        "写：收件人的收件箱（`inbox/<id>.jsonl`）",
        "投递这条路上住着**路由层**（ACL / 限流 / 去重 / 灭环）与 `flock`，\
             而它们是用户自己在改的东西（`CCBUS_RATE_*` / `CCBUS_DEDUP_WINDOW` / `CCBUS_TTL` …）。\
             后端重实现一份 = 两套路由规则同时在跑，而**被拦的那一条不会报错、只会不见**。",
        "同 `cc-list`。⚠ 若哪天要收，**先收读面再收写面** —— 写面出错是不可见的。",
    ),
    (
        "cc-kill",
        "破坏性：杀会话 ＋ 进程树，清名册 · 清台账 · 清那个 id 的状态",
        "它做的事**比后端自己那条 `kill` 多**（后者只杀 tmux 会话），而多出来的那几样\
             全是 cc-bus 的私有文件。更要紧的是它的**门**：`agents.tsv` 第 4 列那个 pid \
             ＋ 登记的完整地址一起核 —— 08-13 实测过不核的后果是杀掉无辜进程与会话。\
             ⇒ 门要住在懂那套语义的一侧，本模块不重写一遍。",
        "同 `cc-agents`：pid 那一列能从命令面拿到的那天。**这一条是四条里最后收的**\
             —— 破坏性动作的门重写错一次的代价，本仓已经付过。",
    ),
];

/// 从**生产段**现算：本模块今天经 [`crate::plugin::invoke`] 转调了哪几条 cc-bus 命令。
///
/// 取法与 `readonly_guard::g6_reach` 那一格**刻意相同**（`run("` / `run_as("` 之后
/// 那一个字符串字面量）—— 两处认的是同一件事实，取法不同才会各说各话。
fn transcalled_today() -> Vec<String> {
    let prod = crate::guard_support::production_code(include_str!("../../../src/backend/control/cc_bus.rs"));
    let mut out: Vec<String> = Vec::new();
    for opener in ["run(\"", "run_as(\""] {
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
/// **不判那条理由说得对不对**（`writing.md`：住址级判得了，语义判不了）。
/// 它买到的是「这一族里没有一条是**没人裁过**就躺在那儿的」。
#[test]
fn every_shelled_out_command_carries_a_written_ruling() {
    let today = transcalled_today();
    // 反空真地板：抽取塌了的话，下面两个差集都会是空的，本条会零命中地绿。
    assert!(
        today.len() >= 3,
        "生产段只抠到 {} 条转调（地板 3，实测 4）—— 抽取塌了，本条此刻在空转：{today:?}",
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
             收进后端的那一拍要**同轮**把它这一行删掉（不删就是一条幽灵，\n\
             下一个人会照着一份不成立的理由做判断）。"
    );
    for (cmd, face, why, unlock) in TRANSCALLS {
        assert!(
            !face.trim().is_empty(),
            "`{cmd}` 没写写面 —— 那一栏正是只读铁律那条豁免理由要引用的东西"
        );
        assert!(
            why.chars().count() >= 40,
            "`{cmd}` 的理由只有 {} 字 —— 那是占位不是理由",
            why.chars().count()
        );
        assert!(
            unlock.chars().count() >= 20,
            "`{cmd}` 没写解锁条件 —— 没有解锁条件的裁定会一直躺着，\
                 而『躺着』与『裁过』在盘上长得一模一样"
        );
    }
}

/// ★ `KR113D1`：`bus-state` **一次回全**，而且 `agents` 那一半与 `bus-list` **同源**。
///
/// 钉的是**数据流**，不是「函数存在」：两条命令必须落到同一个 `agents_via_cc_list`，
/// 否则 `bus-state.agents` 与 `bus-list.agents` 会各自漂。
/// ⚠ 起子进程那一步在沙箱里跑不了（没装 cc-bus）⇒ 本条扫的是生产段的**接线**，
/// 真跑由 `tests/e2e/daemon-cc-bus.sh` 那一族负责。
#[test]
fn bus_state_answers_both_halves_from_one_call() {
    let prod = crate::guard_support::production_code(include_str!("../../../src/backend/control/cc_bus.rs"));
    let body = prod
        .split("pub(crate) fn state_for_inbound")
        .nth(1)
        .expect("`state_for_inbound` 不在生产段里了 —— `bus-state` 的本体没了");
    let body = &body[..body.find("\n}").expect("函数体没有收尾 —— 抽取坏了")];
    for half in ["agents_via_cc_list()", "spawned_via_cc_agents()"] {
        assert!(
            body.contains(half),
            "`state_for_inbound` 里没有 `{half}` —— 「一次回全」少了一半。\n\
                 少的那一半会让调用方拿到一份**看上去完整**的答案（`spawned: []` 与\n\
                 「问不到」在它那儿长得一模一样）。"
        );
    }
    assert!(
        prod.contains("fn list_for_inbound() -> Result<serde_json::Value, (String, String)> {\n    Ok(serde_json::json!({ \"agents\": agents_via_cc_list()? }))"),
        "`bus-list` 不再走 `agents_via_cc_list` 了 —— 两条命令的 `agents` 从此会各漂各的"
    );
}

/// ★ `cc-agents` 那张表的三态，逐态一格；**认不出的行落选，不当成「已退」**。
#[test]
fn parse_spawned_reads_the_table_and_keeps_the_three_states() {
    let text = "ID                 状态   目录                                     初始任务\n\
                    proj_cc            活     /home/user/proj                           跑门禁\n\
                    ghost_cc           已退   /home/user/ghost                          收尾\n\
                    old_cc             活?    /home/user/old                            \n";
    let got = parse_spawned(text);
    assert_eq!(got.len(), 3, "表头没被跳过或数据行丢了：{got:?}");
    assert_eq!(got[0]["id"], "proj_cc");
    assert_eq!(got[0]["live"], json!(true));
    assert_eq!(got[0]["dir"], "/home/user/proj");
    assert_eq!(got[0]["task"], "跑门禁");
    assert_eq!(got[1]["live"], json!(false));
    // ★ 「核不了」必须是 `null`，不是 `false` —— 与 `bus-list` 的 `live` 同一套三态。
    //   把这一档并进「已退」正是 cc-bus 自己头注里记着的那族事故的共同起点。
    assert_eq!(got[2]["live"], serde_json::Value::Null);
    assert_eq!(got[2]["task"], "");
}

#[test]
fn non_data_lines_never_become_spawned_rows() {
    for line in [
        "(还没 spawn 过会话)",
        "ID 状态 目录 初始任务",
        "",
        "只有一列",
        "some_id 不是状态 /d 任务",
    ] {
        assert!(
            parse_spawned(line).is_empty(),
            "这行不该被当成 spawn 记录：{line:?}"
        );
    }
}

/// ★ 三态的**字面量**取自 cc-bus 的输出，不是我们自己发明的枚举 —— 三者互不相同。
///
/// ⚠ 钉「互不相同」而不是逐条对字符串：把 `活?` 并进 `活`，这一格才是它要拦的东西。
#[test]
fn the_three_spawned_states_stay_three_different_answers() {
    let live = spawned_live_of(SPAWNED_LIVE).expect("`活` 认不出来了");
    let unver = spawned_live_of(SPAWNED_UNVERIFIED).expect("`活?` 认不出来了");
    let exited = spawned_live_of(SPAWNED_EXITED).expect("`已退` 认不出来了");
    assert_ne!(live, unver, "「活着」与「核不了」必须分得开");
    assert_ne!(exited, unver, "「已退」与「核不了」必须分得开");
    assert_ne!(live, exited);
    assert!(
        spawned_live_of("活着").is_none(),
        "认不出的状态串必须落选（回 None），不许猜一个具体答案"
    );
}

#[test]
fn parse_list_reads_the_table_and_skips_everything_else() {
    let text = "ID           TMUX               待读\n\
                    agent-communication_cc agent-communication_cc:0.0 0\n\
                    x_cc         x_cc:0.0           2\n";
    let got = parse_list(text);
    assert_eq!(got.len(), 2, "表头没被跳过或数据行丢了：{got:?}");
    assert_eq!(got[0]["id"], "agent-communication_cc");
    assert_eq!(got[0]["unread"], 0);
    assert_eq!(got[1]["target"], "x_cc:0.0");
    assert_eq!(got[1]["unread"], 2);
}

/// ★ 超长 id 会把固定宽度的列**挤在一起** —— 实测那时列间仍有一个空格，
/// 所以按空白分列仍然对。这一格钉的就是「挤了也还认得出来」。
#[test]
fn a_long_id_that_overflows_the_column_is_still_parsed() {
    let one = parse_list("verylongagentname_that_overflows verylongagentname:0.0 7\n");
    assert_eq!(one.len(), 1);
    assert_eq!(one[0]["unread"], 7);
}

#[test]
fn non_data_lines_never_become_agents() {
    for line in [
        "(还没有登记的 agent)",
        "ID TMUX 待读",
        "",
        "只有两列 x",
        "id target 不是数字",
    ] {
        assert!(
            parse_list(line).is_empty(),
            "这行不该被当成 agent：{line:?}"
        );
    }
}

/// `P4f-Y4`：找不到时要说**查过哪儿**。
///
/// ⚠ 拼那句话的活搬去通用口了，**这一格没跟着搬**：它核的是 cc-bus 自己那三样
///（两处固定位置的形状 + [`NOT_INSTALLED_HINT`] 这句尾巴），而 `tests/e2e/daemon-cc-bus.sh` 的
/// 第 6 组逐字 `grep` 的正是这三条。搬走它等于把那三条 e2e 的单测对位丢掉。
#[test]
fn the_not_installed_message_names_the_places_it_looked() {
    let home = PathBuf::from("/home/u");
    let fixed = fixed_candidates(None, Some(&home), "cc-list");
    assert_eq!(fixed.len(), 2, "固定位置应当是两处：{fixed:?}");
    let msg = crate::plugin::discover::not_installed_message(
        "cc-list",
        &fixed,
        9,
        NOT_INSTALLED_HINT,
    );
    assert!(msg.contains("/home/u/.local/bin/cc-list"), "{msg}");
    assert!(
        msg.contains(".claude/skills/cc-bus/scripts/cc-list"),
        "{msg}"
    );
    assert!(msg.contains("9 个目录"), "PATH 那半没说：{msg}");
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
    assert!(classify_send(Some(0), "").is_ok());
    let bad = classify_send(Some(2), "x").unwrap_err().0;
    let rej = classify_send(Some(3), "x").unwrap_err().0;
    let other = classify_send(Some(9), "x").unwrap_err().0;
    let killed = classify_send(None, "x").unwrap_err().0;
    assert_ne!(bad, rej, "「收件人非法」与「被路由层拦」必须分得开");
    assert_ne!(bad, other);
    assert_ne!(rej, other);
    assert_eq!(killed, other, "被信号打断与其它失败同档（都是 failed）");
}

/// `P4f-Y5` 的另一半：daemon **不重复校验收件人合法性**。
///
/// 传一个 cc-bus 自己会拒的名字（含 `/`），本侧必须**放行到 cc-send 那一步**——
/// 由它去拒（rc=2 → `invalid_args`）。这样白名单只有一份。
#[test]
fn the_daemon_does_not_re_implement_the_recipient_charset_rule() {
    let ok = parse_send(&json!({ "to": "a/b", "text": "x" }));
    assert!(
        ok.is_ok(),
        "本侧不该判收件人字符集 —— 那会造出第二份规则：{ok:?}"
    );
    // 形状还是要判的（这不是安全边界，是「能不能构成一次有意义的调用」）
    for bad in [
        json!({}),
        json!({ "to": "x" }),
        json!({ "text": "x" }),
        json!({ "to": "", "text": "x" }),
        json!({ "to": "  ", "text": "x" }),
        json!({ "to": 1, "text": "x" }),
    ] {
        assert!(parse_send(&bad).is_err(), "形状不对却放行了：{bad:?}");
    }
}
