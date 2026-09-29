use super::*;
use serde_json::Value;

fn spec(sid: &str, candidates: Vec<Option<&str>>) -> ResumeSpec {
    ResumeSpec {
        session_id: sid.to_string(),
        launch_candidates: candidates
            .into_iter()
            .map(|o| o.map(str::to_string))
            .collect(),
        claude_dir: String::new(),
        fallback_cwd: String::new(),
        already_in_tmux: false,
        agent_kind: String::new(),
    }
}

/// 契约：ResumeSpec → CommandPlan 的 wire 形状 + camelCase + aterm 4 caps 名 + substitutedFrom。
/// DG3（#2D）：ResumeSpec 的 `agent_kind` 从 **camelCase** wire `agentKind` 反序列化（resolve I/O 面
/// 全 camelCase）；缺省 → `""`（= claude 兼容）。DG6 据此构 codex resume vs claude --resume。
#[test]
fn resume_spec_parses_camelcase_agent_kind_default_claude() {
    let spec: ResumeSpec =
        serde_json::from_str(r#"{"sessionId":"s","agentKind":"codex"}"#).expect("parse");
    assert_eq!(spec.agent_kind, "codex");
    // 缺 agentKind → default ""（缺=claude，向后兼容旧 monitor 不发此字段）。
    let spec2: ResumeSpec = serde_json::from_str(r#"{"sessionId":"s"}"#).expect("parse");
    assert_eq!(spec2.agent_kind, "");
}

#[test]
fn resolve_builds_plan_with_aterm_field_names() {
    let s = spec(
        "abcd1234-5678-90ab-cdef-1234567890ab",
        vec![None, Some("cct"), Some("cc")],
    );
    let plan = resolve(&s).expect("valid");
    // 序列化 → 校验 camelCase 键名（尤其 aterm 4 caps 名逐字对齐、免映射）。
    let v: Value = serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
    assert_eq!(v["mode"], "PtyInject");
    assert_eq!(
        v["command"],
        "cct --resume abcd1234-5678-90ab-cdef-1234567890ab"
    );
    // substitutedFrom：MVP 恒省略——backend 不做候选消解、无「被替换的原命令」（对齐 aterm
    // 语义：仅解析 launch≠原意首候选时才有；见 resolve() 注 + aterm 2026-07-18 确认）。
    assert!(
        v.get("substitutedFrom").is_none(),
        "MVP 无候选消解 → substitutedFrom 省略（不再误设为被用候选）"
    );
    assert_eq!(v["sessionName"], "cc-abcd1234"); // cc-<sid8>
    let caps = &v["capabilities"];
    assert!(
        caps.get("supportsSendKeys").is_some(),
        "aterm caps 名 supportsSendKeys"
    );
    assert!(caps.get("supportsCapture").is_some());
    assert!(caps.get("supportsMultiClient").is_some());
    assert!(caps.get("supportsMultiWindow").is_some());
    // 短名不得出现（否则两端要映射）。
    assert!(caps.get("sendKeys").is_none(), "不得用短名 sendKeys");
}

/// DG6：agent_kind="codex" → `<base> resume <uuid>`（**子命令、无 --resume**）+ 会话名 `cx-`；
/// 默认基底 `codex`、自定义候选照用。golden-parity aterm CodexInvocation.resumeInvocation/resumeSessionName。
#[test]
fn resolve_codex_builds_resume_subcommand_and_cx_name() {
    let uuid = "019f75dd-875c-7c81-9eda-32f866b2c60f";
    // 无候选 → 默认 codex。
    let s = ResumeSpec {
        agent_kind: "codex".into(),
        ..spec(uuid, vec![])
    };
    let v: Value =
        serde_json::from_str(&serde_json::to_string(&resolve(&s).expect("valid")).unwrap())
            .unwrap();
    assert_eq!(v["command"], format!("codex resume {uuid}"));
    assert_eq!(v["sessionName"], "cx-019f75dd"); // cx-<sid8>（非 cc-）
    assert_eq!(v["mode"], "PtyInject");
    // 自定义 codex 启动候选 → `<base> resume <sid>`。
    let s2 = ResumeSpec {
        agent_kind: "codex".into(),
        ..spec(uuid, vec![Some("mycodex")])
    };
    let v2: Value =
        serde_json::from_str(&serde_json::to_string(&resolve(&s2).expect("valid")).unwrap())
            .unwrap();
    assert_eq!(v2["command"], format!("mycodex resume {uuid}"));
}

/// DG6 审计补：非规范 agent_kind（大小写/其它值）→ **落 Claude 路**（`--resume`/`cc-`），
/// 防 Codex 分支误吞。契约：wire 值定死小写 `"codex"`（大小写敏感）。
#[test]
fn resolve_non_codex_agent_kind_falls_back_to_claude() {
    for ak in ["", "claude", "Codex", "CODEX", "foo"] {
        let s = ResumeSpec {
            agent_kind: ak.to_string(),
            ..spec("sid_x", vec![Some("claude")])
        };
        let v: Value =
            serde_json::from_str(&serde_json::to_string(&resolve(&s).expect("valid")).unwrap())
                .unwrap();
        assert_eq!(
            v["command"], "claude --resume sid_x",
            "agent_kind={ak:?} 应落 Claude"
        );
        assert_eq!(v["sessionName"], "cc-sid_x");
    }
}

/// 无候选 → 默认 `claude`、substitutedFrom 省略（None → skip_serializing_if）。
#[test]
fn resolve_defaults_to_claude_when_no_candidates() {
    let s = spec("sid_123", vec![None, Some("   ")]); // 全空/空白 → 无可用
    let plan = resolve(&s).expect("valid");
    let v: Value = serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
    assert_eq!(v["command"], "claude --resume sid_123");
    assert!(
        v.get("substitutedFrom").is_none(),
        "无候选 → substitutedFrom 省略"
    );
}

/// B2：非法 sessionId（含 shell 元字符）→ 错误 {code,message}，不进 command（注入防线）。
#[test]
fn resolve_rejects_injection_in_session_id() {
    for bad in ["", "a b", "sid;rm -rf /", "$(whoami)", "a`b`", "x/y"] {
        let s = spec(bad, vec![Some("cc")]);
        let err = resolve(&s).expect_err("must reject");
        assert_eq!(err.0, "invalid_session_id", "拒 {bad:?}");
    }
    // 合法集通过。
    assert!(resolve(&spec("abc-DEF_123", vec![Some("cc")])).is_ok());
}

/// 审计 security①：B2 对称化——launchCandidate（base）含 shell 元字符 → unsafe_launch_candidate，
/// 不进 command（此前 base 零校验、注入透传：`["cc; rm -rf /"]`→`cc; rm -rf / --resume …`）。
#[test]
fn resolve_rejects_injection_in_launch_candidate() {
    for bad in [
        "cc; rm -rf /",
        "a$(id)",
        "x`whoami`",
        "a|b",
        "a>b",
        "a\nb",
        "a&b",
    ] {
        let s = spec("abc123", vec![Some(bad)]);
        let err = resolve(&s).expect_err("must reject base");
        assert_eq!(err.0, "unsafe_launch_candidate", "拒 base {bad:?}");
    }
    // 合法 launcher（带 flag/路径/等号）通过。
    assert!(resolve(&spec("abc123", vec![Some("/usr/bin/cc --foo=bar")])).is_ok());
}

/// 审计 quality-阻塞：`resolve_from_json`（= `run()` 的分发核）真覆盖 happy 路径——
/// stdin JSON 串 → CommandPlan JSON 串（含 aterm caps 名）。此前 `run()` 零覆盖。
#[test]
fn resolve_from_json_happy_returns_command_plan_json() {
    let out = resolve_from_json(
        r#"{"sessionId":"abcd1234-ef","launchCandidates":["cc"],"claudeDir":"/c","fallbackCwd":"/t","alreadyInTmux":false}"#,
    )
    .expect("ok");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["command"], "cc --resume abcd1234-ef");
    assert_eq!(v["capabilities"]["supportsSendKeys"], true);
}

/// 审计 quality-阻塞：`run()` 的 bad_request 分支真覆盖（此前只测 `serde_json::from_str` 本身、
/// 不走分发）。畸形 JSON 经 `resolve_from_json` → `(bad_request, _)`。
#[test]
fn resolve_from_json_malformed_is_bad_request() {
    let err = resolve_from_json("{not json").expect_err("must err");
    assert_eq!(err.0, "bad_request");
}

/// 分发核对非法 sid / 不安全 base 同样短路成对应错误码（端到端错误 taxonomy）。
#[test]
fn resolve_from_json_propagates_validation_errors() {
    assert_eq!(
        resolve_from_json(r#"{"sessionId":"a;b"}"#).unwrap_err().0,
        "invalid_session_id"
    );
    assert_eq!(
        resolve_from_json(r#"{"sessionId":"ok1","launchCandidates":["c;d"]}"#)
            .unwrap_err()
            .0,
        "unsafe_launch_candidate"
    );
}

// ════════════════════════════════════════════════════════════════════════
// 〔V126 · TL2〕`resolve` / `--resolve` 是给仓外 aterm 的**跨仓承诺**
// ════════════════════════════════════════════════════════════════════════
//
// 要求住址：用户裁决 **`V126`**（`设计/99 §1`，2026-09-25）逐字「后端 `resolve` 帧命令与 `--resolve`
// 子命令（给 aterm 冻结的跨仓契约）保留；在设计里登记为跨仓承诺并加判据钉住形状」；
// 契约正文住 `src/doc/IPC-PROTOCOL.md` §10「`resolve`」一节的「跨仓承诺」小节。
//
// 为什么这一族要比上面那几条更硬：仓内**零调用方**（`D §D7`）⇒ 改坏了**仓里没有任何东西会红**，
// 消费方在仓外、按字节读。上面那几条只核几个字段名与几条错误码；这一族把**整份线上形状**钉在
// 一份冻结金样上（`tests/__fixtures__/resolve-contract.golden.json`），并与那一节文档三方对拍。
//
// 异源：金样是冻结字节（手写、不从实现生成）· 代码侧读**生产**纯函数与源码 · 文档侧读 IPC-PROTOCOL 那一节。

fn v126_golden() -> Value {
    let p =
        crate::guard_support::repo_root().join("tests/__fixtures__/resolve-contract.golden.json");
    let raw = std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("读不到冻结金样 {}：{e}", p.display()));
    serde_json::from_str(&raw).expect("冻结金样不是 JSON")
}

fn v126_list(g: &Value, key: &str) -> Vec<String> {
    let v: Vec<String> = g[key]
        .as_array()
        .unwrap_or_else(|| panic!("金样缺 `{key}`"))
        .iter()
        .map(|x| x.as_str().expect("金样列表里只放字符串").to_string())
        .collect();
    assert!(!v.is_empty(), "金样 `{key}` 是空的 —— 下面的两向相等会空真");
    v
}

fn v126_sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

/// 本文件所在的生产源码（剥掉测试段与注释）。
fn v126_prod_src() -> String {
    let raw = include_str!("../../../src/backend/control/resolve_query.rs");
    guard_core::strip_comment_lines(&crate::guard_support::production_code(raw))
}

/// 〔V126〕R1 样例逐字节：金样每条请求过**生产** `resolve_from_json` ⇒ 成品串逐字节 == 金样；
/// 错误样例 ⇒ 码 == 金样；成品的键集 ⊆ 承诺的出参字段、`capabilities` 键集 == 四名、`mode` ∈ 承诺的取值。
#[test]
fn v126_every_frozen_sample_still_produces_the_same_bytes() {
    let g = v126_golden();
    let plan_fields = v126_list(&g, "plan_fields");
    let caps = v126_sorted(v126_list(&g, "capabilities"));
    let modes = v126_list(&g, "modes");
    let samples = g["samples"].as_array().expect("金样缺 samples");
    let (mut ok, mut err) = (0, 0);
    for s in samples {
        let req = s["request"].as_str().expect("request 是串");
        let got = resolve_from_json(req);
        if let Some(want) = s["product"].as_str() {
            ok += 1;
            let out = got.unwrap_or_else(|e| panic!("冻结样例 {req} 该成功，却回了 {e:?}"));
            assert_eq!(out, want, "冻结样例 {req} 的成品字节变了 —— 这是一次跨仓契约变更（IPC-PROTOCOL §10 跨仓承诺小节）");
            let v: Value = serde_json::from_str(&out).unwrap();
            for k in v.as_object().unwrap().keys() {
                assert!(plan_fields.contains(k), "成品里多出一个承诺外的键 `{k}`");
            }
            let got_caps = v126_sorted(
                v["capabilities"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect(),
            );
            assert_eq!(got_caps, caps, "capabilities 的键集 ≠ 承诺的四名");
            assert!(
                modes.iter().any(|m| v["mode"] == m.as_str()),
                "mode 不在承诺的取值里：{}",
                v["mode"]
            );
        } else {
            err += 1;
            let want = s["error"]
                .as_str()
                .expect("样例要么有 product 要么有 error");
            let e = got.expect_err("冻结的错误样例该失败");
            assert_eq!(e.0, want, "冻结错误样例 {req} 的码变了");
        }
    }
    assert!(
        ok >= 1 && err >= 1,
        "金样里成功 / 失败两形都要有（ok={ok} err={err}），否则这条只判了一半"
    );
}

/// 生产源码里一个 camelCase 结构体的**线上字段名**（标识符 camelCase 化；结构体前必须紧挨着 `rename_all = "camelCase"`，
/// 没有它标识符就不是线上名）。
fn v126_wire_fields(src: &str, decl: &str) -> Vec<String> {
    let start = src
        .find(decl)
        .unwrap_or_else(|| panic!("生产段里找不到 `{decl}`"));
    let head = &src[..start];
    let attr_at = head
        .rfind("#[serde(rename_all = \"camelCase\")]")
        .unwrap_or_else(|| panic!("`{decl}` 前没有 camelCase 属性"));
    assert!(
        !head[attr_at..].contains("struct "),
        "离 `{decl}` 最近的 camelCase 属性属于别的结构体"
    );
    let body = &src[start..start + src[start..].find("\n}").expect("结构体没收尾")];
    // 字段上的 `serde(rename…)` / `alias` 会让线上名与标识符分家 —— 那样下面读标识符就是在读错的东西
    //（`ResumeSpec.claude_dir` 头注逐字：「除非再挂一条 `serde(rename)` —— 那是把一条契约拆成两个真相源」）。
    for bad in ["rename", "alias"] {
        assert!(
            !body.contains(bad),
            "`{decl}` 的字段上挂了 serde `{bad}` —— 线上名不再等于标识符"
        );
    }
    let fields: Vec<String> = body
        .lines()
        .skip(1)
        .filter_map(|l| {
            let t = l.trim();
            if t.starts_with('#') {
                return None;
            }
            let (name, _) = t.split_once(':')?;
            let mut out = String::new();
            let mut up = false;
            for ch in name.trim().chars() {
                if ch == '_' {
                    up = true;
                } else if up {
                    out.extend(ch.to_uppercase());
                    up = false;
                } else {
                    out.push(ch);
                }
            }
            Some(out)
        })
        .collect();
    assert!(
        !fields.is_empty(),
        "`{decl}` 一个字段都没读出来 —— 下面的相等会空真"
    );
    fields
}

/// 〔V126〕R2 三个线上结构体的字段名（生产源码现读）== 金样（两向）：
/// `ResumeSpec` == `request_fields` · `CommandPlan` == `plan_fields` · `Capabilities` == `capabilities`。
/// 样例逐字节（R1）看不见**缺席即省略**的那几个出参（`launchLabel` / `substitutedFrom` 今天恒缺席），这一条补上。
#[test]
fn v126_the_wire_field_names_are_the_frozen_ones() {
    let src = v126_prod_src();
    let g = v126_golden();
    for (decl, key) in [
        ("struct ResumeSpec {", "request_fields"),
        ("struct CommandPlan {", "plan_fields"),
        ("struct Capabilities {", "capabilities"),
    ] {
        assert_eq!(
            v126_sorted(v126_wire_fields(&src, decl)),
            v126_sorted(v126_list(&g, key)),
            "`{decl}` 的线上字段 ≠ 冻结金样 `{key}`（两向）—— 改名 / 增删字段都是跨仓契约变更"
        );
    }
}

/// 〔V126〕R3 错误码全集：生产段交给错误出口的码字面量集合 == 金样 `error_codes`（两向）；
/// 流那条的登记表 `codes` == 全集减去只属于一次性那条的；信封只有 `code` / `message` 两键、退出码 == 金样。
#[test]
fn v126_the_error_codes_and_the_envelope_are_the_frozen_ones() {
    let g = v126_golden();
    let src = v126_prod_src();
    // 码的出口只有两形：`(码, 消息)` 元组（`Err((…))` / `map_err(|e| (…))`）与 `emit_err(码, …)` ——
    // 两形都是「`(` 之后（隔着空白）紧跟一个蛇形字面量、再跟 `,`」。元组可能折行，所以跳空白。
    let mut found = Vec::new();
    for (i, _) in src.match_indices('(') {
        let after = src[i + 1..].trim_start();
        let Some(rest) = after.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else { continue };
        let lit = &rest[..end];
        let tail = rest[end + 1..].trim_start();
        if tail.starts_with(',')
            && lit.contains('_')
            && lit.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        {
            found.push(lit.to_string());
        }
    }
    let codes = v126_sorted(v126_list(&g, "error_codes"));
    assert_eq!(
        v126_sorted(found),
        codes,
        "生产段真会回的错误码 ≠ 冻结金样（两向）"
    );

    let oneshot_only = v126_list(&g, "oneshot_only_codes");
    let stream_want: Vec<String> = codes
        .iter()
        .filter(|c| !oneshot_only.contains(c))
        .cloned()
        .collect();
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|c| c.name == "resolve")
        .expect("inbound::REGISTRY 里没有 `resolve` —— 跨仓承诺的流那条入口没了");
    assert_eq!(
        v126_sorted(spec.codes.iter().map(|s| s.to_string()).collect()),
        stream_want,
        "流那条 `resolve` 的登记码 ≠ 承诺全集 − 只属于一次性那条的"
    );

    let env: Value = serde_json::from_str(&error_envelope("bad_request", "m".into())).unwrap();
    let keys = v126_sorted(env.as_object().unwrap().keys().cloned().collect());
    assert_eq!(
        keys,
        v126_sorted(v126_list(&g, "error_envelope")),
        "错误信封的键 ≠ 承诺"
    );
    assert_eq!(env["code"], "bad_request");
    assert_eq!(
        i64::from(ERROR_EXIT),
        g["error_exit"].as_i64().expect("金样缺 error_exit"),
        "一次性那条的错误退出码 ≠ 承诺"
    );
    assert!(
        src.contains("fn emit_err(") && src.contains("ERROR_EXIT\n"),
        "`emit_err` 不再以 `ERROR_EXIT` 收尾 —— 退出码与承诺脱钩了"
    );
}

/// 〔V126〕R4 两条入口都在：流命令（`inbound::REGISTRY` 的 `resolve`，经 `resolve_json_for_inbound`）·
/// 一次性（`main.rs` 分派那一臂 ＋ `SUBCOMMANDS` 有 `--resolve`）。仓内零调用方 ⇒ 删掉哪一条仓里都不会有别的东西红。
#[test]
fn v126_both_entry_points_of_the_commitment_are_still_wired() {
    assert!(
        crate::SUBCOMMANDS.contains(&"--resolve"),
        "`SUBCOMMANDS` 里没有 `--resolve`（一次性那条入口）"
    );
    let main = guard_core::strip_comment_lines(include_str!("../../../src/backend/main.rs"));
    guard_core::pin_line(
        &main,
        "Some(\"--resolve\") => control::resolve_query::run(&agent_home, &args),",
    )
    .unwrap_or_else(|e| panic!("`main.rs` 分派里 `--resolve` 那一臂不在了：{e}"));
    let inbound = guard_core::strip_comment_lines(&crate::guard_support::production_code(
        include_str!("../../../src/backend/stream/inbound.rs"),
    ));
    let at = inbound
        .find("name: \"resolve\",")
        .expect("`inbound.rs` 里找不到 `resolve` 那一格");
    let tail = &inbound[at..];
    let cell = &tail[..tail.find("\n    },").expect("那一格没收尾")];
    assert!(
        cell.contains("resolve_query::resolve_json_for_inbound("),
        "流那条 `resolve` 不再经 `resolve_json_for_inbound` —— 两条路不再共用一个纯函数"
    );
}

/// 〔V126〕R5 文档那一节与金样两向相等：`IPC-PROTOCOL.md`「跨仓承诺」小节里四行列表
/// （入参 / 出参 / `capabilities` 四名 / 错误码）逐行取反引号里的名字 == 金样四个集合。
#[test]
fn v126_the_protocol_doc_lists_exactly_the_frozen_shape() {
    let doc =
        std::fs::read_to_string(crate::guard_support::repo_root().join("src/doc/IPC-PROTOCOL.md"))
            .expect("读不到 IPC-PROTOCOL.md");
    let start = doc
        .find("##### ★ 跨仓承诺（`V126`")
        .expect("IPC-PROTOCOL 里找不到「跨仓承诺（V126）」小节");
    let sec = &doc[start..];
    let sec = &sec[..sec[5..].find("\n#").map(|i| i + 5).unwrap_or(sec.len())];
    let names_after = |lead: &str| -> Vec<String> {
        let line = sec
            .lines()
            .find(|l| l.starts_with(lead))
            .unwrap_or_else(|| panic!("小节里找不到以 {lead} 开头的那一行"));
        let body = &line[line
            .find('：')
            .map(|i| i + '：'.len_utf8())
            .expect("那一行没有「：」")..];
        body.split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    };
    let g = v126_golden();
    for (lead, key) in [
        ("- **入参**", "request_fields"),
        ("- **出参**", "plan_fields"),
        ("- **`capabilities` 四名**", "capabilities"),
        ("- **错误码**", "error_codes"),
    ] {
        assert_eq!(
            v126_sorted(names_after(lead)),
            v126_sorted(v126_list(&g, key)),
            "IPC-PROTOCOL 跨仓承诺小节「{lead}」那一行 ≠ 冻结金样 `{key}`（两向）"
        );
    }
}
