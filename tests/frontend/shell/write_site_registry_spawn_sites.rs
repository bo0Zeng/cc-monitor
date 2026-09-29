use std::path::{Path, PathBuf};

/// `(文件, 函数, 起的是什么, 为什么必须起进程, 三条策略)`。**默认拒绝**：人群从源码派生。
///
/// # ★ 第五列是 `15 §5.1 A3` 同拍加的：**每个落点选了哪三个策略**
///
/// 格式两种，二选一：
/// - `"<Console> · <Lifetime> · <Stderr>"` —— 三个枚举变体名，逐字。
///   带 `（宿主注入：<构造器>）` 后缀的，说明这一处**自己不写策略**（它在 `backend/`，
///   平台原语进不去），答案由宿主那侧那个具名构造器给。
/// - `"—— …"` 开头 —— **这一处刻意不进那个出口**，后面写清为什么（今天三处：
///   `build.rs` 两处是构建期，加出口本身那一处）。
///
/// ⚠ 它**不是散文**：[`the_three_policies_each_site_declares_match_the_code`]
/// 把这一列与盘面对拍（变体名从 `spawn_managed.rs` 的枚举派生，写错一个字母就红）。
/// ⚠ **射程边界，写下来**：对拍的粒度是**文件**，不是「这一行」——
/// 同一个文件里两个落点互换策略，本条看不出来（`launch.rs` 今天就有三个落点、
/// 两种三元组）。它接得住的是「某个落点改了策略而账本没跟」「策略名被改掉」
/// 这两族，接不住同文件内的对调。别把它读成更强的东西。
const SPAWNS: &[(&str, &str, &str, &str, &str)] = &[
    // ── 构建期（`build.rs`）：**每次 `cargo build`／`cargo check` 都在开发者机器上真跑**。
    // 08-08 并进本表之前，它整个在所有登记表的扫描面之外。
    ("build.rs", "check_vendor_freshness", "`git`（读 vendor 目录的最后一次改动）",
     "vendor 新鲜度自检：只读地问 git，参数是仓内固定路径、不吃用户输入。\
          它必须起进程是因为「vendor 目录相对上游有没有漂」这件事只有 git 知道",
     "—— **不进那个出口**：它跑在构建期、在开发者机器上，`00 §1.5.2` 那三个问题对它一个都不成立（没有 GUI 宿主可弹窗、没有 monitor 进程可随、错误就该打到 `cargo` 的 stderr 上）"),
    ("build.rs", "check_acct_iso_vendor_freshness", "`sh -c`（算 vendored 脚本的指纹）",
     "同上的第二半，对 `cc-acct-iso` 那份 vendor 算摘要；命令串是常量，\
          唯一的变量是仓内路径。⚠ 它跑在**构建期**，比运行时的任何一处都早",
     "—— 同上：构建期，刻意不进那个出口"),
    ("ccm_probe.rs", "probe_with", "`bash -lic <常量探测串>`",
     "P3t-Y2：本机 ccm 的**能力集**探测。命令串是 `CCM_PROBE_CMD` —— 与远端那条**逐字同一个常量**，\
          零插值。必须起进程的理由是「本机装没装 ccm、装的是哪一版」只有这台机器自己知道；\
          而 shell 里的 `command -v` 那种探法只答得了「在不在」，答不了能力集，\
          拿它当依据渲染就会在老 ccm 上渲出带未知 flag 的命令且**已经没有回落可走**（fail-open）。\
          `bash -lic` 那层与远端同语义（PATH/别名/函数按交互终端解析），`ccm` 正是靠它才被找到。\
          ⚠ **命令是参数**（D 阶段补审为了能测「挂住」而开）——生产侧唯一实参是 `CCM_PROBE_CMD`，\
          由 `the_only_production_probe_command_is_the_constant` 按源码钉住，别读成「这里能跑任意命令」
          ★ 三条策略为什么是这三格：探针绝不该在用户桌面上闪窗口；`-lic` 起出来的整棵树超时时要一起收（只杀 `bash` 漏得掉用户 rc 起的东西）；有用的字节只在 stdout 上。",
     "Hidden · JobKillOnClose · Null"),
    // ── 🔴 `K-R69`：**直接问我们自己放下去的那一份**「你是谁」。
    ("ccm_probe.rs", "probe_binary_uncached", "`<我们那份 ccm> --ccm-probe`（不经 shell）",
     "`KR69D2`：本机那条 `ccm` 入口的**身份**。必须起进程的理由与上一行不同 ——\
          上一行问的是「你 PATH 上那个是谁」（非走登录 shell 不可，PATH 就是 rc 决定的），\
          这一行问的是「**我们放下去的那一份是谁**」，路径我们自己知道 ⇒ 一个 shell 都不起，\
          也就不吃用户 rc 的任何影响。两张名片一比才判得出「你 PATH 上那个是旧的」，\
          而**只比路径认不出同名不同物** —— 那正是本件的题面（用户 `~/.local/bin/ccm` 那份旧 bash）。\
          ⚠ 参数是**路径**，来自 `local_backend::local_ccm_entry_name()` 拼出来的落点，\
          不吃任何用户输入；等待 / 读 / 解析与上一行**共用** `probe_spawned`（抄第二份必漂）
          ★ 三条策略为什么是这三格：同上一行逐字，只有 `Hidden` 那格更重：Windows 上问的是我们自己放下去的 `ccm.exe`（控制台子系统），不带 flag 就是每问一次身份闪一次黑框。",
     "Hidden · JobKillOnClose · Null"),
    // 〔SH1 · V136〕驾驶舱那条本机 shell 读 `local_shell_read`〔散文墓碑〕那一行出表了（`K-R112` 写下的出表条件兑现）：
    //   驾驶舱读名册 / 读收件箱都改走后端（`bus-state` / `bus-inbox`，转调 cc-bus 的机器可读读命令），monitor 本机不再起 `bash`。
    // 〔SR1a · 2026-09-24〕`dial_host.rs::open` 那一行**摘了**：它不再起 `--dial` 拨号代理子进程 ——
    //   拨号挪进本机那一个常驻后端，经流上的链路做（`link_mux.rs`）。〔墓碑 —— 那一行的要点：
    //   「界面拿一条 SSH 链路的唯一入口 … 起的是 `cc-monitor-backend` … argv 只有一个常量 flag，
    //   主机名 / 用户名 / 私钥路径走环境变量」。〕
    // 🔴 **`K-R104`（09-13）：`account_usage.rs` 那一行（本机执行面）删了。**
    //    那个函数不存在了 —— 本机用量探针不再在界面进程里 `sh -c <载荷>`，
    //    它与远端那条**是同一条路**：往那台机器的后端发几条帧命令。
    //    ⇒ 界面进程这一侧起进程的面**净少一处**（这是好事，也是本表存在的理由）。
    ("launch.rs", "launch_local_posix_via", "用户配置的终端 argv[0]",
     "在用户的终端里起会话 —— 承接 C13「最后那次 exec 在用户终端里」，这是本产品的主用途
          ★ 三条策略为什么是这三格：`Detached` 就是先前那句 `process_group(0)`。`Hidden` 在 POSIX 上是空的 —— **窗口是终端出口自己开的**，不是 `CreateProcess` 开的，别读成「这条路不开窗」。",
     "Hidden · Detached · Null"),
    ("launch.rs", "launch_powershell_window", "`wt.exe` / `powershell.exe`",
     "Windows 侧同上；两个名字都是常量，不吃用户输入
          ★ 三条策略为什么是这三格：🔴 **全仓唯一一处 `NewVisible`**（Plan B 那一跳），而且是刻意的（`00 §1.5.2` 逐字点名「别把它一起改掉」）。\
          `Detached`：用户的终端不该随 monitor 一起死，关掉界面 ≠ 关掉他正在敲字的会话。\
          ⚠ **这个函数里有两跳，第一格不一样**：Plan A（`wt.exe`）是 `Inherit · Detached · Inherit` —— \
          它今天一个 creation flag 都没带，而且多半只是把请求转交给已在跑的 Windows Terminal 进程。\
          本轮**照盘面写、不顺手改**（没有任何 Windows 读数支持那个改动，而这是主用途那条路）。\
          第五列记的是 Plan B 那一跳；本表的对拍粒度是文件，盖不到同函数内两跳的差别 —— 如实记。",
     "NewVisible · Detached · Inherit"),
    // 🔴 〔`K-R135` / `R88` 09-15〕**这一行就是 `R88` 放行的那一行，只加了这一行。**
    //
    // `R87` 原本写着「本件不需要起进程」——**那是假前提，`R88` 已推翻**：`§0b` 明禁拿
    // `$env:PATH` 判用户级 PATH（它是机器级＋用户级拼起来的）⇒ **状态 / 加 / 撤三样
    // 都得碰 `HKCU\Environment`**，而「现在状态」那一格**根本不可能靠用户自己去跑**。
    //
    // **为什么必须起进程**：Rust 侧够到那一档只有两条路 —— 起 PowerShell，或直接读写
    // 注册表。后者现打编不过（`windows::Win32::System::Registry` 是 `E0432`，全仓
    // `Cargo.toml` 开这个 feature **0 处**、现有注册表调用 **0 处**），而且 `R88` 按份量
    // 否掉了它：① 直接写注册表会造出**第二份 PATH 编辑实现**，而走 Tauri 命令的理由
    // 本身就是「别给同一族动作另起一条路」；② `[Environment]::SetEnvironmentVariable`
    // **自带 `WM_SETTINGCHANGE` 广播**，自己写注册表就得自己记得广播，忘了就是
    // 「改了、新终端看不到」—— **那正是 `K-R135` 在杀的那个形状**。
    // 〔WF1 · K〕那个 .NET 调用按 `REG_SZ` 写、读回展开值（`%VAR%` 被冻成字面）⇒ 生成的那段改在 PowerShell 里按原类型写注册表、
    // 广播自己做（`profile_installer.rs::SETTING_CHANGE_BROADCAST`）；仍是这一跳、这一段生成的字节，写点仍是加 / 撤两处。
    //
    // ⚠ **argv 是什么**：`powershell.exe -NoProfile -NonInteractive -Command <脚本>`。
    // `<脚本>` **只可能是 `profile_installer` 那三个 `render_*` 函数的输出**
    // （探 / 加 / 撤），**不吃任何用户输入** —— 里面唯一的变量是 `tool_registry` 申报的
    // 那个目录。同文件的 `the_generated_path_command_edits_only_the_user_scope_and_never_via_setx`
    // 在数「生产段起进程恰好一处、写用户级 PATH 恰好两处」，别读成「这里能跑任意命令」。
    ("profile_installer.rs", "run_user_path_powershell", "`powershell.exe -NoProfile -NonInteractive -Command <我们自己生成的那段>`",
     "`R85` 用户逐字「应当让用户手动点击加，也能管理删除」⇒ **点击即执行是允许的**（`K33` 禁的是产品**替**用户决定，用户点一下就是用户自己决定）。\
          必须起进程的理由是**那一档只有 Windows 的用户级环境块里有**，而 Rust 侧够得着它的另一条路（直接写注册表）会造出第二份 PATH 编辑实现、并把 `WM_SETTINGCHANGE` 广播的责任揽到自己身上 —— 两条都被 `R88` 否掉了。\
          ⚠ 跑的**就是界面上显示给用户看的那段字节** ⇒ 「点按钮」与「自己复制去跑」逐字同一份，实现只有一处
          ★ 三条策略为什么是这三格：`Hidden` 那格**先前没人回答过**（裸 `.output()`）—— `-NonInteractive` 只保证不等人回车，挡不住新开一个控制台。`Captured`：stderr 是下面那句报错的一部分。",
     "Hidden · JobKillOnClose · Captured"),
    // 〔MIG-3a〕`dialect.rs::ask_get_alias` 那一行（从前住 monitor 的方言模块）随方言进了那台后端（`platform/shell/mod.rs::powershell_command`，〔OSA〕目录模块，后端 `readonly_guard::spawn_registry` 登记）。
    ("launch.rs", "ssh_client_available", "探测用的 `ssh`",
     "只探测「本机有没有 ssh」，不带用户参数
          ★ 三条策略为什么是这三格：同上：先前是裸 `.output()`，Windows 上闪一个 `where.exe` 的黑框。`Captured`：输出就是返回值（`status.success()`）。",
     "Hidden · JobKillOnClose · Captured"),
    ("lib.rs", "open_with_os", "`cmd` / `open` / `xdg-open`",
     "按平台打开日志目录：三个名字都是常量，路径是 monitor 自己的目录
          ★ 三条策略为什么是这三格：`Detached` 是承重的：fire-and-forget，**绝不能是 `JobKillOnClose`** —— 那会在本函数返回、句柄一丢的瞬间把刚打开的文件管理器杀掉。",
     "Hidden · Detached · Inherit"),
    ("local_backend.rs", "supervise_with_stdio", "被监护的后端二进制",
     "本机后端监护：二进制路径来自 `candidates`（有 `candidates_never_point_into_a_build_tree` 守着）。\
          ⚠ P2 起它的 stdin 可能是 `piped()` 而不再恒为 `null` —— 那是本机入方向通道的管子\
          （`local_stdio_consumer`）。〔RL1〕先前那个 `stdio=None` 的薄壳随本机中转并进常驻后端删了，spawn 只剩这一个入口
          ★ 三条策略为什么是这三格：🔴 `设计/00 §1.5.2` 点名的那一处：它先前**同时**犯三个错（无 `CREATE_NO_WINDOW` · 无 job 绑定 · `stderr(Stdio::null())`），三格各对应一条策略。本层收注入参数，一个平台原语都不认识。",
     "Hidden · JobKillOnClose · ToLog（宿主注入：local_backend_supervised）"),
    // 〔LOC1a · 第四波 4D〕`local_query` 模块的 `run_query`〔散文墓碑〕那一行删了：本机那几问改走 `<local>` 长连接，
    //   monitor 不再起一次性本机后端（`设计/05 §14.6`）。
    // 〔MIG-1 · `99 §2.1 ⑯`〕`ssh -G` 那一行出表：解析 ssh config 搬进后端（`dial/ssh_config.rs::resolve`），monitor 不再起 `ssh`。
    // ── `K-P1`：常驻那条路 ──────────────────────────────────────────────
    ("local_backend_host.rs", "spawn_detached", "被脱离起来的后端二进制",
     "本机后端**脱离宿主**起：`process_group(0)` + stdio 全 null + 协议改走回环监听口。\
          二进制路径来自 `resolve_backend_bin`，而**它今天只是个适配器** —— 真正的答案\
          （exe 旁的 local_backend → 这份产物自己带的那份 → 释放出来）出自\
          `local_backend::resolve_or_extract` 那**一份共用的解析**，\
          `local_backend::start_or_extract` 走的也是同一份。\
          〔`K-R43` 订正：本行原先写「与 `start_or_extract` 同一个顺序，由一条对拍判据钉着」——\
           那时是**两份手写实现**，而那条判据只对拍顺序，`K-R42` 在它眼皮底下漂过一次仍全程绿。\
           今天钉的不是「两份同序」，是「**两条路都走那一份，且旁边不许再长出第二份取法**」\
           （`local_backend_host::the_two_resolution_paths_still_agree_on_the_order`，名字没改、机制换了）。〕\
          ⚠ 它必须住在**宿主知识层**而不是 `backend/`：`process_group` 来自 \
          `std::os::unix::process::CommandExt`，而 `std::os::unix` 在 \
          `backend/backend_tests.rs::the_backend_half_stays_platform_agnostic` 的禁针里 —— 写进去当场红，\
          而「加一条平台例外」被那张表的递减棘轮堵着（`PLATFORM_EXCEPTIONS.len() <= 1`，今天正好 1）
          ★ 三条策略为什么是这三格：「三样一起才叫脱离」里的两样：`Detached` 就是 `process_group(0)`，`Null` 是 stderr 那一根（stdin/stdout 仍在函数体里）。`Hidden` 那格**先前没人回答过** —— 常驻实例在 Windows 上留一个可关的黑框，等于常驻当场没了。",
     "Hidden · Detached · Null"),
    // ── 🔴 `15 §5.1 A3`（09-18）：**出口本身**。它是唯一一处「起进程」不在上面那些
    //    落点里的 —— 因为上面那些落点今天全都把那一下交给了它。
    ("spawn_managed.rs", "spawn_managed", "调用方给的那个二进制 ＋ argv",
     "`00 §1.5.2` 的唯一出口。它起进程不是为了做某件事，而是**为了让别人不用自己起** ——\
          三条策略（要不要窗口 · 要不要随我死 · 错误往哪去）在这里落成平台原语\
          （`creation_flags` / `process_group(0)` / Job Object / stderr 的四种去处），\
          全仓只此一份。\
          ⚠ 本行与上面那些**不是同一类**：上面每一行回答的是「这件事为什么非起进程不可」，\
          这一行回答的是「起进程这件事为什么只许有一个出口」。\
          由 `spawn_managed::tests::the_spawn_verbs_and_platform_primitives_live_only_here`\
          按源码派生地钉住（`.spawn()` / `creation_flags` / `process_group` / `kill_on_drop`\
          在别处出现一次就红）。",
     "—— **它就是那个出口本身**，没有「它选了哪三条」这回事"),
    // 🔴 〔第十三刀 2026-09-23〕**文件管理窗口的独立进程。** 这一行是新的**一整面**，
    //    不是搬家：此前那个窗口跑在 monitor 自己的进程里（次线程 ＋ `run_native`）。
    ("proc.rs", "spawn_window", "`cc-monitor-filewin`（本包的第二个 `[[bin]]`，一个窗口一个）",
     "用户 2026-09-22 逐字裁「**窗口生命周期就是销毁**」，而 winit **一个进程只许一个事件循环** \
          ⇒ 「关掉就销毁」与「关掉之后还能再打开」在同进程形态下**不可同时成立** \
          ⇒ 只剩「一个窗口一个进程」这条路。必须起进程的理由就是这一条，**它不是为了隔离**。\
          顺带解掉的两条（第二趟开窗必然失败 · 那 165 MiB 关掉就真还给系统）逐条住 `filewin/proc.rs` 头注 §二。\
          〔反订正 · 2026-09-24 · X1：上一版这里还有「关窗的拆卸竞态可能 abort 整个 app」一条 —— \
          那条读数来自台架的 `xdotool windowclose`（`XDestroyWindow`，产品里不存在），\
          出处 `真相源/107 §2` 的〔反订正〕块与 `设计/60 §「Xvfb 抖动」`〕\
          ⚠ **argv 上一个字都没有**：种子（那一屏行 ＋ 源 ＋ cwd ＋ reveal）走 **stdin**。\
          两条硬理由 —— ① 环境变量装不下（一屏上限 5 万条，JSON 是兆字节级，\
          而 Linux 一条环境变量的上限是 32 页 ⇒ `execve` 直接 `E2BIG`）；\
          ② argv 是世界可读的（`/proc/<pid>/cmdline`），而种子里带着 `RemoteConfig`\
          （主机名 · 用户名 · 私钥**路径**）—— C2 那一版的拨号代理宿主也用过同一条理由（SR1a 起它不再起进程）。\
          ⚠ 二进制的来路只有两处（环境变量 `CCM_FILEWIN_BIN` · exe 旁那份），\
          都不在就**出声**（`D11`：不留退路），由 `filewin::proc::tests` 那一摞钉住。\
          ⚠ 收尸归本落点自己（`Detached` 不改父子关系）：一条阻塞在 `waitpid` 上的线程，\
          零 CPU、零唤醒 —— 刻意**不**用「隔一会儿看一眼」，那是一个新节拍
          ★ 三条策略为什么是这三格：`Hidden` —— Windows 上不带它系统会**替子进程新开一个可关的控制台**，\
          用户一关 `CTRL_CLOSE_EVENT` 就把文件窗口杀了；egui 那个窗口自己会出来，它不需要控制台。\
          `Detached` 是**承重的**，与 `lib.rs::open_with_os` 逐字同形：这条 Tauri 命令一返回句柄就丢，\
          `JobKillOnClose` 会在那一瞬间把刚开出来的窗口收掉；顺带买到「关掉 monitor 不带走已开的文件窗口」。\
          `Inherit` 与 C2 那一版的拨号代理同形（SR1a 起它不再起进程）：它 stderr 上只有「窗口为什么没立起来」那一句，\
          接管它要再起一条泵。⚠ 代价如实记：装机那份 GUI app 没有 stderr 控制台 ⇒ 那句话今天会丢。",
     "Hidden · Detached · Inherit"),
    ("local_backend_host.rs", "run_resident_stop", "`<本机后端> --resident-stop`（不经 shell）",
     "〔STOP · 主会话裁〕停本机常驻后端：本机远端同一条 —— 在这台机器上跑一次那个一次性子命令，\
          由它做「请它收尾 → 宽限期内按 pidfd 等 → 到点强杀」（同机监督者，k8s / systemd 同形），monitor 只拿回结局。\
          必须起进程的理由：等与强杀住后端那一份（`control/resident.rs::stop_pid`），monitor 不再自己发信号、自己等；\
          参数是**我们自己记下的那个二进制**与一个常量 flag，零用户输入
          ★ 三条策略为什么是这三格：一次性子命令不该闪窗；就地等它退（别留后代）；失败那句 `{code,message}` 在 stderr 上，要读回来说给人听。",
     "Hidden · JobKillOnClose · Captured"),
];

fn src_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::crate_src_root()
}

/// 语料 = `src/` 整棵树 **+ `build.rs`**〔audit-0805 08-08〕。
///
/// ★ 为什么非把 `build.rs` 并进来：本表问的是「**谁能碰这台机器**」，
/// 而构建脚本每次 `cargo build`／`cargo check` 都在开发者机器上真跑
/// （它起 `sh` 与 `git`、往 `OUT_DIR` 复制内嵌后端）。
/// 08-08 实测：全仓所有登记表/守卫的扫描根都是 `src/frontend/shell/src` · `src/backend`
/// · `src/common` · `src` · `doc` —— **`src/frontend/shell/build.rs` 一张表都没扫到**，
/// 它是这些扫描面共同的盲点（与 F65「三张表共享同一个没写下来的前提」同族）。
fn corpus() -> Vec<(PathBuf, String)> {
    let mut files = guard_core::scan_tree!(&src_root(), &["rs"]);
    let bs = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    let src = std::fs::read_to_string(&bs).expect("读不到 build.rs —— 它是本表的一部分");
    files.push((bs, src));
    files
}

/// 某一行所在的函数名（往回找最近的 `fn`）。
fn enclosing_fn(lines: &[&str], at: usize) -> String {
    for l in lines[..=at].iter().rev() {
        if let Some(rest) = l.split(" fn ").nth(1).or_else(|| l.strip_prefix("fn ")) {
            let n: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !n.is_empty() {
                return n;
            }
        }
    }
    "<找不到外层函数>".to_string()
}

/// `spawn_managed.rs` 里某个枚举的变体名 —— **从源码派生，不手写**。
///
/// 手写一份的话，枚举改名之后这张表会安静地继续「通过」，
/// 而它声称对拍的那件事已经不存在了。
fn variants_of(enum_name: &str) -> Vec<String> {
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/spawn_managed.rs"
    ));
    let head = format!("pub enum {enum_name} {{");
    let i = src
        .find(&head)
        .unwrap_or_else(|| panic!("`spawn_managed.rs` 里找不到 `{head}` —— 枚举改名或搬家了"));
    let rest = &src[i + head.len()..];
    let end = rest
        .find("\n}")
        .unwrap_or_else(|| panic!("`pub enum {enum_name}` 的花括号没收口 —— 抽取器看不懂它了"));
    rest[..end]
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_suffix(','))
        .filter(|l| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric()))
        .map(str::to_string)
        .collect()
}

/// ★★ **第五列与盘面对拍**〔`15 §5.1 A3`，09-18〕。
///
/// # 它守什么
///
/// 「这个落点选了哪三条策略」这句话，在本表里**是可判的**，不是散文：
/// ① 三个变体名必须真的是 `spawn_managed.rs` 那三个枚举里的（名单从源码派生）；
/// ② 那三个 `X::Variant` 必须真的出现在**声明它们的那份源码**里
///    （默认是落点自己那个文件；带「宿主注入」后缀的，是 `spawn_managed.rs`）；
/// ③ 「不进那个出口」的三处要明说（`——` 开头），而且**只许是那三处**。
///
/// # ⚠ 射程（写下来，别读成更强）
///
/// 对拍粒度是**文件**不是行：同一个文件里两个落点把三元组互换，本条看不出来
/// （`launch.rs` 今天就是三个落点两种三元组）。它接得住的是
/// 「某处改了策略而账本没跟」「策略名被改掉」「新落点没申报策略」这三族。
#[test]
fn the_three_policies_each_site_declares_match_the_code() {
    let consoles = variants_of("ConsolePolicy");
    let lifetimes = variants_of("Lifetime");
    let sinks = variants_of("StderrSink");
    // 反向自检：名单塌了 ⇒ 下面全成了「随便写什么都不在名单里」的假红／或恒绿。
    assert_eq!(
        (consoles.len(), lifetimes.len(), sinks.len()),
        (3, 2, 4),
        "三个枚举的变体数变了（现打 {consoles:?} / {lifetimes:?} / {sinks:?}）——\n\
             变体增减本身不是错，但**它一定要经过这张表**：每一个落点都得回答\
             「新那格算不算我」。改完这一行，再逐条看第五列。"
    );

    let files = corpus();
    let src_of = |stem: &str| -> String {
        files
            .iter()
            .find(|(p, _)| p.file_name().and_then(|s| s.to_str()) == Some(stem))
            .map(|(_, raw)| guard_core::production_code(raw))
            .unwrap_or_else(|| panic!("语料里找不到 {stem} —— 它搬家了，第五列也就无从对拍"))
    };

    let mut opted_out = 0usize;
    let mut checked = 0usize;
    for (file, func, _, _, policy) in SPAWNS {
        if let Some(why) = policy.strip_prefix("——") {
            assert!(
                why.trim().len() > 10,
                "{file}::{func} 说自己不进那个出口，却没写清为什么 —— \
                     「刻意不做」也会过期，没有理由的豁免下一轮没人判得了真伪"
            );
            opted_out += 1;
            continue;
        }
        let triple: Vec<&str> = policy
            .split('（')
            .next()
            .unwrap_or(policy)
            .split(" · ")
            .map(str::trim)
            .collect();
        assert_eq!(
            triple.len(),
            3,
            "{file}::{func} 的第五列不是三格：{policy:?}\n\
                 ⇒ 格式是 `\"<Console> · <Lifetime> · <Stderr>\"`，\
                 或者 `\"—— <为什么不进那个出口>\"`。"
        );
        for (i, (name, allowed)) in [
            ("ConsolePolicy", &consoles),
            ("Lifetime", &lifetimes),
            ("StderrSink", &sinks),
        ]
        .iter()
        .enumerate()
        {
            let v = triple[i];
            assert!(
                allowed.iter().any(|a| a == v),
                "{file}::{func} 第 {} 格写的是 `{v}`，而 `{name}` 今天的变体是 {allowed:?}",
                i + 1
            );
        }
        // ② 那三个名字得真的写在**声明它们的那份源码**里。
        let home = if policy.contains("宿主注入") {
            "spawn_managed.rs"
        } else {
            *file
        };
        let src = src_of(home);
        for (i, name) in ["ConsolePolicy", "Lifetime", "StderrSink"]
            .iter()
            .enumerate()
        {
            let needle = format!("{name}::{}", triple[i]);
            assert!(
                src.contains(&needle),
                "{file}::{func} 的账本写着 `{needle}`，而 `{home}` 的生产段里找不到它。\n\
                     ★ 两种来路，先分清：\n\
                     ① 那一处真的改了策略 ⇒ **回来改第五列**（改了行为不回来改理由，\
                        账本当天就开始撒谎）；\n\
                     ② 这一行的「住哪」判错了 —— `backend/` 那两处自己不写策略，\
                        它们的第五列要带「（宿主注入：…）」后缀。"
            );
        }
        checked += 1;
    }
    assert_eq!(
        opted_out, 3,
        "「不进那个出口」的落点从 3 处变成了 {opted_out} 处。\n\
             今天那三处是：`build.rs` 两处（构建期）＋ 出口自己那一处。\
             多一处 = 有人给自己开了豁免；少一处 = 构建期那两条被并进来了（那是好事，改这个数）。"
    );
    // 〔LOC1a · 第四波 4D〕地板 14 → 13：本机一次性查询那一个落点（`local_query` 模块的 `run_query`〔散文墓碑〕）随本机那几问
    //   改走 `<local>` 长连接删了 ⇒ 人群恰好少一个（15 → 13 的另一个见失败读数，人群按现打为准）。
    // 〔SH1 · 4D〕地板 13 → 12：驾驶舱本机 shell 读那一个落点随读面改走后端删了。
    // 〔合并 MIG-1 × 主线 eebf51de〕地板 12 → 11：两边各自删掉的落点相加（现打 11；两边合并前各自现打都 ≥ 12）。
    assert!(
        checked >= 11,
        "只对拍到 {checked} 个带策略的落点 —— 09-18 现打 14 个，\
         〔第十三刀 09-23〕加了文件管理窗口那个独立进程之后 15 个，〔LOC1a 09-25〕删一次性本机查询之后 13 个。本条此刻在空转"
    );
}

#[test]
fn every_local_spawn_is_declared() {
    let files = corpus();
    let mut found: Vec<(String, String)> = Vec::new();
    for (path, src) in &files {
        let prod = guard_core::production_code(src);
        let lines: Vec<&str> = prod.lines().collect();
        let stem = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();
        // 🔴 〔`15 §5.1 A3` 09-18〕**人群的锚点从一个变成两个。**
        //
        // A3 之前「起进程」与「造一个 `Command`」是同一件事，所以数后者就够了。
        // 今天不是了：走 [`crate::spawn_managed::spawn_managed`] 那个五参数形态的落点
        // **自己不造 `Command`**（出口替它造）⇒ 只数 `Command::new(` 的话，
        // 那种落点会**悄悄从人群里消失**，而它一样在用户机器上起进程
        //（现打：`lib.rs::open_with_os` 就是这一形）。
        // ⇒ 第二个锚点 = **调用那个唯一出口**。两个锚点是并集，落点只要沾一个就得申报。
        //
        // ⚠ **出口自己那份源码只按第一个锚点数**：在 `spawn_managed.rs` 里面，
        //   这几个名字是它的**实现与签名**，不是「又一个起进程的地方」——
        //   连签名行都会命中（`pub fn spawn_managed_tokio(`），那会给这张表添三行
        //   只描述出口内部结构的噪声。出口自己在表里**恰好占一行**，那一行已经写清了
        //   「它就是那个出口本身」。
        let is_the_exit = stem == "spawn_managed.rs";
        for (i, l) in lines.iter().enumerate() {
            let hit = l.contains(concat!("Command::", "new("))
                || (!is_the_exit
                    && [
                        "spawn_managed(",
                        "spawn_managed_cmd(",
                        "spawn_managed_tokio(",
                    ]
                    .iter()
                    .any(|n| l.contains(n)));
            if hit {
                found.push((stem.clone(), enclosing_fn(&lines, i)));
            }
        }
    }
    found.sort();
    found.dedup();
    assert!(
        found.len() >= 5,
        "全树只找到 {} 处本机起进程（08-08 实测 8 个「文件::函数」）—— 抽取器坏了，本条此刻无效",
        found.len()
    );

    let missing: Vec<String> = found
        .iter()
        .filter(|(f, n)| !SPAWNS.iter().any(|(sf, sn, ..)| sf == f && sn == n))
        .map(|(f, n)| format!("  {f}::{n}"))
        .collect();
    assert!(
        missing.is_empty(),
        "这些地方**会在用户机器上起一个进程，但没人申报**：\n{}\n\n\
             ⚠ 08-08 实测：往生产段加一句 `Command::new(\"sh\").arg(\"-c\")`，全仓判据一条不红。\n\
             登记进 `SPAWNS`：写清**起的是什么**、**为什么必须起进程**、\
             以及〔A3 之后〕**它选了哪三条策略**。\n\
             backend 侧同类表在 `readonly_guard`（`ALLOWED` + `SPAWN_SITES_TODAY`）。",
        missing.join("\n")
    );

    let stale: Vec<String> = SPAWNS
        .iter()
        .filter(|(f, n, ..)| !found.iter().any(|(ff, nn)| ff == f && nn == n))
        .map(|(f, n, ..)| format!("  {f}::{n}"))
        .collect();
    // ⚠ **红了还要讲对成因**〔08-08〕：死行有两种完全不同的来路 ——
    // ① 那处代码真的改名/删了；② **扫描面缩了**（语料不再包含那个文件）。
    // 实测把 `build.rs` 从语料里拿掉，旧诊断说「已经不起进程了（改名或删了）」，
    // 而 `build.rs` 里那两处一个字没动 —— 照它去查会查错方向。
    // 本区反复吃过这个亏（「讲错成因的红灯比不红更坏」），所以这里先分辨再说话。
    let scanned: Vec<String> = files
        .iter()
        .filter_map(|(p, _)| p.file_name().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    let out_of_corpus: Vec<&str> = SPAWNS
        .iter()
        .map(|(f, ..)| *f)
        .filter(|f| !scanned.iter().any(|s| s == f))
        .collect();
    assert!(
        stale.is_empty(),
        "申报表里这些落点在语料里找不到了：\n{}\n\
             ★ 两种来路，先分清再动手：\n\
             ① 那处代码真的改名/删了 ⇒ 更新登记（改名也要红 —— 名字变了就该有人重新看一眼它起的是什么）；\n\
             ② **扫描面缩了** —— 这些文件压根不在本次语料里：{out_of_corpus:?}\n\
                （非空就说明是这一种：去看 `corpus()` 少扫了什么，别去改登记表）",
        stale.join("\n")
    );
}
