fn probe_cfg() -> crate::ssh_source::RemoteConfig {
    crate::ssh_source::RemoteConfig {
        host: "这个主机一定不存在-audit0805".into(),
        label: "probe".into(),
        port: 1,
        user: "nobody".into(),
        key_path: None,
        backend_path: "/tmp/nope".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

// 〔RW1 · 第四波 09-24〕这里原来是「删远端文件的入口真的过了围栏吗」（喂 `/etc/passwd` 给 SFTP 直删、
//   要求零网络就被结构守卫拒）。F11 改经远端后端删（`files-delete-session`，只收 sid）之后，
//   那条 SFTP 直删与它的守卫一起走了；「只收 sid · 落点由后端按 sid 找」的判据住后端。

/// ★ 第二道围栏（canonicalize 之后）与卸载路的围栏：**源码层**判据。
///
/// ⚠ 跑不了真路（要先连上远端 / 红线不许起真连接）⇒ 只判「那行还在」。
/// **判源码是代理不是标的**（F41 记过）：挡得住「短路 / 删掉」，
/// 挡不住「围栏还在但被喂了洗过的路径」。后者进 `ROADMAP §5`。
#[test]
fn both_remote_path_sinks_still_ask_their_fence() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    for (f, fence) in [
        ("uninstall_remote_backend", "is_safe_remote_backend_path"),
        // 〔RW1 · 第四波 09-24〕`remove_remote_file` 那一行随 F11 改经后端删走了。
    ] {
        let at = prod
            .find(&format!("fn {f}"))
            .unwrap_or_else(|| panic!("生产段里没有 `{f}` —— 抽取器坏了，本条此刻无效"));
        let mut body = Vec::new();
        for (i, line) in prod[at..].lines().enumerate() {
            let cont = line.starts_with("where") || line.starts_with(')') || line.trim() == "{";
            if i > 0 && !line.is_empty() && !line.starts_with(char::is_whitespace) && !cont {
                break;
            }
            body.push(line);
        }
        let body = body.join("\n");
        // ⚠ **整行形状**，不是「提到过」。第一版写 `contains("!{fence}(")`，
        //   于是 `if false && !is_safe_…(…)` 这种短路**照样绿** —— 变异当场证伪。
        //   F24 那一族：我要的事实是「围栏在做判定」，而我匹配了「围栏出现过」。
        // ⚠ **每一处调用都必须是判定行**，不是「有一处就行」。
        //   `remove_remote_file` 是**双重守卫**（canonicalize 前后各一道）；
        //   第一版写 `.any(...)`，于是短路其中一道、另一道还在 ⇒ 照样绿。
        //   **同一条判据在同一轮里被变异证伪两次**（先是「提到过 vs 在判定」，
        //   再是「有一处 vs 每一处」）—— 记在这里，因为两次都是我先写完才发现的。
        let calls = body
            .lines()
            .filter(|l| l.contains(&format!("{fence}(")))
            .count();
        let gates = body
            .lines()
            .filter(|l| l.trim().starts_with(&format!("if !{fence}(")))
            .count();
        assert!(
            calls >= 1 && gates == calls,
            "`{f}` 里 `{fence}` 被调 {calls} 次，其中只有 {gates} 次是判定行。\n\
                 围栏要么被删了，要么被短路了\n\
                 （`if false && !…` 这种改法留着调用、却不再判定）。\n\
                 它下一步会去删用户远端机器上的文件。\n\
                 ⚠ 本条只看「那一行的形状」（源码层，理由见头注）：\n\
                 挡得住删除与短路，**挡不住**「围栏还在但被喂了洗过的路径」。"
        );
    }
}
use super::*;

/// 单一来源漂移守卫②：部署为远端 `~/.local/bin/ccm` 的 **CLI 本体**。
///
/// 这些不是"要素清单"而是**血的教训清单**，每条对应一个真实踩过的坑：
///  - `=%s:` / `=$` ：tmux `-t` 必须精确匹配（INVARIANTS §31a）。裸目标会杀错/打错兄弟会话；
///    `=名` 无尾冒号则在 send-keys/capture-pane/set-option 上 rc=1 完全失效。
///  - `exec` ：不能省。⚠ **理由在 `U-NP④`（08-14）之后换了一条**：旧理由是
///    「身份 poller 读 `sessions/$PID.json`，不 exec 则 PID 对不上」，而那条 poller 已删
///    （身份改由后端打，认的是 pidfile 自己的名字 = claude 的 PID）。今天留着它的
///    理由是「不在 agent 与终端之间多一层 shell」＋ 本 needle 本身就是部署契约。
///  - `@ccm_sid` / `@ccm_agent` ：身份随行，cc-monitor 靠它精确认会话。
///  - `@ccm_sid_expect` ：F04——通道A（建时/exec 时立即声明"打算跑这个 sid"）写这个 key，
///    与通道B（独立读会话文件确认后才写的 `@ccm_sid`；`U-NP④` 之后由 **backend** 写）分离。破坏性动作只认 `@ccm_sid`，
///    不被"声明了但从未真正跑起来"的会话骗过（旧审计 D6 的坑）。
///
///    **R09 复核订正（2026-07-28）——这条分离的作用域是「`shared/ccm` 内部」，不是全仓。**
///    此前多处写作"通道B 是 `@ccm_sid` 唯一写者"，那句话不准确：全仓有**两个**写者。
///    另一个是 `src/session-backend.ts::TMUX_BACKEND.createRunAttach`——**兜底渲染器**
///    自己拼 tmux 命令时，在 create 分支**直写裸 `@ccm_sid`**。
///
///    那不是漏改，是 F04 Phase B 方案A 的明确取舍：兜底路径**不经 ccm**，
///    因而没有"意图→事实"的提升机制；若那里改写 `@ccm_sid_expect`，这个 key 将
///    **永远不会被提升**，于是 Gate 2 的 `@ccm_sid` 半支永久判不出它 → 该会话变得不可 kill
///    （向后兼容回归，正是 §5.1 第 3 条要防的）。所以两侧**故意写不同的 key**。
///
///    两个方向相反的断言各自被钉住，别把任一侧"改成一致"：
///      · 本函数下方的 needle 扫描：`shared/ccm` **必须**写 `@ccm_sid_expect`；
///      · `tests/session-backend.test.ts`（"#72 + F03.4甲′"那条黄金串）：兜底渲染器
///        **必须**写裸 `@ccm_sid`。已实测：把兜底侧改成 `_expect` 会让后者转红。
///    **成功标准④ 不受此例外影响**——终端起会话那条路径的意图声明全程在 `shared/ccm` 内
///    （写 expect；事实由后端提升，见 `U-NP④`），与兜底渲染器无交集。
///  - `CLAUDE_CONFIG_DIR` ：账号注入必须在**最终 exec 的那个 shell 里**设。
///  - `--print` / `--ccm-probe` ：F03 的渲染等价断言 + 安装自检/降级判据依赖它们。
#[test]
fn ccm_cli_has_required_elements() {
    // U1a（2026-08-01）：三张表 + `-t` 扫描口径搬进 `crate::ccm_cli_contract`。
    // **判据一条没改、没加、没减** —— 搬出去只是为了让 U9 迁到 `control/` 时
    // 改的是「喂哪份脚本文本」，而不是把这些断言重写一遍（账本 S11：迁移是强度
    // 悄悄下降的经典时机）。强度读数的基线对拍在那个模块的
    // `ccm_cli_strength_is_at_or_above_baseline` 〔散文墓碑〕。
    use crate::ccm_cli_contract as contract;

    // 🔴 〔`K-R48` 第二拍 09-11〕**这里原来还有五段断言，全部打在 `CCM_CLI_SCRIPT` 上，
    //    随 `shared/ccm` 一起删了**：住址账本两条循环（`ledger.needles` / `ledger.channel_a`）·
    //    `pin_t_def` 〔散文墓碑〕（`$t` 只许被赋值一次）· `scan_t_targets(...).require(floor, …)`
    //    （tmux 目标必须是 `=名:` 形态，`INVARIANTS §31a`）。
    //    它们量的全是「**那个 bash 脚本怎么写的**」，被测对象没了就没了。
    //
    // ⚠ **它们守的性质没有一条被丢掉，逐条给新住址**：
    //    · `=名:` 精确目标 ⇒ `control::ccm::plan` 的渲染判据（`--print` 黄金串里每个
    //      `-t` 都是 `'=名:'`，变异刀 #8「attach 目标退回裸名字」当场红）;
    //    · 通道A 写**意图**标记 `@ccm_sid_expect` ⇒ 变异刀 #7「写事实标记而非意图标记」;
    //    · `$t` 不许二次赋值 ⇒ 那是 bash 变量的病，Rust 里没有那个形状（`Plan` 里是字段）。
    //    ⚠ 「跨语言那一半」（TS 侧 `deriveTmuxName` 对拍 · `capabilities ⊇ CLI_REQUIRED_CAPS`）
    //      仍**只**住 e2e（`ccm-cli.test.sh` 5 条 · `ccm-contract-parity.sh` 5 条），别当 Rust 判据能顶。
}

/// `K-R48` 第二拍：远端 `~/.local/bin/ccm` 今天是**入口**，不是实现。
///
/// 🔴 **这一条的岗位是「别让它长回去」**：`K33` 逐字「所有命令只许有一处，其他都是
/// 根据传参来调用」。一个 shim 里只要出现第二个分支，那句话就又破了 ——
/// 而破的时候没有任何别的判据会出声（它不进任何 e2e，没有一台真远端可跑）。
#[test]
fn the_remote_ccm_entry_is_an_entry_not_an_implementation() {
    let shim = ccm_entry_shim("/home/pi/.cc-monitor/bin/cc-monitor-backend");
    // ① 真的把 argv 转给后端，且走的是 `intercept` 的第二条入口（子命令形）。
    assert!(
        shim.contains("exec '/home/pi/.cc-monitor/bin/cc-monitor-backend' ccm \"$@\"")
            || shim.contains("exec /home/pi/.cc-monitor/bin/cc-monitor-backend ccm \"$@\""),
        "shim 没把 argv 原样转给后端的 `ccm` 子命令：\n{shim}"
    );
    // ② **零实现**：除了 shebang、一行注释、一行 exec，不许有别的可执行行。
    let code: Vec<&str> = shim
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    assert_eq!(
        code.len(),
        1,
        "远端 ccm 入口里出现了第二条可执行语句 —— 那就是第二处实现了（K33）。\n\
             它只许有一行 `exec <后端> ccm \"$@\"`。现打：{code:?}"
    );
    // 唯一那一行必须**就是一次 exec**（不许起子进程再包一层：那会吃掉退出码与信号）。
    //
    // 🔴 09-15 放宽过这一条的**形状**（允许 `exec` 前挂「一次性环境变量赋值」前缀），
    // 为的是那一行 `CCM_SELF=…`。〔MC1 · 2026-09-24〕那个变量删了、shim 回到一行裸 `exec`；
    // 放宽的那一格留着不收（它挡的「吃掉退出码 / 第二处实现」下面三条照挡），
    // 「一个环境变量都不许设」由 `local_backend_tests.rs::remote_shim_sets_no_environment_of_its_own` 钉。
    let head = code[0];
    let after_assigns = head
        .split_whitespace()
        .skip_while(|w| {
            w.split_once('=').is_some_and(|(n, _)| {
                !n.is_empty() && n.chars().all(|c| c.is_ascii_uppercase() || c == '_')
            })
        })
        .next()
        .unwrap_or("");
    assert_eq!(
        after_assigns, "exec",
        "唯一那一行不是「（可选的大写环境变量赋值）+ `exec`」——\
             起子进程再包一层会吃掉退出码与信号。现打：{head}"
    );
    assert_eq!(
        head.matches("exec ").count(),
        1,
        "出现了不止一次 `exec` —— 那不再是「转交」而是逻辑。现打：{head}"
    );
    for forbidden in ["$(", "`", ";", "&&", "||", "|", "if ", "case "] {
        assert!(
            !head.contains(forbidden),
            "唯一那一行里出现了 `{forbidden}` —— shim 长出了第二处实现（K33）。现打：{head}"
        );
    }
    // ③ 路径必须经 POSIX quote（backend_path 是用户填的，可能带空格 / 引号）。
    let tricky = ccm_entry_shim("/home/用户/带 空格/it's");
    assert!(
        tricky.contains(&shell_quote_core::posix_quote("/home/用户/带 空格/it's")),
        "backend_path 没经 `shell_quote_core::posix_quote` —— 带空格的路径会被拆成两个词。\n{tricky}"
    );
}

/// F08b：仅当交叉编译产物已放进 embedded-backends/（build.rs 置了 `embedded_backends` cfg）
/// 才编译/运行——证实内嵌真生效：Linux 两格取到 ELF 二进制 + build_id 非空。CI 无二进制时
/// 本测试被 cfg 掉，不误报。〔DP1〕取字节口是 `byte_table::pick`（按 (OS, arch)）。
#[cfg(embedded_backends)]
#[test]
fn embedded_backend_binaries_present_and_valid() {
    use crate::byte_table::{key_of, pick, Product};
    for arch in ["x86_64", "aarch64"] {
        let key = key_of("Linux", arch).expect("表 A 认得这一格");
        let bin = pick(Product::Backend, key).expect("内嵌二进制应存在");
        assert!(
            !bin.build_id.unwrap_or_default().is_empty(),
            "build_id 非空"
        );
        assert_eq!(&bin.bytes[..4], b"\x7fELF", "{arch} 应是 ELF");
        assert!(bin.bytes.len() > 100_000, "{arch} 体积应非平凡");
    }
    assert!(key_of("Linux", "riscv64").is_err(), "未知 arch → 表外");
}

/// 🔴 `K-R70`：**那道身份见证真的会咬人** —— 四格（纯函数，不依赖内嵌产物在不在）。
///
/// ⚠ 这一条与上面那条判据分工：那条钉**接线**（有没有无条件跑），这条钉**行为**
/// （跑了会不会说真话）。少任何一条，另一条都能被一个恒答 `true` 的实现骗过去。
#[test]
fn the_build_stamp_witness_actually_bites() {
    let (o, c) = (env!("BACKEND_STAMP_OPEN"), env!("BACKEND_STAMP_CLOSE"));
    let real = format!("头部随便什么{o}p9-sample{c}尾部随便什么");
    assert!(
        super::bytes_carry_build_stamp(real.as_bytes(), "p9-sample"),
        "带着自己那个戳的字节被判「问不出身份」—— 见证会误拒正品"
    );
    assert!(
        !super::bytes_carry_build_stamp(real.as_bytes(), "p9-other"),
        "戳写着 `p9-sample` 而问它是不是 `p9-other`，它答了「是」—— 见证形同虚设"
    );
    assert!(
        !super::bytes_carry_build_stamp(b"no stamp at all", "p9-sample"),
        "一段没有戳的字节被判「身份可信」—— 那正是老启发式的失效面"
    );
    assert!(
        !super::bytes_carry_build_stamp(real.as_bytes(), ""),
        "空身份必须判假：空串会让「戳」退化成两个界标挨着，而那一形是噪音不是身份"
    );
    // ⚠ 反向自检：**戳不是随便一处提到 build_id 就算**。
    //   旧启发式 `bytes_contain(bytes, build_id)` 会被裸出现的 id 喂饱 —— 而 backend
    //   的 hello 帧里本来就带着这个串 ⇒ 那条判据在任何一份后端上都恒真。
    assert!(
        !super::bytes_carry_build_stamp(b"...p9-sample...", "p9-sample"),
        "裸出现一次 id 就被当成身份戳 —— 那退回了 `K-R70` 之前那条恒真的启发式"
    );
}

/// ★★ 🔴 `K-R70`（09-12）：**内嵌那份的身份只许来自它自己的字节。**
///
/// # 它取代了什么，以及为什么不是「换个写法」
///
/// 〔散文墓碑〕〔本条原名 `the_identity_witness_is_derived_from_the_manifest_not_written_by_hand`，
///  钉的是那个见证布尔 `id_from_manifest` 只能由「那份清单在不在」推出来、不许写死 `true`
///  （写死会让 `deploy_embedded_backend` 里那道 `bytes_contain` 兜底整个跳过；
///   08-08 实测写死 x86_64 那处，monitor 1004 一条都不红）。
///  **它守的动作是对的，守的东西是错的**：那个「见证」见证的是**一份旁挂清单在不在**，
///  而清单是 `release.yml` 从源码常量 `const BUILD_ID` 抠出来写的 ——
///  三个载体的清单**恒等**，恒等的东西一格证据都不提供
///  （`K-R68` 摸底 · `DECISIONS.md#R26` 裁定零：那是把标签当成了指纹）。〕
///
/// 今天身份**只有一条来路**：`build.rs` 从二进制字节里扫 `CC_MONITOR_BUILD_STAMP`。
/// 于是本条钉三件事：
///
/// 1. 两个 `BackendBinary` 的 `build_id` **只许**是 `env!("BACKEND_EMBEDDED_ID_<ARCH>")`
///    —— 出现字面量、或退回源码 id（老 `pick()` 那条「问不出就拿源码顶上」的路）都红；
/// 2. 部署路上**真的**跑了 [`bytes_carry_build_stamp`]，而且**不带前置条件**
///    （老写法 `!bin.id_from_manifest && …` 正是「有清单就整个跳过」）；
/// 3. 界标那两个字面量**不许**在本文件里出现第二份（闭集唯一住址在后端源码）。
///
/// 顺带钉住 arch 那条跨文件契约的**另一半**：`build.rs` 期待的每个 arch，
/// 〔DP1〕`byte_table.rs` 里都必须真有一槽（漏一个 ⇒ 取字节口对它返回 `None`，
/// 远端自动部署对那个 arch **悄悄关闭** —— 与上一条判据守的是同一个事故形状的两端）。
#[test]
fn the_embedded_identity_comes_from_the_bytes_not_from_a_label() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join("src/sftp.rs")).expect("读不到 sftp.rs");
    let prod = guard_core::production_code(&src);
    // 〔DP1 · 第四波〕两份 musl 的槽与它们的身份取值口搬进了 `byte_table.rs`（全仓唯一的取字节口）；
    //   「出门前那道见证」仍在本文件（部署路）。①④ 读那一份，② 读这一份，③ 两份都读。
    let table_src =
        std::fs::read_to_string(root.join("src/byte_table.rs")).expect("读不到 byte_table.rs");
    let table = guard_core::production_code(&table_src);
    // 运行时拼，免得命中本条自己的说明文字。
    let field = format!("{}_id:", "build");
    let inits: Vec<&str> = table
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with(&field) && l.contains("env!"))
        .collect();
    assert_eq!(
        inits.len(),
        2,
        "生产段里找到 {} 处 `{field}` 的 env 取值（应当 2：X86 / ARM）—— \
             抽取器坏了或那两个 static 被改写了，本条会零命中地绿：{inits:?}",
        inits.len()
    );
    for l in &inits {
        assert!(
            l.contains("env!(\"BACKEND_EMBEDDED_ID_"),
            "这一格的身份不是从**字节**来的：{l}\n\
                 ★ 只有 `BACKEND_EMBEDDED_ID_<ARCH>` 是 `build.rs` 从这份二进制的字节里\n\
                 扫出来的（`CC_MONITOR_BUILD_STAMP`）。退回 `BACKEND_BUILD_ID`（源码 id）\n\
                 就是「问不出就拿源码的答案顶上」—— 把一个失败面换成一个假答案；\n\
                 写一份 `.build_id` 旁文件再读它，是把标签换个地方抄（`KR70D1` 逐字点名的失效方向）。"
        );
    }
    // ② 部署路真的跑了那道见证，而且**不带前置条件**。
    let witness = format!("{}_carry_build_stamp(", "bytes");
    let calls: Vec<&str> = prod
        .lines()
        .map(str::trim)
        .filter(|l| l.contains(&witness) && !l.starts_with("pub fn"))
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "生产段里 `{witness}` 的调用处有 {} 个（应当恰好 1：`deploy_embedded_backend` 出门前那一道）：{calls:?}",
        calls.len()
    );
    assert_eq!(
        calls[0], "if !bytes_carry_build_stamp(bin.bytes, bin.build_id) {",
        "那道见证被加了前置条件或换了形状：{}\n\
             ★ 老写法 `!bin.id_from_manifest && !bytes_contain(…)` 的病就在前半句：\n\
             **有清单时整道闸跳过**，而会出事的那一形（有人塞了别的字节、清单照旧）\n\
             恰恰在那一支里。⇒ 它必须无条件跑。",
        calls[0]
    );
    // ③ 界标闭集只有一个住址（在后端源码里），本文件只许 `env!` 取。
    for mark in [env!("BACKEND_STAMP_OPEN"), env!("BACKEND_STAMP_CLOSE")] {
        assert!(
            !table.contains(&format!("\"{mark}\"")),
            "byte_table.rs 生产段里出现了界标字面量 `{mark}`"
        );
        assert!(
            !mark.is_empty(),
            "`BACKEND_STAMP_OPEN/CLOSE` 是空串 —— `build.rs` 从后端源码抠界标失败了，\n\
                 而空界标会让 `bytes_carry_build_stamp` 恒答 false ⇒ 自动部署整个静默关闭。"
        );
        assert!(
            !prod.contains(&format!("\"{mark}\"")),
            "本文件生产段里出现了界标字面量 `{mark}` —— 闭集唯一住址在\n\
                 `src/backend/main.rs`（`BUILD_STAMP_OPEN`/`CLOSE`），\n\
                 这里只许 `env!(\"BACKEND_STAMP_OPEN\")` / `env!(\"BACKEND_STAMP_CLOSE\")` 取。"
        );
    }

    // 跨文件契约的另一半：`build.rs` 期待的每个 arch，这里都要真有一份。
    let build_rs = std::fs::read_to_string(root.join("build.rs")).expect("读不到 build.rs");
    let arches: Vec<String> = build_rs
        .lines()
        .find_map(|l| {
            let rest = l.trim().strip_prefix("for arch in [")?;
            Some(
                rest.trim_end_matches(|c| c == '{' || c == ' ' || c == ']')
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| {
            panic!("`build.rs` 里找不到 `for arch in [...]` —— 与上一条判据同一个锚点，一起修")
        });
    assert!(
        arches.len() >= 2,
        "从 `build.rs` 只抠到 {} 个 arch",
        arches.len()
    );
    for arch in &arches {
        assert!(
            table.contains(&format!("BACKEND_EMBEDDED_ID_{}", arch.to_uppercase())),
            "`build.rs` 会为 `{arch}` 嵌入二进制并发 `BACKEND_EMBEDDED_ID_{}`，\n\
                 而 `byte_table.rs` 生产段里没有对应的那一槽 ⇒ 取字节口对 (Linux, {arch}) 返回 `None`，\n\
                 **远端自动部署对这个 arch 悄悄关闭**（`build.rs` 那侧只 `cargo:warning=`，不会红）。\n\
                 与「发版流水线要为每个 arch 备料」那条守的是同一个事故形状的两端。",
            arch.to_uppercase()
        );
    }
}

/// ★★ **发版流水线必须为 `build.rs` 期待的每一个 arch 都备好料**〔audit-0805 08-08〕。
///
/// # 缺一个 arch 的后果是**静默的**，而且已经出货过
///
/// `build.rs` 的 `embed_backends` 缺件时只 `cargo:warning=`（**不是 error**）：
///
/// > 缺少内嵌 backend {arch} —— 远端自动部署将关闭
///
/// 而它旁边的注释逐字记着这条路的历史：「原来这里**连 warn 都没有** —— 缺二进制就
/// 静默不置 cfg、取字节那一口返回 None、远端自动部署整个消失而无人知晓。
/// **那正是 v2.19–v2.22 那批安装包的事故形状**」。
///
/// 警告是**刻意**的（本机开发树本来就常常只有一个 arch —— 今天就是：
/// `embedded-backends/` 里只有 x86_64）。⇒ **保证「出货的那份两个 arch 都在」的，
/// 只剩 `release.yml` 一处**，而在本条之前没有任何判据读它那几行。
///
/// # 人群从 `build.rs` 派生
///
/// 不手写 `["x86_64", "aarch64"]`（隔壁 `embedded_backend_binaries_present_and_valid`
/// 就是手写的，而且它带 `#[cfg(embedded_backends)]` —— 本机缺一个 arch 时**整条不编译**，
/// 平时没人走）。这里读 `build.rs` 那个 `for arch in [...]`：**谁将来加第三个 arch，
/// 本条当天就会要求流水线跟上**。
#[test]
fn the_release_pipeline_stages_every_arch_that_build_rs_embeds() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let build_rs = std::fs::read_to_string(root.join("build.rs")).expect("读不到 build.rs");
    // ⚠ **只看非注释行**〔08-08 变异逼出来的〕：第一版读原文，于是把那行
    // `cargo zigbuild --target aarch64-…` **注释掉**，本条照样绿 —— 它命中的是
    // 那行注释自己。「判据看的是围栏，还是围栏的说明书」，本会话第三次。
    let rel = guard_core::strip_hash_comment_lines(
        // 〔搬树 2026-09-17〕`root` 是 crate 根（`<repo>/src/bridge`）⇒ 再爬**一级**
        // 只到 `<repo>/src`。仓根要爬两级，走唯一住址。
        &std::fs::read_to_string(
            crate::guard_support::repo_root().join(".github/workflows/release.yml"),
        )
        .expect("读不到 release.yml"),
    );

    // 人群：`embed_backends` 里那个 `for arch in [...]`。
    let arches: Vec<String> = build_rs
        .lines()
        .find_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix("for arch in [")?;
            Some(
                rest.trim_end_matches(|c| c == '{' || c == ' ' || c == ']')
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| {
            panic!("`build.rs` 里找不到 `for arch in [...]` —— 写法变了，本条会零命中地绿")
        });
    // 抽取器自检：抠不到就别拿一个空表去「全部通过」。
    assert!(
        arches.len() >= 2,
        "从 `build.rs` 只抠到 {} 个 arch（08-08 实测 2：x86_64 / aarch64）—— 抽取器坏了",
        arches.len()
    );
    assert!(
        rel.lines().count() >= 100,
        "`release.yml` 只剩 {} 行 —— 读法坏了或流水线被掏空",
        rel.lines().count()
    );

    for arch in &arches {
        for (needle, why) in [
            (
                format!("--target {arch}-unknown-linux-musl"),
                "没有为这个 arch 交叉编译",
            ),
            (
                format!("staged/cc-monitor-backend-{arch}"),
                "编了但没按 `build.rs` 期待的名字放进 staged/",
            ),
            // 🔴 〔`K-R70` 09-12〕这里原来还有第三条：`staged/cc-monitor-backend-<arch>.build_id`，
            //    理由逐字「少了旁挂的 .build_id 清单（没有它，运行时只能回退到会误拒正品的启发式）」。
            //    **那条清单没有了**（它是从源码常量抠出来的标签，不是指纹 ——
            //    `K-R68` · `DECISIONS.md#R26` 裁定零），身份改从字节里扫。
            //    ⇒ 接替它的不是一条**按 arch** 的判据（校验那一步是 `foreach ($a in …)`，
            //      路径里带的是变量不是字面 arch，按 arch 去 grep 只会零命中地红），
            //      而是这个 arch 出现在那个 `foreach` 的清单里 ＋ 循环外那两条（见下）。
            (
                format!("\"{arch}\""),
                "这个 arch 不在校验那一步的 `foreach` 清单里 ⇒ 它的字节**没有人问过身份**",
            ),
        ] {
            assert!(
                rel.contains(&needle),
                "`release.yml` 里找不到 `{needle}` —— {why}。\n\
                     ★ 后果是**静默的**：`build.rs` 缺件时只 `cargo:warning=`（刻意如此，\n\
                     因为本机开发树常常只有一个 arch），于是**安装包照出，只是远端自动部署\n\
                     对这个 arch 悄悄关闭** —— v2.19–v2.22 那批安装包就是这个形状。\n\
                     ⚠ 人群是从 `build.rs` 的 `for arch in [...]` 派生的：要么让流水线跟上，\n\
                     要么先把那一行改掉（改它会逼你想清楚「不再支持这个 arch」这件事）。"
            );
        }
    }

    // ── 🔴 〔`K-R70` 09-12〕**流水线真的去问过那份字节** ─────────────────────
    //
    // 上面那条按 arch 的清单只买到「它在校验的名单里」；这两条买的是**校验本身还在**。
    // 两个锚各自不可替代：
    //   · `ReadAllBytes` —— 它**真的把那份二进制读进来了**（不是 stat、不是读旁边的谁）；
    //   · `const BUILD_STAMP_OPEN` —— 界标是**从后端源码抠的**，不是在 yml 里手抄一份
    //     （手抄那一份哪天与源码漂开，校验会以「假红」的形式提醒错人）。
    for (needle, why) in [
        (
            "ReadAllBytes",
            "校验那一步没有把二进制的字节读进来 —— 那它验的就不是这份字节，\
                 而是它旁边的某个文件（`K-R70` 整件治的就是这个）",
        ),
        (
            "const BUILD_STAMP_OPEN",
            "身份戳的界标不是从后端源码抠的 —— 手抄一份就多一个会漂的住址；\
                 漂开那天校验会红，而红的原因与真病无关",
        ),
    ] {
        assert!(
            rel.contains(needle),
            "`release.yml` 里找不到 `{needle}` —— {why}。\n\
                 ⚠ 后果与下面 `if-no-files-found` 那条同族：**静默** —— \n\
                 安装包照出，只是内嵌的那份没人问过它是谁。"
        );
    }

    // fail-closed 那一半：一个都没 stage 到时，上传步骤必须当场失败而不是传个空包。
    assert!(
        rel.contains("if-no-files-found: error"),
        "上传 `embedded-backends` 的那一步没有 `if-no-files-found: error` —— \n\
             staged/ 空了它会**成功地上传一个空 artifact**，下游 job 下载到空目录，\n\
             最后出的安装包不带任何内嵌后端。这正是本条要挡的那个事故的上游一环。"
    );
}

#[test]
fn deploy_decision_truth_table() {
    // 无标记 → 部署
    assert!(matches!(
        deploy_decision(None, "p1b-overflow"),
        DeployAction::Deploy(_)
    ));
    // 版本不符 → 部署
    assert!(matches!(
        deploy_decision(Some("p1a-history"), "p1b-overflow"),
        DeployAction::Deploy(_)
    ));
    // 一致（含尾随空白）→ 跳过
    assert_eq!(
        deploy_decision(Some("p1b-overflow"), "p1b-overflow"),
        DeployAction::Skip
    );
    assert_eq!(
        deploy_decision(Some("p1b-overflow\n"), "p1b-overflow"),
        DeployAction::Skip,
        "标记文件可能带尾随换行，trim 后比对"
    );
}

// ═══ 〔DP1 · 第四波〕远端判身份认字节（`设计/96 §7.2`）═══════════════════════════════
//
// 要求住址：`设计/96 §7.2.1`，逐字：「**读它字节里那段身份戳，不跑它**」；`§7.2.4`：「读不出来时的显式失败 —— 四态，不许合并」；
// `§7.2.3`：「部署决策的对照物只能是后者」（手上那份字节自报的，不是源码常量）。
// 〔墓碑 —— 这里原来是 K-W4 `§0c` 那几格：旁挂版本标记（目录级）与「落点那个文件在不在」两个事实合起来判的真值表。
//  旁挂标记在后端那条路上退役了（它是标签不是指纹），那几格随判定函数一起换成下面这几格。〕

/// I1：六形逐形（期望取自 `96 §7.2.4` 那张表 ＋ 0 字节那一格按「没装」）。
#[test]
fn identity_decision_answers_each_state_without_merging_them() {
    const EXPECT: &str = "p9-sample";
    let d = |id: RemoteIdentity| identity_decision(&id, EXPECT, "devbox", "/h/.cc-monitor/bin/ccm");
    assert!(
        matches!(d(RemoteIdentity::Missing), Ok(DeployAction::Deploy(_))),
        "没装 ⇒ 装"
    );
    assert!(
        matches!(d(RemoteIdentity::Empty), Ok(DeployAction::Deploy(_))),
        "0 字节 ⇒ 装"
    );
    assert_eq!(
        d(RemoteIdentity::Stamp(EXPECT.into())),
        Ok(DeployAction::Skip),
        "同一版 ⇒ 复用"
    );
    let Ok(DeployAction::Deploy(why)) = d(RemoteIdentity::Stamp("p8-older".into())) else {
        panic!("另一版 ⇒ 该换");
    };
    assert!(
        why.contains("p8-older") && why.contains(EXPECT),
        "换的理由没说清两边各是哪一版：{why}"
    );
    // 三种「判不清它是谁」：显式失败，而且三句话互不相同（下一步不同：一个没身份、一个身份不唯一、一个判不了）。
    let no = d(RemoteIdentity::NoStamp).unwrap_err();
    let many = d(RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()])).unwrap_err();
    let cant = d(RemoteIdentity::Unreadable("Permission denied".into())).unwrap_err();
    for e in [&no, &many, &cant] {
        assert!(
            e.contains("devbox") && e.contains("/h/.cc-monitor/bin/ccm"),
            "没说哪台哪个文件：{e}"
        );
    }
    assert!(no.contains("不说自己是哪一版"), "{no}");
    assert!(many.contains("a1") && many.contains("b2"), "{many}");
    assert!(
        cant.contains("判不了") && cant.contains("Permission denied"),
        "{cant}"
    );
    assert!(no != many && many != cant && no != cant);
    // 出路是一个真存在的动作（机器页「卸载后端」），不是一句空话。
    assert!(no.contains("卸载后端") && many.contains("卸载后端"));
}

/// I2：扫描的回话 → 身份。退出码 0 / 1 / 其它 · 重复戳去重 · 两个不同戳 · 空身份不收。
#[test]
fn the_stamp_scan_answer_maps_to_exactly_one_identity_state() {
    let (o, c) = (env!("BACKEND_STAMP_OPEN"), env!("BACKEND_STAMP_CLOSE"));
    let line = |id: &str| format!("{o}{id}{c}\n");
    assert_eq!(
        interpret_stamp_scan(Some(0), &line("p9-sample"), ""),
        RemoteIdentity::Stamp("p9-sample".into())
    );
    // 同一个戳在字节里出现两次（`grep -o` 逐处吐）⇒ 仍是一个身份。
    assert_eq!(
        interpret_stamp_scan(
            Some(0),
            &format!("{}{}", line("p9-sample"), line("p9-sample")),
            ""
        ),
        RemoteIdentity::Stamp("p9-sample".into())
    );
    assert_eq!(
        interpret_stamp_scan(Some(0), &format!("{}{}", line("b2"), line("a1")), ""),
        RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()])
    );
    assert_eq!(
        interpret_stamp_scan(Some(0), &line(""), ""),
        RemoteIdentity::NoStamp
    );
    assert_eq!(
        interpret_stamp_scan(Some(1), "", ""),
        RemoteIdentity::NoStamp
    );
    assert!(matches!(
        interpret_stamp_scan(Some(2), "", "grep: /x: Permission denied"),
        RemoteIdentity::Unreadable(w) if w.contains("Permission denied")
    ));
    // 没送退出码（链路被掐）≠ 0：不许读成「扫到了」或「没有」。
    assert!(matches!(
        interpret_stamp_scan(None, &line("p9-sample"), ""),
        RemoteIdentity::Unreadable(_)
    ));
}

/// I2b：那条命令只读、界标不写字面量、路径过引号、身份至少一个字符（与 `build.rs::bytes_build_id` 同一条纪律）。
#[test]
fn the_stamp_scan_command_is_read_only_and_quoted() {
    let cmd = stamp_scan_cmd("/h/a b/.cc-monitor/bin/ccm");
    assert!(cmd.starts_with("LC_ALL=C grep -aoE "), "{cmd}");
    assert!(
        cmd.ends_with("-- '/h/a b/.cc-monitor/bin/ccm'"),
        "路径没过引号：{cmd}"
    );
    assert!(
        cmd.contains("[[:alnum:]_.-]+"),
        "身份那一段不是「至少一个字符」：{cmd}"
    );
    for m in [env!("BACKEND_STAMP_OPEN"), env!("BACKEND_STAMP_CLOSE")] {
        assert!(cmd.contains(m), "界标没进命令：{m} / {cmd}");
    }
    // 只读：引号之外没有任何会写的东西（界标里的 `>>` 在引号里，是正则的一部分）。
    let unquoted: String = cmd.split('\'').step_by(2).collect();
    assert!(unquoted.contains("grep"), "拆引号拆歪了：{unquoted}");
    for w in [">", "rm ", "mv ", "tee", "chmod", "sed -i", ";", "|", "&"] {
        assert!(!unquoted.contains(w), "扫描命令里有写：{w} / {cmd}");
    }
}

/// I3：两条后端部署路都读那份字节自报的身份；旁挂标记在这两条路上零命中（带正控）。
#[test]
fn both_backend_deploy_paths_read_the_identity_from_the_bytes_not_from_a_marker() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    let marker_forms = |code: &str| -> Vec<&'static str> {
        [
            "/.build_id",
            "read_marker(",
            "put_marker(",
            "deploy_decision(",
        ]
        .into_iter()
        .filter(|f| code.contains(f))
        .collect()
    };
    for sig in [
        "pub async fn ensure_backend_deployed(",
        "pub async fn deploy_remote_backend(",
        "pub async fn uninstall_remote_backend(",
    ] {
        let code = dp1_body(&prod, sig);
        assert_eq!(
            marker_forms(&code),
            Vec::<&str>::new(),
            "{sig} 又碰起了旁挂标记：\n{code}"
        );
        if !sig.contains("uninstall") {
            guard_core::find_pinned(&code, "remote_identity(")
                .unwrap_or_else(|e| panic!("{sig}：不是恰好一处问那台上那份是谁（{e}）"));
            guard_core::find_pinned(&code, "identity_decision(")
                .unwrap_or_else(|e| panic!("{sig}：不是恰好一处按身份判（{e}）"));
        }
    }
    // 正控：四种标记写法都认得出来。
    assert_eq!(
        marker_forms("read_marker(x) put_marker(y) deploy_decision(z) \"{dir}/.build_id\""),
        vec![
            "/.build_id",
            "read_marker(",
            "put_marker(",
            "deploy_decision("
        ]
    );
}

// ── K-W4b：取样层那四个状态的**映射规则**逐格各一条 ─────────────────────
// 上面那几格买的是「判定那一半」与「两条路真的去问了」；取样这一半（`metadata` /
// `try_exists` 的答案怎么变成 `TargetBinary`）09-06 之前一条判据都没有：
// 把 `probe_target_binary` 的体换成恒答 `Present`，全量 cargo **0 红**（沙箱实测）。
// 下面五格逐格钉一条规则，第六格是反向自检（证明它们不是恒真）。

/// 映射规则①：`metadata` 说它在、且**有字节** ⇒ `Present`。
#[test]
fn probe_metadata_with_bytes_maps_to_present() {
    assert_eq!(
        interpret_target_probe(Some(Some(2_300_000)), None),
        TargetBinary::Present
    );
    assert_eq!(
        interpret_target_probe(Some(Some(1)), None),
        TargetBinary::Present,
        "1 字节也是「有字节」—— 只有恰好 0 才是 Empty 那一格"
    );
}

/// 映射规则②：`metadata` 说它在、size **恰好 0** ⇒ `Empty`，不是 `Present`。
/// 0 字节不是假想形态：`upload_atomic` 那条「绝不 set_metadata」注释记的就是
/// 真机 e2e 把后端截成 0 字节、不可 exec 的那次事故，而 `try_exists` 会把它算成「在」。
#[test]
fn probe_metadata_saying_zero_bytes_maps_to_empty() {
    assert_eq!(
        interpret_target_probe(Some(Some(0)), None),
        TargetBinary::Empty
    );
    assert_ne!(
        interpret_target_probe(Some(Some(0)), None),
        interpret_target_probe(Some(Some(1)), None),
        "0 字节与有字节判成了同一格 ⇒ 身份那一步的 0 字节那一格（〔DP1〕按没装装）永远走不到"
    );
}

/// 映射规则③（本件的承重格）：`metadata` 成功而**服务器不给 size**（`Some(None)`）
/// ⇒ 仍是 `Present`。
/// `TargetBinary` 与取样壳的头注逐字：「服务器不给 size（size=None）≠ 0 字节」——
/// 把「没说」读成「空」，等于对着一台好机器每次连接都重传 2.3MB。
#[test]
fn probe_a_server_that_gives_no_size_is_not_the_empty_cell() {
    assert_eq!(
        interpret_target_probe(Some(None), None),
        TargetBinary::Present
    );
    assert_ne!(
        interpret_target_probe(Some(None), None),
        TargetBinary::Empty,
        "「服务器没给 size」被读成了「0 字节」"
    );
}

/// 映射规则④：`metadata` 失败、补问 `try_exists` **明确答不在** ⇒ `Missing`。
#[test]
fn probe_stat_failed_and_try_exists_says_no_maps_to_missing() {
    assert_eq!(
        interpret_target_probe(None, Some(false)),
        TargetBinary::Missing
    );
}

/// 映射规则⑤：`metadata` 失败、`try_exists` **也答不出来** ⇒ `Unknown`。
/// 不许滑成 `Missing`（一次 stat 失败换一次全量重传，版本门控就废了），
/// 也不许滑成 `Present`（那正是本枚举要治的那个静默）。
#[test]
fn probe_stat_failed_and_try_exists_cannot_answer_maps_to_unknown() {
    assert_eq!(interpret_target_probe(None, None), TargetBinary::Unknown);
    assert_ne!(
        interpret_target_probe(None, None),
        interpret_target_probe(None, Some(false)),
        "「问不出来」与「明确不在」判成了同一格 —— 这两者正是要分开的那两件事"
    );
}

/// **反向自检**：上面五格每一条都可能是恒真的（函数恒答那一张脸，断言照样绿）。
/// 这一格喂**全部六种输入**，钉的是「每一格只由它自己那条规则命中」——
/// 任何一臂被改到别的状态，下面必有一行不等。
#[test]
fn probe_no_cell_answers_in_place_of_another() {
    let table: [(Option<Option<u64>>, Option<bool>, TargetBinary, &str); 6] = [
        (Some(Some(9)), None, TargetBinary::Present, "有字节"),
        (Some(Some(0)), None, TargetBinary::Empty, "恰好 0 字节"),
        (Some(None), None, TargetBinary::Present, "服务器不给 size"),
        (
            None,
            Some(false),
            TargetBinary::Missing,
            "stat 失败 + try_exists 说不在",
        ),
        (
            None,
            Some(true),
            TargetBinary::Present,
            "stat 失败 + try_exists 说在",
        ),
        (
            None,
            None,
            TargetBinary::Unknown,
            "stat 失败 + try_exists 也答不出",
        ),
    ];
    for (size, exists, want, what) in table {
        assert_eq!(interpret_target_probe(size, exists), want, "{what}");
    }
    // 四个状态一个不少地被这张表喂到 —— 少一行就等于那一格没人看。
    for want in [
        TargetBinary::Present,
        TargetBinary::Missing,
        TargetBinary::Empty,
        TargetBinary::Unknown,
    ] {
        assert!(
            table.iter().any(|(_, _, w, _)| *w == want),
            "{want:?} 这一格没有输入喂给它"
        );
    }
    // 恒答任何一张脸都会被这三对逮住（不是「函数存在」那种空真）。
    assert_ne!(
        interpret_target_probe(Some(Some(0)), None),
        interpret_target_probe(Some(Some(9)), None)
    );
    assert_ne!(
        interpret_target_probe(Some(None), None),
        interpret_target_probe(Some(Some(0)), None)
    );
    assert_ne!(
        interpret_target_probe(None, Some(false)),
        interpret_target_probe(None, None)
    );
}

/// **防空转**（K-W4b）：上面六格全在纯函数上，取样壳只要不接到它就是死代码，
/// 而六格照样绿 —— 那正是 09-06 之前那个洞（`probe_target_binary` 体恒答 `Present`
/// ⇒ 全量 cargo 0 红）。这一格钉的是取样壳**真的走**那个纯解释函数、
/// 并且**没有**把状态直接写死在 async 体里。
///
/// 形状照抄同文件的 `both_backend_deploy_paths_read_the_identity_from_the_bytes_not_from_a_marker`
/// （含它那种反向自检）。
///
/// ⚠ 射程：它看的是**源码文本**，不是运行期。挡得住「体被换成常量 / 纯函数没接上」，
/// 挡不住「调了纯函数但把返回值扔了」——那一形由上面六格与类型系统一起管。
#[test]
fn the_probe_shell_really_goes_through_the_pure_interpreter() {
    fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let i = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {sig}——守卫失效了"));
        let j = src[i..].find("\n}\n").map(|k| i + k).unwrap_or(src.len());
        &src[i..j]
    }
    const SIG: &str = "async fn probe_target_binary(";
    let src = include_str!("../../src/bridge/src/sftp.rs");
    let code = body(src, SIG)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    // 反向自检：**真取到体了**。取不到（空串）时下面那条 `!contains` 会恒真地全绿，
    // 这一格就从守卫变成假绿源 ⇒ 先用一条正向断言把空串挡在外面。
    assert!(
        code.contains(SIG) && code.contains("fs.stat("),
        "取到的不是 probe_target_binary 的体（拿到 {} 字节）",
        code.len()
    );
    assert!(
        code.contains("interpret_target_probe("),
        "取样壳没走那个纯解释函数 —— 四态映射的那几格全成了死代码，掏空它一条都不会红"
    );
    assert!(
        !code.contains("TargetBinary::"),
        "取样壳里直接写死了状态 —— 映射规则又回到了不可测的 async 体里"
    );
}

// 〔RW1 · 第四波 09-24〕这里原来是远端删会话那道结构守卫的单元判据；守卫随 SFTP 直删一起走了，
//   「哪几份才许删」那一问的判据住后端（`session_file_for_delete` 的删会话那一族）。

#[test]
fn remote_parent_and_marker() {
    assert_eq!(
        remote_parent("/home/pi/.cc-monitor/bin/cc-monitor-backend"),
        "/home/pi/.cc-monitor/bin"
    );
    assert_eq!(remote_parent("/x"), "/");
    assert_eq!(remote_parent("rel/path"), "rel");
    assert_eq!(remote_parent("noslash"), ".");
    // 〔DP1〕旁挂标记的路径拼法随标记一起退役（后端那条路读字节自己的身份戳）。
}

/// **T04 审计⑤**：抽出来的谓词要对两个消费者都成立，且**标记词是必需条件**
/// ——那是防误删的关键（把"这是 cc-monitor 管的目录"变成路径本身的性质）。
#[test]
fn safe_managed_path_requires_a_marker() {
    // 四条通用条件
    for bad in ["", "  ", "relative/x", "/a/../b/cc-monitor", "/"] {
        assert!(
            !is_safe_remote_managed_path(bad, &["cc-monitor"]),
            "{bad:?} 不该通过"
        );
    }
    // **没有标记词一律不通过**——哪怕是个完全正常的绝对路径
    assert!(!is_safe_remote_managed_path(
        "/home/u/.local/bin/x",
        &["cc-monitor"]
    ));
    assert!(is_safe_remote_managed_path(
        "/home/u/.cc-monitor/d",
        &["cc-monitor"]
    ));
    // 多标记词：任一命中即可（acct-iso 就是两个）
    let m = &["cc-acct-iso", ".cc-monitor"];
    assert!(is_safe_remote_managed_path("/opt/cc-acct-iso", m));
    assert!(is_safe_remote_managed_path("/home/u/.cc-monitor/ai", m));
    assert!(!is_safe_remote_managed_path("/opt/other", m));
}

// ===== T04 审计① 上传读回判据（此前这条路完全没有读回）=====

// 〔SR1b · 2026-09-24〕读回那一趟住本机后端（它交回**比对的事实**：读回长度 · 首个差异，读不回 ⇒ `None`；
//   那一侧怎么算由后端 `dial_sftp_tests` 的部署那一趟判）。这里判的是**判定与话**：拿事实喂 `verify_readback`。

#[test]
fn upload_verify_catches_same_length_corruption() {
    let want = b"#!/bin/sh\nexec ccm \"$@\"\n";
    // 等长但一字节不同——**只比长度是查不出来的**，而此前连长度都没比
    let k = (want.len() / 2) as u64;
    let e = verify_readback(
        "/r/x",
        want.len() as u64,
        Some((want.len() as u64, Some(k))),
    )
    .unwrap_err();
    assert!(e.contains("长度相同"), "{e}");
    assert!(
        e.contains(&format!("首个差异在第 {k} 字节")),
        "要指出位置：{e}"
    );
    // 〔DP1〕「下次会重来」那半句挪到了 `upload_verified`（它当场删掉传坏的那一份），这里只说坏在哪。
    assert!(e.contains("/r/x"), "{e}");
}

#[test]
fn upload_verify_catches_truncation_and_unreadable() {
    let e = verify_readback("/r/x", 10, Some((5, Some(5)))).unwrap_err();
    assert!(e.contains("长度不匹配"), "{e}");
    assert!(e.contains("期望 10 字节"), "{e}");
    // 读不回来 ≠ 写对了
    let e2 = verify_readback("/r/x", 10, None).unwrap_err();
    assert!(e2.contains("读不回"), "{e2}");
    assert!(e2.contains("/r/x"), "{e2}");
}

#[test]
fn upload_verify_passes_on_exact_bytes() {
    // 二进制（含 NUL 与非 UTF-8）也要过——判定只看事实，不碰字节本身
    assert!(verify_readback("/r/d", 7, Some((7, None))).is_ok());
    assert!(verify_readback("/r/d", 0, Some((0, None))).is_ok());
    // 反向：长度对上了而差异在 ⇒ 不许放行（这一格是「只比长度」那一形的阴性对照）
    assert!(verify_readback("/r/d", 7, Some((7, Some(0)))).is_err());
}

// 〔W5-ALIAS · 第五波先行〕这里原来是远端 profile 读取那一族的四条判据（读不出不当空文件 · 非 UTF-8 拒 ·
//   有字节读到空拒 · `RemoteFile::read` 走 fail-safe 读取器）。被测对象 `interpret_profile_read`〔散文墓碑〕/
//   `read_profile_text`〔散文墓碑〕/ `RemoteFile`〔散文墓碑〕随 `fenced_block::apply`〔散文墓碑〕一起删了：它们只剩
//   远端 `ccm` 入口一个用户，而那一处改走 `upload_verified`（部署物按字节比，不按文本读）。用户文件（rc）的读
//   经那台后端的 `files-peek`，「不存在 / 读不出 / 有字节读到空」那三分由后端答（`user_files::Peeked`）。

// 〔AL1 · 2026-09-24〕`rollback_note_matches_what_actually_happened` 搬走了〔散文墓碑〕
// —— 措辞的住址从远端独有的那一份换成了本机远端共用的 `fenced_block::undo_note`〔散文墓碑〕，
// 判据跟着住到 `fenced_block_tests.rs` 那条「撤的措辞只说真发生的事」；〔W5-ALIAS〕那一族后来随序列一起删了。

/// **结构性守卫**：两条 deploy 路径的**内容**上传必须走 verified。
///
/// 范围只覆盖 `deploy_remote_backend` 与 `deploy_remote_acct_iso` 两个函数体
/// ——**第一版写成"全文件不许有裸 upload_atomic"，当场被自己抓**：
/// ccm helper 那条路（`&profile, stripped/merged`）**故意**用裸上传，
/// 因为它下游紧接着自己的读回 + 回滚（`sftp.rs` 那三处 `verify_readback`）。
/// 守卫范围比性质宽 = 假红 = 会被人关掉。
///
/// 版本标记允许裸上传：它是"校验通过"的凭证，必须最后写。
#[test]
fn deploy_paths_use_verified_upload_for_content() {
    fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let i = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {sig}——守卫失效了"));
        let j = src[i..].find("\n}\n").map(|k| i + k).unwrap_or(src.len());
        &src[i..j]
    }
    let checks = [
        (
            body(
                include_str!("../../src/bridge/src/sftp.rs"),
                "pub async fn deploy_remote_backend(",
            ),
            "deploy_remote_backend",
        ),
        (
            body(
                include_str!("../../src/bridge/src/acct_iso_deploy.rs"),
                "pub async fn deploy_remote_acct_iso(",
            ),
            "deploy_remote_acct_iso",
        ),
    ];
    let mut verified_total = 0usize;
    for (b, what) in checks {
        let code = b
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 反向自检：真取到函数体了。〔SR1b〕上传经本机后端（`upload_verified` 读回比对 · `put_marker` 只写标记）。
        assert!(
            code.contains("upload_verified("),
            "{what}: 取到的体里没有上传，守卫在空转"
        );
        verified_total += code.matches("upload_verified(").count();
        for l in code.lines() {
            if !(l.contains("put_marker(") || l.contains("fs.put(")) {
                continue;
            }
            assert!(
                l.contains("marker"),
                "{what}: 内容上传没走读回比对（`upload_verified`）—— {}",
                l.trim()
            );
        }
    }
    // 计数自检：2 处后端二进制 + 6 个 acct-iso 脚本
    assert_eq!(
        verified_total, 7,
        "期望 1(backend 体内) + 6(acct-iso)，实得 {verified_total}"
    );
}

#[test]
fn safe_backend_path_accepts_convention_rejects_suspicious() {
    assert!(is_safe_remote_backend_path(
        "/home/pi/.cc-monitor/bin/cc-monitor-backend"
    ));
    assert!(!is_safe_remote_backend_path("")); // 空
    assert!(!is_safe_remote_backend_path("relative/cc-monitor")); // 非绝对
    assert!(!is_safe_remote_backend_path("/")); // 根
    assert!(!is_safe_remote_backend_path("/etc/passwd")); // 不含 cc-monitor
    assert!(!is_safe_remote_backend_path(
        "/home/pi/.cc-monitor/../../../etc/x"
    )); // 含 ..
}

// 〔SR1b · 2026-09-24〕`the_sftp_dependency_is_really_on_russh_sftp_three` 搬去了后端（`tests/backend/dial_sftp_tests.rs`）：
//   `russh-sftp` 出了界面清单（界面进程零 SFTP），今天只在 `src/backend/Cargo.toml` 里 —— 判据跟着依赖走，读后端那份清单与 lock。

/// 🔴 〔MC1 · 2026-09-24〕`设计/71 §13.3` ①：**「部署后端」只有一个动作** —— 后端本体 ＋ `ccm` 入口。
///
/// 两向：`deploy_remote_backend` 的函数体里**恰好一处** `put_ccm_entry(` 调用；
/// 全文件生产段里推入口的原语（`ccm_entry_shim(`）**恰好一处**、就住 `put_ccm_entry` 里 ——
/// 装别名块那条（`install_remote_alias_block`，〔W5-ALIAS〕今天住 `profile_installer.rs`）所在的文件**零命中**（从前它一次做两件事，`71 §13.1` 那个 ① ②）。
///
/// 死值验：把 `deploy_remote_backend` 里那一句 `put_ccm_entry(&fs, &path)` 摘掉 ⇒ 第一条红。
#[test]
fn deploying_the_backend_also_puts_the_ccm_entry_and_nothing_else_does() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    let body_of = |sig: &str| -> String {
        let i = prod
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {sig}——守卫失效了"));
        let j = prod[i..].find("\n}\n").map(|k| i + k).unwrap_or(prod.len());
        prod[i..j].to_string()
    };
    let deploy = body_of("pub async fn deploy_remote_backend(");
    guard_core::find_pinned(&deploy, "put_ccm_entry(&fs, &path)")
        .unwrap_or_else(|e| panic!("部署后端没有连同 ccm 入口一起放（{e}）"));
    // 〔W5-ALIAS〕装别名块那条搬进了 `profile_installer.rs`：那份文件的生产段里推入口的原语**零命中**。
    let alias_home =
        guard_core::production_code(include_str!("../../src/bridge/src/profile_installer.rs"));
    guard_core::find_pinned(&alias_home, "pub async fn install_remote_alias_block(")
        .unwrap_or_else(|e| panic!("装别名块那条不在别名域了 —— 判据够不着被测对象（{e}）"));
    for prim in ["ccm_entry_shim", "put_ccm_entry", "CCM_CLI_REMOTE_PATH"] {
        assert!(
            !guard_core::contains_word(&alias_home, prim),
            "别名域又在推入口了（{prim}）—— 那是「部署后端」的事"
        );
    }
    let helper = body_of("async fn put_ccm_entry(");
    guard_core::find_pinned(&helper, "ccm_entry_shim(backend_path)")
        .unwrap_or_else(|e| panic!("put_ccm_entry 里推的不是那三行入口（{e}）"));
    guard_core::find_pinned(&prod, "ccm_entry_shim(")
        .unwrap_or_else(|e| panic!("推入口的原语不是恰好一处（{e}）"));
}

/// 〔SR1b · 2026-09-24〕**界面那一侧对着真后端 ＋ 真 sshd**：部署那几问经 `RemoteFs`（`files` 链路）、
/// 传输经中继（`sftp_pool::transfer_call` / `watch_ticket`），全程界面进程零 SSH。
///
/// 只由 `tests/evidence/SR1b-sftp-loopback.py --monitor` 带 `SR1B_LOOPBACK`
/// （`{host,port,user,key_path,backend,home,rhome,up,dl_remote,dl_local}`）来跑；那台 sshd 的 sftp 起始目录是临时的 `rhome`，
/// 写不到真 home。买到：部署判定四形（缺 ⇒ 部署 · 装完 ⇒ 那台 sshd 上真扫出戳、跳过 · 截成 0 字节 ⇒ 重部署 ·
/// 〔DP1〕无戳的文件 ⇒ 显式失败不覆盖）· 入口一次写 / 一次不动 ·
/// 卸载按钮删后端那一份 · 两个写根之外 ⇒ 后端围栏拒、原话带回 · 上传 / 下载经中继走完、帧翻成 `Snap` 终局。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "要真 sshd ＋ 真后端二进制：由 tests/evidence/SR1b-sftp-loopback.py --monitor 带环境变量来跑"]
async fn sr1b_loopback_deploy_and_transfer_through_the_resident_backend() {
    use futures::StreamExt;
    let _local = crate::backend::control::inbound_client::local_origin_test_lock();
    let raw = std::env::var("SR1B_LOOPBACK").expect("没有 SR1B_LOOPBACK —— 这条只该由读数脚本来跑");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let s = |k: &str| v[k].as_str().unwrap().to_string();
    let home = s("home");
    let rhome = s("rhome");
    let mut child = std::process::Command::new(s("backend"))
        .env("HOME", &home)
        .env("TMUX_TMPDIR", &home)
        .env_remove("TMUX")
        .env_remove("CCM_LISTEN_PORT")
        .env_remove("CCM_LISTEN_TOKEN")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起不了后端");
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    std::thread::spawn(move || {
        crate::backend::control::local_backend::local_stdio_consumer(stdin, stdout)
    });
    let backend_path = format!("{rhome}/.cc-monitor/bin/cc-monitor-backend");
    let cfg = crate::ssh_source::RemoteConfig {
        host: s("host"),
        label: "sr1b-loopback".into(),
        port: v["port"].as_u64().unwrap() as u16,
        user: s("user"),
        key_path: Some(s("key_path")),
        backend_path: backend_path.clone(),
        host_key_fingerprint: None,
        addresses: vec![],
        jump: None,
    };
    // ① 部署那几问（判定函数原样，执行经后端）
    let fs = RemoteFs::open(&cfg).await.expect("开不了 files 链路");
    assert_eq!(fs.home(), rhome, "起始目录不是 sshd 给的那个");
    // 〔DP1〕身份读那份字节自己的戳（那台 sshd 上真跑一次只读扫描），不读旁挂标记。
    //   送去的字节里埋一段戳（界标取自 `build.rs` 交来的 env），其余是 3 MB 的噪声。
    let stamp = format!(
        "{}sr1b-id{}",
        env!("BACKEND_STAMP_OPEN"),
        env!("BACKEND_STAMP_CLOSE")
    );
    let mut bytes: Vec<u8> = (0..3_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
    bytes.splice(1_000_000..1_000_000, stamp.bytes());
    let decide =
        |id: RemoteIdentity| identity_decision(&id, "sr1b-id", "sr1b-loopback", &backend_path);
    // 〔DP1〕读数脚本 ② 在这个落点上留下了一份 3 MB 的随机字节（没有身份戳）⇒ 那台 sshd 上真扫一次：
    //   显式失败、一个字节都不写（盘上那份原样）；出路是机器页「卸载后端」—— 这里就用那颗按钮的真命令删掉它。
    let leftover = std::fs::read(&backend_path).expect("读数脚本 ② 留下的那份不在 —— 台架变了");
    let d_pre = decide(remote_identity(&cfg, &fs, &backend_path).await.unwrap());
    assert!(
        matches!(&d_pre, Err(e) if e.contains("不说自己是哪一版")),
        "无戳的旧文件 ⇒ 该显式失败：{d_pre:?}"
    );
    assert_eq!(
        std::fs::read(&backend_path).unwrap(),
        leftover,
        "判定之后盘上那份变了"
    );
    let msg = uninstall_remote_backend(cfg.clone())
        .await
        .expect("卸载（出路）");
    assert!(msg.starts_with(&format!("已删除 {backend_path}")), "{msg}");
    let d0 = decide(remote_identity(&cfg, &fs, &backend_path).await.unwrap());
    assert!(
        matches!(d0, Ok(DeployAction::Deploy(_))),
        "落点缺 ⇒ 该部署：{d0:?}"
    );
    fs.mkdirs(remote_parent(&backend_path)).await.unwrap();
    upload_verified(&fs, &backend_path, &bytes, 0o700)
        .await
        .expect("上传 ＋ 读回");
    let id1 = remote_identity(&cfg, &fs, &backend_path).await.unwrap();
    assert_eq!(
        id1,
        RemoteIdentity::Stamp("sr1b-id".into()),
        "那台上那份的戳没被扫出来"
    );
    assert_eq!(decide(id1), Ok(DeployAction::Skip), "装完 ⇒ 该跳过");
    assert_eq!(
        std::fs::read(&backend_path).unwrap(),
        bytes,
        "盘上那份不是送去的字节"
    );
    std::fs::write(&backend_path, b"").unwrap();
    let d2 = decide(remote_identity(&cfg, &fs, &backend_path).await.unwrap());
    assert!(
        matches!(d2, Ok(DeployAction::Deploy(_))),
        "截成 0 字节 ⇒ 该重部署：{d2:?}"
    );
    // 落点上是一份不说自己是谁的东西 ⇒ 显式失败、不覆盖（判定层不写；盘上那份原样）。
    std::fs::write(&backend_path, b"#!/bin/sh\necho not ours\n").unwrap();
    let d3 = decide(remote_identity(&cfg, &fs, &backend_path).await.unwrap());
    assert!(
        matches!(&d3, Err(e) if e.contains("不说自己是哪一版")),
        "无戳的文件 ⇒ 该显式失败：{d3:?}"
    );
    upload_verified(&fs, &backend_path, &bytes, 0o700)
        .await
        .unwrap();
    // ② 入口：第一次写、第二次不动
    let e1 = put_ccm_entry(&fs, &backend_path).await.unwrap();
    let e2 = put_ccm_entry(&fs, &backend_path).await.unwrap();
    // 〔W5-ALIAS〕回报从 `fenced_block::Applied`〔散文墓碑〕换成「写没写」。
    assert!(e1, "第一次该放入口");
    assert!(!e2, "第二次该一个字节都不动");
    assert!(std::path::Path::new(&format!("{rhome}/.cc-monitor/bin/ccm")).is_file());
    // ③ 两个写根之外 ⇒ 后端围栏拒、原话带回、盘上零改动
    let outside = format!("{rhome}/.cc-monitor/elsewhere/cc-monitor-backend");
    let e = upload_verified(&fs, &outside, b"x", 0o700)
        .await
        .unwrap_err();
    assert!(
        e.contains("~/.cc-monitor/bin/"),
        "拒绝的话没说只许哪两处：{e}"
    );
    assert!(!std::path::Path::new(&outside).exists());
    drop(fs);
    // ④ 卸载按钮（真命令）：〔DP1〕只删后端那一份（旁挂标记退役）
    let msg = uninstall_remote_backend(cfg.clone()).await.expect("卸载");
    assert!(msg.starts_with(&format!("已删除 {backend_path}")), "{msg}");
    assert!(!std::path::Path::new(&backend_path).exists());
    // ⑤ 上传经中继：开单 → 订阅即起跑 → 终局 Done，暂存件逐字节等于本机那份
    let origin = crate::origin::Origin(cfg.origin_label());
    let up = s("up");
    let r = crate::sftp_pool::transfer_call(
        cfg.clone(),
        crate::sftp_pool::TRANSFER_UPLOAD,
        &serde_json::json!({ "local_path": up }),
    )
    .await
    .expect("开单（上传）");
    let (id, key) = (r["id"].as_str().unwrap(), r["key"].as_str().unwrap());
    let last = crate::sftp_pool::watch_ticket(&origin, id)
        .expect("订阅")
        .collect::<Vec<_>>()
        .await
        .pop()
        .unwrap();
    let want = std::fs::read(&up).unwrap();
    assert_eq!(
        last.end,
        Some(crate::sftp_pool::End::Done {
            bytes: want.len() as u64
        })
    );
    let staged = std::fs::read(format!("{rhome}/.cc-monitor/staging/{key}.part")).unwrap();
    assert!(staged == want, "暂存件不是本机那份的字节");
    // ⑥ 下载经中继
    let dl_local = s("dl_local");
    let r = crate::sftp_pool::transfer_call(
        cfg.clone(),
        crate::sftp_pool::TRANSFER_DOWNLOAD,
        &serde_json::json!({ "remote_path": s("dl_remote"), "local_path": dl_local }),
    )
    .await
    .expect("开单（下载）");
    let last = crate::sftp_pool::watch_ticket(&origin, r["id"].as_str().unwrap())
        .expect("订阅")
        .collect::<Vec<_>>()
        .await
        .pop()
        .unwrap();
    assert!(
        matches!(last.end, Some(crate::sftp_pool::End::Done { .. })),
        "{last:?}"
    );
    assert!(
        std::fs::read(&dl_local).unwrap() == std::fs::read(s("dl_remote")).unwrap(),
        "下载落地的字节不对"
    );
    let _ = child.kill();
    let _ = child.wait();
    println!("SR1B-LOOPBACK-MONITOR ok");
}

// ═══ 〔DP1 · 第四波〕自动部署不再静默 ═══════════════════════════════════════════════════
//
// 要求住址：`设计/96 §7.1.4`，逐字：「拒绝是一个会到达用户的结论，不是一行 `debug` 日志」·
// 「**返回类型上不许有『成功』这一支**：拒绝要与『部署成功』在类型上分得开，界面才显示得出来」。

/// 切出生产段里一个函数的体（到列 0 的 `}` 为止）。
fn dp1_body(prod: &str, sig: &str) -> String {
    let at =
        guard_core::find_pinned(prod, sig).unwrap_or_else(|e| panic!("{sig} 不是恰好一处：{e}"));
    let rest = &prod[at..];
    let end = rest.find("\n}\n").unwrap_or(rest.len());
    rest[..end].to_string()
}

/// 本条要逮的两种「静默」写法：`Ok(None)`（没部署也算成功）与 `debug!`（只留一行没人看的日志）。
fn dp1_silent_forms(body: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if body.contains("Ok(None)") {
        out.push("Ok(None)");
    }
    if body.contains("tracing::debug!") || body.contains(" debug!(") {
        out.push("debug!");
    }
    out
}

/// D1：`ensure_backend_deployed` 的返回类型是 `Result<String, DeployError>`（没有「没部署也算成功」那一支），
/// 体内零 `Ok(None)` · 零 `debug!`；表拒绝恰好交成 `DeployError::Refused` 一处。
#[test]
fn the_auto_deploy_never_refuses_silently() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    let body = dp1_body(&prod, "pub async fn ensure_backend_deployed(");
    let sig = body.lines().next().unwrap_or_default();
    assert!(
        sig.contains("-> Result<String, DeployError>"),
        "返回类型变了：{sig} —— `Option` 回来就是「没部署也算成功」那一支回来了"
    );
    assert_eq!(
        dp1_silent_forms(&body),
        Vec::<&str>::new(),
        "体内又有了静默的拒绝：\n{body}"
    );
    guard_core::find_pinned(&body, "Err(DeployError::Refused(").unwrap_or_else(|e| {
        panic!("表拒绝没有恰好一处交成 `DeployError::Refused`（{e}）：\n{body}")
    });
    // 正控：两种静默写法都认得出来。
    assert_eq!(
        dp1_silent_forms("x => return Ok(None),\n tracing::debug!(\"跳过\");"),
        vec!["Ok(None)", "debug!"]
    );
}

/// D2：调用方拿到 `Err` ⇒ 经远端健康通道恰发一条 `kind = "deploy"`（不阻断，照旧接着试连已有后端）。
#[test]
fn a_failed_auto_deploy_reaches_the_screen_through_remote_health() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    let at = guard_core::find_pinned(&prod, "crate::sftp::ensure_backend_deployed(cfg).await")
        .unwrap_or_else(|e| panic!("调用处不是恰好一处：{e}"));
    let rest = &prod[at..];
    let err_at = rest.find("Err(e) =>").expect("找不到 Err 那一支");
    let arm = &rest[err_at..];
    let arm = &arm[..arm.find("\n        }\n").unwrap_or(arm.len())];
    guard_core::find_pinned(arm, "kind: \"deploy\"")
        .unwrap_or_else(|e| panic!("Err 那一支没有恰好一条 `kind: \"deploy\"`（{e}）：\n{arm}"));
    guard_core::find_pinned(arm, "app.emit(crate::bridge::events::REMOTE_HEALTH")
        .unwrap_or_else(|e| panic!("Err 那一支没有恰好一次发到远端健康通道（{e}）：\n{arm}"));
    assert!(
        arm.contains("e.say(&host_label)"),
        "发出去的不是那句话本身：\n{arm}"
    );
    assert!(
        arm.contains("None"),
        "Err 那一支不再「不阻断、按没确认处理」：\n{arm}"
    );
}

/// 〔DP1 · 第四波〕读回比对不对 ⇒ **当场删掉传坏的那一份**（旁挂标记退役后，断链那一环就是这一删）。
///
/// 要求住址：`设计/96 §7.2.3` 部署决策的对照物是那份字节自报的身份 ⇒ 一份传坏却恰好还带着对的戳的字节，
/// 下次会被判「已是这一版」—— 删掉它，下次就是「落点没有 ⇒ 装」。
/// 形态判据（`upload_verified` 要经真 `RemoteFs`，单测起不了那条链路；行为那一半在回环 sshd 读数里没有现成的坏读回可造）：
/// 读回判定的 `Err` 那一支里恰好一次 `fs.remove(remote_path)`，且排在 `fs.put(` 之后。
#[test]
fn a_bad_readback_removes_the_upload_it_just_made() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    let body = dp1_body(&prod, "pub(crate) async fn upload_verified(");
    let put = guard_core::find_pinned(&body, "fs.put(remote_path").expect("上传那一问不在了");
    let bad = guard_core::find_pinned(&body, "let Err(bad) = verify_readback(")
        .expect("读回判定那一支不在了");
    let rm = guard_core::find_pinned(&body, "fs.remove(remote_path)")
        .unwrap_or_else(|e| panic!("读回不对那一支没有恰好一次删掉那一份（{e}）：\n{body}"));
    assert!(put < bad && bad < rm, "次序不是「传 → 判 → 删」：\n{body}");
}

/// 🔴 〔W5-ALIAS · 第五波先行〕**A1：别名块的真相不住 `sftp.rs`**（`audit/B-decouple.md` §2 第 12 条：
/// 「别名块真相从 `sftp.rs` 搬到别名域；`sftp.rs` 已不做 SFTP」；`设计/71 §3`「渲染 · 读回 · 启动文件归方言，
/// 写入 … 那台机器后端的文件管理那一面」—— 部署那一族管不着别名块）。
///
/// 两向：那 6 个符号的**定义**与两条别名块命令在 `sftp.rs` 生产段里**零命中**；正控：同一把尺子
/// （`find_pinned`：恰好一处 ＋ 两侧边界）量 `profile_installer.rs`，8 个全量得出。
/// 人群会怎么长：别的路往 `sftp.rs` 加一个别名块相关的定义 ⇒ 这里不红（名单是这 8 个）——
/// 它钉的是「这几样搬走了、没搬回来」，不是「`sftp.rs` 里没有任何与 rc 有关的东西」。
#[test]
fn the_alias_block_truth_no_longer_lives_in_sftp() {
    let sftp = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    let alias_home =
        guard_core::production_code(include_str!("../../src/bridge/src/profile_installer.rs"));
    let defs = [
        "pub(crate) const CCM_PROFILE_BEGIN: &str",
        "pub(crate) const CCM_PROFILE_END: &str",
        "pub(crate) const CCM_WRAPPER_SNIPPET: &str",
        "pub(crate) fn builtin_alias_names(",
        "pub fn merge_profile_block(",
        "pub fn strip_profile_block(",
        "pub async fn install_remote_alias_block(",
        "pub async fn uninstall_remote_alias_block(",
    ];
    let stayed: Vec<&str> = defs.iter().copied().filter(|d| sftp.contains(d)).collect();
    assert!(
        stayed.is_empty(),
        "别名块的真相又住回 `sftp.rs` 了：{stayed:?} —— 那份文件只管部署"
    );
    for d in defs {
        guard_core::find_pinned(&alias_home, d).unwrap_or_else(|e| {
            panic!("正控：`profile_installer.rs` 里量不出 `{d}`（{e}）—— 这把尺子是瞎的")
        });
    }
}
