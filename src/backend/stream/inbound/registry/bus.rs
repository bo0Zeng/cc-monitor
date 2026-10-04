//! 命令表 · cc-bus 总线：`bus-*`。

use crate::stream::inbound::spec::{CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // P4f：cc-bus 的两条基础命令。**转调本机的 cc-bus 命令**，不在后端里重实现总线
    //（用户 08-13 逐字：「细节先按原本的就行」「后面我可能要改ccbus」）。
    // ⚠ 刻意**没有** `bus-recv`：`cc-recv` 会推进已读位置，backend 代读等于把消息从人那里
    //   偷走。「有没有新的」由 `bus-list` 的待读数回答（只读、不消费）。理由全文在 `control/cc_bus.rs`。
    CommandSpec {
        name: "bus-list",
        doc_anchor: Some("#### `bus-list`"),
        codes: &["not_installed", "timed_out", "failed"],
        fields: &["agents", "ccm_sid", "id", "live", "target", "unread"],
        takes_input: false,
        run: Run::Blocking(|_r| crate::control::cc_bus::list_for_inbound().map(Some)),
    },
    // 广播：列名单（同 `bus-list` 那一个函数）→ 挑在线的 → 逐个投递（同 `bus-send` 那一处起进程）。
    //   原是 monitor 里的组合；界面改经通道直接说后端（`src/frontend/ui/cc-bus-control.ts`），组合收进这一侧（业务解释只有一个家）。
    //   起子进程并等它们退出 ⇒ 阻塞档，同下面几条。部分投递失败**不整条回错**（成品里逐个列），
    //   只有「一条都还没发」的那一步（列名单）失败才回码。
    CommandSpec {
        name: "bus-broadcast",
        doc_anchor: Some("#### `bus-broadcast`"),
        // `bad_id`：给的 `from` 形状过不了 `shell_quote_core::bus_id_ok`（交给 `cc-send` 之前先判，一个人都没发）。
        codes: &[
            "invalid_args",
            "not_installed",
            "timed_out",
            "failed",
            "bad_id",
        ],
        fields: &[
            "detail",
            "error",
            "failed",
            "from",
            "id",
            "liveness_unknown",
            "sent",
            "skipped_offline",
            "text",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::broadcast_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "bus-kill",
        doc_anchor: Some("#### `bus-kill`"),
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &["id", "killed", "stale_only"],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::kill_for_inbound(&r.args).map(Some)),
    },
    CommandSpec {
        name: "bus-send",
        doc_anchor: Some("#### `bus-send`"),
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
        fields: &["from", "live", "registered", "sent", "to"],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::send_for_inbound(&r.args).map(Some)),
    },
    // 派生一个协作 agent（转调 `cc-spawn`）。起子进程并等它退出 ⇒ 阻塞档，
    // 同上面那三条。**会起一个真 agent 进程（烧额度）** —— 这一跳不重试由调用方负责，
    // 超时那一档的说法里明写「可能已经起来了」（`control::cc_bus::classify_spawn`）。
    CommandSpec {
        name: "bus-spawn",
        doc_anchor: Some("#### `bus-spawn`"),
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &["id", "said", "spawned"],
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
        doc_anchor: Some("#### `bus-state`"),
        codes: &["not_installed", "timed_out", "failed"],
        // 多了 `registered_at` · `spawned_at` · `skipped`（cc-bus 的 `--tsv` 形答）。
        fields: &[
            "agents",
            "ccm_sid",
            "dir",
            "id",
            "live",
            "registered_at",
            "skipped",
            "spawned",
            "spawned_at",
            "target",
            "task",
            "unread",
        ],
        takes_input: false,
        run: Run::Blocking(|_r| crate::control::cc_bus::state_for_inbound().map(Some)),
    },
    // 驾驶舱读收件箱：转调 `cc-log`（只读，不推已读位置 —— 不是 `bus-recv`）。阻塞档。
    CommandSpec {
        name: "bus-inbox",
        doc_anchor: Some("#### `bus-inbox`"),
        codes: &[
            "invalid_args",
            "bad_id",
            "not_installed",
            "timed_out",
            "failed",
        ],
        fields: &[
            "class",
            "from",
            "id",
            "lines",
            "messages",
            "skipped",
            "text",
            "truncated",
            "ts",
        ],
        takes_input: true,
        run: Run::Blocking(|r| crate::control::cc_bus::inbox_for_inbound(&r.args).map(Some)),
    },
];
