//! 计划（planned-build，下称 pb）的读面：这台机器上的 pb 工作区、每片的图与状态，加工成界面直接排版的成品。
//!
//! # 规则只住 pb
//!
//! 本族一个 pb 的规矩都不写：盘上的 `图.md` · `排期.md` 一份都不读，编号怎么拆、哪个值算签收、
//! 阶段表长什么样全由 `pb dump`（只读 · 不要身份 · 不写盘 · 带形状版本号）答。本族只做三件「换个方向排」的事：
//!
//! | 做什么 | 住哪 |
//! |---|---|
//! | 找 pb · 起 pb（本族唯一一处）· 认 `pb dump` 的退出码与形状版本 · 代敲人的命令 | [`locate`] · [`pb`] · [`dump`] · [`command`] |
//! | 摘掉 `agent_view`（点到那一格再给）· 把边与文件倒过来索引 · 把负责人与 signer 的 id 对到会话 | [`product`] |
//! | 每个工作区留上一次读好的那一份（某片读不成 ⇒ 那一片给旧的 ＋ 原因 ＋ 时刻）· 输出摘要 | [`book`] |
//! | 盯计划仓 · 工作区 `.env` · 各片仓库里声明过的文件，输出摘要或要你看的数变了才推一帧 `changed {plan}` | [`watch`] |
//! | 要你看的四种（顶层全签收 · 校验不通过 · 负责人的会话停了 · 负责人的会话在等你）· 退回落没落地 · 送进会话的那一行 | [`needs`] |
//! | 文件窗口反查：一个目录落在哪一片的仓库里、每份文件归哪一格（`plan-files`）| [`owners`] |
//! | 线上形状（界面的类型由它经 ts-rs 生成；成品从 product 起就是这些类型）| [`wire`] |
//! | 认可与退回的记录（后端自己的 `~/.cc-monitor/plan-review.json`）· 照它给成品标认可与退回的状态 | [`review`] |
//!
//! # 计划仓一个字节都不写
//!
//! 计划仓（`.planned-build/`）与工作区 `.env` 只有 pb 与人写；本族只起 `pb dump` 这一条只读子命令（不带 `PB_ID`，pb 认作人），
//! 读面的状态全在进程内存里。判据 `tests/backend/plan/` 里那条「跑前跑后工作区逐字节不变」钉着。
//! 认可与退回只记在后端自己家里（[`review`]，写经 `Own`），不回写 pb。

pub(crate) mod book;
pub(crate) mod command;
pub(crate) mod dump;
pub(crate) mod groups;
pub(crate) mod locate;
pub(crate) mod needs;
pub(crate) mod new_slice;
pub(crate) mod owners;
pub(crate) mod product;
pub(crate) mod refs;
pub(crate) mod review;
pub(crate) mod watch;
pub(crate) mod wire;

/// 起 pb 的一条命令最多等多久。量过：252 格的研究盘 `dump` 1.1 秒，`render` 同量级；留一个数量级给慢盘与远端。
pub(crate) const PB_DEADLINE_SECS: u64 = 20;

/// 起一次 pb，本族唯一一处：找 python（裸名，在 `PATH` 上按这台的规矩找）· 当前目录设在 `ws`（pb 自己从那儿往上找工作区）·
/// 环境只给白名单（不带 `PB_ID` ⇒ pb 认作人）· 限时 [`PB_DEADLINE_SECS`]。
/// 跑完了（含过了期限被收掉：`waited_secs` 有数）⇒ `Done`，结局由调用方各自判；找不到 python · 起不来 ⇒ 一句话 ＋ 原话。
pub(crate) fn pb(
    entry: &std::path::Path,
    ws: &std::path::Path,
    argv: &[&str],
) -> Result<crate::plugin::invoke::Done, copy_core::said::Said> {
    pb_fed(entry, ws, argv, None)
}

/// 同 [`pb`]（起法就这一处），另把 `stdin` 喂进 pb 的标准输入（`dump --refs` 读一段话）；`None` ⇒ 空设备。
pub(crate) fn pb_fed(
    entry: &std::path::Path,
    ws: &std::path::Path,
    argv: &[&str],
    stdin: Option<&[u8]>,
) -> Result<crate::plugin::invoke::Done, copy_core::said::Said> {
    use copy_core::copy_text;
    let py = crate::plugin::discover::find(
        "python3",
        &[],
        true,
        &copy_text("bePlan.dump.noPythonHint", &[]),
    )?;
    let entry_s = entry.to_string_lossy().to_string();
    let all: Vec<&str> = std::iter::once(entry_s.as_str())
        .chain(argv.iter().copied())
        .collect();
    crate::plugin::invoke::run_fed(&py, &all, PB_DEADLINE_SECS, &[], Some(ws), stdin).map_err(|e| {
        let raw = match e {
            crate::plugin::invoke::NotRun::ArgListTooLong => None,
            crate::plugin::invoke::NotRun::Failed(s) => s.raw.or(Some(s.said)),
        };
        copy_core::said::Said {
            said: copy_text("bePlan.dump.notRun", &[]),
            raw,
        }
    })
}

/// 一个 id（权力范围的负责人 · 节点的 signer）对到的会话：主会话 ＝ 它自己那个标签页；子 agent ＝ 父会话那个标签页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Whose {
    /// 是一个会话自己的 id。
    Session {
        sid: String,
        /// 此刻活着就有：在跑 · 等你 · 闲着（说不清 ⇒ 活着但 `None`）。
        live: Option<Live>,
    },
    /// 是一个子 agent 的 id：挂在 `parent` 那个会话底下；`parent_title` ＝ 那个会话叫什么（与标签页同一条规则，后端写好）。
    Subagent {
        parent: String,
        parent_title: String,
        live: Option<Live>,
    },
    /// 对不上这台的任何会话记录。
    Unknown,
}

/// 一个活会话此刻在干什么（与主窗口标签页同一判）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Live {
    /// `working` · `needs_you` · `idle`；说不清 ⇒ `None`。
    pub(crate) activity: Option<crate::agents::SessionActivity>,
    /// 在等你时等的是哪一类（批准 · 回答 · 计划 · 说不清）。
    pub(crate) needs: Option<crate::observe::recognize::facts::NeedsKind>,
}

/// 加工一份 dump 时问「这个 id 是谁」的口：生产由帧面宿主拼（会话表 ＋ 记录树），判据喂一张表。
pub(crate) type WhoPort<'a> = &'a dyn Fn(&str) -> Whose;

/// 计划读面判据共用的夹具（合成 dump · 假 pb 插件）。
#[cfg(test)]
#[path = "../../../tests/backend/plan/fixture.rs"]
pub(crate) mod fixture;
