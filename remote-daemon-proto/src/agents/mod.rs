//! `S2`（2026-08-14）：**agent 适配层** —— 每个 agent 一份，装它**专属**的知识。
//!
//! # 它为什么存在
//!
//! 〔用 08-14〕逐字：「**现在的daemon几乎都是兼容claudecode, 那就标清楚, 分离清楚,
//! 后面搞兼容其他agent的时候才方便**」。
//!
//! 病灶不是"代码散"，是**「加一个 agent 要改哪几处」这份清单只住在人的脑子里**。
//! `S2` 开工前实测：codex 的知识在三处，而三处**长得完全不一样** ——
//! `usage_query.rs` 里是 `aggregate(claude_dir).and_then(|()| aggregate_codex())`，
//! `resolve_query.rs` 里是 `spec.agent_kind.trim() == "codex"`，
//! grep 出其中一处**找不到另一处**。接第三个 agent 的人得先把它们找出来，而没有东西会告诉他有几处。
//!
//! # 分界：**格式知识**进来，**值判别**留在通用层
//!
//! `D3` 逐字：「agent 维度只许出现在**值**里，不许出现在**字段名**里」。照它推：
//!
//! | 归这里 | 留通用层 |
//! |---|---|
//! | 会话文件长什么样（信封 / 事件名 / 用量字段在哪一层） | `agent_kind == "codex"` 这种**值**上的派发 |
//! | 会话住哪个目录、文件怎么命名 | 派发之后两边**共用**的形状（校验、错误出口、CommandPlan 骨架） |
//! | 起会话/resume 的**命令形状**与默认命令名 | |
//!
//! 两条判据钉住这个分界，都住 [`crate::agent_locality_guard`]：
//! ① 专有的格式针**只许**在 `agents/<名>/` 下出现；
//! ② 通用层里的 kind 派发点**逐个登记**（今天恰好 1 处）—— 那份登记表**就是**上面说的那份清单。
//!
//! # ⚠ 两个 agent 都在了，但**故意还没有 trait**
//!
//! `D4` 逐字要求「接口由**现有能力反推**，不凭空设计」。`S3` 把 Claude 那半也搬进来之后，
//! 两个实现终于摆在一起了 —— 而**它们的形状差得比预想大**：
//! `codex/` 是 `parse`+`usage`+`resume` 三块（自带一整套记录抽取器），
//! `claudecode/` 是 `paths`+`records`+`liveness`+`accounts`+`resume` 五块（记录抽取住在通用机器里）。
//! 唯一严格对称的只有 `resume`。⇒ 现在就立 trait 会得到一个"两边都别扭"的抽象。
//! 立接口的动作留给 `L2`，由 `S6`（最小假 agent）**反过来**逼出真正需要的那几个方法。
//!
//! ⚠ 另一半如实说〔`S4b` 08-14 订正〕：那个 `claude_dir` **参数名已经清了**
//!（8 个文件；生产段 `claude_dir` 64 行 → 3 行，剩下的 3 处全是冻结的 wire 字段名；
//! `agent_locality_guard` 判据③钉住不许长回来）。
//! **但通用层仍然叫得出 agent 的名字** —— 换了个形状：8 个文件 / 27 处
//! `agents::<名>::…` 的**调用点**（`agent_locality_guard::ADAPTER_CALL_SITES` 逐条登记）。
//! 「知识收进适配层」与「通用层不再叫得出 agent 名字」是两件事，本层只做到了第一件；
//! 第二件卡在**还没有接口**（`L2`），归 `S6`。
//!
//! # 〔`S5` 08-14〕**注册表**：本文件从"目录索引"变成了"这台机器认得哪几个 agent"
//!
//! [`REGISTRY`] + [`visible_homes`] 让 daemon **有能力**声明它看得见哪些 agent
//! （`G1` 成功标准③）。⚠ **能力先建、生产路径今天不接** —— `main.rs` 仍硬写
//! `homes: Vec::new()`，一个线上字节都没变。理由不是保守：填 `homes` 是一次**跨仓契约变更**
//! （仓外 aterm 的 hello fixture 按精确字节对），而本机没有 aterm 仓、验不了它的运行时
//! （前提 `P3`）。⇒ 把"何时真填"留成**一次纯发布决策**：改 `main.rs` 那一行，
//! `wire::tests::production_hello_leaves_homes_empty_so_claude_bytes_stay_frozen`
//! 当场变红，逼那一天的人重新裁一次。**那条判据是提醒，不是障碍，别删它。**
//!
//! ## 为什么注册表住这里，而不是通用层的某个新文件
//!
//! 「加一个 agent 要改哪几处」这份清单本来就**必然**包含本文件 ——
//! 下面那两行 `pub(crate) mod …` 不加，新的适配层根本编不进来。
//! 把注册表放在别处只会让必改的文件从 1 个变成 2 个。
//! ⚠ 代价如实写，以及它**为什么不能**记进 `ADAPTER_CALL_SITES`：
//! 那张表里每一条的含义是「该被压到零的耦合」，而注册表这几行**方向相反** ——
//! 它该随 agent 数增长。混进去，`S6` 就没法拿那个数当成绩。
//! ⇒ 拆成 `agent_locality_guard::AGENT_REGISTRY_SITES` 单独一张；
//! 扣出人群的**对价**是判据⑦把本文件的处数钉死成 `REGISTRY.len()`（**一家一行**），
//! 谁想把别处的直呼挪进来刷数，当场红。
//! （同轮还补了判据④的针：此前只认全路径 `agents::<名>::`，本文件写的相对路径 `codex::`
//! 一处都数不到 —— 那是本区第二次「量具的作用域比事实**小**」。）

use crate::wire::AgentHome;
use std::path::{Path, PathBuf};

pub(crate) mod claudecode;
pub(crate) mod codex;

/// 〔`S6`〕**夹具家** —— 本区验收件的最小假 agent。
///
/// ⚠ **那行 `#[cfg(test)]` 就是它与一个真 agent 的全部差别**（外加它不进 [`REGISTRY`]）：
/// 生产二进制里一个字节都没有它，真 `hello.homes` 永远不会声明它。
/// 两件事都由 `fake::tests::the_fixture_agent_never_ships` 双向钉住，
/// 登记住 `agent_locality_guard::tests::FIXTURE_HOMES`（**带天花板**——
/// 没有天花板的话「夹具家」就成了往 `agents/` 里塞东西躲判据①的逃生舱）。
#[cfg(test)]
pub(crate) mod fake;

/// 一个 agent 适配层在注册表里的样子〔`S5`〕。
///
/// ⚠ **刻意不是 trait**（`D4` 逐字「接口由现有能力反推，不凭空设计」；`S4b §2` 复述过一遍）。
/// 今天两家能对称答出来的只有"我叫什么"与"我的 home 在哪"这两问 ——
/// 那就先把这两问定成数据，剩下的等 `S6` 用最小假 agent **反过来**逼。
/// 用函数指针而不是方法，正是为了让这一步**不需要**先决定 trait 长什么样。
pub(crate) struct Adapter {
    /// wire 上的 `agent_kind` 值。**由适配层自己提供** —— 通用层里一个 agent 名的
    /// 字面量都不该有（`D3`）。
    pub(crate) kind: &'static str,
    /// 这个 agent 在本机的 home 目录候选。`None` = 连候选都说不出（⇒ 一定看不见）。
    ///
    /// ⚠ 它**只答"该在哪"**。"在不在"由 [`home_is_visible`] 统一判 ——
    /// 让每家自己定判准的话，「看得见一个 agent」就成了两套语义，
    /// 而 `S6` 的最小假 agent 得先猜自己该伪造哪一套。
    pub(crate) home: fn() -> Option<PathBuf>,
}

/// **这个 daemon 认得哪几个 agent**〔`S5`〕。加一个 agent = 加一行（+ 上面加一行 `mod`）。
///
/// ⚠ 它与 `agent_locality_guard::tests::HOMES`（判据用的"agent 家"清单）**必须一样长**，
/// 由 `every_agent_adapter_has_exactly_one_registry_entry` 双向钉住：
/// 建了 `agents/<名>/` 却不登记 ⇒ 那家永远"看不见"，而没有任何东西会说。
/// ⚠ **一家一行**（不是每字段一行）：`ADAPTER_CALL_SITES` 数的是**行**，
/// 而这张表要回答的是「加一个 agent 要回来改**几处**」——
/// 一家拆成四行会让那个读数变成排版的函数。
#[rustfmt::skip]
pub(crate) const REGISTRY: &[Adapter] = &[
    Adapter { kind: claudecode::AGENT_KIND, home: claudecode::home },
    Adapter { kind: codex::AGENT_KIND,      home: codex::home },
];

/// **这台机器上看得见哪些 agent** —— 直接产出 `hello.homes` 的那张表〔`S5`，`G1` 成功标准③〕。
///
/// # 判准是「**home 目录存在**」，三选一，理由写在这里
///
/// 候选三条语义不同：home 目录存在（**装了**）· 目录下有会话记录（**用过**）·
/// 可执行文件在 `PATH` 里（**能起**）。取第一条：
///
/// 1. **字段语义只允许这一条**。`homes` 的每一项是 `{agent_kind, path}` —— 一个**路径**。
///    按 `PATH` 判的话，claude 那家 [`claudecode::home`] 恒 `Some`（有默认值），
///    我们就得为一个**不存在的目录**报一个路径 —— 那是在说谎，消费方拿它去列会话只会得到空。
/// 2. **它与 daemon 真正能干的事对齐**。四类能力里的三类（会话发现与判活 · 会话内容读 ·
///    用量）**全部** root 在 home 之下；home 不在，这三类一律零输出。
///    ⇒「home 在」就是「daemon 对这个 agent 的观测面能干活」。
///    ⚠ 第四类（起会话/resume）确实靠 `PATH`，本判准**覆盖不到** —— 如实登记，见 `§4`。
/// 3. **排除「目录下有会话记录」**：那是"用过"不是"装了"。三条具体害处 ——
///    ① 刚装好还没开过会话的 agent 会被判成看不见，而 daemon 的 watcher 正是要
///    inotify 那个目录**等第一条会话出现**（自相矛盾）；② `hello` 一次连接只发一次，
///    而"有没有会话记录"在连接期内会变（清理/轮转）—— 会中途变旧的值不该进握手帧；
///    ③ 它要扫盘，而 hello 是 flush 前的**第一件事**，给握手加一次全树扫描是加了一段不确定的延迟。
/// 4. **排除「可执行文件在 `PATH` 里」**：① 它答不出 `path` 该填什么（见 1）；
///    ② daemon 自己的 `PATH` 与用户在 tmux 里的 `PATH` 常常不是一回事（非交互 SSH 登录）
///    —— 拿 daemon 的 `PATH` 判"用户能不能起 claude"是**问错了人群**；
///    ③ 它答的是"能不能起"，而 `homes` 声明的是"数据在哪"。
///
/// ⇒ 对 `S6` 的含义（判准决定假 agent 怎么伪造自己）：**`mkdir` 一个 home 目录就够了**。
/// 这是三条里最容易伪造的一条，而这正合适 —— `S6` 要证的是"通用层零改动"，
/// 不是"假 agent 起得来"。
///
/// # ⚠ 生产路径今天**不调它**
///
/// 见本模块头注：`main.rs` 仍硬写 `homes: Vec::new()`。摘掉下面这个 `allow` 的那天，
/// 就是把那一行换成 `agents::visible_homes()` 的那天。
#[allow(dead_code)] // `S5`：能填不真填 —— 接线是一次纯发布决策，不是忘了。
pub(crate) fn visible_homes() -> Vec<AgentHome> {
    visible_among(REGISTRY)
}

/// [`visible_homes`] 的**可喂夹具**那一半：注册表进、看得见的那些出，**顺序保持注册序**。
///
/// # 为什么要多这一层（而不是让 [`visible_homes`] 直接读 `REGISTRY`）
///
/// 真 home 由**环境变量**解析（`CLAUDE_CONFIG_DIR` / `HOME` / `CODEX_HOME`），
/// 而在测试里改进程环境既不可靠（并行跑的别的测试也在读）又不安全。
/// 传注册表进来之后，判据可以喂一张**合成注册表**（`Adapter.home` 是 `fn` 指针 ⇒
/// 非捕获闭包/普通 fn 就够），于是**整条链**（`None` 候选 · 缺席的 home · 同名文件 ·
/// 顺序）全部可以在夹具里验，不依赖跑测试这台机器上装了什么。
///
/// ⚠ 它**仍然真的碰文件系统** —— 判准就是文件系统事实，把那一步也抽成参数就只剩一个
/// 恒真的壳，那种"能力"验的是它自己。
///
/// ★ `S6` 的入口就是这里：最小假 agent = 一条 `Adapter` + 一个 `mkdir` 出来的 home。
#[allow(dead_code)] // 同上：唯一的生产调用点是 `visible_homes`，而它今天不接线。
pub(crate) fn visible_among(registry: &[Adapter]) -> Vec<AgentHome> {
    registry
        .iter()
        // `None` = 这家连候选路径都说不出（如 codex 在没有 `HOME`/`CODEX_HOME` 的环境里）
        // ⇒ 直接出局，不要拿一个空路径去 stat。
        .filter_map(|a| (a.home)().map(|home| (a.kind, home)))
        .filter(|(_, home)| home_is_visible(home))
        .map(|(kind, home)| AgentHome {
            agent_kind: kind.to_string(),
            path: home.to_string_lossy().into_owned(),
        })
        .collect()
}

/// 判准本身：**是一个存在的目录**。
///
/// ⚠ `is_dir()` 而不是 `exists()`：同名的**文件**不是一个 agent 的家。
/// 这条区分不是洁癖 —— 报出去的 `path` 会被消费方当目录去拼子路径。
#[allow(dead_code)] // 同上。
fn home_is_visible(home: &Path) -> bool {
    home.is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成夹具的根。**非捕获 fn**（不是闭包）—— [`Adapter::home`] 是裸函数指针，
    /// 而这正是 `S5` 选函数指针的一个副产物：假 agent 不需要任何运行时装配。
    fn fixture_root() -> PathBuf {
        std::env::temp_dir().join(format!("ccm-s5-registry-{}", std::process::id()))
    }

    /// 一个**存在**的 home（测试会先把它 `mkdir` 出来）。
    fn synth_home_present() -> Option<PathBuf> {
        Some(fixture_root().join("present-home"))
    }
    /// 一个**不存在**的 home —— 判准该把它挡掉。
    fn synth_home_absent() -> Option<PathBuf> {
        Some(fixture_root().join("absent-home"))
    }
    /// 一个同名的**文件** —— 存在，但不是目录，同样该被挡掉。
    fn synth_home_is_a_file() -> Option<PathBuf> {
        Some(fixture_root().join("a-file"))
    }
    /// **说不出候选路径**的那家（codex 在没有 `HOME`/`CODEX_HOME` 的环境里就是这样）。
    fn synth_home_unknown() -> Option<PathBuf> {
        None
    }

    #[rustfmt::skip]
    const SYNTH_REGISTRY: &[Adapter] = &[
        Adapter { kind: "alpha",   home: synth_home_present },
        Adapter { kind: "ghost",   home: synth_home_absent },
        Adapter { kind: "nameless", home: synth_home_unknown },
        Adapter { kind: "filey",   home: synth_home_is_a_file },
    ];

    /// `S5-Y1`：**看得见 = home 目录存在**。整条链喂合成注册表，一次验四种形态。
    ///
    /// ⚠ 三个阴性项（`ghost` 缺席 · `nameless` 说不出路径 · `filey` 是文件）是本条的
    /// 全部意义所在：只喂存在的那家、只断言"返回非空"的话，判准坏成「永远全放行」照样绿 ——
    /// 而那种坏法正是本件最怕的：daemon 会声明它其实**看不见**的 agent，
    /// 消费方照着那个 path 去列会话，只会得到空/报错。
    ///
    /// ⚠ 本条**完全不依赖跑测试这台机器上装了什么** —— 注册表是合成的，
    /// home 路径由 pid 隔开。真机那半由
    /// `wire::tests::the_daemon_can_already_discover_homes_it_just_does_not_send_them` 兜。
    #[test]
    fn only_agents_whose_home_directory_exists_are_visible() {
        let root = fixture_root();
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("present-home")).expect("建 home");
        std::fs::write(root.join("a-file"), b"not a home").expect("建文件");

        let got = visible_among(SYNTH_REGISTRY);
        let seen: Vec<(&str, &str)> = got
            .iter()
            .map(|h| (h.agent_kind.as_str(), h.path.as_str()))
            .collect();
        let want_path = synth_home_present().expect("夹具").to_string_lossy().into_owned();
        assert_eq!(
            seen,
            vec![("alpha", want_path.as_str())],
            "判准是「home 目录**存在**」，四种形态各验一次：\n\
             · alpha（目录在）——进；\n\
             · ghost（目录不在）——一项都不许产，否则 daemon 会声明它其实看不见的 agent；\n\
             · nameless（连候选路径都说不出）——出局，别拿空路径去 stat；\n\
             · filey（同名的**文件**）——不算家，报出去的 path 会被消费方当目录拼子路径。"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// `S5-Y1` 的另一半：**生产注册表**这条路与合成那条是同一条 —— 别只有夹具是活的。
    ///
    /// ⚠ 它是**恒等式**（`visible_homes()` ≡ `visible_among(REGISTRY)`），
    /// 所以在一台**一个 agent home 都没有**的机器上它是空转的（两边都空）——
    /// 如实登记在功能件 `§4`。它逮的是「有人把 `visible_homes` 的body 掏空/改成读别的表」。
    #[test]
    fn the_public_entry_point_is_the_registry_run_through_the_same_criterion() {
        assert_eq!(
            visible_homes(),
            visible_among(REGISTRY),
            "`visible_homes()` 不再是「注册表 × 判准」了 —— \
             生产入口与被判据喂过夹具的那条路已经分叉"
        );
    }

    /// `S5-Y3`：注册表每家一条、kind 非空且互不相同。
    ///
    /// ⚠ 重复的 kind 是**静默**坏法：`homes` 里出现两条同 `agent_kind`，
    /// 消费方（`claude_home_from_hello` 那类 `.find(kind == …)`）只会拿到第一条，
    /// 另一条永远无人消费 —— 编译过、判据全绿、行为错。
    #[test]
    fn the_registry_names_every_agent_exactly_once() {
        assert!(
            REGISTRY.len() >= 2,
            "注册表只剩 {} 家 —— 有人把某个适配层从表里删了（或抽取坏了）",
            REGISTRY.len()
        );
        let mut kinds: Vec<&str> = REGISTRY.iter().map(|a| a.kind).collect();
        assert!(
            kinds.iter().all(|k| !k.trim().is_empty()),
            "有 agent 的 kind 是空的 ⇒ 它在 wire 上没有身份：{kinds:?}"
        );
        let before = kinds.len();
        kinds.sort_unstable();
        kinds.dedup();
        assert_eq!(
            kinds.len(),
            before,
            "注册表里有**重复的 agent_kind**：{kinds:?}\n\
             ⇒ `homes` 会出现两条同 kind 的项，而消费侧按 kind 取第一条 —— \
             第二条永远无人消费，且没有任何东西会报错。"
        );
    }

    /// `S5-Y3` 的另一半：注册表答得出的 home **就是**各适配层自己那条解析路。
    ///
    /// ⚠ 本条**不断言具体路径**（那取决于跑测试这台机器的 `HOME`/`CLAUDE_CONFIG_DIR`）——
    /// 它断言的是"注册表接的是那根管子"：函数指针指错了家（两条都接 claude），
    /// 上面两条都还会绿，而 daemon 会把同一个目录报成两个 agent 的家。
    #[test]
    fn each_registry_entry_asks_its_own_adapter_for_the_home() {
        let claude = REGISTRY
            .iter()
            .find(|a| a.kind == claudecode::AGENT_KIND)
            .expect("注册表里没有 claude 那家");
        assert_eq!(
            (claude.home)(),
            claudecode::home(),
            "claude 那条的 home 指针没接到 `claudecode::home`"
        );
        let cx = REGISTRY
            .iter()
            .find(|a| a.kind == codex::AGENT_KIND)
            .expect("注册表里没有 codex 那家");
        assert_eq!(
            (cx.home)(),
            codex::home(),
            "codex 那条的 home 指针没接到 `codex::home`"
        );
        assert_ne!(
            claudecode::AGENT_KIND,
            codex::AGENT_KIND,
            "两家的 kind 撞了"
        );
    }
}
