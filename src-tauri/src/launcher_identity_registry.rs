//! **起会话方身份落点清账 + 递减棘轮**〔`K-P5b` 的 `KP5BD2`〕。
//!
//! # 人群是「**所有起会话方**」，不是「所有经后端起的会话」
//!
//! `K-P5` 的摸底把后一种写法顶回去了，理由是现打的：**「经后端起」这个谓词今天对
//! 5 个起会话方里的 2 个为假** —— `T2` 整条路一次后端都不调；`T1` 只问两件事
//! （`--list-accounts` / `--resolve`）且两件都诚实降级，决定 argv、设 env、`exec`
//! 三件事全归它自己。照那句话立判据，人群会退化成 `L1`/`L2`/`L3`（全是从界面起的），
//! 而那三条本来就有 sid 有账号 ⇒ **判据恒绿**，本件的正主整个落在人群之外。
//! ⇒ PM 裁走乙：人群改成「所有起会话方」+ 这张**递减棘轮登记表**。
//!
//! # 它买的是什么（`K-P5b` 派工单逐字，别读宽）
//!
//! 买的是「**新增一个起会话方时有人红**」，**不是**「今天这 5 处都对」。
//! 今天 5 处里只有 1 处真的把身份塞进了环境（`L1`），另外 4 处每一处都写明了
//! 「今天为什么没落 + 归谁」——**多一处 ⇒ 红**（防回潮）；**少一处 ⇒ 也红**
//! （提醒把棘轮往下拧）。形状照 `session_name_registry` / `polling_registry`。
//!
//! # 🔴 「删一处添一处看不见」这一格，正面答
//!
//! 这一形是 `K-R17` 那一拍逐字记过的失效面。本表**一半买到、一半买不到**：
//!
//! | 半 | 怎么枚举 | 删一处 | 添一处 | 一删一添 |
//! |---|---|---|---|---|
//! | **账本那半**（`L1`/`L2`/`L3`）| 从 `parity_ledger.rs` 里**按行抠出**能力等于 [`LAUNCH_CAPS`] 的那些行，比的是**命令名的集合** | 红 | 红 | **红**（名字变了，集合就不等） |
//! | **终端那半**（`T1`/`T2`）| 人点的两个锚点（它们**不是 tauri 命令，结构上进不了账本**）| 红（锚点没了 ⇒ 命中数掉） | **看不见**（一个**新文件**里出现第三个终端启动器，本表一个字节都不动） | **半红** —— 删的那一半会红，添的那一半不会 |
//!
//! ⇒ 一句话：**账本那半是集合相等（一删一添红），终端那半只挡得住删、挡不住新文件里的添。**
//! 这是一条**诚实边界**，不是「今天恰好没有」。要买住终端那半的「添」，
//! 得给它一个**目录扫描型**的人群（自己定扫描面 + 排除 `vendor/` / `node_modules/` / `target/`），
//! 而那一改的风险正是本工作区最高频的那族病（量具的作用域对不上它守的性质）
//! ⇒ **单独立件再做，别顺手改**（同 `payload.rs` 那张五格闭表给自己写的解锁条件）。
//!
//! 注：本模块整体在 `#[cfg(test)]` 内，非测试构建为空、零运行期开销。

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// 账本里「起一条会话」的那两项能力。
    ///
    /// 两项而不是一项：`session.launch` 是三条**从界面起**的路，
    /// `launch.send-into` 是「往**已存在**的 tmux 送载荷」那条（它不在 `session.launch` 里，
    /// 但它同样会让一个 agent 进程出生 ⇒ 同属本表的人群）。
    const LAUNCH_CAPS: &[&str] = &["session.launch", "launch.send-into"];

    /// `L1` 那条路今天的行为判据 —— 它没了，本表也要红。
    ///
    /// ⚠ 这一格买的只是「那条判据还在」，**不是**「那条判据有牙」。
    /// 有没有牙由它自己那五格与逐刀变异回答，本表不重复买。
    const PLANTED_JUDGE: &str = "the_launcher_plants_the_session_identity_into_the_process_environment";

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
            label: "L1 · 本机 UI 起（tauri 命令 → `history.rs::launch_local`）",
            ledger_cmds: &["resume_history_session", "new_local_session"],
            anchors: &[],
            plants: true,
            why: "★ **本拍落的就是这一处**：`history.rs::launch_local` 在拼装那一行把 \
                  `launch_identity_prefix` 拼进真正交出去的那一串，token 由 \
                  `payload::route_key_for_session` 铸（**共用那一份，不是第二份**）。\
                  行为判据见 `PLANTED_JUDGE`。\
                  ⚠ **它有一个今天补不上的洞，别读成全覆盖**：走 ccm 容器那一支时，\
                  外侧这句 `export` 会在 tmux 边界被吃掉（tmux server 的 `update-environment` \
                  默认列表不含它）—— 与 `K-H2b` 给 `ANTHROPIC_BASE_URL` 踩过的**同一个坑**，\
                  那一次的修法是在 `shared/ccm` 的容器载荷**内侧**补一句转发。\
                  `shared/ccm` 是 `K-P5b` 逐字点名的红线文件 ⇒ 那一句**本拍没补**，\
                  归 PM 解掉红线之后的那一拍。",
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
            label: "L3 · 往已存在的 tmux 送载荷（`daemon_launch.rs::daemon_send_into`）",
            ledger_cmds: &["daemon_send_into"],
            anchors: &[],
            plants: false,
            why: "今天没落，理由是**它不是「起一条新会话」**：send-into 把载荷送进一条\
                  **已经存在**的 tmux 会话，那条会话的身份在它**建的时候**就该打过了 —— \
                  `K-P5 §3 三` 现打：这条路的 `ccmSid` 逐字是 `undefined`，\
                  注释写着「复用会话已在建时打过标」。在这里再塞一次会造出**第二个身份来源**，\
                  而那正是本族要消灭的东西。**归 `L2` 那一拍一起想**（同一条前端漏斗）。",
        },
        Launcher {
            label: "T1 · POSIX 终端里的那一下（`shared/ccm` 的 `exec`）",
            ledger_cmds: &[],
            anchors: &[
                ("shared/ccm", "exec \"${argv[@]}\"", 1),
                ("shared/ccm", "exec bash -c \"$seq\"", 1),
            ],
            plants: false,
            why: "今天没落，理由是 `shared/ccm` 是 `K-P5b` 派工单逐字点名的**红线文件**\
                  （本拍不许碰）。⚠ 它同时是 5 处里**最便宜的一处**：同一个文件里\
                  已经有一份一模一样的形状 —— `derive_bus_id` / `CC_BUS_ID`\
                  （起会话方在 `exec` 前 `export`、**无条件覆盖继承值**、函数与配方两种表示、\
                  由 `e2e/ccm-contract-parity.sh` A 组差分钉住）⇒ 照抄即可。\
                  **归 PM 解掉红线之后的那一拍。**",
        },
        Launcher {
            label: "T2 · Windows 终端里的那一下（`profile_installer.rs` 生成的 `function cc`）",
            ledger_cmds: &[],
            anchors: &[(
                "src-tauri/src/profile_installer.rs",
                "& claude $RemainingArgs",
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
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri 的上级")
            .to_path_buf()
    }

    /// 从账本源码里**按行**抠出 `("<命令>", "<能力>", …)` 这一形。
    ///
    /// ⚠ 为什么不走 `guard_core::production_code`：`parity_ledger.rs` **整个模块在
    /// `#[cfg(test)]` 里**，剥完是空的 ⇒ 那把尺子在这里量出来的是零。
    /// ⇒ 这里读原文，靠**行的形状**把散文与行尾注释挡在外面
    ///（账本里有好几处 `assert_eq!(…); // …launch.send-into…` 的行尾注释，
    /// 裸数字面串会把它们一起数进来）。
    ///
    /// ⚠ 它只认**单行三元组** —— 跨行写的那些抠不到。所以下面的自检钉的是「抠到的总行数」，
    /// 而那个数是**下界**，不是账本大小（`K-P5 §3 六` 记过同一格：116 / 141 / 145 是三把
    /// 作用域不同的尺子，别混读）。
    fn ledger_rows(raw: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for line in raw.lines() {
            let t = line.trim();
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
        let raw = read("src-tauri/src/parity_ledger.rs");
        let rows = ledger_rows(&raw);

        // 抽取器自检 ①：抠得到东西（否则下面是空真）。
        assert!(
            rows.len() >= 100,
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

        // `L1` 那条路的行为判据还在 —— 它没了，本表也要红。
        let hist = read("src-tauri/src/history.rs");
        assert!(
            hist.contains(PLANTED_JUDGE),
            "`L1` 那条路的行为判据 `{PLANTED_JUDGE}` 不在 `history.rs` 里了 —— \
             登记表说它落了身份，而**证明这件事的那条判据被删了**"
        );
    }

    /// ★★ `KP5BD1` 的「只有一份」那一半：**身份变量名与铸法各只有一个家**。
    ///
    /// 人群 = `src-tauri/src` **整棵树**的 `.rs` 生产段（`scan_tree!` 目录扫描，
    /// 不是手写名单；本文件按构造被摘除，而且它整个在 `#[cfg(test)]` 里，剥完也是空的）。
    ///
    /// # ⚠ 它买不到什么（如实写）
    ///
    /// - 它只看 Rust 侧的 `src-tauri/src`。别的树（`remote-daemon-proto/` · 前端 `src/` ·
    ///   `shared/`）里再写一份，本条一个字节都不会动。⇒ 那一天要靠本表的**人群**那两条，
    ///   而人群那两条只挡得住「新增起会话方」，挡不住「同一处又写了第二份铸法」。
    /// - 它买的是「**没有第二个家**」，不是「这一个家里写得对」——
    ///   后者是 `PLANTED_JUDGE` 那五格的活。
    #[test]
    fn the_identity_token_has_exactly_one_mint_and_one_env_var_name() {
        let root = repo_root().join("src-tauri/src");
        let files = guard_core::scan_tree!(&root, &["rs"]);
        assert!(
            files.len() >= 90,
            "只扫到 {} 份 `.rs`（09-02 现打 104）—— 遍历坏了，本条会零命中地绿",
            files.len()
        );

        // (针, 该有几处, 那几处分别是什么)
        let probes: [(&str, usize, &str); 2] = [
            (
                crate::history::LAUNCH_ID_VAR,
                1,
                "`history.rs` 里那个 `LAUNCH_ID_VAR` 常量的字面量，仅此一处",
            ),
            (
                "route_key_for_session(",
                3,
                "`payload.rs` 的定义 1 + 中转那一处 1 + `history.rs` 身份那一处 1",
            ),
        ];
        let mut counts = [0usize; 2];
        let mut prod_total = 0usize;
        let mut sites: Vec<String> = Vec::new();
        for (path, raw) in &files {
            let prod = guard_core::production_code(raw);
            prod_total += prod.len();
            for (k, (needle, _, _)) in probes.iter().enumerate() {
                let n = prod.matches(needle).count();
                if n > 0 {
                    counts[k] += n;
                    sites.push(format!("{} `{needle}` × {n}", path.to_string_lossy()));
                }
            }
        }
        assert!(
            prod_total > 200_000,
            "全树剥完只剩 {prod_total} 字节 —— 剥法坏了，本条是空真"
        );
        for (k, (needle, want, what)) in probes.iter().enumerate() {
            assert_eq!(
                counts[k], *want,
                "\n★★ `{needle}` 在 `src-tauri/src` 的生产段里有 {} 处（期望 {want} 处 = {what}）。\n\
                 **多了** ⇒ 身份这件事长出了第二个家。`KP5BD1` 的「铸法只有一份」是承重的：\n\
                 照 `K-H2c` 那一拍买到的形状，**共用一份实现**才能让「漂开」在结构上不可表示；\n\
                 两处各写一份、再用判据焊住，买到的只是「今天这几条输入两侧同答」。\n\
                 **少了** ⇒ 身份注入或那份铸法被摘掉了。\n\
                 命中分布：\n  {}",
                counts[k],
                sites.join("\n  ")
            );
        }
    }
}
