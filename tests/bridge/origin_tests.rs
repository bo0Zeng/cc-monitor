//! `Origin` 的判据。`设计/00 §2.5 ①` 的第一刀。
//!
//! # 🔴 反空真：绿从哪来
//!
//! 四组各是**相等断言**，不是「没抛就绿」：
//! · `A` 线上形状**逐字节**与今天相同（三种值双向 round-trip，字面量写死）
//! · `B` 三处哨兵住址**两向**相等（少一处、改一个字都红）
//! · `C` 三个变体的语义**逐个点名**（`Unspecified` 不等于 `Local` 那一条单独一格）
//! · `D` 迁移进度是一条**递减棘轮**（现打还在用裸 `&str`/`String` 的处数，只许变少）
//!
//! `D` 那一格尤其要说清：它**不是**「都换完了」的判据，是「**别再新增**」的判据。
//! 头注里写着现打的数与口径，改小了要连着改那个数、被人看见一次。

use super::*;

// ── A：线上形状与今天逐字节相同 ────────────────────────────────────────────
//
// 🔴 **这一组是本文件存在的前提。** `Origin` 落地那一拍必须零行为变化 ——
//   前端今天送 `null` / `"<local>"` / `"aya"` 三种值，后端今天读得懂。
//   若 `serde` 形状变了，这个类型就不能替换任何一处签名（换了就是改协议）。
#[test]
fn the_wire_shape_is_byte_identical() {
    // 写死的字面量，不从代码里取 —— 从代码里取就成了「它跟自己一致」。
    for (wire, want) in [
        ("null", Origin::Unspecified(())),
        ("\"<local>\"", Origin::local()),
        ("\"aya\"", Origin::Named("aya".into())),
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
    for bad in ["3", "[]", "{}", "true"] {
        assert!(
            serde_json::from_str::<Origin>(bad).is_err(),
            "线上 {bad} 被解成了一个 Origin —— 那是在猜"
        );
    }
}

// ── B：三处哨兵住址两向相等 ────────────────────────────────────────────────
#[test]
fn the_sentinel_agrees_with_the_two_existing_homes() {
    // Rust 那一处（本文件之外的既有住址）
    assert_eq!(
        LOCAL,
        crate::backend::control::inbound_client::LOCAL_ORIGIN,
        "本文件的 `LOCAL` 与 `inbound_client::LOCAL_ORIGIN` 不一致 —— \n\
         那两个值必须逐字节相同，否则 `Origin::is_local()` 与后端那侧的判定会分叉，\n\
         而分叉的后果**不报错**：它只是查不到那个 origin，然后静默当成「没有这台机器」。"
    );
    // TS 那一处
    let ts = include_str!("../../src/backend-policy.ts");
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

// ── C：三个变体的语义逐个点名 ──────────────────────────────────────────────
#[test]
fn unspecified_is_not_local() {
    // 🔴 **这一格单独立，因为它是最容易被写错的那一条。**
    //   `INVARIANTS §40`「本地 ＝ 不走 ssh 的远端」⇒「没说」要么被拒、
    //   要么由调用点补默认，**不许在类型这一层悄悄当成本机**。
    //   今天那 8 处 `Option<String>` 的歧义正在这里：`None` 到底是哪个意思，
    //   要读每一处的上下文才知道。
    let u = Origin::Unspecified(());
    assert!(
        !u.is_local(),
        "`Unspecified` 被判成了本机 —— 那是在替调用方做决定"
    );
    assert!(!u.is_remote(), "`Unspecified` 被判成了远端");
    assert_eq!(u.host_name(), None);
    assert_eq!(u.as_wire_str(), None, "「没说」不该有线上字符串");
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
    assert_eq!(l.as_wire_str(), Some(LOCAL));

    let r = Origin::Named("aya".into());
    assert!(r.is_remote() && !r.is_local(), "远端那一个的两个判定不互补");
    assert_eq!(r.host_name(), Some("aya"));
    assert_eq!(r.as_wire_str(), Some("aya"));
}

// ── D：迁移进度 —— 递减棘轮 ───────────────────────────────────────────────
//
// 🔴 **它不是「都换完了」的判据，是「别再新增」的判据。**
//   `Origin` 落地那一刻，盘上有 46 处 `origin: &str` ＋ 33 处 `origin: String`
//   ＋ 8 处 `origin: Option<String>`（`真相源/97` 现打）。换掉它们是分批的活。
//
// ⚠ **口径写死**：数的是 `src/bridge/src/**.rs` 与 `src/backend/**.rs` 的**生产段**里
//   形如 `origin: &str` / `origin: String` / `origin: Option<String>` 的**签名处**。
//   不数注释、不数测试段、不数 `origin: Origin`（那是已经换过的）。
//
// ⚠ **为什么是棘轮而不是恒等**：恒等要求「每换一处就改一次这个数」，
//   那会让每一批迁移都多一次无谓的 diff；而棘轮在「**变多**」这个方向上是严的
//   —— 新写一处裸 `&str` 的 origin 参数当场红，那正是要挡的。
//   ⚠ 反过来说：它在「变少」方向上**是瞎的**（这是刻意的，不是漏）。
// ⚠ **这个数是现打的 93，不是 46+33+8=87。** 我第一版写 87 —— 那是从
//   `真相源/97` 的**全仓 grep** 算来的，而本条的口径是「**两棵树的生产段**」：
//   两者人群不同（普查含注释与测试段、不含 `src/backend` 的一部分）。
//   🔴 **这正是「抄一个别处的数当分母」的典型** —— 本仓反复治的那一形。
//   ⇒ 用本条自己现打的数，并把口径写在上面。
const ORIGIN_MIGRATION_CEILING: usize = 93;

#[test]
fn no_new_raw_string_origin_parameters() {
    let mut hits: Vec<String> = Vec::new();
    for root in ["src/bridge/src", "src/backend"] {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(root);
        // 🔴 **这里不许手写遍历。** `scanning_guard_registry::tests::\
        //   no_new_guard_walks_the_tree_without_excluding_itself` 是一条**结构性**判据：
        //   扫描型判据一律走 `guard_core::scan_tree!`，因为它**按构造**摘除调用者自己那份。
        //   我第一版就是手写的 while-stack ⇒ 当场被那条判据逮住。
        //   ⚠ 如实说清它今天买到了什么：本条的两棵树（`src/bridge/src` · `src/backend`）
        //     都**不含本文件**（判据住 `tests/bridge/`）⇒ 那个摘除今天是**空转**的。
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
         就是给这个概念再造一种表达 —— 而 `设计/00 §2.5 ①` 那一节整篇治的就是这个。\n\
         ⇒ 新代码用 `crate::origin::Origin`。\n\
         ⚠ 换掉一批之后**把上面那个数改小**（连着改，别攒着）。",
        hits.len(),
        ORIGIN_MIGRATION_CEILING
    );
}
