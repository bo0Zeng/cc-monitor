use std::fs;
use std::path::{Path, PathBuf};

/// 本机读面登记表：`(相对 src/bridge 的路径, 类别, 命中行数, 为什么 + 退役归谁)`。
///
/// 多一处 ⇒ 下面那条红（防「F10 还没做而直读点增长」）；
/// 少一处 ⇒ **也红**（退役了要把棘轮往下拧）。
const REGISTERED: &[(&str, &str, usize, &str)] = &[
    // 〔F10b 末批〕`history.rs` **按角色分条** —— 逐函数量过，那些命中不是一类活。
    // ★ 分条的理由：登记表原本按「文件 × 单一类别」记账，而这个文件承载多种角色 ⇒
    // 「读面迁完」时那条登记不会消失、`readers` 也不会降，账就成了假的。
    // 🔴 〔`K-R97` 09-12〕**那条 `reader` 真退役了，本文件的处数之和 15 → 13。**
    //    `list_history_projects` 不再 `resolve_claude_dir()` + `records_dir()` 自己遍历，
    //    改问本机后端要 `--list-projects`（住 `backend/observe/local_query.rs`）——
    //    这正是那条登记自己写着的退役条件，逐字兑现。⚠ **分条这件事因此付了息**：
    //    它当初就是为了让「迁完了」这件事在账上看得见，而今天它确实少了一行。
    // 〔C4d · 第四波 4B〕`src/history.rs` 两行摘了：`fence` 2（展开一个项目的路径围栏）与 `no-counterpart` 1（Codex 合成项目）——
    //   本机历史清单整段搬进本机常驻后端（`history-sessions` 出成品；Codex 枚举进后端 `agents/codex/history.rs`，
    //   `no-counterpart` 那一行自己写的退役条件「backend 侧补上 codex 的项目枚举」兑现了）。9 → 6。
    // 〔LOC1b · 第四波 4D〕`src/history.rs` 的 `fence` 一行（1 处：`stream_read_session_jsonl` 本机那一支解析 records 根
    //   只为验 `target.starts_with(&root)`）摘了 —— 那一支删了：本机冷读也经本机后端的 `history-read`，围栏归后端
    //   （`observe/history_query.rs::validate_session_path`）。那一行写着的「两侧各一道」换成了两侧同一道廉价预检
    //   （拒 `..` ＋ 必须 `.jsonl`，不解析根）。⚠ 这一行的针只认 `claude_dir` 一族，**从没数到**那一支真正的读
    //   （`File::open`）—— 那个读者这一拍一起没了（B-decouple §2.2 点名的漏数）。
    // 〔RW1 · 第四波 09-24〕这里原来有 `src/history.rs` 的 `write` 一行（写操作恰好也读 dir 来定位文件：
    //   `delete_history_session` · `create_branch_session`，4 处）。两件都改经后端（删会话 `files-delete-session`
    //   只收 sid · 本机分叉 exec 本机后端 `--fork-session`），本进程不再解析 dir 来定位要写的那一份 ⇒ 4 → 0，摘行。
    (
        "src/history.rs",
        "payload",
        2,
        "把 `CLAUDE_CONFIG_DIR` 拼进**启动命令串**：`config_dir_prefix_ps`(1102/1111)。\
             〔CP2b · 4C〕`validate_config_dir_posix` / `validate_config_dir_ps` 那三行是拒绝的话里提到这个变量名，\
             话搬进了文案表（`rsHistory.configDir.invalid`），源码里只剩取文口 ⇒ 5 → 2。\
             ⚠ **不属读面**，随 F06/F07 走。",
    ),
    (
        "src/ssh_source.rs",
        "remote",
        9,
        "★ **说的全是远端主机的 claude 目录**：backend `hello` 帧的 `claude_dir` 字段。\
             **根本不是本机读面** ⇒ 不属 F10。\
             〔`K-R59` 09-11：**10 → 9**。退役的那 1 行是原先并列写在这里的第二样 —— \
             `daemonless` 那条远端 shell 串里的 `\\${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects`，\
             随定框 `K35` 整段删除。⚠ **口径没变，仍是远端**：拧下来的不是「本机读面少了一行」。〕\
             ⚠ 我摸底时差点把它算成本机的 8 行 —— 同名最便宜的误导。\
             〔backend-split `S4` 08-14〕**8 → 10**：additive 迁移在消费侧多了一个解析点 —— \
             `claude_home_from_hello`（优先 `hello.homes`、回退 `claude_dir`）加上它的两处调用。\
             口径**没变**，还是远端：涨的两行说的仍是**远端** backend 自陈的目录，不是本机的。\
             ⚠ 这两行是**真的多出来的**，不是数字漂了 —— 本来可以把参数改名躲开针来保住 8，\
             那才是「改数字了事」的镜像（为了不动数字去拧代码）。",
    ),
    (
        "src/lib.rs",
        "hub",
        3,
        "启动时解析 `claude_dir` 并派生 sessions/tasks 等目录往下传 —— \
             **一处入口，不读内容**。切后端之后仍要在（得告诉后端读哪儿）⇒ **不属**退役范围。\
             〔CF1 · 第四波 09-24〕**7 → 6**：projects 那一处（喂 monitor 自己那套 jsonl watcher）随 watcher 删了 ——\
             本机会话内容改走本机后端的 `line` 帧，monitor 这一侧不再需要知道 projects 在哪。\
             〔LOC1b · 第四波 4D〕**6 → 4**：给全文索引构建线程交 `claude_dir` 那两行随本机内存索引删了（本机搜索改问本机后端）；\
             **4 → 3**：派生 `sessions/` 目录那一行随 monitor 自己那份判活删了（本机判活改由本机后端的帧来）。",
    ),
    (
        "src/paths.rs",
        "hub",
        10,
        "**路径真相源** —— 只回答「`~/.claude` 与它的子目录在哪」，自己不读内容。\
             切后端之后它**仍然要在** ⇒ **不属**退役范围。\
             〔`P17` 2026-09-22：9 → 10。多的那一处是 `env::var(CCM_DATA_DIR)` ——\
              monitor 自己那个数据目录的**出口**（此前它一处出口都没有 ⇒ 任何一趟\
              「把 monitor 跑起来量点东西」都会写进用户真 profile，09-21 在那台\
              Win11 虚拟机上现打到过这一形）。⇒ 它同样是**路径真相源、不读内容**，\
              归 `hub`；切后端之后**仍然要在**（得告诉后端「这一趟的家在哪」）。〕",
    ),
    (
        "src/local_backend_host.rs",
        "non-read",
        4,
        "〔`K-P1` 08-26〕**一个字节的用户数据都没读。**四个命中全是「拿这条路径当身份比」：\
             3 处在 `hello_verdict`（解 hello 帧的**冻结 wire 字段** `claude_dir` + 比 + 那句诊断），\
             1 处是 `start_detached` 里问一次 `paths::resolve_claude_dir()` —— \
             它只用来**算那台机的监听口**（`listen_port_for`）并核对「那个口上的后端看的是不是同一个目录」。\
             ⚠ 这一格恰恰是**反过来**的：它存在的理由是**不许静默复用**别人的后端。\
             ⇒ **不属**退役范围（切后端之后仍要有人回答「我该连哪个口」）。",
    ),
    (
        "src/backend/control/payload.rs",
        "payload",
        3,
        "只把 `CLAUDE_CONFIG_DIR` 拼进要给别人执行的载荷（`env` 前缀那一段）。\
             **不读任何文件** ⇒ **不属**读面。它随 F06/F07 走。\
             〔CP2b · 第四波 09-25〕**5 → 3**：少的两处是两句拒收报错里写着的 `CLAUDE_CONFIG_DIR` 字样，\
             随全量抽表搬进文案表（`rsPayload.*`）；拼载荷的那三处没动。口径没变。",
    ),
    (
        "src/cc_bus_deploy.rs",
        "write",
        14,
        "〔RW1 · 第四波 09-24〕**13 → 14**：部署改经本机后端写之后，`deploy_into` 收一个 `claude_dir` 参数并把它交给后端当根\
             （多一处 `claude_dir` 引用）；本进程一个字节不写，写那一跳住后端 `files-put` / `files-rename` / `files-chmod`。\n\
             `PS1` 的部署：`fenced_dest` 解析 `<claude_dir>/skills` 并做 realpath 围栏、\
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
    // 〔RM1b · 第四波〕那一条（`src/plugins.rs`，`reader`，5 处）**真退役**，退役条件原文逐字兑现：
    //   「backend 侧补一条 `--list-marketplaces`……那条一落地，本机改走后端、远端那半……也一起补平
    //   —— 一件事同时清两笔账」。落地的名字是帧命令 `plugins-marketplaces`（CLI 面 `--plugins-marketplaces`），
    //   本体从 `plugins.rs` 原样搬进后端 `observe/plugins_query.rs` ⇒ 本文件 0 处，整行摘掉。
    // 〔LOC1b · 第四波 4D〕`src/search.rs` 那条 `reader`（4 处：全文索引构建时遍历 records 目录）**退役**，但**不是**按它自己写的
    //   解锁条件（「backend 侧也有索引」）退的：主会话 09-25 按目标形裁（`00 §2.5 ①` 本机远端同一条代码路径 · `90 §4 F`
    //   搜索收口到后端），本机搜索改问本机后端的 `history-search`、每次现扫。那一行担心的代价（「用性能换账面」）如实量过：
    //   `调研/第四波记录/LOC1b.md §3`。本文件 0 处，整行摘掉。
    // 〔LOC1b · 第四波 4D〕`src/adapter.rs` 那条 `reader`（3 处：`records_dir` / `liveness_dir` 一族的路径解析）**真退役**：
    //   那几个门面零调用方（冷读 · 判活 · 搜索都改问本机后端），删了 ⇒ 本文件 0 处，整行摘掉。
    // 〔RM1b · 第四波〕`src/tasks.rs` 那条 `reader`（3 处，「读 `tasks/<sid>/*.json`（issue #11 的任务面）。
    //   退役归 F10 本体。」）**真退役**：读任务文件搬进后端 `tasks-list`（本机与远端同一条路），
    //   monitor 那份直读函数与「解析当前任务根」那个小帮手一起删了 ⇒ 本文件 0 处，整行摘掉。
    // 〔C4c · 第四波 4B〕`src/accounts.rs` 那条 `remote`（2 处：「远端后端版本较旧…」那两句提示里提到了配置目录那个环境变量）
    //   **整行摘掉**：那两句随 `degraded_notice`〔散文墓碑〕一起删了（账号清单改由那台机器的后端出成品、「缺账号 0」那句由后端说）
    //   ⇒ 本文件 0 处。⚠ 与这张表别的「往下走」一样**不算工作量减少**：它本来就不是本机读面。
    (
        "src/mcp.rs",
        "reader",
        6,
        "读 `.claude.json` 里的 MCP 服务器声明。退役归 F10 本体。\
             ⚠ 08-06 从 2 改到 7：`.claude.json` 的**三个候选路径**（项目 / 上级 / 家目录）\
             与本地·远端两个来源标签此前都不在针里 —— 也就是说这条读面的**大部分**没被数到。\
             ⚠ 〔devbench F10b，08-10〕7 → 8，多出来的**不是新读点**：是给远端读加超限拒收时\
             那句错误文案里提到了 `.claude.json`。★ 与上一条〔F10b-2 订正分类〕同一个口径问题 ——\
             `hits()` 数的是「提到那几个词的行」，**提示文案也算**。⇒ 这一格**不算工作量增加**。\
             〔CP2b · 第四波 09-25〕**8 → 6**：同一口径反过来 —— 读远端 `.claude.json` 失败 / 超限的两句报错\
             搬进文案表（`rsMcp.remote.*`），本文件少了两行「提到」。读点一个没少，**不算退役**。",
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
        2,
        "T02 配置面审计视图（只读、不轮询）。退役归 F10 本体。\
             〔CP2b · 第四波 09-25〕**3 → 2**：项目级作用域那句展示文案（`<项目目录>/.claude/settings.json …`）\
             搬进文案表（`rsConfigSurface.scope.projectPath`）。读点没少，**不算退役**。",
    ),
    (
        "src/hooks_diag.rs",
        "reader",
        4,
        "hooks 诊断读 settings。退役归 F10 本体。\
             ⚠ 08-06 从 1 改到 7：原先只数到 `CLAUDE_CONFIG_DIR` 那一行，\
             而**真正读盘的那几行**（`~/.claude/settings.json` 的三条失败诊断文案 · \
             `home.join(\".claude\")` 兜底路径 · 远端探测串 · 来源标签）全在针外。\
             〔CP2b · 第四波 09-25〕**7 → 4**：那三条失败诊断**文案**搬进文案表（`rsHooksDiag.diagnose.*`）；\
             读盘那几行一个没少，**不算退役**。",
    ),
    // 🔴 〔`K-R48` 第二拍 09-11〕原来这里有一行 `src/ccm_cli_contract.rs`（`non-read` 1 处：
    //    契约清单里出现过 `CLAUDE_CONFIG_DIR` 这个变量名）。本拍把那个模块从 2773 行砍到
    //    只剩 7 条 cc-spawn 判据，那张清单随 `shared/ccm` 一起删了 ⇒ 那个变量名不再出现。
    //    **账跟着删**（登记表腐烂比没有登记更糟）。
    (
        "src/tool_registry.rs",
        "non-read",
        8,
        "T01 受管工具登记表的一句**文案**里提到它，不读文件 ⇒ **不属**读面。\
             ⚠ **08-10（devbench F06）4 → 5**：新增的 `NOT_MANAGED` 反向登记表里，\
             `planned-build` 那条理由写着它装在 `<claude_dir>/skills/planned-build/`。\
             仍是**文案**（说明它为什么不由 cc-monitor 装），零文件读取。\
             ⚠ **09-11（`K-R60`）5 → 7**：两条新的**申报路径字面量** —— \
             `~/.claude/skills/cc-bus`（cc-bus 的 `installable` 从假申报改对之后，\
             『装得了就必须申报装到哪』当场要它）与 `~/.claude/projects/`\
             （Claude Code 自己写的会话记录，app 装不了、只读）。\
             两条都仍是**登记表里的申报字面量**，本文件零文件读取 —— \
             真去 stat 它们的是 `config_surface`（已在本表里单列，仍是 3 处）。\
             ⚠ **〔AS2 · 第四波 4B〕7 → 9**：`skill-install` 那一条的两个申报字面量 —— \
             落点 `.claude/skills` 与足迹路径 `~/.claude/skills`（资产目录里「装到这台」的 skill 写在那下面）。\
             仍是登记表里的申报字面量，本文件零文件读取。\
             ⚠ **〔CP2b · 4C〕9 → 8**：cc-acct-iso 账号库那条 `note` 里提到 `CLAUDE_CONFIG_DIR` 的那句话\
             搬进了文案表（`rsToolRegistry.tools.acctIsoVaultNote`），源码里只剩取文口。",
    ),
    (
        "src/skill_host.rs",
        "non-read",
        7,
        "〔RW1 · 第四波 09-24〕**5 → 7**：远端版 `remote_views` 里一处 `claude_dir` 局部变量 ＋ 它的一次使用\
             （远端配置根按 `<后端的 home>/.claude` 算，问的是**那台机器的后端**，本机零读）。\n\
             ★ **devbench F02 新增，且这条登记本身逮到了一个真缺陷** ——\
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
/// backend 侧另有 3 处 `home_dir()`（`observe/accounts_query.rs`），**刻意不并进来**：
/// 那是**远端那台机器上**的 home，语义不同（monitor 碰的是用户自己的机器），
/// 而后端的写侧由它自己的 `readonly_guard` 整个禁掉。
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
        // 〔RM1a〕命令收了 origin 之后，本机那一臂的体搬进 `local_report`（一字未改）；远端那一臂不碰本机 home。
        "local_report",
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
    // 〔AL1 · 2026-09-24〕`设计/71` 别名两跳里要落盘 / 读回的那两条。纯渲染那一条不碰 home。
    (
        "lib.rs",
        "aliases_read",
        "`~/.cc-monitor/aliases.sh`（读回口：那份别名文件今天有哪几条）",
        "只读。monitor 自己的目录，用 `home_dir()` 只为「每个用户各一份」；\
             `home_dir()` 在这里而不在 `account_aliases.rs` 里 —— 那边把 `home` 当参数收，测试拿临时目录当 home。",
    ),
    // 〔RW1 · 第四波 09-24〕这里原来还有 `lib.rs::aliases_install` 一行（它自己 `home_dir()`、再交本进程落盘）。
    //   写改走本机后端之后，home 由后端答（`user_files::Door::home`），这一条不再伸手进用户 home ⇒ 摘行。
    (
        "ccm_probe.rs",
        "local_ccm_entry_status",
        "`~/.cc-monitor/bin/<本机 ccm 入口名>`（`K-R69`：在不在 + 它自报的身份）",
        "**不是伸手拿用户的东西**：这是 monitor 自己的目录，那一份也是我们自己放下去的\
             （写侧登记在 `write_site_registry` 的 `local_backend.rs::install_local_ccm_entry`）。\
             `home_dir()` 只为「每个用户各一份」。\
             🔴 **它刻意够不到 `~/.local/bin/ccm`** —— 用户那份旧的由产品**一个字节都不碰**\
             （`K34` 逐字：原本的配置要手动删除）；那一份的存在与否是靠**跑一次 `--ccm-probe`**\
             问出来的，不是靠 stat 一个路径（比路径认不出同名不同物）。",
    ),
    // 〔RM1f〕本机那一份代码全景小程序的落点：`~/.cc-monitor/bin/`（本机后端找它的第二个候选）。
    (
        "panorama_bytes.rs",
        "place_local",
        "`~/.cc-monitor/bin/cc-monitor-panorama[.exe]`（本机代码全景小程序）",
        "**不是伸手拿用户的东西**：monitor 自己的目录（与自释放出来的本机后端同一个）。只在本机后端答\
             「没装 / 装的太旧」时放一次；写侧登记在 `write_site_registry` 的 `local_backend.rs::place_local_panorama`",
    ),
    (
        "local_backend_host.rs",
        "cc_monitor_dir",
        "`~/.cc-monitor`（`K-P1` 的 attach token 与「谁在听那个口」）",
        "**不是伸手拿用户的东西**：这是 monitor 自己的目录，只有我们写、只有我们读。\
             用 `home_dir()` 正是为了「每个用户各一份」—— 而那恰恰是这一格要买的东西：\
             token 文件 `0600` 是回环 TCP 上**唯一**挡住同机别的用户的门。\
             写侧两条登记在 `write_site_registry` 的 `local_backend_host.rs::ensure_listen_token` 与 \
             `local_backend_host.rs::write_listen_pid`",
    ),
    // 〔SR1a · 2026-09-24〕拨号代理的二进制解析那一行**摘了**：monitor 不再找 / 起拨号代理（拨号挪进本机常驻后端）。
    (
        "local_backend_host.rs",
        "start_local_backend",
        "`~/.cc-monitor/bin`（P2z 自释放内嵌后端的落点）",
        "**不是伸手拿用户的东西**：这是 monitor 自己的缓存目录，只有我们写、只有我们读。\
             它用 `home_dir()` 只是为了「每个用户各一份」。写侧登记在 `write_site_registry` 的\
             `local_backend.rs::extract_embedded_to`；释放出来的文件按 build_id 命名 ⇒ 幂等、不覆盖别版",
    ),
    // 〔C4d · 第四波 4B〕`("local_accounts.rs", "local_accts_dir")` 那一行同拍去掉 —— 它自己写的退役条件兑现了：
    //   钉契约目录名的那条判据搬到了后端（`accounts_query_tests.rs::the_accounts_library_lives_under_the_contract_directory_name`，
    //   对 `acct-core` 的常量与后端缺省解析那一处），本机那份参照实现连同这一处 `home_dir()` 一起删了。
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
        // 〔`设计/60 §5.4e` · `24e` 第二刀 · 09-20〕原生文件管理窗口那颗「本机」按钮的落脚点。
        "shell.rs",
        "local_home",
        "home 本身（当**起点路径**）",
        "它不是「伸手拿东西」，是给那个窗口一个开始浏览的地方 —— \
             之后列哪个目录**由用户走到哪决定**，而列目录那一条是 \
             `filewin::source::list_local`（只读 `read_dir` ＋ `metadata`，不落盘）。\
             ⚠ 与 `profile_installer.rs::fence_profile_path` 那一行**不是同一类**：\
             那一条拿 home 划界（围栏），这一条只是起点，**它不围任何东西** —— \
             也就是说「用户能在这个窗口里浏览到 home 之外」是设计如此，不是漏了围栏。\
             写侧归 `filewin::transfer`（〔F7c 09-24〕上传经通道：monitor 的传输台只写远端暂存区，\
             落进用户目录那一下是后端 `files-commit-upload`，先过围栏）",
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
        readers, 4,
        "`reader` 条数变了（**实测 4 条** —— ⚠ 这句话本身腐过一次：数字从 11 一路走到 7，\
             而这段文案一直写着「实测 10 条」，是 S11 那族出现在**判据自己的报错文案**里）。这个数就是 **F10 的真实工作面** —— \
             多一条要说明为什么又加了直读点，少一条说明退役了一处（把棘轮往下拧）。\n\
             ⚠ 棘轮史：11 → **10**（F10b 第一批，`usage.rs` 退役 —— 它改走本机后端的 `--usage`）\n\
             → **9**（F10b 第二批：`accounts.rs` **改分类**为 `remote` —— 它本来就不是本机读面，\n\
             ⚠ **那一格不算退役、不算工作量减少**，只是把误分类改对，理由写在它自己那条登记里）\n\
             → **8**（F10b 第二批·下半：`local_accounts.rs` **真退役** —— 那 3 个命中全属\n\
             `list_local_session_accounts`〔散文墓碑〕一个函数，它改走本机后端的 `--session-accounts`；\n\
             顺带删掉 `proc_claude_config_dir`/`pid_alive` 两个**平台原语的第二份实现**，\n\
             它们的家在后端的 `platform/proc.rs`）。\n\
             → **7**（F10b 末批：`history.rs` 的 reader 条**转成 `no-counterpart`** ——\n\
             ⚠ **那不是退役**，是量清「今天的查询集下它迁不了」并写明退役条件。\n\
             ★ 至此本机读面**在现有后端查询集下已无可退**：剩下的每一处都有\n\
             有名有姓的缺口（缺字段 / 缺索引 / 缺 codex 支持 / 根本不是读面）。\n\
             ⚠ **别把这个数往上调**：往上调等于承认又加了直读点，那要先说清为什么。\n\
             → **8**〔`P8a` 08-12〕**本表第一次往上走**，说清如下：`plugins.rs` 新开了\n\
             marketplace 只读枚举。⚠ 数字与上面那个 8 撞了名而**来历相反**（那次是退役退下来的，\n\
             这次是加上去的）—— 别把这段史读成「回到了那时的状态」。\n\
             为什么不绕开：绕法只有两条，**两条都更差** —— ① 走后端（那要新子命令 +\n\
             BUILD_ID + 协议文档 + 内嵌重编，是另一件事的体量，而本件是梯队 5 的只读面）；\n\
             ② 不做（`U10d` 已裁「marketplace 面的只读枚举**可做**」）。\n\
             ⇒ 记账不记功：退役条件写在那条登记里，且它与远端那半是**同一条**\n\
             （backend 补 `--list-marketplaces` 一次清两笔）。\n\
             → **9**〔`K-R92` 09-12〕`history.rs` 的 `list_history_projects` 那条\n\
             **从 `no-counterpart` 转回 `reader`**。⚠ **第三次「往上走」，而三次来历各不相同**：\n\
             `P8a` 那次是真加了直读点，这次是**把一处误分类改对**（同 `accounts.rs` 那次的形状、\n\
             方向相反）。理由：`no-counterpart` 的字面含义是「backend 侧没有对侧」，\n\
             而 `K-R83` 已经把 `--list-projects` 那一行的 `sessionIds` 补齐了 ⇒ 对侧有了。\n\
             **F10 的工作面没变大，变真的是账** —— 那一处本来就要迁，只是从前记在\n\
             「等后端补东西」那一栏里，看起来不像工作量。\n\
             → **8**〔`K-R97` 09-12〕**同一条登记，这次是真退役**：`list_history_projects`\n\
             改问本机后端要 `--list-projects`（走 `backend::observe::local_query::run_query`），\n\
             `resolve_claude_dir()` + `records_dir()` 那两行就地消失。\n\
             ⚠ **与上面那些「往下走」的来历也不一样**：`usage.rs`/`local_accounts.rs` 那两次退的是\n\
             一整个文件的读点，这次退的是**一条分条登记**（同文件还剩 fence/write/payload/codex 四类，\n\
             那几类本来就不属退役范围）。⇒ 本文件的处数之和 15 → 13，`readers` 9 → 8。\n\
             ⚠ **没有跟着退的那一半，写清楚免得成暗账**：codex 那条 `no-counterpart` 原封不动 ——\n\
             后端的 `--list-projects` 只服务 claude，本机仍自己合成 codex 的合成项目。\n\
             → **7**〔RM1b · 第四波〕**真退役**：`tasks.rs` 读 `tasks/<sid>/*.json` 那一整条\
             改问那台机器的后端 `tasks-list`（本机也走后端，远端那半随之补平 —— `parity_ledger` \
             `session.tasks` 那笔欠账同拍结清）。本文件处数 3 → 0，整行摘掉。\n\
             → **6**〔RM1b · 第四波〕**真退役**：`plugins.rs` 那条（`P8a` 那次「往上走」加的）\
             按它自己写下的退役条件退掉 —— 后端补了 `plugins-marketplaces`，本机改走后端，\
             `parity_ledger` `plugins.marketplaces` 同拍结清。本文件处数 5 → 0，整行摘掉。\n\
             → **5**〔LOC1b · 第四波 4D〕**退役，但不是按它自己写的解锁条件**：`search.rs` 那条（本机全文索引）——\
             主会话 09-25 按目标形裁（`00 §2.5 ①` 本机远端同一条代码路径），本机搜索改问本机后端 `history-search`、\
             每次现扫（「backend 侧也有索引」那个条件没兑现，代价读数在 `第四波记录/LOC1b.md §3`）。本文件处数 4 → 0，整行摘掉。\n\
             → **4**〔LOC1b · 第四波 4D〕**真退役**：`adapter.rs` 那条（记录目录 / 活性目录的路径解析）—— 那几个门面零调用方、删了。"
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
/// `externalBin` **没有**进主配置 —— 因为 `tauri-build` 要求**当前 target** 的 local_backend
/// 在编译期就存在，进主配置会让 `cargo test` 也需要一份后端二进制，
/// 那正是 C2 反面（两半不许在构建期互相咬住）刚钉住的东西。
/// ⇒ 它住进**发版补丁配置** `tauri.sidecar.conf.json`，只在 `tauri build --config` 时注入。
///
/// ⇒ 本条换成后继形态：**盯新的家**，并且钉住「棘轮一格没放」。
/// ⚠ **这不是降强度**：断言从「一条」变成「三条」（local_backend 契约有家 · stem 与
/// `LOCAL_BACKEND_STEM` 一致 · 棘轮上限没被放宽），而且扫描面从主配置**换到了它真正的家** ——
/// 留在旧扫描面上才是降强度（它永远不会再红）。
#[test]
fn the_local_backend_contract_has_exactly_one_home_and_f10s_ratchet_is_untouched() {
    // ① local_backend 契约必须有家，而且**不在主配置里**（进主配置 = 每个编译点都要一份二进制）。
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
        "`{key}` 回到了主配置 —— 那会让 `cargo test` 也需要一份当前 target 的后端二进制\n\
             （实测报错：`resource path binaries/cc-monitor-backend-<triple> doesn't exist`），\n\
             等于把两半在**构建期**绑死。它的家是 `tauri.sidecar.conf.json`，只在发版时 `--config` 注入。"
    );
    let patch = fs::read_to_string(root().join("tauri.sidecar.conf.json"))
        .expect("读不到 tauri.sidecar.conf.json —— local_backend 契约没有家了");
    assert!(
        patch.contains(key.as_str()),
        "发版补丁配置里没有 `{key}` —— 那安装包里就不会带上本机后端（C7）"
    );

    // ② stem 与 Rust 侧的 `LOCAL_BACKEND_STEM` 必须是同一个（同一个名字不许两侧各写一份，定框 §4）。
    let stem = crate::backend::control::local_backend::LOCAL_BACKEND_STEM;
    assert!(
        patch.contains(&format!("binaries/{stem}")),
        "补丁配置里的本机后端路径与 Rust 侧的 `LOCAL_BACKEND_STEM`（{stem:?}）对不上 —— \n\
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
    // 〔LOC1b · 4D〕判出它的今天是本机后端（`session_removed.cause`），monitor 这边钉「原样交出去」（`ssh_source_f032_idle_tests` ③）。
    // ★ 留着这段而不是删掉：**限制解除的理由本身是要交代的** ——
    // 否则下一个人只看到限制没了，不知道换了什么在保证它。
}
