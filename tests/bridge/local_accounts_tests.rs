use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// 每个测试独占的临时目录。仓库约定不引 `tempfile`，用 pid + 计数器保唯一。
/// **测试绝不碰用户真实的 `~/.claude-accts`** —— 全部在这里面。
struct Sandbox(PathBuf);
static SEQ: AtomicU64 = AtomicU64::new(0);
impl Sandbox {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "l3a-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&p).expect("mkdir sandbox");
        Sandbox(p)
    }
    /// 写 manifest。**文件名写死成字面量，刻意不用 `MANIFEST_NAME`。**
    ///
    /// U7-4：此前这里是 `self.0.join(MANIFEST_NAME)` —— 测试的**写侧**与生产的**读侧**
    /// 用同一个常量，常量一起变，测试**结构上不可能因为它变了而失败**。
    /// U7-3 实测：把内核里的 `MANIFEST_NAME` 改成 `"accts.json"`，daemon 红了 9 条，
    /// monitor **全绿**。那不是「没测到」，是「测不到」。
    ///
    /// 常量是**实现**，文件名是**契约**（bash 写侧 / daemon / 本机三方共用）。
    /// 测试该钉契约，所以这里用字面量。
    fn write_manifest(&self, json: &str) {
        std::fs::write(self.0.join("accounts.json"), json).expect("write manifest");
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 〔audit-0805 08-06〕**`read_capped` 的三种失败必须仍然分得开**，外加上限的边界语义。
///
/// # 为什么这条值得钉
///
/// `ROADMAP §5` 的 3j 复核结论逐字是：这三种（文件不存在 / 不是普通文件 / 过大）
/// 走的是同一个 `Err` 臂，**但理由串是分得开的**，而设置面板真的把它渲染出来
/// （`renderNotEnabled` 那行「原因：…」）。⇒ 那句「分得开」**此前没有任何判据读它** ——
/// 谁把三条消息合成一句「读不了」，前端就退回一个没有身份的失败（E4 要治的正是这个），
/// 而**没有东西会红**。
///
/// 顺带钉上限的边界：`meta.len() > cap` ⇒ **正好等于 cap 是放行的**。
/// E5 要求「上限与超限语义成对定义」，而 `>` 与 `>=` 的差别正是这一对里最容易滑的一格。
///
/// ⚠ 全程在 `Sandbox` 的临时目录里，**绝不碰用户真实的 `~/.claude-accts`**（同本文件既有约定）。
#[test]
fn read_capped_keeps_its_three_failures_distinguishable() {
    let sb = Sandbox::new();

    // ① 不存在：理由里要有路径，且**不能**冒充另外两种。
    let missing = sb.0.join("nope.json");
    let e = read_capped(&missing, 1024).expect_err("不存在的文件必须是 Err");
    assert!(
        e.contains("nope.json"),
        "理由里没有路径，用户看不出是哪一个：{e}"
    );
    assert!(
        !e.contains("不是普通文件") && !e.contains("过大"),
        "「不存在」被说成了另一种失败：{e}"
    );

    // ② 不是普通文件（目录）。
    let dir = sb.0.join("adir");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let e = read_capped(&dir, 1024).expect_err("目录必须是 Err");
    assert!(e.contains("不是普通文件"), "目录没有得到自己那条理由：{e}");

    // ③ 过大：理由里要带**实际字节数**（不然用户不知道差多少）。
    let big = sb.0.join("big.json");
    std::fs::write(&big, vec![b'x'; 10]).expect("write");
    let e = read_capped(&big, 4).expect_err("超限必须是 Err");
    assert!(e.contains("过大"), "超限没有得到自己那条理由：{e}");
    assert!(e.contains("10"), "理由里没带实际字节数：{e}");

    // ④ 三条理由**两两不同** —— 合并成一句就在这里红。
    let e_missing = read_capped(&missing, 1024).unwrap_err();
    let e_dir = read_capped(&dir, 1024).unwrap_err();
    let e_big = read_capped(&big, 4).unwrap_err();
    assert!(
        e_missing != e_dir && e_dir != e_big && e_missing != e_big,
        "三种失败给了相同的理由串，前端只能显示一个没有身份的「读不了」：\n               不存在={e_missing}\n  目录={e_dir}\n  过大={e_big}"
    );

    // ⑤ 边界：正好等于上限**放行**（`>` 不是 `>=`）；差一个字节就拒。
    let exact = sb.0.join("exact.json");
    std::fs::write(&exact, vec![b'y'; 8]).expect("write");
    assert_eq!(
        read_capped(&exact, 8).expect("正好等于上限应当放行"),
        vec![b'y'; 8],
        "读回来的内容与写进去的不一致"
    );
    assert!(
        read_capped(&exact, 7).is_err(),
        "超出一个字节没被拒 —— 上限那一格滑了"
    );
}

#[test]
fn no_manifest_is_not_an_error_just_disabled() {
    let sb = Sandbox::new();
    let r = list_from_dir(&sb.0);
    assert!(r.available, "「本机没启用多账号」是正常状态，不是能力缺失");
    assert!(!r.meta.as_ref().unwrap().enabled);
    assert!(r.meta.as_ref().unwrap().error.is_some(), "要给出人话原因");
    assert!(r.accounts.is_empty());
}

#[test]
fn bad_json_and_bad_schema_are_reported_not_panicked() {
    let sb = Sandbox::new();
    sb.write_manifest("{ not json");
    assert!(!list_from_dir(&sb.0).meta.unwrap().enabled);
    sb.write_manifest(r#"{"version":99,"accounts":[]}"#);
    let m = list_from_dir(&sb.0).meta.unwrap();
    assert!(!m.enabled);
    assert!(m.error.unwrap().contains("99"));
}

/// ★ Z01：**`configDir` 键缺席 = 账号 0**，判据是结构性的，不认名字。
#[test]
fn account_zero_is_the_absent_key_not_a_name_and_not_an_empty_string() {
    let sb = Sandbox::new();
    let shared = sb.0.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join(CREDENTIALS_NAME), "{}").unwrap();
    sb.write_manifest(&format!(
        r#"{{"version":1,"sharedStore":{shared:?},"accounts":[
                 {{"name":"zero","mode":"bare"}},
                 {{"name":"empty","configDir":""}},
                 {{"name":"named-0","configDir":{cfg:?}}}
               ]}}"#,
        shared = shared.to_string_lossy(),
        cfg = sb.0.join("acct-a").to_string_lossy(),
    ));
    let r = list_from_dir(&sb.0);
    let names: Vec<&str> = r.accounts.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["zero", "named-0"],
        "空串那条应被丢弃：空值 ≠ 未设"
    );

    let zero = &r.accounts[0];
    assert!(zero.config_dir.is_none(), "账号 0 对外出 None，不是空串");
    assert!(zero.exists, "「裸起」这个状态永远可达");
    assert!(zero.logged_in, "账号 0 的登录态查共享库");
    // 名字叫什么都行——判据是键在不在，不是名字。
    assert_eq!(zero.name, "zero");
}

#[test]
fn logged_in_is_existence_only_and_unknown_is_false() {
    let sb = Sandbox::new();
    let a = sb.0.join("acct-a");
    std::fs::create_dir_all(&a).unwrap();
    sb.write_manifest(&format!(
        r#"{{"version":1,"accounts":[
                 {{"name":"a","configDir":{a:?}}},
                 {{"name":"gone","configDir":{gone:?}}},
                 {{"name":"zero"}}
               ]}}"#,
        a = a.to_string_lossy(),
        gone = sb.0.join("nope").to_string_lossy(),
    ));
    let r = list_from_dir(&sb.0);
    assert!(!r.accounts[0].logged_in, "目录在但没有凭据文件 ⇒ false");
    assert!(r.accounts[0].exists);
    assert!(!r.accounts[1].exists, "目录不在 ⇒ exists false");
    // 账号 0 且 manifest 没写 sharedStore ⇒ 探不到 ⇒ false（「不知道」，不假装已登录）
    assert!(!r.accounts[2].logged_in);

    // 同 `write_manifest`：文件名写死成字面量，刻意不用 `CREDENTIALS_NAME`。
    // 用常量的话，测试写哪个文件、生产找哪个文件会一起变 ⇒ 测不出常量漂移。
    std::fs::write(a.join(".credentials.json"), "{}").unwrap();
    assert!(list_from_dir(&sb.0).accounts[0].logged_in);
}

#[test]
fn unsafe_config_dirs_are_dropped_not_fatal() {
    let sb = Sandbox::new();
    for bad in [
        "relative/path",
        "/",
        "/a/../b",
        "/a/b;id",
        "/a/$(id)",
        "/a/\u{202E}b",
        "",
    ] {
        assert!(!is_safe_config_dir(bad), "应判不安全: {bad:?}");
    }
    for good in [
        "/home/u/.claude-accts/a",
        "C:\\Users\\u\\accts\\a",
        "\\\\srv\\share\\a",
    ] {
        assert!(is_safe_config_dir(good), "应判安全: {good:?}");
    }
    // 坏条目只丢自己，不拖垮整表。
    sb.write_manifest(
        r#"{"version":1,"accounts":[{"name":"bad","configDir":"rel"},{"name":"zero"}]}"#,
    );
    let r = list_from_dir(&sb.0);
    assert_eq!(r.accounts.len(), 1);
    assert_eq!(r.accounts[0].name, "zero");
}

/// ★ U7-4：**欺骗字符的覆盖面**，逐组各取一个代表。
///
/// # 这条为什么单独立一件事做
///
/// U7-3 把 `is_deceptive_char` 抽进 `acct-core` 时做变异验证：
/// 删掉内核里的 NEL（`U+0085`）⇒ acct-core 红、daemon 红、**monitor 全绿**。
/// 当时如实登记了原因 —— 不是接线没生效，是本模块**只测过 `U+202E` 一个码位**，
/// 而那恰好是两侧本来都有的。
///
/// 我刻意**没在那次重构里顺手补** —— 补测试要单独设计，
/// 混在重构里做等于用新写的测试给新写的代码背书。
///
/// # 判据：按**来源分组**取代表，不是堆码位
///
/// 每组各一个，任何一组从内核里掉出去，本测试立刻红：
#[test]
fn every_group_of_deceptive_characters_is_rejected_in_a_config_dir() {
    // (码位, 这一组是什么, U7-3 之前谁缺它)
    let groups: &[(char, &str, &str)] = &[
        // ⚠ NEL 属 Cc 类，`is_control()` 本来就挡着它 ⇒ 把它从内核集合里删掉
        // **不会**让本测试红。U7-3 我曾把「本机缺 NEL」当安全洞报出来，U7-4 实测证伪：
        // 集合确实差过一项，可观察行为没差。留在表里是为了这条注记本身。
        (
            '\u{0085}',
            "NEL（C1 换行；is_control 已覆盖，非真洞）",
            "集合差过、行为没差",
        ),
        ('\u{00A0}', "NBSP", "两侧都有"),
        ('\u{1680}', "Ogham space mark", "daemon 缺"),
        ('\u{2003}', "各类空格（U+2000..200A）", "daemon 缺"),
        ('\u{200B}', "零宽空格/连接符", "两侧都有"),
        ('\u{2028}', "行分隔", "两侧都有"),
        ('\u{202E}', "双向覆盖（RLO）", "两侧都有"),
        ('\u{202F}', "narrow NBSP", "daemon 缺"),
        ('\u{205F}', "medium mathematical space", "daemon 缺"),
        ('\u{2060}', "word joiner / 不可见运算符", "daemon 缺"),
        ('\u{2066}', "双向隔离", "两侧都有"),
        ('\u{3000}', "ideographic space", "daemon 缺"),
        ('\u{FEFF}', "ZWNBSP / BOM", "两侧都有"),
    ];
    assert!(
        groups.len() >= 13,
        "分组表被削短了（{} 组）—— 本断言在空转",
        groups.len()
    );
    for (c, what, who) in groups {
        let path = format!("/home/u/.claude-accts/a{c}b");
        assert!(
            !is_safe_config_dir(&path),
            "U+{:04X}（{what}；U7-3 之前{who}）没被挡下 —— \n\
                 它能在 UI 里把账号名/路径伪造成另一个样子。",
            *c as u32
        );
    }
    // 反向：去掉欺骗字符之后同一条路径必须**通过**，否则上面全是空转。
    assert!(
        is_safe_config_dir("/home/u/.claude-accts/ab"),
        "干净路径被误判成不安全 —— 上面那些断言全都不算数了"
    );
}

/// ★ U7-4：账号库目录名是**契约**，写死成字面量核对。
///
/// `local_accts_dir()` 拼的是 `$HOME/<ACCTS_DIR_NAME>`。此前没有任何测试碰它 ——
/// 常量改了、本机就去别处找账号库，而 UI 上的表现只是「一个账号都没有」。
#[test]
fn the_accounts_library_lives_under_the_contract_directory_name() {
    let d = local_accts_dir().expect("取不到 HOME —— 本断言在空转");
    assert_eq!(
        d.file_name().and_then(|s| s.to_str()),
        Some(".claude-accts"),
        "账号库目录名变了。这是 bash 写侧 / daemon / 本机三方共用的契约名，\n\
             改了它本机就去别处找账号库，UI 上只表现为「一个账号都没有」。"
    );
}

// U7-3：**那条读对面源文件的跨 crate 契约守卫已退役。**
//
// 它是真的（剥注释、剥测试段、有字节地板与锚点自检，注释里还记着第一版是安慰剂、
// 被变异证伪后修好）—— 但守卫只能**发现**漂移。四个常量与 `is_deceptive_char`
// 现在都住在共享 crate `acct-core`，两侧 import 同一份 ⇒ 漂移**不可表示**，
// 想不一致得先把 import 删掉。
//
// 这是 U6b-3 那条横切约定的又一次应用：判据能被绕过时，先问「能不能让它不可表示」。
// 不可表示之后，判据本身是死重量。
//
// ⚠ **没有一并合掉的两个同名函数**：`is_safe_config_dir` 与 `norm_dir`。
//
// 🔴 **`N-F1c`（09-05）订正上半句的理由**：那句话原先逐字写着
// 「本机侧要认 Windows 盘符（`looks_absolute`）且必须允许 `\` 作分隔符，
//  所以改成拒 `\..\`；daemon 是 Linux-only，直接把 `\` 当危险字符拒掉。
//  硬合只能二选一：要么本机失去 Windows 路径，要么 daemon 失去对 `\` 的拒绝。」
// —— **后半段今天不成立了**：本机读口改问后端之后，那个「Linux-only」的前提没了
// （同一个二进制要在 Windows 上答本机的账号），于是 daemon 那份**也**按同一句话
// 拆成了「安全性质 + 平台形式」，两边的字符集现在逐字相同。
// ⇒ 「二选一」那个两难是**假的**：它假设了 daemon 只跑 Linux。
//
// ⚠ 那**仍然不等于该合**（两条理由，都还硬着）：
//   ① daemon crate 是 bin-only、刻意不进 workspace，import 不了 `acct-core` 之外的东西
//      —— 而这两个函数要合就得先有个共同的家，`acct-core` 是唯一候选；
//   ② `norm_dir` 两侧仍然真的不同（本机多剥一层 `\`），合它是另一件事。
// ⇒ 合并条件写清：把这两个函数搬进 `acct-core`（连同它们的判据），两侧 import 同一份。
// ⚠ **同一句订正在 `acct-core` 的模块头注里还有一份没改** —— 那份文件不在 `N-F1c`
//   的写区里（本件只许动 daemon 那一个函数），已按诚实边界交回 PM。

// ---- K-A1：鉴权方式这一维（生产者②） ----

/// ★ **跨生产者对拍，本机这一半。**
///
/// 喂的是 `acct_core::auth_kind_parity_manifest`（daemon 那半喂的是**同一个函数**
/// 的输出），断的是 `acct_core::AUTH_KIND_PARITY_CASES` 里手写的金样。
/// ⇒ 两个生产者里任意一个自己填一个默认值，它那半当场红。
/// daemon 那半住 `src/backend/observe/accounts_query.rs::
/// tests::auth_kind_parity_daemon_side`。
///
/// ⚠ 射程如实写（`KA6c`）：**只覆盖 `authKind` / `authReady` 这一维**。
/// 其余 6 个字段今天仍是两份实现各写一遍，这条对拍看不见它们漂。
#[test]
fn auth_kind_parity_local_side() {
    let sb = Sandbox::new();
    for c in &acct_core::AUTH_KIND_PARITY_CASES {
        let d = sb.0.join(c.name);
        std::fs::create_dir_all(&d).unwrap();
        if c.credentials_present {
            // 同 `write_manifest`：文件名刻意写死，别用常量（否则测不出常量漂移）。
            std::fs::write(d.join(".credentials.json"), "{}").unwrap();
        }
    }
    std::fs::create_dir_all(sb.0.join("shared")).unwrap();
    // ⚠ `auth_kind_parity_manifest` 是**手搓 JSON**：它把 `root` 直接 `push_str` 进一个
    //   JSON 串字面量里，自己不做转义。Windows 上沙箱根长成
    //   `C:\Users\…\AppData\Local\Temp\l3a-…` —— `\U` / `\A` / `\L` / `\T` 一个都不是
    //   合法 JSON 转义 ⇒ **整份 manifest 解析失败** ⇒ `list_from_dir` 走
    //   「manifest 不是合法 JSON」那条臂、回 0 个账号。
    //   〔09-09 云端 windows-latest 首跑实得 `left: 0 / right: 6`，正是这一形。〕
    //   ⇒ 这里按 JSON 的规矩把 `\` 转义掉。**刻意不改成正斜杠绕开**：
    //   manifest 里仍然是原生 Windows 路径，而产品那一侧要认的就是带 `\` 的那一种
    //   （`is_safe_config_dir` 明写反斜杠是 Windows 分隔符、不在拒绝集里）——
    //   换成 `/` 等于让本条在 Windows 上不再驱动那一形。
    //   Linux 上路径不含 `\` ⇒ 这一行是恒等变换，那一侧一个字节没变。
    let root_json = sb.0.to_string_lossy().replace('\\', "\\\\");
    sb.write_manifest(&acct_core::auth_kind_parity_manifest(&root_json));
    let r = list_from_dir(&sb.0);
    assert_eq!(
        r.accounts.len(),
        acct_core::AUTH_KIND_PARITY_CASES.len(),
        "账号数对不上，逐格断言会漏掉没出来的那几个：{:?}",
        r.accounts.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
    for (i, c) in acct_core::AUTH_KIND_PARITY_CASES.iter().enumerate() {
        let a = &r.accounts[i];
        assert_eq!(a.name, c.name, "顺序变了，下面几格就对错人了");
        assert_eq!(
            a.auth_kind.map(|k| k.as_contract_str()),
            Some(c.expect_auth_kind),
            "{}：本机产出的 authKind 与金样不一致",
            c.name
        );
        assert_eq!(
            a.auth_ready,
            Some(c.expect_auth_ready),
            "{}：本机产出的 authReady 与金样不一致",
            c.name
        );
        // `loggedIn` 逐字节旧语义：仍然只是「凭据文件在不在」。
        assert_eq!(
            a.logged_in, c.credentials_present,
            "{}：loggedIn 的语义被这次改动动了（它该只是 stat 结果）",
            c.name
        );
    }
}

/// ★ `KAY4` 的 **Rust 侧那一格**（那条零命中守卫是 vitest，扫不到这里）。
///
/// 守的性质：本文件里这一维**不许有第二条计算路径** —— `auth_ready` 只许来自
/// `acct_core::auth_ready(`，`authKind` 只许来自 `AuthKind::from_manifest(`。
/// 有人在这儿手写 `if kind == AuthKind::ApiKey { true } else { … }`，本条红。
///
/// ⚠ 射程如实写：**只管本文件**（daemon 那份由它自己那条同名判据守），
/// 而且是**字面量扫描** —— 把 helper 重新 `use` 成别名就绕得过去。
/// 真正的地板不是它，是 `acct-core` 里只有一份实现。
#[test]
fn the_auth_dimension_has_exactly_one_computation_path() {
    let me = include_str!("../../src/bridge/src/local_accounts.rs");
    assert!(
        me.len() > 20_000,
        "include_str! 没读到源码，本条在空转（实得 {} 字节）",
        me.len()
    );
    let cut = me
        .find("#[cfg(test)]")
        .expect("找不到 #[cfg(test)] 锚点 —— 切法失效了");
    // **去注释口径**：本文件的注释里就在解释这一维，按裸文本数会把散文也数进来。
    let prod: String = me[..cut]
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        prod.contains("fn list_from_dir(accts_dir: &Path)") && prod.len() > 4_000,
        "剥注释剥过头了（实得 {} 字节）—— 下面几条会零命中地绿",
        prod.len()
    );
    assert_eq!(
        prod.matches("auth_ready(").count(),
        1,
        "生产段里 `auth_ready(` 出现了不止一次 —— 要么有了第二条计算路径，要么该收进 acct-core"
    );
    assert_eq!(
        prod.matches("AuthKind::").count(),
        1,
        "生产段里出现了不止一处 `AuthKind::` —— 分类只许经 `AuthKind::from_manifest`"
    );
    assert_eq!(
        prod.matches("ApiKey").count(),
        0,
        "生产段里出现了 `ApiKey` —— 按 kind 分流的规则只许住 acct-core"
    );
}

// ---- `N-F1c`：读口改问本机后端 ----

/// daemon `--list-accounts` 出参的一份**已知假清单**：首行 meta，其后每账号一行。
///
/// ⚠ 逐字写死成字面量、**不用**任何生产常量拼 —— 同本文件 `write_manifest`
/// 那条纪律：测试该钉契约，用生产常量拼的话常量一起变、判据结构上不可能红。
fn fake_listing() -> String {
    [
        r#"{"kind":"accounts-meta","enabled":true,"acctsDir":"/h/lib","manifestPath":"/h/lib/accounts.json","updatedAt":"2026-09-05T00:00:00Z","sharedStore":"/h/shared","count":3,"error":null,"accountZeroAware":true}"#,
        r#"{"name":"zero","email":"","configDir":null,"isDefault":false,"mode":"isolated","exists":true,"loggedIn":true}"#,
        r#"{"name":"alice","email":"a@x.edu","configDir":"/h/lib/alice","isDefault":true,"mode":"isolated","exists":true,"loggedIn":true}"#,
        r#"{"name":"bob","email":"","configDir":"/h/lib/bob","isDefault":false,"mode":"isolated","exists":false,"loggedIn":false}"#,
    ]
    .join("\n")
}

/// ★★ `NF1cD1` 的**正面读数** —— 只断「调用了 `run_query`」不算数。
///
/// 喂一份**已知的假清单**，断它答出几个、名字是什么、字段有没有在这一跳里丢掉。
///
/// ⚠ 它买不到什么，如实写：**不证明后端真的会这么答**（那要真 sidecar，
/// 属 e2e；开发树里 `externalBin` 现打零命中，`nc2` 已登记）。
/// 它证明的是**这一跳的解析与折叠是对的**，而 `NcM1` 那一刀（读口改回直读磁盘）
/// 会让它当场红 —— 直读磁盘那条路根本不看这份 stdout。
#[test]
fn a_known_fake_listing_comes_back_with_the_right_accounts() {
    let (meta, accounts) = match classify_local_accounts(QueryOutcome::Ok(fake_listing())) {
        LocalAccountsOutcome::Listed { meta, accounts } => (meta, accounts),
        other => panic!("喂了一份合法清单，却没落到 Listed 那一档：{other:?}"),
    };
    assert_eq!(accounts.len(), 3, "答出来的账号数不对");
    assert_eq!(
        accounts.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        vec!["zero", "alice", "bob"],
        "名字或顺序不对 —— 顺序是承重的（界面按后端给的次序排）"
    );
    assert_eq!(meta.count, 3);
    assert!(meta.enabled);
    assert_eq!(meta.shared_store.as_deref(), Some("/h/shared"));
    assert_eq!(meta.manifest_path, "/h/lib/accounts.json");
    assert!(
        meta.account_zero_aware,
        "后端说了它认账号 0，这一跳不许把这一位丢掉（丢了界面会多喊一句降级）"
    );
    // 账号 0：`configDir` 是 null ⇒ 下游据此「不注入」。空串**不算**缺席，
    // 所以这一跳不许把 null 折成 `Some(\"\")`。
    assert!(
        accounts[0].config_dir.is_none(),
        "账号 0 的 configDir 被改写了：{:?}",
        accounts[0].config_dir
    );
    assert_eq!(accounts[1].config_dir.as_deref(), Some("/h/lib/alice"));
    assert!(accounts[1].is_default);
    assert_eq!(accounts[1].email, "a@x.edu");
    assert!(!accounts[2].exists, "bob 的目录不存在，这一位要如实带过来");

    // 折成前端那个形状之后仍然是「答出来了」。
    let r = classify_local_accounts(QueryOutcome::Ok(fake_listing())).into_result();
    assert!(r.available && r.error.is_none());
    assert_eq!(r.accounts.len(), 3);
    assert_eq!(r.meta.expect("meta 丢了").count, 3);
}

/// ★★ `NF1cD3`：**三个结局在类型上分得开，三句话两两不同。**
///
/// 形状照 `daemon_policy` 那族「四条文案两两不同」——
/// 判定不是「源码里出现了三个枚举名」，是**说出来的那三句话真的不一样**。
#[test]
fn the_three_endings_are_told_apart_and_say_different_things() {
    const EMPTY_ANSWER: &str = r#"{"kind":"accounts-meta","enabled":false,"acctsDir":"/h/lib","manifestPath":"/h/lib/accounts.json","updatedAt":null,"sharedStore":null,"count":0,"error":"清单不可读","accountZeroAware":true}"#;
    let listed = classify_local_accounts(QueryOutcome::Ok(EMPTY_ANSWER.into()));
    let no_backend = classify_local_accounts(QueryOutcome::NoBackend(
        "sidecar 不在旁边；找过 [\"/opt/x\"]".into(),
    ));
    let unreadable = classify_local_accounts(QueryOutcome::Failed {
        code: Some(2),
        stderr: "unknown argument\n".into(),
    });

    // ① 类型上就分得开（不靠读字符串）。
    assert!(
        matches!(listed, LocalAccountsOutcome::Listed { .. }),
        "后端答了一份 meta，却没落到 Listed：{listed:?}"
    );
    assert!(
        matches!(no_backend, LocalAccountsOutcome::NoBackend(_)),
        "「后端不在」落错档：{no_backend:?}"
    );
    assert!(
        matches!(unreadable, LocalAccountsOutcome::Unreadable(_)),
        "「答了但读不动」落错档：{unreadable:?}"
    );

    // ② 三句话**两两不同**，且每一句都说得下去（掏空成一个短语，上面那条照样绿）。
    let copies = [listed.copy(), no_backend.copy(), unreadable.copy()];
    for i in 0..copies.len() {
        for j in (i + 1)..copies.len() {
            assert_ne!(
                copies[i], copies[j],
                "第 {i} 档与第 {j} 档说的是同一句话 —— 那两格在用户眼里就没分开"
            );
        }
    }
    for (n, c) in copies.iter().enumerate() {
        assert!(
            c.chars().count() >= 30,
            "第 {n} 档只有 {} 个字 —— 一句说不出原因与下一步的话等于只给了个名字",
            c.chars().count()
        );
    }
    // 「后端不在」那一句必须**明说它不是「你没有账号」**，并说得出下一步。
    assert!(
        copies[1].contains("不是") && copies[1].contains("发版"),
        "「后端不在」那句话没把「够不着 ≠ 没有」说出来，也没给下一步：{}",
        copies[1]
    );

    // ③ 🔴 `NcM4` 那一刀的落点：**「后端不在」与「后端说这里有 0 个账号」必须分得开。**
    //    后者是**答案**（`enabled:false` + 原因），前者是**够不着**。
    let answer = classify_local_accounts(QueryOutcome::Ok(EMPTY_ANSWER.into())).into_result();
    let cannot_reach = classify_local_accounts(QueryOutcome::NoBackend("x".into())).into_result();
    assert!(
        answer.available && answer.error.is_none() && answer.meta.is_some(),
        "「后端答了、只是这台机没启用多账号」被折成了不可用"
    );
    assert!(answer.accounts.is_empty());
    assert!(
        !cannot_reach.available,
        "「后端不在」被折成 available:true —— 那就是把「够不着」渲染成「你没有账号」"
    );
    assert!(cannot_reach.error.is_some(), "够不着的时候必须说得出理由");
    assert!(
        cannot_reach.meta.is_none(),
        "够不着的时候不许伪造一份 meta —— 界面会拿它的 count 去渲染一张空表"
    );
    assert_ne!(
        (
            answer.available,
            answer.error.is_some(),
            answer.meta.is_some()
        ),
        (
            cannot_reach.available,
            cannot_reach.error.is_some(),
            cannot_reach.meta.is_some()
        ),
        "两种「零个账号」在前端拿到的形状上一模一样 —— 界面就只能猜"
    );
}

/// ★★ 「代码还在但走不到」那一族在**本件读口**上的落点。
///
/// 形状照 `a_short_circuit_cannot_fake_the_honest_degrade`（`local_query.rs` 里那条）：
/// 单测环境**没有** sidecar ⇒ 真走了那条路才拿得到**带理由**的诚实降级；
/// 被短路（比如 `return Ok(Default::default())`）给出的是「空但成功」——
/// 而扫源码的守卫看不见那种错，调用那行文字还在。
///
/// ⚠ 它证明的是「调用发生了、且拿到了后端不在的答复」，
/// **不证明** happy path 正确（那一格由上面那条正面读数与将来的 e2e 分担）。
#[tokio::test]
async fn the_read_port_really_asks_the_backend() {
    // 前提自检：本环境必须没有 sidecar，否则下面几条会走 happy path 而空转。
    let probe = run_query(
        env!("CCM_TARGET_TRIPLE"),
        &["--list-accounts"],
        &*crate::spawn_managed::local_backend_one_shot_query(),
    );
    assert!(
        matches!(probe, QueryOutcome::NoBackend(_)),
        "测试环境里居然找得到 sidecar —— 本条的前提不成立，下面几条会空转。\n\
             （若哪天单测环境真带 sidecar，本条要改成显式指一个不存在的 target triple）"
    );
    let r = list_local_accounts()
        .await
        .expect("这条路的诚实降级是 Ok(available=false)，不该是 Err");
    assert!(!r.available, "没有 sidecar 却报 available=true");
    let why = r.error.expect(
        "`list_local_accounts` 返回了「空但成功」——\n\
             没有 sidecar 时它**必须说出理由**（定框 §5：tagged 返回 + reason）。\n\
             ⇒ 拿不出 reason 就意味着那条查询根本没发生（被短路了）。",
    );
    assert!(
        why.contains("本机后端不在"),
        "理由没说清是「后端不在」还是别的：{why}"
    );
    assert!(r.accounts.is_empty() && r.meta.is_none());
}
