//! 两家替身的足迹面（判「足迹汇总照每一家的足迹面各收一份 · 各号目录底下碰的那几份各列一行」用；
//! 收信钩子两家各一形：甲家另声明信任，记在家目录下另一份文件、值是那一条命令的固定哈希；乙家不要信任）。
//!
//! 两家每一格都不同形：受管工具 · 环境项的 id 与申报路径各一份、账号面身份表各一张（名字 · 份数都不同），
//! 通用层若只认一家 · 或拿默认那一家的身份表去解释另一家的号，判据就对不上。

use crate::agents::footprint_decl::{
    Carrier, EnvProbe, HostScope, Provisioning, Text, ToolDestination, ToolSource, ToolSpec,
    TouchEffect, TouchedFile, UnmanagedEnv,
};
use crate::agents::{IdentityClass, IdentityRoot};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// 甲家的号目录里那两份（凭据 · 配置）。
pub const IDENTITY_A: &[(&str, IdentityRoot, IdentityClass)] = &[
    (
        "fake-login.ndjson",
        IdentityRoot::ConfigDir,
        IdentityClass::Secret,
    ),
    ("fake-config.json", IdentityRoot::Home, IdentityClass::State),
];

/// 乙家的号目录里那三份（与甲家名字不同、份数不同）。
pub const IDENTITY_B: &[(&str, IdentityRoot, IdentityClass)] = &[
    ("b-token", IdentityRoot::ConfigDir, IdentityClass::Secret),
    (
        "b-token.lock",
        IdentityRoot::ConfigDir,
        IdentityClass::Derived,
    ),
    ("b.toml", IdentityRoot::ConfigDir, IdentityClass::State),
];

/// 甲家那一条受管工具（`carriers` 要 `'static`，各写一份字面量）。
const TOOLS_A: &[ToolSpec] = &[ToolSpec {
    id: "fake-a-records",
    display_name: Text(|| "替身甲的记录树".to_string()),
    installable: false,
    uninstallable: false,
    carriers: &[Carrier {
        source: ToolSource::NotOurs,
        destination: ToolDestination::NotInstalledByUs,
        touches: &[TouchedFile {
            path: "~/.fake-a/convos/",
            host: HostScope::Either,
            effect: TouchEffect::ReadOnly,
        }],
    }],
}];

/// 乙家那一条受管工具。
const TOOLS_B: &[ToolSpec] = &[ToolSpec {
    id: "fake-b-records",
    display_name: Text(|| "替身乙的记录树".to_string()),
    installable: false,
    uninstallable: false,
    carriers: &[Carrier {
        source: ToolSource::NotOurs,
        destination: ToolDestination::NotInstalledByUs,
        touches: &[TouchedFile {
            path: "~/.fake-b/logs/",
            host: HostScope::Either,
            effect: TouchEffect::ReadOnly,
        }],
    }],
}];

/// 甲家那一条环境项（它的命令行）。
const ENV_A: &[UnmanagedEnv] = &[UnmanagedEnv {
    id: "fake-a-cli",
    display_name: Text(|| "替身甲的命令行".to_string()),
    required: false,
    how: None,
    asks_install: true,
    who: Provisioning::UserProvides,
    probe: EnvProbe::OnPath,
    named: "fakea",
    host: HostScope::Either,
    site: "tests/backend/agents/fake/footprint.rs::ENV_A",
}];

/// 乙家那一条环境项。
const ENV_B: &[UnmanagedEnv] = &[UnmanagedEnv {
    id: "fake-b-cli",
    display_name: Text(|| "替身乙的命令行".to_string()),
    required: false,
    how: None,
    asks_install: true,
    who: Provisioning::UserProvides,
    probe: EnvProbe::OnPath,
    named: "fakeb",
    host: HostScope::Either,
    site: "tests/backend/agents/fake/footprint.rs::ENV_B",
}];

/// 甲家收信钩子那份文件的形：`{"onStop": [{"run": "<命令>"}, …]}`（与 Claude Code 那一形不同，通用层不认格名）。
fn commands_a(v: &Value) -> Vec<String> {
    runs_a(v).into_iter().map(|(_, c)| c).collect()
}

/// `onStop` 底下每一条的（序, 命令）。
fn runs_a(v: &Value) -> Vec<(usize, String)> {
    v.get("onStop")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .enumerate()
                .filter_map(|(i, it)| Some((i, it.get("run")?.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn slots_a(command: &str) -> Vec<(Vec<&'static str>, Value)> {
    vec![(vec!["onStop"], serde_json::json!({ "run": command }))]
}

/// 甲家信任住的那份：家目录下 `fake-trust.txt`，一行一对 `<键> = <值>`。
pub const TRUST_FILE_A: &str = "fake-trust.txt";

/// 甲家的信任键：`<钩子那份文件>#onStop/<序>`。
pub fn trust_key_a(hooks_file: &Path, index: usize) -> String {
    format!("{}#onStop/{index}", hooks_file.display())
}

/// 甲家的信任值：那一条命令的 FNV-1a 64 位，`fnv1a:` 打头。
pub fn trust_value_a(command: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in command.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a:{h:016x}")
}

fn trust_expected_a(hooks_file: &Path, v: &Value) -> Vec<(String, String, String)> {
    runs_a(v)
        .into_iter()
        .map(|(i, c)| {
            let (k, val) = (trust_key_a(hooks_file, i), trust_value_a(&c));
            (c, k, val)
        })
        .collect()
}

fn trust_recorded_a(raw: &str, key: &str) -> Option<String> {
    raw.lines().find_map(|l| {
        let (k, v) = l.split_once(" = ")?;
        (k == key).then(|| v.trim().to_string())
    })
}

/// 有那个键 ⇒ 换掉那一行；没有 ⇒ 末尾加一行。
fn trust_merge_a(now: &str, key: &str, value: &str) -> Option<String> {
    let line = format!("{key} = {value}");
    let mut found = false;
    let mut out: Vec<String> = now
        .lines()
        .map(|l| match l.split_once(" = ") {
            Some((k, _)) if k == key => {
                found = true;
                line.clone()
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        out.push(line);
    }
    Some(out.join("\n") + "\n")
}

/// 乙家收信钩子那份文件的形：`{"stop": ["<命令>", …]}`；不要信任。
fn commands_b(v: &Value) -> Vec<String> {
    v.get("stop")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn slots_b(command: &str) -> Vec<(Vec<&'static str>, Value)> {
    vec![(vec!["stop"], Value::String(command.to_string()))]
}

fn settings_a(home: &Path) -> [PathBuf; 2] {
    [
        home.join("fake-settings.json"),
        home.join("fake-settings.local.json"),
    ]
}

fn settings_b(home: &Path) -> [PathBuf; 2] {
    [home.join("b-hooks.json"), home.join("b.toml")]
}

/// 甲家的足迹面。
pub const FACE_A: crate::agents::FootprintFace = crate::agents::FootprintFace {
    tools: TOOLS_A,
    env: ENV_A,
    under_agent_home: |_| None,
    user_settings: settings_a,
    stop_hook_commands: commands_a,
    stop_hook_slots: slots_a,
    hook_trust: Some(crate::agents::HookTrust {
        file: |home| home.join(TRUST_FILE_A),
        expected: trust_expected_a,
        recorded: trust_recorded_a,
        merge: trust_merge_a,
    }),
};

/// 乙家的足迹面。
pub const FACE_B: crate::agents::FootprintFace = crate::agents::FootprintFace {
    tools: TOOLS_B,
    env: ENV_B,
    under_agent_home: |_| None,
    user_settings: settings_b,
    stop_hook_commands: commands_b,
    stop_hook_slots: slots_b,
    hook_trust: None,
};

/// 两家替身（`fake` 是默认起的那一家）：各带自己的足迹面与账号面身份表。
pub fn two_footprint_families() -> &'static [crate::agents::Adapter] {
    static TWO: std::sync::OnceLock<Vec<crate::agents::Adapter>> = std::sync::OnceLock::new();
    TWO.get_or_init(|| {
        let fam = |kind, env, wrapper, default, face, identity| {
            let row = super::family_row(kind, env, wrapper, default, || None);
            crate::agents::Adapter {
                footprint: Some(face),
                accounts: row
                    .accounts
                    .map(|a| crate::agents::AccountsFace { identity, ..a }),
                ..row
            }
        };
        vec![
            fam("fake", "CCM_FAKE_A", "fakea", true, FACE_A, IDENTITY_A),
            fam("fake2", "CCM_FAKE_B", "fakeb", false, FACE_B, IDENTITY_B),
        ]
    })
}
