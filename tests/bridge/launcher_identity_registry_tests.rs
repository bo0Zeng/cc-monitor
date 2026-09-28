use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// 账本里「起一条会话」的能力。
///
/// `session.launch` 是**从界面起**的那几条路。〔C4e · 第四波 4C〕原来还有第二项 `launch.send-into`
/// （「往**已存在**的 tmux 送载荷」那条，它同样会让一个 agent 进程出生 ⇒ 同属本表的人群）：
/// 它唯一的命令 `backend_send_into`〔散文墓碑〕迁到界面（`src/tmux-control.ts::sendInto` 经通道直接说后端的 `launch`），
/// 那项能力随之从账本里没了 ⇒ `L3` 从「账本那半」挪到「锚点那半」（人群一个没少，见 `REGISTERED` 那一行）。
const LAUNCH_CAPS: &[&str] = &["session.launch"];

/// `L1` 那条路今天的行为判据 —— 它没了，本表也要红。
///
/// ⚠ 这一格买的只是「那条判据还在」，**不是**「那条判据有牙」。
/// 有没有牙由它自己那五格与逐刀变异回答，本表不重复买。
const PLANTED_JUDGE: &str = "the_identity_token_is_planted_and_handed_back";

/// 一处**起会话方**。
///
/// 「起会话方」的口径（`K-P5 §3 二` 定的，本表原样沿用）：生产代码里**最后一处能决定
/// 那次 agent 进程启动环境**的地方 —— 它产出的那一串 / 那一组 argv 直接导致一个 agent
/// 进程出生，且在它之后没有别的生产代码还有机会往这次启动的环境里加东西。
struct Launcher {
    /// 摸底那一拍给的标号，别改（件文件与上报口都按它点名）。
    label: &'static str,
    /// **机器枚举那一半**：它在账本里的 tauri 命令名。空 = 这一处进不了账本。
    ledger_cmds: &'static [&'static str],
    /// **人点那一半**：`(相对仓根的路径, 认它的针, 生产段里该有几处)`。
    anchors: &'static [(&'static str, &'static str, usize)],
    /// 今天有没有把身份塞进进程环境。
    plants: bool,
    /// 落了 ⇒ 判据住址与它的洞；**没落 ⇒ 为什么没落 + 归谁**（`KP5BD3` 逐条要的那句）。
    why: &'static str,
}

/// **今天的 5 个起会话方**（`K-P5 §3 二` 现打，PM `§6 裁一` 采纳）。
///
/// 🔴 多一处 ⇒ 红；少一处 ⇒ 也红。新增一个起会话方**必须**来这里加一行，
/// 并回答「它把身份塞进环境了吗；没有的话为什么、归谁」。
const REGISTERED: &[Launcher] = &[
    Launcher {
        // 〔MIG-2 · `99 §2.1 ⑬`〕住址换了：计划与渲染搬进本机后端（帧命令 `launch-local`，`control/launch_render/local.rs::plan`），
        //   monitor 只剩开终端窗口那一条 Tauri 命令（`open_local_terminal`）⇒ 账本那半是它，锚点那半指后端那一处。
        label: "L1 · 本机 UI 起（本机后端 `launch-local` → monitor `open_local_terminal`）",
        ledger_cmds: &["open_local_terminal"],
        anchors: &[("src/backend/control/launch_render/local.rs", "let prefix = identity_prefix(&token, facts.windows);", 1)],
        plants: true,
        why: "★ **本拍落的就是这一处**：〔MIG-2〕本机后端 `local.rs::plan` 在拼装那一行把 \
                  `launch_identity` 算出来的那句前缀拼进真正交出去的那一串，token 由 \
                  `payload::route_key_for_session` 铸（**共用那一份，不是第二份**）。\
                  行为判据见 `PLANTED_JUDGE`。\
                  ★〔`K-P5h` `KP5HD1`〕**同一处今天还多买到一格**：那个铸出来的 token \
                  不再被扔掉，而是经 `launch_local` → `new_local_session` 交回给调用方 \
                  （判据 `the_minted_identity_token_is_handed_back_to_the_caller`）—— \
                  `K-P5g` 现打的卡点「写侧把 token 铸完就扔」在这一处收掉了。\
                  ⚠ 那**没有**改这一行的 `plants`：塞不塞进环境与交不交出来是两件事。\
                  ⚠ **它有一个今天补不上的洞，别读成全覆盖**：走 ccm 容器那一支时，\
                  外侧这句 `export` 会在 tmux 边界被吃掉（tmux server 的 `update-environment` \
                  默认列表不含它）—— 与 `K-H2b` 给 `ANTHROPIC_BASE_URL` 踩过的**同一个坑**，\
                  那一次的修法是在容器载荷**内侧**补一句转发。\
                  ★〔`K-R48` 第二拍 09-11 订正〕**那个洞今天补上了**：容器路三条转发\
                  （`CLAUDE_CONFIG_DIR` / `ANTHROPIC_BASE_URL` / `CCM_LAUNCH_ID`）都在，\
                  由 `control::ccm::plan::tests::the_container_path_forwards_every_inherited_variable_inward` \
                  逐条钉。⚠ 补的是**转发**那一格，不是本行的 `plants` —— 塞不塞进环境与转不转发是两件事。",
    },
    Launcher {
        label: "L2 · 开窗（`launch.rs::launch_remote_terminal`，远端与本机开窗共用）",
        ledger_cmds: &["launch_remote_terminal"],
        anchors: &[],
        plants: false,
        why: "今天没落，两条理由。① **它的漏斗在前端**（`remote-launch-run.ts` 的 \
                  `invokeLaunchOrCopyFallback`），而 `K-P5b` 的写区里一个 `.ts` 都没有。\
                  ② 更要紧的一条：`K-P5 §3 四` 现打过，这条路上**今天已经有一个起会话时\
                  打身份的落点**（`src/session-backend.ts` 的 `setSid`，写的是 tmux option）\
                  ⇒ 它要做的是**换载体**，不是像 `L1` 那样**加一条线**，形状不同，不许照抄。\
                  **归 PM 下一拍（写区要含前端）。**",
    },
    Launcher {
        // 〔C4e · 第四波 4C〕住址换了：monitor 的 Tauri 命令 `backend_send_into`〔散文墓碑〕→ 界面 `src/tmux-control.ts::sendInto`
        //   （经通道直接说后端的 `launch{mode:"send-into"}`）。它不再是 Tauri 命令 ⇒ 进不了账本，改成人点的锚点。
        label: "L3 · 往已存在的 tmux 送载荷（`src/tmux-control.ts::sendInto`）",
        ledger_cmds: &[],
        anchors: &[("src/tmux-control.ts", "export async function sendInto(", 1)],
        plants: false,
        why: "今天没落，理由是**它不是「起一条新会话」**：send-into 把载荷送进一条\
                  **已经存在**的 tmux 会话，那条会话的身份在它**建的时候**就该打过了 —— \
                  `K-P5 §3 三` 现打：这条路的 `ccmSid` 逐字是 `undefined`，\
                  注释写着「复用会话已在建时打过标」。在这里再塞一次会造出**第二个身份来源**，\
                  而那正是本族要消灭的东西。**归 `L2` 那一拍一起想**（同一条前端漏斗）。",
    },
    Launcher {
        label: "T1 · POSIX 终端里的那一下（`control/ccm` 的 `exec`）",
        ledger_cmds: &[],
        // 🔴 〔`K-R48` 第二拍 09-11〕**住址换了：`shared/ccm` → `control/ccm/mod.rs`。**
        //    〔用@09-11 `K33`〕那个 bash 脚本删了，`exec` 那一下搬进了后端二进制的
        //    一次性模式（`exec_or_spawn`：POSIX 上 `CommandExt::exec`，非 unix 退成
        //    「起它 + 等它 + 透传退出码」）。**这一处今天仍然没落身份**，理由见 `why`。
        anchors: &[
            ("src/backend/control/ccm/mod.rs", "fn exec_or_spawn(", 1),
            ("src/backend/control/ccm/mod.rs", ".exec()", 1),
        ],
        plants: false,
        why: "今天没落，理由**换了一条，而且比原来那条硬**。原来写的是「`shared/ccm` 是 \
                  `K-P5b` 派工单逐字点名的红线文件（本拍不许碰）」——那是**流程**理由，\
                  而那个文件 `K-R48`（09-11）已经删了，理由随它作废。\
                  今天的理由是**没裁**：一次性模式在 tmux 内由谁去打 `@ccm_sid` 这件事\
                  `K-R48` 第一拍逐字登记为「没裁」（直接后果是那 356 条判词里的 2 条 `K`）。\
                  身份今天只由常驻那份打，而一次性模式跑在用户终端里、不是常驻那份。\
                  ⚠ **别照抄 `CC_BUS_ID` 那个形状**：那一个是「起会话方 `export` 给自己起的 agent」，\
                  而身份要的是「让 cc-monitor 认得出这条会话」—— 两件事的载体不同（env vs tmux option）。\
                  **归 `K-R48` 的下一拍**（先裁「一次性模式在 tmux 内由谁打 `@ccm_sid`」，再接线）。",
    },
    Launcher {
        label: "T2 · Windows 终端里的那一下（`platform/shell/dialect.rs` 写的 `function cc`，别名块经 `assets/aliases/block.rs` 装）",
        ledger_cmds: &[],
        // 🔴 〔`KR135D2` 09-15〕**锚点跟着翻正了**：那一行从 `& claude $RemainingArgs`
        //    改成走 `ccm`（`K33`「所有命令只许有一处」＋ `K28`）。`K-R132` 上一轮现打
        //    验过「动那一行 ⇒ 本条与 profile_installer 那条同时红」——**两处一起改**
        //    正是它当时要求的，不是绕过它。
        //    ⚠ 锚点钉的是**源码里那个 format 串**（`{word}` 现算自 `CCM_ENTRY_WORD`），
        //    不是渲染后的文本 —— 抄一份 `ccm` 进来就是那个词的第二个住址。
        // 〔MIG-3a · 主会话 09-27 裁〕别名块进了那台后端：生成 `function cc` 的那一处住 `src/backend/assets/aliases/block.rs`
        //   （`{word}` 现算自后端 `control::ccm::SUBCOMMAND_WORD`）。
        // 〔OSA · V156〕PowerShell 那个 `function cc` 的写法搬进后端 OS 适配层（`platform/shell/dialect.rs::ps_wrapper_function`），
        //   `{word}` 仍由通用层交进来（`control::ccm::SUBCOMMAND_WORD`）。
        anchors: &[(
            "src/backend/platform/shell/dialect.rs",
            "& {word} $RemainingArgs",
            1,
        )],
        plants: false,
        why: "今天没落，理由**不是技术上做不到，是件计划逐字禁止在本件单独裁它**：\
                  `K-P5b §4` 写着 `T2` 与 `K-P4`（调出终端两平台一套语义）在同一片面上，\
                  而 `K-P4` 下一拍也是摸底 ⇒ **两件一起想，别在这里单独裁一半**。\
                  ⚠ 现打的一格值得记：这条路**已经在铸 nonce 了**\
                  （`cc.ps1.tpl` 的 `$marker = \"ccm-bind-<PID>-<guid8>\"`），\
                  只是把它落进**窗口标题**而不是环境 ⇒ 它要做的也是**换载体**。\
                  **归 `K-P4` 与本族合并的那一拍。**",
    },
];

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

/// 从账本源码里**按行**抠出 `("<命令>", "<能力>", …)` 这一形。
///
/// ⚠ 为什么不走 `guard_core::production_code`：`parity_ledger.rs` **整个模块在
/// `#[cfg(test)]` 里**，剥完是空的 ⇒ 那把尺子在这里量出来的是零。
/// ⇒ 这里读原文，靠**行的形状**把散文与行尾注释挡在外面
///（账本里有好几处 `assert_eq!(…); // …launch.send-into…` 的行尾注释，
/// 裸数字面串会把它们一起数进来）。
///
/// ⚠ 它认**单行三元组**与 rustfmt 拆开的那一形（`(` 下紧跟两行字面量）；别的跨行写法抠不到。所以下面的自检钉的是「抠到的总行数」，
/// 而那个数是**下界**，不是账本大小（`K-P5 §3 六` 记过同一格：116 / 141 / 145 是三把
/// 作用域不同的尺子，别混读）。
fn ledger_rows(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = raw.lines().map(str::trim).collect();
    for (k, t) in lines.iter().enumerate() {
        // 〔MIG-2 · 合并 fmt 之后〕rustfmt 会把放不下一行的三元组拆成 `(` / `"命令",` / `"能力",` / … / `),`
        //   ⇒ 两种排法都认：拆开那一形取紧跟 `(` 的两行（同样只认「两个字面量打头」）。
        if *t == "(" {
            let lit = |l: Option<&&str>| -> Option<String> {
                let r = l?.strip_prefix('"')?.strip_suffix("\",")?;
                (!r.contains('"')).then(|| r.to_string())
            };
            if let (Some(cmd), Some(cap)) = (lit(lines.get(k + 1)), lit(lines.get(k + 2))) {
                out.push((cmd, cap));
            }
            continue;
        }
        let Some(rest) = t.strip_prefix("(\"") else {
            continue;
        };
        let Some(i) = rest.find('"') else { continue };
        let cmd = &rest[..i];
        let Some(rest) = rest[i + 1..].strip_prefix(", \"") else {
            continue;
        };
        let Some(j) = rest.find('"') else { continue };
        out.push((cmd.to_string(), rest[..j].to_string()));
    }
    out
}

fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {p:?}：{e}"))
}

/// ★★ 半 A（**机器枚举**）：账本里能力属于 [`LAUNCH_CAPS`] 的那些命令，
/// 必须**逐个名字**等于本表登记的那一组。
///
/// 比的是**集合**不是**个数** ⇒ 「删一处、添一处」在这一半上是**红的**（头注那张表）。
#[test]
fn the_ledger_half_of_the_launcher_population_matches_the_registry() {
    // 〔步 7c 剖分 2026-09-19 · C 类〕那张平价账本跟着测试段搬进了 `tests/bridge/`。
    let raw = read("tests/bridge/parity_ledger_tests.rs");
    let rows = ledger_rows(&raw);

    // 抽取器自检 ①：抠得到东西（否则下面是空真）。
    // 〔C4e · 第四波 4C〕地板 100 → 99：抓屏 · 杀会话 · 送键 · 就地 resume 四条命令退役，单行三元组人群真少了 4 行（现打 99）。
    // 〔C4e 批 3b〕地板 99 → 94：cc-bus 查在线 · 发消息 · 派生 · 广播 · 收掉五条命令退役，单行三元组人群真少了 5 行（现打 94）。
    // 〔US1 · 第四波 4D〕地板 94 → 93：`read_apikey_credentials_status` / `apikey_routing_for` 两条退役（单行三元组人群真少了；现打 93）。
    // 〔HX2 · 第四波 4D〕地板 93 → 92：`write_apikey_credentials_key` 退役（单行三元组人群真少了 1 行；现打 92）。
    // 〔LOC1a · 第四波 4D〕地板 93 → 92：`get_session_tasks` 退役（单行三元组人群真少了 1 行；现打 92）。
    // 〔合并 LOC1b × 主线 290d8c33〕主线 92 ＋ LOC1b −3（全文搜索 · 查索引状态 · 重建索引）⇒ 89。
    // 〔SH1 · 4D〕地板 88 → 86：驾驶舱读面两条命令退役（单行三元组人群真少了 2 行）。
    // 〔MIG-3a〕地板 86 → 79：MCP 读写六条 ＋ 推拉两条命令退役（单行三元组人群真少了 7 行；现打 79）。
    // 〔MIG-3a〕地板 79 → 75：资产同步 ＋ skill 装卸三条命令退役（单行三元组人群真少了 4 行；现打 75）。
    // 〔MIG-3a〕地板 75 → 73：acct-iso 两问退役（现打 73）。
    // 〔MIG-3a〕地板 73 → 70：收件箱三条退役（现打 70）。
    // 〔MIG-3a〕地板 70 → 65：别名六条退役 −6、`bound_terminal_count` ＋1（现打 65）。
    assert!(
        rows.len() >= 47, // 〔MIG-3b 续〕48 → 47：全景问 · 写 · 撤三行退役、`panorama_place` · `chan_cancel` 两行进（现打 47）// 〔MIG-3b 续〕49 → 48：公钥推送那一行退役（现打 48）// 〔MIG-3a · 09-28 预裁〕50 → 49：`deploy_remote_acct_iso`〔散文墓碑〕 退役（现打 49）// 〔合并 MIG-1 × 主线 eebf51de〕主线 57 ＋ MIG-1 本路退役的单行三元组（列 tmux 两条 · 测试连接 · 端口转发 · 活会话等）⇒ 现打 50 // 〔合并 MIG-3b × 主线 81f92f6a〕主线 60 ＋ MIG-3b −3（钩子诊断本机那一行 · 删会话 · 分叉三条单行三元组退役）// 〔合并 MIG-3a × 主线 ad308378〕基数 75 ＋ MIG-3a −11（acct-iso −2 · 收件箱 −3 · 别名 −5 · cc-bus −1）＋ MIG-2 −4 ⇒ 60（现打；MIG-2 那一侧的读数写的是 −3，合并后实数 −4） // 〔MIG-2〕75 → 72：本机起会话 ＋ 渲染 ＋ 探针那几条单行命令退役 −5、`open_local_terminal` / `relay_all_sessions_switch` 进 +2（现打 72）// 〔合并 HX2 × 主线 06b5dc08〕基于 290d8c33：LOC1b −3 ＋ HX2 −1 ⇒ 88。〔SH1〕−2 ⇒ 86。
        "只从账本里抠到 {} 行单行三元组（09-02 现打 116）—— 抽取器坏了，本条会零命中地绿",
        rows.len()
    );
    // 抽取器自检 ②：**非空对照** —— 这把尺子分得出「起会话」与别的能力，
    // 不是恒答「全都是起会话」。
    let all_caps: BTreeSet<&str> = rows.iter().map(|(_, c)| c.as_str()).collect();
    assert!(
        all_caps.len() >= 20,
        "抠出来的能力只有 {} 种 —— 筛子把别的能力也吞了，下面那条不再是「筛出起会话」",
        all_caps.len()
    );

    let found: BTreeSet<&str> = rows
        .iter()
        .filter(|(_, cap)| LAUNCH_CAPS.contains(&cap.as_str()))
        .map(|(cmd, _)| cmd.as_str())
        .collect();
    let want: BTreeSet<&str> = REGISTERED
        .iter()
        .flat_map(|l| l.ledger_cmds.iter().copied())
        .collect();
    assert_eq!(
        found, want,
        "\n★★ 账本里的**起会话方**与本表对不上。\n\
             **多一处** = 又多了一个起会话方 —— 来 `REGISTERED` 加一行，\n\
             并回答那句必答题：**它把身份塞进进程环境了吗？没有的话为什么、归谁？**\n\
             **少一处** = 退役了一条路 —— 把那一行删掉，并把棘轮往下拧。\n\
             ⚠ 名字换了（一删一添）在这里也是红的：本条比的是**集合**，不是个数。"
    );
}

/// ★★ 半 B（**人点的**）：两条终端启动器还在它们被登记的地方。
///
/// `T1` / `T2` **不是 tauri 命令，结构上进不了账本** —— 这是 `K-P5` 摸底顶回来、
/// PM `§6 裁一` 采纳的那一格，不是本表偷懒。
#[test]
fn the_terminal_launchers_are_still_where_the_registry_says() {
    for l in REGISTERED {
        for (path, needle, want) in l.anchors {
            let raw = read(path);
            // 剥生产段：`.rs` 连测试段一起剥，shell 只剥整行 `#` 注释。
            // ⚠ **两种都走共享原语，本文件不自己写第二份剥法** —— `structural_scan` 那条
            //   「每一个剥注释的变换器都要登记」当场逮到过本文件的第一版（它自己写了个
            //   `starts_with('#')` 的 `production()`），而那条登记表住在**本拍的红线文件**里。
            //   ⇒ 正确出路是它给的第一条：**改成调共享原语**，不是去改登记表。
            let prod = if path.ends_with(".rs") {
                guard_core::production_code(&raw)
            } else {
                guard_core::strip_hash_comment_lines(&raw)
            };
            // 抽取器自检：剥完还剩东西（剥法坏了 ⇒ 下面是空真）。
            assert!(
                prod.len() > 2000,
                "{path} 剥完只剩 {} 字节 —— 剥法坏了，{} 这一格是空真",
                prod.len(),
                l.label
            );
            let n = prod.matches(needle).count();
            assert_eq!(
                n, *want,
                "\n★ {} 的锚点 `{needle}` 在 {path} 的生产段里有 {n} 处（登记 {want} 处）。\n\
                     **少了** = 这条终端启动器搬家或退役了 ⇒ 来本表改登记（顺便看看棘轮能不能往下拧）。\n\
                     **多了** = 同一个文件里多了一处起 agent 的地方 ⇒ 它也得回答那句必答题。\n\
                     ⚠ 本条**看不见新文件里的第三个终端启动器** —— 那是模块头注写明的诚实边界。",
                l.label
            );
        }
    }
}

/// ★★ 正题（`KP5BD3`）：**今天恰好一处**把身份塞进了环境，
/// 其余每一处都写明了「为什么没落 + 归谁」。
///
/// 这个 `1` 就是棘轮本身：落第二处的那一天本条会红，红的用途是**逼人回来把这个数拧上去**
/// 并把那一行的 `why` 从「为什么没落」改成「判据住哪」。
#[test]
fn exactly_one_launcher_plants_the_identity_today() {
    assert_eq!(
        REGISTERED.len(),
        5,
        "起会话方从 5 处变了 —— 先回模块头注读那张「买得到 / 买不到」的表，再改这个数"
    );
    let planted: Vec<&str> = REGISTERED
        .iter()
        .filter(|l| l.plants)
        .map(|l| l.label)
        .collect();
    assert_eq!(
        planted.len(),
        1,
        "\n把身份塞进环境的起会话方有 {} 处（登记 1 处 = `L1`）。\n\
             **多了** ⇒ 好事：把这个数拧上去，并把那一行的 `why` 改成判据住址。\n\
             **少了** ⇒ `L1` 那条路上的身份注入被摘掉了。实得：{planted:?}",
        planted.len()
    );
    assert!(
        planted[0].starts_with("L1"),
        "落地的那一处不是 `L1` 了 —— 本件 `KP5BD3` 逐字挑的是它：{planted:?}"
    );

    // 没落的那四处：每一处都要说得出「为什么」和「归谁」。
    // 形状照 `session_name_registry::every_duplicate_producer_names_its_retirement_owner`。
    let mut pending = 0usize;
    for l in REGISTERED {
        if l.plants {
            continue;
        }
        pending += 1;
        assert!(
            l.why.len() > 120,
            "{}：「今天为什么没落」写得太短，等于没写（{} 字节）",
            l.label,
            l.why.len()
        );
        assert!(
            l.why.contains('归'),
            "{}：写了为什么没落，却没说**归谁**下一拍做 —— \
                 那样它就只是一条抱怨，不是一笔挂了账的债",
            l.label
        );
    }
    assert_eq!(
        pending, 4,
        "还没落身份的起会话方从 4 处变了 —— 棘轮该往下拧了（或者有人往回走了）"
    );

    // `L1` 那条路的行为判据还在 —— 它没了，本表也要红。〔MIG-2〕判据随那条路搬进了后端测试段。
    let hist = read("tests/backend/control/launch_render/local_tests.rs");
    assert!(
        hist.contains(PLANTED_JUDGE),
        "`L1` 那条路的行为判据 `{PLANTED_JUDGE}` 不在 `local_tests.rs` 里了 —— \
             登记表说它落了身份，而**证明这件事的那条判据被删了**"
    );
}

/// ★★ `KP5BD1` 的「只有一份」那一半：**身份变量名与铸法各只有一个家**。
///
/// 人群 = `src/bridge/src` **整棵树**的 `.rs` 生产段（`scan_tree!` 目录扫描，不是手写名单）。
///
/// ⚠ 〔`P4` 2026-09-21〕先前括号里那两道保险的**第一道今天不生效**：原文是「本文件
/// 按构造被摘除，而且它整个在 `#[cfg(test)]` 里，剥完也是空的」。「按构造被摘除」那一刀
/// 在这一处不生效（判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中）。
/// ⇒ 承重的是另外两样：**住址**（本文件住 `tests/bridge/`，不在这棵树里）＋ 它整个在
/// `#[cfg(test)]` 里、剥完是空的。
///
/// # ⚠ 它买不到什么（如实写）
///
/// - 它只看 Rust 侧的 `src/bridge/src`。别的树（`src/backend/` · 前端 `src/` ·
///   `shared/`）里再写一份，本条一个字节都不会动。⇒ 那一天要靠本表的**人群**那两条，
///   而人群那两条只挡得住「新增起会话方」，挡不住「同一处又写了第二份铸法」。
/// - 它买的是「**没有第二个家**」，不是「这一个家里写得对」——
///   后者是 `PLANTED_JUDGE` 那五格的活。
#[test]
fn the_identity_token_has_exactly_one_mint_and_one_env_var_name() {
    // 〔MIG-2〕身份那一格随本机起会话搬进后端：人群 = 两棵 Rust 生产树（monitor ＋ 后端）。
    let files: Vec<(PathBuf, String)> = ["src/bridge/src", "src/backend"]
        .iter()
        .flat_map(|t| guard_core::scan_tree_excluding(&repo_root().join(t), &["rs"], &[]))
        .collect();
    assert!(
        files.len() >= 250,
        "只扫到 {} 份 `.rs` —— 遍历坏了，本条会零命中地绿",
        files.len()
    );
    // (针, 该有几处, 那几处分别是什么)
    let probes: [(&str, usize, &str); 2] = [
        (
            "\"CCM_LAUNCH_ID\"",
            // 〔OSA · V156〕4 → 5：容器路「往里转」那一处从格式串 `"export CCM_LAUNCH_ID={}; …"` 换成 `posix::export("CCM_LAUNCH_ID", …)`
            //   （`export` 的写法搬进 `platform::shell::posix`），同一处、变量名成了独立字面量 —— 家没多。
            5,
            "写侧 `launch_render/local.rs::LAUNCH_ID_VAR` 1 ＋ 读侧 `observe/accounts_query.rs::LAUNCH_ID_ENV` 1 ＋ `ccm/plan.rs` 容器路三处（读继承的 · 判继承值 · 往里转）",
        ),
        (
            "route_key_for_session(",
            2,
            "`launch_render/payload.rs` 的定义 1 ＋ `launch_render/local.rs` 身份那一处 1",
        ),
    ];
    let mut counts = [0usize; 2];
    let mut sites: Vec<String> = Vec::new();
    for (path, raw) in &files {
        let prod = guard_core::production_code(raw);
        for (k, (needle, _, _)) in probes.iter().enumerate() {
            let n = prod.matches(needle).count();
            if n > 0 {
                counts[k] += n;
                sites.push(format!("{} `{needle}` × {n}", path.to_string_lossy()));
            }
        }
    }
    for (k, (needle, want, what)) in probes.iter().enumerate() {
        assert_eq!(
            counts[k], *want,
            "\n★★ `{needle}` 在两棵生产树里有 {} 处（期望 {want} 处 = {what}）。多了 ⇒ 身份长出了第二个家；少了 ⇒ 注入或铸法被摘掉了。\n  {}",
            counts[k],
            sites.join("\n  ")
        );
    }
}
