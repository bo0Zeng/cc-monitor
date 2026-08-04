//! F10（出口④ 装棘轮）：**本机读面清账 + 递减棘轮**。
//!
//! # 为什么是棘轮而不是「退役」
//!
//! F10 的正题是「本机读面退役」——把 monitor 自己直读 `~/.claude` 换成走本机后端。
//! 定框 C7 解锁了它（§3 决策表那行逐字写着「**解锁**了本机读面退役」）。
//!
//! **但今天退不了**：F05a 只给了「起与看住」的机制，`resolve_beside_this_exe` 恒走
//! 诚实降级（安装包里还没有 sidecar，那是 **F05b**）⇒ **本机没有在跑的后端进程**，
//! 没有对侧可切。这正是 skill 那个「目标今天钉不上（终点被别的件挡着）」的出口 ——
//! **装递减棘轮，钉住当下，不跳过也不硬钉。**
//!
//! 它防的是一件很具体的事：**F10 还没做，而直读点越来越多。**
//! 那种增长每一处看都合理（「就读一下 claude_dir」），合起来就是 F10 的工作量翻倍。
//!
//! # 分类：`hub` 与 `reader` 是两回事
//!
//! | 类别 | 含义 | 退役时怎么处理 |
//! |---|---|---|
//! | `hub` | **路径真相源** —— 只回答「那些目录在哪」，自己不读内容 | 保留（切后端后它仍是路径来源） |
//! | `reader` | 真的去读文件内容 | **这些才是要退役的** |
//! | `payload` | 只把路径拼进要给别人执行的命令串 | 随 F06/F07 走，不属读面 |
//! | `remote` | 说的是**远端主机**的 claude 目录（daemon hello 的字段、远端 shell 串） | **根本不是本机读面**，不属 F10 |
//! | `non-read` | 只出现那个名字（契约清单 / 登记表文案），不读文件 | 不属读面 |
//!
//! ⚠ 只数「有几处提到 `claude_dir`」会把三类混成一个数，而**只有 `reader` 那一类是 F10 的活**。
//!
//! # ★ 数字是机器数的，不是我手数的
//!
//! F05 摸底时我按一个更松的模式手数出「20 个文件」；本表首跑（剥测试段 + 剔注释行）
//! 数出 **17 个文件 / 66 行**（首跑还先报了个 13/48 —— 那是本判据**自己的缺陷**：
//! 只跳过整行注释、把**行尾注释**里的提及也算进去了，已修）。
//! ⇒ **以机器那个数为准**，而且**机器数错了也要先修机器再登记** ——
//! 与 F09 那次（我数 8 处、判据数 13 处）是同一条纪律的第三次。
//!
//! ⚠ 真正是 F10 工作面的只有 `reader` 那 11 条 —— 其余是 hub / payload / remote / non-read。
//!
//! # 它查什么、查不了什么
//!
//! 查 monitor Rust **生产段**里提到 `claude_dir` / `CLAUDE_CONFIG_DIR` / `.claude/projects` /
//! `records_dir` 的行数，按文件逐个对账。
//!
//! ⚠ **查不了「换个名字读同一批文件」**（比如把路径先存进一个不叫 `claude_dir` 的变量）——
//! 与本仓其它约定型守卫同一档。**比没有强，别读成证明。**
//! ⚠ **也不区分「读了几次」**：一行里读三个文件仍算一行。它钉的是**面**不是次数。

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 本机读面登记表：`(相对 src-tauri 的路径, 类别, 命中行数, 为什么 + 退役归谁)`。
    ///
    /// 多一处 ⇒ 下面那条红（防「F10 还没做而直读点增长」）；
    /// 少一处 ⇒ **也红**（退役了要把棘轮往下拧）。
    const REGISTERED: &[(&str, &str, usize, &str)] = &[
        (
            "src/history.rs",
            "reader",
            15,
            "最大的一块：会话历史列表/读取/元数据。**F10 的主要工作量在这里**。\
             退役归 F10 本体（要本机后端在跑，即 F05b）。",
        ),
        (
            "src/ssh_source.rs",
            "remote",
            8,
            "★ **说的全是远端主机的 claude 目录**：daemon `hello` 帧的 `claude_dir` 字段 · \
             daemonless 那条远端 shell 串里的 `\\${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects`。\
             **根本不是本机读面** ⇒ 不属 F10。\
             ⚠ 我摸底时差点把它算成本机的 8 行 —— 同名最便宜的误导。",
        ),
        (
            "src/lib.rs",
            "hub",
            7,
            "启动时解析 `claude_dir` 并派生 projects/sessions/tasks 四个目录往下传 —— \
             **一处入口，不读内容**。切后端之后仍要在（得告诉后端读哪儿）⇒ **不属**退役范围。",
        ),
        (
            "src/paths.rs",
            "hub",
            7,
            "**路径真相源** —— 只回答「`~/.claude` 与它的子目录在哪」，自己不读内容。\
             切后端之后它**仍然要在** ⇒ **不属**退役范围。",
        ),
        (
            "src/backend/control/payload.rs",
            "payload",
            5,
            "只把 `CLAUDE_CONFIG_DIR` 拼进要给别人执行的载荷（`env` 前缀那一段）。\
             **不读任何文件** ⇒ **不属**读面。它随 F06/F07 走。",
        ),
        (
            "src/search.rs",
            "reader",
            4,
            "全文索引构建时遍历 records 目录。退役归 F10 本体。",
        ),
        (
            "src/adapter.rs",
            "reader",
            3,
            "适配器层的 `records_dir`/`tasks_dir` 解析（哪个 agent 的记录目录）。退役归 F10 本体。",
        ),
        (
            "src/local_accounts.rs",
            "reader",
            3,
            "本机账号库：`resolve_claude_dir` + 读 `sessions/`。退役归 F10 本体。",
        ),
        (
            "src/tasks.rs",
            "reader",
            3,
            "读 `tasks/<sid>/*.json`（issue #11 的任务面）。退役归 F10 本体。",
        ),
        (
            "src/accounts.rs",
            "remote",
            2,
            "⚠ **〔F10b-2 订正分类〕它根本不属退役范围** —— 头注逐字写着它是\
             「这件事的**远端**那半（把 daemon 的 `--list-accounts` 包成 Tauri 命令）」，\
             生产段**零本机文件读**。那 2 个命中是**用户可见的提示字符串**里提到了 \
             `CLAUDE_CONFIG_DIR`（第 82/88 行「远端 daemon 版本较旧…」那两句）。\
             ⚠ 这不是退役、**不算工作量减少** —— 是把一条误分类改对。\
             ★ 它暴露了量法的口径：`hits()` 数的是「提到那几个词的行」，\
             里面会有提示文案与 `/proc` 环境键名，**不等于「未退役的直读点」**。",
        ),
        (
            "src/mcp.rs",
            "reader",
            2,
            "读 `.claude.json` 里的 MCP 服务器声明。退役归 F10 本体。",
        ),
        (
            "src/adapter/claude_code.rs",
            "reader",
            1,
            "Claude Code 适配器自己那一处路径。退役归 F10 本体。",
        ),
        (
            "src/config_surface.rs",
            "reader",
            1,
            "T02 配置面审计视图（只读、不轮询）。退役归 F10 本体。",
        ),
        (
            "src/hooks_diag.rs",
            "reader",
            1,
            "hooks 诊断读 settings。退役归 F10 本体。",
        ),
        (
            "src/ccm_cli_contract.rs",
            "non-read",
            1,
            "只在契约清单里出现 `CLAUDE_CONFIG_DIR` 这个**变量名**，不读文件 ⇒ **不属**读面。",
        ),
        (
            "src/tool_registry.rs",
            "non-read",
            1,
            "T01 受管工具登记表的一句**文案**里提到它，不读文件 ⇒ **不属**读面。",
        ),
    ];

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    /// 一处「碰本机 claude 目录」的源码形态。运行时拼，免得命中本文件自己的说明。
    fn needles() -> Vec<String> {
        let c = "claude";
        vec![
            format!("{c}_dir"),
            format!("CLAUDE_CONFIG_DIR"),
            format!(".{c}/projects"),
            "records_dir".to_string(),
        ]
    }

    fn hits(prod: &str) -> usize {
        let ns = needles();
        prod.lines()
            .filter_map(|l| {
                // ⚠ **首跑的缺陷**：只跳过「整行注释」，于是 `mod accounts; // …CLAUDE_CONFIG_DIR`
                // 这种**行尾注释**里的提及也被算成读点（`lib.rs` 因此被数成 8 行而真值是 4）。
                // ⇒ 先把行尾 `//` 之后砍掉，再看剩下的代码部分。
                let code = match l.find("//") {
                    Some(i) => &l[..i],
                    None => l,
                };
                (!code.trim().is_empty()).then_some(code)
            })
            .filter(|code| ns.iter().any(|n| code.contains(n.as_str())))
            .count()
    }

    /// 递归遍历 `src/`，**不是硬编码文件名单**。
    fn rust_files() -> Vec<(String, String)> {
        let src = root().join("src");
        let mut out = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p: PathBuf = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_some_and(|x| x == "rs") {
                    let rel = format!(
                        "src/{}",
                        p.strip_prefix(&src)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/")
                    );
                    out.push((rel, fs::read_to_string(&p).unwrap_or_default()));
                }
            }
        }
        out.sort();
        out
    }

    /// ★ 抽取器自检。
    #[test]
    fn the_scan_actually_reads_the_monitor_tree() {
        let files = rust_files();
        assert!(
            files.len() >= 60,
            "只扫到 {} 个 .rs —— 遍历器坏了",
            files.len()
        );
        let me = files
            .iter()
            .find(|(n, _)| n == "src/local_read_surface_registry.rs")
            .map(|(_, s)| s.as_str())
            .expect("扫不到本文件");
        assert!(
            guard_core::production_code(me).len() < me.len() / 2,
            "本文件剥完还剩一半以上 —— 剥法没生效，说明文字会被当成命中"
        );
    }

    /// ★ 递减棘轮：目录内容 == 登记表，**连每个文件的行数一起钉**。
    #[test]
    fn the_local_read_surface_matches_the_registry_line_for_line() {
        let mut want: Vec<(String, usize)> = REGISTERED
            .iter()
            .map(|(f, _, n, _)| ((*f).to_string(), *n))
            .collect();
        want.sort();
        let mut got: Vec<(String, usize)> = rust_files()
            .into_iter()
            .filter_map(|(rel, raw)| {
                let n = hits(&guard_core::production_code(&raw));
                (n > 0).then_some((rel, n))
            })
            .collect();
        got.sort();
        assert_eq!(
            got, want,
            "\n本机读面与登记表对不上。\n\
             **多一处/多一行** = F10 还没做而直读点在增长 —— 那会让 F10 的工作量翻倍。\n\
             先回答它属哪一类（`hub` 路径真相源 / `reader` 真读内容 / `payload` 只拼串），\n\
             `reader` 还要写退役归属。\n\
             **少一处** = 退役了 —— 把登记表那条删掉并把棘轮往下拧。\n\
             ⚠ 数字**以本条为准**，不以手数为准（F05 摸底手数出 20 个文件，机器数是 13 个）。"
        );
    }

    /// ★ `reader` 这一类**必须**写退役归属；`hub`/`payload` 必须说清为什么不属退役范围。
    #[test]
    fn every_reader_names_its_retirement_owner() {
        let mut readers = 0;
        for (f, kind, _, why) in REGISTERED {
            assert!(
                matches!(*kind, "hub" | "reader" | "payload" | "remote" | "non-read"),
                "{f} 的类别 `{kind}` 不在三类里 —— 新类别要先在模块头注那张表里定义"
            );
            match *kind {
                "reader" => {
                    readers += 1;
                    assert!(why.contains("退役归"), "{f} 记成 reader 却没说谁退役它");
                }
                _ => assert!(
                    why.contains("不属"),
                    "{f} 记成 `{kind}` 却没说清为什么不属退役范围 —— \
                     那样它会被下一个人当成 F10 的工作量"
                ),
            }
        }
        // 抽取器自检：一条 reader 都没认出来时上面全空转。
        assert_eq!(
            readers, 9,
            "`reader` 条数变了（实测 10 条）。这个数就是 **F10 的真实工作面** —— \
             多一条要说明为什么又加了直读点，少一条说明退役了一处（把棘轮往下拧）。\n\
             ⚠ 棘轮史：11 → **10**（F10b 第一批，`usage.rs` 退役 —— 它改走本机后端的 `--usage`）\n\
             → **9**（F10b 第二批：`accounts.rs` **改分类**为 `remote` —— 它本来就不是本机读面，\n\
             ⚠ **那一格不算退役、不算工作量减少**，只是把误分类改对，理由写在它自己那条登记里）。\n\
             ⚠ **别把这个数往上调**：往上调等于承认又加了直读点，那要先说清为什么。"
        );
    }

    /// ★ **前提触发器 —— 已经触发过一次，这是它的后继形态**（F05b，2026-08-04）。
    ///
    /// # 它原来长什么样、为什么要换
    ///
    /// 原形：断言 `tauri.conf.json` 里**没有** `externalBin`，一出现就红并喊
    /// 「F05b 落地了 ⇒ F10 的正题现在能做了」。
    ///
    /// F05b 落地时它**确实红了**，而且红得对。但落地形态与它预设的不同：
    /// `externalBin` **没有**进主配置 —— 因为 `tauri-build` 要求**当前 target** 的 sidecar
    /// 在编译期就存在，进主配置会让 `cargo test` 也需要一份 daemon 二进制，
    /// 那正是 C2 反面（两半不许在构建期互相咬住）刚钉住的东西。
    /// ⇒ 它住进**发版补丁配置** `tauri.sidecar.conf.json`，只在 `tauri build --config` 时注入。
    ///
    /// ⇒ 本条换成后继形态：**盯新的家**，并且钉住「棘轮一格没放」。
    /// ⚠ **这不是降强度**：断言从「一条」变成「三条」（sidecar 契约有家 · stem 与
    /// `SIDECAR_STEM` 一致 · 棘轮上限没被放宽），而且扫描面从主配置**换到了它真正的家** ——
    /// 留在旧扫描面上才是降强度（它永远不会再红）。
    #[test]
    fn the_sidecar_contract_has_exactly_one_home_and_f10s_ratchet_is_untouched() {
        // ① sidecar 契约必须有家，而且**不在主配置里**（进主配置 = 每个编译点都要一份二进制）。
        let key = format!("external{}", "Bin"); // 运行时拼，免得命中本文件自己的说明
        let main_conf =
            fs::read_to_string(root().join("tauri.conf.json")).expect("读不到 tauri.conf.json");
        assert!(
            main_conf.len() > 500,
            "tauri.conf.json 只有 {} 字节，抽错了？",
            main_conf.len()
        );
        assert!(
            !main_conf.contains(key.as_str()),
            "`{key}` 回到了主配置 —— 那会让 `cargo test` 也需要一份当前 target 的 daemon 二进制\n\
             （实测报错：`resource path binaries/cc-monitor-remote-<triple> doesn't exist`），\n\
             等于把两半在**构建期**绑死。它的家是 `tauri.sidecar.conf.json`，只在发版时 `--config` 注入。"
        );
        let patch = fs::read_to_string(root().join("tauri.sidecar.conf.json"))
            .expect("读不到 tauri.sidecar.conf.json —— sidecar 契约没有家了");
        assert!(
            patch.contains(key.as_str()),
            "发版补丁配置里没有 `{key}` —— 那安装包里就不会带上本机后端（C7）"
        );

        // ② stem 与 Rust 侧的 `SIDECAR_STEM` 必须是同一个（同一个名字不许两侧各写一份，定框 §4）。
        let stem = crate::backend::control::local_backend::SIDECAR_STEM;
        assert!(
            patch.contains(&format!("binaries/{stem}")),
            "补丁配置里的 sidecar 路径与 Rust 侧的 `SIDECAR_STEM`（{stem:?}）对不上 —— \n\
             消费侧 `resolve_with` 找的是 `{stem}-<triple>` 与裸 `{stem}`，\n\
             两边写不一样 ⇒ 安装包里带了一个谁也找不到的文件。"
        );

        // ⚠ **刻意不在这里再钉一遍 `reader` 的条数。**
        // 那个数（今天 11）已经由同模块的
        // `every_registered_file_declares_what_kind_of_read_it_is` 钉着；
        // 在这里抄第二份就是「判据存了真相源的副本」（定框 §4 逐字禁止）——
        // F11 的 E4 变异就是被那种副本骗过去的。
        //
        // ⇒ F10 的交接写在本条头注与 `ROADMAP` 里，不写成第二个数字：
        // **F05b 已落地、本机后端真的起起来了**（真机实测日志逐字为
        // `本机后端: Started { pid: 6072, attempt: 1 }`），所以 F10 的正题现在能做 ——
        // 把那些 `reader` 直读点切到后端，然后把那条棘轮往下拧。
        // 注意 F01b 留的死限：本地 sid 一进 `tmux_raw_registry`，`/branch` 的灰点 bug 会回来。
    }
}
