use super::*;
use crate::control::launch_render::payload::{render_tmux_outer, TmuxOuter, TmuxTarget};

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("夹具不是合法 JSON —— 重跑 npm run gen:payload-golden")
}

/// ★ 对拍的 TS 那一半必须还在**且还在做那件事** —— 只 `include_str!` 不读它，
/// 编译器会说 `never used`；而只查「文件在」也拦不住有人把断言掏空。
#[test]
fn the_typescript_half_still_asserts_the_fixture_is_current() {
    assert!(
        TS_HALF.contains("renderTmuxOuterFixture()"),
        "TS 那一半不再调用现场渲染 ⇒ 「夹具陈旧」这件事没人管了"
    );
    assert!(
        TS_HALF.contains("tmux-outer-golden.json"),
        "TS 那一半不再读入库夹具"
    );
}

/// ★ 计数自检：先证明「有东西可比」，再比。**相等，不是地板。**
#[test]
fn the_fixture_actually_has_cases_and_covers_all_three_cells() {
    let f = fixture();
    assert_eq!(
        f.cases.len(),
        EXPECT_CASES,
        "夹具用例数变了。夹具被清空/截断时，下面那条逐条对拍会零命中零失败地绿；\
         正常加用例请把 EXPECT_CASES 一起改（那正是它写成相等的理由）"
    );
    let mut modes: Vec<&str> = f.cases.iter().map(|c| c.mode.as_str()).collect();
    modes.sort_unstable();
    modes.dedup();
    let mut want = EXPECT_MODES.to_vec();
    want.sort_unstable();
    assert_eq!(
        modes, want,
        "三格没被盖全 —— 只数总条数挡不住「13 条全写成 create」"
    );
}

/// ★ 本模块的正题：同一组输入，Rust 渲染出来的必须与 TS 入库的**逐字节**相同。
///
/// ⚠ 这是 launch 这条路上**唯一**能看见「少一个 `&&`」「`-t` 少了 `=`/`:`」
/// 「`set-titles-string` 拼错一个字符」这一类差异的判据 ——
/// 那几样都会落进用户的 shell 去执行，一个字节的差异就是行为差异。
#[test]
fn rust_tmux_outer_rendering_matches_the_typescript_golden_byte_for_byte() {
    let f = fixture();
    let mut mismatches = Vec::new();
    for c in f.cases {
        let name = c.name.clone();
        let want = c.cmd.clone();
        // ★ 跑**生产命令本体**（`render_launch_payload`），不是自己重搭一个 spec。
        let got = match crate::control::launch_render::wire::render_launch_payload(c.req) {
            Ok(p) => p,
            Err(e) => format!("<Err: {e}>"),
        };
        if got != want {
            mismatches.push(format!(
                "  用例「{name}」\n    TS  : {want:?}\n    Rust: {got:?}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} 条外层命令两侧不一致：\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// ★ 夹具**真的在打那三格的形状**，不是一堆碰巧相等的空串。
///
/// 反空真：上面那条逐字节对拍在「两侧都产空串」时也是绿的。
#[test]
fn every_fixture_case_really_rendered_a_tmux_command() {
    for c in fixture().cases {
        assert!(
            c.cmd.starts_with("tmux "),
            "用例「{}」渲染出来的不是一条 tmux 命令：{:?}",
            c.name,
            c.cmd
        );
        match c.mode.as_str() {
            "create" => {
                assert!(
                    c.cmd.contains("tmux new-session -d -s ")
                        && c.cmd.contains(" && tmux send-keys -t ")
                        && c.cmd.contains(" && tmux attach -t "),
                    "create 那一格的形状不对：{:?}",
                    c.cmd
                );
                // `C14`：起会话就是起会话，不要 `or` —— 建失败必须整条非零退出。
                assert!(
                    !c.cmd.contains("new-session -d -s") || !c.cmd.contains("2>/dev/null &&"),
                    "create 那一格又把 `new-session` 的错吞了（#76 那一形）：{:?}",
                    c.cmd
                );
                assert!(
                    !c.cmd.contains("; tmux attach"),
                    "尾部 attach 又变成无条件的了（`;` 而不是 `&&`）：{:?}",
                    c.cmd
                );
            }
            "send-into" => {
                assert!(
                    !c.cmd.contains("new-session"),
                    "send-into 那一格不许有 new-session（会话确实在，建它就产孤儿）：{:?}",
                    c.cmd
                );
                assert!(
                    c.cmd.contains("tmux send-keys -t ") && c.cmd.contains("; tmux attach -t "),
                    "send-into 那一格的形状不对：{:?}",
                    c.cmd
                );
            }
            "attach" => {
                assert!(
                    c.cmd.starts_with("tmux attach -t ") && !c.cmd.contains("send-keys"),
                    "attach 那一格不许带载荷：{:?}",
                    c.cmd
                );
            }
            other => panic!("用例「{}」的 mode 是 {other:?}，不在三格里", c.name),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// 语义那一半 —— **金标准结构上盖不到的那几维**
// ═════════════════════════════════════════════════════════════════════════════

/// ★ Rust 侧**拒**掉 TS 座会照拼的那几类坏输入。
///
/// 〔LR2〕那个 TS 座（`session-backend.ts`）已删；本条留着，因为它钉的是 **Rust 这一侧自己把门**，
/// 与 TS 在不在无关（函数名里的「TS 座」是立它时的对照物，不改名 —— 别处散文按名点它）。
/// 座当年的头注逐字「座只在这些**已安全**的片段外拼后端语法，不做校验/转义」——
/// 它收的是调用方 quote 好的片段，安全靠一句调用约定。本侧收生料 ⇒ 自己把门。
///
/// ⚠ **这一维进不了金标准**：左边（TS）产得出、右边（Rust）拒，
/// 「两侧同一串」的夹具里根本写不下这种样本 —— 同 `payload.rs` 头注记的那条
/// 「TS 生成夹具这个机制抓不到安全姿态差异」。
#[test]
fn the_rust_side_refuses_what_the_typescript_seat_would_have_concatenated() {
    let bad: &[(&str, TmuxOuter, Option<&str>)] = &[
        (
            "raw 名字里有分号（裸拼 ⇒ 另起一条命令）",
            TmuxOuter::Attach {
                target: TmuxTarget::Raw("cc-1; rm -rf /"),
            },
            None,
        ),
        (
            "空会话名",
            TmuxOuter::Attach {
                target: TmuxTarget::Raw(""),
            },
            None,
        ),
        (
            "quoted 名字里有换行（把一条命令劈成两条）",
            TmuxOuter::Attach {
                target: TmuxTarget::Quoted("cc\n1"),
            },
            None,
        ),
        (
            "@ccm_sid 越界（它是裸拼的）",
            TmuxOuter::Create {
                target: TmuxTarget::Raw("cc-1"),
                cwd: None,
                ccm_sid: Some("a b"),
            },
            Some("claude"),
        ),
        (
            "cwd 是空串（空值 ≠ 未设）",
            TmuxOuter::Create {
                target: TmuxTarget::Raw("cc-1"),
                cwd: Some(""),
                ccm_sid: None,
            },
            Some("claude"),
        ),
        (
            "create 少送了载荷（会渲染出只建空会话再接进去的命令）",
            TmuxOuter::Create {
                target: TmuxTarget::Raw("cc-1"),
                cwd: None,
                ccm_sid: None,
            },
            None,
        ),
        (
            "attach 多送了载荷（格搞错了，不静默丢）",
            TmuxOuter::Attach {
                target: TmuxTarget::Raw("cc-1"),
            },
            Some("claude"),
        ),
    ];
    for (why, outer, payload) in bad {
        match render_tmux_outer(outer, *payload) {
            Ok(cmd) => panic!("{why} —— 本该拒，却渲染出了：{cmd:?}"),
            // 业务拒绝必须带标：TS 侧靠它区分「坏输入（不许回落）」与「IPC 异常（可回落）」。
            Err(e) => assert!(
                e.starts_with("REFUSE:"),
                "{why} —— 拒了但没打 REFUSE 标：{e:?}"
            ),
        }
    }
}

/// ★ **正控**：上面那条全是「该红的红了」，它在渲染器整个坏掉时也绿。
/// 这条证明同一批目标形态里，合法的那些真的产得出来。
#[test]
fn the_same_shapes_render_fine_when_the_inputs_are_legal() {
    assert_eq!(
        render_tmux_outer(
            &TmuxOuter::Attach {
                target: TmuxTarget::Raw("cc-1")
            },
            None
        )
        .expect("合法 attach 渲不出来"),
        "tmux attach -t =cc-1:"
    );
    assert_eq!(
        render_tmux_outer(
            &TmuxOuter::SendInto {
                target: TmuxTarget::Quoted("开新 Claude")
            },
            Some("claude")
        )
        .expect("合法 send-into 渲不出来"),
        "tmux send-keys -t '=开新 Claude:' 'claude' Enter; tmux attach -t '=开新 Claude:'"
    );
}

/// ★ **两层的 cwd 不许同时送** —— tmux 那两格的内层没有 `cd`。
///
/// 没有这一条，一个送错的顶层 `cwd` 会静默多渲染出一段 `cd '…' && `，
/// 而那段在 `send-keys` 的载荷里 —— 它会被键进会话执行，用户看得见后果、
/// 而对拍夹具里没有这一形（夹具里的 req 由生产构造口产，顶层 cwd 恒 null）。
#[test]
fn the_two_layers_of_cwd_cannot_both_be_sent() {
    let f = fixture();
    // 先证明夹具里确实一条都没有（否则下面那条"拒"就无从谈起）。
    assert!(
        f.cases.iter().all(|c| c.req.cwd.is_none()),
        "夹具里出现了顶层 cwd 非空的用例 —— 生产构造口的契约变了"
    );
    let mut req: crate::control::launch_render::wire::PayloadRenderRequest = serde_json::from_str(
        r#"{"env":[],"cwd":"/w","launcher":"claude","args":[],"nestedEnv":["X"],"wrap":[],
                "outer":{"mode":"send-into","name":"cc-1","quoting":"raw"}}"#,
    )
    .expect("手搭的 req 解析不了");
    let got = crate::control::launch_render::wire::render_launch_payload(req);
    assert!(
        got.is_err() && got.as_ref().unwrap_err().starts_with("REFUSE:"),
        "两层 cwd 同时送没有被拒：{got:?}"
    );
    // 反向：把顶层 cwd 去掉，同一条请求必须渲得出来（证明上面那条不是因为别的原因红的）。
    req = serde_json::from_str(
        r#"{"env":[],"cwd":null,"launcher":"claude","args":[],"nestedEnv":["X"],"wrap":[],
            "outer":{"mode":"send-into","name":"cc-1","quoting":"raw"}}"#,
    )
    .expect("手搭的 req 解析不了");
    assert!(
        crate::control::launch_render::wire::render_launch_payload(req).is_ok(),
        "去掉顶层 cwd 之后仍然渲不出来 —— 上面那条红的不是它该报的那件事"
    );
}

/// ★ **线上那一格自己的闸**：`attach` 带载荷字段必须被**这条 IPC** 拒。
///
/// 🔴 **这条是死值验补出来的**：`M6`（把 `render_launch_payload` 里那道
/// 「`env`/`args`/`launcher` 必须为空」整个关掉）**一刀下去 1465 条全绿** ——
/// 因为上面那条 `the_rust_side_refuses_…` 喂的是渲染器本体
/// （[`render_tmux_outer`] 自己也有一道同义的闸），**根本走不到 wire 这一层**。
/// ⇒ 两层各有一道闸，而判据只盖了里面那道；外面那道**没有判据就等于不存在**。
///
/// ⚠ 两道闸不是重复：渲染器那道挡的是「crate 内的调用方传错」，
/// 这一道挡的是「**webview 送来一个格搞错的请求**」—— 后者的上游不受本仓控制。
#[test]
fn the_wire_refuses_an_attach_request_that_still_carries_a_payload() {
    const CLEAN: &str = r#"{"env":[],"cwd":null,"launcher":"","args":[],"nestedEnv":["X"],"wrap":[],
            "outer":{"mode":"attach","name":"cc-1","quoting":"raw"}}"#;
    let parse = |json: &str| {
        serde_json::from_str::<crate::control::launch_render::wire::PayloadRenderRequest>(json)
            .expect("手搭的 req 解析不了")
    };
    let dirty = [
        (
            "env",
            CLEAN.replace(r#""env":[]"#, r#""env":[{"kind":"unset-config-dir"}]"#),
        ),
        (
            "args",
            CLEAN.replace(r#""args":[]"#, r#""args":["--resume"]"#),
        ),
        (
            "launcher",
            CLEAN.replace(r#""launcher":"""#, r#""launcher":"claude""#),
        ),
    ];
    for (field, json) in &dirty {
        // 抽取器自检：替换真的发生了（没替上的话下面那条会拿干净请求去测「该拒」）。
        assert_ne!(
            json.as_str(),
            CLEAN,
            "`{field}` 那一处没替上 —— 这一轮在空转"
        );
        match crate::control::launch_render::wire::render_launch_payload(parse(json)) {
            Ok(cmd) => panic!(
                "attach 请求带着 `{field}` 却渲出来了：{cmd:?}\n\
                 —— 那几个字段被静默丢掉了，而调用方以为它起了一个会话。"
            ),
            Err(e) => assert!(e.starts_with("REFUSE:"), "拒了但没打 REFUSE 标：{e:?}"),
        }
    }
    // ★ 正控：同一条请求**不带**那几个字段时必须渲得出来
    //   （否则上面那三次「拒了」在整条路坏掉时也成立）。
    assert_eq!(
        crate::control::launch_render::wire::render_launch_payload(parse(CLEAN))
            .expect("干净的 attach 请求渲不出来"),
        "tmux attach -t =cc-1:"
    );
}

/// ★ **`outer` 缺席 = 老形态，字节一个都没变**。
///
/// `#[serde(default)]` 是承重的：入库夹具 `payload-golden.json` 的 10 条用例一个字都没改。
/// 这条从**同一个命令**走一趟没有 `outer` 的请求，钉住那条兼容不是靠「大概没事」。
#[test]
fn a_request_without_outer_still_renders_the_plain_payload() {
    let req: crate::control::launch_render::wire::PayloadRenderRequest = serde_json::from_str(
        r#"{"env":[{"kind":"unset-config-dir"}],"cwd":"/w","launcher":"claude","args":[],
            "nestedEnv":["X"],"wrap":[]}"#,
    )
    .expect("没有 outer 的请求解析不了 —— serde(default) 掉了");
    assert_eq!(
        crate::control::launch_render::wire::render_launch_payload(req).expect("老形态渲不出来"),
        "unset CLAUDE_CONFIG_DIR; cd '/w' && claude"
    );
}

/// ★★ 〔LR2〕**把生产命令 `render_launch_payload` 的真输出交给 e2e**（数据出口，不是判据）。
///
/// `resume-suite` · `resume-backend-frames` · `tmux-target-acceptance` 三套 e2e 要证的是
/// 「app 真正会跑的那一串在真 tmux 上干了什么」。步 22b·B 之后那一串由 Rust 渲染，
/// 而三套 e2e 此前 import 的是 TS 那五个 builder —— 从那天起它们验的是一份不在执行链上的副本。
/// ⇒ 请求由 e2e 那一侧用**生产 TS 的** `plan*` ＋ `buildLaunchRenderRequest` 产
/// （`tests/e2e/launch-render-driver.ts`），经环境变量 `CCM_E2E_RENDER_REQ` 递进来，
/// 这里拿**生产 wire 类型**反序列化、跑**生产命令本体**，原样吐出去。
///
/// 输出：渲得出 ⇒ `LAUNCH_RENDER<<<命令>>>`；被拒 ⇒ `LAUNCH_RENDER_ERR<<<理由>>>`
/// （e2e 有「该拒」的用例，拒绝本身就是读数，所以不 panic）。
/// **请求缺失 / 解析不了 ⇒ panic**：那是 e2e 那一侧坏了，吐空串会被读成「渲出了空命令」。
///
/// 跑法：`cargo test --lib emit_launch_render_for_e2e -- --ignored --nocapture`（`src/backend` 下）。
#[test]
#[ignore]
fn emit_launch_render_for_e2e() {
    let raw = std::env::var("CCM_E2E_RENDER_REQ")
        .expect("缺 CCM_E2E_RENDER_REQ —— 本出口只给 tests/e2e/launch-render-driver.ts 用");
    let req: crate::control::launch_render::wire::PayloadRenderRequest =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("请求解析不了（{e}）：{raw}"));
    match crate::control::launch_render::wire::render_launch_payload(req) {
        Ok(cmd) => {
            assert!(
                !cmd.contains('\n'),
                "渲出来的命令带换行，标记行会被截断：{cmd:?}"
            );
            println!("LAUNCH_RENDER<<<{cmd}>>>");
        }
        Err(e) => println!("LAUNCH_RENDER_ERR<<<{}>>>", e.replace('\n', " ")),
    }
}
