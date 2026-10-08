//! 开终端窗口的平台那一半〔余下〕：壳里平台 cfg 的唯一住址（同 [`super::fs`]）。
//! 原住 `launch.rs`，逐字搬来（头注与判据来历一个字没动）：POSIX 那条路（规范化终端出口 · `bash -lic` 的 spawn 计划）·
//! Windows 那条路（`wt.exe` / `powershell.exe` 开窗 · `ssh.exe` 预检）。「跑什么」的校验与 argv 照旧住 `launch.rs::build_local_posix_argv`；
//! 两条 Tauri 命令（`open_terminal_window` · `open_local_terminal`）也留在 `launch.rs`，经本文件的 [`open_local`] · [`ssh_client_missing`] 分平台。

use crate::copy_table::copy_text;

/// 上一条的**下一跳**：`(argv, 终端出口)` → 真正要 spawn 的 `(program, args)`。
///
/// # 为什么它非抽出来不可〔`K-H2b` `D6` 第九拍，08-29〕
///
/// `K-H2b` 那条「中转前缀真的拼上去了」的判据量的是
/// **`history::launch_local` 交给送法的那一串**。送法拿到之后再动手，那条判据看不见 ——
/// 收工前按铁律 15 找「第八层」时实打过两刀，两刀都在这一跳附近：
/// - 在 [`build_local_posix_argv`] 里剥掉 `export ANTHROPIC_BASE_URL=…; ` ⇒ **当时全绿**
///   （隔壁那条透传判据只喂一个不带前缀的 payload ⇒ 输入域 = 1，与 `D6 阻-2` 同族）；
/// - 在 `launch_local_posix_via` 里、`build_local_posix_argv` **之后**剥 ⇒ **也全绿**
///   （那一段当时就地拼 `Command`，没有任何纯函数可打）。
///
/// ⇒ 两刀的修法是同一条：**把这一跳也变成纯函数**，让「载荷逐字节走过去」有东西钉。
///
/// # 🔴 本函数返回之后那 3 行 —— **今天有判据了，两支各一条**〔`D7 阻-1` / `D7 阻-2`，08-29〕
///
/// 本函数返回之后只剩 3 行（`Command::new(program)` · `.args(&args)` ·
/// 以及 `cwd` / `env` / `stdio` / `process_group` 那几个设置）——
/// **在那 3 行里再剥一次前缀，实打 `1229 passed` 全绿**（刀 `Z1c`，08-29）。
///
/// 🔴🔴 **先前这里逐字写着「要买它得真开一个窗口再去看那个进程的 argv」—— 那是假话。**
/// `D7` 照既有那条 `local_posix_spawn_actually_runs_the_command` 的形状写了一条判据量过：
/// 带刀 `Z1c` `1229 passed; 1 failed`（红）· 干净树 `1230 passed; 0 failed`（绿）
/// ⇒ **不用开窗 · 不用真终端 · 不用动 `paths.rs` · 不用 `set_var("HOME")` ·
/// 不用 `--test-threads=1` · 不用 `#[ignore]`。**
/// ★ 「做不到」也是一个断言，而那一句只对**它当时想写的那一种**判据成立
///（同族第二次；上一次是 `InjectFactSources` 头注 ㈠ 那条「要动真实家目录」）。
///
/// ⇒ 今天那 3 行由**两条**判据看着，一支一条（`term` 是这条链的分叉点，两支都买）：
/// - `term = None`（无窗口回落）⇒ `the_spawned_process_really_gets_the_relay_prefix_without_a_terminal`
///   （观测点 = 起出去的那个进程自己看到的 `ANTHROPIC_BASE_URL`）；
/// - `term = Some(...)`（开窗）⇒ `the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix`
///   （观测点 = **假终端**自己收到的 argv；`#!/bin/sh` 脚本打 `"$@"`，**没有窗口会弹出来**）。
///
/// 🔴 **为什么非两条不可**：刀 `L10`（剥前缀**只放在 `term.is_some()` 那一支**）
/// **活过了只买 `None` 那一支的修法** —— 装上走 `None` 的判据之后它仍然 `1229` 全绿、
/// `GATE: OK`，而**生产上装了规范化终端出口的机器走的正是开窗那一支**。
/// ⇒ **这条链上的每一条判据，两支都要有；只买一支买到的是「判据去的那一支」。**
/// **「跑什么」的全部** —— 载荷校验 + argv + 终端包装，一跳到底。
///
/// ★ `launch_local_posix_via` 因此只剩「**怎么起**」（`cwd` / `env` / `stdio` / `process_group`）。
/// 这条边界是**实打逼出来的**：先前那两段（[`build_local_posix_argv`] 与就地拼 `Command`）
/// 中间隔着一跳没有判据，在那里剥掉中转前缀 ⇒ 全绿。
#[cfg(not(windows))]
pub(crate) fn local_posix_spawn_plan(
    cmd: &str,
    term: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let argv = crate::launch::build_local_posix_argv(cmd)?;
    Ok(build_local_posix_spawn(term, &argv))
}

#[cfg(not(windows))]
pub(crate) fn build_local_posix_spawn(
    term: Option<&str>,
    argv: &[String],
) -> (String, Vec<String>) {
    match term {
        // `xdg-terminal-exec` / `x-terminal-emulator` 都收 `-- <argv…>` 之后的命令行。
        Some(t) => (
            t.to_string(),
            std::iter::once("--".to_string())
                .chain(argv.iter().cloned())
                .collect(),
        ),
        None => (argv[0].clone(), argv[1..].to_vec()),
    }
}

/// P5L-Y3：**规范化**终端出口，有序候选。
///
/// # 为什么是这两个，为什么**不探测具体终端**
///
/// 用户逐字〔用 08-12〕：「**纯 bash 的意思是暂时不考虑其他终端, 但是所有的功能都要一样**」。
/// 那句话反对的是**终端专属集成**（像 Windows 那条 `wt.exe` 带专属参数的路）——
/// 而下面这两个恰恰是**不挑**的出口：freedesktop 的 `xdg-terminal-exec` 与 Debian 的
/// `x-terminal-emulator` 都把「用哪个终端」交还给桌面/用户配置。
///
/// ⇒ **绝不在这里列 gnome-terminal / konsole / alacritty 之类的具名终端**。
/// 那才是「挑」，也正是「会在别人机器上错」的来源。
///
/// # ★★ 为什么表里**只有一个**（D 阶段补审 08-12）
///
/// 第一版还放了 `x-terminal-emulator`。D 阶段核参数约定时发现**它们的约定未必一样**：
/// · `xdg-terminal-exec` 的用法串逐字 `[options] [--] [command [arguments ...]]` ⇒ `--` **已核实**；
/// · `x-terminal-emulator` 是 **Debian alternatives 的间接层** —— 本机指向 `ptyxis`
///   （man 逐字 `[-- PROGRAM ARGUMENTS]`，且「In general, you should use `--`」，对得上），
///   但**别的机器上它可能指向 `xterm`/`gnome-terminal`，那些的传统约定是 `-e <command>`**。
///
/// ⇒ 用一个**未核实**的约定去开窗，失败形态正是原判据担心的那个：
/// 终端开出来了、命令没跑，用户看到一个空白窗口且极难归因。
/// **宁可少覆盖一台机器，也不要开一个空白窗口。**
///
/// 要加回来的话，正确形状是「每个出口带自己的参数构造器」，而不是共用一个 `--`。
#[cfg(not(windows))]
pub(crate) const TERMINAL_EXITS: &[&str] = &["xdg-terminal-exec"];

/// P5L-Y1：挑一个存在的终端出口。都不在 ⇒ `None`（调用方诚实降级，见 `launch_local_posix`）。
///
/// **纯函数化的那一半**（`pick_terminal_exit_from`）供判据用 —— 生产这条只是喂它一个真实探针。
#[cfg(not(windows))]
pub(crate) fn pick_terminal_exit() -> Option<&'static str> {
    pick_terminal_exit_from(TERMINAL_EXITS, &|c| which_exists(c))
}

/// 判据入口：候选表与「在不在」的判定都作为参数进来，好在不依赖真实机器的前提下钉顺序。
#[cfg(not(windows))]
pub(crate) fn pick_terminal_exit_from(
    candidates: &[&'static str],
    exists: &dyn Fn(&str) -> bool,
) -> Option<&'static str> {
    candidates.iter().copied().find(|c| exists(c))
}

#[cfg(not(windows))]
fn which_exists(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|d| {
                let p = d.join(cmd);
                std::fs::metadata(&p).map(|m| m.is_file()).unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// L1：在 **POSIX 本机**跑一条命令 —— 不经 ssh、不经 PowerShell。
///
/// 这是 §40「本地 = 不走 ssh 的远端」在传输层的落点：远端那条路是
/// `ssh -t host -- 'bash -lic <cmd>'`，本地就是把 `ssh` 那一跳**去掉**，其余不变。
///
/// 三处设计：
/// - **不开 GUI 终端窗口**。POSIX 上没有「唯一的终端」这种东西：开窗口要先猜用户用哪个
///   终端模拟器，是平白引入一个会在别人机器上错的决定。
///   ⚠⚠ **这条原本还有半句，断言容器一定是 tmux（`ccm --tmux` 自己会建）—— 那是假的**
///   〔audit-0805 F08 / 报告 B-2〕：生产构造出来的是 `cc --resume <sid>`，**不带 `--tmux`**
///   （带 `--tmux` 的别名是 `cct`），而 `shared/ccm` 的 `use_tmux` 默认 0
///   ⇒ 走的是非容器分支 `exec "${argv[@]}"`。加上这里 stdio 全 null，
///   **产出的是一个无 tty、无 tmux 的进程**，不是「留在 tmux 里等 attach」。
///   ★ 同一条推理本仓在别处写对过：`src/doc/IPC-PROTOCOL.md` 逐字
///   「决定性的事实是 `stdin` 不接键盘（`stdin=DEVNULL`）—— 用户敲进去的字会被脚本吃掉」。
///   ★★ **P3t（2026-08-11）已经改了一半，本段随之更新。**
/// 本机起会话的计划（今天在本机后端 `local.rs::plan`）现在**先过 CLI 渲染器**（`render_local_ccm`），渲得出来就带
///   `--tmux` ⇒ ccm 走容器分支、会话留在 tmux 里有真 tty，本函数只负责把它拉起来。
///   上面那句「产出的是一个无 tty、无 tmux 的进程」现在只描述**回落那条路**
///   （渲染器拒了才走的 `build_local_posix_command`），由 〔散文墓碑〕
///   `the_local_resume_payload_has_no_session_container_today` 继续钉； 〔散文墓碑〕
///   正面事实由 `the_rendered_local_command_really_carries_the_container` 钉。 〔散文墓碑〕
///   ⚠ 会话名要由前端传下来（P3t-Y2b；前端问本机后端的 `terminal-name-mint` 铸，问不到 ⇒ `None` ⇒ 走回落）。
///   功能后果（claude 在 `stdin=/dev/null` 下具体怎么表现）红线内**没实测**，是推的。
/// - **脱离 app 的进程组**（`process_group(0)`）+ stdio 全 null：
///   否则子进程会跟着 app 的 Ctrl-C 一起走，也会把 app 的 stdio 占住。
/// - **起一条线程收尸**。`process_group` 不改变父子关系 ⇒ 不 `wait` 就留僵尸。
///   线程随子进程结束而结束（`ccm` 建完会话就返回，是短命进程）。
#[cfg(not(windows))]
pub fn launch_local_posix(cmd: &str, cwd: Option<&str>) -> Result<(), String> {
    launch_local_posix_via(cmd, cwd, pick_terminal_exit())
}

/// P5L：把「用哪个终端出口」做成**入参**。
///
/// ★ 这不是为了好看，是**判据不能在开发者桌面上开真窗口**：
/// 既有的 `local_posix_spawn_actually_runs_the_command` 是一条**真跑**的行为测试，
/// 改之前它直接 spawn `bash`；接上终端出口之后它会**真的弹出一个终端窗口**
/// —— 实测跑了一次（本机 `xdg-terminal-exec` → `ptyxis`）。
/// ⇒ 出口作为参数进来，判据传 `None` 走无窗口那条 —— **或者传一个自己造的假终端脚本**。
///
/// 🔴🔴 **订正〔`D7 阻-2`，08-29〕：先前这里逐字写着「开窗那条路因此没有行为级判据
/// （只有形状判据）」—— 那句话在 `P5L` 那一轮是对的，今天是假话，而且它宽了两格。**
/// - 载荷那一半：`the_local_argv_hands_the_command_through_byte_for_byte_prefix_and_all`
///   喂 `Some("xdg-terminal-exec")` 走的就是开窗那一支（`D7 §A3` 现打）；
/// - spawn 那一半：`the_terminal_we_hand_the_command_to_really_gets_the_relay_prefix`
///   喂一个**假终端**（临时目录里的 `#!/bin/sh` 脚本，把 `"$@"` 写进文件）——
///   `Command::new(program)` 拿绝对路径直接 exec 它，**没有任何窗口会弹出来**。
/// ⇒ **「不能在开发者桌面上开真窗口」≠「开窗那一支买不到」**：前者是真的，后者是
///   把「做不到的那一种做法」说成了「做不到这件事」。
///
/// ⚠ **今天仍然没有的**：真终端（`xdg-terminal-exec` → 用户配的那个模拟器）拿到 argv
/// 之后**会不会照着跑**。那是它自己的约定，本仓测不到 ⇒ 真机验收归 `auto-e2e`。
/// 别把「我们交出去的那一份是对的」读成「窗口真开出来了」。
#[cfg(not(windows))]
pub(crate) fn launch_local_posix_via(
    cmd: &str,
    cwd: Option<&str>,
    term: Option<&str>,
) -> Result<(), String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    use std::process::{Command, Stdio};

    // ★★ `K-H2b` `D6` 第九拍：**「跑什么」整段收成一个纯函数**，本函数只剩「怎么起」。
    //    先前这里是 `build_local_posix_argv` + 就地拼 `Command` 两段，中间那一跳
    //    **没有任何判据** —— 实打：在这两段之间插一个剥掉 `export ANTHROPIC_BASE_URL=…; `
    //    的映射，`1229 passed` 全绿（第八层的下游版本）。
    let (program, args) = local_posix_spawn_plan(cmd, term)?;
    // ★★ P5L-Y1（`U14`〔用 08-12〕「要做，功能必须一样」）：**真开一个终端窗口**。
    //
    // 改之前这里是 `Stdio::null()` 直接 spawn ⇒ 产出一个**无 tty、无窗口**的进程
    //（`P3t` 摸底记下的那条：用户敲进去的字会被脚本吃掉）。
    //
    // ⚠ **载荷一个字不改**：`argv` 原样进终端的参数位。本件只加「怎么开窗」，不碰「开什么」。
    let opened_window = term.is_some();
    if !opened_window {
        // 诚实降级：一个规范化出口都没有 ⇒ 回落到改之前那条无窗口的路，**并说清**。
        // 不静默、也不报一个与真实原因无关的错。
        tracing::warn!(
            "本机没有规范化终端出口（试过 {TERMINAL_EXITS:?}）—— \
             回落到无窗口直起；命令仍会跑，但没有可交互的终端"
        );
    }
    let mut builder = Command::new(&program);
    builder.args(&args);
    // 只有真实存在的目录才作起始目录（与 Windows 那条路同一条纪律）。
    if let Some(d) = cwd.filter(|c| std::path::Path::new(c).is_dir()) {
        builder.current_dir(d);
    }
    // F06b-1d（C9）：backend 把后端路径交给它亲手开的这个窗口 —— 窗口里那次 `ccm resume`
    // 据此去调 `--resolve`（旧 `shared/ccm::resolve_from_backend` 〔散文墓碑〕，`K-R48` 已删；
    // 今天那一问在后端进程内直接答）。local_backend 不在就不设。
    if let Some((k, v)) = crate::local_backend::backend_bin_env_for_window(
        crate::local_backend_host::running_backend_bin(),
    ) {
        builder.env(k, v);
    }
    builder.stdin(Stdio::null()).stdout(Stdio::null());
    // 三条策略：
    // · `Hidden` —— POSIX 上没有「控制台窗口」这回事，这一条在这儿是空的；
    //   **窗口是 `program` 自己开的**（`local_posix_spawn_plan` 选出来的那个终端出口），
    //   不是 `CreateProcess` 的 flag 开的。别把它读成「这条路不开窗」。
    // · `Detached` —— 先前那句 `process_group(0)` 就是这一条：否则终端里 Ctrl-C 的
    //   SIGINT 会打到整个前台进程组，monitor 和用户刚开的会话一起走。
    // · `Null` —— 它的 stdio 本来就全 null（stdin/stdout 在上面），真正说话的是它开出来的
    //   那个终端窗口；接进我们的滚动日志只会把终端的噪声灌进去。
    let mut child = spawn_managed_cmd(
        &mut builder,
        ConsolePolicy::Hidden,
        Lifetime::Detached,
        StderrSink::Null,
    )
    .map_err(|e| copy_text("rsLaunch.local.spawnFailed", &[("e", &e.to_string())]))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    tracing::info!(
        "launch: local posix exec (no ssh){}",
        if opened_window {
            " · 已开终端窗口"
        } else {
            " · 无窗口回落"
        }
    );
    Ok(())
}

/// Windows 宿主上没有这条路：Windows **本地**走 L2 的 PowerShell 分支，不是本函数。
/// 保留同名同签名，是为了让调用点不必自己写 `cfg`（平台差异收在这一层）。
#[cfg(windows)]
pub fn launch_local_posix(_cmd: &str, _cwd: Option<&str>) -> Result<(), String> {
    Err(copy_text("rsLaunch.local.notOnWindows", &[]).into())
}

/// 在新终端窗口跑一条 PowerShell 命令（加载用户 profile、`-NoExit` 保留窗口）。
///
/// Plan A：wt.exe（Windows Terminal）新标签；Plan B：powershell.exe +
/// CREATE_NEW_CONSOLE 独立控制台。`local_cwd`＝Some 且为本地存在目录时作为窗口
/// 起始目录（远端拉起传 None——cwd 是远端路径）。
///
/// # 🔴 **登记：这个函数体分两半 —— 文本那一半买得到而且已经买了，行为那一半买不到**
///
/// 〔`K-H2b` `D8 阻-6` 08-29 立 · `D9 阻-2`/`阻-4` 打回 · `C` 第十轮 09-02 重写〕
///
/// ## 🔴🔴 先记这段登记自己犯过的病（别删，它就是这一格的病理）
///
/// 上一版这 24 行里**同时躺着一条全称和它的反例**：
/// 一句写「本机门禁**在构造上**看不见这个函数体」＋「**唯一的买法**是 CI 上一条 Windows job，
/// 落点不在写区」，另一句写「量文本的判据照样看得见它」。
/// **而被拿去定路由（判成「买不到 / 落点在写区外」）的是假的那一条。**
/// ⇒ 要订正的不是措辞，是**这段登记自相矛盾**。〔`D9` 逮到；PM `§17 裁二` 接〕
///
/// ## ① 量**文本**的那一类：**看得见，而且今天真的有牙**
///
/// `#[cfg(windows)]` 是**类型检查**那一层剔的；而 `guard_core::production_code`
/// 只剥 `#[cfg(test)] mod X { }` 段与注释（整行的与行尾的都剥 —— 行尾那一半是 `K-R3`
/// 09-01 才补上的），**它不剥任何 `cfg`** ⇒ 本函数体在**文本**这一层原样在场。
///
/// 现打（量具 `tests/evidence/K-H2b-C10-cfgwin-visibility.py`，喂的是本工作树的
/// `src/frontend/shell/src/launch.rs`，09-02；量具先拿本文件那条真判据钉的三个等号自检过
/// 复刻对不对得上 —— 对不上它就拒绝出读数）：
///
/// | 构造 | 剥完全文件几处 | 其中在本函数体内 |
/// |---|---|---|
/// | `Command::new(` | 4 | **2** |
/// | `.env(k, v)` | 3 | **2** |
/// | `backend_bin_env_for_window(` | 2 | **1** |
///
/// ⇒ 本文件那条**普通 `#[test]`**
/// `launch_tests.rs::every_terminal_window_backend_opens_carries_the_backend_path`
/// （就在 `cargo` 门里跑）用**三条等号断言**钉着这 5 个构造。
///
/// **实打（`C` 第十轮 刀 `R10M1`，沙箱快道 `cargo test -p monitor --lib`，09-02）**：
/// 把本函数体里 Plan B 那个 `if let Some((k, v)) = backend_env { builder.env(k, v); }`
/// 换成 `let _ = backend_env;`（＝真实缺陷形状「开窗点漏了带 env」；锚点是那三行，**全文命中 1**）
/// ⇒ **`1243 passed; 2 failed`**（同树干净分母 **`1245 passed; 0 failed`**），红名单**恰好两条**：
/// `launch_tests.rs::every_terminal_window_backend_opens_carries_the_backend_path` 与当时那张「谁给 agent 进程定 env」的
/// 两半人群闭表（那张表数的串级几处随起会话只交一行 `ccm …` 一起没了，表也删了；开窗这一跳的 `.env(k, v)` 由上面那条照旧钉着）。
///
/// ⚠ **分母话**：那是**一刀打出来的红名单**，不是「全部判据」的枚举 ——
/// 我没有逐条去数还有几条判据碰得到这个函数体。⇒ 只能写「**我这一刀量到的是这两条**」。
///
/// ## ② 量**行为**的那一类：在本机上**造不出来**，这一半才是真的买不到
///
/// 要驱动本函数体，先得有这个 item；而 Linux 上它在类型检查之前就没了
/// ⇒ 任何「真的调用它、看它干了什么」的判据在本机**不存在**。
/// **它的 `cfg` 就写在下面那一行** —— 这正是 PM 08-29 那条落法要的凭据：
/// **标「平台判不了」要给得出 `cfg`；给不出 `cfg` = 它在本平台编译 = 不是判不了。**
///
/// **实打（刀 `R10M2`，`D8P35` 同形，第九轮 `R9M9` 的复打，09-02）**：在
/// `let encoded = powershell_encoded_command(ps_command);`
/// 之前把当时交进来那一行前面的中转前缀剥掉（锚点是那一行，**全文命中 1**）
/// ⇒ 🔴 **`1245 passed; 0 failed`**，与同树干净分母逐字相同 —— **零感知**。
/// 今天交进这个窗口的只是一行 `ccm …`（中转地址由 `ccm` 在最终 exec 那一处定），那段前缀不在了；
/// 「量行为的判据在本机造不出来」这一半照旧。
///
/// ⇒ 买**这一半**要 CI 上一条 Windows job（或交叉编译 ＋ `cargo test --target`），
/// 落点 `.github/workflows/ci.yml`，**不在 `K-H2b` 的写区** ⇒ 归 PM 立跟进件。
/// ⚠ `tests/scripts/gate.sh` 头注自陈「本地门禁比 CI 严」，而**这一格恰是反过来的那一格**，
/// 别把那句话读成全称。
///
/// ## 🔴 `阻-4`：这里不许再写全称，要写清是哪一类
///
/// 上一版那句「**本机门禁在构造上看不见这个函数体**」是**全称**（分母＝门禁七格里的
/// 全部判据），而它在 ① 那一格上当场为假。今天的写法是**两半各自带读数、判据指名点姓**。
/// ⇒ 这一族今天的口径逐字是：
/// **「带 `#[cfg(windows)]`」蕴含「量行为的判据看不见它」，不蕴含「所有判据都看不见它」。**
///
/// ⚠ 同族第三次（`D8` 表里的 `F3` · `D9` 抓的这一处 · PM `§17 裁一` 自陈的那句）——
/// 三次都是同一个动作：**把一个准确的局部读数放大成全称**。
///
/// ## ⚠ 别把这一条读宽（两处邻居，都不属于本族）
///
/// - 同一条腿上的 [`powershell_encoded_command`]（本文件末尾）是 `cfg(any(windows, test))`：
///   Linux 上 test 档真编译、单测在那儿跑 ⇒ 不属于本族（`D8 阻-2`）；第九轮已给它配了
///   `utils_tests.rs::the_relay_prefix_survives_the_powershell_encoding_byte_for_byte`。
/// - 原 `history.rs` 的 `PRODUCTION_LAUNCH_SINK`〔散文墓碑〕（今天是 `launch.rs::open_local_terminal`） 的 `#[cfg(windows)]` 那一支（`D8` 表里的 `F3`，
///   `D8` **没打**、标着「推的」）第九轮打了、**是红的**；`C` 第十轮刀 `R10M8` 复打，
///   读数一致：**`1244 passed; 1 failed`**，红的是当时那条「中转取点只经接缝」的判据
///   （随起会话只交一行 `ccm …` 一起删了：中转地址今天只在 `ccm` 里定）。
#[cfg(windows)]
pub fn launch_powershell_window(ps_command: &str, local_cwd: Option<&str>) -> Result<(), String> {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    use std::process::Command;

    // 仅当是本地存在的目录才作为起始目录（远端路径/无效路径一律忽略）。
    let start_dir = local_cwd.filter(|c| std::path::Path::new(c).is_dir());

    // -EncodedCommand（base64 of UTF-16LE）：命令含空格 / 括号 / `;`，直接当字符串穿
    // wt.exe（用 `;` 分隔多 tab）会被切碎。编码后只含 [A-Za-z0-9+/=]，任何一层 shell
    // 都不会误解析。详本文件末尾的 powershell_encoded_command。
    let encoded = powershell_encoded_command(ps_command);
    // 不带 `-NoProfile`：必须加载用户 PowerShell profile（cc / __ccm_bind / 代理 env）。
    // -NoExit：命令退出后窗口保留（可读错误、可继续敲）。用系统自带 powershell.exe。
    let ps_args = ["-NoExit", "-EncodedCommand", encoded.as_str()];

    // Plan A：wt.exe 新标签里跑 powershell。
    let mut wt_args: Vec<String> = Vec::new();
    if let Some(d) = start_dir {
        wt_args.push("-d".into());
        wt_args.push(d.into());
    }
    wt_args.push("powershell.exe".into());
    for a in ps_args {
        wt_args.push(a.into());
    }
    // F06b-1d（C9）：同 POSIX 那条 —— 见 `backend_bin_env_for_window` 头注。
    // ⚠ **这一格的诚实边界**：`wt.exe` 多半只是把请求转交给**已在跑的** Windows Terminal 进程，
    //   新标签的环境来自那个进程、不是本次 spawn ⇒ **这里设的 env 未必落得进去**。
    //   下面 Plan B（CREATE_NEW_CONSOLE 直起 powershell）是真正会继承的那条。
    let backend_env = crate::local_backend::backend_bin_env_for_window(
        crate::local_backend_host::running_backend_bin(),
    );
    let mut wt = Command::new("wt.exe");
    wt.args(&wt_args);
    if let Some((k, v)) = backend_env.clone() {
        wt.env(k, v);
    }
    // ★★ 三条策略。**Plan A 与 Plan B 只有第一格不同，而那是照着盘面写的：**
    //   · `Inherit`（本处，Plan A）—— 🔴 `wt.exe` 今天**一个 creation flag 都没带**，
    //     而且它多半只是把请求转交给**已在跑的** Windows Terminal 进程（这一格的诚实边界
    //     上面那段注释已经写过一次）⇒ 真正开窗的不是这次 `CreateProcess`。
    //     ⚠ 刻意**不**顺手改成 `NewVisible`：那是一次**没有任何 Windows 读数支持**的
    //     行为改动，而这条路正是本产品的主用途。本轮只搬「谁来写这三格」，不动盘面。
    //   · `Detached` —— 用户的终端**不该随 monitor 一起死**：关掉界面不等于关掉他正在
    //     敲字的那个会话 ⇒ 绝不能进 `JobKillOnClose` 那个 Job。
    //   · `Inherit` —— 窗口里的话说给用户听，不该灌进我们的滚动日志。
    if spawn_managed_cmd(
        &mut wt,
        ConsolePolicy::Inherit,
        Lifetime::Detached,
        StderrSink::Inherit,
    )
    .is_ok()
    {
        tracing::info!("launch: powershell window via wt.exe");
        return Ok(());
    }

    // Plan B：powershell.exe + CREATE_NEW_CONSOLE，conhost 兜底。
    let mut builder = Command::new("powershell.exe");
    builder.args(ps_args);
    // F06b-1d（C9）：同上。这一格是**真新起的进程**，env 一定继承。
    if let Some((k, v)) = backend_env {
        builder.env(k, v);
    }
    if let Some(d) = start_dir {
        builder.current_dir(d);
    }
    // ★★ 第一格与 Plan A 不同，也是照着盘面写的：先前那句 `creation_flags(CREATE_NEW_CONSOLE)`
    // 逐字就是 `ConsolePolicy::NewVisible`。**全仓唯一一处 `NewVisible`，而且是刻意的** ——
    // 设计稿逐字：「别把 `launch.rs` 的 `CREATE_NEW_CONSOLE` 一起改掉……这正说明为什么要
    // 唯一出口 + 显式声明，而不是全局加一个 flag」。后两格与 Plan A 相同。
    spawn_managed_cmd(
        &mut builder,
        ConsolePolicy::NewVisible,
        Lifetime::Detached,
        StderrSink::Inherit,
    )
    .map_err(|e| format!("spawn powershell failed: {e}"))?;
    tracing::info!("launch: powershell window via fallback console");
    Ok(())
}

/// Windows 本机 ssh.exe 可用性预检：缺 OpenSSH 客户端时 spawn 出的窗口只会报
/// "not recognized"（spawn 本身成功→前端误报成功）——预检失败直接 Err 走剪贴板回退。
#[cfg(windows)]
pub(crate) fn ssh_client_available() -> bool {
    use crate::spawn_managed::{spawn_managed_cmd, ConsolePolicy, Lifetime, StderrSink};
    let mut cmd = std::process::Command::new("where.exe");
    cmd.arg("ssh").stdout(std::process::Stdio::piped());
    // 三条策略：
    // · `Hidden` —— 🔴 **这处先前是裸 `.output()`，也就是没人回答过这个问题**：
    //   monitor 是 `windows_subsystem = "windows"` 的 GUI app，起一个控制台子系统的
    //   `where.exe` 而不带 `CREATE_NO_WINDOW` ⇒ 用户桌面上会闪一个黑框。
    //   这正是「唯一出口」顺手关掉的那一族。
    // · `JobKillOnClose` —— 一次性探测，我们就地等它退；掐断时不许留后代。
    // · `Captured` —— 它的输出**是返回值**（`status.success()` 那一格），不是被丢了。
    spawn_managed_cmd(
        &mut cmd,
        ConsolePolicy::Hidden,
        Lifetime::JobKillOnClose,
        StderrSink::Captured,
    )
    .and_then(|c| c.wait_with_output())
    .map(|o| o.status.success())
    .unwrap_or(false)
}

/// 开终端那一问要不要先说「本机缺 OpenSSH 客户端」：只有 Windows 这一问有意义（它的窗口里跑的是 `ssh.exe`）；别处恒 `false`
/// （POSIX 上开窗只走 [`open_window`]，没有终端出口时回 [`TerminalOpen::NoWindow`]）。
pub fn ssh_client_missing() -> bool {
    #[cfg(windows)]
    {
        !ssh_client_available()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 开窗那一下**成了**的两种结局（真失败走 `Err`，一句人话）。调用方按它判「这是既定设计」，不按哪句话里的字判。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalOpen {
    /// 开了一个终端窗口。
    Opened,
    /// 这台按既定设计不开窗：POSIX 上没有规范化终端出口（不替你挑终端模拟器）。
    NoWindow,
}

/// 开一个终端窗口跑 `command`（「这台电脑能不能开终端窗口、用哪个」只在这一处答）：Windows 开 PowerShell 窗口；
/// POSIX 有规范化终端出口（[`pick_terminal_exit`]）就交它开窗，没有 ⇒ [`TerminalOpen::NoWindow`]
/// （不回落到无窗口直起：要人交互的那一行跑在看不见的地方等于没跑）。
pub fn open_window(command: &str) -> Result<TerminalOpen, String> {
    #[cfg(windows)]
    {
        launch_powershell_window(command, None).map(|()| TerminalOpen::Opened)
    }
    #[cfg(not(windows))]
    {
        open_window_via(command, pick_terminal_exit())
    }
}

/// [`open_window`] 的 POSIX 本体，终端出口是入参（判据传 `None` / 假终端，不在开发者桌面上开真窗口）。
#[cfg(not(windows))]
pub(crate) fn open_window_via(command: &str, term: Option<&str>) -> Result<TerminalOpen, String> {
    match term {
        Some(t) => launch_local_posix_via(command, None, Some(t)).map(|()| TerminalOpen::Opened),
        None => Ok(TerminalOpen::NoWindow),
    }
}

/// 在本机开一个终端窗口跑 `cmd`（工作目录 `cwd`，不在就不设）：POSIX 交用户的终端出口（[`launch_local_posix`]）；
/// Windows 开 PowerShell 窗口（[`launch_powershell_window`]）。
pub fn open_local(cmd: &str, cwd: Option<&str>) -> Result<(), String> {
    #[cfg(not(windows))]
    let out = launch_local_posix(cmd, cwd);
    #[cfg(windows)]
    let out = launch_powershell_window(cmd, cwd);
    out
}

// ── PowerShell `-EncodedCommand` 的编码：只有本文件 Windows 那一臂（`launch_powershell_window`）用；
//    `test` 也编，单测（`utils_tests.rs`）在 Linux 上跑。
/// 把命令字符串编码成 PowerShell `-EncodedCommand` 接受的格式：
/// **UTF-16LE 字节序列的标准 base64**（PowerShell 文档里所谓的 "Unicode" 编码）。
///
/// 用途：安全地把含空格 / 括号 / 引号 / `;` 的 PowerShell 命令透过 `wt.exe` →
/// `powershell.exe` 多层 shell 传递。base64 token 只含 `[A-Za-z0-9+/=]`，**不含**
/// 任何一层 shell 的引号 / 分隔符（wt 用 `;` 分隔多 tab、cmd 用引号配对），因此
/// 不会被任何一层误解析——彻底绕开"多层引号转义地狱"。
#[cfg(any(windows, test))]
pub(crate) fn powershell_encoded_command(cmd: &str) -> String {
    let utf16le: Vec<u8> = cmd.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64_encode(&utf16le)
}

/// 标准 RFC 4648 base64（含 `=` 填充）。仅 `powershell_encoded_command` 使用，
/// 故不引第三方 crate（实现 ~15 行，且 RFC 测试向量守护）。
#[cfg(any(windows, test))]
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TBL[((n >> 18) & 63) as usize] as char);
        out.push(TBL[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TBL[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TBL[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
