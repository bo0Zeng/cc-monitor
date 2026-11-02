//! 查找 · 按内容搜接到窗口上：发那一问 · 命中那一摞（排序 · 跳到那一行）· 搜索框与状态那一行。逻辑住 [`crate::find`] · [`crate::grep`]。

use super::*;

impl FileWindow {
    /// 🔴**发一趟搜索** —— 那一件在窗口上的落点。
    ///
    /// 回值 = 真的发出去了一趟（框是空的又不是按按钮、或这个窗口没有 tokio 运行时 / 没接上通道 ⇒ `false`，后两种出声）。
    /// 发的是**原样的搜索词**（语法这一侧不读）；「只搜当前目录」开着 ⇒ 带上当前目录，否则后端搜家目录。
    /// 搜索**只有后端那一条路**（SFTP 给不了搜索），后端没起来就出声，不退回本地遍历。
    ///
    /// `force_rebuild` 是那颗「重建索引」按钮；平时要不要重走由后端回的那三个判断决定（[`crate::find`] 头注）。
    pub fn fire_search(&mut self, ctx: Option<egui::Context>, force_rebuild: bool) -> bool {
        if force_rebuild {
            self.status_wanted = true;
        }
        let asked = find::Asked {
            query: self.query.clone(),
            under: (!self.search_whole).then(|| self.cwd_path()),
            machine: self.search_whole,
            sort: self.hit_sort,
        };
        self.hit_pick = None;
        self.search.attach(ctx);
        self.search.invalidate(&asked);
        if asked.query.trim().is_empty() && !force_rebuild {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            self.search
                .say(&copy_text("rsFilewinShell.search.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            self.search.say(NO_LINE.as_str());
            return false;
        };
        let mine = self.search.start();
        let board = self.search.clone();
        let origin = self.source.origin();
        h.spawn(async move {
            find::run_search_at(board, line, origin, asked, mine, force_rebuild).await;
        });
        true
    }

    /// 命中那一摞滚到底了：后端说后面还有、没有一趟在飞 ⇒ 同号再要下一屏。回值 = 发出去了。
    pub fn fire_more(&mut self, ctx: Option<egui::Context>) -> bool {
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            return false;
        };
        let Some((mine, asked, offset)) = self.search.claim_more() else {
            return false;
        };
        self.search.attach(ctx);
        let board = self.search.clone();
        let origin = self.source.origin();
        h.spawn(async move {
            find::fetch_more(board, line, origin, mine, asked, offset).await;
        });
        true
    }

    /// 命中那一摞画成的行：那一问的号与落地趟数都没变 ⇒ 用上一帧那一份（几千条命中不每帧整份克隆、重拼）。
    pub fn hit_rows(&mut self) -> Arc<Vec<crate::rows::HitRow>> {
        let key = (self.search.current(), self.search.rounds());
        if (self.hit_rows.0, self.hit_rows.1) != key {
            let rows = self.search.hit_rows();
            self.hit_rows = (key.0, key.1, Arc::new(rows));
        }
        self.hit_rows.2.clone()
    }

    /// 点了结果表的一列表头 ⇒ 换排序、按新序从头再问（排是后端排的）。
    pub fn click_hit_header(&mut self, col: find::SortCol, ctx: Option<egui::Context>) -> bool {
        self.hit_sort = self.hit_sort.clicked(col);
        self.fire_search(ctx, false)
    }

    /// 打开第 `i` 条文件名命中：文件 ⇒ 进它所在的目录、高亮它；目录 ⇒ 进去。清掉搜索框（屏幕换回目录列表）。
    /// 回值 = 真的跳了（下标对得上这一摞）。
    pub fn jump_to_find_hit(&mut self, i: usize) -> bool {
        let Some(hit) = self.search.hit(i) else {
            return false;
        };
        let at = crate::source::RemotePath::from_bytes(&hit.path);
        if hit.dir {
            self.navigate_to_at(at);
        } else {
            let name = String::from_utf8_lossy(hit.name_bytes()).to_string();
            self.navigate_to_at(at.parent());
            self.set_reveal(&name);
        }
        self.query.clear();
        self.hit_pick = None;
        true
    }

    /// **发一趟按内容搜**：根 = 窗口现在在看的那个目录（同文件名搜索那一条取舍），要找的那一串 = 那个框里的字。
    /// 回值 = 真的发出去了（框是空的 / 没有运行时 / 没接上通道 ⇒ `false`，后两种出声）。上一趟还在飞 ⇒ 先撤掉它。
    pub fn fire_grep(&mut self, ctx: Option<egui::Context>) -> bool {
        let needle = self.grep_query.trim().to_string();
        self.grep.attach(ctx);
        if needle.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            self.grep
                .say(&copy_text("rsFilewinShell.search.noRuntime", &[]));
            return false;
        };
        let Some(line) = self.line.clone() else {
            self.grep.say(NO_LINE.as_str());
            return false;
        };
        let (mine, cancel) = self.grep.start(&needle);
        let board = self.grep.clone();
        let origin = self.source.origin();
        let root = self.cwd_path();
        h.spawn(async move {
            grep::run_grep(board, line, origin, root, needle, mine, cancel).await;
        });
        true
    }

    /// 这一帧该画「按内容搜」的命中：文件名那个框是空的、按内容那个框里有字。
    pub fn showing_grep(&self) -> bool {
        self.search_content
            && !self.grep_query.trim().is_empty()
            && self.grep.shown().needle == self.grep_query.trim()
    }

    /// 点了第 `i` 条按内容搜的命中 ⇒ 进它所在的目录、高亮它；清掉那个框（屏幕换回目录列表，那一行亮着）。
    /// 回值 = 真的跳了（下标对得上这一摞）。
    pub fn jump_to_grep_hit(&mut self, i: usize) -> bool {
        let Some(o) = self.grep.shown().outcome else {
            return false;
        };
        let Some((dir, name)) = grep::jump_target(&o, i) else {
            return false;
        };
        self.navigate_to_at(dir);
        self.set_reveal(&name);
        self.grep_query.clear();
        true
    }

    /// 这一帧该画命中，还是画当前目录。**纯函数**（判据直接喂它两侧）。
    ///
    /// 🔴 判准是**搜索框里有没有字**，不是「有没有拿到命中」——
    /// 后者会让「搜了、一条都没中」退回目录列表，而那和「没搜」在屏幕上分不开
    /// （用户会以为搜索没生效）。
    pub fn showing_hits(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// 搜索那两条状态行（按名字 · 按内容）；目录换了而范围是「当前目录以下」⇒ 先按新目录再搜。
    pub(super) fn search_status_frame(&mut self, ui: &mut egui::Ui) {
        // 范围是「当前目录以下」、框里有字、而目录换了 ⇒ 范围变了，按新目录再搜一趟。
        if !self.search_whole
            && !self.query.trim().is_empty()
            && self.search.asked_under().as_ref() != Some(&self.cwd_path())
        {
            let ctx = ui.ctx().clone();
            self.fire_search(Some(ctx), false);
        }
        // 搜索结果顶上那一条状态行（范围 · 个数 · 文件清单多久前 · 刷新）：只在搜索时摆。
        if self.showing_hits() || self.search.is_running() || self.status_wanted {
            let machine = self.source.label();
            if let Some(a) = self.search.status_ui(ui, &machine, self.search_whole) {
                let ctx = ui.ctx().clone();
                match a {
                    find::SearchAction::Refresh => {
                        self.fire_search(Some(ctx), true);
                    }
                    find::SearchAction::Scope(whole) => {
                        self.search_whole = whole;
                        self.fire_search(Some(ctx), false);
                    }
                    find::SearchAction::Retry => {
                        self.fire_search(Some(ctx), false);
                    }
                    find::SearchAction::CopyPath(path) => {
                        ctx.copy_text(path);
                        self.receipt = Some(copy_text("rsFilewinShell.receipt.pathCopied", &[]));
                    }
                }
            }
        }
        // 按内容搜的状态行（在搜 ＋「停」· 几处几个文件 · 跳过几个［详情］）。
        if self.grep.is_running() || self.showing_grep() {
            if self.grep.status_ui(ui) {
                let ctx = ui.ctx().clone();
                self.fire_grep(Some(ctx));
            }
        }
    }

    /// 文件名命中那一摞。
    pub(super) fn hits_frame(&mut self, ui: &mut egui::Ui) {
        let hits = self.hit_rows();
        if hits.is_empty() && self.search.has_outcome() && !self.search.index_missing() {
            self.no_hits_ui(ui);
        } else if self.search.has_outcome() {
            self.hit_rows_frame(ui, &hits);
        }
    }

    pub(super) fn no_hits_ui(&mut self, ui: &mut egui::Ui) {
        // 没结果：中央一句 ＋（范围是当前目录以下时）「搜整台机器」。
        let q = self.query.clone();
        let whole = self.search_whole;
        let mut go_whole = false;
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.label(find::no_match_line(&q));
            if !whole
                && ui
                    .button(copy_text("rsFilewinFind.action.searchMachine", &[]))
                    .clicked()
            {
                go_whole = true;
            }
        });
        if go_whole {
            self.search_whole = true;
            let ctx = ui.ctx().clone();
            self.fire_search(Some(ctx), false);
        }
    }

    pub(super) fn hit_rows_frame(&mut self, ui: &mut egui::Ui, hits: &[crate::rows::HitRow]) {
        let tail = if self.search.page_failed() {
            crate::rows::HitTail::Failed
        } else if self.search.more_to_come() {
            crate::rows::HitTail::More
        } else {
            crate::rows::HitTail::End
        };
        // 🔴收数口是 [`HitTally`]，**不是** `self.tally` —— 它交出的下标只指命中这一摞，
        //    而下面那几条胶水索引的是 `listing.rows`（另一摞东西）。
        show_hit_rows(
            ui,
            &hits,
            self.hit_sort,
            self.hit_pick,
            tail,
            &mut self.hits_tally,
        );
        let ctx = ui.ctx().clone();
        if let Some(col) = self.hits_tally.sort_click.take() {
            self.click_hit_header(col, Some(ctx.clone()));
        }
        if let Some(i) = self.hits_tally.picked.take() {
            self.hit_pick = Some(i);
        }
        if std::mem::take(&mut self.hits_tally.retry_more) {
            self.search.retry_more();
            self.fire_more(Some(ctx.clone()));
        }
        // 滚到底（最后一行露出来了）⇒ 往下再要一屏（后端说还有才发；失败过就等「重试」）。
        if !hits.is_empty() && self.hits_tally.last_row >= hits.len() {
            self.fire_more(Some(ctx));
        }
    }

    pub(super) fn grep_frame(&mut self, ui: &mut egui::Ui) {
        // 按内容搜的命中：每行点得开（跳到那份文件），收数口是 [`GrepTally`]（它的下标只指这一摞）。
        let rows: Vec<grep::GrepRow> = self
            .grep
            .shown()
            .outcome
            .map(|o| grep::grep_rows(&o))
            .unwrap_or_default();
        grep::show_grep_rows(ui, &rows, &mut self.grep_tally);
    }

    /// 工具条上搜索那一格：框内左侧两段「名字 | 内容」· 输入框 · 右侧 ×（有字时）· 在飞指示。
    /// 「名字」🔴 **`changed()` 就发** —— 一敲就出（没有去抖：在飞的旧那几趟后端按号收手、这一侧按号丢掉，结果不会错）；
    /// 「内容」回车才发（按内容搜要把整棵树读一遍）。范围与「刷新」在结果顶上那一条状态行里（`frame_body`）。
    pub(crate) fn search_box(&mut self, ui: &mut egui::Ui, compact: Option<f32>) {
        let p = crate::theme::palette(ui.ctx());
        let mut fire = false;
        let mut fire_grep = false;
        let mut switch: Option<bool> = None;
        let mut clear = false;
        let w = compact.unwrap_or(SEARCH_BOX_WIDTH);
        let focused = ui.memory(|m| m.has_focus(egui::Id::new(SEARCH_BOX_ID)));
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(egui::Stroke::new(
                1.0,
                if focused { p.accent } else { p.border_soft },
            ))
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (content, label) in [
                    (false, copy_text("acctNew.name.label", &[])),
                    (true, copy_text("rsFilewinShell.search.byContent", &[])),
                ] {
                    let on = self.search_content == content;
                    if crate::kit::toggle(ui, "", &label, on).clicked() && !on {
                        switch = Some(content);
                    }
                }
                let (buf, hint) = if self.search_content {
                    (
                        &mut self.grep_query,
                        copy_text("rsFilewinShell.search.hintContent", &[]),
                    )
                } else {
                    (
                        &mut self.query,
                        copy_text("rsFilewinShell.search.hint", &[]),
                    )
                };
                let has_text = !buf.is_empty();
                let r = ui.add(
                    egui::TextEdit::singleline(buf)
                        .id(egui::Id::new(SEARCH_BOX_ID))
                        .frame(egui::Frame::NONE)
                        .desired_width((w - 110.0).max(60.0))
                        .hint_text(hint),
                );
                if self.search_content {
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        fire_grep = true;
                    }
                } else if r.changed() {
                    fire = true;
                }
                if self.search.is_running() || self.grep.is_running() {
                    ui.spinner();
                }
                if has_text
                    && ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(egui_phosphor::regular::X).color(p.text2),
                            )
                            .frame(false),
                        )
                        .clicked()
                {
                    clear = true;
                }
            });
        if let Some(content) = switch {
            // 换一种搜法：框里的字跟着过去（名字 → 内容不立刻读整棵树，等回车）。
            if content {
                self.grep_query = std::mem::take(&mut self.query);
            } else {
                self.query = std::mem::take(&mut self.grep_query);
                fire = true;
            }
            self.search_content = content;
            ui.memory_mut(|m| m.request_focus(egui::Id::new(SEARCH_BOX_ID)));
        }
        if clear {
            self.query.clear();
            self.grep_query.clear();
            self.grep.stop();
        }
        let ctx = ui.ctx().clone();
        if fire {
            self.fire_search(Some(ctx.clone()), false);
        }
        if fire_grep {
            self.fire_grep(Some(ctx));
        }
    }
}
