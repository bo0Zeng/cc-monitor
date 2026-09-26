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

use crate::copy_table::copy_text;
use std::path::{Path, PathBuf};

/// 内嵌的那些文件（条数以 `FILES.len()` 为准，判据对拍，不在散文里写数）。**单一事实源 = `src/shared/cc-bus/`**，这里只是编译期固化。
///
/// ⚠ 加文件要同时加到这里 —— 判据 `the_embedded_file_list_matches_the_repo` 会对拍，
/// 少一个当场红（否则装出去的是个**缺件的** skill，而那比不装更糟）。
const FILES: &[(&str, &[u8])] = &[
    ("SKILL.md", include_bytes!("../../shared/cc-bus/SKILL.md")),
    (
        "examples/cc-busd.service",
        include_bytes!("../../shared/cc-bus/examples/cc-busd.service"),
    ),
    // 〔保活 09-24〕设计 95 §3bis：保活是 cc-bus 的**调用方**，所以它住 `examples/`，
    // 不进 `scripts/`（那里的命令面条数有判据钉着，而「保活不是 cc-bus 的功能」本来就该在结构上看得见）。
    (
        "examples/cc-keepalive",
        include_bytes!("../../shared/cc-bus/examples/cc-keepalive"),
    ),
    (
        "examples/config",
        include_bytes!("../../shared/cc-bus/examples/config"),
    ),
    // 〔kinds 09-24〕设计 95 §2.2「部署要跟上」：**带注释的默认 kinds 表**随包落盘。
    // ⚠ 它落在 `<claude_dir>/skills/cc-bus/examples/`，**不是** `~/.cc-bus/kinds.tsv` ——
    //   只读铁律第 7 条例外只放行 `skills/cc-bus` 这一个落点（`fenced_dest`）。
    //   脚本按「`~/.cc-bus/kinds.tsv` → 随包这一份 → 内置 msg」的顺序找，所以随包这份**就是生效的默认**，
    //   用户要改时复制到 `~/.cc-bus/` 再改（`cc-bus-install.sh` 顺手放一份 `.example`）。
    (
        "examples/kinds.tsv",
        include_bytes!("../../shared/cc-bus/examples/kinds.tsv"),
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
        "scripts/cc-bus-adapt.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-adapt.sh"),
    ),
    (
        "scripts/cc-bus-adapt-posix.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-adapt-posix.sh"),
    ),
    (
        "scripts/cc-bus-adapt-windows.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-adapt-windows.sh"),
    ),
    (
        "scripts/cc-bus-agent-claude.sh",
        include_bytes!("../../shared/cc-bus/scripts/cc-bus-agent-claude.sh"),
    ),
    (
        "scripts/cc-commit",
        include_bytes!("../../shared/cc-bus/scripts/cc-commit"),
    ),
    (
        "scripts/cc-peek",
        include_bytes!("../../shared/cc-bus/scripts/cc-peek"),
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
    // 〔SH1 · V136〕只读看收件箱尾巴（后端 `bus-inbox` 转调它）。
    (
        "scripts/cc-log",
        include_bytes!("../../shared/cc-bus/scripts/cc-log"),
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
/// 〔RW1 · 第四波 09-24〕它从此**只读**：`<claude_dir>/skills` 若已在，解到底必须仍在 `claude_dir` 底下；
/// 还不在 ⇒ 由后端在写的时候逐级建（`files-put` 的 `parents`，每一级各过后端那道围栏）。
/// 从前这里先 `mkdir -p` 再判 —— 那一步写已经随「用户文件只经后端写」搬走了。
/// ⚠ 后端那道围栏（根 = `claude_dir`，解完链接不许跑出根）是**同一件事的第二道**：这里判的是「人话报错」，
///   后端判的是「写不出去」。
fn fenced_dest(claude_dir: &Path) -> Result<PathBuf, String> {
    let skills = claude_dir.join("skills");
    if skills.exists() {
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
    }
    Ok(skills.join("cc-bus"))
}

/// 落点相对 `claude_dir` 的那一段（交给后端当 `rel`）。
fn dest_rel(rel: &str) -> String {
    format!("skills/cc-bus/{rel}")
}

/// ★ **幂等**：逐文件比内容，一致就跳过（经门读：读的就是后端要写的那一份）。
async fn same_content(
    door: &impl crate::user_files::Door,
    root: &str,
    rel: &str,
    bytes: &[u8],
) -> Result<bool, String> {
    Ok(door
        .peek(root, &dest_rel(rel))
        .await?
        .text
        .as_deref()
        .map(str::as_bytes)
        == Some(bytes))
}

/// ★ **可撤销**：覆盖前把整个目录改名成 `cc-bus.bak-<时间戳>`（经后端的 `files-rename`）。
///
/// ⚠ 用**改名**不是拷贝：改名是原子的，且不会在中途留下半份备份。
/// ⚠ 已经一致（幂等命中全部文件）时**不备份** —— 每点一次就多一份垃圾备份，
/// 那会让「可撤销」变成「攒垃圾」。
async fn backup_existing(
    door: &impl crate::user_files::Door,
    root: &str,
    dest: &Path,
) -> Result<Option<PathBuf>, String> {
    if door.stat_kind(&dest.display().to_string()).await?.is_none() {
        return Ok(None);
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bak_name = format!("cc-bus.bak-{ts}");
    door.rename(root, "skills/cc-bus", &format!("skills/{bak_name}"))
        .await
        .map_err(|e| {
            format!(
                "备份 {} → {bak_name} 失败：{e}（**没动原目录**）",
                dest.display()
            )
        })?;
    Ok(Some(dest.with_file_name(bak_name)))
}

/// 部署到指定 `claude_dir`（可注入，供判据用替身门跑）。
///
/// 〔RW1 · 第四波 09-24〕用户裁「只允许后端的文件管理部分写文件」也管本机 ⇒ 本机这一趟
/// **一个字节都不在本进程落**：读（幂等判定）· 改名（备份）· 写（`parents` 逐级建）· 改权限（可执行位）
/// 全经本机后端的文件管理那一面。
pub async fn deploy_into(
    door: &impl crate::user_files::Door,
    claude_dir: &Path,
) -> Result<CcBusDeployReport, String> {
    let dest = fenced_dest(claude_dir)?;
    let root = claude_dir.display().to_string();

    // 先算幂等：全都一致就**什么都不做**（不备份、不写）。
    let mut all_same = door.stat_kind(&dest.display().to_string()).await? == Some("dir".into());
    if all_same {
        for (rel, bytes) in FILES {
            if !same_content(door, &root, rel, bytes).await? {
                all_same = false;
                break;
            }
        }
    }
    if all_same {
        return Ok(CcBusDeployReport {
            dest: dest.display().to_string(),
            written: 0,
            unchanged: FILES.len() as u32,
            backup: None,
            warning: None,
        });
    }

    let backup = backup_existing(door, &root, &dest).await?;
    let mut written = 0u32;
    for (rel, bytes) in FILES {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| format!("内嵌的 {rel} 不是 UTF-8 —— 这一件装不了（后端写面只收文本）"))?;
        door.put(&root, &dest_rel(rel), text, None, false, true)
            .await
            .map_err(crate::user_files::Refused::said)?;
        // `scripts/` 下的都要可执行 —— 装完不能跑等于没装。
        // 〔保活 09-24〕`examples/` 里带 shebang 的那一份（`cc-keepalive`，给 cron 直接调）同理；
        //   按「字节以 `#!` 开头」认，不按文件名列 —— 列名单会在下一个示例脚本进来时漏。
        // ⚠ Windows 上没有可执行位（从前 `platform_fs::make_executable` 在那边就是空操作），
        //   后端的改权限在那边如实回失败 ⇒ 这一步只在 unix 上发。
        if !cfg!(windows) && (rel.starts_with("scripts/") || bytes.starts_with(b"#!")) {
            door.chmod(&root, &dest_rel(rel), 0o755).await?;
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
/// （用户 08-13 裁「**仓内那份为准**」）。⇒ 真相源 = 内嵌的那一组字节串（`FILES`）。
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
        } else if std::fs::read(&p).map(|got| got != *bytes).unwrap_or(true) {
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
        return Some(copy_text("rsCcBusDeploy.ccm.notFound", &[]).into());
    }
    let missing: Vec<&str> = CC_SPAWN_NEEDS
        .iter()
        .copied()
        .filter(|c| !probe.capabilities.iter().any(|x| x == c))
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(copy_text("rsCcBusDeploy.ccm.tooOld", &[]))
}

/// 上一条的 **Windows 对侧** —— 它**真探**，而且探的是**哪一份**说得清〔ccbus-win 09-24〕。
///
/// # 🪦 上一版：「它**不做**这一格预检，而且**把『没做』说出来**」〔win-compile 09-09〕〔散文墓碑〕
///
/// 那一版回的是一句固定的「这一格没做预检」。它的头注自己写着解锁条件，逐字：
/// 「今天 monitor 在 Windows 上确实没有任何 ccm 探测形态（`probe_local_ccm` 那一族整族
/// 带 `#[cfg(not(windows))]`）；**哪天有了，这条该换成真探测，而不是继续报『没做』**」。
/// ⇒ **那一天是 `K-R69`**：`ccm_probe::probe_binary_uncached` 直接问一个二进制
/// `<bin> --ccm-probe`（不经 shell、跨平台），`ccm_probe::local_ccm_entry_status` 用它问
/// **cc-monitor 自己装下去的那一份**（`~/.cc-monitor/bin/<本机 ccm 入口名>`）。
/// 本条从此复用那一处，**不另起进程**（起进程的登记住在 `ccm_probe.rs`）。
///
/// # 它买到 / 买不到什么
///
/// ✅ **买到**：「cc-monitor 装的那份 `ccm` 是不是**这一版**」—— 那份自报的 `build=`
///   与本 monitor 编进来的 `BACKEND_BUILD_ID` 逐字比；外加 [`CC_SPAWN_NEEDS`] 四条能力在不在。
/// ❌ **买不到**：「你 PATH 上那个 `ccm` 是谁」—— `cc-spawn` 在 bash 里调的是 PATH 上的那个，
///   而问 PATH 要走登录 shell（`bash -lic`），Windows 上没有那条路（`ccm_probe::probe_path_ccm`
///   的 Windows 臂同样回 `None`，`KU22` 待决）。⇒ **哪怕全对也回 `Some`**：
///   把「查的是那一份」说出来 —— `None` 在这条链上的含义是「**探过了 PATH 上那个、够新**」，
///   Windows 上从来没探过它，回 `None` 就是把「查不了」读成「没问题」（`TargetBinary` 那条纪律）。
/// ❌ **买不到**：Windows 上 `cc-spawn` 本身跑不跑得起来 —— 它要 tmux，而 cc-bus 的 Windows
///   那一侧投递通道今天是显式的 rc=13（`cc-bus-adapt-windows.sh`）。本条只答「版本对不对」。
///
/// ⚠ 它**不是错误**（与非 Windows 那条同一条纪律）：装本身做完了，命令仍回 `Ok`。
#[cfg(windows)]
fn local_ccm_too_old_warning() -> Option<String> {
    let st = crate::ccm_probe::local_ccm_entry_status();
    Some(windows_ccm_precheck(
        st.entry.as_deref().map(|e| (e, &st.ours)),
        env!("BACKEND_BUILD_ID"),
    ))
}

/// Windows 那条预检的**话怎么说** —— 纯函数（探测结果与期望的 build 都是入参）。
///
/// 抽成纯函数是为了让**五种情形在 Linux 上也判得到**：上一版的判据带 `#[cfg(windows)]`，
/// 本仓唯一跑它的地方是云端 windows-latest；本条的判据在哪台机器上都跑。
///
/// `ours`：`None` = cc-monitor 那份 `ccm` 还没装下来；`Some((住址, 名片))` = 装了并问过一次。
/// 返回值**总是一句话**（理由见 [`local_ccm_too_old_warning`] 的 Windows 臂）。
///
/// ⚠ 门控是 `any(windows, test)` 而不是 `#[allow(dead_code)]`：非 Windows 的生产构建里它
///   确实没有调用方，而 `allow` 会把将来真正的死代码一并盖住（`history.rs` 那条同一个取法）。
#[cfg(any(windows, test))]
pub(crate) fn windows_ccm_precheck(
    ours: Option<(&str, &crate::ccm_probe::CcmProbeResult)>,
    want_build: &str,
) -> String {
    // 〔CP1 台账 09-24〕这几句对用户可见（设置页 cc-bus 区）：不上 markdown 星号、不上 `{:?}` 数组形、
    //   不上工单号、不说内部推理 —— 台账那几行因此从「改」换成「保留」。
    let join = |xs: &[&str]| xs.join(&copy_text("rsCcBusDeploy.win.listSep", &[]));
    let needs = join(CC_SPAWN_NEEDS);
    let path_caveat = &copy_text("rsCcBusDeploy.win.pathCaveat", &[]);
    let Some((at, card)) = ours else {
        return copy_text(
            "rsCcBusDeploy.win.notInstalled",
            &[("needs", &needs.to_string())],
        );
    };
    if !card.installed {
        return copy_text(
            "rsCcBusDeploy.win.noVersion",
            &[("at", &at.to_string()), ("needs", &needs.to_string())],
        );
    }
    let missing: Vec<&str> = CC_SPAWN_NEEDS
        .iter()
        .copied()
        .filter(|c| !card.capabilities.iter().any(|x| x == c))
        .collect();
    let unknown = copy_text("rsCcBusDeploy.win.unknown", &[]);
    let build = card.build.as_deref().unwrap_or(&unknown);
    if build != want_build {
        let lack = if missing.is_empty() {
            String::new()
        } else {
            copy_text(
                "rsCcBusDeploy.win.lack",
                &[("list", &(join(&missing)).to_string())],
            )
        };
        return copy_text(
            "rsCcBusDeploy.win.stale",
            &[
                ("at", &at.to_string()),
                ("build", &build.to_string()),
                ("wantBuild", &want_build.to_string()),
                ("lack", &lack.to_string()),
                ("pathCaveat", &path_caveat.to_string()),
            ],
        );
    }
    if !missing.is_empty() {
        return copy_text(
            "rsCcBusDeploy.win.missing",
            &[
                ("at", &at.to_string()),
                ("list", &(join(&missing)).to_string()),
                ("pathCaveat", &path_caveat.to_string()),
            ],
        );
    }
    copy_text(
        "rsCcBusDeploy.win.ok",
        &[
            ("at", &at.to_string()),
            ("needs", &needs.to_string()),
            ("pathCaveat", &path_caveat.to_string()),
        ],
    )
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
    // 〔RW1〕落盘经本机后端的文件管理那一面（`user_files::BackendDoor`），本进程不写。
    let door = crate::user_files::BackendDoor::new(crate::origin::Origin::local());
    let mut report = deploy_into(&door, &claude_dir).await?;
    report.warning = warning;
    Ok(report)
}

#[cfg(test)]
#[path = "../../../tests/bridge/backend/control/cc_bus_deploy_tests.rs"]
mod tests;
