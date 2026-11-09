//! 开终端窗口的平台那一半：壳里平台 cfg 的唯一住址（同 [`super::fs`]）。
//! POSIX：挑一个终端（设置里指定的 · 系统出口 · 常见终端逐个探）开窗跑 `bash -lic <命令>`；
//! Windows：`wt.exe` / `powershell.exe` 开窗。POSIX 那一臂「跑什么」的校验与 argv 也住这里（[`build_local_posix_argv`]）；
//! 开窗那条 Tauri 命令（`open_terminal_window`）留在 `launch.rs`，经本文件的 [`open_local`] 开窗。

use copy_core::said::Said;

/// 常见终端怎么交给它一条命令：`(程序名, 命令前垫的参数)`，各家约定不同（逐个照它的用法串核过；后面整串 argv 原样接上）。
/// 先两个「交给系统定」的出口（freedesktop `xdg-terminal-exec` —— Ubuntu 26.04 的「默认终端」就按它的配置走 ·
/// Debian / Ubuntu alternatives 的 `x-terminal-emulator`，Debian 政策要它收 `-e <命令> <参数…>`），再各桌面的默认与常见终端。
#[cfg(not(windows))]
pub(crate) const EMULATORS: &[(&str, &[&str])] = &[
    ("xdg-terminal-exec", &["--"]),
    ("x-terminal-emulator", &["-e"]),
    // GNOME 一系：Ptyxis（Ubuntu 25.10 起 · Fedora 41 起默认）· GNOME Console（上游默认）· GNOME Terminal。都收 `-- <命令…>`。
    ("ptyxis", &["--"]),
    ("kgx", &["--"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    // Xfce / MATE：`-x` ＝「命令行剩下的全部在终端里跑」（它们的 `-e` 收一整个字符串，不用）。
    ("xfce4-terminal", &["-x"]),
    ("mate-terminal", &["-x"]),
    ("qterminal", &["-e"]),
    ("tilix", &["-e"]),
    ("ghostty", &["-e"]),
    ("kitty", &[]),
    ("alacritty", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("foot", &[]),
    ("xterm", &["-e"]),
];

/// 各桌面自己的默认终端（按 `XDG_CURRENT_DESKTOP` 里的名字，大小写不论）：自动挑时提到两个系统出口之后、别的具名终端之前。
#[cfg(not(windows))]
pub(crate) const DESKTOP_DEFAULTS: &[(&str, &[&str])] = &[
    ("GNOME", &["ptyxis", "kgx", "gnome-terminal"]),
    ("KDE", &["konsole"]),
    ("XFCE", &["xfce4-terminal"]),
    ("MATE", &["mate-terminal"]),
    ("LXQt", &["qterminal"]),
];

/// 自动挑的顺序：两个系统出口 → 这个桌面的默认（[`DESKTOP_DEFAULTS`]）→ [`EMULATORS`] 里其余的原序。
#[cfg(not(windows))]
pub(crate) fn auto_order(desktop: &str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = EMULATORS[..2].iter().map(|(n, _)| *n).collect();
    for d in desktop.split(':') {
        for (de, firsts) in DESKTOP_DEFAULTS {
            if d.eq_ignore_ascii_case(de) {
                for n in firsts.iter() {
                    if !names.contains(n) {
                        names.push(n);
                    }
                }
            }
        }
    }
    for (n, _) in EMULATORS {
        if !names.contains(n) {
            names.push(n);
        }
    }
    names
}

/// 设置里那一格（config.json `terminal`）：空 ⇒ 自动；等于表里某个名字 ⇒ 照它的约定；
/// 否则当命令前缀按空白切开，命令接在后面（`<前缀> bash -lic …`）。
#[cfg(not(windows))]
fn prefix_of(setting: &str) -> Vec<String> {
    let s = setting.trim();
    match EMULATORS.iter().find(|(n, _)| *n == s) {
        Some((n, before)) => std::iter::once(*n)
            .chain(before.iter().copied())
            .map(String::from)
            .collect(),
        None => s.split_whitespace().map(String::from).collect(),
    }
}

/// 设置里指定的那个终端这台上没有（名字是设置里那一格的第一个词）。
#[cfg(not(windows))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetMissing(pub String);

/// 这一趟用哪个终端 ⇒ 它的 argv 前缀（命令接在后面）。自动挑一个都没探到 ⇒ `Ok(None)`（[`TerminalOpen::NoWindow`]）；
/// 设置里指定的那个不在 ⇒ [`SetMissing`]（[`TerminalOpen::SetMissing`]；不退成自动挑到的别的终端）。两种都由前端照实说、给设置入口。
/// 纯函数：设置 · 桌面 · 「这个程序在不在」都是入参（判据不碰真机器）。
#[cfg(not(windows))]
pub(crate) fn pick_terminal_from(
    setting: Option<&str>,
    desktop: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Option<Vec<String>>, SetMissing> {
    match setting.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => {
            let prefix = prefix_of(s);
            if exists(&prefix[0]) {
                Ok(Some(prefix))
            } else {
                Err(SetMissing(prefix[0].clone()))
            }
        }
        None => Ok(auto_order(desktop)
            .into_iter()
            .find(|n| exists(n))
            .map(prefix_of)),
    }
}

/// 设置里那一格的原值（config.json `terminal`；没设 / 读不动 ⇒ `None` ＝ 自动）。读经设置那一处（上限照声明表）。
#[cfg(not(windows))]
fn terminal_setting() -> Option<String> {
    let v = crate::config::load_config_at(&crate::config::resolve_config_path()?).ok()?;
    v.get("terminal")?.as_str().map(String::from)
}

#[cfg(not(windows))]
fn current_desktop() -> String {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
}

/// 生产那一问：按设置 ＋ 这台的 `PATH` 挑。
#[cfg(not(windows))]
pub(crate) fn pick_terminal() -> Result<Option<Vec<String>>, SetMissing> {
    pick_terminal_from(
        terminal_setting().as_deref(),
        &current_desktop(),
        &program_exists,
    )
}

/// 设置页那一行要画的事实：自动会挑谁 · 本机探到了哪些 · 设置里现在是什么。界面只画，不判。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct TerminalChoices {
    /// 这台电脑上开终端要不要挑（Windows 上恒开 PowerShell ⇒ `false`，那一行不出现）。
    pub applies: bool,
    /// 自动会挑的那个；一个都没探到 ⇒ `None`。
    pub auto: Option<String>,
    /// 本机探到的（按自动挑的顺序）。
    pub found: Vec<String>,
    /// 设置里那一格的原值（空 ＝ 自动）。
    pub setting: String,
}

/// [`TerminalChoices`] 的纯函数那一半。
#[cfg(not(windows))]
pub(crate) fn terminal_choices_from(
    setting: Option<&str>,
    desktop: &str,
    exists: &dyn Fn(&str) -> bool,
) -> TerminalChoices {
    let found: Vec<String> = auto_order(desktop)
        .into_iter()
        .filter(|n| exists(n))
        .map(String::from)
        .collect();
    TerminalChoices {
        applies: true,
        auto: found.first().cloned(),
        found,
        setting: setting.unwrap_or_default().trim().to_string(),
    }
}

pub fn terminal_choices() -> TerminalChoices {
    #[cfg(not(windows))]
    {
        terminal_choices_from(
            terminal_setting().as_deref(),
            &current_desktop(),
            &program_exists,
        )
    }
    #[cfg(windows)]
    {
        TerminalChoices {
            applies: false,
            auto: None,
            found: Vec::new(),
            setting: String::new(),
        }
    }
}

/// `PATH` 里（或给的是路径时那个文件本身）有没有这个可执行文件。
#[cfg(not(windows))]
fn program_exists(cmd: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let runnable = |p: &std::path::Path| {
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    };
    if cmd.contains('/') {
        return runnable(std::path::Path::new(cmd));
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| runnable(&d.join(cmd))))
        .unwrap_or(false)
}

/// 窗口里命令跑完之后留一个交互 shell（同 Windows 那一形的 `-NoExit`）：出错时用户看得见那句话，窗口不闪退。
/// 命令本身作为 `$1` 原样交进去（不拼进脚本串）。
#[cfg(not(windows))]
pub(crate) const KEEP_OPEN: &str = r#"eval "$1"; exec "${SHELL:-bash}" -l"#;

/// 远端命令长度上限（防 IPC 侧异常输入；正常 resume 命令 <300 字节）。
#[cfg(not(windows))]
pub(crate) const MAX_REMOTE_CMD: cap_core::Cap = cap_core::Cap {
    bytes: 4096,
    over: cap_core::Over::Reject,
};

/// L1（local-as-remote）：**与传输无关**的那层命令校验 —— 三条送法一律适用。
///
/// 远端那条送法的同一组判据今天住本机后端（`dial/terminal.rs`，帧命令 `terminal-ssh`）；这里只剩 POSIX 本地那条路
/// （[`build_local_posix_argv`]）用。
///
/// **刻意不含「拒绝双引号」那条**：它的理由是 PowerShell 5.1 向 native 程序传参对内嵌 `"`
/// 有历史畸变（见调用处），是**那条送法的**约束，不是命令本身的性质。
/// 把它一并搬进来，等于把一个 Windows 怪癖套到 Linux 上 —— 判据要落在性质上。
#[cfg(not(windows))]
fn validate_launch_cmd(cmd: &str, what: &str) -> Result<(), Said> {
    use crate::detail::Shell as _;
    if cmd.trim().is_empty() {
        return Err(Said::of(
            "rsLaunch.refuse.empty",
            &[("what", &what.to_string())],
        ));
    }
    if cmd.len() > MAX_REMOTE_CMD.as_len() {
        return Err(Said::of(
            "rsLaunch.refuse.tooLong",
            &[
                ("what", &what.to_string()),
                ("len", &(cmd.len()).to_string()),
                ("max", &MAX_REMOTE_CMD.bytes.to_string()),
            ],
        ));
    }
    if cmd.chars().any(|c| c.is_control()) {
        return Err(Said::of(
            "rsLaunch.refuse.control",
            &[("what", &what.to_string())],
        ));
    }
    Ok(())
}

/// L1：**POSIX 本地**送法 —— 直接 exec，**不要 ssh 包**。
///
/// 返回 argv 而不是命令串：本地没有「要穿过一层 shell」的问题，拼成串再让别人拆是
/// 白白造一个注入面。`bash -lic` 这层**保留**——它和远端那条路是同一个语义
///（PATH / 别名 / 函数按「用户粘贴进交互终端」解析），`ccm` 正是靠它才被找到。
///
/// ⇒ 与远端那条送法的关系就是 §2「payload 共享、transport 只管送」：
/// 同一个 `cmd`，本地是 `bash -lic <cmd>`，远端是把这同一串再包进 ssh（那一层今天由本机后端 `terminal-ssh` 渲）。
/// 两侧各有测试逐字节钉住（本侧 `local_posix_sends_the_payload_itself_without_an_ssh_wrap`，
/// 远端那侧在 `tests/backend/dial_terminal_tests.rs`）。
#[cfg(not(windows))]
pub(crate) fn build_local_posix_argv(cmd: &str) -> Result<Vec<String>, Said> {
    validate_launch_cmd(cmd, &copy_core::copy_text("rsLaunch.what.localCmd", &[]))?;
    Ok(vec!["bash".into(), "-lic".into(), cmd.into()])
}

/// 命令串 → 真正要 spawn 的 `(program, args)`：终端前缀 ＋ `bash -lic <留窗脚本> bash <命令>`。
/// 整跳是纯函数，判据量它产出的东西（载荷逐字节在最后一格）。
#[cfg(not(windows))]
pub(crate) fn local_posix_spawn_plan(
    cmd: &str,
    term: &[String],
) -> Result<(String, Vec<String>), Said> {
    let argv = build_local_posix_argv(cmd)?;
    Ok(build_local_posix_spawn(term, &argv))
}

#[cfg(not(windows))]
pub(crate) fn build_local_posix_spawn(term: &[String], argv: &[String]) -> (String, Vec<String>) {
    // argv = [bash, -lic, <命令>]（`build_local_posix_argv`）。
    let (shell, flags, payload) = (&argv[0], &argv[1], &argv[2]);
    let tail = [
        shell.clone(),
        flags.clone(),
        KEEP_OPEN.to_string(),
        shell.clone(),
        payload.clone(),
    ];
    let program = std::path::Path::new(&term[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let args = if ONE_STRING.contains(&program) {
        // 收一整串的那几家：整条 argv 按 POSIX 单引号拼成一个参数（它们按 shell 词法切回去，内容逐字节不变）。
        let joined = tail
            .iter()
            .map(|a| shell_quote_core::posix_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        term[1..].iter().cloned().chain([joined]).collect()
    } else {
        term[1..].iter().cloned().chain(tail).collect()
    };
    (term[0].clone(), args)
}

/// 命令前缀之后只收**一个**参数、自己按 shell 词法切开的终端：Tilix（`-e` 后面多于一个参数时它用空格拼起来、只给带空格的那几格
/// 加双引号，不转义 `\` 与单引号 ⇒ 任意载荷会走样；只给一个参数时原样交 shell 词法切）。
#[cfg(not(windows))]
pub(crate) const ONE_STRING: &[&str] = &["tilix"];

/// 在 **POSIX 本机**开一个终端窗口跑一条命令（不经 ssh、不经 PowerShell）：挑到终端 ⇒ 开窗；一个都没探到 ⇒ [`TerminalOpen::NoWindow`]
/// （不回落到无窗口直起：要人交互的那一行跑在看不见的地方等于没跑）；设置里指定的不在 ⇒ [`TerminalOpen::SetMissing`]。
#[cfg(not(windows))]
pub fn launch_local_posix(cmd: &str, cwd: Option<&str>) -> Result<TerminalOpen, Said> {
    match pick_terminal() {
        Ok(term) => open_window_via(cmd, cwd, term.as_deref()),
        Err(SetMissing(name)) => {
            tracing::info!("launch: 设置里指定的终端 {name} 不在");
            Ok(TerminalOpen::SetMissing)
        }
    }
}

/// [`launch_local_posix`] 挑完终端之后那一跳，终端是入参（判据传 `None` / 假终端，不在开发者桌面上开真窗口）。
#[cfg(not(windows))]
pub(crate) fn open_window_via(
    cmd: &str,
    cwd: Option<&str>,
    term: Option<&[String]>,
) -> Result<TerminalOpen, Said> {
    match term {
        Some(t) => launch_local_posix_via(cmd, cwd, t)
            .map(|()| TerminalOpen::Opened)
            .map_err(LaunchErr::into_said),
        None => Ok(TerminalOpen::NoWindow),
    }
}

/// [`launch_local_posix_via`] 没成的两种：命令过不了校验（那一句已写好）· 起进程那一下没成（系统那个错原样，判据按种类认 `ETXTBSY`）。
#[cfg(not(windows))]
#[derive(Debug)]
pub(crate) enum LaunchErr {
    Plan(Said),
    Spawn(std::io::Error),
}

#[cfg(not(windows))]
impl LaunchErr {
    /// 给人看的那一句：起不来 ⇒ 「启动本机命令失败 · <原因词>」，系统原话进详情（也记一行日志）。
    pub(crate) fn into_said(self) -> Said {
        match self {
            LaunchErr::Plan(s) => s,
            LaunchErr::Spawn(e) => {
                tracing::warn!("launch: spawn terminal failed: {e}");
                Said::with_raw(
                    copy_core::copy_text(
                        "rsLaunch.local.spawnFailed",
                        &[("why", &copy_core::spawn_reason(e.kind()))],
                    ),
                    e,
                )
            }
        }
    }
}

/// 终端作为入参：判据喂一个自己造的假终端脚本，不在开发机上弹真窗口。
#[cfg(not(windows))]
pub(crate) fn launch_local_posix_via(
    cmd: &str,
    cwd: Option<&str>,
    term: &[String],
) -> Result<(), LaunchErr> {
    use crate::spawn_managed::{spawn_managed_with, ConsolePolicy, Lifetime, StderrSink};
    use copy_core::{Discard, Why};
    use std::process::Stdio;

    let (program, args) = local_posix_spawn_plan(cmd, term).map_err(LaunchErr::Plan)?;
    // 后端把后端路径交给它亲手开的这个窗口（窗口里那次 `ccm` 据此找到本机后端）。local_backend 不在就不设。
    let backend_env = crate::local_backend::backend_bin_env_for_window(
        crate::local_backend_host::running_backend_bin(),
    );
    // `Hidden`：POSIX 上没有控制台窗口这回事，窗口是终端程序自己开的 · `Detached`：别跟着 monitor 的进程组收信号 ·
    // `Null`：说话的是它开出来的那个窗口。
    let mut child = spawn_managed_with(
        &program,
        ConsolePolicy::Hidden,
        Lifetime::Detached,
        StderrSink::Null,
        |builder| {
            builder.args(&args);
            // 只有真实存在的目录才作起始目录（与 Windows 那条路同一条纪律）。
            if let Some(d) = cwd.filter(|c| std::path::Path::new(c).is_dir()) {
                builder.current_dir(d);
            }
            if let Some((k, v)) = backend_env {
                builder.env(k, v);
            }
            builder.stdin(Stdio::null()).stdout(Stdio::null());
        },
    )
    .map_err(LaunchErr::Spawn)?;
    // `process_group` 不改父子关系 ⇒ 收尸线程（终端程序多半很快把窗口交给自己的服务进程就退）。
    std::thread::spawn(move || {
        child.wait().discard(Why::Reap);
    });
    tracing::info!("launch: 已开终端窗口（{program}）");
    Ok(())
}

/// 在新终端窗口跑一条 PowerShell 命令（加载用户 profile、`-NoExit` 保留窗口）。
///
/// Plan A：wt.exe（Windows Terminal）新标签；Plan B：powershell.exe +
/// CREATE_NEW_CONSOLE 独立控制台。`local_cwd`＝Some 且为本地存在目录时作为窗口
/// 起始目录（远端拉起传 None——cwd 是远端路径）。
///
/// # 🔴 **这个函数体分两半：量文本的判据看得见它，量行为的判据在本机看不见它**
///
/// ## ① 量**文本**的那一类：看得见
///
/// `#[cfg(windows)]` 是**类型检查**那一层剔的；而 `guard_core::production_code`
/// 只剥 `#[cfg(test)] mod X { }` 段与注释（整行的与行尾的都剥），**它不剥任何 `cfg`**
/// ⇒ 本函数体在**文本**这一层原样在场。
/// `launch_tests.rs::every_terminal_window_backend_opens_carries_the_backend_path`
/// （普通 `#[test]`，在 `cargo` 门里跑）用**三条等号断言**钉着本函数体里的构造
/// （`spawn_managed_with(` · `.env(k, v)` · `backend_bin_env_for_window(`）：把 Plan B 那句
/// `builder.env(k, v)` 换成 `let _ = backend_env;`（开窗点漏了带 env）⇒ 它红。
///
/// ## ② 量**行为**的那一类：在本机上**造不出来**
///
/// 要驱动本函数体，先得有这个 item；而 Linux 上它在类型检查之前就没了
/// ⇒ 任何「真的调用它、看它干了什么」的判据在本机**不存在**。
/// **它的 `cfg` 就写在下面那一行**：标「平台判不了」要给得出 `cfg`；给不出 `cfg` = 它在本平台编译 = 不是判不了。
/// 买**这一半**要 CI 上一条 Windows job（或交叉编译 ＋ `cargo test --target`），落点 `.github/workflows/ci.yml`。
/// ⚠ `tests/scripts/gate.sh` 头注说「本地门禁比 CI 严」，而**这一格恰是反过来的那一格**，
/// 别把那句话读成全称。
///
/// ## 口径
///
/// **「带 `#[cfg(windows)]`」蕴含「量行为的判据看不见它」，不蕴含「所有判据都看不见它」。**
/// 别把一个准确的局部读数放大成全称。
///
/// ## ⚠ 别把这一条读宽
///
/// - 同一条腿上的 [`powershell_encoded_command`]（本文件末尾）是 `cfg(any(windows, test))`：
///   Linux 上 test 档真编译、单测在那儿跑 ⇒ 不属于本族；它由
///   `utils_tests.rs::the_relay_prefix_survives_the_powershell_encoding_byte_for_byte` 钉着。
#[cfg(windows)]
pub fn launch_powershell_window(ps_command: &str, local_cwd: Option<&str>) -> Result<(), Said> {
    use crate::spawn_managed::{spawn_managed_with, ConsolePolicy, Lifetime, StderrSink};

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
    // ★★ 三条策略。**Plan A 与 Plan B 只有第一格不同，而那是照着盘面写的：**
    //   · `Inherit`（本处，Plan A）—— 🔴 `wt.exe` 今天**一个 creation flag 都没带**，
    //     而且它多半只是把请求转交给**已在跑的** Windows Terminal 进程（这一格的诚实边界
    //     上面那段注释已经写过一次）⇒ 真正开窗的不是这次 `CreateProcess`。
    //     ⚠ 刻意**不**顺手改成 `NewVisible`：那是一次**没有任何 Windows 读数支持**的
    //     行为改动，而这条路正是本产品的主用途。这里只搬「谁来写这三格」，不动盘面。
    //   · `Detached` —— 用户的终端**不该随 monitor 一起死**：关掉界面不等于关掉他正在
    //     敲字的那个会话 ⇒ 绝不能进 `JobKillOnClose` 那个 Job。
    //   · `Inherit` —— 窗口里的话说给用户听，不该灌进我们的滚动日志。
    if spawn_managed_with(
        "wt.exe",
        ConsolePolicy::Inherit,
        Lifetime::Detached,
        StderrSink::Inherit,
        |wt| {
            wt.args(&wt_args);
            if let Some((k, v)) = backend_env.clone() {
                wt.env(k, v);
            }
        },
    )
    .is_ok()
    {
        tracing::info!("launch: powershell window via wt.exe");
        return Ok(());
    }

    // Plan B：powershell.exe + CREATE_NEW_CONSOLE，conhost 兜底。
    // ★★ 第一格与 Plan A 不同，也是照着盘面写的：`creation_flags(CREATE_NEW_CONSOLE)`
    // 就是 `ConsolePolicy::NewVisible`。**全仓唯一一处 `NewVisible`，而且是刻意的** ——
    // 别把 `launch.rs` 的 `CREATE_NEW_CONSOLE` 一起改掉：这正说明为什么要
    // 唯一出口 + 显式声明，而不是全局加一个 flag。后两格与 Plan A 相同。
    spawn_managed_with(
        "powershell.exe",
        ConsolePolicy::NewVisible,
        Lifetime::Detached,
        StderrSink::Inherit,
        |builder| {
            builder.args(ps_args);
            // F06b-1d（C9）：同上。这一格是**真新起的进程**，env 一定继承。
            if let Some((k, v)) = backend_env {
                builder.env(k, v);
            }
            if let Some(d) = start_dir {
                builder.current_dir(d);
            }
        },
    )
    .map_err(|e| {
        Said::with_raw(
            copy_core::copy_text(
                "rsLaunch.local.powershellFailed",
                &[("why", &copy_core::spawn_reason(e.kind()))],
            ),
            e,
        )
    })?;
    tracing::info!("launch: powershell window via fallback console");
    Ok(())
}

/// 开窗那一下**成了**的两种结局（真失败走 `Err`，一句人话）。调用方按它判，不按哪句话里的字判。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalOpen {
    /// 开了一个终端窗口。
    Opened,
    /// 这台找不到能开的终端（POSIX：设置里没指定、自动也没探到）。前端照实说、给设置入口。
    #[cfg_attr(
        all(windows, not(test)),
        expect(dead_code, reason = "只有 POSIX 那一臂造它")
    )]
    NoWindow,
    /// 设置里指定的那个终端这台上没有（POSIX）。前端照实说、给设置入口。
    #[cfg_attr(
        all(windows, not(test)),
        expect(dead_code, reason = "只有 POSIX 那一臂造它")
    )]
    SetMissing,
}

/// 在本机开一个终端窗口跑 `cmd`（工作目录 `cwd`，不在就不设）—— 「这台电脑能不能开终端窗口、用哪个」只在这一处答：
/// POSIX 按设置 / 探到的终端开（[`launch_local_posix`]）；Windows 开 PowerShell 窗口（[`launch_powershell_window`]）。
pub fn open_local(cmd: &str, cwd: Option<&str>) -> Result<TerminalOpen, Said> {
    #[cfg(not(windows))]
    let out = launch_local_posix(cmd, cwd);
    #[cfg(windows)]
    let out = launch_powershell_window(cmd, cwd).map(|()| TerminalOpen::Opened);
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
    cmd_wire::b64::b64_encode(&utf16le)
}
