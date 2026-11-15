//! 假 agent 的起会话面（`LaunchFace`）：默认启动器 · resume 那个字面量 · 起会话事实 · 附件写法（`AttachRefer`）。

use super::{rewind, AGENT_KIND};
use crate::agents::{
    Adapter, AttachRef, AttachRefer, Choice, ListedModel, ModeSwitch, ModelList, ModelSwitch,
    PermissionFace, PermissionMode, Refer, TaskFace, TaskText,
};

/// 能力 10：默认启动器。
pub const DEFAULT_COMMAND: &str = "fakeagent";

/// 能力 11：resume 那个字面量 —— **子命令形，且动词与 Codex 也不同**。
pub const RESUME_TOKEN: &str = "revive";

/// 权限档表：每一档两个参数、旗标与两家都不同形；`wild` 只认不选。
const MODES: &[PermissionMode] = &[
    PermissionMode {
        value: "ask",
        text: || "Ask".to_string(),
        args: &["--gate", "ask"],
    },
    PermissionMode {
        value: "roam",
        text: || "Roam".to_string(),
        args: &["--gate", "roam"],
    },
];

const SHOWN: &[Choice] = &[Choice {
    value: "wild",
    text: || "Wild".to_string(),
}];

/// 静态的模型表（清单读不成时用它）。
const MODELS: &[Choice] = &[Choice {
    value: "f-static",
    text: || "Static".to_string(),
}];

/// 清单形：一行一个模型 `原值|显示名|推理档,推理档|缺省档`（推理档 · 缺省档可空）。缺一列 · 原值空 ⇒ 整份读不成。
fn read_brains(out: &str) -> Option<Vec<ListedModel>> {
    let models = out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let [value, text, efforts, default] = l.trim().split('|').collect::<Vec<_>>()[..]
            else {
                return None;
            };
            (!value.is_empty()).then(|| ListedModel {
                value: value.to_string(),
                text: text.to_string(),
                efforts: efforts
                    .split(',')
                    .filter(|e| !e.is_empty())
                    .map(str::to_string)
                    .collect(),
                default_effort: (!default.is_empty()).then(|| default.to_string()),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (!models.is_empty()).then_some(models)
}

/// 起会话事实（`Adapter.launch` 那一格）。**组合与两家都不同**：Claude 是「有身份面 · 留 pidfile」，
/// Codex 是「两样都没有」；这一家是「有身份面 · 判不了活」。
/// 通用层若在哪一格上按名字认人，喂这一家就会答错（`fake_tests.rs` 的 ccm 规划那几条）。
/// 起会话面那几格（权限档 · 换档 · 模型）也与两家都不同形：权限档每档两个参数、换档投一句命令、模型参数 `--brain`、
/// 清单跑 `fakeagent brains --offline`（判据里换成测试档写出的回显小程序）、换模型那一句带 `@推理档`。
pub const LAUNCH: crate::agents::LaunchFace = crate::agents::LaunchFace {
    adapter_id: AGENT_KIND,
    speaker_name: "Fake",
    default_launcher: DEFAULT_COMMAND,
    launcher_alias: None,
    resume_token: RESUME_TOKEN,
    resume_short: None,
    preset_sid: None,
    // 夹具家同两家：起的时候就交。
    first_prompt_arg: true,
    permission: Some(PermissionFace {
        modes: MODES,
        shown: SHOWN,
        switch: Some(ModeSwitch::Command(|m| format!(":gate {m}"))),
    }),
    self_sid_env: None,
    launch_args: &[],
    nested_env: &["FAKEAGENT_PARENT"],
    is_default: false,
    models: Some(MODELS),
    model_args: Some(|m, effort| {
        let mut v = vec!["--brain".to_string(), m.to_string()];
        if let Some(e) = effort {
            v.extend(["--think".to_string(), e.to_string()]);
        }
        v
    }),
    model_list: Some(ModelList {
        args: &["brains", "--offline"],
        read: read_brains,
    }),
    model_default: None,
    model_text: None,
    model_pick: None,
    model_switch: Some(ModelSwitch {
        command: |m, effort| match effort {
            Some(e) => format!(":brain {m} @{e}"),
            None => format!(":brain {m}"),
        },
        read: None,
        saves: None,
        note: None,
        confirm: None,
    }),
    rewind: Some(rewind::FACE),
    task: Some(TaskFace {
        args: task_args,
        text: TaskText::File,
    }),
};

/// 一次性跑一件活：`fakeagent chore --into <结果文件> -- <说明>`（子命令 · 旗标与两家都不同形）；结果文本从结果文件读。
fn task_args(say: &str, out: &str) -> Vec<String> {
    ["chore", "--into", out, "--", say]
        .map(str::to_string)
        .to_vec()
}

/// 通用表之外这一家还认的图片扩展名：通用表里没有的那一种。
const IMAGES_EXTRA: &[&str] = &["bmp"];

/// 附件写法（`Adapter.attach` 那一格）：图片单独粘绝对路径、别的文件写 `<相对路径>`（与两家都不同形）；多申报一种图片扩展名。
pub const ATTACH: AttachRefer = AttachRefer {
    image: paste_abs,
    file: angle_rel,
    images_extra: Some(IMAGES_EXTRA),
};

fn paste_abs(a: &AttachRef<'_>) -> Refer {
    Refer::Paste(a.abs.to_string())
}

fn angle_rel(a: &AttachRef<'_>) -> Refer {
    Refer::Line(format!("<{}>", a.rel))
}

/// 不申报额外图片扩展名的那一行替身的 kind。
pub const PLAIN_KIND: &str = "fake-plain";

/// 两行替身（判「图片扩展名一张通用表 ＋ 一家额外申报」用）：[`AGENT_KIND`] 带 [`ATTACH`]；[`PLAIN_KIND`] 写法相同、不申报额外的扩展名。
pub fn attach_rows() -> [Adapter; 2] {
    let row = |kind, attach| Adapter {
        kind,
        attach: Some(attach),
        ..super::bare_row(super::home)
    };
    [
        row(AGENT_KIND, ATTACH),
        row(
            PLAIN_KIND,
            AttachRefer {
                images_extra: None,
                ..ATTACH
            },
        ),
    ]
}
