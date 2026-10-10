//! `Origin` 的判据，在 `A`/`C`/`E` 三组。
//!
//! # 🔴 反空真：绿从哪来
//!
//! 四组各是**相等断言**，不是「没抛就绿」：
//! · `A` 线上形状**逐字节**与今天相同（**两个**幸存值双向 round-trip，字面量写死）
//!   ＋ 线上 `null` **被拒**，而拒的那句话逐字带着出路
//! · `B` 三处哨兵住址**两向**相等（少一处、改一个字都红）
//! · `C` 语义**逐个点名**（「没给名字」不等于 `Local` 那一条单独一格）
//! · `D` 迁移进度是一条**递减棘轮**（现打还在用裸 `&str`/`String` 的处数，只许变少）
//!
//! # 🔴 `Unspecified(())` 退役之后，这几组怎么改的
//!
//! 变体退役了，而它守的那条性质（「**「没说」不许被悄悄当成本机**」）**一条判据都没少**，
//! 只是各自换了被钉的那个值 —— 处置与理由逐条写在 `src/comms/inward/origin.rs` 头注里，
//! 这里只记「哪一条钉到哪儿去了」，免得下一个人以为是删了判据换绿：
//!
//! | 上一拍钉的 | 这一拍钉的 | 住在哪一条 |
//! |---|---|---|
//! | 线上 `null` 解得出 `Unspecified` | 线上 `null` **解不出来**，报错里逐字有 `"<local>"` | `A` 组 `an_unknown_shape_is_refused_not_guessed` |
//! | `Unspecified.is_local() == false` | `Origin("").is_local() == false`（空白名既不是本机也不是远端）| `C` 组 `a_blank_name_is_neither_local_nor_remote` |
//! | `route(null)` ⇒ `Err`，点名命令 ＋ 逐字 `"<local>"` | `route("")` ⇒ `Err`，**同样**点名命令 ＋ 逐字 `"<local>"` | `E` 组 `route_names_both_wire_values_and_refuses_the_one_that_says_nothing` |
//! | 本机与「没说」序列化出来不同 | **没有任何 `Origin` 序列化成 `null`**（换成对线上形状的全集断言）| `E` 组 `route_is_exactly_two_outcomes_and_local_is_not_a_missing_value` |
//!
//! `D` 那一格尤其要说清：它**不是**「都换完了」的判据，是「**别再新增**」的判据。
//! 头注里写着现打的数与口径，改小了要连着改那个数、被人看见一次。
//!
//! # 🔴 步 12·C（2026-09-20）加的两组：`E` · `F`
//!
//! 第一刀（步 12·A）只定类型、不换调用点；这一刀开始**真的有命令吃它**了，
//! 于是多出一种第一刀不存在的失败形状：**「没说」被某条命令悄悄当成本机**。
//!
//! · `E` `Origin::route` 的三态**逐个点名**（线上字面量写死）——
//!   `null` 必须被拒，而且拒的那句话要点名是哪条命令、并说清本机该送什么。
//! · `F` **两向集合相等**：吃 `origin: Origin` 的 `#[tauri::command]`
//!   == 体里经 `.route(` 分本机的那些。两边不同源（一边读签名、一边读体）⇒ 不是恒等。
//!   带一条**阳性对照**（喂一份手写分本机的假源码，摘出来必须是「只在左边」那一形）。
//!
//! ⚠ **这两组都不判**「本机那一支干得对不对」——那是各命令自己的测试。
//!   它们买的是「**分本机这一步只有一个住址，而且那个住址不许把 `null` 当本机**」。

use crate::origin::*;

// ── A：线上形状与今天逐字节相同 ────────────────────────────────────────────
//
// 🔴 **这一组是本文件存在的前提。** `Origin` 落地那一拍必须零行为变化 ——
//   前端今天送 `null` / `"<local>"` / `"devbox"` 三种值，后端今天读得懂。
//   若 `serde` 形状变了，这个类型就不能替换任何一处签名（换了就是改协议）。
#[test]
fn the_two_surviving_wire_values_are_byte_identical() {
    // 写死的字面量，不从代码里取 —— 从代码里取就成了「它跟自己一致」。
    // ⚠ 表里**只剩两行**：`null` 那一行搬去了下面那条「被拒」的判据。
    //   两件事刻意不合在一处 —— 「这两个值没变」与「那一个值没了」是两个事实。
    for (wire, want) in [
        ("\"<local>\"", Origin::local()),
        ("\"devbox\"", Origin("devbox".into())),
    ] {
        let got: Origin =
            serde_json::from_str(wire).unwrap_or_else(|e| panic!("读不进 {wire}：{e}"));
        assert_eq!(got, want, "读：线上 {wire} 解出来不是 {want:?}");
        let back = serde_json::to_string(&want).expect("写不出去");
        assert_eq!(
            back, wire,
            "写：{want:?} 序列化出来不是 {wire} —— **协议变了**"
        );
    }
}

#[test]
fn an_unknown_shape_is_refused_not_guessed() {
    // 🔴 数字 / 数组 / 对象都不是 origin。不许有一种「猜」的路 ——
    //   猜出来的那一趟会去操作一台不存在的机器，而且不报错。
    //
    // 🔴 **`null` 从「解得出一个变体」变成了「解不出来」** —— 这一格就是
    //   `Unspecified(())` 退役之后那条性质的新住址：「没说」不再有**任何**类型表示，
    //   于是命令的代码**结构上**见不到那一档。
    for bad in ["null", "3", "[]", "{}", "true"] {
        assert!(
            serde_json::from_str::<Origin>(bad).is_err(),
            "线上 {bad} 被解成了一个 Origin —— 那是在猜"
        );
    }
    // ★ 拒得掉不够，**拒的那句话要带着出路**。
    //   serde 的 stock 报错只会说「invalid type: null, expected a string」——
    //   那句话是对的，但它把调用方留在原地。本文件手写 `Deserialize` 的**全部理由**
    //   就是这一句：`expecting()` 里逐字有 `"<local>"`。
    //   ⚠ 这一格与 `E` 组那条 `route` 的拒绝词**不是同一个事实**：
    //     一个是「线上送了 null」（边界拒），一个是「线上送了空白名」（漏斗拒）。
    let err = serde_json::from_str::<Origin>("null")
        .expect_err("线上 null 被解成了一个 Origin")
        .to_string();
    assert!(
        err.contains(LOCAL),
        "拒 `null` 那句话没告诉调用方本机该送 `{LOCAL}`：{err}\n\
         ⇒ `Deserialize` 被换回 `derive` 了（stock 报错不含这个串），\n\
            或者 `expecting()` 那句话被改写了。"
    );
}

// ── B：三处哨兵住址两向相等 ────────────────────────────────────────────────
#[test]
fn the_sentinel_agrees_with_the_two_existing_homes() {
    // Rust 那一处（本文件之外的既有住址）
    assert_eq!(
        LOCAL,
        crate::inbound_client::LOCAL_ORIGIN,
        "本文件的 `LOCAL` 与 `inbound_client::LOCAL_ORIGIN` 不一致 —— \n\
         那两个值必须逐字节相同，否则 `Origin::is_local()` 与后端那侧的判定会分叉，\n\
         而分叉的后果**不报错**：它只是查不到那个 origin，然后静默当成「没有这台机器」。"
    );
    // TS 那一处
    let ts = include_str!("../../../src/frontend/ui/backend-policy.ts");
    let needle = "export const LOCAL_ORIGIN = ";
    let n = ts.matches(needle).count();
    assert_eq!(
        n, 1,
        "`backend-policy.ts` 里 `{needle}` 命中 {n} 次（应当 1）—— 抽取器坏了"
    );
    let ts_val = ts
        .split_once(needle)
        .and_then(|(_, r)| r.split_once(';'))
        .map(|(v, _)| v.trim().trim_matches('"').to_string())
        .expect("抠不出 TS 侧的值");
    assert_eq!(
        ts_val, LOCAL,
        "TS 侧 `{ts_val}` 与本文件的 `LOCAL`（`{LOCAL}`）不一致"
    );
}

// ── C：语义逐个点名 ────────────────────────────────────────────────────────
#[test]
fn a_blank_name_is_neither_local_nor_remote() {
    // 🔴 **这一格单独立，因为它是最容易被写错的那一条。**
    //   `INVARIANTS §40`「本地 ＝ 不走 ssh 的远端」⇒「没说」要么被拒、
    //   要么由调用点补默认，**不许在类型这一层悄悄当成本机**。
    //
    // 🔴 **它钉的值换了：`Unspecified` → 空白名。**
    //   `null` 已经在反序列化那一层被拒（`A` 组那一格），构造不出来；
    //   而空串是它退役之后线上**唯一**还能表达「没说」的值 —— 而且它在盘上
    //   **真的**被当过本机：`subagent·rs` 那个 `Backend::for_origin`〔散文墓碑〕上一拍逐字写着
    //   「`origin` 缺省 / **空串** = 本机」。⇒ 只删 `null` 不管空串，
    //   等于把同一个洞从一个值搬到另一个值。
    for blank in ["", " ", "\t", "\n  "] {
        let u = Origin(blank.to_string());
        assert!(
            !u.is_local(),
            "空白名 {blank:?} 被判成了本机 —— 那是在替调用方做决定"
        );
        assert!(!u.is_remote(), "空白名 {blank:?} 被判成了远端");
        assert_eq!(u.host_name(), None, "空白名 {blank:?} 不该答出一个机器名");
    }
    // ⚠ 射程边界，写出来：空白名**仍然构造得出来**（`Origin` 的字段是 `pub` 的），
    //   也仍然有线上字符串。挡它的是 `route` 那道闸（`E` 组），不是类型本身。
    //   这一格买的只是「那三个判定不会把它当成一台机器」。
    assert_eq!(Origin(String::new()).as_wire_str(), "");
}

#[test]
fn local_and_remote_are_exactly_complementary() {
    let l = Origin::local();
    assert!(l.is_local() && !l.is_remote(), "本机那一个的两个判定不互补");
    assert_eq!(
        l.host_name(),
        None,
        "🔴 `host_name` 只答远端 —— 本机回 None 是刻意的"
    );
    assert_eq!(l.as_wire_str(), LOCAL);

    let r = Origin("devbox".into());
    assert!(r.is_remote() && !r.is_local(), "远端那一个的两个判定不互补");
    assert_eq!(r.host_name(), Some("devbox"));
    assert_eq!(r.as_wire_str(), "devbox");
}

// ── D：迁移进度 —— 递减棘轮 ───────────────────────────────────────────────
//
// 🔴 **它不是「都换完了」的判据，是「别再新增」的判据。**
//   `Origin` 落地那一刻，盘上有 46 处 `origin: &str` ＋ 33 处 `origin: String`
//   ＋ 8 处 `origin: Option<String>`（现打）。换掉它们是分批的活。
//
// ⚠ **口径写死**：数的是 `src/frontend/shell/src/**.rs` 与 `src/backend/**.rs` 的**生产段**里
//   形如 `origin: &str` / `origin: String` / `origin: Option<String>` 的**签名处**。
//   不数注释、不数测试段、不数 `origin: Origin`（那是已经换过的）。
//
// ⚠ **为什么是棘轮而不是恒等**：恒等要求「每换一处就改一次这个数」，
//   那会让每一批迁移都多一次无谓的 diff；而棘轮在「**变多**」这个方向上是严的
//   —— 新写一处裸 `&str` 的 origin 参数当场红，那正是要挡的。
//   ⚠ 反过来说：它在「变少」方向上**是瞎的**（这是刻意的，不是漏）。
// ⚠ **这个数是现打的 93，不是 46+33+8=87。** 我第一版写 87 —— 那是从
//   **全仓 grep** 算来的，而本条的口径是「**两棵树的生产段**」：
//   两者人群不同（普查含注释与测试段、不含 `src/backend` 的一部分）。
//   🔴 **这正是「抄一个别处的数当分母」的典型** —— 本仓反复治的那一形。
//   ⇒ 用本条自己现打的数，并把口径写在上面。
// 🔴 **93 → 88，降的 5 处逐处记在这里**（本条自己要求
//    「换掉一批之后**把上面那个数改小**（连着改，别攒着）」）。
//
//    五处全部出自「同义双份命令合成一条带 origin 的」那一刀 —— 被合掉的那 5 条
//    远端命令，签名从 `origin: String` 变成了 `host: &str`：
//      · `remote_branch.rs` 的 `create_remote_branch_session`〔散文墓碑〕
//      · `remote_history.rs` 的 `delete_remote_history_session`〔散文墓碑〕
//      · `stream_remote_history_sessions`〔散文墓碑〕（随远端会话清单搬进本机后端一起删了）
//      · `stream_read_remote_session`〔散文墓碑〕（函数也删了：本机远端合成一条 `history·rs::stream_read_session_jsonl`）
//      · `list_remote_mcp_project_dirs`〔散文墓碑〕（本机远端同一条 `mcp-read`，那个远端分支删了）
//
// 🔴 **为什么参数名从 `origin` 改成 `host`，而不是原样留着**：
//    这五个函数今天拿到的是**已经分过本机**的机器名（分本机那一步住合并后那条命令里）。
//    继续叫 `origin` 是句假话 —— `origin` 的取值域含 `"<local>"`，而这五处
//    **结构上收不到它**。⇒ 改名是「名字说真话」，顺带这个数跟着降，不是为了降它才改名。
//
// ⚠ **这个数是跑出来的**：把上限临时改成 0、让本条印出现打的 88，再照它写。
//    93 − 5 = 88 恰好也对得上，但「算出来恰好相等」不是判据（本仓治过的同形病）。
// 🔴 **88 → 86，降的 2 处逐处记在这里**（本条自己要求
//    「换掉一批之后**把上面那个数改小**（连着改，别攒着）」）。
//
//    两处都出自「`origin` 归一的**最后两对**同义双份命令合成一条带 origin 的」那一刀 ——
//    被合掉的那 2 条远端命令，签名从 `origin: String` 变成了 `host: &str`：
//      · `write_remote_mcp_server`〔散文墓碑〕（原住 `mcp.rs`，随 MCP 进后端删了）
//      · `remove_remote_mcp_server`〔散文墓碑〕（同上）
//
//    改名的理由与上一拍那五处**逐字同形**：这两个函数今天拿到的是**已经分过本机**的
//    机器名（分本机那一步住合并后那条命令里），继续叫 `origin` 是句假话
//    （`origin` 的取值域含 `"<local>"`，而它们**结构上收不到它**）。
//
// ⚠ **这个数是跑出来的**：把上限临时改成 0、让本条印出现打的 86，再照它写
//   （现打那一行逐字「裸字符串 origin 参数现打 86 处，上限 0」）。
//   88 − 2 = 86 恰好也对得上，但「算出来恰好相等」不是判据（本仓治过的同形病）。
// ⚠ 这一拍**没有**新增一条吃 `Origin` 的命令却让这个数不动的情形：
//   新落地的 `sftp_pool::sftp_chmod` 收的是 `RemoteConfig` 不是 origin，不进本条人群。
//
// 🔴 **86 → 85，降的 1 处记在这里**（本条自己要求
//    「换掉一批之后**把上面那个数改小**（连着改，别攒着）」）。
//
//    那一处是**全仓最后一条在入方向收 `Option<String>` origin 的命令**：
//      · `subagent·rs` 的 `load_subagent`〔散文墓碑〕（`origin: Option<String>` → `origin: crate::origin::Origin`）
//
//    🔴 **它与上两拍那七处不是同一件事，别读成同一形**：上两拍是「远端那条命令拿到的
//    已经是分过本机的机器名 ⇒ 改叫 `host`」（名字说真话）；这一拍是「这条命令**本来**
//    就在自己心里把两个『没说』的值（`None` 与 `""`）悄悄当本机 ⇒ 让它收 `Origin`，
//    并把分本机那一步交给唯一的漏斗 `route`」。前者是**改名**，后者是**去 `null` 化**。
//    ⇒ 这个数跟着降只是副产物；步 2 买的是「线上那个 `null` 没了」。
//
// ⚠ **这个数是跑出来的**：把上限临时改成 0、让本条印出现打的 85，再照它写
//   （现打那一行逐字「裸字符串 origin 参数现打 85 处，上限 0」）。
//   86 − 1 = 85 恰好也对得上，但「算出来恰好相等」不是判据（本仓治过的同形病）。
//
// 🔴 **〔条 66〕85 → 83，降的 2 处记在这里**：「退出行为」那个值搬到后端那台机器上，
//    monitor 侧那两处裸字符串 origin 随原来那条推送链一起没了 ——
//      · 推生效值的那条 tauri 命令（`origin: String`）退役；
//      · 读进程内那张表的 `kill_on_exit(origin: &str)` 退役。
//    新的两条命令（`backend_exit_policy` / `set_backend_exit_policy`）与退出臂那一问**一开始就收 `Origin`**，
//    不进本条人群。⚠ 这个数是跑出来的：上限临时改成 0，现打那一行逐字「裸字符串 origin 参数现打 83 处，上限 0」。
//
// 🔴 **83 → 81**：
//    · 基线 `1f7a8bf7` 上现打就是 **82**（上限 83 那一格富余不是本路造成的，本路起步时就在；来历没追）；
//    · 本路降 1 处：`lib·rs::batch_to_payloads`（`origin: Option<String>`，`None` = 本机）→ `origin: &Origin` ——
//      它同时是漂移记账的那台，缺省当本机正是「没说被悄悄当成本机」那一形。
// ⚠ 这个数是跑出来的：上限临时改成 0，现打那一行逐字「裸字符串 origin 参数现打 81 处，上限 0」。
// 🔴 **81 → 83 → 81（上限没动）**：主线外清单那一拍（`session_book` 的 `In::Branch` / `Out::Branch`）新写了两处 `origin: String`，
//    推上去 CI 红；那两格改收 `crate::origin::Origin`（分本机仍在线上串那一层，账本键照旧是线上串）。
// ⚠ 这个数是跑出来的：上限临时改成 0，现打那一行逐字「裸字符串 origin 参数现打 81 处，上限 0」。
const ORIGIN_MIGRATION_CEILING: usize = 81;

#[test]
fn no_new_raw_string_origin_parameters() {
    let mut hits: Vec<String> = Vec::new();
    for root in ["src/frontend/shell/src", "src/backend"] {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join(root);
        // 🔴 **这里不许手写遍历。** `scanning_guard_registry::tests::\
        //   no_new_guard_walks_the_tree_without_excluding_itself` 是一条**结构性**判据：
        //   扫描型判据一律走 `guard_core::scan_tree!`，因为它**按构造**摘除调用者自己那份。
        //   我第一版就是手写的 while-stack ⇒ 当场被那条判据逮住。
        //   ⚠ 如实说清它今天买到了什么：本条的两棵树（`src/frontend/shell/src` · `src/backend`）
        //     都**不含本文件**（判据住 `tests/frontend/shell/`）⇒ 那个摘除今天是**空转**的。
        //     换它的真实收益是两条：① 遍历口径只有一份（不下构建产物目录 · 非 UTF-8 跳过
        //     这两条边界都是别处现打逮出来的）· ② 住址错时**当场 panic** 而不是
        //     像原来那样 `read_dir` 失败就 `continue`（那会让本条静默少扫一棵树）。
        for (path, src) in guard_core::scan_tree!(&dir, &["rs"]) {
            let prod = guard_core::production_source(&src);
            for pat in ["origin: &str", "origin: String", "origin: Option<String>"] {
                for _ in 0..prod.matches(pat).count() {
                    hits.push(format!("{}  {pat}", path.display()));
                }
            }
        }
    }
    // 抽取器自检：扫到 0 处 ⇒ 要么剥法把生产段剥空了、要么两棵树的住址错了。
    assert!(
        hits.len() >= 10,
        "只扫到 {} 处裸字符串 origin —— 抽取器坏了（剥过头 / 住址错），本条在空转",
        hits.len()
    );
    assert!(
        hits.len() <= ORIGIN_MIGRATION_CEILING,
        "裸字符串 origin 参数现打 {} 处，上限 {}。\n\
         **它只许变少。** 新写一处 `origin: &str` / `origin: String` / `origin: Option<String>` \n\
         就是给这个概念再造一种表达 —— 而那一节整篇治的就是这个。\n\
         ⇒ 新代码用 `crate::origin::Origin`。\n\
         ⚠ 换掉一批之后**把上面那个数改小**（连着改，别攒着）。",
        hits.len(),
        ORIGIN_MIGRATION_CEILING
    );
}

// ── E：`route` 的三态 —— 步 12·C 的地基 ───────────────────────────────────
//
// 🔴 **这一组判的是「『没说』不会被悄悄当成本机」。**
//
// `INVARIANTS §40` 逐字「本地 ＝ 不走 ssh 的远端」⇒ 本机是一个**具名**的 origin。
// 合并之后每条命令都要在入口分一次本机，而「分错」这件事**不报错**：
// 调用方送 `null`，命令去动了本机的文件，用户看到的是一次「成功」。
// ⇒ 三态在这里逐个点名，**写死线上字面量**（不从代码里取 —— 从代码里取就成了
//   「它跟自己一致」，那是本文件 `A` 组头注已经写过的那条纪律）。

#[test]
fn route_names_both_wire_values_and_refuses_the_one_that_says_nothing() {
    // 线上那两种值，逐个从**字面量**解出来再路由 —— 走的是真正的 `serde` 那条路。
    let local: Origin = serde_json::from_str("\"<local>\"").expect("读不进 <local>");
    let remote: Origin = serde_json::from_str("\"devbox\"").expect("读不进 devbox");
    // 🔴 **「没说」这一档也从线上字面量解出来** —— `null` 已经解不进来了（`A` 组钉着），
    //   而空串**解得进来**，这正是它今天还需要一道闸的理由。
    let unsaid: Origin = serde_json::from_str("\"\"").expect("读不进空串");

    assert_eq!(
        local.route("t"),
        Ok(Route::Local),
        "线上 `\"<local>\"` 没有被路由成本机 —— 那本机那条路就没人走得到了"
    );
    assert_eq!(
        remote.route("t"),
        Ok(Route::Remote("devbox")),
        "线上 `\"devbox\"` 没有被路由成那台远端"
    );

    // 🔴 **这一格是本组的全部力气所在。**
    let err = unsaid
        .route("某条命令")
        .expect_err("线上空串被路由出去了 —— 「没给名字」被当成了一台机器");
    assert!(
        err.contains("某条命令"),
        "拒绝的那句话没点名是哪条命令，排障时无从下手：{err}"
    );
    // 拒的那句话里要**逐字**给出本机该送什么，否则调用方只知道被拒、不知道怎么改。
    assert!(
        err.contains(LOCAL),
        "拒绝的那句话没告诉调用方本机该送 `{LOCAL}`：{err}"
    );
    // ⚠ 全空白也算「没给名字」—— 只挡 `""` 等于给 `" "` 留门。
    assert!(
        Origin(" \t ".into()).route("t").is_err(),
        "全空白的名字被路由出去了 —— 只挡空串就是给它留门"
    );
    // ★ 反向：非空白的名字**不许**被这道闸误杀（假红比不查更坏）。
    assert_eq!(
        Origin(" devbox ".into()).route("t"),
        Ok(Route::Remote(" devbox ")),
        "带空格的机器名被误当成「没给名字」，而且 `route` 不许 trim（trim 会让\n\
         今天找不到配置的 label 突然找得到 —— 那是行为变更，不归本步）"
    );
}

#[test]
fn route_is_exactly_two_outcomes_and_local_is_not_a_missing_value() {
    // `Route` 只有两个变体 —— 「没说」在 `route` 那一步就成了 `Err`，
    // 拿到 `Route` 的代码**没有办法**把它误当本机。
    // 这一格用**穷尽 match**（没有 `_` 臂）把这件事钉在类型上：
    // 哪天有人给 `Route` 加第三个变体，这里编译不过。
    for (o, want_local) in [(Origin::local(), true), (Origin("devbox".into()), false)] {
        let got = o.route("t").expect("具名的 origin 不该被拒");
        let is_local = match got {
            Route::Local => true,
            Route::Remote(_) => false,
        };
        assert_eq!(is_local, want_local, "{o:?} 路由错了边");
    }

    // 🔴 **这一条换成了对线上形状的全集断言。**
    //
    //    上一拍它逐字是「本机与『没说』序列化出来不是同一个东西」—— 那条断言以
    //    `Unspecified` 存在为前提，变体退役之后它连编译都过不去。
    //    **但那件事本身没了**：今天**没有任何 `Origin` 序列化成 `null`** ——
    //    比「两者不相等」更强的一条，而且它不需要第二个变体才说得出口。
    //
    //    为什么这一条不是废话：合并之前本机那几条命令**根本没有 origin 参数**，
    //    前端省掉它就对了；合并之后省掉它就是 `null`，而 `null` ≠ 本机。
    //    两者在 UI 上都长成「没填」—— 所以线上必须**一个字节**都装不下「没填」。
    for o in [
        Origin::local(),
        Origin("devbox".into()),
        Origin(String::new()),
        Origin(" ".into()),
    ] {
        let wire = serde_json::to_string(&o).expect("写不出去");
        assert_ne!(
            wire, "null",
            "{o:?} 序列化成了 `null` —— 「没填」又回到线上了"
        );
        assert!(
            wire.starts_with('"') && wire.ends_with('"'),
            "{o:?} 的线上形状不是字符串（实得 {wire}）—— `serde(transparent)` 被拿掉了？"
        );
    }
}

// ── F：合并后的命令**一律**经 `route` 分本机 ──────────────────────────────
//
// 🔴 **它治的是一种不报错的失败**：一条吃 `Origin` 的命令自己手写
// `if origin.is_local() { 本机 } else { 远端 }` —— 编译得过、测试多半也绿，
// 而一个**空白名**会掉进 `else` 那一臂，拿着一个空的机器名去连远端，
// 报出来的话与真实原因毫无关系（`local_origin_registry` 整篇治的正是这一形）。
//
// ⚠ **判法是两向集合相等，不是「有几处」**：
//   左边 = 签名里吃 `origin: crate::origin::Origin` 的 `#[tauri::command]`；
//   右边 = 函数体里调了 `.route(` 的那些。
//   两边**不同源**（一边读签名、一边读体）⇒ 不是恒等。
//   · 新写一条吃 `Origin` 却手写分支的命令 ⇒ 只在左边 ⇒ 红；
//   · 某条命令不再吃 `Origin` 了却还留着 `.route(` ⇒ 只在右边 ⇒ 红。

/// 从生产段里摘出每条 `#[tauri::command]` 的 (名字, 参数列表, 函数体)。
///
/// ⚠ **收尾括号从签名之后找起** —— 这是本轮实打踩过的坑（`history_tests.rs` 那条
/// 同形判据）：`pub(crate) async fn …(` 这样的签名**自己就含一个 `)`**。
/// 这里从 `fn` 后面的第一个 `(` 起算，天然避开。
fn tauri_commands_with_bodies(src: &str) -> Vec<(String, String, String)> {
    let attr = format!("#[tauri::{}]", "command");
    let mut out = Vec::new();
    for (i, _) in src.match_indices(&attr) {
        let rest = &src[i..];
        let Some(fpos) = rest.find("fn ") else {
            continue;
        };
        // 属性与 fn 之间只许别的属性/空白 —— 否则不是同一个声明。
        if rest[..fpos].contains('{') {
            continue;
        }
        let after = &rest[fpos + 3..];
        let Some(open) = after.find('(') else {
            continue;
        };
        let name = after[..open].trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        // 参数列表：从第一个 `(` 配对到它的 `)`（参数里有泛型尖括号与嵌套括号）。
        let mut depth = 0usize;
        let mut close = None;
        for (k, c) in after[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { continue };
        let params = after[open + 1..close].to_string();
        // 函数体：从参数列表之后的第一个 `{` 到**列 0 的右大括号**
        //（`rustfmt` 保证顶层 item 这么收；同 `parity_ledger_tests::top_level_fn_bodies` 的口径）。
        let tail = &after[close..];
        let Some(brace) = tail.find('{') else {
            continue;
        };
        let body_rest = &tail[brace..];
        let end = body_rest
            .find("\n}\n")
            .map(|x| x + 2)
            .unwrap_or(body_rest.len());
        out.push((name.to_string(), params, body_rest[..end].to_string()));
    }
    out
}

#[test]
fn every_origin_taking_command_splits_local_through_route() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("src/frontend/shell/src");
    // 🔴 遍历走 `scan_tree!`（同本文件 `D` 组的理由：`scanning_guard_registry` 那条
    //    结构性判据禁止手写遍历）。
    let mut takes_origin: std::collections::BTreeSet<String> = Default::default();
    let mut calls_route: std::collections::BTreeSet<String> = Default::default();
    let mut total_cmds = 0usize;
    for (_, raw) in guard_core::scan_tree!(&dir, &["rs"]) {
        let prod = guard_core::production_source(&raw);
        for (name, params, body) in tauri_commands_with_bodies(&prod) {
            total_cmds += 1;
            let flat: String = params.split_whitespace().collect::<Vec<_>>().join(" ");
            // 认的是**类型**，不是参数名：叫 origin 但收 String 的那一档归递减棘轮管。
            if flat.contains("origin: crate::origin::Origin") || flat.contains("origin: Origin") {
                takes_origin.insert(name.clone());
            }
            if body.contains(".route(") {
                calls_route.insert(name);
            }
        }
    }
    // 反向自检①：抽取器真的摘到了命令（塌成 0 的话下面那条相等是 `{} == {}`）。
    // 地板 100 → 50：命令按设计逐批迁通道、人群在缩（今天 98）；这是抽取器反空真地板（塌了是个位数），不是计数棘轮。
    assert!(
        total_cmds >= 45, // 50 → 45：会话正文四条退役（今天 47）
        "只摘到 {total_cmds} 条 `#[tauri::command]` —— 抽取器坏了，本条在空转"
    );
    // 反向自检②：左边那个人群不许是空集 —— 步 12·C 落地之后它至少有 5 条。
    assert!(
        takes_origin.len() >= 5,
        "吃 `Origin` 的命令只数到 {} 条（现打应 ≥5）—— 人群塌了，本条在空转：{takes_origin:?}",
        takes_origin.len()
    );
    // ★ 有牙的那条：两向集合相等。
    assert_eq!(
        takes_origin, calls_route,
        "**吃 `Origin` 的命令 与 经 `route` 分本机的命令，两边对不上。**\n\
         · 只在左边（吃了 `Origin` 却没走 `route`）＝ 有人手写了分本机那一步。\n\
           手写的那一版会把线上的**空白名**掉进「远端」那一臂，\n\
           然后拿着一个 `None` 的机器名去连 —— 报出来的话与真实原因毫无关系，\n\
           而且**不报错的那一半更贵**：掉进「本机」那一臂就是替调用方做了决定。\n\
         · 只在右边（走了 `route` 却不吃 `Origin`）＝ 签名退回裸字符串了，\n\
           那一档归 `ORIGIN_MIGRATION_CEILING` 那条递减棘轮。\n\
         ⇒ 出路只有一条：`match origin.route(\"<命令名>\")? {{ Route::Local => …, Route::Remote(host) => … }}`。"
    );
}

#[test]
fn the_command_body_scanner_would_notice_a_hand_rolled_split() {
    // ★ **阳性对照** —— 上面那条全靠 `tauri_commands_with_bodies` 摘得准。
    //   这里喂它一份**手写分本机**的假源码：签名吃 `Origin`、体里不调 `route`，
    //   摘出来必须是「只在左边」那一形。摘不出来 ⇒ 上面那条在空转。
    let poisoned = format!(
        "#[tauri::{}]\npub async fn fake_cmd(origin: crate::origin::Origin, x: String) -> Result<(), String> {{\n    if origin.is_local() {{ return Ok(()); }}\n    Err(x)\n}}\n",
        "command"
    );
    let got = tauri_commands_with_bodies(&poisoned);
    assert_eq!(got.len(), 1, "阳性对照：一条命令都没摘到 —— 抽取器坏了");
    let (name, params, body) = &got[0];
    assert_eq!(name, "fake_cmd");
    assert!(
        params.contains("origin: crate::origin::Origin"),
        "阳性对照：签名里的 `Origin` 没摘出来 —— 上面那条的**左边**永远是空集，\
         于是「有人手写分本机」这一形它一辈子看不见"
    );
    assert!(
        !body.contains(".route("),
        "阳性对照：这份假源码体里本来就没有 `.route(`，却被摘出来了 —— 体的切法错位了"
    );
    assert!(
        body.contains("is_local()"),
        "阳性对照：体没切到（切出来的是 {body:?}）—— 切法塌了，\
         而塌了之后「体里没有 `.route(`」恒成立 ⇒ 上面那条会**反过来**假红/假绿"
    );
}
