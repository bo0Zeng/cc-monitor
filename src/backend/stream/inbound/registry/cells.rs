//! 命令表 · 成品的格：`cells-catalog`（每件成品有哪些格、各是值还是写好的字）。

use crate::stream::inbound::spec::{out, CommandSpec, Run};

pub(super) const SPECS: &[CommandSpec] = &[
    // **格目录**：出口据它写自己的声明（要哪几格），不读核心代码就知道有没有那一格。本体 `faces/cells_catalog.rs`：
    //   从每件成品的 Rust 类型序列化出来，不手写第二份。纯计算 ⇒ 不进阻塞档（同 `history-search-merge`）。
    CommandSpec {
        name: "cells-catalog",
        summary: "每件成品有哪些格",
        codes: &[],
        fields: &[
            out("pending", "已判了要补、还没落地的格：`product` · `path` · `kind`（落地那一刻从这里挪进 `products`）"),
            out("products", "每件成品一项：`name`（记录 `record` · 会话事实 `facts` · 需手动清单的一行 `needs_row` · 骨架行 `index_row` · 行摘要 `read_row`（`history-read.rows[]`）· 会话帧按帧的 `kind`）· `frozen`（两个前端照它读的成品面：格只许加，不删不改名不换类型）· `cells`：每格 `path`（`a.b` 嵌套 · `a[]` 列表每项 · `a.*` 以 id 为键的表每项 · `a[t=x]` 列表里按判别格挑的那一种 · `a{t=x}` 非列表的那一种；每一种都有的格写在挑法外面）· `kind`（`value` 值 · `text` 核心写好的字 · `tone` 语气）· `type`（`string` · `number` · `bool` · `enum` 闭集的词 · `object` 原样透传的一团）"),
        ],
        takes_input: false,
        run: Run::Async(|_r| Box::pin(async move { Ok(Some(crate::faces::cells_catalog::catalog())) })),
    },
];
