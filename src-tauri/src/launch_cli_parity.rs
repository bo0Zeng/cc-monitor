//! U8c-2c-1：`launch_core::cli::render_ccm_invocation` **↔** TS `tryRenderCli` 的**逐字节对拍**。
//!
//! 机制与 `launch_payload_parity.rs` 完全相同（入库夹具，两侧各自与它比，
//! 绝不让 Rust 去调 TS 现场生成 —— 那是 U7-4 的自洽夹具病根）。
//!
//! ⚠ **ok 与 refusal 两类都比**：只比 ok 的话，「该降级却渲染出来了」抓不到，
//! 而那正是 §33 铁律要防的形态。

use launch_core::cli::{render_ccm_invocation, Action, CliAccount, CliSpec, Container};
use serde::Deserialize;
use std::collections::BTreeSet;

const FIXTURE: &str = include_str!("../crates/launch-core/fixtures/cli-golden.json");

/// 与 `launch_payload_parity` 同理：写成相等而不是地板，加/删用例被迫回来改这个数。
const EXPECT_CASES: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    #[serde(rename = "_")]
    _comment: String,
    #[serde(rename = "defaultLauncher")]
    default_launcher: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    caps: Option<Vec<String>>,
    ctx: Ctx,
    ok: bool,
    out: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Ctx {
    transport: Kind,
    action: FxAction,
    container: FxContainer,
    cwd: Option<String>,
    account: FxAccount,
    launcher_override: Option<String>,
    ccm_sid: Option<String>,
    #[serde(default)]
    model_override: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kind {
    kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum FxAction {
    New,
    Resume { sid: String },
    Attach { name: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum FxContainer {
    None,
    Tmux {
        name: String,
        /// **刻意声明却不读**：`deny_unknown_fields` 要求把 TS 侧的每个字段都列出来，
        /// 而 CLI 渲染只用 `name` 与 `mode`（`nameQuoting` 是兜底渲染器那半的事）。
        /// 不声明就会被当未知字段拒掉 —— 那正是 `deny_unknown_fields` 该有的样子。
        #[allow(dead_code)]
        #[serde(rename = "nameQuoting")]
        name_quoting: String,
        mode: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum FxAccount {
    Base,
    Account {
        #[serde(default)]
        name: Option<String>,
        /// 同上：CLI 侧只需要**名字**（`--account <名>`），`configDir` 是载荷那半的事。
        #[allow(dead_code)]
        #[serde(rename = "configDir")]
        config_dir: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("夹具不是合法 JSON —— 重跑 npm run gen:payload-golden")
    }

    /// ★ 计数自检：先证明「有东西可比」，再比。**ok 与 refusal 各自也要有下限** ——
    /// 只剩 ok 那半的话，「该降级却渲染出来了」就没人管了。
    #[test]
    fn the_fixture_covers_both_ok_and_refusal() {
        let f = fixture();
        assert_eq!(
            f.cases.len(),
            EXPECT_CASES,
            "夹具用例数变了（加/删用例请一起改 EXPECT_CASES）"
        );
        let ok = f.cases.iter().filter(|c| c.ok).count();
        let refused = f.cases.len() - ok;
        assert!(ok >= 6, "ok 类只有 {ok} 条");
        assert!(
            refused >= 5,
            "refusal 类只有 {refused} 条 —— §33 要防的正是「该降级却渲染出来了」"
        );
    }

    /// ★ 正题：同一组输入，两种语言的产出（命令串**或**降级理由）逐字节相同。
    #[test]
    fn rust_cli_rendering_matches_the_typescript_golden_byte_for_byte() {
        let f = fixture();
        let mut bad = Vec::new();
        for c in &f.cases {
            let caps: BTreeSet<String> = c.caps.clone().unwrap_or_default().into_iter().collect();
            let installed = c.caps.is_some();
            let action = match &c.ctx.action {
                FxAction::New => Action::New,
                FxAction::Resume { sid } => Action::Resume { sid },
                FxAction::Attach { name } => Action::Attach { name },
            };
            let container = match &c.ctx.container {
                FxContainer::None => Container::None,
                FxContainer::Tmux { name, mode, .. } => Container::Tmux {
                    name,
                    send_into: mode == "send-into",
                },
            };
            let account = match &c.ctx.account {
                FxAccount::Base => CliAccount::Base,
                FxAccount::Account { name, .. } => CliAccount::Named {
                    name: name.as_deref(),
                },
            };
            let launcher = c
                .ctx
                .launcher_override
                .as_deref()
                .unwrap_or(&f.default_launcher);
            let spec = CliSpec {
                is_ssh: c.ctx.transport.kind == "ssh",
                action,
                container,
                cwd: c.ctx.cwd.as_deref(),
                account,
                ccm_sid: c.ctx.ccm_sid.as_deref(),
                model: c.ctx.model_override.as_deref(),
                launcher,
                default_launcher: &f.default_launcher,
                args: &[],
                ccm_path: "ccm",
            };
            let (got_ok, got) = match render_ccm_invocation(&spec, &caps, installed) {
                Ok(cmd) => (true, cmd),
                Err(r) => (false, r.reason()),
            };
            if got_ok != c.ok || got != c.out {
                bad.push(format!(
                    "  用例「{}」\n    TS  : ok={} {:?}\n    Rust: ok={} {:?}",
                    c.name, c.ok, c.out, got_ok, got
                ));
            }
        }
        assert!(
            bad.is_empty(),
            "{} / {} 条 CLI 渲染两侧不一致：\n{}",
            bad.len(),
            f.cases.len(),
            bad.join("\n")
        );
    }
}
