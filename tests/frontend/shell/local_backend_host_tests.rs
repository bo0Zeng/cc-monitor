//! # 要求住址（本族分块，逐块点）： / `D7` / `D11`
//!
//! 核原文，逐块：
//! 〔脱离后接回自己的后端〕「前端不在也活着，前端起来能**接回去**」；「monitor 重启后还能接回自己的后端」。
//! 〔中转并进常驻后端〕「monitor 不单独起、不单独收中转」。
//! 〔本机起 / 停 / 一台只起一个〕「三个控件都留着」。
//! 〔两条找二进制的路共用一处〕`D1`「一个判定只有一个家」。
//! 〔起不来与死亡账要归因准确〕`D7`「失败要显式、归因要准确」；`D11` 展开「后端不在 ⇒ 这件事**不发生**，并且**说清楚为什么**」。
//! 〔本机取字节前先问 OS〕乙 · 迁移底账：「**这道闸要消失**」。〔几条量具〕丙，服务上面那几块的源码扫描。
//! 〔监听口要钥匙 · 脱离后不留僵尸 · 起真后端必须 fail-closed 地隔离用户 tmux〕三块已升格：`INVARIANTS §48.1` · `§48.2` · `§48.3`（
//! 「本机后端宿主三条」）。三块各自的判据逐个点名在那三小节的「谁在守」里。〔IV1 改指 2026-09-25〕
//! ⚠ 按字样数会把注释 / 字符串里的测试属性字样也数成判据；族内真判据 49 条。

use super::*;

/// M2：**两条载体起本机后端时交的是同一份环境**：中转端口 ＋（隔离跑时）家；没有另指凭据 / 注解位置的格。
///
/// ① 行为：[`backend_env_from`] 产出的键集两向相等（期望手写）；端口值取自注入侧常量（异源）。
/// ② 接线：`start_local_backend` 里 `backend_env()` **恰好算一次**、排在常驻那条会 `return` 的分支之前，
///    常驻那条与被监护那条各交一次（文本，如实登记：按行为量要真跑 `start_local_backend`，吃真实的 `~/.cc-monitor`）。
#[test]
fn both_carriers_hand_the_backend_the_same_relay_envs() {
    let keys = |envs: &[(String, String)]| -> Vec<String> {
        let mut k: Vec<String> = envs.iter().map(|(k, _)| k.clone()).collect();
        k.sort_unstable();
        k
    };
    let plain = backend_env_from(&|_| None);
    assert_eq!(
        keys(&plain),
        vec!["CCM_RELAY_PORT"],
        "没隔离跑时交给本机后端的不是恰好中转端口那一格"
    );
    assert_eq!(
        plain[0].1,
        relay_route_core::PORT.to_string(),
        "端口不是注入侧那个常量 —— 注入侧拼的 URL 会指向一个没人听的口"
    );
    let iso = backend_env_from(&|k| (k == "CCM_DATA_DIR").then(|| "/iso/home".to_string()));
    assert_eq!(keys(&iso), vec!["CCM_DATA_DIR", "CCM_RELAY_PORT"]);
    assert!(iso.contains(&("CCM_DATA_DIR".into(), "/iso/home".into())));
    // 空串的家 == 没设（不交一格空值）。
    assert_eq!(
        keys(&backend_env_from(
            &|k| (k == "CCM_DATA_DIR").then(String::new)
        )),
        vec!["CCM_RELAY_PORT"]
    );

    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let at = guard_core::find_pinned(&prod, "pub fn start_local_backend()")
        .unwrap_or_else(|e| panic!("`start_local_backend` 不是恰好一处：{e}"));
    let body = &prod[at..];
    let made = guard_core::find_pinned(body, "let handed = backend_env();")
        .unwrap_or_else(|e| panic!("`start_local_backend` 里没有恰好一处算那份环境：{e}"));
    let detached =
        guard_core::find_pinned(body, "match start_detached(&resolve, &landing, &handed) {")
            .unwrap_or_else(|e| panic!("常驻那条没交那份环境（或交了别的）：{e}"));
    let supervised = guard_core::find_pinned(body, "handed,\n")
        .unwrap_or_else(|e| panic!("被监护那条没交那份环境：{e}"));
    assert!(
        made < detached && detached < supervised,
        "顺序：算({made}) < 常驻那条({detached}) < 被监护那条({supervised}) —— \
         算在常驻分支之后 = 生产主路上后端拿不到中转端口"
    );
}

/// M1：**monitor 生产段里零处起中转**（`--relay` 零命中），正控取自后端那一处。
///
/// 本机中转住进常驻后端之后，monitor 这一半再没有任何理由在 argv 里写 `--relay`；
/// 有一处 = 有人又在 monitor 里另起了第三个进程（那个孤儿的来路）。
/// 正控：同一把尺子喂一段**合成的**起法（带 `--relay` 的那一形 argv）数得到它 ⇒ 尺子不瞎。
/// ⚠ 刻意不 `include_str!` 后端那份文件：那会多一条跨半边的编译期边（`cross_half_edge_registry`），为一条正控不值。
#[test]
fn the_monitor_half_starts_no_relay_process() {
    let needle = format!("\"--{}\"", "relay");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(files.len() > 100, "只扫到 {} 份 —— 取法坏了", files.len());
    let hits: Vec<String> = files
        .iter()
        .filter(|(_, raw)| guard_core::contains_word(&guard_core::production_code(raw), &needle))
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert_eq!(
        hits,
        Vec::<String>::new(),
        "monitor 生产段里又出现了起中转的 argv"
    );
    let positive = guard_core::production_code(&format!(
        "fn start(exe: &Path) {{\n    let pid = start(&exe, &[{needle}], port)?;\n}}\n"
    ));
    assert!(
        guard_core::contains_word(&positive, &needle),
        "正控失灵：合成的那一处起法也数不到 —— 尺子瞎了，上面那条零命中是空真"
    );
}

// ══ 第二道锁 ② 的**唯一入口** ═══════════
//
// # 它为什么存在
//
// 只靠一条**同行文本形状**（「本体里有一行同时含 `CCM_E2E_TMUX_SHIM_BIN` 与 `.expect(`」）判 ② 的话，
// 把取值写成 `var(SHIM).unwrap_or_else(|_| var(<真 PATH 那个变量>).expect(..))` ——
// **同一行上两样都还在** ⇒ 判据说合规，而变量缺席时 shim = 真 PATH，
// backend 沿真 PATH 找到真 tmux ⇒ 盖用户的 `[50]`。那就是**降级裸跑**。
//
// # 买到的是什么：**把「fail-closed」这件事从文本形状换成运行期读数**
//
// 下面 `require_tmux_shim` 是**纯函数**（喂它一个 `Result`，不读环境）
// ⇒ 「拿不到就炸」这条性质能被一条**普通 `#[test]`**（不带 `#[ignore]`）
// 在**默认门禁里真跑一遍**：[`the_one_shim_gate_really_fails_closed`]。
// 谁把这里改成降级，**门禁当场红** —— 那是机器守的，不是纪律、也不是文本钉。
//
// # 买不到的是什么（别读大）
//
// 「**这条测试走不走这个口**」仍然是一条**文本判据**（守卫里的 ㈠：本体里出现
// `demand_tmux_shim(`）。攻击面从**5 处**缩到**1 处**，不是缩到 0：
// 一条根本不进人群的测试（诚实边界 5「第四种来历」）照旧够不着。
//
// 「**口是 fail-closed 的**」不等于「**走口的测试就是 fail-closed 的**」：
//   let shim = std::panic::catch_unwind(|| crate::local_backend_host::demand_tmux_shim(".."))
//       .unwrap_or_else(|_| std::env::var("PATH").unwrap_or_default());
// 闭包真跑了 · `panic!` 真发生在口里 —— 而变量缺席时 `shim` 就是**真 `PATH`**。
// 两者之间隔着「**谁能接住那声 panic**」，门禁里没有一条守它（守卫头注**诚实边界 8**）。
//
// # 🔴 说得准的话：这道闸买到的是**防一部分手滑**，买不到**挡住存心绕**
//
// 说得准的写法是「**量过的这几形挡得住**」，不是「手滑挡得住」
// （写「全部 / 任何」就是在报一个数，同句给分母 —— 而「手滑的所有形状」这个分母给不出）。
// 不用「保证」「买断」「就还在」这类词描述它。
//
// **买到**：
// · 谁把口里那句 `panic!` 改成降级 ⇒ **门禁当场红**（只点
//   [`the_one_shim_gate_really_fails_closed`]）；
// · 反空真那半格不是仪式（「恒炸」的实现过不了）；
// · 「自己现取变量」的写法被拒。
//
// **买不到**：
// ① **调用方在「口」与「调用点」之间接住那声 panic 再降级**（上面那一形）。
//    ⚠ **这条缝不是纯理论的**：`catch_unwind` 本来就是这两个文件里的**现成写法** ——
//    `local_backend.rs` 的 `local_stdio_consumer_guarded` 里一处，
//    本文件 [`the_one_shim_gate_really_fails_closed`] 自己也用了一处 ⇒ **照抄的成本为零**。
//    （另一侧：`catch_unwind` 得**有意**去接住 panic，这一形与「手滑」不是同样可能。）
// ② 多行块注释那一形（把真调用整段包进 `/* */`、换个桩值）由共用剥法的块注释词法挡着，
//    边界见守卫头注**诚实边界 9**。
//
// 🔴🔴 **别再在这里加文本钉**：每给调用处钉一个形状，就有另一个**同行为**的形状绕过去。
//   这条路**买不到这条性质**，两条理由各自独立成立：
//   ① **panic 是可以被接住的** —— 入口做得再好也守不住调用方；
//   ② **生产合法地需要那条不隔离的路**（真实用户就是不带 shim 起后端）
//      ⇒ **在这两个前提下**（生产要留一条不带 shim 的入口 · 测试与生产同 crate、
//        因此调得到那条入口），**没有类型能让「带 shim」变成强制的**。
//        ⚠ 前提是承重的，别把这句读成无条件的：把这几条 e2e 搬进一个独立的集成测试
//        crate、只暴露带见证的入口，这句话就不成立了 —— 那是另一件事的体量。
//   ⇒ 真正让这条性质成立的做法是让 **backend 只碰别人显式交给它的那个 tmux server**；
//     那要动生产段，**另立一件**，这里不改。
//
//   ⚠⚠ **它也管不到「调用排在 spawn 之前还是之后」** —— 那一格没钉，登记在
//   下面守卫头注 ㈠ 那一段。炸得晚，backend 已经起来了。
/// 那个变量的名字。
///
/// ⚠ **别把下面这句写宽**（写「只有 / 全部」就是在报一个数，同句给分母）。
/// 分母 = 仓内全部 `.rs`（去 `target` / `node_modules` / `vendor` / `.git` / `dist` /
/// `embedded-backends`）里非 `//` 打头的代码行。提到这个名字的代码行里，
/// 多数是**判据自己的 `const`**（`SHIM` / `PRIVATE_TMUX` / 本 `const` / 报文串）；
/// 真去 `std::env::var(..)` **取它的值**的是本模块的 [`demand_tmux_shim`]（经本 `const` 取）
/// ＋ 两条 `let _ = std::env::var(..)` 的**纯读点名行** ——
/// 它们存在的理由是喂饱姊妹守卫 `every_ignored_test_here_…` 的 `c.contains(PRIVATE_TMUX)`，
/// **不取值、不影响 fail-closed**。
/// 这里刻意不写行号：行号随每次改动推移，嵌进句子里下一次改动就成了假话。
/// 要重打就站在工作树根：
/// · `git ls-files '*.rs' | grep -vE '(^|/)(target|node_modules|vendor|dist|embedded-backends)/'`
///   再逐文件取「`trim_start()` 后不以 `//` 打头、且含 `CCM_E2E_TMUX_SHIM_BIN`」的行。
/// ⇒ 只能这么说：**「拿它当 shim 目录用」的取法只有 [`demand_tmux_shim`] 这一处**；
///   **不能说**「全仓只有一处碰这个变量」——那句话是假的。
pub(crate) const TMUX_SHIM_VAR: &str = "CCM_E2E_TMUX_SHIM_BIN";

/// **纯函数**：把「取变量的结果」变成 shim 目录。**拿不到就炸，绝不降级。**
///
/// 它刻意**不自己读环境** —— 读环境的是下面那个 [`demand_tmux_shim`]。
/// 分开的理由只有一条：**这样「fail-closed」才测得了**，
/// 不用去动进程级的环境变量（那会跟别的测试线程打架，也会让读数依赖跑法）。
pub(crate) fn require_tmux_shim(got: Result<String, std::env::VarError>, why: &str) -> String {
    match got {
        Ok(dir) => dir,
        Err(e) => panic!(
            "要 {TMUX_SHIM_VAR}（{e:?}）—— {why}\n\
                 ★ 被起的后端一上来就往它连得到的 tmux server 装三条**全局** hook\n\
                 （固定槽位 [50]，**没有关掉它的开关**）⇒ 不隔离就是去改用户真实 tmux 的状态。\n\
                 ⚠ 这不是理论：08-11 打没过用户 9 个真实会话；08-26 / 08-27 / 08-29 各盖过一次 [50]。\n\
                 ⇒ 必须 **fail closed**：拿不到 shim 就炸，**绝不降级裸跑**。\n\
                 跑法：bash tests/e2e/local-backend-supervise.sh"
        ),
    }
}

/// 五个落点取 shim 的**唯一**口子。加新的起真后端的测试，也从这里取。
pub(crate) fn demand_tmux_shim(why: &str) -> String {
    require_tmux_shim(std::env::var(TMUX_SHIM_VAR), why)
}

/// **停得掉 · 起得回来 · 状态跟着变**（实测）。
///
/// # 为什么按 `/proc` 看而不是读状态字段
///
/// 「状态说停了」不等于「进程真没了」。
/// 只读我们自己维护的那个字段是**自证** —— 把 `stop()` 整个换成「只改字段」也会绿。
/// ⇒ 每一步都按 pid 看 `/proc`，状态字段只作为**第二条**断言。
///
/// # 沙箱：`HOME` 与 `CLAUDE_CONFIG_DIR` 都要设
///
/// backend 的 `resolve_agent_home()` 是
/// `$CLAUDE_CONFIG_DIR` 优先、`$HOME/.claude` 兜底。
/// 只设 `HOME` 的话跑起来一切正常，隔离却是假的。
///
/// ⚠ 本条**不调 `start_local_backend()`** —— 那个函数吃的是**真实**的 `~/.cc-monitor/bin`
/// 且不接受环境注入 ⇒ 在测试里调它就会读用户真实的配置目录。
/// 它那半（宿主知识 + 幂等）由下面那条机检管。
/// ⚠⚠ **诚实边界**：它 `#[cfg(embedded_backends)]` 门着，
/// 而 **CI 的 `cargo test` 之前一步都不铺 `src/frontend/shell/embedded-backends/`**
///（铺它的是 `release.yml`，不是 `ci.yml`）⇒ `build.rs` 不置 cfg
/// ⇒ **本条在 CI 上等于不存在**，只在「开发机上、且 `embedded-backends/` 齐」时跑。
/// 姊妹条 `the_local_backend_host_really_registers_an_inbound_client` 登记了同一条边界（10c）。
///
/// ⚠ 另一条边界：它**不调生产的 `start_local_backend` / `stop_local_backend`**
/// （前者读真实 `~/.cc-monitor`、不接受环境注入）⇒ **生产的停口零覆盖**，
/// 那一格由 `the_stop_command_really_calls_this_module` 的源码接线钉补上。
///
/// # ★★ `#[ignore]` + fail-closed + 真用 shim：测试不许动这台机器
///
/// 本条起一个真后端，而后端一上来就**无条件**往它连得到的 tmux server 装三条**全局** hook
/// （固定槽位 `[50]`，**没有关掉它的开关**）。作为普通 `#[test]` 的话，
/// **任何人在铺了 `embedded-backends/` 的树上跑一次 `cargo test`**（**包括用户自己
/// clone 下来跑一遍**）**都会改这台机器的 tmux 全局状态** —— 靠纪律守不住。
///
/// 三道锁一起上，少一道都不够：
/// ① `#[ignore]` ⇒ 默认 `cargo test` **跑不到它**；
/// ② `CCM_E2E_TMUX_SHIM_BIN` **fail-closed**（`expect`，且排在**任何 spawn 之前**）
///    ⇒ 连 `cargo test -- --ignored` 也不能裸跑它；
/// ③ 那个 shim **真的挂进被监护后端的 `PATH` 最前面**。
///
/// # ⚠⚠ ③ 的射程要**分两格看**
///
/// | 跑法 | ③ 买到什么 | 为什么 |
/// |---|---|---|
/// | **文档指定的那条**：`bash tests/e2e/local-backend-supervise.sh` | **冗余** | `tests/e2e/tmux-shim.sh` 里那句 `export PATH="$TMUX_SHIM_BIN:$PATH"` 已经把 shim 挂进了**测试进程自己的 `PATH`**，而 `supervise_with_stdio` 只做 `env_remove("TMUX")` + 逐条 `env(k, v)`、**从不 `env_clear()`** ⇒ 子进程本来就继承那份带 shim 的 `PATH` |
/// | **手工 `cargo test -- --ignored`**（变量设上、但 shim 不在自己 `PATH` 上） | ★ **这一格是真的** | 没有 ③ 的话，backend 继承的是那个人的真 `PATH` ⇒ 沿真 `PATH` 找到真 tmux ⇒ 盖用户的 `[50]` |
///
/// ⇒ **③ 值得留，但别把它读成「e2e 那条路上也靠它挡着」** —— 那条路上挡住的是 shim 的继承。
/// 看着 ③ 的判据是 `every_test_that_starts_the_real_backend_demands_a_private_tmux`
/// 的 ㈡ 与 ㈢ **两格**（没有它们，③ 退掉门禁一条都不红）：
/// - ㈡ **写法**（`"PATH"` 与 `"{<绑定名>}:` 同行，插值**紧跟开引号**）—— 串首，不只是「冒号左边」：
///   `format!("/usr/bin:{shim}:{}", PATH)` 写法上 shim 排第二，冒号左边那种判准照样放行；
/// - ㈢ **处数**（`PATH` 这个 env 键在本体里**恰好写一次**）—— 同一张 `envs` 里再追加一条
///   `("PATH", …)`，`supervise_with_stdio` 的 `for (k, v) in &envs { cmd.env(k, v); }` **后写的赢**，
///   backend 拿到的 `PATH` 里一点 shim 都没有，而 ㈡ 照样说合规。
/// ⚠ 两格**刻意分开**（一个值不装两件事）：破 ㈡ 要「特意在串首前面塞一段」，破 ㈢ 只要「顺手再加一条环境变量」。
///
/// ⚠⚠ 而**只清 `TMUX` 是不够的**（`supervise_with_stdio` 内部就在清它）：
/// `TMUX` 一空，tmux 客户端**回落到默认 socket** `/tmp/tmux-$UID/default` ——
/// 那**正是**用户那台 server。隔离必须靠**显式选择器**（shim 强插 `-L`/`-S`），
/// 不能靠「不继承某个变量」。
///
/// # 🔴 复跑纪律：**换了 `embedded-backends/` 的有无之后，必须 `touch src/frontend/shell/build.rs`**
///
/// 本条由 `#[cfg(embedded_backends)]` 门着 ⇒ **它在不在，取决于 `build.rs` 那一趟看见了什么**。
/// 在**同一个 `CARGO_TARGET_DIR`** 里把 `src/frontend/shell/embedded-backends/` 从「无」加成「有」，
/// **`build.rs` 不重跑**，读到的是「无 emb」那一档的条数，`touch build.rs` 之后才把本条算进来。
/// ⚠⚠ **这个坑看不出来**：两档的输出面长得一模一样。本条与下面姊妹条各留一份这条纪律。
///
/// ⇒ **纪律**：换 emb 状态之后 `touch src/frontend/shell/build.rs`；
/// 最省事的做法是**换一个全新的 target 目录名**（`build.rs` 从零跑一遍，结构上撞不到）。
///
/// ⚠ **机制没有实验证明，别当结论用**：候选是那几个二进制常是 `cp -p` 拷进来的，
/// **mtime 停在拷贝源那一刻**（比上一次 build 还旧）⇒ cargo 的指纹看不出「输入变新了」。
/// `build.rs` 里那句 `rerun-if-changed` 是无条件登记的（`for arch in [..]` 里直接 `println!`，
/// 不在任何 `exists()` 分支中），归因不到它身上。
#[cfg(all(embedded_backends, target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "起真 backend ⇒ 会装全局 tmux hook。走 tests/e2e/local-backend-supervise.sh 那条带 shim 的路"]
fn the_local_backend_host_can_be_stopped_and_started_again() {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    // ★★ **fail closed，而且排在一切之前**：拿不到 shim 就当场炸，绝不降级裸跑。
    //    降级裸跑 = 去改用户真实 tmux server 的 `[50]` 槽位。
    // 🔴 **理由串提成一个局部常量，好让那一行「一行到底」。**
    //
    // 理由串放下一行的话，`cargo fmt` 会把它排成「`let shim =` 换行再写调用」——
    // 语义一个字节没变，而守卫的读法会把它判成「`=` 后断行」那一形假阳。
    // ⇒ 不去放宽读法，把那一行写成它要的形状。
    const WHY: &str = "本条起真后端，而后端一上来就往它连得到的 tmux server 装全局 hook";
    let shim = demand_tmux_shim(WHY);
    let _guard = crate::inbound_client::local_origin_test_lock();
    let bin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("embedded-backends")
        .join("cc-monitor-backend-x86_64");
    let home = std::env::temp_dir().join(format!("p2s-restart-{}", std::process::id()));
    let cfg_dir = home.join(".claude");
    std::fs::create_dir_all(cfg_dir.join("projects")).expect("建沙箱 HOME");
    let envs = vec![
        ("HOME".to_string(), home.display().to_string()),
        (
            "CLAUDE_CONFIG_DIR".to_string(),
            cfg_dir.display().to_string(),
        ),
        // ★ 隔离**真的用上**：shim 目录挂在 `PATH` 最前面，backend shell out 的 `tmux`
        //   会被强插显式选择器 ⇒ 它连得到的只有 e2e 自己那台 server。
        (
            "PATH".to_string(),
            format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
        ),
    ];
    let now = || {
        Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        })
    };
    let spawn = || {
        local_backend::supervise_with_stdio(
            bin.clone(),
            vec![
                local_backend::BACKEND_SEP.into(),
                local_backend::STREAM_WORD.into(),
                "--tail-only".into(),
            ],
            envs.clone(),
            local_backend::CrashLimits::default(),
            now(),
            Arc::new(|e| println!("[P2s 实测] {e:?}")),
            Some(Arc::new(|i, o| {
                local_backend::local_stdio_consumer(&local_backend::StdioRoute::local(), i, o)
            })),
            crate::spawn_managed::local_backend_supervised(),
        )
    };
    let wait_channel = |want: bool| -> bool {
        for _ in guard_core::wall::rounds() {
            let on = crate::inbound_client::local_client().is_some();
            if on == want {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    };
    let alive = |pid: u32| Path::new(&format!("/proc/{pid}")).exists();

    // ── 起 ────────────────────────────────────────────────────────
    *LOCAL_BACKEND.lock().expect("锁") = Some(spawn());
    assert!(
        wait_channel(true),
        "到兜底上限通道没登记上 —— backend 没起来"
    );
    // 通道先登记、pid 后落盘：等状态里出现 pid（同一个 5s 上限），不在登记的那一刻就问死。
    let status_pid = || {
        tauri::async_runtime::block_on(crate::backend_control::backend_status(
            cmd_wire::LOCAL.into(),
        ))
        .expect("查状态")
        .pid
    };
    let pid1 = guard_core::wall::rounds()
        .find_map(|_| {
            status_pid().or_else(|| {
                std::thread::sleep(Duration::from_millis(50));
                None
            })
        })
        .expect("到兜底上限状态里一直没有 pid —— backend 起来了却没报 pid");
    assert!(alive(pid1), "状态给了 pid={pid1}，但 /proc 里没有这个进程");
    // ★ 本条带 `#[ignore]`，由 `tests/e2e/local-backend-supervise.sh` 驱动，
    //   而那个脚本的收尾自检是「**标记数 < 跑成的测试数 ⇒ 有测试提前退出**」
    //   ⇒ 不打标记的话，它一进那条 `--ignored` 路就会把套件判红，
    //   而红的理由是**假的**（不是断言没走完，是本条从来不打标记）。
    println!("E2E-OK P2s 本机后端起得来（pid={pid1}，/proc 里真有）");

    // ── 停：进程必须**真的**没了 ──────────────────────────────────
    LOCAL_BACKEND
        .lock()
        .expect("锁")
        .as_ref()
        .expect("刚起的")
        .stop();
    let mut gone = false;
    for _ in guard_core::wall::rounds() {
        if !alive(pid1) {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        gone,
        "`stop()` 之后 pid={pid1} 还在 /proc 里 —— 「状态说停了」不等于「进程真没了」。\n\
             ★ 这条断言刻意不读状态字段：只改字段的实现会让那种断言绿着过。"
    );
    assert!(wait_channel(false), "进程没了，通道却还挂在登记表里");
    println!("E2E-OK P2s `stop()` 之后 pid={pid1} 真的从 /proc 里消失、通道也摘了");

    // ── 再起：必须是**新的**一条命 ────────────────────────────────
    *LOCAL_BACKEND.lock().expect("锁") = Some(spawn());
    assert!(
        wait_channel(true),
        "停了之后起不回来 —— 那就只有「停」没有「起」"
    );
    let pid2 = tauri::async_runtime::block_on(crate::backend_control::backend_status(
        cmd_wire::LOCAL.into(),
    ))
    .expect("查状态")
    .pid
    .expect("再起之后没有 pid");
    assert!(alive(pid2), "再起给了 pid={pid2}，但 /proc 里没有");
    assert_ne!(
        pid1, pid2,
        "两次拿到同一个 pid —— 那说明「再起」其实什么都没做，\n\
             或者句柄根本没被换掉（`OnceLock` 时代就是这个形态：写一次就锁死）。"
    );
    println!("E2E-OK P2s 停了之后起得回来，而且是**新的**一条命（{pid1} → {pid2}）");

    // ── 收尾 ──────────────────────────────────────────────────────
    if let Some(h) = LOCAL_BACKEND.lock().expect("锁").take() {
        h.stop();
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ★ **接线钉之三：停口**〔D 阶段补审 08-11 新增〕。
///
/// 补审逐字：起那半有 `the_startup_path_really_calls_this_module` 钉着，
/// **停那半没有对称物** —— `backend_stop → stop_local_backend` 这根线断了不会有任何东西红，
/// 而 `stop_local_backend` 里那句 `g.take()`（头注专门解释「不 take 会让下一次『起』
/// 误以为还在跑」）改成 `g.as_ref()` 也不会红。
///
/// ⚠ 射程：本条是**源码接线钉**，只证明那根线写在那里；
/// 「停了进程真没了」那半由 `local_backend_host::tests` 里那条实测管（而它今天 `#[cfg]` 门着，见 10c）。
#[test]
fn the_stop_command_really_calls_this_module() {
    let dc = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_control.rs"
    ));
    // 停口改成 `async`、本机那一支交阻塞线程池跑（`spawn_blocking(…::stop_local_backend)`，传的是函数本身）。
    guard_core::find_pinned(
        &dc,
        "spawn_blocking(crate::local_backend_host::stop_local_backend)",
    )
    .unwrap_or_else(|e| {
        panic!(
            "`backend_control` 的停口没有接到 `stop_local_backend`（{e}）——\n\
                 那么 UI 上的「停」对本机是个空动作，而它照样回一句成功的话。"
        )
    });
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let at = guard_core::find_pinned(&me, "pub fn stop_local_backend(").expect("停口不在了");
    let body: String = me[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    guard_core::find_pinned(&body, "let _life = LIFECYCLE").unwrap_or_else(|e| {
        panic!("`stop_local_backend` 没攥着起停串行锁（{e}）—— 停的同时可以又起一个")
    });
    guard_core::find_pinned(&body, ".take();").unwrap_or_else(|e| {
        panic!(
            "`stop_local_backend` 不再把句柄 `take()` 走（{e}）。\n\
                 ★ 留着它的后果很具体：`start_local_backend` 的判据是「句柄在表里 = 在跑」\n\
                 ⇒ 停完之后再点「起」会被当成「已经在跑」拒绝，本机后端再也起不回来。"
        )
    });
}

/// T5 **本机「停」走同一条一次性子命令**：`stop_detached` 恰好调一次 `run_resident_stop`，自己不发信号、不强杀、不等；
/// 整份生产段里没有发 `-TERM` / `-KILL` 的路（HX1 那套 SIGTERM → 等 → SIGKILL 删干净）；`run_resident_stop` 起的是那个二进制 ＋ `--resident-stop`。
/// 要求：「本机那一格也改走同一条 `--resident-stop`（本机远端同形），
/// monitor 侧 `local_backend_host.rs` 那套 SIGTERM → 等 → SIGKILL 删掉」。
#[test]
fn the_local_stop_rides_the_same_one_shot_supervisor() {
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let body_of = |head: &str| -> String {
        let at = guard_core::find_pinned(&me, head).unwrap_or_else(|e| panic!("{e}"));
        me[at..]
            .lines()
            .skip(1)
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n")
    };
    let stop = body_of("fn stop_detached(");
    assert_eq!(stop.matches("run_resident_stop(").count(), 1, "{stop}");
    for banned in ["kill(", "SIGTERM", "SIGKILL", "sleep("] {
        assert_eq!(
            stop.matches(banned).count(),
            0,
            "`stop_detached` 里又出现了 `{banned}`：{stop}"
        );
    }
    for banned in ["\"-TERM\"", "\"-KILL\"", "Command::new(\"kill\")"] {
        assert_eq!(
            me.matches(banned).count(),
            0,
            "生产段又长出发信号的路：`{banned}`"
        );
    }
    let run = body_of("fn run_resident_stop_within(");
    // 本机落点就是 `ccm` ⇒ `ccm -- <停常驻那个词>`（词住 `relay_route_core::RESIDENT_STOP`；打头的 `--` 让它当后端用）。
    guard_core::find_pinned(
        &run,
        "cmd.args([local_backend::BACKEND_SEP, relay_route_core::RESIDENT_STOP])",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // 等带期限（与本机探针同一段等法），交的环境与起它时同一份。
    for needle in [
        "crate::backend_once::capture_full(",
        ".envs(handed.iter().cloned())",
    ] {
        guard_core::find_pinned(&run, needle).unwrap_or_else(|e| panic!("{e}"));
    }
    assert_eq!(
        run.matches("wait_with_output").count(),
        0,
        "又成了没有期限的等"
    );
    guard_core::find_pinned(
        &body_of("fn run_resident_stop("),
        "run_resident_stop_within(bin, RESIDENT_STOP_DEADLINE)",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    guard_core::find_pinned(&run, "crate::remote_resident::read_stop(")
        .unwrap_or_else(|e| panic!("{e}"));
}

/// ★★ **本机只许用本平台能跑的二进制**。
///
/// 今天字节按 (OS, arch) 查表（`byte_table`，规矩 4）⇒ 「先问 OS」不再是一道闸，
/// 是**键的一半**。本条钉：本机那条取用点的键是**这台机器自己**（`Key::this_machine()`）、取的是后端那一类，
/// 而且键在取字节之前定下。本机那一行的承诺（表 B）不在这里判：放之前问手上那份字节自己（`place-verdict`）。「musl 字节只落在 Linux 格」那一半是表的行为，钉在
/// `byte_table_tests::musl_bytes_only_ever_land_on_linux_cells`。
///
/// ⚠ 射程：它是**源码判据**，只证明取用点这样写着；证明不了「Windows 上真的不会释放」——
/// 那要一台 Windows（归 `auto-e2e`）。如实登记，不拿源码判据冒充跨平台实测。
#[test]
fn the_local_backend_only_takes_a_binary_this_platform_can_run() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let body = body_of(&prod, "pub fn start_local_backend() -> StartOutcome {");
    let take = guard_core::find_pinned(&body, "byte_table::choose(").unwrap_or_else(|e| {
        panic!(
            "`start_local_backend` 里没有恰好一处 `byte_table::choose(`（{e}）—— 取用点变了就来改本条"
        )
    });
    let key = guard_core::find_pinned(&body, "Key::this_machine()")
        .unwrap_or_else(|e| panic!("`start_local_backend` 取字节的键不是「这台机器自己」（{e}）"));
    assert!(key < take, "键在取字节之后才定 —— 取用点的形状变了");
    let tail = &body[take..];
    assert!(
        tail.contains("choose(this_machine)") && !tail.contains("Route::"),
        "本机那条取用点不是「这台」那一格（表 B 不在 monitor 判，取用点不该再带 origin）：\n{tail}"
    );
}

/// P2s（`C8`①）：**已经在跑就不重复起** —— 钉的是**机制**，不是那句诊断文案。
///
/// # 这条判据被补审判过一次死刑
///
/// 原版全部内容是「`find_pinned(prod, "本机后端已经在跑…")` + 位置在函数头之后」。
/// 审计逐字：「它不验 `return`、不验条件、不验「不 spawn」、甚至不验那句话在
/// `start_local_backend` 体内」。骗过它的改法（幂等当场失效而判据全绿）：
///
/// ```text
/// if h.current_pid().is_some() {
///     tracing::warn!("本机后端已经在跑（C8①：每台机只许一个）");   // 不 return
/// }
/// ```
///
/// # 现在钉三件（都在切出来的函数体内）
///
/// ① 判据是 **`g.is_some()`**，不是 `current_pid()` —— 后者有竞态：
///    `pid` 由 supervise 线程在 spawn 后才写，本函数返回时它还是 0，门形同虚设。
/// ② 「检查」与「存句柄」之间不许有窗口：起停串行锁 `LIFECYCLE` 在检查之前拿、**恰好一次**、绑在 `_life` 上活到函数尾
///    （句柄表本身只在检查与存那两下拿 —— 起的那几秒不攥着它，查状态不被挡住）。
/// ③ 那一支必须 **`return`** —— 只打日志不返回等于没有门。
#[test]
fn starting_twice_does_not_spawn_a_second_local_backend_host() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let at = guard_core::find_pinned(&prod, "pub fn start_local_backend(")
        .expect("入口不在了 —— 改了名就来改本条");
    let body: String = prod[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.len() > 200,
        "切出来的函数体只有 {} 字节 —— 切错了，本条在空转",
        body.len()
    );

    guard_core::find_pinned(&body, "if g.is_some() {").unwrap_or_else(|e| {
        panic!(
            "`start_local_backend` 的幂等判据不是 `if g.is_some()`（{e}）。\n\
                 ⚠ 若换回了 `current_pid()`：那是**竞态**判据 —— `pid` 由 supervise 线程在 spawn\n\
                 之后才写，本函数返回时仍是 0 ⇒ 串行点两下就能起出第二个后端，\n\
                 而第一个句柄被覆盖、**再没有任何代码能 stop 它**。"
        )
    });

    let life = guard_core::find_pinned(&body, "let _life = LIFECYCLE.lock()").unwrap_or_else(|e| {
        panic!(
            "起的那一趟没有从头攥着起停串行锁（{e}）—— 「检查」与「存句柄」之间就有窗口，那正是双起的入口。\n\
             ⚠ 绑在 `_life` 上才活到函数尾；写成 `let _ = …` 当场就放了。"
        )
    });
    assert!(
        life < body.find("if g.is_some() {").expect("上面刚 pin 过"),
        "串行锁拿在检查之后 —— 检查那一刻没被串住"
    );

    let guard_at = body.find("if g.is_some() {").expect("上面刚 pin 过");
    let tail = &body[guard_at..];
    let ret = tail.find("return").unwrap_or(usize::MAX);
    let close = tail.find("\n    ").unwrap_or(0);
    assert!(
        ret < tail.len() && ret < close.max(ret + 1) + 400,
        "幂等那一支里找不到 `return` —— 只打日志不返回等于没有门（补审给的就是这个骗法）"
    );
}

// ══════════════════════════════════════════════════════════════════
// 常驻那条路的判据
// ══════════════════════════════════════════════════════════════════

/// 按**行**切一个函数体：从 `fn <name>(` 那行起，到第一行**恰好是 `}`** 为止。
/// （与本文件既有那几条判据同一个切法 —— 别造第二种。）
///
/// # ★★ 反空真自检：这里为什么**没有**一个字节数地板〔09-09 换掉〕
///
/// 原先第三个参数是 `min: usize`（「这个体最少该有多少字节」，逐条给，
/// `is_detached` 那条给的是 60）。**09-09 那趟 `cargo fmt --all` 当场把它打红**：
/// rustfmt 把 `DETACHED` 那四行链式调用压回一行，体从 84 字节变成 **57 字节 < 60**，
/// 判定行逐字「`pub fn is_detached(` 切出来的体只有 57 字节（下限 60）——
/// 切错了，调用方那条判据在空转」。
///
/// ★ **那是一句假诊断**：切法一个字都没错，是**这把尺子把排版当成了性质的一部分**。
/// 本文件 [`braced_block`] 的头注逐字写着「假诊断比不红更贵：它把人引到错的地方」。
///
/// ⇒ 处置**不是**把 60 调到 50 —— 那是把闸往下调，本仓对此有明纪律。
/// 是把这把尺子换成**真正表达那个性质**的两条，两条都与排版无关：
///
/// | 要守的性质 | 今天的判据 | 旧地板买到过没有 |
/// |---|---|---|
/// | 窗口**有界**：切法停在了列 0 那一行收尾上，不是一路吞到文件尾 | 收尾那一行**找得到** | ❌ **一格都没买** —— 它只有下界，吞得越多越绿 |
/// | 窗口**非空**：切出来的体里有代码 | 至少一行非空白 | ✅（但顺带把排版也断言了进去） |
/// | 切到的**是那一段**、不是别人的体 | **调用方自己那根锚** | ❌ 从来不是它买的 |
///
/// 第三行那条今天**每一个调用点都已经有**（`contains(..)` / `matches(..) == 1` /
/// `find(..).unwrap_or_else(panic)`），下面 `both_production_spawn_paths_..`
/// 那条更是把它逐字叫作「反向自检②」。
///
/// ★ 「字节数地板 → 语义锚点」这个换法**本仓已有判例**：
/// 规则 `strip_clean.rs::main_production_section_keeps_its_load_bearing_items` 的头注写着
/// 「字节数地板挡不住『单个文件被剥空/剥过头』…… 这条用**语义锚点**直接钉住那一类失效」。
fn body_of(prod: &str, head: &str) -> String {
    let at = guard_core::find_pinned(prod, head)
        .unwrap_or_else(|e| panic!("切不出 `{head}`（{e}）—— 改了名就来改本条"));
    let rest: Vec<&str> = prod[at..].lines().skip(1).collect();
    // ★ 反空真自检①（**有界**）：列 0 那一行收尾找不到 ⇒ 窗口一路吞到了文件尾。
    let Some(end) = rest.iter().position(|l| *l == "\u{7d}") else {
        panic!(
            "`{head}` 一路切到文件末尾都没遇上列 0 那一行收尾 —— 这个窗口**无界**：\n\
                 它把后面所有 item 都吞了进来，调用方每一条 `contains` 都会在**别人的代码**上恒真。\n\
                 ★ 这一格是旧那个字节数下限**从来没买到过**的：下界只嫌少，不嫌多。"
        )
    };
    let body = rest[..end].join("\n");
    // ★ 反空真自检②（**非空**）：体里一行代码都没有 ⇒ 下面那些否定式恒真地绿。
    assert!(
        body.lines().any(|l| !l.trim().is_empty()),
        "`{head}` 切出来的体里一行代码都没有 —— 切错了，调用方那条判据在空转。\n\
             ★ 这一格挡的是「切法退化成空串」：`!contains(..)` 那一族在空串上全都恒真。"
    );
    body
}

/// 按**花括号配平**切一个块：从 `marker` 之后的第一个 `{` 起，到配平的那个 `}` 止。
///
/// ⚠ **本文件只许有这一份切法**〔08-27 抽出来〕：退出臂那条判据
/// 原先内联了一份，`重-2` 那条又要一份 —— 而这个文件自己的注释逐字写着
/// 「刻意不另造一种 —— 两种切法迟早在同一段代码上给出两个答案」。
/// `lo` / `hi` 是这一块的字节数上下限，**逐条给**（反空真：切错了就红，别在空串上绿着）。
///
/// # ⚠⚠ `marker` 必须**唯一** —— 它认的是身份，不是位置
///
/// 只取 `prod.find(marker)` 的文本第一处、不断言唯一的话，
/// 像 `Adopt::Refused(why) =>` 这种在 `local_backend_host.rs` 的生产段里**命中 2 处**的标记
/// （`start_detached` 那一臂 · `probe_and_attach_after_spawn` 里那条 `=> Err(why),`）
/// 就只是**恰好**切中了排在前面的那一臂。
///
/// ★ 要害在**靠位置对的，不是靠身份对的**：换个函数顺序就**静默换人**，
/// 而换人之后红出来的诊断是「配平切错了」—— **那是一句假诊断**（切法没错，是标记指到了
/// 另一个人身上）。本文件下面逐字写着「**假诊断比不红更贵：它把人引到错的地方**」。
/// ⇒ 唯一性先断言。同文件的姊妹切法 [`body_of`] 走 `guard_core::find_pinned`（整行相等），
/// 本函数是这个文件里唯一一个靠文本位置定人群的切法，所以这一行由它自己补上。
fn braced_block<'a>(prod: &'a str, marker: &str, lo: usize, hi: usize) -> &'a str {
    let hits = prod.matches(marker).count();
    assert_eq!(
        hits, 1,
        "`{marker}` 在人群里命中 {hits} 处（该恰好 1 处）——\n\
             ★ 命中 ≥2 处说明这个标记**不唯一**：本条切的是哪一处**取决于文本顺序**，\n\
             此刻它很可能正在**切错人**，而切错之后红出来的诊断会是「配平切错了」——那是假诊断。\n\
             ★ 命中 0 处 = 它搬家或改名了，本条会零命中地绿。\n\
             ⇒ 把 `marker` 加长到能**认出身份**（比如带上 `=> {{`），**别放宽这条断言**。"
    );
    let at = prod
        .find(marker)
        .unwrap_or_else(|| panic!("找不到 `{marker}` —— 它搬家或改名了，本条会零命中地绿"));
    let bytes = prod.as_bytes();
    // ⚠ 从 `marker` **之后**找块起点，不是从它开头找 —— `marker` 自己可能就带着一对
    //   花括号（`StartOutcome::Failed { reason, looked_at }` 那种解构），
    //   从开头找会切到那一对上去（实测：切出 21 字节）。
    // ★ **例外：`marker` 自己以 `{` 结尾时，那个 `{` 就是块起点。**
    //   〔08-27 实测：上面那条唯一性断言逼着把标记加长成 `Adopt::Refused(why) => {`，
    //   而跳过它之后找到的第一个 `{` 落在块体里 `format!("{port} 口上那个 backend…")`
    //   的 `{port}` 上 ⇒ **切出 6 字节**、红在「配平切错了」—— 又是一句假诊断。〕
    //   加长标记到 `… => {` 是本文件给「同名两臂」消歧的标准手段，所以这一格在这里接住。
    let scan_from = if marker.ends_with('{') {
        at + marker.len() - 1
    } else {
        at + marker.len()
    };
    let open = (scan_from..bytes.len())
        .find(|&i| bytes[i] == b'{')
        .unwrap_or_else(|| panic!("`{marker}` 之后找不到块起点"));
    let mut depth = 0i32;
    let mut end = bytes.len();
    for i in open..bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                end = i + 1;
                break;
            }
        }
    }
    let body = &prod[open..end];
    assert!(
        body.len() > lo && body.len() < hi,
        "`{marker}` 切出来 {} 字节（该在 {lo}..{hi} 之间）—— 配平切错了，本条会零命中地绿",
        body.len()
    );
    body
}

/// ★★ **脱离的落点不许住 `backend/`，而且那个注入点真的被用上了。**
///
/// # 两半，缺一半都不成立
///
/// ① **`backend/` 那半干净** —— 那条现成的 `platform_home_guard::platform_forms_live_only_at_home`
///    管着禁针（含 `std::os::unix`）。本条只**补它够不到的一格**：
///    整棵 `backend/` 里 `process_group(` 零命中。
///    ⚠ 那条判据查的是**禁针字面**，有人把脱离藏进一个跨平台包装 crate（或换个名字）
///    就零命中地绿 —— 所以下面②必须是**位置性**的，不能只查「文件里有这个词」。
///
/// ② **位置性**（「钉『文件里有 `process::id()`』钉不住它用在哪」）：
///    · `start_local_backend` 体内**恰好一处** `start_detached(`；
///    · 它排在 `start_or_extract(` **之前**（回落是回落，不是主路）；
///    · `spawn_detached` 体内真的有 `process_group(0)` 与 stdio 全 null。
#[test]
fn the_detach_landing_is_the_host_layer_and_the_injection_is_really_used() {
    // ── ① `backend/` 那半 ──────────────────────────────────────────
    // 平台无关那一道由 `platform_home_guard::platform_forms_live_only_at_home` 判（人群含这一组）；本条只补 `process_group(`。
    // `backend/` 目录没了：宿主无关那一道改看逐个点名的那一组（`backend_client_guard_tests.rs::GUARDED`）。
    let files: Vec<(std::path::PathBuf, String)> = crate::backend_client_guard::guarded_sources();
    assert!(
        files.len() >= 5 && files.iter().all(|(_, s)| !s.is_empty()),
        "只读到 {} 份那一组的文件 —— 抽取坏了，本条在空转",
        files.len()
    );
    let hits: Vec<String> = files
        .iter()
        .filter(|(_, s)| guard_core::production_code(s).contains("process_group("))
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(
        hits.is_empty(),
        "`backend/` 里出现了 `process_group(`：{hits:?}\n\
             它来自 `std::os::unix::process::CommandExt`，而 `std::os::unix` 在\n\
             `platform_home_guard::platform_forms_live_only_at_home` 的禁针里 —— 那一层不认识平台。\n\
             ⇒ 脱离的落点只能是**宿主知识层**（`local_backend_host.rs`），\n\
             照 `platform::fs::make_executable` 那个注入先例把「怎么起」喂进去。\n\
             ⚠ 没有平台例外表：壳里平台形态只许住 `platform/`。"
    );

    // ── ② 位置性 ────────────────────────────────────────────────────
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let start = body_of(&me, "pub fn start_local_backend(");
    assert_eq!(
        start.matches("start_detached(").count(),
        1,
        "`start_local_backend` 体内 `start_detached(` 不是恰好一处 —— \
             零处 = 注入点没被用上（常驻整条路死在那里，而所有判据照样绿）；\
             多处 = 有第二条起法。"
    );
    let a = start.find("start_detached(").expect("上面刚数过");
    let b = start
        .find("start_or_extract(")
        .expect("回落那条路不在了 —— 那非 Linux 平台就没有本机后端了");
    assert!(
        a < b,
        "`start_or_extract(`（今天那条）排在 `start_detached(` 前面 —— \
             那样常驻那条路永远走不到，而它「写在那里」这件事看起来一切正常。"
    );

    // ⚠ needle 要**唯一确定那一个事实**：`fn spawn_detached(` 命中 2 处
    //   （`#[cfg(target_os = "linux")]` 那个 + 非 Linux 的诚实降级壳）。
    //   「把 needle 扩到能唯一确定那个事实的大小」。
    //   带上参数那一行 ⇒ 唯一。
    let spawn = body_of(
        &me,
        "fn spawn_detached(\n    bin: &std::path::Path,\n    extra_env: &[(String, String)],",
    );
    // ⚠ 最后一针是**常量名**不是那个串：串本身住在常量声明里，
    //   而它与后端那侧逐字一致由 `the_listen_env_names_are_the_same_string_on_both_sides` 管。
    //   钉「这里用的是那个常量」而不是「这里出现了那个串」，正好挡住「顺手在这里写死一个串」。
    // 🔴**第一针从 `process_group(0)` 换成 `Lifetime::Detached`。**
    //   换的是**谁写它**，不是写不写：那句平台原语现在住 `spawn_managed.rs`
    //   （唯一出口），这一处声明的是**策略**。⚠ 换针之后这条判据仍然数得出同一件事 ——
    //   把这一行的 `Detached` 改成 `JobKillOnClose`（脱离当场失效、行为全变）⇒ 本条红。
    //   ⚠ **第二针 `Stdio::null()` 照旧留着**：stdin/stdout 那两根仍由本函数自己写，
    //   而「stdio 全 null」里 stderr 那一根由第三条策略（`StderrSink::Null`）管
    //   ⇒ 两针合起来才盖得住原来那一句话，所以它也单独成针。
    for needle in [
        "Lifetime::Detached",
        "StderrSink::Null",
        "Stdio::null()",
        "RESIDENT_ENV",
    ] {
        assert!(
            spawn.contains(needle),
            "`spawn_detached` 体内找不到 `{needle}`。\n\
                 三样一起才叫脱离：`Lifetime::Detached`（平台原语 `process_group(0)` 由唯一出口代办；\
                 否则 Ctrl-C 的 SIGINT 打到整个前台进程组）\
                 + stdio 全 null（今天它 153ms 内死掉的**真正原因**就是那对管子；\
                 stdin/stdout 在本函数里，stderr 由 `StderrSink::Null` 管）\
                 + 协议改走这台家里的套接字（管子没了总得有别的说话方式）。"
        );
    }
    assert!(
        spawn.contains("env_remove(\"TMUX\")"),
        "`spawn_detached` 没有清掉 `TMUX` —— tmux 客户端在 `TMUX` 有值时**按它给的 socket 走**，\
             `TMUX_TMPDIR` 完全不起作用。那正是 08-11 打没用户 9 个真实 tmux 会话的机制。"
    );
}

/// ★★ **`detached` 的源头只能是「起它的时候走没走那条路」。**
///
/// 拿 `channel` / `pid` 反推是**假信号** —— 拿 `SSH_CONNECTION` 反推就是这一形：
/// 「假信号不会报错，它只是**一直说是**」，而在只有正例的测试里永远绿。
/// ⇒ 本条既钉**接线**（读的是哪一份记录），也给一格**负例**。
#[test]
fn detached_reads_the_path_that_was_taken_not_a_guess() {
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let is_det = body_of(&me, "pub fn is_detached(");
    assert!(
        is_det.contains("DETACHED"),
        "`is_detached` 不再读那条「真的走过脱离路」的记录 —— 它现在读的是什么？"
    );
    for forbidden in ["client_for", "current_pid", "LOCAL_BACKEND", "channel"] {
        assert!(
            !is_det.contains(forbidden),
            "`is_detached` 体内出现了 `{forbidden}` —— 那是**反推**。\n\
                 反推出来的信号不会报错，它只会一直说是（`SSH_CONNECTION` 那一形）。"
        );
    }
    let dc = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_control.rs"
    ));
    // 命令本身是 `async`、本体在 `backend_status_now`（`spawn_blocking` 里跑）。
    let status = body_of(&dc, "fn backend_status_now(\n    origin: String,");
    assert_eq!(
        status.matches("local_backend_host::is_detached()").count(),
        1,
        "`backend_status` 里 `is_detached()` 不是恰好一处 —— 零处 = 那一格没接上（前端永远读到缺席）"
    );

    // ── 负例①：**纯函数**的那张真值表（四格全走到）─────────────────
    assert!(
        !detach_wanted(false, None),
        "非 Linux 平台不许走脱离那条路 —— `process_group` 只在那儿有"
    );
    assert!(
        !detach_wanted(true, Some("1")),
        "`{NO_DETACH_ENV}=1` 关不掉脱离 —— 那本条要的负例就永远走不到了"
    );
    assert!(detach_wanted(true, None), "Linux + 没关 ⇒ 该走那条路");
    assert!(
        detach_wanted(true, Some("  ")),
        "空串按「没设」算 —— shell 里 `export {NO_DETACH_ENV}=` 是常态"
    );

    // ── 负例②：**没走那条路 ⇒ `backend_status` 必须回 `detached: false`** ──
    //    这一格走的是**真命令**，不是读源码。
    let _guard = crate::inbound_client::local_origin_test_lock();
    assert!(
        DETACHED.lock().expect("锁").is_none(),
        "测试开始时 `DETACHED` 就不是空的 —— 前一条判据留了状态，本条读数不可信"
    );
    let st = tauri::async_runtime::block_on(crate::backend_control::backend_status(
        cmd_wire::LOCAL.into(),
    ))
    .expect("查状态");
    assert_eq!(
        st.detached,
        Some(false),
        "没走过脱离那条路，`detached` 却不是 false —— 那一格在猜"
    );
    let remote =
        tauri::async_runtime::block_on(crate::backend_control::backend_status("某台远端".into()))
            .expect("查远端状态");
    assert!(
        remote.detached.is_none(),
        "远端的 `detached` 不是 null —— 那是在替一台看不见的机器编读数"
    );
}

/// ★★ **跨 crate 字面量对拍**：常驻开关的 env 名两侧必须逐字一样。
///
/// 漂了**不会报错**：起出来的后端会把它当成「没设」而走 stdio 那条路，
/// 于是宿主等在一个永远不会有人 bind 的口上，日志里只有一句「断开」。
/// 形状：`include_str!` 读对侧那份、抠出字面量、逐字比。
///
/// ⚠ 这条 `include_str!` 是一条**跨半边**（monitor→backend）的语料边，
/// 已登记在 `cross_half_edge_registry`（它自己那条判据管着「不许长到生产段」）。
#[test]
fn the_listen_env_names_are_the_same_string_on_both_sides() {
    // env 名住后端最下层那一份（帧面监听引它）。
    let names = include_str!("../../../src/backend/platform/child_env.rs");
    for (backend, vis, rust_name, ours) in [(names, "pub ", "RESIDENT", RESIDENT_ENV)] {
        let head = format!("{vis}const {rust_name}: &str =");
        let line = backend
            .lines()
            .find(|l| l.trim_start().starts_with(&head))
            .unwrap_or_else(|| panic!("backend 那份里找不到 `{head}` —— 名字改了就来改这条"));
        let lit = line
            .split('"')
            .nth(1)
            .unwrap_or_else(|| panic!("`{rust_name}` 那一行不是 `= \"…\";` 的形状"));
        assert_eq!(
            lit, ours,
            "常驻开关的 env 名两侧漂了（`{rust_name}`）：backend {lit:?} / monitor {ours:?}\n\
                 ⚠ 这种漂**不会报错** —— backend 会当成「没设」走 stdio 那条路，\n\
                 而宿主等在一个永远没人绑的套接字上。"
        );
    }
}

/// ★★ **两条「找那个二进制」的路必须给同一个答案** —— 今天靠的是「只有一份实现」。
///
/// # 怎么守：**抽一份共用的，再钉住两条路都真的走它**（名字说的是「顺序」，引它的地方不跟着动）
///
/// 守的那件事：同一台机上两条路不许找到不同的二进制（那正是「每台机 N 个后端」的另一个入口）。
/// 「两份手写实现 + 一条只对拍顺序的判据」守不住它：只改一份的取法（例如释放失败改说一句
/// 分得开的话）、顺序两边都不动 ⇒ 对拍照样绿，而两条路对同一个失败已经在说两句性质不同的话。
/// ⇒ 两条路共用 `resolve_or_extract` 一份，本条钉住两条路都真的走它（与下面防空转那条同形）。
///
/// # 它今天钉四件
///
/// ① 那份共用的解析**存在**（切得出体，且体里那三问俱在 —— 反空真）；
/// ②③ 两条路**各自恰好一处**调它；
/// ④ 两条路体内**都不再有**自己那套取法（`extract_embedded_to(` /
///    `resolve_beside_this_exe(` 一处都不许剩；「问产物带没带」那一问搬去了宿主的 `byte_table::choose`，
///    两条路共用同一个交进来的 `embedded`）——
///    少了④，谁在旁边**再写一份**并列的取法，②③ 照样绿。
///
/// ⚠ **诚实边界**：它是**约定型守卫**（查源码形态）——
/// 挡得住「接线被删 / 旁边又长出第二份取法」，挡不住「把共用那份自己改坏」。
/// 共用那份的内容由 `local_backend` 那侧的
/// `the_self_extract_path_really_asks_the_product_whether_it_carries_one` 钉。
#[test]
fn the_two_resolution_paths_still_agree_on_the_order() {
    let theirs = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend.rs"
    ));
    // ① 那份共用的解析真的在，而且三问俱在 —— 它是下面三条的地板。
    let shared = body_of(&theirs, "pub fn resolve_or_extract(");
    for needle in [
        "match embedded {",
        "extract_embedded_to(",
        "extraction_failure_reason(",
    ] {
        guard_core::find_pinned(&shared, needle).unwrap_or_else(|e| {
            panic!(
                "那份共用的解析体内 `{needle}` 不是恰好一处（{e}）——\n\
                     它是本条的地板：共用那份自己塌了，下面「都走了它」这几条\n\
                     买到的就只是「都走了一个空壳」。\n逐字：{shared}"
            )
        });
    }
    let mine = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    for (who, prod, head) in [
        ("常驻这条", &mine, "fn resolve_backend_bin("),
        ("今天那条", &theirs, "pub fn start_or_extract("),
    ] {
        let body = body_of(prod, head);
        // ②③ 恰好一处调用 —— 两处就说不清哪一处才是它的答案。
        guard_core::find_pinned(&body, "resolve_or_extract(").unwrap_or_else(|e| {
            panic!(
                "{who}路体内 `resolve_or_extract(` 不是恰好一处（{e}）——\n\
                     一处都没有 ⇒ 它又自己找去了，而「自己找」就是两份取法各漂各的那一形：\n\
                     同一台机上两条路对同一个失败说两句性质不同的话。\n逐字：{body}"
            )
        });
        // ④ 自己那套取法一处都不许剩。
        for own in [
            "extract_embedded_to(",
            "byte_table::choose(",
            "resolve_beside_this_exe(",
        ] {
            assert!(
                !body.contains(own),
                "{who}路体内又出现了 `{own}` —— 旁边长出了第二份取法。\n\
                     ★ 少了本条，「都调了那份共用的」照样绿，而真正跑起来的是哪一份没人说得准。\n逐字：{body}"
            );
        }
    }
}

/// ★★ **防空转**：两个生产落点**真的走了**那份共用的 `ETXTBSY` 分类。
///
/// # 它买的是哪一格
///
/// `local_backend` 那四条判据全长在**纯函数**上（判别 / 重试 / 上限 / 两句话）——
/// 实现只要不接到调用点就是死代码，而那四格照样绿。这一格钉的是**两个落点各自的
/// 函数体里真的走了那一份**，而且**没有各自留一条裸 `spawn()` 直接放弃的路**。
///
/// 切函数体那一步**复用本文件的 `body_of`，
/// 不抄第三份切法**：本文件 `block_of` 的头注逐字写着「两种切法迟早在同一段代码上
/// 给出两个答案」。反向自检因此是**两层**：`body_of` 自带的「窗口有界 + 非空」，
/// 加下面那条**锚点自检**（取到的体里必须有它自己那句独有的话）——
/// 少了任一层，取不到体时下面几条都会在一段不相干的文本上恒真地绿。
/// 两层都与排版无关（按字节数量的地板 rustfmt 一压行就假红），见 `body_of` 头注。
///
/// # ⚠ 射程：只到这两处
///
/// `backend::observe::local_query::run_query`
/// 那处起进程的落点**刻意不在分母里**：它 exec 的是
/// `resolve_beside_this_exe` 找到的本机后端，**不是我们刚写出来的那个文件**
/// （`extract_embedded_to` 的产物它够不着）⇒ 那条路上不存在这个竞态的必要条件。
/// monitor 侧起进程只经 `spawn_managed`（`src/frontend/clippy.toml` 起进程那一组挡别处的 `Command::new`）——
/// 本条只管其中的 **2** 处。
#[test]
fn both_production_spawn_paths_go_through_the_shared_etxtbsy_verdict() {
    // 🔴 **两个变量刻意不同名**：同名的话后一个遮掉前一个，`spawn_detached` 就会去
    //    `local_backend.rs` 里找、当场「一处都找不到」。
    //    ⇒ 前者按它头注的自述叫 `host_side`（宿主知识那一半），后者叫 `backend_side`。
    let host_side = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let backend_side = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend.rs"
    ));
    for (who, prod, head, anchor) in [
        (
            "local_backend.rs::supervise_with_stdio",
            &backend_side,
            "pub fn supervise_with_stdio(",
            "SuperviseEvent::GaveUp",
        ),
        (
            "local_backend_host.rs::spawn_detached",
            &host_side,
            "fn spawn_detached(\n    bin: &std::path::Path,\n    extra_env: &[(String, String)],",
            // 锚点从 `process_group(0)` 换成 `Lifetime::Detached`：
            // 那句平台原语搬进唯一出口了，这一处留下的是**它声明的策略**。
            // 锚点的职责没变 —— 证明「切出来的体真的是这一段」。
            "Lifetime::Detached",
        ),
    ] {
        let body = body_of(prod, head);
        // 反向自检②：`body_of` 那条只保证「窗口有界且非空」，这条保证**切到的是那一段**。
        assert!(
            body.contains(anchor),
            "{who}：切出来的体里没有它自己那句 `{anchor}` —— 切到别处去了，下面几条在空转"
        );
        assert!(
            body.contains("spawn_with_etxtbsy_retry("),
            "{who}：这个落点没走那份共用分类 ——\n\
                 「这台机器上它就是起不来」与「这一刻恰好撞上了一个会自己过去的竞态」\n\
                 又装回同一个值、同一句话、同一个结局里了（本条防的正是这个）"
        );
        assert!(
            body.contains("TransientBusy"),
            "{who}：没有「会自己过去的竞态」那一支 —— 认得出，可结局仍然只有一个"
        );
        assert!(
            body.contains("Broken"),
            "{who}：没有「这台机器上它就是起不来」那一支"
        );
        assert!(
            !body.contains("cmd.spawn()"),
            "{who}：还留着一条裸 `cmd.spawn()` 的放弃路 —— 两条并存迟早分叉，\n\
                 而分叉的那一天，用户在哪条路上会看到哪句话没人说得准"
        );
    }
}

/// 套接字是**按家算出来的**：同一个家恒等，不同的家不同；住家里那个只给本人的 `run/` 目录（宿主与后端同一个函数）。
#[test]
fn the_socket_follows_the_home() {
    use std::path::Path;
    let a = Path::new("/home/someone/.cc-monitor");
    assert_eq!(
        relay_route_core::listen_socket_for(a),
        a.join("run").join("backend.sock")
    );
    assert_ne!(
        relay_route_core::listen_socket_for(a),
        relay_route_core::listen_socket_for(Path::new("/home/other/.cc-monitor"))
    );
}

/// hello 的三张脸。**「有人占着」不等于「占着它的是我们的」** ——
/// 这一格就是 `EADDRINUSE` 那条诚实边界的落点。
#[test]
fn a_stranger_on_our_port_is_refused_out_loud_not_silently_reused() {
    let ours = format!(
        "{{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"host_arch\":\"x86_64\",\
              \"claude_dir\":\"/h/.claude\",\"capabilities\":[],\"emits\":[],\"commands\":[]}}"
    );
    assert_eq!(hello_verdict(&ours, Some("b1"), &[]), HelloVerdict::Ours);
    // ① 版本不对
    match hello_verdict(&ours, Some("b2"), &[]) {
        HelloVerdict::Stranger(w) => assert!(w.contains("b1") && w.contains("b2"), "{w}"),
        v => panic!("旧版本的后端被当成了我们的：{v:?}"),
    }
    // ② 它看的 Claude 目录与这个 monitor 设置里填的不同 ⇒ **仍是我们的**：门牌只跟着家走，Claude 目录归那台后端解析，
    //    这里不比（比的话 ⇒ 改了设置之后本机后端一个都接不上）。「另一个家」那一形由 host_env 比（`hx2_` 那一族）。
    let other_claude = ours.replace("/h/.claude", "/other/.claude");
    assert_ne!(other_claude, ours, "替换没落在靶上");
    assert_eq!(
        hello_verdict(&other_claude, Some("b1"), &[]),
        HelloVerdict::Ours
    );
    // ③ 压根不是我们的协议
    assert!(matches!(
        hello_verdict("HTTP/1.1 200 OK", Some("b1"), &[]),
        HelloVerdict::Stranger(_)
    ));
    assert!(matches!(
        hello_verdict("", Some("b1"), &[]),
        HelloVerdict::Stranger(_)
    ));
    // ④ 手上没带后端字节（「我这一版」是 `None`）⇒ 不比 build、照那台报的接；不是我们的协议照样拒。
    assert_eq!(hello_verdict(&ours, None, &[]), HelloVerdict::Ours);
    assert!(matches!(
        hello_verdict("HTTP/1.1 200 OK", None, &[]),
        HelloVerdict::Stranger(_)
    ));
}

/// ★★ **起真后端的判据必须 fail-closed 地要一个私有 tmux 隔离。**
///
/// # 它不是纪律，是一次事故的直接产物
///
/// 被起的后端一上来就**无条件**往它连得到的 tmux server 装三条全局 hook、
/// 固定槽位 `[50]`、**没有关掉它的开关**，载荷里烤着那一个后端的 pid+starttime。
/// 不隔离就是去改用户真实 tmux server 的状态 —— 08-11 那次同族事故打没了用户 **9 个**真实会话。
/// ⚠⚠ **08-26 实现期间又发生了一次**：我在写这段代码时手工起了一个真后端做冒烟，
/// 没走 shim ⇒ 它把用户真实 tmux server 的 `[50]` 槽位**整个盖成了我那个进程的 pid**
/// （当时真有一个 monitor 的后端在跑）。当场按原值恢复了，但**那正是本条要防的东西**。
///
/// # 形状
///
/// `local_backend.rs` 有一条同职的（`every_real_backend_e2e_demands_a_private_tmux_dir`），
/// 而它**只扫它自己那个文件** —— 这里的真进程判据住这里，落在它的扫描面之外。
/// 「守卫范围 ≠ 性质范围」那一族，这里是它的又一形。⇒ 本文件自己补一条。
///
/// # ⚠⚠ 射程：**本条的人群画在属性上，够不着最危险的那一形**
///
/// 本条的人群逐字是「**带 `#[ignore]` 的**」（裸 `#[ignore]`），而属性可以不写、也可以写成
/// `#[ignore = "…"]` —— 那样的起真后端的测试，本条与那条姊妹判据谁也够不着。
/// ⇒ 正题在本文件下面那条
/// [`every_test_that_starts_the_real_backend_demands_a_private_tmux`]：
/// 它的人群按「**那个二进制哪来的**」派生，**两个文件一起扫**，不看任何属性。
/// **本条留着**，它守的是一格更窄但仍然真的性质（`#[ignore]` 那一族的形状回归），
/// 别把它读成「起真后端这件事有人守了」—— 守它的是下面那条。
#[test]
fn every_ignored_test_here_that_spawns_a_real_backend_demands_private_tmux() {
    const PRIVATE_TMUX: &str = "CCM_E2E_TMUX_SHIM_BIN";
    // 🔴 **语料跟着测试搬。**
    //
    // 本条数的是「**本模块里**带 `#[ignore]` 的真进程判据」。剖分把本模块的测试段
    // 整个搬到了 `tests/frontend/shell/local_backend_host_tests.rs`（就是本文件），
    // 而 `src/frontend/shell/src/local_backend_host.rs` 今天**一个 `#[test]` 都没有**
    // ⇒ 老语料切出 1 个块，下面整条在空转（那条 `chunks.len() >= 5` 的反空真按设计响了）。
    let src = include_str!("local_backend_host_tests.rs");
    // 按行切成「一个 `#[test]` 到下一个 `#[test]`」的块。
    // ⚠ **不在语料串上做裸 `split`**（`needle_anchor_registry` 把它判为「匹配单位比事实小」那一族）。
    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in src.lines() {
        if line.trim() == concat!("#[te", "st]") {
            if !cur.is_empty() {
                chunks.push(cur.join("\n"));
                cur.clear();
            }
            continue;
        }
        cur.push(line);
    }
    chunks.push(cur.join("\n"));
    // 抽取器自检：切不出块就下面全空转。
    assert!(
        chunks.len() >= 5,
        "只切出 {} 个测试块 —— 切法坏了",
        chunks.len()
    );
    let ignored: Vec<&String> = chunks
        .iter()
        .filter(|c| c.lines().any(|l| l.trim() == "#[ignore]"))
        .collect();
    // ⚠ **反向锚点**：本文件今天真的有 `#[ignore]` 的真进程判据。
    //   一条都没有的话，下面那个循环零圈 ⇒ 本条是安慰剂。
    assert!(
        !ignored.is_empty(),
        "本文件里一条 `#[ignore]` 都没有 —— 那本条此刻在空转。\n\
             真进程判据被删了？那「同机只起一个 · 脱离不留僵尸」就没有兑现证据了。"
    );
    for c in &ignored {
        let name = c
            .lines()
            .find_map(|l| l.trim().strip_prefix("fn "))
            .unwrap_or("<读不出名字>");
        assert!(
            c.contains(PRIVATE_TMUX),
            "`{name}` 是一条 `#[ignore]` 的真进程判据，却没有要 `{PRIVATE_TMUX}`。\n\
                 ★ 被起的后端一上来就往它连得到的 tmux server 装三条**全局** hook\n\
                 （固定槽位 `[50]`，**没有关掉它的开关**）⇒ 不隔离就是去改用户真实 tmux 的状态。\n\
                 ⚠ 这不是理论：08-11 打没过用户 9 个真实会话；08-26 实现期间又盖过一次 `[50]`。\n\
                 ⇒ 必须 **fail closed**：拿不到 shim 就 `expect` 炸掉，绝不降级裸跑。"
        );
    }
}

/// ★★ **取 shim 那个口自己，在默认门禁里真跑一遍**。
///
/// # 它买的是「**口自己**不是形状钉」这一格 —— 只有这一格
///
/// 本条钉的是**这个纯函数**。② 这条锁还包含「谁进了这个口」（文本钉）与
/// 「**进了口的人会不会把那声炸接住再降级**」（没有判据在守）
/// ⇒ 逐条登记在正题守卫头注的**诚实边界 8**；**别把本条读成「② 关上了」**。
///
/// 只靠「同一行上既有 `CCM_E2E_TMUX_SHIM_BIN` 又有 `.expect(`」这条**文本形状**判的话，
/// 把取值写成 `var(SHIM).unwrap_or_else(|_| var(<真 PATH>).expect(..))` ⇒ 三样东西都还在、
/// 判据说合规，而 fail-closed **没了**。⇒ 取 shim 只有 [`demand_tmux_shim`] 一个口，
/// 而**这条测试盯的正是那个口**：喂它一个「变量不在」的读数，它**必须炸**。
///
/// # 为什么是纯函数而不是去动环境变量
///
/// `cargo test` 一个进程里跑很多线程，改进程级环境变量会跟别人打架；
/// 而且 `tests/e2e/local-backend-supervise.sh` 那条路**是设了**这个变量的
/// ⇒ 「按环境当场试一把」的写法会让读数随跑法翻面。
/// [`require_tmux_shim`] 收的是 `Result`，所以这一格**与环境无关**，两条路上读数相同。
///
/// # 反空真
///
/// 只断「缺变量会炸」是**半格**：一个恒 `panic!` 的实现也能过。
/// ⇒ 另加一格「有值时原样交出来」，两格一起才钉住「**恰好是 fail-closed，不是恒炸**」。
#[test]
fn the_one_shim_gate_really_fails_closed() {
    // ① 缺变量 ⇒ 必须炸。**这是运行期读数，不是源码文本的形状。**
    let r = std::panic::catch_unwind(|| {
        require_tmux_shim(Err(std::env::VarError::NotPresent), "本条自检")
    });
    assert!(
        r.is_err(),
        "`require_tmux_shim` 拿不到 {TMUX_SHIM_VAR} 却**没有炸** —— 这就是降级裸跑。\n\
             ★ 它一降级，所有走这个口的 e2e 都会沿真 PATH 找到真 tmux，\n\
             backend 一上来就往用户那台 server 装三条全局 hook（槽位 [50]，没有开关）。\n\
             08-11 打没过用户 9 个真实会话；08-26 / 08-27 / 08-29 各盖过一次。\n\
             ⚠ 只靠一条**同行文本形状**钉 ② 的话，\n\
             `unwrap_or_else` 里再套一个 `.expect(` 就走过去了 —— 这一格是运行期读数。"
    );
    // ② 反空真：有值时必须**原样**交出来。少了这一格，「恒 panic」也能过上面那一格。
    let got = require_tmux_shim(Ok("/夹具/shim-bin".to_string()), "本条自检");
    assert_eq!(
        got, "/夹具/shim-bin",
        "`require_tmux_shim` 拿到值却没有原样交出来 —— 那么上面那一格是空真：\n\
             一个「永远炸」的实现照样能过，而它会把 e2e 全部打死在起跑线上。"
    );
}

/// ★★★ **人群画在「会起真后端的测试」上 —— 不是画在某个属性上**。
///
/// # 它为什么是单独一条，而不是把上面那条改宽一点
///
/// 上面那条（`every_ignored_test_here_that_spawns_a_real_backend_demands_private_tmux`）
/// 的人群是「**带 `#[ignore]` 的**」，`local_backend.rs` 那条
/// （`every_real_backend_e2e_demands_a_private_tmux_dir`）的人群是
/// 「**带 `#[ignore]` 且提到 `CCM_E2E_BACKEND` 的**」。两条**都把人群画在属性上**，
/// 而属性可以不写、也可以换写法：姊妹守卫的人群判据是 `l.trim() == "#[ignore]"`（**逐字相等**），
/// 认不出 `#[ignore = "…"]`。起真后端的测试一旦落在它们的人群外，
/// 那个后端一上来就**无条件**装三条**全局** tmux hook（槽位 `[50]`，没有开关），
/// 去改用户真实 tmux 的状态。⇒ 本条按**来历**派生，两种写法都够得着。
///
/// # 人群怎么派生的 —— 按「**那个二进制哪来的**」，不按「怎么标记的」
///
/// 一条测试会不会装全局 hook，取决于**它起的是不是那个真 backend**。
/// ⇒ 判准取**二进制的来历**，三条（默认拒绝，出现任何一条就进人群）：
///
/// | 来历 | 长什么样 |
/// |---|---|
/// | 内嵌那份 | 源码里出现 `embedded-backends` |
/// | e2e 传进来的那份 | 源码里出现 `CCM_E2E_BACKEND` |
/// | 共享 e2e 沙箱 | 源码里出现 `E2eSandbox::demand` |
///
/// **两个文件一起扫**：`local_backend_host_tests.rs` 与 `local_backend_tests.rs`。
///
/// # 要求：fail-closed 地要一个私有 tmux **并且真的用上它** —— 两种合法形态
///
/// ① **自己要 + 自己挂**（**三格缺一不可**）：
///    ㈠ 本体里走了取 shim 的**唯一入口** `demand_tmux_shim(..)`（**第二道锁**：取到）。
///       ㈠ 只判「**走没走那个口**」；「**口关不关得上**」由
///       [`the_one_shim_gate_really_fails_closed`] 在**默认门禁里真跑一遍**
///       （纯函数 + `catch_unwind`，与环境无关）—— 那一格是**机器守的，不是文本钉**。
///       它**不是**「走这个口的测试就 fail-closed」：两者之间隔着「**谁能接住那声 panic**」，
///       那一格没有判据在守（诚实边界 8）。
///       「这条测试**走不走**那个口」仍是一条**文本判据**，与人群判据（来历字面量）同一档
///       ⇒ 一条根本不进人群的测试（诚实边界 5「第四种来历」）照旧够不着。
///       ⚠ 还有一格**没钉**：那个口的调用**排在任何 spawn 之前**。顶着它的是**数据依赖**
///       （`envs` 要用 `shim`，取值挪不到 spawn 之后）—— 那是一条结构性理由，不是判据；
///       只登记，不给 ㈠ 再加一层文本钉。
///    ㈡ 本体里有一行**同时**含 `"PATH"` 与 `"{<绑定名>}:`（**第三道锁**：挂进 `PATH` 最前面）
///       —— 注意开头那个**双引号**：插值必须是那个格式串的**串首**，不只是「冒号左边」；
///    ㈢ `PATH` 作为 env 键在本体里**恰好被写一次**（**同一张 `envs` 里后写的赢**）。
/// ② 委托给 `E2eSandbox::demand()` —— 而**转发者自己被本条单独钉住**（见下面第 ③ 段），
///    ㈠㈡㈢ 三格都钉，不然「委托」就是一张空头支票。
///    ⚠ 委托方**自己本体里 `PATH` 要写 0 次**（㈢ 在这条腿上的取值）：`PATH` 全由
///    `demand()` 一处代办（`envs()` 逐字 `vec![self.shim_path.clone()]`），
///    委托方再写一条排在它后面就是**把它盖掉**。
///
/// ⚠ **㈡ 与 ㈢ 是两件事，刻意不合成一格**（一个值不装两件事）：
/// ㈡ 判**那一行长什么样**（写法上 shim 排第几），㈢ 判**本体里写了几次**（有没有被后一条盖掉）。
/// 两者的**代价也不是一种**：破 ㈡ 要「特意在串首前面塞一段」，破 ㈢ 只要「顺手再加一条环境变量」。
/// 合成一格之后，诊断就说不出差在哪一件。
/// 「取到了一个变量」与「用上了它」是两件事 —— 只判 ㈠ 的话，把 shim 挪到 `PATH` 末尾、
/// 或整格换成一个无关的变量，都照样合规。
///
/// # ⚠ 诚实边界（逐形登记，不假装覆盖）
///
/// 1. **人群是「起真后端」，不是「起任何进程」。** 用**自造脚本**喂 `supervise()`
///    的那几条（`a_child_that_floods_stdout_and_exits_is_still_detected_as_dead` 等）
///    **刻意不在人群里**：它们起的东西不装 tmux hook。判准是「后果」不是「动作」。
/// 2. **扫描面是这两个文件。** 别处新写一条起真后端的测试，本条看不见。
///    `CCM_E2E_BACKEND` 与 `E2eSandbox` 今天只在这两个文件里；`embedded-backends` 还出现在
///    别的源文件里（构建脚本 · 登记表数据 · 文档注释 / 报错文案），那几处没有起真后端的测试。
///    扫描面外**没有守**。
/// 3. **守卫自己要排除掉。** 判准是「本体里出现 `include_str!` 这个宏」——
///    读源码的是守卫，不是运行期测试。⇒ 一条**既起真后端又读源码**的测试会被漏掉。
///    今天被这条规则排除、且带来历字面量的块只有守卫自己（本条本体 · 姊妹守卫
///    `every_real_backend_e2e_demands_a_private_tmux_dir`），都不起进程。
///    ⚠ 那个判准串在下面是**拼出来的**（`concat!`），不是写死的字面量 ——
///    `cross_half_edge_registry::every_non_literal_include_is_registered_with_a_reason`
///    数的是「`include_*!` 后面跟着 `(`」的**出现次数**，注释与字符串里也算
///    ⇒ 直接写字面量会让本条把自己变成那张登记表上两处「解析不出路径的 include」。
/// 4. **它证不了 shim 真的挡住了。** 它只证「要了、缺了就炸、写在串首、而且只写了一次」。
///    ㈠㈡㈢ 三格判的都是**源码文本的形状** ——「某一行长什么样」与「某个东西在本体里出现几次」——
///    它们**不追这个值的去向**。⇒ 量过的几形逮不到的，逐形登记在第 6 条。
///    ㈡ 钉的是写法，写法与解析结果之间那一跳靠 `PATH` 的语义（最前面那个目录赢）。
/// 5. 🔴 **第四种来历逮不到**。
///    形状：一条**普通 `#[test]`**（无 `#[ignore]`、无 `cfg`）、**就在这两个文件里**、
///    真调 `local_backend::supervise_with_stdio(..)` 起后端，只把二进制来历换成
///    **第四种**取法（例如一个新环境变量）⇒ 三条来历字面量一条都不出现
///    ⇒ 它不进人群，本条与两条姊妹守卫**全不出声**。
///    ⚠ 它也**不会被别的网接住**：起进程那一组 clippy 名单只管**生产段**，测试段整段放开，
///    而这样一条测试调的又正是生产侧那个合法的落点 ⇒ clippy 照样绿。
///
///    **为什么不换成「按调用派生」的判准**（`调了 supervise* / spawn_detached
///    且没 fail-closed 地要 shim`）—— 在同一个扫描面上量过：
///
///    | 判准 | 人群 | 真阳 | 假阳 | 真阳率 |
///    |---|---|---|---|---|
///    | 现判准（来历字面量） | 6 | 6 | 0 | **6/6** |
///    | 候选（按调用派生） | 9 | 4 | 5 | **4/9** |
///    | 候选（再剔掉「首个 `#[test]` 之前那一块」之后的最好情形） | 7 | 4 | 3 | **4/7** |
///
///    候选那 5 条假阳的来历有两种，**两种都不是能调参数调掉的**：
///    ㈠ **2 条根本不是测试** —— 本切法的第 0 块是「首个 `#[test]` 之前的全部内容」，
///       生产代码里当然有 `supervise(` / `spawn_detached(`（那正是被调的落点）
///       ⇒ 候选判准会在一棵干净的树上当场假红；
///    ㈡ **3 条是诚实边界 1 那一族** —— 拿**自造脚本**喂 `supervise()` 的
///       （`a_child_that_floods_stdout_and_exits_is_still_detected_as_dead` ·
///       `stop_returns_promptly_even_if_the_child_closed_stdout_but_lives_on` ·
///       `e2e_a_binary_that_always_dies_is_given_up_on_within_the_cap`）。
///       它们起的东西不装 tmux hook，**要求它们要 shim 是一条没有意义的要求**。
///    而且候选判准**还净漏 2 条今天已经管住的**：两条 `e2e_*` 委托给 `E2eSandbox`，
///    自己不直接调 `supervise*` ⇒ 候选覆盖 4 条，现判准覆盖 6 条。
///    ⇒ **选：留现判准，把这一形逐形登记在此**。这一形没有机器守着，
///    靠的是代码评审 + 本段这条登记。加第四种取二进制的方式，
///    **把它加进下面 `PROVENANCE` 那张表**，那是本条唯一的入口。
/// 6. 🔴 **㈡㈢ 判的是文本形状，不追值的去向**（逐形登记，不加钉）。
///    分母 = 本条扫描面那两个文件：
///
///    | # | 逮不到什么 | 为什么 | 今天有几处 |
///    |---|---|---|---|
///    | 6a | 那张 `envs` 写得完全对，却**根本没被交给起进程那一跳**（换成另一个 vec / `clone()` 被丢掉） | ㈡㈢ 只读源码文本，**不做数据流** | **0**（五个落点都真把那张表传进 `supervise*`） |
///    | 6b | 绑定名**被重新绑**：`let shim = var(SHIM).expect(..); … let shim = <别的东西>;` | 绑定名从「要 shim 那一行」现取，**只取第一处**，后面再绑一次它看不见 | **0** |
///    | 6c | `PATH` 由本体**调的另一个函数**追加（helper / builder），或写在转发者那 **20 行窗口之外** | ㈢ 数的是**本体**（转发者是 `demand()` 起 20 行）里的处数 | **0**（今天五处全是本体里的字面写法） |
///    | 6d | ㈢ 的「写」口径是**正向数写位形状**（`"PATH"` 后面紧跟一个方法调用）⇒ 键名**拼出来**的（`concat!("PA","TH")` / `format!("PAT{}", "H")` 这类）数不到 | 按字面串数，不是按语义数 | **0** |
///    | 6e | 反方向的**假红**：把 `let old = var("PATH")…;` 提到上一行、写 `format!("{shim}:{old}")` ⇒ 那一行没有 `"PATH"` ⇒ ㈡ 判不合规 | ㈡ 要求 `"PATH"` 与插值**同一行** | **0**。⚠ 这一形是**看得见的**（红了就会去读诊断），与假绿不是一种代价 |
///    | 6f | 委托腿上，第二条 `PATH` 被写进 **`E2eSandbox::envs()` 或别的 helper**，而不是测试本体 | ㈢ 在委托腿上数的是**测试本体**（要 0 次）；helper 不是测试块，不进人群 | **0**（`envs()` 逐字 `vec![self.shim_path.clone()]`） |
///    | 6g | **shim 那条绑定本身是个复合串**：`let shim = format!("/usr/bin:{}", var(SHIM).expect(..));` ⇒ ㈡㈢ 两格全绿，而后端实得 `PATH = /usr/bin:<shim>:<真PATH>`，真 `/usr/bin/tmux` 先被解析到 | ㈡ 只看**绑定名**出现在串首，**不看那个名字里装的是什么** | **0**（五个落点的绑定行右手边全部以 `std::env::var("CCM_E2E_TMUX_SHIM_BIN")` 打头） |
///    | 6h | 写位形状不是「`"PATH"` 后紧跟方法调用」的写法。枚举过四种 —— ㈠ `(format!("PATH"), v)` · ㈡ `("PATH", v)` 这种 `&str` 元组 · ㈢ `cmd.env("PATH", v)` · ㈣ `map.insert("PATH", v)` | 正向数只认那一种形状，只少算（假绿）、不多算 | **0**。㈡ 过不了 `envs: Vec<(String, String)>` 的类型；㈢㈣ 根本不走 `envs` 这条路（属 6a）；只有 ㈠ 是「类型过得了、也真进 `envs`」的 |
///    | 6i | **先把键名绑到一个名字上**：`const KEY: &str = "PATH"; … (KEY.to_string(), <真 PATH>)` ⇒ 正向数看的是**字面量**，而 `"PATH"` 后面跟着的是 `;` ⇒ 一处都数不到 | 正向数认的是「字面量与转换**相邻**」；绑一次名，转换就落在名字上了 | **0**。只登记、不加钉 —— 这一族是构造性的 |
///
///    ⚠ 分母是「一个键落进 `envs` 的所有走法」，那个数给不出 ⇒ 这里只写「**枚举过的这几种**」。
///    在这份文件里写「只有 N 种 / 全部 / 任何」，同句给出分母；给不出就写「量过的这几种」。
///    ⇒ 一句话：**㈡㈢ 买的是「这几行写对了」，不是「这个值真的到了后端手里」。**
///    后一格见第 4 条。
///
/// 7. 🔴🔴 **③ 的形状钉到此为止 —— 这是停机条件**。
///
///    **此后新发现的 ③ 绕过形，一律走登记**（`6j`…往下排），**不再加钉。** 三条理由：
///
///    ① ③ 自己的边界（第 4 条）已经写明它是「**源码文本的形状**」钉、「**不追值的去向**」。
///       **用文本匹配钉运行期行为，永远能被换一种写法绕过** —— 这是构造性的，不是没做好。
///    ② **靠什么守，逐格给**（不给全称）：
///       · ① `#[ignore]`（默认 `cargo test` 够不到）—— **机器守的**：退掉一条的 `#[ignore]`
///         ⇒ 门禁当场红，`NotPresent` panic 落在本体第 2 条语句、早于任何 spawn。
///       · ② fail-closed 取变量 —— 取 shim 收敛到 `demand_tmux_shim` 一个口，而
///         [`the_one_shim_gate_really_fails_closed`] 在**默认门禁里真跑那个口**。分三句：
///         ㈠ **「口自己关得上」是机器守的**（运行期读数，「恒炸」与「降级」各自单断）；
///         ㈡ **「谁进了那个口」仍是文本钉**（人群那一档）；
///         ㈢ **「进了口的人会不会把那声炸接住再降级」没有判据在守**（诚实边界 8）。
///         ⇒ ② 买到的是**防一部分手滑**，**不是**「② 关上了」。
///       · ③ shim 挂进 `PATH` —— **形状钉**，本条停机，只登记。
///       ③ 只覆盖「**手工 `--ignored` ＋ 变量设上 ＋ shim 不在自己 `PATH` 上**」那一格 ——
///       这一格拆开写在两条被守测试的头注里。
///    ③ `6g` 那一形要有人**主动写** `let shim = format!("/usr/bin:{}", …)` —— 那不是手滑，
///       是有人在**拆这道锁**。**对「主动拆锁」加更多形状钉，收益递减而假阳递增**
///       （`var_os` 那一形就是一个假阳）。假阳会训练人绕过判据，比没有判据更坏。
///
///    ⚠⚠ **这条停机只管 ③。** ①（`#[ignore]`：默认 `cargo test` 够不到）与
///    ②（fail-closed：缺 `CCM_E2E_TMUX_SHIM_BIN` 当场炸）**若被找到绕过，那是真缺口，照修不误** ——
///    停机条件**一个字都盖不到它们**。② 那条「接住那声 panic」的缺口，修法不是再加一钉，
///    而是让后端只碰别人显式交给它的那个 tmux server（要动生产段，另立一件），见第 8 条。
/// 8. 🔴🔴 **② 开着的那条缝：调用方接住那声 panic**。
///
///    **形状**（逐字，**故意不放进代码块**：不是给人抄的示范）：
///    · `let shim = std::panic::catch_unwind(|| crate::local_backend_host::demand_tmux_shim(".."))`
///    · `    .unwrap_or_else(|_| std::env::var("PATH").unwrap_or_default());`
///    **它为什么全绿，逐格**：㈠ 含那个口 ⇒ 绿 · 姊妹守卫那一行真是 `let ` 绑定 ⇒ 绿 ·
///    ㈡ `format!("{shim}:{}", …)` 那一行没动 ⇒ 绿 · ㈢ 写位仍是 1 ⇒ 绿 ·
///    口自己一个字没动 ⇒ [`the_one_shim_gate_really_fails_closed`] 绿。
///    而变量缺席时 `shim` 就是**真 `PATH`** ⇒ 降级裸跑。
///
///    **⇒ 这道闸买到的是「**防一部分**手滑」，买不到「挡住存心绕」。**
///    别用「保证」「买断」「fail-closed 就还在」这类词描述它；也别把「防手滑」写成全称。
///
///    | | 说得准的话 |
///    |---|---|
///    | **买到** | 口里那句 `panic!` 被改成降级 ⇒ 门禁当场红，只点那条测试 |
///    | **买到** | 反空真那半格不是仪式（「恒炸」的实现过不了） |
///    | **买到** | 「自己现取」变量的写法被拒 |
///    | **买不到** | 调用方在「口」与「调用点」之间**接住那声 panic** 再降级 |
///
///    ⚠ **这条缝不是纯理论的**：`catch_unwind` **本来就是这两个文件里的现成写法** ——
///    `local_backend.rs` 的 `local_stdio_consumer_guarded` 一处、
///    [`the_one_shim_gate_really_fails_closed`] 自己一处 ⇒ **照抄的成本为零**。
///    另一侧也要写上：`catch_unwind` 得**有意**去接住那声 panic，这一形与「手滑」不是同样可能。
///
///    **为什么不再加一钉**（两条理由各自独立成立）：
///    ① **panic 是可以被接住的** —— 「唯一入口 ＋ 入口 fail-closed」再硬，
///       调用方一句 `catch_unwind` 就降级了；**入口做得再好，也守不住调用方**。
///    ② **生产合法地需要那条不隔离的路** —— 真实用户就是不带 shim 起 backend
///       ⇒ **在这两个前提下**（生产要留一条不带 shim 的入口 · 测试与生产同 crate、
///       因此调得到那条入口），**没有类型能让「带 shim」变成强制的**。
///       ⚠ 前提是承重的：把这几条 e2e 搬进一个**独立的集成测试 crate**、只暴露带见证的入口，
///       这句话就不成立 —— 那是另一件事的体量。
///    🔴 **真正的修法另立一件**：让被起的 backend **只碰别人显式交给它的那个 tmux server**
///      —— 那样测试侧写成什么样都伤不到这台机器。它**要动生产段** ⇒ 是排期决定，不放进这里。
/// 9. **多行块注释那一形**：把真调用整段包进 `/* */`、再写一句桩值 `let shim = String::new();`。
///    这一形落在「防手滑」以内（多行块注释正是编辑器「注释掉这几行」的默认产物）。
///    它今天**关着**：共用剥法 `guard_core::production_code` / `guard_core::strip_comment_lines`
///    用的是**带状态的词法**（串 / 行注释 / 块注释开合与深度），块注释的内容被抹成等长空格，
///    `asks_itself` 在它上面不为真，本条落回 `MISS_LOCK2` **当场红**（报文点名「两条腿都没走」）。
///    两把尺子的单位对齐：
///    · **本条**先剥整份文件、再切块（`guard_core::strip_comment_lines(整份)` → `guard_core::test_attr_chunks`）
///      ⇒ 交进剥法的单位就是**文件**；
///    · **看门判据** `guard_core::assert_block_comment_model_holds` 量**两个单位**
///      （整份文件 ＋ 按 `#[test]` 切出的每一块）⇒ 「整份配平、单块不配平」那一形它自己就会红并点名到块。
///    ⚠ **仍然不是全称**：对齐的是这两个单位；别的单位（函数体窗口 `body_of(..)` · 任意切片）
///    不在这一格里。新剥法带一条**静默兜底**（词法与文本对不上时一个字都不剥），
///    兜底触发的那份文件上这个洞会重开 —— 那件事由
///    [`tests::no_monitor_file_falls_back_to_leaving_block_comments_in`] 看着。
///    本条只登记，不加钉（它守着「生产段零改动」）；让后端只碰别人交给它的那个 tmux server 才是根治，见第 8 条。
/// 10. ⚠ **共享剥法的盲点方向**。
///    共享原语连 **`*` 与 `/*` 打头的整行**一起剥，而 Rust 的**解引用行**
///    （`*g = Some(..)` / `*DETACHED.lock()… = None;`）正是那个形状
///    ⇒ 一条把来历字面量或 `"PATH".to_string()` 写在 `*deref = …` 行上的测试，
///    本条会**看不见**（那会是假绿）。
///    今天落在本条人群那 6 个块里的这类行全是 `*XXX.lock()… = …;` 解引用赋值，
///    两种剥法逐格对照（人群进出 · 委托 · 要口 · `writes` · ㈡ 的三值）判决相同。
///    这类行的条数随文件改动变，重打就取「`trim_start()` 后以 `*` 或 `/*` 打头、且不以 `//` 打头」的行。
///    ⇒ **这里登记的是一个方向，不是一个今天存在的漏。**
#[test]
fn every_test_that_starts_the_real_backend_demands_a_private_tmux() {
    const SHIM: &str = "CCM_E2E_TMUX_SHIM_BIN";
    // ㈠ 判的是「**走没走那个唯一的口**」，不是「同一行上有没有 `.expect(`」
    // （同行形状挡不住 `unwrap_or_else(.. .expect(..))` 那种降级）。
    // ★ 口子本身 fail-closed 这件事**不在这里判** —— 它由
    //   `the_one_shim_gate_really_fails_closed` 在默认门禁里**真跑一遍**。
    //   ⇒ 这一条现在只管「谁进了那个口」，那条管「口是不是关得上」。
    const GATE: &str = "demand_tmux_shim(";
    const DELEGATE: &str = "E2eSandbox::demand";
    // 真后端的三种来历。**默认拒绝**：出现任何一条就进人群。
    const PROVENANCE: [&str; 3] = ["embedded-backends", "CCM_E2E_BACKEND", DELEGATE];
    // 两条合法的腿。**取常量比字面量**：下面的反空真③要按腿数人，
    // 而按字面量数就会在改一个字的时候静默数成 0（那正是「人群缩水」那一族）。
    const LEG_SELF_SERVED: &str = "自己要 + 自己挂";
    const LEG_DELEGATES: &str = "委托给 E2eSandbox::demand()";
    const LEG_NEITHER: &str = "两条腿都没走（连第二道锁 ② 都没有）";
    // 「差在哪一格」——㈡ 与 ㈢ **各占一条**，不许合成一句（一个值不装两件事）。
    const MISS_NONE: &str = "";
    const MISS_LOCK2: &str = "没走取 shim 的那个唯一入口 `demand_tmux_shim(..)`，\
             也没委托给 `E2eSandbox::demand()`（**第二道锁 ②**）。\n      \
             ⚠ 本格判的是「**走没走那个口**」，不是「同一行上有没有 `.expect(`」——\
             后者挡不住降级（`unwrap_or_else(|_| var(<真 PATH>).expect(..))` \
             同行上三样都在，而 fail-closed 没了）。**口本身关不关得上**由 \
             `the_one_shim_gate_really_fails_closed` 在默认门禁里真跑一遍";
    // ⚠⚠ **「读不出绑定名」与「读出来了但没挂串首」是两件事，各一条报文**：
    //   合成一个 `bool` 的话，把那条合法调用按 rustfmt 在 `=` 后断行 ⇒ 落的是前一件，
    //   印出来的却是「没把它写在 `PATH` 串首」，**而串首那一行一个字没动**。
    const MISS_NO_BINDING: &str = "含取 shim 那个口的那一行**读不出绑定名**\
             （**第三道锁 ③ · ㈡ 取名**）—— 本条要的形状是**一行到底**的 \
             `let <名字> = …demand_tmux_shim(..)`。\n      \
             ⚠ 判决仍然是 fail closed（读不懂就不说「合规」），**变的只是这句话说的是哪一件事**：\
             说的是「**我读不懂那一行**」，不是「你没把它挂在 `PATH` 串首」。\n      \
             ⚠ **一形已知会落到这里的假阳**：按 rustfmt 在 `=` 后断行（`let shim =` 换行再写调用）\
             —— 语义逐字不变而本条会红。要的是**单行写法**；\
             这一格没有放宽（放宽要另做一次「先量再选」，这里不做）。\n      \
             ★ **多行块注释那一形不落到这里**：共用剥法 `guard_core::production_code` 带**块注释的开合状态**\
             （`guard-core/src/lib.rs::try_strip_block_comments`），块注释内容被抹成等长空格\
             ⇒ `asks_itself` 在它上面为假 ⇒ 内层行怎么写都落到上面那条 `MISS_LOCK2`，\
             没有「以 `let ` 打头就静默通过」那一格。\n      \
             本条**先把整份文件过一遍剥法、再切块**（`guard_core::test_attr_chunks`）\
             ⇒ 剥法的输入单位是**文件**；看门判据 \
             `no_monitor_file_falls_back_to_leaving_block_comments_in` 量**两个单位**\
             （整份文件 ＋ 每一块）⇒ `/*` 与 `*/` 分落两块那一形**两个独立红源**各红一次。\n      \
             ⚠ **射程不是全称**：对齐的是这两个单位；函数体窗口 / 任意切片那些单位看不见。\n      \
             ⚠ 剥法词法与交进去的那段文本对不上时**一个字都不剥**（兜底），\
             那一段上这个洞会重开 —— 由 `no_monitor_file_falls_back_to_leaving_block_comments_in` 看着\
             （⚠ 它量的是**文件 ＋ `#[test]` 块**这两个单位，别的单位它看不见）";
    const MISS_FRONT: &str = "要了 shim、绑定名也读出来了，**却没有一行把它写在给后端的 \
             `PATH` 串首**（**第三道锁 ③ · ㈡ 写法**）—— 要的形状是 \
             `(\"PATH\", format!(\"{shim}:{}\", ..))`，插值**紧跟开引号**；\
             写成 `\"/usr/bin:{shim}:{}\"` 也不算：那样真 tmux 先被解析到。\n      \
             ⚠ 这一条**只**说这一件事 —— 「读不出绑定名」归 `MISS_NO_BINDING`";
    // ⚠⚠ 下面两条报文不**断言**「写了 `PATH`」：判据看的只是一个**文本形状**，
    //   断言写了就会对着一次纯读说假话。
    //   ⇒ 只说**数到了什么形状**、要的是几处，并把本体里提到 `PATH` 的行**原样贴出来**。
    const MISS_TWICE: &str = "shim 那一行写对了，但本体里 `PATH` 这个 env 键的\
             **写位形状不止一处**（**第三道锁 ③ · ㈢ 处数**）—— `supervise_with_stdio` 是 \
             `for (k, v) in &envs { cmd.env(k, v); }`，**后写的赢** ⇒ 后面那条会把 shim 整个盖掉。\n      \
             ⚠ 判的是**形状**不是语义：数的是「`\"PATH\"` 后面紧跟一个方法调用」\
             （`.to_string()` / `.into()` / `.to_owned()`；`String::from(\"PATH\")` 先归一成这个形状）\
             —— 那是 `envs: Vec<(String, String)>` 逼出来的**键位**写法。\
             **纯读**（`var(\"PATH\")` / `var_os(\"PATH\")` / `vars()` 里比键名）**不算**";
    const MISS_DELEGATE_OVERRIDE: &str = "它走的是**委托**腿（`PATH` 由 \
             `E2eSandbox::demand()` 一处代办），而本体里出现了**把 `PATH` 当 env 键写下的形状**\
             （**第三道锁 ③ · ㈢ 处数**）—— 起进程那一跳后写的赢，\
             排在 `sb.envs()` 之后的那条会把 shim 整个盖掉。委托方本体里这个形状应当**0 处**。\n      \
             ⚠ 判的是形状不是语义（同上：`\"PATH\"` 后面紧跟方法调用；纯读不算）";
    // 🔴 **两个语料都跟着测试搬。**
    //
    // 本条的人群是「**起真后端的测试**」，而测试段剖分之后整个住进了 `tests/`：
    // `local_backend_host.rs` → `tests/frontend/shell/local_backend_host_tests.rs`（本文件）
    // `local_backend.rs` → `tests/frontend/shell/local_backend_tests.rs`
    // 两份生产文件今天**一个 `#[test]` 都没有** ⇒ 老语料一共切出 2 个块，
    // 下面整条在空转（那条 `total_chunks >= 40` 的反空真按设计响了）。
    // ⚠ 第一格的标签也跟着改成 `_tests.rs` —— 下面那张「必须在人群里」的名单是
    // `文件.rs::符号` 形的**住址**，而 `structural_scan::every_symbol_address_in_the_sources_still_resolves`
    // 会逐条核它指的符号今天住哪儿。
    let files: [(&str, &str); 2] = [
        (
            "local_backend_host_tests.rs",
            include_str!("local_backend_host_tests.rs"),
        ),
        (
            "local_backend_tests.rs",
            include_str!("local_backend_tests.rs"),
        ),
    ];

    // ── 🔴🔴 剥法按**整份文件**跑，切块在**剥完之后**做 ──
    //
    // 反过来（先按 `#[test]` 切块、再对每一块剥）的话，剥法的输入单位是「块」，而看门判据
    // `no_monitor_file_falls_back_to_leaving_block_comments_in` 量的是「文件」——
    // 两把尺子的作用域对不上：把 `/*` 与 `*/` **分落两块**
    // （整份文件配平、单块不配平；对 rustc 就是一条普通的跨行块注释、照样编译），
    // 真调用与那行合规的 `PATH` 写位一起躺进注释、只留一个桩值
    // ⇒ 那一块掉进剥法的**静默兜底**（一个字都不剥）⇒ `asks_itself` 在**注释里的文本**上
    // 为真 ⇒ 本条判「合规」，而看门判据**还是绿的**（它看的是整份文件，整份是配平的）。
    //
    // ⇒ **次序**：整份文件先过 `guard_core::strip_comment_lines`，**再**切块。
    //   块注释的开合状态在整份文件上一定配平（能编译的 Rust 一定配平）
    //   ⇒ 剥完之后每一块都干净，交进判据的单位与看门判据量的单位**是同一个**。
    //
    // ⚠ **两条边界，都写清**：
    //   ① 落在块注释**里面**的那种 `#[test]` 行会被抹成等长空格 ⇒ 它不再是边界，
    //      前后两块合成一块。**那是对的**：注释里的 `#[test]` 不是一条测试。
    //      这一格的读数落在 `guard-core` 那条
    //      `cutting_first_and_stripping_first_do_not_give_the_same_answer`（⑤ 那一格）。
    //   ② 剥法**不改行数、不改字节位**（块注释等长抹空格 · 整行注释换空行）
    //      ⇒ 下面转发者那个 `take(20)` 的窗口仍然是原文的那 20 行。
    //
    // ⚠ 切法用共享原语 `guard_core::test_attr_chunks`（一个事实一个权威源）；
    //   姊妹守卫 `local_backend_tests.rs::every_real_backend_e2e_demands_a_private_tmux_dir`
    //   里还有一份自己的切法（它「先切块再剥」）。
    // ⚠⚠ **刻意写成 `Vec::new()` ＋ `push`，不写 `files.iter().map(..).collect()`**：
    //   `needle_anchor_registry` 那条递减棘轮的语料变量集
    //   **按名字走传递闭包**，而 `let <名> = files…` 这一行会让 `stripped` 进集合，
    //   再由 `let me = stripped…` 把 **`me` 这个短名**卷进去 ——
    //   于是本文件**别的测试里既有的三行**（`body.contains("LOCAL_RELAY")` ·
    //   `head.contains("target_os = \"linux\"")` · `!me.contains("DefaultHasher")`）
    //   被算成新增欠账，棘轮 `.contains(` · `.find(` · `.matches(` **三条齐红**。
    //   **不许调它的上限**（递减棘轮），也不该把三行别人的代码改了 ⇒ 断掉那条派生边：
    //   RHS 不以语料变量打头，`stripped` 就不进集合。语义与 `map().collect()` 逐字等同。
    let mut stripped: Vec<(&str, String)> = Vec::new();
    for (who, src) in files {
        stripped.push((who, guard_core::strip_comment_lines(src)));
    }
    // ⚠ 判人群要看**代码**，不能看文档注释 —— 本条的头注里就写着那三条来历字面量。
    //   （注释里写着 `#[ignore]` 与 `CCM_E2E_BACKEND` 的守卫，看注释就会把自己算进人群。）
    //
    // ⚠⚠ **剥注释用共享原语** `guard_core::strip_comment_lines`
    //   （`guard-core/src/lib.rs::strip_comment_lines`），理由两条，各自独立成立：
    //   ① **仓规**：`structural_scan.rs::every_comment_stripping_transformer_is_registered`
    //      「剥注释只许有一个权威实现」；那条判据的采集面是 `fn … -> String` / `-> Vec<`，
    //      **看不见闭包**，私有副本写成闭包它也不红 —— 那是它的盲点，不是许可。
    //   ② 只剥「`//` 打头的整行」的剥法剥不掉**一行式**块注释
    //      （`/* 历史写法留档：demand_tmux_shim("旧") */`），
    //      下面 ㈡ 的 `find(..GATE..)` 先撞上那一行、`strip_prefix("let ")` 失败
    //      ⇒ 两条守卫一起**假红**。
    //
    //   ⚠ 代价：`strip_comment_lines` 连 **`*` 与 `/*` 打头的整行**一起剥，而 Rust 的**解引用行**
    //   （`*g = Some(..)`）与多行字符串里以 `**` 打头的续行正是那个形状 ⇒ 会多剥掉这批行。
    //   落在本条人群里的是 `*XXX.lock()… = …;` 那几行，与 ㈡ ㈢ 判的东西不相干；
    //   这个盲点方向登记在头注「诚实边界 10」。
    //   ⚠ 它**换空行、不删行** ⇒ 行位不变（下面转发者那 20 行窗口因此仍是原文前 20 行）。

    // ── 第三道锁 ③ 的判据〔`D1` 回修 08-31，阻塞 4〕：**取到 shim ≠ 用上 shim** ──
    //
    // 审计两刀实测：把 `format!("{shim}:{}", PATH)` 换成 `format!("{}:{shim}", PATH)`
    // （shim 从最前挪到最后）、以及把那一格整个换成 `("CCM_AUDIT_UNUSED", shim)`
    // （③ 整个退掉），**全量门禁新红都是 0** —— 因为上面那条「同一行 SHIM + `.expect(`」
    // 只看**取没取到**，不看**挂没挂上**。⇒ 这里补一条。
    //
    // ㈡ 判什么：本体里有一行**同时**含 `"PATH"` 与 `"{<绑定名>}:`（**注意开头那个双引号**）——
    // 插值必须是那个格式串的**串首** = 那个目录挂在 `PATH` **最前面**，而 `PATH` 的语义是
    // **最前面那个目录赢**。挂在后面（`{}:{shim}`）解析先撞到真 tmux ⇒ 隔离没了。
    // 绑定名不写死，从「要 shim 那一行」上现取（`let <名字> = std::env::var(SHIM)…`），
    // 改个变量名不该变成一次假红。
    //
    // ⚠⚠ **那个开头的双引号是 09-01 `D2` 复审买回来的**〔阻塞 1，形 ①〕：
    //   本条第一版只判「`{绑定名}:` 出现在含 `"PATH"` 的某一行上」——
    //   而 `format!("/usr/bin:{shim}:{}", PATH)` 照样满足它（冒号左边有插值），
    //   **写法上 shim 却排第二**，真 `/usr/bin/tmux` 先被解析到。复审实测**新红 0**。
    //   ⇒ 「在冒号左边」买不到「在最前面」，**要钉到串首**。
    //   先量后选（铁律 18，分母 = 走「自己要 + 自己挂」腿的 4 条 + 转发者 1 处 = 5 个落点）：
    //   收紧之后**假阳 0/5**（五处写法全同、全部通过）⇒ 买。全表见件文件 `§0l-1`。
    // ⚠⚠ **返回三值，不返回 `bool`**：两种「不合规」是两件事 ——
    //   ① 含口的那一行**读不出绑定名**（不是 `let <名字> = …` 的形状）；
    //   ② 读出来了，但**没有一行把它挂在 `PATH` 串首**。
    //   挤进同一个 `false` 的话，把那条合法调用按 rustfmt 在 `=` 后断行（语义逐字不变）
    //   ⇒ 走的是 ①，印出来的却是 ② 的说法 ——**串首那一行一个字没动**。
    //   两种都 fail closed、都算违例；三值只决定**说的是哪一件**，不是加钉。
    enum ShimOnPath {
        /// 读出了绑定名，而且真有一行把它挂在 `PATH` 串首。
        Front,
        /// **读不出绑定名** —— 含口的那一行不是 `let <名字> = …` 的形状（附那一行原文）。
        NoBinding(String),
        /// 读出了绑定名，但没有一行同时含 `"PATH"` 与 `"{<绑定名>}:`（附那个绑定名）。
        NotFront(String),
    }
    let shim_first_on_path = |code: &str| -> ShimOnPath {
        let gate_line = code.lines().find(|l| l.contains(GATE)).map(|l| l.trim());
        let binding = gate_line
            .and_then(|l| l.strip_prefix("let "))
            .and_then(|r| r.split_once('='))
            .map(|(n, _)| n.trim().to_string());
        match binding {
            Some(n) => {
                // ⚠ 头上那个 `\"` 就是「串首」那一格：`"{shim}:` 而不是 `{shim}:`。
                let front = format!("\"{{{n}}}:");
                if code
                    .lines()
                    .any(|l| l.contains("\"PATH\"") && l.contains(&front))
                {
                    ShimOnPath::Front
                } else {
                    ShimOnPath::NotFront(n)
                }
            }
            // 取不出绑定名 ⇒ **判不合规**（fail closed）：读不懂就别说「合规」。
            // 变的只是**说法**：说「我读不懂那一行」，不说「你没挂在串首」。
            None => ShimOnPath::NoBinding(
                gate_line
                    .unwrap_or("<本体里根本没有那个口 —— 那该由 ㈠ 先拦下>")
                    .to_string(),
            ),
        }
    };

    // ── ㈢ `PATH` 作为 env 键**恰好写一次** ──
    //
    // 同一张 `envs` 里**再追加一条** `("PATH".to_string(), std::env::var("PATH").unwrap_or_default())`：
    // `local_backend::supervise_with_stdio` 是 `for (k, v) in &envs { cmd.env(k, v); }`
    // （`local_backend.rs::supervise_with_stdio`）⇒ **后写的赢** ⇒ backend 拿到的 `PATH` 里
    // **一点 shim 都没有**，而 ㈡ 照样说合规（它看见第一条合规的行就够了）。
    // ⇒ 光判「有没有一行写对」不够，还要判「**有没有第二行把它盖掉**」。
    //
    // ⚠ 数的是「**写**」不是「**读**」。不用减法（`count("PATH") − count(var("PATH"))`）：
    // 那数的是「`PATH` 被**提到**几次，减掉认得的那一种读法」—— 任何别的读法
    // （`var_os("PATH")` …）都被算成一次写 ⇒ 假红，诊断还说「本体里也写了 `PATH`」。
    // 「读」这一族的分母给不出，再减一种读法下一种照样漏。
    //
    // 〔**口径：正向数写位**〕正面数「它落在写位上」这个形状：
    //   `"PATH"` 后面**紧跟一个方法调用**（中间只许空白），并且先把
    //   `String::from("PATH")` 归一成 `"PATH".to_string()`。
    // ★ **这个形状为什么算「写位」**：`envs` 的类型是 `Vec<(String, String)>`
    //   （`local_backend::supervise_with_stdio` 的签名）⇒ 一个 `&'static str` 字面量
    //   要当键放进去，**必须走一次 `&str → String` 的转换**；枚举到的转换写法：
    //   `.to_string()` / `.into()` / `.to_owned()`（都是「后面紧跟点」）
    //   ＋ `String::from(..)`（上面那一步归一掉）。
    //
    // ⚠ **这不是全称**：转换要发生一次，但不必发生在那个字面量身上 ——
    //     const KEY: &str = "PATH";              // ← `"PATH"` 后面是 `;`，正向数看不见
    //     (KEY.to_string(), <真 PATH>),          // ← 后写的赢，shim 被整个盖掉
    //   类型封死的是**类型**，不是「字面量与转换的相邻关系」。「写位」只枚举到这几种，
    //   一个键最终落进 `envs` 的所有走法给不出分母。这一形登记为 `6i`。
    //   量过的写位形里正向数数不到的：`("PATH", v)` 这种 `&str` 元组**类型过不了**；
    //   `cmd.env("PATH", v)` 与 `map.insert("PATH", v)` **不走 `envs` 这条路**（属 6a 那一族）；
    //   `(format!("PATH"), v)` 类型过得了、也真进 `envs`，登记进 `6h`。
    //   ⇒ ㈢ 量到的假绿面：`6d`（键名拼出来）＋ `6h` 的那一格 ＋ `6i`（先把字面量绑到名字上）。
    //   量过的 8 种纯读写法上，正向数把纯读误判成写 0/8，减法 6/8。
    //
    // 🔴 **出错的方向**：减法 `path_writes = W + R₂ ≥ W` ⇒ **只会多算**（错在**假红**这一侧）；
    //   正向数**只会少算**（错在**假绿**这一侧，悄悄过去）。仍然取正向数，理由两条：
    //     ① **两侧的代价不对称**：假红会**训练人绕过判据**（比没有判据更坏），
    //        还会让报文对着一次纯读说假话；假绿只是「这一格没守到」，**不会把别的格一起毁掉**。
    //     ② **噪声比**：读法探针上减法 6/8 假阳、正向数 0/8。
    //   ⚠ 这两条都不含全称：写「假绿面只剩 N 格」之前，先给出
    //     「一个键落进 `envs` 的所有走法」这个分母 —— 给不出就别写。
    //
    // ⚠ **两条腿都要数，只是要的数不一样**（治的是所有同职的地方，不是一处）：
    //   · 走「自己要 + 自己挂」的 ⇒ **恰好 1**（那一条就是它自己写的 shim 那条）；
    //   · 走「委托」的 ⇒ **恰好 0** —— `PATH` 全由 `E2eSandbox::demand()` 一处代办
    //     （`envs()` 逐字 `vec![self.shim_path.clone()]`），委托方**自己再写一条就是覆盖**。
    let path_writes = |code: &str| -> usize {
        let norm = code.replace("String::from(\"PATH\")", "\"PATH\".to_string()");
        norm.match_indices("\"PATH\"")
            .filter(|&(i, m)| norm[i + m.len()..].chars().find(|c| !c.is_whitespace()) == Some('.'))
            .count()
    };
    // 报文只贴**证据**，不替读者下结论（一条判据的报文说假话，比它红错更坏）。
    // ⇒ 把本体里**提到** `PATH` 的行原样贴出来，让人自己看哪一行是写、哪一行是读。
    let path_mentions = |code: &str| -> String {
        let hit: Vec<String> = code
            .lines()
            .filter(|l| l.contains("\"PATH\""))
            .map(|l| format!("        {}", l.trim()))
            .collect();
        if hit.is_empty() {
            "        （本体里一行都没提到 \"PATH\"）".to_string()
        } else {
            hit.join("\n")
        }
    };

    // (名字, 合规吗, **走的是哪条腿**, **差在哪一格**)
    // ⚠ 「哪条腿」记的是**它归谁管**，不是「它过没过」——
    //   下面的反空真③要按腿数「有几条被 ㈡ 查着」，把没过的那几条从腿里踢出去
    //   会让「全员违例」自动变成「这条腿没人走」。
    // ⚠⚠ **只管一个方向**：①「**全员违例**」不会被读成「这条腿没人走」
    //   （它们仍然归 `LEG_SELF_SERVED`，地板照旧数到 4，正题那张违例列表照常印出来）。
    //   ②「**有人改走别的腿 / 掉出人群**」这个方向**照旧会让地板抢先红**，
    //   那一趟正题的违例列表印不出来 ⇒ 代价是「**要跑两趟**」，不是「诊断被吃掉」：
    //   地板自己的报文里带着**腿名与全员名单**，人群缩水那一格另有反空真②逐条点名。
    //   地板不挪到正题之后：那是**行为改动**。
    // ⚠⚠ 第 4 格（差在哪）：㈡ 与 ㈢ 是**两件事**
    //   （写法 vs 处数），代价也不是一种 ⇒ **诊断必须分得开**，不许合成一格。
    //   它是「合规吗」那个 `bool` 的**说明**，不是它的替身 —— 两个值各装一件事。
    // ⚠ 第 4 格是 `String`：报文要带**实得的证据**
    //   （数到几处 · 本体里提到 `PATH` 的那几行），不然读的人只能猜判据看见了什么。
    //   **仍然是派生时定死**，不是打印时按腿回猜 —— 那一条纪律没变。
    let mut population: Vec<(String, bool, &'static str, String)> = Vec::new();
    let mut total_chunks = 0usize;
    for (who, src) in &stripped {
        // 🔴 `src` **已经在整份文件上剥过了**（见上面那段）⇒ 这里**不再逐块剥**：
        //   剥法的输入单位是「文件」，不是「块」。
        let chunks = guard_core::test_attr_chunks(src);
        total_chunks += chunks.len();
        for c in &chunks {
            let code: &str = c;
            // 守卫排除：读源码的是守卫（诚实边界 3）。**串拼出来，别写死**（同上）。
            if code.contains(concat!("include_", "str!(")) {
                continue;
            }
            if !PROVENANCE.iter().any(|p| code.contains(p)) {
                continue;
            }
            let name = code
                .lines()
                .find_map(|l| l.trim().strip_prefix("fn "))
                .and_then(|r| r.split_once('('))
                .map(|(n, _)| n.to_string())
                .unwrap_or_else(|| "<读不出名字>".to_string());
            // 两种合法形态：**自己要 + 自己挂**，或委托给 `E2eSandbox::demand()`。
            let asks_itself = code.contains(GATE);
            let writes = path_writes(code);
            let delegates = code.contains(DELEGATE);
            let (ok, leg, missing) = if delegates {
                // 委托 ⇒ `PATH` 归转发者一处代办，本体里再写一条就是把它盖掉。
                (
                    writes == 0,
                    LEG_DELEGATES,
                    format!(
                        "{MISS_DELEGATE_OVERRIDE}\n      实得：写位形状 {writes} 处（要的是 0 处）。\
                             本体里提到 `PATH` 的行，原样贴在这里，自己核对：\n{}",
                        path_mentions(code)
                    ),
                )
            } else if !asks_itself {
                (false, LEG_NEITHER, MISS_LOCK2.to_string())
            } else {
                // 走「自己要」这条腿 ⇒ 归 ㈡㈢ 管。
                // ⚠ ㈡ 今天分**两件事各说各的**〔`D5` 阻塞 2〕：读不出名字 / 读出来了但没挂串首。
                match shim_first_on_path(code) {
                    ShimOnPath::NoBinding(l) => (
                        false,
                        LEG_SELF_SERVED,
                        format!(
                            "{MISS_NO_BINDING}\n      \
                                 实得那一行（剥掉「`//` / `*` / `/*` 打头的整行」之后）：{l}"
                        ),
                    ),
                    ShimOnPath::NotFront(n) => (
                        false,
                        LEG_SELF_SERVED,
                        format!(
                            "{MISS_FRONT}\n      实得绑定名：`{n}`。\
                                 本体里提到 `PATH` 的行，原样贴在这里，自己核对：\n{}",
                            path_mentions(code)
                        ),
                    ),
                    // ㈢：写对的那一行会不会被**后面又一条写位**盖掉。
                    ShimOnPath::Front if writes != 1 => (
                        false,
                        LEG_SELF_SERVED,
                        format!(
                            "{MISS_TWICE}\n      实得：写位形状 {writes} 处（要的是 1 处）。\
                                 本体里提到 `PATH` 的行，原样贴在这里，自己核对：\n{}",
                            path_mentions(code)
                        ),
                    ),
                    ShimOnPath::Front => (true, LEG_SELF_SERVED, MISS_NONE.to_string()),
                }
            };
            population.push((format!("{who}::{name}"), ok, leg, missing));
        }
    }

    // ── 反空真①：抽取器真的切出了东西 ────────────────────────────────
    assert!(
        total_chunks >= 40,
        "两个文件一共只切出 {total_chunks} 个测试块 —— 切法坏了，下面整条在空转"
    );
    // ── 反空真②：人群非空，而且**该在里面的那几条真在里面** ───────────
    //    〔判据规范 7：先证「够得到」（地板），再问有没有违例；否则「零违例」是空真。〕
    //    ⚠ 这是**地板不是等号**：新写一条起真后端的测试会自动进人群、自动被要求合规。
    //      地板只挡「已知的那几条**静默掉出人群**」（改名 / 换来历 / 被删）。
    let names: Vec<&str> = population.iter().map(|(n, _, _, _)| n.as_str()).collect();
    // 🔴 住址跟着符号搬：这六条测试今天
    //    逐条住在 `_tests.rs` 里（符号一个都没少、也没改名，只是换了文件）。
    for must in [
        "local_backend_host_tests.rs::the_local_backend_host_can_be_stopped_and_started_again",
        "local_backend_host_tests.rs::e2e_a_second_host_adopts_the_running_backend_instead_of_starting_a_second_one",
        "local_backend_host_tests.rs::e2e_a_detached_backend_that_dies_leaves_no_zombie",
        // `local_backend_tests.rs` 里起真后端验本机 tmux 帧进 monitor 账本的那条随那本账删了。
        "local_backend_tests.rs::the_local_backend_host_really_registers_an_inbound_client",
        "local_backend_tests.rs::e2e_the_supervisor_restarts_a_real_backend_after_it_is_killed",
    ] {
        assert!(
            names.contains(&must),
            "`{must}` 掉出了人群 —— 它起真后端的来历不见了（改名？换写法？）。\n\
                 ★ 人群缩水与「本来就没有违例」在输出上一模一样，只有这条地板认得出来。\n\
                 现打人群：{names:?}"
        );
    }

    let self_served = population
        .iter()
        .filter(|(_, _, leg, _)| *leg == LEG_SELF_SERVED)
        .count();
    assert!(
        self_served >= 3,
        "走「{LEG_SELF_SERVED}」那条腿的只剩 {self_served} 条 —— \
             第三道锁 ③（shim 真的挂进后端的 `PATH` 最前面）的判据**只落在这条腿上**，\n\
             人少到这个份上它就快成空转了：那时「零违例」与「没人被查」在输出上一模一样。\n\
             现打人群（名字 · 走哪条腿）：{:?}",
        population
            .iter()
            .map(|(n, _, leg, _)| format!("{n} [{leg}]"))
            .collect::<Vec<_>>()
    );

    // ── ③ 转发者自己必须是 fail-closed 的（不然「委托」是空头支票）─────
    // ⚠ 与人群那一侧同一把尺子：**先剥掉注释再判**〔09-01 加的；同日收口成共享原语〕。
    //   ㈢ 是**按处数**判的 ⇒ 转发者这一段里随便一句注释提到 `"PATH"` 就会变成一次假红。
    //   剥注释只会让 ㈠㈡ 更严（少几行可看），不会放水。
    //   ⚠ 共享原语把被剥的行**换成空行**（不删行）、块注释**等长抹成空格**（不删字节）
    //   ⇒ 这仍然是 `demand()` 起**原文** 20 行，行位不变；下面报文贴出来的也是这一份。
    //
    // 🔴🔴 **窗口在剥完之后切**：先切一个 20 行的窗口再剥的话，交进剥法的是**窗口**，
    //   而看门判据量的是**文件**。窗口里出现一个不收口的 `/*`
    //   （`*/` 落在第 21 行之后）⇒ 这个窗口掉进兜底、一个字不剥，而文件级照旧配平。
    //   ⇒ **取剥过的整份文件**（上面那份 `stripped`），窗口在**剥完之后**切。
    //   ⚠ 锚点也在剥过的文本上找：那是**更严**，不是放水 ——
    //   锚点要是落进了块注释里，`find` 会**找不到**并当场 panic（fail closed）；
    //   在**原文**里找的话，注释里的一句 `fn demand() -> Self {` 就能把窗口带偏。
    let me: &str = stripped
        .iter()
        .find(|(who, _)| *who == "local_backend_host_tests.rs")
        .map(|(_, s)| s.as_str())
        .expect("`local_backend_host_tests.rs` 不在语料里 —— 上面那张 `files` 表被改坏了");
    let at = me
        .find(concat!("fn dem", "and() -> Self {"))
        .expect("`E2eSandbox::demand()` 不在了 —— 委托那条腿没了，来改本条");
    let demand: String = me[at..].lines().take(20).collect::<Vec<_>>().join("\n");
    // ⚠⚠ **不拿「整段里有 `SHIM` 也有 `.expect(`」判**：`demand()` 体内**另外两行**
    //   （`CCM_E2E_BACKEND` / `CCM_E2E_WORK`）带着 `.expect(`，`SHIM` 那个**变量名**也在，
    //   把取 shim 那一行换成 `.unwrap_or_default()` 一条都不红。三样东西都在，语义却不是那回事
    //   （与 `local_backend.rs` 那条退出臂判据头注里写的
    //   `let kill = kill_on_exit(..); if h.current_pid().is_some() { h.stop(); }` 同一形）。
    assert!(
        demand.contains(GATE),
        "`E2eSandbox::demand()` 不再走取 shim 的那个唯一入口（`{GATE}`）——\n\
             那么所有委托给它的测试都在裸跑，而本条会对着一张空头支票说「合规」。\n\
             ⚠⚠ **判的不是「同一行上既有 `{SHIM}` 又有 `.expect(`」**：\n\
             把取值换成 `var({SHIM}).unwrap_or_else(|_| var(<真 PATH>).expect(..))`\n\
             ⇒ 同一行上三样东西都还在，而 fail-closed 没了。\n\
             ⇒ 这里只判「走没走那个口」，「口关不关得上」由\n\
             `the_one_shim_gate_really_fails_closed` 在默认门禁里**真跑一遍**。\n\
             实得：\n{demand}"
    );
    // ⚠ 转发者的**第三道锁**也要钉：
    //   委托那条腿上，「挂进 `PATH` 最前面」这件事全由 `demand()` 一处代办
    //   ⇒ 它退掉，两条 `e2e_*` 的隔离一起没，而人群那一侧一声不吭。
    // ⚠ 三值各说各的：两种都不合规，报文说清是哪一件。
    let (fwd_ok, fwd_why) = match shim_first_on_path(&demand) {
        ShimOnPath::Front => (true, String::new()),
        ShimOnPath::NoBinding(l) => (
            false,
            format!(
                "含取 shim 那个口的那一行**读不出绑定名** —— 要的形状是一行到底的\n\
                     `let <名字> = …{GATE}..)`。⚠ **这句话说的是「我读不懂那一行」**，\n\
                     不是「没挂在串首」\n\
                     （rustfmt 在 `=` 后断行会落到这一条，而串首那一行一个字没动）。\n\
                     实得那一行：{l}"
            ),
        ),
        ShimOnPath::NotFront(n) => (
            false,
            format!(
                "绑定名读出来了（`{n}`），**却没有一行把它挂到给后端的 `PATH` 最前面** ——\n\
                     取到了 shim 却不用它 = 第二道锁做了、第三道锁没做，backend 照样沿真 `PATH`\n\
                     找到真 tmux（手工 `cargo test -- --ignored` 那一格）。\n\
                     ⚠ 判的是「同一行上既有 `\"PATH\"` 又有 `\"{{<绑定名>}}:`」——\n\
                     插值要**紧跟开引号**（= 那个格式串的串首）。写成 `\"/usr/bin:{{shim}}:{{}}\"`\n\
                     也不算：那样真 `/usr/bin/tmux` 先被解析到。"
            ),
        ),
    };
    assert!(
        fwd_ok,
        "`E2eSandbox::demand()` 的**第三道锁 ③ · ㈡** 没过：\n{fwd_why}\n\
             实得（`demand()` 起**原文** 20 行，剥掉「`//` / `*` / `/*` 打头的整行」之后；\n\
             被剥的行**换成空行**，所以行位与原文一一对应）：\n{demand}"
    );
    // ⚠ 转发者的 ㈢ 也要钉：
    //   `demand()` 里那条 `shim_path` 写对了，可要是同一段里再写一次 `PATH`，
    //   委托那条腿上的两条 `e2e_*` 一起没隔离，而 ㈡ 照样说合规。
    //   ⚠ 射程：它看的是 `demand()` **起 20 行**这个窗口（`take(20)`）——
    //     写在窗口之外的第二条 `PATH` 它看不见（头注「诚实边界 6c」已登记）。
    assert!(
        path_writes(&demand) == 1,
        "`E2eSandbox::demand()` 里 `PATH` 这个 env 键的**写位形状**数到 {} 处（要的是恰好 1 处）——\n\
             起进程那一跳是 `for (k, v) in &envs {{ cmd.env(k, v); }}`，**后写的赢**\n\
             ⇒ 后面那条会把带 shim 的那条整个盖掉，backend 拿到的 `PATH` 里一点 shim 都没有。\n\
             ⚠ 判的是**形状**不是语义：数的是「`\"PATH\"` 后面紧跟一个方法调用」\n\
             （`.to_string()` / `.into()` / `.to_owned()`；`String::from(\"PATH\")` 先归一）——\n\
             那是 `envs: Vec<(String, String)>` 逼出来的**键位**写法；**纯读不算**\n\
             （`var(\"PATH\")` / `var_os(\"PATH\")` / `vars()` 里比键名）。\n\
             ⇒ 下面是 `demand()` 起**原文** 20 行、剥掉「`//` / `*` / `/*` 打头的整行」之后的样子\
             （被剥的行换成空行），**自己核对哪一行是写、哪一行是读**：\n{demand}",
        path_writes(&demand)
    );

    // ── 正题 ─────────────────────────────────────────────────────────
    let bad: Vec<String> = population
        .iter()
        .filter(|(_, ok, _, _)| !ok)
        // ⚠ 「差在哪」是**派生时就定好的那一格**，不是在这里按腿回猜：
        //   ㈡（写法）与 ㈢（处数）都归 `LEG_SELF_SERVED`，按腿回猜就把两件事说成一件。
        .map(|(n, _, leg, missing)| format!("{n}\n      走的腿：{leg}\n      差在：{missing}"))
        .collect();
    assert!(
        bad.is_empty(),
        "这几条会起**真** backend，却没有 fail-closed 地要 `{SHIM}` **并真的用上它**：\n  {}\n\
             ★ 被起的后端一上来就往它连得到的 tmux server 装三条**全局** hook\n\
             （固定槽位 `[50]`，**没有关掉它的开关**）⇒ 不隔离就是去改用户真实 tmux 的状态。\n\
             ⚠ 这不是理论：08-11 打没用户 9 个真实会话；08-26 / 08-27 / 08-29 各盖过一次 `[50]`。\n\
             ⚠⚠ **只写 `env_remove(\"TMUX\")` 不算** —— `TMUX` 一空，tmux 就回落到默认 socket\n\
             `/tmp/tmux-$UID/default`，那**正是**用户那台 server。隔离要靠**显式选择器**。\n\
             ⚠⚠ **只取到 shim 也不算**（第三道锁）—— 取到一个值却不把它挂进\n\
             backend 的 `PATH` 最前面，那是一次仪式：backend 照样沿真 `PATH` 找到真 tmux。\n\
             ⚠⚠ **写对一行也不够**（第三道锁 · ㈢）—— 同一张 `envs` 里再写一条 `PATH`，\n\
             `for (k, v) in &envs {{ cmd.env(k, v); }}` **后写的赢**，shim 那条就被盖掉了。\n\
             ⇒ 两条合法出路：本体里 `let <名字> = {GATE}..)`（**一行到底**）\n\
             **并**写一行 `(\"PATH\", format!(\"{{<名字>}}:{{}}\", ..))`（插值**紧跟开引号**、\n\
             且本体里 `PATH` 这个键**只写这一次**）；或走 `{DELEGATE}()`。\n\
             ⚠ 本体里自己 `env::var(\"{SHIM}\").expect(..)` 现取不算：㈠ 判的是\n\
             「**走没走那个唯一入口**」，自己现取当场红。",
        bad.join("\n  ")
    );
}

/// 起本机后端被拒（口上有东西、接不上：升级后上一次脱离留下的旧后端还占着口最常见）是一件动得了手的事，
/// 下一步那一句要进本机那一台的状态成品（设置窗本机那一行照画），不许只进日志。
/// 判据：那一句写在真被拒的那一臂（不从 `reason` 串里认字）、取的是那条文案、带着占口进程的 pid；每次起之前先清掉上一次的。
/// 成品带不带它由 `a_refused_local_start_carries_the_next_step` 钉。
#[test]
fn the_start_refusal_reaches_the_local_machine_row() {
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    // 标记带上 `=> {`：裸的 `Adopt::Refused(why) =>` 在生产段里命中 2 处。
    let refused = with_copy(braced_block(&me, "Adopt::Refused(why) => {", 300, 3000));
    for (needle, why) in [
        (
            "note_local_refused(Some(",
            "被拒那一臂不再把下一步交给本机那一台的状态成品 —— 用户什么都看不到",
        ),
        (
            "\"rsLocalBackendHost.start.refusedNext\"",
            "只报「起不来」没用：要说现在该干什么，那一句住这条文案",
        ),
        (
            "(\"pid\", &listener_pid(&sock)",
            "「结束该进程」要说得出是哪个进程 —— pid 由内核报（套接字的对端）",
        ),
    ] {
        assert!(
            refused.contains(needle),
            "被拒那一臂里没有 `{needle}`。\n说法：{why}"
        );
    }
    let start = body_of(&me, "pub fn start_local_backend() -> StartOutcome {");
    assert!(
        start.contains("note_local_refused(None)"),
        "起之前不清上一次被拒的那句 ⇒ 这一次起成了，本机那一行还挂着旧的下一步"
    );
}

/// 本机后端那一趟正在起（还没接上）⇒ 正在起、说的是「启动 cc-monitor」那一步；起完没接上 ⇒ 没起来；接上了 ⇒ 连着。
#[test]
fn a_local_backend_being_started_reads_as_starting() {
    use crate::ui_contract::LocalBackend as L;
    let _g = crate::inbound_client::local_origin_test_lock();
    super::note_local_foreign(None);
    super::note_local_starting(true);
    let starting = super::local_backend(false, Some("p8c-mine"));
    let attached = super::local_backend(true, Some("p8c-mine"));
    super::note_local_starting(false);
    let stage = copy_text("machineState.stage.attach", &[]);
    assert!(
        matches!(&starting, L::Starting { said, .. } if said.contains(&stage)),
        "{starting:?}"
    );
    assert_eq!(attached, L::Up);
    assert!(matches!(
        super::local_backend(false, Some("p8c-mine")),
        L::Down { .. }
    ));
}

/// 口上占着别的构建 ⇒ 按两边构建的序说旧 / 新；解不出序、或手上没带后端字节（开发构建）⇒ 照没起来说。
#[test]
fn a_foreign_build_on_the_port_reads_by_its_order() {
    use crate::ui_contract::LocalBackend as L;
    let _g = crate::inbound_client::local_origin_test_lock();
    let at = |foreign: Option<&str>, mine: Option<&str>| {
        super::note_local_foreign(foreign.and_then(|b| foreign_in(b, true)));
        let got = super::local_backend(false, mine);
        super::note_local_foreign(None);
        got
    };
    assert!(matches!(
        at(Some("p7a-old"), Some("p8c-mine")),
        L::Older { .. }
    ));
    assert!(matches!(
        at(Some("p9a-new"), Some("p8c-mine")),
        L::Newer { .. }
    ));
    assert!(matches!(at(Some("odd"), Some("p8c-mine")), L::Down { .. }));
    assert!(matches!(at(Some("p7a-old"), None), L::Down { .. }));
}

/// 起本机后端的失败分两档：动得了手的（删文件 / 停进程 / 改权限位）把下一步交给本机那一台的状态成品；
/// 诚实降级（这一份产物里没带本机后端 · 释放内嵌也失败 · 家目录问不到、没有落点可放）只进日志。`Failed` 的构造点数钉死：新增一处就回来分档。
/// 「家目录问不到」不交下一步（不调 `note_local_refused`）：它不是一台机器上动得了手的那一类（没有文件可删、没有进程可停、
/// 没有权限位可改），问不到家时数据目录、日志落点一并没有；那一句（「无主目录」）经 `note_never_started` 照「没起来」进本机那一行与死亡账，
/// 不另起一条要人去做什么的提示。
#[test]
fn the_user_actionable_start_failures_all_reach_the_user() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    assert!(
        prod.len() > 20_000,
        "`local_backend_host.rs` 的生产段只剩 {} 字节（下限 20_000）—— 剥过头了，本条在空转",
        prod.len()
    );
    assert_eq!(
        prod.matches("StartOutcome::Failed {").count(),
        6,
        "`StartOutcome::Failed` 的构造点不是 6 处了 —— 新增一处失败就给它分档：\n\
             动得了手 ⇒ 调 `note_local_refused` 把下一步交给本机那一行；诚实降级 ⇒ 只进日志。\n\
             改完把这个数改对，别只改数。"
    );
    // 握手 / 钥匙读不动都原样进「接不上」那一臂，那一臂交下一步。
    let adopt = body_of(&prod, "fn adopt_with(");
    assert!(
        adopt.contains("Err(e) => Adopt::Refused(e)"),
        "握手失败不再原样交给「接不上」那一臂"
    );
    assert_eq!(
        prod.matches("note_local_refused(").count() - prod.matches("fn note_local_refused(").count(),
        3,
        "`note_local_refused` 在这里不是 3 处（被拒那一臂交一句 · 起之前清一次 · 停掉口上那一份之后清一次）——\n\
             少了 = 某一条动得了手的路退回了只写日志；多了 = 又有一条路分进这一档，回来把本条一起改。"
    );
}

/// ★★ **放行理由必须覆盖那处 `sleep` 的每一个用途。**
///
/// 那处 `sleep` 住 `listen_pause`，它有**两个**调用方：接管刚脱离起来的那个后端（等它把口听上）·
/// 监护刚起的那一个（等它说上话）。理由只写一件就只说清了一半。
///
/// ★ 理由就是那处放行的诚实边界 —— 说法与代码不是一回事时，放行在替一个不存在的性质背书。
/// 而理由是散文，散文不会自己红 ⇒ 本条把它变成机检：**调用方的名字从生产码里派生**（不写死），
/// 逐个要求那一处 `#[expect(…, reason = "…")]` 点得到。多一个调用方而不去改理由 ⇒ 当场红。
#[test]
fn the_one_wait_here_names_every_caller_in_its_reason() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    // 反空真①：本文件生产段里就该恰好一处 `sleep`（登记表登的 `处数` 就是这个 1）。
    let sleeps = prod.matches("thread::sleep(").count();
    assert_eq!(
        sleeps, 1,
        "生产段里 `thread::sleep(` 有 {sleeps} 处（登记表登的是 1 处）——\
             多一处就要回答它属哪一类、等什么、上限多少"
    );

    // 每一行的「外层函数是谁」：按行扫，遇到 `fn 名(` 就换人。
    let enclosing = |needle: &str, skip_def: bool| -> Vec<String> {
        let mut cur = String::new();
        let mut out: Vec<String> = Vec::new();
        for l in prod.lines() {
            let t = l.trim_start();
            if let Some(rest) = t
                .strip_prefix("pub async fn ")
                .or_else(|| t.strip_prefix("pub fn "))
                .or_else(|| t.strip_prefix("async fn "))
                .or_else(|| t.strip_prefix("fn "))
            {
                if let Some(name) = rest.split(['(', '<']).next() {
                    cur = name.trim().to_string();
                }
            }
            if l.contains(needle)
                && !(skip_def && t.starts_with("fn "))
                && cur != needle.trim_end_matches('(')
            {
                out.push(cur.clone());
            }
        }
        out.sort();
        out.dedup();
        out
    };

    // 那一处 `sleep` 住哪个函数。
    let home_of_sleep = enclosing("thread::sleep(", false);
    assert_eq!(
        home_of_sleep,
        vec!["listen_pause".to_string()],
        "那处 `sleep` 不住 `listen_pause` 了（现在住 {home_of_sleep:?}）——\
             它搬家了，本条与登记表那一行都要跟着重判"
    );

    // 谁在用它 —— **从代码派生，不写死**（它是当函数值交出去的，不带括号）。
    let callers = enclosing("listen_pause", true);
    // 反空真②：少于两个调用方 ⇒ 抽取坏了（或真的只剩一条路，那也要回来重判说法）。
    assert!(
        callers.len() >= 2,
        "只抽到 {} 个 `listen_pause` 的调用方：{callers:?} —— \
             `重-4` 那条病的形状正是「两个调用方只写了一个」，抽不出两个本条就在空转",
        callers.len()
    );

    // 那一处放行的理由（`listen_pause` 头上那个 `#[expect(clippy::disallowed_methods, reason = "…")]`）必须点到每一个。
    // 按**行**切（不按字节偏移切 —— 中文在这份文件里到处都是，按字节切会切在字符中间）。
    let lines: Vec<&str> = prod.lines().collect();
    let def = lines
        .iter()
        .position(|l| l.trim_start().starts_with("fn listen_pause("))
        .expect("生产段里没有 `fn listen_pause(`");
    let open = lines[..def]
        .iter()
        .rposition(|l| l.trim_start().starts_with("#[expect("))
        .expect("`listen_pause` 头上没有 `#[expect(` —— 那一处 `sleep` 的放行不见了");
    let reason: String = lines[open..def]
        .iter()
        .find_map(|l| l.trim_start().strip_prefix("reason = \""))
        .expect("`listen_pause` 头上那个 `#[expect(` 没写 `reason`")
        .to_string();
    assert!(
        reason.len() > 100,
        "切出来的放行理由只有 {} 字节 —— 切错了，下面整段在空转",
        reason.len()
    );
    for c in &callers {
        assert!(
            reason.contains(c.as_str()),
            "`listen_pause` 那一处放行的理由没有点到 `{c}` —— 而它是那处 `sleep` 的调用方之一。\n\
                 ★ 理由要说清等什么、上限是多少，而两个调用方等的**不是同一件事**\n\
                 （一个等刚脱离的那个听上口、一个等刚被监护的那个说上话）⇒ 只写其中一个 = 理由在替一个不存在的性质背书。\n\
                 抽到的调用方全体：{callers:?}"
        );
    }
}

/// 〔台架防真家〕仓里起沙箱后端的入口都带「这是一次沙箱跑」的标记（`relay_route_core::SANDBOX_ENV`）：
/// 门禁（摘完继承来的 `CCM_*` 之后）· e2e 各套最前面 source 的那一份 · 真进程判据的沙箱环境。变量名两侧逐字一样。
/// 真起一次的那一半在 `e2e_the_door_follows_the_home_not_the_claude_dir`（读常驻后端的 `/proc/<pid>/environ`）。
#[test]
fn every_sandboxed_backend_start_carries_the_sandbox_mark() {
    let want = format!("export {}=1", relay_route_core::SANDBOX_ENV);
    for (script, text, scrub_end) in [
        (
            "tests/scripts/gate.sh",
            include_str!("../../scripts/gate.sh"),
            "unset gate_v",
        ),
        (
            "tests/e2e/sandbox-env.sh",
            include_str!("../../e2e/sandbox-env.sh"),
            "unset _ccm_sandbox_k _ccm_sandbox_names",
        ),
    ] {
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .collect();
        let end = lines
            .iter()
            .position(|l| *l == scrub_end)
            .unwrap_or_else(|| panic!("`{script}` 里找不到摘环境那一段的收尾 `{scrub_end}`"));
        assert!(
            lines[end + 1..].iter().take(3).any(|l| *l == want),
            "`{script}` 摘完继承来的环境之后没有 `{want}` —— 那里起的常驻后端认不出自己是沙箱跑"
        );
    }
    let me = include_str!("local_backend_host_tests.rs");
    // 头那一串拆成两段：整串只该出现在被测那一处（否则先找到的是这一行自己）。
    let envs_fn = body_of(me, concat!("fn envs(&self) -> Vec<(String, String)> ", "{"));
    guard_core::find_pinned(&envs_fn, "relay_route_core::SANDBOX_ENV")
        .expect("真进程判据的沙箱环境里没带「沙箱跑」的标记（恰好一处）");
}

/// 宿主只读不写常驻后端那几份（套接字 · 锁 · 「谁在听」，写者只有常驻后端）；连上之后交的 attach 行不带任何钥匙。
/// 守的要求：`INVARIANTS §48.1`「门由内核给」。
#[test]
fn the_host_never_writes_the_resident_dir_and_carries_no_key() {
    let host = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    for f in ["fn start_detached(", "fn adopt_with(", "fn send_attach("] {
        let fn_text = body_of(&host, f);
        for w in [
            "OpenOptions",
            "fs::write",
            "create_new",
            "rename",
            "remove_file",
        ] {
            assert!(
                !guard_core::contains_word(&fn_text, w),
                "`{f}` 里有写文件的 `{w}` —— 常驻后端那几份的写者只有它自己"
            );
        }
    }
    assert_eq!(
        crate::local_backend_host::attach_line(None, None, None),
        "{\"attach\":true}\n"
    );
    assert_eq!(
        crate::local_backend_host::attach_line(Some("Asia/Shanghai"), None, None),
        "{\"attach\":true,\"tz\":\"Asia/Shanghai\"}\n",
        "看的这一台的时区跟在旁边一格（后端按 `cmd_wire::attach::AttachLine` 读 `tz`）"
    );
    // 本机换装只认同一版（`hello_verdict`）⇒ 不带这一版、不问新旧；名单照本机名单台此刻那份带上。
    guard_core::find_pinned(&host, "let tz = crate::inbound_client::viewer_tz();")
        .expect("本机常驻那条 attach 行的时区不是请求信封那同一个");
    guard_core::find_pinned(
        &host,
        "attach_line(tz.as_deref(), None, Some(watch.clone()))",
    )
    .expect("本机常驻那条 attach 行带了这一版 / 没带名单");
    for w in ["token", "LISTEN_TOKEN"] {
        assert!(
            !guard_core::contains_word(&host, w),
            "宿主生产段里还有钥匙的痕迹 `{w}`"
        );
    }
}

/// ★★ **monitor 这棵树上，没有一份文件让块注释原样穿过去。**
///
/// # 它看着的是哪件事
///
/// 本文件里十来条判据都在数 `guard_core::production_code` 从本文件源码剥出来的那份文本。
/// 共用剥法带一条**静默兜底**：词法与某份文件对不上时
/// （`depth` 不收口 / 还停在字符串里）**一个字都不剥**、原文原样返回 ——
/// 那是「宁可留洞，不许造假红」的取舍（剥狠了会砍掉真代码，那是拿假阳换真阳）。
///
/// ⇒ 兜底一旦触发，那份文件上「把真代码包进 `/* */` 再换个桩值」的洞**当场重开**，
/// 而所有判据一个数都不动。**本条就是不让那件事静悄悄发生的那道判据**：
/// 塞一行能编译的 `(br"\", "/*")` 让一份文件走进兜底，本条**当场红并逐份点名**，
/// 全树只有本条会说话。
///
/// 地板与 monitor 全树那条「剥得干不干净」的自检同职：**扫到的份数低于它说明遍历坏了**，
/// 不是代码变干净了。⚠ 棘轮纪律：只许升不许降。
///
/// # 两个单位各量一遍
///
/// `guard_core::assert_block_comment_model_holds` 量**两个单位**：
/// 整份文件 ＋ `guard_core::test_attr_chunks` 按 `#[test]` 切出的**每一块**
/// ⇒ 「`/*` 落 A 块、`*/` 落 B 块」（整份配平、单块不配平，rustc 眼里就是条普通跨行块注释）
/// 那一形**本条当场红并点名到块**。只量文件的话，按块剥的判据在那一块上掉进兜底，
/// 把真调用与那行合规的 `PATH` 写位一起放进注释、只留一个桩值，那条判据就判「合规」，而本条照绿。
/// 被它看着的那条守卫（[`tests::every_test_that_starts_the_real_backend_demands_a_private_tmux`]）
/// **先剥整份、再切块** ⇒ 两把尺子的单位是同一个；
/// 这一形有**两个独立的红源**（本条的块那一半 · 那条守卫落回 `MISS_LOCK2`）。
///
/// ⚠ **射程不是全称**：两个单位以外的单位它一个都看不见 ——
/// 函数体窗口（`body_of(..)` / `brace_block(..)`）· 任意切片 `&src[a..b]`。
///
/// ⚠ **覆盖面**：本条只喂 monitor 这半边；backend 侧与 guard-core 各有自己那一条。
/// 共享 crate 里不归这几条管的源文件，生产段也经 `guard_core::production_code` 剥，
/// 它掉进兜底的话这几条判据一条都不会响。
///
/// ⚠ 两个地板各自量、各自报（**块数不是文件数**）：`min_files` 挡「遍历坏了」，
/// `min_blocks` 挡「切法坏了」。棘轮纪律：只许升不许降。
#[test]
fn no_monitor_file_falls_back_to_leaving_block_comments_in() {
    // 🔴 **两棵树一起喂。**
    //
    // monitor 这半边今天住两处：生产段 `src/frontend/shell/src`、测试段 `<repo>/tests/frontend/shell`。
    // 而本条第二个单位（`#[test]` 块）**整个住在测试段** ⇒ 只喂 `src/` 那一棵的话
    // 块数掉到三分之一，「没有文件走兜底」这个结论就是在三分之一的语料上得出的。
    // 块数地板 1050：登记表（一百五十来块）住 `tests/common/guard-core/rules/`，归 guard-core 那一条量（它的地板跟着抬）。
    guard_core::assert_block_comment_model_holds(
        &[
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &guard_core::addr::tests_root().join("frontend/shell"),
            // 文件窗口独立成包：它的判据搬到 `tests/frontend/filewin/`（代码随 `src` 那棵根的人群声明收）。
            &guard_core::addr::tests_root().join("frontend/filewin"),
        ],
        100,
        1050,
    );
}

// ══════════════════════════════════════════════════════════════════
// 同机共用 · 不留僵尸：**真进程**判据（`#[ignore]`，`cargo test` 不跑它们）。
//
// 入口是 `tests/e2e/local-backend-supervise.sh`（第二趟 `--ignored`），执行链有两条：
// 本机门禁 `tests/scripts/gate.sh` 的 `run_e2e local-backend <地板>` 与 `ci.yml` 那一行。
// ══════════════════════════════════════════════════════════════════

/// 起真后端的沙箱。**三样东西一个都不许缺**（fail closed）。
#[cfg(target_os = "linux")]
struct E2eSandbox {
    bin: std::path::PathBuf,
    /// 前面挂着 tmux shim 的 PATH —— `C7i` 的**唯一**隔离原语。
    shim_path: (String, String),
    work: std::path::PathBuf,
    /// 这一趟起过的那些进程。**一交出 `DETACHED` 就立刻塞进这里**，
    /// 中间不留任何「拿在手里但没人管」的窗口 —— 那个窗口正是 08-26 漏网的机制。
    kept: std::sync::Mutex<Vec<crate::spawn_managed::ManagedChild>>,
    /// 这一趟在 `DETACHED` 里出现过的每一个 pid。**`kept` 那一格兜不住脱离那条路**：
    /// 「上一个宿主退了」一关写半边、那条流就断 ⇒ `reap_detached` 当场把 `Child` 取进收尸线程
    /// （它在那儿等一个还活着的进程）⇒ [`Self::forget_like_a_host_that_exited`] 拿到的是 `None`、`Drop` 手里没有它
    /// ⇒ 同机共用那条里第一个宿主起的那个后端会活过本条（`local-backend-supervise.sh`
    /// 收尾只收本套件自己的、先查漏网，漏了就红）。
    /// ⇒ 按 pid 收，杀前核 `/proc/<pid>/exe` 是不是 [`Self::bin`]（防 pid 复用误伤）。
    seen: std::sync::Mutex<Vec<u32>>,
}

#[cfg(target_os = "linux")]
impl E2eSandbox {
    fn demand() -> Self {
        let bin = std::env::var("CCM_E2E_BACKEND").expect("要 CCM_E2E_BACKEND");
        // ★ **fail closed**：拿不到 shim 就炸，绝不降级裸跑 ——
        //   裸跑 = 去改用户真实 tmux server 的 `[50]` 槽位（08-11 / 08-26 各出过一次）。
        let shim = demand_tmux_shim("委托腿上的 PATH 全由本处一处代办");
        let work =
            std::path::PathBuf::from(std::env::var("CCM_E2E_WORK").expect("要 CCM_E2E_WORK"));
        Self {
            bin: std::path::PathBuf::from(bin),
            shim_path: (
                "PATH".to_string(),
                format!("{shim}:{}", std::env::var("PATH").unwrap_or_default()),
            ),
            work,
            kept: std::sync::Mutex::new(Vec::new()),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// 记下一个出现在 `DETACHED` 里的 pid（0 = 不知道，不记）。
    fn remember(&self, pid: u32) {
        if pid != 0 {
            self.seen
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(pid);
        }
    }

    /// 「上一个宿主退了」= 把进程内那点状态清掉，**但不碰那个进程**；
    /// 它手里那个 `Child` **当场**交给沙箱看着。
    fn forget_like_a_host_that_exited(&self) {
        // 先把手里那个 `Child` 拿走（流一断，读端收尾会去收尸 ⇒ 晚了就被它拿走了）。
        let taken = DETACHED.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = &taken {
            self.remember(h.pid);
        }
        if let Some(c) = taken.and_then(|h| h.child) {
            self.kept.lock().unwrap_or_else(|e| e.into_inner()).push(c);
        }
        // ★ **真宿主退出时那条 socket 会关掉** —— 只清进程内的句柄是**演砸的模型**：那条流还挂着，下一个宿主认不出已有实例。
        if let Some(c) = crate::inbound_client::local_client() {
            crate::inbound_client::unregister(&c);
            // ⚠ `shutdown()` 只叫醒等着的调用方，**它不关 socket**；关写半边才是后端那边的 EOF。
            //   等到后端处理完这条流结束（它收掉这条流、我们的读端见了底）。
            let ended = c.close_write();
            c.shutdown();
            let done = tauri::async_runtime::block_on(async move {
                tokio::time::timeout(guard_core::wall::CEILING, ended).await
            });
            assert!(done.is_ok(), "关了写半边，到兜底上限后端还没收掉这条流");
        }
    }

    /// 造一个**独立的家** —— 端口是家（`$HOME/.cc-monitor`）算出来的，所以不同的家 = 不同的口。
    /// 这正是同机共用那条「非空对照」要用的东西。
    fn become_host_with_home(&self, tag: &str) -> std::path::PathBuf {
        let home = self.work.join(format!("host-{tag}"));
        let claude = home.join(".claude");
        std::fs::create_dir_all(claude.join("projects")).expect("建沙箱 HOME");
        std::fs::create_dir_all(claude.join("sessions")).expect("建沙箱 sessions");
        // ⚠ 进程级环境变量 ⇒ 这几条判据**必须 `--test-threads=1`**
        //   （e2e 脚本正是这么跑的）。写在这里是因为「宿主是谁」本来就是进程级的事实。
        std::env::set_var("HOME", &home);
        std::env::set_var("CLAUDE_CONFIG_DIR", &claude);
        claude
    }

    fn envs(&self) -> Vec<(String, String)> {
        vec![
            self.shim_path.clone(),
            // 「这是一次沙箱跑」：要占真家里的门牌就拒绝起（台架防真家）。
            (relay_route_core::SANDBOX_ENV.to_string(), "1".to_string()),
        ]
    }
}

/// ★★ **判据不许在机器上留活进程** —— 而且这一条比它听起来重要得多。
///
/// 〔08-26 实测教训，实现期间真发生了一次〕漏一个的后果**不是**「多一个进程」：
/// e2e 的 trap 会把 shim 目录 `rm -rf` 掉，而那个漏网的 backend **还活着**；
/// 它下一次装 tmux hook（watcher 换人时会重装）沿 PATH 找不到 shim ⇒ **落到真 tmux 上**
/// ⇒ 用户真实 server 的 `[50]` 槽位被盖成它的 pid。当时按原值恢复了，但那是靠人。
///
/// ⇒ 收尾放进 `Drop`：**测试 panic 时它照样跑**（unwind 会走析构），
/// 而写在测试体末尾的收尾在第一条断言红掉时就被跳过了 —— 那正是这次漏网的机制。
#[cfg(target_os = "linux")]
impl Drop for E2eSandbox {
    fn drop(&mut self) {
        // ⚠ **按句柄收，不扫 `/proc`**：扫目录会撞 `scanning_guard_registry`
        // （「有扫描型判据在测试段里裸遍历目录」），而且按 exe 路径杀是**按模式杀** ——
        // 这台机器上还跑着用户自己的真后端，那种收法迟早会误伤。
        // ⇒ 只收**我们自己起出来的那几个句柄**，一个不多一个不少。
        let mut all: Vec<crate::spawn_managed::ManagedChild> =
            std::mem::take(&mut *self.kept.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(h) = DETACHED.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.remember(h.pid);
            all.extend(h.child);
        }
        for mut c in all {
            let _ = c.kill();
            let _ = c.wait();
        }
        // 句柄已经不在我们手里的那几个（见 `seen` 头注）：按 pid 收，只收二进制对得上的。
        let bin = std::fs::canonicalize(&self.bin).unwrap_or_else(|_| self.bin.clone());
        let seen = std::mem::take(&mut *self.seen.lock().unwrap_or_else(|e| e.into_inner()));
        for pid in seen {
            let exe = std::fs::read_link(format!("/proc/{pid}/exe"));
            if exe.as_deref().ok() != Some(bin.as_path()) {
                continue; // 已经走了，或 pid 被别人复用了 —— 都不许动
            }
            let _ = std::process::Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
            // 它是本进程的子进程（`process_group` 不改父子关系）⇒ 收尸线程会 `wait` 它；这里只等它从表里消失。
            for _ in guard_core::wall::rounds() {
                if !alive(pid) || proc_state(pid) == Some('Z') {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
}

/// 把结局拆开看。**别写成 `matches!` + 一句「起不出来」** ——
/// 那样红的时候只知道「不是 `Started`」，而四种结局的排查方向完全不同
/// （没走那条路 / 已经在跑 / 起不出来各有各的下一步）。
#[cfg(target_os = "linux")]
fn expect_started(out: DetachOutcome, who: &str) -> std::path::PathBuf {
    match out {
        DetachOutcome::Done(StartOutcome::Started(p)) => p,
        DetachOutcome::Done(StartOutcome::AlreadyRunning) => {
            panic!("{who}：期望起一个新的，实得「已经在跑」—— 上一条判据留了活进程？")
        }
        DetachOutcome::Done(StartOutcome::Failed { reason, looked_at }) => {
            panic!("{who}：起不出脱离的 backend —— {reason}；找过 {looked_at:?}")
        }
        DetachOutcome::NotTaken => panic!(
            "{who}：**这条路根本没走**（`detach_wanted` 判了 false）。\
                 不是 Linux？还是 `{NO_DETACH_ENV}` 被设了？"
        ),
    }
}

/// 同上，反过来那一面。**「没认出来」的四种来路完全不同**，红的时候要说得出是哪一种。
#[cfg(target_os = "linux")]
fn expect_adopted(out: DetachOutcome, who: &str) {
    match out {
        DetachOutcome::Done(StartOutcome::AlreadyRunning) => {}
        DetachOutcome::Done(StartOutcome::Started(p)) => panic!(
            "{who}：**又起了一个**（{}）—— 那正是「每台机 N 个后端互相盖 [50] 槽位」",
            p.display()
        ),
        DetachOutcome::Done(StartOutcome::Failed { reason, .. }) => {
            panic!("{who}：没认出已有实例 —— {reason}")
        }
        DetachOutcome::NotTaken => panic!("{who}：这条路根本没走"),
    }
}

#[cfg(target_os = "linux")]
fn proc_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().next()?.chars().next()
}

#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// ★★ **同机第二个宿主不许起第二个 backend —— 它必须接上第一个。**
///
/// # 为什么这一格是硬的
///
/// backend 一起来就**无条件**往 tmux server 装三条全局 hook、固定槽位 `[50]`、
/// **没有关掉它的开关**，载荷里烤着那一个后端的 pid+starttime
/// ⇒ **脱离而不认已有实例 = 每台机 N 个后端互相盖槽位，比今天更糟**。
///
/// # 它怎么会失效 —— 以及本条为此付了什么
///
/// 「夹具只起一个宿主 ⇒『认已有实例』那一支恒不走，判据是**空真**」。
/// ⇒ 本条有**两个宿主**，而且配了一个**非空对照**：换一个家目录（= 换一个口）之后
/// 同样两步必须**真的起出第二个进程**。两个读数一起看才说明「不起第二个」是**判据在起作用**，
/// 而不是「这条路本来就起不出进程」。
#[test]
#[ignore]
fn e2e_a_second_host_adopts_the_running_backend_instead_of_starting_a_second_one() {
    #[cfg(target_os = "linux")]
    {
        let _guard = crate::inbound_client::local_origin_test_lock();
        let sb = E2eSandbox::demand();
        let _ = sb.become_host_with_home("adopt");
        *DETACHED.lock().expect("锁") = None;

        // ── 宿主① ────────────────────────────────────────────────
        let bin = sb.bin.clone();
        expect_started(
            start_detached(&|| Ok(bin.clone()), &bin, &sb.envs()),
            "第一个宿主",
        );
        let pid1 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
        assert!(alive(pid1), "起出来的 pid={pid1} 不在进程表里");
        println!("E2E-OK SHARE 第一个宿主起出脱离的后端（pid={pid1}）");

        // 「谁在听那个口」那份记录要真的落了盘 —— 没有它，下一个宿主停不了它。
        let home = crate::config::resolve_monitor_data_dir().expect("沙箱的家");
        let home = relay_route_core::listen_socket_for(&home);
        assert_eq!(listener_pid(&home), pid1, "套接字的对端不是起出来的那一个");
        println!("E2E-OK SHARE 套接字的对端就是起出来的那一个（停按钮的凭据）");

        // ── 宿主①退出（不碰那个进程 —— 那正是「脱离」）────────────
        sb.forget_like_a_host_that_exited();
        assert!(
            alive(pid1),
            "上一个宿主一退，那个后端就跟着走了 —— 那不叫脱离"
        );
        println!("E2E-OK SHARE 宿主退出之后那个 backend **还活着**（真脱离）");

        // ── 宿主② ────────────────────────────────────────────────
        let bin2 = sb.bin.clone();
        expect_adopted(
            start_detached(&|| Ok(bin2.clone()), &bin2, &sb.envs()),
            "第二个宿主",
        );
        let pid2 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
        assert_eq!(pid2, pid1, "第二个宿主接上的不是同一个进程");
        assert!(
            crate::inbound_client::local_client().is_some(),
            "接上了却没登记入方向通道 —— 那只是「连上了」，不是「接上了」"
        );
        println!("E2E-OK SHARE 第二个宿主**接上**了同一个后端（pid 不变），没起第二个");

        // ── ★ 非空对照：换一个家 = 换一个套接字 ⇒ 必须**真的**起出第二个 ──
        sb.forget_like_a_host_that_exited();
        let _ = sb.become_host_with_home("control");
        assert_ne!(
            relay_route_core::listen_socket_for(
                &crate::config::resolve_monitor_data_dir().expect("沙箱的家")
            ),
            relay_route_core::listen_socket_for(&home),
            "两个家算出同一个套接字 —— 那这条对照说明不了任何事"
        );
        *DETACHED.lock().expect("锁") = None;
        let bin3 = sb.bin.clone();
        expect_started(
            start_detached(&|| Ok(bin3.clone()), &bin3, &sb.envs()),
            "非空对照（换一个数据目录 = 换一个口）",
        );
        let pid3 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
        assert_ne!(pid3, pid1, "对照组拿到了同一个 pid —— 对照不成立");
        println!("E2E-OK SHARE 非空对照：换一个数据目录（换一个口）**真的**起出了第二个进程");

        // ── 收尾：全交给沙箱的 `Drop`（**panic 时它照样跑**，写在这里的收尾不会）──
        //    最后那个还在 `DETACHED` 里，`Drop` 会把它一起收掉。
        let _ = std::env::var("CCM_E2E_TMUX_SHIM_BIN"); // 让本条自己也点名那个隔离
    }
}

/// ★★ **脱离之后不留僵尸。**
///
/// # 断言必须落在**进程表**上，不是落在我们自己的事件上
///
/// `spawn_managed.rs::Lifetime` 的 `Detached` 那一格逐字：「`process_group` **不改变父子关系** ⇒ 不 `wait` 就留僵尸」。
/// 若断言落在「消费者收到了 EOF」上，僵尸照样在 —— 那条断言会绿着放过整个缺陷。
/// ⇒ 这里读 `/proc/<pid>/stat` 的**状态字**：既不许是 `Z`，最后也不许还在。
#[test]
#[ignore]
fn e2e_a_detached_backend_that_dies_leaves_no_zombie() {
    #[cfg(target_os = "linux")]
    {
        let _guard = crate::inbound_client::local_origin_test_lock();
        let sb = E2eSandbox::demand();
        let _ = sb.become_host_with_home("zombie");
        *DETACHED.lock().expect("锁") = None;

        let bin = sb.bin.clone();
        expect_started(
            start_detached(&|| Ok(bin.clone()), &bin, &sb.envs()),
            "本条",
        );
        let pid = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
        assert!(alive(pid), "起出来的 pid={pid} 不在进程表里");

        // **从外面**把它结束掉（不是走我们的 `stop`）—— 那才是「它自己崩了」的形状。
        // 生产段那个发信号的 `signal_term` 删了（「停」改走一次性 `--resident-stop`）⇒ 这里自己发。
        let st = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .expect("起不来 kill");
        assert!(st.success(), "发不出 SIGTERM：{st}");

        // 流断了 ⇒ 读方拿到 EOF ⇒ 收尸。**事件驱动**，这里只是等那个事件走完。
        let mut gone = false;
        let mut ever_zombie = None;
        for _ in guard_core::wall::rounds() {
            match proc_state(pid) {
                None => {
                    gone = true;
                    break;
                }
                Some('Z') => ever_zombie = Some('Z'),
                Some(_) => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            gone,
            "pid={pid} 到兜底上限还在进程表里（最后见到的状态：{:?}）。\n\
                 ★ 若它是 `Z`：那就是这条 DoD 要防的僵尸 —— `process_group` 不改父子关系，\n\
                 不 `wait` 它就一直挂在进程表上，而我们自己的事件（EOF）**早就到了**。",
            ever_zombie
        );
        println!("E2E-OK REAP 脱离的后端被结束之后进程表里那个 pid 消失了（不是 Z）");
        // ★ 收尸走的是**我们自己那条线**：`Child` 被 `reap_detached` 取走交给收尸线程，
        //   而句柄里 **pid 与二进制路径都还在** —— 「停」那一步照样有凭据。
        //   ⚠ 这两半要一起断：只断「pid 没了」的话，把整条收尸线删掉、改成
        //   「进程自己被 init 收走」也会绿（而那要等 monitor 退出）。
        {
            let g = DETACHED.lock().expect("锁");
            let h = g.as_ref().expect("句柄");
            assert!(
                h.child.is_none(),
                "`Child` 还留在句柄里 —— `reap_detached` 没把它交出去，那条收尸线没跑"
            );
            assert_eq!(h.pid, pid, "收尸之后 pid 记录被抹了 —— 「停」就没凭据了");
            assert!(
                !h.bin.as_os_str().is_empty(),
                "二进制路径被抹了 —— 身份核对没了对照物"
            );
        }
        println!("E2E-OK REAP 收尸走的是我们自己那条线（`Child` 交给了收尸线程，pid+二进制仍在）");
        *DETACHED.lock().expect("锁") = None;
        let _ = std::env::var("CCM_E2E_TMUX_SHIM_BIN");
    }
}

/// ★ 常驻后端起的子进程不带它自有的那几格环境（监听口 · 钥匙文件 · 中转口 · 诊断文件）。
///
/// 量法：PATH 最前面挂一个记账的 `tmux`（把它自己环境里 `CCM_` 开头的名字记一行，再 `exec` 隔离用的 shim），
/// 起一个真的脱离后端，等它起一次 tmux（观测那一路一上来就问），读账。
/// 正控：常驻后端自己的环境里那几格都在（读的是对的进程）；账至少一行（它真起过 tmux）。
/// 故意交下去的（家 `CCM_DATA_DIR`）与测试台自己的（`CCM_E2E_*`）不算自有的。
#[test]
#[ignore]
fn e2e_children_of_the_resident_backend_carry_none_of_its_own_env() {
    #[cfg(target_os = "linux")]
    {
        let _guard = crate::inbound_client::local_origin_test_lock();
        let sb = E2eSandbox::demand();
        let _ = sb.become_host_with_home("child-env");
        *DETACHED.lock().expect("锁") = None;
        let home = crate::config::resolve_monitor_data_dir().expect("沙箱的家");
        let wrap = home.join("wrap-bin");
        std::fs::create_dir_all(&wrap).expect("建记账目录");
        let log = home.join("tmux-env.log");
        let shim = demand_tmux_shim("记账的 tmux 之后接隔离的那一个");
        let script = format!(
            "#!/bin/sh\nenv | sed -n 's/^\\(CCM_[A-Z_]*\\)=.*/\\1/p' | sort -u | tr '\\n' ' ' >> '{}'\necho >> '{}'\nexec '{}/tmux' \"$@\"\n",
            log.display(),
            log.display(),
            shim
        );
        std::fs::write(wrap.join("tmux"), script).expect("写记账的 tmux");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(wrap.join("tmux"), std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        let mut env = sb.envs();
        for (k, v) in env.iter_mut() {
            if k == "PATH" {
                *v = format!("{}:{v}", wrap.display());
            }
        }
        let bin = sb.bin.clone();
        expect_started(start_detached(&|| Ok(bin.clone()), &bin, &env), "本条");
        let pid = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
        sb.remember(pid);
        let own = ["CCM_RESIDENT", "CCM_BACKEND_STDERR_LOG"];
        let mine: Vec<String> = environ_of(pid).into_iter().map(|(k, _)| k).collect();
        for k in own {
            assert!(
                mine.iter().any(|m| m == k),
                "正控：常驻后端自己的环境里没有 {k}（读错了进程？）"
            );
        }
        let mut lines: Vec<String> = Vec::new();
        for _ in guard_core::wall::rounds() {
            lines = std::fs::read_to_string(&log)
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect();
            if !lines.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            !lines.is_empty(),
            "正控：到兜底上限常驻后端一次 tmux 都没起 —— 这条量不到东西"
        );
        let leaked: Vec<&str> = lines
            .iter()
            .flat_map(|l| l.split_whitespace())
            .filter(|n| own.contains(n) || *n == "CCM_RELAY_PORT")
            .collect();
        assert_eq!(
            leaked,
            Vec::<&str>::new(),
            "常驻后端起的 tmux 带着它自有的环境：{lines:?}"
        );
        println!(
            "E2E-OK CHILD-ENV 常驻后端起的 tmux 不带它自有的那几格环境（记了 {} 次，自己身上那几格都在）",
            lines.len()
        );
        let _ = std::env::var("CCM_E2E_TMUX_SHIM_BIN");
    }
}

/// 读一个进程**起来时**的整份环境（`/proc/<pid>/environ`，NUL 分隔）。
#[cfg(target_os = "linux")]
fn environ_of(pid: u32) -> Vec<(String, String)> {
    let raw = std::fs::read(format!("/proc/{pid}/environ")).expect("读 /proc/<pid>/environ");
    raw.split(|b| *b == 0)
        .filter(|kv| !kv.is_empty())
        .filter_map(|kv| {
            let kv = String::from_utf8_lossy(kv);
            let (k, v) = kv.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

/// 门牌只跟着家走：**从带另一个 `CLAUDE_CONFIG_DIR` 的终端起 monitor，本机后端照样接得上**
/// （同一个进程，不起第二个）；停它的那一趟（`--resident-stop`，后端自己按家算口）找得到同一个；再起时 monitor 不另交数据目录。
/// 守的要求：「一台机器一个常驻后端」「门牌只跟着家走」；体检读数：改一个设置 ⇒ 本机后端一个都接不上 / 一台两个后端。
/// 非空对照：同机共用那一条「换一个家 = 换一个口 ⇒ 真的起出第二个」。
#[test]
#[ignore]
fn e2e_the_door_follows_the_home_not_the_claude_dir() {
    #[cfg(target_os = "linux")]
    {
        let _guard = crate::inbound_client::local_origin_test_lock();
        let sb = E2eSandbox::demand();
        let claude_a = sb.become_host_with_home("door");
        *DETACHED.lock().expect("锁") = None;
        // 起它的环境 = shim 的 PATH ＋ 生产那一份（去掉中转口：不许让测试后端去绑这台机器真的那个中转口）。
        let env_now = || {
            let mut env = sb.envs();
            env.extend(
                backend_env()
                    .into_iter()
                    .filter(|(k, _)| k != "CCM_RELAY_PORT"),
            );
            env
        };
        let bin = sb.bin.clone();
        expect_started(
            start_detached(&|| Ok(bin.clone()), &bin, &env_now()),
            "第一次起",
        );
        let pid1 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
        sb.remember(pid1);
        // 常驻后端的环境里只有常驻开关，没有任何钥匙或钥匙文件（门由内核给）。
        let env1 = environ_of(pid1);
        assert!(
            env1.contains(&(RESIDENT_ENV.to_string(), "1".to_string())),
            "正控：环境里没有常驻开关 —— 读错了进程或那一格没交"
        );
        assert_eq!(
            env1.iter()
                .filter(|(k, _)| k.starts_with("CCM_LISTEN"))
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            Vec::<&str>::new(),
            "常驻后端的环境里还有监听口 / 钥匙那一族"
        );
        assert!(
            env1.contains(&(relay_route_core::SANDBOX_ENV.to_string(), "1".to_string())),
            "沙箱起的常驻后端没带「沙箱跑」的标记"
        );
        println!("E2E-OK HOME-0 常驻后端只拿到常驻开关与沙箱标记，没有钥匙");
        let seen = |pid: u32| -> Option<String> {
            environ_of(pid)
                .into_iter()
                .find(|(k, _)| k == "CLAUDE_CONFIG_DIR")
                .map(|(_, v)| v)
        };
        assert_eq!(
            seen(pid1),
            Some(claude_a.display().to_string()),
            "没填覆盖时后端该沿用起它的那个环境"
        );

        // ── 换一个带别的 `CLAUDE_CONFIG_DIR` 的终端起 monitor ──
        sb.forget_like_a_host_that_exited();
        let home_dir = std::path::PathBuf::from(std::env::var("HOME").expect("沙箱 HOME"));
        let claude_c = home_dir.join("claude-c");
        std::fs::create_dir_all(&claude_c).expect("建 claude-c");
        std::env::set_var("CLAUDE_CONFIG_DIR", &claude_c);
        let bin2 = sb.bin.clone();
        expect_adopted(
            start_detached(&|| Ok(bin2.clone()), &bin2, &env_now()),
            "改了设置之后再起",
        );
        assert_eq!(
            DETACHED.lock().expect("锁").as_ref().expect("句柄").pid,
            pid1,
            "换了 Claude 目录之后接上的不是同一个后端"
        );
        println!("E2E-OK HOME-1 换了带别的 Claude 目录的终端：本机后端照样接得上，还是同一个（pid={pid1}）");

        // ── 停：一次性 `--resident-stop` 自己按家算口、在家里找 pid 记录 ──
        match stop_detached() {
            Some(Ok(a)) => assert_eq!(
                (a.stopped, a.pid),
                (crate::remote_resident::StopWord::Graceful, Some(pid1)),
                "停的那一趟没找到同一个后端"
            ),
            other => panic!("停不掉：{other:?}"),
        }
        assert!(
            !alive(pid1) || proc_state(pid1) == Some('Z'),
            "说停了，pid={pid1} 还在跑"
        );
        println!("E2E-OK HOME-2 停的那一趟（后端自己按家算口）找到了同一个后端并让它收尾退出");

        // ── 再起：monitor 不交数据目录（设置里那一格由后端自己读 `prefs.json`，`machine_prefs_tests` 钉），后端拿到的是起它的那个环境 ──
        sb.forget_like_a_host_that_exited();
        let bin3 = sb.bin.clone();
        expect_started(
            start_detached(&|| Ok(bin3.clone()), &bin3, &env_now()),
            "停了之后再起",
        );
        let pid2 = DETACHED.lock().expect("锁").as_ref().expect("句柄").pid;
        sb.remember(pid2);
        assert_ne!(pid2, pid1);
        assert_eq!(
            seen(pid2),
            Some(claude_c.display().to_string()),
            "monitor 另交了一个 Claude 目录（数据目录那一格该由后端自己读）"
        );
        println!("E2E-OK HOME-3 再起时 monitor 不另交 Claude 目录，后端拿到的是起它的那个环境");
        let _ = std::env::var("CCM_E2E_TMUX_SHIM_BIN");
    }
}

/// 查状态不被一趟起 / 停挡住：起停串行锁（`LIFECYCLE`）被一趟攥着时，`local_pid_and_attempts`（`backend_status` 本机那一支）
/// 照样当场回来。守的要求：「锁不跨长等待」（停后端要是在锁里无期限地等，`backend_status` 要的那把锁就被它攥着）。
#[test]
fn status_is_not_blocked_while_a_start_or_stop_holds_the_lifecycle_lock() {
    let (held_tx, held_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let _g = LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner());
        held_tx.send(()).expect("报已攥住");
        let _ = release_rx.recv();
    });
    held_rx.recv().expect("等它攥住");
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done_tx.send(local_pid_and_attempts().is_ok());
    });
    let got = done_rx.recv_timeout(guard_core::wall::CEILING);
    release_tx.send(()).expect("放手");
    holder.join().expect("攥锁那条线程");
    assert_eq!(
        got,
        Ok(true),
        "起 / 停攥着串行锁时，查状态被挡住了（或报错）"
    );
}

/// 停那一趟带期限：替身后端不答话（一直睡），期限到 ⇒ 当场回来、说「没答完」，并把那一趟收掉。
/// 守的要求：「等待带期限」（不用没有期限的 `wait_with_output()`）。
#[cfg(unix)]
#[test]
fn the_stop_run_gives_up_at_its_deadline() {
    let dir = std::env::temp_dir().join(format!("lbh-stop-deadline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let fake = dir.join("ccm");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\nexec sleep {}\n",
            2 * guard_core::wall::CEILING_SECS
        ),
    )
    .expect("写替身");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let t0 = std::time::Instant::now();
    let got = run_resident_stop_within(&fake, std::time::Duration::from_millis(300));
    let took = t0.elapsed();
    let _ = std::fs::remove_dir_all(&dir);
    let want = copy_core::copy_text(
        "rsLocalBackendHost.stop.timedOut",
        &[(
            "dur",
            &copy_core::format_elapsed(std::time::Duration::from_millis(300)),
        )],
    );
    assert_eq!(
        got.err().map(|s| s.said),
        Some(want.to_string()),
        "不答话的那一趟没按期限回来"
    );
    assert!(
        took < guard_core::wall::CEILING,
        "期限 300 毫秒，实际等了 {took:?} —— 没按期限收"
    );
}

// ══ 死亡账接线 —— 三处观测点真的喂到了 `record_death` ═══════════
//
// 有判据 · 有账 · 有文案，而没有生产调用点，那本账就永远是空的。
// 下面这几条钉的是三处观测点真的调了 `record_death`，
// 而且**每一处判出来的是对的那一格**。

/// 本机后端那本死亡账的读数（三处接线都记在生产那一本 `local_ledger` 上）。
fn local_health() -> crate::backend_policy::Health {
    crate::backend_policy::local_ledger().health()
}

/// ★★ `KP3W2` 脱离路：**`Handshake` 那一维不是一个孤零零的字面量。**
///
/// 两半缺一不可：
/// ① **纯函数**那一半 —— 拿一份**真的**见证进去得到「说过话」。
///    见证只有一个构造入口（`BackendHello::of`，由 `inbound_client`
///    那条「见证不许凭空造」的判据钉着）⇒ 这一格拿不到假见证，
///    而那正是这条推理**在类型上**成立的地方。
///    死值验：把 `handshake_from_hello` 的返回改成 `NeverSpoke` ⇒ 本格当场红。
/// ② **接线**那一半 —— 那道门还在，而且证据是从它那儿取的、一路交到收尸那一刻。
#[test]
fn the_detached_handshake_is_derived_from_the_hello_gate() {
    let line = concat!(
        r#"{"kind":"hello","build_id":"kp3b-jia","host_arch":"x86_64","#,
        r#""commands":["ping"]}"#
    );
    let frame = crate::stream_source::decode(line).expect("这一行该解析成 hello 帧");
    let witness = crate::inbound_client::witness_of(&frame)
        .expect("hello 帧该给得出见证 —— 给不出的话下面那一格是空转的");
    assert_eq!(
        handshake_from_hello(&witness),
        crate::backend_policy::Handshake::Spoke,
        "拿到了一份**真的** hello 见证，这一维却不是「说过话」——\n\
             ★ 2026-07-09 那次死循环的判别式就是它：「非零退出 **且** 从来没说过话」= 被拒了，\n\
             两个条件缺一不可。这一维答错，脱离路上一个说过话之后 `exit 2` 的 backend\n\
             会被判成「被拒了」，而下一步的建议正好相反。"
    );

    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    assert!(
        prod.len() > 20_000,
        "剥完只剩 {} 字节 —— 本条在空转",
        prod.len()
    );
    // 那道门本身（`attach_stream` 的前两行之一，本机两条载体共用的那一处读 hello 的口 `local_backend::local_witness`）：它没了，上面那条推理就没了前提。
    guard_core::find_pinned(&prod, "local_backend::local_witness(&frame)").unwrap_or_else(|e| {
        panic!(
            "`attach_stream` 那道 hello 门不是恰好一处（{e}）——\n\
                     ⇒ 「进得了流循环 = 一帧合法 hello 已经到手」这条推理没有前提了，\n\
                     而收尸那一刻报的仍然是「说过话」：那时它才真的变成一个编出来的值。"
        )
    });
    for l in [
        "let handshake = handshake_from_hello(&witness);",
        "reap_detached(handshake, reader_end);",
    ] {
        guard_core::pin_line(&prod, l).unwrap_or_else(|why| {
            panic!(
                "{why}\n\
                     ⇒ 脱离路那两维不再是**从观测一路带下来的**。\n\
                     少了第一行 = `Handshake` 又变回一个写死在收尸处的字面量；\n\
                     少了第二行 = 那两维根本没交到 `wait()` 回来那一刻手上。"
            )
        });
    }
    // 「起不来」那一支的读端**明写没观测到**，不冒充一次干净 EOF。
    guard_core::pin_line(
        &prod,
        "reader: crate::backend_policy::ReaderEnd::NotObserved,",
    )
    .unwrap_or_else(|why| {
        panic!(
            "{why}\n\
                     ⇒ 「从来没起来」那条路上根本没有读端，而这里给它填了一个别的值。\n\
                     填 `CleanEof` 是一句假话（那条路上从来没有过管子），\n\
                     正是 `platform/fallback_guard.rs` 治的那一族。"
        )
    });
}

/// ★★ `KP3W3` 起不来：**账上那一行里的原因与调用方拿到的那一句逐字相同。**
///
/// 逐字，不是「包含」——「包含」放得过「在正确那句外面又裹了一层自己的措辞」，
/// 而本文件那一族的纪律逐字是「两份措辞迟早对不上」。
#[test]
fn the_never_started_reason_is_the_same_string_the_caller_gets() {
    let _guard = crate::inbound_client::local_origin_test_lock();
    // 中性夹具串：**不取自任何路径或夹具名**。
    let reason = "那一份不在，旁边也没有 ⇒ 拒绝报「已起」".to_string();
    let looked_at = vec![
        std::path::PathBuf::from("/甲/乙"),
        std::path::PathBuf::from("/丙"),
    ];
    let before = local_health();
    let out = note_never_started(StartOutcome::Failed {
        reason: copy_core::said::Said::from(reason.clone()),
        looked_at: looked_at.clone(),
    });
    // ① 结局**原样**还回去 —— 记一笔不许动调用方手上的东西。
    match &out {
        StartOutcome::Failed {
            reason: got,
            looked_at: paths,
        } => {
            assert_eq!(got.said, reason, "记账把调用方那句原因改写了");
            assert_eq!(paths, &looked_at, "记账把「找过哪些地方」改了");
        }
        _ => panic!("`Failed` 进去，出来的不是 `Failed` —— 接线把结局换掉了"),
    }
    // ② 账上**恰好**多一格「从来没起来」，别的三格一动不动。
    let after = local_health();
    assert_eq!(
        (
            after.never_started,
            after.crashed,
            after.refused,
            after.misread
        ),
        (
            before.never_started + 1,
            before.crashed,
            before.refused,
            before.misread
        ),
        "「从来没起来」这一笔没落在它自己那一格上（前 {before:?} / 后 {after:?}）"
    );
    // ③ 那一行与「拿同一份证据重算一遍」**逐字相同**。
    let same_evidence = crate::backend_policy::DeathEvidence {
        outcome: crate::backend_policy::Outcome::NeverSpawned,
        handshake: crate::backend_policy::Handshake::NeverSpoke,
        reader: crate::backend_policy::ReaderEnd::NotObserved,
        start_failure: Some((reason.clone(), looked_at.clone())),
    };
    let d = crate::backend_policy::verdict(&same_evidence).expect("起不来是一件要上账的事");
    assert_eq!(
        crate::backend_policy::death_kind(&d),
        copy_core::copy_static!("rsBackendPolicy.death.neverStarted"),
        "同一份证据判出来的不是「从来没起来」—— 那接线喂进去的三维就不是这一份"
    );
    assert_eq!(
        after.last.as_deref(),
        Some(crate::backend_policy::ledger_line(&d).as_str()),
        "账上那一行与「拿同一份证据重算一遍」不是同一个串。\n\
             ⇒ 接线那一处对 `reason` / `looked_at` 动过手（本条是**逐字**比，不是「包含」）。"
    );
    // ④ **非空对照**：起来了不许上账 —— 否则上面那个 +1 只能说明「什么都记」。
    let mid = local_health();
    let back = note_never_started(StartOutcome::Started(std::path::PathBuf::from("/丁")));
    assert!(
        matches!(back, StartOutcome::Started(_)),
        "`Started` 进去出来的不是 `Started`"
    );
    assert_eq!(
        local_health().seen(),
        mid.seen(),
        "起来了也在账上留了一行 —— 那本账在数的不是死亡"
    );
}

/// ★★ `KP3W3` 行为（监护路）：**三只假后端，各落进三个不同的格。**
///
/// 走的是**生产的那个 `on_event` 闭包**（[`backend_supervise_events`]），
/// 不是在测试里另写一份判读 —— 否则量的是判据自己。
///
/// | 假 backend | 它干了什么 | 该落哪一格 |
/// |---|---|---|
/// | ① | 发一帧合法 hello 之后 `exit 3` | 崩了（`crashed` +1，`refused` 不动） |
/// | ② | 一个字节不说就 `exit 2` | 被拒了（`refused` +1，`crashed` 不动） |
/// | ③ | 说过话之后**我们这一侧**读错误 | 读坏了（`misread` +1，`crashed` 不动） |
///
/// ⚠ **诚实边界，别读大**：
/// - 三只都是 `sh`，不是真 backend ⇒ 它们不碰 tmux，也不证明真后端会这么死；
/// - ③ 那一格的「读错误」在真管道上造不出来（`read_capped_line_sync` 的 `Err`
///   要一次真的 IO 错误）⇒ 它喂的是一个**注入的消费者**，报「说过话了 + 读端出错」。
///   ⇒ 这一格量的是**闭包怎么判**，不是「读错误真的会发生」。
///   ①② 两格走的是**真的** `local_stdio_consumer`，那两维是它自己观测出来的。
#[test]
fn three_fake_backends_land_in_three_different_cells() {
    use std::sync::Arc;
    let _guard = crate::inbound_client::local_origin_test_lock();
    let hello = concat!(
        r#"{"kind":"hello","v":1,"build_id":"kp3b-yi","host_arch":"x86_64","#,
        r#""claude_dir":"/dev/null","commands":["ping"]}"#
    );
    // 崩一次就放弃 ⇒ 每只恰好一个 `Exited` 事件，不会重起。
    let once = local_backend::CrashLimits {
        max_crashes: 1,
        window_ms: 60_000,
    };
    let wait_for = |what: &str, pred: &dyn Fn(&crate::backend_policy::Health) -> bool| {
        for _ in guard_core::wall::rounds() {
            let h = local_health();
            if pred(&h) {
                return h;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!(
            "到兜底上限账上没等到「{what}」那一笔 —— 实得 {:?}",
            local_health()
        );
    };

    // ── ① 说过话之后 exit 3 ⇒ 崩了 ────────────────────────────────
    let base = local_health();
    let h1 = local_backend::supervise_with_stdio(
        std::path::PathBuf::from("sh"),
        vec![
            "-c".into(),
            "printf '%s\\n' \"$CCM_KP3B_HELLO\"; exit 3".into(),
        ],
        vec![("CCM_KP3B_HELLO".to_string(), hello.to_string())],
        once,
        Arc::new(|| 0),
        backend_supervise_events(),
        Some(Arc::new(|i, o| {
            local_backend::local_stdio_consumer(&local_backend::StdioRoute::local(), i, o)
        })),
        crate::spawn_managed::local_backend_supervised(),
    );
    let a = wait_for("崩了", &|h| {
        h.crashed > base.crashed || h.refused > base.refused
    });
    h1.stop();
    assert_eq!(
        (a.crashed, a.refused, a.misread),
        (base.crashed + 1, base.refused, base.misread),
        "① 说过话之后 `exit 3` 没落进「崩了」（前 {base:?} / 后 {a:?}）。\n\
             ★ 落进「被拒了」= 那一维被填成了「从来没说过话」，而它明明说过 ——\n\
             那正是 2026-07-09 判别式被架空的形状。"
    );

    // ── ② 一个字节不说就 exit 2 ⇒ 被拒了 ──────────────────────────
    let h2 = local_backend::supervise_with_stdio(
        std::path::PathBuf::from("sh"),
        vec!["-c".into(), "exit 2".into()],
        Vec::new(),
        once,
        Arc::new(|| 0),
        backend_supervise_events(),
        Some(Arc::new(|i, o| {
            local_backend::local_stdio_consumer(&local_backend::StdioRoute::local(), i, o)
        })),
        crate::spawn_managed::local_backend_supervised(),
    );
    let b = wait_for("被拒了", &|h| {
        h.refused > a.refused || h.crashed > a.crashed
    });
    h2.stop();
    assert_eq!(
        (b.crashed, b.refused, b.misread),
        (a.crashed, a.refused + 1, a.misread),
        "② 一个字节都没说就 `exit 2` 没落进「被拒了」（前 {a:?} / 后 {b:?}）。\n\
             ★ 落进「崩了」= monitor 会去重连重发同一个参数，那就是 2026-07-09 的死循环。"
    );

    // ── ③ 说过话之后我们这一侧读错误 ⇒ 读坏了，且**不算它崩了一次** ────
    let broken: local_backend::StdioSink = Arc::new(|_in, _out| local_backend::ConsumerReport {
        exit: local_backend::ConsumerExit::Early,
        witness: local_backend::StreamWitness::Observed {
            handshake: crate::backend_policy::Handshake::Spoke,
            reader: crate::backend_policy::ReaderEnd::Broken(
                "读端出错（注入）：这一维说的是我们这一侧".to_string(),
            ),
        },
    });
    let h3 = local_backend::supervise_with_stdio(
        std::path::PathBuf::from("sh"),
        vec!["-c".into(), "sleep 30".into()],
        Vec::new(),
        once,
        Arc::new(|| 0),
        backend_supervise_events(),
        Some(broken),
        crate::spawn_managed::local_backend_supervised(),
    );
    let c = wait_for("读坏了", &|h| {
        h.misread > b.misread || h.crashed > b.crashed
    });
    h3.stop();
    assert_eq!(
        (c.crashed, c.refused, c.misread),
        (b.crashed, b.refused, b.misread + 1),
        "③ 我们这一侧读错误没落进「读坏了」（前 {b:?} / 后 {c:?}）。\n\
             ★ 被计成一次崩溃 = `B1` 那条「一个错误的诊断」原样复发：\n\
             三次之后整个进程周期不再起来，日志写「崩了 3 次」。"
    );
}

/// 给人看的话进了文案表（`copy_text("key", …)`）⇒ 「这一段代码说了什么」＝
/// 代码本身 ＋ 它取的那几条表项的原文（占位符原样留着）。只看代码的话，「那句话说没说下一步」
/// 这一类判据从此永远读不到那句话 —— 字搬了家，尺子跟着去新家量。
fn with_copy(code: impl AsRef<str>) -> String {
    let code = code.as_ref();
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../../../src/shared/copy/table.json"))
            .expect("文案表读不懂");
    let mut out = code.to_string();
    let needle = "copy_text(";
    let mut rest = code;
    while let Some(at) = rest.find(needle) {
        // `cargo fmt` 会把长调用拆行：`copy_text(` 与 key 之间可以隔着换行与缩进。
        let tail = rest[at + needle.len()..].trim_start();
        rest = tail;
        let Some(tail) = tail.strip_prefix('"') else {
            continue;
        };
        let key = &tail[..tail.find('"').expect("取文口的 key 没收尾")];
        let zh = table["entries"][key]["zh"]
            .as_str()
            .unwrap_or_else(|| panic!("表里没有 {key}"));
        out.push('\n');
        out.push_str(zh);
        rest = tail;
    }
    out
}

// ═══ 常驻后端身份带数据目录 ═══════════════════════════════════════════════
//
// 要求：「常驻后端身份带数据目录（接错了拒并出声）」；「要么注解那几条命令也先核路径，
// 要么常驻后端的身份带上数据目录」：隔离数据目录的 monitor 不许写穿到真 profile。

/// 🔴 I2：hello 的 `host_env` 与这一趟要交的那几格两向比（期望手写）：等 ⇒ 我们的；值不等 · 它多一格 · 它少一格 ⇒ 拒，
/// 那句话点名那一格、两边各是什么；名不在名单里的（`PATH` 之类）不参与比。
#[test]
fn hx2_a_backend_started_for_another_data_dir_is_refused_out_loud() {
    let hello_with = |env: &str| {
        format!(
            "{{\"kind\":\"hello\",\"v\":1,\"build_id\":\"b1\",\"host_arch\":\"x86_64\",\
             \"claude_dir\":\"/h/.claude\"{env}}}"
        )
    };
    let seen =
        hello_with(",\"host_env\":{\"CCM_DATA_DIR\":\"/iso/a\",\"CCM_RELAY_PORT\":\"8788\"}");
    let want = |home: &str| -> Vec<(String, String)> {
        vec![
            ("CCM_RELAY_PORT".into(), "8788".into()),
            ("CCM_DATA_DIR".into(), home.into()),
            ("PATH".into(), "/shim:/usr/bin".into()),
        ]
    };
    assert_eq!(
        hello_verdict(&seen, Some("b1"), &want("/iso/a")),
        HelloVerdict::Ours,
        "同一份环境（外加一格不在名单里的 PATH）⇒ 该是我们的"
    );
    match hello_verdict(&seen, Some("b1"), &want("/iso/b")) {
        HelloVerdict::Stranger(w) => {
            assert!(
                w.contains("CCM_DATA_DIR")
                    && w.contains("/iso/a")
                    && w.contains("/iso/b")
                    && !w.contains("CCM_RELAY_PORT："),
                "{w}"
            );
        }
        v => panic!("另一个家起的后端被当成了我们的：{v:?}"),
    }
    // C-L5：值是汉字 ⇒ 与前面的汉字之间不隔空格
    assert!(
        matches!(hello_verdict(&seen, Some("b1"), &want("/iso/a")[..1]), HelloVerdict::Stranger(w) if w.contains(&copy_core::copy_text(
            "rsLocalBackendHost.dataDir.slot",
            &[("name", "CCM_DATA_DIR"), ("theirs", "/iso/a"), ("ours", copy_core::copy_static!("hostKey.compare.none"))]
        ))),
        "它多一格（它住隔离的家，这一趟住默认的家）⇒ 该拒"
    );
    assert!(
        matches!(
            hello_verdict(&hello_with(""), Some("b1"), &want("/x")),
            HelloVerdict::Stranger(_)
        ),
        "hello 里没有 host_env（没被交任何一格）而这一趟要交 ⇒ 该拒"
    );
    assert_eq!(
        hello_verdict(&hello_with(""), Some("b1"), &[]),
        HelloVerdict::Ours,
        "两边都空 ⇒ 我们的"
    );
}

/// 🔴 I3：monitor 交的那几格的名字 == 后端回显的名单（两向；读后端源码现抠，异源）；[`backend_env_from`] 交的名 ⊆ 名单
/// （生产那一份不许多交一格回显不了的）。
#[test]
fn hx2_the_handed_names_are_exactly_what_the_backend_echoes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |rel: &str| {
        guard_core::production_code(
            &std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("读 {rel}：{e}")),
        )
    };
    let const_value = |text: &str, decl: &str| -> String {
        let at = guard_core::find_pinned(text, decl)
            .unwrap_or_else(|e| panic!("`{decl}` 不是恰好一处：{e}"));
        let after_decl = &text[at + decl.len()..];
        after_decl[..after_decl.find('"').expect("常量没收尾")].to_string()
    };
    let wire = read("backend/wire/mod.rs");
    let decl = "pub const HOST_ECHO_ENVS: [&str; 2] = [";
    let at = guard_core::find_pinned(&wire, decl).expect("后端回显名单不在了");
    // 名单写在一行里（两格，`cargo fmt` 不拆）：取那一行到收尾。
    let line = wire[at + decl.len()..].lines().next().unwrap_or_default();
    let end = guard_core::find_pinned(line, "];").expect("名单没收尾（或不在一行里）");
    let echo_list: Vec<&str> = line[..end]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut backend: Vec<String> = Vec::new();
    for (needle, file, decl) in [
        (
            "child_env::RELAY_PORT",
            "backend/platform/child_env.rs",
            "pub const RELAY_PORT: &str = \"",
        ),
        (
            "relay_route_core::DATA_DIR_ENV",
            "common/relay-route-core/src/lib.rs",
            "pub const DATA_DIR_ENV: &str = \"",
        ),
    ] {
        assert!(
            echo_list.contains(&needle),
            "后端回显名单里没有 `{needle}`：{echo_list:?}"
        );
        backend.push(const_value(&read(file), decl));
    }
    assert_eq!(
        echo_list.len(),
        2,
        "后端回显名单不是恰好两格：{echo_list:?}"
    );
    let mut mine: Vec<String> = HANDED_ENVS.iter().map(|s| s.to_string()).collect();
    backend.sort();
    mine.sort();
    assert_eq!(mine, backend, "monitor 交的名 ≠ 后端回显的名");
    let mut handed: Vec<String> = backend_env_from(&|_| Some("/iso".into()))
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    handed.sort();
    assert_eq!(handed, mine, "生产交的那份环境不是恰好名单那几格");
}

/// 〔RK1 小尾巴〕`~/.cc-monitor` 这一层**建的那一下**就是 0700；**已在的**不动（两向）。
/// 要求：「RK1 小尾巴（`~/.cc-monitor` 首建权限按 umask ⇒ 0700）」。
#[test]
#[cfg(unix)]
fn hx1_the_monitor_home_dir_is_born_private_and_an_existing_one_is_left_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let mode =
        |p: &std::path::Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    let base = std::env::temp_dir().join(format!("ccm-hx1-home-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    std::fs::create_dir_all(&base).expect("base");
    let fresh = base.join("new").join(".cc-monitor");
    crate::platform::fs::ensure_private_dir(&fresh).expect("建");
    assert_eq!(mode(&fresh), 0o700);
    let old = base.join("old");
    std::fs::create_dir_all(&old).expect("预置");
    std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    crate::platform::fs::ensure_private_dir(&old).expect("已在");
    assert_eq!(mode(&old), 0o755, "已在的那一层被改了权限");
    std::fs::remove_dir_all(&base).ok();
}

/// **monitor 生产段每一处建目录都登记在案，建后端自家目录（`~/.cc-monitor` 一族）的只有
/// `platform::fs::ensure_private_dir` 一处**（本机起后端前 · token / pid 那一层 · 释放二进制 / ccm 入口那几处经注入）。
/// 守的要求：「建自家目录收成一个小函数 …… 判据：生产段建 `~/.cc-monitor` 的调用点 == 那个函数一处（两向，带正控）」。
/// 形状：`src/frontend/shell/src` 生产段里 `fs::create_dir(` / `fs::create_dir_all(` / `fs::DirBuilder::new(` / 调 `platform_core::dir::private_dir*` 的所在 (文件, 函数) == 登记表（两向）；
/// 登记表里「后端自家目录」那一格恰好是那一个函数；正控：合成语料里多一处必被认出。后端那一半另有一份（`own_dir_tests`）。
#[test]
fn hx1_every_monitor_dir_creation_is_registered_and_only_one_builds_the_backend_home() {
    const OWN_HOME: &str = "后端自家目录";
    const DIR_CREATORS: &[(&str, &str, &str)] = &[
        ("platform/fs.rs", "ensure_private_dir", OWN_HOME),
        // 数据目录底下那几层（日志 · 握手表 · 数据目录本身）经 `platform::own_fs` 建，它落到上面那一个函数。
        (
            // 原 `utils.rs`：原子写搬进 `host-core`（两个前端共用那一份）。
            "host-core/atomic.rs",
            "atomic_write_json",
            "调用方给的 JSON 文件的父目录（monitor 数据目录一族）",
        ),
        // `config.rs::patch_config_at` 那一行摘了：config.json 的目录改走 `platform::fs::ensure_private_dir`（0700）。
        // `session_map.rs` 那条 watcher 线程那一行摘了：monitor 自己那份本机判活（连同它盯的 sessions 目录）删了。
    ];
    fn creations(rel: &str, prod: &str) -> Vec<(String, String)> {
        // 建法只住 `platform-core`（后端同一份）：调它那一下也是一处建目录。
        let needles = [
            "fs::create_dir(",
            "fs::create_dir_all(",
            "fs::DirBuilder::new(",
            "platform_core::dir::private_dir",
        ];
        let mut out = Vec::new();
        let mut cur = String::new();
        for line in prod.lines() {
            let head_of_line = line.trim_start();
            for kw in [
                "pub(crate) async fn ",
                "pub async fn ",
                "pub(crate) fn ",
                "pub fn ",
                "async fn ",
                "fn ",
            ] {
                if let Some(rest) = head_of_line.strip_prefix(kw) {
                    cur = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    break;
                }
            }
            if needles.iter().any(|n| line.contains(n)) {
                out.push((rel.to_string(), cur.clone()));
            }
        }
        out
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"]);
    assert!(files.len() >= 100, "只扫到 {} 份 —— 遍历坏了", files.len());
    let mut found: Vec<(String, String)> = Vec::new();
    for (path, src) in &files {
        // 按模块住址认：本包 manifest 明写的兄弟源码树（`host-core` · 文件窗口 …）里的键带包名（`host-core/atomic.rs`）。
        let rel = guard_core::module_address(&root, path);
        found.extend(creations(&rel, &guard_core::production_code(src)));
    }
    found.sort();
    found.dedup();
    let mut want: Vec<(String, String)> = DIR_CREATORS
        .iter()
        .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
        .collect();
    want.sort();
    assert_eq!(found, want, "monitor 生产段建目录的地方与登记表对不上 —— 建的若是 `~/.cc-monitor` 一族，改走 `platform::fs::ensure_private_dir`");
    let home: Vec<&str> = DIR_CREATORS
        .iter()
        .filter(|(_, _, w)| *w == OWN_HOME)
        .map(|(f, _, _)| *f)
        .collect();
    assert_eq!(home, vec!["platform/fs.rs"]);
    // 正控。
    assert_eq!(
        creations(
            "x.rs",
            "fn sneaky() {\n    std::fs::create_dir_all(p).ok();\n}\n"
        ),
        vec![("x.rs".to_string(), "sneaky".to_string())]
    );
}

/// 推给界面的出错事件不许是日志行：本机后端说过话之后以退出码 1 退出（崩溃那一格），
/// 界面收到的每一条都要带文案键、那句话 = 照键与参数从文案表取出来的那一句，句子里没有日志行的形状
/// （模块路径 `a::b` · `key=value` · 方括号账名）。原话只许待在复制详情里。
#[cfg(unix)]
#[test]
fn a_local_backend_crash_reaches_the_ui_as_a_copy_keyed_sentence_not_a_log_line() {
    use std::os::unix::process::ExitStatusExt;
    // 这一笔记在本机那台的死亡账上（进程里只有一份）：同别的读写那本账的几条一样拿那把锁，免得并行时多出一笔记到别人头上。
    let _guard = crate::inbound_client::local_origin_test_lock();
    let got = crate::ui_error::tests::during(|| {
        note_detached_death(
            std::process::ExitStatus::from_raw(1 << 8),
            crate::backend_policy::Handshake::Spoke,
            crate::backend_policy::ReaderEnd::CleanEof,
        );
    });
    assert_eq!(got.len(), 1, "本机后端崩了，界面该恰好收到一条：{got:?}");
    crate::ui_error::tests::assert_is_copy_not_log(&got[0]);
}

/// 叫停不是死亡：被监护的本机后端说过话之后由 `stop()` 结束（设置里停 / 重启 / 更新本机都走这一下），
/// 账上不许多一笔「崩溃」。Windows 上 `kill()` 是 `TerminateProcess(…, 1)`，记成死亡的话就是一笔「崩溃 · 退出码 1」
/// 弹到界面上；这里在 Linux 上是信号 9，同一条路。
#[cfg(unix)]
#[test]
fn stopping_the_supervised_local_backend_is_not_recorded_as_a_crash() {
    use std::sync::Arc;
    let _guard = crate::inbound_client::local_origin_test_lock();
    let hello = concat!(
        r#"{"kind":"hello","v":1,"build_id":"stop-not-death","host_arch":"x86_64","#,
        r#""claude_dir":"/dev/null","commands":["ping"]}"#
    );
    let base = local_health();
    // 生产那份事件处理照用；包一层只为知道「那一命结束」那一件已经处理完了（之后账上该有的都有了）。
    let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let events: Arc<dyn Fn(local_backend::SuperviseEvent) + Send + Sync> = {
        let prod = backend_supervise_events();
        let exited = Arc::clone(&exited);
        Arc::new(move |e| {
            let done = matches!(e, local_backend::SuperviseEvent::Exited { .. });
            prod(e);
            if done {
                exited.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        })
    };
    let h = local_backend::supervise_with_stdio(
        std::path::PathBuf::from("sh"),
        vec![
            "-c".into(),
            "printf '%s\\n' \"$CCM_STOP_HELLO\"; exec sleep 30".into(),
        ],
        vec![("CCM_STOP_HELLO".to_string(), hello.to_string())],
        local_backend::CrashLimits::default(),
        Arc::new(|| 0),
        events,
        Some(Arc::new(|i, o| {
            local_backend::local_stdio_consumer(&local_backend::StdioRoute::local(), i, o)
        })),
        crate::spawn_managed::local_backend_supervised(),
    );
    for _ in guard_core::wall::rounds() {
        if h.current_pid().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(h.current_pid().is_some(), "到兜底上限没起来");
    // 等 hello 被读到（说过话那一维：读到 hello 才登记本机入方向通道）再叫停。
    let t0 = std::time::Instant::now();
    while crate::inbound_client::local_client().is_none() {
        assert!(
            t0.elapsed() < guard_core::wall::CEILING,
            "到兜底上限还没读到 hello"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.stop();
    // 等「那一命结束」那一件被处理完（不是睡一会儿再看账）。
    while !exited.load(std::sync::atomic::Ordering::SeqCst) {
        assert!(
            t0.elapsed() < guard_core::wall::CEILING,
            "到兜底上限还没处理完那一命结束"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let after = local_health();
    assert_eq!(
        (after.crashed, after.refused, after.misread),
        (base.crashed, base.refused, base.misread),
        "叫停被记成了一次死亡（前 {base:?} / 后 {after:?}）"
    );
}

/// 被监护那一支（Windows · 关掉脱离时）起完到握手之间本机那一行是「连接中」：替身起来了、没说话 ⇒ 起那一趟还没回来、读作连接中；
/// 放它说 hello ⇒ 起那一趟回来、读作连着。起完不等握手就回来 ⇒ 第一句断言红（中间那一段读作没连上）。
#[cfg(unix)]
#[test]
fn the_supervised_start_reads_as_starting_until_the_hello() {
    use std::sync::Arc;
    let kind = |l: crate::ui_contract::LocalBackend| {
        serde_json::to_value(l).unwrap()["state"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    let _guard = crate::inbound_client::local_origin_test_lock();
    super::note_local_foreign(None);
    let dir = std::env::temp_dir().join(format!("ccm-sup-hello-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建不了夹具目录");
    let (up, gate) = (dir.join("up"), dir.join("gate"));
    let hello = concat!(
        r#"{"kind":"hello","v":1,"build_id":"sup-hello","host_arch":"x86_64","#,
        r#""claude_dir":"/dev/null","commands":["ping"]}"#
    );
    let envs = vec![
        ("CCM_SUP_HELLO".to_string(), hello.to_string()),
        ("CCM_SUP_UP".to_string(), up.display().to_string()),
        ("CCM_SUP_GATE".to_string(), gate.display().to_string()),
    ];
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _starting = super::Starting::on();
        let h = super::start_supervised(backend_supervise_events(), |events| {
            local_backend::supervise_with_stdio(
                std::path::PathBuf::from("sh"),
                vec![
                    "-c".into(),
                    concat!(
                        ": > \"$CCM_SUP_UP\"; ",
                        "while [ ! -e \"$CCM_SUP_GATE\" ]; do sleep 0.02; done; ",
                        "printf '%s\\n' \"$CCM_SUP_HELLO\"; exec sleep 30"
                    )
                    .into(),
                ],
                envs,
                local_backend::CrashLimits::default(),
                Arc::new(|| 0),
                events,
                Some(Arc::new(|i, o| {
                    local_backend::local_stdio_consumer(&local_backend::StdioRoute::local(), i, o)
                })),
                crate::spawn_managed::local_backend_supervised(),
            )
        });
        let _ = tx.send(h);
    });
    let t0 = std::time::Instant::now();
    while !up.exists() {
        assert!(
            t0.elapsed() < guard_core::wall::CEILING,
            "到兜底上限替身还没起来"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let early = rx.try_recv().ok();
    let returned_early = early.is_some();
    let between = kind(super::local_backend_now());
    std::fs::write(&gate, "").expect("放行不了替身");
    let h = early.or_else(|| rx.recv_timeout(guard_core::wall::CEILING).ok());
    let after = kind(super::local_backend_now());
    if let Some(h) = &h {
        h.stop();
    }
    while crate::inbound_client::local_client().is_some() {
        assert!(
            t0.elapsed() < guard_core::wall::CEILING,
            "到兜底上限通道还没摘"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        (between.as_str(), returned_early),
        ("starting", false),
        "被监护那一支起完、还没握手：起那一趟已经回来了 ⇒ 本机后端读作没起来"
    );
    assert!(h.is_some(), "说上话之后起那一趟没回来");
    assert_eq!(after, "up", "说上话之后本机后端不是连着");
}

/// 刚起的后端晚 3 秒才听上口（冷启动 · 机器忙）：照样等到、接上；假钟每一拍走 20ms，不真等。
#[test]
fn a_backend_that_starts_listening_three_seconds_late_is_still_attached() {
    let t0 = std::time::Instant::now();
    let clock = std::cell::Cell::new(t0);
    let up_at = t0 + std::time::Duration::from_secs(3);
    let got = super::wait_listening(
        std::path::Path::new("/run/x/backend.sock"),
        || Ok((clock.get() >= up_at).then_some("ours")),
        || None,
        Some(super::AFTER_SPAWN_LISTEN_WITHIN),
        || clock.get(),
        || clock.set(clock.get() + std::time::Duration::from_millis(20)),
    );
    assert_eq!(got.map_err(|e| e.said), Ok(Some("ours")));
    assert!(clock.get() >= up_at, "没等到它听上就回了");
}

/// 真起不来时照实报：进程先退了 ⇒ 当场报它怎么退的（不等满期限）；一直活着却不听 ⇒ 到期限报等了多久、等的是哪个口。
#[test]
fn a_backend_that_never_listens_is_reported_for_what_was_awaited() {
    let sock = std::path::Path::new("/run/x/backend.sock");
    let t0 = std::time::Instant::now();
    let tick = std::time::Duration::from_millis(20);

    let clock = std::cell::Cell::new(t0);
    let dies_at = t0 + std::time::Duration::from_millis(100);
    let exited = super::wait_listening::<()>(
        sock,
        || Ok(None),
        || (clock.get() >= dies_at).then(|| "exit status: 3".to_string()),
        Some(super::AFTER_SPAWN_LISTEN_WITHIN),
        || clock.get(),
        || clock.set(clock.get() + tick),
    )
    .expect_err("进程退了还当它听上了");
    assert_eq!(
        exited.said,
        copy_core::copy_text(
            "rsLocalBackendHost.afterSpawn.exited",
            &[("path", "/run/x/backend.sock"), ("how", "exit status: 3")]
        )
    );
    assert!(
        clock.get() < t0 + std::time::Duration::from_secs(1),
        "进程退了还等满期限才报"
    );

    let clock = std::cell::Cell::new(t0);
    let within = std::time::Duration::from_secs(60);
    let silent = super::wait_listening::<()>(
        sock,
        || Ok(None),
        || None,
        Some(within),
        || clock.get(),
        || clock.set(clock.get() + tick),
    )
    .expect_err("一直没人听还当它听上了");
    assert!(clock.get() >= t0 + within, "没到期限就报了");
    assert!(
        silent.said.contains("/run/x/backend.sock")
            && silent.said.contains(&copy_core::format_elapsed(within)),
        "报错没说等的是哪个口、等了多久：{}",
        silent.said
    );
}

/// **用户级 PATH 上写的那个目录（后端按落点算），就是本机真放 `ccm` 下去的那个。**
/// 真落点：`local_backend_host.rs` 起本机后端时的 `extract_dir` ＝ `own_fs::OwnBin::issue()` 那一层；
/// 那一层真跑一次（家喂成临时目录）比对后端落点那个常量的目录，加上源码里 `extract_dir` 恰取自那个句柄。
/// 有人改成另拼一个路径，它会**红**而不是静默放过：「PATH 指向一个空目录」与「根本没补 PATH」在用户终端上是同一个结果。
#[test]
fn the_path_line_points_at_the_directory_we_really_install_ccm_into() {
    let (dir, _) = relay_route_core::BACKEND_LANDING_REL
        .rsplit_once('/')
        .expect("后端落点的目录");
    let home = std::env::temp_dir().join(format!("ccm-path-line-{}", std::process::id()));
    let bin = crate::platform::own_fs::OwnBin::issue_with(&|k| {
        matches!(k, "HOME" | "USERPROFILE").then(|| home.clone().into_os_string())
    })
    .expect("家是绝对路径");
    let want = dir.split('/').fold(home.clone(), |p, seg| p.join(seg));
    assert_eq!(
        bin.dir(),
        want,
        "放 ccm 的那一层不是后端落点那个常量的 `{dir}` —— 「PATH 上写的那个目录」与「盘上真放 ccm 的那个目录」分家了。\n\
             ⚠ 指错目录与根本没补 PATH，在用户终端上是**同一个结果**（命令找不到）。"
    );
    // ⚠ 先把空白抹掉再比：rustfmt 会把链式调用断成几行。
    let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let prod = squeeze(&guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    )));
    for needle in [
        "letSome(bin)=crate::platform::own_fs::OwnBin::issue()else{",
        "letextract_dir=bin.dir().to_path_buf();",
    ] {
        assert_eq!(
            prod.matches(needle).count(),
            1,
            "`local_backend_host.rs` 起本机后端时的落点不再恰取自 `OwnBin::issue()`（`{needle}`）—— 真落点另拼了一份"
        );
    }
}

/// 凭据文件的位置只跟着家走：起本机后端时交的那份环境里没有任何一格另指它的位置（后端按家推，文件名照声明表 `relay_route_core::APIKEY_CREDENTIALS_FILE`）。
#[test]
fn nobody_hands_the_backend_a_path_for_the_credentials_file() {
    let envs = super::backend_env_from(&|_| None);
    let names: Vec<&str> = envs.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        names,
        vec!["CCM_RELAY_PORT"],
        "交给本机后端的环境多了一格：{envs:?}"
    );
    // 正控：设了家那一格，它出现在交出去的环境里。
    let iso = super::backend_env_from(&|k| {
        (k == crate::config::DATA_DIR_ENV).then(|| "/iso/home".to_string())
    });
    assert!(iso
        .iter()
        .any(|(k, v)| k == crate::config::DATA_DIR_ENV && v == "/iso/home"));
}

/// 没起来那几样的字：名字旁那个词 · 问题行那一句照文案表；被拒的下一步只在没连着时带。
#[test]
fn the_local_backend_words_come_from_the_words_table() {
    use crate::ui_contract::LocalBackend as L;
    let _g = crate::inbound_client::local_origin_test_lock();
    note_local_foreign(None);
    note_local_refused(Some("结束口上那个进程再重开（结构夹具）"));
    let got = local_backend(false, Some("p8c-mine"));
    note_local_refused(None);
    assert_eq!(
        got,
        L::Down {
            dot: cmd_wire::machine::MachineDot::Failed,
            tone: cmd_wire::cells::Tone::Fail,
            word: copy_text("machinePage.state.down", &[]),
            said: copy_text("machineState.down.local", &[]),
            fixes: vec![cmd_wire::machine::FixButton {
                fix: cmd_wire::machine::MachineFix::Retry,
                label: copy_text("front.act.retry", &[]),
            }],
            next: Some("结束口上那个进程再重开（结构夹具）".to_string()),
        }
    );
}

/// 没起来那几样各带点画哪一档（机器点同一套五档）与问题行的语气：壳写好，界面不按 `state` 另判。
#[test]
fn the_local_backend_says_its_dot_and_tone() {
    use crate::ui_contract::LocalBackend as L;
    use cmd_wire::cells::Tone;
    use cmd_wire::machine::MachineDot as D;
    let _g = crate::inbound_client::local_origin_test_lock();
    let face = |l: &L| match l {
        L::Up => None,
        L::Starting { dot, tone, .. }
        | L::Down { dot, tone, .. }
        | L::Older { dot, tone, .. }
        | L::Newer { dot, tone, .. } => Some((*dot, *tone)),
    };
    note_local_foreign(None);
    note_local_starting(true);
    let starting = local_backend(false, Some("p8c-mine"));
    note_local_starting(false);
    assert_eq!(face(&starting), Some((D::Pending, Tone::Busy)));
    assert_eq!(
        face(&local_backend(false, Some("p8c-mine"))),
        Some((D::Failed, Tone::Fail))
    );
    note_local_foreign(foreign_in("p7a-old", true));
    assert_eq!(
        face(&local_backend(false, Some("p8c-mine"))),
        Some((D::Warn, Tone::Warn))
    );
    // 口上占着更新的构建：没连着、出错 ⇒ 画成没连着（比这一版新而连着的那台是绿点，与出错那一句对不上）。
    note_local_foreign(foreign_in("p9a-new", true));
    let newer = local_backend(false, Some("p8c-mine"));
    assert!(matches!(newer, L::Newer { .. }), "{newer:?}");
    assert_eq!(face(&newer), Some((D::Failed, Tone::Fail)));
    note_local_foreign(None);
    assert_eq!(face(&local_backend(true, Some("p8c-mine"))), None);
}

/// 起本机后端被拒（口上有东西、接不上）：下一步那一句跟着走，直到再起一次把它清掉；接上了不说。
#[test]
fn a_refused_local_start_carries_the_next_step() {
    use crate::ui_contract::LocalBackend as L;
    let _g = crate::inbound_client::local_origin_test_lock();
    let next = |l: L| match l {
        L::Down { next, .. } | L::Older { next, .. } | L::Newer { next, .. } => next,
        L::Up | L::Starting { .. } => None,
    };
    note_local_foreign(None);
    note_local_refused(Some("停掉那个进程再重开"));
    assert_eq!(
        next(local_backend(false, Some("p8c-mine"))).as_deref(),
        Some("停掉那个进程再重开")
    );
    note_local_foreign(foreign_in("p7a-old", false));
    assert_eq!(
        next(local_backend(false, Some("p8c-mine"))).as_deref(),
        Some("停掉那个进程再重开"),
        "口上是另一个家里的旧版那一形也带着（自家的上一版给［更新］、不带这一句：`an_older_backend_in_our_home_offers_update_not_retry`）"
    );
    note_local_foreign(None);
    assert_eq!(local_backend(true, Some("p8c-mine")), L::Up, "接上了就不说");
    note_local_refused(None);
    assert_eq!(next(local_backend(false, Some("p8c-mine"))), None);
}

/// 壳里不造机器状态成品（各台的成品在后端 `deploy::machine` 定）。
/// 正控：同一个认法在那张表的家里认得出那一处构造。
#[test]
fn the_shell_builds_no_machine_state_of_its_own() {
    let builds = |src: &str| {
        guard_core::production_code(src).lines().any(|l| {
            let l = l.trim();
            l.contains("MachineState {") && !l.contains("-> MachineState {")
        })
    };
    let home = guard_core::addr::repo_root().join("src/backend/deploy/machine.rs");
    assert!(
        builds(&std::fs::read_to_string(&home).expect("读那张表的家")),
        "认法在那张表的家里认不出构造 —— 认法坏了"
    );
    let files: Vec<(std::path::PathBuf, String)> = guard_core::addr::shell_population_roots()
        .iter()
        .flat_map(|root| guard_core::scan_tree!(root, &["rs"]))
        .collect();
    assert!(
        files.len() > 50,
        "壳的人群只扫到 {} 份 —— 走法坏了",
        files.len()
    );
    let shell: Vec<String> = files
        .iter()
        .filter(|(_, src)| builds(src))
        .map(|(p, _)| guard_core::addr::rel_of(p))
        .collect();
    assert_eq!(shell, Vec::<String>::new(), "壳里另有一处在造机器状态成品");
}

/// 起来时起本机后端不挡调用方：替身那一趟卡着（等放行），调用方已经回来了；放行之后结局照样交到 `then`。
/// 同步起 ⇒ 替身跑在调用方的线程上，不等放行当场回来，调用方回来时它已经起完 ⇒ 红（不靠墙钟）。
#[test]
fn the_launch_start_returns_before_the_backend_is_up() {
    let caller = std::thread::current().id();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<bool>();
    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let f = finished.clone();
    super::start_off_thread(
        move || {
            let off_caller = std::thread::current().id() != caller;
            if off_caller {
                let _ = release_rx.recv_timeout(guard_core::wall::CEILING);
            }
            f.store(true, std::sync::atomic::Ordering::SeqCst);
            if off_caller {
                StartOutcome::AlreadyRunning
            } else {
                StartOutcome::Started(std::path::PathBuf::from("on-caller"))
            }
        },
        move |out| {
            let _ = done_tx.send(matches!(out, StartOutcome::AlreadyRunning));
        },
    );
    assert!(
        !finished.load(std::sync::atomic::Ordering::SeqCst),
        "起本机后端那一趟还没放行，调用方就等它起完了"
    );
    release_tx.send(()).expect("替身那一趟不在了");
    assert_eq!(
        done_rx.recv_timeout(guard_core::wall::CEILING),
        Ok(true),
        "结局没交到 then"
    );
}

/// 本机后端起不来（没带后端 · 释放失败 · 起了接不上 · 被拒）⇒ 本机那一行（`LocalBackend::Down`）说的是起不来那一句，
/// 不是泛泛的「本机后端未运行」；那一句在「从来没起来」的唯一喂点（`note_never_started`）记下。
#[test]
fn a_failed_local_start_says_why_on_the_local_row() {
    use crate::ui_contract::LocalBackend as L;
    let _g = crate::inbound_client::local_origin_test_lock();
    super::note_local_foreign(None);
    super::note_local_failed(Some("没带本机后端 · 测试那一句"));
    let failed = super::local_backend(false, Some("p8c-mine"));
    super::note_local_failed(None);
    let plain = super::local_backend(false, Some("p8c-mine"));
    assert!(
        matches!(&failed, L::Down { said, .. } if said == "没带本机后端 · 测试那一句"),
        "{failed:?}"
    );
    assert!(
        matches!(&plain, L::Down { said, .. } if *said == copy_text("machineState.down.local", &[])),
        "{plain:?}"
    );
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let feed = body_of(&me, "fn note_never_started(");
    assert!(
        feed.contains("note_local_failed("),
        "起不来那一句没在 `note_never_started` 记下 ⇒ 本机那一行只说「未运行」，用户不知道为什么"
    );
}

/// 口上那一份别的构建（判据造它的那一处）。
fn foreign_in(build: &str, same_home: bool) -> Option<Foreign> {
    Some(Foreign {
        build: build.into(),
        same_home,
    })
}

/// 口上是**同一个家里**的上一版（家与中转口那几格对得上；本机后端脱离起、退出不停 ⇒ 每次升级 app 都撞上它）⇒ 修法是［更新］，
/// 不再写「结束进程 … 再重试」那一句；另一个家里的旧版 · 比这一版新的照旧只有［重试］＋ 下一步那一句（不拿旧 app 顶掉新后端）。
#[test]
fn an_older_backend_in_our_home_offers_update_not_retry() {
    use crate::ui_contract::LocalBackend as L;
    use cmd_wire::machine::MachineFix as F;
    let _g = crate::inbound_client::local_origin_test_lock();
    let at = |build: &str, same_home: bool| {
        note_local_refused(Some("结束进程 1 再重试"));
        note_local_foreign(foreign_in(build, same_home));
        let got = local_backend(false, Some("p8c-mine"));
        note_local_foreign(None);
        note_local_refused(None);
        got
    };
    let fixes_next = |l: &L| match l {
        L::Down { fixes, next, .. }
        | L::Older { fixes, next, .. }
        | L::Newer { fixes, next, .. } => (
            fixes
                .iter()
                .map(|f| (f.fix, f.label.clone()))
                .collect::<Vec<_>>(),
            next.clone(),
        ),
        L::Up | L::Starting { .. } => panic!("{l:?}"),
    };
    let ours = at("p7a-old", true);
    assert!(matches!(ours, L::Older { .. }), "{ours:?}");
    assert_eq!(
        fixes_next(&ours),
        (vec![(F::Update, copy_text("front.act.update", &[]))], None),
        "同一个家里的上一版：只给［更新］，不叫人去结束进程"
    );
    let other = at("p7a-old", false);
    assert_eq!(
        fixes_next(&other),
        (
            vec![(F::Retry, copy_text("front.act.retry", &[]))],
            Some("结束进程 1 再重试".into())
        ),
        "另一个家里的旧版不是这个 app 的上一版：不替它换"
    );
    let newer = at("p9a-new", true);
    assert!(matches!(newer, L::Newer { .. }), "{newer:?}");
    assert_eq!(
        fixes_next(&newer).0,
        vec![(F::Retry, copy_text("front.act.retry", &[]))]
    );
}

/// 探口读到的 hello ⇒ 记下的那一份别的构建：构建与这一版不同才记；家与中转口那几格对不对得上一起记（[`foreign_of`]）。
#[test]
fn the_foreign_build_remembers_whether_it_lives_in_our_home() {
    let hello = |build: &str, port: &str| {
        serde_json::json!({
            "kind": "hello", "v": 1, "build_id": build, "host_arch": "x86_64",
            "claude_dir": "/h/.claude", "capabilities": [], "emits": [], "commands": [],
            "host_env": { "CCM_RELAY_PORT": port },
        })
        .to_string()
    };
    let env = vec![("CCM_RELAY_PORT".to_string(), "7000".to_string())];
    let mine = Some("p8c-mine");
    assert_eq!(
        foreign_of(&hello("p7a-old", "7000"), mine, &env),
        foreign_in("p7a-old", true)
    );
    assert_eq!(
        foreign_of(&hello("p7a-old", "7001"), mine, &env),
        foreign_in("p7a-old", false)
    );
    assert_eq!(foreign_of(&hello("p8c-mine", "7001"), mine, &env), None);
    assert_eq!(foreign_of(&hello("p7a-old", "7000"), None, &env), None);
    assert_eq!(foreign_of("HTTP/1.1 200 OK", mine, &env), None);
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let probe = body_of(&me, "fn probe_listen_socket(");
    assert!(
        probe.contains("note_local_foreign(foreign_of(&line, mine, want_env))"),
        "探口那一趟没经 `foreign_of` 记 ⇒ 本机那一行分不出是不是自家的上一版：{probe}"
    );
}

/// 停（也是［更新］与「重启」的前一半）：手上没有句柄、而这个家的口上有人在听（升级后上一版留下的那一份）⇒
/// 照样拿这一版的落点跑一次 `-- --resident-stop`，按它的结局回话；没人在听 ⇒ 照实说没在跑，不起那一趟。
/// 替身落点记下它被怎么调、答一句「收尾停了」；在听的那一位是本测试里的一个套接字（替身不真停它）。
#[cfg(unix)]
#[test]
fn stopping_with_no_handle_still_stops_whoever_listens_in_our_home() {
    use crate::remote_resident::StopWord;
    // 停成之后清本机那一格的记录（进程里一份）⇒ 与读那一格的几条串行。
    let _g = crate::inbound_client::local_origin_test_lock();
    let dir = std::env::temp_dir().join(format!("lbh-stop-foreign-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let sock = dir.join("listen.sock");
    let args = dir.join("args");
    let fake = dir.join("ccm");
    let script = format!(
        "#!/bin/sh\nprintf '%s ' \"$@\" > '{}'\necho '{{\"stopped\":\"graceful\",\"pid\":4242}}'\n",
        args.display()
    );
    std::fs::write(&fake, script).expect("写替身");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let asked = std::sync::atomic::AtomicU32::new(0);
    let bin = || {
        asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(fake.clone())
    };
    let deadline = guard_core::wall::CEILING;
    // ① 没人在听 ⇒ 没在跑，落点都不去要。
    let none = stop_listener_on(&sock, &bin, deadline).expect("没人在听");
    assert_eq!(none.stopped, StopWord::NotRunning);
    assert_eq!(
        asked.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "没人在听也去要落点、起那一趟"
    );
    // ② 口上有人在听（我们没有它的句柄）⇒ 跑一次这一版的 `--resident-stop`，照它的结局说。
    let listener = std::os::unix::net::UnixListener::bind(&sock).expect("占口");
    let got = stop_listener_on(&sock, &bin, deadline).expect("停那一趟");
    drop(listener);
    let argv = std::fs::read_to_string(&args).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        (got.stopped, got.pid),
        (StopWord::Graceful, Some(4242)),
        "口上占着上一版时「停」谎报了「没在跑」"
    );
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        argv.trim(),
        format!("{} --resident-stop", local_backend::BACKEND_SEP)
    );
    // 接线：停口两样句柄都没有时走这一段；停成之后清掉口上那一份的记录。
    let me = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend_host.rs"
    ));
    let stop = body_of(&me, "pub fn stop_local_backend(");
    assert!(
        stop.contains("stop_listener_on("),
        "`stop_local_backend` 两样句柄都没有时不去问口上 ⇒ 又回「没在跑」：{stop}"
    );
    let on = body_of(&me, "fn stop_listener_on(");
    for needle in ["run_resident_stop_within(", "note_local_foreign(None)"] {
        assert!(
            on.contains(needle),
            "`stop_listener_on` 里没有 `{needle}`：{on}"
        );
    }
}
