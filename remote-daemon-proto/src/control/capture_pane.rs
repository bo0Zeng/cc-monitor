//! `K-R86`（2026-09-13）：**`capture-pane` 只读原语** —— 把某个 tmux 会话**此刻**那一屏
//! 的文本抓回来，抓完就返回。
//!
//! # 它补的是哪个洞
//!
//! monitor 侧 `src-tauri/src/parity_ledger.rs` 的 `tmux.manage` 那一格逐字记了一个月：
//! 「能不能预览这件事一点没变，**仍等 daemon 出原语**」。**本模块就是那条原语。**
//!
//! 在它之前 daemon 会 `list-sessions`（`common/session_snapshot.rs`）、会
//! `display-message`（[`super::gate`]）、会 `kill-session`（[`super::kill`]）、
//! 会 `send-keys`（[`super::launch`]）—— **唯独没有「把那一屏取回来」**。
//!
//! ⚠ **它只是原语。** monitor 侧那条 `capture_remote_pane` 今天仍然只有远端一条路
//! （`src-tauri/src/tmux.rs` 不在本件写区）—— 欠账从「等 daemon 出原语」变成
//! 「等 monitor 侧接上去」，**没有被结掉**。
//!
//! # 🔴 只出原语，不出轮询（`KR86D3`）
//!
//! 本模块**抓一次、立刻返回**。「抓几次 / 隔多久再抓一次」由**调用方**决定 ——
//! 那一半归 `K-R87`。零定时器铁律（`no_timer_guard`）的人群包含本文件，
//! 任何会让线程自己醒来的构件在这里当场红；另有
//! `readonly_guard::capture_is_read_only::the_capture_site_is_one_shot` 钉住
//! 「这一处恰好起一次进程、零循环构件」。**一次抓一屏是刀口纪律，不是性能取舍。**
//!
//! # ★ 为什么它住 `control/`（这是判断，不是实测 —— 写出来让下一个人能反对）
//!
//! 它**只读**，按「读 / 改变世界」那条线看更像 `observe/`。三条现打的理由压向 `control/`：
//!
//! 1. **tmux 这个外部程序的 argv 直传承载今天全在 `control/` 与 `common/`**
//!    （`gate` / `kill` / `launch` / `identity_tag` / `tmux_hook` / `session_snapshot`），
//!    `observe/` 那一侧碰 tmux 走的是 `platform/shell.rs` 的 `sh -c`，与本处形态不同。
//! 2. **进不了 `common/`**：那一层的门槛①是「≥2 个**上层**用」，而本模块今天
//!    只有一个调用方（`main.rs` 的一次性分派臂）⇒ 不达标，硬塞就是把门槛变成杂物间。
//! 3. ⚠ **`gate` 那条「只读但归 control/」的理由（定框 C13：有没有决策权）在这里
//!    不成立** —— 本模块不参与任何「能不能改这个会话」的决策。不许照抄那句话。
//!
//! # 三态分得开（`KR86D2`）：**抓不到时说得出真实原因，不是静默空串**
//!
//! | 结局 | 判据来自哪儿 | 码 |
//! |---|---|---|
//! | tmux 这个程序起不来 | `Command::output()` 自己的 `Err`（内核级，不猜） | `no_tmux` |
//! | tmux 在、但一个 server 都没有 | 退出码非零 ＋ stderr 命中 [`NO_SERVER_NEEDLES`] | `no_server` |
//! | server 在、这个目标不存在 | 退出码非零 ＋ stderr 命中 [`NO_TARGET_NEEDLES`] | `no_such_session` |
//! | 其它失败 | 退出码非零、两张针都不命中 | `capture_failed` ＋ **stderr 原样回包** |
//! | 成功 | 退出码 0 | `Ok(那一屏)` |
//!
//! ⚠ **空屏是合法的成功**：一个刚建起来、什么都没打印的 pane 抓回来就是空串、退出码 0。
//! 所以「抓不到」这件事**不靠输出是不是空**判 —— 靠退出码。
//! （这一条与 [`super::gate::probe`] 相反：那边 `display-message` 对不存在的目标是
//!  **rc=0 + 空输出**，只能拿「输出为空」当判据；`capture-pane` 对不存在的目标是
//!  **rc≠0 + stderr 有话**，daemon 直接拿得到，不必像 monitor 那侧渲染 `NO_PANE` 哨兵。）
//!
//! ⚠ **兜底那一档是「说不清但不撒谎」**：tmux 换了措辞 ⇒ 两张针都不命中 ⇒ 落
//! `capture_failed` 并把 stderr **原样**带出去。它比「压成会话不存在」诚实 ——
//! 后者是拿一个**具体而错误**的答案冒充知识。
//!
//! # 只读，而且「只读」这件事有人在数
//!
//! 这一处起进程已登记进 `readonly_guard::spawn_registry::ALLOWED`。⚠ 那张表的键是
//! `(文件, 起什么程序)`，**分不出被调的是哪条 tmux 子命令**（`ALLOWED` 里
//! `plugin/invoke.rs` 那条自陈过这个 `K6b` 盲区）⇒ 光登记买不到「只读」。
//! 真正钉住它的是 `readonly_guard::capture_is_read_only`：
//! **值级**（这一处发出去的 argv 逐元素就是那条只读形）＋ **文本级**
//! （生产段里不出现任何会改 tmux 状态的动词）＋ 反向自检（合成样本必须被逮到）。

use crate::common::tmux_utf8::UTF8_CLIENT_FLAG;
use std::process::{Command, Stdio};

/// 命令级错误：`(code, message)`。与 [`super::launch`] / [`super::gate`] / [`super::kill`] 同型。
pub(crate) type CmdErr = (&'static str, String);

/// tmux 的「把这一屏打到 stdout」子命令。
pub(crate) const CAPTURE_SUBCOMMAND: &str = "capture-pane";

/// `-p` = 打到 stdout（而不是存进 tmux 自己的 buffer —— 存 buffer 会**改 tmux 状态**）。
pub(crate) const PRINT_TO_STDOUT: &str = "-p";

/// `-t` = 目标。
pub(crate) const TARGET_FLAG: &str = "-t";

/// 这一处**只许**发这一条 argv。
///
/// 🔴 顺序有两条硬约束，都不是排版：
/// - [`UTF8_CLIENT_FLAG`] **必须排在子命令之前**（`K-R12`：放后面是
///   `rc=1 + unknown flag -u`，而那个响错会被下面的判法读成「抓不到」）；
/// - [`PRINT_TO_STDOUT`] 不许换成 `-b`／落 buffer 的写法 —— 那就不再是只读。
///
/// 逐元素由 `readonly_guard::capture_is_read_only` 钉死。
pub(crate) fn capture_argv(target: &str) -> [&str; 5] {
    [
        UTF8_CLIENT_FLAG,
        CAPTURE_SUBCOMMAND,
        PRINT_TO_STDOUT,
        TARGET_FLAG,
        target,
    ]
}

/// stderr 里「这台机上一个 tmux server 都没有」的形状。
///
/// `(针, 为什么它算这一档)` —— 加一条要连理由一起加。
pub(crate) const NO_SERVER_NEEDLES: &[(&str, &str)] = &[
    (
        "error connecting to",
        "🔴 **本轮真 tmux 打回来的那一句**（沙箱 tmux，显式 `-S <不存在的路径>`）：\
         逐字 `error connecting to /tmp/…/sock (No such file or directory)`。\
         ⚠ 它与下面那条**不是同一句话**，而我先前只登记了下面那条 ⇒ \
         第一趟门禁当场红（落进兜底档 `capture_failed`）。**留着这行来历**：\
         「tmux 连不上时会说 no server running」是个想当然，实测把它证伪了一半。",
    ),
    (
        "no server running",
        "tmux 在**默认 socket**（生产走的那条）上找不到 server 时的原话\
         （`no server running on /tmp/tmux-<uid>/default`）。\
         ⚠ **这一句本轮没有在真 tmux 上打过**：本模块的判据只造得出显式 `-S` 那条路，\
         而默认 socket 在沙箱里没法造出「无 server」态（那台沙箱自己在用 tmux）。\
         ⇒ 它今天只由 `every_registered_needle_lands_in_its_own_bucket` \
         用合成样本证明「落对了档」，**不是**「见过它」。",
    ),
];

/// stderr 里「server 在，而这个目标不存在」的形状。
///
/// `(针, 为什么它算这一档)`。
pub(crate) const NO_TARGET_NEEDLES: &[(&str, &str)] = &[
    (
        "can't find pane",
        "tmux 对 target-pane 解析失败时的原话。`=名:` 收的正是 target-pane（`F01`）",
    ),
    (
        "can't find session",
        "同上，只是解析走到了 target-session 那一支（旧版 tmux / 别的目标形）",
    ),
    (
        "no such session",
        "另一种措辞，一并收 —— 多认一种不会把别的失败误判成它（默认档是 `capture_failed`）",
    ),
];

/// 一次 `capture-pane` 的**原始读数**：退出码 ＋ 两条流。
///
/// 判法与取数分开，是为了让三态**各自测得到**：`no_server` / `no_such_session` /
/// 成功那三档由真 tmux 在隔离 socket 上打出来，而判法本身是纯函数、可以单独喂样本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawCapture {
    /// 退出码。`None` = 被信号打断（`ExitStatus::code()` 在 unix 上那一档）。
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// 「tmux 这个程序起不来」那一档的**唯一造句处**。
///
/// 抽成函数不是为了省字：`KR86D2` 要求这一档与「会话不存在」**分得开**，
/// 而真把 tmux 从 PATH 上摘掉不在沙箱门禁的射程内（那台沙箱自己要用 tmux）
/// ⇒ 判据只能打这一处。⚠ **如实登记**：本条钉的是「这一档存在、且与另外两档
/// 不同码不同话」，**不是**「在一台没装 tmux 的机器上实测过」。
pub(crate) fn tmux_unavailable(e: &std::io::Error) -> CmdErr {
    (
        "no_tmux",
        format!("起不来 tmux（这台机装了吗？PATH 里有吗？）：{e}"),
    )
}

/// 把一次原始读数判成结局。**纯函数。**
///
/// 判「抓没抓到」看**退出码**，不看输出是不是空 —— 空屏是合法的成功（模块头注那一段）。
pub(crate) fn classify(raw: &RawCapture) -> Result<String, CmdErr> {
    if raw.code == Some(0) {
        return Ok(raw.stdout.clone());
    }
    let said = raw.stderr.to_ascii_lowercase();
    let hit = |table: &[(&str, &str)]| table.iter().any(|(n, _)| said.contains(n));
    let tail = raw.stderr.trim();
    if hit(NO_SERVER_NEEDLES) {
        return Err((
            "no_server",
            format!("这台机上没有在跑的 tmux server ⇒ 无屏可抓。tmux 原话：{tail}"),
        ));
    }
    if hit(NO_TARGET_NEEDLES) {
        return Err((
            "no_such_session",
            format!("tmux server 在，而这个目标不存在 ⇒ 无屏可抓。tmux 原话：{tail}"),
        ));
    }
    Err((
        "capture_failed",
        format!(
            "抓屏失败，而这条失败不在已登记的两张针里（退出码 {:?}）—— \
             原样回包，别拿一个具体而错误的原因冒充它：{tail}",
            raw.code
        ),
    ))
}

/// 真起进程那一处 —— **全 crate 唯一一处 `capture-pane`**。
///
/// `socket`：`None` = 让 tmux 自己按环境解析默认 socket（**生产恒 `None`**）；
/// `Some(p)` = 显式 `-S <p>`。它**不是配置口**，是为了让「真能拿回内容」这件事
/// 在一个**隔离的 tmux server** 上测得出来 —— 同 `layering_guard::layer_sources_at`
/// 的「根可注入」：活体夹具要让**真判据本身**跑在真东西上，不是跑在它的复刻上。
fn spawn_capture(socket: Option<&str>, target: &str) -> Result<RawCapture, CmdErr> {
    let mut cmd = Command::new("tmux");
    if let Some(s) = socket {
        cmd.args(["-S", s]);
    }
    let out = cmd
        .args(capture_argv(target))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| tmux_unavailable(&e))?;
    Ok(RawCapture {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// [`capture`] 的本体，**socket 由调用方给**（理由见 [`spawn_capture`]）。
pub(crate) fn capture_on(socket: Option<&str>, name: &str) -> Result<String, CmdErr> {
    let name = checked_name(name)?;
    // Gate 1（`=name:` 精确匹配）—— 裸 `-t <名>` 会被 tmux 按「精确名 → 名字开头 → glob」
    // 解析，只有 `sib-2` 在时 `-t sib` 抓的是 `sib-2`（`F01`，`launch::exact_target` 头注）。
    let target = super::launch::exact_target(&name);
    classify(&spawn_capture(socket, &target)?)
}

/// 抓一次 `name` 这个 tmux 会话此刻的那一屏。**只读**：不 attach、不落 buffer、不写盘。
///
/// ⚠ **刻意不过身份门（Gate 2）**：这是一次只读快照，与 monitor 侧同族那一处口径一致
/// （`src-tauri/src/exec_site_registry.rs` 里 `capture_remote_pane` 那一行逐字：
/// 「只读快照，MASTERPLAN 明确不为它加身份门」）。破坏性动作那三道门在 [`super::gate`]，
/// 与本处无关 —— **别顺手给它加门，也别顺手把那三道门搬过来**。
pub(crate) fn capture(name: &str) -> Result<String, CmdErr> {
    capture_on(None, name)
}

/// 会话名的形状校验 —— **不在这里写第二份**。
///
/// 逐字复用 [`super::kill::parse_name`]：同一条理由（`:` / `=` 是 tmux 目标语法的一部分，
/// 名字里带它们会让 `=name:` 变成别的意思），一条规矩只许有一个住址。
/// ⚠ 它今天的入参形状是帧面命令的 `args`（一段 JSON），而本条 CLI 面只有一个位置参数
/// ⇒ **在这里包一层，不去改它的签名**：改签名会动到 `kill` 的帧面，而那份文件不在本件写区。
fn checked_name(raw: &str) -> Result<String, CmdErr> {
    super::kill::parse_name(&serde_json::json!({ "name": raw }))
}

/// 帧面入口（`K-R104`）：`capture-pane`。
///
/// `args`：`{name}`。回 `{name, screen}` —— `screen` 是那一屏的**原文**
/// （`R58`〔用 09-13〕逐字「直接抓屏给我看」：这一层一个字都不解析、不裁剪、不归一）。
///
/// 🔴 **与 CLI 面 [`run`] 共用同一个本体 [`capture`]**：`K33` 逐字「所有命令只许有一处，
/// 其他都是根据传参来调用」。两个面**只差取参数与包信封的方式**。
///
/// ⚠ **空屏是合法的成功**（模块头注那一段）—— 这里照样回 `ok:true` ＋ 空 `screen`，
/// 不许把它压成一条错误：那是 `KR101D1` ③ 明令禁止的那一形。
pub(crate) fn capture_for_inbound(
    args: &serde_json::Value,
) -> Result<serde_json::Value, (String, String)> {
    let name = super::kill::parse_name(args).map_err(|(c, m)| (c.to_string(), m))?;
    let screen = capture(&name).map_err(|(c, m)| (c.to_string(), m))?;
    Ok(serde_json::json!({ "name": name, "screen": screen }))
}

/// 一次性 CLI 入口：`--capture-pane <会话名>`。
///
/// 成功 ⇒ 那一屏**原样**写 stdout（同 `--read-session` 的透传口径）＋ exit 0；
/// 失败 ⇒ stderr 一行 `{code, message}` JSON ＋ exit 2（同 `--resolve` 那套信封）。
///
/// ⚠ **子命令那个 `--` 字面量刻意留在 `main.rs`，本文件一个都没有**：
/// `protocol_doc_guard::dispatch_registry_is_complete` 按「生产段里出现 `"--` 字面量」
/// 收人，本文件一旦持有它就得进 `DISPATCH_FILES`（那份文件不在本件写区）。
pub(crate) fn run(args: &[String]) -> i32 {
    let Some(name) = args.get(1) else {
        return emit_err((
            "invalid_args",
            "缺会话名 —— 这条子命令收一个位置参数：要抓的 tmux 会话名".to_string(),
        ));
    };
    if args.len() > 2 {
        return emit_err((
            "invalid_args",
            format!(
                "多余参数 {:?} —— 这条子命令只收一个位置参数（会话名），\
                 「抓几次」由调用方决定，不由它自己循环",
                &args[2..]
            ),
        ));
    }
    match capture(name) {
        Ok(screen) => {
            // 原样透传：pane 里本来就有换行，别再包一层 JSON 把它转义掉。
            print!("{screen}");
            0
        }
        Err(e) => emit_err(e),
    }
}

/// 错误信封：stderr 一行 JSON ＋ 退出码 2。形状与 `--resolve` 逐字相同。
fn emit_err((code, message): CmdErr) -> i32 {
    let line = serde_json::json!({ "code": code, "message": message });
    eprintln!("{line}");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(code: Option<i32>, stdout: &str, stderr: &str) -> RawCapture {
        RawCapture {
            code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    /// 隔离 socket 的路径 —— 每个测试一个，**显式 `-S`**，不碰任何默认 socket。
    fn iso_socket(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ccm-kr86-{}-{tag}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir.join("sock")
    }

    /// 在隔离 socket 上跑一条 tmux 命令。**无 `-S` 一律不跑** —— 本测试自己带。
    fn tmux_on(sock: &std::path::Path, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .arg("-S")
            .arg(sock)
            .args(args)
            .output()
            .expect("tmux 不可执行 —— 本测试要求环境有 tmux（刻意不静默跳过）")
    }

    /// ★★ `KR86D1` 的正题：**这条原语真能被调到，而且真的把内容拿回来了。**
    ///
    /// # 为什么是真 tmux，而不是一份脚本化的假探测器
    ///
    /// 件文件点名的失效方向是「判**源码里有 `capture-pane` 字面量**」——
    /// 那种判据在 `control/ccm/plan.rs` 今天就有那个字面量的情况下**恒绿**。
    /// 喂假探测器只强一格：它证得了「判法对」，证不了「这条命令真跑得通、真拿得回屏幕」。
    /// ⇒ 起一个**真的 tmux server**（隔离 socket，`-S`，`-f /dev/null` 不读用户配置），
    /// 往里打一段**只属于本测试的中性串**，再走**生产入口** [`capture_on`] 抓回来。
    ///
    /// ⚠ 断言的那个串**不取自夹具的名字**（`6g`：断言别取自夹具的名字 / 路径）。
    #[test]
    fn capturing_a_real_pane_brings_the_screen_back() {
        let sock = iso_socket("live");
        let marker = "ZQ7X-marker-line";
        let payload = format!("printf '%s\\n' {marker}; exec cat");
        let out = tmux_on(
            &sock,
            &[
                "-f",
                "/dev/null",
                "new-session",
                "-d",
                "-s",
                "kr86live",
                "sh",
                "-c",
                payload.as_str(),
            ],
        );
        assert!(
            out.status.success(),
            "隔离 socket 上建会话失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );

        // ⚠ **夹具侧的有界等待，不是被测行为的一部分**：pane 里那行字是由**另一个进程**
        //   写进 pty 的，tmux 什么时候把它读进屏幕缓冲不归本原语管。
        //   等不到就**失败**（不静默跳过），上限给足；真被测的那一下是循环体里那次
        //   `capture_on` —— 它每一轮都是一次完整的「抓一次」。
        //   🔴 这段循环住在**测试段**：`KR86D3` 禁的是**生产段**里的轮询，
        //   由 `readonly_guard::capture_is_read_only::the_capture_site_is_one_shot` 钉着。
        let mut screen = String::new();
        for _ in 0..200 {
            screen = capture_on(Some(&sock.to_string_lossy()), "kr86live")
                .expect("真会话必须抓得到，抓不到说明这条原语根本没通");
            if screen.contains(marker) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            screen.contains(marker),
            "抓回来的那一屏里没有刚打进去的那一行 —— \
             这条命令要么没跑、要么拿回来的不是屏幕内容。实得：{screen:?}"
        );

        // ★ 反向对照：同一个 server 上问一个**不存在**的会话，必须是「会话不存在」，
        //   不是空串、也不是「没有 server」。
        let miss = capture_on(Some(&sock.to_string_lossy()), "kr86nope")
            .expect_err("不存在的会话不许回成功");
        assert_eq!(
            miss.0, "no_such_session",
            "server 在、目标不在 ⇒ 该报 `no_such_session`。实得：{miss:?}"
        );

        // 收摊：只杀**本测试自己建的那个会话**（红线：不许 `kill-server`）。
        let _ = tmux_on(&sock, &["kill-session", "-t", "=kr86live:"]);
    }

    /// ★★ `KR86D2` 的正题之一：**「一个 server 都没有」是自己一档，真 tmux 打出来的。**
    #[test]
    fn a_socket_with_no_server_says_so_in_its_own_words() {
        let sock = iso_socket("dead");
        let e = capture_on(Some(&sock.to_string_lossy()), "whatever")
            .expect_err("没有 server 的 socket 上不许回成功");
        assert_eq!(
            e.0, "no_server",
            "没有 server 时报的是 `{}` —— 那一档被压进别的码里了。实得：{e:?}",
            e.0
        );
    }

    /// ★★ `KR86D2` 的刀口：**「没 tmux」与「会话不存在」两态分得开** —— 码与话都不许压成一句。
    ///
    /// ⚠ 诚实边界：`no_tmux` 那一档打的是 [`tmux_unavailable`]（那一档的唯一造句处），
    /// **没有**在一台真的没装 tmux 的机器上实测过 —— 沙箱自己要用 tmux。
    #[test]
    fn missing_tmux_and_missing_session_are_two_different_answers() {
        let absent = tmux_unavailable(&std::io::Error::from(std::io::ErrorKind::NotFound));
        let gone =
            classify(&raw(Some(1), "", "can't find pane: =x:")).expect_err("目标不存在不许回成功");
        assert_ne!(
            absent.0, gone.0,
            "「这台机没有 tmux」与「会话不存在」共用了同一个码 `{}` —— \
             压成一句之后，拿到这条错的人没法判断该去装 tmux 还是该去看会话名",
            absent.0
        );
        assert_ne!(
            absent.1, gone.1,
            "两档的码分开了、话却一模一样 —— 人读的是话，那等于没分开"
        );
        assert_eq!(absent.0, "no_tmux");
        assert_eq!(gone.0, "no_such_session");
    }

    /// ★ **空屏是合法的成功** —— 「抓不到」不许靠「输出是空的」判。
    ///
    /// 这一条挡的正是件文件写的那个失效方向：`会话不存在时回空串/假成功 ⇒ 红`。
    /// 反过来也要断：**真·空屏**必须是 `Ok("")`，否则一个刚起来的会话会被误报成不存在。
    #[test]
    fn an_empty_screen_is_a_success_and_a_failure_is_never_an_empty_string() {
        assert_eq!(classify(&raw(Some(0), "", "")).expect("空屏是成功"), "");
        for (code, stderr) in [
            (Some(1), "no server running on /tmp/x/sock"),
            (Some(1), "can't find pane: =x:"),
            (Some(1), "some brand new tmux wording"),
            (None, ""),
        ] {
            let e = classify(&raw(code, "", stderr)).expect_err("失败不许回成功");
            assert!(
                !e.1.trim().is_empty(),
                "失败那一档回了一句空话 —— 那与静默空串是同一件事。码：{}",
                e.0
            );
        }
    }

    /// ★ 兜底那一档**说不清但不撒谎**：两张针都不命中 ⇒ `capture_failed` ＋ stderr 原样带出。
    #[test]
    fn an_unrecognised_failure_carries_the_real_words_instead_of_a_guess() {
        let e = classify(&raw(Some(3), "", "tmux: 未来某个版本的新措辞"))
            .expect_err("非零退出不许回成功");
        assert_eq!(
            e.0, "capture_failed",
            "认不出来的失败被塞进了一个具体的码 —— 那是拿一个错答案冒充知识"
        );
        assert!(
            e.1.contains("未来某个版本的新措辞"),
            "认不出来时连原话都没带回去，这条错就成了死胡同。实得：{}",
            e.1
        );
    }

    /// ★ 两张针**逐条**喂一个合成样本，逐条要求它落在自己那一档。
    ///
    /// 分母 = 两张表的条数（现算）。没有这一条，往表里加一条从来没生效过的针也不会有人发现。
    #[test]
    fn every_registered_needle_lands_in_its_own_bucket() {
        assert!(
            !NO_SERVER_NEEDLES.is_empty() && !NO_TARGET_NEEDLES.is_empty(),
            "两张针里有一张空了 —— 那一档从此静默落进兜底"
        );
        for (needle, why) in NO_SERVER_NEEDLES {
            assert!(why.len() >= 20, "针 {needle:?} 没写清它为什么算这一档");
            let e = classify(&raw(Some(1), "", &format!("tmux: {needle} blah")))
                .expect_err("非零退出不许回成功");
            assert_eq!(e.0, "no_server", "针 {needle:?} 没落进 `no_server`");
        }
        for (needle, why) in NO_TARGET_NEEDLES {
            assert!(why.len() >= 20, "针 {needle:?} 没写清它为什么算这一档");
            let e = classify(&raw(Some(1), "", &format!("tmux: {needle}: =x:")))
                .expect_err("非零退出不许回成功");
            assert_eq!(
                e.0, "no_such_session",
                "针 {needle:?} 没落进 `no_such_session`"
            );
        }
    }

    /// ★ 名字的形状校验**真的复用了 `kill` 那一份**，不是这里又抄了一遍。
    ///
    /// 逐个坏形状打过去：它们必须全部在**起进程之前**就被拒（回 `invalid_args`），
    /// 而不是被拼进 `=name:` 送给 tmux。
    #[test]
    fn a_name_that_would_break_the_target_never_reaches_tmux() {
        for bad in ["", "  ", "a:b", "=a", "a\nb"] {
            let e = capture(bad).expect_err("坏形状的名字不许放行");
            assert_eq!(
                e.0, "invalid_args",
                "名字 {bad:?} 没被形状检查拦住，而是走到了 tmux 那一步。实得：{e:?}"
            );
        }
    }

    /// ★ argv 逐元素 —— 值级的「只读」在这里断一次（另一半在 `readonly_guard`）。
    #[test]
    fn the_argv_is_the_read_only_capture_form_in_this_exact_order() {
        assert_eq!(
            capture_argv("=x:"),
            ["-u", "capture-pane", "-p", "-t", "=x:"],
            "这一处发出去的 argv 变了 —— `-u` 必须排在子命令之前（K-R12），\
             `-p` 必须是「打到 stdout」（换成落 buffer 就不再只读）"
        );
    }
}
