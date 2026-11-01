//! 键盘 · 多选 · 右键菜单 —— 接到窗口上的那几跳。
//!
//! 纯的那一份（选中态 · 键位翻译 · 「能做什么」那张表）住 `select.rs`；这里只有胶水，
//! 而每一条胶水都是一个具名方法（同 `apply_click` 那条理由：写在 `frame_body` 里的话，
//! 两头各有判据、中间这一跳谁都没在看）。

use super::*;

impl FileWindow {
    /// 键盘 / 菜单那一下做不了时说的那句话。
    pub fn key_notice(&self) -> Option<&str> {
        self.key_notice.as_deref()
    }

    /// 键盘这一帧该不该归列表。**五道闸**，任一成立就不接：
    ///
    /// 0. 这个目录视图不是焦点那一个（双栏时另一栏、后台标签页）—— 不闸的话按一下 Delete 两栏各删一次；
    /// 1. 有模态框摆着（[`Self::modal_up`]）—— 键是给那个框的；
    /// 2. 右键菜单摆着 —— Esc / 点别处先把它收掉；
    /// 3. 画的是搜索命中那一摞 —— 那一摞交不出下标（`rows::HitTally` 头注那条），
    ///    按 Delete 删的会是**另一摞**里同一个下标的文件；
    /// 4. 有控件拿着键盘焦点（搜索框里正在打字、一颗按钮刚被 Tab 到）—— 字是给它的。
    ///    ⚠ 点一下列表里的行，焦点就交出去了（egui 点别处即交；行不可聚焦），键盘回到列表。
    /// 导航键（后退前进 · 刷新 · 地址栏 · 搜索框）那道闸：模态框 · 菜单 · 文字框拿着键盘时不接；
    /// 与列表那道不同，画着搜索命中那一摞时照样接（导航不碰任何一摞的下标）。
    pub fn nav_keys_blocked(&self, ctx: &egui::Context) -> bool {
        !self.focused || self.modal_up() || self.menu.is_some() || ctx.egui_wants_keyboard_input()
    }

    /// 在列表上方说一句（键盘那一路同一个出口：下一次按键就清）。
    pub fn set_key_notice(&mut self, n: String) {
        self.key_notice = Some(n);
    }

    pub fn keys_blocked(&self, ctx: &egui::Context) -> bool {
        !self.focused
            || self.modal_up()
            || self.menu.is_some()
            || self.showing_hits()
            || ctx.egui_wants_keyboard_input()
    }

    /// 🔴 **胶水**：这一帧 egui 收到的按键 → 意图 → 一件一件做。回值 = 做了几件。
    pub fn apply_keys(&mut self, ctx: &egui::Context) -> usize {
        if self.keys_blocked(ctx) {
            return 0;
        }
        let (events, now) = ctx.input(|i| (i.events.clone(), i.time));
        let its = select::intents(&events);
        let n = its.len();
        for it in its {
            self.apply_intent(it, now, Some(ctx.clone()));
        }
        n
    }

    /// 做一件意图。回值 = 真的做成了（做不了的那几形**已出声**，见 [`Self::key_notice`]）。
    ///
    /// `now` 是 egui 的输入时钟（秒），只给打字跳转那一格比「隔了多久」用。
    pub fn apply_intent(&mut self, it: Intent, now: f64, ctx: Option<egui::Context>) -> bool {
        self.key_notice = None;
        match it {
            Intent::Step { by, extend } => self.step(by, extend),
            Intent::Edge { end, extend } => {
                let len = self.listing.rows.lock().unwrap().len();
                self.go_to(select::edge_target(len, end), extend)
            }
            Intent::Parent => {
                let before = self.cwd.clone();
                self.navigate_up();
                self.cwd != before
            }
            Intent::SelectAll => {
                let rows = self.listing.rows.lock().unwrap();
                self.selection.select_all(&rows);
                !rows.is_empty()
            }
            Intent::Open => self.open_picked(ctx),
            Intent::Delete => self.perform(Action::Delete, ctx),
            Intent::Rename => self.perform(Action::Rename, ctx),
            Intent::Peek => {
                self.want_peek = true;
                true
            }
            Intent::NewFolder => self.begin_mkdir(),
            Intent::Download => self.perform(Action::Download, ctx),
            Intent::Upload => self.start_pick(crate::picker::Purpose::Upload, ctx),
            Intent::CopyPath => {
                let paths: Vec<String> = self
                    .picked_rows()
                    .into_iter()
                    .map(|r| r.path.clone())
                    .collect();
                if paths.is_empty() {
                    self.key_notice = Some(copy_text("rsFilewinSelect.refusal.none", &[]));
                    return false;
                }
                if let Some(c) = &ctx {
                    c.copy_text(paths.join("\n"));
                }
                self.receipt = Some(copy_text("rsFilewinShell.receipt.pathCopied", &[]));
                true
            }
            Intent::Clear => {
                let had = !self.selection.is_empty();
                self.selection.clear();
                had
            }
            Intent::Type(t) => {
                let prefix = self.type_ahead.feed(now, &t).to_string();
                let hit = {
                    let rows = self.listing.rows.lock().unwrap();
                    select::jump_target(&rows, &prefix)
                        .map(|i| (i, self.selection.move_to(&rows, i, false)))
                };
                match hit {
                    Some((i, _)) => {
                        self.key_scroll = Some(i);
                        true
                    }
                    None => {
                        self.key_notice = Some(copy_text(
                            "rsFilewinShell.intent.noPrefix",
                            &[("prefix", &prefix.to_string())],
                        ));
                        false
                    }
                }
            }
        }
    }

    /// ↑↓：光标挪一步，并记下「这一帧要滚进视野」。
    pub(super) fn step(&mut self, by: isize, extend: bool) -> bool {
        let target = {
            let rows = self.listing.rows.lock().unwrap();
            select::step_target(rows.len(), self.selection.cursor_index(&rows), by)
        };
        self.go_to(target, extend)
    }

    /// 光标落到第 `target` 行（`None` = 空列表，什么都不做）。
    pub(super) fn go_to(&mut self, target: Option<usize>, extend: bool) -> bool {
        if let Some(t) = target {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.move_to(&rows, t, extend);
        }
        self.key_scroll = target;
        target.is_some()
    }

    /// 回车：选中的**恰好一项** ⇒ 目录进去、文件编辑。
    ///
    /// ⚠ 文件那一支**直接**走 [`Self::begin_edit`]，不先问 [`select::actions_for`]：
    /// 那张表对「超编辑上限」只会说「没有编辑这一项」，而 `begin_edit` 会说**为什么**
    /// （多了多少字节）—— 两者判的是同一个函数（`editor::is_editable` 就是
    /// `why_not_editable` 的 `is_none()`），所以准不准一致，只是这一支说得更具体。
    pub(super) fn open_picked(&mut self, ctx: Option<egui::Context>) -> bool {
        let one = {
            let rows = self.listing.rows.lock().unwrap();
            match self.selection.picked_indices(&rows).as_slice() {
                [i] => Some((*i, rows[*i].opens_as_dir())),
                _ => None,
            }
        };
        match one {
            Some((_, true)) => self.perform(Action::Open, ctx),
            Some((i, false)) => self.open_file(i, ctx),
            None => {
                self.key_notice = Some(select::refusal(Action::Open, self.selection.len()));
                false
            }
        }
    }

    /// 🔴 **对选中那几项做一件事 —— 菜单与键盘的唯一执行口。**
    ///
    /// 先**再问一次** [`select::actions_for`]（菜单是开菜单那一刻的快照，键盘压根没问过）：
    /// 不在表里 ⇒ 出声、不做。在表里 ⇒ 落回行上那几颗按钮**已经在走**的那几个 `begin_*`
    /// —— 一条新写路都没长（一次问完 · 只经通道说 `call` 全在那几个函数后面）。
    pub fn perform(&mut self, a: Action, ctx: Option<egui::Context>) -> bool {
        let (idx, allowed) = {
            let rows = self.listing.rows.lock().unwrap();
            let idx = self.selection.picked_indices(&rows);
            let picked: Vec<&crate::source::Listed> = idx.iter().map(|&i| &rows[i]).collect();
            let allowed = select::actions_for(&picked);
            (idx, allowed)
        };
        if !allowed.contains(&a) {
            self.key_notice = Some(select::refusal(a, idx.len()));
            return false;
        }
        if let Some(why) = self.refused_here(a) {
            self.key_notice = Some(why);
            return false;
        }
        match (a, idx.as_slice()) {
            (Action::Delete, _) => self.delete_picked(&idx, ctx),
            (Action::Open, [i]) => self.activate(*i),
            (Action::Edit, [i]) => self.begin_edit(*i, ctx),
            (Action::View, [i]) => self.begin_view(*i),
            (Action::Copy, [i]) => self.begin_copy(*i),
            (Action::Download, [i]) => self.begin_pull(*i),
            // 多项：选一个本机目录，一组一条 `transfer-download`。
            (Action::Download, _) => self.begin_pull_many(&idx),
            // 两栏的事归窗口那一级（`Workspace::copy_to_other`）。
            (Action::Across, _) => {
                self.across_wanted = true;
                true
            }
            // 一项或多项。
            (Action::Size, _) => self.start_sizes(&idx, ctx),
            // 恰好一份文件。
            (Action::Extract, [i]) => self.start_extract(*i, ctx),
            // 一项或多项（目录整棵）。
            (Action::CrossCopy, _) => self.begin_cross_rows(&idx),
            (Action::Rename, [i]) => self.begin_rename(*i),
            // 一项或多项：同一个框（多项时框上说件数）。
            (Action::Chmod, _) => self.begin_chmod_rows(&idx),
            (Action::Properties, [i]) => self.begin_props(*i, ctx),
            // `actions_for` 只对恰好一项给出单项动作 ⇒ 这一支走不到；
            // 真走到了也**出声**，不静默。
            _ => {
                self.key_notice = Some(select::refusal(a, idx.len()));
                false
            }
        }
    }

    /// 删掉第 `idx` 那几行 —— **一摞**，走 [`Self::start_writes`]（⇒ `run_writes`：
    /// 一次问完 → 串行做）。「批量底层已做好」说的就是那个函数。
    ///
    /// 🔴 每一行照旧过 [`Self::writable_row`] 那道第二闸；有一行过不去 ⇒ **整摞不起**并出声
    /// （起一摞「删 3 项」却悄悄只删 2 项，正是「选中态 == 批量那一摞」这条相等的反面）。
    pub(super) fn delete_picked(&mut self, idx: &[usize], ctx: Option<egui::Context>) -> bool {
        let mut ops = Vec::with_capacity(idx.len());
        for &i in idx {
            let Some(row) = self.writable_row(i) else {
                self.key_notice = Some(select::refusal(Action::Delete, idx.len()));
                return false;
            };
            self.describe_for_delete(&row);
            ops.push(crate::writeops::delete_op(&row));
        }
        self.start_writes(ops, ctx)
    }

    /// 删除那一问里这一行右端那一格：文件夹写「文件夹」（不写项数，前一路定的）、别的写大小；图标同列表。
    pub(super) fn describe_for_delete(&self, row: &crate::source::Listed) {
        let meta = if row.is_dir {
            copy_text("rsFilewinWriteops.delete.rowDir", &[])
        } else {
            row.size_text.clone().unwrap_or_default()
        };
        self.write_board
            .describe(&row.path, crate::kind::icon(row.class), meta);
    }

    /// 🔴 **胶水**：列表说「第 `i` 行被单击了（带着这几个修饰键）」→ 选中态跟着变。
    ///
    /// ⚠ 「点行这一下把键盘交回列表」**不在这儿做**：egui 自己在「点了别处」时让拿着焦点的
    /// 控件交出焦点（输入框与按钮都是；`SurrenderFocusOn::Clicks` 是缺省），而点一下可聚焦的
    /// 控件**并不**给它焦点（按钮要 Tab 过去才拿得到）⇒ 点完行之后没人拿着焦点，键盘自然归列表。
    /// 〔死值验现打〕第一版这里还有一句「显式清焦点」、行上还把 `Sense::click()` 换成了不可聚焦的那一档，
    /// 理由都是「点行会把焦点给行」—— **两刀各摘一次，一条判据都不红**，读 egui 源码核实那个前提不成立
    /// ⇒ 两处都撤回了。承重的是 egui 那条缺省行为，由
    /// `clicking_a_row_takes_the_keyboard_back_from_a_focused_button` 钉着（egui 换缺省那天它红）。
    pub fn apply_pick_click(&mut self) -> bool {
        let Some((i, mods)) = self.tally.picked_click else {
            return false;
        };
        {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.click(&rows, i, mods);
        }
        self.key_notice = None;
        true
    }

    /// **胶水**：列表说「在框选」→ 选中跟着框变（按下那一刻的选中记下：Ctrl 框加进它、Esc 回到它）；松手 ⇒ 收。
    pub fn apply_band(&mut self) -> bool {
        if self.tally.band_cancel {
            if let Some(b) = self.band_base.take() {
                self.selection = b;
            }
            return true;
        }
        let Some((span, add)) = self.tally.band else {
            return false;
        };
        let base = self
            .band_base
            .get_or_insert_with(|| self.selection.clone())
            .clone();
        {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.band(&rows, &base, span, add);
        }
        if self.tally.band_end {
            self.band_base = None;
        }
        self.key_notice = None;
        true
    }

    /// **胶水**：列表说「第 `i` 行被拖起了」→ 拖的是哪一摞（它在选中里 ⇒ 整摞；
    /// 不在 ⇒ 改成只选它 —— 与右键同一个手感，[`select::Selection::pick_for_menu`]）→ 记下「在拖」。
    /// 松手落在另一栏 ⇒ 工作区走「复制到另一栏」那一个入口（不另起一条复制路）。
    pub fn apply_drag_start(&mut self) -> bool {
        let Some(i) = self.tally.drag_started else {
            return false;
        };
        {
            let rows = self.listing.rows.lock().unwrap();
            self.selection.pick_for_menu(&rows, i);
        }
        self.dragging = true;
        true
    }

    /// 🔴 **胶水**：列表说「第 `i` 行被右键点了」→ 选中态按右键的手感变 → 摆出菜单；
    /// 说「空白处被右键点了」→ 清掉选中 → 摆出对这个目录的菜单（新建 · 上传 · 终端）。
    ///
    /// `at` = 菜单摆在哪儿（指针那一刻的位置；判据直接喂）。
    pub fn apply_menu_click(&mut self, at: egui::Pos2) -> bool {
        let row = self.tally.menu_clicked;
        if row.is_none() && !self.tally.menu_blank {
            return false;
        }
        if self.modal_up() {
            return false;
        }
        let (kind, why, actions, n) = {
            let rows = self.listing.rows.lock().unwrap();
            match row {
                Some(i) => self.selection.pick_for_menu(&rows, i),
                None => self.selection.clear_picked(),
            }
            let idx = self.selection.picked_indices(&rows);
            let picked: Vec<&crate::source::Listed> = idx.iter().map(|&i| &rows[i]).collect();
            let kind = select::picked_kind(&picked);
            // 表里有、这一份做不了的那几件 ⇒ 那一句（菜单上灰着说）。
            let why: Vec<(Action, String)> = select::menu_groups(kind, false)
                .into_iter()
                .flatten()
                .filter_map(|it| match it {
                    Item::Act(a) => select::blocked(a, &picked).map(|w| (a, w)),
                    _ => None,
                })
                .collect();
            (kind, why, select::actions_for(&picked), idx.len())
        };
        self.key_notice = None;
        // 归属问到了、单选的那一份归某一格 ⇒ 表里「带计划」那一组。
        let plan = self
            .picked_owner()
            .map(|(_, o)| crate::plan::menu_label(&o));
        let dual = self.dual;
        let groups = select::menu_groups(kind, plan.is_some())
            .into_iter()
            .map(|g| {
                g.into_iter()
                    // 「复制到另一栏」只在两栏时有。
                    .filter(|it| dual || *it != Item::Act(Action::Across))
                    .map(|it| {
                        let w = why
                            .iter()
                            .find(|(a, _)| it == Item::Act(*a))
                            .map(|(_, w)| w.clone());
                        (it, w)
                    })
                    .collect()
            })
            .collect();
        self.menu = Some(MenuAt {
            at,
            groups,
            actions,
            n,
            plan,
            serial: MENU_SERIAL.fetch_add(1, Ordering::SeqCst) + 1,
        });
        true
    }

    /// 菜单那一层的 egui id（判据按它找菜单画在哪一块）。
    pub fn menu_id(serial: u64) -> egui::Id {
        egui::Id::new(("filewin-row-menu", serial))
    }

    /// 菜单上的一项此刻做不了的那一句：这一份做不了（开菜单时算好的）· 这台做不到 · 断着线。
    pub(super) fn item_refusal(&self, it: Item, row_why: &Option<String>) -> Option<String> {
        use crate::chrome::Cmd;
        if row_why.is_some() {
            return row_why.clone();
        }
        match it {
            Item::Act(a) => self.refused_here(a),
            Item::InPlan | Item::SelectAll => None,
            Item::Mkdir => self.offline_cmd(Cmd::Mkdir),
            Item::NewFile => self.offline_cmd(Cmd::NewFile),
            Item::Upload => self.offline_cmd(Cmd::Upload),
            Item::Term => self.offline_cmd(Cmd::Term),
        }
    }

    /// 画右键菜单（经 kit 那一种菜单项）；点了哪一项就交出去。点别处 / Esc ⇒ 收掉。
    pub(super) fn menu_ui(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.menu.clone() else {
            return;
        };
        let ctx = ui.ctx().clone();
        let rows: Vec<Vec<crate::kit::MenuRow>> = m
            .groups
            .iter()
            .map(|g| {
                g.iter()
                    .map(|(it, why)| {
                        crate::kit::MenuRow::new(it.icon(), it.label(m.n, m.plan.as_deref()))
                            .key(it.shortcut().map(|k| ctx.format_shortcut(&k)))
                            .danger(it.danger())
                            .blocked(self.item_refusal(*it, why))
                    })
                    .collect()
            })
            .collect();
        let (mut entries, at) = crate::kit::grouped(rows);
        if entries.is_empty() {
            entries.push(crate::kit::Entry::Note(MENU_EMPTY.to_string()));
        }
        let mut open = true;
        let hit = crate::kit::context_menu(ui, Self::menu_id(m.serial), m.at, &mut open, &entries)
            .and_then(|k| at.get(k).copied())
            .and_then(|(g, i)| m.groups.get(g).and_then(|g| g.get(i)))
            .map(|(it, _)| *it);
        if hit.is_some() || !open {
            self.menu = None;
        }
        match hit {
            Some(Item::Act(a)) => {
                self.perform(a, Some(ctx));
            }
            Some(Item::InPlan) => {
                self.open_in_plan(Some(ctx));
            }
            Some(Item::Mkdir) => self.run_command(crate::chrome::Cmd::Mkdir, Some(ctx)),
            Some(Item::NewFile) => self.run_command(crate::chrome::Cmd::NewFile, Some(ctx)),
            Some(Item::Upload) => self.run_command(crate::chrome::Cmd::Upload, Some(ctx)),
            Some(Item::Term) => self.run_command(crate::chrome::Cmd::Term, Some(ctx)),
            Some(Item::SelectAll) => {
                let rows = self.listing.rows.lock().unwrap();
                self.selection.select_all(&rows);
            }
            None => {}
        }
    }
}

/// 开过几次右键菜单 —— 每次开菜单换一个 egui id（理由住 [`MenuAt::serial`]）。
///
/// 🔴**进程级，不是每个目录视图一个**：双栏时两栏同时画在一个 egui 上下文里，
/// 各数各的话两栏的第一个菜单撞同一个 id ⇒ 在另一栏右键那一下「只关不开」（与序号本来要防的同一形）。
pub(super) static MENU_SERIAL: AtomicU64 = AtomicU64::new(0);
