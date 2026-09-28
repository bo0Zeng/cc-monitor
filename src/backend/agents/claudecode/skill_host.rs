//! 〔MIG-3a · `设计/99 §2.1 ⑬` · 主会话 09-27 裁〕**skill 接入面的声明**（从 monitor `skill_host.rs` 搬来）：
//! 哪几个 skill 接进来了、它们住哪、产物里哪个文件是人要改的 —— 都是 Claude 的布局知识，住适配层（注册表 `Adapter.assets` 那一格）。
//!
//! 用户原话：「**这些 cc-bus、code-picture 的开发要充分解耦，只要是 skill 的集成都要好好解耦，因为 skill 容易改变**」
//! ⇒ 会变的（命令名 · 文件名 · schema）一律不进宿主；不变的四样（住哪个目录 · 产文本产物 · 产物里人要改的那个文件 · 装不装得上）
//! 就是 [`SkillSpec`] 的四段。**不调用任何 skill**（只做存在性探测）；读写那一半在通用层 `assets/skill_inbox.rs`，写经本进程文件管理面。

use copy_core::copy_text;
use std::path::{Path, PathBuf};

/// 一个接入的 skill。
pub(crate) struct SkillSpec {
    pub id: &'static str,
    pub label: fn() -> String,
    pub discover: Discover,
    pub artifacts: Artifacts,
    /// 产物根下人要改的那几个文件（相对 [`Artifacts::root`]）。
    pub editable: &'static [&'static str],
}

/// 怎么判它在不在场（只 `exists`，不执行任何东西）。
pub(crate) enum Discover {
    /// `<claude 目录>/skills/<dir>/<probe_file>`。
    ClaudeSkill {
        dir: &'static str,
        probe_file: &'static str,
    },
}

/// 产物住哪：`<项目目录>/<root>/<实例>/<instance_marker>`。
pub(crate) struct Artifacts {
    pub root: &'static str,
    pub instance_marker: &'static str,
}

pub(crate) const SKILLS: &[SkillSpec] = &[
    SkillSpec {
        id: "planned-build",
        label: || copy_text("rsSkillHost.skills.plannedBuildLabel", &[]),
        discover: Discover::ClaudeSkill {
            dir: "planned-build",
            probe_file: "bin/pb.py",
        },
        artifacts: Artifacts {
            root: ".claude/planned-build",
            instance_marker: "STATUS.md",
        },
        editable: &["INBOX.txt"],
    },
    SkillSpec {
        id: "cc-bus",
        label: || copy_text("rsSkillHost.skills.ccBusLabel", &[]),
        discover: Discover::ClaudeSkill {
            dir: "cc-bus",
            probe_file: "SKILL.md",
        },
        artifacts: Artifacts {
            root: "cc-monitor/src/shared/cc-bus",
            instance_marker: "SKILL.md",
        },
        editable: &[],
    },
];

fn spec_of(skill_id: &str) -> Result<&'static SkillSpec, String> {
    SKILLS
        .iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| copy_text("rsSkillHost.spec.unknown", &[("skillId", skill_id)]))
}

fn editable_paths(spec: &SkillSpec, cwd: &Path) -> Vec<PathBuf> {
    let root = cwd.join(spec.artifacts.root);
    spec.editable.iter().map(|f| root.join(f)).collect()
}

/// 每个接入的 skill 在 `cwd` 这个项目里的样子：`{id, label, missing_reason, instances, editable}`（线上名与界面 `SkillView` 逐字）。
pub(crate) fn views(cwd: &Path) -> Vec<serde_json::Value> {
    views_in(&super::paths::resolve_home(), cwd)
}

/// [`views`] 的本体：Claude 目录由调用方给（判据给临时目录，不碰真家目录）。
fn views_in(claude_dir: &Path, cwd: &Path) -> Vec<serde_json::Value> {
    SKILLS
        .iter()
        .map(|spec| {
            let Discover::ClaudeSkill { dir, probe_file } = &spec.discover;
            let expected = claude_dir.join("skills").join(dir).join(probe_file);
            let missing = (!expected.exists()).then(|| {
                copy_text(
                    "rsSkillHost.describe.missing",
                    &[
                        ("skill", spec.id),
                        ("expected", &expected.display().to_string()),
                    ],
                )
            });
            let root = cwd.join(spec.artifacts.root);
            let mut instances: Vec<String> = std::fs::read_dir(&root)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| {
                            let d = e.path();
                            d.is_dir() && d.join(spec.artifacts.instance_marker).exists()
                        })
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            instances.sort();
            serde_json::json!({
                "id": spec.id,
                "label": (spec.label)(),
                "missing_reason": missing,
                "instances": instances,
                "editable": editable_paths(spec, cwd)
                    .into_iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// 🔴 **能不能碰这一份**：`requested` 解到底之后必须恰是那个 skill 声明的可编辑文件之一（文件必须已存在），
/// 且不是 Claude 的数据文件（纵深）。回 `(项目根, 相对段)` —— 读写都经文件管理面，根是解过链接的项目目录。
pub(crate) fn editable_target(
    skill_id: &str,
    cwd: &Path,
    requested: &Path,
) -> Result<(PathBuf, String), String> {
    let spec = spec_of(skill_id)?;
    let real = requested.canonicalize().map_err(|e| {
        copy_text(
            "rsSkillHost.editable.resolveFailed",
            &[
                ("requested", &requested.display().to_string()),
                ("e", &e.to_string()),
            ],
        )
    })?;
    let allowed: Vec<PathBuf> = editable_paths(spec, cwd)
        .iter()
        .filter_map(|p| p.canonicalize().ok())
        .collect();
    if !allowed.contains(&real) {
        return Err(copy_text(
            "rsSkillHost.editable.notInSet",
            &[
                ("real", &real.display().to_string()),
                ("id", spec.id),
                ("editable", &format!("{:?}", spec.editable)),
                ("root", spec.artifacts.root),
            ],
        ));
    }
    // 纵深：声明表写歪了也不许碰会话记录 —— 判定借 `paths::is_session_record_file`（与删会话那一道同读 Claude 的目录结构，全仓一份）。
    if super::paths::is_session_record_file(&real.to_string_lossy()) {
        return Err(copy_text(
            "rsSkillHost.editable.protectedData",
            &[("real", &real.display().to_string())],
        ));
    }
    let root = cwd.canonicalize().map_err(|e| {
        copy_text(
            "rsSkillHost.target.resolveFailed",
            &[("cwd", &cwd.display().to_string()), ("e", &e.to_string())],
        )
    })?;
    let rel = real
        .strip_prefix(&root)
        .map_err(|_| {
            copy_text(
                "rsSkillHost.target.outside",
                &[
                    ("real", &real.display().to_string()),
                    ("root", &root.display().to_string()),
                ],
            )
        })?
        .to_string_lossy()
        .into_owned();
    Ok((root, rel))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/skill_host_tests.rs"]
mod tests;
