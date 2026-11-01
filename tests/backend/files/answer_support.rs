//! 判据按能力名问读族的那一口（[`crate::files::test_support`]）：本包的测试与上层的测试（命令表那几条结构判据 · 写面判据读回摘要）共用。
//! 只用本包 `[dependencies]` 里有的 crate。

use crate::common::said::Said;
use crate::files::Ask;
use cmd_wire::files as w;

/// 每条 [`Ask`] 在判据里的能力名（生产按入参类型分派，不读名字）。
pub trait Named: Ask {
    const CAPABILITY: &'static str;
}

impl Named for w::LsReq {
    const CAPABILITY: &'static str = "files.ls";
}
impl Named for w::StatReq {
    const CAPABILITY: &'static str = "files.stat";
}
impl Named for w::FindReq {
    const CAPABILITY: &'static str = "files.find";
}
impl Named for w::IndexStatusReq {
    const CAPABILITY: &'static str = "files.index.status";
}
impl Named for w::RebuildReq {
    const CAPABILITY: &'static str = "files.index.rebuild";
}
impl Named for w::ReadTextReq {
    const CAPABILITY: &'static str = "files.read.text";
}
impl Named for w::HomeReq {
    const CAPABILITY: &'static str = "files.home";
}
impl Named for w::SizeReq {
    const CAPABILITY: &'static str = "files.size";
}
impl Named for w::ReadChunkReq {
    const CAPABILITY: &'static str = "files.read.chunk";
}
impl Named for w::GrepReq {
    const CAPABILITY: &'static str = "files.grep";
}

/// 本族的能力名单（每条 [`Ask`] 的 [`Named::CAPABILITY`]）。与 [`answer`] 的分派两向对得上由
/// `inbound_structure_guards::the_files_read_family_is_online_exactly_as_it_is_declared` 钉（上线的那组 == 本表，且每条不落 `unknown_capability`）。
pub const CAPABILITIES: &[&str] = &[
    <w::LsReq as Named>::CAPABILITY,
    <w::StatReq as Named>::CAPABILITY,
    <w::FindReq as Named>::CAPABILITY,
    <w::RebuildReq as Named>::CAPABILITY,
    // 浏览名单跟着连接走（不是 `Ask`：帧面硬臂带本连接的号进 `files::browse`）；判据里记在 [`TEST_CONN`] 名下。
    "files.browse",
    <w::ReadTextReq as Named>::CAPABILITY,
    <w::ReadChunkReq as Named>::CAPABILITY,
    <w::SizeReq as Named>::CAPABILITY,
    <w::GrepReq as Named>::CAPABILITY,
    <w::HomeReq as Named>::CAPABILITY,
    <w::IndexStatusReq as Named>::CAPABILITY,
];

/// 判据里交浏览名单用的那条连接号（帧面那一侧每条连接一个号，从 1 起）。
pub const TEST_CONN: u64 = 1;

/// 判据里按能力名问一条（同命令表那一扇门：入参先按它的类型解，形状不对 ⇒ `bad_args`；回包按线上那一形摊回 JSON）。
/// 名字从每条 [`Ask`] 的 [`Named::CAPABILITY`] 来，不在里面 ⇒ `unknown_capability`。
pub fn answer(
    name: &str,
    args: &serde_json::Value,
    tz: &crate::common::time::Tz,
) -> Result<serde_json::Value, Said> {
    fn one<A: Ask + serde::de::DeserializeOwned>(
        args: &serde_json::Value,
        tz: &crate::common::time::Tz,
    ) -> Result<serde_json::Value, Said>
    where
        A::Reply: serde::Serialize,
    {
        let v = if args.is_null() {
            serde_json::json!({})
        } else {
            args.clone()
        };
        let req: A = serde_json::from_value(v).map_err(|e| {
            Said::new(
                "bad_args",
                crate::common::contract::malformed(&e.to_string()),
            )
        })?;
        crate::files::answer(req, tz).map(|r| serde_json::to_value(r).expect("回包总能序列化"))
    }
    match name {
        n if n == <w::LsReq as Named>::CAPABILITY => one::<w::LsReq>(args, tz),
        n if n == <w::StatReq as Named>::CAPABILITY => one::<w::StatReq>(args, tz),
        n if n == <w::FindReq as Named>::CAPABILITY => one::<w::FindReq>(args, tz),
        n if n == <w::RebuildReq as Named>::CAPABILITY => one::<w::RebuildReq>(args, tz),
        "files.browse" => {
            let v = if args.is_null() {
                serde_json::json!({})
            } else {
                args.clone()
            };
            let req: w::BrowseReq = serde_json::from_value(v).map_err(|e| {
                Said::new(
                    "bad_args",
                    crate::common::contract::malformed(&e.to_string()),
                )
            })?;
            crate::files::browse(TEST_CONN, req)
                .map(|r| serde_json::to_value(r).expect("回包总能序列化"))
        }
        n if n == <w::ReadTextReq as Named>::CAPABILITY => one::<w::ReadTextReq>(args, tz),
        n if n == <w::ReadChunkReq as Named>::CAPABILITY => one::<w::ReadChunkReq>(args, tz),
        n if n == <w::SizeReq as Named>::CAPABILITY => one::<w::SizeReq>(args, tz),
        n if n == <w::GrepReq as Named>::CAPABILITY => one::<w::GrepReq>(args, tz),
        n if n == <w::HomeReq as Named>::CAPABILITY => one::<w::HomeReq>(args, tz),
        n if n == <w::IndexStatusReq as Named>::CAPABILITY => one::<w::IndexStatusReq>(args, tz),
        other => Err(Said::new(
            "unknown_capability",
            crate::common::contract::malformed(&format!("unknown capability `{other}`")),
        )),
    }
}

/// 同 [`answer`]，名字写线上那一形（`files-ls`）。
pub fn answer_wire(
    wire_name: &str,
    args: &serde_json::Value,
    tz: &crate::common::time::Tz,
) -> Result<serde_json::Value, Said> {
    answer(&wire_name.replace('-', "."), args, tz)
}
