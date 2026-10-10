//! 计划（planned-build，下称 pb）的读面：这台机器上的 pb 工作区、每片的图与状态，加工成界面直接排版的成品。
//!
//! # 规则只住 pb
//!
//! 本族一个 pb 的规矩都不写：盘上的 `图.md` · `排期.md` · `签收.md` 一份都不读，编号怎么拆、做完怎么算、
//! 阶段表长什么样全由 `pb dump`（只读 · 不要身份 · 不写盘 · 带形状版本号）答。本族只做三件「换个方向排」的事：
//!
//! | 做什么 | 住哪 |
//! |---|---|
//! | 找 pb、跑 `pb dump`、认退出码与形状版本 | [`locate`] · [`dump`] |
//! | 摘掉 `agent_view`（点到那一格再给）· 把边与文件倒过来索引 · 把接手 / 签收人的 id 对到会话 | [`product`] |
//! | 每个工作区留上一次读好的那一份（某片读不成 ⇒ 那一片给旧的 ＋ 原因 ＋ 时刻）· 输出摘要 | [`book`] |
//! | 盯计划仓与工作区 `.env`，输出摘要或要你看的数变了才推一帧 `plan_changed` | [`watch`] |
//! | 要你看的四种（顶块走到看全局 · 判据红 · 接手的会话停了 · 接手的会话在等你）· 退回落没落地 · 送进会话的那一行 | [`needs`] |
//! | 文件窗口反查：一个目录落在哪一片的仓库里、每份文件归哪一格（`plan-files`）| [`owners`] |
//! | 线上形状（界面的类型由它经 ts-rs 生成；帧面出口过一遍）| [`wire`] |
//! | 认可与退回的记录（后端自己的 `~/.cc-monitor/plan-review.json`）· 照它给成品标认可与退回的状态 | [`review`] |
//!
//! # 计划仓一个字节都不写
//!
//! 计划仓（`.planned-build/`）与工作区 `.env` 只有 pb 与人写；本族只起 `pb dump` 这一条只读子命令（不带 `PB_ID`，pb 认作人），
//! 读面的状态全在进程内存里。判据 `tests/backend/plan/` 里那条「跑前跑后工作区逐字节不变」钉着。
//! 认可与退回只记在后端自己家里（[`review`]，第四层逐份登记），不回写 pb。

pub(crate) mod book;
pub(crate) mod command;
pub(crate) mod dump;
pub(crate) mod locate;
pub(crate) mod needs;
pub(crate) mod owners;
pub(crate) mod product;
pub(crate) mod review;
pub(crate) mod watch;
pub(crate) mod wire;

/// 一个 id（块的接手 · 签收的「由」）对到的会话：主会话 ＝ 它自己那个标签页；子 agent ＝ 父会话那个标签页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Whose {
    /// 是一个会话自己的 id。
    Session {
        sid: String,
        /// 此刻活着就有：在跑 · 等你 · 闲着（说不清 ⇒ 活着但 `None`）。
        live: Option<Live>,
    },
    /// 是一个子 agent 的 id：挂在 `parent` 那个会话底下。
    Subagent { parent: String, live: Option<Live> },
    /// 对不上这台的任何会话记录。
    Unknown,
}

/// 一个活会话此刻在干什么（与主窗口标签页同一判）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Live {
    /// `working` · `needs_you` · `idle`；说不清 ⇒ `None`。
    pub(crate) activity: Option<crate::agents::SessionActivity>,
    /// 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
    pub(crate) needs: Option<crate::observe::facts_query::NeedsKind>,
}

/// 加工一份 dump 时问「这个 id 是谁」的口：生产由帧面宿主拼（会话表 ＋ 记录树），判据喂一张表。
pub(crate) type WhoPort<'a> = &'a dyn Fn(&str) -> Whose;

/// 计划读面判据共用的夹具（合成 dump · 假 pb 插件）。
#[cfg(test)]
#[path = "../../../tests/backend/plan/fixture.rs"]
pub(crate) mod fixture;
