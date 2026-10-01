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
    let miss =
        capture_on(Some(&sock.to_string_lossy()), "kr86nope").expect_err("不存在的会话不许回成功");
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
    let e =
        classify(&raw(Some(3), "", "tmux: 未来某个版本的新措辞")).expect_err("非零退出不许回成功");
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
    // 〔DUP3 §5 ③ ⑦〕规则换成 `gate_rules` 那一份：`"  "` 与 `=a` 是合法的已有会话名（attach 同样放行），不在这里。
    for bad in ["", "a:b", "a\nb", "a\u{202e}b"] {
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

/// ★★**跨语言金样**：界面直接收的这份成品，两侧读同一份 `tests/__fixtures__/tmux-control.golden.json`。
///
/// 守的要求：「**成品的两侧对拍**：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）；
/// 线上形状由一份跨语言金样钉住（后端测试产出 == 金样 · TS 解码器读同一份）」。
/// 抓屏从这一拍起由界面经通道直接问（`src/frontend/ui/tmux-control.ts::capturePane`），monitor 那一跳只搬字节 ——
/// 于是「成品长什么样」「拒绝码有哪几个」从此只有后端这一侧与金样说了算，TS 那一半在 `tests/frontend/ui/tmux-control.vitest.ts`。
///
/// 三格各自异源：请求样例过**生产**解析器（`kill::parse_name`，抓屏复用它）· 成品 == 生产构造器 [`reply`] ·
/// 码集合 == 后端登记表 `inbound::REGISTRY` 那一块（手写在 `inbound.rs`，不从本文件派生）。
#[test]
fn the_capture_product_matches_the_cross_language_golden() {
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/tmux-control.golden.json"))
            .expect("金样读不出来");
    let c = &g["capture-pane"];
    let name =
        super::super::kill::parse_name(&c["request"]).expect("金样的请求样例过不了生产解析器");
    let screen = c["reply"]["screen"]
        .as_str()
        .expect("金样的成品样例缺 `screen`");
    assert_eq!(
        reply(&name, screen),
        c["reply"],
        "后端出的抓屏成品与金样不相等 —— 改了键名或多 / 少一格，界面那一侧就会读成「两端对不上」"
    );
    let spec = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == "capture-pane")
        .expect("后端登记表里没有 `capture-pane`");
    let mut want: Vec<&str> = spec.codes.to_vec();
    want.sort_unstable();
    let mut got: Vec<&str> = c["codes"]
        .as_array()
        .expect("金样缺 `codes`")
        .iter()
        .map(|v| v.as_str().expect("码不是字符串"))
        .collect();
    got.sort_unstable();
    assert_eq!(
        got, want,
        "金样里的拒绝码与后端登记的不相等 —— 界面那张「码 → 一句话」的表就会漏一档或多一档"
    );
}
