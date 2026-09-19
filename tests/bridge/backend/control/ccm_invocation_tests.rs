use super::*;

/// ★★ **P3t-Y1**：POSIX 本机放行、**Windows 本机仍拒**（`C12` 逐字「windows不要tmux」）。
///
/// 纯函数判据 —— 直接喂 `CliSpec.local_posix`，**不碰那个进程内全局量**
/// （碰它会与金串对拍那条并行干扰，08-11 实测栽过一次）。
#[test]
fn posix_local_is_allowed_windows_local_is_not() {
    // 用同模块既有的 `render`（它带全量 caps）—— 自己拼 caps 会漏掉必需项，
    // 那样「远端也被拒」会被误读成「我改坏了远端」（第一版就是这么假红的）。
    let mut s = base_spec();
    assert!(render(&s).is_ok(), "远端那支被改坏了");

    // ② 本机 + POSIX：**放行**（本件的正题）
    s.is_ssh = false;
    s.local_posix = true;
    assert!(
        render(&s).is_ok(),
        "POSIX 本机仍被拒 —— 那么本机还是走旧路，产出的是一个**无 tty、无 tmux** 的进程，\n\
             用户敲进去的字会被脚本吃掉（`launch_local_posix` 头注与 `src/doc/IPC-PROTOCOL.md` 各记了一份）。"
    );

    // ③ 本机 + 非 POSIX（Windows）：**仍拒**
    s.local_posix = false;
    assert_eq!(
        render(&s),
        Err(Refusal::NotSsh),
        "Windows 本机被放行了 —— `C12` 逐字「windows不要tmux」。\n\
             ★ 这一格正是「翻面翻成更弱的判据」最容易丢的那半（P2s-Y3 / P3 刀 0 各栽过一次）。"
    );
}

fn base_spec() -> CliSpec<'static> {
    CliSpec {
        is_ssh: true,
        local_posix: false,
        action: Action::New,
        container: Container::None,
        cwd: None,
        account: CliAccount::Base,
        ccm_sid: None,
        model: None,
        launcher: "claude",
        default_launcher: "claude",
        args: &[],
        ccm_path: "ccm",
    }
}

/// 判据自带的静态能力清单（见本模块头注：**不复用 `CLI_REQUIRED_CAPS`**）。
const STATIC_CAPS_EXPECTED: &[&str] = &[
    "new", "resume", "attach", "tmux", "cwd", "launcher", "ccm-sid",
];
/// 维度各自声明的能力（§37），不在静态清单里。
const DIMENSION_CAPS_EXPECTED: &[&str] = &["account", "model"];

fn caps_all() -> BTreeSet<String> {
    STATIC_CAPS_EXPECTED
        .iter()
        .chain(DIMENSION_CAPS_EXPECTED)
        .map(|c| (*c).to_string())
        .collect()
}

fn caps_without(missing: &str) -> BTreeSet<String> {
    let mut c = caps_all();
    assert!(
        c.remove(missing),
        "{missing} 本来就不在全集里，这条用例是空转"
    );
    c
}

fn render(spec: &CliSpec) -> Result<String, Refusal> {
    render_ccm_invocation(spec, &caps_all(), true)
}

fn dim(id: &str) -> &'static Dim {
    DIMENSION_ORDER
        .iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("维度注册表里没有 {id}"))
}

/// 覆盖「会影响渲染结果」的各个方向的一个小矩阵。给那几条「对所有形态都成立」的
/// 结构性判据当输入。
fn spec_matrix() -> Vec<CliSpec<'static>> {
    let mut out = Vec::new();
    for action in [
        Action::New,
        Action::Resume { sid: "s1" },
        Action::Attach { name: "cc-x" },
    ] {
        for container in [
            Container::None,
            Container::Tmux {
                name: "cc-x",
                send_into: false,
            },
            Container::Tmux {
                name: "cc-x",
                send_into: true,
            },
        ] {
            // `K-R89`：第四态 [`CliAccount::Inherit`] 也进矩阵 —— 不进的话
            // 「对每个 spec 都成立」那几条判据的输入域里根本没有它。
            for account in [
                CliAccount::Base,
                CliAccount::Named { name: Some("z") },
                CliAccount::Named { name: None },
                CliAccount::Inherit,
            ] {
                for (ccm_sid, model) in [(None, None), (Some("sid-1"), Some("opus"))] {
                    let mut s = base_spec();
                    s.action = action.clone();
                    s.container = container.clone();
                    s.account = account.clone();
                    s.ccm_sid = ccm_sid;
                    s.model = model;
                    out.push(s);
                }
            }
        }
    }
    out
}

#[test]
fn the_spec_matrix_is_not_accidentally_empty() {
    // 下面几条「对矩阵里每个 spec 都成立」的判据，矩阵一空就零命中变绿。
    let m = spec_matrix();
    // `K-R89`：账号那一维从 3 态变 4 态（多了 `Inherit`）⇒ 3 * 3 * **4** * 2。
    assert_eq!(m.len(), 3 * 3 * 4 * 2, "矩阵规模变了，请确认覆盖面还在");
    assert!(
        m.iter().any(|s| render(s).is_ok()),
        "矩阵里没有一个渲染得出来的 spec —— 那「applies ⇒ 必被拒」那条就恒真了"
    );
}

// ── 静态能力闸 ───────────────────────────────────────────────────────────

/// ★ 杀 R5（`CLI_REQUIRED_CAPS` 少一项 `cwd` —— 此前没有任何东西红）。
///
/// 两半都要有：**清单相等**这一半才是杀掉「悄悄删一项」的那半；
/// **逐项抽掉**这一半证明每项是真被执行、不是只声明（写了不查也是白写）。
#[test]
fn every_static_capability_is_declared_and_individually_enforced() {
    assert_eq!(
        CLI_REQUIRED_CAPS, STATIC_CAPS_EXPECTED,
        "静态能力清单变了。改它是改「装了哪种 ccm 才肯走 CLI 形态」的门槛，\
             要同步 TS 的 CLI_REQUIRED_CAPS 与 \
             src/backend/control/ccm/mod.rs 的 CAPABILITIES\n\
             〔`K-R61` 09-11：这里原先点的是那份 bash `ccm` 的 `--ccm-probe` —— \
             那个文件 `07e4e72` 就删了，与 `K-R61` 治的是同一种悬空引用。\
             本清单是**子集检查** ⇒ 对面加 token 不影响本条；删/改名才影响。〕"
    );
    for missing in STATIC_CAPS_EXPECTED {
        let got = render_ccm_invocation(&base_spec(), &caps_without(missing), true);
        assert_eq!(
            got,
            Err(Refusal::MissingCap((*missing).to_string())),
            "缺静态能力 {missing} 时没有诚实降级"
        );
    }
}

/// ★★ **P3t-Y3 翻面**：本条的**依据换了**，测的东西没变弱。
///
/// 原来这里是 `s.is_ssh = false;` 一句就断言 `NotSsh` —— 那在 P3t 之前成立
/// （`is_ssh` 是唯一的入口条件），P3t 之后**不再成立**：闸变成了
/// `!is_ssh && !local_posix`，`is_ssh = false` 单独一条已经**推不出**拒绝。
///
/// 不翻会怎样：它靠 `base_spec()` 的 `local_posix: false` **偶然**继续绿，
/// 而它自陈钉的是「不走 ssh 就拒」。⇒ 那是一条读数正确、说法过宽的判据，
/// 与 Y4 刚治的 §36（只绑 Windows）转述是同一种病 —— 只不过这次病灶在判据里。
/// 现在把两个条件**都写出来**，并让「本条只覆盖 Windows 那一格」当场可见。
///
/// 本条守的仍是**顺序**（`NotInstalled` 早于平台闸），POSIX/Windows 两格由
/// `posix_local_is_allowed_windows_local_is_not` 守 —— 两条不重叠。
#[test]
fn not_installed_and_not_ssh_are_checked_before_anything_else() {
    assert_eq!(
        render_ccm_invocation(&base_spec(), &caps_all(), false),
        Err(Refusal::NotInstalled)
    );
    let mut s = base_spec();
    s.is_ssh = false;
    s.local_posix = false; // ← P3t 起这一条是**必需**的：光 `is_ssh = false` 推不出拒绝
    assert_eq!(
        render(&s),
        Err(Refusal::NotSsh),
        "Windows 本机（`!is_ssh && !local_posix`）没被拒 —— 平台闸破了"
    );
    // 两者同时成立时先报「没装」—— 这两条 reason 会进 console.debug，顺序即诊断。
    assert_eq!(
        render_ccm_invocation(&s, &BTreeSet::new(), false),
        Err(Refusal::NotInstalled)
    );
}

// ── 降级理由（生产侧唯一的降级线索）─────────────────────────────────────

/// ★ 杀 R4（`AttachNeedsTmux` 的理由被改掉，此前无人红）。
///
/// 这是**钉住**，不是推导 —— 这几句字符串是与 TS 侧的契约（夹具逐字节比的就是它们）。
/// 其中六条另有跨语言夹具背书（下一条测试对着入库文件核）；
/// **`AttachNeedsTmux` 是唯一没有夹具用例的那条**，所以 R4 才存活。
/// `Refusal` 的**变体名**，从源码里的枚举定义派生。
///
/// ⚠ 08-08 之前，两条降级理由判据的人群都是**手写清单**，连「Refusal 有七个变体」
/// 这个数也是手写的。实测：给枚举加第八个变体、并像真人那样补上它的 `reason()` 臂，
/// **全仓 988 条判据一条不红** —— 那句新的用户可见文案就此无人看管，
/// 而本模块头注写着「渲染失败**必须带理由**，`reason` 是生产文案」。
/// ⇒ 与 F24（`wire.rs` 手写帧清单）同一族：**人群要从枚举本身取**。
/// 某个变体的**代表性降级理由**（带占位的用夹具里真实出现的那一份）。
fn sample_reason(variant: &str) -> String {
    match variant {
        "MissingCap" => Refusal::MissingCap("tmux".into()).reason(),
        "DimensionCannotSpeak" => Refusal::DimensionCannotSpeak("account".into()).reason(),
        "DimensionNeedsCap" => Refusal::DimensionNeedsCap {
            dim: "model".into(),
            cap: "model".into(),
        }
        .reason(),
        "NotInstalled" => Refusal::NotInstalled.reason(),
        "NotSsh" => Refusal::NotSsh.reason(),
        "SendIntoHasNoCliForm" => Refusal::SendIntoHasNoCliForm.reason(),
        "AttachNeedsTmux" => Refusal::AttachNeedsTmux.reason(),
        other => panic!(
            "变体 `{other}` 没有代表样本 —— 新变体要在这里给一个，\
                 否则夹具对拍认不出它（这一步刻意不自动化：带占位的理由要人来选值）"
        ),
    }
}

fn refusal_variants() -> Vec<String> {
    let src = include_str!("../../../../src/bridge/src/backend/control/ccm_invocation.rs");
    let at = src
        .find("pub enum Refusal {")
        .expect("找不到 `pub enum Refusal` —— 抽取器坏了，两条判据此刻无效");
    let mut out = Vec::new();
    for line in src[at..].lines().skip(1) {
        if !line.is_empty() && !line.starts_with(char::is_whitespace) {
            break; // 顶格行 = 枚举收尾
        }
        let s = line.trim();
        if s.starts_with("///") || s.starts_with("//") || s.is_empty() {
            continue;
        }
        let name: String = s
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && name.starts_with(char::is_uppercase) {
            out.push(name);
        }
    }
    out
}

#[test]
fn every_refusal_reason_is_pinned_byte_for_byte() {
    let pairs: &[(Refusal, &str)] = &[
        (Refusal::NotInstalled, "远端未装 ccm"),
        (Refusal::NotSsh, "本地路径不走 CLI 渲染器"),
        (Refusal::MissingCap("tmux".into()), "远端 ccm 缺能力 tmux"),
        (
            Refusal::SendIntoHasNoCliForm,
            "send-into（idle-tmux 就地复用）无 CLI 等价语法，诚实降级",
        ),
        (Refusal::AttachNeedsTmux, "attach 必须是 tmux 容器"),
        (
            Refusal::DimensionCannotSpeak("account".into()),
            "维度 account 无法用 CLI 语法表达（cliFlags 返回 null）",
        ),
        (
            Refusal::DimensionNeedsCap {
                dim: "model".into(),
                cap: "model".into(),
            },
            "维度 model 需要远端 ccm 能力 model，但它不支持",
        ),
    ];
    // ★ 人群**从枚举派生**，不再手写「七个」这个数。
    let variants = refusal_variants();
    assert!(
        variants.len() >= 7,
        "只从 `Refusal` 抽到 {} 个变体（08-08 实测 7）—— 抽取器坏了，本条此刻无效：{variants:?}",
        variants.len()
    );
    assert_eq!(
        pairs.len(),
        variants.len(),
        "`Refusal` 有 {} 个变体，而这张逐字表只列了 {} 条 —— 新变体的**用户可见文案**没人看管。\n\
             ⚠ 08-08 实测：加第八个变体并补上它的 `reason()` 臂，全仓判据一条不红。\n\
             本模块头注写着「渲染失败必须带理由，`reason` 是生产文案」—— 那句话要有人读。\n\
             变体：{variants:?}",
        variants.len(),
        pairs.len()
    );
    // 每个变体都要在表里出现。
    //
    // ⚠ **不要读成「改名也会红」**：表里写的是 `Refusal::X` 这样的路径，
    // 改名时编译器会强制两侧一起改 ⇒ 本条对改名**无话可说**，那由编译器兜着。
    // 它真正接住的是「变体在、表里没有」——与上面那条计数互为补充：
    // 计数说「少了几条」，这条说「少的是哪一条」。
    // （08-08 变异实测：只改名 9 处后全绿，我原本在这里写了「改名也要红」，
    //   那是没验过就写下的断言 —— 已改准。）
    for v in &variants {
        assert!(
            pairs
                .iter()
                .any(|(r, _)| format!("{r:?}").starts_with(v.as_str())),
            "变体 `{v}` 不在逐字表里 —— 是新加的，还是改了名？两种都要人来看一眼"
        );
    }
    for (r, want) in pairs {
        assert_eq!(&r.reason(), want, "{r:?} 的降级理由变了");
    }
}

/// 上一条那七句里的六句，**在入库夹具里逐字出现**（夹具是 TS 生成的）——
/// 所以那六句不是自说自话。带文件规模自检：夹具读空时 `contains` 会全假、方向是红，
/// 但那时报的错会很难懂，所以先断言它有内容。
#[test]
fn the_reasons_the_fixture_covers_really_come_from_the_typescript_side() {
    // P4b：搬家后改用 `include_str!` —— 夹具被删/改名 ⇒ **编译失败**，
    // 而不是运行时才发现（同两条 parity 判据的纪律）。
    let fx = include_str!("../../../../src/bridge/src/backend/control/fixtures/cli-golden.json");
    assert!(fx.len() > 1000, "夹具只有 {} 字节，像是坏了", fx.len());
    // ★ 人群**从枚举派生**：每个变体的降级理由都要在夹具里出现，
    //   除非它登记在下面这张豁免表里并写明「谁顶了它」。**默认拒绝。**
    const NO_FIXTURE_CASE: &[(&str, &str)] = &[(
        "AttachNeedsTmux",
        "夹具没有这条用例（模块头注逐字记着它是唯一一条）—— \
             由行为判据 `attaching_into_a_non_tmux_container_is_refused` 顶着",
    )];
    let variants = refusal_variants();
    assert!(
        variants.len() >= 7,
        "只从 `Refusal` 抽到 {} 个变体 —— 抽取器坏了，本条此刻无效",
        variants.len()
    );
    let mut checked = 0usize;
    for v in &variants {
        if let Some((_, why)) = NO_FIXTURE_CASE.iter().find(|(n, _)| n == v) {
            let _ = why;
            continue;
        }
        checked += 1;
        let want = sample_reason(v);
        assert!(
            fx.contains(&want),
            "夹具里找不到变体 `{v}` 的降级理由：{want:?}\n\
                 ⚠ 那句话是**生产文案**（TS 那边 `{{ok:false, reason}}` 直接上屏）。\n\
                 要么给它补一条夹具用例，要么登记进 `NO_FIXTURE_CASE` 并写明谁顶了它。"
        );
    }
    // 常驻自检：豁免表把人群吃空时，上面整段空转。
    assert!(
        checked >= 5,
        "只核了 {checked} 个变体（08-08 实测 6）—— 豁免表是不是被塞大了？"
    );
}

// ── attach 分支 ─────────────────────────────────────────────────────────

/// ★ 杀 R4 的行为那一半：`attach` 落在非 tmux 容器上必须被拒。
/// 夹具里没有这个形态（它只有「attach + tmux」那条 ok 用例）。
#[test]
fn attaching_into_a_non_tmux_container_is_refused() {
    let mut s = base_spec();
    s.action = Action::Attach { name: "cc-x" };
    s.container = Container::None;
    assert_eq!(render(&s), Err(Refusal::AttachNeedsTmux));
}

/// `ccm attach <名>` 不接受任何修饰 —— 所以它在维度循环**之前**返回，
/// 连维度要的能力都不收（§33 登记在案的刻意豁免）。
#[test]
fn attach_reads_the_container_name_and_no_modifiers_at_all() {
    let mut s = base_spec();
    s.action = Action::Attach {
        name: "被忽略的动作名",
    };
    s.container = Container::Tmux {
        name: "cc-x",
        send_into: false,
    };
    s.ccm_sid = Some("sid-1");
    s.model = Some("opus");
    s.cwd = Some("/w");
    s.launcher = "claude-dev";
    s.account = CliAccount::Named { name: Some("z") };
    assert_eq!(render(&s).as_deref(), Ok("ccm attach cc-x"));
    // 连账号/模型维度的能力都不要 —— 全都缺也照样渲染得出来。
    let only_static: BTreeSet<String> = STATIC_CAPS_EXPECTED
        .iter()
        .map(|c| (*c).to_string())
        .collect();
    assert_eq!(
        render_ccm_invocation(&s, &only_static, true).as_deref(),
        Ok("ccm attach cc-x")
    );
}

// ── 维度：逐个的 applies / cliFlags / requiredCaps ───────────────────────

#[test]
fn identity_dimension_speaks_only_when_there_is_a_ccm_sid() {
    let d = dim("identity");
    assert!(d.caps.is_empty(), "identity 不该要求任何能力");
    for s in spec_matrix() {
        assert_eq!(d.applies(&s), s.ccm_sid.is_some());
    }
    let mut s = base_spec();
    s.ccm_sid = Some("sid-1");
    assert_eq!(
        (d.cli_flags)(&s),
        Some(vec!["--ccm-sid=sid-1".to_string()]),
        "identity 是 `--ccm-sid=<值>` 一个 token，不是空格分隔的两个"
    );
}

/// ⚠ **名字里那个「three」今天已经比它测的东西窄了一格**（`K-R89` 09-13）：
/// 账号维度现在有**四**形（`Base` · `Named{有名字}` · `Named{只有目录}` · `Inherit`）。
/// **刻意不改名** —— 与 `history.rs` 那条同族的处置（改判据名要同拍跑 `pb doc`，
/// 而本件写区里没有那份生成区）⇒ 如实登记在这里，下面四形逐个断言。
#[test]
fn account_dimension_always_speaks_up_and_has_three_shapes() {
    let d = dim("account");
    assert_eq!(d.caps, &["account"]);
    // **恒真**：F05 的教训 —— 账号维度沉默 = 远端拿到的是「上一次是谁」。
    for s in spec_matrix() {
        assert!(d.applies(&s), "账号维度必须永远表态");
    }
    let mut s = base_spec();
    assert_eq!((d.cli_flags)(&s), Some(vec!["--base".to_string()]));
    s.account = CliAccount::Named { name: Some("z") };
    assert_eq!(
        (d.cli_flags)(&s),
        Some(vec!["--account".to_string(), "z".to_string()])
    );
    s.account = CliAccount::Named { name: None };
    assert_eq!(
        (d.cli_flags)(&s),
        None,
        "只有 configDir 没有名字时必须老实说「说不出」（§35），不能悄悄降级成 --base"
    );
    // 🔴 `K-R89`：第四形 —— **说得出，而答案是省略**。
    s.account = CliAccount::Inherit;
    assert_eq!(
        (d.cli_flags)(&s),
        Some(Vec::<String>::new()),
        "继承那一态必须渲染成「一个 flag 都不加」（`R28`）——\n\
             它**不是** `None`（那是「说不出、整条降级」），也**不是** `--base`\n\
             （那是「显式清空」，把继承偷换成清空正是 #75 的病灶形状）。"
    );
}

/// 🔴 `K-R89` `KR89D2`：**「省略」与「显式清空」在渲染出来的那一串上必须真的不同。**
///
/// 上面那条比的是**维度函数**的返回值；本条比的是**整条命令**——
/// 谁哪天把 `Inherit` 那一臂改成 `Some(vec!["--base".into()])`，
/// 上面那条会红、本条也会红，而**本条红在生产渲染器的出口上**。
///
/// ⚠ **本条不管 `ccm` 拿到这条命令之后怎么解释省略** —— 那一半的唯一住址是
/// `src/backend/control/ccm/plan.rs::resolve_account`（`R08` 的 `-z` 闸
/// ＋ manifest 默认号两支），由那边的
/// `plan::the_four_ways_an_account_gets_picked` 钉着。**两侧各钉各的，别压成一句。**
#[test]
fn inheriting_renders_a_command_with_no_account_flag_at_all() {
    let mut s = base_spec();
    s.account = CliAccount::Inherit;
    let cmd = render(&s).expect("继承那一态在 `R28` 之后渲染得出来");
    assert!(
        !cmd.contains("--account"),
        "继承那一态渲染出了 `--account` —— 那是替用户挑了一个号。实得：{cmd}"
    );
    assert!(
        !cmd.contains("--base"),
        "继承那一态渲染出了 `--base` —— 那是把「继承」偷换成「显式清空」（#75）。实得：{cmd}"
    );
    // 阳性对照：同一个 spec 换成 `Base`，那一串里就**必须**有 `--base`
    //（否则上面两条「不含」在渲染器整个哑掉时也会绿）。
    s.account = CliAccount::Base;
    let base_cmd = render(&s).expect("账号 0 一直渲染得出来");
    assert!(
        base_cmd.contains("--base"),
        "阳性对照塌了：`Base` 也不吐 `--base` 了 ⇒ 上面那两条「不含」是空真。实得：{base_cmd}"
    );
}

#[test]
fn model_dimension_is_conditional_by_design() {
    let d = dim("model");
    assert_eq!(d.caps, &["model"]);
    // §37：没配偏好 ⇒ 不触发 ⇒ 远端 claude 用它自己的默认模型（那正是用户的期望），
    // 也因此**不要求** model 能力。
    for s in spec_matrix() {
        assert_eq!(d.applies(&s), s.model.is_some());
    }
    let mut s = base_spec();
    s.model = Some("opus");
    assert_eq!(
        (d.cli_flags)(&s),
        Some(vec!["--model".to_string(), "opus".to_string()])
    );
}

/// R1 / R2 / R3 的处置：**这两个维度在 CLI 渲染器里改不了任何输出**（`cli_flags`
/// 恒返回空），所以「拓宽/收窄它们的 `applies`」这类变异**不可能**被行为判据杀掉 ——
/// 那不是判据的缺口，是这两格本身是惰性的。
///
/// 能钉、也值得钉的是**惰性本身**：哪天有人给它们加了真 flag，夹具未必抓得到
/// （`env-reset` 那格在 CLI 路径上根本到不了，见下一条），这条会红。
#[test]
fn the_two_inert_dimensions_contribute_no_flags_for_any_shape() {
    for id in ["env-reset", "nested-env-reset"] {
        let d = dim(id);
        assert!(d.caps.is_empty(), "{id} 声明了能力，但它一个 flag 都不吐");
        for s in spec_matrix() {
            assert_eq!(
                (d.cli_flags)(&s),
                Some(Vec::<String>::new()),
                "{id} 开始吐 flag 了 —— 那它的 applies 就成了活判据，\
                     请回来给夹具补用例（尤其 env-reset：它在 CLI 路径上到不了）"
            );
        }
    }
}

/// ★ 杀 R2（`env-reset` 的 `applies` 被改成恒真）。
///
/// `env-reset` 只在 `send_into: true` 时触发，而那种形态**在维度循环之前**就被
/// #76 防线拒掉了 —— 也就是说这一格在本渲染器里**不可达**。把这条不可达性钉成判据：
/// 哪天 #76 防线被挪走，它会红，提醒「env-reset 变活了，去补夹具」。
#[test]
fn env_reset_can_never_be_reached_in_the_cli_renderer() {
    let d = dim("env-reset");
    let mut seen = 0;
    for s in spec_matrix() {
        if d.applies(&s) {
            seen += 1;
            assert_eq!(
                render(&s),
                Err(Refusal::SendIntoHasNoCliForm),
                "env-reset 触发了、却渲染出了东西 —— 这一格从不可达变成了活的"
            );
        }
    }
    assert!(seen > 0, "矩阵里没有一个 spec 触发 env-reset —— 这条恒真了");
}

// ── 三条铁律：#76 防线 / §35 安全网 / 能力闸逐维度交错 ───────────────────

#[test]
fn send_into_is_refused_before_any_dimension_runs() {
    let mut s = base_spec();
    s.container = Container::Tmux {
        name: "cc-x",
        send_into: true,
    };
    assert_eq!(render(&s), Err(Refusal::SendIntoHasNoCliForm));
    // 「在维度之前」这半：连账号说不出（§35）都不该抢在它前面报。
    s.account = CliAccount::Named { name: None };
    assert_eq!(
        render(&s),
        Err(Refusal::SendIntoHasNoCliForm),
        "#76 防线必须先于维度循环 —— 否则同一个上下文会报出另一条诊断"
    );
}

#[test]
fn a_dimension_that_cannot_speak_abandons_the_whole_line() {
    let mut s = base_spec();
    s.account = CliAccount::Named { name: None };
    s.cwd = Some("/w");
    assert_eq!(
        render(&s),
        Err(Refusal::DimensionCannotSpeak("account".into())),
        "§35：说不出就整条放弃，不是跳过这个维度继续渲染出一条丢了账号的命令"
    );
}

#[test]
fn a_triggered_dimension_carries_its_own_capability_requirement() {
    let mut s = base_spec();
    s.model = Some("opus");
    assert_eq!(
        render_ccm_invocation(&s, &caps_without("model"), true),
        Err(Refusal::DimensionNeedsCap {
            dim: "model".into(),
            cap: "model".into()
        })
    );
    // 没触发就不收 —— 缺 model 能力但没配模型时照样渲染得出来（§37）。
    assert!(render_ccm_invocation(&base_spec(), &caps_without("model"), true).is_ok());
}

/// 能力检查与 flags 是**逐维度交错**的，不是「先把所有能力查完再渲染」。
/// 两个方向各一条 —— 把两种「看起来等价」的实现区分开。
#[test]
fn the_capability_gate_is_interleaved_per_dimension_not_hoisted() {
    // 方向一：靠前的维度说不出、靠后的维度缺能力 ⇒ 必须报**靠前那条**。
    // 把能力检查整体提到循环外，这里会报 DimensionNeedsCap{model}。
    let mut s = base_spec();
    s.account = CliAccount::Named { name: None }; // account(20) 说不出
    s.model = Some("opus"); // model(25) 要 model 能力
    assert_eq!(
        render_ccm_invocation(&s, &caps_without("model"), true),
        Err(Refusal::DimensionCannotSpeak("account".into())),
        "能力检查被提到了维度循环外 —— reason 是生产侧唯一的降级线索，换一个就是换一条诊断"
    );

    // 方向二：**同一个**维度既缺能力又说不出 ⇒ 报缺能力（本维度内能力先于 flags）。
    let mut s = base_spec();
    s.account = CliAccount::Named { name: None };
    assert_eq!(
        render_ccm_invocation(&s, &caps_without("account"), true),
        Err(Refusal::DimensionNeedsCap {
            dim: "account".into(),
            cap: "account".into()
        })
    );
}

// ── 整条命令的形状 ──────────────────────────────────────────────────────

/// 一条把五个维度里会吐 flag 的三个**同时**触发的命令 —— 顺序即契约，
/// 而且它同时钉住 `--tmux=`/`--cwd`/`--launcher`/`--` 各自的位置。
#[test]
fn a_fully_loaded_invocation_emits_every_part_in_registry_order() {
    let mut s = base_spec();
    s.action = Action::Resume { sid: "s1" };
    s.container = Container::Tmux {
        name: "cc-x",
        send_into: false,
    };
    s.ccm_sid = Some("sid-1");
    s.account = CliAccount::Named { name: Some("z") };
    s.model = Some("opus");
    s.cwd = Some("/w");
    s.launcher = "claude-dev";
    s.args = &["-p"];
    assert_eq!(
        render(&s).as_deref(),
        Ok(concat!(
            "ccm resume s1 --tmux=cc-x --ccm-sid=sid-1 ",
            "--account z --model opus --cwd /w --launcher claude-dev -- -p"
        ))
    );
}

/// ★ 杀 R6（`args` 那一段整段失效 —— 此前无人红：夹具里没有带 args 的用例，
/// 因为生产今天零 producer）。`--` 之后逐个 token 各自 quote，不是拼成一个串。
#[test]
fn args_go_after_a_bare_double_dash_and_are_quoted_one_by_one() {
    let mut s = base_spec();
    s.args = &["-p", "两个 词"];
    assert_eq!(render(&s).as_deref(), Ok("ccm new --base -- -p '两个 词'"));
    let mut s = base_spec();
    s.args = &[];
    assert_eq!(
        render(&s).as_deref(),
        Ok("ccm new --base"),
        "空 args 不该吐出一个孤零零的 --"
    );
}

#[test]
fn launcher_is_only_named_when_it_differs_from_the_default() {
    let mut s = base_spec();
    s.launcher = "claude";
    assert_eq!(render(&s).as_deref(), Ok("ccm new --base"));
    s.launcher = "claude-dev";
    assert_eq!(
        render(&s).as_deref(),
        Ok("ccm new --base --launcher claude-dev")
    );
}

// ── argv 的 quote 边界 ─────────────────────────────────────────────────

/// ★ 杀 R7（放行集里少一个字符 —— 此前无人红：夹具的 token 里恰好没有 `.`）。
///
/// 判据自带放行集（见本模块头注：遍历被测代码自己是恒真的）。与 TS
/// `argv()` 的 `/^[A-Za-z0-9_@%+=:,./-]+$/` 同一份规则。
const ARGV_BARE_CHARS: &str = "_@%+=:,./-";

#[test]
fn argv_leaves_every_allowed_character_bare() {
    for c in ARGV_BARE_CHARS.chars() {
        let token = format!("a{c}b");
        assert_eq!(
            argv(&token),
            token,
            "{c:?} 在放行集里，却被 quote 了 —— 与 TS 的 argv() 分家了"
        );
    }
    for token in ["abc", "ABC0", "/usr/local/bin/claude", "v1.2.3-rc.1"] {
        assert_eq!(argv(token), token);
    }
}

#[test]
fn argv_quotes_everything_else_including_the_empty_token() {
    assert_eq!(argv(""), "''", "空 token 不 quote 会在命令行里整个消失");
    for token in ["a b", "a;b", "a|b", "$(id)", "a\nb", "中文", "a*b", "a>b"] {
        let got = argv(token);
        assert_ne!(got, token, "{token:?} 该被 quote");
        assert!(
            got.starts_with('\'') && got.ends_with('\''),
            "{token:?} 的 quote 结果形状不对：{got}"
        );
    }
    // 单引号自身走内核那份逃逸（`quote_singleton_guard` 钉住只有一个家）。
    assert_eq!(argv("a'b"), shell_quote_core::posix_quote("a'b"));
}
