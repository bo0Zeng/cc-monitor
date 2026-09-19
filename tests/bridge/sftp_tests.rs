
fn probe_cfg() -> crate::ssh_source::RemoteConfig {
    crate::ssh_source::RemoteConfig {
        host: "这个主机一定不存在-audit0805".into(),
        label: "probe".into(),
        port: 1,
        user: "nobody".into(),
        key_path: None,
        daemon_path: "/tmp/nope".into(),
        host_key_fingerprint: None,
        addresses: Vec::new(),
        jump: None,
    }
}

/// ★★ **删远端文件的入口真的过了围栏吗**〔audit-0805 08-08，Phase G 第 53 件〕。
///
/// 本文件有三条 `is_safe_remote_*` 围栏，各自都有直接的行为判据 ——
/// **但主语是围栏本身**。08-08 实测：把 `uninstall_remote_daemon` 与
/// `remove_remote_file` 里那三处 `if !is_safe_…` 全部短路，
/// **全仓 984 条判据一条不红**。而那两条路紧接着是
/// `sftp.remove_file(...)` —— **删用户远端机器上的文件**。
/// 与 F47（本机删除路）/ F48（建分支路）同一族，这次在远端。
///
/// # 这条能跑真路
///
/// 第一道围栏在 `connect_sftp` **之前**：喂一个非法远端路径 ⇒ 应当在
/// **零网络**的情况下被拒。围栏没接上的话，它会往下走去连一个不存在的主机，
/// 报的是连接错 —— 两句话分得开。
///
/// ⚠ 第二道围栏（`canonicalize` **之后**那处）跑不了真路：要到那一步得先连上。
/// 那半只能靠源码判，已写在下面并如实标注。
#[tokio::test]
async fn the_remote_delete_entry_point_actually_goes_through_the_fence() {
    let cfg = probe_cfg();
    let err = remove_remote_file(&cfg, "/etc/passwd")
        .await
        .expect_err("非法远端路径竟然没被拒 —— 围栏没接上");
    assert!(
        err.contains("refuse") || err.contains("jsonl"),
        "拒绝了，但不是围栏拒的（错误：{err}）—— \
             说明它已经越过围栏去连主机了，而下一步是 `sftp.remove_file`。"
    );
}

/// ★ 第二道围栏（canonicalize 之后）与卸载路的围栏：**源码层**判据。
///
/// ⚠ 跑不了真路（要先连上远端 / 红线不许起真连接）⇒ 只判「那行还在」。
/// **判源码是代理不是标的**（F41 记过）：挡得住「短路 / 删掉」，
/// 挡不住「围栏还在但被喂了洗过的路径」。后者进 `ROADMAP §5`。
#[test]
fn both_remote_path_sinks_still_ask_their_fence() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/sftp.rs"));
    for (f, fence) in [
        ("uninstall_remote_daemon", "is_safe_remote_daemon_path"),
        ("remove_remote_file", "is_safe_remote_jsonl"),
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

/// 单一来源漂移守卫①：写进远端 profile 的**别名块**。
/// F02 起本块只剩组合层别名；`K-R48` 第二拍起实现住后端本体
/// （远端那个 `~/.local/bin/ccm` 是 [`ccm_entry_shim`]，见下一条判据）。
#[test]
fn ccm_aliases_snippet_has_required_elements() {
    for needle in [
        ".local/bin", // CLI 落点必须进 PATH，否则别名全指向不存在的命令
        "cc()",       // 裸起（`K-R58` 起 = 就在当前目录，ccm 不再替用户挑）
        "cct()",      // tmux 版
        "ccm --tmux", // 别名只做组合，不自己建容器
        "declare -f", // 防覆盖用户已有同名函数
    ] {
        assert!(
            CCM_WRAPPER_SNIPPET.contains(needle),
            "别名块缺关键要素: {needle}"
        );
    }
    // 别名块**不得**再含实现（那是 CLI 的事；混回来就又变成两套实现）。
    for forbidden in ["__ccm_rbind()", "exec claude", "tmux new-session"] {
        assert!(
            !CCM_WRAPPER_SNIPPET.contains(forbidden),
            "别名块不该含实现细节 {forbidden}——实现属于 ~/.local/bin/ccm"
        );
    }
}

/// `KR58D2` —— `src/doc/IPC-PROTOCOL.md` §11 里描述别名块的那一句，**行数与名单同句**。
///
/// 本区最高频的那条病就是「数与名单同句、只改一半」⇒ 这里**两样一起对**，
/// 而且两样都**现算**自真相源 [`CCM_WRAPPER_SNIPPET`]（= `src/shared/ccm-aliases.sh` 本身），
/// 判据里不抄第二份名单、不写死行数。
///
/// ⚠ **它买到的射程只有这一句**：§11 其余部分（`shared/ccm` · `CCM_CLI_SCRIPT`）
/// 在 `K-R48` 第二拍之后已经是**存量馊话**，本判据够不着，也不假装够得着。
///
/// ⚠ 判据够不着被测对象时必须**响亮地红**，不许变成空真 ⇒ 找不到那一句就 panic。
#[test]
fn the_protocol_doc_sentence_about_the_alias_block_matches_the_file() {
    const IPC_DOC: &str = include_str!("../../src/doc/IPC-PROTOCOL.md");
    let want_names = crate::sftp::builtin_alias_names();
    assert!(
        !want_names.is_empty(),
        "从 src/shared/ccm-aliases.sh 里一个别名都没解析出来 —— 判据够不着被测对象了，先修判据"
    );
    let want_lines = CCM_WRAPPER_SNIPPET.lines().count();

    let sent = IPC_DOC
        .lines()
        .find(|l| l.contains("src/shared/ccm-aliases.sh`，**"))
        .expect(
            "src/doc/IPC-PROTOCOL.md 里描述别名块的那一句找不到了 —— \
                 要么它被改写了、要么被删了；无论哪种，这条对账现在是瞎的",
        );
    let bold = sent
        .split("**")
        .nth(1)
        .expect("那一句里的粗体段没了 —— 对账抓不到数与名单");

    assert!(
        bold.contains(&format!("{want_lines} 行")),
        "行数对不上：src/shared/ccm-aliases.sh 现在 {want_lines} 行，而文档那句写的是「{bold}」"
    );
    assert!(
        bold.contains(&format!("这 {} 个", want_names.len())),
        "别名个数对不上：现在 {} 个（{}），而文档那句写的是「{bold}」",
        want_names.len(),
        want_names.join(" / ")
    );
    let mut doc_names: Vec<&str> = bold.split('`').skip(1).step_by(2).collect();
    doc_names.sort_unstable();
    assert_eq!(
        doc_names, want_names,
        "名单对不上：文档那句列的是 {doc_names:?}，盘上真有的是 {want_names:?}"
    );
}

/// 单一来源漂移守卫②：部署为远端 `~/.local/bin/ccm` 的 **CLI 本体**。
///
/// 这些不是"要素清单"而是**血的教训清单**，每条对应一个真实踩过的坑：
///  - `=%s:` / `=$` ：tmux `-t` 必须精确匹配（INVARIANTS §31a）。裸目标会杀错/打错兄弟会话；
///    `=名` 无尾冒号则在 send-keys/capture-pane/set-option 上 rc=1 完全失效。
///  - `exec` ：不能省。⚠ **理由在 `U-NP④`（08-14）之后换了一条**：旧理由是
///    「身份 poller 读 `sessions/$PID.json`，不 exec 则 PID 对不上」，而那条 poller 已删
///    （身份改由 daemon 打，认的是 pidfile 自己的名字 = claude 的 PID）。今天留着它的
///    理由是「不在 agent 与终端之间多一层 shell」＋ 本 needle 本身就是部署契约。
///  - `@ccm_sid` / `@ccm_agent` ：身份随行，cc-monitor 靠它精确认会话。
///  - `@ccm_sid_expect` ：F04——通道A（建时/exec 时立即声明"打算跑这个 sid"）写这个 key，
///    与通道B（独立读会话文件确认后才写的 `@ccm_sid`；`U-NP④` 之后由 **daemon** 写）分离。破坏性动作只认 `@ccm_sid`，
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
///    （写 expect；事实由 daemon 提升，见 `U-NP④`），与兜底渲染器无交集。
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
    let shim = ccm_entry_shim("/home/pi/.cc-monitor/bin/cc-monitor-remote");
    // ① 真的把 argv 转给后端，且走的是 `intercept` 的第二条入口（子命令形）。
    assert!(
        shim.contains("exec '/home/pi/.cc-monitor/bin/cc-monitor-remote' ccm \"$@\"")
            || shim.contains("exec /home/pi/.cc-monitor/bin/cc-monitor-remote ccm \"$@\""),
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
    // 🔴 09-15 放宽了这一条的**形状**，没放宽它的**性质**：允许 `exec` 前挂
    // POSIX 的「一次性环境变量赋值」前缀（`VAR=值 exec …`）。那一形仍然是
    // 同一个进程被 `exec` 掉 —— 退出码与信号照样透传，`K33` 的「只许有一处实现」
    // 也没破（赋值不是分支、不是第二处实现）。
    // 为什么需要它：容器路要靠 `CCM_SELF` 知道「我是被当作什么叫的」，
    // 而那个值**必须在 `exec` 之前**进环境，否则进不了后端进程。
    // ⚠ 只放这一形：下面三条把「真正会吃掉退出码 / 长出第二处实现」的写法全挡住。
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
    // ③ 路径必须经 POSIX quote（daemon_path 是用户填的，可能带空格 / 引号）。
    let tricky = ccm_entry_shim("/home/用户/带 空格/it's");
    assert!(
        tricky.contains(&shell_quote_core::posix_quote("/home/用户/带 空格/it's")),
        "daemon_path 没经 `shell_quote_core::posix_quote` —— 带空格的路径会被拆成两个词。\n{tricky}"
    );
}

#[test]
fn merge_profile_block_append_replace_idempotent() {
    let snippet = "ccm() { :; }";
    // 空 existing → 仅块。
    let m1 = merge_profile_block("", snippet, "远端 ~/.bashrc").unwrap();
    assert!(m1.contains(CCM_PROFILE_BEGIN));
    assert!(m1.contains("ccm() { :; }"));
    assert!(m1.contains(CCM_PROFILE_END));

    // 无块 → 追加，原内容保留在前。
    let existing = "export PATH=/x\nalias ll='ls -l'\n";
    let m2 = merge_profile_block(existing, snippet, "远端 ~/.bashrc").unwrap();
    assert!(m2.starts_with(existing), "块外内容保留在前");
    assert!(m2.contains(CCM_PROFILE_BEGIN));

    // 幂等：同 snippet 再 merge 不变。
    assert_eq!(
        merge_profile_block(&m2, snippet, "远端 ~/.bashrc").unwrap(),
        m2,
        "merge∘merge == merge"
    );

    // 重装（换 snippet 内容）→ 整块替换，只有一个块，块外内容仍保留。
    let m3 = merge_profile_block(&m2, "ccm() { echo new; }", "远端 ~/.bashrc").unwrap();
    assert!(m3.starts_with(existing), "重装仍保留块外内容");
    assert!(
        m3.contains("echo new") && !m3.contains("{ :; }"),
        "块被整块替换"
    );
    assert_eq!(m3.matches(CCM_PROFILE_BEGIN).count(), 1, "重装不重复加块");
}

/// 审计 B1 回归：块外内容（含块**后**的用户内容）在替换时绝不丢。
#[test]
fn merge_profile_block_preserves_content_after_block() {
    let existing =
        format!("head_line\n{CCM_PROFILE_BEGIN}\nold()\n{CCM_PROFILE_END}\ntail_user_line\n");
    let m = merge_profile_block(&existing, "ccm() { echo new; }", "远端 ~/.bashrc").unwrap();
    assert!(m.contains("head_line"), "块前内容保留");
    assert!(
        m.contains("tail_user_line"),
        "块后用户内容保留（B1 不能吞掉）"
    );
    assert!(m.contains("echo new") && !m.contains("old()"), "块整块替换");
    assert_eq!(m.matches(CCM_PROFILE_BEGIN).count(), 1);
}

/// 审计 B1 核心：BEGIN 存在但其后无 END（损坏/截断）→ Err 中止，**绝不**误配前面的 END
/// 而吞掉用户内容。
#[test]
fn merge_profile_block_aborts_on_orphan_begin() {
    // END 在前、孤立 BEGIN 在后无配对 END：独立 find 会误配 → 旧实现吞内容。新实现报错。
    let corrupt = format!("{CCM_PROFILE_END}\nuser_a\n{CCM_PROFILE_BEGIN}\nuser_b\n");
    assert!(
        merge_profile_block(&corrupt, "ccm() { :; }", "远端 ~/.bashrc").is_err(),
        "孤立 BEGIN（其后无 END）必须中止而非吞内容"
    );
    // 纯孤立 BEGIN（截断的安装）→ Err。
    let truncated = format!("user_x\n{CCM_PROFILE_BEGIN}\nhalf");
    assert!(merge_profile_block(&truncated, "ccm() { :; }", "远端 ~/.bashrc").is_err());
}

/// F08b：仅当交叉编译产物已放进 embedded-daemons/（build.rs 置了 `embedded_daemons` cfg）
/// 才编译/运行——证实内嵌真生效：按 arch 取到 ELF 二进制 + build_id 非空。CI 无二进制时
/// 本测试被 cfg 掉，不误报。
#[cfg(embedded_daemons)]
#[test]
fn embedded_daemon_binaries_present_and_valid() {
    for arch in ["x86_64", "aarch64"] {
        let bin = daemon_binary(arch).expect("内嵌二进制应存在");
        assert!(!bin.build_id.is_empty(), "build_id 非空");
        assert_eq!(&bin.bytes[..4], b"\x7fELF", "{arch} 应是 ELF");
        assert!(bin.bytes.len() > 100_000, "{arch} 体积应非平凡");
    }
    assert!(daemon_binary("riscv64").is_none(), "未知 arch → None");
}

/// 🔴 `K-R70`：**那道身份见证真的会咬人** —— 四格（纯函数，不依赖内嵌产物在不在）。
///
/// ⚠ 这一条与上面那条判据分工：那条钉**接线**（有没有无条件跑），这条钉**行为**
/// （跑了会不会说真话）。少任何一条，另一条都能被一个恒答 `true` 的实现骗过去。
#[test]
fn the_build_stamp_witness_actually_bites() {
    let (o, c) = (env!("DAEMON_STAMP_OPEN"), env!("DAEMON_STAMP_CLOSE"));
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
    //   旧启发式 `bytes_contain(bytes, build_id)` 会被裸出现的 id 喂饱 —— 而 daemon
    //   的 hello 帧里本来就带着这个串 ⇒ 那条判据在任何一份 daemon 上都恒真。
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
///  （写死会让 `deploy_embedded_daemon` 里那道 `bytes_contain` 兜底整个跳过；
///   08-08 实测写死 x86_64 那处，monitor 1004 一条都不红）。
///  **它守的动作是对的，守的东西是错的**：那个「见证」见证的是**一份旁挂清单在不在**，
///  而清单是 `release.yml` 从源码常量 `const BUILD_ID` 抠出来写的 ——
///  三个载体的清单**恒等**，恒等的东西一格证据都不提供
///  （`K-R68` 摸底 · `DECISIONS.md#R26` 裁定零：那是把标签当成了指纹）。〕
///
/// 今天身份**只有一条来路**：`build.rs` 从二进制字节里扫 `CC_MONITOR_BUILD_STAMP`。
/// 于是本条钉三件事：
///
/// 1. 两个 `DaemonBinary` 的 `build_id` **只许**是 `env!("DAEMON_EMBEDDED_ID_<ARCH>")`
///    —— 出现字面量、或退回源码 id（老 `pick()` 那条「问不出就拿源码顶上」的路）都红；
/// 2. 部署路上**真的**跑了 [`bytes_carry_build_stamp`]，而且**不带前置条件**
///    （老写法 `!bin.id_from_manifest && …` 正是「有清单就整个跳过」）；
/// 3. 界标那两个字面量**不许**在本文件里出现第二份（闭集唯一住址在 daemon 源码）。
///
/// 顺带钉住 arch 那条跨文件契约的**另一半**：`build.rs` 期待的每个 arch，
/// 这里都必须真有一份 `DaemonBinary`（漏一个 ⇒ `daemon_binary()` 对它返回 `None`，
/// 远端自动部署对那个 arch **悄悄关闭** —— 与上一条判据守的是同一个事故形状的两端）。
#[test]
fn the_embedded_identity_comes_from_the_bytes_not_from_a_label() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join("src/sftp.rs")).expect("读不到 sftp.rs");
    let prod = guard_core::production_code(&src);
    // 运行时拼，免得命中本条自己的说明文字。
    let field = format!("{}_id:", "build");
    let inits: Vec<&str> = prod
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
            l.contains("env!(\"DAEMON_EMBEDDED_ID_"),
            "这一格的身份不是从**字节**来的：{l}\n\
                 ★ 只有 `DAEMON_EMBEDDED_ID_<ARCH>` 是 `build.rs` 从这份二进制的字节里\n\
                 扫出来的（`CC_MONITOR_BUILD_STAMP`）。退回 `DAEMON_BUILD_ID`（源码 id）\n\
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
        "生产段里 `{witness}` 的调用处有 {} 个（应当恰好 1：`deploy_embedded_daemon` 出门前那一道）：{calls:?}",
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
    // ③ 界标闭集只有一个住址（在 daemon 源码里），本文件只许 `env!` 取。
    for mark in [env!("DAEMON_STAMP_OPEN"), env!("DAEMON_STAMP_CLOSE")] {
        assert!(
            !mark.is_empty(),
            "`DAEMON_STAMP_OPEN/CLOSE` 是空串 —— `build.rs` 从 daemon 源码抠界标失败了，\n\
                 而空界标会让 `bytes_carry_build_stamp` 恒答 false ⇒ 自动部署整个静默关闭。"
        );
        assert!(
            !prod.contains(&format!("\"{mark}\"")),
            "本文件生产段里出现了界标字面量 `{mark}` —— 闭集唯一住址在\n\
                 `src/backend/main.rs`（`BUILD_STAMP_OPEN`/`CLOSE`），\n\
                 这里只许 `env!(\"DAEMON_STAMP_OPEN\")` / `env!(\"DAEMON_STAMP_CLOSE\")` 取。"
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
            prod.contains(&format!("DAEMON_EMBEDDED_ID_{}", arch.to_uppercase())),
            "`build.rs` 会为 `{arch}` 嵌入二进制并发 `DAEMON_EMBEDDED_ID_{}`，\n\
                 而 `sftp.rs` 生产段里没有对应的 `DaemonBinary` ⇒ `daemon_binary(\"{arch}\")` 返回 `None`，\n\
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
/// `build.rs` 的 `embed_daemons` 缺件时只 `cargo:warning=`（**不是 error**）：
///
/// > 缺少内嵌 daemon {arch} —— 远端自动部署将关闭
///
/// 而它旁边的注释逐字记着这条路的历史：「原来这里**连 warn 都没有** —— 缺二进制就
/// 静默不置 cfg、`daemon_binary()` 返回 None、远端自动部署整个消失而无人知晓。
/// **那正是 v2.19–v2.22 那批安装包的事故形状**」。
///
/// 警告是**刻意**的（本机开发树本来就常常只有一个 arch —— 今天就是：
/// `embedded-daemons/` 里只有 x86_64）。⇒ **保证「出货的那份两个 arch 都在」的，
/// 只剩 `release.yml` 一处**，而在本条之前没有任何判据读它那几行。
///
/// # 人群从 `build.rs` 派生
///
/// 不手写 `["x86_64", "aarch64"]`（隔壁 `embedded_daemon_binaries_present_and_valid`
/// 就是手写的，而且它带 `#[cfg(embedded_daemons)]` —— 本机缺一个 arch 时**整条不编译**，
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

    // 人群：`embed_daemons` 里那个 `for arch in [...]`。
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
                format!("staged/cc-monitor-remote-{arch}"),
                "编了但没按 `build.rs` 期待的名字放进 staged/",
            ),
            // 🔴 〔`K-R70` 09-12〕这里原来还有第三条：`staged/cc-monitor-remote-<arch>.build_id`，
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
    //   · `const BUILD_STAMP_OPEN` —— 界标是**从 daemon 源码抠的**，不是在 yml 里手抄一份
    //     （手抄那一份哪天与源码漂开，校验会以「假红」的形式提醒错人）。
    for (needle, why) in [
        (
            "ReadAllBytes",
            "校验那一步没有把二进制的字节读进来 —— 那它验的就不是这份字节，\
                 而是它旁边的某个文件（`K-R70` 整件治的就是这个）",
        ),
        (
            "const BUILD_STAMP_OPEN",
            "身份戳的界标不是从 daemon 源码抠的 —— 手抄一份就多一个会漂的住址；\
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
        "上传 `embedded-daemons` 的那一步没有 `if-no-files-found: error` —— \n\
             staged/ 空了它会**成功地上传一个空 artifact**，下游 job 下载到空目录，\n\
             最后出的安装包不带任何内嵌 daemon。这正是本条要挡的那个事故的上游一环。"
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

/// K-W4 `§0c` 那条断裂：`.build_id` 是**目录级**的，光凭它判不出落点那个文件在不在。
///
/// 这一格钉的是**两件事不许再压在一个读数上**：喂**同一份**版本事实
/// （标记在、且与期望相符），只让「落点那个文件」这一侧变，判定必须跟着变。
/// 它红的时候说明 `deploy_decision_at` 又把 `Missing` 当成了「版本对就跳过」——
/// 那正是「新名/被删的二进制永远不会上传，而用户看到的是连不上」那个静默。
#[test]
fn a_matching_marker_no_longer_speaks_for_a_binary_that_is_not_there() {
    const EXPECT: &str = "p1b-overflow";
    // 版本这一侧两个世界完全相同（标记在、逐字相符）——只有文件那一侧不同。
    assert_eq!(
        deploy_decision_at(Some(EXPECT), EXPECT, TargetBinary::Present),
        DeployAction::Skip,
        "文件在 + 版本对 ⇒ 仍然跳过（这一半是今天的行为，不许动）"
    );
    let missing = deploy_decision_at(Some(EXPECT), EXPECT, TargetBinary::Missing);
    let DeployAction::Deploy(reason) = &missing else {
        panic!(
            "标记相符但落点没有二进制，判定仍是 Skip —— \
                 一个 `.build_id` 又同时替「版本对不对」和「那个文件在不在」两件事说了话"
        );
    };
    // 「说得出是哪种坏」：这一句必须谈那个文件，而不是谈版本。
    assert!(
        reason.contains("落点没有 daemon 二进制"),
        "原因没说清是「那个文件不在」：{reason}"
    );
    assert!(
        !reason.contains("版本不符"),
        "版本明明是相符的，别把「文件不在」说成「版本不符」：{reason}"
    );
    // 两侧的事实各自有各自的话 —— 同一句里也要说清版本这一侧是什么状态。
    assert!(
        reason.contains(EXPECT),
        "同一句话里没带上版本那一侧的事实：{reason}"
    );
    // 三种版本状态下，「文件不在」这句话都要说得出来（不是只在版本相符时才说）。
    for (marker, what) in [
        (Some(EXPECT), "版本相符"),
        (Some("p1a-history"), "版本不符"),
        (None, "无标记"),
    ] {
        let DeployAction::Deploy(r) = deploy_decision_at(marker, EXPECT, TargetBinary::Missing)
        else {
            panic!("{what} + 文件不在 ⇒ 竟然跳过");
        };
        assert!(r.contains("落点没有 daemon 二进制"), "{what}: {r}");
    }
}

/// 反向那一刀：**没有顺手改成「每次都重传」**。
/// `sftp.rs` 那套 Batch8/9 stale 防御是买来的——版本门控必须仍然是承重的，
/// 且「stat 问不出来」不许被读成「文件不在」（那等于每次连接都重传 2.3MB）。
#[test]
fn splitting_the_two_facts_did_not_dismantle_the_version_gate() {
    const EXPECT: &str = "p1b-overflow";
    // ① 文件在 + 版本对 ⇒ Skip（门控还在，不是每次都传）
    assert_eq!(
        deploy_decision_at(Some(EXPECT), EXPECT, TargetBinary::Present),
        DeployAction::Skip
    );
    assert_eq!(
        deploy_decision_at(Some("p1b-overflow\n"), EXPECT, TargetBinary::Present),
        DeployAction::Skip,
        "trim 语义不许在合并判定里丢掉"
    );
    // ② 问不出来 ⇒ 与今天同答（Skip），不许当成「不在」
    assert_eq!(
        deploy_decision_at(Some(EXPECT), EXPECT, TargetBinary::Unknown),
        DeployAction::Skip,
        "stat 问不出来被读成「文件不在」⇒ 一次 stat 失败换一次全量重传，门控就废了"
    );
    // ③ 版本不符 ⇒ 照旧 Deploy，且说的是版本（presence 没把版本门控短路掉）
    let DeployAction::Deploy(reason) =
        deploy_decision_at(Some("p1a-history"), EXPECT, TargetBinary::Present)
    else {
        panic!("版本不符 + 文件在 ⇒ 竟然跳过，stale 防御被拆了");
    };
    assert!(
        reason.contains("版本不符"),
        "文件在而版本不符，这一句该谈版本：{reason}"
    );
    // ④ 无标记 ⇒ 照旧 Deploy
    assert!(matches!(
        deploy_decision_at(None, EXPECT, TargetBinary::Present),
        DeployAction::Deploy(_)
    ));
}

/// 0 字节那一格 **不是假想形态**：本模块 `upload_atomic` 里「绝不 set_metadata」
/// 那条注释记的就是真机 e2e 把 daemon 截成 0 字节、不可 exec 的那次事故。
/// 而 `try_exists` 会把它算成「在」⇒ 只问存在性的修法在这一形上仍然静默。
#[test]
fn a_zero_byte_daemon_is_not_a_deployed_daemon() {
    const EXPECT: &str = "p1b-overflow";
    let DeployAction::Deploy(reason) =
        deploy_decision_at(Some(EXPECT), EXPECT, TargetBinary::Empty)
    else {
        panic!("标记相符 + 落点是 0 字节 ⇒ 竟然跳过（那个文件不可 exec）");
    };
    assert!(
        reason.contains("0 字节"),
        "原因没说清是「那个文件是空的」：{reason}"
    );
    assert!(
        !reason.contains("版本不符"),
        "版本是相符的，别说成版本不符：{reason}"
    );
}

/// **防空转**：上面三格全在纯函数上，实现只要不接到调用点就是死代码，而三格照样绿。
/// 这一格钉的是**两条 daemon 部署路真的去问了那个文件**：
/// `ensure_daemon_deployed`（自动部署）与 `deploy_remote_daemon`（手动按钮）。
///
/// ⚠ 射程：只到 daemon 那两条路。`acct_iso_deploy` 那条**刻意不在分母里**——
/// 它的标记落在目录上、内容是同一次上传的一批脚本，是另一种形状（见
/// `deploy_decision` 的头注）；那条路今天有没有同族的病，本格判不了。
#[test]
fn both_daemon_deploy_paths_ask_the_file_itself_not_only_the_marker() {
    fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let i = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {sig}——守卫失效了"));
        let j = src[i..].find("\n}\n").map(|k| i + k).unwrap_or(src.len());
        &src[i..j]
    }
    let src = include_str!("../../src/bridge/src/sftp.rs");
    for sig in [
        "pub async fn ensure_daemon_deployed(",
        "pub async fn deploy_remote_daemon(",
    ] {
        let code = body(src, sig)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 反向自检：真取到函数体了（不然下面两条断言在空串上恒假、这一格变成假红/假绿源）
        assert!(
            code.contains("marker_path("),
            "{sig}: 取到的体里连版本标记都没读，守卫在空转"
        );
        assert!(
            code.contains("probe_target_binary("),
            "{sig}: 没有取样落点那个文件在不在 —— 判定只拿到了版本这一半事实"
        );
        assert!(
            code.contains("deploy_decision_at("),
            "{sig}: 仍在用只看版本的判定"
        );
        assert!(
            !code.contains("deploy_decision("),
            "{sig}: 还留着裸 `deploy_decision(` 调用 —— 两条判定并存迟早分叉"
        );
    }
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
/// 真机 e2e 把 daemon 截成 0 字节、不可 exec 的那次事故，而 `try_exists` 会把它算成「在」。
#[test]
fn probe_metadata_saying_zero_bytes_maps_to_empty() {
    assert_eq!(
        interpret_target_probe(Some(Some(0)), None),
        TargetBinary::Empty
    );
    assert_ne!(
        interpret_target_probe(Some(Some(0)), None),
        interpret_target_probe(Some(Some(1)), None),
        "0 字节与有字节判成了同一格 ⇒ deploy_decision_at 的 Empty 那一臂永远走不到"
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
/// 形状照抄同文件的 `both_daemon_deploy_paths_ask_the_file_itself_not_only_the_marker`
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
        code.contains(SIG) && code.contains("sftp"),
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

#[test]
fn is_safe_remote_jsonl_guard() {
    // 合法：projects/<单层目录>/<sid>.jsonl
    assert!(is_safe_remote_jsonl(
        "/home/pi/.claude/projects/proj/abc-123.jsonl"
    ));
    // 兼容 CLAUDE_CONFIG_DIR 自定义目录（不硬编码 .claude）
    assert!(is_safe_remote_jsonl(
        "/opt/claude-data/projects/-home-pi-x/sid.jsonl"
    ));
    // 非 .jsonl → 拒
    assert!(!is_safe_remote_jsonl(
        "/home/pi/.claude/projects/proj/note.txt"
    ));
    assert!(!is_safe_remote_jsonl(
        "/home/pi/.claude/projects/proj/abc.json"
    ));
    // 不在 projects/ → 拒
    assert!(!is_safe_remote_jsonl("/home/pi/.ssh/id_ed25519.jsonl"));
    assert!(!is_safe_remote_jsonl("/etc/passwd.jsonl"));
    // 含 .. 上跳 → 拒
    assert!(!is_safe_remote_jsonl(
        "/home/pi/.claude/projects/../../../etc/x.jsonl"
    ));
    // 审计 S-1：projects 下直接放 jsonl（无中间目录层）→ 拒
    assert!(!is_safe_remote_jsonl("/tmp/projects/x.jsonl"));
    // 层级过深（≠ <dir>/<sid>.jsonl）→ 拒
    assert!(!is_safe_remote_jsonl("/a/projects/b/c/x.jsonl"));
    // 文件名只是 ".jsonl" → 拒
    assert!(!is_safe_remote_jsonl("/x/projects/dir/.jsonl"));
    // 空中间目录段 → 拒
    assert!(!is_safe_remote_jsonl("/x/projects//abc.jsonl"));
}

#[test]
fn remote_parent_and_marker() {
    assert_eq!(
        remote_parent("/home/pi/.cc-monitor/bin/cc-monitor-remote"),
        "/home/pi/.cc-monitor/bin"
    );
    assert_eq!(remote_parent("/x"), "/");
    assert_eq!(remote_parent("rel/path"), "rel");
    assert_eq!(remote_parent("noslash"), ".");
    assert_eq!(
        marker_path("/home/pi/.cc-monitor/bin/cc-monitor-remote"),
        "/home/pi/.cc-monitor/bin/.build_id"
    );
    assert_eq!(marker_path("/x"), "/.build_id");
}

#[test]
fn strip_removes_paired_block_keeps_surrounding() {
    let s = format!("head\n{CCM_PROFILE_BEGIN}\nccm() {{ :; }}\n{CCM_PROFILE_END}\ntail\n");
    let out = strip_profile_block(&s, "远端 ~/.bashrc").unwrap();
    assert_eq!(out, "head\ntail\n");
    assert!(!out.contains(CCM_PROFILE_BEGIN));
    // 幂等：再 strip 不变
    assert_eq!(strip_profile_block(&out, "远端 ~/.bashrc").unwrap(), out);
}

#[test]
fn strip_noop_when_no_block() {
    let s = "just user content\nno block here\n";
    assert_eq!(strip_profile_block(s, "远端 ~/.bashrc").unwrap(), s);
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

/// **T04 审计②：迁移后远端这三个边界的语义确实变了，逐条锁死。**
/// 我原话"判定没变"已被实测证伪——写在这里免得下次又当成"没变"。
#[test]
fn remote_merge_boundary_semantics_after_migration() {
    let snip = "ccm() { :; }";
    // ① 行内 marker 不再命中 → 追加，且**用户那两行 echo 一个字节都不动**
    //    （旧实现会切断第一行、吃掉第二行——远端一个未申报就修掉的数据丢失）
    let inline =
        format!("a\necho \"{CCM_PROFILE_BEGIN}\"\necho \"{CCM_PROFILE_END}\"\nuser code\n");
    let got = merge_profile_block(&inline, snip, "远端 ~/.bashrc").unwrap();
    assert!(got.starts_with(&inline), "块外内容必须逐字保留：{got}");
    assert!(got.contains(snip));
    // ② BEGIN 与 END 同一行 → 现在 Err（**退化，如实记**：旧实现能替换该行）
    let same_line = format!("a\n{CCM_PROFILE_BEGIN} {CCM_PROFILE_END}\nb\n");
    let e = merge_profile_block(&same_line, snip, "远端 ~/.bashrc").unwrap_err();
    assert!(e.contains("找不到配对的 END"), "{e}");
    // ③ 缩进 marker → 归一到列 0（旧实现保留 BEGIN 缩进、丢 END 缩进，不自洽）
    let indented = format!("a\n  {CCM_PROFILE_BEGIN}\nold\n\t{CCM_PROFILE_END}\nb\n");
    let got = merge_profile_block(&indented, snip, "远端 ~/.bashrc").unwrap();
    assert!(
        got.contains(&format!("\n{CCM_PROFILE_BEGIN}\n")),
        "缩进应归一到列 0：{got}"
    );
    assert!(
        got.starts_with("a\n") && got.ends_with("b\n"),
        "块外保留：{got}"
    );
}

// ===== T04 审计① 上传读回判据（此前这条路完全没有读回）=====

#[test]
fn upload_verify_catches_same_length_corruption() {
    let want = b"#!/bin/sh\nexec ccm \"$@\"\n";
    // 等长但一字节不同——**只比长度是查不出来的**，而此前连长度都没比
    let mut bad = want.to_vec();
    let k = bad.len() / 2;
    bad[k] ^= 0x01;
    let e = verify_uploaded_bytes("/r/x", want, Some(&bad)).unwrap_err();
    assert!(e.contains("长度相同"), "{e}");
    assert!(e.contains("首个差异在第"), "要指出位置：{e}");
    // **关键**：措辞必须说清标记没写，否则用户不知道下次会重试
    assert!(e.contains("未写入版本标记"), "{e}");
}

#[test]
fn upload_verify_catches_truncation_and_unreadable() {
    let want = b"0123456789";
    let e = verify_uploaded_bytes("/r/x", want, Some(b"01234")).unwrap_err();
    assert!(e.contains("长度不匹配"), "{e}");
    assert!(e.contains("期望 10 字节"), "{e}");
    // 读不回来 ≠ 写对了
    let e2 = verify_uploaded_bytes("/r/x", want, None).unwrap_err();
    assert!(e2.contains("读不回"), "{e2}");
    assert!(e2.contains("未写入版本标记"), "{e2}");
}

#[test]
fn upload_verify_passes_on_exact_bytes() {
    // 二进制（含 NUL 与非 UTF-8）也要过——daemon 是可执行文件，String 路线走不通
    let bin = &[0x7f, b'E', b'L', b'F', 0x00, 0xff, 0xfe];
    assert!(verify_uploaded_bytes("/r/d", bin, Some(bin)).is_ok());
    assert!(verify_uploaded_bytes("/r/d", b"", Some(b"")).is_ok());
}

/// Phase G 阻塞①：**「读不出来」绝不能变成「文件是空的」**。
///
/// 旧代码是 `read_optional(..).map(from_utf8_lossy).unwrap_or_default()`，
/// 读失败 → `existing = ""` → install 跳过备份 + 整份覆盖用户 `.bashrc`；
/// uninstall 回「没有 ccm 块，无需卸载」。
#[test]
fn read_failure_is_not_an_empty_file() {
    // 读失败 + 明确不存在 → 当新建（这条是**反向自检**：不能一律 Err，否则首次安装就废了）
    assert_eq!(
        interpret_profile_read("远端 ~/.bashrc", None, Some(false), None),
        Ok(None)
    );
    // 读失败 + 文件确实在 → 必须 Err
    let e = interpret_profile_read("远端 ~/.bashrc", None, Some(true), None).unwrap_err();
    assert!(e.contains("读不出"), "{e}");
    assert!(e.contains("未改动任何文件"), "{e}");
    // 读失败 + 连"在不在"都问不出来 → 也必须 Err（不许乐观当新建）
    let e2 = interpret_profile_read("远端 ~/.bashrc", None, None, None).unwrap_err();
    assert!(e2.contains("读不出"), "{e2}");
}

/// Phase G 阻塞②：**非 UTF-8 的 profile 必须拒绝，不许有损重写**。
///
/// 有损路线的恶性在于它**自带合格证**：备份写的是已经变成 U+FFFD 的那份，
/// 读回校验两边同样有损 → 逐字节相同 → 校验通过。所以这里断言的是"根本不进那条路"。
#[test]
fn non_utf8_profile_is_refused_instead_of_lossily_rewritten() {
    // GBK 的「中」= 0xD6 0xD0，单独出现不是合法 UTF-8
    let gbk = b"# \xd6\xd0\xce\xc4\nexport PATH=$PATH\n";
    let e = interpret_profile_read("远端 ~/.bashrc", Some(gbk), None, None).unwrap_err();
    assert!(e.contains("不是合法 UTF-8"), "{e}");
    assert!(e.contains("前 2 字节合法"), "偏移要说清，实得：{e}");
    assert!(e.contains("未改动任何文件"), "{e}");
    // 有损重写会把它变成什么——写在这里，好让人一眼看到丢了什么
    assert_ne!(
        String::from_utf8_lossy(gbk).into_owned().as_bytes(),
        gbk,
        "这条测试的前提没了：这串本来就该是有损的"
    );

    // **反向自检**：合法的多字节 UTF-8（中文注释）必须原样通过、往返零损失
    let utf8 = "# 中文注释\nexport PATH=$PATH\n";
    assert_eq!(
        interpret_profile_read("远端 ~/.bashrc", Some(utf8.as_bytes()), None, None),
        Ok(Some(utf8.to_string()))
    );
}

/// Phase G：本机侧 v1.7.9 的那道防线（磁盘有字节却读到空）补到远端侧。
#[test]
fn bytes_on_disk_but_read_empty_is_refused() {
    let e = interpret_profile_read("远端 ~/.bashrc", Some(b""), None, Some(120)).unwrap_err();
    assert!(e.contains("有 120 字节"), "{e}");
    assert!(e.contains("未改动任何文件"), "{e}");
    // 反向自检：真的空文件（size 0 / 问不到 size）不能被拦
    assert_eq!(
        interpret_profile_read("远端 ~/.bashrc", Some(b""), None, Some(0)),
        Ok(Some(String::new()))
    );
    assert_eq!(
        interpret_profile_read("远端 ~/.bashrc", Some(b""), None, None),
        Ok(Some(String::new()))
    );
}

/// **结构性守卫**：两处 profile 读-改-写的**初始读取**必须走 fail-safe 读取器。
///
/// 范围**只覆盖「喂给文本变换的那一次读取」**，即函数体开头到
/// `merge_profile_block`/`strip_profile_block` 之间那一段。
///
/// 第一版写成"整个函数体不许出现 `read_optional(sftp, &profile)`"，**当场被自己抓红**：
/// 同一函数里的**写后读回校验**正当地用它，而且那处用 lossy 也是安全的——写进去的一定是
/// 合法 UTF-8，传输损坏会变成 U+FFFD → 与期望不符 → Mismatch → 回滚，方向 fail-safe。
/// 收窄了**两次**才对上，两次都是自己抓自己：
/// ① 初版扫整个函数体 → 撞上写后读回校验那处正当的 `read_optional(sftp, &profile)`；
/// ② 收到"变换之前"后仍假红 → `install` 的读取段里还有一处正当的 `read_optional`，
///    读的是刚部署的 **ccm CLI 脚本**（`CCM_CLI_REMOTE_PATH`），不是 profile。
/// 所以禁的必须是**读 profile 那一次**的确切形态，不是"任何 `read_optional`"。
/// 守卫范围必须等于性质范围；本会话第三次栽在同一形状上，故把订正过程留在注释里。
#[test]
fn profile_read_modify_write_goes_through_the_failsafe_reader() {
    fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let i = src
            .find(sig)
            .unwrap_or_else(|| panic!("找不到 {sig}——守卫失效了"));
        let j = src[i..].find("\n}\n").map(|k| i + k).unwrap_or(src.len());
        &src[i..j]
    }
    let src = include_str!("../../src/bridge/src/sftp.rs");
    let mut checked = 0usize;
    for (sig, transform) in [
        (
            "pub async fn uninstall_remote_ccm_helper(",
            "strip_profile_block(",
        ),
        (
            "pub async fn install_remote_ccm_helper(",
            "merge_profile_block(",
        ),
    ] {
        let code = body(src, sig)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 反向自检①：真取到函数体了（不是空串在空转）
        assert!(code.contains("upload_atomic("), "{sig}: 取到的体里没有上传");
        let cut = code
            .find(transform)
            .unwrap_or_else(|| panic!("{sig}: 找不到 {transform}——守卫失效了"));
        let before = &code[..cut];
        // 反向自检②：截出来的前半段非空，且确实是读取段
        assert!(
            before.len() > 40 && before.contains("profile"),
            "{sig}: 截出的读取段不像读取段（{} 字节）",
            before.len()
        );
        assert!(
            before.contains("read_profile_text(sftp, &profile"),
            "{sig}: 喂给 {transform} 的 profile 读取没走 fail-safe 读取器"
        );
        assert!(
            !before.contains("read_optional(sftp, &profile)"),
            "{sig}: 又直接拿 read_optional 读 profile 了——那会把「读不出来」当成空文件，\
                 于是跳过备份 + 整份覆盖 / 谎报无需卸载"
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "期望恰好两个 profile 命令，实得 {checked}");
}

/// Phase G：回滚措辞必须与实际发生的事一致（机制不许声称做了它没做的事）。
#[test]
fn rollback_note_matches_what_actually_happened() {
    assert!(rollback_note(false).contains("已尝试回滚"));
    let n = rollback_note(true);
    assert!(n.contains("没有可回滚的内容"), "{n}");
    assert!(!n.contains("已尝试回滚"), "空 existing 时不许说回滚过：{n}");
    assert!(n.contains("请手动清理"), "要给出恢复路径：{n}");
}

/// **结构性守卫**：两条 deploy 路径的**内容**上传必须走 verified。
///
/// 范围只覆盖 `deploy_remote_daemon` 与 `deploy_remote_acct_iso` 两个函数体
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
                "pub async fn deploy_remote_daemon(",
            ),
            "deploy_remote_daemon",
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
        // 反向自检：真取到函数体了
        assert!(
            code.contains("upload_atomic"),
            "{what}: 取到的体里没有上传，守卫在空转"
        );
        verified_total += code.matches("upload_atomic_verified(").count();
        for l in code.lines() {
            if !l.contains("upload_atomic(") {
                continue;
            }
            assert!(
                l.contains("marker"),
                "{what}: 内容上传仍走裸 upload_atomic —— {}",
                l.trim()
            );
        }
    }
    // 计数自检：2 处 daemon 二进制 + 6 个 acct-iso 脚本
    assert_eq!(
        verified_total, 7,
        "期望 1(daemon 体内) + 6(acct-iso)，实得 {verified_total}"
    );
}

#[test]
fn strip_aborts_on_malformed_begin_without_end() {
    // **这条测试原先把 bug 编码进去了**（T04 审计阻塞）：它断言悬空 BEGIN 时
    // strip 是 no-op，而调用方据此打印「远端 … 没有 ccm 块，无需卸载」——
    // 那正是同一个 commit 里被定义为 bug 的形态，只是发生在「卸」这半边。
    // 现在两侧的装与卸四条路全走 `find_pair`，此处必须 Err 中止。
    let corrupt = format!("a\n{CCM_PROFILE_BEGIN}\nccm() {{ :; }}\nuser code\n");
    let e = strip_profile_block(&corrupt, "远端 ~/.bashrc").unwrap_err();
    assert!(e.contains("找不到配对的 END"), "{e}");
    assert!(e.contains("已中止"), "要让用户知道我们没动文件：{e}");
    assert!(e.contains("远端 ~/.bashrc"), "要说清是哪个文件：{e}");
    // 而**没有** BEGIN 时仍是正常的 no-op（别把这条也变成错误）
    assert_eq!(
        strip_profile_block("just user code\n", "远端 ~/.bashrc").unwrap(),
        "just user code\n"
    );
}

#[test]
fn safe_daemon_path_accepts_convention_rejects_suspicious() {
    assert!(is_safe_remote_daemon_path(
        "/home/pi/.cc-monitor/bin/cc-monitor-remote"
    ));
    assert!(!is_safe_remote_daemon_path("")); // 空
    assert!(!is_safe_remote_daemon_path("relative/cc-monitor")); // 非绝对
    assert!(!is_safe_remote_daemon_path("/")); // 根
    assert!(!is_safe_remote_daemon_path("/etc/passwd")); // 不含 cc-monitor
    assert!(!is_safe_remote_daemon_path(
        "/home/pi/.cc-monitor/../../../etc/x"
    )); // 含 ..
}
