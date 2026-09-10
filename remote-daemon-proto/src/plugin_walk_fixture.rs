//! `K-W2E`（验收口径住计划仓 `features/K-W2E-最小假插件走通全流程.md`；
//! 功能正文住 `plugin-split` 的 `EF06`）：**最小假插件走通全流程**。
//!
//! # 先说清本文件是什么、不是什么
//!
//! 它是**夹具 + 判据**，一行生产语义都不加。`cfg(test)` 挂在**文件级**（内层属性形，带 `!`）
//! ⇒ 生产二进制里**零字节**（由 [`tests::this_fixture_never_ships`] 双向钉住）。
//! ⚠ 那个属性的**逐字形状不在本注释里出现第二次** —— 那条判据数它出现几次，
//! 散文里复述一份会把它数成 2（本仓「判据在自己的语料里找到自己」那一族）。
//!
//! ★ **它与 `agents/fake/` 是两条轴上的两件东西，别混起来读**：那一份是**最小假 agent**
//! （`daemon-split` 的 `S6`，走的 7 段是「发现 / 宣告 / 读会话 / 判活 / 账号 / 用量 / resume」，
//! 全程纯函数、一个进程都不起）；本份是**最小假插件**（找它 / 问它会什么 / 传 argv 起它 /
//! 拿码 / 翻语义），而且**必须真起一个进程** —— 那一条与 `S6` 恰好相反，理由见下面「跳⑥」。
//!
//! # 为什么落在这里，而不是 `plugin/fake/mod.rs`（甲档）—— 三条，都是现打的
//!
//! 1. 🔴 **甲档要动的两张登记表不在本轮写区里。** `plugin/mod.rs::layer_guard` 那条
//!    `the_plugin_layer_collection_is_complete` 的诊断文案**逐字要求**：往那一层加文件要
//!    「**同轮**加进 `agent_boundary_guard::CORE_FILES` 与它的棘轮表」——
//!    而 `agent_boundary_guard.rs` 不在本轮写区（写区只有 `plugin/` · `layering_guard.rs` ·
//!    `evidence/`）。⇒ 甲档要么违纪，要么上报之后停一轮。
//! 2. 🔴 **甲档会被通用层自己的判据当场打红，而那不是误伤、是它干对了活。**
//!    假插件必须带**它自己那一份码表**（`E6`：码 → 语义每插件一份），而那张表的形状
//!    正是 `plugin::layer_guard::the_generic_port_does_not_translate_exit_codes` 认的两根针
//!    （`Some(<整数字面量>)` / 裸整数 `match` 臂），外加
//!    `the_only_exit_code_constants_here_are_the_registered_generic_ones` 认的 `i32` 常量登记面。
//!    ⇒ 把假插件放进 `plugin/`，等于让通用层里长出一张具体插件的码表 ——
//!    **本件要证的那件事，会被本件自己破坏掉。**
//! 3. ✅ **乙档的代价，现打之后发现不存在。** 件文件 `§0b` 给乙档记的代价逐字是
//!    「`layering_guard` 里进 `plugin` 的边要多登记几条，而那张表**条数钉死**（今天 5）」。
//!    现打：那条判据（`layering_guard::the_interface_into_plugin_is_exactly_the_registered_set`）
//!    的**人群只有两层** —— 它逐字 `for layer in ["control", "observe"]`，
//!    而 `layer_sources` 按 `src/<层名>` 拼路径 ⇒ **顶层文件根本不在它的采集面里**；
//!    况且它扫的是 `production_code` 的产物，而本文件的每一条 `plugin` 边都在
//!    `#[cfg(test)]` 里 ⇒ **两道构造性摘除，各自独立地让那张表一条都不用动**。
//!    ⇒ 乙档对那张表的真实代价是 **0 条**，件文件那一行是**估的，不是量的**（本件顶回去）。
//!    ⚠ 而这正好是一条**射程读数**，归 `KW2E5`：那张表自称「进 `plugin/` 的边逐条登记」，
//!    真实射程是「**`control/` 与 `observe/` 两层生产段里**的边」。将来 `sidecars/`
//!    那一层（`K-W2D`）调 `plugin::invoke::run`，这张钉死 5 条的表**一个字都不会说**。
//!
//! # 全流程有几跳、本份夹具真走了哪几跳（尺子甲，件文件 `§0b`）
//!
//! 九跳的切法照件文件（一跳 = 跨一次边界或一次真相源交接，且盘上有一段自己的代码）。
//! 本份夹具驱动 **6 跳**，逐跳标明「**真过了通用层的机器** / **自问自答**」——
//! 形状照 `agents/fake/walk` 头注那条纪律（「走全流程的两半，边界写在明处」）：
//!
//! | 跳 | 本夹具 | 真过通用层吗 | 凭什么这么说 |
//! |---|---|---|---|
//! | ① 宣告接得下 | ❌ 走不到 | — | 要在真 `Hello.commands` 里出现，就得先在 `inbound::REGISTRY` 里有一条命令，而那是 PM 持有的文件（走 `§4` 上报） |
//! | ② 宣告做不到 | ✅ 一半 | **半个**：判定那一半真的从 `inbound::REGISTRY` 派生；**没进真 `Hello`** | `§0d` 出路**丙**。理由与代价逐条写在 [`tests::the_pre_declaration_reuses_the_word_the_registry_already_owns`] |
//! | ③ 收命令分派 | ❌ 走不到 | — | 同 ①（要一条真命令） |
//! | ④ 找它 | ✅ | **真过** | `plugin::discover::find` 真在盘上找一个真的可执行文件（`libexec/` 那一级**故意是空的**，找法必须走过它） |
//! | ⑤ 问它会什么 | ✅ | **真过** | `plugin::probe::negotiate`，而喂给它的是**一个真进程刚打出来的字节** —— 今天那 8 条判据喂的全是合成文本 |
//! | ⑥ 传 argv 起它 | ✅ | **真过** | `plugin::invoke::run`，**本 crate 第一次真起一个插件进程**（今天 `invoke.rs` 6 条 + `cc_bus.rs` 7 条，一条都没起过进程） |
//! | ⑦ 拿码摘诊断 | ✅ | **真过** | 码与两条流来自真进程（今天那两条是手搓 `Done{…}`） |
//! | ⑧ 把码翻成语义 | ✅ | **自问自答，而这一格的自问自答是对的** | `E6` 逐字要求码表**每插件一份** ⇒ 通用层按定框就不该有它。这一跳「没过通用层」是设计，不是缺口 |
//! | ⑨ 回帧给客户端 | ❌ 走不到 | — | 要真 daemon 收发帧（派工令：不起真 daemon） |
//!
//! ⇒ **一句话带分母**：九跳里本夹具真走 **6**（②半 · ④⑤⑥⑦⑧），
//! 其中**真过通用层的是 4 跳半**（④⑤⑥⑦ + ②的判定半）· **自问自答 1 跳**（⑧，按定框应当如此）·
//! **走不到 3 跳**（①③⑨，归真机 / 归上报，见 `KW2E6`）。
//!
//! ★ 而本件真正买到的那样东西**不是跳数**：件文件 `§0c` 现打过「`plugin/` 25 条 +
//! `cc_bus.rs` 7 条里**没有任何一条同时驱动 ≥2 跳**」。本文件的
//! [`tests::one_fake_plugin_walks_six_hops_as_one_path`] 是**第一条把 ②④⑤⑥⑦⑧ 串成一条路**的判据。
//!
//! # 这个假插件的每一样都**刻意与仓里那个真插件族不同形**（`S6` 那条地基）
//!
//! | 维 | 那个真插件族 | 本假插件 |
//! |---|---|---|
//! | 可执行文件的粒度 | **一个动词一个二进制** | **一个二进制 + 子命令** |
//! | 探测入口 | 一个**旗标**（`--…-probe`） | 一个**子命令**（无前导 `-`） |
//! | 候选路径 | 用户装机位（`~/.local/bin` · agent 的技能目录），**兜 `PATH`** | 调用方给的一个根 + `libexec/` 一级，**刻意不兜 `PATH`** |
//! | 契约走 env 的那一样 | **可选**的身份覆盖 | **必需**的作业域；缺了就用一个自己的码拒 |
//! | 退出码语义 | `2` = 收件人非法 · `3` = 路由层拒绝 | `5` = 台账过期该重试 · `6` = 作业域上锁 · `7` = 不认识这条子命令 |
//!
//! ⚠ 「刻意不同形」不是趣味：假的若照抄真的，通用层拿真的的知识去解释它**恰好也能读出东西**
//! ⇒ 走通证明的是「一份布局被施加到另一个名字上」，那是**一个粉饰的通过**
//! （`agents/fake/mod.rs` 头注逐字）。本条由
//! [`tests::this_fake_plugin_is_deliberately_unlike_the_real_one`] 逐维钉住 ——
//! 而且**对拍的是那个真适配层今天的活体**（`control/cc_bus.rs` 的函数与生产段），
//! 不是手抄的死 fixture。
//!
//! ⚠ 同时**刻意避开代码全景那一族的词汇**（`K-W2D` 正在写 `sidecars/codepicture/`，
//! 撞形状会让两件的读数互相污染）：本夹具一个字都不提全景 / 图 / 调用者 / 索引那一族。
//!
//! # 诚实边界（写在这里，因为它们删了不会红）
//!
//! - 跳①③⑨ **本地验不了**，不是「以后再说」：它们要一个真 daemon 收发帧。今天真跑过那条路的
//!   是 `e2e/daemon-cc-bus.sh`（CI 地板 **50**，`.github/workflows/ci.yml:646` 逐字
//!   `run: bash e2e/assert-pass-floor.sh daemon-cc-bus 50`）——⚠ **它不在门禁九格里**
//!   （`grep -c daemon-cc-bus scripts/gate.sh` ⇒ **0**，09-04 现打），而且它测的是
//!   **那个既有插件**，不是「加一个新插件」。
//! - 本文件**不证明**「加一个插件，宿主零改动」。它证的是「**这几样知识凑得出一条真跑得动的路**」，
//!   并给出那句话今天的**差距读数**（`KW2E5`，逐处住址在
//!   [`tests::adding_a_plugin_still_costs_the_host_something`] 的头注里）。
//! - 期限那一跳依赖机器上有没有 `timeout(1)`：**两条路各有一格判据**，
//!   并且**说得出自己走的是哪一条**（[`tests::the_deadline_lands_on_the_child_not_on_the_host`]）。

#![cfg(test)]

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::plugin::invoke::TIMED_OUT_CODE;

    // ═══════════════════════════════════════════════════════════════════════
    // 一 · 这个假插件自己的知识（它的「适配层」—— 按 `E6`，这些东西**只能**住这里）
    // ═══════════════════════════════════════════════════════════════════════

    /// 假插件的名字 = 探测输出身份行的值。**运行时拼**（同 `agents/fake` 那条纪律：
    /// 判据文本不许自己成为语料）。
    fn plugin_name() -> String {
        format!("hed{}", "ron")
    }

    /// 探测入口 —— **一个子命令**（那个真插件族用的是旗标）。
    const PROBE_SUB: &str = "facets";
    /// 干活的子命令。
    const WORK_SUB: &str = "seal";
    /// 会卡住的子命令（只有期限那一格用它）。
    const NAP_SUB: &str = "nap";
    /// **把自己拿到的整份环境的键打回来**的子命令（`K-R26` 加，环境继承那两格用它）。
    ///
    /// # 为什么读 `/proc/self/environ` 而不是跑 `env`
    ///
    /// 要量的是「**通用调用口交给子进程的那一份**」，而 `env` 印的是**壳自己那一刻的**环境
    /// —— 壳（`/bin/sh`）会往里加自己的东西（POSIX 要求它导出 `PWD`），
    /// 于是「子进程看到的键」里会混进**不是继承来的**几个，
    /// 下面那条「子进程的键集 ⊆ 白名单 ∪ 显式喂的」就会红在一个与本题无关的原因上。
    /// `/proc/self/environ` 是内核在 `execve` 那一刻放下的那份拷贝，
    /// 壳之后 `setenv` 一律不写它 ⇒ 它**逐字节**就是 `Command` 交出去的那一份。
    ///
    /// ⚠ 拿不到 `/proc` 的机器上它会印出空的一份 —— 那时**不许静默过**：
    /// 用它的那两格都先断言「键集非空且含 `PATH`」（否则「不含那两个键」是空真）。
    const ROSTER_SUB: &str = "roster";
    /// 契约走 env 的那一样：作业域。**必需**（那边那一样是可选的身份覆盖）。
    const REALM_ENV: &str = "HEDRON_REALM";
    /// 夹具里用的作业域值。
    const REALM_VALUE: &str = "alpha";

    /// 它自己声明的能力 token（探测输出里那一串）。
    const DECLARED_CAPS: &str = "seal,verify,emit-ledger,dry-run";
    /// **宿主侧的必需清单** —— 刻意只要其中两个（子集检查，不是全等）。
    const REQUIRED_CAPS: &[&str] = &["seal", "emit-ledger"];

    /// 找不到时那句话的**尾巴** —— 这是假插件自己的话，通用层不该认识它。
    ///
    /// ⚠ 刻意用中文：它要进断言，而 `not_installed_message` 会把**候选路径原样印进输出** ——
    /// 尾巴若取自路径里出现得了的字符，那条断言就会**靠路径恒真**〔`brief` `6g`〕。
    const NOT_FOUND_HINT: &str = "（夹具口径：只查上面这两处，刻意不兜 PATH）";

    /// 它自己的退出码 —— **语义与那个真插件族互斥**（同一个数在两边不是同一件事）。
    const STALE_LEDGER: i32 = 5;
    const REALM_LOCKED: i32 = 6;
    const UNKNOWN_SUB: i32 = 7;

    /// 给子进程的期限（秒）。宽一点：这个数不进任何断言，只是不许没有。
    const DEADLINE_SECS: u64 = 20;
    /// 期限那一格专用的秒数（要小于假插件卡住的时长）。
    const NAP_DEADLINE_SECS: u64 = 1;

    /// 假插件那个可执行文件的**正文**。
    ///
    /// 逐行拼而不是一整块字面量：一整块里若出现**列 0 的右大括号**，
    /// `guard_core` 的剥法会**在那里收尾** ⇒ 本文件的测试段被剥成两半，
    /// 而剥法自检（`every_daemon_file_strips_clean`）与
    /// `assert_no_test_code` 会当场红。⇒ 全文一个大括号都不用（`case`/`if`/`while` 足够）。
    fn script_text(name: &str) -> String {
        let lines: Vec<String> = vec![
            "#!/bin/sh".to_string(),
            "# 夹具用的假插件 —— 一个可执行文件 + 子命令。刻意与仓里那个真插件族不同形。"
                .to_string(),
            "case \"$1\" in".to_string(),
            format!("  {PROBE_SUB})"),
            format!("    printf 'name={name}\\n'"),
            "    printf 'version=0.2.0-fixture\\n'".to_string(),
            format!("    printf 'capabilities={DECLARED_CAPS}\\n'"),
            "    printf 'realms=alpha,beta\\n'".to_string(),
            "    printf 'parent=%s\\n' \"$(cat /proc/$PPID/comm 2>/dev/null)\"".to_string(),
            "    printf 'inherited-path=%s\\n' \"$PATH\"".to_string(),
            "    exit 0 ;;".to_string(),
            format!("  {WORK_SUB})"),
            format!("    if [ -z \"${REALM_ENV}\" ]; then"),
            "      printf 'no realm was handed over\\n' >&2".to_string(),
            format!("      exit {REALM_LOCKED}"),
            "    fi".to_string(),
            "    if [ \"$2\" = \"--ledger\" ] && [ \"$3\" = \"stale\" ]; then".to_string(),
            "      printf 'the ledger id is stale\\n' >&2".to_string(),
            format!("      exit {STALE_LEDGER}"),
            "    fi".to_string(),
            format!("    printf 'sealed in %s\\n' \"${REALM_ENV}\""),
            "    exit 0 ;;".to_string(),
            format!("  {NAP_SUB})"),
            "    sleep 3".to_string(),
            "    exit 0 ;;".to_string(),
            format!("  {ROSTER_SUB})"),
            // 一行一个 `键=值`；调用方只取 `=` 左边那一半（值里可能有 `=`，右边不许再切）。
            "    tr '\\0' '\\n' < /proc/self/environ".to_string(),
            "    exit 0 ;;".to_string(),
            "  *)".to_string(),
            "    printf 'unknown subcommand\\n' >&2".to_string(),
            format!("    exit {UNKNOWN_SUB}"),
            "    ;;".to_string(),
            "esac".to_string(),
        ];
        lines.join("\n") + "\n"
    }

    /// 知识 ①（跳④）：候选路径 + 找不到时那句尾巴。
    ///
    /// **两级，第一级刻意留空** —— 找法必须真的走过一个不存在的候选才到第二级；
    /// 少了这一点，「顺序即优先级」这一格就是空真。
    /// `search_path` 由 [`walk`] 给 `false`（那个真插件族给的是 `true`，
    /// 由 [`this_fake_plugin_is_deliberately_unlike_the_real_one`] 对拍钉住）。
    fn discovery_of(root: &Path, name: &str) -> (Vec<PathBuf>, &'static str) {
        (
            vec![root.join("libexec").join(name), root.join(name)],
            NOT_FOUND_HINT,
        )
    }

    /// 知识 ②（跳⑥）：探测调用的 argv。
    fn probe_argv() -> Vec<&'static str> {
        vec![PROBE_SUB]
    }

    /// 知识 ③（跳⑦）：真活调用的 argv 与它**必需**的那一项 env。
    fn work_call() -> (Vec<&'static str>, Vec<(&'static str, &'static str)>) {
        (vec![WORK_SUB], vec![(REALM_ENV, REALM_VALUE)])
    }

    /// 知识 ④（跳⑧）：**码 → 语义**。按 `E6` 它只能住这里，不许上收到 `plugin/`。
    ///
    /// ⚠ 本表刻意**两种落地形各写一处**：`Some(0)` 是整数字面量形、
    /// `Some(STALE_LEDGER)` 是具名常量形。前者会被
    /// `plugin::layer_guard::the_generic_port_does_not_translate_exit_codes` 的形状针认出，
    /// 后者会被 `the_only_exit_code_constants_here_are_the_registered_generic_ones` 的登记面认出
    /// ——**两条判据的射程都只到 `plugin/`**，所以本表住在这里一格都不红。
    /// 这就是本文件落在层外（乙档）而不是层内（甲档）的第二条理由。
    fn code_word(code: Option<i32>) -> &'static str {
        match code {
            Some(0) => "sealed",
            Some(STALE_LEDGER) => "stale_ledger",
            Some(REALM_LOCKED) => "realm_locked",
            Some(UNKNOWN_SUB) => "broke",
            Some(TIMED_OUT_CODE) => "deadline_hit",
            Some(_) => "broke",
            None => "signalled",
        }
    }

    /// 那个真插件族对**同一个数**的说法 —— 活体对拍用（不是手抄的表）。
    fn real_plugin_word(code: Option<i32>) -> String {
        match crate::control::cc_bus::classify_send(code, "detail") {
            Ok(()) => "<成事>".to_string(),
            Err((word, _)) => word,
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 二 · 跳② 的判定那一半（`§0d` 出路**丙**）
    // ═══════════════════════════════════════════════════════════════════════

    /// 插件轴的「接得下但做不到」判定 —— 与 `main.rs::unavailable_from` **同形**：
    /// **从 `inbound::REGISTRY` 的 `codes` 派生**，不手写第二张「命令→依赖什么」的表。
    ///
    /// 三态与那一份逐字相同（这一格是唯一容易假绿的地方）：
    /// `Some(false)`（**确证没装**）⇒ 列进表；`Some(true)` ⇒ 不列；
    /// `None`（**判不出来**）⇒ **不列**（「不知道」不许倒向「做不到」——
    /// 倒错那一侧会让能用的功能从界面上无声消失）。
    fn unavailable_from_plugin(
        installed: Option<bool>,
        code: &str,
    ) -> Vec<crate::wire::Unavailable> {
        let mut out = Vec::new();
        if installed == Some(false) {
            for spec in crate::inbound::REGISTRY {
                if spec.codes.contains(&code) {
                    out.push(crate::wire::Unavailable {
                        command: spec.name.to_string(),
                        code: code.to_string(),
                    });
                }
            }
        }
        out
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 三 · 能力表 + 走全流程的 driver（反向夹具**常驻**，不做一次性手工变异）
    // ═══════════════════════════════════════════════════════════════════════

    /// 本夹具真走得到的那几跳，**名字进错误信息**，所以是常量不是字面量。
    ///
    /// ⚠ 顺序 = [`walk`] 的推进顺序 = [`Caps`] 的字段序 = [`KNOWLEDGE`] 的下标，
    /// 四处对不上就会让反向夹具点错名（[`Caps::without`] 的自检看着这件事）。
    const HOPS: &[&str] = &[
        "跳② 宣告做不到（事前，只到「说得出」）",
        "跳④ 找它",
        "跳⑥ 传 argv 起它",
        "跳⑤ 问它会什么",
        "跳⑦ 拿码摘诊断",
        "跳⑧ 把码翻成语义",
    ];

    /// 每一跳缺了就走不动的**那一样知识**，与 [`HOPS`] 一一对应。
    const KNOWLEDGE: &[&str] = &[
        "做不到时那个词",
        "候选路径与那句尾巴",
        "探测调用的 argv",
        "必需能力清单",
        "真活调用的 argv 与 env",
        "退出码语义表",
    ];

    /// 假插件交出来的一整套知识。
    ///
    /// ⚠ **每一项都是 `Option`，这是本结构存在的全部理由**（形状照 `agents/fake::FakeCaps`）：
    /// 反向夹具要把某一样知识**挖掉**，而挖掉之后流程必须在一个**说得出话**的地方停。
    /// 用 `Option` 而不是真去注释掉一个函数，是为了让那个反向夹具成为**常驻判据**
    /// 而不是一次性手工变异 —— 手工变异证明的是「那天它会红」，
    /// 常驻判据证明的是「以后它一直会红」。
    #[derive(Clone)]
    struct Caps {
        unavailable_code: Option<&'static str>,
        discovery: Option<fn(&Path, &str) -> (Vec<PathBuf>, &'static str)>,
        probe_argv: Option<fn() -> Vec<&'static str>>,
        required_caps: Option<&'static [&'static str]>,
        work_call: Option<fn() -> (Vec<&'static str>, Vec<(&'static str, &'static str)>)>,
        code_table: Option<fn(Option<i32>) -> &'static str>,
    }

    impl Caps {
        /// 还剩几样知识在（[`without`](Self::without) 的自检用）。
        fn present(&self) -> usize {
            [
                self.unavailable_code.is_some(),
                self.discovery.is_some(),
                self.probe_argv.is_some(),
                self.required_caps.is_some(),
                self.work_call.is_some(),
                self.code_table.is_some(),
            ]
            .iter()
            .filter(|b| **b)
            .count()
        }

        /// 完整的一套（正题用）。
        ///
        /// ⚠ `unavailable_code` 取 [`REGISTRY_OWNED_CODE`]：
        /// 那个词必须是 `inbound::REGISTRY` 里真有人登记的，而不是本表编的
        /// （由 [`the_pre_declaration_reuses_the_word_the_registry_already_owns`] 钉住）。
        fn full() -> Self {
            Self {
                unavailable_code: Some(REGISTRY_OWNED_CODE),
                discovery: Some(discovery_of),
                probe_argv: Some(probe_argv),
                required_caps: Some(REQUIRED_CAPS),
                work_call: Some(work_call),
                code_table: Some(code_word),
            }
        }

        /// 挖掉**一样**知识（反向夹具用）。
        ///
        /// 名字必须是 [`KNOWLEDGE`] 里的一条 —— 写错就 **panic**，不静默返回原样：
        /// 静默的话反向夹具会**变成正题**再跑一遍，而正题是绿的
        /// （`agents/fake::FakeCaps::without` 逐字同一条）。
        fn without(&self, knowledge: &str) -> Self {
            assert!(
                KNOWLEDGE.contains(&knowledge),
                "`{knowledge}` 不是一样登记过的知识 —— 反向夹具挖了个不存在的洞，\
                 这一格此刻在把正题当反向跑（而正题是绿的）。已登记的：{KNOWLEDGE:?}"
            );
            let mut c = self.clone();
            match knowledge {
                "做不到时那个词" => c.unavailable_code = None,
                "候选路径与那句尾巴" => c.discovery = None,
                "探测调用的 argv" => c.probe_argv = None,
                "必需能力清单" => c.required_caps = None,
                "真活调用的 argv 与 env" => c.work_call = None,
                "退出码语义表" => c.code_table = None,
                other => unreachable!("`{other}` 在 KNOWLEDGE 里却没有对应字段 —— 两处漂开了"),
            }
            c
        }
    }

    /// 插件轴那个「它没被装上」的命令级 code —— **本文件里它只有这一处住址**。
    ///
    /// ⚠ 它**不是我造的词**：`inbound::REGISTRY` 里那几条转调插件的命令自己登记着它，
    /// 而「它必须有主」这件事由 [`the_pre_declaration_reuses_the_word_the_registry_already_owns`]
    /// 钉住 —— 那边改了名，这里当场红（派生出来的表会变空，而空表会被那条断言逮住）。
    ///
    /// ★ 本文件第一版为它写过**两份**（一份运行时拼、一份字面量）＋一条相等断言看着；
    /// 自查按「一个事实一个住址」收成一份 —— 相等断言守不住的是「两份一起被改错」，
    /// 而一处住址**根本不给那种机会**（`brief` 13b 那一族）。
    const REGISTRY_OWNED_CODE: &str = "not_installed";

    /// 流程停下来的原因 —— **必须说得出「哪一跳、为什么」**。
    ///
    /// # ★ 为什么是**五种**而不是一种（这一格是自查抓到的，写下来别再合并回去）
    ///
    /// 第一版只有 `MissingKnowledge` 一种，于是「**把执行位去掉**」这件事
    /// 也被报成「缺了一样知识」—— 而真话是「那个文件在，但它不是个能跑的东西」。
    /// 那**正是**件文件 `KW2E3` 逐字禁掉的坏法（笼统归因），只是换了个词：
    /// 不是笼统的「调用失败」，而是笼统的「缺知识」。
    /// ⇒ 五种**刻意分开**，每一种说的是一件不同的事：
    ///
    /// | 变体 | 它说的那件事 | 谁该去修 |
    /// |---|---|---|
    /// | `MissingKnowledge` | 假插件**自己少了一样知识**（反向夹具挖的洞） | 写适配层的人 |
    /// | `NotFound` | 盘上**找不到**那个可执行文件；`why` 是通用口拼的「我查过这几处」 | 装插件的人 |
    /// | `CouldNotStart` | 找到了、**起不来**（`invoke::NotRun`：参数太大 / IO 错） | 分两种，`why` 里说清是哪一种 |
    /// | `Rejected` | 起来了、**协商没过**（`probe::Rejected::message()`） | 换一份新的插件 |
    /// | `Inconsistent` | 跑通了、但那一跳的读数**自相矛盾**（比如只差一项 env 结果却一样） | 写夹具的人（它没在测该测的东西） |
    #[derive(Debug, PartialEq, Eq)]
    enum Stop {
        MissingKnowledge {
            hop: &'static str,
            knowledge: &'static str,
        },
        NotFound {
            hop: &'static str,
            why: String,
        },
        CouldNotStart {
            hop: &'static str,
            why: String,
        },
        Rejected {
            hop: &'static str,
            why: String,
        },
        Inconsistent {
            hop: &'static str,
            why: String,
        },
    }

    /// `invoke::NotRun` 的两支各自说清楚 —— 合成一个桶就等于**归错因**
    /// （`invoke.rs` 那个枚举的头注逐字：调用方要分得出「我给的东西太大」与「那个程序坏了」）。
    fn why_not_run(e: crate::plugin::invoke::NotRun) -> String {
        match e {
            crate::plugin::invoke::NotRun::ArgListTooLong => {
                "参数塞不进一次命令调用（内核的单参数上限）—— 我给的东西太大，不是那个程序坏了"
                    .to_string()
            }
            crate::plugin::invoke::NotRun::Failed(msg) => msg,
        }
    }

    /// 走完一遍之后手上的东西 —— 正题那一格要逐跳核它。
    ///
    /// `Debug` 是**承重的**：反向那几格红的时候要把「它到底走成了什么样」原样端出来，
    /// 而「停下来了」与「走完了但内容不对」只有把它印出来才分得开。
    #[derive(Debug)]
    struct Walk {
        done: Vec<&'static str>,
        declared: Vec<crate::wire::Unavailable>,
        bin: PathBuf,
        answered_caps: Vec<String>,
        extras: Vec<(String, String)>,
        refused_code: Option<i32>,
        refused_diag: String,
        refused_word: &'static str,
        sealed_code: Option<i32>,
        sealed_word: &'static str,
        sealed_stdout: String,
    }

    /// 把假插件推过 **6 跳**。
    ///
    /// # ⚠ 边界（本函数最容易被读错的一句，写在这里而不是只写在计划里）
    ///
    /// 跳④⑤⑥⑦ **真的调用通用层的机器**（`plugin::discover::find` ·
    /// `plugin::probe::negotiate` · `plugin::invoke::run` · `Done::code`/`diagnosis()`），
    /// 本件没动它们一个字节。跳② **只有判定那一半**是真的（从 `inbound::REGISTRY` 派生），
    /// **没有进真 `Hello`** —— 那需要一条真命令，而那是 PM 持有的文件。
    /// 跳⑧ 是假插件**拿自己的知识自问自答**，而这一格的自问自答**是对的**：
    /// `E6` 要求码表每插件一份。
    ///
    /// ⇒ 本函数走得通，证明的是「**这四样知识凑得出一条真跑得动的路**」，
    /// **不证明**「加一个插件宿主零改动」—— 后者今天不成立，差距读数在
    /// [`adding_a_plugin_still_costs_the_host_something`]。
    fn walk(caps: &Caps, root: &Path) -> Result<Walk, Stop> {
        let name = plugin_name();
        let mut done: Vec<&'static str> = Vec::new();

        // ── 跳② 宣告做不到（事前）：判定那一半，从 `REGISTRY` 派生 ──────────
        let code = caps.unavailable_code.ok_or(Stop::MissingKnowledge {
            hop: HOPS[0],
            knowledge: KNOWLEDGE[0],
        })?;
        let declared = unavailable_from_plugin(Some(false), code);
        if declared.is_empty() {
            // 知识在、而表是空的 ⇒ 那是**注册表侧**的事实变了（那个词被改名 / 没人登记它），
            // 不是「缺了一样知识」。两者分开报。
            return Err(Stop::Inconsistent {
                hop: HOPS[0],
                why: format!("`inbound::REGISTRY` 里没有任何一条命令登记 `{code}`"),
            });
        }
        done.push(HOPS[0]);

        // ── 跳④ 找它：**真过通用层** ────────────────────────────────────────
        let disc = caps.discovery.ok_or(Stop::MissingKnowledge {
            hop: HOPS[1],
            knowledge: KNOWLEDGE[1],
        })?;
        let (fixed, hint) = disc(root, &name);
        // ★ `why` 原样带上通用口拼的那句「我查过这几处」——`§3` 刀 C 要的正是这句话的形状：
        //   「找不到，我查过这几处」，**不是**笼统的「调用失败」，也不是「缺了一样知识」。
        let bin = crate::plugin::discover::find(&name, &fixed, false, hint)
            .map_err(|why| Stop::NotFound { hop: HOPS[1], why })?;
        done.push(HOPS[1]);

        // ── 跳⑥ 传 argv 起它：**真过通用层，且真起一个进程** ────────────────
        let argv = caps.probe_argv.ok_or(Stop::MissingKnowledge {
            hop: HOPS[2],
            knowledge: KNOWLEDGE[2],
        })?;
        let asked = argv();
        let out = crate::plugin::invoke::run(&bin, &asked, DEADLINE_SECS, &[]).map_err(|e| {
            Stop::CouldNotStart {
                hop: HOPS[2],
                why: why_not_run(e),
            }
        })?;
        if out.code != Some(0) {
            return Err(Stop::Inconsistent {
                hop: HOPS[2],
                why: format!(
                    "探测调用退出码是 {:?}（该是 0）：{}",
                    out.code,
                    out.diagnosis()
                ),
            });
        }
        done.push(HOPS[2]);

        // ── 跳⑤ 问它会什么：**真过通用层**，喂的是真进程刚打出来的字节 ──────
        let required = caps.required_caps.ok_or(Stop::MissingKnowledge {
            hop: HOPS[3],
            knowledge: KNOWLEDGE[3],
        })?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        // ★ `why` 原样带上通用口那句话：它会**点名缺的那一个 token**，
        //   而不是「缺能力」四个字，也不是「调用失败」（`probe` 那 8 条判据守的就是这个）。
        let answer = crate::plugin::probe::negotiate(&text, &name, required).map_err(|e| {
            Stop::Rejected {
                hop: HOPS[3],
                why: e.message(),
            }
        })?;
        done.push(HOPS[3]);

        // ── 跳⑦ 拿码摘诊断：真活两趟（一趟被拒、一趟成事）─────────────────
        let work = caps.work_call.ok_or(Stop::MissingKnowledge {
            hop: HOPS[4],
            knowledge: KNOWLEDGE[4],
        })?;
        let (work_argv, work_env) = work();
        let refused =
            crate::plugin::invoke::run(&bin, &work_argv, DEADLINE_SECS, &[]).map_err(|e| {
                Stop::CouldNotStart {
                    hop: HOPS[4],
                    why: why_not_run(e),
                }
            })?;
        let sealed = crate::plugin::invoke::run(&bin, &work_argv, DEADLINE_SECS, &work_env)
            .map_err(|e| Stop::CouldNotStart {
                hop: HOPS[4],
                why: why_not_run(e),
            })?;
        // 「同一条 argv、只差那一项 env，结果必须不同」—— 少了这一句，
        // 一个恒答同一张脸的假插件也能走完全流程。
        if refused.code == sealed.code || refused.diagnosis().is_empty() {
            return Err(Stop::Inconsistent {
                hop: HOPS[4],
                why: format!(
                    "同一条 argv 只差那一项 env，两趟却给了同一张脸：\
                     被拒 code={:?} diag={:?} · 成事 code={:?}",
                    refused.code,
                    refused.diagnosis(),
                    sealed.code
                ),
            });
        }
        done.push(HOPS[4]);

        // ── 跳⑧ 把码翻成语义：**每插件一份**（`E6`）⇒ 这一跳按定框就是自问自答 ──
        let table = caps.code_table.ok_or(Stop::MissingKnowledge {
            hop: HOPS[5],
            knowledge: KNOWLEDGE[5],
        })?;
        let refused_word = table(refused.code);
        let sealed_word = table(sealed.code);
        if refused_word == sealed_word {
            return Err(Stop::Inconsistent {
                hop: HOPS[5],
                why: format!(
                    "两个不同的码（{:?} / {:?}）被翻成了同一个语义词 `{refused_word}` —— \
                     那张码表此刻在恒答一张脸",
                    refused.code, sealed.code
                ),
            });
        }
        done.push(HOPS[5]);

        Ok(Walk {
            done,
            declared,
            bin,
            answered_caps: answer.capabilities.clone(),
            extras: answer.extras.clone(),
            refused_code: refused.code,
            refused_diag: refused.diagnosis(),
            refused_word,
            sealed_code: sealed.code,
            sealed_word,
            sealed_stdout: String::from_utf8_lossy(&sealed.stdout).into_owned(),
        })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 四 · 夹具的搭建（**只写临时目录**，与用户的任何真实数据零交集）
    // ═══════════════════════════════════════════════════════════════════════

    /// 夹具根 —— 按 tag + pid 隔开。
    ///
    /// ⚠ 目录名取**中性名**，一个字都不取自被断言的内容〔`brief` `6g`〕：
    /// `not_installed_message` 会把候选路径原样印进那句话，
    /// 名字若与断言的子串同源，那条断言就**靠路径恒真**。
    fn fixture_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pwf-{tag}-{}", std::process::id()))
    }

    /// 建一份夹具：**只在第二级候选处**放那个可执行文件（第一级 `libexec/` 刻意不建）。
    fn build_fixture(tag: &str) -> PathBuf {
        let root = fixture_root(tag);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建夹具根");
        let name = plugin_name();
        let f = root.join(&name);
        std::fs::write(&f, script_text(&name)).expect("写假插件");
        arm(&f);
        root
    }

    /// 给一个文件加执行位（`discover::is_executable` 判的正是它）。
    fn arm(f: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        #[cfg(not(unix))]
        {
            let _ = f;
        }
    }

    /// 读一读本文件自己的源码（`this_fixture_never_ships` 与那条天花板要用）。
    fn own_source() -> &'static str {
        include_str!("plugin_walk_fixture.rs")
    }

    /// daemon 的 `src/` 根。
    fn src_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// 那个真适配层今天的生产段（活体对拍语料，不是手抄的 fixture）。
    fn real_adapter_production() -> String {
        crate::guard_support::production_code(include_str!("control/cc_bus.rs"))
    }

    /// 通用调用口那一层今天的生产段，逐文件。
    ///
    /// ⚠ 走 `guard_core::scan_tree!` 而不是自己 `read_dir`：`scanning_guard_registry`
    /// 那条递减棘轮逐字要求新写的扫描型判据都走它。
    /// **代价如实写**：它按构造摘掉调用者自己那一份，而本文件不在 `plugin/` 下，
    /// 所以这里**没有**构造性缺口 —— `plugin/mod.rs` 那边有（它的调用者就在层内）。
    fn generic_port_production() -> Vec<(String, String)> {
        let root = src_root().join("plugin");
        let mut out: Vec<(String, String)> = guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(p, s)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, crate::guard_support::production_code(&s))
            })
            .collect();
        out.sort();
        // 采集自检：`plugin/` 今天是 4 个 `.rs`，而本文件不在层内 ⇒ 一个都不该被摘掉。
        assert_eq!(
            out.len(),
            4,
            "`plugin/` 采集到 {} 个文件（今天应当是 4 个）—— 采集坏了，\
             下面几条负向断言此刻在空转。实得：{:?}",
            out.len(),
            out.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
        out
    }

    /// 本夹具的**全部词汇** —— 负向断言的分母就是这张表。
    fn fixture_vocabulary() -> Vec<(String, &'static str)> {
        let mut v: Vec<(String, &'static str)> = vec![
            (plugin_name(), "假插件的名字"),
            (PROBE_SUB.to_string(), "假插件的探测子命令"),
            (WORK_SUB.to_string(), "假插件干活的子命令"),
            (NAP_SUB.to_string(), "假插件会卡住的子命令"),
            (ROSTER_SUB.to_string(), "假插件报回整份环境键的子命令"),
            (REALM_ENV.to_string(), "假插件的环境变量名"),
            (REALM_VALUE.to_string(), "假插件夹具里的作业域值"),
        ];
        for t in DECLARED_CAPS.split(',') {
            v.push((t.to_string(), "假插件声明的能力 token"));
        }
        for c in [
            Some(0),
            Some(STALE_LEDGER),
            Some(REALM_LOCKED),
            Some(UNKNOWN_SUB),
            None,
        ] {
            v.push((code_word(c).to_string(), "假插件的语义码"));
        }
        v
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 五 · 判据
    // ═══════════════════════════════════════════════════════════════════════

    /// `KW2E2`：这个假插件**每一维都刻意与那个真插件族不同形** —— 逐维活体对拍。
    ///
    /// # 为什么这是地基而不是趣味
    ///
    /// 假的若照抄真的，通用层拿真的的知识去解释它**恰好也能读出东西** ——
    /// 那时「走通全流程」证明的是「一份布局被施加到另一个名字上」，
    /// 那是**一个粉饰的通过**（`agents/fake/mod.rs` 头注逐字）。
    ///
    /// # ⚠ 它对拍的是**活体**，不是手抄的表
    ///
    /// 候选路径与退出码语义两维都去问 `control/cc_bus.rs` **今天的函数**；
    /// 「兜不兜 `PATH`」那一维去钉它生产段里**那一行**。
    /// ⇒ 那边哪天改了形，这一格会红，而红的意思是「两边还不同形吗，来个人看一眼」。
    #[test]
    fn this_fake_plugin_is_deliberately_unlike_the_real_one() {
        let name = plugin_name();
        let home = PathBuf::from("/home/u");

        // ── ① 候选路径：两边**互不相交**，且各有一级对方没有的 ─────────────
        let theirs = crate::control::cc_bus::fixed_candidates(None, Some(&home), &name);
        let (mine, hint) = discovery_of(&home, &name);
        assert!(
            !theirs.is_empty() && !mine.is_empty(),
            "有一边的候选列表是空的 ⇒ 下面的不相交断言是空真：mine={mine:?} theirs={theirs:?}"
        );
        for p in &mine {
            assert!(
                !theirs.contains(p),
                "候选路径撞上了那个真插件族的装机位：{p:?}"
            );
        }
        fn has_component(ps: &[PathBuf], seg: &str) -> bool {
            ps.iter()
                .any(|p| p.components().any(|c| c.as_os_str() == seg))
        }
        assert!(
            has_component(&mine, "libexec"),
            "本夹具那一级 `libexec` 不见了 —— 本条在空转"
        );
        assert!(
            !has_component(&theirs, "libexec"),
            "那个真插件族也走 `libexec` 了 —— 这一维不再不同形"
        );
        assert!(
            has_component(&theirs, ".local"),
            "对照坏了：那个真插件族不再找用户装机位"
        );
        assert!(
            !has_component(&mine, ".local"),
            "本夹具跑去找用户装机位了 —— 那是照抄"
        );

        // ── ② 兜不兜 `PATH`：那边显式 `true`，本夹具显式 `false` ───────────
        let prod = real_adapter_production();
        guard_core::pin_line(
            &prod,
            "crate::plugin::discover::find(name, &fixed, true, NOT_INSTALLED_HINT)",
        )
        .unwrap_or_else(|why| {
            panic!(
                "那个真适配层「兜 `PATH`」那一行对不上了：{why}\n\
                 本条钉的**不是**那一行不许变，而是「两边在这一维上还不同形吗」——\
                 它变了就该有人来看一眼（本夹具给的是 `false`，由下面那句 `0 个目录` 钉住）。"
            );
        });
        let missing = crate::plugin::discover::not_installed_message(&name, &mine, 0, hint);
        assert!(
            missing.contains("PATH 上的 0 个目录"),
            "本夹具没走成「不兜 PATH」那一侧：{missing}"
        );

        // ── ③ 探测入口：**子命令**（无前导 `-`），不是旗标 ───────────────────
        let asked = probe_argv();
        assert_eq!(asked.len(), 1, "探测 argv 的形状变了：{asked:?}");
        assert!(
            !asked[0].starts_with('-'),
            "探测入口长成了旗标 —— 那正是那个真插件族的形状：{asked:?}"
        );

        // ── ④ 退出码语义：同一个数，两边**不是同一件事** ────────────────────
        for c in [
            Some(2),
            Some(3),
            Some(STALE_LEDGER),
            Some(REALM_LOCKED),
            Some(UNKNOWN_SUB),
            Some(TIMED_OUT_CODE),
            None,
        ] {
            assert_ne!(
                code_word(c),
                real_plugin_word(c).as_str(),
                "退出码 {c:?} 在两边是同一个语义词 —— 码表照抄了，\
                 而「同一个数在不同插件里语义互斥」正是 `E6` 那条判断的全部依据"
            );
        }
        // 唯一刻意重叠的那一格：`0` 两边都是成事 —— 那不是照抄，是「成功」这件事本身。
        assert_eq!(code_word(Some(0)), "sealed");
        assert_eq!(real_plugin_word(Some(0)), "<成事>");
        // 三档非零码互不相同（并成一个码，人就分不出「该重试」与「被拒」）。
        assert_ne!(code_word(Some(STALE_LEDGER)), code_word(Some(REALM_LOCKED)));
        assert_ne!(code_word(Some(REALM_LOCKED)), code_word(Some(UNKNOWN_SUB)));
        assert_ne!(code_word(Some(STALE_LEDGER)), code_word(Some(UNKNOWN_SUB)));

        // ── ⑤ 必需清单是**子集**，不是全等（子集检查才是三个真实消费者的形状）──
        let declared: Vec<&str> = DECLARED_CAPS.split(',').collect();
        assert!(
            REQUIRED_CAPS.len() < declared.len(),
            "必需清单与声明的能力一样长 —— 那就验不到子集检查这件事"
        );
        for r in REQUIRED_CAPS {
            assert!(declared.contains(r), "必需清单里的 `{r}` 假插件根本没声明");
        }

        // ── ⑥ 知识表与 `Caps` 的字段**不许漂开** ────────────────────────────
        let full = Caps::full();
        assert_eq!(
            full.present(),
            KNOWLEDGE.len(),
            "`Caps::full()` 交出来的知识数与 `KNOWLEDGE` 对不上 —— 两处漂开了"
        );
        assert_eq!(HOPS.len(), KNOWLEDGE.len(), "跳与知识不是一一对应了");
        for k in KNOWLEDGE {
            assert_eq!(
                full.without(k).present(),
                KNOWLEDGE.len() - 1,
                "挖「{k}」这个洞没有挖到任何东西"
            );
        }
    }

    /// `KW2E1`/`KW2E3` 正题：**一个假插件把 6 跳串成一条路**，而通用层一行没改。
    ///
    /// # 它与今天那 32 条判据的差别（这一句是本件的全部内容）
    ///
    /// 件文件 `§0c` 现打：`plugin/` **25 条**（`discover` 5 · `probe` 8 · `invoke` 6 ·
    /// `mod.rs::layer_guard` 6）加 `control/cc_bus.rs` **7 条**，逐条读下来
    /// **没有任何一条同时驱动 ≥2 跳**，而且**没有一条真起过进程**
    /// （`invoke.rs` 那 6 条是 `argv_for` 的纯函数断言与手搓 `Done{…}`；
    /// `cc_bus.rs` 那 7 条是 `parse_list`/`classify_send`/`fixed_candidates` 一族纯函数）。
    /// ⇒ 本条是**第一条**把 ②④⑤⑥⑦⑧ 串起来、并且真起进程的判据。
    ///
    /// # ⚠ 边界逐跳写在断言旁边，不许含混成「走通了」
    #[test]
    fn one_fake_plugin_walks_six_hops_as_one_path() {
        let root = build_fixture("walk");
        let name = plugin_name();
        // ⚠ 这句话**刻意不预判原因**：停车的原因由 `Stop` 那五种自己说
        //（把「走不通」一律归成「接口面漏了东西」，正是本件在治的笼统归因 —— 自查抓到）。
        let w = walk(&Caps::full(), &root).unwrap_or_else(|e| {
            panic!(
                "这条路没走通。原因看下面这一行是哪一种：缺知识 ⇒ 接口面漏了东西；\
                 没装 / 起不来 ⇒ 夹具没建对；协商没过 ⇒ 假插件的探测输出与必需清单对不上；\
                 读数自相矛盾 ⇒ 这一格没在测该测的东西。\n实得：{e:?}"
            )
        });
        assert_eq!(
            w.done.as_slice(),
            HOPS,
            "走到的跳与登记的对不上 —— driver 与 `HOPS` 漂开了"
        );

        // 跳②（**判定那一半是真的，没进真 `Hello`**）：表非空，且每条都自洽。
        assert!(!w.declared.is_empty(), "事前那张表是空的");
        for u in &w.declared {
            assert!(
                crate::inbound::COMMANDS.contains(&u.command.as_str()),
                "声明做不到的 `{}` 根本不在 `commands` 里 —— 本字段说的是「接得下但做不到」",
                u.command
            );
            assert_eq!(u.code, REGISTRY_OWNED_CODE);
        }

        // 跳④（**真过通用层**）：找到的是**第二级**候选 —— 第一级不存在，找法真走过它。
        assert_eq!(
            w.bin,
            root.join(&name),
            "找到的不是第二级候选 —— 「顺序即优先级」这一格此刻是空真（第一级根本没建）"
        );
        assert!(
            !root.join("libexec").exists(),
            "第一级候选被建出来了 —— 那样「走过一个不存在的候选」就没验到"
        );

        // 跳⑤（**真过通用层**，喂的是真进程刚打出来的字节）。
        let declared: Vec<&str> = DECLARED_CAPS.split(',').collect();
        assert_eq!(
            w.answered_caps.len(),
            declared.len(),
            "协商读回来的能力数与假插件声明的对不上：{:?}",
            w.answered_caps
        );
        for r in REQUIRED_CAPS {
            assert!(
                w.answered_caps.iter().any(|c| c == r),
                "必需的 `{r}` 没在回答里 —— 而协商却放行了"
            );
        }
        // 「其余键原样带回」那一格：本层不解释插件自己的域枚举。
        assert!(
            w.extras.iter().any(|(k, _)| k == "realms"),
            "插件自己的其它键被吞了：{:?}",
            w.extras.iter().map(|(k, _)| k).collect::<Vec<_>>()
        );

        // 跳⑥⑦（**真过通用层，且真起了进程**）：同一条 argv，只差那一项 env。
        assert_eq!(
            w.refused_code,
            Some(REALM_LOCKED),
            "被拒那一趟的码不对（假插件没按自己的契约拒）"
        );
        assert_eq!(w.sealed_code, Some(0), "成事那一趟没成");
        assert!(
            w.refused_diag.contains("realm"),
            "诊断没从 stderr 摘到那一行：{:?}",
            w.refused_diag
        );
        assert!(
            w.sealed_stdout.contains(REALM_VALUE),
            "那一项 env 没被交给子进程 —— `invoke::run` 的 `env` 入参此刻没人在验：{:?}",
            w.sealed_stdout
        );

        // 跳⑧（**自问自答，而这一格按 `E6` 就该如此**）。
        assert_eq!(w.refused_word, "realm_locked");
        assert_eq!(w.sealed_word, "sealed");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `KW2E3` 的**牙**：那条路真的**起了一个进程**，而不是喂了一个手搓的 `Done{…}`。
    ///
    /// # 它凭什么分得出来（这一格是本件自查逼出来的，写清来历）
    ///
    /// 规划 `§3` 的 `7u` 那一刀（「把实现整个退掉，还有多少条新断言仍绿」）时先纸上推了一遍：
    /// 把 [`walk`] 里那三处 `invoke::run` 换成**手搓的 `Done{…}`**（正是今天 `invoke.rs`
    /// 那 6 条判据的形状），上面那条走全流程的正题**会全绿** ——
    /// 因为它断的全是**契约**（码是几 · 诊断里有哪个词 · stdout 里有没有那一项 env），
    /// 而契约是手搓得出来的。
    /// ⇒ 「真起进程」这件事在那条判据里**没有牙**。本格补的就是那颗牙。
    ///
    /// 判据 = 那个真进程报回来**只有真进程才产得出**的两样事实：
    /// ① 它自己的**父进程名**（`/proc/$PPID/comm`）—— 手搓的 `Done` 不知道这个；
    /// ② 它看到的 **`PATH`**，且必须与宿主逐字相同 —— 手搓的更不知道。
    /// 两样都走**探测输出的 `extras`**（通用层「其余键原样带回」那一格）拿回来。
    ///
    /// ⚠ **射程**：它证的是「跳⑥ 那一趟真的 fork/exec 了」，
    /// **不证**跳⑦⑧ 的读数来自真进程（那两跳的牙在上面那条正题的契约断言里）。
    #[test]
    fn the_walk_really_started_a_process_not_a_hand_built_done() {
        let root = build_fixture("real");
        let w = walk(&Caps::full(), &root).unwrap_or_else(|e| panic!("走不通：{e:?}"));
        let get = |k: &str| {
            w.extras
                .iter()
                .find(|(kk, _)| kk == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let parent = get("parent");
        assert!(
            !parent.is_empty(),
            "探测输出里没有那个真进程报回的**父进程名** —— 那一趟多半根本没 fork/exec\n\
             （手搓一个 `Done{{…}}` 也能满足上面那条正题的每一句契约断言，\
              所以「真起进程」这件事只有本格看得见）。实得 extras：{:?}",
            w.extras
        );
        let child_path = get("inherited-path");
        let host_path = std::env::var("PATH").unwrap_or_default();
        assert!(!host_path.is_empty(), "宿主 `PATH` 是空的 ⇒ 下面那句是空真");
        assert_eq!(
            child_path, host_path,
            "子进程看到的 `PATH` 与宿主不同 —— 要么那一趟没真起进程，\
             要么有人给 `invoke::run` 加了清环境的动作（后者见\
             `nothing_here_hands_the_plugin_a_designed_way_back_into_the_host_commands` 的头注）"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `KW2E3` 反向夹具（**常驻**，不是一次性手工变异）：
    /// **挖掉任何一样知识，流程都在一个说得出「哪一跳缺了哪一样」的地方停。**
    ///
    /// # 两向都验（只验「逮得住」会引进假阳）
    ///
    /// ① 正向对照是上面那条（不挖洞就走得完）；
    /// ② 本格内部再验一次「**停的位置随挖的洞变**」—— 一个在第一跳恒停的 driver
    ///    对每个洞都会「红」，那种红是假的；
    /// ③ 还验一次「挖一个**不存在**的洞要 panic」——静默返回原样的话，
    ///    反向夹具会变成正题再跑一遍，而正题是绿的；
    /// ④ 🔴 并且要求停车的**归因也对**：挖的是知识，报的就必须是 `MissingKnowledge`。
    ///    报成 `NotFound`/`CouldNotStart` 那一族 = 说得出话但**说错了话**，
    ///    照着去修会修错地方 —— 那比说不出话更坏（`Stop` 头注那张表就是为这一格立的）。
    #[test]
    fn digging_out_any_one_hop_stops_where_it_can_name_it() {
        let root = build_fixture("hole");
        let full = Caps::full();
        let mut stopped: Vec<(&str, &'static str, &'static str)> = Vec::new();
        for k in KNOWLEDGE {
            match walk(&full.without(k), &root) {
                Ok(w) => panic!(
                    "挖掉「{k}」之后流程仍然走完了 {:?} —— 少一样知识却一路绿灯，\
                     那正是 `KW2E3` 禁掉的那种「笼统地成功」",
                    w.done
                ),
                Err(Stop::MissingKnowledge { hop, knowledge }) => {
                    assert!(
                        HOPS.contains(&hop),
                        "停在一个不认识的跳 `{hop}`（挖的是「{k}」）"
                    );
                    assert!(
                        KNOWLEDGE.contains(&knowledge),
                        "停下来时报的知识 `{knowledge}` 不在登记表里（挖的是「{k}」）"
                    );
                    stopped.push((k, hop, knowledge));
                }
                Err(other) => panic!(
                    "挖的是**一样知识**（「{k}」），停下来报的却是另一族原因：{other:?}\n\
                     ⇒ 归因错了。「缺知识」「没装」「起不来」「协商没过」「读数自相矛盾」\
                     是五件不同的事，各有各的人去修（`Stop` 头注那张表）——\
                     把它们合成一个桶，正是本件在治的那种笼统归因换了个词。"
                ),
            }
        }
        // 停的位置必须**随挖的洞变**，而且这里是**一一对应**（比 `S6` 的「≥5 个不同段」更强）。
        let distinct: std::collections::BTreeSet<&str> =
            stopped.iter().map(|(_, h, _)| *h).collect();
        assert_eq!(
            distinct.len(),
            HOPS.len(),
            "{} 个洞只停出了 {} 个不同的跳：{stopped:?}\n\
             ⇒ driver 对「挖了哪个洞」不够敏感，本格的「会红」有一部分是假的",
            KNOWLEDGE.len(),
            distinct.len()
        );
        // 点名点错了人，照着去修会修错地方 —— 说得出话但说错了话，比说不出话更坏。
        for (holed, hop, named) in &stopped {
            assert_eq!(
                holed, named,
                "挖的是「{holed}」，停下来却点名「{named}」（跳 `{hop}`）"
            );
        }
        // 挖一个不存在的洞：必须 panic。
        let r = std::panic::catch_unwind(|| Caps::full().without("这一样知识不存在"));
        assert!(
            r.is_err(),
            "挖一个不存在的洞被静默放过了 —— 反向夹具此刻在把正题当反向跑"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `KW2E3` 刀 C（**常驻**）：把那个可执行文件拿掉 / 去掉执行位 ⇒
    /// 跳④ 红的那句话是「**找不到，我查过这几处**」，不是笼统的「调用失败」。
    ///
    /// 「没装 / 找不到」必须是一个**能自证的**回答：用户级目录不在非登录 shell 的 `PATH` 里
    /// 是常态，说不出查过哪儿的话，人只能靠猜（`discover` 那 5 条判据守的就是这句话的形状）。
    ///
    /// ⚠ 断言刻意**不取夹具目录的名字**〔`brief` `6g`〕：那句话会把候选路径原样印出来，
    /// 断言若取自路径，它就靠路径恒真。这里断的是**两处候选都被列出来** +
    /// **调用方那句尾巴在** + **PATH 那一半的目录数是 0**。
    #[test]
    fn a_missing_or_unarmed_executable_says_where_it_looked() {
        let name = plugin_name();

        // ── ① 一个字节都没有：两处候选都要出现在那句话里 ────────────────────
        let root = fixture_root("gone");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建夹具根");
        let (fixed, hint) = discovery_of(&root, &name);
        let err = crate::plugin::discover::find(&name, &fixed, false, hint)
            .expect_err("一个可执行文件都没有，却说找到了");
        for c in &fixed {
            assert!(
                err.contains(&c.display().to_string()),
                "那句话没列出候选 {c:?}：{err}"
            );
        }
        assert!(err.contains("刻意不兜"), "调用方那句尾巴丢了：{err}");
        assert!(
            err.contains("PATH 上的 0 个目录"),
            "PATH 那一半说错了：{err}"
        );
        assert!(
            !err.contains("失败"),
            "归因说成了「失败」—— 那正是这条判据要挡的笼统说法：{err}"
        );

        // ── ② 文件在、**没有执行位**：仍然是「找不到」，不是「起不来」 ───────
        let f = root.join(&name);
        std::fs::write(&f, script_text(&name)).expect("写假插件");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).expect("chmod");
            assert!(
                !crate::plugin::discover::is_executable(&f),
                "没有执行位的文件被当成可执行文件了"
            );
            let err2 = crate::plugin::discover::find(&name, &fixed, false, hint)
                .expect_err("没有执行位却说找到了");
            assert!(err2.contains("刻意不兜"), "{err2}");
        }

        // ── ③ 反向对照：加上执行位就该找到 —— 少了这一句，上面两格可能只是恒红 ──
        arm(&f);
        #[cfg(unix)]
        {
            let got = crate::plugin::discover::find(&name, &fixed, false, hint)
                .expect("加上执行位之后应当找得到");
            assert_eq!(got, f);
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `KW2E3` 刀 C 的**常驻形**：把执行位去掉，整条路必须停在**跳④**，
    /// 而且说的是「**找不到，我查过这几处**」——不是「缺了一样知识」、不是「调用失败」。
    ///
    /// # 为什么做成常驻而不是一次性变异
    ///
    /// 手工变异证明的是「那天它会红」，常驻判据证明的是「以后它一直会红」
    /// （`agents/fake/mod.rs` 逐字）。而这一格**真的逮到过东西**：本文件第一版的 `Stop`
    /// 只有一种变体，于是这条路在这里报的是「缺了一样知识」——
    /// 说得出话，但**说错了话**。`Stop` 那五种就是这一格逼出来的。
    #[test]
    fn an_unarmed_plugin_stops_at_hop_four_saying_where_it_looked() {
        let root = fixture_root("unarmed");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建夹具根");
        let name = plugin_name();
        let f = root.join(&name);
        // 文件在、正文对、**就是没有执行位**。
        std::fs::write(&f, script_text(&name)).expect("写假插件");
        let (fixed, _) = discovery_of(&root, &name);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).expect("chmod");
            match walk(&Caps::full(), &root) {
                Err(Stop::NotFound { hop, why }) => {
                    assert_eq!(hop, HOPS[1], "停对了原因，却停在别的跳上");
                    for c in &fixed {
                        assert!(
                            why.contains(&c.display().to_string()),
                            "那句话没列出候选 {c:?}：{why}"
                        );
                    }
                    assert!(why.contains("刻意不兜"), "调用方那句尾巴丢了：{why}");
                    assert!(
                        why.contains("PATH 上的 0 个目录"),
                        "PATH 那一半说错了：{why}"
                    );
                }
                other => panic!(
                    "去掉执行位之后没停在跳④ 的「找不到」上，而是：{other:?}\n\
                     ⇒ 「那个文件不是个能跑的东西」被说成了别的事。\
                     `discover` 那 5 条判据守的就是这句话的形状。"
                ),
            }

            // 反向对照：加上执行位就走得完 —— 少了这一句，本格可能只是恒红。
            arm(&f);
            assert!(
                walk(&Caps::full(), &root).is_ok(),
                "加上执行位之后仍然走不完 —— 本格上面那半是恒红，证不了任何事"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `KW2E4`（`§0d` 出路**丙**）：事前那句「我做不到」用的词，
    /// **必须是 `inbound::REGISTRY` 里那条命令自己登记过的**。
    ///
    /// # 为什么走丙，不走甲也不走乙（论据，不是偏好）
    ///
    /// - **甲**（借一条已有命令说话）：那样它就**不是「一个新插件」**了 ——
    ///   `EF06` 问的恰恰是「**加一个**插件」，借别人的命令说话正好把靶子打歪。
    /// - **乙**（`REGISTRY` 8 → 9）：靶子最正，但要动 PM 持有的 `inbound.rs`
    ///   + `protocol_doc_guard` 对拍文档 + 那几个钉死计数 ⇒ **走 `§4` 上报，不许自批**。
    /// - **丙**（只驱动判定那一半）：一个共享文件都不碰，代价是跳② **只买到一半** ——
    ///   「说得出」有了，「**说出口**」没有。这一半的缺口由本格最后那两句**如实钉住**，
    ///   不许让它看起来像已经有了。
    ///
    /// # 本格真正逮的是什么（不是同义反复）
    ///
    /// 那张表从 `REGISTRY.codes` 派生，而 [`REGISTRY_OWNED_CODE`] 那个字面量是**第二份拷贝**。
    /// 有人把 `REGISTRY` 里的那个词改名（比如收窄成 `plugin_missing`），
    /// 派生出来的表会**静默变空** —— 所有测试照绿，而插件轴上的第四条面从此永远不说话。
    /// 本格把那次改名变成一次红。形状照 `main.rs::the_declared_code_is_one_the_registry_already_declares`
    /// （那一条守的是 tmux 轴的 `no_tmux`，本条守的是插件轴的那个词；两条互不覆盖）。
    ///
    /// # 🔴 三条红线，本格一条都没碰（逐条点名，便于复核）
    ///
    /// 1. `main.rs` 生产段那一行 `unavailable: Vec::new(),` —— 本文件**没有**改它；
    /// 2. `wire.rs::hello_unavailable_is_additive_present_and_absent` 的两串期望字节 ——
    ///    本文件**没有**构造任何 `Frame::Hello`，一个字节都没碰；
    /// 3. 宣称的 `code` 必须是那条命令自己登记过的 —— 那正是本格的正题。
    #[test]
    fn the_pre_declaration_reuses_the_word_the_registry_already_owns() {
        // ⚠ 那个词在本文件里**只有一处住址**（[`REGISTRY_OWNED_CODE`]）——
        //   本文件第一版为它写过两份（一份运行时拼、一份字面量）＋一条相等断言看着，
        //   自查时按「一个事实一个住址」收成一份：一处住址就不需要那条断言了。
        //   它真正的对照在下面 ①：那个词**必须是 `inbound::REGISTRY` 里真有人登记的**。

        // ① 那个词**必须有主**：`REGISTRY` 里得真有命令登记它。
        let owners: Vec<&str> = crate::inbound::REGISTRY
            .iter()
            .filter(|s| s.codes.contains(&REGISTRY_OWNED_CODE))
            .map(|s| s.name)
            .collect();
        assert!(
            !owners.is_empty(),
            "`REGISTRY` 里没有任何一条命令登记 `{REGISTRY_OWNED_CODE}` —— \
             要么那个词被改名了、要么转调插件的命令都没了。\n\
             无论哪种，插件轴上的第四条面此刻**永远为空**，而它自己不会喊疼。"
        );

        // ② 三态：只在**有把握**时开口，没把握时退回今天的行为。
        assert!(
            unavailable_from_plugin(Some(true), REGISTRY_OWNED_CODE).is_empty(),
            "装着的时候还在说做不到"
        );
        assert!(
            unavailable_from_plugin(None, REGISTRY_OWNED_CODE).is_empty(),
            "「判不出来」被压成了「做不到」—— 那一侧会让能用的功能从界面上无声消失"
        );
        let missing = unavailable_from_plugin(Some(false), REGISTRY_OWNED_CODE);
        assert_eq!(
            missing.len(),
            owners.len(),
            "确证没装时列出来的，不是 `REGISTRY` 里登记了那个词的那几条：{missing:?}"
        );
        for u in &missing {
            let spec = crate::inbound::REGISTRY
                .iter()
                .find(|s| s.name == u.command)
                .unwrap_or_else(|| panic!("声明了一条 `REGISTRY` 里没有的命令：{}", u.command));
            assert!(
                spec.codes.contains(&u.code.as_str()),
                "给 `{}` 声明的原因 `{}` 不在它自己登记的 codes {:?} 里 —— \
                 事前说的和事后回的不是同一句话，客户端得为此维护第二张翻译表",
                u.command,
                u.code,
                spec.codes
            );
            assert!(
                crate::inbound::COMMANDS.contains(&u.command.as_str()),
                "`{}` 不在 `commands` 里 —— 「根本不接」那一格由不在 `commands` 里表达，\
                 不该出现在这张「接得下但做不到」的表上",
                u.command
            );
        }

        // ③ 事前那个词与**事后真调用**回的那个词是同一个 —— 后者来自一次真的盘上失败。
        let root = fixture_root("word");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建夹具根");
        let name = plugin_name();
        let (fixed, hint) = discovery_of(&root, &name);
        let after = crate::plugin::discover::find(&name, &fixed, false, hint)
            .err()
            .map(|_| REGISTRY_OWNED_CODE.to_string())
            .expect("没装却说找到了");
        assert_eq!(
            after, missing[0].code,
            "事前说的与事后回的不是同一个词 —— `wire.rs` 那个字段的头注逐字要求\
             「事前与事后是同一句话，只是来得早」"
        );
        let _ = std::fs::remove_dir_all(&root);

        // ④ 🔴 **缺口如实钉住**：这个假插件自己的名字**进不了** `commands`
        //    ⇒ 它「说得出」，但**说不出口**。走出路乙（`REGISTRY` 8 → 9）才买得到另一半，
        //    而那要动 PM 持有的文件 ⇒ `§4` 上报。
        assert!(
            !crate::inbound::COMMANDS.contains(&plugin_name().as_str()),
            "假插件的名字混进了 `inbound::COMMANDS` —— 那是**出货面**，夹具不许进去"
        );
        assert!(
            !crate::inbound::REGISTRY
                .iter()
                .any(|s| s.name == plugin_name()),
            "假插件混进了 `inbound::REGISTRY` —— 真填 `hello.commands` 那天，\
             daemon 会向仓外消费方声明一条**不存在**的命令"
        );
    }

    /// `KW2E7`③：**通用层不许因此认识一个具体插件** —— 本夹具的词汇一个都不许漏进
    /// `plugin/` 的生产段。
    ///
    /// # 为什么非得自己写一条
    ///
    /// `plugin::layer_guard::the_generic_port_names_no_concrete_plugin` 的分母是
    /// `layer_guard::concrete_plugin_words` 那张表（成员与条数只住那一处，这里**不复述**）。
    /// 09-04 之前那张表只有**一族**词；同日加了**第二族**（代码全景那个插件）。
    /// 而它的头注逐字仍然写着「**换一族词汇的插件它一个都认不出来**」——
    /// ⇒ 本夹具换的正是**第三族**词，那条判据对它**零覆盖**，别指望它接住。
    /// 这一格就是 `E6` 那句「每加一个插件要加它自己那组专有针」在本件上的兑现。
    ///
    /// ⚠ 本条的**射程**：分母 = [`fixture_vocabulary`] 那张表（今天现算，见报错文案里的数），
    /// 不是「所有可能的插件词」。
    #[test]
    fn the_generic_port_never_learns_this_fixtures_vocabulary() {
        let vocab = fixture_vocabulary();
        assert!(!vocab.is_empty(), "词表空了 ⇒ 本条恒绿");
        let mut hits: Vec<String> = Vec::new();
        for (file, prod) in generic_port_production() {
            for (no, line) in prod.lines().enumerate() {
                for (w, why) in &vocab {
                    if line.contains(w.as_str()) {
                        hits.push(format!(
                            "  plugin/{file}:{} [{why}] {}",
                            no + 1,
                            line.trim()
                        ));
                        break;
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "通用调用口的生产段里出现了**这个夹具插件**的词汇：\n{}\n\n\
             ⇒ 这一层只提供形状（找它 / 传 argv / 问能力 / 拿到码），\
             具体插件的子命令名、环境变量、能力 token、语义码一律走**参数**或住调用方那一侧。\n\
             ⚠ 本条的射程：今天认的就是本夹具登记的这 {n} 个词。",
            hits.join("\n"),
            n = vocab.len(),
        );

        // 阴性对照：这张词表真的会咬人（不然它哪天被改空，正题会**安静地全绿**）。
        let synthetic = format!("    let out = run(&bin, &[\"{PROBE_SUB}\"], 5, &[])?;");
        assert!(
            vocab.iter().any(|(w, _)| synthetic.contains(w.as_str())),
            "合成的违规样本没被认出来 —— 正题那条此刻是空转的：{synthetic}"
        );
        // 反向：只是**恰好含同一个子串**的通用写法不许误命中（误伤会训练人绕过判据）。
        let innocent = "    let (prog, argv) = argv_for(bin, args, deadline_secs, deadline_bin);";
        assert!(
            !vocab.iter().any(|(w, _)| innocent.contains(w.as_str())),
            "把通用写法误判成了夹具词汇：{innocent}"
        );
    }

    /// `KW2E7`①②：**这份夹具永远不上生产**，而且它**不是一条躲判据的逃生舱**。
    ///
    /// # 三条，各有各的钉法
    ///
    /// 1. **零字节**：本文件那个**文件级** `cfg(test)`（内层属性形）——`main.rs` 里那一行 `mod`
    ///    是无条件的，所以「不上生产」这件事全押在这个属性上。它掉了 ⇒ 生产二进制里凭空多出
    ///    一个假插件的知识与一段 shell 脚本，而没有任何东西会说。
    ///    ⚠ 它的逐字形状**只许在本文件出现一次**（就是文件开头那一处）——
    ///    下面那条断言数的正是这个 1，散文里复述一份就会把它数成 2。
    /// 2. **不进任何真注册表**：那一半在
    ///    [`the_pre_declaration_reuses_the_word_the_registry_already_owns`] 的第 ④ 段。
    /// 3. **天花板**（形状照 `agent_locality_guard::FIXTURE_HOMES_CEILING = 1`）：
    ///    全 crate 的**测试段**里，经通用调用口真起进程的地方**只许有本文件一处**。
    ///    没有天花板的话，「夹具」就成了往那条唯一起进程口上挂东西的逃生舱 ——
    ///    而 `readonly_guard::spawn_registry` 那条相等断言只数**生产段**，它看不见这一侧。
    #[test]
    fn this_fixture_never_ships() {
        // ① 文件级 `cfg(test)` 在，而且生产段里一个函数都没有。
        let own = own_source();
        let attr = format!("#![cfg({})]", "test");
        assert_eq!(
            own.matches(attr.as_str()).count(),
            1,
            "本文件的文件级 `{attr}` 不是恰好一处 —— 掉了就会被编进生产二进制，\
             多了说明有人复制了一份"
        );
        let prod = crate::guard_support::production_code(own);
        assert!(
            !prod.contains("fn "),
            "本文件剥掉测试段之后还剩函数 —— 那些东西会进生产二进制：\n{prod}"
        );
        assert!(
            !prod.contains("crate::plugin"),
            "本文件的生产段里出现了 `plugin` 边 —— 那条边会进 `layering_guard` 的账，\
             而本文件的每一条都该在 `#[cfg(test)]` 里"
        );
        crate::guard_support::assert_no_test_code("plugin_walk_fixture.rs", &prod);

        // ② `main.rs` 里那一行 `mod` 声明在（掉了本文件整个不编译，但它也可能被改成
        //    别的形状；钉住它，顺带把「PM 预批的那一行」写成一条读数）。
        let main_rs = std::fs::read_to_string(src_root().join("main.rs")).expect("读 main.rs");
        let decl = format!("mod plugin_walk_{};", "fixture");
        assert_eq!(
            main_rs.matches(decl.as_str()).count(),
            1,
            "`main.rs` 里 `{decl}` 不是恰好一处"
        );

        // ③ 天花板：测试段里经通用口起进程的，除本文件之外**零处**。
        //
        //    ⚠ 已知的**非真起进程**命中逐条登记 —— 那两处是分层判据自己的合成样本
        //    （字符串里的调用形，不起任何进程）。这张表**只许变短**。
        const SYNTHETIC_ONLY: &[(&str, &str)] = &[(
            "layering_guard.rs",
            "分层扫描的两处合成违规样本（字符串字面量，不起进程）",
        )];
        let needle = format!("invoke::{}(", "run");
        assert!(
            guard_core::test_source(own).contains(needle.as_str()),
            "本文件的测试段里找不到 `{needle}` —— 抽取坏了，下面那条天花板在空转"
        );
        let mut others: Vec<String> = Vec::new();
        for (p, raw) in guard_core::scan_tree!(&src_root(), &["rs"]) {
            if guard_core::test_source(&raw).contains(needle.as_str()) {
                others.push(
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
            }
        }
        others.sort();
        let mut want: Vec<String> = SYNTHETIC_ONLY
            .iter()
            .map(|(f, _)| (*f).to_string())
            .collect();
        want.sort();
        assert_eq!(
            others, want,
            "测试段里经通用调用口起进程的文件与登记的对不上。\n\
             **多出来的**：先回答一句「这是不是又一处夹具，挂在那条唯一的起进程口上」——\
             `E5` 裁定插件调用**复用那一处口**，而 `readonly_guard::spawn_registry` \
             那条相等断言只数生产段、看不见测试段这一侧。\n\
             **少了的**：那处合成样本没了就摘登记（登记表腐烂比没有登记更糟）。"
        );
    }

    /// `KW2E2` 的两条环境前提之一：**期限落在子进程上，不在宿主里** ——
    /// 两条路各跑一趟，并且**说得出自己走的是哪一条**。
    ///
    /// # 为什么这一格非要「说得出口」
    ///
    /// `invoke::deadline_bin()` 找不到 `timeout(1)` 就**如实裸跑**（没有期限）。
    /// 那条降级今天由 `argv_for` 的纯函数判据钉着形状，
    /// 但**没有任何东西说得出「这台机器上今天走的是哪一条」** ——
    /// 而「一个绿说不清测没测到」正是 `K-R24` 那一族刚教过的账。
    ///
    /// ⇒ 本格问那个真进程：**你的父进程是谁**。
    /// · 有 `timeout(1)` ⇒ 父进程就是它 ⇒ 期限**真的**交给了子进程；
    /// · 没有 ⇒ 父进程是测试进程自己 ⇒ **裸跑**，而这一格会把这件事说出来。
    ///
    /// ⚠ **不改进程环境**（`std::env::set_var` 与并行跑的别的判据是竞态 ——
    /// 本 crate 里三处头注逐字写过这条），所以哪一条路被走到由机器决定，
    /// 本格的责任是**分得出来**、并且**两条都有断言**。
    #[test]
    fn the_deadline_lands_on_the_child_not_on_the_host() {
        let root = build_fixture("deadline");
        let name = plugin_name();
        let (fixed, hint) = discovery_of(&root, &name);
        let bin = crate::plugin::discover::find(&name, &fixed, false, hint).expect("该找得到");
        // ⚠ 把 `NotRun` 那两支的原文带上：「参数太大」（自己能修）与「那个程序坏了」
        //（自己修不了）是两件事，合成一句「起不来」就是归错因（`invoke::NotRun` 的头注逐字）。
        let out = crate::plugin::invoke::run(&bin, &probe_argv(), DEADLINE_SECS, &[])
            .unwrap_or_else(|e| panic!("那个假插件没跑起来：{}", why_not_run(e)));
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let answer = crate::plugin::probe::negotiate(&text, &name, REQUIRED_CAPS)
            .unwrap_or_else(|e| panic!("协商没过：{}", e.message()));
        let parent = answer
            .extras
            .iter()
            .find(|(k, _)| k == "parent")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        assert!(
            !parent.is_empty(),
            "问不出子进程的父进程是谁（`/proc` 读不到？）—— 本格此刻分不出两条路，\
             按「判不了」记，不许当成绿"
        );

        let deadline_cmd = crate::plugin::discover::on_path("timeout");
        match &deadline_cmd {
            Some(_) => {
                // 路 ①：有期限命令 ⇒ 它必须是子进程的**父进程**。
                assert_eq!(
                    parent, "timeout",
                    "这台机器上有期限命令，而子进程的父进程是 `{parent}` —— \
                     期限没交给子进程，`argv_for` 那个前缀此刻没有落地"
                );
                // 而且它**真的会收**：卡住那条子命令必须被收掉，码是那个通用码。
                let napped = crate::plugin::invoke::run(&bin, &[NAP_SUB], NAP_DEADLINE_SECS, &[])
                    .unwrap_or_else(|e| panic!("卡住那条子命令没跑起来：{}", why_not_run(e)));
                assert!(
                    napped.timed_out(),
                    "卡住的子进程没被期限收掉：code={:?}",
                    napped.code
                );
                assert_eq!(napped.code, Some(TIMED_OUT_CODE));
                assert_eq!(
                    code_word(napped.code),
                    "deadline_hit",
                    "超时那一格没被翻成插件自己的语义"
                );
            }
            None => {
                // 路 ②：**没有**期限命令 ⇒ 裸跑。这一趟必须**说得出口**：
                // 父进程不是期限命令，且 argv 里一个秒数都不许有。
                assert_ne!(
                    parent, "timeout",
                    "这台机器上找不到期限命令，子进程的父进程却是它 —— 两个读数互相矛盾"
                );
                let (prog, argv) = crate::plugin::invoke::argv_for(&bin, &[NAP_SUB], 7, None);
                assert_eq!(prog, bin, "裸跑时起的应当是插件自己");
                assert!(
                    !argv.contains(&"7".to_string()),
                    "秒数漏进了插件的 argv：{argv:?}"
                );
                // 🔴 卡住那条子命令**故意不跑**：没有期限的裸跑会把门禁挂死。
                //    这一格因此**只买到「说得出口」**，买不到「真被收掉」——如实写在这里。
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// PM 09-04 夜点名要顺带答的那一格（`DECISIONS.md` 末节 `R4`）：
    /// **「插件 → 宿主基础命令」这一跳今天有没有？**
    ///
    /// # 答：**设计上没有；而事实上有一条没人设计过的暗路。**
    ///
    /// ① **设计上零处**（本格 ① ② 两段钉住）：`plugin/` 的生产段里
    ///    不认识 `inbound`、不认识那张命令表；`layering_guard` 的
    ///    `plugin_layer_must_not_reference_control_or_observe` 对 `plugin → control`
    ///    是**零容忍** ⇒ 通用调用口在结构上**说不出**一条宿主命令的名字。
    ///    交给子进程的只有三样：argv · 额外 env · 关掉的 stdin —— 没有任何回程端点。
    ///
    /// ② **暗路 —— 09-05 关掉了，这一段是它的病历**〔`K-R26`〕。
    ///    09-04 夜本格逐字记着：`invoke::run` **不 `env_clear()`** ⇒ 子进程**继承 daemon 的
    ///    整份环境**，而常驻监听口那两个变量（`listen::ENV_PORT` / `listen::ENV_TOKEN`）
    ///    恰好就是「接上宿主 + 过鉴权」需要的两样 ⇒ 只要 daemon 是带着它们起的，
    ///    **任何被它起的插件读一读自己的环境就能回连宿主、发全部基础命令**。
    ///    那不是「已经有了回调口」—— 它是**一条没有协议、没有权限模型、没有审计的**回程，
    ///    是**缺口的一种形状**，不是能力。
    ///    ★ 当时的行为读数（devbox 沙箱现打）：父进程 38 个键 · 子进程 39 个
    ///    （= 38 全继承 + 1 个显式交办的）· 那两个键**都在**。
    ///
    /// ⇒ 今天 `invoke::run` 先 `env_clear()` 再按 `invoke::INHERITED_ENV_KEYS` 喂。
    ///    本格的 ③ 段因此**换了方向**：它此前断言「子进程的环境与宿主逐字相同」，
    ///    今天断言「**它是白名单 ∪ 显式交办的那几项，不多一个**」——
    ///    而「那两个键到底在不在子进程里」这条**行为**读数搬去了
    ///    [`a_plugin_started_here_never_sees_the_listen_port_or_token`]
    ///    （它要一个**父进程环境里真有那两个键**的进程，本格给不出）。
    ///
    /// ⇒ **正门仍然没开**（`KR26D4` 只写题面，不写代码）：真要给插件一条回调口，它该是
    ///    `invoke::run` 的**第五个入参**（一份「这次调用允许回调哪几条基础命令」的显式清单），
    ///    落地形态是把常驻口的地址与一枚**一次性、按调用发的、只授这几条命令**的短票
    ///    显式塞进子进程环境；ⓐ（清环境）已经做掉了，剩下的是
    ///    ⓑ 那份清单的取值空间钉在 `inbound::COMMANDS` 上（同 `Hello.unavailable` 的口径，
    ///    不许自造第二套词）。题面住计划仓 `features/K-R26-…md` 的 `§4`。
    #[test]
    fn nothing_here_hands_the_plugin_a_designed_way_back_into_the_host_commands() {
        // ① 通用调用口**说不出**宿主命令的名字。
        for (file, prod) in generic_port_production() {
            for needle in [
                format!("inbo{}", "und"),
                format!("COMM{}", "ANDS"),
                format!("REGIS{}", "TRY"),
            ] {
                assert!(
                    !prod.contains(needle.as_str()),
                    "`plugin/{file}` 的生产段里出现了 `{needle}` —— 通用调用口开始认识\
                     宿主的命令表了，那条「插件 → 宿主基础命令」的跳从此有了一半，\
                     而它没有协议、没有权限模型。先回定框。"
                );
            }
        }

        // ② 交给子进程的只有 argv / env / 关掉的 stdin —— 没有回程端点。
        let handed = crate::plugin::invoke::argv_for(
            Path::new("/opt/p/tool"),
            &["x"],
            5,
            Some(Path::new("/usr/bin/timeout")),
        );
        assert_eq!(
            handed.1,
            vec!["5".to_string(), "/opt/p/tool".to_string(), "x".to_string()],
            "argv 的形状变了 —— 本格据它说「argv 里没有回程端点」"
        );

        // ③ 那条路今天的**行为读数** —— 换了方向〔`K-R26` 09-05〕：
        //    此前断言「子进程的环境与宿主逐字相同」（暗路的证据），
        //    今天断言「子进程的键集 = 白名单 ∩ 宿主 ∪ 显式交办的那几项，**不多一个**」。
        //    `PATH` 仍然当证人：它在白名单里，所以**值**必须逐字相同 ——
        //    这一半买的是「那一趟真的 fork/exec 了」，与清不清环境无关。
        let root = build_fixture("inherit");
        let name = plugin_name();
        let (fixed, hint) = discovery_of(&root, &name);
        let bin = crate::plugin::discover::find(&name, &fixed, false, hint).expect("该找得到");
        // ⚠ 把 `NotRun` 那两支的原文带上：「参数太大」（自己能修）与「那个程序坏了」
        //（自己修不了）是两件事，合成一句「起不来」就是归错因（`invoke::NotRun` 的头注逐字）。
        let out = crate::plugin::invoke::run(&bin, &probe_argv(), DEADLINE_SECS, &[])
            .unwrap_or_else(|e| panic!("那个假插件没跑起来：{}", why_not_run(e)));
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let answer = crate::plugin::probe::negotiate(&text, &name, REQUIRED_CAPS)
            .unwrap_or_else(|e| panic!("协商没过：{}", e.message()));
        let child_path = answer
            .extras
            .iter()
            .find(|(k, _)| k == "inherited-path")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let host_path = std::env::var("PATH").unwrap_or_default();
        assert!(
            !host_path.is_empty() && !child_path.is_empty(),
            "两边有一边的 `PATH` 是空的 ⇒ 本段是空真：host={host_path:?} child={child_path:?}"
        );
        assert_eq!(
            child_path, host_path,
            "子进程看到的 `PATH` 与宿主不同 —— `PATH` 在 `invoke::INHERITED_ENV_KEYS` 里，\
             它的值必须原样过去。要么那一趟没真起进程，要么有人把它从白名单里摘了\
             （摘了它 `discover` 的兜底档就自相矛盾：找得到插件却跑不动）。"
        );
        // ③ 的另一半：源码面 —— 清环境这个动作**在**，而且只有一处。
        let invoke_prod = crate::guard_support::production_code(include_str!("plugin/invoke.rs"));
        assert_eq!(
            invoke_prod.matches("env_clear").count(),
            1,
            "`plugin/invoke.rs` 的生产段里 `env_clear` 不是恰好一处。\n\
             **没有** ⇒ 那条暗路又开了：子进程重新继承 daemon 的整份环境，\
             常驻口的地址与令牌跟着漏过去（本格头注 ② 段就是它的病历）。\n\
             **两处以上** ⇒ 先回答一句「哪一处是真的」—— 清两遍不会更干净，\
             只会让「白名单只有一个家」这句话开始漂。"
        );
        // 那两个会随环境一起漂过去的变量名，点住住址（不复述它们的值）。
        assert!(
            !crate::listen::ENV_PORT.is_empty() && !crate::listen::ENV_TOKEN.is_empty(),
            "常驻口那两个变量名空了 —— 本格头注 ② 段指的就是它们"
        );
        assert_ne!(crate::listen::ENV_PORT, crate::listen::ENV_TOKEN);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 六 · `K-R26`：那条暗路关掉了吗 —— **行为**读数，不是源码读数
    // ═══════════════════════════════════════════════════════════════════════

    /// 内层那条判据的**全名**（外层用 `--exact` 按它把测试二进制自己重新拉起来）。
    ///
    /// 写错了会**红而不会静默变绿**：内层一条都不跑 ⇒ 读数行不出现 ⇒ 外层当场红。
    const INHERIT_INNER_TEST: &str =
        "plugin_walk_fixture::tests::a_plugin_started_here_never_sees_the_listen_port_or_token_inner";

    /// 外层给内层打的记号。**没有它内层一条断言都不做** —— 那时它是被人手工
    /// `--ignored` 捞起来跑的，父进程环境里没有那两个键，断言会是**空真**。
    const INHERIT_MARK: &str = "PWF_ENV_INHERIT_CHILD";

    /// 内层把读数打在这个前缀后面；外层据它判「内层真的跑到了那几行断言」。
    const INHERIT_READING: &str = "PWF-ENV-READING";

    /// 喂给「daemon 侧那个进程」的两个**值**。
    ///
    /// ⚠ 只有值住这里 —— **键名一律现取** `listen::ENV_PORT` / `listen::ENV_TOKEN`，
    /// 手抄一份字面量的话，那两个常量改了名本格会**安静地**继续绿
    ///（它量的就变成「一个没人用的键没漏过去」）。
    const FAKE_PORT_VALUE: &str = "51999";
    const FAKE_TOKEN_VALUE: &str = "0123456789abcdef0123456789abcdef";

    /// `KR26D1`/`KR26D3` 正题：**被通用调用口起出来的插件，读不到常驻监听口的地址与令牌。**
    ///
    /// # 为什么要多一层进程（这一层不是排场）
    ///
    /// 要量的那件事是「**父进程环境里有的东西，会不会漏进子进程**」——
    /// 那就必须有一个**父进程环境里真有那两个键**的进程去调 `invoke::run`。
    /// 而本 crate 里三处头注逐字禁掉了另一条路：`std::env::set_var` 与并行跑的别的判据是**竞态**。
    /// ⇒ 唯一干净的形状是**把测试二进制自己重新拉起来**，用 `Command::env` 把那两个键
    ///    交给那个新进程（形状照 `relay::server::tests` 那台真子进程中转，同一条纪律）。
    ///
    /// ⇒ **分母说清楚**：内层那个进程**不是** `cargo test` 那个进程，
    ///    它是一个由 `Command` 起、环境里带着 `listen::ENV_PORT`/`ENV_TOKEN` 的新进程 ——
    ///    而**生产里 daemon 拿到那两个键的方式一模一样**（`listen.rs` 头注逐字：
    ///    token「只能由宿主生成、当 env 传进来」，daemon 自己造不出它）。
    ///    这就是它凭什么代表生产：**同一条投喂路，只是投喂的人换成了判据。**
    ///
    /// # ⚠ 本格量的是**行为**，不是「源码里有没有 `env_clear`」
    ///
    /// 源码面那一句（白名单只有一个家）另有一条判据。两条刻意分开：
    /// 合成一条的话，「白名单里被人塞进了那两个键」这种坏法会从**行为**这一侧溜过去。
    #[test]
    fn a_plugin_started_here_never_sees_the_listen_port_or_token() {
        let exe = std::env::current_exe().expect("测试二进制自己的路径");
        let out = std::process::Command::new(exe)
            .args([
                INHERIT_INNER_TEST,
                "--exact",
                "--ignored",
                // `--nocapture`：不给它，内层那行读数会被 libtest 收进自己的捕获，
                // 外层在管道上一个字节都读不到 ⇒ 下面「读数行在不在」这一格成空真。
                "--nocapture",
                "--test-threads=1",
            ])
            .env(INHERIT_MARK, "1")
            // ★ 键名现取，值是夹具的。
            .env(crate::listen::ENV_PORT, FAKE_PORT_VALUE)
            .env(crate::listen::ENV_TOKEN, FAKE_TOKEN_VALUE)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("起不来那个内层进程");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        // 读数原样抬进外层的输出：门禁日志里要看得见它（`brief` 15：写在源码注释里等于埋掉）。
        for line in text.lines().filter(|l| l.contains(INHERIT_READING)) {
            println!("  ⤷ {line}");
        }
        // ★ 反空真放在成败断言**之前**：内层若一条都没跑，下面那句「它绿了」毫无意义。
        assert!(
            text.contains(INHERIT_READING),
            "内层进程没打出那行读数 —— 它多半一条判据都没跑到\
             （`{INHERIT_INNER_TEST}` 这个全名写错了？`--ignored` 没生效？）。\n\
             内层的全部输出：\n{text}"
        );
        assert!(
            out.status.success(),
            "内层判据红了 —— **那正是这一格要报的读数**，原文照抄：\n{text}"
        );
    }

    /// [`a_plugin_started_here_never_sees_the_listen_port_or_token`] 的**内层**。
    ///
    /// 标 `#[ignore]` 是刻意的（同 `relay::server::tests` 那个入口）：它在正常那一趟里
    /// 一条断言都不跑，算成 `passed` 就是往门禁里塞一条恒绿的仪式 ⇒ 让它算 `ignored`。
    #[test]
    #[ignore = "内层入口：只在被外层用 PWF_ENV_INHERIT_CHILD 拉起时才做断言"]
    fn a_plugin_started_here_never_sees_the_listen_port_or_token_inner() {
        if std::env::var(INHERIT_MARK).is_err() {
            // 手工 `--ignored` 捞起来跑的那一趟：父进程里没有那两个键 ⇒ 断言会是空真。
            // 这里**不做断言也不打读数行** —— 「它到底跑没跑」由外层那条数读数行的断言看着。
            return;
        }
        let port_key = crate::listen::ENV_PORT;
        let token_key = crate::listen::ENV_TOKEN;

        // ── 分母①：本进程（扮演 daemon）的环境键 ────────────────────────────
        let parent: std::collections::BTreeSet<String> = std::env::vars_os()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(
            parent.contains(port_key) && parent.contains(token_key),
            "扮演 daemon 的这个进程环境里没有那两个键 ⇒ 下面每一句都是空真。\
             外层那两句 `.env(…)` 是不是掉了？本进程的键数={}",
            parent.len()
        );

        // ── 分母②：子进程真正拿到的那一份 ───────────────────────────────────
        let root = build_fixture("envroster");
        let name = plugin_name();
        let (fixed, hint) = discovery_of(&root, &name);
        let bin = crate::plugin::discover::find(&name, &fixed, false, hint).expect("该找得到");
        let out = crate::plugin::invoke::run(
            &bin,
            &[ROSTER_SUB],
            DEADLINE_SECS,
            // 调用方**显式**喂的那一项 —— 它照旧要活着（那不是继承，是交办）。
            &[(REALM_ENV, REALM_VALUE)],
        )
        .unwrap_or_else(|e| panic!("那个假插件没跑起来：{}", why_not_run(e)));
        let dumped = String::from_utf8_lossy(&out.stdout).into_owned();
        let _ = std::fs::remove_dir_all(&root);
        let child: std::collections::BTreeSet<String> = dumped
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.to_string()))
            .filter(|k| !k.is_empty())
            .collect();

        // ── 读数（两个分母 + 两个键在不在），一行打完 ────────────────────────
        println!(
            "{INHERIT_READING} 父进程键数={} · 子进程键数={} · 口在子进程里={} · \
             令牌在子进程里={} · 子进程键集={:?}",
            parent.len(),
            child.len(),
            child.contains(port_key),
            child.contains(token_key),
            child.iter().collect::<Vec<_>>()
        );

        // ── 反空真：子进程真的报回了一份非空的键集 ───────────────────────────
        assert!(
            child.contains("PATH"),
            "子进程报回的键集里连 `PATH` 都没有 ⇒ 它要么没跑起来、要么读不到 `/proc` —— \
             下面「那两个键不在里面」此刻是空真。报回来的原文：{dumped:?}"
        );
        assert!(
            child.contains(REALM_ENV),
            "调用方**显式**喂的那一项没到子进程手上 —— 那不是继承，是交办，\
             关暗路不许把它一起关掉。子进程键集={:?}",
            child.iter().collect::<Vec<_>>()
        );

        // ── 正题 ─────────────────────────────────────────────────────────────
        assert!(
            !child.contains(port_key),
            "常驻监听口的**地址**漏进了插件进程：`{port_key}`。\n\
             ⇒ 任何被 daemon 起的插件读一读自己的环境就能接上宿主 —— \
             那是一条没有协议、没有权限模型、没有审计的回程。\n\
             子进程键集={:?}",
            child.iter().collect::<Vec<_>>()
        );
        assert!(
            !child.contains(token_key),
            "常驻监听口的**令牌**漏进了插件进程：`{token_key}`。\n\
             ⇒ 接上之后连鉴权都过得去。子进程键集={:?}",
            child.iter().collect::<Vec<_>>()
        );

        // ── ★ 比「那两个键不在」更强的一句：**子进程的键集 ⊆ 白名单 ∪ 显式交办** ──
        //
        // 只断言那两个键的话，本格买到的是「**这两个**秘密没漏」；
        // 断言 ⊆ 之后买到的是「**任何**不在白名单里的键都漏不出去」——
        // 而后者才是 `env_clear` 那一刀真正的性质（明天多一个秘密键，本格照样咬得住）。
        let allowed: std::collections::BTreeSet<String> = crate::plugin::invoke::INHERITED_ENV_KEYS
            .iter()
            .map(|(k, _)| (*k).to_string())
            .chain(std::iter::once(REALM_ENV.to_string()))
            .collect();
        let leaked: Vec<&String> = child.difference(&allowed).collect();
        assert!(
            leaked.is_empty(),
            "这些键既不在 `invoke::INHERITED_ENV_KEYS` 里、也不是这次调用显式交办的，\
             却出现在插件进程的环境里：{leaked:?}\n\
             ⇒ `env_clear()` 那一刀漏了，或者有人往白名单里加了东西而没写为什么。\n\
             白名单今天 {} 个键（成员只住 `invoke.rs` 那一处，这里不复述）· \
             子进程键集={:?}",
            crate::plugin::invoke::INHERITED_ENV_KEYS.len(),
            child.iter().collect::<Vec<_>>()
        );
        // 反空真：白名单不许是空的（空表 + 空子进程环境 ⇒ 上面那句是 `[] ⊆ {}`）。
        assert!(
            !crate::plugin::invoke::INHERITED_ENV_KEYS.is_empty(),
            "白名单是空的 —— 上面那条 ⊆ 此刻在空转"
        );
        for (k, why) in crate::plugin::invoke::INHERITED_ENV_KEYS {
            assert!(
                !k.is_empty() && !why.is_empty(),
                "白名单里有一条没写为什么（或键是空的）：`{k}` —— \
                 `KR26D2` 逐字要求**逐键给论据，给不出的不进**"
            );
        }
    }

    /// `KW2E5`：**「加一个插件，宿主零改动」今天不成立 —— 差几处**（诚实差距读数）。
    ///
    /// # 🔴 本格**不宣布成功**，也**不许把这个数改小**
    ///
    /// 照 `S6` 那条决定性的理由：验收件报的那个数是用来评价 `K-W1A`/`K-W2D` 的；
    /// 本件若顺手去收接口，「前面几件把地基打成什么样」就再也没人量得出来。
    ///
    /// # 现打的落点清单（件文件 `§0e` 重打；条件性的那几行**逐条标出来**）
    ///
    /// | # | 落点 | 判据强制吗 | 什么条件下才轮到它 |
    /// |---|---|---|---|
    /// | 1 | `inbound::REGISTRY` 加一条 `CommandSpec` | ✅ 红（`COMMANDS`/`REGISTRY` 双向相等 + `protocol_doc_guard` 对拍） | **无条件** —— 插件要被宿主调，就得有一条命令 |
    /// | 2 | 协议文档对应锚点 | ✅ 红（`protocol_doc_guard` 认 `doc_anchor`） | 无条件（跟着 1） |
    /// | 3 | 新插件自己的适配层（码表 · 候选路径 · 必需清单） | ➖ **不算宿主改动** | `E6`/`E9` 明写它就该每插件一份 |
    /// | 4 | `layering_guard::ALLOWED_INTO_PLUGIN`（今天 **5 条**，条数钉死） | ✅ 红 | 🔴 **只在适配层住 `control/` 或 `observe/` 时** —— 那条判据逐字 `for layer in ["control", "observe"]` |
    /// | 5 | `readonly_guard::spawn_registry`（`SPAWN_SITES_TODAY` 今天 **9**，相等断言） | ✅ 红 | 🔴 **只在没有复用那唯一一处起进程口时**；`E5` 裁定复用 ⇒ 这个数**不许涨**（本件一处没动） |
    /// | 6 | `plugin::layer_guard::FILES_IN_THIS_LAYER`（逐个列名） | ✅ 红 | 🔴 **只在往 `plugin/` 里加文件时** —— 而按 `E6` 新插件的东西根本不该进那一层 |
    /// | 7 | `agent_boundary_guard::CORE_FILES` + 它的棘轮表 | ✅ 红 | 🔴 同 6（`FILES_IN_THIS_LAYER` 的诊断文案逐字要求**同轮**加） |
    /// | 8 | 新插件**自己那组专有针** | ❌ **没人管** | 无条件 —— `cc_bus_boundary_guard` 那 5 根针是那一族的专有词，对新插件零覆盖 |
    ///
    /// **⇒ 两个数，各带条件（这才是诚实的形状，一个数说不清）**：
    /// · **无条件**要动宿主的：**2 处**（1、2）+ **1 处没人管**（8）；
    /// · 加上「适配层住 `control/`」这个今天唯一的先例：**3 处**（1、2、4）+ 1 处没人管；
    /// · 件文件 `§0e` 那个「**7 处宿主落点、6 处判据强制、1 处没人管**」的粗读数
    ///   把 4/5/6/7 一律算成无条件 —— 本件重打之后判：**4 是条件性的**（人群只有两层）、
    ///   **5 今天按 `E5` 不该涨**、**6/7 按 `E6` 根本不该轮到**。
    ///   ⚠ 这**不是**把差距改小：无条件那两处一处没少，而 8 那一格仍然没人管；
    ///   变的是「哪几处是真的每次都要动」。
    ///
    /// **本件自己付了几处**（一个真实数据点）：**1 处** —— `main.rs` 那一行 `mod`（PM 预批）。
    /// 而这 1 处买到的东西**不含**跳①③⑨：本夹具进不了真 `Hello`、进不了 `dispatch`。
    ///
    /// # ⚠ 第 8 行 PM 特别点名：本夹具**要不要**自己带一组专有针
    ///
    /// **带了。** [`the_generic_port_never_learns_this_fixtures_vocabulary`] 就是它 ——
    /// 分母是本夹具自己那张词表，与那一族的 5 根针**零重叠**。
    /// ⚠ 但**说清它买到的是什么**：它只守「本夹具的词不许漏进通用层」，
    /// **不守**「所有将来的插件的词」——`E6` 要求的是「每加一个插件要加它自己那组专有针」，
    /// 而**没有任何机检能逼下一个人加**。那一格今天仍然没人管，第 8 行**不许划掉**。
    #[test]
    fn adding_a_plugin_still_costs_the_host_something() {
        // 落点 1/2 的机检**今天真在**：命令表两侧互为镜子，且每条转调型命令都带文档锚点。
        assert_eq!(
            crate::inbound::COMMANDS.len(),
            crate::inbound::REGISTRY.len(),
            "命令表两侧的条数对不上 —— 落点 1 那道机检此刻自己就是红的"
        );
        let owners: Vec<&crate::inbound::CommandSpec> = crate::inbound::REGISTRY
            .iter()
            .filter(|s| s.codes.contains(&REGISTRY_OWNED_CODE))
            .collect();
        assert!(
            !owners.is_empty(),
            "没有任何一条命令登记那个词 —— 落点 1/2 的读数此刻量不出来"
        );
        for s in &owners {
            assert!(
                s.doc_anchor.is_some(),
                "转调插件的命令 `{}` 没有文档锚点 —— 落点 2 那道对拍对它是空的",
                s.name
            );
        }

        // 落点 8：那一族的专有针对本夹具**零覆盖** —— 这一格是本行「没人管」的证据。
        //
        // ⚠ 分母：`cc_bus_boundary_guard` 那 5 根针的**词**住它自己那份源码里，
        //   这里不复述成第二份字面量（`brief` 13b）—— 只断言「它的语料里一个本夹具的词都没有」。
        let their_guard =
            crate::guard_support::production_code(include_str!("cc_bus_boundary_guard.rs"));
        let vocab = fixture_vocabulary();
        let covered: Vec<&str> = vocab
            .iter()
            .map(|(w, _)| w.as_str())
            .filter(|w| their_guard.contains(*w))
            .collect();
        assert!(
            covered.is_empty(),
            "那一族的边界判据里出现了本夹具的词 {covered:?} —— \
             那样「专有针对新插件零覆盖」这条读数就变了，落点 8 的说法要同轮改"
        );

        // 落点 5：`E5` 裁定「插件调用复用那一处口」⇒ 本件一处起进程点都不许加。
        // 生产段那个数由 `readonly_guard::spawn_registry` 的相等断言看着（本件没动它）；
        // 这里只钉本文件这一侧：本文件的**生产段**里一个 `Command` 都没有。
        let own_prod = crate::guard_support::production_code(own_source());
        assert!(
            !own_prod.contains("Command::"),
            "本文件的生产段里出现了起进程的形 —— 那会让 `SPAWN_SITES_TODAY` 那个数涨一格"
        );

        // 本件自己付的那 1 处：`main.rs` 的一行 `mod`（而**不是**两处、也不是零处）。
        let main_rs = std::fs::read_to_string(src_root().join("main.rs")).expect("读 main.rs");
        let decl = format!("mod plugin_walk_{};", "fixture");
        assert_eq!(
            main_rs.matches(decl.as_str()).count(),
            1,
            "本件付给宿主的那一处落点不是恰好一处"
        );
    }
}
