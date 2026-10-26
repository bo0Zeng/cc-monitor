//! 一帧怎么画：[`FileWindow::frame_body`] 那一段 —— 画之前先收货（选文件框 · 编辑 · 拖入 · 各趟结局 · 就地那一格）、
//! 那几行提示、每一趟开头那一问、列表与它的胶水。

use super::*;

impl FileWindow {
    /// 🔴**每一帧的正文。** 从 `eframe::App::ui` 里剥出来的，
    /// 剥的理由只有一个：**让它进得了执行链**。
    ///
    /// `eframe::App::ui` 的签名逐字要一个 `&mut eframe::Frame`
    /// （`eframe-0.36.2/src/epi.rs` 的 `trait App`），而那个类型在判据里**造不出来**
    /// ⇒ 不剥出来，这个窗口**每一帧真正画的那段代码一条判据都没有**（判据只在零件上，把零件从这里摘掉也不红）。
    /// 这正是本仓那条「判据不在执行链上就等于不存在」。
    ///
    /// ⇒ 从此 `eframe::App::ui` 只剩一句委派，判据直接喂本函数。
    /// 那一句委派今天住 [`crate::workspace::Workspace`]（它才是 `eframe::App`）：
    /// 每个标签页的正文就是这里，外面只多了标签栏 · 双栏 · 预览那一层。
    pub fn frame_body(&mut self, ui: &mut egui::Ui) {
        self.font_frame(ui);
        if self.edit_frame(ui) {
            return;
        }
        if self.showing_picture() {
            let ctx = ui.ctx().clone();
            self.settle_pick_with(Some(ctx));
            self.view_page(ui);
            return;
        }
        // 工具条（后退 · 前进 · 上一级 · 地址栏 · 搜索格）与命令栏不在这里画：
        //   它们作用于焦点那一栏，由最外一层画在窗口顶上（[`Self::toolbar_ui`] · [`Self::command_bar_ui`]，住 `chrome.rs`）。
        // 🔴**键盘** —— 在画列表之前接：这一帧按的键，这一帧的列表就要画出结果
        //    （光标那一圈、滚进视野）。能不能接由 `keys_blocked` 那四道闸说了算。
        //    ⚠ 滚进视野要**上一帧**真物化的那一段 ⇒ 在 `tally` 被清零之前取。
        let (prev_first, prev_last) = (self.tally.first_row, self.tally.last_row);
        {
            let ctx = ui.ctx().clone();
            self.apply_keys(&ctx);
        }
        self.drop_zone_ui(ui);
        self.notices_ui(ui);
        self.search_status_frame(ui);
        self.boards_frame(ui);
        self.settle_frame(ui);
        // 🔴搜索框里有字 ⇒ 画命中，否则画当前目录。**二选一，不并排** ——
        //    并排会让「你现在看的是哪一摞」变成一个要靠标题猜的问题。
        if self.showing_hits() {
            self.hits_frame(ui);
        } else if self.showing_grep() {
            self.grep_frame(ui);
        } else {
            self.listing_frame(ui, prev_first, prev_last);
        }
        self.row_glue(ui);
    }

    /// 字体：第一帧复核；出了问题那一条一直摆着。
    pub(super) fn font_frame(&mut self, ui: &mut egui::Ui) {
        // 🔴 **第一帧**才复核得了字体 —— 之前碰 `fonts_mut` 会 panic，
        //    而本仓 release 是 `panic = "abort"`（理由逐条住 `fonts.rs §四`）。
        if self.font.settle(ui.ctx()) {
            *FONT_VERDICT.lock().unwrap() = Some(self.font.clone());
        }
        // 字体出问题就**一直**摆在这儿，不自动消失：
        // 它是一个「到你改掉为止都成立的状态」，不是一次性事件
        // （同 `INVARIANTS §12` 对「设置没生效」那条的判法）。
        if let Some(note) = self.font.notice() {
            crate::kit::banner(ui, crate::kit::Tone::Warn, &note, &[]);
        }
    }

    /// 编辑页那一形：正文是编辑面 ⇒ 画它、交 `true`（这一帧别的都不画）。
    pub(super) fn edit_frame(&mut self, ui: &mut egui::Ui) -> bool {
        // 编辑页（或单独建的视图正在编辑一份）：正文是编辑面，不是列表。
        if self.edit_tab || self.editing.is_some() || self.edits.opening().is_some() {
            self.settle_opened_edits();
            self.settle_saved_edits();
            {
                let ctx = ui.ctx().clone();
                self.settle_pick_with(Some(ctx));
            }
            self.edit_page(ui);
            return true;
        }
        false
    }

    pub(super) fn drop_zone_ui(&mut self, ui: &mut egui::Ui) {
        // 从桌面拖着文件经过 ⇒ 列表区一层虚线框「松开上传 → 目录（n 个文件）」；松手那一下由 `take_drops` 接。
        let hovering = ui.input(|i| i.raw.hovered_files.len());
        if hovering > 0 && self.focused && !self.modal_up() {
            let dir = crate::source::remote_basename(&self.cwd).to_string();
            crate::kit::drop_zone(
                ui.ctx(),
                ui.available_rect_before_wrap(),
                &copy_text(
                    "rsFilewinShell.drop.release",
                    &[("dir", &dir), ("n", &hovering.to_string())],
                ),
            );
        }
    }

    /// 列表上面那几条：开终端说的话 · 目录打不开 · 截断。
    pub(super) fn notices_ui(&mut self, ui: &mut egui::Ui) {
        // 一次性的那几句（键位做不成的原因 · 跳到隐藏文件 · …）不画在这里：由窗口那一级收成右下角的回执（`Workspace::frame`）。
        // 开终端那一下说的话 —— 摆着不走（到你换台机器 / 换个系统为止都成立的状态）⇒ 一条警告条。
        if let Some(said) = self.term_notice() {
            crate::kit::banner(ui, crate::kit::Tone::Warn, &said, &[]);
        }
        if let Some(said) = self.plan_notice() {
            crate::kit::banner(ui, crate::kit::Tone::Warn, &said, &[]);
        }
        // 目录打不开：哪一种（后端的码）说一句 ＋ 出路「回上一级」「回主目录」；系统原话进「复制详情」。
        let open_fail = self.listing.open_fail.lock().unwrap().clone();
        if let Some((kind, raw)) = open_fail {
            let name = crate::source::remote_basename(&self.cwd).to_string();
            let key_text = match kind {
                crate::source::OpenFail::NotFound => {
                    copy_text("rsFilewinShell.open.notFound", &[("name", &name)])
                }
                crate::source::OpenFail::Denied => {
                    copy_text("rsFilewinShell.open.denied", &[("name", &name)])
                }
                crate::source::OpenFail::NotDir => {
                    copy_text("rsFilewinShell.open.notDir", &[("name", &name)])
                }
                crate::source::OpenFail::Other => {
                    copy_text("rsFilewinShell.open.other", &[("name", &name)])
                }
            };
            // 〔复制详情条带 §5.3〕同一颗按钮（错误条最右）：复制出去的首行是屏上那句，下面是路径与原话。
            let lines = copy_core::detail::Detail::new()
                .item(copy_core::detail::Label::Path, &self.cwd)
                .item(copy_core::detail::Label::Raw, &raw)
                .render();
            let body = format!("{key_text}\n{lines}");
            let detail_id = ui.id().with("open-fail-detail");
            match crate::kit::banner_with_detail(
                ui,
                crate::kit::Tone::Error,
                &key_text,
                &[
                    copy_text("rsFilewinShell.open.up", &[]),
                    copy_text("rsFilewinShell.open.home", &[]),
                ],
                Some((detail_id, &body)),
            ) {
                Some(0) => self.navigate_up(),
                Some(1) => self.want_home = true,
                _ => {}
            }
        } else if let Some(f) = self.listing.error.lock().unwrap().clone() {
            let id = ui.id().with("list-fail-detail");
            crate::kit::failure_banner(ui, crate::kit::Tone::Error, &f.said, &[], &f.detail, id);
        }
        // 🔴 截断也要出声 —— 「这个目录里就这么多」与「后端只给了前 N 条」在屏幕上长得一样。
        if self.listing.truncated.load(Ordering::SeqCst) && !self.showing_hits() {
            let total = self.listing.total.load(Ordering::SeqCst);
            let said = copy_text(
                "rsFilewinShell.frame.truncated",
                &[
                    ("n", &(crate::source::LS_LIMIT).to_string()),
                    ("total", &total.to_string()),
                ],
            );
            if crate::kit::banner(
                ui,
                crate::kit::Tone::Warn,
                &said,
                &[copy_text("rsFilewinShell.frame.searchByName", &[])],
            )
            .is_some()
            {
                self.search_content = false;
                ui.ctx()
                    .memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_BOX_ID)));
            }
        }
    }

    /// 画在列表之前的那几问（属性 · 每一趟开头那一问 · 写类确认）。
    pub(super) fn boards_frame(&mut self, ui: &mut egui::Ui) {
        // 「属性」那一问。
        self.props_ui(ui);
        // 后台那几趟（上传 · 下载 · 复制 · 解压 · 算大小 · 删除）的进度、停与结局不在这里画：
        //   它们是窗口底部「进度」表里的一行（`crate::progress`，由 `Workspace` 画）。这里只剩每一趟开头那一问（模态，画在列表之前）。
        for j in self.my_trips() {
            j.trip.ask_ui(ui);
        }
        self.cross_ui(ui);
        self.copy_ui(ui);
        // 🔴`§4.6.4` 那一摞：一次问完的确认框 ／ 结果（「被围栏挡住那几句话」那一段删了）。
        //    同样模态、同样画在列表之前。
        self.write_board.ui(ui);
        self.write_ui(ui);
    }

    /// 画列表之前先消化到货（选文件框 · 编辑 · 拖入 · 各趟结局 · 就地那一格），再把这一帧的计数清零。
    pub(super) fn settle_frame(&mut self, ui: &mut egui::Ui) {
        // 系统选文件框 / 存盘框选完了 ⇒ 上传走拖入那一条（先一次问完同名，再并行传）· 下载起那一趟。
        {
            let ctx = ui.ctx().clone();
            self.settle_pick_with(Some(ctx));
        }
        // 🔴编辑那一摞：**先消化到货，再画** ——
        //    反了的话这一帧画的是上一帧的状态（读完了却还显示「正在读」）。
        self.settle_opened_edits();
        self.settle_saved_edits();
        self.take_drops(&ui.ctx().clone());
        self.settle_trips();
        self.settle_finished_writes();
        self.settle_inline();
        // 每帧从零数起 —— 这两个数是「这一帧物化了多少行」，不是累计。
        self.tally = RenderTally::default();
        self.hits_tally = HitTally::default();
        self.grep_tally = GrepTally::default();
    }

    /// 当前目录那一摞：表头 · 加载中 / 空目录 / 列表（含就地那一格）。
    pub(super) fn listing_frame(&mut self, ui: &mut egui::Ui, prev_first: usize, prev_last: usize) {
        let jump = self.listing_jump(ui, prev_first, prev_last);
        let want = self.reveal.as_ref().map(|r| r.name.clone());
        // 计划反查：目录落在某一片的仓库里 ⇒ 多一列「格」（不在 ⇒ 这一列宽 0，什么都不多）。
        let plan = self.listing.plan.lock().unwrap().clone();
        self.cols.cell = if plan.is_some() {
            crate::rows::CELL_COL
        } else {
            0.0
        };
        // 表头：点一列排序（再点反向），拖分隔线改列宽。
        if let Some(by) = crate::rows::show_header(ui, &mut self.cols, self.sort) {
            self.click_header(by);
        }
        let rows = self.listing.rows.lock().unwrap();
        let loading = self.listing.is_loading();
        let failed = self.listing.error.lock().unwrap().is_some();
        // 就地新建那一格（列表里冒出来的那一行）：空目录里也照样画列表。
        let new_here = self
            .write_prompt
            .as_ref()
            .is_some_and(|p| matches!(p.kind, PromptKind::Mkdir | PromptKind::NewFile));
        if rows.is_empty() && loading && !new_here {
            // 加载：300 ms 内什么都不画，之后骨架行。
            let since = *self
                .loading_since
                .get_or_insert_with(|| ui.input(|i| i.time));
            if ui.input(|i| i.time) - since > 0.3 {
                crate::kit::skeleton_rows(ui, 5);
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
        } else if rows.is_empty()
            && !failed
            && !new_here
            && self.listing.hidden.lock().unwrap().is_empty()
        {
            drop(rows);
            self.empty_dir_ui(ui);
        } else {
            self.loading_since = None;
            let unreadable = self.listing.unreadable.load(Ordering::SeqCst);
            let tail = (unreadable > 0).then(|| {
                copy_text(
                    "rsFilewinShell.frame.unreadable",
                    &[("n", &unreadable.to_string())],
                )
            });
            // 就地那一格：新建 ⇒ 文件夹那一段之后插一行（名字缺省、选中）；改名 ⇒ 那一行的名字格换成输入框。
            let (mut text, shown, at) =
                inline_rows(&rows, self.write_prompt.as_ref().filter(|p| p.is_inline()));
            let fail = self.prompt_failure();
            let blur_submits = self.inline_failed_text.as_deref() != Some(text.as_str());
            let mut cell = at.map(|row| crate::rows::InlineCell {
                row,
                text: &mut text,
                error: fail.as_ref().map(|f| f.said.as_str()),
                detail: fail.as_ref().map_or("", |f| f.detail.as_str()),
                blur_submits,
                select: self.inline_select.take(),
                busy: self.inline_busy,
                outcome: None,
            });
            crate::rows::show_file_rows_with_tail(
                ui,
                shown.as_deref().unwrap_or(&rows),
                &mut self.tally,
                jump,
                want.as_deref(),
                Some(&self.selection),
                &self.cols,
                tail.as_deref(),
                cell.as_mut(),
                plan.as_ref(),
            );
            let outcome = cell.and_then(|c| c.outcome);
            drop(rows);
            // 插进来的那一行不是目录里的一项 ⇒ 点到的下标挪回目录那一摞（点在那一行上 ＝ 没点）。
            if let (Some(_), Some(i)) = (&shown, at) {
                tally_skip_inserted(&mut self.tally, i);
            }
            if let Some(p) = self.write_prompt.as_mut().filter(|p| p.is_inline()) {
                p.text = text;
            }
            match outcome {
                Some(true) => {
                    let ctx = ui.ctx().clone();
                    self.commit_inline(Some(ctx));
                }
                Some(false) => self.cancel_write(),
                None => {}
            }
        }
    }

    /// 这一帧要滚到哪：reveal 那一下优先，否则键盘挪出视野的光标。
    pub(super) fn listing_jump(
        &mut self,
        ui: &mut egui::Ui,
        prev_first: usize,
        prev_last: usize,
    ) -> Option<f32> {
        // 🔴reveal 的两半在这里落地：**算**出偏移（只算一次）＋ 高亮那个名字。
        //    ⚠ 偏移是算的不是找的 —— 那条纪律（`show_rows` 才是主语）。
        // ⚠ **先问 reveal（它自己拿锁），再拿锁画** —— 顺序反了就要克隆整摞行。
        let pitch = crate::rows::row_pitch(ui);
        let jump = match self.take_reveal_offset(pitch) {
            Some(Ok(y)) => Some(y),
            Some(Err(why)) => {
                *self.listing.error.lock().unwrap() = Some(why.into());
                None
            }
            None => None,
        };
        // 键盘挪了光标 ⇒ 不在视野里才滚（reveal 那一下优先：它也是「只滚一次」）。
        let key_jump = self
            .key_scroll
            .take()
            .and_then(|i| select::scroll_for(i, prev_first, prev_last, pitch));
        jump.or(key_jump)
    }

    /// 空目录：一句 ＋「上传」「新建」。
    pub(super) fn empty_dir_ui(&mut self, ui: &mut egui::Ui) {
        self.loading_since = None;
        match crate::kit::empty_state(
            ui,
            egui_phosphor::regular::FOLDER_SIMPLE,
            &copy_text("rsFilewinShell.frame.empty", &[]),
            &[
                format!(
                    "{} {}",
                    egui_phosphor::regular::UPLOAD_SIMPLE,
                    copy_text("rsFilewinShell.frame.emptyUpload", &[])
                ),
                format!(
                    "{} {}",
                    egui_phosphor::regular::PLUS,
                    copy_text("rsFilewinChrome.command.new", &[])
                ),
            ],
        ) {
            Some(0) => {
                let ctx = ui.ctx().clone();
                self.run_command(crate::chrome::Cmd::Upload, Some(ctx));
            }
            Some(1) => {
                self.begin_mkdir();
            }
            _ => {}
        }
    }

    /// 画完之后那几条胶水：点了命中就跳 · 单击 / 右键 / 拖起一行 · 菜单。
    pub(super) fn row_glue(&mut self, ui: &mut egui::Ui) {
        // ⚠ 这三条只对**目录列表**那一摞有意义（下标索引的是 `listing.rows`）。
        //   命中那一摞交不出下标 —— 第四刀靠的是「那个函数不画可点控件」这条纪律，
        //   第五刀换成了**类型**（上面那一段）。
        // 按内容搜那一摞的胶水：点了第 i 条 ⇒ 跳过去（下标只指命中那一摞，与下面几条胶水不相干）。
        if let Some(i) = self.grep_tally.jump.take() {
            self.jump_to_grep_hit(i);
        }
        // 文件名命中那一摞：点了第 i 条 ⇒ 跳过去。
        if let Some(i) = self.hits_tally.jump.take() {
            self.jump_to_find_hit(i);
        }
        self.apply_click();
        // 🔴单击改选中 · 右键摆菜单。然后画菜单（它在最上层）。行上没有按钮：动作全走菜单与键盘（同一个 `perform`）。
        self.apply_pick_click();
        self.apply_band();
        // 第八条胶水：拖起一行。
        self.apply_drag_start();
        let at = ui
            .ctx()
            .input(|i| i.pointer.interact_pos())
            .unwrap_or_default();
        self.apply_menu_click(at);
        self.menu_ui(ui);
    }
}

/// 就地那一格要画成什么：输入框里的字 · 插了新行之后的那一摞（新建才有）· 那一格在第几行。
/// 新建 ⇒ 文件夹那一段之后插一行；改名 ⇒ 就是那一行。
pub(super) fn inline_rows(
    rows: &[crate::source::Listed],
    prompt: Option<&WritePrompt>,
) -> (String, Option<Vec<crate::source::Listed>>, Option<usize>) {
    let mut text = String::new();
    let mut shown: Option<Vec<crate::source::Listed>> = None;
    let mut at: Option<usize> = None;
    if let Some(p) = prompt {
        text = p.text.clone();
        match &p.kind {
            PromptKind::Rename { from, .. } => {
                at = rows.iter().position(|r| &r.path == from);
            }
            kind => {
                let i = rows.iter().rposition(|r| r.is_dir).map_or(0, |i| i + 1);
                let mut v = rows.to_vec();
                v.insert(
                    i,
                    crate::source::Listed::plain(crate::source::Row {
                        name: text.clone(),
                        path: crate::source::child_of(&p.dir, &text),
                        is_dir: matches!(kind, PromptKind::Mkdir),
                        size: 0,
                        lossy_name: false,
                    }),
                );
                shown = Some(v);
                at = Some(i);
            }
        }
    }
    (text, shown, at)
}

/// 列表里插了一行（就地新建）：点到的下标挪回目录那一摞（点在插进来的那一行上 ＝ 没点）。
pub(super) fn tally_skip_inserted(t: &mut RenderTally, i: usize) {
    let back = |x: usize| match x.cmp(&i) {
        std::cmp::Ordering::Less => Some(x),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(x - 1),
    };
    t.clicked = t.clicked.and_then(back);
    t.menu_clicked = t.menu_clicked.and_then(back);
    t.drag_started = t.drag_started.and_then(back);
    t.picked_click = t.picked_click.and_then(|(x, m)| back(x).map(|x| (x, m)));
}
