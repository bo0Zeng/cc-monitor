//! typed 命令的两份生成物由类型现算：形状指纹表（`src/common/cmd-wire/src/shapes.rs`，后端 hello 交出去、客户端拿来比）·
//! 界面那张命令表（`src/frontend/ui/generated/TypedCommands.ts`：命令名 ⇒ 入参 · 回包 · 码）。
//! 跟盘上那份不一样 ⇒ 重写盘上那份、并红（提交新的那一份）。

use super::{Codes, Command};
use crate::stream::inbound::REGISTRY;

/// 一条 typed 命令的形状（测试档）：入参 · 回包的 TS 名，连同它们引到的每个具名类型的 TS 声明，＋ 码表。
pub(crate) struct Shape {
    pub(crate) req: String,
    pub(crate) reply: String,
    pub(crate) codes: &'static [&'static str],
    /// `(TS 名, 声明)`，按名排、去重：入参与回包本身 ＋ 它们递归引到的每个具名类型。
    pub(crate) decls: Vec<(String, String)>,
    /// 开跑之后撤不动（阻塞档）。登记那一档定，[`typed`] 填；算进指纹：两端指纹对得上 ⇒ 撤不撤得动也一致。
    pub(crate) uncancellable: bool,
    /// 带 `data` 的那几个码 ⇒ `data` 的 TS 名（码表次序）；算进指纹：两端读 `data` 的形状对不上 ⇒ 指纹就对不上。
    pub(crate) data: Vec<(&'static str, String)>,
    /// 一份入参照这条命令的入参类型解（与分派同一个解码 [`super::take`]）；解不开 ⇒ 那一句。
    pub(crate) decode: fn(&serde_json::Value) -> Result<(), String>,
}

fn decode_of<C: Command>(args: &serde_json::Value) -> Result<(), String> {
    super::take::<C::Req>(args)
        .map(drop)
        .map_err(|f| format!("{}：{}", f.message, f.raw.unwrap_or_default()))
}

pub(crate) use cmd_wire::fingerprint::{ty_of, Ty};

/// [`super::Codes::Data`] 的那一份 TS 形状：元组逐位一个；[`super::Each`] ＝ 每个码同一形（`n` ＝ 码数）。
pub(crate) trait DataTys {
    fn tys(n: usize) -> Vec<Ty>;
}

impl<T: ts_rs::TS + 'static> DataTys for super::Each<T> {
    fn tys(n: usize) -> Vec<Ty> {
        (0..n).map(|_| ty_of::<T>()).collect()
    }
}

macro_rules! data_tys {
    ($($t:ident),*) => {
        impl<$($t: ts_rs::TS + 'static),*> DataTys for ($($t,)*) {
            fn tys(_: usize) -> Vec<Ty> {
                vec![$(ty_of::<$t>()),*]
            }
        }
    };
}
data_tys!();
data_tys!(A);
data_tys!(A, B);
data_tys!(A, B, C);
data_tys!(A, B, C, D);
data_tys!(A, B, C, D, E);
data_tys!(A, B, C, D, E, F);

pub(crate) fn shape_of<C: Command>() -> Shape {
    let (req, reply) = (ty_of::<C::Req>(), ty_of::<C::Reply>());
    let mut decls = req.decls;
    decls.extend(reply.decls);
    let codes = <C::Err as Codes>::DATA;
    let tys = <<C::Err as Codes>::Data as DataTys>::tys(codes.len());
    assert_eq!(
        tys.len(),
        codes.len(),
        "`{}`：带 `data` 的码与 `data` 的类型对不上位",
        C::NAME
    );
    let mut data = Vec::new();
    for (code, ty) in codes.iter().zip(tys) {
        decls.extend(ty.decls);
        data.push((*code, ty.name));
    }
    Shape {
        req: req.name,
        reply: reply.name,
        codes: <C::Err as Codes>::ALL,
        decls: decls.into_iter().collect(),
        uncancellable: false,
        data,
        decode: decode_of::<C>,
    }
}

impl Shape {
    /// 形状指纹：名 · 声明 · 码表逐行拼起来的 FNV-1a 32 位，8 位十六进制（两端比的就是它）。
    pub(crate) fn fingerprint(&self) -> String {
        let mut text = format!(
            "req {}\nreply {}\ncodes {}\ncancel {}\n",
            self.req,
            self.reply,
            self.codes.join(","),
            if self.uncancellable { "no" } else { "yes" }
        );
        // 带 `data` 的码：没有就不写这一行（不带 `data` 的命令指纹不变）。
        if !self.data.is_empty() {
            let d: Vec<String> = self.data.iter().map(|(c, t)| format!("{c}={t}")).collect();
            text.push_str(&format!("data {}\n", d.join(",")));
        }
        for (name, decl) in &self.decls {
            text.push_str(&format!("{name}\n{decl}\n"));
        }
        cmd_wire::fingerprint::fnv32(&text)
    }
}

/// 走登记表跑一条命令（同分派那一层：写句柄是测试档那一份；异步档在当场起的运行时上等它）。各族的测试经它问。
pub(crate) fn ask(cmd: &str, args: serde_json::Value) -> super::super::spec::Outcome {
    let spec = REGISTRY
        .iter()
        .find(|s| s.name == cmd)
        .unwrap_or_else(|| panic!("`{cmd}` 不在登记表里"));
    let req = crate::wire::Request {
        id: "t".to_string(),
        cmd: cmd.to_string(),
        args,
        within_ms: None,
        view: serde_json::Value::Null,
        tz: Default::default(),
        until: None,
    };
    let own = crate::platform::fs::Own::for_tests();
    match super::super::spec::invoke(spec.run, &crate::stream::inbound::hands(Some(&own))) {
        Some(super::super::spec::Prepared::Blocking(f)) => f(req),
        Some(super::super::spec::Prepared::Async(f)) => {
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
                    ..(s.run.shape)()
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
    let path = crate::guard_support::repo_root().join(rel);
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

#[test]
fn the_ui_command_map_is_what_the_types_say() {
    let rows = typed();
    let mut imports = std::collections::BTreeSet::new();
    let mut body = String::new();
    for (name, shape) in &rows {
        for t in [&shape.req, &shape.reply] {
            // 具名类型才 import（`null` · 内联的不用）。
            if shape.decls.iter().any(|(n, _)| n == t) {
                imports.insert(t.clone());
            }
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
    }
    // 帧里现算小成品的主题：那一份就是重问那条命令的回包（同一个处理器跑出来的），类型照那条命令的。
    let mut pushed = String::new();
    use super::super::topic::{Ask, Topic, TopicExt};
    for t in Topic::ALL {
        let s = t.spec();
        if s.ask == Ask::None {
            continue;
        }
        assert!(
            rows.iter().any(|(n, _)| *n == s.reask),
            "主题 `{}` 现算小成品照 `{}` 问，它不是 typed 命令",
            s.name,
            s.reask
        );
        pushed.push_str(&format!(
            "  \"{}\": TypedCommands[\"{}\"][\"reply\"];\n",
            s.name, s.reask
        ));
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
/** `changed` 帧里现算小成品的主题 ⇒ 那一份的类型（＝ 重问那条命令的回包，形状随那条命令的指纹核过）。 */\n\
export interface TopicProducts {{\n{pushed}}}\n"
    );
    fresh("src/frontend/ui/generated/TypedCommands.ts", &want);
}

/// 外层命令那道边界：下层（经资产域那扇门交上来的）回了这一条码表里没有的码 ⇒ 落这一条自己的那一格失败（`fallback`），
/// 那一句照旧，原码与原话进原话（复制详情）；码表里有的照交。
#[test]
fn a_code_the_outer_table_lacks_lands_on_that_commands_own_failure() {
    use super::super::spec::Fail;
    super::codes! {
        enum OuterErr {
            IoFailed = "io_failed",
        }
    }
    let kept =
        super::of_fail(Fail::new("io_failed", "读不了".into()), OuterErr::IoFailed).into_fail();
    assert_eq!(
        (kept.code.as_str(), kept.message.as_str()),
        ("io_failed", "读不了")
    );
    assert_eq!(kept.raw, None);

    let lower =
        Fail::new("not_dir", "那不是一个目录".into()).with_raw(Some("ENOTDIR (os error 20)"));
    let f = super::of_fail(lower, OuterErr::IoFailed).into_fail();
    assert_eq!(f.code, "io_failed", "码表里没有的码不上信封");
    assert_eq!(f.message, "那不是一个目录");
    assert_eq!(
        f.raw.as_deref(),
        Some("not_dir: 那不是一个目录\nENOTDIR (os error 20)"),
        "原码与原话进原话"
    );
}

/// ★ 手机嵌的 Rust 核心按 `cmd-wire::replies` 那张表解回包：表里每一条都是 typed 命令，解它用的类型就是后端那条命令回的类型（TS 名相同 ⇒ 同一个类型）。
/// 表里写错了类型（拿 `Sent` 去解 `session-model`）⇒ 手机把对的回包当成对不上 —— 这条当场红。
#[test]
fn the_phone_decodes_each_reply_with_the_type_the_backend_answers_with() {
    let rows: std::collections::BTreeMap<&str, Shape> = typed().into_iter().collect();
    let names = cmd_wire::replies::reply_names();
    assert!(!names.is_empty(), "表是空的 —— 本条在空转");
    for (cmd, name) in names {
        let shape = rows
            .get(cmd)
            .unwrap_or_else(|| panic!("`{cmd}` 不是 typed 命令（或不在登记表里）"));
        assert_eq!(
            shape.reply, name,
            "`{cmd}`：手机拿 `{name}` 解，后端回的是 `{}`",
            shape.reply
        );
    }
}
