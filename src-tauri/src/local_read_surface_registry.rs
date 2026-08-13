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
//! | `fence` | **路径围栏** —— 解析 records 根只为验「这个路径在不在里面」，不读内容 | **刻意保留**（纵深防御）：即使读交给后端，围栏也该在两侧各有一道 |
//! | `write` | 写操作（删/分叉）**恰好也读 dir 来定位文件** | 不属读面；各归其主（删无对侧、分叉走 `--fork-session`） |
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
//! ⚠ 真正是 F10 工作面的只有 `reader` 那一类 —— 其余是 hub / payload / remote / non-read。
//! **有几条以本文件那条 `assert_eq!` 为准，这里刻意不再抄一份**：上一版这里写着 11，
//! 而同一个文件里的机检断言写着 7 —— 一个文件内部自相矛盾，正是「散文数字必有家」
//! （定框 E12）最短的证明。
//!
//! # 它查什么、查不了什么
//!
//! 查 monitor Rust **生产段**里提到 `claude_dir` / `CLAUDE_CONFIG_DIR` / `.claude/projects` /
//! `records_dir` / **`.claude`** 的行数，按文件逐个对账。
//!
//! ⚠ 最后那个针是 08-06 补的，补之前这张表**数字比事实小 18 行**：
//! `home.join(".claude")` · `~/.claude/settings.json` · `.claude.json` 这些写法一个都不在人群里。
//! ★ 值得记的是**漏的形状**：没有漏掉任何一个文件 —— 五个文件本来就在表上，
//! 漏的是**它们内部没被数到的行**。于是这张表**看起来是全的**（文件集正确），
//! 数字却偏小，而 F10 的工作量正是按这个数估的。
//! ⇒ 「登记表覆盖了哪些文件」与「登记表的数对不对」是两件事，前者绿不代表后者绿。
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
        // 〔F10b 末批〕`history.rs` **按角色拆成四条** —— 逐函数量过，那 15 个命中不是一类活。
        // ★ 拆条的理由：登记表原本按「文件 × 单一类别」记账，而这个文件承载四种角色 ⇒
        // 「读面迁完」时那条登记不会消失、`readers` 也不会降，账就成了假的。
        // ⚠ 四条的**处数之和仍是 15**，与实测那一侧的口径一致（比较逻辑已改成「按文件求和」）。
        (
            "src/history.rs",
            "no-counterpart",
            2,
            "`list_history_projects`(168/169) 遍历 records 根列项目。\
             ⚠ **今天的查询集下迁不了、不属能退役的范围** —— 逐字段量过：\
             daemon `--list-projects` 每行只给 `dirName` / `projectPath` / `sessionCount` / \
             `lastActivityMs` **四个字段**，而本函数还要 `starred_count` / `hidden_count`（本机\
             metadata，**按会话 sid 查**）与 `has_live`（`SessionMap` 活状态）—— \
             `analyze_project_dir` 正是靠 `read_dir` + 从文件名推 sid 才算得出它们。\
             ⇒ 要么 daemon 补「每项目的会话 sid 清单」，要么每个项目再来一次 `--list-sessions`\
             （N 次进程 spawn，而这是用户常开的界面）。\
             ★ **退役条件：daemon 的 `--list-projects` 每行带上会话 sid 清单**（或等价字段）。",
        ),
        (
            "src/history.rs",
            "fence",
            2,
            "`stream_history_sessions_in_project`(430/431) 的**路径围栏** —— 它解析 records 根\
             **只为验前端传来的 `project_dir` 在不在里面**（`refuse: … outside …`），与 499 同形。\
             ⚠ **刻意保留、不属退役范围**（纵深防御，理由同那条 `fence`）。",
        ),
        (
            "src/history.rs",
            "no-counterpart",
            1,
            "`list_history_projects` 的 **codex 变体**(232)。⚠ **不属**「今天能退役」的范围：\
             实测 daemon 的 `history_query.rs` 里 **codex / kinds / agent_kind 零命中** ——\
             `--list-projects` 只服务 claude。退役条件 = daemon 侧补上 codex 的项目枚举（DG3 那一族）。",
        ),
        (
            "src/history.rs",
            "fence",
            1,
            "`stream_read_session_jsonl`(499) 的**路径围栏** —— 它解析 records 根**只为验\
             `target.starts_with(&root)`（拒绝越界路径），不读内容。\
             ⚠ **刻意保留、不属退役范围**：即使把读交给后端，围栏也该两侧各有一道\
             （daemon 侧自己也有 canonicalize 前缀校验）—— 那是纵深防御，同 `remote_branch.rs` \
             那句「两个 id 已过白名单，仍照常 shell_quote」。",
        ),
        (
            "src/history.rs",
            "write",
            4,
            "写操作**恰好也读 dir 来定位文件**：`delete_history_session`(621/622) · \
             `create_branch_session`(744/745)。⚠ **不属读面** —— 删会话 **daemon 侧无对侧**\
             （14 条一次性子命令里没有删）；分叉走 `--fork-session`。",
        ),
        (
            "src/history.rs",
            "payload",
            5,
            "把 `CLAUDE_CONFIG_DIR` 拼进**启动命令串**：`validate_config_dir_posix`(1031) · \
             `validate_config_dir_ps`(1054/1057) · `config_dir_prefix_ps`(1102/1111)。\
             ⚠ **不属读面**，随 F06/F07 走。",
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
            9,
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
            "src/cc_bus_deploy.rs",
            "write",
            13,
            "`PS1` 的部署：`fenced_dest` 解析 `<claude_dir>/skills` 并做 realpath 围栏、\
             `deploy_into`/`deploy_local_cc_bus` 取 dir 再往下写。\
             ⚠ **不属读面** —— 它是**写**操作（本仓第一处往 `<claude_dir>` 写的，\
             `U10b` 裁定后的第 7 条例外），恰好也要解析 dir 来定位落点，与 `history.rs` 那条 \
             `write` 同类。⇒ 不归 F10 的退役范围。\
             ⚠ 〔`PS2` 08-13〕9 → **13**：本文件又加了 `install_state_in` / `cc_bus_install_state`
             （查「本机装的是哪一版」，**纯读**）。它们**是**读面，但读的是**本模块自己刚写下去的
             那份**（`<claude_dir>/skills/cc-bus/`），与 F10 要退役的「读 claude 的会话数据」
             不是一回事 —— 后端化之后这一格该跟着部署那条一起走，不单独退役。",
        ),
        // 〔P8a 08-12〕新增的直读点 —— **老实登记，不绕棘轮**（棘轮要的是论证，不是禁令）。
        (
            "src/plugins.rs",
            "reader",
            5,
            "`survey_marketplaces_in`(165/166) 读 `<claude_dir>/plugins/known_marketplaces.json`，\
             `list_plugin_marketplaces` 那条命令(203/204) 取 dir 再转交它；签名占 1 处。\
             ⚠ 机器数 **5 处**，我手数是 4 —— 与 F09/F10 首跑同一条纪律的第四次：**以机器数为准**。\
             ★ **退役归 F10**，退役条件与本仓其它 reader 同形：**daemon 侧补一条\
             `--list-marketplaces`**（它已经会读远端 `~/.claude`，`--list-projects` / \
             `--list-sessions` / `--list-subagents` 是现成的形状）。那条一落地，\
             本机改走后端、远端那半（今天记在 `parity_ledger` 的 `plugins.marketplaces` \
             那行上）也一起补平 —— **一件事同时清两笔账**。",
        ),
        (
            "src/search.rs",
            "reader",
            4,
            "全文索引构建时遍历 records 目录。\
             ⚠ **〔F10b 第三批实测〕这一处刻意不退役，解锁条件明确** —— 它与前几批**不是同一类**：\
             前几批是「查询直通」（monitor 只是转发 + 反序列化），而本文件维护一个\
             **本地全文索引**（`build_blocking` 走一遍 records 建内存索引，之后每次搜索走内存）。\
             而 daemon 的 `--search` **没有索引**：它每次调用用 `WalkDir` 走一遍 \
             `<claude_dir>/projects/**/*.jsonl`（见 `observe/search_query.rs` 头注）。\
             ⇒ 迁它 = 把「建一次索引 + 内存查」换成「每次搜索 spawn 一个进程 + 走全部 jsonl」，\
             **那是用性能换账面**，而本机恰好是用户搜得最多的那一侧。\
             ★ **退役归「daemon 侧也有索引」之后**（或一条能便宜地喂索引的查询）——那就是它的解锁条件。\
             那对远端同样有价值 —— 今天远端每次搜索也在走全库。已进 `ROADMAP §5`。",
        ),
        (
            "src/adapter.rs",
            "reader",
            3,
            "适配器层的 `records_dir`/`tasks_dir` 解析（哪个 agent 的记录目录）。退役归 F10 本体。",
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
            8,
            "读 `.claude.json` 里的 MCP 服务器声明。退役归 F10 本体。\
             ⚠ 08-06 从 2 改到 7：`.claude.json` 的**三个候选路径**（项目 / 上级 / 家目录）\
             与本地·远端两个来源标签此前都不在针里 —— 也就是说这条读面的**大部分**没被数到。\
             ⚠ 〔devbench F10b，08-10〕7 → 8，多出来的**不是新读点**：是给远端读加超限拒收时\
             那句错误文案里提到了 `.claude.json`。★ 与上一条〔F10b-2 订正分类〕同一个口径问题 ——\
             `hits()` 数的是「提到那几个词的行」，**提示文案也算**。⇒ 这一格**不算工作量增加**。",
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
            3,
            "T02 配置面审计视图（只读、不轮询）。退役归 F10 本体。",
        ),
        (
            "src/hooks_diag.rs",
            "reader",
            7,
            "hooks 诊断读 settings。退役归 F10 本体。\
             ⚠ 08-06 从 1 改到 7：原先只数到 `CLAUDE_CONFIG_DIR` 那一行，\
             而**真正读盘的那几行**（`~/.claude/settings.json` 的三条失败诊断文案 · \
             `home.join(\".claude\")` 兜底路径 · 远端探测串 · 来源标签）全在针外。",
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
            5,
            "T01 受管工具登记表的一句**文案**里提到它，不读文件 ⇒ **不属**读面。\
             ⚠ **08-10（devbench F06）4 → 5**：新增的 `NOT_MANAGED` 反向登记表里，\
             `planned-build` 那条理由写着它装在 `<claude_dir>/skills/planned-build/`。\
             仍是**文案**（说明它为什么不由 cc-monitor 装），零文件读取。",
        ),
        (
            "src/skill_host.rs",
            "non-read",
            5,
            "★ **devbench F02 新增，且这条登记本身逮到了一个真缺陷** ——\
             不是「记上账」那么简单，值得写清楚：\n\
             五处命中（**F03 从 3 涨到 5**：新增 IPC 层的 `views()` 里一处\
             `paths::resolve_claude_dir()` 调用 + 一处 `claude_dir` 局部变量）：\
             `discover()` 的参数名与它内部的 `claude_dir.join(\"skills\")`、\
             声明表里的 `root: \".claude/planned-build\"`，以及 IPC 那两处。\
             ⚠ **仍然零文件内容读取 Claude 数据** —— `discover` 只 `Path::exists()`，\
             `instances` 只 `read_dir` 列目录项，`read_skill_file` 读的是\
             **计划目录里的 `INBOX.txt`**（用户自己项目的文件，不是 Claude 的 jsonl）。\
             ⇒ 归 `non-read`，不属 F10 的退役范围。\n\
             ⚠⚠ **本表的针把两种 `.claude` 混在一起了，这里必须点破**：一种是\
             `~/.claude`（**Claude 的数据目录**，projects/sessions 住那儿，F10 要退役的是它）；\
             另一种是 `<cwd>/.claude`（**项目自己的配置目录**，planned-build 的计划就住那儿）。\
             本模块碰的是**后者**，与 Claude 的数据源无关。针只认字面 `.claude` ⇒ 两者同样命中，\
             靠这一列的分类来分开。**这不是针的缺陷**（放宽命中面是对的），是分类该干的活。\n\
             ★ **它逮到的真缺陷**：本模块初版写的是 `home.join(\".claude/skills\")` ——\
             **写死了 `~/.claude`**。而本仓有多账号隔离（`cc-acct-iso`：每账号一个\
             `CLAUDE_CONFIG_DIR`，`skills`/`memory` symlink 回共享库）⇒ **切号之后那条路径会指错**。\
             本表报「多一处未登记」时我才去读它，才发现该用 `paths::resolve_claude_dir()`。\
             ⇒ 已改成 `claude_dir` 入参。**这条判据的价值不止于账本完整性。**",
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
            // 〔audit-0805 08-06〕**`.claude` 这个目录名本身也要算**。
            //
            // 原来四个针里最"宽"的是 `.claude/projects` —— 于是
            // `home.join(".claude")`、`~/.claude/settings.json`、`.claude.json`
            // 这些**同样是本机 claude 面**的写法一个都不在人群里。
            // 实测加上它之后：本机读面从 60 行涨到 78 行（+18），
            // **没有新文件**，全落在已登记的五个文件上 ——
            // 也就是说漏的不是"某个没人知道的模块"，而是**已登记文件里没被数到的那些行**。
            // 那更坏：登记表看起来是全的，数字却比事实小，而 F10 的工作量正是按这个数估的。
            format!(".{c}"),
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

    /// ★★ **谁伸手进用户的 home —— 一张覆盖索引**〔audit-0805 08-08，Phase G 第 88 件〕。
    ///
    /// # 为什么再加一张表（它不是第四个权威源）
    ///
    /// 「谁能读」这一侧本会话补了两块：`.ssh`（第 85 件）与本模块的 claude 目录棘轮。
    /// 但两块的人群都是**按目录名**取的 —— 也就是说，**下一个被伸手的目录仍然不在任何人群里**
    ///（08-08 实测：`~/.config` / `~/.local` 本机读面今天为 0，那是「碰巧没有」不是「有人守着」）。
    ///
    /// ⇒ 换一个**真正派生**的人群：生产段里每一处 `home_dir()` 调用。
    /// 它按构造覆盖 claude / `.ssh` / profile / `.codex` / 数据目录 / 将来任何新目录。
    /// 第四列写的是**这一处归谁管** —— 本表只回答「有没有人管」，
    /// 具体守法仍在各自那张表里（E3：本表不复制它们的内容）。
    ///
    /// ⚠ **扫描面只有 monitor 树**（本模块的 `rust_files()` 就是这么定的）。
    /// daemon 侧另有 3 处 `home_dir()`（`observe/accounts_query.rs`），**刻意不并进来**：
    /// 那是**远端那台机器上**的 home，语义不同（monitor 碰的是用户自己的机器），
    /// 而 daemon 的写侧由它自己的 `readonly_guard` 整个禁掉。
    /// 把两侧混进一张表会让「这一处归谁管」这一列失去意义。
    const HOME_REACHES: &[(&str, &str, &str, &str)] = &[
        (
            "codex.rs",
            "resolve_codex_dir",
            "`~/.codex`",
            "Codex 那一族的用量读面；与 claude 面平行，由 usage-core 的口径判据管",
        ),
        (
            "config_surface.rs",
            "config_surface_report",
            "配置面清单的根",
            "只读诊断页；落点由本模块的 claude 棘轮数着",
        ),
        (
            "data_paths.rs",
            "candidate_profile_dirs",
            "PowerShell profile 的候选目录",
            "profile 安装面；路径围栏在 `profile_installer::fence_profile_path`",
        ),
        (
            "hooks_diag.rs",
            "diagnose_local_cc_bus_hooks",
            "cc-bus 钩子的安装位置",
            "只读诊断；本文件另有 `this_module_never_writes` 守着不写",
        ),
        (
            "local_daemon.rs",
            "start_local_backend",
            "`~/.cc-monitor/bin`（P2z 自释放内嵌 daemon 的落点）",
            "**不是伸手拿用户的东西**：这是 monitor 自己的缓存目录，只有我们写、只有我们读。\
             它用 `home_dir()` 只是为了「每个用户各一份」。写侧登记在 `write_site_registry` 的\
             `local_backend.rs::extract_embedded_to`；释放出来的文件按 build_id 命名 ⇒ 幂等、不覆盖别版",
        ),
        (
            "local_accounts.rs",
            "local_accts_dir",
            "账号隔离目录",
            "账号面；写侧在 `write_site_registry`",
        ),
        (
            "mcp.rs",
            "claude_json_candidates",
            "`~/.claude.json`",
            "**读**用户配置（SS-14 只许写 `.mcp.json`，写侧由 `mcp.rs` 的项目目录判据钉）",
        ),
        (
            "paths.rs",
            "resolve_claude_dir",
            "`~/.claude`",
            "claude 目录真相源（`hub`）；本模块棘轮的中心",
        ),
        (
            "paths.rs",
            "resolve_monitor_data_dir",
            "`~/.claude/claudecode-frontend`",
            "monitor 自己的数据目录；写侧在 `write_site_registry`",
        ),
        (
            "profile_installer.rs",
            "fence_profile_path",
            "home 本身（当**围栏基准**）",
            "它不是「伸手拿东西」，是**拿 home 来划界** —— 第 86 件加的那道围栏",
        ),
        (
            "ssh_source.rs",
            "list_ssh_host_aliases",
            "`~/.ssh/config`",
            "第 85 件的 `.ssh` 读面表：恰好一处 + 只吐别名",
        ),
        (
            "ssh_source.rs",
            "expand_tilde",
            "`~` 展开（不落到具体目录）",
            "纯路径变换，调用方各自受自己那张表管",
        ),
    ];

    /// ★ 正题：**每一处 `home_dir()` 都要在表里，且表里不留死行**。
    #[test]
    fn every_reach_into_the_user_home_is_indexed() {
        let files = rust_files();
        assert!(
            files.len() >= 80,
            "只扫到 {} 个 .rs —— 遍历器坏了，本条会零命中地绿",
            files.len()
        );
        let needle = format!("home_{}()", "dir");
        let mut found: Vec<(String, String)> = Vec::new();
        for (rel, src) in &files {
            let prod = guard_core::production_code(src);
            let lines: Vec<&str> = prod.lines().collect();
            for (i, l) in lines.iter().enumerate() {
                if !l.contains(&needle) || l.trim_start().starts_with("fn ") {
                    continue;
                }
                // 归属按「上一处 `fn 名字`」判（粗，但归错会红在名字对不上上）。
                let mut fname = "<找不到外层函数>".to_string();
                for prev in lines[..=i].iter().rev() {
                    if let Some(rest) = prev
                        .split(" fn ")
                        .nth(1)
                        .or_else(|| prev.strip_prefix("fn "))
                    {
                        fname = rest
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        break;
                    }
                }
                let stem = rel.rsplit('/').next().unwrap_or(rel).to_string();
                found.push((stem, fname));
            }
        }
        found.sort();
        found.dedup();
        let mut declared: Vec<(String, String)> = HOME_REACHES
            .iter()
            .map(|(f, n, _, _)| (f.to_string(), n.to_string()))
            .collect();
        declared.sort();
        assert_eq!(
            found, declared,
            "伸手进用户 home 的落点变了。\n\
             ★ 本表回答的是「**有没有人管**」，不是「怎么管」——多出来的那一处，\n\
             要在第四列写清它归哪张表（claude 棘轮 / `.ssh` 读面表 / profile 围栏 / 写点表 …）。\n\
             ⚠ 之所以按 `home_dir()` 取人群而不是按目录名：按目录名取的话，\n\
             **下一个被伸手的目录仍然不在任何人群里** —— 08-08 实测 `~/.config`/`~/.local`\n\
             本机读面为 0，那是「碰巧没有」不是「有人守着」。\n\
             ⚠ 少了的：那一处被删/改名了 ⇒ 删登记；若是**扫描面缩了**（`rust_files()` 少扫了），\n\
             先修扫描面，别改表。"
        );
    }

    /// ★ 抽取器自检。
    #[test]
    fn the_scan_actually_reads_the_monitor_tree() {
        let files = rust_files();
        assert!(
            // ★ audit-0805 F16：60 → **80**（今日实测，与 `structural_scan` 同一棵树）。
            files.len() >= 80,
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
        // 〔F10b 末批〕**按角色分条之后，同一个文件可以有多条登记** ⇒ 这里先按文件把处数**加起来**
        // 再与实测比。⚠ 这不是放宽：实测那一侧仍然是「文件 → 命中行数」，两边的口径必须一致，
        // 而分条改变的是**登记的粒度**，不是被比的量。
        let mut sum: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for (f, _, n, _) in REGISTERED {
            *sum.entry((*f).to_string()).or_default() += *n;
        }
        let mut want: Vec<(String, usize)> = sum.into_iter().collect();
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
                matches!(
                    *kind,
                    "hub"
                        | "reader"
                        | "payload"
                        | "remote"
                        | "non-read"
                        | "fence"
                        | "write"
                        | "no-counterpart"
                ),
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
            readers, 8,
            "`reader` 条数变了（**实测 8 条** —— ⚠ 这句话本身腐过一次：数字从 11 一路走到 7，\
             而这段文案一直写着「实测 10 条」，是 S11 那族出现在**判据自己的报错文案**里）。这个数就是 **F10 的真实工作面** —— \
             多一条要说明为什么又加了直读点，少一条说明退役了一处（把棘轮往下拧）。\n\
             ⚠ 棘轮史：11 → **10**（F10b 第一批，`usage.rs` 退役 —— 它改走本机后端的 `--usage`）\n\
             → **9**（F10b 第二批：`accounts.rs` **改分类**为 `remote` —— 它本来就不是本机读面，\n\
             ⚠ **那一格不算退役、不算工作量减少**，只是把误分类改对，理由写在它自己那条登记里）\n\
             → **8**（F10b 第二批·下半：`local_accounts.rs` **真退役** —— 那 3 个命中全属\n\
             `list_local_session_accounts` 一个函数，它改走 sidecar 的 `--session-accounts`；\n\
             顺带删掉 `proc_claude_config_dir`/`pid_alive` 两个**平台原语的第二份实现**，\n\
             它们的家在 daemon 的 `platform/proc.rs`）。\n\
             → **7**（F10b 末批：`history.rs` 的 reader 条**转成 `no-counterpart`** ——\n\
             ⚠ **那不是退役**，是量清「今天的查询集下它迁不了」并写明退役条件。\n\
             ★ 至此本机读面**在现有 daemon 查询集下已无可退**：剩下的每一处都有\n\
             有名有姓的缺口（缺字段 / 缺索引 / 缺 codex 支持 / 根本不是读面）。\n\
             ⚠ **别把这个数往上调**：往上调等于承认又加了直读点，那要先说清为什么。\n\
             → **8**〔`P8a` 08-12〕**本表第一次往上走**，说清如下：`plugins.rs` 新开了\n\
             marketplace 只读枚举。⚠ 数字与上面那个 8 撞了名而**来历相反**（那次是退役退下来的，\n\
             这次是加上去的）—— 别把这段史读成「回到了那时的状态」。\n\
             为什么不绕开：绕法只有两条，**两条都更差** —— ① 走 daemon（那要新子命令 +\n\
             BUILD_ID + 协议文档 + 内嵌重编，是另一件事的体量，而本件是梯队 5 的只读面）；\n\
             ② 不做（`U10d` 已裁「marketplace 面的只读枚举**可做**」）。\n\
             ⇒ 记账不记功：退役条件写在那条登记里，且它与远端那半是**同一条**\n\
             （daemon 补 `--list-marketplaces` 一次清两笔）。"
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
        // ⚠ **F01b 留的那条死限已由 P3 刀 0 解除**（原文：「本地 sid 一进 `tmux_raw_registry`，
        // `/branch` 的灰点 bug 会回来」）。当时成立，是因为本地那条 diff **只产 `Gone`**；
        // P3 刀 0 让它按 `pid + procStart` 判出 `Superseded`（要正面证据，缺 `procStart` 退回 `Gone`）
        // ⇒ 进表之后 `/branch` 会走 `(Some(origin), Superseded)` = 归档，不再是灰点。
        // 由 `session_map::diff_detects_superseded_only_with_positive_identity_evidence` 钉住。
        // ★ 留着这段而不是删掉：**限制解除的理由本身是要交代的** ——
        // 否则下一个人只看到限制没了，不知道换了什么在保证它。
    }
}
