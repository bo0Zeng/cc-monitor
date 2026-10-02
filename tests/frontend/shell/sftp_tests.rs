use super::*;

fn probe_cfg() -> crate::ssh_source::RemoteConfig {
    crate::ssh_source::RemoteConfig {
        host: "这个主机一定不存在-audit0805".into(),
        label: "probe".into(),
        port: 1,
        user: "nobody".into(),
        key_path: None,
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

// 这里原来是「删远端文件的入口真的过了围栏吗」（喂 `/etc/passwd` 给 SFTP 直删、
//   要求零网络就被结构守卫拒）。F11 改经远端后端删（`files-delete-session`，只收 sid）之后，
//   那条 SFTP 直删与它的守卫一起走了；「只收 sid · 落点由后端按 sid 找」的判据住后端。

// 卸载路那道围栏 `is_safe_remote_backend_path`〔散文墓碑〕删了：卸的是固定落点 `~/.cc-monitor/bin/ccm`（常量），
//   没有外来路径要守；本条（`both_remote_path_sinks_still_ask_their_fence`〔散文墓碑〕）随之删。

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

    // 🔴 **这里原来还有五段断言，全部打在 `CCM_CLI_SCRIPT` 上，
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

// `the_remote_ccm_entry_is_an_entry_not_an_implementation`〔散文墓碑〕 删了：远端 `ccm` 不再是三行入口，就是后端本身
//   （`K33`「所有命令只许有一处」从此由「那个文件就是后端」结构上成立）。已部署的旧入口怎么认住 `ccm_legacy_tests.rs`。

/// F08b：仅当交叉编译产物已放进 embedded-backends/（build.rs 置了 `embedded_backends` cfg）
/// 才编译/运行——证实内嵌真生效：Linux 两格取到 ELF 二进制 + build_id 非空。CI 无二进制时
/// 本测试被 cfg 掉，不误报。取字节口是 `byte_table::pick`（按 (OS, arch)）。
#[cfg(embedded_backends)]
#[test]
fn embedded_backend_binaries_present_and_valid() {
    use crate::byte_table::pick;
    use deploy_contract::key_of;
    for arch in ["x86_64", "aarch64"] {
        let key = key_of("Linux", arch).expect("表 A 认得这一格");
        let bin = pick(key).expect("内嵌二进制应存在");
        assert!(!bin.build_id.is_empty(), "build_id 非空");
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
/// `byte_table.rs` 里都必须真有一槽（漏一个 ⇒ 取字节口对它返回 `None`，
/// 远端自动部署对那个 arch **悄悄关闭** —— 与上一条判据守的是同一个事故形状的两端）。
#[test]
fn the_embedded_identity_comes_from_the_bytes_not_from_a_label() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join("src/sftp.rs")).expect("读不到 sftp.rs");
    let prod = guard_core::production_code(&src);
    // 两份 musl 的槽与它们的身份取值口搬进了 `byte_table.rs`（全仓唯一的取字节口）；
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

/// ★★ **发版流水线必须为 `build.rs` 期待的每一个 arch 都备好料**。
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
        // 〔搬树 2026-09-17〕`root` 是 crate 根（`<repo>/src/frontend/shell`）⇒ 再爬**一级**
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
            // 🔴 这里原来还有第三条：`staged/cc-monitor-backend-<arch>.build_id`，
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

    // ── 🔴 **流水线真的去问过那份字节** ─────────────────────
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

// `deploy_decision_truth_table`〔散文墓碑〕随 `deploy_decision` 删了（旁挂标记那条路整条退役）。

// ═══ 部署决策进本机常驻后端，本模块只放字节 ═══════════════════════
//
// 要求：「`sftp.rs` 部署决策（该不该换 · 换成什么 · 身份判定）进后端；monitor 只放字节」。
// 〔墓碑 —— 这里原来是两条部署路「读字节自报的身份、不读旁挂标记」与取样壳「真走纯解释函数」两格源码判据，
//  以及身份判定那几格纯函数判据：判定整个搬去了共享 crate（今天判定住后端 `control/deploy_plan.rs`、形状住 `deploy-contract`；纯判据跟着搬，期望一字未改）。〕

/// 🔴 两条部署路（自动 · 按钮）各**恰好一次**问本机常驻后端要计划、照计划取字节；本文件生产段里**零处**再做判定
/// （问那台 `uname` · 查表 A/B · 扫身份戳 · 判新旧 · 判落点 · 认旧入口），也零处碰旁挂标记。带正控。
#[test]
fn the_deploy_paths_only_place_bytes_the_backend_decides() {
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    let decisions = |code: &str| -> Vec<&'static str> {
        [
            "probe_key(",
            "choose(",
            "key_from_uname(",
            "judge(",
            "stamp_scan_cmd(",
            "interpret_stamp_scan(",
            "interpret_target_probe(",
            "identity_decision(",
            "landing_verdict(",
            "legacy_verdict(",
            "is_newer(",
            "is_ours(",
            "connect_and_exec_capture(",
        ]
        .into_iter()
        .filter(|f| code.contains(f))
        .collect()
    };
    assert_eq!(
        decisions(&prod),
        Vec::<&str>::new(),
        "sftp.rs 又自己判起了部署 —— 判定住本机常驻后端（`deploy-plan`）"
    );
    for sig in [
        "pub async fn ensure_backend_deployed(",
        "pub async fn deploy_remote_backend(",
    ] {
        let code = dp1_body(&prod, sig);
        guard_core::find_pinned(&code, "ask_plan(")
            .unwrap_or_else(|e| panic!("{sig}：不是恰好一处问后端要计划（{e}）"));
        guard_core::find_pinned(&code, "planned_binary(&plan)")
            .unwrap_or_else(|e| panic!("{sig}：不是恰好一处照计划取字节（{e}）"));
        for marker in [
            "/.build_id",
            "read_marker(",
            "put_marker(",
            "deploy_decision(",
        ] {
            assert!(!code.contains(marker), "{sig} 又碰起了旁挂标记：{marker}");
        }
    }
    // 正控：每一种判定写法都认得出来。
    assert_eq!(
        decisions("probe_key( choose( key_from_uname( judge( stamp_scan_cmd( interpret_stamp_scan( interpret_target_probe( identity_decision( landing_verdict( legacy_verdict( is_newer( is_ours( connect_and_exec_capture(").len(),
        13
    );
}

/// 计划的解码器读后端那一侧同一份金样（`tests/__fixtures__/deploy-plan.golden.json`，后端 `deploy_plan_tests` 核键集 == 真产出）；
/// 严格收：少一格 · 多一格 · 认不出的动作 / 旧落点结论都是错（两侧漂了当场说出来）。
#[test]
fn the_plan_decoder_reads_the_golden() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/deploy-plan.golden.json")).unwrap();
    let product = &golden["product"];
    let p = decode_plan(product).expect("金样解不开");
    assert_eq!(p.key.label(), "Linux / x86_64");
    assert_eq!(p.expected, "p0a-placeholder");
    assert!(
        matches!(&p.action, DeployAction::Keep { theirs, .. } if theirs == "p0b-placeholder"),
        "{:?}",
        p.action
    );
    assert_eq!(
        p.legacy,
        deploy_contract::LegacyVerdict::Unknown("placeholder".into())
    );
    assert_eq!(
        p.leftovers,
        vec![".cc-monitor/bin/ccm.1-2-3.tmp".to_string()]
    );
    let with = |k: &str, v: serde_json::Value| {
        let mut x = product.clone();
        x[k] = v;
        x
    };
    let mut missing = product.clone();
    missing.as_object_mut().unwrap().remove("why");
    let mut extra = product.clone();
    extra["surprise"] = serde_json::json!(1);
    for bad in [
        missing,
        extra,
        with("action", serde_json::json!("overwrite")),
        with("action", serde_json::json!("skip")),
        with("legacy", serde_json::json!("remove")),
        with("os", serde_json::json!("Plan9")),
        with("expected", serde_json::json!("")),
        with("leftovers", serde_json::json!("ccm.1-2-3.tmp")),
    ] {
        assert!(decode_plan(&bad).is_err(), "该拒却收了：{bad}");
    }
    let skip = with("action", serde_json::json!("skip"));
    let skip = {
        let mut x = skip;
        x["theirs"] = serde_json::Value::Null;
        x["legacy"] = serde_json::json!("absent");
        x["legacy_why"] = serde_json::Value::Null;
        x
    };
    let p = decode_plan(&skip).expect("skip ＋ absent 那一形解不开");
    assert_eq!(
        (p.action, p.legacy),
        (DeployAction::Skip, deploy_contract::LegacyVerdict::Absent)
    );
}

/// 要求：「部署失败留下半截 …tmp，之后连上也不清」· 题面 WF2 第 2 条「下次连上清旧的」。
/// 计划里的残件（后端判的）在**每次连上**的那条路上照删：自动部署与手动部署两个入口各恰好一处（在执行链上，不只是解码得出来）。
#[test]
fn the_planned_leftovers_are_swept_on_every_connect() {
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    for sig in [
        "pub async fn ensure_backend_deployed(",
        "pub async fn deploy_remote_backend(",
    ] {
        let body = dp1_body(&prod, sig);
        guard_core::find_pinned(
            &body,
            "sweep_leftovers(&plan.leftovers, &fs, &cfg.origin_label()).await;",
        )
        .unwrap_or_else(|e| panic!("{sig} 里没有照删计划里的残件：{e}"));
    }
}

/// 🔴`deploy-plan` 那一跳在本机后端里拨号 ⇒ 它的逐地址指纹随计划交回、由 monitor 按**同一个**判定固化
/// （`dial_host::settle_host_key`，monitor 自己开链路那几条也走它），不另写一份。两向：解码器把金样里的 `ack` 原样收进来；
/// 问计划那一口恰好一处把它交给 `settle_host_key`。
#[test]
fn the_plan_ack_is_pinned_by_the_same_judgement_as_every_other_dial() {
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/deploy-plan.golden.json")).unwrap();
    let p = decode_plan(&golden["product"]).expect("金样解不开");
    assert_eq!(
        p.ack.fingerprints.get("10.0.0.2:22").map(String::as_str),
        Some("SHA256:placeholder"),
        "解码器没把 ack 的逐地址指纹收进来：{:?}",
        p.ack
    );
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    let body = dp1_body(&prod, "async fn ask_plan_for(");
    guard_core::find_pinned(
        &body,
        "crate::dial_host::settle_host_key(cfg, &dial, &plan.ack)",
    )
    .unwrap_or_else(|e| panic!("问计划那一口没把 ack 交给固化判定（{e}）：\n{body}"));
}

// 这里原来是远端删会话那道结构守卫的单元判据；守卫随 SFTP 直删一起走了，
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
    // 旁挂标记的路径拼法随标记一起退役（后端那条路读字节自己的身份戳）。
}

// `safe_managed_path_requires_a_marker`〔散文墓碑〕随谓词删了（第 2 个消费者 `acct_iso_deploy` 退役，只剩零个）。

// ===== T04 审计① 上传读回判据（此前这条路完全没有读回）=====

// 读回那一趟住本机后端（它交回**比对的事实**：读回长度 · 首个差异，读不回 ⇒ `None`；
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
    // 「下次会重来」那半句挪到了 `upload_verified`（它当场删掉传坏的那一份），这里只说坏在哪。
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

// 这里原来是远端 profile 读取那一族的四条判据（读不出不当空文件 · 非 UTF-8 拒 ·
//   有字节读到空拒 · `RemoteFile::read` 走 fail-safe 读取器）。被测对象 `interpret_profile_read`〔散文墓碑〕/
//   `read_profile_text`〔散文墓碑〕/ `RemoteFile`〔散文墓碑〕随 `fenced_block::apply`〔散文墓碑〕一起删了：它们只剩
//   远端 `ccm` 入口一个用户，而那一处改走 `upload_verified`（部署物按字节比，不按文本读）。用户文件（rc）的读
//   经那台后端的 `files-peek`，「不存在 / 读不出 / 有字节读到空」那三分由后端答（`user_files::Peeked`）。

// `rollback_note_matches_what_actually_happened` 搬走了〔散文墓碑〕
// —— 措辞的住址从远端独有的那一份换成了本机远端共用的 `fenced_block::undo_note`〔散文墓碑〕，
// 判据跟着住到 `fenced_block_tests.rs` 那条「撤的措辞只说真发生的事」；那一族后来随序列一起删了。

/// **结构性守卫**：两条 deploy 路径的**内容**上传必须走 verified。
///
/// 范围只覆盖 `deploy_remote_backend` 函数体（另一个 `deploy_remote_acct_iso`〔散文墓碑〕 退役了）
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
                include_str!("../../../src/frontend/shell/src/sftp.rs"),
                "pub async fn deploy_remote_backend(",
            ),
            "deploy_remote_backend",
        ),
        // `deploy_remote_acct_iso`〔散文墓碑〕 那一格随命令删了：cc-acct-iso 的字节随后端二进制走（后端 `files-put` 逐份 CAS 写）。
    ];
    let mut verified_total = 0usize;
    for (b, what) in checks {
        let code = b
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 反向自检：真取到函数体了。上传经本机后端（`upload_verified` 读回比对 · `put_marker` 只写标记）。
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
    // 计数自检：按钮那条路体内 1 处后端二进制（acct-iso 那 6 份随命令退役：7 → 1）。
    assert_eq!(
        verified_total, 1,
        "期望 1(backend 体内)，实得 {verified_total}"
    );
}

// `the_sftp_dependency_is_really_on_russh_sftp_three` 搬去了后端（`tests/backend/dial_sftp_tests.rs`）：
//   `russh-sftp` 出了界面清单（界面进程零 SFTP），今天只在 `src/backend/Cargo.toml` 里 —— 判据跟着依赖走，读后端那份清单与 lock。

/// 要求：「PATH 上放什么：后端二进制本身，名字叫 `ccm`；落点 `~/.cc-monitor/bin/ccm` —— 本机与远端同一个；
/// 不要的三样：① 转发 shim · ② 软链 · ③ 与后端重复的第二份字节」· 「一个二进制，落 ~/.cc-monitor/bin/ccm（它自己就是 ccm，没有 shim）」。
///
/// 两向：部署两条路（自动 · 按钮）往落点写的恰是后端字节（`upload_verified(&fs, LANDING_REL, bin.bytes` 各恰一处），
/// 生产段里**零处**再造入口（shim 的记号只许作为「认旧的」出现在 `ccm_legacy.rs`）；落点常量与后端那一侧同源（`relay_route_core`）。
#[test]
fn the_landing_holds_the_backend_bytes_and_nothing_else_is_put_there() {
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    for sig in [
        "pub async fn ensure_backend_deployed(",
        "pub async fn deploy_remote_backend(",
    ] {
        let code = dp1_body(&prod, sig);
        guard_core::find_pinned(&code, "upload_verified(&fs, LANDING_REL, bin.bytes, 0o700)")
            .unwrap_or_else(|e| panic!("{sig}：落点上放的不是后端字节（{e}）"));
    }
    for gone in [
        "ccm_entry_shim",
        "put_ccm_entry",
        "CCM_CLI_REMOTE_PATH",
        "exec {} ccm",
    ] {
        assert!(!prod.contains(gone), "sftp.rs 又在造 ccm 入口了：{gone}");
    }
    assert_eq!(LANDING_REL, relay_route_core::BACKEND_LANDING_REL);
    assert_eq!(LANDING_REL, ".cc-monitor/bin/ccm");
}

/// **界面那一侧对着真后端 ＋ 真 sshd**：部署那几问经 `RemoteFs`（`files` 链路）、
/// 传输经中继（`sftp_pool::transfer_call` / `watch_ticket`），全程界面进程零 SSH。
///
/// 只由 `tests/evidence/SR1b-sftp-loopback.py --monitor` 带 `SR1B_LOOPBACK`
/// （`{host,port,user,key_path,backend,home,rhome,up,dl_remote,dl_local}`）来跑；那台 sshd 的 sftp 起始目录是临时的 `rhome`，
/// 写不到真 home。买到：部署判定四形（缺 ⇒ 部署 · 装完 ⇒ 那台 sshd 上真扫出戳、跳过 · 截成 0 字节 ⇒ 重部署 ·
/// 无戳的文件 ⇒ 显式失败不覆盖）·旧三行入口 ⇒ 认出来、换成后端本体 ·
/// 卸载按钮删后端那一份 · 两个写根之外 ⇒ 后端围栏拒、原话带回 · 上传 / 下载经中继走完、帧翻成 `Snap` 终局。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "要真 sshd ＋ 真后端二进制：由 tests/evidence/SR1b-sftp-loopback.py --monitor 带环境变量来跑"]
async fn sr1b_loopback_deploy_and_transfer_through_the_resident_backend() {
    use futures::StreamExt;
    let _local = crate::inbound_client::local_origin_test_lock();
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
    std::thread::spawn(move || crate::local_backend::local_stdio_consumer(stdin, stdout));
    // 落点是固定的 `~/.cc-monitor/bin/ccm`（SFTP 那一侧家目录相对；台架的 sshd 要把 shell 的 `HOME` 也设成 `rhome`，
    //   身份扫描那一发走 shell、读的是 `"$HOME"/.cc-monitor/bin/ccm`）。
    let backend_path = format!("{rhome}/{LANDING_REL}");
    let cfg = crate::ssh_source::RemoteConfig {
        host: s("host"),
        label: "sr1b-loopback".into(),
        port: v["port"].as_u64().unwrap() as u16,
        user: s("user"),
        key_path: Some(s("key_path")),
        host_key_fingerprint: None,
        addresses: vec![],
        jump: None,
    };
    // ① 部署那几问（判定在本机常驻后端：`deploy-plan` 沿同一条 SSH 问那台；放字节经 `files` 链路）
    let fs = RemoteFs::open(&cfg).await.expect("开不了 files 链路");
    // 身份读那份字节自己的戳（那台 sshd 上真跑一次只读扫描），不读旁挂标记。
    //   送去的字节里埋一段戳（界标取自 `build.rs` 交来的 env），其余是 3 MB 的噪声。
    let stamp = format!(
        "{}sr1b-id{}",
        env!("BACKEND_STAMP_OPEN"),
        env!("BACKEND_STAMP_CLOSE")
    );
    let mut bytes: Vec<u8> = (0..3_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
    bytes.splice(1_000_000..1_000_000, stamp.bytes());
    // 这一版「带着」的那一格就是台架那台（本机 sshd）的键，自报 `sr1b-id`（与送去的字节同一个戳）。
    let mine = [(
        deploy_contract::Key::this_machine().expect("台架那台的键认不出"),
        "sr1b-id",
    )];
    let decide = || async { ask_plan_for(&cfg, &mine).await.map(|p| p.action) };
    // 读数脚本 ② 在这个落点上留下了一份 3 MB 的随机字节（没有身份戳）⇒ 那台 sshd 上真扫一次：
    //   显式失败、一个字节都不写（盘上那份原样）；出路是机器页「卸载后端」—— 这里就用那颗按钮的真命令删掉它。
    let leftover = std::fs::read(&backend_path).expect("读数脚本 ② 留下的那份不在 —— 台架变了");
    let d_pre = decide().await;
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
    assert!(msg.starts_with("已删除 ~/.cc-monitor/bin/ccm"), "{msg}");
    let d0 = decide().await;
    assert!(
        matches!(d0, Ok(DeployAction::Deploy(_))),
        "落点缺 ⇒ 该部署：{d0:?}"
    );
    fs.mkdirs(remote_parent(LANDING_REL)).await.unwrap();
    upload_verified(&fs, LANDING_REL, &bytes, 0o700)
        .await
        .expect("上传 ＋ 读回");
    assert_eq!(
        decide().await,
        Ok(DeployAction::Skip),
        "装完 ⇒ 该跳过（那台上那份的戳没被扫出来就不会是这一格）"
    );
    assert_eq!(
        std::fs::read(&backend_path).unwrap(),
        bytes,
        "盘上那份不是送去的字节"
    );
    std::fs::write(&backend_path, b"").unwrap();
    let d2 = decide().await;
    assert!(
        matches!(d2, Ok(DeployAction::Deploy(_))),
        "截成 0 字节 ⇒ 该重部署：{d2:?}"
    );
    // 落点上是一份不说自己是谁的东西 ⇒ 显式失败、不覆盖（判定层不写；盘上那份原样）。
    std::fs::write(&backend_path, b"#!/bin/sh\necho not ours\n").unwrap();
    let d3 = decide().await;
    assert!(
        matches!(&d3, Err(e) if e.contains("不说自己是哪一版")),
        "无戳的文件 ⇒ 该显式失败：{d3:?}"
    );
    // ② 落点上是旧版放的三行入口 ⇒ 认得出、判「换成后端本体」。
    std::fs::write(
        &backend_path,
        "#!/bin/sh\n# cc-monitor: ccm = 后端本体的一次性模式（K33：所有命令只许有一处）\nexec '/x/cc-monitor-backend' ccm \"$@\"\n",
    )
    .unwrap();
    let d4 = decide().await;
    assert!(
        matches!(d4, Ok(DeployAction::Deploy(_))),
        "旧入口 ⇒ 该换成后端本体：{d4:?}"
    );
    upload_verified(&fs, LANDING_REL, &bytes, 0o700)
        .await
        .unwrap();
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
    // ④ 卸载按钮（真命令）：只删后端那一份（旁挂标记退役）
    let msg = uninstall_remote_backend(cfg.clone()).await.expect("卸载");
    assert!(msg.starts_with("已删除 ~/.cc-monitor/bin/ccm"), "{msg}");
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
    // 上传那一路的传完带整份摘要（64 位十六进制；算法对不对由后端那一侧对拍 `sha2`）。
    assert!(
        matches!(&last.end, Some(crate::sftp_pool::End::Done { bytes, sha256: Some(h) })
            if *bytes == want.len() as u64 && h.len() == 64),
        "{:?}",
        last.end
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

// ═══ 自动部署不再静默 ═══════════════════════════════════════════════════
//
// 要求：「拒绝是一个会到达用户的结论，不是一行 `debug` 日志」·
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
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
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
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/ssh_source.rs"
    ));
    let at = guard_core::find_pinned(&prod, "crate::sftp::ensure_backend_deployed(cfg).await")
        .unwrap_or_else(|e| panic!("调用处不是恰好一处：{e}"));
    let rest = &prod[at..];
    let err_at = rest.find("Err(e) =>").expect("找不到 Err 那一支");
    let arm = &rest[err_at..];
    let arm = &arm[..arm.find("\n        }\n").unwrap_or(arm.len())];
    guard_core::find_pinned(arm, "kind: \"deploy\"")
        .unwrap_or_else(|e| panic!("Err 那一支没有恰好一条 `kind: \"deploy\"`（{e}）：\n{arm}"));
    guard_core::find_pinned(arm, "health(payload)")
        .unwrap_or_else(|e| panic!("Err 那一支没有恰好一次发到远端健康通道（{e}）：\n{arm}"));
    assert!(arm.contains("e.say()"), "发出去的不是那句话本身：\n{arm}");
    assert!(
        arm.contains("None"),
        "Err 那一支不再「不阻断、按没确认处理」：\n{arm}"
    );
}

/// 读回比对不对 ⇒ **当场删掉传坏的那一份**（旁挂标记退役后，断链那一环就是这一删）。
///
/// 部署决策的对照物是那份字节自报的身份 ⇒ 一份传坏却恰好还带着对的戳的字节，
/// 下次会被判「已是这一版」—— 删掉它，下次就是「落点没有 ⇒ 装」。
/// 形态判据（`upload_verified` 要经真 `RemoteFs`，单测起不了那条链路；行为那一半在回环 sshd 读数里没有现成的坏读回可造）：
/// 读回判定的 `Err` 那一支里恰好一次 `fs.remove(remote_path)`，且排在 `fs.put(` 之后。
#[test]
fn a_bad_readback_removes_the_upload_it_just_made() {
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    let body = dp1_body(&prod, "pub(crate) async fn upload_verified(");
    let put = guard_core::find_pinned(&body, "fs.put(remote_path").expect("上传那一问不在了");
    let bad = guard_core::find_pinned(&body, "let Err(bad) = verify_readback(")
        .expect("读回判定那一支不在了");
    let rm = guard_core::find_pinned(&body, "fs.remove(remote_path)")
        .unwrap_or_else(|e| panic!("读回不对那一支没有恰好一次删掉那一份（{e}）：\n{body}"));
    assert!(put < bad && bad < rm, "次序不是「传 → 判 → 删」：\n{body}");
}

/// 🔴 **A1：别名块的真相不住 `sftp.rs`**（`audit/B-decouple.md` §2 第 12 条：
/// 「别名块真相从 `sftp.rs` 搬到别名域；`sftp.rs` 已不做 SFTP」；「渲染 · 读回 · 启动文件归方言，
/// 写入 … 那台机器后端的文件管理那一面」—— 部署那一族管不着别名块）。
///
/// 两向：那 6 个符号的**定义**与两条别名块命令在 `sftp.rs` 生产段里**零命中**；正控：同一把尺子
/// （`find_pinned`：恰好一处 ＋ 两侧边界）量 `profile_installer.rs`，8 个全量得出。
/// 人群会怎么长：别的路往 `sftp.rs` 加一个别名块相关的定义 ⇒ 这里不红（名单是这 8 个）——
/// 它钉的是「这几样搬走了、没搬回来」，不是「`sftp.rs` 里没有任何与 rc 有关的东西」。
#[test]
fn the_alias_block_truth_no_longer_lives_in_sftp() {
    let sftp = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    // 别名块的真相再搬一次：进了那台后端（`src/backend/assets/aliases/block.rs`）。
    let alias_home =
        guard_core::production_code(include_str!("../../../src/backend/assets/aliases/block.rs"));
    let defs = [
        "pub(crate) const CCM_PROFILE_BEGIN: &str",
        "pub(crate) const CCM_PROFILE_END: &str",
        "pub(crate) const CCM_WRAPPER_SNIPPET: &str",
        "pub(crate) fn merge_profile_block(",
        "pub(crate) fn strip_profile_block(",
        // 远端装 / 卸那两条命令删了（并进 `aliases_block_*`），名单 8 → 6。
        // 别名块不再定义别名（`cc` / `cct` / `cca` 进了清单）⇒ 「块里定义了哪几个名字」那个函数删了，6 → 5。
    ];
    let stayed: Vec<&str> = defs.iter().copied().filter(|d| sftp.contains(d)).collect();
    assert!(
        stayed.is_empty(),
        "别名块的真相又住回 `sftp.rs` 了：{stayed:?} —— 那份文件只管部署"
    );
    for d in defs {
        guard_core::find_pinned(&alias_home, d).unwrap_or_else(|e| {
            panic!("正控：`assets/aliases/block.rs` 里量不出 `{d}`（{e}）—— 这把尺子是瞎的")
        });
    }
}

// ═══ 部署只升不降：`BUILD_ID` 可比序 ═══════════════════════════════════
//
// 要求住址：主会话 4D 裁 D-b 逐字「多个 monitor 连同一远端：部署只在「我的比盘上的新」时才换（BUILD_ID 可比序）」；
// 它改写规矩 2「对就复用，不对就换」与「恰一个戳 ≠ ⇒ 换」那一格。
// 审计 `E-compat.md` §E3（两个不同版本的 monitor 连同一台远端，互相重部署）。

/// 🔴 B2b：自动部署那条路遇到「不动」⇒ 回**那台上的**身份（源码切臂：`Keep` 臂交出 `Some(theirs)`，末尾只在没有时才回这一版 id）。
#[test]
fn hx2_keeping_a_newer_backend_reports_its_identity_not_ours() {
    let prod = guard_core::production_code(include_str!("../../../src/frontend/shell/src/sftp.rs"));
    let f = guard_core::find_pinned(&prod, "pub async fn ensure_backend_deployed(")
        .expect("自动部署那个函数不在了");
    let body = &prod[f..];
    let arm = guard_core::find_pinned(body, "DeployAction::Keep { theirs, why } => {")
        .expect("自动部署那条路没有 Keep 臂");
    let arm_end = arm + body[arm..].find("\n        }").expect("Keep 臂没收尾");
    let arm_body = &body[arm..arm_end];
    // Keep 臂之后还要扫一次旧落点 ⇒ 臂里交出那台上的身份、函数末尾回它（不再在臂里直接 return）。
    assert_eq!(
        arm_body.matches("Some(theirs)").count(),
        1,
        "Keep 臂没回那台上的身份：{arm_body}"
    );
    guard_core::find_pinned(
        body,
        "Ok(theirs.unwrap_or_else(|| bin.build_id.to_string()))",
    )
    .expect("自动部署末尾回的不是「那台上的身份，没有才是这一版」");
    assert!(!arm_body.contains("upload_verified"), "Keep 臂里写了字节");
}
