//! T02：**配置面审计视图**的后端——「cc-monitor 到底动过你哪些文件」。
//!
//! ## 它同时是 T01 那笔债的清算
//!
//! T01 收工时 `tool_registry::TOOLS` **零生产消费者**，我当时明写了处置条件：
//! T02 收工若仍无消费者就删掉注册表（同一轮我以「只有测试在用」为由删过
//! `WriteVerdict::is_ok`，尺子得一致）。本模块把 `ToolSpec` 的**七个字段全部用上**：
//! `id`/`display_name` 分组，`source` 进「从哪来」列，`destination` **决定这条路径
//! 在本机还是远端**，`installable`/`uninstallable` 进「能否装/撤」列，`touches` 是表格主体。
//!
//! ## 一条硬纪律：**解析不了就说解析不了，绝不显示成"缺失"**
//!
//! 六个工具申报的路径里有四种本机根本查不到：远端路径（要 SSH）、相对项目目录的
//! `.mcp.json`（得先知道是哪个项目）、Windows 侧 `$PROFILE`（由 PowerShell 决定）、
//! 以及一层 glob（`~/.local/bin/cc-*`，这个能查但要另走一条路）。
//! 把这些一律画成红叉是**对能用的安装报假警报**——B04 审计已经抓过一次同型病
//! （只 `-x` 两个固定路径，于是装在 `/usr/local/bin` 且在 PATH 上的能用安装被报成"指不到"）。
//! 所以 [`SurfaceState`] 里没有"疑似缺失"这一档，只有 `Present` / `Absent` /
//! `Undetermined { why }`，**而 `why` 是必填的**。
//!
//! ## 只读
//!
//! 本模块不写任何用户文件（红线），也**不新增轮询**（红线）——一次按需扫完就返回。

use crate::tool_registry::{
    ToolDestination, ToolSource, ToolSpec, TouchEffect, TouchedFile, TOOLS,
};
use std::path::{Path, PathBuf};

/// 一条申报路径在**本机**能被解析到什么程度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathResolution {
    /// 本机一个确定的路径，可以直接查。
    Local(PathBuf),
    /// 本机一层 glob：在 `dir` 里找 `prefix*suffix`。
    LocalGlob {
        dir: PathBuf,
        prefix: String,
        suffix: String,
    },
    /// 远端路径。本页不连 SSH，所以查不到——**这不是"缺失"**。
    Remote(String),
    /// 相对某个项目目录，得先知道是哪个项目。
    NeedsProjectDir(String),
    /// Windows 侧 `$PROFILE`，路径由 PowerShell 决定。
    WindowsProfile,
}

/// 现状。**没有"疑似缺失"这一档**（见模块文档）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SurfaceState {
    Present {
        detail: String,
    },
    Absent,
    /// 查不了。`why` **必填**：用户看到"未确定"时必须知道为什么，
    /// 否则它和"缺失"在观感上没区别，而那正是本模块要避免的假警报。
    Undetermined {
        why: String,
    },
}

/// 注入的文件系统探针。做成注入是为了让**解析 + 观测**两步都能纯测
/// ——`sftp.rs` 那次教训：不可注入 = 不可测 = 那行代码没有门禁。
pub struct FsProbe<'a> {
    /// 返回 `(是否目录, 字节数)`；不存在返回 `None`。
    pub meta: &'a dyn Fn(&Path) -> Option<(bool, u64)>,
    /// 列一层目录里的**文件名**；读不了返回 `None`（≠ 空目录）。
    pub list: &'a dyn Fn(&Path) -> Option<Vec<String>>,
}

/// 把申报路径解析成本机可查的形态。
///
/// **「本机还是远端」从 `dest` 推导，不新增字段**（`TouchedFile` 的文档写了理由）。
/// `~/.claude/...` 走 [`crate::hooks_diag::claude_config_dir`]——那条 `CLAUDE_CONFIG_DIR`
/// 规则只准解释一次。
pub fn resolve_touched_path(
    declared: &str,
    dest: &ToolDestination,
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
) -> Result<PathResolution, String> {
    // **先把散文挡在门外。** 这条是被自己的反向自检抓出来加的：
    // `~/.local/bin/cc-*（12 条软链）` 原先能"成功"解析成
    // `LocalGlob { prefix: "cc-", suffix: "（12 条软链）" }`——glob 分支把散文吞进了 suffix，
    // 于是审计页会去找一个名叫 `cc-*（12 条软链）` 的东西，永远 0 匹配，
    // 而表格上显示的是干干净净的"缺失"。**比报错更坏**。
    //
    // 判据是白名单而不是"不许出现哪些坏字符"：申报路径的每个字符必须是
    // **ASCII graphic**（字母数字 + 标点，不含空白）。本仓六个工具的真实路径全部满足；
    // 散文一定不满足（全角括号、汉字、空格任一即出局）。
    // **已知代价如实写明**：真含空格或非 ASCII 的路径也会被拒——那种情况得显式加支持，
    // 而不是靠这条判据放水，因为放水就等于把散文一起放进来。
    if let Some(bad) = declared.chars().find(|c| !c.is_ascii_graphic()) {
        return Err(format!(
            "申报路径含非 ASCII-graphic 字符 {bad:?}（{declared:?}）——\
             给人看的说明请放 `note` 字段，`path` 只放机器可解析的路径"
        ));
    }
    match dest {
        ToolDestination::UserShellProfile => {
            if declared == "$PROFILE" {
                Ok(PathResolution::WindowsProfile)
            } else {
                // 落点是"用户选的 profile"却申报了别的路径 → 声明自相矛盾，宁可报错
                Err(format!(
                    "落点是 UserShellProfile，申报路径却是 {declared:?}（期望 \"$PROFILE\"）"
                ))
            }
        }
        ToolDestination::ProjectRelative(_) => {
            if declared.starts_with('~') || declared.starts_with('/') {
                return Err(format!(
                    "落点是 ProjectRelative，申报路径却是绝对/家目录形态 {declared:?}"
                ));
            }
            Ok(PathResolution::NeedsProjectDir(declared.to_string()))
        }
        ToolDestination::RemoteHomeRelative(_) => Ok(PathResolution::Remote(declared.to_string())),
        ToolDestination::LocalHomeRelative(_) => {
            let rest = declared
                .strip_prefix("~/")
                .ok_or_else(|| format!("本机路径必须以 `~/` 开头，实得 {declared:?}"))?;
            // `~/.claude/…` 的真实基准目录是 `CLAUDE_CONFIG_DIR`（若它确实是个目录）
            let (base, rel) = match rest.strip_prefix(".claude/") {
                Some(r) => (
                    crate::hooks_diag::claude_config_dir(cfg_dir_env, home, is_dir),
                    r.to_string(),
                ),
                None => (home.to_path_buf(), rest.to_string()),
            };
            let rel = rel.trim_end_matches('/');
            if rel.is_empty() {
                return Err(format!("申报路径解析后为空：{declared:?}"));
            }
            // glob 只允许出现在**最后一段**，且只允许一个 `*`
            let (dir_part, last) = match rel.rsplit_once('/') {
                Some((d, l)) => (Some(d), l),
                None => (None, rel),
            };
            if dir_part.is_some_and(|d| d.contains('*')) {
                return Err(format!("glob 只允许在最后一段，实得 {declared:?}"));
            }
            if let Some((prefix, suffix)) = last.split_once('*') {
                if suffix.contains('*') {
                    return Err(format!("只支持一个 `*`，实得 {declared:?}"));
                }
                let dir = match dir_part {
                    Some(d) => base.join(d),
                    None => base,
                };
                return Ok(PathResolution::LocalGlob {
                    dir,
                    prefix: prefix.to_string(),
                    suffix: suffix.to_string(),
                });
            }
            Ok(PathResolution::Local(base.join(rel)))
        }
    }
}

/// 观测一条已解析的路径。
pub fn observe(res: &PathResolution, fs: &FsProbe) -> SurfaceState {
    match res {
        PathResolution::Local(p) => match (fs.meta)(p) {
            None => SurfaceState::Absent,
            Some((true, _)) => match (fs.list)(p) {
                Some(v) => SurfaceState::Present {
                    detail: format!("目录，{} 项", v.len()),
                },
                // 目录在但列不了（权限）——**不能说成"空目录"**
                None => SurfaceState::Undetermined {
                    why: "目录存在但列不出内容（权限？）".into(),
                },
            },
            Some((false, n)) => SurfaceState::Present {
                detail: format!("文件，{n} 字节"),
            },
        },
        PathResolution::LocalGlob {
            dir,
            prefix,
            suffix,
        } => match (fs.list)(dir) {
            None => SurfaceState::Undetermined {
                why: format!("列不出目录 {}（不存在或无权限）", dir.display()),
            },
            Some(names) => {
                let n = names
                    .iter()
                    .filter(|s| {
                        s.len() >= prefix.len() + suffix.len()
                            && s.starts_with(prefix.as_str())
                            && s.ends_with(suffix.as_str())
                    })
                    .count();
                if n == 0 {
                    SurfaceState::Absent
                } else {
                    SurfaceState::Present {
                        detail: format!("{n} 项匹配"),
                    }
                }
            }
        },
        PathResolution::Remote(p) => SurfaceState::Undetermined {
            why: format!("远端路径（{p}）——本页不连 SSH，请到部署向导里查"),
        },
        PathResolution::NeedsProjectDir(p) => SurfaceState::Undetermined {
            why: format!("相对项目目录（{p}）——要先选定项目才知道查哪里"),
        },
        PathResolution::WindowsProfile => SurfaceState::Undetermined {
            why: "Windows 侧 $PROFILE，路径由 PowerShell 决定，本机无从解析".into(),
        },
    }
}

/// 「这东西从哪来」。用上 `ToolSpec::source`。
pub fn source_label(s: &ToolSource) -> String {
    match s {
        ToolSource::EmbeddedText { repo_path } => format!("仓内文件（编译期内嵌）：{repo_path}"),
        ToolSource::RepoDir { repo_path } => format!("仓内目录：{repo_path}"),
        ToolSource::Vendored {
            repo_path,
            fingerprint_file,
        } => format!("vendored + 指纹：{repo_path}（{fingerprint_file}）"),
        ToolSource::EmbeddedBinary { repo_path } => format!("交叉编译内嵌的二进制：{repo_path}"),
        ToolSource::Generated => "由 cc-monitor 现场生成的文本".to_string(),
    }
}

/// 「我们对它做什么」。措辞直接决定用户的危险感知，所以定在后端、UI 不再各写一遍。
pub fn effect_label(e: TouchEffect) -> &'static str {
    match e {
        TouchEffect::ReadOnly => "只读（诊断用），我们不写",
        TouchEffect::FencedBlock => "插入/更新一个有围栏的块；卸载时按围栏精确剥离",
        TouchEffect::OwnedFile => "整个文件由 cc-monitor 拥有，部署时整体覆盖",
        TouchEffect::GenerateOnly => "只生成待贴文本，由你自己粘贴——我们不写这个文件",
    }
}

/// 表格里的一行。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SurfaceRow {
    pub tool_id: &'static str,
    pub tool_name: &'static str,
    pub source_label: String,
    pub path_declared: &'static str,
    /// 解析出的本机路径（远端 / 项目相对 / `$PROFILE` 一律 `None`）。
    pub path_resolved: Option<String>,
    pub note: Option<&'static str>,
    pub effect_label: &'static str,
    pub state: SurfaceState,
    pub installable: bool,
    pub uninstallable: bool,
}

fn row(
    t: &'static ToolSpec,
    f: &'static TouchedFile,
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
    fs: &FsProbe,
) -> SurfaceRow {
    let resolved = resolve_touched_path(f.path, &t.destination, home, cfg_dir_env, is_dir);
    let (path_resolved, state) = match &resolved {
        Ok(r) => {
            let shown = match r {
                PathResolution::Local(p) => Some(p.to_string_lossy().into_owned()),
                PathResolution::LocalGlob {
                    dir,
                    prefix,
                    suffix,
                } => Some(format!("{}/{prefix}*{suffix}", dir.display())),
                _ => None,
            };
            (shown, observe(r, fs))
        }
        // **声明自相矛盾也要如实显示**，不能静默跳过一行——那会让表格看着很干净而实际漏了东西
        Err(e) => (
            None,
            SurfaceState::Undetermined {
                why: format!("申报路径与落点不自洽：{e}"),
            },
        ),
    };
    SurfaceRow {
        tool_id: t.id,
        tool_name: t.display_name,
        source_label: source_label(&t.source),
        path_declared: f.path,
        path_resolved,
        note: f.note,
        effect_label: effect_label(f.effect),
        state,
        installable: t.installable,
        uninstallable: t.uninstallable,
    }
}

/// 遍历注册表建表。纯函数（`is_dir` / `fs` 注入）。
pub fn build_rows(
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
    fs: &FsProbe,
) -> Vec<SurfaceRow> {
    TOOLS
        .iter()
        .flat_map(|t| {
            t.touches
                .iter()
                .map(move |f| row(t, f, home, cfg_dir_env, is_dir, fs))
        })
        .collect()
}

/// `settings.json` 的一个作用域。**这不是"我们碰的文件"**，而是「会影响钩子诊断结论」的文件
/// ——B04 登记项：那时只看 `<cfg>/settings.json` 一处，而钩子可以定义在别的作用域里，
/// 于是"没装"的结论可能是错的。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SettingsScope {
    pub scope: &'static str,
    pub path: String,
    pub state: SurfaceState,
    /// 文件里有没有 cc-bus 那两个钩子程序的字样。读不到 → `None`（**不猜**）。
    pub has_cc_bus_hooks: Option<bool>,
    pub precedence_note: &'static str,
}

/// cc-bus 钩子在 settings 里的两个程序名。只用来做**存在性提示**，不替代
/// `hooks_diag::diagnose` 的正经解析。
const HOOK_PROGRAMS: [&str; 2] = ["cc-register", "cc-bus-stop-hook"];

fn scope_row(
    scope: &'static str,
    path: PathBuf,
    precedence_note: &'static str,
    read: &dyn Fn(&Path) -> Option<String>,
    fs: &FsProbe,
) -> SettingsScope {
    let raw = read(&path);
    let has = raw
        .as_deref()
        .map(|s| HOOK_PROGRAMS.iter().any(|p| s.contains(p)));
    SettingsScope {
        scope,
        path: path.to_string_lossy().into_owned(),
        state: observe(&PathResolution::Local(path.clone()), fs),
        has_cc_bus_hooks: has,
        precedence_note,
    }
}

/// 列出**用户级**的两个 settings 作用域，并把「项目级没查」如实写成一行。
pub fn build_settings_scopes(
    home: &Path,
    cfg_dir_env: Option<&Path>,
    is_dir: &dyn Fn(&Path) -> bool,
    read: &dyn Fn(&Path) -> Option<String>,
    fs: &FsProbe,
) -> Vec<SettingsScope> {
    let cfg = crate::hooks_diag::claude_config_dir(cfg_dir_env, home, is_dir);
    vec![
        scope_row(
            "用户级",
            cfg.join("settings.json"),
            "钩子诊断读的就是这一份",
            read,
            fs,
        ),
        scope_row(
            "用户级 local",
            cfg.join("settings.local.json"),
            "优先级高于上一行；这里定义的钩子同样生效",
            read,
            fs,
        ),
        SettingsScope {
            scope: "项目级",
            path: "<项目目录>/.claude/settings.json 与 settings.local.json".into(),
            // **明说没查**，不假装查过（B04 登记项）
            state: SurfaceState::Undetermined {
                why: "本页不猜项目目录，所以没查。若钩子定义在项目级，\
                      上面那两行「没装」的结论可能是错的"
                    .into(),
            },
            has_cc_bus_hooks: None,
            precedence_note: "优先级最高",
        },
    ]
}

/// 一次审计的完整回报。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfigSurfaceReport {
    pub rows: Vec<SurfaceRow>,
    pub settings_scopes: Vec<SettingsScope>,
    /// 解析基准，展示用（让用户知道 `~/.claude` 被解释成了哪里）。
    pub claude_config_dir: String,
    pub home: String,
}

/// 扫一次配置面。**只读、一次性**（不新增轮询）。
#[tauri::command]
pub async fn config_surface_report() -> Result<ConfigSurfaceReport, String> {
    tokio::task::spawn_blocking(|| {
        let home = dirs::home_dir().ok_or_else(|| "取不到 HOME".to_string())?;
        let cfg_env = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
        let is_dir = |p: &Path| p.is_dir();
        let meta = |p: &Path| {
            std::fs::metadata(p)
                .ok()
                .map(|m| (m.is_dir(), if m.is_dir() { 0 } else { m.len() }))
        };
        let list = |p: &Path| {
            std::fs::read_dir(p).ok().map(|it| {
                it.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
            })
        };
        let read = |p: &Path| std::fs::read_to_string(p).ok();
        let fs = FsProbe {
            meta: &meta,
            list: &list,
        };
        let cfg_dir = crate::hooks_diag::claude_config_dir(cfg_env.as_deref(), &home, &is_dir);
        Ok(ConfigSurfaceReport {
            rows: build_rows(&home, cfg_env.as_deref(), &is_dir, &fs),
            settings_scopes: build_settings_scopes(&home, cfg_env.as_deref(), &is_dir, &read, &fs),
            claude_config_dir: cfg_dir.to_string_lossy().into_owned(),
            home: home.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structural_scan::ScanReport;

    fn home() -> PathBuf {
        PathBuf::from("/h")
    }
    fn no_dir(_: &Path) -> bool {
        false
    }
    fn yes_dir(_: &Path) -> bool {
        true
    }

    /// 一个什么都没有的文件系统。
    fn empty_probe<'a>() -> FsProbe<'a> {
        FsProbe {
            meta: &|_| None,
            list: &|_| None,
        }
    }

    // ===== 解析：五种结果各一条 =====

    #[test]
    fn resolves_local_home_paths() {
        let r = resolve_touched_path(
            "~/.local/bin/ccm",
            &ToolDestination::LocalHomeRelative("x"),
            &home(),
            None,
            &no_dir,
        )
        .unwrap();
        assert_eq!(r, PathResolution::Local(PathBuf::from("/h/.local/bin/ccm")));
    }

    /// `~/.claude/…` **必须**走 `CLAUDE_CONFIG_DIR` 那条规则，而且只解释一次。
    #[test]
    fn claude_paths_honor_config_dir_and_fall_back() {
        let acct = PathBuf::from("/h/.claude-accts/z");
        let with = resolve_touched_path(
            "~/.claude/settings.json",
            &ToolDestination::LocalHomeRelative("x"),
            &home(),
            Some(&acct),
            &yes_dir,
        )
        .unwrap();
        assert_eq!(
            with,
            PathResolution::Local(PathBuf::from("/h/.claude-accts/z/settings.json"))
        );
        // 环境变量指向的不是目录 → 回落 `~/.claude`（与 hooks_diag 同一条规则）
        let without = resolve_touched_path(
            "~/.claude/settings.json",
            &ToolDestination::LocalHomeRelative("x"),
            &home(),
            Some(&acct),
            &no_dir,
        )
        .unwrap();
        assert_eq!(
            without,
            PathResolution::Local(PathBuf::from("/h/.claude/settings.json"))
        );
    }

    #[test]
    fn resolves_one_level_glob() {
        let r = resolve_touched_path(
            "~/.local/bin/cc-*",
            &ToolDestination::LocalHomeRelative("x"),
            &home(),
            None,
            &no_dir,
        )
        .unwrap();
        assert_eq!(
            r,
            PathResolution::LocalGlob {
                dir: PathBuf::from("/h/.local/bin"),
                prefix: "cc-".into(),
                suffix: String::new(),
            }
        );
    }

    #[test]
    fn remote_project_and_profile_are_not_local() {
        assert_eq!(
            resolve_touched_path(
                "~/.local/bin/ccm-daemon",
                &ToolDestination::RemoteHomeRelative("x"),
                &home(),
                None,
                &no_dir
            )
            .unwrap(),
            PathResolution::Remote("~/.local/bin/ccm-daemon".into())
        );
        assert_eq!(
            resolve_touched_path(
                ".mcp.json",
                &ToolDestination::ProjectRelative("x"),
                &home(),
                None,
                &no_dir
            )
            .unwrap(),
            PathResolution::NeedsProjectDir(".mcp.json".into())
        );
        assert_eq!(
            resolve_touched_path(
                "$PROFILE",
                &ToolDestination::UserShellProfile,
                &home(),
                None,
                &no_dir
            )
            .unwrap(),
            PathResolution::WindowsProfile
        );
    }

    /// 申报路径与落点**不自洽**时要报错，而不是猜一个。
    #[test]
    fn declaration_inconsistency_is_an_error_not_a_guess() {
        for (declared, dest) in [
            (
                "/etc/passwd",
                ToolDestination::LocalHomeRelative("x"), // 不以 ~/ 开头
            ),
            ("~/somewhere", ToolDestination::ProjectRelative("x")),
            ("~/.bashrc", ToolDestination::UserShellProfile),
            (
                "~/.local/*/bin",
                ToolDestination::LocalHomeRelative("x"), // glob 不在最后一段
            ),
            (
                "~/.local/bin/*-*",
                ToolDestination::LocalHomeRelative("x"), // 两个 *
            ),
        ] {
            assert!(
                resolve_touched_path(declared, &dest, &home(), None, &no_dir).is_err(),
                "{declared:?} 配 {dest:?} 应判不自洽"
            );
        }
    }

    // ===== 观测：不确定必须带理由，且绝不冒充"缺失" =====

    #[test]
    fn undetermined_always_carries_a_reason() {
        for res in [
            PathResolution::Remote("~/x".into()),
            PathResolution::NeedsProjectDir(".mcp.json".into()),
            PathResolution::WindowsProfile,
        ] {
            match observe(&res, &empty_probe()) {
                SurfaceState::Undetermined { why } => {
                    assert!(!why.trim().is_empty(), "{res:?} 的理由不能是空的");
                }
                other => panic!("{res:?} 不该被判成 {other:?}——那是对能用的安装报假警报"),
            }
        }
    }

    #[test]
    fn present_absent_and_dir_listing() {
        let f = FsProbe {
            meta: &|p| match p.to_string_lossy().as_ref() {
                "/h/f" => Some((false, 42)),
                "/h/d" => Some((true, 0)),
                _ => None,
            },
            list: &|p| {
                if p == Path::new("/h/d") {
                    Some(vec!["a".into(), "b".into()])
                } else {
                    None
                }
            },
        };
        assert_eq!(
            observe(&PathResolution::Local("/h/f".into()), &f),
            SurfaceState::Present {
                detail: "文件，42 字节".into()
            }
        );
        assert_eq!(
            observe(&PathResolution::Local("/h/d".into()), &f),
            SurfaceState::Present {
                detail: "目录，2 项".into()
            }
        );
        assert_eq!(
            observe(&PathResolution::Local("/h/nope".into()), &f),
            SurfaceState::Absent
        );
    }

    /// 目录在但**列不出来**（权限）≠ 空目录。混掉的话用户会以为东西被删了。
    #[test]
    fn unlistable_dir_is_undetermined_not_empty() {
        let f = FsProbe {
            meta: &|_| Some((true, 0)),
            list: &|_| None,
        };
        match observe(&PathResolution::Local("/h/d".into()), &f) {
            SurfaceState::Undetermined { why } => assert!(why.contains("列不出")),
            other => panic!("实得 {other:?}"),
        }
    }

    #[test]
    fn glob_counts_only_real_matches() {
        let f = FsProbe {
            meta: &|_| None,
            list: &|_| {
                Some(vec![
                    "cc-send".into(),
                    "cc-recv".into(),
                    "ccm".into(), // 不匹配 cc-*（缺连字符）
                    "other".into(),
                ])
            },
        };
        let g = PathResolution::LocalGlob {
            dir: "/h/.local/bin".into(),
            prefix: "cc-".into(),
            suffix: String::new(),
        };
        assert_eq!(
            observe(&g, &f),
            SurfaceState::Present {
                detail: "2 项匹配".into()
            }
        );
        // 一个都不匹配 → 是真的没有，可以说 Absent
        let none = FsProbe {
            meta: &|_| None,
            list: &|_| Some(vec!["x".into()]),
        };
        assert_eq!(observe(&g, &none), SurfaceState::Absent);
        // 目录列不出来 → **不能说 Absent**
        assert!(matches!(
            observe(&g, &empty_probe()),
            SurfaceState::Undetermined { .. }
        ));
    }

    // ===== 结构性守卫：注册表里**每一条**申报路径都必须可解析 =====

    /// 要件 1+2+3：枚举 `TOOLS` 里每一条 `touches[].path`，逐条要求它能被解析成五种之一，
    /// 落进 `Err` 就是违规；`require` 拿计数自检。
    ///
    /// 这条守的是一个具体的退化：有人图省事把散文写回 `path`
    /// （第一版 6 个工具里就有 4 条是散文），于是审计页那一行永远显示"申报路径与落点不自洽"。
    fn every_declared_path_resolves() -> ScanReport {
        let mut r = ScanReport {
            checked: 0,
            violations: Vec::new(),
        };
        for t in TOOLS {
            for f in t.touches {
                r.checked += 1;
                if let Err(e) = resolve_touched_path(f.path, &t.destination, &home(), None, &no_dir)
                {
                    r.violations.push(format!("{}/{:?}：{e}", t.id, f.path));
                }
            }
        }
        r
    }

    #[test]
    fn all_registry_paths_are_machine_resolvable() {
        every_declared_path_resolves()
            .require(10, "TOOLS 的申报路径")
            .unwrap();
    }

    /// **反向自检**：守卫真的会抓散文。把 T01 第一版那四条散文路径各喂一次，必须全红。
    /// （不是构造的边角——它们是这个文件昨天的真实内容。）
    #[test]
    fn prose_paths_are_rejected() {
        for declared in [
            "~/.bashrc（或所选 profile）",
            "~/.claude/settings.json 的 hooks 段",
            "~/.local/bin/cc-*（12 条软链）",
            "<项目目录>/.mcp.json",
        ] {
            let dest = if declared.starts_with('<') {
                ToolDestination::ProjectRelative("x")
            } else {
                ToolDestination::LocalHomeRelative("x")
            };
            let r = resolve_touched_path(declared, &dest, &home(), None, &no_dir);
            // **必须直接 Err。** 第一版这里写的是"Err 或者解析出带括号的假路径都算抓到"，
            // 于是 `~/.local/bin/cc-*（12 条软链）` 溜了过去——它成功解析成
            // `LocalGlob { prefix: "cc-", suffix: "（12 条软链）" }`，`dir` 干干净净，
            // 我的谓词只看了 `dir`。测试当场红，才补上 `resolve_touched_path` 开头那道
            // ASCII-graphic 白名单。**断言要打在"解析必须失败"上，不是打在结果长相上。**
            assert!(
                r.is_err(),
                "散文路径 {declared:?} 必须被判不自洽，实得 {r:?}"
            );
        }
        // 而现在真文件里一条散文都没有
        assert!(every_declared_path_resolves().violations.is_empty());
    }

    /// **`TouchedFile` 的远端性从 `destination` 推导**这条假设要被钉住：
    /// 哪天出现「本机落点却申报远端文件」的组合，这里会红，届时才该给 `TouchedFile` 加字段。
    #[test]
    fn locality_is_derivable_from_destination_today() {
        for t in TOOLS {
            let remote = matches!(t.destination, ToolDestination::RemoteHomeRelative(_));
            for f in t.touches {
                // 申报路径里不许再自己写"远端"——那是 destination 的职责
                assert!(
                    !f.path.contains("远端"),
                    "{}：远端性该由 destination 表达，不该编进 path {:?}",
                    t.id,
                    f.path
                );
                let res =
                    resolve_touched_path(f.path, &t.destination, &home(), None, &no_dir).unwrap();
                assert_eq!(
                    matches!(res, PathResolution::Remote(_)),
                    remote,
                    "{}/{:?} 的远端判定与 destination 不一致",
                    t.id,
                    f.path
                );
            }
        }
    }

    // ===== 建表：七个字段都真被用上（T01 审计 I2 的验收点） =====

    #[test]
    fn rows_cover_every_touched_file_and_use_all_spec_fields() {
        let f = empty_probe();
        let rows = build_rows(&home(), None, &no_dir, &f);
        let expected: usize = TOOLS.iter().map(|t| t.touches.len()).sum();
        assert_eq!(rows.len(), expected, "每条 touches 都要有一行");
        for r in &rows {
            assert!(!r.tool_id.is_empty() && !r.tool_name.is_empty()); // id / display_name
            assert!(!r.source_label.is_empty()); // source
            assert!(!r.effect_label.is_empty()); // touches[].effect
            assert!(!r.path_declared.is_empty()); // touches[].path
        }
        // destination：远端那条必须解析不出本机路径
        let daemon = rows.iter().find(|r| r.tool_id == "remote-daemon").unwrap();
        assert!(daemon.path_resolved.is_none());
        // installable / uninstallable：cc-bus 两者都 false，ccm 两者都 true
        let ccbus = rows.iter().find(|r| r.tool_id == "cc-bus").unwrap();
        assert!(!ccbus.installable && !ccbus.uninstallable);
        let ccm = rows.iter().find(|r| r.tool_id == "ccm").unwrap();
        assert!(ccm.installable && ccm.uninstallable);
        // note：至少两个工具用上了
        let with_note: std::collections::HashSet<_> = rows
            .iter()
            .filter(|r| r.note.is_some())
            .map(|r| r.tool_id)
            .collect();
        assert!(
            with_note.len() >= 2,
            "note 至少两个工具用上，实得 {with_note:?}"
        );
    }

    /// `GenerateOnly` 的措辞必须**明确说我们不写**——这是用户定的调，写错了就是失信。
    #[test]
    fn generate_only_wording_says_we_do_not_write() {
        let l = effect_label(TouchEffect::GenerateOnly);
        assert!(l.contains("不写"), "实得 {l:?}");
        assert!(l.contains("待贴文本"));
        assert!(effect_label(TouchEffect::ReadOnly).contains("不写"));
    }

    // ===== B04 登记项：settings 的多个作用域 =====

    #[test]
    fn settings_scopes_include_local_and_admit_project_is_unchecked() {
        let f = FsProbe {
            meta: &|p| {
                if p.to_string_lossy().ends_with("settings.json") {
                    Some((false, 10))
                } else {
                    None
                }
            },
            list: &|_| None,
        };
        let read = |p: &Path| {
            if p.to_string_lossy().ends_with("settings.local.json") {
                Some("{\"hooks\":{\"SessionStart\":\"cc-register\"}}".to_string())
            } else {
                Some("{}".to_string())
            }
        };
        let s = build_settings_scopes(&home(), None, &no_dir, &read, &f);
        assert_eq!(s.len(), 3);
        assert!(s[0].path.ends_with("/.claude/settings.json"));
        assert!(s[1].path.ends_with("/.claude/settings.local.json"));
        // local 里有钩子字样 → 必须报 true（B04 的病：只看第一份会说"没装"）
        assert_eq!(s[1].has_cc_bus_hooks, Some(true));
        assert_eq!(s[0].has_cc_bus_hooks, Some(false));
        // 项目级：**明说没查**，且不给 has_cc_bus_hooks 一个假答案
        assert_eq!(s[2].scope, "项目级");
        assert_eq!(s[2].has_cc_bus_hooks, None);
        match &s[2].state {
            SurfaceState::Undetermined { why } => {
                assert!(why.contains("没查"), "实得 {why}");
                assert!(why.contains("可能是错的"), "要点明结论可能错，实得 {why}");
            }
            other => panic!("项目级不该有确定结论，实得 {other:?}"),
        }
    }

    /// 读不到文件时 `has_cc_bus_hooks` 必须是 `None`（**不猜 false**）。
    #[test]
    fn unreadable_settings_does_not_claim_absence_of_hooks() {
        let s = build_settings_scopes(&home(), None, &no_dir, &|_| None, &empty_probe());
        assert_eq!(s[0].has_cc_bus_hooks, None);
        assert_eq!(s[1].has_cc_bus_hooks, None);
    }

    /// **本模块只准读，不准写**（红线）。守法是**白名单**而不是"不许出现哪些写法"
    /// ——本会话的教训：黑名单版本被审计用五种我没想到的写法绕过，
    /// 而白名单枚举每一处 `fs::` 用法并要求它们**都**在允许集合里，新写法自动被拦。
    #[test]
    fn this_module_only_reads() {
        let src = include_str!("config_surface.rs");
        let code = src.split(concat!("#[cfg", "(test)]")).next().unwrap_or(src);
        // 剥掉注释与文档：注释里出现 `fs::write` 这个词不该判红（本会话踩过：
        // 把注释当代码，一条守卫数出 3 处而实际只有 1 处）
        let stripped: String = code
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//")
            })
            .collect::<Vec<_>>()
            .join("\n");
        // 反向自检：剥完还得看得见真代码，否则守卫在空转
        assert!(
            stripped.contains("pub fn resolve_touched_path"),
            "剥过头了，守卫在空转"
        );
        let mut uses: Vec<&str> = Vec::new();
        for (i, _) in stripped.match_indices("std::fs::") {
            let rest = &stripped[i + "std::fs::".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let idx = stripped[i..].find(&name).map(|k| i + k).unwrap_or(i);
            uses.push(&stripped[idx..idx + name.len()]);
        }
        // 允许集合就这三个，全部只读
        for u in &uses {
            assert!(
                matches!(*u, "metadata" | "read_dir" | "read_to_string"),
                "本模块只准只读的 fs 调用，发现 std::fs::{u}"
            );
        }
        // 计数自检（要件 3）：一处都没扫到 = 守卫失效了，而不是代码变干净了
        assert!(
            uses.len() >= 3,
            "只扫到 {} 处 std::fs:: 用法——守卫可能失效了（期望 metadata/read_dir/read_to_string 各至少一处）",
            uses.len()
        );
        // 且明确不许出现这些（即便将来换成别的前缀写法，上面的白名单也已经兜住 std::fs::）
        for bad in [
            "OpenOptions",
            "create_dir",
            "remove_file",
            "set_permissions",
        ] {
            assert!(!stripped.contains(bad), "本模块不得出现 {bad}——这一页只读");
        }
    }
}
