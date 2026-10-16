use super::*;

// 这里原来是「删远端文件的入口真的过了围栏吗」（喂 `/etc/passwd` 给 SFTP 直删、
//   要求零网络就被结构守卫拒）。F11 改经远端后端删（`files-delete-session`，只收 sid）之后，
//   那条 SFTP 直删与它的守卫一起走了；「只收 sid · 落点由后端按 sid 找」的判据住后端。

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
    use crate::ccm_cli_contract as contract;

    //
    // ⚠ **它们守的性质没有一条被丢掉，逐条给新住址**：
    //    · `=名:` 精确目标 ⇒ `control::ccm::plan` 的渲染判据（`--print` 黄金串里每个
    //      `-t` 都是 `'=名:'`，变异刀 #8「attach 目标退回裸名字」当场红）;
    //    · 通道A 写**意图**标记 `@ccm_sid_expect` ⇒ 变异刀 #7「写事实标记而非意图标记」;
    //    · `$t` 不许二次赋值 ⇒ 那是 bash 变量的病，Rust 里没有那个形状（`Plan` 里是字段）。
    //    ⚠ 「跨语言那一半」（TS 侧 `deriveTmuxName` 对拍 · `capabilities ⊇ CLI_REQUIRED_CAPS`）
    //      仍**只**住 e2e（`ccm-cli.test.sh` 5 条 · `ccm-contract-parity.sh` 5 条），别当 Rust 判据能顶。
}

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
        assert!(
            bin.build_id.is_some_and(|id| !id.is_empty()),
            "内嵌了字节却问不出身份：{:?}",
            bin.build_id
        );
        assert_eq!(
            bin.build_id,
            crate::byte_table::my_backend_id(),
            "这一槽的身份不是「我这一版」"
        );
        assert_eq!(&bin.bytes[..4], b"\x7fELF", "{arch} 应是 ELF");
        assert!(bin.bytes.len() > 100_000, "{arch} 体积应非平凡");
    }
    assert!(key_of("Linux", "riscv64").is_err(), "未知 arch → 表外");
}

/// ★★ 🔴 `K-R70`（09-12）：**内嵌那份的身份只许来自它自己的字节。**
///
/// # 它取代了什么，以及为什么不是「换个写法」
///
/// 今天身份**只有一条来路**：`build.rs` 从二进制字节里扫 `CC_MONITOR_BUILD_STAMP`。
/// 于是本条钉三件事：
///
/// 1. 每一槽的 `build_id` **只许**是 `my_backend_id()`（`build.rs` 从内嵌字节里扫出、几份彼此相等的那一个）
///    —— 出现字面量、或退回源码 id（老 `pick()` 那条「问不出就拿源码顶上」的路）都红；
/// 2. 字节出门前的自检住后端（`deploy::install`，照计划核那一格自报的身份）；
/// 3. 界标那两个字面量**不许**在本文件里出现第二份（闭集唯一住址在契约 crate）。
///
/// 顺带钉住 arch 那条跨文件契约的**另一半**：`build.rs` 期待的每个 arch，
/// `byte_table.rs` 里都必须真有一槽（漏一个 ⇒ 取字节口对它返回 `None`，
/// 远端自动部署对那个 arch **悄悄关闭** —— 与上一条判据守的是同一个事故形状的两端）。
#[test]
fn the_embedded_identity_comes_from_the_bytes_not_from_a_label() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join("src/deploy.rs")).expect("读不到 deploy.rs");
    let prod = guard_core::production_code(&src);
    // 两份 musl 的槽与它们的身份取值口搬进了 `byte_table.rs`（全仓唯一的取字节口）；
    //   ①④ 读那一份，③ 两份都读。
    let table_src =
        std::fs::read_to_string(root.join("src/byte_table.rs")).expect("读不到 byte_table.rs");
    let table = guard_core::production_code(&table_src);
    // 运行时拼，免得命中本条自己的说明文字。
    let field = format!("{}_id:", "build");
    let inits: Vec<&str> = table
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with(&field))
        .collect();
    assert_eq!(
        inits.len(),
        3,
        "生产段里找到 {} 处 `{field}` 取值（应当 3：X86 / ARM / 本机原生）—— \
             抽取器坏了或那几槽被改写了，本条会零命中地绿：{inits:?}",
        inits.len()
    );
    for l in &inits {
        assert_eq!(
            *l, "build_id: my_backend_id(),",
            "这一格的身份不是从**字节**来的：{l}\n\
                 ★ 只有 `my_backend_id()`（`build.rs` 从内嵌字节里扫出的 `BACKEND_EMBEDDED_ID`）是字节自报的；\n\
                 写字面量、或写一份 `.build_id` 旁文件再读它，是把标签换个地方抄（`KR70D1` 逐字点名的失效方向）。"
        );
    }
    // ② 字节出门前的自检住后端（`deploy::install` 照计划核那一格自报的身份，判据 `install_tests` 那一条）。
    // ③ 界标闭集只有一个住址（契约 crate），这两份生产段只许经 `deploy_contract` 取。
    for mark in [deploy_contract::STAMP_OPEN, deploy_contract::STAMP_CLOSE] {
        assert!(
            !mark.is_empty(),
            "契约 crate 的界标是空串 —— 空界标会让身份自检恒答「问不出」⇒ 自动部署整个静默关闭"
        );
        for (name, code) in [("byte_table.rs", &table), ("deploy.rs", &prod)] {
            assert!(
                !code.contains(&format!("\"{mark}\"")),
                "{name} 生产段里出现了界标字面量 `{mark}` —— 闭集唯一住址在契约 crate\n\
                 （`deploy_contract::STAMP_OPEN` / `STAMP_CLOSE`），这里只许引那两个常量。"
            );
        }
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
            table.contains(&format!("\"/backend-{arch}\"")),
            "`build.rs` 会为 `{arch}` 把二进制拷进 `OUT_DIR/backend-{arch}`，\n\
                 而 `byte_table.rs` 生产段里没有对应的那一槽 ⇒ 取字节口对 (Linux, {arch}) 返回 `None`，\n\
                 **远端自动部署对这个 arch 悄悄关闭**（`build.rs` 那侧只 `cargo:warning=`，不会红）。\n\
                 与「发版流水线要为每个 arch 备料」那条守的是同一个事故形状的两端。"
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
        // `root` 是 crate 根（`<repo>/src/frontend/shell`）⇒ 再爬**一级**
        // 只到 `<repo>/src`。仓根要爬两级，走唯一住址。
        &std::fs::read_to_string(
            guard_core::addr::repo_root().join(".github/workflows/release.yml"),
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
            //    `K-R68`），身份改从字节里扫。
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
    //   · `const STAMP_OPEN` —— 界标是**从契约 crate 的源码抠的**，不是在 yml 里手抄一份
    //     （手抄那一份哪天与源码漂开，校验会以「假红」的形式提醒错人）。
    for (needle, why) in [
        (
            "ReadAllBytes",
            "校验那一步没有把二进制的字节读进来 —— 那它验的就不是这份字节，\
                 而是它旁边的某个文件（`K-R70` 整件治的就是这个）",
        ),
        (
            "const STAMP_OPEN",
            "身份戳的界标不是从契约 crate 的源码抠的 —— 手抄一份就多一个会漂的住址；\
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

// ═══ 装那一趟整趟在本机常驻后端，本模块只交事实 ═══════════════════════
//
// 要求：「部署决策（该不该换 · 换成什么 · 身份判定）与那一趟装都在后端；monitor 只交字节文件的路径、收进度」。

/// 🔴 两条部署路（自动 · 按钮）各**恰好一次**请本机常驻后端装（`install(`），本文件生产段里**零处**再做判定或碰那台
/// （问那台 `uname` · 查表 A/B · 扫身份戳 · 判新旧 · 判落点 · 放字节 · 删残件），也零处碰旁挂标记。带正控。
#[test]
fn the_deploy_paths_only_hand_facts_to_the_backend() {
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/deploy.rs"));
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
            "identity_of_bytes(",
            "build_relation(",
            "connect_and_exec_capture(",
            "fs.put(",
            "fs.mkdirs(",
        ]
        .into_iter()
        .filter(|f| code.contains(f))
        .collect()
    };
    assert_eq!(
        decisions(&prod),
        Vec::<&str>::new(),
        "deploy.rs 又自己判起了部署 / 自己放起了字节 —— 那一趟住本机常驻后端（`deploy-install`）"
    );
    for sig in [
        "pub async fn ensure_backend_deployed(",
        "pub async fn deploy_remote_backend(",
    ] {
        let code = dp1_body(&prod, sig);
        guard_core::find_pinned(&code, "install(")
            .unwrap_or_else(|e| panic!("{sig}：不是恰好一处请后端装（{e}）"));
        for marker in [
            "/.build_id",
            "read_marker(",
            "put_marker(",
            "deploy_decision(",
        ] {
            assert!(!code.contains(marker), "{sig} 又碰起了旁挂标记：{marker}");
        }
    }
    let body = dp1_body(&prod, "async fn install_for(");
    guard_core::find_pinned(&body, ".call(INSTALL_CMD, args, INSTALL_BUDGET)")
        .unwrap_or_else(|e| panic!("装那一口不是恰好一处问本机后端（{e}）"));
    // 正控：每一种写法都认得出来。
    assert_eq!(
        decisions("probe_key( choose( key_from_uname( judge( stamp_scan_cmd( interpret_stamp_scan( interpret_target_probe( identity_decision( identity_of_bytes( build_relation( connect_and_exec_capture( fs.put( fs.mkdirs(").len(),
        13
    );
}

/// 应答的解码器读后端那一侧同一份金样（`tests/__fixtures__/deploy-install.golden.json`，后端 `deploy_plan_tests` 核键集 == 真产出）；
/// 严格收：少一格 · 多一格 · 认不出的动作 · 空身份都是错（两侧漂了当场说出来）。
#[test]
fn the_reply_decoder_reads_the_golden() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/deploy-install.golden.json"
    ))
    .unwrap();
    let product = &golden["product"];
    let got = decode_installed(product).expect("金样解不开");
    assert_eq!(
        (
            got.did,
            got.build.as_str(),
            got.os.as_str(),
            got.said.as_str()
        ),
        (Did::Keep, "p0b-placeholder", "Linux", "placeholder")
    );
    let with = |k: &str, v: serde_json::Value| {
        let mut x = product.clone();
        x[k] = v;
        x
    };
    let mut missing = product.clone();
    missing.as_object_mut().unwrap().remove("said");
    let mut extra = product.clone();
    extra["surprise"] = serde_json::json!(1);
    for bad in [
        missing,
        extra,
        with("action", serde_json::json!("overwrite")),
        with("build", serde_json::json!("")),
        with("ack", serde_json::json!("x")),
    ] {
        assert!(decode_installed(&bad).is_err(), "该拒却收了：{bad}");
    }
    assert_eq!(
        decode_installed(&with("action", serde_json::json!("skip"))).map(|i| i.did),
        Ok(Did::Skip)
    );
}

/// 进度帧解码读同一份金样里的 `step`：那台 · 哪一步 · 那一句（后端写好的）原样进机器状态那一格。
#[test]
fn the_step_frame_reads_the_golden() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/deploy-install.golden.json"
    ))
    .unwrap();
    let got = crate::stream_source::decode(&golden["step"].to_string()).ok();
    assert_eq!(
        got,
        Some(cmd_wire::frame::Frame::Deploy(cmd_wire::frame::Deploy {
            machine: "placeholder".into(),
            step: cmd_wire::frame::DeployStep::Update,
            said: "placeholder".into(),
        }))
    );
}

/// 🔴装那一趟在本机后端里拨号 ⇒ 它的逐地址指纹随应答交回、由 monitor 按**同一个**判定固化
/// （`dial_host::settle_host_key`，monitor 自己开链路那几条也走它），不另写一份。两向：解码器把金样里的 `ack` 原样收进来；
/// 装那一口恰好一处把它交给 `settle_host_key`。
#[test]
fn the_install_ack_is_pinned_by_the_same_judgement_as_every_other_dial() {
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/deploy-install.golden.json"
    ))
    .unwrap();
    let got = decode_installed(&golden["product"]).expect("金样解不开");
    assert_eq!(
        got.ack.fingerprints.get("192.0.2.2:22").map(String::as_str),
        Some("SHA256:placeholder"),
        "解码器没把 ack 的逐地址指纹收进来：{:?}",
        got.ack
    );
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/deploy.rs"));
    let body = dp1_body(&prod, "async fn install_for(");
    guard_core::find_pinned(
        &body,
        "crate::dial_host::settle_host_key(cfg, &dial, &got.ack)",
    )
    .unwrap_or_else(|e| panic!("装那一口没把 ack 交给固化判定（{e}）：\n{body}"));
}

// `the_sftp_dependency_is_really_on_russh_sftp_three` 搬去了后端（`tests/backend/dial_sftp_tests.rs`）：
//   `russh-sftp` 出了界面清单（界面进程零 SFTP），今天只在 `src/backend/Cargo.toml` 里 —— 判据跟着依赖走，读后端那份清单与 lock。

/// 要求：「PATH 上放什么：后端二进制本身，名字叫 `ccm`；落点 `~/.cc-monitor/bin/ccm` —— 本机与远端同一个」。
/// 落点上放的是那一格后端字节本身（后端 `install_tests` 钉）；生产段里**零处**再造入口；落点常量与后端那一侧同源（`relay_route_core`）。
#[test]
fn the_landing_constant_is_the_backend_one_and_no_entry_is_made_here() {
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/deploy.rs"));
    for gone in [
        "ccm_entry_shim",
        "put_ccm_entry",
        "CCM_CLI_REMOTE_PATH",
        "exec {} ccm",
    ] {
        assert!(!prod.contains(gone), "deploy.rs 又在造 ccm 入口了：{gone}");
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
/// 无戳的文件 ⇒ 显式失败不覆盖；从前那份三行入口也一样）·
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
        .env_remove("CCM_LISTEN_TOKEN_FILE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("起不了后端");
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    std::thread::spawn(move || {
        crate::local_backend::local_stdio_consumer(
            &crate::local_backend::StdioRoute::local(),
            stdin,
            stdout,
        )
    });
    // 落点是固定的 `~/.cc-monitor/bin/ccm`（SFTP 那一侧家目录相对；台架的 sshd 要把 shell 的 `HOME` 也设成 `rhome`，
    //   身份扫描那一发走 shell、读的是 `"$HOME"/.cc-monitor/bin/ccm`）。
    let backend_path = format!("{rhome}/{LANDING_REL}");
    let cfg = crate::stream_source::RemoteConfig {
        host: s("host"),
        label: "sr1b-loopback".into(),
        port: v["port"].as_u64().unwrap() as u16,
        user: s("user"),
        key_path: Some(s("key_path")),
        host_key_fingerprint: None,
        addresses: vec![],
        jump: None,
    };
    // ① 装那一趟整趟在本机常驻后端（`deploy-install`）：这一版「带着」的那一格就是台架那台的键、字节是被测的那份后端本身
    //   （它得真跑得起 `--place-self`），身份读它自己的戳。
    let fs = RemoteFs::open(&cfg).await.expect("开不了 files 链路");
    let me = std::fs::read(s("backend")).expect("读不到被测后端");
    let deploy_contract::RemoteIdentity::Stamp(id) = deploy_contract::identity_of_bytes(
        &me,
        deploy_contract::Marks {
            open: deploy_contract::STAMP_OPEN,
            close: deploy_contract::STAMP_CLOSE,
        },
    ) else {
        panic!("被测后端不说自己是哪一版");
    };
    let key = deploy_contract::Key::this_machine().expect("台架那台的键认不出");
    let slots = vec![serde_json::json!({
        "os": key.os.label(), "arch": key.arch.label(), "id": id, "path": s("backend"),
    })];
    let decide = || async { install_for(&cfg, slots.clone()).await.map(|i| i.did) };
    // 读数脚本 ② 在这个落点上留下了一份 3 MB 的随机字节（没有身份戳）⇒ 那台 sshd 上真扫一次：
    //   显式失败、一个字节都不写（盘上那份原样）；出路是机器页「卸载后端」—— 这里就用那颗按钮的真命令删掉它。
    let leftover = std::fs::read(&backend_path).expect("读数脚本 ② 留下的那份不在 —— 台架变了");
    let d_pre = decide().await;
    assert!(
        matches!(&d_pre, Err(e) if e.said.contains("不说自己是哪一版")),
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
    assert_eq!(decide().await, Ok(Did::Deploy), "落点缺 ⇒ 该装");
    assert_eq!(
        std::fs::read(&backend_path).unwrap(),
        me,
        "盘上那份不是送去的字节"
    );
    assert_eq!(
        decide().await,
        Ok(Did::Skip),
        "装完 ⇒ 该跳过（那台上那份的戳没被扫出来就不会是这一格）"
    );
    std::fs::write(&backend_path, b"").unwrap();
    assert_eq!(decide().await, Ok(Did::Deploy), "截成 0 字节 ⇒ 该重装");
    // 落点上是一份不说自己是谁的东西 ⇒ 显式失败、不覆盖（盘上那份原样）。
    std::fs::write(&backend_path, b"#!/bin/sh\necho not ours\n").unwrap();
    let d3 = decide().await;
    assert!(
        matches!(&d3, Err(e) if e.said.contains("不说自己是哪一版")),
        "无戳的文件 ⇒ 该显式失败：{d3:?}"
    );
    std::fs::write(&backend_path, &me).unwrap();
    // ③ 暂存区之外 ⇒ 后端围栏拒、原话带回、盘上零改动
    let outside = format!("{rhome}/.cc-monitor/elsewhere/cc-monitor-backend");
    let e = fs.put(&outside, b"x", 0o700, true).await.unwrap_err();
    assert!(
        e.said.contains("~/.cc-monitor/staging/"),
        "拒绝的话没说只许哪一处：{e}"
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
        &serde_json::json!({ "items": [{ "local_path": up }], "remote_dir": "/tmp" }),
    )
    .await
    .expect("开单（上传）");
    let (id, key) = (
        r["items"][0]["id"].as_str().unwrap(),
        r["items"][0]["staged"].as_str().unwrap(),
    );
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
        &serde_json::json!({
            "items": [{ "remote_path": s("dl_remote"), "name": std::path::Path::new(&dl_local).file_name().unwrap().to_string_lossy() }],
            "local_dir": std::path::Path::new(&dl_local).parent().unwrap().to_string_lossy(),
            "on_exists": "overwrite",
        }),
    )
    .await
    .expect("开单（下载）");
    let last = crate::sftp_pool::watch_ticket(&origin, r["items"][0]["id"].as_str().unwrap())
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

/// D1：`ensure_backend_deployed` 的返回类型是 `Result<String, Said>`（没有「没部署也算成功」那一支），
/// 体内零 `Ok(None)` · 零 `debug!`；后端的失败原样往上交（`?`）。
#[test]
fn the_auto_deploy_never_refuses_silently() {
    let prod =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/deploy.rs"));
    let body = dp1_body(&prod, "pub async fn ensure_backend_deployed(");
    let sig = body.lines().next().unwrap_or_default();
    assert!(
        sig.contains("-> Result<String, Said>"),
        "返回类型变了：{sig} —— `Option` 回来就是「没部署也算成功」那一支回来了"
    );
    assert_eq!(
        dp1_silent_forms(&body),
        Vec::<&str>::new(),
        "体内又有了静默的拒绝：\n{body}"
    );
    guard_core::find_pinned(&body, "install(cfg).await?")
        .unwrap_or_else(|e| panic!("后端的失败没有原样往上交（{e}）：\n{body}"));
    // 正控：两种静默写法都认得出来。
    assert_eq!(
        dp1_silent_forms("x => return Ok(None),\n tracing::debug!(\"跳过\");"),
        vec!["Ok(None)", "debug!"]
    );
}

/// D2：调用方拿到 `Err` ⇒ 经远端健康通道恰发一条 `kind = "deploy"`（不阻断，照旧接着试连已有后端）。
#[test]
fn a_failed_auto_deploy_reaches_the_screen_through_remote_health() {
    let prod = guard_core::addr::stream_source_production();
    let at = guard_core::find_pinned(&prod, "crate::deploy::ensure_backend_deployed(cfg).await")
        .unwrap_or_else(|e| panic!("调用处不是恰好一处：{e}"));
    let rest = &prod[at..];
    let err_at = rest.find("Err(said) =>").expect("找不到 Err 那一支");
    let arm = &rest[err_at..];
    let arm = &arm[..arm.find("\n        }\n").unwrap_or(arm.len())];
    guard_core::find_pinned(arm, "kind: \"deploy\"")
        .unwrap_or_else(|e| panic!("Err 那一支没有恰好一条 `kind: \"deploy\"`（{e}）：\n{arm}"));
    guard_core::find_pinned(arm, "health(payload)")
        .unwrap_or_else(|e| panic!("Err 那一支没有恰好一次发到远端健康通道（{e}）：\n{arm}"));
    assert!(
        arm.contains("message: said.said"),
        "发出去的不是那句话本身：\n{arm}"
    );
    assert!(
        arm.contains("detail: said.detail"),
        "发出去的没带复制详情：\n{arm}"
    );
    assert!(
        arm.contains("None"),
        "Err 那一支不再「不阻断、按没确认处理」：\n{arm}"
    );
}

/// 🔴 **A1：别名块的真相不住 `deploy.rs`**（
/// 「别名块真相从 `sftp.rs` 搬到别名域；`sftp.rs` 已不做 SFTP」；「渲染 · 读回 · 启动文件归方言，
/// 写入 … 那台机器后端的文件管理那一面」—— 部署那一族管不着别名块）。
///
/// 两向：那 6 个符号的**定义**与两条别名块命令在 `deploy.rs` 生产段里**零命中**；正控：同一把尺子
/// （`find_pinned`：恰好一处 ＋ 两侧边界）量 `profile_installer.rs`，8 个全量得出。
/// 人群会怎么长：别的路往 `deploy.rs` 加一个别名块相关的定义 ⇒ 这里不红（名单是这 8 个）——
/// 它钉的是「这几样搬走了、没搬回来」，不是「`deploy.rs` 里没有任何与 rc 有关的东西」。
#[test]
fn the_alias_block_truth_no_longer_lives_in_sftp() {
    let sftp =
        guard_core::production_code(include_str!("../../../src/frontend/shell/src/deploy.rs"));
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
        "别名块的真相又住回 `deploy.rs` 了：{stayed:?} —— 那份文件只管部署"
    );
    for d in defs {
        guard_core::find_pinned(&alias_home, d).unwrap_or_else(|e| {
            panic!("正控：`assets/aliases/block.rs` 里量不出 `{d}`（{e}）—— 这把尺子是瞎的")
        });
    }
}
