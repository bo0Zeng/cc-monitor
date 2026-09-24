use super::*;

#[test]
fn a_real_session_id_is_accepted() {
    assert!(sid_is_safe("9d66c46d-bf88-4f99-877e-455555555555"));
    assert!(sid_is_safe("a_b-1"));
}

/// fail closed：能破坏 tmux 格式串 / 命令语义的都不许过。
///
/// ⚠ **`-t` 这种「像旗标」的值刻意不在这张表里** —— 它由 `[A-Za-z0-9_-]` 放行，
/// 而那是对的：`set-option -t <handle> @ccm_sid <值>` 里的值在非选项参数之后，
/// tmux 不会把它再当选项解析；且我们 argv 直传、不过 shell。
/// 写在这里是因为「看起来危险就该禁」是个很容易顺手加进来的错判 —— 真 sid（UUID）
/// 本来就带 `-`，收窄到禁 `-` 会把正常会话全挡掉。
#[test]
fn a_sid_that_could_break_the_format_string_is_rejected() {
    for bad in ["", "a b", "#{session_name}", "a;b", "$(id)", "a\nb", "a'b"] {
        assert!(!sid_is_safe(bad), "{bad:?} 不该被放行");
    }
    assert!(!sid_is_safe(&"x".repeat(129)), "超长 sid 不该被放行");
}

/// ★ 空 pane 目标必须挡住 —— 实测 `-t ''` 会静默解析成「某个会话」。
#[test]
fn an_empty_pane_target_is_rejected() {
    assert!(
        !pane_is_safe(""),
        "空目标放行 ⇒ sid 会被打到一个碰巧的会话上"
    );
    assert!(!pane_is_safe("%"), "只有 % 没有数字也不是 pane id");
}

#[test]
fn only_percent_digits_is_a_pane_id() {
    assert!(pane_is_safe("%0"));
    assert!(pane_is_safe("%12"));
    for bad in ["0", "$0", "@0", "%a", "%1x", " %1", "%1 "] {
        assert!(!pane_is_safe(bad), "{bad:?} 不该被当成 pane id");
    }
}

/// 一个不存在的 pid 拿不到 `TMUX_PANE` ⇒ 走「不在 tmux 里」，**不会**去猜一个会话。
///
/// ⚠ 本条**不起 tmux**：`pane_of` 在 `gate::probe` 之前返回 `None`。
#[test]
fn a_pid_without_tmux_pane_never_reaches_tmux() {
    // PID 0 在 Linux 上不是一个可读的 `/proc` 目录 ⇒ 读不到环境。
    assert_eq!(
        tag(0, "9d66c46d-bf88-4f99-877e-455555555555"),
        Outcome::NotInTmux
    );
}

/// sid 不合法时**连环境都不读**（顺序也是判据的一部分：先 fail closed 再做 IO）。
#[test]
fn a_bad_sid_short_circuits_before_any_io() {
    assert_eq!(tag(std::process::id(), "bad sid"), Outcome::RejectedSid);
}

// ═══════════════════════════════════════════════════════════════════════════
// 启动期令牌（`CCM_RBIND_TOKEN`）—— `设计/80 §8.7` 步 2
//
// 本组买到什么 / 买不到什么（写清楚，别读宽）：
//   ✅ 形状白名单 fail closed 到位（正/反两侧都断）
//   ✅ **真起一个带那个变量的子进程**，后端从它的 `/proc/<pid>/environ` 里读得出来
//      —— 这一格是本刀的正题，它不是「函数返回了一个我们喂进去的值」那种恒真
//   ✅ 那次读的射程没有悄悄变大（两个写死的常量，键名不是参数）
//   ✅ 令牌的值进不了日志
//   ❌ **端到端那一维买不到**：`§8.7` 明写「1 与 2 之间有顺序依赖（没有 token 进环境，
//      后端读不到）」—— 往启动命令里注这个变量（步 1）与本地半认这个 marker（步 3）
//      都不在本刀射程里。本组造那个进程用的是自己的夹具，**不依赖另一路**。
// ═══════════════════════════════════════════════════════════════════════════

/// 一个形状合法的令牌（32 个小写十六进制字符）。夹具用，值本身无意义。
const GOOD_TOKEN: &str = "0123456789abcdef0123456789abcdef";

#[test]
fn a_well_formed_launch_token_is_accepted() {
    assert!(token_is_safe(GOOD_TOKEN));
    assert!(token_is_safe(&"f".repeat(32)));
    assert!(token_is_safe(&"0".repeat(32)));
}

/// fail closed：**任何**偏离一律当「没有令牌」，不当「大概是它」。
///
/// 理由住 `token_is_safe` 的头注：下游用途是跨机器的 join 键，
/// 而 `设计/80 §8.5 ②` 那个布尔（「这个 sid 有没有令牌」）只有在
/// 「有 = 形状确定对」时才说得准。
#[test]
fn anything_that_is_not_exactly_thirty_two_lowercase_hex_is_rejected() {
    for bad in [
        "",                                  // 空
        "0123456789abcdef0123456789abcde",   // 31 个
        "0123456789abcdef0123456789abcdef0", // 33 个
        "0123456789ABCDEF0123456789abcdef",  // 大写
        "0123456789abcdefg123456789abcdef",  // 非十六进制字母
        " 123456789abcdef0123456789abcdef",  // 前导空格
        "0123456789abcdef0123456789abcde ",  // 尾随空格（**刻意不 trim**）
        "0123456789abcdef0123456789abcd\n",  // 换行
        "0123456789abcdef-123456789abcdef",  // 连字符
    ] {
        assert!(!token_is_safe(bad), "{bad:?} 不该被当成一个令牌");
    }
}

/// ★★ **本刀的正题：真起一个带着那个变量的进程，后端读得出它。**
///
/// # 为什么必须是真进程（而不是喂一个字符串给 `token_is_safe`）
///
/// 要证的不是「白名单会放行 32 个 hex」，是「**那条读 `/proc/<pid>/environ` 的路真的通**」
/// —— `设计/80 §8.3` 那张表声称「后端已经在读这个文件」，本条是它的现打核。
/// 喂字符串的版本对「`proc_env_var` 的键名拼错了」「`EnvRead` 那三支接错了」
/// 一律恒绿（本仓逐字「判据不在执行链上就等于不存在」）。
///
/// # 三组对照，缺任何一组读数都不可信
///
/// | 组 | 子进程的环境 | 期望 |
/// |---|---|---|
/// | 正题 | `CCM_RBIND_TOKEN=<32 hex>` | `Some(那个串)` |
/// | 阴性一 | **不设** | `None`（「这条会话没有令牌」） |
/// | 阴性二 | 设成一个形状不对的值 | `None`（fail closed，**不是**把它原样报出去） |
///
/// ★ 阴性二不是陪跑：把校验整条删掉，正题与阴性一**都还绿**，只有它红。
#[test]
fn the_token_is_read_back_out_of_a_real_child_process_environ() {
    /// 起一个 `sleep`，可选地给它注一个 `CCM_RBIND_TOKEN`。回 (子进程句柄, pid)。
    fn spawn_sleeper(token: Option<&str>) -> std::process::Child {
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("60");
        // ★ 先 `env_remove` 再按需 `env`：本测试进程自己的环境里要是碰巧有这个变量
        //   （开发机上完全可能 —— 步 1 落地之后 monitor 就在注它），
        //   阴性一会继承到它、当场变成一条假绿。
        // 🔴 **这里刻意写字面量，不用 `RBIND_TOKEN_ENV`** —— 死值验现打逮到的一格。
        //
        // 用那个常量的话，注进去的名字与读出来的名字**同源**：把常量改成
        // `CCM_RBIND_TOKEN_V2`（正是这个双写点真实的失效方向）之后，
        // 本条**照样全绿** —— 本仓逐字「恒等两侧同源会恒真」。
        // 换成字面量之后那一刀当场红。〔09-23 死值验刀 6 现打，先绿后红都量过。〕
        cmd.env_remove("CCM_RBIND_TOKEN");
        if let Some(t) = token {
            cmd.env("CCM_RBIND_TOKEN", t);
        }
        let kid = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("起不来 `sleep` —— 本条的夹具坏了，读数一个字都不能信");
        settle_past_the_exec_window(kid.id());
        kid
    }

    /// 🔴 **等这个子进程走出 `execve` 窗口** —— 不等就是一条真的间歇性假红。
    ///
    /// # 这不是保险起见，是现打出来的
    ///
    /// `platform/proc.rs::EnvRead` 的头注逐字记着支四：`/proc/<pid>/environ`
    /// **读得到、但回 0 字节** —— 成因之一是「exec 窗口（60–140 µs，进程刚 `execve`、
    /// mm 还没装好）」。本条第一版没等，**全量跑第一次就红了一发**；
    /// 单独跑 5 次全绿（并发一起跑时才够窄）。
    /// 现打（本机 09-23，`sleep` × 500，spawn 之后立刻读）：**1/500 回 0 字节**。
    /// ⇒ 每个探针约 0.2%，本条 3 个探针 ⇒ 每轮约 0.6%，正是那一发的来源。
    ///
    /// # 为什么判据是「environ 非空」而不是「读到那个令牌」
    ///
    /// 阴性组**本来就没有**那个令牌 —— 拿「读到令牌」当门会让阴性组死等。
    /// 「environ 非空」对四组是同一个门，而且它逐字就是那一支要区分的东西
    /// （`Unreadable` = 环境这一刻取不到 · `Unset` = 读得到、这个键不作数）。
    ///
    /// ⚠ **不许把超时那一格改成静默放过**：等不到就 `panic` —— 那时读到的
    /// `None` 说的是「环境取不到」，不是「没设」，拿它当读数就是假绿。
    fn settle_past_the_exec_window(pid: u32) {
        // exec 窗口是微秒级；让 500 次「重读 + 让出 CPU」覆盖它，不引入任何计时器。
        for _ in 0..500 {
            match std::fs::read(format!("/proc/{pid}/environ")) {
                Ok(b) if !b.is_empty() => return,
                _ => std::thread::yield_now(),
            }
        }
        panic!(
            "pid {pid} 的 environ 等了 500 轮仍然读不出字节 —— \
             夹具坏了（子进程死了？），此刻任何 `None` 都不是「没设」而是「取不到」"
        );
    }

    // ── 正题 ───────────────────────────────────────────────────────────────
    let mut kid = spawn_sleeper(Some(GOOD_TOKEN));
    let got = rbind_token_of(kid.id());
    let _ = kid.kill();
    let _ = kid.wait();
    assert_eq!(
        got.as_deref(),
        Some(GOOD_TOKEN),
        "后端读不出一个**真进程**环境里的 {RBIND_TOKEN_ENV} \
         —— `设计/80 §8.3` 那张「零件都在盘上」的表在这一格上不成立"
    );

    // ── 阴性一：压根没设 ───────────────────────────────────────────────────
    let mut bare = spawn_sleeper(None);
    let got = rbind_token_of(bare.id());
    let _ = bare.kill();
    let _ = bare.wait();
    assert_eq!(
        got, None,
        "没设那个变量却报出了一个令牌 —— 那会让 `§8.5 ②` 那个布尔恒真"
    );

    // ── 阴性二：设了，但形状不对 ⇒ fail closed ────────────────────────────
    let mut bad = spawn_sleeper(Some("NOT-A-TOKEN"));
    let got = rbind_token_of(bad.id());
    let _ = bad.kill();
    let _ = bad.wait();
    assert_eq!(
        got, None,
        "形状过不了却被原样报出去了 —— 校验被绕开了（fail closed 没兑现）"
    );
}

/// 一个读不出环境的 pid ⇒ `None`，**不是** panic、也不是一个瞎猜的值。
///
/// PID 0 在 Linux 上不是一个可读的 `/proc` 目录 ⇒ 走 `EnvRead::Unreadable` 那一支，
/// 而本调用方把它与「没设」合并（理由写在 `rbind_token_of` 的头注：四条路都是
/// 同一个保守答案）。
#[test]
fn a_pid_whose_environ_cannot_be_read_yields_no_token() {
    assert_eq!(rbind_token_of(0), None);
}

/// ★★ **那次读的射程不许悄悄变大**〔`设计/80 §8.7` 步 2 的硬约束 1〕。
///
/// # 它守的性质
///
/// `/proc/<pid>/environ` 里有用户**全部的密钥类环境变量**。本文件抠出来的必须
/// 全是**写死的常量**，键名**不许成为一维参数** —— 那样这一处就从「读两个写死的东西」
/// 变成了「任意环境变量读原语」。多一处 `proc_env_var(pid, …)` ⇒ 红，
/// 来回答「新那个键是什么、为什么它没把这里变成任意环境变量读」。
///
/// # 为什么本文件现在配得上一把自己的尺子（`K-R21` 那一拍**曾裁定不装**）
///
/// `accounts_query_tests.rs` 里那条判据的头注逐字登记着：`K-R21` PM 裁定
/// **不为 `identity_tag.rs` 装第二把尺子**，理由是「那个人群今天产出过 0 条假话，
/// 为它付一把新尺子的固定成本不划算」。
/// 🔴 **那一拍的前提今天变了两处**：① 本文件从读**一个**键变成读**两个**；
/// ② 新那个键的值是**敏感数据**（`§8.6 ③`）。⇒ 装。
///
/// ⚠ 它**不是** `accounts_query` 那条铁律的第二个住址：那一条说的是
/// `--session-accounts` 那条**查询**（今天仍是两个键），主语不是全后端。
#[test]
fn the_env_keys_this_file_reads_are_exactly_two_named_constants() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/identity_tag.rs"
    ));
    // 反空真地板：抽取塌了 ⇒ 下面几条都会零命中地绿。
    assert!(
        prod.len() > 2_000,
        "剥完只剩 {} 字节 —— 剥法坏了，本条此刻在空转",
        prod.len()
    );
    let total = prod.matches("proc_env_var(pid, ").count();
    assert_eq!(
        total, 2,
        "\n本文件生产段里 `proc_env_var(pid, …)` 有 {total} 处（登记 2 处）。\n\
         **多了** ⇒ 又读了第三个环境变量：来本文件头注那一格写清它是什么、\n\
         以及为什么这里仍然不是「任意环境变量读」原语。\n\
         **少了** ⇒ 有一条读回路被摘掉了。"
    );
    // ★ 数量对「读的是不是同一批东西」是瞎的 ⇒ 两个实参各自钉住。
    for arg in [
        "proc_env_var(pid, TMUX_PANE_ENV)",
        "proc_env_var(pid, RBIND_TOKEN_ENV)",
    ] {
        assert_eq!(
            prod.matches(arg).count(),
            1,
            "`{arg}` 在生产段里不见了 / 变形了 —— \
             键名要么被换成了别的东西，要么变成了一维参数"
        );
    }
    // 两个键的**字面值**也钉住：变量名是跨仓/跨路的契约，改了它读侧恒 `None`
    // 而 `None` 是合法值 ⇒ **不会有任何东西报错**（`K-P5f` 在 `CCM_LAUNCH_ID` 上栽的那一课）。
    assert_eq!(
        prod.matches("const RBIND_TOKEN_ENV: &str = \"CCM_RBIND_TOKEN\";")
            .count(),
        1,
        "启动期令牌的变量名被改了 —— 它是两路共用的契约（`设计/80 §8.7` 那张表钉死）"
    );
}

/// ★★ **令牌的值进不了日志**〔`设计/80 §8.6 ③`：它是敏感数据〕。
///
/// # 为什么本文件要自带这一条
///
/// 本仓那条日志白名单判据（`relay/creds_guard.rs` 的 `KS4`）的头注第 1 条诚实边界
/// 逐字写着「**只扫 `relay/`**」⇒ **它够不到本文件**。现打核过，成立。
/// ⇒ 不装这一条，「不进日志」就只是一句写在头注里的话。
///
/// # 判法：**装值的那几个名字**不许出现在任何日志宏的实参里
///
/// 人群可枚举（本文件生产段里的 `tracing::*!` / `println!` / `eprintln!` / `write*!`），
/// 所以能做白名单外的黑名单点名 —— 点的是**值**（`raw` / `token` / 那个常量的读回值），
/// 不是整行。
///
/// # ⚠ 它认不出什么（别读成「令牌不可能泄漏」）
///
/// 与 `creds_guard::KS4` 同一条边界：`let s = format!("{raw}"); warn!("{s}");`
/// 这种转手它看不见。**拦得住「顺手」，拦不住「刻意绕」。**
#[test]
fn the_token_value_never_reaches_a_log_macro() {
    let prod = crate::guard_support::production_code(include_str!(
        "../../../src/backend/control/identity_tag.rs"
    ));
    const LOG_MACROS: &[&str] = &[
        "tracing::warn!",
        "tracing::error!",
        "tracing::info!",
        "tracing::debug!",
        "tracing::trace!",
        "println!",
        "eprintln!",
    ];
    // 反空真地板：本文件生产段里**确实有**一处日志宏（`rbind_token_of` 里那句 warn）。
    // 一处都扫不到 ⇒ 抽取坏了，下面那格恒绿。
    let sites: Vec<&str> = prod
        .lines()
        .map(str::trim)
        .filter(|l| LOG_MACROS.iter().any(|m| l.starts_with(m)))
        .collect();
    assert!(
        !sites.is_empty(),
        "生产段里一处日志宏都没扫到 —— 抽取器坏了，本条此刻在空转"
    );
    // 一句日志宏可能跨行：从宏名那一行起收到它的 `);` 为止。
    let mut in_log = false;
    let mut offenders: Vec<String> = Vec::new();
    for line in prod.lines() {
        let t = line.trim();
        if LOG_MACROS.iter().any(|m| t.starts_with(m)) {
            in_log = true;
        }
        if in_log {
            for name in ["{raw}", "{raw:", "raw)", "raw,", "{token}", "{token:"] {
                if t.contains(name) {
                    offenders.push(format!("  {t}   （命中 {name:?}）"));
                }
            }
            if t.ends_with(");") || t.ends_with(")") {
                in_log = false;
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "\n★★ 有日志宏在印那个**装着令牌的变量**：\n{}\n\n\
         `设计/80 §8.6 ③`：令牌是敏感数据 —— 它出现在 `/proc/<pid>/environ`、\n\
         `/proc/<pid>/cmdline` 与 shell 历史里，而那几处按进程属主设权限；\n\
         **日志文件不是**。要诊断「注进去了但形状不对」，印**长度**就够\n\
         （`rbind_token_of` 里那句 warn 就是那么写的）。",
        offenders.join("\n")
    );
}
