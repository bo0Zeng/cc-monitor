//! `K-H2` `KH1` 的机检那一半：**「这条路由发到哪儿、用哪把 key」这个决定只有一处做得了。**
//!
//! # 为什么单住一个文件
//!
//! 与隔壁 `creds_guard.rs` / `bind_guard.rs`（以及〔AR1〕已退役的 `nodelay_guard.rs`）同一个理由，
//! 它们的头注逐字写着：扫描型判据要走 `guard_core::scan_tree!`
//! ⇒ **判据与被扫的代码必须不在同一个文件**，否则「摘掉自己」正好把靶子摘了
//!（⚠ 那一刀经 `#[path]` 挂载时**不生效**，也就是本仓今天的全部判据 —— 详见下一段）。
//! 本模块扫的是 `table.rs`，所以它不能住 `table.rs`。
//!
//! ⚠ 〔`P4` 2026-09-21〕那三份头注里「那个宏**按构造摘掉调用者自己那一份**」这句
//! **在这一处不生效**（判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比不命中），
//! 三处都已订正。上面那条结论今天只对「判据写在被扫文件自己的 `#[cfg(test)]` 段里」
//! 那一形成立；本模块不在人群里靠的是住址（它住 `tests/backend/relay/`）。
//!
//! # ★★ 「决定点」这个人群怎么定义（件计划点名要答的那一问）
//!
//! **人群 = 后端生产段里每一处能决定「这条请求发到哪个上游」或「用哪把 key」的代码。**
//! 它落成**三条腿**，每条腿都是一个**相等断言**，每条都说得出分母：
//!
//! | 腿 | 它决定什么 | 谁钉的 | 恰好几处 |
//! |---|---|---|---|
//! | ㈠ `Row{base` 字面量（**去空白后**数，09-09 起） | **哪个上游配哪把 key**（焊接那一步） | 本文件 | **1** |
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
        let dir = crate::guard_support::src_root();
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

    /// 标识符字符 —— 口径抄 `guard_core::find_pinned` 里那份（它是私有的，这里同形复写）。
    fn ident_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '-'
    }

    /// 与 [`sites`] 同族，但**先把排版抹掉再找针**：把每份生产段的空白（含换行）
    /// 全部删掉，针也写成无空白形。返回**文件名**（一处一条）。
    ///
    /// # 为什么要有它〔09-09〕
    ///
    /// [`sites`] 是**逐行** `contains`：针一旦跨行就零命中。09-09 那趟 `cargo fmt --all`
    /// 把 `table.rs` 里唯一那处焊接从一行拆成了五行 ⇒ ㈠ 那条当场数出 **0 处**，
    /// 判定行逐字「把「一个上游」与「一把 key」焊在一起的地方有 0 处，应当**恰好 1** 处：[]」。
    ///
    /// ★ 病不在 rustfmt，在**判据把排版当成了性质的一部分** ——
    /// 「哪个上游配哪把 key，只有一个地方决定得了」这句话里没有「在同一行上」这几个字。
    /// （`table.rs` 里那句「这一行**必须留在一行上**」的注释就是这条病留下的化石：
    /// 它拿**生产代码的排版**去迁就判据。今天判据不再要那一格了。）
    ///
    /// ⚠ **它比 [`sites`] 更逮得住**，不是更松：一处**写成多行**的第二个焊接点，
    /// 逐行那把尺子一个字都数不到（现打：老尺子 1 处、新尺子 2 处），本函数数得到。
    ///
    /// ⚠ 代价：**给不出行号**（行的边界被抹掉了）。承重的本来就是「文件名 + 处数」——
    /// [`sites`] 的头注逐字写着「行号只当线索」，所以这一格是可以换的。
    ///
    /// ⚠ **头部边界是承重的，不是装饰**：空白删干净之后
    /// `pub(crate) struct Row {` + `base: Base,` 会连成 `…structRow{base:Base,`，
    /// 那个**声明**于是也含着 `Row{base` 这根针。头部边界（针首字符是标识符字符时，
    /// 前一个字符不许是）把它挡在门外 —— 顺带也挡住 `SessionRow{base`
    /// 那种「被撑大的命中」（`F05`/`F16` 那一形：**匹配单位比事实小**）。
    fn sites_layout_blind(files: &[(String, String)], needle: &str) -> Vec<String> {
        let head_is_ident = needle.chars().next().is_some_and(ident_char);
        let mut out = Vec::new();
        for (path, prod) in files {
            let flat: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
            let mut from = 0usize;
            while let Some(rel) = flat[from..].find(needle) {
                let at = from + rel;
                let stretched =
                    head_is_ident && flat[..at].chars().next_back().is_some_and(ident_char);
                if !stretched {
                    out.push(path.clone());
                }
                from = at + flat[at..].chars().next().map_or(1, char::len_utf8);
            }
        }
        out
    }

    /// ㈠ 那根针 —— 「把一个上游与一把 key 焊成同一个值」那一步，**无空白形**。
    ///
    /// 与 09-09 之前那根 `Row { base` 是**同一根**，只是空白被抹掉了；
    /// 分母因此一格没变（唯一的差别是头部边界现在把 `structRow{base` 与
    /// `SessionRow{base` 显式挡在外面，见 [`sites_layout_blind`] 头注末段）。
    const WELD: &str = "Row{base";

    fn table_production() -> String {
        let files = crate_production();
        let (_, prod) = files
            .iter()
            .find(|(p, _)| p.ends_with("table.rs"))
            .expect("扫不到 `table.rs` —— 取法坏了，本条按红处理");
        prod.clone()
    }

    /// ★★★ **㈠ 焊接那一步只有一处**：把「一个上游」与「一把 key」焊成同一个值，
    /// 整个后端生产段里**恰好 1 处**，而且在**上游选择**（`accounts/table.rs`）里。
    ///
    /// ⚠ 〔`设计/20 §7` 步 2〕先前这句话的后半截是「在 `table.rs` 的 `mod sealed` 里」。
    /// `mod sealed` 那一格拆掉了（`§7` 步 2 逐字要求），换来的是**访问器收成
    /// `pub(super)`** —— 中转连 `Row` 这个类型都点不到，**那是编译器买的**。
    /// 本条的判定因此从「在 `sealed` 里面」改成「在 `accounts/` 里面」；
    /// 处数那一格（恰好 1）**一个字节没动**。
    ///
    /// # 它守的性质，与它的人群
    ///
    /// 性质：**「哪个上游配哪把 key」这个决定只有一个地方做得了。**
    /// 人群：整个 crate 生产段**去掉全部空白之后**，`Row{base` 这根针的处数
    /// （走 [`sites_layout_blind`]）。
    ///
    /// ⚠⚠ **分母如实写**：这根针是**子串**。写成 `Row { key: k, base: b }`（字段反序）
    /// 它数不到 —— 但那种写法**在 `mod sealed` 之外根本编不过**（字段私有）。
    /// ⇒ 本条真正管的是 **`sealed` 里面有没有第二处**；
    /// **外面一处都写不出来那一半，是编译器买的，不是本条买的。**
    ///
    /// # ★ 09-09：针从「逐行找 `Row { base`」换成「去空白后找 `Row{base`」
    ///
    /// 09-09 那趟 `cargo fmt --all` 把 `table.rs` 里唯一那处焊接拆成了五行，
    /// 逐行那把尺子当场数出 **0 处**、本条判定行逐字「……有 0 处，应当**恰好 1** 处：[]」
    /// —— 而**没有任何人多焊或少焊一行**。理由与换法写在 [`sites_layout_blind`] 的头注里。
    ///
    /// 两把尺子逐格对拍（只读现打）：
    ///
    /// | 人群 | 老尺子（逐行） | 新尺子（去空白 + 头部边界） |
    /// |---|---|---|
    /// | fmt 前（基线） | 1 ⇒ 绿 | 1 ⇒ 绿 |
    /// | 今天（fmt 后） | **0 ⇒ 红（假红）** | 1 ⇒ 绿 |
    /// | 焊接点被整个搬走 | 0 ⇒ 红 | 0 ⇒ 红 |
    /// | `sealed` 里多焊一处（写成**一行**） | **1 ⇒ 绿（漏了）** | 2 ⇒ 红 |
    /// | `sealed` 里多焊一处（写成**多行**） | 0 ⇒ 红（诊断指错方向） | 2 ⇒ 红 |
    /// | fmt 前 + 多焊一处（写成**多行**） | **1 ⇒ 绿（漏了）** | 2 ⇒ 红 |
    ///
    /// ⇒ 换尺子**没有掉牙**：老尺子对「第二处焊接写成多行」是**瞎的**，新尺子看得见。
    ///
    /// # ⚠⚠⚠ 而它在「`sealed` 里面」那一格**恰恰最弱**（`D1-M4` 实测）
    ///
    /// 上面那句「本条真正管的是 `sealed` 里面」读起来像一句承诺，**它不是**。
    /// 根子在于：**一个字面量能焊出任意多行** —— 它长在一个循环里，
    /// 「焊了几行、每行装的是什么」**这根针一个字都数不出来**。
    /// `D1-M4` 实测：在 `sealed` 里把回落整个加回来（同一个字面量多焊几行，
    /// 包括一行「什么键都匹配」的），**`Row { base` 仍然恰好 1 处**
    /// ⇒ `table_guard` 单跑 **10 passed / 0 failed**，本条**一声不吭**。
    /// 〔09-09 换针之后这一格**一个字没变**：那一刀多出来的行是同一个字面量在循环里跑出来的，
    /// 语法上仍然只有一处焊接 —— 换尺子买不到「决定的条数」，别读成买到了。〕
    ///
    /// ⇒ **准确的说法**：本条数的是**语法处数**，不是**决定的条数**。
    /// 它挡得住「有人在别的文件里另起一处焊接」，挡不住「在这一处里焊出一张错的表」。
    /// **后者今天由 `KH2`/`KH4` 那几条行为判据守**（它们量的是真转发落到哪个端点、带了哪把 key），
    /// 不由本条守。别把「恰好 1 处」读成「表一定是对的」。
    #[test]
    fn the_only_place_that_welds_an_upstream_to_a_key_is_inside_upstream_selection() {
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

        let welds = sites_layout_blind(&files, WELD);
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
            welds[0].contains("accounts/upstream_select/table.rs")
                || welds[0].contains("accounts\\upstream\\table.rs"),
            "唯一那一处不在 `accounts/upstream_select/table.rs` 而在 {} —— 靶子挪了",
            welds[0]
        );

        // ★ 它必须落在**上游选择里面**。`mod sealed` 那一格 `设计/20 §7` 步 2 拆掉了
        //   （理由整段住 `accounts/table.rs` 里那条 `★★ 🔴` 注释），换来的是
        //   **访问器收成 `pub(super)`** —— 那是编译器买的，不是本条买的。
        //   本条今天断的是「焊接点没有溜出上游选择」。
        let table = table_production();
        let inside = sites_layout_blind(
            &[("accounts/upstream_select/table.rs".to_string(), table)],
            WELD,
        );
        assert_eq!(
            inside.len(),
            1,
            "`accounts/upstream_select/table.rs` 里的焊接点不是恰好一处（实得 {} 处）。",
            inside.len()
        );
    }

    /// ★★★ **㈡ 开上游连接的地方只有一处**，而且它在**交换面**上（`server.rs::send_upstream`）。
    ///
    /// # ⚠⚠ 🔴 〔`设计/20 §7` 步 1〕**靶子搬了一次家，经过写在这里**
    ///
    /// 先前那唯一一处是 `table::Row::connect`（上游选择），本条的名字与判定都点着 `table.rs`。
    /// 两层解耦之后「连上游」**归中转**（`20 §4`：`exchange` 那一行逐字是
    /// 「resolve → **连上游** → pump → tee」），中转手里拿到的是 `Destination`
    /// 里那个 `&Base` ⇒ 调用点必然在 `server.rs`。
    ///
    /// **性质一格没松，换了个说法**：先前是「只有表里那一行连得出去」，
    /// 今天是「**只有上游选择答的那一个目的地连得出去**」——
    /// 而「中转自己造不出一个 `Base` 来连」由隔壁那条
    /// [`the_relay_has_no_default_upstream_to_fall_back_to`] 的两向相等断言兜。
    /// ⇒ 本条 ＋ 那条，合起来才等于先前那一句。**单看本条会把它读得比实际强。**
    ///
    /// # 没有这一条会漏掉什么（这就是它不是冗余的理由）
    ///
    /// 只钉㈠的话，有人完全可以在别处再写一句 `upstream::connect(&some_base)`，
    /// **绕过 `resolve`** 把请求发出去 —— 那时㈠仍然是 1，而「上游选择说了算」已经不成立。
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
    fn the_only_place_that_opens_an_upstream_connection_is_the_exchange() {
        let files = crate_production();
        assert!(
            files.len() >= 30,
            "只扫到 {} 个文件 —— 取法坏了",
            files.len()
        );

        let calls = sites(&files, "upstream::connect(");
        assert_eq!(
            calls.len(),
            1,
            "开上游连接的地方有 {} 处，应当**恰好 1** 处：{calls:?}\n\
             ⚠ 第二处就是一条**绕过 `resolve`** 的路：它决定了请求实际连到哪儿，\n\
             而那正是两层解耦要收进上游选择的那个决定。",
            calls.len()
        );
        assert!(
            calls[0].contains("server.rs"),
            "唯一那一处不在交换面（`server.rs`）而在 {} —— 靶子挪了",
            calls[0]
        );

        // 非空对照：同一把尺子**数得到**别的东西（证明它不是恒返回一条）。
        let lookups = sites(&files, "table.lookup(");
        assert!(
            !lookups.is_empty(),
            "非空对照失败：同一把尺子连 `table.lookup(` 都数不到 —— 这把尺子是瞎的"
        );
    }

    /// ★★★ **中转里没有任何可以回落的默认上游**（`设计/20 §4`「常量跟着职责走」）。
    ///
    /// # 它买的是什么 —— 与它买不到什么
    ///
    /// `table.rs` 头注那张表里有一行今天**自陈是假的**，逐字：
    /// 「进程里『没有默认上游可回落』| 只有一条文本棘轮 | **假**：`DEFAULT_UPSTREAM`
    /// 是中转的一个 crate 常量」（那时它住 `server.rs`）。把那个常量挪过层边界之后，
    /// **中转那几份文件里再也没有一个上游字面量** —— 「查不到就回落到它」这句代码
    /// 在中转里**写不出来**，因为那个值不在它的作用域里。
    ///
    /// ⚠⚠ **诚实边界，别读成买断**：`Base` 三个字段与 `Base::parse` 仍是 `pub(crate)`
    /// ⇒ 中转**有意**去 `Base::parse("https://…")` 现造一个，本条**抓不住**（针是那个
    /// 常量名与那条 URL 字面量，不是「有没有第二条造 `Base` 的路」）。
    /// 本条守的是「**顺手回落**」那一形，与它同族的
    /// [`the_only_place_that_opens_an_upstream_connection_is_the_exchange`] 一起看才完整。
    ///
    /// # 反空真：**两向都断**，不是「扫不到就绿」
    ///
    /// 「中转里零处」单独立着是典型的空真（针拼错、人群取空，一样绿）。
    /// ⇒ 同一把尺子在**上游选择**里必须数到**恰好 1 处**：数不到就说明尺子瞎了，当场红。
    #[test]
    fn the_relay_has_no_default_upstream_to_fall_back_to() {
        let files = crate_production();
        // 上游选择的人群：`accounts/upstream_select/` 底下那几份。中转 = `relay/` 里**除它之外**的。
        // 〔NT2 · V25〕适配层（`agents/`）是默认上游那一格今天的住址（用户 V25「写死, 跟着适配层」）。
        // 〔RELAY〕按 crate 根之后的相对路径判：工作树目录名里可能正好带 `relay`（`w4-relay/` 让下面每份文件都「在中转里」）。
        let rel = |p: &str| {
            let q = p.replace('\\', "/");
            q.rsplit_once("src/backend/")
                .map_or(q.clone(), |(_, r)| r.to_string())
        };
        let is_upstream_selection = |p: &str| rel(p).starts_with("accounts/upstream_select/"); // 〔`A3` 第二波〕上游选择收窄到 `accounts/upstream_select/`（`accounts/iso.rs` 不是上游选择）
        let in_adapter = |p: &str| rel(p).starts_with("agents/");
        let in_relay = |p: &str| rel(p).starts_with("relay/");

        // 两根针：读那一格的唯一入口 ＋ 那个值。**两根都数**，免得有人只搬走名字、把字面量留在原地。
        // 期望处数是**显式登记的**（不是「>0 就算」）—— 多一处就要来加一行，说清它是什么。
        // `(针, 上游选择里的处数, 适配层里的处数, 为什么)`
        const SITES: &[(&str, usize, usize, &str)] = &[
            (
                "default_upstreams(",
                1,
                1,
                "〔NT2 · V25〕上游选择里 `Upstreams::from_env` 那一次遍历 · 适配层里它的定义",
            ),
            (
                "https://api.anthropic.com",
                0,
                1,
                "〔NT2 · V25〕那条 URL 字面量只出现在适配层 claude-code 那一格（`agents/claudecode` 的 `UPSTREAM`）",
            ),
        ];
        // 〔条 59〕先前那个**进程级**常量（`DEFAULT_UPSTREAM`）整删了 —— 它对每一个 `seg1`
        // 都成立，于是「codex 的请求发给 Anthropic」在上游选择里也写得出来。
        // 〔NT2 · V25〕上游选择自己那张每 agent 一行的表（`AGENT_UPSTREAMS`）也整删了（搬回适配层）——
        // 两个名字在**整个 crate** 的代码里零处（注释里的墓碑不算：剥掉注释再数）。
        // ⇒ 反空真由上面那张表的第一行担：同一把尺子数得到继任者，它才不是瞎的。
        let code: Vec<(String, String)> = files
            .iter()
            .map(|(p, prod)| (p.clone(), guard_core::strip_comment_lines(prod)))
            .collect();
        for gone in ["DEFAULT_UPSTREAM", "AGENT_UPSTREAMS"] {
            let revived = sites(&code, gone);
            assert!(
                revived.is_empty(),
                "`{gone}` 又回来了：{revived:?}\n\
                 🔴 默认上游是**每 agent 一格、跟着适配层**（`agents::Adapter::upstream`）；一个对所有 agent \
                 都成立的值，就是「未登记的 agent 回落到某一家」那条被明禁的路；另起一张表，就是「跟着适配层」又不在同一格。"
            );
        }
        for (needle, want_sel, want_adapter, why) in SITES {
            let hits = sites(&code, needle);
            let selection: Vec<&String> =
                hits.iter().filter(|p| is_upstream_selection(p)).collect();
            let adapter: Vec<&String> = hits.iter().filter(|p| in_adapter(p)).collect();
            let relay_side: Vec<&String> = hits
                .iter()
                .filter(|p| in_relay(p) && !is_upstream_selection(p))
                .collect();
            // ★ 非空对照（这一条**先断**）：尺子在适配层里数得到，它才不是瞎的。
            assert_eq!(
                (selection.len(), adapter.len()),
                (*want_sel, *want_adapter),
                "`{needle}` 在（上游选择, 适配层）里应当**恰好** ({want_sel}, {want_adapter}) 处（{why}），\
                 实得 上游选择 {selection:?} · 适配层 {adapter:?}\n\
                 数不到 ⇒ 这把尺子是瞎的，下面那条「中转零处」就是空真。"
            );
            assert!(
                relay_side.is_empty(),
                "中转（`relay/` 里 `accounts/` 之外）出现了 `{needle}`：{relay_side:?}\n\
                 ⚠ 有那个值，「查不到就回落到它」就又写得出来了，而最坏的失效形态是\n\
                 **codex 的请求被发给 Anthropic**（`设计/20 §3.1` 拍板 (b) 甲逐字点名）。",
            );
        }
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
    /// # 🔴 〔`设计/20 §7` 步 1〕**第三版：锁换了持有者，判法也跟着换**
    ///
    /// 第二版钉的是一串含缩进的源码字面
    /// （`let mut up = {\n        let table = relay.table.read()`），
    /// 而两层解耦之后 **`server.rs` 里根本没有 `relay.table` 这个东西了** ——
    /// 那把锁归上游选择，由 `accounts::upstream_select::Accounts::resolve` 自己持有，活到它返回为止。
    ///
    /// ⇒ 性质换了一个说法，**一格没松**：
    /// 「读锁不跨 `pump`」 ⇔ **中转交给 `resolve` 的那个闭包里不许出现 `pump(`**
    /// （锁在 `resolve` 返回时就放了，而 `pump` 在它之后）。
    /// 本条因此改成**切窗口**：把 `relay.dest.resolve(` 那一整个实参块切出来，断它不含 `pump(`。
    /// ⚠ 这一版**比第二版好一格**：`D4 §G1` 逐字登记过第二版「挡不住把 `pump(` 搬进块里」
    /// 那一形 —— **今天挡得住了**，那正是本条现在唯一在断的事。
    ///
    /// # ⚠ 射程（它挡得住什么、挡不住什么，一起写）
    ///
    /// - **挡得住**：把 `pump(` 搬进 `resolve` 的闭包里（`D4 §G1` 记着的那一形）。
    /// - **挡不住**：上游选择自己把读锁的生命周期拉长（比如把守卫 `Box::leak` 出去、
    ///   或换成一个跨请求持有的守卫）—— 那不在本条的窗口里。**没实测，如实登记。**
    /// - **误红的方向**：它钉的是 `relay.dest.resolve(` 这一串字面，
    ///   给 `dest` 改个名字就会打红。合法出路是**同一拍把这里一起改**，
    ///   不许把针放宽（放宽不可逆）。
    #[test]
    fn the_upstream_selection_lock_does_not_outlive_the_streaming_pump() {
        let files = crate_production();
        let (_, server) = files
            .iter()
            .find(|(p, _)| p.ends_with("server.rs"))
            .expect("扫不到 `server.rs` —— 取法坏了，本条按红处理");

        let at = guard_core::find_pinned(server, "relay.dest.resolve(")
            .expect("切不出中转那一问（`relay.dest.resolve(`）—— 本条按红处理，不是绿");
        let block =
            brace_block(server, at).expect("`resolve` 那个实参块的花括号没配平 —— 按红处理");

        // 反空真自检㈠：真的切到了那个闭包（它里面必须有三支里的两支）。
        assert!(
            block.contains("Destination::Refuse") && block.contains("Destination::Substitute"),
            "切出来的窗口里没有 `Destination` 的分支 —— 取法坏了，下面的断言在空转。窗口：{block}"
        );
        // 反空真自检㈡：窗口没有跨进下一个 item（`pump` 那一段在它之后，不许被吃进来）。
        assert!(
            !block.contains("let outcome = pump("),
            "`resolve` 的实参块窗口跨进了后面那一段 —— 窗口无界，本条的结论不算数"
        );

        // ★ 正题：闭包里一处 `pump(` 都不许有。
        assert_eq!(
            block.matches("pump(").count(),
            0,
            "上游选择的读锁活到 `resolve` 返回为止，而这个闭包里出现了 `pump(`：\n\
             ⇒ 一条 SSE 长流会把那把读锁按住几分钟。`RwLock` 写优先 ⇒ \n\
             用户配一次 key 会被堵在**最长那条在飞流**后面。窗口：{block}"
        );
        // 反空真：`pump(` 真的在这份生产段里（只是在窗口**外面**）——
        // 否则上面那条 `== 0` 可能只是因为这份文件里压根没有 `pump`。
        assert!(
            server.contains("let outcome = pump("),
            "扫到的 `server.rs` 里没有 `pump(` —— 取法坏了，本条按红处理"
        );

        // ★ 另一半在上游选择：那把读锁**恰好一处**取，而且就在 `resolve` 里。
        let (_, accounts) = files
            .iter()
            .find(|(p, _)| {
                p.ends_with("accounts/upstream_select/mod.rs")
                    || p.ends_with("accounts\\upstream\\mod.rs")
            })
            .expect("扫不到 `accounts/upstream_select/mod.rs` —— 取法坏了，本条按红处理");
        assert_eq!(
            accounts.matches("self.table.read()").count(),
            1,
            "上游选择取读锁的地方不是恰好一处 —— 锚点不唯一，本条的结论不算数"
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
    /// 「backend 不开 `harden`」五轮买来的性质整个挂在 manifest 一行上，而没人看着它。〕
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
