//! ★★ **`K-R12` 下一拍（09-04）：「tmux 的打印通道必须是 UTF-8」这一个口径的家。**
//!
//! # 先答那一问：`-u` 与 `LC_ALL=C.UTF-8` 是**一个口径的两种表示**，不是两个口径
//!
//! 三条实测（配方与原始输出在 `evidence/K-R12-locale-lab.md` 与
//! `evidence/K-R12-deathvalue.md`，容器内 tmux 3.4 + `-S` 私有 socket + `od -c` 读字节）：
//!
//! 1. **它们拨的是同一个开关。** tmux 判「客户端是不是 UTF-8」只有一个内部标志；
//!    `-u` 直接置它，`LC_ALL`/`LC_CTYPE`/`LANG` 取第一个非空值做一次**大小写不敏感的
//!    `UTF-8`/`UTF8` 子串匹配**后置它。两条路的**结果面逐字节相同**（同一台 server、
//!    同一条命令，只换客户端那一侧）。
//! 2. **它们的优先级是有序的，不是并列的。** `LC_ALL=zh_CN.GB18030` **加** `-u` ⇒ 干净；
//!    `env -i`（环境全清）**加** `-u` ⇒ 干净。⇒ `-u` 压在 env 那一路之上，
//!    两者不是两个各管一半的性质，是同一个性质的两个入口。
//! 3. **但它们在调用点上不可互换。** 这才是为什么家里要放**两个**值而不是一个：
//!    - `sh -c '<脚本串>'` 那一形：脚本里可能有多条 `exec … tmux …` 分支，
//!      改 argv 要**逐条改**，而漏掉的那一条只在缺 `timeout` 的机器上跑 ⇒ 漏了也照绿；
//!      挂在 `Command` 上的一行 env **结构性地**盖住同一个脚本里的全部分支。
//!    - **跨 SSH 的命令串**那一形：那边没有本地 `Command` 可挂 env，
//!      走 SSH 的 `request_env` 要赌对端 `AcceptEnv`（不认就**静默拒绝**）
//!      ⇒ 只有 argv 里那个旗是不用赌的。
//!
//! ⇒ **一个口径（「这个 tmux 客户端必须按 UTF-8 打」）· 两种表示 · 一条按调用点形态选表示的规矩。**
//! 表示是两个，家只有一个 —— 这就是本模块。
//!
//! # 怎么选：**看调用点的形态，不看口味**
//!
//! | 调用点形态 | 用哪个表示 | 为什么 |
//! |---|---|---|
//! | `sh -c '<可能有多条分支的脚本>'` | [`UTF8_CLIENT_ENV`] | 一行盖住全部分支，「漏一条」这个动作不存在 |
//! | 本机 argv 直传（一处就是一处） | [`UTF8_CLIENT_FLAG`] | 不依赖任何继承来的 env，而 daemon 的 env 最不受控 |
//! | 跨 SSH 的命令串 | [`UTF8_CLIENT_FLAG`] | 对端不需要装任何 locale、不需要 sshd 配合 |
//!
//! ⚠ [`UTF8_CLIENT_FLAG`] **必须排在子命令之前**。放到后面实测是
//! `rc=1 + unknown flag -u` —— 那本该是响的，可本仓两处调用点都刻意不看退出码，
//! 于是那个响错会被压成「一个会话都没有」/「会话不存在」。
//! ⇒ 位置这一维由调用点各自的判据单独钉（`control/gate.rs` 与 monitor 的 `tmux.rs` 各一条）。
//!
//! # 为什么它进 `common/` —— 三条门槛逐条对
//!
//! 门槛逐字在 `common/mod.rs`：① ≥2 个**上层**用 · ② 平台无关 · ③ 无域知识。
//!
//! - ① **只在「一个口径」这个读法下成立，如实说清**：把两个表示当成**两个**口径去数，
//!   旗今天只有 `control/` 一层用、env 今天只有 `observe/` 一层用，**两个都不达标**，
//!   诚实的处置就只能是各自留在原地（= 今天这个「三份靠人对齐」的状态）。
//!   按「一个口径两种表示」去数，本模块被 `control/` 与 `observe/` **两层**同时用 ⇒ 达标。
//!   上面那三条实测就是这个读法的依据，不是措辞选择。
//!   [`tab_underflow`] 单独看也达标（两层各有调用点）。
//! - ② **平台无关**：这里只有两个字面量与一个纯字符串谓词，没有平台 `cfg`、
//!   不认识任何 OS 的文件布局或 ABI。tmux 的这条编码规矩在它跑得起来的每个系统上同形。
//! - ③ **无域知识**：不认识 `WatchEvent` / `ResumeSpec` 这类本 crate 的类型。
//!   ⚠ 它确实认识**一个外部程序的接口**（tmux 怎么打、怎么被要求按 UTF-8 打）——
//!   与 `common/fs.rs` 认识 `std::fs` 同型：那是**外部接口知识**，不是本 crate 的域模型。
//!   这一条是判断，不是实测，写在这里让下一个人能反对。
//!
//! # ⚠ 家只有一个，而**镜子有三面**（如实登记边界）
//!
//! 本 crate 内的两个消费者（`control/gate.rs` · `observe/watcher.rs`）**引用**这里，
//! 编译器兜住，漂不了。monitor 是**另一个二进制**：两个 crate 不共享源码树，
//! 而共享 crate 的落点（`src-tauri/crates/*-core`）没有一个的职责装得下
//! 「怎么起 tmux」这件事 ⇒ 那一份只能留在 `src-tauri/src/tmux.rs`，
//! 由它那侧的**跨仓对拍**读本文件把两边焊住（判据名与作用域写在那边的头注里）。

/// **UTF-8 客户端旗（argv 形）** —— 一个口径两种表示里的那个**旗**。
///
/// 用法与位置纪律见模块头注：**必须排在子命令之前**。
pub(crate) const UTF8_CLIENT_FLAG: &str = "-u";

/// **UTF-8 客户端 env（env 形）** —— 同一个口径的另一种表示，`(变量名, 值)`。
///
/// 挂在起 `sh` 的那个 `Command` 上，一行覆盖脚本串里的全部 `exec` 分支。
///
/// ⚠ 值里那个 `UTF-8` 是**给 tmux 的子串匹配看的**，不是给 `setlocale` 看的：
/// 实测 `zz_ZZ.UTF-8`（这个 locale 根本不存在）照样干净，而 `C` / `LC_ALL=''` 脏。
/// ⇒ 「`C.UTF-8` 万一没装」不是本条的失效面；「挂成空串」「忘了挂」才是，而那两种都是静默的。
pub(crate) const UTF8_CLIENT_ENV: (&str, &str) = ("LC_ALL", "C.UTF-8");

/// ★★ **`K-R12` 的 `J1`：段数下溢 —— 「拆不出段」不许长得像「字段是空的」。**
///
/// > 按 TAB 切 tmux 的打印通道，切出的段数 **< 预期 N** ⇒ 出声 **+ 拒绝把这行当好数据**。
///
/// # 为什么是「下溢」而不是「恰好 N」
///
/// 实测：**合法内容只会把段数推高，永远不会推低** —— 会话名里的真 TAB 被 tmux 转义成
/// 字面 `\t` 两个字符（那行段数不变），而 `pane_current_path` 里的真 TAB 会多切一段。
/// ⇒ `< N` **零误报**；`!= N` 会误伤（monitor 的 `parse_tmux_ls` 今天仍在犯，那一条另立件）。
///
/// # 为什么下溢是**完备**检测器
///
/// 那层改写是**每客户端全有全无**的：同一台 server、同一条命令，只换客户端那一侧，
/// 输出要么整条干净、要么整条被改写 ⇒ 通道一脏，格式串里的 TAB **全部**消失，
/// 段数必然从 N 塌到 **1**。**不存在「内容被改写了但 TAB 还在」的中间态**
/// ⇒ 一条判据同时盖住「分隔符被吞」与「内容被改写」两半，**而且格式串一个字节不用动**。
///
/// # 它装在哪
///
/// 装在**raw 的入口**，不是每个 splitter 上：只取第 1 段的那些消费者永远取得到，
/// 下溢判据装在它们身上恒真，而它们在通道脏时拿到的「会话名」就是整行。
pub(crate) fn tab_underflow(line: &str, expected: usize) -> bool {
    line.split('\t').count() < expected
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **本 crate 里「一个口径一个家」的人群**：`(口径名, 家里那一行的逐字声明片段)`。
    ///
    /// ⚠ 这不是词表，是**关系**的一端：下面两条判据把它读成
    /// 「这一行只许出现在本文件里，别处只许出现**引用**」。
    /// 加一个口径进来 ⇒ 它自动进入「家唯一」与「消费者不许自带声明」两条判据的射程。
    ///
    /// # 🔴 为什么是函数而不是 `const` 表 —— 值一律**现算**
    ///
    /// 第一版把 `"-u"` 与 `("LC_ALL", "C.UTF-8")` 手抄进了这张表。那等于**又写了一份**
    /// 同一个值 —— 正是本模块在治的那个病，而且长在治它的代码里
    /// （`brief` 13b：闭集只许一个住址，要印出来就**现算**，不许写字面量；
    ///  `brief` 15：拿本轮的病理回头打一遍自己写的代码）。
    /// 现算之后的分工也更干净：**值漂了**由 monitor 那条跨仓对拍逮
    /// （单断：只有它逮），本表只管**「家在不在、别处有没有第二个」**。
    ///
    /// # 而锚点只钉**声明这件事**，不钉类型写法（否则会误伤）
    ///
    /// 第二版的锚点里带着类型标注（`: &str =` / `: (&str, &str) =`）。实测：把 env 那条改成
    /// `(&'static str, &'static str)`（一个**完全合法、语义零变化**的写法）两侧当场都红，
    /// 而它们印出来的话是「家不在本文件里」（本侧）与「daemon 那个家里少了 env 形」（monitor 侧）
    /// —— **两句都指向完全错误的方向**：真实原因只是有人换了个类型写法。
    /// 〔skill 铁律 18：误伤会训练人绕过判据，比没有判据更坏〕
    /// ⇒ 锚点收成 `const <名>:` / `fn <名>(`：它认的是「这里有一个声明」，
    /// 对类型写法与签名演进免疫，而**引用**（`tmux_utf8::UTF8_CLIENT_FLAG` / `tab_underflow(…)`）
    /// 一个都不会命中。
    ///
    /// # ⚠ 锚点**刻意以非标识符字符收尾**（`:` 或 `(`）
    ///
    /// 第三版写成 `const <名>`（收在标识符上）。**病历要说准**，这一处的经过是：
    /// 那一版跑一次真的改名变异（把家里那个 env 改成 `UTF8_CLIENT_ENVX`）时，daemon 侧
    /// **编译就过不去**（本模块自己的测试引着那个名字）⇒ 那一趟看到的红是**编译器**给的，
    /// 不是本判据给的；而 monitor 那侧的同职断言当时**真的绿着** ——
    /// `const UTF8_CLIENT_ENV` 是 `const UTF8_CLIENT_ENVX` 的**前缀**，裸 `contains` 照样命中。
    /// 那正是本仓「**匹配单位比事实小**」那一族（`find_pinned` 的头注逐字记着
    /// `remote-daemon-proto` 被 `remote-daemon-proto-X` 撑大那个活体）。
    /// 收在 `:` / `(` 上之后，「被撑大」这一形按构造不可能发生。
    ///
    /// # 表里存的是**标识符**，锚点由 [`decl_anchor`] 现拼
    ///
    /// 🔴 这一层不是为了少打字：反向自检要造一份**改了名的声明**当夹具，
    /// 而那份夹具必须从**标识符**派生，**不许从锚点的文本派生** ——
    /// 从锚点派生的夹具会跟着锚点一起变松，于是「锚点变松了」这件事**自己看不见**。
    /// 实测两趟：夹具从**锚点文本**派生那一版，把锚点末尾那个 `:` 去掉 ⇒ 反向自检**照样绿**；
    /// 改成从**标识符**派生之后，同一刀当场 `FAILED`（报文逐字：
    /// 「`const UTF8_CLIENT_FLAGX` 这样一个改了名的声明也被算成第二个家」）。
    /// 〔`E3`：判据与活体夹具必须共用同一份权威源 —— 这里那份权威源是标识符，不是锚点〕
    fn kou_jing_homes() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("UTF-8 客户端旗（argv 形）", "const", "UTF8_CLIENT_FLAG"),
            ("UTF-8 客户端 env（env 形）", "const", "UTF8_CLIENT_ENV"),
            ("段数下溢判据（J1）", "fn", "tab_underflow"),
        ]
    }

    /// 「这里有一个名叫 `ident` 的 `kind` 声明」这一事实的锚点。
    ///
    /// 末尾那个 `:` / `(` 是承重的（理由见 [`kou_jing_homes`] 的第二段）。
    fn decl_anchor(kind: &str, ident: &str) -> String {
        match kind {
            "const" => format!("const {ident}:"),
            "fn" => format!("fn {ident}("),
            other => panic!("不认识的声明种类 `{other}` —— 加一种就在这里加一条"),
        }
    }

    /// 本 crate 里**引用**这个家的两个消费者（相对 `src/`）—— 层各一个，正是门槛 ① 的那两层。
    const CONSUMERS: &[&str] = &["control/gate.rs", "observe/watcher.rs"];

    /// 判据的**核**：给一份 `(名字, 生产段)` 表与一条声明，数出「除家以外还有谁也声明了它」。
    ///
    /// # 为什么抽出来
    ///
    /// 下面「家唯一」那条今天的真值是 **0 处** ⇒ 它是一条 `[] == []` 的空真断言，
    /// 而〔`brief` 第 9 条〕治空真只能靠**喂真会违规的输入**、并且必须喂给**真判据本身** ——
    /// 夹具另写一份扫描，证明的是那一份。⇒ 真判据与反向自检都只经由这一个函数。
    fn second_homes(files: &[(String, String)], decl: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (name, prod) in files {
            let n = prod.matches(decl).count();
            if n > 0 {
                out.push(format!("{name}（{n} 处）"));
            }
        }
        out.sort();
        out
    }

    /// 本 crate 生产段语料（**摘除本文件自己** —— 家在这里，不摘就人人都是「第二个家」）。
    fn crate_production_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        guard_core::scan_tree!(&root, &["rs"])
            .into_iter()
            .map(|(p, raw)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, crate::guard_support::production_code(&raw))
            })
            .collect()
    }

    /// ★★ **正题一：每个口径在本 crate 里恰好一个家，而那个家就是本文件。**
    ///
    /// 两半都断言，缺一半这条就只买到一半：
    /// - **正面**：家里那一行**真的在**（`find_pinned` 要求恰好一处、且两侧有边界）——
    ///   没有这一半，把家删掉之后「别处零声明」照样成立，本条会绿着报「口径只有一个家」。
    /// - **反面**：别处**零声明**。
    #[test]
    fn each_kou_jing_has_exactly_one_home_and_it_is_this_file() {
        let home = crate::guard_support::production_code(include_str!("tmux_utf8.rs"));
        crate::guard_support::assert_no_test_code("common/tmux_utf8.rs", &home);
        let others = crate_production_sources();
        // 反空真：语料塌了 ⇒ 下面「别处零声明」会空着绿。
        assert!(
            others.len() >= 30,
            "本 crate 只采到 {} 个 .rs —— 遍历坏了，「别处零声明」此刻是空转的",
            others.len()
        );
        let bytes: usize = others.iter().map(|(_, c)| c.len()).sum();
        assert!(
            bytes >= 150_000,
            "生产段语料只有 {bytes} 字节 —— 剥过头了，本条此刻在空转"
        );
        let homes = kou_jing_homes();
        for (label, kind, ident) in &homes {
            let decl = &decl_anchor(kind, ident);
            guard_core::find_pinned(&home, decl).unwrap_or_else(|e| {
                panic!(
                    "口径「{label}」的家不在本文件里（或有两处）：{e}\n\
                     家没了 ⇒ 消费者那侧的 `use` 会编译错；家有两处 ⇒ 本文件自己就开始漂。"
                )
            });
            let dup = second_homes(&others, decl);
            assert!(
                dup.is_empty(),
                "口径「{label}」在本 crate 里有**第二个家**：\n  {}\n\
                 ⇒ 从此两份靠人对齐，而漂开的那一天没有任何东西会红。\n\
                 正解是 `use crate::common::tmux_utf8::…` 引用这里，不是再声明一份。\n\
                 真要新开一个口径 ⇒ 把它加进 `kou_jing_homes()`，家仍然只许在本文件。",
                dup.join("\n  ")
            );
        }
    }

    /// ★★ **正题二：两个消费者只许「引用」，不许自带声明。**
    ///
    /// 与正题一**刻意分开**：那条管「家有几个」，这条管「该引用的人真的在引用」。
    /// 合成一条会让「某一层悄悄不用它了」变成静默 —— 那一层从此不受这个口径管，
    /// 而「家唯一」照样成立。
    ///
    /// ⚠ 两个消费者用 `include_str!` 读（同一个 crate 内，不是跨半边的编译期边）。
    #[test]
    fn both_consumer_layers_reference_the_home_instead_of_declaring_their_own() {
        let files: Vec<(String, String)> = vec![
            (
                "control/gate.rs".to_string(),
                crate::guard_support::production_code(include_str!("../control/gate.rs")),
            ),
            (
                "observe/watcher.rs".to_string(),
                crate::guard_support::production_code(include_str!("../observe/watcher.rs")),
            ),
        ];
        // 人群自检：登记的两个消费者与真读到的两个逐条对上（表改了、读的没改 ⇒ 红）。
        let read: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            read, CONSUMERS,
            "登记的消费者与真读到的对不上 —— 表和读法漂开了，本条在管别的文件"
        );
        for (name, prod) in &files {
            // 非空对照：文件真的读到了、也真的剥出了生产段。
            assert!(
                prod.len() > 2_000,
                "`{name}` 的生产段只有 {} 字节 —— 没读到或剥过头，下面的断言是空转的",
                prod.len()
            );
            assert!(
                prod.contains("crate::common::tmux_utf8::"),
                "`{name}` 不再引用 `common::tmux_utf8` 的家 —— 它要么自带了一份，\
                 要么这个口径在这一层被悄悄弃用了。两种都得有人回答一句。"
            );
            for (label, kind, ident) in &kou_jing_homes() {
                assert!(
                    !prod.contains(&decl_anchor(kind, ident)),
                    "`{name}` 自带了口径「{label}」的声明 —— 那就是第三份靠人对齐的开始。"
                );
            }
        }
    }

    /// ★★ **反向自检：上面那两条**真的会咬人**（喂真会违规的输入给真判据的核）。**
    ///
    /// 没有这一格，「家唯一」就是 `[] == []`：闸死了照样绿。
    /// 〔`brief` 第 9 条 / 本仓「负向断言没有输入就等于没有」已经栽过五次〕
    #[test]
    fn the_one_home_scan_actually_bites() {
        // ⚠ 夹具的文件名取**中性名**，且下面的断言只认**声明本身**（来自内容），不认路径
        //   〔`6g`：断言取自夹具的名字会靠路径恒真〕。
        let homes = kou_jing_homes();
        for (label, kind, ident) in &homes {
            let decl = decl_anchor(kind, ident);
            let planted = vec![("a/b.rs".to_string(), format!("pub {decl}\n"))];
            assert!(
                !second_homes(&planted, &decl).is_empty(),
                "口径「{label}」被人在别处又声明一份，而判据的核没出声 —— 它此刻是空转的"
            );
            // 只是**引用**不算第二个家（否则正解会被判红，而误伤会训练人绕过判据）。
            let referencing = vec![(
                "a/b.rs".to_string(),
                format!("use crate::common::tmux_utf8::{ident};\n    let _ = {ident};\n"),
            )];
            assert!(
                second_homes(&referencing, &decl).is_empty(),
                "口径「{label}」：**引用**被判成了第二个家 —— 那把正解判红了"
            );
            // 🔴 **一个「改了名」的声明不许命中** —— 夹具从**标识符**派生（不是从锚点派生），
            // 所以锚点一变松，这一格就红。实测：锚点末尾少了那个 `:`／`(` 时本格当场失败。
            let renamed = decl_anchor(kind, &format!("{ident}X"));
            let renamed_file = vec![("a/b.rs".to_string(), format!("pub {renamed}\n"))];
            assert!(
                second_homes(&renamed_file, &decl).is_empty(),
                "口径「{label}」：`{renamed}` 这样一个**改了名**的声明也被算成第二个家 —— \
                 锚点的匹配单位比事实小（本仓「被撑大」那一族），\
                 于是「家改了名 / 家没了」这一形从此看不见"
            );
        }
        // 人群不许空：空表会让上面两条等号断言变成「空 == 空」，恒绿。
        assert!(
            !homes.is_empty() && !CONSUMERS.is_empty(),
            "登记表空了 —— 上面两条会空着绿"
        );
    }

    /// 段数下溢谓词本体：**下溢红、恰好绿、过溢绿**。
    ///
    /// 家搬到这里之后，谓词本身的方向也该在家里有一条 —— 两个消费者各自那条量的是
    /// 「它在那个调用点上的处置对不对」，方向这一维不该只长在其中一边。
    #[test]
    fn the_underflow_predicate_only_fires_downward() {
        assert!(tab_underflow("一段而已", 6), "1 < 6 ⇒ 下溢");
        assert!(tab_underflow("a\tb\tc\td\te", 6), "5 < 6 ⇒ 下溢");
        assert!(!tab_underflow("a\tb\tc\td\te\tf", 6), "恰好 6 ⇒ 不红");
        assert!(
            !tab_underflow("a\tb\tc\td\te\tf\tg", 6),
            "7 段是合法内容（路径里有真 TAB）⇒ **不许红**，否则就成了 `!= 6` 那个误伤"
        );
        assert!(tab_underflow("15_/tmp/x/sock", 2), "N=2 那条同理");
        assert!(!tab_underflow("15\t/tmp/x/sock", 2));
        assert!(
            !tab_underflow("$0\t\t1", 3),
            "🔴 中间那段是空串（那个 option 没设）是**合法**的 —— 与「拆不出」是两件事"
        );
    }

    /// ⚠ **两种表示不许被写成同一个值**（它们是两个入口，不是一个别名）。
    ///
    /// 这一条挡的是一种很省事的「收口」：把 env 那一份也写成 `-u`、或把旗写成 `LC_ALL`。
    /// 那样两个调用点形态里必有一个从此静默失效，而口径表看起来还很整齐。
    #[test]
    fn the_two_representations_stay_two_different_things() {
        assert_ne!(
            UTF8_CLIENT_FLAG, UTF8_CLIENT_ENV.1,
            "旗与 env 的值被写成了同一个 —— 其中一处必然已经失效"
        );
        assert!(
            UTF8_CLIENT_FLAG.starts_with('-') && !UTF8_CLIENT_FLAG.contains(' '),
            "旗必须是**单个** argv 词（带空格会被当成一个参数整体传下去）：{UTF8_CLIENT_FLAG:?}"
        );
        assert!(
            !UTF8_CLIENT_ENV.0.is_empty() && !UTF8_CLIENT_ENV.1.is_empty(),
            "env 那一份挂成空串就是**静默**失效（实测：设了但空 ⇒ 判脏）"
        );
        // 值本身要过 tmux 那条子串规矩 —— 不然「设了」与「设对了」是两件事。
        let v = UTF8_CLIENT_ENV.1.to_ascii_uppercase();
        assert!(
            v.contains("UTF-8") || v.contains("UTF8"),
            "env 的值里没有 `UTF-8`/`UTF8` 子串 ⇒ tmux 认它为**非** UTF-8 客户端：{:?}",
            UTF8_CLIENT_ENV.1
        );
    }
}
