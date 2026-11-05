//! typed 命令的两份生成物由类型现算：形状指纹表（`src/common/cmd-wire/src/shapes.rs`，后端 hello 交出去、客户端拿来比）·
//! 界面那张命令表（`src/frontend/ui/generated/TypedCommands.ts`：命令名 ⇒ 入参 · 回包 · 码）。
//! 跟盘上那份不一样 ⇒ 重写盘上那份、并红（提交新的那一份）。
//! 读登记表（主 crate 的 `stream::inbound::REGISTRY`）⇒ 挂主 crate（`cross/wire.rs`）；形状那一套住 `wire::typed::test_support`。

use crate::stream::inbound::REGISTRY;
use crate::wire::typed::test_support::Shape;
use crate::wire::typed::Codes;

/// 走登记表跑一条命令（同分派那一层：写句柄是测试档那一份；异步档在当场起的运行时上等它）。各族的测试经它问。
pub(crate) fn ask(cmd: &str, args: serde_json::Value) -> crate::wire::spec::Outcome {
    let spec = REGISTRY
        .iter()
        .find(|s| s.name == cmd)
        .unwrap_or_else(|| panic!("`{cmd}` 不在登记表里"));
    let req = crate::wire::Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        ..Default::default()
    };
    let own = crate::platform::fs::Own::for_tests();
    match crate::wire::spec::invoke(spec.run, &crate::stream::inbound::hands(Some(&own))) {
        Some(crate::wire::spec::Prepared::Blocking(f)) => f(req),
        Some(crate::wire::spec::Prepared::Async(f)) => {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("起运行时")
                .block_on(f(req))
        }
        None => panic!("`{cmd}` 是帧面硬臂，不经登记表跑"),
    }
}

/// 登记表里每条 typed 命令与它的形状，按命令名排。
fn typed() -> Vec<(&'static str, Shape)> {
    let mut v: Vec<(&'static str, Shape)> = REGISTRY
        .iter()
        .map(|s| {
            (
                s.name,
                Shape {
                    uncancellable: s.run.blocking(),
                    ..s.run.shape()
                },
            )
        })
        .collect();
    v.sort_by_key(|(n, _)| *n);
    v
}

/// 手机发出去的每条命令的入参（跨语言金样，手机那一侧 `SentRequestsTest` 照它每个拼入参的口写）：逐份照那条命令的入参类型解得开。
/// 金样里的命令都得是登记表里的 typed 命令。
#[test]
fn every_request_the_mobile_sends_decodes_by_that_commands_args_type() {
    let golden: std::collections::BTreeMap<String, Vec<serde_json::Value>> = serde_json::from_str(
        include_str!("../../__fixtures__/mobile-requests.golden.json"),
    )
    .expect("mobile-requests.golden.json 解不开");
    assert!(golden.len() > 20, "金样里没几条命令 —— 本条在空转");
    let rows = typed();
    let mut bad = Vec::new();
    for (cmd, sent) in &golden {
        let Some((_, shape)) = rows.iter().find(|(n, _)| n == cmd) else {
            bad.push(format!("{cmd}：登记表里没有这条 typed 命令"));
            continue;
        };
        for args in sent {
            if let Err(why) = (shape.decode)(args) {
                bad.push(format!("{cmd} {args}：{why}"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "手机发的入参照后端入参类型解不开：\n{}",
        bad.join("\n")
    );
}

/// 现算的那一份与盘上的比；不一样 ⇒ 写成现算的那一份、红。
fn fresh(rel: &str, want: &str) {
    let path = guard_core::addr::repo_root().join(rel);
    let got = std::fs::read_to_string(&path).unwrap_or_default();
    if got != want {
        std::fs::write(&path, want).unwrap();
        panic!("`{rel}` 跟类型对不上，已按类型重写：提交新的那一份");
    }
}

#[test]
fn the_shapes_table_is_what_the_types_say() {
    let rows = typed();
    assert!(
        !rows.is_empty(),
        "登记表里一条 typed 命令都没有 —— 本条在空转"
    );
    let mut table = String::new();
    let mut stuck = String::new();
    for (name, shape) in &rows {
        table.push_str(&format!("    (\"{name}\", \"{}\"),\n", shape.fingerprint()));
        if shape.uncancellable {
            stuck.push_str(&format!("    \"{name}\",\n"));
        }
    }
    let want = format!(
        "//! 每条 typed 命令的形状指纹：入参 · 回包的 TS 声明（连同它们引到的每个类型）＋ 错误码，合起来的 FNV-1a 32 位，8 位十六进制。\n\
//! 🔴 由后端判据 `wire::typed::tests::the_shapes_table_is_what_the_types_say` 从类型现算写出（不一致就重写这份并红）：别手改。\n\
\n\
/// `(命令, 形状指纹)`，按命令名排。指纹里算进了撤不撤得动（见 [`UNCANCELLABLE`]）。\n\
pub const SHAPES: &[(&str, &str)] = &[\n{table}];\n\
\n\
/// typed 命令里开跑之后撤不动的那几条（阻塞档），按命令名排。客户端撤单时读本侧这一份：指纹对得上才会发，对上了两边必然一致。\n\
pub const UNCANCELLABLE: &[&str] = &[\n{stuck}];\n\
\n\
/// 这条命令在本份编进来的形状指纹；不是 typed 命令 ⇒ `None`。\n\
pub fn shape_of(cmd: &str) -> Option<&'static str> {{\n    SHAPES.iter().find(|(c, _)| *c == cmd).map(|(_, h)| *h)\n}}\n"
    );
    fresh("src/common/cmd-wire/src/shapes.rs", &want);
}

/// 一个 TS 类型名里要 import 的那几个具名类型（`null` · 内联的不用）：本身就是一份声明的；带类型参数的（`SyncPatch<A, B>`）⇒ 打头那个 ＋ 每个参数里具名的。
fn named_in(t: &str, decls: &[(String, String)]) -> Vec<String> {
    t.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|tok| !tok.is_empty())
        .filter(|tok| {
            decls
                .iter()
                .any(|(n, _)| n == tok || n.starts_with(&format!("{tok}<")))
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn the_ui_command_map_is_what_the_types_say() {
    let rows = typed();
    let mut imports = std::collections::BTreeSet::new();
    let mut body = String::new();
    let mut data = String::new();
    for (name, shape) in &rows {
        for t in [&shape.req, &shape.reply] {
            imports.extend(named_in(t, &shape.decls));
        }
        let codes = shape
            .codes
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(" | ");
        body.push_str(&format!(
            "  \"{name}\": {{ req: {}; reply: {}; code: {codes} }};\n",
            shape.req, shape.reply
        ));
        if !shape.data.is_empty() {
            let per: Vec<String> = shape
                .data
                .iter()
                .map(|(code, ty)| {
                    imports.extend(named_in(ty, &shape.decls));
                    format!("\"{code}\": {ty}")
                })
                .collect();
            data.push_str(&format!("  \"{name}\": {{ {} }};\n", per.join("; ")));
        }
    }
    let imports: String = imports
        .iter()
        .map(|t| format!("import type {{ {t} }} from \"./{t}\";\n"))
        .collect();
    let want = format!(
        "// This file was generated by src/backend/wire/typed.rs（判据 `typed::tests::the_ui_command_map_is_what_the_types_say` 从类型现算写出）. Do not edit this file manually.\n\
{imports}\n\
/** typed 帧命令：命令名 ⇒ 入参 · 回包 · 它自己会回的码（`bad_args` 恒在：入参解不出来）。 */\n\
export interface TypedCommands {{\n{body}}}\n\
\n\
/** 拒绝信封里带 `data` 的码 ⇒ 那一份 `data` 的类型（随那条命令的形状指纹核过）；表里没有的码不带 `data`。 */\n\
export interface RefusalData {{\n{data}}}\n"
    );
    fresh("src/frontend/ui/generated/TypedCommands.ts", &want);
}

/// 外层命令那道边界：下层（经资产域那扇门交上来的）回了这一条码表里没有的码 ⇒ 落这一条自己的那一格失败（`fallback`），
/// 那一句照旧，原码与原话进原话（复制详情）；码表里有的照交。
#[test]
fn a_code_the_outer_table_lacks_lands_on_that_commands_own_failure() {
    crate::wire::typed::codes! {
        enum OuterErr {
            IoFailed = "io_failed",
        }
    }
    let kept = crate::wire::typed::of_fail(
        crate::wire::spec::fail("io_failed", "读不了"),
        OuterErr::IoFailed,
    )
    .into_said();
    assert_eq!(
        (kept.code.as_deref(), kept.said.as_str()),
        (Some("io_failed"), "读不了")
    );
    assert_eq!(kept.raw, None);

    let lower =
        crate::wire::spec::fail("not_dir", "那不是一个目录").and_raw(Some("ENOTDIR (os error 20)"));
    let f = crate::wire::typed::of_fail(lower, OuterErr::IoFailed).into_said();
    assert_eq!(
        f.code.as_deref(),
        Some("io_failed"),
        "码表里没有的码不上信封"
    );
    assert_eq!(f.said, "那不是一个目录");
    assert_eq!(
        f.raw.as_deref(),
        Some("not_dir: 那不是一个目录\nENOTDIR (os error 20)"),
        "原码与原话进原话"
    );
}

/// 契约错拒绝体不带出调用方给的原值（特别是哨兵串 / 秘密）。
#[test]
fn take_bad_args_omits_sentinel_value_from_rejection() {
    #[derive(serde::Deserialize, Debug)]
    struct DummyReq {
        #[expect(dead_code, reason = "只为反序列化那一下按类型拒，值没人读")]
        port: u16,
    }
    let sentinel = "SENTINEL_SECRET_VALUE_12345";
    let args = serde_json::json!({ "port": sentinel });
    let fail = crate::wire::typed::take::<DummyReq>(&args).expect_err("should fail");
    let rejection = format!(
        "code={} message={} raw={:?}",
        fail.code_or_fallback(),
        fail.said,
        fail.raw
    );
    assert!(
        !rejection.contains(sentinel),
        "拒绝体中泄露了哨兵串：{rejection}"
    );
}

/// 线上那一形里没有两端都不读的格：桌面 · 壳 · 手机 · 文件窗口都不读、给 AI 读的说明也没点名的格不上线。
/// 后端自己还要用的那几格留在类型里、不出帧（`serde(skip)`）；照生成的 TS 声明认（与线上 `serde_json` 同一套属性）。
#[test]
fn cells_no_reader_reads_are_off_the_wire() {
    fn decl<T: ts_rs::TS>() -> String {
        T::decl(&ts_rs::Config::default())
    }
    let cases: Vec<(&str, String, &[&str])> = vec![
        (
            "PlanStale",
            decl::<cmd_wire::plan::PlanStale>(),
            &["sinceText"],
        ),
        (
            "PlanArchived",
            decl::<cmd_wire::plan::PlanArchived>(),
            &["replacedBy"],
        ),
        (
            "PlanPendingSet",
            decl::<cmd_wire::plan::PlanPendingSet>(),
            &["blockedCode"],
        ),
        (
            "TurnRow",
            decl::<crate::observe::recognize::turns::TurnRow>(),
            &["endText", "peers"],
        ),
        (
            "Merged",
            decl::<cmd_wire::search::Merged>(),
            &["sessionCount"],
        ),
        (
            "AgentFacts",
            decl::<cmd_wire::session::AgentFacts>(),
            &["permissionDefault"],
        ),
        (
            "SessionCommand",
            decl::<cmd_wire::session::SessionCommand>(),
            &["needsArgs"],
        ),
        (
            "SessionAccountReply",
            decl::<cmd_wire::accounts::SessionAccountReply>(),
            &["queryable"],
        ),
        (
            "VerifyReport",
            decl::<cmd_wire::accounts::VerifyReport>(),
            &["warns"],
        ),
        (
            "SessionFacts",
            decl::<crate::observe::recognize::facts::SessionFacts>(),
            &["handedBack", "bgTasks", "answerOff"],
        ),
        (
            "RotTryDecision",
            decl::<cmd_wire::rotation::RotTryDecision>(),
            &["holdAccount"],
        ),
        (
            "RotTryUse",
            decl::<cmd_wire::rotation::RotTryUse>(),
            &["capUnknown"],
        ),
        (
            "RotStrategyItem",
            decl::<cmd_wire::rotation::RotStrategyItem>(),
            &["wordsBad"],
        ),
    ];
    let mut on: Vec<String> = Vec::new();
    for (ty, d, gone) in &cases {
        for g in *gone {
            let named = d.match_indices(*g).any(|(at, _)| {
                let before = d[..at].chars().next_back();
                let after = &d[at + g.len()..];
                before.is_some_and(|c| c.is_whitespace() || c == '{')
                    && (after.starts_with(':') || after.starts_with("?:"))
            });
            if named {
                on.push(format!("{ty}.{g}"));
            }
        }
    }
    assert!(on.is_empty(), "这几格还在线上：{on:?}");
}

/// 失败只一种值（`Said`）：经 `spec::fail` 带码，投出去那一下码缺 ⇒ 兜底码。
#[test]
fn a_failure_settles_with_its_code_or_the_fallback() {
    use crate::common::said::Said;
    use crate::wire::spec::Settle as _;
    assert_eq!(
        crate::wire::spec::fail("bad_args", "x".to_string())
            .settle("cmd")
            .code,
        "bad_args"
    );
    assert_eq!(Said::from("y").settle("cmd").code, "failed");
}
