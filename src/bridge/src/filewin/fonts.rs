//! 步 `24e` 第四刀：**让原生窗口上的字画得出来**。
//!
//! 设计住 `调研/设计/60 §5.4g`；现打读数住 `调研/真相源/102`。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 一、这一刀在修什么：窗口今天**基本没法读**
//! ═══════════════════════════════════════════════════════════════════════
//!
//! egui 自带四份字体（`Hack` · `Ubuntu-Light` · `NotoEmoji-Regular` ·
//! `emoji-icon-font`，见 `epaint` crate 的 `FontDefinitions::default`），**一个汉字都没有**。
//! 而 `filewin/` 整棵树的界面字面量里有 130 个不同的非 ASCII 字符。
//!
//! 2026-09-20 现打（量具见 §三，两个方向的对照都过）：
//!
//! | 配置 | 探针 130 字 · 比例 | 探针 130 字 · 等宽 |
//! |---|---|---|
//! | **本刀之前** | 画不出 **121** | 画不出 **120** |
//! | 装上系统 CJK 字体 | **0** | **0** |
//!
//! 等宽比比例多认一个字：`→`（U+2192）在 `Hack` 里有，而 `Hack` 只在等宽那条链上。
//!
//! ⚠ 两条**不要搞混**的：
//! ① `⬆` `⚠` `📁` `📄` `…` `—` 这六个符号**本来就画得出**（在那两份 emoji 字体里）。
//!    量具第一版说它们画不出 —— 那是**量具在撒谎**，原因见 §三。
//! ② 窗口标题里的中文（`shell::open_detached_seeded` 那个 `format!`）**不经 egui** ——
//!    它是窗口管理器画的，本刀碰不到它，也不需要碰。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 二、字体从哪来：**读系统字体，不内嵌**
//! ═══════════════════════════════════════════════════════════════════════
//!
//! 三条路，选第二条：
//!
//! | 路 | 代价 | 判 |
//! |---|---|---|
//! | 内嵌全量 CJK 字体 | 本机现打 `NotoSansCJK-Regular.ttc` = **19 MB**，进 git 仓 | ❌ 仓膨胀；exe 现在才 31 MB |
//! | 内嵌**子集**字体 | 小，但只覆盖**我们自己写的标签** | ❌ 文件名是**任意**的（用户自己的路径就叫 `/home/user/文档/…`） |
//! | **读系统字体 ＋ 读不到就出声** | 要一张候选路径表 | ✅ 见下 |
//!
//! ## 为什么「一张候选路径表」不算用户否决过的那种「换个系统就换一层」
//!
//! 挂载那一层被否，理由是**每个 OS 要一套不同的实现**。这里不一样：**它是数据，不是层**。
//! 一张路径表，两个平台各几行，读文件的代码是同一份。
//!
//! ## 为什么不引 `fontdb` / `font-kit` 这类字体库
//!
//! 三条现打出来的理由：
//! ① 本仓**离线构建**，新 crate 不在 cargo 缓存里就编不动；
//! ② 门禁有 `lockfile_conflict_guard`（`monitor` 与 `src/backend` 两份 lock 对账），
//!    新依赖树多一个共享 crate 就多一次撞版本的机会 —— 现在就有一条 `cfg_aliases` 的旧账没清；
//! ③ 它买到的只是「把路径表换成扫目录」，而**出声那一步照样得自己做**。
//!
//! ⚠ 代价写明：**候选表没命中的机器上就是豆腐块**。所以有 §三 那道复核 ＋ [`verify`] 的出声。
//!
//! ## ⚠ 没做的：macOS
//!
//! `mod.rs §一` 已判：macOS **开不出这个窗口**（winit 没有 `with_any_thread` 扩展）。
//! ⇒ 往候选表里塞 `/System/Library/Fonts/PingFang.ttc` 是**留一条跑不到的豁免**，本仓不许。
//!   哪天 macOS 的窗口问题解了，连这张表一起重判。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 三、量具：为什么**不能**用 egui 自己的 `has_glyph`
//! ═══════════════════════════════════════════════════════════════════════
//!
//! `epaint` 的 `Font::has_glyph` 实现逐字（那个 crate 里的 `text::font` 模块）：
//!
//! ```text
//! self.resolve_face(c) != self.cached_family.replacement_face_key
//! ```
//!
//! 它拿**「这个字落在哪张 face 上」**跟**「替换字形住在哪张 face 上」**比。
//! ⇒ **凡是和替换字形同住一张 face 的字，它一律报「画不出」**，哪怕那个字明明有。
//!
//! 现打逮到的原形：它说 `Hack` 的 `^`（一个 ASCII 字符）在等宽字体里画不出。
//! 病根是等宽那条链上替换字形正好也住 `Hack` ⇒ Hack 的每一个字都被误判。
//! 上游自己也知道有相关的坑，源码里留着 `TODO(emilk): this is a false negative …🤦‍♂️`。
//!
//! ## 换的量具：**比字形图集里的格子**
//!
//! 画不出来的字会被画成替换字形 ⇒ 它在字形图集里的格子与替换字形**逐字节相同**。
//!
//! ```text
//! 画不出(c)  ⟺  cell(c) == cell('◻')
//! ```
//!
//! 🔴 基准**必须是 `'◻'`（U+25FB）**，不是 U+FFFD。
//! `'◻'` 是 `epaint` 在 `FontsImpl::new` 里写死的那个替换字符（它那边叫 `PRIMARY_REPLACEMENT_CHAR`，白色中方块），
//! ⇒ `cell('◻')` **构造上就等于替换格**：`resolve_face('◻')` 必然落在
//! **替换字形住的那张 face** 上（epaint 内部正是拿 `'◻'` 把那张 face 找出来的），于是走替换分支、
//! 画的就是替换字形。`'◻'` 一个字都找不到时 epaint 退到 `'?'`，同一条推理照样成立。
//!
//! ⚠ 第一版量具拿 U+FFFD 当基准，**碰对了** —— 只因为 U+FFFD 在这几份字体里自己也缺。
//!   哪天装进来的字体带 U+FFFD，那条量具就会**恒绿**。这不是小数点问题，是量具会翻向。
//!
//! ## 这条量具的已知副作用（写明，不藏）
//!
//! `'◻'` 与 `'?'` 自己会被判成「画不出」。⇒ [`PROBE`] 里不许出现这两个字，
//! 判据 `the_probe_excludes_the_two_chars_the_ruler_cannot_judge` 钉着这一条。
//!
//! ## 🔴 量具坏了也要出声
//!
//! [`verify`] 每次跑都先拿**两个方向的对照**验一遍量具自己：
//! 必须画得出的 ASCII、以及 Unicode 里**未分配**的码位。任一方向不对，
//! [`verify`] 报的是**「量具不可用」**，而**不是**「一切正常」。
//! ⇒ 「探针零缺字」这句话只有在量具当场自证过之后才出得来。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 四、两拍：**装** 与 **复核** 必须分开 —— 这是库定的，不是我定的
//! ═══════════════════════════════════════════════════════════════════════
//!
//! `egui::Context::fonts_mut` 在第一帧之前调会 **panic**，逐字：
//!
//! ```text
//! No fonts available until first call to Context::run()
//! ```
//!
//! 那句话住 `egui` crate 的 `Context::fonts_mut`（**逐字引，不给行号**）。
//! ```text
//! ```
//!
//! 而本仓 `[profile.release]` 是 `panic = "abort"` ⇒ **在创建闭包里复核就是当场把进程打掉**。
//!
//! ⇒ 形状：
//! | 拍 | 在哪 | 干什么 |
//! |---|---|---|
//! | 第一拍 [`install`] | `eframe` 的创建闭包（`CreationContext`） | 读文件 ＋ `set_fonts`。**不复核** |
//! | 第二拍 [`verify`] | `App::ui` 的**第一帧** | 跑量具，出 `Option<String>` |
//!
//! ⚠ 所以 [`Attempt`] 刻意**只说「从哪装的」，不说「装好没有」** —— 第一拍问不出后者。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 五、它**没有**买到什么
//! ═══════════════════════════════════════════════════════════════════════
//!
//! [`PROBE`] 是**我们自己写的那些标签** ＋ 界面在用的符号。文件名是**任意**的：
//! 日文 · 韩文 · 生僻汉字 · emoji · 藏文 …… 没有任何探针覆盖得了。
//!
//! ⇒ 「`verify` 回 `None`」这句话的射程逐字是：
//!    **「界面自己的字都画得出来，而且装进来的那份字体覆盖了一段常用汉字」**。
//!    它**不是**「任何文件名都画得出来」。别把这两句话当一句用。

use std::sync::Arc;

/// 探针：**`filewin/` 的界面字面量里出现过的每一个非 ASCII 字符**。
///
/// 🔴 为什么手写、不从源码生成：**恒等两侧同源会退化成恒真**。
/// 这一侧是声明，那一侧（判据 `the_probe_equals_every_non_ascii_char_in_the_window_labels`）现扫源码树。
/// ⇒ 谁加了一个新字而忘了加进来，那条判据红。
///
/// 现值由 2026-09-21 现扫得出（205 字），**不是猜的**；往后靠判据钉。
///
/// 〔`24f` 第四刀 09-21：130 → 205 字。多出来的那 75 个几乎全来自
///  `find.rs` —— 搜索那一侧的**新鲜度那一行**与**退路那几句话**都是中文，
///  而它们真的会被 `SearchBoard::ui` 画出去（`colored_label` / `label`）。
///  ⚠ 这不是「顺手加了几个字」：那几句话正是「索引还没建过」「后端说该重走了」
///  这一类**用户不看就会误判**的话，字体坏掉的时候它们变豆腐块的代价最大。〕
pub const PROBE: &str = concat!(
    "§·—…→⚠⬆、。「」一上下不与东两个中串为了令件份传但位侧做全出列别到制刷前剩务动勾化半单原去参取",
    "口只台同名后吗听周命和哪器回在声复多失契字它完对少尔屏己已布带帧并应建开引录形径慢成或打扫拒拖拿挂搜",
    "搬撞改数整文新时明是更有服期本机权条来标根案次正段没洞流浏消漂点物状现用的监盖目相看着确秒空窗立端符",
    "第答索约级组经给者能自节行被西要覆览认许话该说读负败走趟路跳过运还这进远送那都里重量问限零非首（），",
    "：；？📁📄",
);

/// 出声那句话的前缀。🔴 **必须是纯 ASCII** —— 字体坏了的时候，
/// 这句话是**唯一**还画得出来的东西；用中文写它就跟着一起变豆腐块。
/// 判据 `every_notice_this_module_can_produce_is_pure_ascii` 钉着这一条。
pub const NOTICE_PREFIX: &str = "[font] ";

/// 阳性对照：这几个**必须**判成画得出。判缺任何一个 ⇒ 量具不可用。
const CONTROL_PRESENT: &str = "AZaz09^!/";

/// 阴性对照：Unicode 里**未分配**的码位，必须**全部**判成画得出不来。
///
/// ⚠ 别拿私用区（U+E000 / U+F8FF）当阴性对照：现打逮到 `emoji-icon-font`
/// **真的**占着 U+F8FF（苹果标）⇒ 那个对照会「不合」，而量具其实是好的。
/// ⚠ 这四个都是**现打验过**的（2026-09-20，两种字体配置下都被判缺）。
/// 往里加新码位之前先跑一趟 —— 「我以为它未分配」不算读数。
const CONTROL_ABSENT: [char; 4] = ['\u{0378}', '\u{05EB}', '\u{2FE5}', '\u{10FFFD}'];

/// 第一拍的结果 —— **只说「从哪装的」**。装好没有要等 [`verify`]（见 §四）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attempt {
    /// 从这个路径读到了字体并交给了 egui。
    Loaded { path: String, bytes: usize },
    /// 候选表里一个都不存在。`tried` 是试过的路径，**给用户看的**。
    NoneFound { tried: Vec<String> },
    /// 文件在，但读不动（权限 / IO）。
    ReadFailed { path: String, err: String },
}

/// 候选字体路径，**按优先级**。
///
/// `CCM_CJK_FONT` 排在最前 —— 候选表没命中你这台机器时的出路
/// （环境变量而非配置项：配置项要进前端那张 `CONFIG_KEY_OWNERS`，
///  而这是原生窗口的事，跟 webview 那侧的配置面板不同摊；
///  命名跟着仓里现有的 `CCM_NO_DEVTOOLS` 走）。
pub fn candidates() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("CCM_CJK_FONT") {
        if !p.is_empty() {
            out.push(p);
        }
    }
    #[cfg(target_os = "windows")]
    out.extend(
        [
            // 微软雅黑 —— Vista 起随系统装，**不分区域设置**
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",   // Win7 及更早是 .ttf
            r"C:\Windows\Fonts\msjh.ttc",   // 微软正黑（繁体系统）
            r"C:\Windows\Fonts\simsun.ttc", // 宋体，兜底
        ]
        .into_iter()
        .map(String::from),
    );
    #[cfg(not(target_os = "windows"))]
    out.extend(
        [
            // ↓ 本机（Debian/Ubuntu 系，fonts-noto-cjk）现打就是这一份
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", // Arch
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", // Fedora
            "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", // 文泉驿，小
            "/usr/share/fonts/truetype/arphic/uming.ttc",
        ]
        .into_iter()
        .map(String::from),
    );
    out
}

/// 第一拍：读一份 CJK 字体交给 `ctx`。**在创建闭包里调。不复核**（§四）。
pub fn install(ctx: &egui::Context) -> Attempt {
    install_from(ctx, &candidates())
}

/// 同 [`install`]，但候选表由调用方给 —— 判据要能造出「一个都找不到」那一形。
pub fn install_from(ctx: &egui::Context, candidates: &[String]) -> Attempt {
    let mut tried = Vec::new();
    for path in candidates {
        tried.push(path.clone());
        if !std::path::Path::new(path).is_file() {
            continue;
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                return Attempt::ReadFailed {
                    path: path.clone(),
                    err: e.to_string(),
                }
            }
        };
        let n = bytes.len();
        let mut defs = egui::FontDefinitions::default();
        defs.font_data.insert(
            "cc-cjk".to_owned(),
            Arc::new(egui::FontData {
                font: std::borrow::Cow::Owned(bytes),
                // `.ttc` 里选哪张 face：现打过 0..4 四张，**覆盖率一模一样**
                // ⇒ 这里不需要挑，用 0。
                index: 0,
                tweak: Default::default(),
            }),
        );
        // 🔴 **追加在末尾当兜底**，不是插在最前：拉丁字仍旧由 Ubuntu-Light／Hack 画
        // （等宽那条链要是被 CJK 字体抢走，等宽就不再等宽了）。
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(chain) = defs.families.get_mut(&fam) {
                chain.push("cc-cjk".to_owned());
            }
        }
        ctx.set_fonts(defs);
        return Attempt::Loaded {
            path: path.clone(),
            bytes: n,
        };
    }
    Attempt::NoneFound { tried }
}

/// 一个字符在字形图集里的格子。`None` = 这个字排不出字形（空串／空行）。
fn cell(ctx: &egui::Context, fid: &egui::FontId, c: char) -> Option<([u16; 2], [u16; 2])> {
    ctx.fonts_mut(|f| {
        let g = f.layout_no_wrap(c.to_string(), fid.clone(), egui::Color32::WHITE);
        let gl = g.rows.first()?.glyphs.first()?;
        Some((gl.uv_rect.min, gl.uv_rect.max))
    })
}

/// `s` 里在 `fid` 这个字族下**画不出来**的字（去重、保持出现顺序）。
///
/// ⚠ 必须在第一帧之内或之后调 —— 之前调会 panic（§四）。
pub fn unrenderable(ctx: &egui::Context, fid: &egui::FontId, s: &str) -> Vec<char> {
    let repl = cell(ctx, fid, '◻');
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for c in s.chars() {
        if cell(ctx, fid, c) == repl && seen.insert(c) {
            out.push(c);
        }
    }
    out
}

/// 量具自己过不过两个方向的对照。`None` = 过了；`Some(说明)` = 量具不可用。
pub fn ruler_self_check(ctx: &egui::Context, fid: &egui::FontId) -> Option<String> {
    let bad_present = unrenderable(ctx, fid, CONTROL_PRESENT);
    if !bad_present.is_empty() {
        return Some(format!(
            "ruler broken: {} ASCII control chars reported missing ({:?})",
            bad_present.len(),
            bad_present
        ));
    }
    let absent: String = CONTROL_ABSENT.iter().collect();
    let flagged = unrenderable(ctx, fid, &absent).len();
    if flagged != CONTROL_ABSENT.len() {
        return Some(format!(
            "ruler broken: {}/{} unassigned codepoints reported missing",
            flagged,
            CONTROL_ABSENT.len()
        ));
    }
    None
}

/// 第二拍：**自己复核装没装上**。在 `App::ui` 的第一帧调（§四）。
///
/// `None` = 量具当场自证过，且 [`PROBE`] 里每一个字在**两个字族**下都画得出来。
/// `Some(说明)` = 出了问题；说明是**纯 ASCII 起头**的一句人话，直接画到窗口上。
///
/// 🔴 刻意不是 `bool`，也不是 `Result<(), ()>` —— 与
/// `sftp_pool::copy_remote_path` 同一个形：**静默退化在类型上就做不到**。
pub fn verify(ctx: &egui::Context, attempt: &Attempt) -> Option<String> {
    let families = [
        ("proportional", egui::FontId::proportional(14.0)),
        ("monospace", egui::FontId::monospace(14.0)),
    ];
    // ① 先验量具。量具坏了就别谈探针 —— 那时候「零缺字」是没有意义的一句话。
    for (name, fid) in &families {
        if let Some(why) = ruler_self_check(ctx, fid) {
            return Some(format!("{NOTICE_PREFIX}{why} [{name}]"));
        }
    }
    // ② 再验探针。
    let mut worst: Vec<(&str, Vec<char>)> = Vec::new();
    for (name, fid) in &families {
        let miss = unrenderable(ctx, fid, PROBE);
        if !miss.is_empty() {
            worst.push((name, miss));
        }
    }
    if worst.is_empty() {
        return None;
    }
    let whence = match attempt {
        Attempt::Loaded { path, bytes } => format!("loaded {path} ({bytes} bytes)"),
        Attempt::NoneFound { tried } => format!(
            "no CJK font found; tried {} paths: {}",
            tried.len(),
            tried.join(", ")
        ),
        Attempt::ReadFailed { path, err } => format!("failed to read {path}: {err}"),
    };
    let detail = worst
        .iter()
        .map(|(name, miss)| {
            // 🔴 报**码位**，不报字符本身 —— 那几个字正是画不出来的那几个，
            //    把它们塞进这句话等于让这句话自己也变成豆腐块。
            let sample: Vec<String> = miss
                .iter()
                .take(6)
                .map(|c| format!("U+{:04X}", *c as u32))
                .collect();
            format!(
                "{name}: {} chars unrenderable (e.g. {})",
                miss.len(),
                sample.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "{NOTICE_PREFIX}text will show as boxes -- {detail}. Set CCM_CJK_FONT=/path/to/a/CJK/font.ttc to fix. ({whence})"
    ))
}

/// 窗口手上那份字体状态。
///
/// 🔴 **三态，不是两态** —— 「还没复核」必须和「复核过、没问题」分得开。
/// 合成两态（比如用 `Option<String>` 一个字段）的话，「复核这一步根本没接上」
/// 就长得跟「一切正常」一模一样 —— 那正是本仓 `§2.9` 反复禁的那一形
/// （「跳过」与「过了」在终端上长得一样）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontState {
    /// 压根没装过。判据直接 `FileWindow::new(...)` 建出来的窗口就是这一态。
    NotInstalled,
    /// 装过了，还没复核（第一帧之前）。
    Pending(Attempt),
    /// 复核过了。`None` = 没问题；`Some` = 要画到窗口上那句话。
    Checked(Option<String>),
}

impl FontState {
    /// 第一帧进来时调。**只在这一拍从 `Pending` 走到 `Checked`**；
    /// 回 `true` 表示「这一帧刚刚定下来」（调用方拿它决定要不要记账，
    /// 免得每帧都去抢那把锁）。
    pub fn settle(&mut self, ctx: &egui::Context) -> bool {
        match self {
            Self::Pending(a) => {
                *self = Self::Checked(verify(ctx, a));
                true
            }
            _ => false,
        }
    }

    /// 要画在窗口上的那句话。`None` = 不用出声。
    ///
    /// ⚠ `NotInstalled` **也出声** —— 它意味着「装字体那一步没接上」，
    /// 而那和「字体没装上」对用户是同一件事。**不许让它安静地过去。**
    pub fn notice(&self) -> Option<&str> {
        match self {
            Self::NotInstalled => {
                Some(concat!("[font] ", "font install never ran for this window"))
            }
            Self::Pending(_) => Some(concat!("[font] ", "font check never ran for this window")),
            Self::Checked(n) => n.as_deref(),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/bridge/filewin/fonts_tests.rs"]
mod tests;
