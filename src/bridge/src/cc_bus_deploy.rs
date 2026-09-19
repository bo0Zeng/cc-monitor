//! `PS1`：把仓内那份 cc-bus 部署到 `<claude_dir>/skills/cc-bus/`。
//!
//! # 这条路本来是被**只读铁律**堵死的
//!
//! `src/doc/INVARIANTS.md` 开头那条：「`monitor` 对 `<claude_dir>/…` **只读**」，后附**穷举**的
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
//! （那份头注逐字：「故直接 `include_bytes!` 内嵌」）。⇒ 单一事实源仍是 `src/shared/cc-bus/`，
//! 编译期把它固化进二进制。
//!
//! # `U9`② 顺带有答案了（实测，不是投票）
//!
//! 那一问是「仓内那份 vs `~/.claude/skills/` 那份，拿哪份当真相源」。08-13 实测两份的差异：
//! **只有 2 个文件**（`scripts/cc-spawn` 与 `SKILL.md`），而它们正是 `P4b` 改的那两个
//! ⇒ **仓内那份 = 已装那份 + `P4b` 的修复**，是严格更新的一侧，且它 git 管着、判据钉着。
//! ⇒ **仓内那份当真相源**，本模块把它推下去；反向（把已装那份 import 回仓）会把 `P4b` 撤销。

use std::path::{Path, PathBuf};

/// 内嵌的那 17 个文件。**单一事实源 = `src/shared/cc-bus/`**，这里只是编译期固化。
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
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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
///
/// # ★★ 它有一个 **Windows 对侧**（紧接在下面）〔win-compile 09-09〕
///
/// 探测那一侧（`crate::ccm_probe::probe_local_ccm_uncached`）**整条**都带
/// `#[cfg(not(windows))]` —— 它跑的是 `bash -lic`，是 POSIX 专有的原语。
/// 先前本函数**没有对应的门** ⇒ Windows 上 `monitor` 的 lib 直接编不过（E0425）。
/// ⇒ 两侧各写各的。**本函数的函数体一个字节没动**，非 Windows 上的行为按构造逐字节不变。
#[cfg(not(windows))]
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

/// 上一条的 **Windows 对侧** —— 它**不做**这一格预检，而且**把「没做」说出来**〔win-compile 09-09〕。
///
/// # 为什么不是「加个 cfg 静默 `return None`」
///
/// `None` 在这条链上有一个**确定**的含义：**探过了、够新**（调用方据此不给用户任何提示）。
/// Windows 上根本没探 —— 返回 `None` 就是把「查不了」读成「没问题」。
///
/// 本仓对这件事有一条反复出现的明纪律，最短的落点是 `sftp.rs` 里那个四值枚举
/// `TargetBinary`：它的「问不出来」那一态的注释逐字写着「**不许读成上面任何一个**」，
/// 而那条注释同时说明了它为什么**不是 `bool`**。同一句话在 `CcBusDeployReport`
/// 那个字段的头注里叫「**假成功比失败更坏**」——用户点一次按钮换来的结果，
/// 没有提示他就当成纯成功，而日志他不会去翻。
///
/// # 落成什么形状
///
/// 装**照做**（`deploy_into` 与平台无关，那 17 个文件照样落盘、照样幂等、照样留备份），
/// 但返回一句话，由 `deploy_local_cc_bus` 原样填进报告的 `warning`，
/// 前端 `settings/cc-bus-section.ts` 把它接在成功文案后面显示。
/// ⇒ 用户在 Windows 上读到的是「**这一格没做预检**」，而不是什么都没有。
///
/// ⚠ 它**不是错误**（与非 Windows 那条同一条纪律）：装本身做完了，命令仍回 `Ok`。
/// ⚠ 清单从 [`CC_SPAWN_NEEDS`] **现取**，绝不在这里抄一份字面量 ——
///   `the_deploy_precheck_lists_what_cc_spawn_negotiates` 钉的是「那个常量与 `cc-spawn`
///   真正协商的一致」，抄一份就等于在它看不见的地方开了第二处。
/// ⚠ **诚实边界**：这句话说的是「**本机**没有可查的 ccm」，不是「Windows 上没有 ccm 这种东西」。
///   今天 monitor 在 Windows 上确实没有任何 ccm 探测形态（`probe_local_ccm` 那一族整族
///   带 `#[cfg(not(windows))]`）；哪天有了，这条该换成真探测，而不是继续报「没做」。
#[cfg(windows)]
fn local_ccm_too_old_warning() -> Option<String> {
    Some(format!(
        "本机没有可查的 `ccm` —— **这一格没做预检**：能力探测要跑 `bash -lic`，\
         这台机器上没有那条路，装出去的 `cc-spawn` 够不够新**问不出来**。\
         ⚠ 「没有警告」在这里**不等于「没问题」**：`cc-spawn` 开头仍会协商 {CC_SPAWN_NEEDS:?}，\
         缺一条就以「ccm 版本太旧」退出。请自行确认 `shared/ccm` 已同步到位\
         （顺序：ccm 先、cc-bus 后）。"
    ))
}

/// `cc-spawn` 开头那段能力协商要的东西 —— **与 `src/shared/cc-bus/scripts/cc-spawn` 同一份清单**。
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
#[path = "../../../tests/bridge/cc_bus_deploy_tests.rs"]
mod tests;
