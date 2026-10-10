//! 命令表 · 计划：读面 `plan-list` · `plan-read` · `plan-cell-view`（planned-build 的工作区 · 片 · 一格的 agent 视角；只读）；
//! 反查 `plan-files`（文件窗口：目录里每份文件归哪一格）；代敲 `plan-command`（以人的身份跑 pb 的 `continue` · `pause` · `view`；写盘的是 pb）；
//! 审面 `plan-ack` · `plan-unack` · `plan-return`（认可与退回记在后端自己的 `~/.cc-monitor/plan-review.json`，计划仓一个字节不写）。

use crate::stream::inbound::spec::{arg, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // 三条都在阻塞档：起一次 pb（研究盘 252 格 1.1 秒），开跑之后打不断 ⇒ `cancel` 命中时回 `not_cancellable`。
    CommandSpec {
        name: "plan-list",
        summary: "这台的 pb 工作区与片（目录 ＝ 活会话的工作目录 ∪ `dirs`，pb 自己往上找工作区）",
        codes: &["bad_args"],
        fields: &[arg("dirs", "可选：另要问的目录（串的数组）"), arg("fresh", "可选布尔：`true` ⇒ 认过的目录也重问"), out("pb", "`{state: ok|missing|unsupported, said, version}`：pb 装没装、认不认得它的输出"), out("workspaces", "每个工作区一格 `{workspace, repo, auto, rev, stale, needCount, bySession, slices: [{name, domain, current, progress, needCount, error, stale}]}`")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args, &r.tz).map(Some)),
    },
    CommandSpec {
        name: "plan-read",
        summary: "一个工作区的成品（几片的图 · 状态 · 签收 · 块 · 判据；接手与签收人对到会话；不带 agent_view）",
        codes: &["bad_args", "failed", "no_pb", "not_workspace", "pb_unsupported"],
        fields: &[arg("workspace", "工作区根（`plan-list` 给的那个）"), out("rev", "这一份输出的摘要：变了才算计划变了"), out("readAt", "读到的时刻（epoch ms）"), out("needCount", "这个工作区要你看的数（没认可的，不含 agent 问人那一种）"), out("bySession", "会话 ⇒ 它接手的那一块 `{slice, block, cell, title, top, phase, at, atTitle, via: session|subagent}`（子 agent 接的记在父会话名下；自己接的优先）"), out("slices", "每片一格：读不成 ⇒ `error`；这一刻读不成但读好过 ⇒ 上一次那一份 ＋ `stale {said, since}`；`needs: [{key, kind: top|red|ended|ask, block, cell, sid, acked}]` · `needCount`；每格 `returned`：退回过 ⇒ `{at, to, state: returned|unsure|landed, by: child|body, child}`，没有 ⇒ `null`；顶块（`project`）那一次记在片上的 `returned`"), out("stale", "整次读不成、给的是上一次那一份 ⇒ `{said, raw, since}`；否则 `null`")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args, &r.tz).map(Some)),
    },
    CommandSpec {
        name: "plan-cell-view",
        summary: "一格的 agent 视角（agent 站在这一格时 pb 印给它的那一段，原样）",
        codes: &["bad_args", "no_view"],
        fields: &[arg("id", "格的编号"), arg("slice", "片名"), out("view", "原样那一段"), arg("workspace", "工作区根")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args, &r.tz).map(Some)),
    },
    // 代敲也在阻塞档：起一次 pb（`view` 要画整张图）。写盘的是 pb（`.env` 的 auto · 系统临时目录里那一页），本族不写。
    CommandSpec {
        name: "plan-command",
        summary: "以人的身份代敲 pb 的用户命令：`continue` · `pause`（开关自动接着做，管整个工作区）· `view`（pb 画整张图写进系统临时目录，回页面路径）",
        codes: &["bad_args", "failed", "no_pb", "not_workspace", "pb_unsupported", "refused"],
        fields: &[arg("cmd", "`continue` · `pause` · `view`"), arg("workspace", "工作区根"), out("rc", "pb 的退出码（成了才回，恒 0）"), out("said", "pb 说的第一句"), out("path", "`view` 写的那一页（那台机器上的路径）；别的 ⇒ `null`")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args, &r.tz).map(Some)),
    },
    // 反查只读本子里那一份（不起 pb），照样放阻塞档（读认可那份小文件）。
    CommandSpec {
        name: "plan-files",
        summary: "文件窗口反查：这个目录落在哪一片的仓库里、每份文件归哪一格（只看读好过的工作区，不起 pb）",
        codes: &["bad_args", "failed"],
        fields: &[arg("dir", "那台机器上的绝对路径（串）"), out("workspace", "工作区根；不在任何工作区 ⇒ `null`"), out("slice", "片名；不在任何一片的仓库（`<工作区>/<片名>/`）里 ⇒ `null`"), out("unreadable", "那一片此刻读不成的那一句（条目空）"), out("entries", "每份声明过的文件 `{name, id, title, statusCode, status, block, signAtText, fileState, fileNote, dup: [{id, title}]}`（被几格声明 ⇒ 先声明的作主、其余进 `dup`）"), out("unowned", "无主的那几份（要 pb 给；没给 ⇒ `null`）")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_face::answer(&r.cmd, &r.args, &r.tz).map(Some)),
    },
    // 审面三条也在阻塞档：认可读—改—写后端自己那份小文件（跨进程锁）；退回现读一次计划（起 pb）再走 `terminal-input` 的本体（起 tmux）。
    CommandSpec {
        name: "plan-ack",
        summary: "认可一条要你看（只记在 cc-monitor；键带条目版本，版本换了那一条再出）；推一帧 `plan_changed` 带新的数",
        codes: &["bad_args", "io_failed", "no_such_need", "not_ackable", "not_read", "review_unreadable"],
        fields: &[arg("key", "那一条的键（`plan-read` 里 `needs[].key`）"), arg("slice", "片名"), arg("workspace", "工作区根"), out("acked", "`true`"), out("key", "原样"), out("needCount", "这个工作区此刻要你看的数")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_review_face::answer(&r.cmd, &r.args).map(Some)),
    },
    CommandSpec {
        name: "plan-unack",
        summary: "撤掉一条认可（没有也不算错）；推一帧 `plan_changed` 带新的数",
        codes: &["bad_args", "io_failed", "not_read", "review_unreadable"],
        fields: &[arg("key", "那一条的键"), arg("slice", "片名"), arg("workspace", "工作区根"), out("acked", "`false`"), out("key", "原样"), out("needCount", "这个工作区此刻要你看的数")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_review_face::answer(&r.cmd, &r.args).map(Some)),
    },
    CommandSpec {
        name: "plan-return",
        summary: "把人的话送给负责那一格的会话：后端拼「人 · {编号} {标题}：{原话}」，在等你 ⇒ 拒，已结束 / 认不出 ⇒ 只给复制，能送走 `terminal-input`；送到了记一条已退回",
        codes: &["bad_args", "bad_target", "capture_failed", "child_timed_out", "failed", "io_failed", "no_pb", "no_server", "no_such_cell", "no_such_session", "no_target", "no_tmux", "not_workspace", "pb_unsupported", "review_unreadable", "unobservable"],
        fields: &[arg("client", "自报的前端（过 `terminal-input` 那一道身份门）"), arg("id", "格的编号；顶块 ⇒ `project`（标题是片名，送给顶块接手）"), out("line", "送出的那一行（后端拼；只读）"), out("result", "`delivered` · `unsure`（送达未知，别重发）· `refused` · `copy`（送不了，只给这一行去复制）"), out("said", "`refused` / `copy` 时给人看的那一句"), out("screen", "`screen-changed` 时带的新指纹"), arg("seen_screen", "送之前看到的那一屏的指纹（同 `terminal-input`）"), arg("slice", "片名"), arg("text", "人的原话（换行并成空格）"), arg("to", "可选：`owner`（在长它的，缺省）· `signer`（签它的，pb 给了才有）"), out("to", "送给的那一位（同 `plan-read` 里 `owner` 的形状）"), out("why", "`waiting`（在等你批准或回答）· `ended` · `unknown` · 或 `terminal-input` 的原因"), arg("workspace", "工作区根")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::plan_return_face::answer(&r.cmd, &r.args).map(Some)),
    },
];
