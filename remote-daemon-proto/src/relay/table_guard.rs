//! `K-H2` `KH1` 的机检那一半：**「这条路由发到哪儿、用哪把 key」这个决定只有一处做得了。**
//!
//! # 为什么单住一个文件
//!
//! 与隔壁 `creds_guard.rs` / `bind_guard.rs` / `nodelay_guard.rs` 同一个理由，
//! 它们的头注逐字写着：扫描型判据要走 `guard_core::scan_tree!`，而那个宏
//! **按构造摘掉调用者自己那一份** ⇒ **判据与被扫的代码必须不在同一个文件**，
//! 否则「摘掉自己」正好把靶子摘了。本模块扫的是 `table.rs`，所以它不能住 `table.rs`。
//!
//! # ★★ 「决定点」这个人群怎么定义（件计划点名要答的那一问）
//!
//! **人群 = 后端生产段里每一处能决定「这条请求发到哪个上游」或「用哪把 key」的代码。**
//! 它落成**三条腿**，每条腿都是一个**相等断言**，每条都说得出分母：
//!
//! | 腿 | 它决定什么 | 谁钉的 | 恰好几处 |
//! |---|---|---|---|
//! | ㈠ `Row { base` 字面量 | **哪个上游配哪把 key**（焊接那一步） | 本文件 | **1** |
//! | ㈡ `upstream::connect(` 调用点 | **这条请求实际连到哪儿** | 本文件 | **1** |
//! | ㈢ `expose_for_auth_header(` 调用点 | **换上去的是哪把 key** | `creds_guard.rs`（`KS2`，`K-H2a` 就有） | **1** |
//!
//! ⇒ 三条加起来才是「决定点」。**少一条都不是** ——
//! 只钉㈠，有人可以另开一条 `upstream::connect` 绕过表；只钉㈡，
//! 有人可以在别处再焊一对不同源的上游与 key。
//!
//! # ⚠⚠⚠ 三条加起来**也不是买断** —— `D1` 一刀穿过，读数逐条记这里
//!
//! 本文件先前的头注（与 `table.rs`、`server.rs` 那两处）把这三条腿说成
//! 「编译器买大半、判据只守剩下的一格」。**`D1` 逐条编出反例，三条腿 + 那条棘轮一条都没红：**
//!
//! | `D1` 的刀 | 它绕过了谁 | 读数 |
//! |---|---|---|
//! | `D1-M1` | ㈡（改写调用写法：`use … as dial` + 用 `host_header()` 重建 `Base`） | **488 passed / 0 failed** |
//! | `D1-M2` | 「同源」那句（两行在作用域，A 连 B 渲染） | **编译通过**；只红一条行为断言 |
//! | `D1-M4` | ㈠（**一个字面量焊出多行** + 把回落加回来） | `table_guard` 单跑 **10 passed / 0 failed** |
//!
//! ⇒ **本文件三条腿数的都是「语法处数」，不是「决定的条数」。**
//! 它们买到的是**「多一处显眼的改动会被逼着说清楚」**，
//! **不是**「这条性质不可能被破坏」。真正拦住上面那几形的是
//! `KH2`/`KH4` 那几条**走真子进程、真转发**的行为判据。
//! ★ 这一课与 `K-H2a` 七轮的那条是孪生：那次是「**有判据 ≠ 保证了**」，
//! 这次是「**读着像买断 ≠ 买断了**」——差的那一格叫**量过没有**。
//!
//! # ⚠ 本文件**排除了**什么（别让它替这些背书）
//!
//! 1. **不排除「表里两行填了同一个上游」** —— 那是文件内容，不是代码。
//! 2. **不排除「表本身被人填错」** —— 同上，那是人的活。
//! 3. **不排除「`Row` 造出来之后被改」** —— 那由**字段私有 + 没有任何 setter** 兜，
//!    是**编译器**买的，不是本文件买的。
//! 4. **不排除「两行同时在作用域里，有人拿 A 连、拿 B 渲染」** ——
//!    今天 `handle` 里只有一行在作用域，这一格靠的是**那个事实**，不是类型。如实记着。
//! 5. 针是**子串**，不是语法。把构造写成 `Row { key: k, base: b }`（字段反序）
//!    本文件的㈠数不到 —— 而 `mod sealed` **之外**那一半确实由编译器兜（字段私有，
//!    外面写不出任何一种构造）。
//!    ⚠⚠ **但先前这里由此下的那个结论说反了**：原话是「⇒ 本文件真正的人群其实是
//!    `sealed` 里面」，读起来像「里面这一格有人守着」。**`D1-M4` 实测证伪** ——
//!    `sealed` 里面**一个字面量就能焊出任意多行**（它长在循环里），
//!    「焊了几行、每行装什么」这根针一个字都数不出来：把回落整个加回来，
//!    `table_guard` 单跑 **10 passed / 0 failed**。
//!    ⇒ **准确说法：㈠ 在 `sealed` 外面靠编译器、在 `sealed` 里面几乎什么都不守。**
//! 6. ⚠ **不排除「表里那几行本身是错的」** —— 本文件一条都不看表的**内容**。
//!    那一格全部由 `KH2`/`KH4` 那几条**走真转发**的行为判据承担。

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// 从 `at` 之后第一个 `{` 起，切出配平的花括号块（不含首尾那对括号）。
    ///
    /// ⚠ 它**不认识字符串字面量与注释里的花括号**（同 `creds-core` 与 `creds_store.rs`
    /// 里那两份同形的实现）。用它之前先确认窗口里没有那种东西，
    /// 并且**每一处用它的地方都配一条「窗口没跨进下一个 item」的反空真自检**。
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
                        return Some(&src[open + 1..i]);
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// 整个后端 crate 的生产段（逐文件）。
    ///
    /// ★ 人群是**整个 crate**，不是 `relay/` —— 与 `creds_guard::crate_production` 同一条理由：
    /// 「决定点只有一处」这句话的分母如果只到 `relay/`，那么有人在 `observe/` 里
    /// 再开一条上游连接就不会红。**人群要恰好等于性质。**
    fn crate_production() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        guard_core::scan_tree!(&dir, &["rs"])
            .into_iter()
            .map(|(p, raw)| (p.display().to_string(), production_code(&raw)))
            .collect()
    }

    /// 在整个 crate 的生产段里逐行找一根针，返回 `文件:行号` 的列表。
    ///
    /// ⚠ **那个行号是「剥掉测试段之后」的行号，不是源文件的行号**（`production_code`
    /// 会把 `#[cfg(test)]` 块整块拿掉）。诊断里拿它去 `sed -n 'Np'` 会看到别的东西
    /// —— 承重的是**文件名与处数**，行号只当线索。〔本轮变异实测时亲眼撞到：同一处在
    /// 剥后的编号，比它在源文件里的行号小了整整一个被剥掉的头注段。〕
    fn sites(files: &[(String, String)], needle: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (path, prod) in files {
            for (no, line) in prod.lines().enumerate() {
                if line.contains(needle) {
                    out.push(format!("{path}:{}", no + 1));
                }
            }
        }
        out
    }

    /// `table.rs` 的生产段里 `mod sealed` 那一块。
    ///
    /// **切不出来 ⇒ 按红处理，不是绿**（`K-H2a` 那一件逐字定下的纪律）。
    fn sealed_block(prod: &str) -> String {
        let at = guard_core::find_pinned(prod, "mod sealed {")
            .expect("切不出 `mod sealed` —— 本条按红处理，不是绿");
        let block = brace_block(prod, at).expect("`mod sealed` 的花括号没配平 —— 按红处理");
        // 反空真自检：真的切到了那一块（它里面必须有 `Row` 的声明），
        // 而且**没有跨进下一个 item**（`pub(crate) use sealed::` 在 `mod sealed` 之后）。
        assert!(
            block.contains("pub(crate) struct Row {"),
            "切出来的窗口里没有 `Row` 的声明 —— 取法坏了，下面的断言在空转"
        );
        assert!(
            !block.contains("pub(crate) use sealed::"),
            "`mod sealed` 的窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
        );
        block.to_string()
    }

    fn table_production() -> String {
        let files = crate_production();
        let (_, prod) = files
            .iter()
            .find(|(p, _)| p.ends_with("table.rs"))
            .expect("扫不到 `table.rs` —— 取法坏了，本条按红处理");
        prod.clone()
    }

    /// ★★★ **㈠ 焊接那一步只有一处**：把「一个上游」与「一把 key」焊成同一个值，
    /// 整个后端生产段里**恰好 1 处**，而且在 `table.rs` 的 `mod sealed` 里。
    ///
    /// # 它守的性质，与它的人群
    ///
    /// 性质：**「哪个上游配哪把 key」这个决定只有一个地方做得了。**
    /// 人群：整个 crate 生产段里 `Row { base` 这根针的行数。
    ///
    /// ⚠⚠ **分母如实写**：这根针是**子串**。写成 `Row { key: k, base: b }`（字段反序）
    /// 它数不到 —— 但那种写法**在 `mod sealed` 之外根本编不过**（字段私有）。
    /// ⇒ 本条真正管的是 **`sealed` 里面有没有第二处**；
    /// **外面一处都写不出来那一半，是编译器买的，不是本条买的。**
    ///
    /// # ⚠⚠⚠ 而它在「`sealed` 里面」那一格**恰恰最弱**（`D1-M4` 实测）
    ///
    /// 上面那句「本条真正管的是 `sealed` 里面」读起来像一句承诺，**它不是**。
    /// 根子在于：**一个字面量能焊出任意多行** —— 它长在一个循环里，
    /// 「焊了几行、每行装的是什么」**这根针一个字都数不出来**。
    /// `D1-M4` 实测：在 `sealed` 里把回落整个加回来（同一个字面量多焊几行，
    /// 包括一行「什么键都匹配」的），**`Row { base` 仍然恰好 1 处**
    /// ⇒ `table_guard` 单跑 **10 passed / 0 failed**，本条**一声不吭**。
    ///
    /// ⇒ **准确的说法**：本条数的是**语法处数**，不是**决定的条数**。
    /// 它挡得住「有人在别的文件里另起一处焊接」，挡不住「在这一处里焊出一张错的表」。
    /// **后者今天由 `KH2`/`KH4` 那几条行为判据守**（它们量的是真转发落到哪个端点、带了哪把 key），
    /// 不由本条守。别把「恰好 1 处」读成「表一定是对的」。
    #[test]
    fn the_only_place_that_welds_an_upstream_to_a_key_is_inside_the_sealed_module() {
        let files = crate_production();
        // 采集面自检：整个 crate 的 .rs 不止几个（相等地板会误伤，这里用有理由的下界）。
        assert!(
            files.len() >= 30,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );

        // 反空真：声明恰好一处（没有它，下面那个 1 可能是「针把声明数进来了」）。
        let decls = sites(&files, "pub(crate) struct Row {");
        assert_eq!(
            decls.len(),
            1,
            "`Row` 的声明不是恰好一处：{decls:?} —— 取法坏了或有人加了第二个类型"
        );

        let welds = sites(&files, "Row { base");
        assert_eq!(
            welds.len(),
            1,
            "把「一个上游」与「一把 key」焊在一起的地方有 {} 处，应当**恰好 1** 处：{welds:?}\n\
             ⚠ 多一处，就多一个能各自答错的地方，而它们答的是同一个问题：\n\
             「这条路由发到哪儿、用哪把 key」。\n\
             真要多一处，**必须先在件计划里说清那一处是什么**，不许在实现里把这个数改大。",
            welds.len()
        );
        assert!(
            welds[0].contains("table.rs"),
            "唯一那一处不在 `table.rs` 而在 {} —— 靶子挪了",
            welds[0]
        );

        // ★ 它必须在 `mod sealed` **里面**：在外面就等于字段不再私有，整件事作废。
        let sealed = sealed_block(&table_production());
        assert_eq!(
            sealed.matches("Row { base").count(),
            1,
            "`mod sealed` 里的焊接点不是恰好一处 —— 编译器管得住外面，管不住里面，\n\
             `sealed` 里面这一格只有本条守着。"
        );
    }

    /// ★★★ **㈡ 开上游连接的地方只有一处**，而且它是 `Row` 自己的方法。
    ///
    /// # 没有这一条会漏掉什么（这就是它不是冗余的理由）
    ///
    /// 只钉㈠的话，有人完全可以在 `handle` 里再写一句 `upstream::connect(&some_base)`，
    /// **绕过整张表**把请求发出去 —— 那时㈠仍然是 1，而「路由表说了算」已经不成立。
    ///
    /// ⚠ 分母：针是 `upstream::connect(`。`upstream.rs` 里那个**定义**
    /// （`pub(crate) fn connect(`）不带模块前缀 ⇒ 不进人群，这是有意的。
    ///
    /// # ⚠⚠ 它认得出什么、认不出什么 —— **逐写法带读数**
    ///
    /// 针是**子串** `upstream::connect(`，而 Rust 有一整族改写调用写法的办法。
    /// ⚠⚠ **订正〔`D2` `§五-1` 逮到，`C-补` 08-28 逐形重打〕**：先前这里把三形**一并**列成
    /// 「同族绕过写法」，**其中一形是错的** —— 全路径那一形本条**抓得住**。
    /// 下面每一格要么是读数、要么写清读数是谁打的，**没有一格是「读源码得出的形状」**：
    ///
    /// | 写法 | 针数得到吗 | 本条 | 读数（**分母 488**；来源逐格标注） |
    /// |---|---|---|---|
    /// | `use super::upstream::connect as dial;` 再 `dial(&rebuilt)` | ❌ **零命中**（`connect` 后面没有 `(`） | **认不出** | `D1-M1` 实测：`upstream::connect(` 仍是 1 处 ⇒ **488 passed / 0 failed**。⚠ 这一格是 `D1` 的读数，`C-补` **没有重打** |
    /// | `crate::relay::upstream::connect(…)`（**全路径**） | ⭐ **数得到** —— 全路径**含**这个子串 | ⭐ **抓得住** | `C-补` 自己重打（在 `table.rs` 里插一行全路径调用）：本条**当场红**，判定行逐字「**开上游连接的地方有 2 处，应当\*\*恰好 1\*\* 处**」并点名两个住址。suite **470 passed / 18 failed** —— ⚠ 其中 **17 条是 `relay::server` 的行为判据**，因为这一刀真的多开了一条连接（与本条无关）；**本条是第 18 条**。`D2` `P1` 同向，读数 487+1（它那一刀没产生行为侧的连带） |
    /// | 先 `let f = upstream::connect;` 再 `f(…)` | ❌ 绑定那一行 `connect` 后面没有 `(` | **认不出** | `C-补` 自己重打（切成**不执行**的形状，让唯一的变量是文本）：针仍是 1 处 ⇒ **488 passed / 0 failed**。与 `D2` `P2` 逐字相同 |
    ///
    /// ⚠⚠ **「全路径」那一格先前被写成「绕过写法」，是把守卫说得比它实际弱。**
    /// 把守卫说得比实际弱，与说得比实际强**同样是话与实测对不上**，只是危害方向相反：
    /// 前者会让下一个人以为「反正也守不住」，干脆**放弃这条判据**。
    ///
    /// ⚠ 顺带一条**分母纪律**（这一轮重打时现看见的）：源文件里 `upstream::connect(` 裸 `grep -c`
    /// 数出来是 **3**，而本条数出来是 **1** —— 差的两处都在**注释**里，
    /// `crate_production()` 会把注释剥掉（`guard_core` 那条 `strips_line_comments_so_prose_cannot_feed_a_guard`
    /// 就是为这个立的）。⇒ **拿裸 `grep` 的数来对本条的数，尺子的作用域对不上。**
    ///
    /// ⇒ **本条守的是「有没有第二个显眼的调用点」，不是「有没有第二条连出去的路」。**
    /// 后一句今天**没有任何东西守着** —— 如实登记，别把它读进本条的名字里。
    #[test]
    fn the_only_place_that_opens_an_upstream_connection_is_a_table_row() {
        let files = crate_production();
        assert!(files.len() >= 30, "只扫到 {} 个文件 —— 取法坏了", files.len());

        let calls = sites(&files, "upstream::connect(");
        assert_eq!(
            calls.len(),
            1,
            "开上游连接的地方有 {} 处，应当**恰好 1** 处：{calls:?}\n\
             ⚠ 第二处就是一条**绕过路由表**的路：它决定了请求实际连到哪儿，\n\
             而那正是本件要收进表里的那个决定。",
            calls.len()
        );
        assert!(
            calls[0].contains("table.rs"),
            "唯一那一处不在 `table.rs` 而在 {} —— 靶子挪了",
            calls[0]
        );
        let sealed = sealed_block(&table_production());
        assert_eq!(
            sealed.matches("upstream::connect(").count(),
            1,
            "`mod sealed` 里开上游连接的地方不是恰好一处"
        );

        // 非空对照：同一把尺子**数得到**别的东西（证明它不是恒返回一条）。
        let lookups = sites(&files, "table.lookup(");
        assert!(
            !lookups.is_empty(),
            "非空对照失败：同一把尺子连 `table.lookup(` 都数不到 —— 这把尺子是瞎的"
        );
    }

    /// ★★ `K-H2b` `D2 阻-4`：**那张表的读锁不许跨 `pump`。**
    ///
    /// `std::sync::RwLock` 是**写优先**的：一个在等的写者（= 用户刚配完一把 key，
    /// 下一条请求触发重载）会挡住其后所有读者。而 `pump` 是**流式转发**，
    /// 一条 SSE 长流可以跑几分钟 ⇒ 读锁跨 `pump` 的话，「配一次 key」会被堵在
    /// **最长那条在飞流**后面。
    ///
    /// # ⚠ 它是形状判据，不是行为判据（说清楚）
    ///
    /// 挂起时长**没实测**（那要造一条长流再去配 key）。
    ///
    /// # 它今天钉的到底是什么（`D4 阻-5`：**先前这一段描述的是它自己已经废弃的第一版**）
    ///
    /// 🔴 先前这里逐字写着「本条只钉那个**位置关系**：`table.read()` 必须出现在 `pump(`
    /// 之前，且中间必须有一处 `};`」——**那是第一版，而第一版被 `M32`（把绑定提到块外）
    /// 照绿了**（那个 `};` 在提出去之后仍在）。函数体当场就换成了钉**绑定的形状**，
    /// 而这段头注没跟着改 ⇒ 「改了事实没改说它的那句话」，这一次长在**判据自己身上**。
    ///
    /// **今天这一版钉的是一个确切的绑定形状**（逐字，含缩进）：
    /// `let mut up = {\n        let table = relay.table.read()` 在 `server.rs`
    /// 的生产段里**恰好 1 处**。⇒ 谁把守卫从那个块里提出去（让它活到函数结尾、跨整条
    /// `pump`），这个形状就不再命中，本条红。
    ///
    /// # ⚠ 射程（`D4 §G1`：它挡得住什么、挡不住什么，一起写）
    ///
    /// - **挡得住**：把绑定提到块外（`M32`，实测 `1 failed / 492 passed`）。
    /// - **挡不住**：把 `pump(` 搬进那个块里 —— 形状串照样命中 1、`relay.table.read()`
    ///   照样 1 处 ⇒ **本条绿，而性质已破**。⚠ 这一形**没实测**（要一次不小的重排），
    ///   如实记着，别把它读成「已排除」。
    /// - **误红的方向**：它钉的是**一串带 8 个空格缩进的源码字面**，一次**无害的重命名**
    ///   就会把它打红 —— `D4` 刀 F2 实测：块内局部变量 `table` 纯改名成 `tbl`
    ///   ⇒ **2 条红**（本条 + `the_only_place_that_opens_an_upstream_connection_is_a_table_row`
    ///   的非空对照连带）。**它不是最小面。**
    ///   ⚠ 那时的合法出路只有 `testing.md` 三.12 那两条（**重新裁定** 或 **补登记**）——
    ///   **不许把形状串放宽**（放宽不可逆）。真要改名，同一拍把这里的形状串一起改，
    ///   并在件文件里记一笔「这一刀是重命名，不是把守卫提出去」。
    #[test]
    fn the_routing_table_read_guard_does_not_outlive_the_streaming_pump() {
        let files = crate_production();
        let (_, server) = files
            .iter()
            .find(|(p, _)| p.ends_with("server.rs"))
            .expect("扫不到 `server.rs` —— 取法坏了，本条按红处理");

        // ⚠ **不做位置比较**（`structural_scan` 那条纪律：位置比较要么切段、要么核唯一性，
        //    而这一格根本不需要位置 —— 它要的是**一个确切的绑定形状**）。
        //    第一版写成「`read()` 在 `pump(` 之前，且两者之间有个 `};`」，
        //    被一次「把绑定提到块外」的变异**照绿**（`M32` 实测）⇒ 换成钉那个形状本身。
        let shape = "let mut up = {\n        let table = relay.table.read()";
        assert_eq!(
            server.matches(shape).count(),
            1,
            "读锁的守卫不再**绑在那个块里**（实得 {} 处该形状）—— \n\
             它会活到函数结尾、跨整条 `pump`。`RwLock` 写优先 ⇒ \n\
             用户配一次 key 会被堵在**最长那条在飞流**后面。",
            server.matches(shape).count()
        );
        // 反空真：`pump(` 与 `read()` 都真的在这份生产段里（否则上面那条可能只是巧合成立）。
        assert_eq!(
            server.matches("relay.table.read()").count(),
            1,
            "取读锁的地方不是恰好一处 —— 锚点不唯一，本条的结论不算数"
        );
        assert!(
            server.contains("let outcome = pump("),
            "扫到的 `server.rs` 里没有 `pump(` —— 取法坏了，本条按红处理"
        );
    }

    /// ★★ **棘轮：`Relay` 里不许再有「进程级的上游」或「进程级的 key」。**
    ///
    /// # 它守的不是一条新性质，是**一条已经买到的性质不许被退回去**
    ///
    /// 「进程里没有默认上游可回落」今天是**编译器**兜的 —— 因为那个值**不存在**。
    /// 而「不存在」这件事本身，就是 `struct Relay` 里少了两个字段而已：
    /// 谁把它们加回来，回落就又变得写得出来了，**而且不会有任何东西红**。
    /// ⇒ 本条是那一格的棘轮。〔`K-H2a` 的 `E` 阶段实打过同一形：
    /// 「daemon 不开 `harden`」五轮买来的性质整个挂在 manifest 一行上，而没人看着它。〕
    ///
    /// ⚠ 窗口是 `struct Relay {` 那一块，**有界**，并配两条反空真自检。
    ///
    /// ⚠⚠ **这段头注 `D4 阻-5` 之前被隔壁那条新判据挤掉了**（新判据插进了本段与本条的
    /// `#[test]` 之间）⇒ 那一段时间里，**本条一句头注都没有，而隔壁那条顶着一段讲本条的话**。
    /// ★ 这是「改 A 而 B 无声受损」的一形，**没有任何判据会红** —— 加判据不是这一格的出路
    /// （「每条 `#[test]` 都要有头注」是一条会被空文档喂饱的判据）；出路是**收工前自己回头看
    /// 一眼这一轮有没有制造同形**。经过写在这里，别再删。
    #[test]
    fn the_relay_carries_no_process_wide_upstream_and_no_process_wide_key() {
        let files = crate_production();
        let (_, server) = files
            .iter()
            .find(|(p, _)| p.ends_with("server.rs"))
            .expect("扫不到 `server.rs` —— 取法坏了，本条按红处理");

        let at = guard_core::find_pinned(server, "pub(crate) struct Relay {")
            .expect("切不出 `struct Relay` —— 本条按红处理，不是绿");
        let block = brace_block(server, at).expect("`struct Relay` 的花括号没配平 —— 按红处理");

        // 反空真自检㈠：真的切到了那一块（它里面必须有这两个今天确实在的字段）。
        assert!(
            block.contains("tee:") && block.contains("served:"),
            "切出来的窗口里没有 `tee:` / `served:` —— 取法坏了，下面的断言在空转"
        );
        // 反空真自检㈡：窗口没有跨进下一个 item。
        assert!(
            !block.contains("impl Relay"),
            "`struct Relay` 的窗口跨进了下一个 item —— 窗口无界，下面的断言不算数"
        );

        for banned in ["base:", "key:"] {
            assert!(
                !block.contains(banned),
                "`Relay` 里出现了 `{banned}` —— 那是一个**进程级**的上游或 key。\n\
                 ⚠ `K-H2` `KH2`：有那个值，「查不到就回落到它」就又写得出来了，\n\
                 而最坏的失效形态正是**拿 A 账号的 key 去发 B 账号的请求**。\n\
                 今天这条性质是**编译器**兜的（那个值不存在），本条只是不许有人把它加回来。\n\
                 真要加，先在件计划里说清那个字段是什么、为什么它不是回落的目的地。\n\
                 窗口：{block}"
            );
        }
    }
}
