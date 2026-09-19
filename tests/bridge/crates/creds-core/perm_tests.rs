use super::*;

fn production() -> String {
    guard_core::production_code(include_str!(
        "../../../../src/bridge/crates/creds-core/src/perm.rs"
    ))
}

/// ★★ **`KS5` 的机检那一半：Windows 那半不许是「无操作」。**
///
/// # 窗口有界 + 次数钉死（`KP4` 那两个价钱，一个都不省）
///
/// `KP4` 逐字记着：零命中守卫「读起来有牙」而审计一刀就绕过去了，
/// 原因是窗口是 `[\s\S]*?`（无界）+ 登记表把次数登记成 `null`（不钉次数）。
/// ⇒ 这里的窗口是**那个函数的函数体**（花括号配平切出来），并带**两条反空真自检**：
/// 切不出函数体 ⇒ 红；切出来跨进下一个 item ⇒ 红。次数一律用**相等断言**。
#[test]
fn the_windows_half_is_not_a_no_op() {
    let prod = production();
    let body = fn_body(&prod, "fn windows_set_owner_only_dacl")
        .expect("切不出 `windows_set_owner_only_dacl` 的函数体 —— 本条按红处理，不是绿");

    // 反空真自检：窗口不许跨进下一个 item。
    assert!(
        !body.contains("\nfn ") && !body.contains("\npub fn "),
        "窗口跨进了下一个函数 —— 窗口无界，下面的断言不算数"
    );
    assert!(body.len() > 300, "窗口只有 {} 字节 —— 切法坏了", body.len());

    // ① 真的去改「谁能读这个文件」，**恰好一次**。
    let n_set = body.matches("SetNamedSecurityInfoW(").count();
    assert_eq!(
        n_set, 1,
        "Windows 那半调 `SetNamedSecurityInfoW` 的次数是 {n_set}，应当恰好 1 —— \
             0 次 = 它是个无操作（`KS5` 逐字禁止），多次 = 这条判据量错了对象"
    );
    // ② **断继承**，恰好一次。少了它就是「DACL 设上了但父目录的继承项还会回来」。
    //
    // ⚠ needle 带着那个 `| ` 是**改过一次的**：第一版数的是裸符号名，
    //   而它在 `use` 里还出现一次 ⇒ 实测「2 次，应当 1 次」当场红。
    //   红得对（次数钉死就是要这样），但它数的是**引入**不是**用上** ——
    //   靶子该是那个按位或表达式，因为「设 DACL」与「断继承」是同一次调用的两个位。
    let n_prot = body
        .matches("| PROTECTED_DACL_SECURITY_INFORMATION")
        .count();
    assert_eq!(
        n_prot, 1,
        "断继承那一位出现 {n_prot} 次，应当恰好 1 —— \
             只设 DACL 不断继承，是「看起来做了」的形状（`§0b` 第 3 条整条讲的就是这个）"
    );

    // ③ **派发点**：`make_private` 里必须真的调它，否则上面两条守的是一段死代码。
    let mp = fn_body(&prod, "pub fn make_private")
        .expect("切不出 `make_private` 的函数体 —— 本条按红处理");
    assert_eq!(
        mp.matches("windows_set_owner_only_dacl(").count(),
        1,
        "`make_private` 没有恰好一次派发到 Windows 那半 —— \
             上面两条就成了守着一段没人调的代码"
    );
    // ④ 第三类平台**不许凭空返回成功**（daemon `fallback_guard` 那条道理的同款）。
    assert!(
        mp.contains("不假装做到了"),
        "`make_private` 的非 unix/windows 分支没有诚实报错"
    );
    assert!(
        !mp.contains("let _ = p; // "),
        "`make_private` 里出现了 `make_executable` 那种「参数照收、什么都不做」的形状"
    );
}

/// 取一个函数的函数体（含花括号）。**只认签名的行首锚点**，且要求它恰好出现一次。
fn fn_body<'a>(src: &'a str, sig: &str) -> Option<&'a str> {
    let hits = src.matches(sig).count();
    if hits != 1 {
        return None; // 0 = 找不着；>1 = 认不准，两种都按切不出处理
    }
    let at = src.find(sig)?;
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

/// ★ **`windows_create_owner_only` 的机检覆盖**〔D2 硬伤，08-27〕。
///
/// # 它为什么在 D1 那轮是**零覆盖**
///
/// D1 补 `create_private` 时，Unix 那半有行为判据（`a_file_created_through_create_private_is_born_owner_only`），
/// 而 Windows 那半**一条都没有** —— 那台机器上跑不了它的行为，于是就什么都没写。
/// ⚠ 「跑不了行为」不等于「什么都判不了」：**它长什么样是判得了的**，
/// 而这一格恰恰是隔壁 `the_windows_half_is_not_a_no_op` 已经证明有价值的那一类
/// （`MU5` 实测：把 Windows 半改成无操作，只有机检会红）。
///
/// # 它钉四样（每样都钉次数，不是钉「有没有」）
#[test]
fn the_windows_create_path_really_creates_with_a_dacl() {
    let prod = production();
    let body = fn_body(&prod, "fn windows_create_owner_only")
        .expect("切不出 `windows_create_owner_only` 的函数体 —— 本条按红处理，不是绿");
    assert!(
        !body.contains("\nfn ") && !body.contains("\npub fn "),
        "窗口跨进了下一个函数 —— 窗口无界，下面的断言不算数"
    );
    assert!(body.len() > 300, "窗口只有 {} 字节 —— 切法坏了", body.len());

    // ① 真的去**建**文件，恰好一次。
    assert_eq!(
        body.matches("CreateFileW(").count(),
        1,
        "Windows 那半调 `CreateFileW` 的次数不对 —— 0 次 = 它根本没建文件"
    );
    // ② 建的时候**带着安全描述符**（这就是「出生即窄」在 Windows 上的落法）。
    assert_eq!(
        body.matches("Some(&sa as *const SECURITY_ATTRIBUTES)")
            .count(),
        1,
        "`CreateFileW` 没把 `SECURITY_ATTRIBUTES` 传进去 —— \
             那它就是按父目录的继承 ACL 建出来的，和 Unix 上按 umask 建是同一个病"
    );
    // ③ `CREATE_NEW` = `O_EXCL` 的对应物：已存在就失败，不跟随、不截断。
    //
    // ⚠ needle 末尾那个换行是**改过一次的**，而这已经是本件里**第三次**同一个错：
    //   裸符号名会把 `use` 那一行也数进去（实测「2 次，应当 1 次」）。
    //   前两次是 `PROTECTED_DACL_SECURITY_INFORMATION`（段一）与
    //   `expose_for_auth_header(`（D1，那次数到的是**定义**）。
    //   ⇒ 记在这里当路标：**数一个符号「用了几次」时，先想清楚 `use` 算不算一次。**
    assert_eq!(
        body.matches("CREATE_NEW,\n").count(),
        1,
        "创建方式不是 `CREATE_NEW` —— 那会跟随并截断一个已存在的东西（含别人预置的链接），\
             而那时权限是**它的**不是我们的"
    );
    // ④ ★ **两条路必须共用同一句 SDDL** —— 各写一份的那天没有任何东西会说。
    assert_eq!(
        body.matches("owner_only_sddl(").count(),
        1,
        "建文件那条路没有走 `owner_only_sddl` —— \
             它和 `make_private` 就成了「同一个安全性质两个实现」"
    );
    // ⑤ 非空对照：`make_private` 那条路**也**走同一个 helper（证明这把尺子指的是共用，不是巧合）。
    let harden = fn_body(&prod, "fn windows_set_owner_only_dacl")
        .expect("切不出 `windows_set_owner_only_dacl`");
    assert_eq!(
        harden.matches("owner_only_sddl(").count(),
        1,
        "非空对照失败：收窄那条路没走同一个 helper ⇒ 上面第 ④ 条证不了「共用」"
    );
}

/// `KS11`：Unix 上「过宽」的判断，以及它**说不说得清怎么修**。
#[test]
fn a_unix_mode_wider_than_owner_only_is_called_out_with_a_fix() {
    assert_eq!(judge(&Protection::Unix { mode: 0o600 }), Verdict::OwnerOnly);
    assert_eq!(judge(&Protection::Unix { mode: 0o400 }), Verdict::OwnerOnly);
    // 分母 = 我列出的这 4 形（组可读 / 其他可读 / 全开 / 只多一位）。
    for mode in [0o640, 0o604, 0o666, 0o601] {
        match judge(&Protection::Unix { mode }) {
            Verdict::TooWide { how, fix } => {
                assert!(
                    how.contains(&format!("{mode:04o}")),
                    "说法里没有实际 mode：{how}"
                );
                assert!(fix.contains("chmod 600"), "没说清怎么修：{fix}");
            }
            other => panic!("mode {mode:04o} 应当判过宽，实得 {other:?}"),
        }
    }
}

/// `KS11`：Windows 那半按 DACL 里的宽泛主体判。
#[test]
fn a_windows_dacl_with_a_wide_principal_is_called_out() {
    let tight = wide_principals_in_sddl("D:P(A;;FA;;;S-1-5-21-1-2-3-1001)(A;;FA;;;SY)");
    assert!(
        tight.is_empty(),
        "只给本人 + SYSTEM 的 DACL 不该被判宽：{tight:?}"
    );
    assert_eq!(
        judge(&Protection::Windows {
            wide_principals: tight
        }),
        Verdict::OwnerOnly
    );

    let loose = wide_principals_in_sddl("D:AI(A;;FA;;;WD)(A;;0x1200a9;;;BU)(A;;FA;;;SY)");
    assert_eq!(loose, vec!["WD".to_string(), "BU".to_string()]);
    match judge(&Protection::Windows {
        wide_principals: loose,
    }) {
        Verdict::TooWide { how, fix } => {
            assert!(
                how.contains("WD") && how.contains("BU"),
                "说法里没点名主体：{how}"
            );
            assert!(fix.contains("Everyone"), "没说清怎么修：{fix}");
        }
        other => panic!("带 Everyone 的 DACL 应当判过宽，实得 {other:?}"),
    }
}

/// ★ **「查不出来」不是绿灯。**
///
/// 这一条守的是 daemon `fallback_guard.rs` 整篇讲的那个失败模式：
/// 给一个答不上来的问题编一个看起来无害的答案。
#[test]
fn undetermined_is_not_a_green_light() {
    let v = judge(&Protection::Undetermined {
        why: "这一份构建没有开 `harden`".to_string(),
    });
    assert!(v.needs_attention(), "「查不出来」被当成了没问题");
    match &v {
        Verdict::Undetermined { why } => {
            assert!(why.contains("这不等于它没问题"), "说法太软：{why}");
            assert!(why.contains("harden"), "说法里没带上原因：{why}");
        }
        other => panic!("应当是 Undetermined，实得 {other:?}"),
    }
    // 非空对照：唯一不用出声的就是 OwnerOnly。
    assert!(!Verdict::OwnerOnly.needs_attention());
    assert!(Verdict::TooWide {
        how: String::new(),
        fix: String::new()
    }
    .needs_attention());
}

/// `probe` 在 Unix 上真的量到了盘上的位（不是编的）。
#[cfg(unix)]
#[test]
fn probe_reads_the_real_mode_off_the_disk() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("kh2a-perm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let f = dir.join("probe-target");
    std::fs::write(&f, b"{}").expect("写夹具");

    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).expect("放宽");
    assert_eq!(probe(&f), Protection::Unix { mode: 0o644 });
    assert!(judge(&probe(&f)).needs_attention(), "0644 应当出声");

    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).expect("收紧");
    assert_eq!(probe(&f), Protection::Unix { mode: 0o600 });
    assert_eq!(judge(&probe(&f)), Verdict::OwnerOnly);

    // 不存在的文件 ⇒ **查不出来**，不是「没问题」。
    assert!(matches!(
        probe(&dir.join("nope")),
        Protection::Undetermined { .. }
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

/// ★★★ **`KS5` 阻-2 的行为那一半〔D1，08-27〕：文件在**出生那一刻**就只给本人。**
///
/// # 它量的是别的判据量不到的那一格
///
/// 隔壁 `make_private_really_narrows_a_wide_file_to_owner_only` 量的是
/// 「**把一份已经宽的收窄**」，`creds_store` 那条量的是「**rename 之后**目标的 mode」——
/// 两条都在**出生到收窄**那个窗口之外。而 D1 审计探针实打，那个窗口里的读数是
/// `mode=0664，里面已经有明文 = true`。⇒ 本条把观测点挪到**创建调用返回的那一刻**。
///
/// # 非空对照**刻意不依赖这台机器的 umask**
///
/// 拿「普通 `fs::write` 建出来的比它宽」当对照是脆的：umask 恰好是 `0077` 的机器上
/// 那个对照会**恒等**，于是上面那条断言变成空真而没人知道。
/// ⇒ 对照改成**显式**把一份文件设成 `0o644`，证明这把尺子**分得出宽窄**。
#[cfg(all(unix, feature = "harden"))]
#[test]
fn a_file_created_through_create_private_is_born_owner_only() {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "ccm-born-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("建临时目录");

    let born = dir.join("born");
    {
        let mut f = create_private(&born).expect("建一个只给本人的文件");
        f.write_all(b"PLAINTEXT-IS-ALREADY-IN-HERE")
            .expect("写内容");
        f.sync_all().expect("落盘");
    }
    // ★ **出生那一刻**（文件还在原地，没被 rename 走）就量。
    assert_eq!(
        probe(&born),
        Protection::Unix { mode: 0o600 },
        "文件出生时不是只给本人 —— 那一刻里已经有明文了"
    );
    assert_eq!(judge(&probe(&born)), Verdict::OwnerOnly);
    // 内容确实在里面（否则「窄」的是一个空文件，没意义）。
    assert_eq!(
        std::fs::read_to_string(&born).expect("读回"),
        "PLAINTEXT-IS-ALREADY-IN-HERE"
    );

    // ★ 非空对照：这把尺子分得出宽窄（**不依赖 umask**）。
    let wide = dir.join("wide");
    std::fs::write(&wide, b"x").expect("建对照");
    std::fs::set_permissions(&wide, std::fs::Permissions::from_mode(0o644)).expect("放宽");
    assert_eq!(probe(&wide), Protection::Unix { mode: 0o644 });
    assert!(
        judge(&probe(&wide)).needs_attention(),
        "尺子对 0644 都不出声 ⇒ 上面那条「0600」证不了什么"
    );

    // `create_new` 语义：已存在就**失败**，不跟随、不截断
    //（挡「别人预置一个符号链接」那一形）。
    assert!(
        create_private(&born).is_err(),
        "对已存在的路径应当直接失败（O_EXCL / CREATE_NEW），而不是跟随并截断它"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `KS5` 行为那一半（Unix）：`make_private` 真的把一份宽文件收成了 `0o600`。
#[cfg(all(unix, feature = "harden"))]
#[test]
fn make_private_really_narrows_a_wide_file_to_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("kh2a-harden-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let f = dir.join("harden-target");
    std::fs::write(&f, b"{}").expect("写夹具");
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o666)).expect("先放宽");

    // 非空对照：动手之前它**确实是宽的**（否则下面那条断言可能恒真）。
    assert_eq!(probe(&f), Protection::Unix { mode: 0o666 });
    assert!(judge(&probe(&f)).needs_attention());

    make_private(&f).expect("收紧应当成功");
    assert_eq!(probe(&f), Protection::Unix { mode: 0o600 });
    assert_eq!(judge(&probe(&f)), Verdict::OwnerOnly);
    let _ = std::fs::remove_dir_all(&dir);
}
