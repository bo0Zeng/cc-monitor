//! 命令表 · 会话输入队：`session-queue-*`。

use crate::wire::spec::CommandSpec;

pub(super) const SPECS: &[CommandSpec] = &[
    // 队住会话所在那台（`history/session_queue.rs`，`~/.cc-monitor/session-queue/<sid>.json`），序号由那台发：
    //   入参带 `machine`，本机 ⇒ 就地读改写（阻塞线程上）；别台 ⇒ 经 `--resident-call` 交给那台的常驻（`faces/session_queue_hop.rs`），
    //   够不到 ⇒ `offline`、本机不写 ⇒ 异步档。
    crate::wire::typed::asynchronous::<crate::faces::session_queue_face::SessionQueuePutCmd>(
        "往一条会话投一句（排进那条会话的队；同 id 只入队一次，同一端按端序号保序）",
    ),
    crate::wire::typed::asynchronous::<crate::faces::session_queue_face::SessionQueueCancelCmd>(
        "撤队里一条还没送的",
    ),
    crate::wire::typed::asynchronous::<crate::faces::session_queue_face::SessionQueueReadCmd>(
        "一条会话此刻的队列名单",
    ),
    // 编辑已发的一句、发送那一下的选项：只读，问会话所在那台（同上，经输入队那一跳）⇒ 异步档。
    crate::wire::typed::asynchronous::<crate::faces::session_rewind_face::SessionRewindOptionsCmd>(
        "编辑已发的一句时，发送那一下给哪几项（怎么退 · 什么时候退）",
    ),
];
