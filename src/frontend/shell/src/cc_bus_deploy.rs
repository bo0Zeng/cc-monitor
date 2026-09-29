//! 〔MIG-3a · 子步 3 · 主会话 09-27 裁 ⑯〕**装 cc-bus 之前那一道「本机 `ccm` 够不够新」的预检** —— monitor 探本机 ccm 的那一格。
//!
//! 装本身（`PS1` 内嵌 → `<claude_dir>/skills/cc-bus/` · 幂等 · 覆盖前整目录备份 · 三态）从前住本模块
//! （`deploy_local_cc_bus` / `cc_bus_install_state`〔散文墓碑〕两条 Tauri 命令），今天是**后端代管的资产**：
//! 判 · 写 · 记都在本机后端（`src/backend/assets/cc_bus_install.rs`，帧命令 `cc-bus-install` / `-state`），
//! 装卸账复用 skill 装记录那一份。留在这里的只有「装出来的 `cc-spawn` 在这台跑不跑得起来」：它问的是**本机那个 `ccm`**
//! （PATH 上那个 / cc-monitor 装下去的那份），探法住 `ccm_probe.rs`（本机后端的引导那一族）。

use crate::copy_table::copy_text;

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
    let st = crate::ccm_probe::local_ccm_entry_status(None);
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

/// 〔MIG-3a〕装 cc-bus 之前（界面点「装」的那一下）问一次：本机 `ccm` 够不够新。`None` = 够新（非 Windows：探过 PATH 上那个）。
/// ⚠ **只警告、不拦**：装本身在本机后端做完，这句话接在成功文案后面显示。
#[tauri::command]
pub async fn cc_bus_ccm_precheck() -> Result<Option<String>, String> {
    let warning = tokio::task::spawn_blocking(local_ccm_too_old_warning)
        .await
        .map_err(|e| format!("spawn_blocking join error: {e}"))?;
    if let Some(w) = &warning {
        tracing::warn!("{w}");
    }
    Ok(warning)
}

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/cc_bus_deploy_tests.rs"]
mod tests;
