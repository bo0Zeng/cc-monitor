//! `K-H2a` 的两条机检：**明文出口恰好一处**（`KS2`）· **记日志走白名单**（`KS4`）。
//!
//! # 为什么单住一个文件
//!
//! 与隔壁 `bind_guard.rs` / `nodelay_guard.rs` 同一个理由，它的头注逐字写着：
//! monitor 侧 `scanning_guard_registry` 立过一条递减棘轮 —— 扫描型判据不许裸遍历目录，
//! 要走 `guard_core::scan_tree!`。
//!
//! ⚠ 〔`P4` 2026-09-21〕先前这里跟着抄了一句「那个宏**按构造摘掉调用者自己那一份**」——
//! **那一刀在这一处不生效**（判据由 `#[path]` 挂载 ⇒ `file!()` 是折返路径 ⇒ 后缀比
//! 恒不命中）。结论「**判据与被扫的代码必须不在同一个文件**」仍然成立，但成因只剩一种：
//! 判据若写在被扫文件自己的 `#[cfg(test)]` 段里（不经 `#[path]`），`file!()` 就会命中
//! ⇒「摘掉自己」正好把靶子摘了。本文件今天不在人群里靠的是住址（它住 `tests/backend/relay/`）。
//!
//! # `KS4` 为什么是白名单，而本仓别处大量用黑名单
//!
//! `KS4` 逐字：「只记允许记的那几样（方法 · 路径 · 状态码 · 字节数）。**禁止**
//! 『记全部头，除了 `Authorization`』这种写法。理由：黑名单必漏
//! （`Proxy-Authorization` · `X-Api-Key` · `Cookie` · 各家自定义），
//! 而且上游多一种鉴权方式它就自动过期，**没有任何东西会告诉你它过期了**。」
//!
//! 白名单做得成的前提是**人群可枚举**（`structural_scan.rs` 头注逐字论证过这一条，
//! 而 `platform/fallback_guard.rs` 记着它做不成的那次：人群不同质就只能黑名单）。
//! 这里人群是可枚举的：`relay/` 生产段里的每一处 `eprintln!` / `println!` / `writeln!`。
//! ⇒ 枚举它们，要求**每一个被插进去的东西**都在下面那张表里。
//!
//! # 它**认不出**什么（诚实边界，别读成「日志不可能泄漏」）
//!
//! 1. **只扫 `relay/`**。中转之外别处印了什么，本条不管（今天 key 也只住 `relay/`）。
//! 2. **只认 `{ident}` 内联捕获与逗号分隔的位置实参**。有人写
//!    `let s = format!("{:?}", head.headers); eprintln!("{s}");` ⇒ 本条只看见 `s`，
//!    而 `s` 要进白名单得有人写一行理由 —— 拦得住「顺手」，拦不住「刻意绕」。
//! 3. **不判语义**：一个进了白名单的名字，将来被换成装别的东西，本条看不见。

#[cfg(test)]
mod tests {
    use crate::guard_support::production_code;

    /// 允许出现在中转日志里的东西。**默认拒绝**：不在表里的当场红。
    ///
    /// 每行 `(那个名字, 它为什么可以进日志)`。⚠ 加一行**就是在放宽**，要写得出理由。
    const ALLOWED_LOG_ARGS: &[(&str, &str)] = &[
        ("e", "错误对象本身（`std::io::Error` / 解析错误）—— 它不含请求头"),
        ("a", "监听地址（`local_addr()`）"),
        ("port", "端口号"),
        (
            "INFLIGHT_CONNECTIONS",
            "在途连接数上限，一个编译期常量",
        ),
        ("p", "凭据文件读不动 / 解析不了时的说法（`store::StoreError` 的文本，不含 key）"),
        ("how", "权限过宽宽在哪 —— `perm::judge` 造的句子，只含 mode 位 / SDDL 主体名"),
        ("fix", "怎么修 —— 一句固定的指引"),
        ("why", "为什么查不出权限 —— 只含平台与构建 feature"),
        (
            "loaded.path.display()",
            "凭据文件的路径。**`KS9` 的「路径文档化」就落在这一行**：一个「能手编但没人知道在哪」的文件等于不能手编",
        ),
        (
            "store::TEMPLATE",
            "文件不存在时印的模板。它里面那个 `api_key` 字段的值**是空串**（`store` 的判据钉着）",
        ),
        ("ms[0]", "耗时毫秒数（判据自己印的，生产段不出现）"),
        ("ms[1]", "同上"),
        (
            "rows",
            "**进得了路由表的账号条数**（`K-H2`）。一个 `usize`，与任何一把 key 无关；\
             印它是因为「文件里写了 N 条、只有 M 条能用」这件事必须看得见",
        ),
        (
            "r.id",
            "一条**进不了表**的账号 id（`K-H2`）。它本来就要出现在 URL 路径里（路由键那一段），\
             不是秘密；且它走 `{:?}` 打印 ⇒ 控制字符被转义，不给「把换行塞进日志」留口子",
        ),
        (
            "r.why",
            "那一条为什么进不了表（`K-H2`）。**是一个 `&'static str` 固定文案**，\
             不含文件里的任何内容 —— 这一点由 `table::Rejected::why` 的类型兜着",
        ),
        (
            "note.id",
            "一条**进了表、但行为与默认不同**的账号 id（`K-R1`）。理由与 `r.id` 逐字同一条：\
             它本来就要出现在 URL 路径里，不是秘密；且走 `{:?}` 打印 ⇒ 控制字符被转义",
        ),
        (
            "note.what",
            "那一条哪里与默认不同（`K-R1`：带了路径前缀 / 换了鉴权头形状）。\
             **是一个 `&'static str` 固定文案**，由 `table::Note::what` 的类型兜着 ——\
             ⚠ 刻意**不印**那个前缀本身、也不印那个认不出的词：那两样是文件内容",
        ),
        (
            "legal",
            "`auth_style` 认得的那几个值，由 `creds_core::store::AuthStyle::ALL` **现算**\
             （`brief` 13b：闭集只许有一个住址，不许在日志里再抄一份字面量）。\
             全是编译期 `&'static str`，与任何一把 key 无关",
        ),
    ];

    /// `relay/` 生产段里每一处日志调用的**格式串起首**，`(所在文件, 起首, 它记的是什么)`。
    ///
    /// ⚠ **这是相等断言的另一半**：扫出来的处数必须等于本表行数。
    /// 加一处日志就要来加一行，那正是要的 —— 逼你说清「这一行记的是什么」。
    const LOG_SITES: &[(&str, &str, &str)] = &[
        (
            "relay/listen.rs",
            "[relay] cannot set connection deadline",
            "装期限失败",
        ),
        (
            "relay/listen.rs",
            "[relay] refusing",
            "在途连接顶满，回 503",
        ),
        (
            "relay/listen.rs",
            "[relay] connection ended",
            "一条连接以错误收尾",
        ),
        (
            "relay/listen.rs",
            "[relay] cannot spawn connection thread",
            "起线程失败",
        ),
        (
            "relay/server.rs",
            "[relay] upstream connect failed",
            "连不上上游",
        ),
        (
            "relay/listen.rs",
            "[relay] bad upstream base url",
            "上游基址解析不了",
        ),
        (
            "relay/listen.rs",
            "[relay] cannot bind loopback port",
            "端口起不来",
        ),
        (
            "relay/listen.rs",
            "[relay] listening on",
            "起来了，监听在哪",
        ),
        (
            "relay/listen.rs",
            "[relay] listening (addr unknown",
            "起来了但问不到地址",
        ),
        (
            // ⚠ 〔`设计/20 §7` 步 1〕它**搬家了**：热重载整块归层 2，住址从
            //   `server.rs` 变成 `accounts/mod.rs`。话一个字没改。
            "accounts/mod.rs",
            "[apikey] 凭据文件读不成表，**保留上一张表不动**",
            "`D2 阻-2`：重载时解析失败 —— **不把表换成空**（空表 = 全部 404），\
             留住上一张能用的、只出声。这一形是「表可重载」之后新长出来的",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials file:",
            "凭据文件在哪（`KS9` 路径文档化）",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials problem:",
            "文件读不动 / 解析不了",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials permissions too wide:",
            "权限过宽（`KS11`）",
        ),
        (
            "accounts/creds.rs",
            "[apikey] how to fix:",
            "怎么修（`KS11` 要求两样都有）",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials permissions unknown:",
            "查不出权限，也要出声",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials: this account cannot be used:",
            "一条账号进不了路由表（id 当不了路由段 / `base_url` 解析不了）—— \
             `K-H2`：静默丢一行的症状是「我明明配了，中转永远 404」",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials: configured",
            "配了 —— **只印这个布尔与进得了表的条数**，不印长度、不印掩码",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials: auth_style must be one of:",
            "有一条的 `auth_style` 认不出时，把认得的那几个**现算**着印出来（`K-R1`）——\
             它是给正在排错的人看的最后一句话，所以不许是一份会变旧的字面量清单",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials: this account is not on the default path:",
            "一条**进了表、但行为与默认不同**的账号（`K-R1`：带了路径前缀 / 换了鉴权头形状）。\
             它与上面那条「cannot be used」是两件事：这一条**照发**，只是发出去的字节不同 ⇒ \
             它错了的症状是上游的 404 / 401，与「上游挂了」同形，必须在启动时说出来",
        ),
        (
            "accounts/creds.rs",
            "[apikey] credentials: not configured",
            "没配",
        ),
        (
            "accounts/creds.rs",
            "[apikey] create that file to configure one",
            "没配时印模板",
        ),
    ];

    /// 日志白名单的人群：**两层都在** —— 层 1（`relay/`）与层 2（`accounts/`）。
    ///
    /// ⚠ 〔2026-09-24 层 2 搬出 `relay/`〕先前人群是 `relay/` 一棵树，层 2 住在它底下所以顺带被扫。
    ///   搬走之后只扫 `relay/` 的话，层 2 那 12 行日志会**掉出扫描面** —— 它们记的恰恰是
    ///   凭据文件那一侧的事，是 `KS4` 最该看着的那一批。⇒ 两棵根明写在这里，
    ///   并由判据本体断言「盘上有日志的根 ⇔ 登记表里出现的根 ⇔ 本表」三方相等。
    const LOG_ROOTS: &[&str] = &["relay", "accounts"];

    /// 整个 backend crate 的生产段（逐文件）。`KS2` 的人群是**整个 crate**，不是 `relay/` ——
    /// 「取明文的地方恰好一处」这句话的分母如果只到 `relay/`，
    /// 那么有人在 `observe/` 里再取一次就不会红。**人群要恰好等于性质。**
    fn crate_production() -> Vec<(String, String)> {
        let dir = crate::guard_support::src_root();
        guard_core::scan_tree!(&dir, &["rs"])
            .into_iter()
            .map(|(p, raw)| (p.display().to_string(), production_code(&raw)))
            .collect()
    }

    /// ★★★ **`K-H2a` 裁四那句话的判据**〔`E` 阻-2 回修，08-27〕：
    /// **本 crate 不许打开 `creds-core` 的 `harden` feature。**
    ///
    /// # 它守的是一句**承重**的话，而那句话此前零判据
    ///
    /// 本件对外声称的形状逐字是：「『backend 写不了这份文件』是**编译器**兜的，
    /// 不是一条判据兜的」——`creds_core::perm::make_private` 与 `create_private` 都挂在
    /// `harden` 上，backend 不开它 ⇒ 那两个函数在本 crate 里**根本不存在**。
    ///
    /// ⚠ 而「不开」这件事本身，此前**只是 manifest 上的一行约定**。
    /// `E` 阶段实打：给 `src/backend/Cargo.toml` 那一行加上 `features = ["harden"]`
    /// ⇒ **backend 474 passed / 0 failed，一条都没红**（我自己复打确认，读数逐字相同），
    /// 顺带依赖树 **96 → 101**、`windows*` 条目 **21 → 26**。
    /// ⇒ 五轮买来的那格「性质由编译器买」，整个挂在这一行上，而没人看着它。
    ///
    /// # 人群 = **整个 crate 的 manifest**，不是某一行
    ///
    /// 只断言「`creds-core` 那一行不含 `harden`」是不够的：`harden` 也可能从
    /// `[features]` 段、`default-features`、或另一条重复的依赖声明里进来。
    /// ⇒ 本条断言 **`harden` 这个词在本 crate 的整份 `Cargo.toml` 里出现 0 次**。
    ///
    /// ⚠ **非空对照是承重的**：同一把尺子量 monitor 那份 manifest ⇒ 必须**数得到** `harden`
    /// （monitor 是唯一该开它的一侧）。没有这一格，「0 次」可能只是因为尺子瞎了。
    ///
    /// ⚠ 它**认不出**什么：`--features harden` 从**命令行**传进来（`cargo test -p … --features`）。
    /// 那条路不经 manifest，本条看不见；今天没有任何脚本这么跑后端（`gate.sh` 里
    /// backend 那道门逐字是 `cd src/backend && cargo test`，零 `--features`）。
    #[test]
    fn this_crate_never_turns_on_the_write_half_of_the_credentials_crate() {
        let mine = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
        )
        .expect("读不到本 crate 的 Cargo.toml —— 抽取器坏了，本条会零命中地绿");

        // 抽取器自检：真读到了那份 manifest，而且里面确实声明了 `creds-core`。
        assert_eq!(
            mine.matches("creds-core = {").count(),
            1,
            "本 crate 的 manifest 里 `creds-core` 的声明不是恰好一条 —— 取法坏了或有人加了第二条"
        );

        // ⚠ **剥掉 `#` 注释行再数** —— 这一步是判据第一跑逼出来的：
        //   本 crate 的 manifest 里有**我自己写的两行注释**逐字提到 `harden`
        //   （「刻意不开 `harden`」「不开 `harden` ⇒ Windows 上读不了 DACL」）
        //   ⇒ 第一版实测「出现 2 次，应当 0 次」。
        //   ★ 那两行注释**恰恰是该留的**（它们解释了为什么不开），
        //     所以要动的是人群不是被守对象：**判据只该看生效的声明，不该看解释它的话。**
        let mine = guard_core::strip_hash_comment_lines(&mine);
        let n = mine.matches("harden").count();
        assert_eq!(
            n, 0,
            "本 crate 的 `Cargo.toml` 里出现了 `harden` {n} 次，应当 **0** 次。\n\
             ⚠ `K-H2a` 裁四：backend **只许读**那份凭据文件，不许写。\n\
             `creds_core::perm::{{make_private, create_private}}` 都挂在 `harden` 上 ——\n\
             一旦打开，「backend 写不了这份文件」就从**编译器兜的**退回成**没人兜的**。\n\
             （`E` 阶段实打：打开它之后 backend 474 passed，一条都没红。）"
        );

        // ★ 非空对照：同一把尺子量 monitor 那份 manifest，**必须数得到** `harden`。
        //   没有这一格，上面那个 0 可能只是因为尺子瞎了。
        let theirs = std::fs::read_to_string(
            crate::guard_support::repo_root().join("src/bridge/Cargo.toml"),
        )
        .expect("读不到 monitor 的 Cargo.toml");
        let theirs = guard_core::strip_hash_comment_lines(&theirs);
        assert!(
            theirs.matches("harden").count() >= 1,
            "非空对照失败：monitor 那份 manifest 里也数不到 `harden` —— \
             这把尺子是瞎的，上面那条「0 次」证不了什么"
        );
    }

    /// ★★ **焊缝：我可能自己写出来的每一个头名，都在「先丢掉」那个集合里。**
    ///
    /// 〔`P16` 2026-09-22 从 `server_tests.rs` **搬到这一侧**〕缺这一焊的症状是
    /// **同名鉴权头出现两次** —— HTTP 允许同名头出现多次，上游谁赢**没有定义**，
    /// 而一把不属于这一行的客户端凭据就这样被多送出去一次。
    ///
    /// # 为什么它今天住层 2 这一侧
    ///
    /// 焊的两端先前都住层 1（`AUTH_HEADER_NAMES` ＋ `auth_header_of`）。`P16` 把映射
    /// 按 `C2` 搬去层 2、把名单改成**由映射派生**之后，两端都在层 2
    /// ⇒ 判据跟着搬，焊缝不跨层（一条跨层焊缝正是 `D1` 警告的形状）。
    ///
    /// # 🔴 它判的是**产物**，不是两个定义 —— 否则它就是恒真
    ///
    /// 名单既然是从映射派生的，「名单 ⊇ 映射写得出的头名」在**定义**那一层已经
    /// 按构造成立 ⇒ 拿两个定义对拍就是**两侧同源的恒等 = 恒真**，本仓逐字禁过。
    /// ⇒ 本条改判**真走一遍层 2 拿到的那个 `AuthSwap`**：
    ///
    /// 1. 每一种 `AuthStyle` 各配一行，过**生产段那条真实的** `accounts::decide`；
    /// 2. 它说要写的那个头名，必须在它**自己那一份** `clear` 里；
    /// 3. 🔴 **射程不许缩**：`clear` 必须是**全集** —— 判据自己独立地从
    ///    `AuthStyle::ALL` ＋ `auth_header_of` 派生一遍期望值去比。
    ///    ⇒ 配了 `x-api-key` 的那一行也必须把客户端的 `Authorization` 丢掉。
    ///    有人把 `dispatch_auth` 改成「只丢这一趟要写的那一个」⇒ 本条当场红。
    ///
    /// 两侧异源：期望值是**判据这边**从闭集算的，实际值是**生产段** `dispatch_auth`
    /// 塞进 `AuthSwap` 的。中间那一步（`headers_to_clear()` 有没有被真的传下去）
    /// 正是本条唯一判得动、也唯一判不了别的东西。
    ///
    /// # 反空真
    ///
    /// · 至少两种形状**真的写了头**（全是 `None` 的话 2 与 3 空转）；
    /// · `clear` 非空；· 期望全集非空且至少两项。
    ///
    /// # 买不到
    ///
    /// - **不买「所有鉴权头都被丢掉」** —— 那个分母没人给得出（`Cookie` / 各家自定义 /
    ///   `Proxy-Authorization`…）。本条是**白名单方向**：只保证「我可能写的那几个」
    ///   在丢掉之列，别的原样转发。`Proxy-Authorization` 仍然照旧转发，已登记为射程外。
    /// - **不买「层 1 真的照着 `clear` 丢了」** —— 那由 `server_tests` 那一族的字节判据
    ///   （`the_auth_header_shape_follows_the_row_and_not_a_process_wide_guess`）钉着。
    #[test]
    fn every_header_this_relay_may_write_is_in_the_set_it_clears_first() {
        use creds_core::store::AuthStyle;
        use creds_core::SecretKey;

        // 期望的全集：**判据这边自己**从闭集派生一遍（与生产段那一份异源）。
        let want_clear: std::collections::BTreeSet<String> = AuthStyle::ALL
            .iter()
            .filter_map(|s| crate::accounts::auth_header_of(*s))
            .map(|(n, _)| n.to_ascii_lowercase())
            .collect();
        assert!(
            want_clear.len() >= 2,
            "会写头的形状只有 {} 种 —— 本条在空转（射程那一格无从判起）",
            want_clear.len()
        );

        let base = crate::relay::upstream::Base::parse("https://api.example.com").expect("base");
        let mut wrote = 0usize;
        for style in AuthStyle::ALL.iter().copied() {
            let table = crate::accounts::table::RoutingTable::build(std::iter::once((
                "a".to_string(),
                "acct".to_string(),
                base.clone(),
                Some(SecretKey::new("sk-WELD-PROBE")),
                style,
            )));
            let key = crate::relay::RouteKey {
                seg1: "a".to_string(),
                seg2: "acct".to_string(),
            };
            let mut seen: Option<(Option<&'static str>, Vec<String>)> = None;
            let upstreams = crate::accounts::Upstreams::from_env(&|_| None).expect("内置默认");
            crate::accounts::decide(
                &table,
                &upstreams,
                crate::relay::Mode::Substitute,
                &key,
                &mut |d| {
                    if let crate::relay::Destination::Substitute { auth, .. } = d {
                        seen = Some((
                            auth.write.map(|(n, _)| n),
                            auth.clear.iter().map(|n| n.to_ascii_lowercase()).collect(),
                        ));
                    }
                },
            );
            let (written, clear) = seen.unwrap_or_else(|| {
                panic!("{style:?}：配了 key 的那一行在 `/s/` 下没答 `Substitute`")
            });
            assert!(!clear.is_empty(), "{style:?}：`clear` 是空的 —— 本条在空转");
            // ㈡ 它要写的那个头名必须在它自己那份 `clear` 里。
            if let Some(name) = written {
                wrote += 1;
                assert!(
                    clear.iter().any(|n| n.eq_ignore_ascii_case(name)),
                    "`{name}` 是本中转会写出去的头，却不在它自己交下来的「先丢掉」\
                     那个集合里 ⇒ 客户端也带一个同名的时候，上游会看见两个"
                );
            }
            // ㈢ 🔴 射程不许缩：交下来的必须是**全集**，不只这一趟要写的那一个。
            let got: std::collections::BTreeSet<String> = clear.into_iter().collect();
            assert_eq!(
                got, want_clear,
                "{style:?} 那一行交给层 1 的「先丢掉」集合与全集对不上。\n\
                 🔴 **缩了是行为变更，不是优化**：配了 `x-api-key` 的那一行若不丢掉\
                 客户端的 `Authorization`，那把不属于这一行的凭据就被一起送给上游了。"
            );
        }
        assert!(
            wrote >= 2,
            "只有 {wrote} 种形状写了头 —— 上面 ㈡ 那一格空转了"
        );
    }

    /// ★★ `KS2`：**取明文的地方恰好一处**，而且就是换头那一行。
    #[test]
    fn the_plaintext_leaves_the_type_at_exactly_one_place_in_this_crate() {
        let files = crate_production();
        // 采集面自检：整个 crate 的 .rs 不止几个（**相等地板会误伤**，这里用有理由的下界）。
        assert!(
            files.len() >= 30,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );

        let mut header_sites: Vec<String> = Vec::new();
        let mut persist_sites: Vec<String> = Vec::new();
        for (path, prod) in &files {
            for (no, line) in prod.lines().enumerate() {
                if line.contains("expose_for_auth_header(") {
                    header_sites.push(format!("{path}:{}", no + 1));
                }
                if line.contains("expose_for_persisting(") {
                    persist_sites.push(format!("{path}:{}", no + 1));
                }
            }
        }

        assert_eq!(
            header_sites.len(),
            1,
            "把明文取出来写鉴权头的地方有 {} 处，应当**恰好 1** 处：{header_sites:?}\n\
             ⚠ `KS2` 逐字：加行是收紧、动断言是放宽。真要多一处，\
             **必须先在件计划里说清那一处是什么**，不许在实现里顺手把这个数改大。",
            header_sites.len()
        );
        // ⚠ 〔`P16` 2026-09-22〕住址从 `server.rs`（层 1）换成 `accounts/mod.rs`（层 2）——
        //    **换的是住址，不是处数**：上面那条「恰好 1」的相等断言一个字节都没动。
        //    搬的理由：层 1 的类型面上不许再出现 `creds-core` 的类型（`C2`），
        //    而「把 key 拼成头值」必须拿着 `SecretKey` ⇒ 它只能在层 2。
        //    ⇒ 层 1 从此**碰不到明文**。这一格因此比先前**更紧**，不是搬松了。
        assert!(
            header_sites[0].ends_with(".rs:0") || header_sites[0].contains("accounts/mod.rs"),
            "唯一那处不在 `accounts/mod.rs`（层 2 拼头值那一行）而在 {} —— 靶子挪了。\n\
             ⚠ 它**不许**回到层 1：那会让 `creds-core` 的类型重新爬上层 1 的类型面（`C2`）。",
            header_sites[0]
        );
        // ★ 另一半：**落盘那个出口在本 crate 里应当一次都没有**（`K-H2a` 裁四：backend 只读）。
        assert_eq!(
            persist_sites.len(),
            0,
            "backend 里出现了「把 key 写回文件」的出口：{persist_sites:?}\n\
             裁四逐字：backend 只许读那份文件，不许写。",
        );
    }

    /// 取出一处宏调用的实参面：从 `!(` 后到配平的 `)`。
    fn macro_args(src: &str, at: usize) -> Option<&str> {
        let open = src[at..].find('(')? + at;
        let b = src.as_bytes();
        let (mut depth, mut i) = (0i32, open);
        while i < src.len() {
            match b[i] {
                b'(' => depth += 1,
                b')' => {
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

    /// 从一段实参面里挑出「被插进日志的东西」：格式串里的 `{ident}` + 串之后的位置实参。
    fn interpolated(args: &str) -> (String, Vec<String>) {
        // 第一个字符串字面量 = 格式串（`writeln!(out, "…")` 的 `out` 在它之前，天然被跳过）。
        let Some(q0) = args.find('"') else {
            return (String::new(), Vec::new());
        };
        let bytes = args.as_bytes();
        let mut i = q0 + 1;
        while i < args.len() {
            if bytes[i] == b'\\' {
                i += 2;
                continue;
            }
            if bytes[i] == b'"' {
                break;
            }
            i += 1;
        }
        let fmt = args[q0 + 1..i.min(args.len())].to_string();
        let mut out: Vec<String> = Vec::new();
        // ① 内联捕获 `{name}` / `{name:?}`。
        let mut rest = fmt.as_str();
        while let Some(a) = rest.find('{') {
            let Some(b2) = rest[a..].find('}') else { break };
            let inner = &rest[a + 1..a + b2];
            rest = &rest[a + b2 + 1..];
            let name = inner.split(':').next().unwrap_or("").trim();
            if !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()) {
                out.push(name.to_string());
            }
        }
        // ② 格式串之后的位置实参（顶层逗号分隔）。
        let tail = args.get(i + 1..).unwrap_or("");
        let mut depth = 0i32;
        let mut cur = String::new();
        for c in tail.chars() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    let t = cur.trim().to_string();
                    if !t.is_empty() {
                        out.push(t);
                    }
                    cur.clear();
                    continue;
                }
                _ => {}
            }
            cur.push(c);
        }
        let t = cur.trim().to_string();
        if !t.is_empty() {
            out.push(t);
        }
        (fmt, out)
    }

    /// ★★ `KS4`：中转记日志走**白名单**。
    #[test]
    fn every_log_line_in_the_relay_only_carries_registered_fields() {
        let root = crate::guard_support::src_root();
        let mut files = Vec::new();
        for r in LOG_ROOTS {
            let got = guard_core::scan_tree!(&root.join(r), &["rs"]);
            assert!(!got.is_empty(), "`{r}/` 一份文件都没扫到 —— 取法坏了");
            files.extend(got);
        }
        assert!(
            files.len() >= 8,
            "只扫到 {} 个文件 —— 取法坏了，本断言在空转",
            files.len()
        );

        let mut found: Vec<(String, String)> = Vec::new();
        let mut bad: Vec<String> = Vec::new();
        for (path, raw) in &files {
            // ⚠⚠ 〔`设计/20 §7` 步 1〕先前这里取的是 `file_name()`（**只有文件名**）。
            //    层 2 搬进 `relay/accounts/` 之后那样取会得出 `mod.rs` —— 一个
            //    **指不准是谁**的住址（`relay/mod.rs` 与 `relay/accounts/mod.rs` 同名）。
            //    ⇒ 改成**相对路径**。这是**收紧**：登记表里那一栏从此
            //    点得到唯一一份文件，改不改都不会让一条日志悄悄换个家。
            //    〔2026-09-24〕层 2 搬到 `src/backend/accounts/` 之后，基准从 `relay/` 换成
            //    `src/backend/`（两棵根都相对它），层 1 那几行的住址栏因此多了 `relay/` 前缀。
            let name = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let prod = production_code(raw);
            // ⚠⚠ **针不许互相包含** —— 这一行是改过一次的，经过记这里，因为它正是
            //    本工作区那族病的第一形（**针拼错**），而且是**相等断言逮住的**：
            //    第一版写的是 `["eprintln!", "println!", "writeln!"]`，
            //    而 `println!` **是 `eprintln!` 的子串** ⇒ `server.rs` 那 9 处各被数了两遍，
            //    实测「扫到 26 处，登记表 17 行」（26 = 8 + 9×2）。
            //    若当初写的是「处数 >= 登记表行数」这种松地板，这一刀**根本不会红**。
            //    今天只留两根**互不包含**的针：`println!` 顺带认得 `eprintln!`，
            //    `writeln!` 认得 `writeln!`；`print!` / `write!` 同理各认一对。
            //    ⚠ 射程：`tracing::` 那一族**不认**（今天 `relay/` 生产段零命中，如实记）。
            for m in ["print!", "println!", "write!", "writeln!"] {
                let mut from = 0usize;
                while let Some(rel) = prod[from..].find(m) {
                    let at = from + rel;
                    from = at + m.len();
                    let Some(args) = macro_args(&prod, at) else {
                        continue;
                    };
                    let (fmt, items) = interpolated(args);
                    if fmt.is_empty() {
                        continue;
                    }
                    found.push((name.clone(), fmt.clone()));
                    for it in items {
                        if !ALLOWED_LOG_ARGS.iter().any(|(a, _)| *a == it) {
                            bad.push(format!("{name}: `{it}`（出现在 “{fmt}”）"));
                        }
                    }
                }
            }
        }

        assert!(
            bad.is_empty(),
            "中转日志里插进了**没有登记**的东西：\n  {}\n\n\
             `KS4` 要的是白名单：只记允许记的那几样（方法 · 路径 · 状态码 · 字节数）。\n\
             要加就往 `ALLOWED_LOG_ARGS` 加一行**并写出理由** —— 加不出理由的多半就不该记。\n\
             ⚠ 尤其别写「记全部头，除了 Authorization」：黑名单必漏\n\
             （`Proxy-Authorization` · `X-Api-Key` · `Cookie` · 各家自定义），\n\
             而且上游多一种鉴权方式它就自动过期，没有任何东西会告诉你它过期了。",
            bad.join("\n  ")
        );

        // ★ 相等断言：处数必须等于登记表行数（**不是松地板**）。
        assert_eq!(
            found.len(),
            LOG_SITES.len(),
            "扫到 {} 处日志调用，登记表里有 {} 行 —— 加了一处日志就来加一行登记，\n\
             说清它记的是什么。扫到的是：{:#?}",
            found.len(),
            LOG_SITES.len(),
            found
        );
        // ★ 〔层 2 搬出 `relay/` 那一拍〕**两层的日志都在扫描面里**：盘上扫到日志的根
        //   ⇔ 登记表里出现的根 ⇔ `LOG_ROOTS`，三方相等。少了一棵 = 那一层的日志掉出白名单。
        let root_of = |f: &str| f.split('/').next().unwrap_or("").to_string();
        let on_disk_roots: std::collections::BTreeSet<String> =
            found.iter().map(|(f, _)| root_of(f)).collect();
        let registered_roots: std::collections::BTreeSet<String> =
            LOG_SITES.iter().map(|(f, _, _)| root_of(f)).collect();
        let want_roots: std::collections::BTreeSet<String> =
            LOG_ROOTS.iter().map(|r| (*r).to_string()).collect();
        assert_eq!(
            on_disk_roots, want_roots,
            "盘上扫到日志的根与 `LOG_ROOTS` 对不上 —— 某一层的日志掉出了扫描面（或多出一棵没登记的树）"
        );
        assert_eq!(
            registered_roots, want_roots,
            "登记表里出现的根与 `LOG_ROOTS` 对不上"
        );
        // ★ 〔`设计/90 §1.2` · `设计/20 §6` 命名推论〕**前缀按层分，两向**：
        //   住层 2（`accounts/`）的那几行 ⇔ 前缀是 `[apikey]`；其余（层 1）⇔ 前缀是 `[relay]`。
        //   `--relay` 一个进程承载两层，先前两层共用 `[relay]` ⇒ 读日志的人判不出是哪一层出的事。
        //   ⚠ 反空真：两边都得**非空**，否则「前缀按层分」在一个只剩一层的表上恒真。
        //   ⚠ 本条量的是**登记表**；登记表与盘上逐条对上由下面那一段钉着（同一条判据里）。
        let is_layer_two = |f: &str| f.starts_with("accounts/");
        let two = LOG_SITES.iter().filter(|(f, _, _)| is_layer_two(f)).count();
        let one = LOG_SITES.len() - two;
        assert!(
            two > 0 && one > 0,
            "登记表里只剩一层的日志（层 2 {two} 行 · 层 1 {one} 行）—— 下面那条「前缀按层分」在空转"
        );
        for (file, head, _) in LOG_SITES {
            assert_eq!(
                head.starts_with("[apikey] "),
                is_layer_two(file),
                "{file} “{head}”：前缀与它住的那一层对不上。\n\
                 层 2（apikey / 账号）的日志用 `[apikey]`，层 1（中转，搬字节）用 `[relay]` —— \
                 **不许用中转的名字说账号层的事**（`设计/20 §6`）。"
            );
            assert!(
                head.starts_with("[apikey] ") || head.starts_with("[relay] "),
                "{file} “{head}”：前缀两个都不是"
            );
        }
        // 逐条对上（不是只对数量 —— 数量对得上而内容换了一批，那也是漂移）。
        for (file, head, _) in LOG_SITES {
            assert!(
                found
                    .iter()
                    .any(|(f, fmt)| f == file && fmt.starts_with(head)),
                "登记表里的这一行在盘上找不到了：{file} “{head}”"
            );
        }
    }
}
