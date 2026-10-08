//! 步 `24e` 第四刀：**让原生窗口上的字画得出来**。
//!
//! 设计住（「字体（CJK）」那一行；此前引的 `§5.4g` 那一节从来不存在）；现打读数住。
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
/// 现值由 2026-09-22 现扫得出（313 字），**不是猜的**；往后靠判据钉。
///
/// 〔`24f` 第四刀 09-21：130 → 205 字。多出来的那 75 个几乎全来自
///  `find.rs` —— 搜索那一侧的**新鲜度那一行**与**退路那几句话**都是中文，
///  而它们真的会被 `SearchBoard::ui` 画出去（`colored_label` / `label`）。
///  ⚠ 这不是「顺手加了几个字」：那几句话正是「索引还没建过」「后端说该重走了」
///  这一类**用户不看就会误判**的话，字体坏掉的时候它们变豆腐块的代价最大。〕
///
/// 〔`24e` 第五刀 09-21：205 → 249 字，人群 9 → 10 份（多的是 `writeops.rs`）。
///  多出来的那 44 个几乎全来自那四条写操作的**按钮字面与确认框**
///  （「新建目录」「改名」「删除」「权限」「做勾上的」「都别做」「不可撤销」）
///  ＋ **被围栏挡住那句话**。⚠ 后者是这一批里最要紧的：它是「为什么这件事
///  没做成」的唯一出口，变成豆腐块的时候用户会以为「点了没反应」，
///  而这一刀正是窗口第一次能破坏用户数据的那一刀。〕
///
/// 〔`24e` 第七刀 09-21：249 → 253 字，而它**不是单纯的加** ——
///  `−1 +5`：去掉的是 `哪`（那是 `entry.rs` 里 `"要打开哪个目录？路径是空的"`
///  唯一的住址，那句话随「空路径改成问远端 home」一起没了），
///  加上的是 `以 必 绝 解 须`（`source.rs` 里「对面把 home 解成了空串/相对路径」
///  那两句）。⚠ **那两句同样会被画出去**（工具栏那行 `colored_label(RED, e)`）
///  ⇒ 它们进探针不是宽松，是实况。
///  🔴 顺带一条读数：这一拍**同时**在探针里留下一个用不着的字与漏掉五个字，
///  而那条判据把两个方向都印了出来（「多出来」＋「漏了」）—— 只钉字数的话，
///  `−1 +1` 那种改动会安安静静地过去。〕
///
/// 〔`24e` 第八刀 09-21：253 → 270 字，人群 10 → 11（多的是 `download.rs`）。
///  多出来那 17 个来自**往外拖那两问**：「把〈名〉存到哪儿？」「那儿已经有东西了」
///  ＋ 🔴「**盖掉它就没有备份了，不可撤销**」＋ 结局那两句（「已存到 …」／「拖到 … 没成」）。
///  ⚠ 这一批里承重的是那句「不可撤销」：`download_inner` 是 `.part` → `rename` 上位，
///  原处那个文件没有备份。它变成豆腐块的时候，用户会照点 ——
///  而这一刀正是窗口第一次能**盖掉本机文件**的那一刀。
///  ⚠ 顺带：`哪` 回来了（第七刀它随「要打开哪个目录？」一起走的，这次是「存到哪儿？」）
///  ⇒ 那也是「只钉字数会漏掉 `−1 +1`」的第二个实例。〕
///
/// 〔`24e` 第九刀 09-22：270 → 295 字，人群 11 → 12（多的是 `editor.rs`）。
///  多出来那 25 个来自**编辑面**：标题那一行 · 超上限那一句 · 存的结局两句，
///  ＋ 🔴「**改了还没存 / 关掉就丢掉你敲的那些东西了 —— 远端那份还是旧的**」。
///  ⚠ 这一批里承重的是最后那一句：它是「这一下会丢掉你的输入」的唯一出口。
///  它变成豆腐块的时候，用户会点「丢掉，关」—— 而那些字他没有第二份。〕
///
/// 〔`24e` 第十刀 09-22：295 → 297 字（人群不变，12 份）。多的只有「**高亮**」两个字
///  —— reveal 找不到那一行时那句「这个目录里没有 X …」。
///  ⚠ 那一格的**高亮本身是一块背景色，不是文字** ⇒ 探针管不着它；
///  它的可判读出是 `RenderTally::revealed_row`（同一处写下，不可能漂开）。〕
///
/// 〔`24e` 第十一刀 09-22：297 → 308 字（人群不变）。多的 11 个全来自
///  `entry.rs` 那句**新的失败话**：「窗口没起来：…」＋ 它下面那三支
///  （线程回了成功但在预算内结束 / panic 了原因只在 stderr / 带着下层原文）。
///  🔴 它补的正是一个**静默成功**：此前那个句柄被 `let _ = …` 丢掉，
///  而同一进程里第二次开窗必然失败 ⇒ 屏幕上什么都没有、界面却说「成功」。
///  ⇒ 这一批字变成豆腐块的代价，恰恰是把那个静默又变回静默。〕
///
/// 〔`24e` 第十二刀 09-22：308 → 313 字。多的 5 个来自**那两句退路/截断的话**：
///  「这一屏没走后端：…」与「这个目录条目太多，只拿到了前 N 条 ——
///  **没看见的文件不代表它不在**」。
///  🔴 两句都是「静默降级」的出声口：前者是分层退回了前端，
///  后者是「目录里就这么多」与「只给了前 N 条」在屏幕上分不开。
///  ⚠ 其中一个是 `⇒` —— **它是箭头字形**，与 `→` 同族。
///  `→` 现打在比例字体下画不出（见下面那条反空真锚），`⇒` 装了 CJK 字体之后才有；
///  ⇒ 这一格正是字体那一族存在的理由的实例。〕
// 🔴**−10 ＋1，而这两笔的来路不同，各记一句：**
// · **−10**（功 因 束 此 炸 算 线 静 预 默）—— 它们来自 `entry.rs` 那两句已经不存在的话
//   （「那条线程回了成功，但它在开窗预算内就结束了」「开窗那条线程炸了（panic）」）。
//   开窗改成起一个独立进程之后，那两句的对应物住 `filewin/proc.rs`，而 `proc.rs`
//   **不在本探针的人群里**（它一个字都不画在 egui 上，逐条理由住 `fonts_tests::drawn_files`）。
//   ⚠ **这一笔是「变少」方向**，而地板在这个方向上是瞎的 —— 接得住它的是
//   `the_probe_equals_every_non_ascii_char_in_the_window_labels` 那条**两向差集**。
// · **＋1**（交）—— 来自 `entry.rs` 新那句日志「{n} 行已经交给它了」。
// 🔴〔本机侧退役 2026-09-23〕**−6，一个都不＋：`⇒ 么 什 四 标 立`。**
//
// 逐字对上它们来自哪一句已删的话（**这一笔全在「变少」方向**，而地板在这个方向上
// 是瞎的 ⇒ 接住它的是 `the_probe_equals_every_non_ascii_char_in_the_window_labels`
// 那条**两向差集**，本轮它正是这么红的，红在「探针里多出来、界面上没有」那一格）：
// · `⇒` —— `shell::FileWindow::reload` 那句「这个窗口没有 tokio 运行时
//   ⇒ 问不了后端，这一屏是 monitor 自己列的」。本机侧不在了 ⇒ 没有运行时只剩「出声」。
// · `什` `么` —— 「这个窗口现在看的是本机，没有什么可往外拖的」。
// · `标` —— 「这个窗口现在看的是本机，拖进来的文件没有远端目标」。
// · `立` —— 「这个窗口现在看的是本机，零流量复制只在远端那一侧成立」。
// · `四` —— 「这个窗口现在看的是本机 —— 新建目录 / 改名 / 删除 / 改权限走的是
//   远端那四条 SFTP 命令」。
//
// ⚠ 「本机」两个字**照旧在探针里**，而且来路换了 —— 它们今天来自 `download.rs`
//   那句「存到哪儿（本机）」与 `local_home` 那一路，不是来自一颗「本机」按钮。
//   ⇒ 「一个字还在探针里」不等于「那个功能还在」，这两件事本探针分不开。
// 🔴〔补齐五项 2026-09-23〕**＋8：`小 序 排 此 称 终 请 🔗`**，逐字来路：
// · `排` `序` —— 工具栏那个下拉「排序：名称 / 大小 / 类型」。
// · `此` `终` —— 「在此打开终端」那颗按钮。
// · `小` —— 下拉里「大小」那一档（`source::SortBy::label`）。
// · `请` —— 「终端开不了，请重开这个窗口」（没有运行环境时那句话；CP1 那一轮把「要一个
//   tokio 运行时」那一族裁成 `改·§2.1`，新写的这一句直接按改过的口径写）。
// · `称` —— 下拉里「名称」那一档。
// · `🔗` —— 符号链接那一行的标记（`rows::paint_one_row`）。⚠ 上一版用的是旧面板那个 `↳`，
//   现打它在比例字体链上**装了 Noto CJK 也画不出**（`installing_a_system_font_…` 红在
//   「还缺 1 个字：['↳']」）⇒ 换成 egui 自带 emoji 字体里就有的 `🔗`。
// 🔴**＋11 −3**（窗口改成只经通道说 `call`），逐字来路（现打 `git show` 对过）：
// · ＋`主 发 够 拼 版 连 通 道 错 面` —— `source::said` 那几句（「窗口到主程序那一段…」
//   「这一趟没发出去」「参数这一侧就拼错了」「应答对不上约定」「后端不认…，多半是版本旧了」）
//   与 `shell::NO_LINE`（「这个窗口没连上后端」）；＋`样` —— `source::ask`「和约定的不一样」。
// · −`§` —— `find::call_one` 旧那句「与 `src/doc/IPC-PROTOCOL.md §10` 那份契约不符」；
// · −`但 案` —— `find::routed_text` 旧那句「后端说做完了，但没给答案」。两句随直连登记表那条路一起走了。
// 🔴**−9，一个都不＋：`务 己 慢 搬 服 流 自 量 零`**（「变少」方向，接住它的是两向差集）。
// · 全部来自 `copy.rs` 那两句随 SFTP 复制一起走掉的话：成功那句「服务端自己搬的字节，零流量」
//   与退路那句前缀「这一趟走的是慢路」。复制换到后端（在那台机器上复制，没有会过网的退路）之后，
//   成功那句换成「复制完成：N 字节，在那台机器上复制的，没经过你这台机器」—— 刻意只用探针里已有的字。
// 🔴**＋8：`共 向 式 幕 横 模 滚 长`**，全部来自 `bigfile.rs`（大文件模式）：
// · `共` —— 没排全的那一行行尾那枚「这一行共 N 字」（要求逐字要的那句）。
// · `模 式 幕 长 向 横 滚` —— 编辑面顶上那一行「大文件模式：…只排屏幕上的几行，长行不换行，可横向滚动。」
// 🔴**＋6：`事 任 何 头 选 项`**（与 F9 同一手法：单独一段接在最后，不改前几段），逐字来路（都住 `select.rs` / `shell.rs`）：
// · `选 项` —— 工具栏「已选 N 项」· 菜单上「删除这 N 项」· 做不了那几句（`select::refusal`）。
// · `任 何` —— 「还没有选中任何一项」（按 Delete / F2 时一项都没选）。
// · `头` —— 打字跳转没找到：「没有以「…」开头的项」。
// · `事` —— 菜单一项都没有时那一句（`shell::MENU_EMPTY`）。
// 🔴**＋9：`么 反 变 号 控 收 斜 杠 至`**，全部来自 `editor.rs` 存得回存不回那两句：
// · `editor::too_big_to_save`〔散文墓碑〕「…后端一次最多**收** … 没有发出去。引**号**、**反斜杠**和**控**制字符在发送时会**变**长，删掉**至**少这**么**多再存。」
// · `editor::read_only_notice`〔散文墓碑〕「只读：…可以看、可以复制，不能改。」（这一句的字都已在探针里）。
// 🔴**−9，一个都不＋：`么 反 变 号 控 收 斜 杠 至`**（「变少」方向，接住它的是两向差集）。
// · 上面那两句随「打开即只读」一档删了（存盘装不进一行的改走暂存区分块，`editor.rs::write_text`，不再有存不回的文本）。
// · 新写的两句刻意只用探针里已有的字：`editor.rs::over_cap_notice`「这份有 N 字节，超过 M 字节的上限，多了 K 字节，没有发出去。」·
//   `editor.rs::chunk_failed_notice`「存到第 N 段（共 M 段）时断了：…」（用「段」不用「块」：后者不在探针里）。
// 🔴**＋9：`× ★ ☆ 书 加 懂 找 移 签`**，全部来自 `bookmarks.rs`（书签栏）：
// · `☆ 加` `★` `书 签` —— 那颗切换按钮的两态「☆ 加书签」/「★ 取消书签」。⚠ 两颗星是**状态**，
//   不是装饰：空心 ＝ 当前目录还不在书签里，实心 ＝ 已经在。豆腐块了就分不出这两态。
// · `×` `移` —— 每条书签后面那颗「×」与它的悬停提示「移出书签」。
// · `找` —— 「书签存不了：找不到本程序的数据目录」。`懂` —— 「书签文件读不懂…」（文件被手改坏时那句）。
// 🔴〔FW34 标签页 ＋ 双栏〕**＋14：`● ＋ 双 另 右 好 己 底 忙 恰 标 栏 自 页`**，来自 `workspace.rs` 与 `shell.rs::busy_reason`：
// · `双 栏` —— 工具条「双栏」；`另` —— 「复制到另一栏」；`右` —— 「右栏还没忙完…」。
// · `标 签 页` 里新的是 `标 页`（`签` 上一拍已进）；`＋` —— 标签栏那颗「新标签页」；`●` —— 后台标签「这里有事等你」。
// · `忙` —— 「这个标签页还没忙完（…），先别关」；`恰 好` —— 「要选中恰好一个文件」；
//   `自 己` —— 「两栏是同一个目录，复制过去就是它自己」；`底` —— 跨目录复制算不出公共根时那一句「… 不在 … 底下」。
// 🔴〔FW34 搬家那一跳〕**＋5：`处 搬 板 留 老`**，全部来自 `entry.rs` 搬家那两句
//   「老面板的书签搬不过来 …（旧书签还留在原处）」（`entry.rs` 在本探针的人群里）。
//   ⚠ 这 5 个字是子步 3 那一拍带进来的，那一拍**漏跑了本探针**（只跑了 entry / bookmarks / boundary）⇒ 那个提交上
//   这里两条红，子步 4 这一拍补上。
// 🔴〔书签旧键退役〕**−4：`搬 板 留 老`**（那两句删了；`处` 别处还在用，留下）。
// 🔴〔FW34 预览〕**＋3：`显 示 预`** —— `preview.rs`：工具条 / 面板标题「预览」与没选时那一句
//   「选中一个文件，这里显示它的内容」（`览` 早在探针里，`浏览` 那一族）。
// 🔴〔F9c 与 FW34 合并〕F9c 那 −9 里的 `变 收` 两个字**留下**：`preview.rs`「…或者刚变大了，不预览」·
//   `workspace.rs`「右栏还没忙完（…），先别收」仍在用 ⇒ 合并后净 −7（`么 反 号 控 斜 杠 至`）。
// 🔴〔全量抽表〕**＋9 −3**：窗口上的字搬进了文案表（量具同拍改成「字面量 ＋ 源码里 `copy_text` 取的表条目」），
//   顺带照 CP1 台账与文案规范改了几句：
// · ＋`启` —— 「搜索 / 上传 / 复制 … 启动不了，请重开这个窗口」（原「…要一个 tokio 运行时，这个窗口没拿到」，CP1 裁 §2.1）；
// · ＋`支 持` —— 「那台机器上的后端版本旧了，不支持这个操作」（原句点内部命令名，CP1 裁 §2.1）；
// · ＋`返 回 误` 里新的是 `返 误` —— 「后端返回的内容读不懂」「内部错误，请重开窗口」；
// · ＋`格 求 码 象` —— 「path 字段的格式不对：只认字符串或 b16 编码的对象」「问 home 目录的回答里没有 path 字段」「请求的参数拼不出来」；
// · −`运`（运行时）· −`解`（「远端把 home 解成了…」改说「远端的 home 目录是…」）· −`之`（「…之后不是」那句的改写）。
// 🔴〔filewin 第二份〕**＋9 −24**（find · workspace · bookmarks · transfer · copy · preview · select · editor · upload · entry · create · bigfile · download · rows 进表，
//   顺带照 CP1 台账与术语表改了几句）：
// · ＋`扫 描 间 隔` 里新的是 `描 间 隔` —— 新鲜度那一行「… 秒前扫完 · 扫描间隔 N 秒」（原「后端声明的重走周期」，「重走」是禁档词）；
// · ＋`置 选 择` 里新的是 `置 择` —— 「出错了，请重新选择保存位置」「要存到哪儿？保存位置是空的」（原「落点」，禁档词）；
// · ＋`需` —— 「 · 需要更新」（原「后端说该重走了」）；＋`值` —— 「{k} 不是一个布尔值」；
// · ＋`准` —— 「文件窗口连不上后端（主程序那一侧没准备好）」（原「主程序的通道口没起来」）；
// · ＋`边` —— 「回答里少了 {k} 字段，两边版本可能对不上」（原「后端与这一侧的契约漂了」）；
// · −24：`契 道 该 …` 这些字随「契约漂了」「通道口没起来」「这一侧的契约」「后端声明的」等说法一起走了（CP1 裁 §2.1 / §2.2）。
// 🔴**−9，零 ＋：`住 历 史 场 弄 挡 浏 管 跑`**（343 → 334，现打）—— FN1 那 −8 在 CP2b 那一侧还留着（CP2b 把「挡住了」那句搬进了表项
//   `rsFilewinWriteops.fence.notice`，量具照样数到它）；合并后那句话连同表项一起删了（FN1），多掉的一个 `浏` 是同一句「用历史浏览器」里的。
// 🔴〔用户〕**−8，一个都不＋：`住 历 史 场 弄 挡 管 跑`**（「变少」方向，接住它的是两向差集）。
// · 全部来自 `writeops.rs` 那道本地围栏预判被挡时画的那一句「⚠ 挡住了：… 是 Claude 的会话数据，动它会弄坏正在跑的那场会话。
//   要管这些用历史浏览器。」—— 用户「文件管理器全部都可以改. 不需要任何围栏」⇒ 预判与那句话一起删了。
// 🔴**−1，一个都不＋：`恰`**（338 → 337，与 FW1 合并后现打）—— 表项 `rsFilewinWorkspace.copyToOther.needOne`
//   「要选中恰好一个文件（现在选中了 N 项）」随「复制到另一栏只收一个文件」一起删了（今天收一摞，批量复制）；
//   同一句里的 `好` 别处还在用（留下）。新写的几句刻意只用探针里已有的字。
// 🔴**＋4、零 −：`仍 然 摘 盘`**，全部来自 `editor.rs` 存盘 CAS 那几句：
// · `仍 然` —— stale 之后那颗按钮「仍然覆盖」；`盘` —— 「盘上那份在你打开后被改过了，这次没有存」「盘上那份现在不是能编辑的文本…」；
// · `摘` —— 「后端没交回新内容的摘要」「应答里没有「这份内容的摘要」」（后端太旧 / 契约不符那两形）。
//   其余几句（「丢掉改动，重新打开」等）刻意只用探针里已有的字。
// 🔴**＋2、零 −：`算 链`** —— `size.rs`「算大小」那一项与结局那一句「N 条链接没算进去」（算目录大小）。
// 🔴**＋2、零 −：`查 替`** —— 编辑面那一截查找替换（「查找」「替换为」「全部替换」「没找到…」「大文件模式没有查找替换」）。
// 🔴**＋2、零 −：`解 压`** —— `extract.rs`「解压到这里」那一项 · 在解那一行 · 撞名那一问 · 结局那一句。
//   同一路里删掉的「有损目录里做不了」那几句（`rsFilewinShell.lossyCwd.*`）所用的字别处都还在，一个没少；新写的其余几句刻意只用探针里已有的字。
// 🔴〔发版前文案〕**＋9 −5：＋`之 半 执 批 果 校 缺 验 所` −`也 摞 通 摘 说`**（339 → 343），来自 filewin 那几句去行话：
// 「上一摞」→「上一批」· 断在哪一段改说「文件窗口与主程序之间 / 主程序与后端之间」· 「对面可能已经做了」→「可能已经执行了」·
// 「两边版本可能对不上」→「多半是版本不对」· 回答「缺」某字段 · 「搜索结果」· 「校验值」· 「所以没有打开」；
// 「也 通 摘 说」随改写的句子走了（「摘要」「没说完」那两句不再讲实现）。
// 🔴**＋2 −1：`得 词` 进、`送` 出**（文件名搜索换成 Everything 式）：`得 词` —— 搜索词写错时那一句「搜索词写得不对」；
// `送` 随「只回送了 N 条」换成「已列出 N 条，往下滚动看更多」走了。
// 🔴〔文案第三段 C 类〕**＋4 −4：＋`应 方 析 知` −`扇 持 支 返`**（379 → 379）：一族一句换成新写法 ——
// 「无应答」「内容无法解析」「结果未知」进；「不支持 / 返回 …」那几句的旧说法随改写走了。
pub const PROBE: &str = concat!(
    "·…→、。「」一上下不与丢两个中串为主了交亮代以件份会传位作你侧保做先全关内再写几出列刚删别到制刷前动勾化原去参发取口只可台合同名后含命器回围在坏型复外多",
    "大失字存它完容对小少尔就屏已布帧并序建开当录往径态成或截打扫把拒拖拼按换据排接搜撤操改数整文断新旧时是更最有本机权条来样根框次此段比没法消点版物状现",
    "用的盖目相看着确称程空窗端符第答类索约级组终经结绝编者能节范行表被要覆览认话请读负败起超路跳载辑输过这进远连那部里重销错闭问限除非须首高（），：",
    "共向式幕横模滚长",
    "头选项",
    "变收",
    "×★☆书加懂找移签",
    "双另右己底标栏自页",
    "处",
    "显示预",
    "启格求码误象",
    "值择置间需",
    "仍然盘",
    "算链",
    "查替",
    "解压",
    "之半执批果校缺验所",
    // 窗口换骨架：工具条提示 · 命令栏 · 左栏 · 状态栏 · 属性框 · 「类型」那一列 · 图片预览（−⬆📁📄🔗：图标换成图标字，不再是文字）。
    "从修图地址始子展属左性指片藏试退配隐他其包夹缩",
    // 文件名搜索那几句新进的字。
    "得词",
    // 搜索结果的状态行与表（「文件清单未建」「清单仅前 N 项」「无匹配」「名称 · 按相关度」）；
    //   `描 洞 秒 隔` 随旧的新鲜度那一行（扫描间隔 N 秒 · 有洞）走了。
    "仅匹单度无未清",
    // 窗口照稿换样子（命令栏 · 左栏 · 状态栏 · 搜索结果）：＋`停 击 情 详 车 达`（「停」「详情」「点击重试」「回车」「已达读取上限」）；
    //   −`← ↑ ● 也 切 家 拿 淡 色 见 走 跑 ＋`（导航键位改由代码拼 · 忙点画成圆 · 家目录改主目录 · 旧的几句长提示删了）。
    "停击情详车达",
    // 删除那一问「不可恢复」。
    "恢",
    // 「进度」表：「开始后不可停止」「计算大小」。
    "止计",
    // 断线条：「离线 · 采样 13:40」「离线 · 只读」。
    "离线采",
    // 编辑页：头条面包屑的 `›` · 「放弃改动并重开」「改动保留」（未保存那颗点是画的圆，不是字）；
    //   −`些 敲`（旧的「关掉就丢掉你敲的那些东西了」换成「不保存 = 丢弃改动」）。
    "›弃放留",
    // 就地操作那一批：＋`三 人 填 照 致`（「要写成 644 或 755 这样的三位数」·「其他人」·「需填数字写法」·「不重名的 n 个照传」·「当前权限不一致」）；
    //   −`八 剩 吗 如 身`（旧的「八进制」「剩下的并行传」「要覆盖吗」「这一问只出现一次 … 不如」「目录本身」那几句换成了简洁写法）。
    "三人填照致",
    // 审图那一轮：改权限「需三位数字 · 例 644」·「进度」表断线时在跑的那几行「等待重连」。
    "例待等",
    // 第 4 批：拖进来时「松开上传 → 目录（n 个文件）」；−`儿 定 尾`（上传 / 存到哪儿那两问换成系统框，「那儿」「确定」「结尾」那几句没了）。
    "松",
    "应方析知",
    // 文案第四段 D 类（壳 / 文件窗口那一面压缩）：＋`于 入 及 均 定 持 支 绪 识 该 身 适 途`（「不适用于所选项」「均认不出名字」「未就绪」「该机」…）；
    //   −`⚠ 事 任 何 准 叫 和 哪 备 好 带 引 忙 掉 正 种 给 许 趟 都 面 ； ？`（句中 ⚠ 出表 · 分号问号出句 · 口语说法随压缩走了）。
    "于入及均定持支绪识该身适途",
    // 文案第五段（改调用点那段 · 壳 / 文件窗口）：＋`见 询`（「原因见它自己的错误输出」「查询 {machine} 的后端失败」）；
    //   −`— 东 太 张 必 西 边 还`（句中破折号出句 · 「太旧」「张」「必须」「两边」「还在」那几句随压缩走了）。
    "见询",
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

/// 照主题的字族去这台机器的字体里找。
pub fn faces_for(theme: &filewin_contract::Theme) -> Faces {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    Faces {
        ui: find_face(&db, &theme.font_base),
        mono: find_face(&db, &theme.font_mono),
    }
}

/// 第一拍：主题字体（有就放链首）· 图标字 · 一份 CJK 字体（链尾兜底）交给 `ctx`。**在创建闭包里调。不复核**（§四）。
pub fn install(ctx: &egui::Context, theme: Option<&filewin_contract::Theme>) -> Attempt {
    let faces = theme.map(faces_for).unwrap_or_default();
    install_with(ctx, &candidates(), &faces)
}

/// 同 [`install`]，只装 CJK 回退与图标字、候选表由调用方给（判据用）。
pub fn install_from(ctx: &egui::Context, candidates: &[String]) -> Attempt {
    install_with(ctx, candidates, &Faces::default())
}

/// 一份字体的数据。
fn data(bytes: Vec<u8>, index: u32) -> Arc<egui::FontData> {
    Arc::new(egui::FontData {
        font: std::borrow::Cow::Owned(bytes),
        index,
        tweak: Default::default(),
    })
}

/// 装字：主题字体放链首（拉丁字由它画，同主界面）；图标字跟在 egui 自带正文字后面；CJK 字体**追加在末尾当兜底**。
pub fn install_with(ctx: &egui::Context, candidates: &[String], faces: &Faces) -> Attempt {
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
        match std::fs::read(path) {
            Ok(bytes) => {
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
            Err(e) => {
                attempt = Some(Attempt::ReadFailed {
                    path: path.clone(),
                    err: e.to_string(),
                });
            }
        }
        break;
    }
    ctx.set_fonts(defs);
    attempt.unwrap_or(Attempt::NoneFound { tried })
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
/// 🔴 刻意不是 `bool`，也不是 `Result<(), ()>` —— 与从前池子里零流量复制那一条的裁决类型
/// 同一个形（那一条已退役，形状留在这里）：**静默退化在类型上就做不到**。
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
#[path = "../../../../tests/frontend/filewin/fonts_tests.rs"]
mod tests;
