//! 历史清单与注解 ＋ 会话清单那一族（项目 / 会话清单 · 叫会话什么 · 判活；observe 反过来调这里，这里不引 observe）。

pub mod history_annotations; // 历史注解（星标 / 改名 / 隐藏 / 上次账号）：帧面 `history-annotate` / `history-forget` / `history-last-accounts`（自家状态，经 `Own`；文件就是 monitor 从前那一份，路径由它交）
pub(crate) mod history_browse; // 历史页那一列：帧面 `history-browse`（几台的清单在本机后端并成一列，按看得见的那一窗答）
pub mod history_list; // 历史页的平铺会话清单：帧面 `history-list`（每行带状态与「能做什么」· 按项目的分组 · 搜标题 / 第一句 / 项目名）
pub(crate) mod list_node; // 会话清单变了的节点：发端（watcher）响、流的 tap 订它推 `changed {history_list}`，答清单那一刻放开
pub mod listing; // 会话清单那一族：项目清单 · 平铺会话清单（连缓存）· 热缓存 · 叫会话什么 · 「这一行能做什么」· 分叉来源 · agent 家
pub(crate) mod listing_scan; // 历史清单那一行的逐行扫描（累计的那几格 · 读每一行的窄探针）
pub(crate) mod live;
pub(crate) mod remote_listing; // 远端那台的清单：那台怎么答（`history-listing`，带键集合）· 本机后端经 `--resident-call` 取来记着副本 // 这台此刻活着的会话：pidfile ＋ 判活（原住 `observe/accounts_query.rs`）
                               // 搜索与叫会话什么的通用口径（原共享 crate `search-core` 通用那一半：常量 · snippet 预算 · 最近优先 · 片段 / 截断 · 标题）。
pub(crate) mod search_rules;
pub(crate) mod session_hidden; // 隐藏的判定在历史这一层的口（注解里单个那一格 · 叫主会话什么）
