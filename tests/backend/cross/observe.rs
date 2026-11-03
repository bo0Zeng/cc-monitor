//! 观测面判据里引到上层（控制面 · 帧面 · 进后端的口）的那几条：那一层是独立的包（`backend-observe`）、够不到上层 ⇒ 挂在主 crate。

// 跟读：接控制面真打标那一手的对账几条 · 恢复问过记录交会话账本（帧面宿主）· 帧面 `resync` 的金样（进后端的口）。
#[path = "../observe/watcher_on_upper_tests.rs"]
mod watcher_on_upper_tests;
// 会话内查找经帧面宿主问 `history-find` 的那几条（常驻索引那一臂 == 现扫 · 只读尾巴 · 第几轮 · 只搜用户 · 行号）。
#[path = "../observe/search_query_find_on_upper_tests.rs"]
mod search_query_find_on_upper_tests;
// 账号清单：起会话跟随读清单成品（控制面）· 换模型那一句（帧面）。
#[path = "../observe/accounts_query_on_upper_tests.rs"]
mod accounts_query_on_upper_tests;
// 读正文核：拿帧面那一层的投影去剥一页成品的那两条。
#[path = "../observe/query/body_on_upper_tests.rs"]
mod body_on_upper_tests;
// 全文搜索的秤（经登记表跑帧面 `history-search`；读数不是判据）。
#[path = "../observe/search_query_reading.rs"]
mod search_query_reading;
