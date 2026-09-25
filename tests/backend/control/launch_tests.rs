//! # 要求住址：`INVARIANTS §42` → `src/doc/IPC-PROTOCOL.md §10` 的 `launch` 节（平面 ② 真建 tmux 会话的线上契约）
//!
//! 核原文：`launch` 节逐字「`send-into` / `send-keys-raw` 时不新建会话」·「`typed:true` 只有 `send-keys` 的退出码那么强」；
//! 同节写明它刻意不做的三件（不 attach · 不过 shell · 不顺手建会话）、`created` / `typed` 结局表、`send-keys-raw` 不附 `Enter` —— 本族逐格判的就是那一节。
//! 先过门再键入对 `INVARIANTS §34` Gate 2；精确目标形态对 `INVARIANTS §31a`。
//! ⚠ `§42` 的机检只核字段名落节、不核行为；契约里行为句不漂靠的是本族（射程待主会话确认，见 `JA1.md`）。〔JA1 点址 2026-09-24〕

use super::*;

fn args(v: serde_json::Value) -> serde_json::Value {
    v
}

#[test]
fn parses_a_well_formed_create_request() {
    let r = parse_request(&args(serde_json::json!({
        "mode": "create-or-attach",
        "name": "cc-1a2b3c4d",
        "payload": "claude --resume x",
        "cwd": "/home/u/p",
        "ccm_sid": "1a2b3c4d-0000-0000-0000-000000000000",
    })))
    .expect("应当解析成功");
    assert_eq!(r.mode, Mode::CreateOrAttach);
    assert_eq!(r.name, "cc-1a2b3c4d");
    assert_eq!(r.cwd.as_deref(), Some("/home/u/p"));
}

/// **没有 `attach-only`** —— attach 是平面 ③，backend 开不了你面前的窗。
#[test]
fn attach_is_not_a_mode_here() {
    let e = parse_request(&args(serde_json::json!({
        "mode": "attach-only", "name": "x", "payload": "y"
    })))
    .unwrap_err();
    assert_eq!(e.0, "invalid_args");
    assert!(e.1.contains("平面 ③"), "错误没说清楚为什么：{}", e.1);
}

#[test]
fn shape_validation_rejects_the_things_that_would_break_tmux() {
    let base = |name: &str, payload: &str| serde_json::json!({ "mode": "send-into", "name": name, "payload": payload });
    for (name, payload, why) in [
        ("", "p", "空名字"),
        ("n", "", "空载荷"),
        ("a:b", "p", "名字含 `:`（tmux 目标语法）"),
        ("a=b", "p", "名字含 `=`"),
        ("n", "a\nb", "载荷含控制字符（会多敲一次回车）"),
    ] {
        match parse_request(&base(name, payload)) {
            Ok(_) => panic!("{why} 居然通过了"),
            Err(e) => assert_eq!(e.0, "invalid_args", "{why}"),
        }
    }
    // 超长
    let long = "x".repeat(MAX_FIELD_BYTES + 1);
    assert_eq!(
        parse_request(&base("n", &long)).unwrap_err().0,
        "invalid_args"
    );
    // `ccm_sid` 会被拼进 tmux 格式串，收紧字符集
    let e = parse_request(&serde_json::json!({
        "mode":"create-or-attach","name":"n","payload":"p","ccm_sid":"a b"
    }))
    .unwrap_err();
    assert_eq!(e.0, "invalid_args");
}

/// ★〔`K-P2` `D3` 09-03〕`agent` / `width` / `height` 的形状校验。
///
/// # 为什么 `width`/`height` 要一条**「只给一半就拒」**
///
/// 只给 `width` 时 tmux 会拿默认值补 `height` ⇒ 会话**起得来**、尺寸**只对一半**。
/// 那是「写了个修饰、看起来生效了、其实只生效了一半」——本仓反复消灭的那个形状。
/// ⇒ 这里 fail-fast，而不是让它变成一个没人看得出来的怪尺寸。
#[test]
fn the_create_only_fields_have_their_own_shapes() {
    let ok = |extra: serde_json::Value| {
        let mut v = serde_json::json!({
            "mode":"create-or-attach","name":"n","payload":"p"
        });
        let obj = v.as_object_mut().expect("对象");
        for (k, x) in extra.as_object().expect("对象") {
            obj.insert(k.clone(), x.clone());
        }
        v
    };
    // 合法的一组：三个都给
    let r = parse_request(&ok(serde_json::json!({
        "agent":"claude","width":"220","height":"50"
    })))
    .expect("三个都合法时应当通过");
    assert_eq!(r.agent.as_deref(), Some("claude"));
    assert_eq!(
        (r.width.as_deref(), r.height.as_deref()),
        (Some("220"), Some("50"))
    );
    // 三个都不给也合法（`send-into` 那两条 mode 从来不带它们）
    let bare = parse_request(&ok(serde_json::json!({}))).expect("都不给也该通过");
    assert_eq!((bare.agent, bare.width, bare.height), (None, None, None));

    for (extra, why) in [
        (
            serde_json::json!({"agent":"a b"}),
            "agent 含空格（它进 tmux option 值）",
        ),
        (serde_json::json!({"agent":""}), "agent 为空"),
        (serde_json::json!({"agent":"a\nb"}), "agent 含控制字符"),
        (
            serde_json::json!({"width":"220"}),
            "★只给 width 不给 height",
        ),
        (
            serde_json::json!({"height":"50"}),
            "★只给 height 不给 width",
        ),
        (
            serde_json::json!({"width":"22a","height":"50"}),
            "width 不是纯数字",
        ),
        (serde_json::json!({"width":"","height":"50"}), "width 为空"),
        (
            serde_json::json!({"width":"12345","height":"50"}),
            "width 超过 4 位",
        ),
        (
            serde_json::json!({"width":"220","height":"-5"}),
            "height 带负号",
        ),
    ] {
        match parse_request(&ok(extra)) {
            Ok(_) => panic!("{why} 居然通过了"),
            Err(e) => assert_eq!(e.0, "invalid_args", "{why}"),
        }
    }
}

/// ★★〔`K-P2` `D3` 09-03〕**收得下 ≠ 起作用**：那三个新字段必须真的被 `run` 用掉。
///
/// # 它补的洞
///
/// [`parse_request`] 与 `launch_for_inbound` 的键名有一面镜子
/// （`inbound::structure_guards::launch_fields_match_its_parser_and_output`），
/// 而那面镜子**只看解析器与输出构造器** —— 一个字段完全可以「解析出来、存进结构体、
/// 然后一个地方都不用」。⇒ 症状是 **ccm 照发、backend 照收、`@ccm_agent` 与尺寸静默消失**，
/// 而两侧任何一条现有判据都不会红。**那正是本命令这一拍要防的那件事。**
///
/// ⚠ 这里只能判**源码形态**（真验要起 tmux，红线禁）。诚实边界写在这儿，别读大了。
#[test]
fn the_create_arm_actually_uses_the_three_new_fields() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    // ★ 抽取器自检：剥注释器没把代码也剥掉（建条当天生产段 300+ 行）。
    assert!(
        prod.lines().count() >= 200,
        "`control/launch.rs` 的生产段只剩 {} 行 —— 剥注释器把代码也剥了？下面几条会零命中地绿",
        prod.lines().count()
    );
    for (needle, what) in [
        ("req.agent", "`agent` 读点"),
        ("\"@ccm_agent\"", "`@ccm_agent` 这个 tmux option 名"),
        ("req.width", "`width` 读点"),
        ("req.height", "`height` 读点"),
        ("\"-x\"", "`new-session` 的 `-x`"),
        ("\"-y\"", "`new-session` 的 `-y`"),
    ] {
        assert!(
            prod.contains(needle),
            "生产段里找不到 {what}（needle {needle:?}）—— 字段**收得下却不起作用**：\n\
                 ccm 照发、backend 照收，而 `@ccm_agent` 与窗口尺寸静默消失。\n\
                 `launch_fields_match_its_parser_and_output` 那面镜子只看解析器与输出构造器，\n\
                 **它看不见这一格**（`K-P2` `D3` 立本条的全部理由）。"
        );
    }
}

/// ★ **不许照抄 monitor 的「禁双引号」** —— 那是 PowerShell 专属。
///
/// 这条路是 argv 直传，双引号只是一个普通字符。抄过来会让一大批合法载荷被拒，
/// 而且是以一个在这条路上根本不存在的理由。
#[test]
fn a_double_quote_in_the_payload_is_perfectly_fine_here() {
    let r = parse_request(&serde_json::json!({
        "mode": "send-into",
        "name": "cc-x",
        "payload": "claude --resume \"my session\"",
    }))
    .expect("双引号在 argv 直传的路上是合法字符");
    assert!(r.payload.contains('"'));
}

/// 切出 `run()` 里某一个 `Mode::X =>` 分支的源码。
///
/// ⚠ **收尾锚点必须是「下一个 `Mode::` 分支头」，不能写死某一个分支名。**
/// F04c 实测踩到：新分支 `Mode::SendKeysRaw` 插在 `SendInto` 与 `CreateOrAttach` 之间，
/// 而那两条判据的收尾锚点写死了 `Mode::CreateOrAttach =>` ⇒ 它们把**两个分支当成一个**
/// 扫，断言照样全绿（249 条一条没红）。**扫到了东西，但扫的不是那件事** —— 本仓这一族
/// 的又一次（`tmux_backend_gate_guard` 的硬编码文件表是同一个病）。
/// 「收尾行」与「枚举头」两个针 —— **运行时拼，源码里不留不配对的大括号**。
///
/// ⚠ `readonly_guard::no_test_code_leaks_into_any_production_section` 的剥法是**数括号**：
/// 字符串字面量里一个孤立的大括号会让 `mod tests` 那一层被提前配平收尾，
/// 于是测试属性「泄漏」进生产段。它本轮当场逮到我（`[("launch.rs", 2)]`），
/// 而它的报错文案逐字预言了这个形状。**别去改剥法，改措辞。**
fn tail_brace() -> String {
    // 码点 7d 就是右大括号。**用码点写、不写字面大括号** —— `\u{..}` 自带一对，
    // 于是这一行的括号收支为 0（详见上面 `tail_brace` 头注那段病史）。
    format!("\n{}\n", '\u{7d}')
}
fn enum_mode_head() -> String {
    format!("pub(crate) enum Mode {}", '\u{7b}')
}

fn arm_of<'a>(src: &'a str, head: &str) -> &'a str {
    let at = src
        .find(head)
        .unwrap_or_else(|| panic!("找不到分支 `{head}` —— 抽取坏了，断言会空转"));
    let rest = &src[at + head.len()..];
    let end = rest.find("Mode::").unwrap_or(rest.len());
    let arm = &src[at..at + head.len() + end];
    assert!(
        arm.len() > 150,
        "`{head}` 只切出 {} 字节 —— 抽取坏了",
        arm.len()
    );
    arm
}

/// `=name:` 的形状必须与 monitor 侧一致（F01：裸 `-t` 会打到兄弟会话上）。
#[test]
fn exact_target_shape_matches_the_monitor_side() {
    assert_eq!(exact_target("cc-abc"), "=cc-abc:");
    // 跨轨对拍：monitor `tmux.rs` 里那条 `format!("={target}:")`。
    const MONITOR_TMUX: &str = include_str!("../../../src/bridge/src/backend/control/tmux.rs");
    let prod = crate::guard_support::production_code(MONITOR_TMUX);
    // 运行时拼，避免命中本文件自己。
    let shape = format!("=%s{}", "target}:");
    let needle = shape.replace("%s", "{");
    assert!(
        prod.contains(&needle),
        "monitor 侧的精确匹配形状变了（找不到 `{needle}`）—— 两侧必须同形，\
             否则一边打到兄弟会话上而另一边不会，排查起来会非常难"
    );
}

/// ★ #76 防线的形态迁移：`send-into` **绝不新建会话**。
///
/// TS 侧那条防线（`launch-render-cli.ts` 让 `send-into` 强制走兜底）挡的是
/// 「用 create-or-attach 的语法去近似 send-into」；backend 直接调 tmux 之后那个
/// 表达力缺口没了，但**语义陷阱还在**：顺手新建就是 #76 的反向。
///
/// 这条扫的是 `run()` 的 `SendInto` 分支源码：它里面不许出现 `new-session`。
/// ★ **建会话必须是后台建**〔audit-0805 08-07〕。
///
/// `-d` 不是可有可无的旗标，它**就是**「后台建会话」这件事：没有它，
/// `tmux new-session` 会去 attach 当前终端，而后端这条路上根本没有终端。
///
/// # 为什么这条到今天才有
///
/// 定框 **E10** 逐字举证过「`-d` 去掉 …… SURVIVED `cargo test`，只有 shell e2e 抓住」。
/// 08-07 重跑那份举证：**B（`@ccm_sid` 改名）与 C（Gate 3 门限放松）今天仍然 SURVIVED**，
/// 而 A（去掉 `-d`）**会红** —— 但读诊断就知道那是**假信号**：红的是
/// `backend_kill` 的创建路径人群探测器（它的发现口径恰好含 `-d`），
/// 诊断说的是「那条路没了 ⇒ 删登记」，照做反而会把这条路移出人群。
///
/// ⚠ **订正上面 B 那一格**〔`K-P2` C 第五拍，09-03〕：本文件的创建臂**今天不再写
/// `@ccm_sid`** 了（写的是意图键 `@ccm_sid_expect`，见 [`run`] 里那段头注）
/// ⇒ 「`@ccm_sid` 改名」那个变异**在本文件里已经没有靶子**。
/// 而这一族今天真有人盯着，只是**不在本包**：monitor 侧
/// `ccm_cli_contract::the_intent_tag_and_the_fact_tag_are_not_merged_by_the_move`
/// 逐字数本文件生产段的 `(写点, 读点, 意图)` 三元组 ⇒ **门① 会红、门③ 仍不会**。
/// 别把「门③ 绿」读成「没人看着」，也别读成「有人看着」—— 它们分在两个包里。
///
/// ⇒ E10 说结构守卫**钉得住顺序与字面量**，那这一条就该有人写。本条补上。
/// 它不改变 E10 的结论（argv 的**语义**仍要 e2e），只是把能钉的那半钉住。
#[test]
fn the_create_argv_is_detached_and_names_the_session() {
    let src = include_str!("../../../src/backend/control/launch.rs");
    let prod = src
        .split(concat!("#[cfg", "(test)]"))
        .next()
        .expect("生产段");
    // 抽取器自检：切没了就零命中地绿。
    assert!(
        prod.contains("fn run"),
        "切出来的生产段里没有 `fn run` —— 切点变了，本条会零命中地绿"
    );
    assert!(
        prod.contains(r#"vec!["new-session", "-d", "-s""#),
        "创建分支的 argv 不再是 `[\"new-session\", \"-d\", \"-s\", …]`。\n\
             `-d` 一去掉，tmux 就会去 attach 当前终端 —— backend 这条路上没有终端，\n\
             会话建不起来或挂住，而**除了真二进制 e2e 没人会发现**（E10 举证过）。\n\
             顺序也钉在这里：`-s` 必须紧跟在 `-d` 之后、名字紧跟 `-s`。\n\
             真要改形态，先想清楚谁来接住它 —— 别指望现有 cargo test。"
    );
}

#[test]
fn send_into_never_creates_a_session() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    let arm = arm_of(&src, "Mode::SendInto =>");
    let verb = format!("new-{}", "session");
    assert!(
        !arm.contains(&verb),
        "`send-into` 分支里出现了建会话 —— 那是 #76 的反向：\n\
             用户以为在复用那个 idle 会话，实际被丢进一个新建的空 shell。\n\
             会话不存在时**报错**（`no_such_session`），别顺手建。"
    );
    // F03：存在性检查从 `has-session` 换成了 `gate::admit`（同一次探测顺带取回
    // `@ccm_sid` 与句柄）。**保证没变**：不存在仍回 `no_such_session`，
    // 由 `admit` 里那条 `let Some(p) = probe(target)? else` 兜着。
    assert!(
        arm.contains("gate::admit"),
        "`send-into` 分支不过 `gate::admit` —— 那就既没查会话在不在（`no_such_session`），\
             也没过 §34 的 Gate 2"
    );
}

/// ★★ **前提触发器：载荷内容至今**不是**安全边界**〔audit-0805 08-08，Phase G 第 95 件〕。
///
/// # 它守的是 `ROADMAP §5 4h` 那条登记
///
/// 08-08 先核出来的事实：`payload` 从 webview 一路进到这里，本后端只查
/// **三件形状**（非空 / 长度上限 / 无控制字符），**不查它是什么命令**。
/// 也就是说前端能送任意载荷 —— 于是 monitor 侧 `render_payload` 里那几道字符闸
///（`arg_is_join_safe` · `config_dir_command_safe` · launcher 那道）都是**纵深，不是边界**，
/// 而真边界是**这一条路的 `admit`**（会话身份 §34 Gate 2）与**前端执行面**（CSP/能力表）。
///
/// ⚠ 那条登记整个压在「本函数只做形状检查」上，而**没人盯着它**：
/// 谁哪天给 `check_field` 加一道 shell 安全校验，或把 `payload` 换成结构化的 argv，
/// **边界就搬家了** —— 那是好事，但 `§5 4h` 与 monitor 那几处「纵深不是边界」的注释
/// 会当天变成假话，而没有任何东西会红。
///
/// ⇒ 本条钉「今天仍然只有那三件形状检查」。它红的时候**不是坏消息**：
/// 诊断里直接写清「去把 4h 与那几处注释一起重判」。
///
/// ⚠ 本条是**源码层**（读自己这份源码的函数体）：挡得住「悄悄加/减一道检查」，
/// 挡不住「`MAX_FIELD_BYTES` 被调大」——那是量纲不是姿态，另有 `byte_cap_registry` 管。
#[test]
fn the_payload_is_still_only_shape_checked() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    let at = src
        .find("fn check_field(")
        .expect("`check_field` 不见了 —— 形状检查搬家了，`ROADMAP §5 4h` 要跟着重判");
    // 切到函数体收尾（花括号配平；两个字符字面量成对出现）。
    let bytes = src.as_bytes();
    let open = (at..bytes.len())
        .find(|&i| bytes[i] == b'{')
        .expect("找不到函数体起点");
    let mut depth = 0i32;
    let mut end = bytes.len();
    for i in open..bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                end = i + 1;
                break;
            }
        }
    }
    let body = &src[open..end];
    assert!(
        (5..40).contains(&body.lines().count()),
        "切出来的 `check_field` 有 {} 行，不像那个函数（配平切错了，本条会零命中地绿）",
        body.lines().count()
    );

    // 今天的三件形状检查，一件都不许少（少了 = 姿态变松，也要有人看见）。
    for (needle, what) in [
        ("is_empty()", "非空"),
        ("MAX_FIELD_BYTES", "长度上限"),
        ("is_control", "无控制字符"),
    ] {
        assert!(
            body.contains(needle),
            "`check_field` 里没有「{what}」那一件了（找 `{needle}`）。\n\
                 三件形状检查是 `ROADMAP §5 4h` 的**下界**：少一件，连「形状」都不成立了。"
        );
    }

    // ★ 正题：**不许出现内容/shell 语义的判定**。
    // ⚠ **两种写法都要认**〔08-08 变异证伪〕：第一版只找双引号形（`";"`），
    // 而真去加检查的人写的是 Rust 字符字面量（`';'`）—— 变异当场存活。
    // 「只认一种写法」是本工作区一直在治的病，这次犯在我自己的探针上。
    let dq = '"';
    let sq = '\'';
    let mut probes: Vec<String> = Vec::new();
    for ch in [';', '|', '&', '`', '$'] {
        probes.push(format!("{dq}{ch}{dq}"));
        probes.push(format!("{sq}{ch}{sq}"));
    }
    for name in ["sanitize", "shell_safe", "is_shell", "metachar"] {
        probes.push(name.to_string());
    }
    for probe in probes {
        assert!(
            !body.contains(probe.as_str()),
            "`check_field` 里出现了 `{probe}` —— 看起来它开始**查载荷的内容**了。\n\
                 ★ 这不是坏消息，是**边界搬家了**：`ROADMAP §5 4h` 逐字写着\n\
                 「载荷内容不是安全边界，monitor 侧那几道字符闸是纵深」，\n\
                 而那句话整个压在「本函数只做形状检查」上。\n\
                 ⇒ 请一起改：① `§5 4h` 那一行；② `payload.rs` 里 launcher 闸旁边\n\
                 与其判据头注中「纵深不是边界」那两处；③ 想清楚新边界的**姿态**\n\
                 （拒绝还是回落、错误文案给谁看）。别只加检查不改账。"
        );
    }
}

/// ★ **生产接线（顺序钉）**：`admit` 必须在 `type_payload` **之前**。
///
/// 这条与上面那条不是重复：上面钉「过不过门」，这条钉「门在不在路上」。
/// 反过来（先键入再核验）＝ 门形同虚设、而两条测试都会因为「函数被调用了」而绿。
/// 顺序错的形态在本仓出现过（`launch_wire` 的 env 顺序），是**最容易被 review 漏掉**的一类。
#[test]
fn the_send_into_arm_admits_before_it_types() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    let arm = arm_of(&src, "Mode::SendInto =>");
    let admit_at = arm.find("gate::admit").expect("分支里没有 `gate::admit`");
    let type_at = arm.find("type_payload").expect("分支里没有 `type_payload`");
    assert!(
        admit_at < type_at,
        "`type_payload` 排在 `gate::admit` 前面 —— 载荷先打出去了，门再判就没意义了"
    );
    // 键入的目标必须是 `admit` 回的**句柄**，不是名字/`t`。见 `gate` 模块头注的 TOCTOU 那段。
    assert!(
        arm.contains("type_payload(&handle"),
        "键入的目标不是 `admit` 回的句柄 —— 对名字下手就把 TOCTOU 窗口放回来了"
    );
}

/// ★ `create-or-attach` **刻意不过门**，且理由必须仍然成立：
/// 它只在 `new-session` 成功（＝会话是本次刚建的）时才键入；
/// 建失败但会话已存在时**早返回、根本不键入**。
///
/// 这条是「刻意无机检 + 理由」的反面 —— 理由可机检就机检：
/// 一旦有人让这个分支在「会话已存在」时也去 `type_payload`，本条就红。
#[test]
fn create_or_attach_never_types_into_a_session_it_did_not_just_create() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    // ⚠ F04c 改用 `arm_of`：原来是 `&src[at..]`（一直切到文件末尾）——
    // 那不是「这个分支」，是「这个分支之后的全部生产代码」。
    // 本轮新加的 `every_mode_variant_…` 当场点名了它（**它是最后一个分支，所以一直没出事**，
    // 但只要有人在它后面再加一个 mode，断言面就会静默串到别人身上）。
    let arm = arm_of(&src, "Mode::CreateOrAttach =>");
    let early = arm
        .find("created: false,")
        .expect("找不到「已存在 ⇒ 早返回」那一档");
    let types = arm.find("type_payload").expect("分支里没有 type_payload");
    assert!(
        early < types,
        "「会话已存在 ⇒ 早返回」不再排在键入之前 —— 那就会往一个**不是自己刚建的**\n\
             会话里键入，而这个分支没有 Gate 2（F03 刻意只给 send-into 装门，理由就是这条）。\n\
             要么把早返回放回去，要么这里也得过 `gate::admit`。"
    );
}

/// 结局映射：三种成功形态的 `created`/`typed` 组合必须互不相同（否则调用方分不出来）。
#[test]
fn the_three_success_shapes_are_distinguishable() {
    let created = LaunchOutcome {
        created: true,
        typed: true,
    };
    let idempotent = LaunchOutcome {
        created: false,
        typed: false,
    };
    let sent = LaunchOutcome {
        created: false,
        typed: true,
    };
    assert_ne!(created, idempotent);
    assert_ne!(created, sent);
    assert_ne!(idempotent, sent);
}

// ===== F04c：`send-keys-raw`（发裸键、不附 Enter）=====

/// 新 mode 名解析得出来，而且**旧的两个没被顺手改掉**。
#[test]
fn the_new_mode_name_parses_and_the_old_ones_still_do() {
    assert_eq!(Mode::parse("send-keys-raw"), Some(Mode::SendKeysRaw));
    assert_eq!(Mode::parse("send-into"), Some(Mode::SendInto));
    assert_eq!(Mode::parse("create-or-attach"), Some(Mode::CreateOrAttach));
    // ★ **fail-closed 的那一半**：未知 mode 必须回 `None` ⇒ `invalid_args`。
    // 这正是「为什么是新 mode 名而不是新字段」的全部理由 —— 旧后端会走到这里。
    for unknown in ["send-keys", "attach-only", "SendKeysRaw", "", "send-into "] {
        assert_eq!(Mode::parse(unknown), None, "{unknown:?} 不该被认出来");
    }
    let e = parse_request(&serde_json::json!({
        "mode": "send-keys", "name": "x-cc", "payload": "Escape"
    }))
    .expect_err("未知 mode 必须被拒");
    assert_eq!(e.0, "invalid_args");
    assert!(
        e.1.contains("send-keys-raw"),
        "错误文案没列出真正的 mode 集合，旧后端的使用者会不知道该升级什么：{}",
        e.1
    );
}

/// ★★ **本件的核心性质：裸键分支绝不附 `Enter`。**
///
/// 附上了就把「打断当前回合」（`Escape`）变成「**提交用户输入框里排队的文本**」。
/// 双向钉：裸键那条不许有 `Enter`，而 `send-into` 那条**必须**还有
/// （否则是把两个语义合并成一个 —— 那才是这一整件要拆开的东西）。
#[test]
fn send_keys_raw_never_appends_enter() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    let arm = arm_of(&src, "Mode::SendKeysRaw =>");
    assert!(
        arm.contains("type_keys_raw(&handle"),
        "裸键分支没走 `type_keys_raw`（或没对句柄下手）：{arm}"
    );
    // 抠出两个键入函数的函数体，逐个查 `Enter`。
    let key = "\"Enter\"";
    let raw_body = {
        let at = src
            .find("fn type_keys_raw(")
            .expect("找不到 `type_keys_raw` —— 改名了就把本条一起改");
        let rest = &src[at..];
        &rest[..rest
            .find(tail_brace().as_str())
            .map(|k| k + 3)
            .unwrap_or(rest.len())]
    };
    assert!(
        !raw_body.contains(key),
        "`type_keys_raw` 里出现了 {key} —— 那就不是裸键了。\n\
             生产上唯一会走这条路的是「优雅退出发 `Escape` 打断当前回合」，\n\
             多一个回车 = **提交用户输入框里排队的文本**（`tmux.rs` 头注逐字警告过）。"
    );
    let payload_body = {
        let at = src.find("fn type_payload(").expect("找不到 `type_payload`");
        let rest = &src[at..];
        &rest[..rest
            .find(tail_brace().as_str())
            .map(|k| k + 3)
            .unwrap_or(rest.len())]
    };
    assert!(
        payload_body.contains(key),
        "`type_payload` 不再附 {key} 了 —— 那两个 mode 的区别就消失了，\
             而这一整件存在的理由就是把它们**分开**"
    );
}

/// 裸键分支与 `send-into` **同一道门、同一个顺序**；且不许顺手建会话。
#[test]
fn the_send_keys_raw_arm_admits_before_it_types_and_never_creates() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    let arm = arm_of(&src, "Mode::SendKeysRaw =>");
    let admit_at = arm
        .find("gate::admit")
        .expect("裸键分支不过 `gate::admit` —— 发裸键也是往别人的会话里打字");
    let type_at = arm
        .find("type_keys_raw")
        .expect("分支里没有 `type_keys_raw`");
    assert!(
        admit_at < type_at,
        "键入排在过门之前 —— 门就没意义了（同 `the_send_into_arm_admits_before_it_types`）"
    );
    let verb = format!("new-{}", "session");
    assert!(!arm.contains(&verb), "裸键分支里出现了建会话");
    // ⚠ Gate 3 **不许**出现：`send-keys` 不删除任何东西。
    // monitor 侧 F04 Phase D 审计修过「给非破坏性动作加 Gate 3」那个错法。
    assert!(
        !arm.contains("admit_destructive"),
        "裸键分支走了带 Gate 3 的门 —— 那会让「往一个多窗口会话里打字」被误拒"
    );
}

/// ★★ **覆盖地板（本件补的通用防线）：每个 `Mode` 变体都必须有分支、有解析、被判据扫过。**
///
/// F04c 实测：新增一个变体时，既有那几条「扫某个分支源码」的判据**一条都不会红** ——
/// 它们只认自己写死的那个分支名。⇒ 加一条**枚举驱动**的判据：变体表变了就红，
/// 逼人回来给新变体配判据。（同族：`tmux_backend_gate_guard` 的硬编码文件表。）
#[test]
fn every_mode_variant_has_an_arm_and_a_parse_and_is_named_in_some_judge() {
    // 🔴 〔步 7c 剖分 2026-09-19 · `设计/16 §6.2` C 类〕**两个语料分开了。**
    //
    // 原来只有一份 `raw`（那份生产文件的**全文**，含 `#[cfg(test)]` 段），
    // 生产面与判据面都从它取 —— 那成立的前提是**判据与生产代码同住一份文件**。
    // 剖分之后判据整批住 `tests/backend/control/launch_tests.rs`，
    // 于是下面那句「整份文件里必须有人点名扫过这个分支」在生产文件里**恒假**
    //（现打红：「没有任何判据用 `arm_of` 扫过 `Mode::SendInto` 的分支」）。
    // ⇒ 生产面读生产文件，**判据面读本文件**。
    // ★ 这一格比原来强：原来判据面与生产面同源，往生产文件里贴一句
    //   `arm_of(&src, "Mode::X =>")` 的注释就能骗过它；现在骗不了。
    let raw = include_str!("../../../src/backend/control/launch.rs");
    let judges = include_str!("launch_tests.rs");
    let src = crate::guard_support::production_code(raw);
    // 从 Mode 枚举体里抽变体名。
    let at = src
        .find(enum_mode_head().as_str())
        .expect("找不到 Mode 枚举");
    let body = &src[at..at + src[at..].find(tail_brace().as_str()).expect("枚举没有收尾")];
    let variants: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|l| {
            l.ends_with(',')
                && !l.contains(' ')
                && l.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        })
        .map(|l| l.trim_end_matches(','))
        .collect();
    assert_eq!(
        variants.len(),
        3,
        "`Mode` 变体数变了（实得 {variants:?}）—— **这不是让你改数字**：\n\
             新增一个 mode 至少要补三样 —— `Mode::parse` 的一支、`run()` 的一个分支、\n\
             以及一条**扫那个分支源码**的判据（既有那几条只认自己写死的分支名，\n\
             新分支对它们是隐形的）。补完再把这个数棘上来。"
    );
    for v in &variants {
        assert!(
            src.contains(&format!("Mode::{v} =>")),
            "`Mode::{v}` 在 `run()` 里没有分支（或分支头写法不同）"
        );
        assert!(
            src.contains(&format!("Some(Mode::{v})")),
            "`Mode::{v}` 不在 `Mode::parse` 的映射里 —— 那它永远收不到请求"
        );
        // 判据面：**本文件**里必须有人点名扫过这个分支（剖分后判据都住这儿）。
        assert!(
            judges.contains(&format!("arm_of(&src, \"Mode::{v} =>\")")),
            "没有任何判据用 `arm_of` 扫过 `Mode::{v}` 的分支 —— \n\
                 那个分支可以被改成任何样子而一条判据都不红"
        );
    }
}

/// ★ **`typed` 只有 `send-keys` 退出码那么强**〔audit-0805 F10 下半，报告 I-3〕。
///
/// # 它治的不是「没做探测」，是**替证据说大话**
///
/// `type_payload` / `type_keys_raw` 的全部依据就是 `tmux(&["send-keys", …])?` ——
/// **退 0 就 `Ok`**。而 tmux 在 pane 处于 **copy-mode** 时照样退 0：键被键表吃掉，
/// 载荷根本没进应用（`pane_in_mode` / `copy-mode` / `-X cancel` 在**生产函数体**里零命中 ——
/// ⚠ 措辞 08-06 改准：原写「全仓零命中」，而这几个词今天在**本注释与下面那条判据的
/// needle 表**里都出现着，字面上早已不成立。下面那条判据扫的是抽出来的函数体，
/// 所以它自己不会自匹配 —— 但**说法要跟着判据的真实扫描面走**，否则就是 F23 那一族。）
///
/// 后果是链式的，且**每一环都在放大上一环的乐观**：
/// `send-keys` 退 0 → backend 回 `typed:true` → `backend_launch.rs` 逐字转发 →
/// 前端 `launch-cli-wire.ts:63` 逐字「`typed:false` 时 `reason` 必有值 ——
/// **那是回落到整串走终端的唯一线索**」⇒ `typed:true` **不回落** ⇒ 用户的载荷静默消失。
///
/// # 本条钉什么、不钉什么
///
/// **不钉**「探测做了没有」—— 那要真 tmux 才验得了（红线禁），
/// 而「加个看起来对的探测却没人能证明它管用」正是本区一直在批评的形状（功能件 §4）。
///
/// **钉的是**：那两个函数里**除了退出码之外没有第二种确认**。
/// 这句话今天成立，而契约与两处注释此前都在说「键入成功 / 真的键入了」——
/// 那是**替证据说大话**（定框 **E4**：静默失败要给身份；说大话是它的反面：
/// 把没有身份的成功说成确凿的成功）。
#[test]
fn typed_is_only_as_strong_as_the_send_keys_exit_code() {
    let src = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/launch.rs"
    ));
    for f in ["fn type_payload(", "fn type_keys_raw("] {
        let at = src
            .find(f)
            .unwrap_or_else(|| panic!("找不到 `{f}` —— 改名了就把本条一起改"));
        let rest = &src[at..];
        let body = &rest[..rest
            .find(tail_brace().as_str())
            .map(|k| k + 3)
            .unwrap_or(rest.len())];
        // 抽取器自检：抽出来的得像个函数体。
        assert!(
            (3..30).contains(&body.lines().count()),
            "从 `{f}` 抽出 {} 行，不像函数体（抽取器坏了）",
            body.lines().count()
        );
        // 若将来真加了确认（capture-pane 核对 / display-message 读模态 / -X cancel），
        // 本条会红 —— 那时**要连契约与那两处注释一起改回「真的键入了」**。
        for confirm in ["capture-pane", "display-message", "pane_in_mode", "-X"] {
            assert!(
                !body.contains(confirm),
                "`{f}` 里出现了 `{confirm}` —— 看起来加了第二种确认。\n\
                     ★ 那是**好事**，但契约与注释此刻还写着「只有退出码那么强」：\n\
                     `src/doc/IPC-PROTOCOL.md` 的 `typed` 那几行 · 本文件 `LaunchOutcome::typed` \n\
                     · monitor 侧 `backend_launch.rs::SendIntoResponse::typed`。**一起改。**"
            );
        }
    }
}

/// ★ **契约必须自己说清 `typed` 有多强** —— 删掉那段警示就等于把无条件断言放回来。
///
/// 上一条是**禁词**（不许说大话），本条是**正向要求**（必须说清边界）。
/// 两条缺一不可：只禁词的话，把那句话删了不写替代，契约就退回「什么都没说」，
/// 而消费方默认会按字面把 `typed:true` 读成确凿落地 —— 那正是报告 I-3 的起点。
#[test]
fn the_contract_says_how_strong_typed_actually_is() {
    let root = crate::guard_support::repo_root();
    let doc = std::fs::read_to_string(root.join("src/doc/IPC-PROTOCOL.md"))
        .expect("IPC-PROTOCOL.md 读不到");
    assert!(
        doc.len() > 10_000,
        "IPC-PROTOCOL.md 只读到 {} 字节 —— 抽取器坏了",
        doc.len()
    );
    for needle in [
        "只有 `send-keys` 的退出码那么强",
        "copy-mode",
        "typed_is_only_as_strong_as_the_send_keys_exit_code",
    ] {
        assert!(
            doc.contains(needle),
            "契约里找不到 `{needle}` —— `typed` 的语义边界那段被删了或改写了。\n\
                 ★ 删掉它，契约就退回「什么都没说」，而消费方默认按字面把 `typed:true` \n\
                 读成载荷确凿落地（`launch-cli-wire.ts:63`：那是回落的**唯一线索**）。\n\
                 要改措辞可以，但**三样都得留**：多强 · 已知反例 · 判据名。"
        );
    }
}

/// ★ 契约与两处注释**不许再声称载荷「真的键入了」**。
///
/// 依据只有退出码时，那句话是假的（见上一条）。要改回去，得先有第二种确认。
/// ⚠ needle **运行时拼**，否则本条会在自己的注释里找到它而恒红（F23 那一族的镜像）。
#[test]
fn no_doc_claims_the_payload_really_landed() {
    let root = crate::guard_support::repo_root();
    let overclaim = format!("{}键入了", "真的");
    let files = [
        "src/doc/IPC-PROTOCOL.md",
        "src/bridge/src/backend/control/backend_launch.rs",
    ];
    let mut total = 0usize;
    let mut hits = Vec::new();
    for f in files {
        let body = std::fs::read_to_string(root.join(f))
            .unwrap_or_else(|e| panic!("{f} 读不到：{e} —— 文件搬了就把本条一起改"));
        total += body.len();
        let n = body.matches(overclaim.as_str()).count();
        if n > 0 {
            hits.push(format!("  {f}：{n} 处"));
        }
    }
    assert!(
        total > 20_000,
        "两份文件只读到 {total} 字节 —— 抽取器坏了，本条此刻是空转的"
    );
    assert!(
        hits.is_empty(),
        "这些地方还在声称载荷「真的落进去了」，而依据只有 `send-keys` 的退出码：\n{}\n\n\
             ★ tmux 在 pane 处于 copy-mode 时**照样退 0**（键被键表吃掉）⇒ \n\
             `typed:true` ⇒ 前端不回落（`launch-cli-wire.ts:63` 逐字：那是回落的**唯一线索**）\n\
             ⇒ 用户的载荷静默消失。**这是报告 I-3 那条链。**\n\
             要说「真的键入了」，先给它第二种确认（探测模态 / 回读），\n\
             而那要真 tmux 才验得了 —— 登记在 `ROADMAP §5`，留给 e2e tier2。",
        hits.join("\n")
    );
}
