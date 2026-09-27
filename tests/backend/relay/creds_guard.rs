//! `K-H2a` 的两条机检：**明文出口恰好一处**（`KS2`）· **记日志走白名单**（`KS4`）。
//!
//! # 为什么单住一个文件
//!
//! 与隔壁 `bind_guard.rs`（以及〔AR1〕已退役的 `nodelay_guard.rs`）同一个理由，它的头注逐字写着：
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
//! 1. **只扫 `LOG_ROOTS` 那两棵**（`relay/` 与 `accounts/upstream/` —— `--relay` 进程两层的生产段）。
//!    这两棵之外别处印了什么，本条不管（key 也只经过这两棵：上游选择取明文算头材料，中转只拿算好的头）。
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
        (
            "upstream_failure",
            "中转传输失败那一行（`server::UpstreamFailure::for_log`，`设计/20 §3.1a`）：\
             上游的**主机与端口** ＋ 一句固定文案（卡在哪一跳）＋ 底层那条 `io::Error`。\
             ⚠ 刻意**不带**基址里的路径前缀（凭据文件内容，同 `note.what` 那条理由）、\
             不含请求头、与任何一把 key 无关 —— 这几格由 `UpstreamFailure` 的字段集兜着",
        ),
        ("a", "监听地址（`local_addr()`）"),
        (
            "status",
            "〔RK1〕门拒绝那一格的**状态行**（`door::FORBIDDEN` / `door::MISDIRECTED` 两个常量之一）—— 不含请求里的任何字节",
        ),
        ("port", "端口号"),
        (
            "INFLIGHT_CONNECTIONS",
            "在途连接数上限，一个编译期常量",
        ),
        ("p", "凭据文件读不动 / 解析不了时的说法（`store::StoreError` 的文本，不含 key）"),
        ("how", "权限过宽宽在哪 —— `perm::judge` 造的句子，只含 mode 位 / SDDL 主体名"),
        ("fix", "怎么修 —— 一句固定的指引"),
        (
            "reason",
            "〔RL1〕常驻后端进程内中转起不来的那句「为什么」（`relay::listen::Hosted::Failed`）：\
             只由 `listen::prepare` / `listen::host` 造，内容是端口号、`CCM_RELAY_PORT` 那一格的原串与 io 错误文本 —— \
             不经过任何请求、不碰凭据文件内容",
        ),
        ("why", "为什么查不出权限 —— 只含平台与构建 feature"),
        (
            "loaded.path.display()",
            "凭据文件的路径。**`KS9` 的「路径文档化」就落在这一行**：一个「能手编但没人知道在哪」的文件等于不能手编",
        ),
        (
            "store::template()",
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
            // 〔`设计/20 §3.1a`〕先前是 `[relay] upstream connect failed`，只管「连不上」一支；
            //   今天等响应那四支（没回应就断 · 读出错 · 不是 HTTP · 只有 1xx）也走这一行 ⇒ 回 504。
            "relay/server.rs",
            "[relay] upstream failed",
            "上游连不上 / 没回应 / 回的不是 HTTP（中转自己的传输失败，回 504）",
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
            "[relay] refusing to listen without a relay key",
            "〔RK1〕绑上口之后拿不到钥匙（家目录解析不出 / 铸不出 / 写不进）⇒ 不起。只带路径与 io 错误文本，**永远没有钥匙值**",
        ),
        (
            "relay/server.rs",
            "[relay] refused at the door",
            "〔RK1〕进门三问拒了一条（Origin / Host 非回环 / 钥匙不对）。只印状态行，**不印请求头与路径**（路径里可能正是一把错钥匙）",
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
            "relay/listen.rs",
            "[relay] not hosted",
            "〔RL1〕常驻后端被交了一个认不出的中转端口 ⇒ 不开中转、后端照常",
        ),
        (
            "relay/listen.rs",
            "[relay] cannot spawn accept thread",
            "〔RL1〕常驻后端进程内中转的接受线程起不来",
        ),
        (
            "relay/listen.rs",
            "[relay] 中转住本进程，听",
            "〔RL1〕`Hosted` 的说法（`Display`）：进程内中转在听哪个地址 —— 宿主（`main.rs` 流模式那一处）记进它的日志",
        ),
        (
            "relay/listen.rs",
            "[relay] 被交了中转端口却起不来",
            "〔RL1〕`Hosted` 的说法（`Display`）：进程内中转起不来、后端照常服务",
        ),
        (
            // ⚠ 〔`设计/20 §7` 步 1〕它**搬家了**：热重载整块归上游选择，住址从
            //   `server.rs` 变成 `accounts/mod.rs`。话一个字没改。
            "accounts/upstream/mod.rs",
            "[apikey] 凭据文件读不成表，**保留上一张表不动**",
            "`D2 阻-2`：重载时解析失败 —— **不把表换成空**（空表 = 全部 404），\
             留住上一张能用的、只出声。这一形是「表可重载」之后新长出来的",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials file:",
            "凭据文件在哪（`KS9` 路径文档化）",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials problem:",
            "文件读不动 / 解析不了",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials permissions too wide:",
            "权限过宽（`KS11`）",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] how to fix:",
            "怎么修（`KS11` 要求两样都有）",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials permissions unknown:",
            "查不出权限，也要出声",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials: this account cannot be used:",
            "一条账号进不了路由表（id 当不了路由段 / `base_url` 解析不了）—— \
             `K-H2`：静默丢一行的症状是「我明明配了，请求永远 404」",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials: configured",
            "配了 —— **只印这个布尔与进得了表的条数**，不印长度、不印掩码",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials: auth_style must be one of:",
            "有一条的 `auth_style` 认不出时，把认得的那几个**现算**着印出来（`K-R1`）——\
             它是给正在排错的人看的最后一句话，所以不许是一份会变旧的字面量清单",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials: this account is not on the default path:",
            "一条**进了表、但行为与默认不同**的账号（`K-R1`：带了路径前缀 / 换了鉴权头形状）。\
             它与上面那条「cannot be used」是两件事：这一条**照发**，只是发出去的字节不同 ⇒ \
             它错了的症状是上游的 404 / 401，与「上游挂了」同形，必须在启动时说出来",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] credentials: not configured",
            "没配",
        ),
        (
            "accounts/upstream/creds.rs",
            "[apikey] create that file to configure one",
            "没配时印模板",
        ),
    ];

    /// 日志白名单的人群：**两层都在** —— 中转（`relay/`）与上游选择（`accounts/`）。
    ///
    /// ⚠ 〔2026-09-24 上游选择搬出 `relay/`〕先前人群是 `relay/` 一棵树，上游选择住在它底下所以顺带被扫。
    ///   搬走之后只扫 `relay/` 的话，上游选择那 12 行日志会**掉出扫描面** —— 它们记的恰恰是
    ///   凭据文件那一侧的事，是 `KS4` 最该看着的那一批。⇒ 两棵根明写在这里，
    ///   并由判据本体断言「盘上有日志的根 ⇔ 登记表里出现的根 ⇔ 本表」三方相等。
    // 〔`A3` 第二波〕上游选择的根从 `accounts` 收窄成 `accounts/upstream`：`accounts/` 是账号**域**，
    // 其中 `iso.rs`（账号隔离工具的查询）**不是**中转的上游选择，不进本白名单的人群。
    const LOG_ROOTS: &[&str] = &["relay", "accounts/upstream"];

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

    /// ★★★ **`K-H2a` 裁四那句话的判据** —— 〔RM1a · 第四波〕裁四**收窄**之后，本条改钉收窄后的那一句。
    ///
    /// # 那句话怎么变的
    ///
    /// 原话：「本 crate 不许打开 `creds-core` 的 `harden` feature」—— 那时「backend 写不了这份文件」
    /// 是**编译器**兜的（`make_private` / `create_private` 都挂在 `harden` 上，不开就不存在）。
    /// 远端那台机器上的 key 只能由那台的后端写（上游选择自己的状态文件，`调研/第四波记录/RM1a.md §1`）
    /// ⇒ feature 开了。编译器那一格没了，**两条判据接住**：
    ///
    /// 1. **本条**：`harden` 在本 crate 那份 manifest 里只从**一处声明**进来 —— `creds-core` 那一行
    ///    （不许从 `[features]` 段、`default-features`、第二条重复声明里悄悄多开一路）。
    /// 2. `readonly_guard::g6_dependency_signoff::the_credentials_write_half_is_reached_only_from_the_account_file_face`：
    ///    写半边在本 crate 生产段里的引用处 == 第四层登记的那一份（两向相等）。
    ///
    /// ⚠ **非空对照照旧承重**：同一把尺子量 monitor 那份 manifest ⇒ 必须**数得到** `harden`。
    ///
    /// ⚠ 它**认不出**什么：`--features harden` 从**命令行**传进来（那条路不经 manifest）。
    #[test]
    fn this_crate_turns_on_the_write_half_of_the_credentials_crate_in_exactly_one_declaration() {
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

        // ⚠ **剥掉 `#` 注释行再数**：manifest 里有几行注释逐字提到这个词（解释为什么开），
        //   判据只该看生效的声明，不该看解释它的话。
        let mine = guard_core::strip_hash_comment_lines(&mine);
        let feature = format!("har{}", "den");
        let lines: Vec<&str> = mine.lines().filter(|l| l.contains(&feature)).collect();
        assert_eq!(
            lines.len(),
            1,
            "本 crate 的 `Cargo.toml` 里生效的 `{feature}` 出现在 {} 行，应当**恰好 1** 行（`creds-core` 那一行）：{lines:?}\n\
             ⚠ 裁四收窄后：backend 只有账号域那一份写这份文件；多一处声明 = 多一条没人看着的路。",
            lines.len()
        );
        assert!(
            lines[0].trim_start().starts_with("creds-core = {"),
            "开 `{feature}` 的那一行不是 `creds-core` 的依赖声明：{}",
            lines[0]
        );

        // ★ 非空对照：同一把尺子量 `creds-core` 自己那份 manifest（定义这个 feature 的那一行），**必须数得到**。
        //   〔US1 · 4D〕先前量的是 monitor 那份（它当时也开着）；monitor 从此不读不写这份文件、不开它 ⇒ 反过来钉「monitor 零处」。
        let read_manifest = |rel: &str| {
            guard_core::strip_hash_comment_lines(
                &std::fs::read_to_string(crate::guard_support::repo_root().join(rel))
                    .unwrap_or_else(|e| panic!("读不到 {rel}：{e}")),
            )
        };
        let defining = read_manifest("src/bridge/crates/creds-core/Cargo.toml");
        assert!(
            defining.lines().any(|l| l.trim_start().starts_with(&format!("{feature} = ["))),
            "非空对照失败：`creds-core` 那份 manifest 里数不到定义 `{feature}` 的那一行 —— 这把尺子是瞎的"
        );
        let monitor = read_manifest("src/bridge/Cargo.toml");
        assert!(
            !monitor.lines().any(|l| l.contains(&feature)),
            "monitor 又开了 `{feature}` —— 它不写也不读这份文件（写者与读者都是那台后端），写半边该由编译器挡在它外面"
        );
    }

    /// ★★ **焊缝：我可能自己写出来的每一个头名，都在「先丢掉」那个集合里。**
    ///
    /// 〔`P16` 2026-09-22 从 `server_tests.rs` **搬到这一侧**〕缺这一焊的症状是
    /// **同名鉴权头出现两次** —— HTTP 允许同名头出现多次，上游谁赢**没有定义**，
    /// 而一把不属于这一行的客户端凭据就这样被多送出去一次。
    ///
    /// # 为什么它今天住上游选择这一侧
    ///
    /// 焊的两端先前都住中转（`AUTH_HEADER_NAMES` ＋ `auth_header_of`）。`P16` 把映射
    /// 按 `C2` 搬去上游选择、把名单改成**由映射派生**之后，两端都在上游选择
    /// ⇒ 判据跟着搬，焊缝不跨层（一条跨层焊缝正是 `D1` 警告的形状）。
    ///
    /// # 🔴 它判的是**产物**，不是两个定义 —— 否则它就是恒真
    ///
    /// 名单既然是从映射派生的，「名单 ⊇ 映射写得出的头名」在**定义**那一层已经
    /// 按构造成立 ⇒ 拿两个定义对拍就是**两侧同源的恒等 = 恒真**，本仓逐字禁过。
    /// ⇒ 本条改判**真走一遍上游选择拿到的那个 `AuthSwap`**：
    ///
    /// 1. 每一种 `AuthStyle` 各配一行，过**生产段那条真实的** `accounts::upstream::decide`；
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
    /// - **不买「中转真的照着 `clear` 丢了」** —— 那由 `server_tests` 那一族的字节判据
    ///   （`the_auth_header_shape_follows_the_row_and_not_a_process_wide_guess`）钉着。
    #[test]
    fn every_header_this_relay_may_write_is_in_the_set_it_clears_first() {
        use creds_core::store::AuthStyle;
        use creds_core::SecretKey;

        // 期望的全集：**判据这边自己**从闭集派生一遍（与生产段那一份异源）。
        let want_clear: std::collections::BTreeSet<String> = AuthStyle::ALL
            .iter()
            .filter_map(|s| crate::accounts::upstream::auth_header_of(*s))
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
            let table = crate::accounts::upstream::table::RoutingTable::build(std::iter::once((
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
            let upstreams =
                crate::accounts::upstream::Upstreams::from_env(&|_| None).expect("内置默认");
            crate::accounts::upstream::decide(
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
                "{style:?} 那一行交给中转的「先丢掉」集合与全集对不上。\n\
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
        // ⚠ 〔`P16` 2026-09-22〕住址从 `server.rs`（中转）换成 `accounts/mod.rs`（上游选择）——
        //    **换的是住址，不是处数**：上面那条「恰好 1」的相等断言一个字节都没动。
        //    搬的理由：中转的类型面上不许再出现 `creds-core` 的类型（`C2`），
        //    而「把 key 拼成头值」必须拿着 `SecretKey` ⇒ 它只能在上游选择。
        //    ⇒ 中转从此**碰不到明文**。这一格因此比先前**更紧**，不是搬松了。
        assert!(
            header_sites[0].ends_with(".rs:0")
                || header_sites[0].contains("accounts/upstream/mod.rs"),
            "唯一那处不在 `accounts/upstream/mod.rs`（上游选择拼头值那一行）而在 {} —— 靶子挪了。\n\
             ⚠ 它**不许**回到中转：那会让 `creds-core` 的类型重新爬上中转的类型面（`C2`）。",
            header_sites[0]
        );
        // ★ 另一半：**落盘那个明文出口在本 crate 里应当一次都没有**。
        //   〔RM1a〕裁四收窄后本 crate 有了一个写凭据文件的模块（`accounts/upstream/file_face.rs`），
        //   但它**不自己取明文**：拼落盘文本走 `creds_core::store::merge_account_key`，
        //   明文出口（`expose_for_persisting`）仍只在 `creds-core` 里那一处 ⇒ 本 crate 这边照旧 0。
        assert_eq!(
            persist_sites.len(),
            0,
            "backend 里出现了「自己取明文写回文件」的出口：{persist_sites:?}\n\
             写凭据文件只许经 `creds_core::store` 的纯函数拼，本 crate 不许自己取明文。",
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
            //    上游选择搬进 `relay/accounts/` 之后那样取会得出 `mod.rs` —— 一个
            //    **指不准是谁**的住址（`relay/mod.rs` 与 `relay/accounts/mod.rs` 同名）。
            //    ⇒ 改成**相对路径**。这是**收紧**：登记表里那一栏从此
            //    点得到唯一一份文件，改不改都不会让一条日志悄悄换个家。
            //    〔2026-09-24〕上游选择搬到 `src/backend/accounts/` 之后，基准从 `relay/` 换成
            //    `src/backend/`（两棵根都相对它），中转那几行的住址栏因此多了 `relay/` 前缀。
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
        // ★ 〔上游选择搬出 `relay/` 那一拍〕**两层的日志都在扫描面里**：盘上扫到日志的根
        //   ⇔ 登记表里出现的根 ⇔ `LOG_ROOTS`，三方相等。少了一棵 = 那一层的日志掉出白名单。
        // 〔`A3` 第二波〕根不再都是一段（`accounts/upstream` 两段）⇒ 按 `LOG_ROOTS` 认前缀，不按第一段切。
        let root_of = |f: &str| {
            LOG_ROOTS
                .iter()
                .find(|r| f.starts_with(&format!("{r}/")))
                .map(|r| (*r).to_string())
                .unwrap_or_else(|| f.split('/').next().unwrap_or("").to_string())
        };
        let on_disk_roots: std::collections::BTreeSet<String> =
            found.iter().map(|(f, _)| root_of(f)).collect();
        let registered_roots: std::collections::BTreeSet<String> =
            LOG_SITES.iter().map(|(f, _, _)| root_of(f)).collect();
        let want_roots: std::collections::BTreeSet<String> =
            LOG_ROOTS.iter().map(|r| (*r).to_string()).collect();
        // 〔`A3` 第二波〕白名单圈的非中转那一棵，**恰好**是上游选择的根（盘上现推，住 `upstream_selection_guard`）——
        //   不是整个账号域：`accounts/iso.rs`（账号隔离工具的查询）不是中转的上游选择，不进本白名单。
        let sel_root =
            super::super::upstream_selection_guard::tests::upstream_selection_root_from_disk();
        let non_relay: Vec<&str> = LOG_ROOTS
            .iter()
            .copied()
            .filter(|r| *r != "relay")
            .collect();
        assert_eq!(
            non_relay,
            vec![sel_root.trim_end_matches('/')],
            "中转日志白名单圈的账号那棵（`LOG_ROOTS`）不等于上游选择的根 `{sel_root}` —— \
             圈大了就把账号域里别的块（iso）也算成了中转的上游选择"
        );
        assert_eq!(
            on_disk_roots, want_roots,
            "盘上扫到日志的根与 `LOG_ROOTS` 对不上 —— 某一层的日志掉出了扫描面（或多出一棵没登记的树）"
        );
        assert_eq!(
            registered_roots, want_roots,
            "登记表里出现的根与 `LOG_ROOTS` 对不上"
        );
        // ★ 〔`设计/90 §1.2` · `设计/20 §6` 命名推论〕**前缀按层分，两向**：
        //   住上游选择（`accounts/`）的那几行 ⇔ 前缀是 `[apikey]`；其余（中转）⇔ 前缀是 `[relay]`。
        //   `--relay` 一个进程承载两层，先前两层共用 `[relay]` ⇒ 读日志的人判不出是哪一层出的事。
        //   ⚠ 反空真：两边都得**非空**，否则「前缀按层分」在一个只剩一层的表上恒真。
        //   ⚠ 本条量的是**登记表**；登记表与盘上逐条对上由下面那一段钉着（同一条判据里）。
        let is_upstream_selection = |f: &str| f.starts_with("accounts/upstream/");
        let two = LOG_SITES
            .iter()
            .filter(|(f, _, _)| is_upstream_selection(f))
            .count();
        let one = LOG_SITES.len() - two;
        assert!(
            two > 0 && one > 0,
            "登记表里只剩一层的日志（上游选择 {two} 行 · 中转 {one} 行）—— 下面那条「前缀按层分」在空转"
        );
        for (file, head, _) in LOG_SITES {
            assert_eq!(
                head.starts_with("[apikey] "),
                is_upstream_selection(file),
                "{file} “{head}”：前缀与它住的那一层对不上。\n\
                 上游选择（apikey / 账号）的日志用 `[apikey]`，中转（搬字节）用 `[relay]` —— \
                 **不许用中转的名字说上游选择的事**（`设计/20 §6`）。"
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
