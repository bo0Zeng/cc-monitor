//! **让原生窗口上的字画得出来**。
//!
//! 设计住（「字体（CJK）」那一行；此前引的 `§5.4g` 那一节从来不存在）；现打读数住。
//!
//! ═══════════════════════════════════════════════════════════════════════
//! # 🔴 一、修什么：不装字体，窗口**基本没法读**
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
//! | **改之前** | 画不出 **121** | 画不出 **120** |
//! | 装上系统 CJK 字体 | **0** | **0** |
//!
//! 等宽比比例多认一个字：`→`（U+2192）在 `Hack` 里有，而 `Hack` 只在等宽那条链上。
//!
//! ⚠ 两条**不要搞混**的：
//! ① `⬆` `⚠` `📁` `📄` `…` `—` 这六个符号**本来就画得出**（在那两份 emoji 字体里）。
//!    量具第一版说它们画不出 —— 那是**量具在撒谎**，原因见 §三。
//! ② 窗口标题里的中文（`shell::window::open_detached_seeded` 那个 `format!`）**不经 egui** ——
//!    它是窗口管理器画的，这里碰不到它，也不需要碰。
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
//! | 第一拍 [`load`] | 创建闭包起的一条线程；交回由 `Workspace::frame` 收（[`Loading::poll`]） | 扫字体 · 读文件，收下时 `set_fonts`。**不复核** |
//! | 第二拍 [`verify`] | 收下之后的那一帧 | 跑量具，出 `Option<String>` |
//!
//! 第一拍不在画帧那条线程上：冷缓存下扫一遍系统字体要几秒，窗口要等第一帧画完才摆上屏幕。
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
/// 字数与人群不在这里记：判据现扫源码树、两向差集对，往后靠它钉。
pub const PROBE: &str = concat!(
    "·…→、「」一上下不与丢两个中为主了交亮代以件份会传位作侧保做先全关内再写出列刚删别到制刷前动原发取口只可台合同名后含命器回在坏型复外多",
    "大失字存它完容对小就屏已并序建开当录往径成或截打扫把拒拖按换排接搜撤操改数整文断新时是更最有本机权样根框次此段比法消点",
    "用的盖目相看确称程空窗端符第答类索约级组终经结继续绝编者能节行表被要覆览认读败起超路跳载辑输过这进远连部里重销错闭限除非须首高（），：",
    "向式横模滚长",
    "头选项",
    "变收",
    "×书加找移签",
    "双另右己底标栏自页",
    "处",
    "显示预",
    "启格码",
    "值择置间需",
    "仍然盘",
    "算链",
    "查替",
    "解压",
    "执批果校验所",
    // 工具条提示 · 命令栏 · 左栏 · 状态栏 · 属性框 · 「类型」那一列 · 图片预览。
    "从修图地址始子展属左性指片藏试退配隐他其包夹缩",
    // 文件名搜索。
    "得",
    // 搜索结果的状态行与表（「文件清单未建」「清单仅前 N 项」「无匹配」「名称 · 按相关度」）。
    "仅匹单度无未清",
    // 「停」「详情」「点击重试」「回车」「已达读取上限」。
    "停击情详车达",
    // 删除那一问「不可恢复」。
    "恢",
    // 「进度」表：「开始后不可停止」「计算大小」。
    "止计",
    // 断线条：「离线 · 采样 13:40」「离线 · 只读」。
    "离线采",
    // 编辑页：「放弃改动并重开」「改动保留」「不保存 = 丢弃改动」（面包屑的分隔是 Phosphor 图标字，不进探针）。
    "弃放留",
    // 就地操作：「其他人」「需填数字写法」「当前权限不一致」。
    "人填致",
    // 「进度」表断线时在跑的那几行「等待重连」。
    "待等",
    // 拖进来时「松开上传 → 目录（n 个文件）」。
    "松",
    "应析知",
    // 「不适用于所选项」「均认不出名字」「该机」…
    "于入及均定持支识该身适途",
    // 「原因见它自己的错误输出」。
    "见",
    // 一组一条命令那一层：「逐项选」· 断开那一句「其余待核」。
    "余核逐",
    // 「如名字加 (2)」「存到段 N / M 时中断」。
    "分如",
    // 文件窗口反查 `plan.rs`：「格」一列读不成写「—」· 预览头「归属」· 右键「在计划里看」· 切到计划页。
    "—也切划因明",
    // 图片查看页：「原尺寸」「上一张 / 下一张」「已是第一张 / 最后一张」·「像素过多 · 无法解码」。
    "像寸尺张素",
    // 通道那一句（通信层 `chan/said.rs`）：「本机后端未运行 · 未执行」。
    "运",
    // 「复制为」框的缺省名「n1 副本.txt」。
    "副",
    // 文件窗口反查那一列与预览头：「节点」「权力范围「…」」。
    "力围范",
    // 被几格声明的那一段：「共用 · 也在 {编号 标题}」。
    "共",
    // 通道那一句里机器名为空（`chan/said.rs::blank_origin`）：「请求格式不对 · 机器名为空 …」。
    "求请",
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
    /// 载入的那条线程没交回就没了（起不来 · 半路 panic）。
    Lost,
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
    // 按平台的系统字体候选（Windows 的微软雅黑一族 · Linux 各发行版的 Noto CJK / 文泉驿）住 `platform.rs`。
    out.extend(
        crate::platform::SYSTEM_CJK_FONTS
            .iter()
            .copied()
            .map(String::from),
    );
    out
}

/// 主界面那一套字族里**这台机器装着的第一个**（按名字找；`system-ui` / `sans-serif` 这类泛称不算名字，跳过）。
/// 找不到 ⇒ `None`（照旧用 egui 自带的那一份，CJK 回退不变）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Face {
    pub family: String,
    pub bytes: Vec<u8>,
    pub index: u32,
}

/// 主界面字族那一行里的泛称（CSS 的通用字族）—— 它们不是一份字体的名字。
pub const GENERIC_FAMILIES: [&str; 8] = [
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-sans-serif",
    "ui-monospace",
];

/// 在 `db` 里按 `families` 的先后找第一个装着的。
pub fn find_face(db: &fontdb::Database, families: &[String]) -> Option<Face> {
    families
        .iter()
        .filter(|f| !GENERIC_FAMILIES.contains(&f.to_ascii_lowercase().as_str()))
        .find_map(|f| {
            let q = fontdb::Query {
                families: &[fontdb::Family::Name(f)],
                ..Default::default()
            };
            let id = db.query(&q)?;
            db.with_face_data(id, |data, index| Face {
                family: f.clone(),
                bytes: data.to_vec(),
                index,
            })
        })
}

/// 这扇窗要的另外两份字：正文（主界面 `--font-base`）· 等宽（`--font-mono`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Faces {
    pub ui: Option<Face>,
    pub mono: Option<Face>,
}

/// 照主题的字族去这台机器的字体里找（扫一遍系统字体：冷缓存下要几秒，只在 [`load`] 那条线程上跑）。
fn faces_for(theme: &filewin_contract::Theme) -> Faces {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    Faces {
        ui: find_face(&db, &theme.font_base),
        mono: find_face(&db, &theme.font_mono),
    }
}

/// 后台那一趟载入：交回时把字体交给 `ctx`（[`Loading::poll`]）。
pub struct Loading(std::sync::mpsc::Receiver<(egui::FontDefinitions, Attempt)>);

/// 第一拍：主题字体（有就放链首）· 图标字 · 一份 CJK 字体（链尾兜底）在另一条线程上备好，**不复核**（§四）。
/// 画帧那条线程不等它：没交回之前窗口只画底色与转圈（`Workspace::frame`）。
pub fn load(ctx: &egui::Context, theme: Option<filewin_contract::Theme>) -> Loading {
    load_with(ctx, move || {
        let faces = theme.as_ref().map(faces_for).unwrap_or_default();
        definitions(&candidates(), &faces)
    })
}

/// 同 [`load`]，备字体那一步由调用方给（判据用）。
pub fn load_with(
    ctx: &egui::Context,
    job: impl FnOnce() -> (egui::FontDefinitions, Attempt) + Send + 'static,
) -> Loading {
    let (tx, rx) = std::sync::mpsc::channel();
    let ctx = ctx.clone();
    // 线程起不来 / 半路没了 ⇒ `tx` 随之丢掉，[`Loading::poll`] 交 [`Attempt::Lost`]（照样复核、出声）。
    let spawned = std::thread::Builder::new()
        .name("filewin-fonts".into())
        .spawn(move || {
            if tx.send(job()).is_ok() {
                ctx.request_repaint();
            }
        });
    if let Err(e) = spawned {
        tracing::warn!("filewin: font loading thread did not start: {e}");
    }
    Loading(rx)
}

impl Loading {
    /// 交回了 ⇒ 装进 `ctx`（下一帧起生效）、回「从哪装的」；还没交回 ⇒ `None`。不阻塞。
    pub fn poll(&self, ctx: &egui::Context) -> Option<Attempt> {
        match self.0.try_recv() {
            Ok((defs, attempt)) => {
                ctx.set_fonts(defs);
                Some(attempt)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Attempt::Lost),
        }
    }
}

/// 系统 CJK 字体那一份的读上限：常见的那几份（Noto Sans CJK · 微软雅黑 · 宋体的 `.ttc`）20 MiB 上下。
/// 超了降级：不挂 CJK 兜底（中日韩字画成方框），结局 [`Attempt::ReadFailed`] 照实带出去、进日志。
const CJK_FONT_CAP: host_core::Cap = host_core::Cap {
    bytes: 64 * 1024 * 1024,
    over: host_core::Over::DegradeAndSay,
};

/// 一份字体的数据。
fn data(bytes: Vec<u8>, index: u32) -> Arc<egui::FontData> {
    Arc::new(egui::FontData {
        font: std::borrow::Cow::Owned(bytes),
        index,
        tweak: Default::default(),
    })
}

/// 备字：主题字体放链首（拉丁字由它画，同主界面）；图标字跟在 egui 自带正文字后面；CJK 字体**追加在末尾当兜底**。
fn definitions(candidates: &[String], faces: &Faces) -> (egui::FontDefinitions, Attempt) {
    let mut defs = egui::FontDefinitions::default();
    // 图标（工具条 · 文件种类）：Phosphor 那一份，只占私用区，挂在比例字链第二位（排在 egui 自带正文字之后）。
    egui_phosphor::add_to_fonts(&mut defs, egui_phosphor::Variant::Regular);
    for (name, face, fam) in [
        ("cc-ui", &faces.ui, egui::FontFamily::Proportional),
        ("cc-mono", &faces.mono, egui::FontFamily::Monospace),
    ] {
        if let Some(f) = face {
            defs.font_data
                .insert(name.to_owned(), data(f.bytes.clone(), f.index));
            if let Some(chain) = defs.families.get_mut(&fam) {
                chain.insert(0, name.to_owned());
            }
        }
    }
    let mut tried = Vec::new();
    let mut attempt = None;
    for path in candidates {
        tried.push(path.clone());
        if !std::path::Path::new(path).is_file() {
            continue;
        }
        match host_core::read_capped_bytes(std::path::Path::new(path), CJK_FONT_CAP) {
            Ok(host_core::Capped::Whole(bytes)) => {
                let n = bytes.len();
                // `.ttc` 里选哪张 face：现打过 0..4 四张，**覆盖率一模一样** ⇒ 用 0。
                defs.font_data.insert("cc-cjk".to_owned(), data(bytes, 0));
                // 🔴 **追加在末尾当兜底**：等宽那条链要是被 CJK 字体抢走，等宽就不再等宽了。
                for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    if let Some(chain) = defs.families.get_mut(&fam) {
                        chain.push("cc-cjk".to_owned());
                    }
                }
                attempt = Some(Attempt::Loaded {
                    path: path.clone(),
                    bytes: n,
                });
            }
            Ok(host_core::Capped::TooBig) => {
                attempt = Some(Attempt::ReadFailed {
                    path: path.clone(),
                    err: format!("over the {} byte cap", CJK_FONT_CAP.bytes),
                });
            }
            // 判过 `is_file` 之后又没了：照读不出说。
            Ok(host_core::Capped::Absent) => {
                attempt = Some(Attempt::ReadFailed {
                    path: path.clone(),
                    err: std::io::Error::from(std::io::ErrorKind::NotFound).to_string(),
                });
            }
            Err(e) => {
                attempt = Some(Attempt::ReadFailed {
                    path: path.clone(),
                    err: e.to_string(),
                });
            }
        }
        break;
    }
    let attempt = attempt.unwrap_or(Attempt::NoneFound { tried });
    (defs, attempt)
}

/// 一个字符在字形图集里的格子。`None` = 这个字排不出字形（空串／空行）。
fn cell(ctx: &egui::Context, fid: &egui::FontId, c: char) -> Option<([u16; 2], [u16; 2])> {
    ctx.fonts_mut(|f| {
        let g = f.layout_no_wrap(c.to_string(), fid.clone(), egui::Color32::PLACEHOLDER);
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
/// 🔴 刻意不是 `bool`，也不是 `Result<(), ()>`：
/// **静默退化在类型上就做不到**。
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
        Attempt::Lost => "font loading stopped before handing back".to_string(),
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
    /// 装过了，还没复核（收下字体的那一帧）。
    Pending(Attempt),
    /// 复核过了。`None` = 没问题；`Some` = 要画到窗口上那句话。
    Checked(Option<String>),
}

impl FontState {
    /// 第一帧进来时调。**只在这一帧从 `Pending` 走到 `Checked`**；
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
#[path = "../../../../tests/frontend/filewin/fonts_tests.rs"]
mod tests;
