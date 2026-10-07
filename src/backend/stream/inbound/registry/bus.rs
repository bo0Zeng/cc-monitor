//! 命令表 · cc-bus 总线：`bus-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // P4f：cc-bus 的两条基础命令。**转调本机的 cc-bus 命令**，不在后端里重实现总线
    //（用户 08-13 逐字：「细节先按原本的就行」「后面我可能要改ccbus」）。
    // ⚠ 刻意**没有** `bus-recv`：`cc-recv` 会推进已读位置，backend 代读等于把消息从人那里
    //   偷走。「有没有新的」由 `bus-list` 的待读数回答（只读、不消费）。理由全文在 `control/cc_bus.rs`。
    CommandSpec {
        name: "bus-list",
        summary: "谁在线 + 各自待读多少",
        codes: &["not_installed", "timed_out", "failed"],
        fields: &[out("agents", "在线成员，每项 `{id, target, unread, live, ccm_sid}`"), out("ccm_sid", "那个会话绑的会话 id，没绑 ⇒ `null`"), both("id", "总线身份"), out("live", "这个地址**今天还在不在**"), out("target", "tmux 地址"), out("unread", "待读条数")],
        takes_input: false,
        run: Run::Blocking(|_r| crate::control::cc_bus::list_for_inbound().map(Some)),
    },
    // 广播：列名单（同 `bus-list` 那一个函数）→ 挑在线的 → 逐个投递（同 `bus-send` 那一处起进程）。
    //   原是 monitor 里的组合；界面改经通道直接说后端（`src/frontend/ui/cc-bus-control.ts`），组合收进这一侧（业务解释只有一个家）。
    //   起子进程并等它们退出 ⇒ 阻塞档，同下面几条。部分投递失败**不整条回错**（成品里逐个列），
    //   只有「一条都还没发」的那一步（列名单）失败才回码。
    CommandSpec {
        name: "bus-broadcast",
        summary: "给总线上在线的成员群发一条",
        // `bad_id`：给的 `from` 形状过不了 `shell_quote_core::bus_id_ok`（交给 `cc-send` 之前先判，一个人都没发）。
        codes: &[
            "invalid_args",
            "not_installed",
            "timed_out",
            "failed",
            "bad_id",
        ],
        fields: &[out("detail", "失败那一项的原话"), out("error", "失败那一项的码（`bus-send` 那一套）"), out("failed", "逐个列， `{id, error, detail}`，`error` 是 `bus-send` 那一套码；键刻意不叫 `code` / `message` —— 那一对是整条失败的错误信封"), arg("from", "**可选**，同 `bus-send`：以谁的身份发，也用来「不发给自己」"), both("id", "失败那一项的收件人"), out("liveness_unknown", "问不到身份空间（全是 `null`），退回发给所有登记的"), out("sent", "投出去几条"), out("skipped_offline", "因不在线跳过几个"), arg("text", "正文（非空）")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::broadcast_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "bus-kill",
        summary: "收掉一个总线成员",
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &[both("id", "总线身份"), out("killed", "会话真的被杀了"), out("stale_only", "身份对不上：只摘掉那条陈旧登记，会话与收件箱都没动")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::kill_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "bus-send",
        summary: "发一条消息",
        codes: &[
            "invalid_args",
            // 收件人的形状在交给 `cc-send` 之前就过不了（`INVARIANTS §47` ①）。
            "bad_id",
            "not_installed",
            "rejected",
            "timed_out",
            "too_long",
            "failed",
        ],
        fields: &[arg("from", "**可选**"), out("live", "那个会话今天活着吗，与 `bus-list` 同一套三态"), out("registered", "在总线名单里吗"), out("sent", "投出去了"), both("to", "收件人身份")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::send_for_inbound(&r.args).map(Some)),
    },
    // 派生一个协作 agent（转调 `cc-spawn`）。起子进程并等它退出 ⇒ 阻塞档，
    // 同上面那三条。**会起一个真 agent 进程（烧额度）** —— 这一跳不重试由调用方负责，
    // 超时那一档的说法里明写「可能已经起来了」（`control::cc_bus::classify_spawn`）。
    CommandSpec {
        name: "bus-spawn",
        summary: "派生一个协作 agent",
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &[both("id", "新会话的总线身份；从 `cc-spawn` 的回显里认，**认不出就是 `null`** —— 那是「起了，但名字没认出来」，**不是**「没起来」"), out("said", "`cc-spawn` 的原始回显，给人看"), out("spawned", "恒 `true`")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::spawn_for_inbound(&r.args).map(Some)),
    },
    // `K-R113`（09-13）：**一次回全的具名读命令。**
    //
    // 它不是 `bus-list` 的超集写法 —— `agents` 那一半**就是** `bus-list` 那一半
    //（同一个 `agents_via_cc_list`，由 `cc_bus::tests::bus_state_answers_both_halves_from_one_call`
    // 钉住），多出来的是 spawn 台账。⚠ 两半必须在**同一条命令**里回：总线名单与 spawn 台账
    // 互相引用，分两条命令取回来的两份是两个时刻的，拼出来的状态盘上从没存在过。
    CommandSpec {
        name: "bus-state",
        summary: "总线名单 ＋ spawn 台账一次回全",
        codes: &["not_installed", "timed_out", "failed"],
        // 多了 `registered_at` · `spawned_at` · `skipped`（cc-bus 的 `--tsv` 形答）。
        fields: &[out("agents", "名册，每项 `{id, target, registered_at, unread, live, ccm_sid}`"), out("ccm_sid", "后两格与 `bus-list` 同一套：对身份空间对账，「登记 ≠ 在线」"), out("dir", "派生时的目录"), both("id", "总线身份"), out("live", "三态：`true` / `false` / `null` = 核不了，**不是**「不在」"), out("registered_at", "登记时间，cc-bus 原样"), out("skipped", "两张表里读不懂的行数"), out("spawned", "`cc-spawn` 派生过的会话，每项 `{id, dir, spawned_at, task, live}`"), out("spawned_at", "派生时间"), out("target", "登记的 pane 地址"), out("task", "余下全部，含 TAB"), out("unread", "待读条数")],
        takes_input: false,
        run: Run::Blocking(|_r| crate::control::cc_bus::state_for_inbound().map(Some)),
    },
    // 驾驶舱读收件箱：转调 `cc-log`（只读，不推已读位置 —— 不是 `bus-recv`）。阻塞档。
    CommandSpec {
        name: "bus-inbox",
        summary: "只读看一个 agent 收件箱的尾巴",
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &[out("class", "消息类别（cc-bus 原样）"), out("from", "发件人"), both("id", "必给；交给 `cc-log` 之前先过 `bus_id_ok`，不过 ⇒ `bad_id`、一个进程都不起"), arg("lines", "可缺席，1..=2000，缺省 200"), out("messages", "逐行解析，只取 `from` · `ts` · `text` · `class`；`from` 与 `text` 都空的行不算消息"), out("skipped", "读不懂的行数"), out("text", "正文"), out("truncated", "回显超过 4 MiB ⇒ 保尾，`true`"), out("ts", "时间")],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::inbox_for_inbound(&r.args).map(Some)),
    },
];
