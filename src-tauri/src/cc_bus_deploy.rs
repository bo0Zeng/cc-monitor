//! `PS1`：把仓内那份 cc-bus 部署到 `<claude_dir>/skills/cc-bus/`。
//!
//! # 这条路本来是被**只读铁律**堵死的
//!
//! `doc/INVARIANTS.md` 开头那条：「`monitor` 对 `<claude_dir>/…` **只读**」，后附**穷举**的
//! 例外。`PS1` 摸底（08-12）逐条读完 —— **没有一条覆盖「往 `~/.claude/skills/` 装东西」**，
//! 于是本件当时缩成「一条待裁 + 一处如实登记」，待决 `U10b`。
//!
//! **08-13 用户裁：开。** ⇒ 那条例外写成 `INVARIANTS` 的**第 7 条**，四个配套要求
//! （用户显式动作 · 独立 realpath 白名单 · 幂等 · 可撤销）**一条都不许省** ——
//! 本模块就是那四条的落点，逐条对应到下面四个 `★`。
//!
//! # 为什么源是**内嵌**的而不是读仓
//!
//! 装了的 app 身边**没有仓**。照 `acct_iso_deploy` 的既定做法 `include_bytes!` 内嵌
//! （那份头注逐字：「故直接 `include_bytes!` 内嵌」）。⇒ 单一事实源仍是 `shared/cc-bus/`，
//! 编译期把它固化进二进制。
//!
//! # `U9`② 顺带有答案了（实测，不是投票）
//!
//! 那一问是「仓内那份 vs `~/.claude/skills/` 那份，拿哪份当真相源」。08-13 实测两份的差异：
//! **只有 2 个文件**（`scripts/cc-spawn` 与 `SKILL.md`），而它们正是 `P4b` 改的那两个
//! ⇒ **仓内那份 = 已装那份 + `P4b` 的修复**，是严格更新的一侧，且它 git 管着、判据钉着。
//! ⇒ **仓内那份当真相源**，本模块把它推下去；反向（把已装那份 import 回仓）会把 `P4b` 撤销。

use std::path::{Path, PathBuf};

/// 内嵌的那 17 个文件。**单一事实源 = `shared/cc-bus/`**，这里只是编译期固化。
///
/// ⚠ 加文件要同时加到这里 —— 判据 `the_embedded_file_list_matches_the_repo` 会对拍，
/// 少一个当场红（否则装出去的是个**缺件的** skill，而那比不装更糟）。
const FILES: &[(&str, &[u8])] = &[
    ("SKILL.md", include_bytes!("../../shared/cc-bus/SKILL.md")),
    (
        "examples/cc-busd.service",
        include_bytes!("../../shared/cc-bus/examples/cc-busd.service"),
    ),
    (
        "examples/config",
        include_bytes!("../../shared/cc-bus/examples/config"),
    ),
    (
        "examples/policy.tsv",
        include_bytes!("../../shared/cc-bus/examples/policy.tsv"),
    ),
    (
        "scripts/cc-agents",
        include_bytes!("../../shared/cc-bus/scripts/cc-agents"),
    ),
    (
        "scripts/cc-broadcast",
        include_bytes!("../../shared/cc-bus/scripts/cc-broadcast"),
    ),
    (
        "scripts/cc-bus-install.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-install.sh"),
    ),
    (
        "scripts/cc-bus-lib.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-lib.sh"),
    ),
    (
        "scripts/cc-bus-stop-hook",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-stop-hook"),
    ),
    (
        "scripts/cc-busd",
        include_bytes!("../../shared/cc-bus/scripts/cc-busd"),
    ),
    (
        "scripts/cc-kill",
        include_bytes!("../../shared/cc-bus/scripts/cc-kill"),
    ),
    (
        "scripts/cc-list",
        include_bytes!("../../shared/cc-bus/scripts/cc-list"),
    ),
    (
        "scripts/cc-recv",
        include_bytes!("../../shared/cc-bus/scripts/cc-recv"),
    ),
    (
        "scripts/cc-register",
        include_bytes!("../../shared/cc-bus/scripts/cc-register"),
    ),
    (
        "scripts/cc-send",
        include_bytes!("../../shared/cc-bus/scripts/cc-send"),
    ),
    (
        "scripts/cc-spawned-record",
        include_bytes!("../../shared/cc-bus/scripts/cc-spawned-record"),
    ),
    (
        "scripts/cc-spawn",
        include_bytes!("../../shared/cc-bus/scripts/cc-spawn"),
    ),
    (
        "scripts/cc-whoami",
        include_bytes!("../../shared/cc-bus/scripts/cc-whoami"),
    ),
];

/// 一次部署的结果。**「没变」与「装好了」是两种结果**，不合并 ——
/// 合并的话用户点一次看不出到底动没动盘。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub struct CcBusDeployReport {
    /// 落点绝对路径（给用户看，也便于他自己去查）。
    pub dest: String,
    /// 实际写了几个文件（幂等跳过的不计）。
    pub written: u32,
    /// 与内嵌一致、跳过的。
    pub unchanged: u32,
    /// 覆盖前的备份目录（`None` = 之前没装过，无需备份）。
    pub backup: Option<String>,
    /// ★★ **装成功了、但装出来的东西现在跑不起来**时的那句话〔08-13〕。
    ///
    /// `C15` 之后 `cc-spawn` 硬依赖新 `ccm`（开头做能力协商，缺一条就 `exit 2`）。
    /// 只装 cc-bus、不同步 `ccm` ⇒ **部署这一步一切正常，用户的 `cc-spawn` 当场不能用**。
    ///
    /// ⚠ 为什么必须回到**返回值**里而不是只写日志：这是用户点按钮换来的结果，
    /// 而日志他不会去翻。「成功 + 一句日志」在他眼里就是**纯成功** ——
    /// 那正是本仓一路在治的「假成功比失败更坏」。
    /// ⚠ 它**不是错误**：装本身做完了，且用户完全可能紧接着就同步 `ccm`。
    pub warning: Option<String>,
}

/// ★ **白名单（独立 realpath 围栏）** —— 落点只能是这一个。
///
/// ⚠ 它**不是**「拼出来的路径正好等于它」，而是**canonicalize 之后**再比：
/// 符号链接、`..`、大小写差异都得先归一，否则围栏是纸的。
/// ⚠ 父目录（`<claude_dir>/skills`）可能还不存在 ⇒ canonicalize 那一层，再拼最后一段。
fn fenced_dest(claude_dir: &Path) -> Result<PathBuf, String> {
    let skills = claude_dir.join("skills");
    std::fs::create_dir_all(&skills).map_err(|e| format!("建 {} 失败：{e}", skills.display()))?;
    let real = skills
        .canonicalize()
        .map_err(|e| format!("解析 {} 失败：{e}", skills.display()))?;
    let claude_real = claude_dir
        .canonicalize()
        .map_err(|e| format!("解析 {} 失败：{e}", claude_dir.display()))?;
    // 归一之后 `skills` 必须仍在 claude_dir 底下 —— 挡「skills 是个指向别处的软链」。
    if !real.starts_with(&claude_real) {
        return Err(format!(
            "拒绝：`{}` 归一之后落在 `{}` 之外（软链？）—— 只读铁律的第 7 条例外**只**放行 \
             `<claude_dir>/skills/cc-bus`",
            real.display(),
            claude_real.display()
        ));
    }
    Ok(real.join("cc-bus"))
}

/// ★ **幂等**：逐文件比内容，一致就跳过。
fn same_content(path: &Path, bytes: &[u8]) -> bool {
    std::fs::read(path).map(|got| got == bytes).unwrap_or(false)
}

/// ★ **可撤销**：覆盖前把整个目录改名成 `cc-bus.bak-<时间戳>`。
///
/// ⚠ 用**改名**不是拷贝：改名是原子的，且不会在中途留下半份备份。
/// ⚠ 已经一致（幂等命中全部文件）时**不备份** —— 每点一次就多一份垃圾备份，
/// 那会让「可撤销」变成「攒垃圾」。
fn backup_existing(dest: &Path) -> Result<Option<PathBuf>, String> {
    if !dest.exists() {
        return Ok(None);
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bak = dest.with_file_name(format!("cc-bus.bak-{ts}"));
    std::fs::rename(dest, &bak).map_err(|e| {
        format!(
            "备份 {} → {} 失败：{e}（**没动原目录**）",
            dest.display(),
            bak.display()
        )
    })?;
    Ok(Some(bak))
}

/// 部署到指定 `claude_dir`（可注入，供判据用真目录跑）。
pub fn deploy_into(claude_dir: &Path) -> Result<CcBusDeployReport, String> {
    let dest = fenced_dest(claude_dir)?;

    // 先算幂等：全都一致就**什么都不做**（不备份、不写）。
    let all_same = dest.is_dir()
        && FILES
            .iter()
            .all(|(rel, bytes)| same_content(&dest.join(rel), bytes));
    if all_same {
        return Ok(CcBusDeployReport {
            dest: dest.display().to_string(),
            written: 0,
            unchanged: FILES.len() as u32,
            backup: None,
            warning: None,
        });
    }

    let backup = backup_existing(&dest)?;
    let mut written = 0u32;
    for (rel, bytes) in FILES {
        let p = dest.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("建 {} 失败：{e}", parent.display()))?;
        }
        std::fs::write(&p, bytes).map_err(|e| format!("写 {} 失败：{e}", p.display()))?;
        // `scripts/` 下的都要可执行 —— 装完不能跑等于没装。
        if rel.starts_with("scripts/") {
            crate::platform_fs::make_executable(&p)?;
        }
        written += 1;
    }
    Ok(CcBusDeployReport {
        dest: dest.display().to_string(),
        written,
        unchanged: 0,
        backup: backup.map(|b| b.display().to_string()),
        // 纯函数不探子进程（否则单测会依赖「本机有没有 ccm」）⇒ 这一格由命令层填。
        warning: None,
    })
}

/// `PS2` 的三态。**「没装」「已是最新」「装了但不是这一版」是三件事，不合并。**
///
/// ⚠ 合并任意两个都会骗人：
/// · 把「没装」并进「不是最新」⇒ 用户以为只要点一下更新，其实是第一次装；
/// · 把「不是最新」并进「已装」⇒ 那正是 `P4b` 卡了两天的形态 —— 装着的是旧的，
///   而界面说「已装」，于是没人去点那颗按钮。
///
/// ★ 第三态今天**才**做得出来：它要一个「哪一版才算对」的真相源，而那正是 `U9`②
/// （用户 08-13 裁「**仓内那份为准**」）。⇒ 真相源 = 内嵌的那 17 个字节串。
/// `PS2` 摸底时立的那条判据逐字写着「**加第三态之前先答版本口径**」—— 答了，所以能加。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../src/generated/"))]
pub enum CcBusInstallState {
    /// 落点不存在（或一个内嵌文件都没有）。
    NotInstalled,
    /// 逐文件与内嵌一致。
    UpToDate,
    /// 装着的与内嵌**不一致** —— 带上**差了几个文件**，别只说「不一致」。
    Drifted { differing: u32, missing: u32 },
}

/// 查本机装的是哪一版（**只读**，不写盘）。
pub fn install_state_in(claude_dir: &Path) -> Result<CcBusInstallState, String> {
    let skills = claude_dir.join("skills");
    let dest = skills.join("cc-bus");
    if !dest.is_dir() {
        return Ok(CcBusInstallState::NotInstalled);
    }
    let mut differing = 0u32;
    let mut missing = 0u32;
    for (rel, bytes) in FILES {
        let p = dest.join(rel);
        if !p.exists() {
            missing += 1;
        } else if !same_content(&p, bytes) {
            differing += 1;
        }
    }
    if differing == 0 && missing == 0 {
        Ok(CcBusInstallState::UpToDate)
    } else if missing as usize == FILES.len() {
        // 目录在、但一个内嵌文件都没有 ⇒ 那不是「装了个旧版」，是**根本没装**。
        Ok(CcBusInstallState::NotInstalled)
    } else {
        Ok(CcBusInstallState::Drifted { differing, missing })
    }
}

/// `PS2`：本机 cc-bus 装的是哪一版。**只读**。
#[tauri::command]
pub async fn cc_bus_install_state() -> Result<CcBusInstallState, String> {
    let claude_dir = crate::paths::resolve_claude_dir().ok_or("找不到 claude 目录")?;
    install_state_in(&claude_dir)
}

/// ★★ 装出去的 `cc-spawn` **硬依赖新 `ccm`** —— 装之前先看一眼本机那份够不够新〔08-13〕。
///
/// # 为什么这条非有不可
///
/// `C15` 之后 `cc-spawn` 把命名避让/总线登记/台账全交给了 `ccm`，并在开头做**能力协商**：
/// 缺 `detach` / `tmux-size` / `tmux-base` / `bus-register` 任一条就 `exit 2`。
/// ⇒ **只装 cc-bus、不同步 `ccm`，会当场打断用户的 `cc-spawn`。**
///
/// 08-13 实测这台机器：`~/.local/bin/ccm --ccm-probe` 报的能力是
/// `new,resume,attach,tmux,account,model,cwd,agent,launcher,ccm-sid,print`
/// —— **四条新能力一条都没有**（它停在 B02 之前）。也就是说「装一下 cc-bus」这个动作
/// 今天在这台机器上就会踩中。
///
/// ⚠ **只警告、不拦**：用户完全可能装完 cc-bus 紧接着就同步 `ccm`（顺序本来就该是
/// ccm 先、cc-bus 后，但反过来也只是中间有个窗口）。拦住一个合法流程比漏报更糟。
/// ⚠ 放在**命令层**而不是 `deploy_into` 里：后者是纯函数、被一堆单测直接调，
/// 塞个子进程进去会让那些测试依赖「本机有没有 ccm」——那正是本仓一路在治的环境依赖型假绿。
fn local_ccm_too_old_warning() -> Option<String> {
    // ⚠ **不自己起进程探** —— `ccm_probe::probe_with` 就是「本机 ccm 的能力集探测」，
    //   已经在 `write_site_registry::SPAWNS` 里申报过、有超时、有 `name=ccm` 首行校验
    //   （挡 PATH 里同名但无关的用户脚本）。首版我又写了一份 `Command::new(ccm)`，
    //   **登记表当场逮住**（「会在用户机器上起一个进程，但没人申报」）——
    //   而它把我引到了那个更要紧的事实：**共享原语早就有了，我只是没找**。
    //   ★ 与 08-13 早些时候 `strip_hash_comment_lines` 那次是同一族。
    // ⚠ 走 `probe_local_ccm_uncached` 而不是内层的 `probe_with`：**命令串是它的私事**
    //   （`the_only_production_probe_command_is_the_constant` 钉着「生产侧唯一实参是那个常量」）。
    //   把常量暴露出来给第二个调用方用，等于给那条判据开了个后门。
    let probe = crate::ccm_probe::probe_local_ccm_uncached(std::time::Duration::from_secs(3));
    if !probe.installed {
        return Some("本机探不到 `ccm` —— 装出去的 `cc-spawn` 会报「找不到 ccm」。".into());
    }
    let missing: Vec<&str> = CC_SPAWN_NEEDS
        .iter()
        .copied()
        .filter(|c| !probe.capabilities.iter().any(|x| x == c))
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(format!(
        "本机 ccm 缺能力 {missing:?} ⇒ 装出去的 `cc-spawn` 会以「ccm 版本太旧」退出。\
         请把 `shared/ccm` 同步过去（顺序：ccm 先、cc-bus 后）。"
    ))
}

/// `cc-spawn` 开头那段能力协商要的东西 —— **与 `shared/cc-bus/scripts/cc-spawn` 同一份清单**。
/// 改那边就要改这里（`the_deploy_precheck_lists_what_cc_spawn_negotiates` 钉住两边一致）。
const CC_SPAWN_NEEDS: &[&str] = &["detach", "tmux-size", "tmux-base", "bus-register"];

/// `PS1`：把内嵌的 cc-bus 装到本机 `<claude_dir>/skills/cc-bus/`。
///
/// ★ **用户显式动作**：本命令**只**由设置页那个按钮调用，绝不在启动/后台路径上跑
/// —— 那正是只读铁律那节警告的「一旦允许 monitor 在用户数据上**非显式**写，
/// 用户对『数据源 = 我自己的命令痕迹』的信任就崩了」。
#[tauri::command]
pub async fn deploy_local_cc_bus() -> Result<CcBusDeployReport, String> {
    let claude_dir = crate::paths::resolve_claude_dir().ok_or("找不到 claude 目录")?;
    let warning = local_ccm_too_old_warning();
    if let Some(w) = &warning {
        tracing::warn!("{w}");
    }
    let mut report = deploy_into(&claude_dir)?;
    report.warning = warning;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TmpDir(PathBuf);
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tmpdir(tag: &str) -> TmpDir {
        let p = std::env::temp_dir().join(format!(
            "ps1-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        TmpDir(p)
    }

    /// 内嵌清单必须与仓内那份**逐文件对得上**。
    ///
    /// 少一个 ⇒ 装出去的是**缺件的 skill**，而那比不装更糟：用户以为装好了，
    /// 敲某条命令时才发现没有。
    #[test]
    fn the_embedded_file_list_matches_the_repo() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("shared/cc-bus");
        // ⚠ 走 `guard_core::scan_tree!(&root, &[])` —— **空列表 = 不筛扩展名**，
        //   那个能力是本件 08-13 补进原语的：cc-bus 的脚本（`cc-send` / …）**没有扩展名**，
        //   而原语此前按扩展名筛 ⇒ 想扫这棵树只能自己 `read_dir`，
        //   那又撞 `scanning_guard_registry` 的递减棘轮（「不许把上限调上去让今天好过」）。
        //   ⇒ **缺的是原语的能力，不是纪律的例外。**
        // ⚠ 本条扫的是 `shared/cc-bus/`（另一棵树，不含 `.rs`）⇒ 按构造读不到自己。
        let mut on_disk: Vec<String> = guard_core::scan_tree!(&root, &[])
            .into_iter()
            .map(|(p, _)| {
                p.strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        on_disk.sort();
        let mut embedded: Vec<String> = FILES.iter().map(|(r, _)| r.to_string()).collect();
        embedded.sort();
        assert_eq!(
            embedded, on_disk,
            "内嵌清单与 `shared/cc-bus/` 对不上 —— 加了文件就要加到 `FILES` 里，\
             否则装出去的是个**缺件的** skill"
        );
    }

    /// `PS2`：**三态互相分得开**（这正是本件的正题）。
    #[test]
    fn the_three_install_states_are_distinguishable() {
        let t = tmpdir("state");
        // ① 没装
        assert_eq!(
            install_state_in(&t.0).unwrap(),
            CcBusInstallState::NotInstalled
        );
        // ② 装了、且是这一版
        deploy_into(&t.0).expect("部署");
        assert_eq!(install_state_in(&t.0).unwrap(), CcBusInstallState::UpToDate);
        // ③ 装了、但不是这一版 —— **带着差了几个**，不是一句「不一致」
        let dest = t.0.join("skills/cc-bus");
        std::fs::write(dest.join("SKILL.md"), b"old").unwrap();
        std::fs::remove_file(dest.join("scripts/cc-send")).unwrap();
        assert_eq!(
            install_state_in(&t.0).unwrap(),
            CcBusInstallState::Drifted {
                differing: 1,
                missing: 1
            },
            "「装了旧版」必须带上差异规模 —— 只说「不一致」用户不知道该不该在意"
        );
    }

    /// `PS2`：目录在、但一个内嵌文件都没有 ⇒ 那是**没装**，不是「装了个旧版」。
    ///
    /// ⚠ 这一格是分界：把它判成 `Drifted` 会让界面说「有更新」，
    /// 而用户点下去发现是第一次装 —— 两件事的心理预期完全不同。
    #[test]
    fn an_empty_dir_counts_as_not_installed() {
        let t = tmpdir("empty");
        std::fs::create_dir_all(t.0.join("skills/cc-bus")).unwrap();
        assert_eq!(
            install_state_in(&t.0).unwrap(),
            CcBusInstallState::NotInstalled
        );
    }

    /// ★ 幂等：装两次，第二次**一个字节都不写**、也不留备份。
    /// ★★ **内嵌清单不许漏掉仓里的脚本**〔08-13〕。
    ///
    /// `FILES` 是手写的 `include_bytes!` 清单，而 `shared/cc-bus/scripts/` 是真相源。
    /// 08-13 往那个目录**新建过一个脚本**（`cc-spawned-record`）—— 漏进清单的后果是：
    /// 编译照过、测试照绿，而**装出去的 cc-bus 少一个文件**，
    /// 在用户机器上表现成「新版 cc-spawn 调一个不存在的命令」。
    ///
    /// ⚠ 只钉 `scripts/`：`examples/` 与 `SKILL.md` 是另一族（那边多一个示例文件
    /// 不影响能不能跑），要钉得单独论证。**判据的人群等于它真正证明的那件事。**
    #[test]
    fn every_script_in_the_repo_is_embedded_for_deployment() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("仓根")
            .join("shared/cc-bus/scripts");
        let mut on_disk: Vec<String> = guard_core::shell_scripts(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("仓根"),
        )
        .into_iter()
        .filter(|p| p.replace('\\', "/").contains("shared/cc-bus/scripts/"))
        .filter_map(|p| {
            std::path::Path::new(&p)
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .collect();
        on_disk.sort();
        assert!(
            on_disk.len() >= 10,
            "只列到 {} 个脚本 —— 取法坏了，本断言在空转：{on_disk:?}",
            on_disk.len()
        );
        let embedded: Vec<String> = FILES
            .iter()
            .filter(|(rel, _)| rel.starts_with("scripts/"))
            .map(|(rel, _)| rel.trim_start_matches("scripts/").to_string())
            .collect();
        let missing: Vec<&String> = on_disk.iter().filter(|f| !embedded.contains(f)).collect();
        assert!(
            missing.is_empty(),
            "这些脚本在 `{}` 里，却没进内嵌清单：{missing:?}\n\
             ⇒ 装出去的 cc-bus 会**少这几个文件**，而编译与测试都不会红。\n\
             在 `FILES` 里加一行 `include_bytes!`。",
            dir.display()
        );
        // 反向：清单里不许有仓里已经没有的（幽灵条目会让人以为还装着它）
        let ghosts: Vec<&String> = embedded.iter().filter(|f| !on_disk.contains(f)).collect();
        assert!(ghosts.is_empty(), "内嵌清单里有仓里没有的脚本：{ghosts:?}");
    }

    #[test]
    fn deploying_twice_writes_nothing_the_second_time() {
        let t = tmpdir("idem");
        let first = deploy_into(&t.0).expect("首次部署");
        assert_eq!(first.written as usize, FILES.len());
        assert_eq!(first.unchanged, 0);
        assert_eq!(first.backup, None, "之前没装过，不该有备份");

        let second = deploy_into(&t.0).expect("再次部署");
        assert_eq!(second.written, 0, "幂等：内容一致就不该再写");
        assert_eq!(second.unchanged as usize, FILES.len());
        assert_eq!(
            second.backup, None,
            "一致时**不许**备份 —— 每点一次多一份垃圾备份，那是把「可撤销」变成「攒垃圾」"
        );
    }

    /// ★ 可撤销：内容变了 ⇒ 覆盖前把旧的整个改名留着。
    #[test]
    fn an_overwrite_leaves_a_restorable_backup() {
        let t = tmpdir("bak");
        deploy_into(&t.0).expect("首次");
        let dest = t.0.join("skills/cc-bus");
        // 弄脏一个文件，模拟「已装的是旧版」。
        std::fs::write(dest.join("SKILL.md"), b"old version").unwrap();

        let r = deploy_into(&t.0).expect("覆盖");
        assert!(r.written > 0);
        let bak = r.backup.expect("覆盖必须留备份");
        let bak = Path::new(&bak);
        assert!(bak.is_dir(), "备份目录不在：{}", bak.display());
        assert_eq!(
            std::fs::read(bak.join("SKILL.md")).unwrap(),
            b"old version",
            "备份里必须是**被覆盖前**那份，否则撤不回去"
        );
    }

    /// ★ 围栏：`skills` 是个指向别处的软链 ⇒ **拒收**。
    #[cfg(unix)]
    #[test]
    fn a_symlinked_skills_dir_is_refused() {
        let t = tmpdir("fence");
        let outside = tmpdir("outside");
        std::os::unix::fs::symlink(&outside.0, t.0.join("skills")).unwrap();
        let err = deploy_into(&t.0).expect_err("软链出去必须拒收");
        assert!(err.contains("拒绝"), "{err}");
        assert!(
            !outside.0.join("cc-bus").exists(),
            "已经往围栏外写了 —— 那正是这道围栏要挡的"
        );
    }

    /// ★★ 装前那道能力预检**列的东西必须与 `cc-spawn` 真正协商的一致**〔08-13〕。
    ///
    /// 两边是**同一个事实的两份表达**（Rust 的 `CC_SPAWN_NEEDS` 与 shell 里那行 `for _c in …`）。
    /// 抽不成一份（一个是编译进 monitor 的常量、一个是要部署出去的 shell）⇒ 只能钉一致。
    /// ⚠ 不一致的后果是**预检说没事、装完就坏**：漏列一条，缺那条能力的机器上
    /// 装完 `cc-spawn` 直接 `exit 2`，而部署那步一声不吭地成功了。
    #[test]
    fn the_deploy_precheck_lists_what_cc_spawn_negotiates() {
        let spawn = include_str!("../../shared/cc-bus/scripts/cc-spawn");
        let line = spawn
            .lines()
            .find(|l| l.trim_start().starts_with("for _c in "))
            .expect("`cc-spawn` 里那行能力协商不见了 —— 它是这条判据的另一半");
        // 形如：`for _c in detach tmux-size tmux-base bus-register; do`
        let listed: Vec<&str> = line
            .trim()
            .trim_start_matches("for _c in ")
            .split(';')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        assert!(
            !listed.is_empty(),
            "解析出空清单 —— 抽取器坏了（本条此刻是空转的）"
        );
        assert_eq!(
            listed, CC_SPAWN_NEEDS,
            "装前预检的清单与 `cc-spawn` 真正协商的对不上。\n             \
             左=cc-spawn 实际要的，右=Rust 侧 `CC_SPAWN_NEEDS`。\n             \
             漏列一条 ⇒ 预检说没事、装完 `cc-spawn` 直接 exit 2，而部署那步一声不吭地成功了。"
        );
    }

    /// ★ 三态的**计数**要精确，且「清单外的文件」**故意不算**〔08-13 复核〕。
    ///
    /// # 四态逐个钉
    ///
    /// | 盘上的样子 | 该说 |
    /// |---|---|
    /// | 刚装完 | `UpToDate` |
    /// | 改了 2 个文件 | `Drifted{differing:2, missing:0}` |
    /// | **多一个清单外的文件** | `UpToDate` —— 见下方「为什么故意不算」 |
    /// | 删了 1 个 | `Drifted{differing:0, missing:1}` |
    ///
    /// ⚠ 钉的是**数字**不是「是不是 Drifted」：按钮上写的是「更新（差 N 个文件）」，
    /// N 错了跟状态错了一样骗人。
    ///
    /// # 为什么「多出来的文件」故意不算
    ///
    /// **我们自己的备份就是清单外的文件**（`cc-bus.bak-<ts>` / 08-13 实测用户那份里还有
    /// 四份 07-18 留下的 `scripts.bak-*`）。把它们算成漂移 ⇒ 那颗按钮**永远**写着「更新」，
    /// 而点了也不会变 —— 比不报还坏。
    ///
    /// ⚠ **代价如实写**：哪天有脚本从清单里**删掉**，盘上那份会一直留着而状态仍说
    /// `UpToDate`。这属于 `P2t` 记过的「旧文件永不回收」那一族，它的处置逐字是
    /// 「**真但未发生**」——今天清单只增不减，零实例 ⇒ 不为它造回收机制
    /// （造了也验不出修对没修对）。**这条注释就是那个前提的住址**：清单第一次删东西时，
    /// 来这儿把它改掉。
    #[test]
    fn the_three_states_count_precisely_and_ignore_extra_files() {
        let d = tmpdir("ps2-counts");
        deploy_into(&d.0).unwrap();
        let dest = d.0.join("skills/cc-bus");
        assert_eq!(install_state_in(&d.0).unwrap(), CcBusInstallState::UpToDate);

        std::fs::write(dest.join("SKILL.md"), b"tampered").unwrap();
        std::fs::write(dest.join("scripts/cc-spawn"), b"tampered").unwrap();
        assert_eq!(
            install_state_in(&d.0).unwrap(),
            CcBusInstallState::Drifted {
                differing: 2,
                missing: 0
            },
            "改了两个文件就该报 2 —— 按钮上写的是「更新（差 N 个）」，N 错了跟状态错了一样骗人"
        );

        deploy_into(&d.0).unwrap();
        std::fs::write(dest.join("scripts/cc-legacy-thing"), b"old script").unwrap();
        assert_eq!(
            install_state_in(&d.0).unwrap(),
            CcBusInstallState::UpToDate,
            "清单外的文件**故意不算** —— 我们自己的 .bak 就是清单外的，算了那颗按钮就永远写着「更新」"
        );

        std::fs::remove_file(dest.join("scripts/cc-kill")).unwrap();
        assert_eq!(
            install_state_in(&d.0).unwrap(),
            CcBusInstallState::Drifted {
                differing: 0,
                missing: 1
            },
            "少一个就该报 missing:1，且不该把它算进 differing"
        );
    }

    /// ★ 落点被一个**普通文件**占着（用户手滑 / 旧版留下的残骸）〔08-13 复核〕。
    ///
    /// 两条性质一起钉，因为它们**互相制约**：
    /// · 状态必须说 `NotInstalled` —— 说 `UpToDate` 就是骗人，那颗按钮会写着「已是最新」
    ///   而盘上根本没有 cc-bus（`PS2` 头注逐字：把「装了旧版」说成「已装」⇒ 没人会去点）；
    /// · 部署必须**先备份再替换** —— 直接删掉用户那个文件是不可逆的。
    #[test]
    fn a_regular_file_at_the_destination_is_not_installed_and_gets_backed_up() {
        let d = tmpdir("file-at-dest");
        std::fs::create_dir_all(d.0.join("skills")).unwrap();
        std::fs::write(d.0.join("skills/cc-bus"), b"i am a file, not a dir").unwrap();
        assert_eq!(
            install_state_in(&d.0).expect("查状态"),
            CcBusInstallState::NotInstalled,
            "落点是**文件**时说成「已装/最新」就是骗人 —— 那颗按钮会写着已是最新，而盘上没有 cc-bus"
        );
        let r = deploy_into(&d.0).expect("装");
        assert!(r.written > 0, "该装的一个都没装");
        let bak = r
            .backup
            .expect("覆盖用户那个文件之前**必须**留备份（不可逆动作的底线）");
        assert_eq!(
            std::fs::read_to_string(&bak).expect("备份读得回来"),
            "i am a file, not a dir",
            "备份里不是原来那个文件的内容 —— 那等于没备份"
        );
    }

    /// ★ `skills/` **不可写**（只读目录 / 磁盘满的可控替身）〔08-13 复核〕。
    ///
    /// ⚠ 必须是 `Err` 而不是「装了 0 个文件的 Ok」：后者会让 UI 报「已装到 …（写了 0 个）」，
    /// 又一次「假成功比失败更坏」。
    ///
    /// ⚠⚠ **补门的代价：本条从此在 Windows 上 0 次执行**〔win-compile 09-09〕。
    /// 它靠 `std::os::unix::fs::PermissionsExt` 造「只读目录」这个可控替身，而先前
    /// **漏了门** ⇒ 云端（windows-latest，本仓**唯一**跑 `cargo test` 的平台）上
    /// 编译失败（E0433 ×1 + E0599 ×2）。门照本文件既有口径写成 `#[cfg(unix)]`
    /// （同 `a_symlinked_skills_dir_is_refused` / `deployed_scripts_are_executable`
    /// 那两条 —— 它们用的是同一个平台原语）。
    /// ⚠ 于是「`skills/` 不可写时必须 `Err`、不许返回写了 0 个的 `Ok`」这条性质，
    ///   在 Windows 上**没有任何东西守着**；替身要换成 Windows 的 ACL 才买得回来。
    #[cfg(unix)]
    #[test]
    fn an_unwritable_skills_dir_fails_loudly() {
        use std::os::unix::fs::PermissionsExt;
        let d = tmpdir("ro-skills");
        let skills = d.0.join("skills");
        std::fs::create_dir_all(&skills).unwrap();
        std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o555)).unwrap();
        let r = deploy_into(&d.0);
        // 先恢复权限再断言 —— 否则失败时 `TmpDir::drop` 删不掉，留一地垃圾。
        std::fs::set_permissions(&skills, std::fs::Permissions::from_mode(0o755)).unwrap();
        let e = r.expect_err("`skills/` 不可写时必须报错，不许返回「写了 0 个」的 Ok");
        assert!(
            e.contains("Permission denied") || e.contains("失败"),
            "错误里要留下**原因**，别只说一句「装不了」。实得：{e}"
        );
    }

    /// ★ 装完就能跑：`scripts/` 下的必须可执行。
    #[cfg(unix)]
    #[test]
    fn deployed_scripts_are_executable() {
        use std::os::unix::fs::PermissionsExt;
        let t = tmpdir("exec");
        deploy_into(&t.0).expect("部署");
        let p = t.0.join("skills/cc-bus/scripts/cc-send");
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "装完不可执行等于没装（mode={mode:o}）");
    }
}
