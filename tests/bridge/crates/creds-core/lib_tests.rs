use super::*;

/// 本文件的生产段。**剥掉 `#[cfg(test)]`** —— 下面那些 needle 的字面量就住在本模块里，
/// 不剥的话判据会在自己的测试代码里找到它们（`scanning_guard_registry` 记着这一族实测五次、
/// 五次都不是被判据变红发现的）。剥生产段是那张表里「**按构造读不到自己**」那一类。
fn production() -> String {
    guard_core::production_code(include_str!(
        "../../../../src/bridge/crates/creds-core/src/lib.rs"
    ))
}

/// 从 `at` 之后的第一个 `{` 起，按花括号配平切出一整块（含两端花括号）。
///
/// ★ 它替掉的是 `.find("\n}")` 那种「找收尾」的写法 —— 那种写法有两个毛病：
/// ① 它是**语料变量上的裸匹配**，撞本仓 `needle_anchor_registry` 那条递减棘轮；
/// ② 更实质的：它会在**块里第一个顶格 `}`** 上停住，而不是这一块真正的收尾。
fn brace_block(src: &str, at: usize) -> Option<&str> {
    let open = src[at..].find('{')? + at;
    let b = src.as_bytes();
    let (mut depth, mut i) = (0i32, open);
    while i < src.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open..=i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `KS1` 机检：明文不许有「顺手打印」的路。
#[test]
fn secret_key_has_no_printing_shortcut() {
    let prod = production();
    // 反空真自检：剥完还得剩下真东西，且靶子确实在里面。
    assert!(
        prod.contains("pub struct SecretKey"),
        "剥生产段之后连类型定义都没了 —— 取法坏了，下面的断言在空转"
    );
    assert!(
        prod.len() > 500,
        "生产段只剩 {} 字节 —— 取法坏了",
        prod.len()
    );

    // ① 三条禁止形态，命中任一即红。分母 = 我登记的这 3 形，**不是**「所有打印写法」。
    for bad in [
        "derive(Debug)",
        "derive(Serialize)",
        "impl fmt::Display for SecretKey",
    ] {
        assert!(
            !prod.contains(bad),
            "`SecretKey` 的定义面出现了 `{bad}` —— \
                 KS1 要的是手写 Debug、恒为遮蔽形；这三形任一都会把明文送上一条顺手打印的路"
        );
    }

    // ② **正向**（黑名单必漏，所以同时要有白名单那一半）：手写 Debug 必须真在，且恰好一处。
    let n = prod.matches("impl fmt::Debug for SecretKey").count();
    assert_eq!(
        n, 1,
        "手写 `Debug` 应当恰好一处，实得 {n} —— 0 处 = 类型可以被别的方式印出来；\
             多处 = 编不过，说明这条判据量错了对象"
    );
    // ③ 遮蔽形的字面必须真在那个 impl 里（不是随便一句 `write_str`）。
    // ⚠ 用 `find_pinned` 而不是裸 `.find("…")`：本仓 `needle_anchor_registry` 立着一条
    //   **递减棘轮**（语料变量上的裸匹配「与 `contains` 同族同险：**needle 被撑大时照样绿**」）。
    //   `find_pinned` 额外买两样：**恰好一处**（多一处/零处都报错，不是悄悄取第一处）+ 两侧有边界。
    //   〔08-27 实测：我第一版写裸 `.find` 撞红了那条棘轮，读数「11 处 > 上限 8」。〕
    let at = guard_core::find_pinned(&prod, "impl fmt::Debug for SecretKey")
        .expect("切不出 Debug impl 的锚点 —— 本条按红处理，不是绿");
    let window = brace_block(&prod, at).expect("Debug impl 的花括号没配平 —— 按红处理");
    assert!(
        window.contains("<已遮蔽>"),
        "手写的 Debug 里没有遮蔽形字面 —— 它可能又把内容印回去了。窗口：{window}"
    );
    // 反空真自检之二（`KP4` 那两个价钱之一：窗口不许跨进下一个 item）。
    assert!(
        !window.contains("pub struct") && !window.contains("pub fn"),
        "Debug impl 的窗口跨进了下一个 item —— 窗口无界，上面那条断言不算数"
    );
}

/// 一个碰了内层明文的函数，**对那份明文做了什么**。
///
/// ★ 这个区分是 `D2` 要求「钉住」的那一格：换成「函数体提到 `self.0`」画人群之后，
/// 表会变长（内部用法也进来），而**一张长登记表最容易退化成「谁红了就往里加一行」**。
/// ⇒ 每一行都要说清它属于哪一类，而 [`Handling::HandsOut`] 那一类**钉死条数**。
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Handling {
    /// **把明文原样交到调用方手里。**只许有两条，名字就是那两个具名出口。
    HandsOut,
    /// 从明文**派生**出一个不含明文的东西（掩码）。
    Derives,
    /// 只读出一个**与内容无关**的事实（空不空 / 多长）。
    ReadsOnly,
}

/// `impl SecretKey` 块里**每一个函数体提到 `self.0` 的 `fn`**，及它对明文做了什么。
///
/// **默认拒绝**：人群从源码派生，没在这张表里的当场红。
/// 「谁进人群」由机器定，「它对明文做了什么」才是人的答案。
const INNER_FIELD_USERS: &[(&str, Handling, &str)] = &[
    (
        "is_configured",
        Handling::ReadsOnly,
        "只看它 trim 之后空不空 —— 一个与内容无关的布尔",
    ),
    (
        "len",
        Handling::ReadsOnly,
        "只看字节数。⚠ 长度**本身也是信息**（能把候选集缩小一个量级），\
             所以它只许进本机日志，不许进回帧 —— 手写的 `Debug` 连长度都不印",
    ),
    (
        "is_empty",
        Handling::ReadsOnly,
        "`len() == 0`。clippy 要求有 `len` 就得有它；它读不出任何内容",
    ),
    (
        "masked",
        Handling::Derives,
        "派生出遮蔽形。**它交出去的不是明文** —— 短到看不出前后缀的整条遮掉，\
             由 `masking_keeps_only_the_two_ends_and_swallows_short_keys_whole` 钉着",
    ),
    (
        "expose_for_auth_header",
        Handling::HandsOut,
        "唯一的**换头**出口。调用点恰好 1 处，在 `src/backend/relay/server.rs`",
    ),
    (
        "expose_for_persisting",
        Handling::HandsOut,
        "唯一的**落盘**出口。调用点恰好 1 处，在 `crates/creds-core/src/store.rs`",
    ),
];

/// 把一个 impl 块切成 `(fn 名, 函数体)`。
///
/// ⚠ 切分用 **`fn `**，不是 `pub fn ` —— 后者漏掉 `pub unsafe fn` / `pub(crate) fn` /
/// `async fn`。D2 复审实打：人群针写成 `"pub fn "` 时，`pub unsafe fn` 那一刀
/// **creds-core 23 passed; 0 failed**（我自己复打确认）。
fn fns_in_block(block: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (i, _) in block.match_indices("fn ") {
        let after = &block[i + 3..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let Some(body) = brace_block(block, i) else {
            continue;
        };
        // ★ **签名**（从 `fn` 到函数体左花括号）单独交出去〔`D3` 阻塞二回修，08-27〕。
        //   先前只回 `body`，而 `body` 是 `brace_block` 从 `{` 起切的
        //   ⇒ **参数表根本不在里面**，于是下面第 ⑤ 段对 `body` 做 `find('(')`
        //   两头都错：真的出参形**漏**（25P/0F 一声不吭），
        //   而一个只在体内 `std::mem::swap(&mut a, &mut b)`、**完全不碰明文**的函数**误报**
        //   （报文逐字「收了一个 `&mut` 出参：`(&mut a, &mut b`」）。两处审计都实打过。
        let sig_end = block[i..].find('{').map(|k| i + k).unwrap_or(block.len());
        out.push((name, block[i..sig_end].to_string(), body.to_string()));
    }
    out
}

/// `mod sealed` 里允许住哪几个 fn。**判据只买这张表；性质由编译器买。**
///
/// ★ `K18` 裁定（`D3`）：字段关进私有模块之后，「谁够得到明文」由 **rustc** 回答 ——
/// `sealed` 之外的任何写法（`.0` · 解构 · 自由函数 · 回调 · 还没人想出来的第五形）
/// **编译不过**。⇒ 剩下的活只有一格：**盯住 `sealed` 里长了什么**。
const SEALED_FNS: &[(&str, &str)] = &[
    ("new", "构造，明文从这里进来（唯一入口）"),
    ("is_configured", "读一个与内容无关的布尔"),
    ("len", "读字节数"),
    ("is_empty", "`len() == 0`"),
    ("masked", "从明文派生出遮蔽形"),
    ("expose_for_auth_header", "**出口**：中转换头"),
    ("expose_for_persisting", "**出口**：落盘"),
];

/// ★★★ **`K18` 的正主〔`D3` 08-27〕：边界由编译器守，本条只守登记表。**
///
/// # 它钉四样，而这四样合起来**才是**「编译器会替我们拦住」的前提
///
/// ㈠ `mod sealed` **恰好一个**，且**不是 `pub`**（是 `pub` 的话外面就够得到里面的私有项路径）；
/// ㈡ 字段**不是 `pub`**（`pub struct SecretKey(pub String)` 会当场把整件事作废）；
/// ㈢ `sealed` 里的 fn **逐个登记**（默认拒绝）——这是唯一还需要判据的一格；
/// ㈣ ★ `impl fmt::Debug` **必须在 `sealed` 之外**。
///    这一条不是洁癖：它在外面 ⇒ 它**够不到字段** ⇒ 「`Debug` 不许印明文」这件事
///    从「一条判据在扫」变成「**编译不过**」。搬回去就把那条编译器保证丢了。
///
/// # ⚠ 它**买不到**什么（诚实边界）
///
/// 本条仍是文本扫描，**扫的是 `sealed` 这一个块**。但它与前三版有个本质差别：
/// 前三版的**性质**依赖扫描穷举（而审计逐字写过「我给不出分母」）；
/// 今天**性质由编译器买**，扫描只用来看「这个小块里有没有多长东西」——
/// 扫漏一个的后果是「多一个未登记的函数没被人看见」，**不是「明文从别处出去了」**。
#[test]
fn the_sealed_module_is_the_only_place_that_can_reach_the_field() {
    let prod = production();

    // ㈠ 恰好一个私有 `mod sealed`
    assert_eq!(
        prod.matches("mod sealed {").count(),
        1,
        "`mod sealed` 不是恰好一个"
    );
    assert_eq!(
        prod.matches("pub mod sealed").count(),
        0,
        "`sealed` 成了 `pub` 模块 —— 那样外面就能顺着路径够到里面，整件事作废"
    );
    assert_eq!(
        prod.matches("pub use sealed::SecretKey;").count(),
        1,
        "类型没有被重新导出（或导出了不止一次）"
    );

    let at = guard_core::find_pinned(&prod, "mod sealed {")
        .expect("切不出 `mod sealed` —— 本条按红处理，不是绿");
    let sealed = brace_block(&prod, at).expect("`mod sealed` 的花括号没配平 —— 按红处理");
    assert!(
        sealed.len() > 800,
        "`sealed` 只有 {} 字节 —— 切法坏了",
        sealed.len()
    );

    // ㈡ 字段不是 pub
    assert_eq!(
        sealed.matches("pub struct SecretKey(String);").count(),
        1,
        "字段的声明形状变了 —— 它必须是私有的元组字段"
    );
    assert_eq!(
        prod.matches("SecretKey(pub ").count(),
        0,
        "字段被标成 `pub` 了 —— 那样谁都够得到，`sealed` 白关"
    );

    // ㈢ `sealed` 里的 fn 逐个登记（默认拒绝）
    let names: Vec<String> = fns_in_block(sealed)
        .into_iter()
        .map(|(n, _, _)| n)
        .collect();
    assert!(
        names.len() >= 7,
        "`sealed` 里只切出 {} 个 fn —— 取法坏了，本条在空转：{names:?}",
        names.len()
    );
    let registered: Vec<&str> = SEALED_FNS.iter().map(|(n, _)| *n).collect();
    for n in &names {
        assert!(
            registered.contains(&n.as_str()),
            "`mod sealed` 里多了一个函数：`{n}`，**没有登记**。\n\
                 ⚠ 它够得到明文（这是编译器允许的，因为它在 `sealed` 里）——\n\
                 所以每加一个都要写清它对明文做什么。今天登记的是：{registered:?}"
        );
    }
    for (n, why) in SEALED_FNS {
        assert!(
            names.iter().any(|x| x == n),
            "登记表里的 `{n}` 已经不在 `sealed` 里了 —— 表和盘对不上"
        );
        assert!(why.len() > 8, "`{n}` 这一行没写理由");
    }
    assert_eq!(
        names.len(),
        SEALED_FNS.len(),
        "`sealed` 里的 fn 数与登记表不等"
    );

    // ㈣ ★ `Debug` 必须在 `sealed` 之外 —— 那一条编译器保证就是这么来的。
    assert_eq!(
        sealed.matches("impl fmt::Debug").count(),
        0,
        "`impl fmt::Debug` 被搬进了 `sealed` —— \n\
             那样它又够得到字段了，「`Debug` 不许印明文」就从**编译不过**退回成**靠判据扫**。"
    );
    assert_eq!(
        prod.matches("impl fmt::Debug for SecretKey {").count(),
        1,
        "`Debug` 的实现不见了或多了一份"
    );
}

/// ★★★ **`KS2` 定义面的正主〔D2 回修，08-27〕：人群按「函数体碰没碰 `self.0`」画。**
///
/// # 这是同一条性质的**第三次**换人群，前两次都被一刀绕过
///
/// | 版本 | 人群怎么画 | 被什么绕过（实测） |
/// |---|---|---|
/// | 初版 | 数 `&self.0` 这**一个字面**出现几次 | `self.0.as_str()` / `self.0.clone()`（`D1` 阻-1） |
/// | 二版 | 返回类型含 `str`/`String` 的 **`pub fn`** | `pub unsafe fn`（针是 `"pub fn "`）· **出参形** `fn f(&self, out: &mut String)`（没有 `->` ⇒ 返回类型读成空串 ⇒ 不进人群）（`D2`） |
/// | 今天 | **函数体提到 `self.0` 的每一个 `fn`** | —— |
///
/// ★ 为什么这一版对：**「谁碰了 `self.0`」是一个内在事实，而「它长什么样」有无穷多种写法。**
/// 从「拼法」→「签名」→「函数体」，每一步都是**判据在贴着性质走，而不是贴着语法走**。
///
/// # ⚠ 它**仍然**认不出什么（诚实边界，别读成「明文不可能出去」）
///
/// 1. **不经 `self.0` 的路**：有人在别处 `impl` 一个 trait 把明文带出去 ——
///    那不在这个块里，由 `the_type_has_no_second_impl_block_that_hands_the_inner_string_out` 兜。
/// 2. **分类是人写的**：机器只判「谁进人群」与「`HandsOut` 有几条」，
///    判不了「一条标成 `ReadsOnly` 的函数是不是真的只读」。
///    ⇒ 配一条结构性纵深：块里**任何函数都不许有 `&mut` 出参**（出参正是明文外流的载体）。
/// 3. 宏生成的方法 —— 本类型不用宏，今天没有这一形。
#[test]
fn every_fn_touching_the_inner_field_is_classified() {
    let prod = production();
    let at = guard_core::find_pinned(&prod, "impl SecretKey {")
        .expect("切不出 `impl SecretKey` —— 本条按红处理，不是绿");
    let block = brace_block(&prod, at).expect("impl 块的花括号没配平 —— 按红处理");
    assert!(
        !block.contains("impl fmt::Debug"),
        "impl 块的窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
    );

    let fns = fns_in_block(block);
    assert!(
        fns.len() >= 6,
        "`impl SecretKey` 里只切出 {} 个 fn —— 取法坏了，本条在空转",
        fns.len()
    );

    // ① 人群 = 函数体提到 `self.0` 的那些。**默认拒绝。**
    let touchers: Vec<&String> = fns
        .iter()
        .filter(|(_, _, body)| body.contains("self.0"))
        .map(|(n, _, _)| n)
        .collect();
    assert!(
        touchers.len() >= 4,
        "只有 {} 个 fn 碰了内层字段 —— 取法坏了：{touchers:?}",
        touchers.len()
    );
    let registered: Vec<&str> = INNER_FIELD_USERS.iter().map(|(n, _, _)| *n).collect();
    for name in &touchers {
        assert!(
            registered.contains(&name.as_str()),
            "`impl SecretKey` 里多了一个碰内层明文的 fn：`{name}`，**没有登记**。\n\
                 ⚠ 登记时要说清它对明文做了什么：`HandsOut`（原样交出去）/ `Derives`（派生出不含明文的东西）/ \n\
                 `ReadsOnly`（只读一个与内容无关的事实）。\n\
                 `HandsOut` 那一类**条数是钉死的**，多一条必须先在件计划里说清那一处是什么。\n\
                 今天登记的是：{registered:?}"
        );
    }
    // ② 反向：表里不许留死名字。
    for (name, _, _) in INNER_FIELD_USERS {
        assert!(
            touchers.iter().any(|n| n.as_str() == *name),
            "登记表里的 `{name}` 已经不碰 `self.0` 了 —— 表和盘对不上，删了它"
        );
    }
    assert_eq!(
        touchers.len(),
        INNER_FIELD_USERS.len(),
        "人群与登记表条数不等"
    );

    // ③ ★ **区分那一格**：`HandsOut` 恰好 2 条，而且就是那两个具名出口。
    let hands_out: Vec<&str> = INNER_FIELD_USERS
        .iter()
        .filter(|(_, h, _)| *h == Handling::HandsOut)
        .map(|(n, _, _)| *n)
        .collect();
    assert_eq!(
        hands_out,
        vec!["expose_for_auth_header", "expose_for_persisting"],
        "把明文原样交出去的 fn 变了。**这一格是本判据的全部意义** —— \n\
             把一个新出口标成 `ReadsOnly` 混进表里，正是这张表最容易退化成的样子。"
    );
    // ④ 每一行都要写得出理由（空理由 = 「谁红了就加一行」的症状）。
    for (name, _, why) in INNER_FIELD_USERS {
        assert!(why.len() > 8, "`{name}` 这一行没写理由");
    }

    // ⑤ 结构性纵深：块里**任何 fn 都不许有 `&mut` 出参**。
    //    出参是明文外流的载体，而它**没有返回类型**、也不必是 `pub` —— 分类那一关兜不住它。
    //    D2 复审那一刀（`fn f(&self, out: &mut String)`）正是这一形。
    for (name, sig, _body) in &fns {
        // ⚠ 扫的是**签名**不是函数体〔`D3` 阻塞二回修〕—— 理由见 `fns_in_block` 里那段。
        let Some(open) = sig.find('(') else { continue };
        let Some(close) = sig[open..].find(')') else {
            continue;
        };
        let params = &sig[open..open + close];
        let bad = params
            .match_indices("&mut ")
            .any(|(i, _)| !params[i + 5..].trim_start().starts_with("self"));
        assert!(
            !bad,
            "`{name}` 收了一个 `&mut` 出参：`{params}`。\n\
                 ⚠ 出参是明文外流的载体，而它**没有返回类型**、也不必是 `pub` ——\n\
                 「按返回类型画人群」那一版就是被这一形绕过去的（D2 复审实打，端到端全绿）。"
        );
    }
}

/// `impl SecretKey` 里**每一个能把字符串带出去的 `pub fn`**，及它为什么可以存在。
///
/// **默认拒绝**：不在这张表里的当场红。⚠ 加一行**就是在放宽**，
/// 而 `KS2` 逐字：「真要多一处，**必须在件计划里单独说清那一处是什么**」。
const STRING_RETURNING_EXITS: &[(&str, &str)] = &[
    (
        "expose_for_auth_header",
        "唯一的**换头**出口。只出现在 daemon 生产段，相等断言 == 1",
    ),
    (
        "expose_for_persisting",
        "唯一的**落盘**出口。只出现在 creds-core 生产段，相等断言 == 1",
    ),
    (
        "masked",
        "**不是明文出口** —— 它回的是遮蔽形，由 `masking_keeps_only_the_two_ends_and_swallows_short_keys_whole` 钉着。\
             登记它是因为它的返回类型装得下字符串，而本判据的人群是**按返回类型**画的（宁可多问一句）",
    ),
];

/// ★★★ **`KS2` 全断的正主〔D1 阻-1 回修，08-27〕：按「函数」数，不按「某一种拼法」数。**
///
/// # 它替掉了什么，以及为什么非替不可
///
/// 上一版是 `block.matches("&self.0").count() == 2` —— **性质是「不许有第三个出口」，
/// 而人群是「`&self.0` 这一个字面出现几次」**。D1 审计一刀就绕过去了（PM 08-27 16:44 复打）：
/// 加 `pub fn d1_probe_third_exit(&self) -> &str { self.0.as_str() }`
/// ⇒ **creds-core 20 passed / 8 包合计 1284 passed / daemon 474 passed，一条都没红**。
/// 「把内层字段交出去」的写法至少还有 `self.0.as_str()` · `&*self.0` · `&self.0[..]` ·
/// `self.0.clone()` —— **换一种写法就绕过去，那不是判据，是巧合**。
///
/// # 今天的人群怎么画的（分母写清楚）
///
/// 人群 = `impl SecretKey` 块里**每一个返回类型能装下字符串的 `pub fn`**
/// （返回 `&str` / `String` / `&String`）。**按返回类型画，不按函数体怎么写画** ——
/// 函数体的写法有无穷多种，返回类型只有可数的几种，而「明文出得去」这件事
/// **必然经过返回类型**（`&self` 方法要把内层字符串带出去，只能从返回值走）。
///
/// ⚠ **它认不出什么**（诚实边界，别读成「明文不可能出去」）：
/// 1. 返回**别的类型**而里面裹着明文（`Vec<u8>` / 一个自定义结构体 / `impl Iterator<Item=char>`）——
///    本判据看不见。今天 `impl` 块里没有这一形（下面那条自检数了返回类型的总数）。
/// 2. `pub` 之外的可见性（`pub(crate) fn`）—— 本判据只数 `pub fn`；
///    但 crate 外拿不到它，而 crate 内只有 `store.rs` 一个消费者（由 §全断那条钉着）。
/// 3. 有人给 `SecretKey` **在别处**写 `impl Deref<Target=str>` —— 那不在这个 `impl` 块里。
///    这一形由下面 `the_type_has_no_second_impl_block_that_hands_the_inner_string_out` 兜。
#[test]
fn every_string_returning_exit_is_registered_by_name() {
    let prod = production();
    let at = guard_core::find_pinned(&prod, "impl SecretKey {")
        .expect("切不出 `impl SecretKey` —— 本条按红处理，不是绿");
    let block = brace_block(&prod, at).expect("impl 块的花括号没配平 —— 按红处理");
    assert!(
        !block.contains("impl fmt::Debug"),
        "impl 块的窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
    );

    // 人群：块里每一个 `pub fn <名>(...) -> <返回类型>`。
    let mut string_returning: Vec<String> = Vec::new();
    let mut all_pub_fns = 0usize;
    for seg in block.split("pub fn ").skip(1) {
        all_pub_fns += 1;
        let name = seg
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or("")
            .to_string();
        // 签名 = 到函数体左花括号为止。
        let Some(sig_end) = seg.find('{') else {
            continue;
        };
        let sig = &seg[..sig_end];
        let ret = sig.split("->").nth(1).unwrap_or("").trim();
        // 返回类型能装下字符串的那几种。
        if ret.contains("str") || ret.contains("String") {
            string_returning.push(name);
        }
    }
    // 采集面自检：块里确实有一批 `pub fn`（切歪了就不是「零个出口」而是「没扫到」）。
    assert!(
        all_pub_fns >= 5,
        "`impl SecretKey` 里只扫到 {all_pub_fns} 个 `pub fn` —— 取法坏了，本条在空转"
    );

    let registered: Vec<&str> = STRING_RETURNING_EXITS.iter().map(|(n, _)| *n).collect();
    for name in &string_returning {
        assert!(
            registered.contains(&name.as_str()),
            "`impl SecretKey` 里多了一个能把字符串带出去的 `pub fn`：`{name}`。\n\
                 ⚠ 它**没有登记** —— `KS2` 逐字：加行是收紧、动断言是放宽，\n\
                 真要多一处**必须先在件计划里说清那一处是什么**，不许在实现里顺手把表加大。\n\
                 今天登记的是：{registered:?}"
        );
    }
    // 反向：登记表里不许留死名字（改名了就来改表）。
    for (name, _) in STRING_RETURNING_EXITS {
        assert!(
            string_returning.iter().any(|n| n == name),
            "登记表里的 `{name}` 在 `impl SecretKey` 里找不到了 —— 表和盘对不上"
        );
    }
    assert_eq!(
        string_returning.len(),
        STRING_RETURNING_EXITS.len(),
        "能把字符串带出去的 `pub fn` 有 {} 个，登记了 {} 个：{string_returning:?}",
        string_returning.len(),
        STRING_RETURNING_EXITS.len()
    );
}

/// ★ 上一条那条诚实边界第 3 形的兜底：**本文件里不许有第二个 `impl … for SecretKey`
/// 把内层字符串交出去**（`Deref` / `AsRef<str>` / `Borrow<str>` 都是一句话就能开的后门）。
///
/// 今天只许有一个 `impl SecretKey`（固有方法）+ 一个 `impl fmt::Debug for SecretKey`。
#[test]
fn the_type_has_no_second_impl_block_that_hands_the_inner_string_out() {
    let prod = production();
    // ⚠ 人群**按 impl 这个 item 的「头」画，不按「`impl ` 那一行」画**〔D2 回修，08-27〕。
    //   先前是 `prod[i..].lines().next()` —— **只取一行**，于是
    //       impl std::ops::Deref
    //           for SecretKey
    //       { … }
    //   这种把头断成两行的写法**整个看不见**，而本条的报文逐字点名 `Deref` 是它要挡的那一形。
    //   D2 复审实打：那一刀 creds-core **23 passed; 0 failed**（我自己复打确认）。
    //   今天取的是「从 `impl ` 到它的左花括号为止」——那才是这个 item 的头，换行不影响。
    let impls: Vec<String> = prod
        .match_indices("impl ")
        .filter_map(|(i, _)| {
            let head = &prod[i..i + prod[i..].find('{')?];
            head.contains("SecretKey")
                .then(|| head.split_whitespace().collect::<Vec<_>>().join(" "))
        })
        .collect();
    assert!(
        impls.len() >= 2,
        "只扫到 {} 个与 `SecretKey` 有关的 impl —— 取法坏了，本条在空转：{impls:?}",
        impls.len()
    );
    // 分母 = 我登记的这 2 个 impl 头；多一个就红，**由人来说清那一个是什么**。
    // 规范化之后的头（空白已折叠成单空格）⇒ 换行、多空格都不影响。
    let allowed = ["impl SecretKey", "impl fmt::Debug for SecretKey"];
    for head in &impls {
        assert!(
            allowed.contains(&head.trim()),
            "`SecretKey` 多了一个 impl：`{}`。\n\
                 ⚠ `Deref<Target = str>` / `AsRef<str>` / `Borrow<str>` 这一类**一句话就是一个明文后门**，\n\
                 而按返回类型画人群的那条判据**看不见它们**（它们不在 `impl SecretKey` 块里）。",
            head.trim()
        );
    }
}

/// ★★ **`KS2` 的「全断」那一格**：明文的出口**恰好两个**，而且都得具名。
///
/// 上面 `secret_key_has_no_printing_shortcut` 守的是「不许有顺手打印的路」，
/// 本条守的是另一件事：**不许有第三个出口**。两条缺一都不成立 ——
/// 只有前者 ⇒ 加一个 `pub fn raw(&self) -> &str` 照样绿；
/// 只有后者 ⇒ `derive(Debug)` 照样把明文印出去。
///
/// 量法：在 `impl SecretKey` 的**函数体窗口**里数「返回内层字段」的写法。
/// ⚠ 窗口是**花括号配平切出来的 impl 块**，不是 `[\s\S]*?`（`KP4` 那两个价钱之一）。
#[test]
fn the_plaintext_has_exactly_two_named_exits() {
    let prod = production();
    let at = guard_core::find_pinned(&prod, "impl SecretKey {")
        .expect("切不出 `impl SecretKey` —— 本条按红处理，不是绿");
    let block = brace_block(&prod, at).expect("impl 块的花括号没配平 —— 按红处理");
    // 反空真自检：窗口不许跨进下一个 item。
    assert!(
        !block.contains("impl fmt::Debug"),
        "impl 块的窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
    );
    assert!(
        block.len() > 400,
        "窗口只有 {} 字节 —— 切法坏了",
        block.len()
    );

    // 「把内层字段原样交出去」的唯一写法。
    let exits = block.matches("&self.0").count();
    assert_eq!(
        exits, 2,
        "`SecretKey` 交出明文的地方有 {exits} 处，登记的是 **2** 处：\n\
             · `expose_for_auth_header` —— 中转往上游请求写鉴权头（只在 daemon 生产段）\n\
             · `expose_for_persisting`  —— 把它写回那份文件（只在 creds-core 生产段）\n\
             多一处 ⇒ **必须先在件计划里说清那一处是什么**（`KS2` 逐字：\
             加行是收紧、动断言是放宽，不许在实现里顺手把断言改大）"
    );
    // 两个名字都得真在（否则上面那个 2 可能来自两处匿名写法）。
    for name in ["fn expose_for_auth_header", "fn expose_for_persisting"] {
        assert_eq!(
            block.matches(name).count(),
            1,
            "`{name}` 在 impl 块里应当恰好定义一次"
        );
    }
}

#[test]
fn a_debug_print_never_carries_the_plaintext_or_its_length() {
    let k = SecretKey::new("sk-ant-THIS-MUST-NOT-BE-PRINTED");
    let printed = format!("{k:?}");
    assert!(
        !printed.contains("THIS-MUST-NOT-BE-PRINTED"),
        "`{{:?}}` 把明文印出来了：{printed}"
    );
    // 非空对照：它确实印了**点什么**（不是空串恒过）。
    assert_eq!(printed, "SecretKey(<已遮蔽>)");
    // 长度也不许漏 —— 一把 key 的长度能把候选集缩小一个量级。
    assert!(
        !printed.contains(&k.len().to_string()),
        "`{{:?}}` 把长度印出来了：{printed}"
    );
}

#[test]
fn masking_keeps_only_the_two_ends_and_swallows_short_keys_whole() {
    let long = SecretKey::new("sk-ant-api03-ABCDEFGHIJKLMNOP");
    let m = long.masked();
    assert!(m.starts_with("sk-a"), "前 4 位应当留着，实得 {m}");
    assert!(m.ends_with("MNOP"), "后 4 位应当留着，实得 {m}");
    assert!(!m.contains("api03-ABCDEFGHIJKL"), "中段没被遮住：{m}");
    assert_eq!(
        m.chars().count(),
        "sk-ant-api03-ABCDEFGHIJKLMNOP".chars().count()
    );

    // ★ 短 key **整条遮掉**：留前后各 4 位等于把一把 8 字符的 key 交出去。
    let short = SecretKey::new("abcdefgh");
    assert_eq!(short.masked(), "********");
    assert!(
        !short.masked().contains("abcd"),
        "短 key 的前缀漏出去了：{}",
        short.masked()
    );
    // 非空对照：空的就是空的，不是一串星号。
    assert_eq!(SecretKey::new("   ").masked(), "");
}

#[test]
fn is_configured_looks_at_presence_not_at_content() {
    assert!(!SecretKey::new("").is_configured());
    assert!(!SecretKey::new("  \t\n ").is_configured());
    assert!(SecretKey::new("x").is_configured());
}
