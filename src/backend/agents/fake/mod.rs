//! `S6`：**最小假 agent** —— 本区的验收件把 `G1` 成功标准②「加一个新 agent 只需新增
//! `agents/<名>/`，通用层零改动」变成一个**跑得起来、会红**的东西。
//!
//! # ⚠ 先说本件裁掉了什么（`S6#§2` 的第 1 问，B 阶段必须裁）
//!
//! 两条路二选一：
//!
//! - **先立接口再造假 agent** ⇒ `ADAPTER_CALL_SITES` 当场降到个位数，`S6` 就"配叫零改动"；
//! - **先造假 agent、让它反推接口** ⇒ 更合 `D4`，但第一版的"零改动"是**假的**。
//!
//! **裁定：走第二条 —— 本件一处调用点都不收进接口，`ADAPTER_CALL_SITES` 仍是 8 文件 / 27 处。**
//!
//! 三条理由，第一条是决定性的：
//!
//! 1. **验收件不许移动自己的靶子。** `S6` 报的那个数是用来评价 `S1`–`S5` 的。
//!    先立接口的话，`S6` 报的就是**它自己刚做的事**，而 `S1`–`S5` 到底把地基打成什么样
//!    就再也没人量得出来了。`F+` 方向体检刚点过名的风险是「acceptor 全绿而主线为零」，
//!    验收件自己动主线是它的**镜像** —— 主线动了，而没有人在验。
//! 2. **`D2` 逐字排除了这种做法**：「先立判据，让它当场红出一份真实清单，**再逐处搬**」，
//!    并且明写排除「先把文件挪到 `agents/` 目录再说」。那 27 处**就是**那份清单，
//!    一件之内全收 = 一次性大挪动。
//! 3. **`L2` 的钉法逐字**：「`S6` 的最小假 agent —— 它只实现这组接口，**若接口漏了什么，
//!    `S6` 走不通就会红**」。先设计接口、再让假 agent 照着实现，假 agent 就**按构造**
//!    满足接口，那条"会红"永远不可能红。反推的方向被弄反了。
//!
//! **它排除了什么（代价如实写）**：
//!
//! - 排除了「`S6` 交付一个降到个位数的读数」。**本件不会让那个数下降一处**；
//! - 排除了「`S6` 宣布成功标准②成立」。本件的产出是一个**诚实的差距读数**加一副会红的夹具，
//!   不是一张通过证书；
//! - 也放弃了第一条路的真实好处：12 种能力一次收进接口之后，假 agent 会**全程走通**。
//!   那个好处本件拿不到，留给后续（收接口那件）。而它落地的**验收手段**恰恰是本件造出来的。
//!
//! # ⚠ 第 2 问：假 agent 也算一家吗 —— **算半家：进文件树与 `HOMES`，不进 `REGISTRY`**
//!
//! `agents/mod.rs` 里它的模块声明带 `#[cfg(test)]` ⇒ **生产二进制里一个字节都没有它**。
//! 与真加一个 agent 的差别**只有那一行 cfg**（外加它不在 `REGISTRY` 里）。
//! 判据 `the_fixture_agent_never_ships` 双向钉住这两件事；
//! `agent_locality_guard::FIXTURE_HOMES` 登记「谁是夹具家」并**带天花板**——
//! 没有天花板的话，「夹具家」就成了往 `agents/` 里塞东西躲判据①的逃生舱。
//!
//! # 12 种能力（不是 9 —— 本件订正的读数之一）
//!
//! `S6#§0` 与 PM 交底都写「去重之后只是 **9 种能力**」。**实测那 9 种只覆盖 27 处里的 21 处**：
//! 它们漏掉的正是 `control/resolve_query.rs` 的 6 处 —— 也就是 PM 那个 21/27 计数错误的**残留**
//! （`PR-S5 §3.5` 已经点名要求补，件文件的能力清单没跟着补）。resume 那 3 件事
//! （默认命令 · 命令形 · 会话名前缀）**各是一种能力**，与 `会话文件命名` 同一个粒度。
//! ⇒ 反推出来的接口面是 **12 种能力 / 27 处**，逐条对账住
//! `agent_locality_guard::tests::NEW_AGENT_BLOCKERS`。
//!
//! # 这个假 agent 的每一种能力都**刻意与 Claude 不同形**
//!
//! | 能力 | Claude | 本假 agent |
//! |---|---|---|
//! | 会话记录根 | `<home>/projects` | `<home>/convos` |
//! | 会话文件判定 | `.jsonl` | `.ndjson` |
//! | 会话文件命名 | `<sid>.jsonl` | `sess-<sid>.ndjson` |
//! | pidfile 目录 | `<home>/sessions` | `<home>/live` |
//! | 账号环境变量名 | `CLAUDE_CONFIG_DIR` | `CCM_FAKE_ACCOUNT_DIR` |
//! | 账号信任判定 | `.claude.json` → `projects[cwd].hasTrustDialogAccepted` | `fake-config.json` → `trusted[cwd]` |
//! | 判活 cmdline | 含 `claude` / `node` | 含 `fakeagent` |
//! | 解析本机 home | `$CLAUDE_CONFIG_DIR` 否则 `$HOME/.claude`（**恒有值**） | `$CCM_FAKE_AGENT_HOME`，**没有默认** |
//! | resume 默认命令 | `claude` | `fakeagent` |
//! | resume 命令形 | `<base> --resume <sid>`（flag） | `<base> revive <sid>`（子命令） |
//! | resume 会话名前缀 | `cc` | `fk` |
//!
//! ⚠ **"刻意不同"是本件最要紧的一条设计决定**，不是趣味。假 agent 若照抄 Claude 的布局，
//! 通用层拿 Claude 的知识去解释它的 home **恰好也能读出东西** —— 那时"走通全流程"证明的是
//! 「Claude 的布局被施加到了另一个目录上」，不是「通用层能容纳第二种布局」。
//! 那正是一个**粉饰的通过**。
//!
//! # 走全流程的两半，**边界写在明处**
//!
//! [`walk`] 把假 agent 推过 7 段。**前两段真的过通用层的机器**（`agents::visible_among`
//! 的发现判准 · `wire::Frame::Hello` 的序列化），**后五段没有** ——
//! 它们是假 agent 拿自己的知识**自问自答**，因为通用层今天**没有任何接口**收第二种布局。
//! 这不是本件偷懒，这**就是本件要报的那个差距**：
//!
//! - 前两段 = 今天已经成立的那部分成功标准②；
//! - 后五段 = 还差的 27 处；`walk` 走得通只证明「**这 12 种能力足以走完全流程**」
//!   （`L2` 那组接口的形状够不够用），**不证明通用层能用它们**。

use std::path::{Path, PathBuf};

/// 本假 agent 在 wire 上的 `agent_kind` 值。
///
/// ⚠ 它**永远不会**出现在真 `hello.homes` 里 —— 本层不进 `agents::REGISTRY`
/// （`the_fixture_agent_never_ships` 钉住）。
pub(crate) const AGENT_KIND: &str = "fake";

/// 解析本机 home 的环境变量。⚠ **刻意没有默认值**（Claude 那家恒有值）——
/// 少了这个差别，`visible_among` 里 `None` 那条分支就永远不会被这家走一遍。
const HOME_ENV: &str = "CCM_FAKE_AGENT_HOME";

/// 能力 8：解析本机 home。`None` = 连候选路径都说不出。
pub(crate) fn home() -> Option<PathBuf> {
    std::env::var_os(HOME_ENV).map(PathBuf::from)
}

/// 能力 1：会话记录根。**`convos` 不是 `projects`**。
pub(crate) fn records_root(home: &Path) -> PathBuf {
    home.join("convos")
}

/// 能力 2：这个路径是不是一份会话记录。**`.ndjson` 不是 `.jsonl`**。
pub(crate) fn is_session_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "ndjson")
        && p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("sess-"))
}

/// 能力 3：会话 id → 记录文件名。**带前缀**，与 Claude 的裸 `<sid>.<ext>` 不同形。
pub(crate) fn session_file_name(sid: &str) -> String {
    format!("sess-{sid}.ndjson")
}

/// 能力 4：pidfile 目录。**`live` 不是 `sessions`**。
pub(crate) fn pidfile_root(home: &Path) -> PathBuf {
    home.join("live")
}

/// 能力 5：账号（配置根）环境变量名。
pub(crate) const ACCOUNT_DIR_ENV: &str = "CCM_FAKE_ACCOUNT_DIR";

/// 能力 6：某个配置根下、对某个 cwd 的信任判定。
///
/// 形状与 Claude 那家**不同**：文件名不同、字段路径不同（`trusted[cwd]` 而不是
/// `projects[cwd].hasTrustDialogAccepted`）。
pub(crate) fn trust_of_config(root: &Path, cwd: &str) -> Result<bool, String> {
    let p = root.join("fake-config.json");
    let bytes = std::fs::read(&p).map_err(|e| format!("fake_config_unreadable: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("fake_config_invalid: {e}"))?;
    Ok(v.get("trusted")
        .and_then(|t| t.get(cwd))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
}

/// 能力 7：判活时"这个 cmdline 看起来像不像本 agent"。
///
/// ⚠ 与 Claude 那家**方向相同但词表不同**（那边是「明显不像才判冒名」，含 `claude`/`node`）。
pub(crate) fn cmdline_may_be_agent(lower: &str) -> bool {
    lower.trim().is_empty() || lower.contains("fakeagent")
}

// 〔`设计/50` 删用量〕**原「能力 9：用量聚合」整条去掉了。**
// 它模拟的是「通用层有没有地方收这个能力」，而通用层那一处（`observe/usage_query.rs`）
// 随用量 ② 轴整轴退役 ⇒ **这一格没有对面了**：留着它会让 `agent_locality_guard` 的
// 「能力数 == 卡点数」两个方向漂开（那条判据逐字：「少一种 = 要么那种能力真的收进接口了…」）。
// ⚠ 编号**刻意不重排**（下面仍是「能力 10/11/12」）：编号是给人对照上面那张表用的住址，
// 重排会让所有引用过它的散文一起变成假话。

/// 能力 10：无 `launchCandidate` 时的默认命令基底。
pub(crate) const DEFAULT_COMMAND: &str = "fakeagent";

/// 能力 11：resume 命令 —— **子命令形，且动词与 Codex 也不同**。
pub(crate) fn resume_command(base: &str, session_id: &str) -> String {
    format!("{base} revive {session_id}")
}

/// 能力 12：resume 会话名前缀（Claude `cc` / Codex `cx`）。
pub(crate) const SESSION_NAME_PREFIX: &str = "fk";

/// 能力 13〔RM1b · 第四波〕：一个插件市场的落点 → 它**声明插件的那份清单**在哪。
///
/// 反推自 `observe/plugins_query.rs`（插件市场只读枚举搬进后端，远端也要答得出）：
/// Claude 那家是 `<落点>/.claude-plugin/marketplace.json`，本层**文件名与层级都不同**。
pub(crate) fn marketplace_manifest(install_location: &Path) -> PathBuf {
    install_location.join("catalog.fake.json")
}

// ─────────────────────────────────────────────────────────────────────────────
// 能力表 + 走全流程的 driver
// ─────────────────────────────────────────────────────────────────────────────

/// 12 种能力的**名字**，与 `agent_locality_guard::tests::NEW_AGENT_BLOCKERS` 逐条对账。
///
/// ⚠ 顺序即 [`FakeCaps`] 字段序，[`FakeCaps::without`] 按名字挖洞时靠它。
pub(crate) const CAPABILITIES: &[&str] = &[
    "会话记录根",
    "会话文件判定",
    "会话文件命名",
    "pidfile 目录",
    "账号环境变量名",
    "账号信任判定",
    "判活 cmdline",
    "解析本机 home",
    "resume 默认命令",
    "resume 命令形",
    "resume 会话名前缀",
    // 〔RM1b · 第四波〕能力 13：插件市场的清单住哪（`observe/plugins_query.rs` 反推出来的第 13 种）。
    "插件市场清单",
];

/// 假 agent 交出来的一整套能力。
///
/// ⚠ **每一项都是 `Option`，这是本结构存在的全部理由**：`S6` 的反向夹具要「把某一种能力
/// **删掉**」，而删掉之后流程必须在一个**说得出话**的地方停下来。用 `Option` 而不是真去
/// 注释掉一个函数，是为了让那个反向夹具成为**常驻判据**而不是一次性的手工变异
/// —— 手工变异证明的是"那天它会红"，常驻判据证明的是"以后它一直会红"。
#[derive(Clone)]
pub(crate) struct FakeCaps {
    pub(crate) records_root: Option<fn(&Path) -> PathBuf>,
    pub(crate) is_session_file: Option<fn(&Path) -> bool>,
    pub(crate) session_file_name: Option<fn(&str) -> String>,
    pub(crate) pidfile_root: Option<fn(&Path) -> PathBuf>,
    pub(crate) account_dir_env: Option<&'static str>,
    pub(crate) trust_of_config: Option<fn(&Path, &str) -> Result<bool, String>>,
    pub(crate) cmdline_may_be_agent: Option<fn(&str) -> bool>,
    pub(crate) home: Option<fn() -> Option<PathBuf>>,
    pub(crate) default_command: Option<&'static str>,
    pub(crate) resume_command: Option<fn(&str, &str) -> String>,
    pub(crate) session_name_prefix: Option<&'static str>,
    pub(crate) marketplace_manifest: Option<fn(&Path) -> PathBuf>,
}

impl FakeCaps {
    /// 还剩几种能力在（[`without`](Self::without) 的自检用）。
    pub(crate) fn present(&self) -> usize {
        [
            self.records_root.is_some(),
            self.is_session_file.is_some(),
            self.session_file_name.is_some(),
            self.pidfile_root.is_some(),
            self.account_dir_env.is_some(),
            self.trust_of_config.is_some(),
            self.cmdline_may_be_agent.is_some(),
            self.home.is_some(),
            self.default_command.is_some(),
            self.resume_command.is_some(),
            self.session_name_prefix.is_some(),
            self.marketplace_manifest.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count()
    }

    /// 完整的一套（正题用）。
    pub(crate) fn full() -> Self {
        Self {
            records_root: Some(records_root),
            is_session_file: Some(is_session_file),
            session_file_name: Some(session_file_name),
            pidfile_root: Some(pidfile_root),
            account_dir_env: Some(ACCOUNT_DIR_ENV),
            trust_of_config: Some(trust_of_config),
            cmdline_may_be_agent: Some(cmdline_may_be_agent),
            home: Some(home),
            default_command: Some(DEFAULT_COMMAND),
            resume_command: Some(resume_command),
            session_name_prefix: Some(SESSION_NAME_PREFIX),
            marketplace_manifest: Some(marketplace_manifest),
        }
    }

    /// 挖掉**一种**能力（反向夹具用）。名字必须是 [`CAPABILITIES`] 里的一条 —— 写错就 panic，
    /// 不静默返回原样（静默的话反向夹具会**变成正题**再跑一遍，而它会绿）。
    pub(crate) fn without(&self, capability: &str) -> Self {
        assert!(
            CAPABILITIES.contains(&capability),
            "`{capability}` 不是一种登记过的能力 —— 反向夹具挖了个不存在的洞，\
             这一格此刻在把正题当反向跑（而正题是绿的）。已登记的：{CAPABILITIES:?}"
        );
        let mut c = self.clone();
        match capability {
            "会话记录根" => c.records_root = None,
            "会话文件判定" => c.is_session_file = None,
            "会话文件命名" => c.session_file_name = None,
            "pidfile 目录" => c.pidfile_root = None,
            "账号环境变量名" => c.account_dir_env = None,
            "账号信任判定" => c.trust_of_config = None,
            "判活 cmdline" => c.cmdline_may_be_agent = None,
            "解析本机 home" => c.home = None,
            "resume 默认命令" => c.default_command = None,
            "resume 命令形" => c.resume_command = None,
            "resume 会话名前缀" => c.session_name_prefix = None,
            "插件市场清单" => c.marketplace_manifest = None,
            other => unreachable!("`{other}` 在 CAPABILITIES 里却没有对应字段 —— 两处漂开了"),
        }
        c
    }
}

/// 全流程的 7 段。名字进错误信息，所以是常量而不是字面量。
pub(crate) const STAGES: &[&str] = &[
    "发现",
    "宣告（hello.homes）",
    "读会话",
    "判活",
    "账号",
    "resume",
    // 〔RM1b · 第四波〕第 7 段：插件市场（能力 13）。
    "插件市场",
];

/// 流程停下来的原因 —— **两种都必须说得出话**。
///
/// ⚠ 这个枚举本身就是 `S6` 的正题：件里逐字要求「不许静默当成"这个 agent 没有会话"」。
/// 而通用层今天对同一情形的反应恰恰是**静默**（用量聚合那条查询在一个布局不同的 home 上
/// rc=0、零输出），实测读数见 `PR-S6.md`。两者的差别就是 `L2` 那组接口要补上的东西。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    /// 假 agent 自己少了一种能力 —— 点名是**哪一段**缺了**哪一种**。
    MissingCapability {
        stage: &'static str,
        capability: &'static str,
    },
}

/// 把假 agent 推过全流程。成功 = 走完的段名。
///
/// # ⚠ 边界（本函数最容易被读错的一句，写在这里而不是只写在计划里）
///
/// 前两段（`发现` / `宣告`）**真的调用通用层的机器**：
/// [`crate::agents::visible_among`] 的判准与 [`crate::wire::Frame::Hello`] 的序列化。
/// 后四段（`读会话`/`判活`/`账号`/`resume`）**没有过通用层** ——
/// 通用层今天在这五段上直呼 `crate::agents::claudecode::…`（27 处里的 22 处），
/// 没有任何入口收第二种布局。⇒ 这五段是假 agent 拿自己的知识自问自答。
///
/// 因此本函数走得通，**只证明这 12 种能力凑得出一条完整的路**（`D4`：接口面够不够用），
/// **不证明**通用层能用它们。后者今天不成立，差距逐条登记在
/// `agent_locality_guard::tests::NEW_AGENT_BLOCKERS`（27 处）。
/// 〔SH1 · V137〕假 agent 的 MCP 读面（夹具家那一份布局）：只认 `<项目>/.fake-mcp.json` 的 `servers` 表，一律记成 project 段。
/// 它要证的是「通用层经注册表那一格读 MCP、不认识任何一家的文件名」—— 判据 `fake_tests.rs::the_fake_agents_mcp_face_is_read_through_the_generic_layer`。
pub(crate) const MCP: crate::agents::McpFace = crate::agents::McpFace { read: read_mcp };

fn read_mcp(project_dir: Option<&Path>) -> crate::agents::McpRead {
    let mut out = crate::agents::McpRead::default();
    let Some(dir) = project_dir else { return out };
    let file = dir.join(".fake-mcp.json");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return out;
    };
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(v) => {
            for (name, server) in v
                .get("servers")
                .and_then(|s| s.as_object())
                .into_iter()
                .flatten()
            {
                out.entries.push(crate::agents::McpEntry {
                    scope: "project",
                    name: name.clone(),
                    server: server.clone(),
                    source: file.display().to_string(),
                });
            }
        }
        Err(e) => out
            .problems
            .push(format!("读 {} 失败：{e}", file.display())),
    }
    out
}

pub(crate) fn walk(caps: &FakeCaps, fixture_home: &Path) -> Result<Vec<&'static str>, Stop> {
    let mut done: Vec<&'static str> = Vec::new();

    // ── ① 发现：**真的走通用层**（注册表 × 判准）。
    let home_fn = caps.home.ok_or(Stop::MissingCapability {
        stage: STAGES[0],
        capability: "解析本机 home",
    })?;
    // `Adapter.home` 是裸函数指针，所以"加一个 agent"在这一层真的只是多一条记录。
    let adapter = crate::agents::Adapter {
        kind: AGENT_KIND,
        // 最小假 agent 没有账号维度 —— 它要证的是「通用层零改动」，不是账号。
        account_env: None,
        // 〔AS2〕最小假 agent 没有资产面（它要证的是「通用层零改动」，不是资产）。
        assets: None,
        history: None,
        // 〔NT2 · V25〕最小假 agent 没有默认上游（未登记 ⇒ 上游选择拒）。
        upstream: None,
        // 〔SH1 · V137〕MCP 读面同拍长一格：假 agent 的布局是 `<项目>/.fake-mcp.json` 的 `servers` 表（见 [`MCP`]）。
        mcp: Some(MCP),
        home: home_fn,
    };
    let discovered = crate::agents::visible_among(std::slice::from_ref(&adapter));
    if discovered.len() != 1 {
        return Err(Stop::MissingCapability {
            stage: STAGES[0],
            capability: "解析本机 home",
        });
    }
    done.push(STAGES[0]);

    // ── ② 宣告：**真的走通用层**（塞进 `Hello.homes` 并序列化成一行）。
    let line = crate::wire::to_line(&crate::wire::Frame::Hello {
        v: 1,
        build_id: "s6".into(),
        host_arch: "x86_64".into(),
        claude_dir: "/c".into(),
        homes: discovered.clone(),
        capabilities: vec![],
        emits: vec![],
        commands: vec![],
        // `K-P4`（09-04）：握手帧第四条面。这一格与 `S6` 无关（第三家 agent 不带命令），
        // 空表 ⇒ 省略 ⇒ 下面那串期望字节一个都没动。
        unavailable: vec![],
        host_env: Default::default(),
        uncancellable: vec![],
    })
    .map_err(|_| Stop::MissingCapability {
        stage: STAGES[1],
        capability: "解析本机 home",
    })?;
    if !line.contains(&format!("\"agent_kind\":\"{AGENT_KIND}\"")) {
        return Err(Stop::MissingCapability {
            stage: STAGES[1],
            capability: "解析本机 home",
        });
    }
    done.push(STAGES[1]);

    // ── ③ 读会话：⚠ 以下各段**没有过通用层**（见头注边界）。
    let records_root = caps.records_root.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话记录根",
    })?;
    let is_session = caps.is_session_file.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话文件判定",
    })?;
    let file_name = caps.session_file_name.ok_or(Stop::MissingCapability {
        stage: STAGES[2],
        capability: "会话文件命名",
    })?;
    let mut found: Vec<PathBuf> = Vec::new();
    if let Ok(projects) = std::fs::read_dir(records_root(fixture_home)) {
        let mut dirs: Vec<PathBuf> = projects.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for d in dirs {
            if let Ok(files) = std::fs::read_dir(&d) {
                let mut ps: Vec<PathBuf> = files.flatten().map(|e| e.path()).collect();
                ps.sort();
                found.extend(ps.into_iter().filter(|p| p.is_file() && is_session(p)));
            }
        }
    }
    if found.is_empty() {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "会话记录根",
        });
    }
    // 命名口径与判定口径必须互洽：拿 sid 算出来的名字得等于扫出来的那个文件名。
    let sid = FIXTURE_SESSION_ID;
    if !found
        .iter()
        .any(|p| p.file_name().and_then(|n| n.to_str()) == Some(file_name(sid).as_str()))
    {
        return Err(Stop::MissingCapability {
            stage: STAGES[2],
            capability: "会话文件命名",
        });
    }
    done.push(STAGES[2]);

    // ── ④ 判活：pidfile 目录 + cmdline 判定。
    let pid_root = caps.pidfile_root.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "pidfile 目录",
    })?;
    let cmdline = caps.cmdline_may_be_agent.ok_or(Stop::MissingCapability {
        stage: STAGES[3],
        capability: "判活 cmdline",
    })?;
    let pidfiles = std::fs::read_dir(pid_root(fixture_home))
        .map(|rd| rd.flatten().count())
        .unwrap_or(0);
    if pidfiles == 0 || !cmdline("fakeagent revive x") || cmdline("/usr/bin/vim") {
        return Err(Stop::MissingCapability {
            stage: STAGES[3],
            capability: "判活 cmdline",
        });
    }
    done.push(STAGES[3]);

    // ── ⑤ 账号：环境变量名 + 信任判定。
    let env_name = caps.account_dir_env.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "账号环境变量名",
    })?;
    let trust = caps.trust_of_config.ok_or(Stop::MissingCapability {
        stage: STAGES[4],
        capability: "账号信任判定",
    })?;
    if env_name.trim().is_empty() || trust(fixture_home, FIXTURE_CWD) != Ok(true) {
        return Err(Stop::MissingCapability {
            stage: STAGES[4],
            capability: "账号信任判定",
        });
    }
    done.push(STAGES[4]);

    // ── ⑥ resume：默认命令 + 命令形 + 会话名前缀。
    let base = caps.default_command.ok_or(Stop::MissingCapability {
        stage: STAGES[5],
        capability: "resume 默认命令",
    })?;
    let cmd = caps.resume_command.ok_or(Stop::MissingCapability {
        stage: STAGES[5],
        capability: "resume 命令形",
    })?;
    let prefix = caps.session_name_prefix.ok_or(Stop::MissingCapability {
        stage: STAGES[5],
        capability: "resume 会话名前缀",
    })?;
    if base.trim().is_empty() || prefix.trim().is_empty() || !cmd(base, sid).contains(sid) {
        return Err(Stop::MissingCapability {
            stage: STAGES[5],
            capability: "resume 命令形",
        });
    }
    done.push(STAGES[5]);

    // ── ⑦〔RM1b〕插件市场：清单住哪（能力 13）。夹具在 `<home>/market/` 下放了一份本层形状的清单。
    let manifest = caps.marketplace_manifest.ok_or(Stop::MissingCapability {
        stage: STAGES[6],
        capability: "插件市场清单",
    })?;
    if !manifest(&fixture_home.join(FIXTURE_MARKET_DIR)).is_file() {
        return Err(Stop::MissingCapability {
            stage: STAGES[6],
            capability: "插件市场清单",
        });
    }
    done.push(STAGES[6]);

    Ok(done)
}

/// 夹具里那条会话的 id。**只出现在夹具里**，不是任何真实会话。
pub(crate) const FIXTURE_SESSION_ID: &str = "00000000-0000-4000-8000-0000000000f6";
/// 夹具里那条会话的 cwd。
pub(crate) const FIXTURE_CWD: &str = "/home/u/proj";
/// 〔RM1b〕夹具里那个插件市场的落点（相对 home）。
pub(crate) const FIXTURE_MARKET_DIR: &str = "market";

#[cfg(test)]
#[path = "../../../../tests/backend/agents/fake_tests.rs"]
mod tests;
