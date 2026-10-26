//! 改一份远端文本：编辑页（读 · 存 · 盖掉 · 重读 · 查找替换 · 关之前那一问）。编辑面本身住 [`crate::editor`]。

use super::*;

impl FileWindow {
    // ═══════════════════════════════════════════════════════════════════
    // 🔴改一份远端文本
    // ═══════════════════════════════════════════════════════════════════

    /// 打开着的那一份（`None` = 没在编辑）。判据与界面看同一个值。
    pub fn editing(&self) -> Option<&crate::editor::Pane> {
        self.editing.as_ref()
    }

    /// 打开着的那一份（可改：截图与判据摆「盘上被改过」那一形用）。
    pub fn editing_mut(&mut self) -> Option<&mut crate::editor::Pane> {
        self.editing.as_mut()
    }

    /// 编辑框里那些字（生产那个 `TextEdit` 要的 `&mut String` 从这儿出来）。
    ///
    /// ⚠ **它是生产代码，不是测试钩子**（编辑面那个 `TextEdit` 拿的就是这一个 `&mut`）。
    pub fn editing_text_mut(&mut self) -> Option<&mut String> {
        self.editing.as_mut().map(|p| &mut p.text)
    }

    /// 关窗那一问摆着吗。
    pub fn asking_discard(&self) -> bool {
        self.asking_discard
    }

    /// 起一趟「打开第 `i` 行」。回值 = **真的发出去了**。
    ///
    /// 🔴 改不了的那几档在这儿就答完了（[`crate::editor::why_not_editable`]），
    /// **连那趟往返都不发** —— 而且把**为什么**说出来。
    /// 逐条理由住 `editor.rs` 头注「超了怎么办」那一节。
    pub fn begin_edit(&mut self, i: usize, ctx: Option<egui::Context>) -> bool {
        let Some((row, at, refused)) = self.listing.rows.lock().unwrap().get(i).map(|r| {
            (
                r.clone(),
                self.row_path(r),
                crate::editor::why_not_editable(r),
            )
        }) else {
            return false;
        };
        // 挂在窗口里的目录页：交给窗口开一个编辑页（同一份已开着 ⇒ 切过去），这一页还是目录。
        if self.in_workspace && !self.edit_tab {
            self.want_edit = Some(EditWant { row, at, refused });
            return true;
        }
        self.begin_edit_at(row, at, refused, ctx)
    }

    /// 读一份来编辑（编辑页自己调；单独建的目录视图也走这里，判据那一形）。
    /// 本地预判做不了 ⇒ 编辑页中央说那一句（不是编辑页 ⇒ 列表上方说）。
    pub fn begin_edit_at(
        &mut self,
        listed: Listed,
        at: crate::source::RemotePath,
        refused: Option<String>,
        ctx: Option<egui::Context>,
    ) -> bool {
        // 读着一份时不起第二趟：后到的那一份会把正在改的那一份换掉。
        if let Some(p) = self.edits.open_pending() {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.edit.stillOpening", &[("path", &p)]).into());
            return false;
        }
        self.edit_row = Some(listed.clone());
        let row = listed.row.clone();
        // 本地预判（后端那道编辑门）—— 出声，不灰置；放行了 ⇒ 那道门给的数就是这一份的上限。
        let cap = crate::editor::edit_cap(&listed).unwrap_or_default();
        if let Some(why) = refused.or_else(|| crate::editor::why_not_editable(&listed)) {
            let said = copy_text(
                "rsFilewinShell.edit.refused",
                &[("name", &row.name.to_string()), ("why", &why.to_string())],
            );
            if self.edit_tab {
                self.edit_refused = Some(said);
            } else {
                *self.listing.error.lock().unwrap() = Some(said.into());
            }
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.edit.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string().into());
            return false;
        };
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_open(&row.path);
        self.edit_raw = at.raw.clone().map(|b| (row.path.clone(), b));
        h.spawn(async move {
            use crate::editor::Arrived;
            // 读文本经通道问后端（`files-read-text`）。路径按字节发。
            let got = crate::editor::read_text_at(&line, &origin, &at, cap).await;
            board.deliver(match got {
                Ok(Some(o)) => Arrived::Text {
                    path: row.path.clone(),
                    name: row.name.clone(),
                    text: o.text,
                    sha256: o.sha256,
                    cap,
                },
                Ok(None) => Arrived::NotText { path: row.path },
                Err(why) => Arrived::Failed {
                    path: row.path,
                    why,
                },
            });
        });
        true
    }

    /// 到货了就把编辑面立起来 / 把那句话摆出来。回值 = 这一帧真的消化了一趟。
    ///
    /// ⚠ 到货是**一次性事件**（`take_arrived`）—— 留着的话下一帧会再建一次编辑面，
    /// 把用户已经敲的东西盖掉。
    pub fn settle_opened_edits(&mut self) -> bool {
        use crate::editor::Arrived;
        let Some(a) = self.edits.take_arrived() else {
            return false;
        };
        match a {
            Arrived::Text {
                path,
                name,
                text,
                sha256,
                cap,
            } => {
                let mut pane = crate::editor::Pane::opened(&path, &name, text, sha256, cap);
                // 有损名：读的时候记下的字节交给编辑面（存盘走它）。
                pane.raw_path = self
                    .edit_raw
                    .take()
                    .filter(|(p, _)| *p == path)
                    .map(|(_, b)| b);
                self.editing = Some(pane);
            }
            Arrived::NotText { path } => {
                let said = crate::editor::not_text_notice(&path);
                if self.edit_tab {
                    self.edit_refused = Some(said);
                } else {
                    *self.listing.error.lock().unwrap() = Some(said.into());
                }
            }
            Arrived::Failed { path, why } => {
                let said = copy_text(
                    "reason.fs.readFailed",
                    &[("path", &path.to_string()), ("why", &why.said)],
                );
                if self.edit_tab {
                    self.edit_refused = Some(said);
                } else {
                    *self.listing.error.lock().unwrap() = Some(crate::source::Failed {
                        code: why.code,
                        said,
                        detail: why.detail,
                    });
                }
            }
        }
        true
    }

    /// 存回去。回值 = **真的发出去了**。
    pub fn save_edit(&mut self, ctx: Option<egui::Context>) -> bool {
        // 上一趟还没回来：再发一趟带的是旧摘要，回来就是一场假冲突。
        if self.edits.save_pending() {
            return false;
        }
        let Some(p) = self.editing.clone() else {
            return false;
        };
        // 此刻写不了（断着）⇒ 不发：［保存］灰着、条上说着为什么（Ctrl+S 与「全部保存后关闭」那一路也到这里）。
        if self.link.write_refused().is_some() {
            return false;
        }
        // 🔴 敲超上限 ⇒ 屏幕上先说，不发那趟注定被池子拒的往返。
        if p.over_cap() {
            *self.listing.error.lock().unwrap() = Some(
                copy_text(
                    "rsFilewinShell.edit.tooBig",
                    &[
                        ("n", &(p.text.len()).to_string()),
                        ("limit", &(copy_core::size_text(p.cap)).to_string()),
                    ],
                )
                .into(),
            );
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.save.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string().into());
            return false;
        };
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_save(&p.path);
        h.spawn(async move {
            let at = crate::source::RemotePath::of(&p.path, p.raw_path.as_deref());
            let r = crate::editor::write_text_at(
                &line,
                &origin,
                &at,
                &p.text,
                p.expect_sha256(),
                p.cap,
            )
            .await;
            board.deliver_save(r);
        });
        true
    }

    /// 存盘撞上「盘上那份在你打开之后被改过了」之后，人点了**仍然覆盖**。回值 = 真的发出去了。
    ///
    /// 先重读一趟拿盘上此刻那一份的摘要、再以它为 `expect` 存（`editor::overwrite_anyway`）—— CAS 仍在。
    pub fn overwrite_edit(&mut self, ctx: Option<egui::Context>) -> bool {
        if self.edits.save_pending() {
            return false;
        }
        let Some(p) = self.editing.clone() else {
            return false;
        };
        if p.over_cap() {
            return self.save_edit(ctx);
        }
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string().into());
            return false;
        };
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_save(&p.path);
        h.spawn(async move {
            let at = crate::source::RemotePath::of(&p.path, p.raw_path.as_deref());
            let r = crate::editor::overwrite_anyway_at(&line, &origin, &at, &p.text, p.cap).await;
            board.deliver_save(r);
        });
        true
    }

    /// 存盘撞上 stale 之后，人点了**丢掉我的改动、重新打开**：编辑面收掉、同一份重读一遍。
    /// 回值 = 真的发出去了（拿不到运行时 / 通道 ⇒ 编辑面照旧留着，一个字不丢）。
    pub fn reopen_edit(&mut self, ctx: Option<egui::Context>) -> bool {
        if self.edits.save_pending() {
            return false;
        }
        let Some(p) = self.editing.as_ref() else {
            return false;
        };
        let (path, name, cap) = (p.path.clone(), p.name.clone(), p.cap);
        let at = crate::source::RemotePath::of(&path, p.raw_path.as_deref());
        self.edit_raw = at.raw.clone().map(|b| (path.clone(), b));
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string().into());
            return false;
        };
        self.discard_edit();
        let origin = self.source.origin();
        let board = self.edits.clone();
        board.attach(ctx);
        board.begin_open(&path);
        h.spawn(async move {
            use crate::editor::Arrived;
            let got = crate::editor::read_text_at(&line, &origin, &at, cap).await;
            board.deliver(match got {
                Ok(Some(o)) => Arrived::Text {
                    path,
                    name,
                    text: o.text,
                    sha256: o.sha256,
                    cap,
                },
                Ok(None) => Arrived::NotText { path },
                Err(why) => Arrived::Failed { path, why },
            });
        });
        true
    }

    /// 存的结局到货了就落进那一份上。回值 = 这一帧真的消化了一趟。
    ///
    /// 🔴 失败时 **`text` 一个字都不碰**（那是用户唯一的一份）——
    /// 逐条理由住 `editor::Pane::mark_failed`。
    pub fn settle_saved_edits(&mut self) -> bool {
        let Some(r) = self.edits.take_saved() else {
            return false;
        };
        let Some(p) = self.editing.as_mut() else {
            return false;
        };
        match r {
            Ok(s) => {
                p.mark_saved(&s.sent, s.sha256);
                // 那一问答了「保存」：存成了（且路上没再敲字）⇒ 关这一页。
                if self.close_after_save && !p.dirty() {
                    self.want_close = true;
                }
            }
            Err(crate::editor::SaveError::Stale(why)) => p.mark_stale(why),
            Err(crate::editor::SaveError::Failed(f)) => p.mark_failed_with(f),
        }
        self.close_after_save = false;
        true
    }

    /// 关掉编辑面这一下。回值 = **真的关掉了**（改了没存 ⇒ 先摆出那一问，回 `false`）。
    pub fn close_edit(&mut self) -> bool {
        use crate::editor::Close;
        let Some(p) = self.editing.as_ref() else {
            return false;
        };
        match crate::editor::judge_close(p) {
            Close::Now => {
                self.editing = None;
                self.asking_discard = false;
                true
            }
            Close::NeedsConfirm => {
                self.asking_discard = true;
                false
            }
        }
    }

    /// 那一问答了「丢掉」⇒ 真的关掉。
    pub fn discard_edit(&mut self) {
        self.editing = None;
        self.asking_discard = false;
    }

    /// 那一问答了「不丢」⇒ 收掉问，编辑面留着。
    pub fn keep_editing(&mut self) {
        self.asking_discard = false;
    }

    /// 🔴画**编辑面**：在飞指示 ＋ 那一份文本 ＋ 存的结局 ＋ 关窗那一问。
    ///
    /// ⚠ 它是模态的（同别的几摞）：一份文本改着的时候不该同时去改目录结构。
    /// 〔「查找替换」〕从编辑框此刻的光标 / 选区起找查找框里那几个字（往后找从选区尾起、往前找从选区头起，
    /// 到头绕回）；找到 ⇒ 把它选中、焦点给编辑框；没找到 ⇒ 查找栏上说一句。大文件模式没有这件事（`§5.5`）。回值 ＝ 找到了。
    pub fn find_in_editor(&mut self, ctx: &egui::Context, backward: bool) -> bool {
        let Some(p) = self.editing.as_mut() else {
            return false;
        };
        let id = crate::bigfile::normal_editor_id(&p.path);
        if p.big.is_big() {
            return false;
        }
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let (a, b) = st.cursor.char_range().map_or((0, 0), |r| {
            let r = r.as_sorted_char_range();
            (r.start.0, r.end.0)
        });
        let from = if backward { a } else { b };
        match crate::editor::find_from(&p.text, &p.find.needle, from, backward) {
            Some((s, e)) => {
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(s),
                    egui::text::CCursor::new(e),
                )));
                st.store(ctx, id);
                ctx.memory_mut(|m| m.request_focus(id));
                p.find.notice = None;
                p.reveal = true;
                true
            }
            None => {
                p.find.notice = Some(copy_text(
                    "rsFilewinShell.editor.findNone",
                    &[("needle", &p.find.needle.to_string())],
                ));
                false
            }
        }
    }

    /// 编辑面查找栏那一格（判据与界面同一个口）。没开编辑面 ⇒ `None`。
    pub fn find_bar_mut(&mut self) -> Option<&mut crate::editor::FindBar> {
        self.editing.as_mut().map(|p| &mut p.find)
    }

    /// 替换：`all` ⇒ 全文替换，说换了几处；否则 ⇒ 选区恰好是查找串就换掉它，再找下一个（不是 ⇒ 只找下一个）。
    /// 回值 ＝ 换了几处。改的是编辑框那一份字（与敲键同一个 `String`）；存盘照旧要点「存」。
    pub fn replace_in_editor(&mut self, ctx: &egui::Context, all: bool) -> usize {
        let Some(p) = self.editing.as_mut() else {
            return 0;
        };
        let id = crate::bigfile::normal_editor_id(&p.path);
        if p.big.is_big() || p.find.needle.is_empty() {
            return 0;
        }
        if all {
            let (t, n) = crate::editor::replace_all(&p.text, &p.find.needle, &p.find.with);
            p.text = t;
            p.find.notice = Some(if n == 0 {
                copy_text(
                    "rsFilewinShell.editor.findNone",
                    &[("needle", &p.find.needle.to_string())],
                )
            } else {
                copy_text(
                    "rsFilewinShell.editor.replacedAll",
                    &[("n", &n.to_string())],
                )
            });
            return n;
        }
        let mut st = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let (a, b) = st.cursor.char_range().map_or((0, 0), |r| {
            let r = r.as_sorted_char_range();
            (r.start.0, r.end.0)
        });
        let picked: String = p.text.chars().skip(a).take(b.saturating_sub(a)).collect();
        let mut n = 0;
        if a < b && picked == p.find.needle {
            p.text = crate::editor::replace_chars(&p.text, a, b, &p.find.with);
            let after = a + p.find.with.chars().count();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(after),
            )));
            st.store(ctx, id);
            n = 1;
        }
        self.find_in_editor(ctx, false);
        n
    }

    /// 🔴画**编辑页**（一份文本一个标签页；高度只随窗口变、绝不随内容长）：
    /// 头条（面包屑 · 状态字 · 查找 · 保存 · ⋯）→ 条（盘上被改过 · 保存失败 · 断线）→ 编辑面（行号槽 ＋ 正文，横竖都在面里滚）；
    /// 查找条浮在编辑面右上；底条那一行住窗口的状态栏（[`Self::editor_status`]）。
    /// 键：Ctrl+S 保存（在存着时不起第二趟）· Ctrl+F 查找条 · Esc 只收查找条，**从不关编辑页**。
    pub(super) fn edit_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let big = self.editing.as_ref().is_some_and(|e| e.big.is_big());
        let mut acts = EditActs {
            save: self.edit_keys(ui, big),
            ..EditActs::default()
        };
        let f = self.edit_facts(big);
        self.edit_head(ui, &f, &mut acts);
        if acts.find_toggle {
            self.find_open = !self.find_open;
            if self.find_open {
                ui.memory_mut(|m| m.request_focus(egui::Id::new(FIND_ID)));
            }
        }
        self.edit_banners(ui, &f, &mut acts);
        let body = self.edit_surface(ui, &mut acts);
        self.edit_find_bar(&ctx, body, big);
        self.edit_discard_ask(&ctx, &f.name, &mut acts);
        self.apply_edit_acts(&ctx, &acts);
    }

    /// 编辑页的键：Ctrl+S（交回「要存」）· Ctrl+F 开查找条 · Esc 只收查找条。
    pub(super) fn edit_keys(&mut self, ui: &mut egui::Ui, big: bool) -> bool {
        let save = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        if !big && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.find_open = true;
            ui.memory_mut(|m| m.request_focus(egui::Id::new(FIND_ID)));
        }
        if self.find_open
            && !self.asking_discard
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.find_open = false;
        }
        save
    }

    /// 这一帧编辑页画的是哪一份、什么状态。
    pub(super) fn edit_facts(&self, big: bool) -> EditFacts {
        let pane = self.editing.clone();
        let path = pane
            .as_ref()
            .map(|e| e.path.clone())
            .or_else(|| self.edits.opening())
            .or_else(|| self.edit_row.as_ref().map(|r| r.path.clone()))
            .unwrap_or_else(|| self.cwd.clone());
        let name = crate::source::remote_basename(&path).to_string();
        let idle = !self.edits.save_pending();
        let dirty = pane.as_ref().is_some_and(crate::editor::Pane::dirty);
        let over = pane.as_ref().is_some_and(crate::editor::Pane::over_cap);
        EditFacts {
            pane,
            path,
            name,
            idle,
            dirty,
            over,
            big,
        }
    }

    /// 头条：面包屑 · 状态字 · ⋯ · 保存 · 查找。
    pub(super) fn edit_head(&mut self, ui: &mut egui::Ui, f: &EditFacts, acts: &mut EditActs) {
        // ── 头条（36，`--bg-2`）──
        crate::kit::strip(ui, |ui| {
            let mut go = self.edit_crumbs(ui, f);
            self.edit_state(ui, f);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.edit_head_acts(ui, f, acts, &mut go);
            });
            if let Some(d) = go {
                self.want_dir = Some(d);
            }
        });
    }

    /// 头条左边：那台 › 面包屑（只留最后两级）› 文件名；点哪一段回到那一级（交回要去的目录）。
    pub(super) fn edit_crumbs(&self, ui: &mut egui::Ui, f: &EditFacts) -> Option<String> {
        self.path_crumbs(ui, &f.path)
    }

    /// 那台 › 面包屑（只留最后两级）› 文件名（编辑页 · 查看页的头条共用）；点哪一段交回要去的目录。
    pub(crate) fn path_crumbs(&self, ui: &mut egui::Ui, path: &str) -> Option<String> {
        let p = crate::theme::palette(ui.ctx());
        let name = &crate::source::remote_basename(path).to_string();
        let machine = self.source.label();
        let crumbs = crate::source::breadcrumbs(&crate::source::parent_dir(&path));
        let mut go = None;
        if ui
            .add(
                egui::Button::new(egui::RichText::new(&machine).color(p.text2))
                    .frame_when_inactive(false),
            )
            .clicked()
        {
            go = Some(self.cwd.clone());
        }
        // 前几段折成「…」：只留最后两级目录（窄档同一条规矩）。
        let tail = crumbs.len().saturating_sub(2);
        let sep = egui_phosphor::regular::CARET_RIGHT;
        if tail > 1 {
            ui.label(egui::RichText::new(sep).color(p.faint));
            ui.label(
                egui::RichText::new(copy_text("rsFilewinEditPage.crumb.more", &[])).color(p.text2),
            );
        }
        for (label, full) in crumbs.iter().skip(tail.max(1)) {
            ui.label(egui::RichText::new(sep).color(p.faint));
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(label).color(p.text2))
                        .frame_when_inactive(false),
                )
                .on_hover_text(full)
                .clicked()
            {
                go = Some(full.clone());
            }
        }
        ui.label(egui::RichText::new(sep).color(p.faint));
        ui.label(egui::RichText::new(name).color(p.text));
        go
    }

    /// 状态字：在读 · 在存 · 未保存（带那颗点）· 几点存的。
    pub(super) fn edit_state(&self, ui: &mut egui::Ui, f: &EditFacts) {
        let p = crate::theme::palette(ui.ctx());
        let (pane, dirty) = (&f.pane, f.dirty);
        let state = if self.edits.opening().is_some() {
            Some(copy_text("kit.state.reading", &[]))
        } else if self.edits.saving().is_some() {
            Some(copy_text("rsFilewinEditPage.state.saving", &[]))
        } else if dirty {
            Some(copy_text("rsFilewinEditPage.state.unsaved", &[]))
        } else {
            pane.as_ref().and_then(|e| e.saved_at).map(|t| {
                copy_text(
                    "rsFilewinEditPage.state.saved",
                    &[("time", &crate::source::mtime_text(t).short)],
                )
            })
        };
        if let Some(s) = state {
            ui.add_space(8.0);
            if dirty {
                // 未保存那颗 6px 实心点（与标签上那颗同一个样子）。
                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 3.0, p.text2);
            }
            ui.label(egui::RichText::new(s).small().color(p.text2));
        }
    }

    /// 头条右边（从右往左）：⋯ 菜单 · 保存 · 查找。
    pub(super) fn edit_head_acts(
        &mut self,
        ui: &mut egui::Ui,
        f: &EditFacts,
        acts: &mut EditActs,
        go: &mut Option<String>,
    ) {
        let p = crate::theme::palette(ui.ctx());
        let (pane, path, name) = (&f.pane, &f.path, &f.name);
        let (idle, dirty, over, big) = (f.idle, f.dirty, f.over, f.big);
        use egui_phosphor::regular as ph;
        let entries = [
            (
                ph::ARROW_CLOCKWISE,
                copy_text("rsFilewinEditPage.more.reload", &[]),
            ),
            (
                ph::DOWNLOAD_SIMPLE,
                copy_text("rsFilewinEditPage.more.download", &[]),
            ),
            (
                ph::LIST_MAGNIFYING_GLASS,
                copy_text("rsFilewinEditPage.more.reveal", &[]),
            ),
        ]
        .map(|(icon, label)| crate::kit::Entry::Row(crate::kit::MenuRow::new(icon, label)));
        match crate::kit::menu(ui, egui::RichText::new(ph::DOTS_THREE).size(16.0), &entries).1 {
            Some(0) => acts.reopen = true,
            Some(1) => acts.pull = true,
            Some(2) => {
                *go = Some(crate::source::parent_dir(path));
                self.want_reveal = Some(name.clone());
            }
            _ => {}
        }
        let label = if self.edits.saving().is_some() {
            copy_text("rsFilewinEditPage.state.saving", &[])
        } else {
            copy_text("rsFilewinShell.editor.save", &[])
        };
        // 能不能写读窗口那一格（[`crate::chrome::LinkState::write_refused`]，与工具条 · 菜单同一处）。
        let refused = self.link.write_refused();
        let can = pane.is_some() && dirty && idle && !over && refused.is_none();
        let why = if let Some(r) = refused {
            r
        } else if over {
            copy_text(
                "rsFilewinEditPage.foot.overCap",
                &[("n", &pane.as_ref().map_or(0, |e| -e.headroom()).to_string())],
            )
        } else {
            String::new()
        };
        let b = egui::Button::new(egui::RichText::new(&label).color(if can {
            p.text
        } else {
            p.text2
        }))
        .fill(if can { p.accent_strong } else { p.hover });
        let r = ui.add_enabled(can, b);
        if !why.is_empty() {
            r.clone().on_disabled_hover_text(&why);
        }
        if r.clicked() {
            acts.save = true;
        }
        if !big
            && crate::kit::ghost(
                ui,
                egui_phosphor::regular::MAGNIFYING_GLASS,
                "",
                pane.is_some(),
                "",
            )
            .on_hover_text(copy_text("rsFilewinEditPage.find.hint", &[]))
            .clicked()
        {
            acts.find_toggle = true;
        }
    }

    /// 编辑面顶上那几条：盘上被改过 · 保存失败 · 存成了没拿到摘要 · 断线。
    pub(super) fn edit_banners(&mut self, ui: &mut egui::Ui, f: &EditFacts, acts: &mut EditActs) {
        let (pane, idle, dirty) = (&f.pane, f.idle, f.dirty);
        // ── 条：盘上被改过 · 保存失败 · 断线（字一个不动）──
        if let Some(e) = pane {
            if e.stale {
                match crate::kit::banner(
                    ui,
                    crate::kit::Tone::Warn,
                    &copy_text("rsFilewinEditPage.stale.line", &[("name", &e.name)]),
                    &[
                        crate::editor::OVERWRITE_LABEL.clone(),
                        crate::editor::REOPEN_LABEL.clone(),
                    ],
                ) {
                    Some(0) if idle => acts.overwrite = true,
                    Some(1) if idle => acts.reopen = true,
                    _ => {}
                }
            } else if let Some(Err(why)) = &e.last_save {
                let id = ui.id().with("save-fail-detail");
                if crate::kit::failure_banner(
                    ui,
                    crate::kit::Tone::Error,
                    &copy_text("rsFilewinShell.editor.saveFailed", &[("why", why)]),
                    &[copy_text("rsFilewinEditPage.action.retry", &[])],
                    &e.save_detail,
                    id,
                )
                .is_some()
                    && idle
                {
                    acts.save = true;
                }
            } else if let Some(note) = e.save_note.as_ref().filter(|_| !dirty) {
                // 存成了、没拿到新摘要：那一句挂着，给一颗「重新读取」（重读拿到摘要，之后照常存）。
                if crate::kit::banner(
                    ui,
                    crate::kit::Tone::Warn,
                    note,
                    &[copy_text("rsFilewinEditPage.more.reload", &[])],
                )
                .is_some()
                    && idle
                {
                    acts.reopen = true;
                }
            }
            if let Some(cut) = self.link.cut() {
                let line = match cut {
                    pick_core::Cut::Monitor => copy_text("rsFilewinEditPage.severed.line", &[]),
                    pick_core::Cut::Machine => copy_text(
                        "rsFilewinEditPage.offline.line",
                        &[("machine", &self.source.label())],
                    ),
                };
                crate::kit::banner(ui, crate::kit::Tone::Warn, &line, &[]);
            }
        }
    }

    /// 编辑面（或打不开时那一句、在读时的骨架）；交回编辑面那一块（查找条贴着它的右上）。
    pub(super) fn edit_surface(&mut self, ui: &mut egui::Ui, acts: &mut EditActs) -> egui::Rect {
        let p = crate::theme::palette(ui.ctx());
        // ── 编辑面 ──
        let body = ui.available_rect_before_wrap();
        if let Some(why) = self.edit_refused.clone() {
            match crate::kit::empty_state(
                ui,
                egui_phosphor::regular::FILE_X,
                &why,
                &[
                    copy_text("rsFilewinEditPage.action.download", &[]),
                    copy_text("rsFilewinEditPage.action.retry", &[]),
                ],
            ) {
                Some(0) => acts.pull = true,
                Some(1) => acts.retry_open = true,
                _ => {}
            }
        } else if self.edits.opening().is_some() && self.editing.is_none() {
            crate::kit::skeleton_rows(ui, 8);
        } else if self.editing.is_some() {
            egui::Frame::new().fill(p.bg).show(ui, |ui| {
                ui.set_min_size(body.size());
                let h = ui.available_height();
                crate::bigfile::show(ui, self.editing.as_mut(), h);
            });
        }
        body
    }

    /// 查找条（浮在编辑面右上）与它那几个动作。
    pub(super) fn edit_find_bar(&mut self, ctx: &egui::Context, body: egui::Rect, big: bool) {
        // ── 查找条（浮在编辑面右上，`--card`）──
        let mut find_act = None;
        if self.find_open && !big {
            // 「3 / 12」：一共几处 · 光标在第几处（选区起点之前有几处 ＋ 1；不在任何一处上 ⇒ 0）。
            let count = self
                .editing
                .as_ref()
                .filter(|e| !e.find.needle.is_empty())
                .map(|e| {
                    let at =
                        egui::TextEdit::load_state(&ctx, crate::bigfile::normal_editor_id(&e.path))
                            .and_then(|s| s.cursor.char_range())
                            .map_or(0, |r| r.as_sorted_char_range().start.0);
                    let starts: Vec<usize> = e
                        .text
                        .match_indices(e.find.needle.as_str())
                        .map(|(b, _)| e.text[..b].chars().count())
                        .collect();
                    let k = starts.iter().position(|&s| s == at).map_or(0, |i| i + 1);
                    (k, starts.len())
                });
            if let Some(e) = self.editing.as_mut() {
                let at = egui::pos2(body.right() - 16.0, body.top() + 8.0);
                egui::Area::new(egui::Id::new(("filewin-find-bar", &e.path)))
                    .order(egui::Order::Foreground)
                    .pivot(egui::Align2::RIGHT_TOP)
                    .fixed_pos(at)
                    .show(&ctx, |ui| {
                        egui::Frame::popup(&ctx.global_style()).show(ui, |ui| {
                            find_act = find_row(ui, &mut e.find, count);
                        });
                    });
            }
        }
        if let Some(a) = find_act {
            match a {
                FindAct::Next => {
                    self.find_in_editor(&ctx, false);
                }
                FindAct::Prev => {
                    self.find_in_editor(&ctx, true);
                }
                FindAct::Replace => {
                    self.replace_in_editor(&ctx, false);
                }
                FindAct::ReplaceAll => {
                    self.replace_in_editor(&ctx, true);
                }
                FindAct::Close => self.find_open = false,
            }
        }
    }

    /// 关这一页那一问（改了没存）：取消 · 不保存 · 保存。
    pub(super) fn edit_discard_ask(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        acts: &mut EditActs,
    ) {
        let p = crate::theme::palette(ctx);
        // ── 关这一页那一问（改了没存）：取消 · 不保存 · 保存（焦点在「保存」）──
        if self.asking_discard {
            let hit = crate::kit::dialog(
                &ctx,
                "filewin-edit-discard",
                &copy_text("rsFilewinShell.editor.unsaved", &[("name", &name)]),
                |ui| {
                    ui.label(
                        egui::RichText::new(copy_text("rsFilewinShell.editor.unsavedWarn", &[]))
                            .color(p.text),
                    );
                },
                &[
                    (
                        copy_text("rsFilewinShell.editor.keep", &[]),
                        crate::kit::Btn::Plain,
                    ),
                    (
                        copy_text("rsFilewinShell.editor.discard", &[]),
                        crate::kit::Btn::Plain,
                    ),
                    (
                        copy_text("rsFilewinShell.editor.saveClose", &[]),
                        crate::kit::Btn::Primary,
                    ),
                ],
                2,
                0,
            );
            match hit {
                Some(0) => self.keep_editing(),
                Some(1) => {
                    self.discard_edit();
                    self.want_close = true;
                }
                Some(2) => {
                    self.asking_discard = false;
                    self.close_after_save = true;
                    acts.save = true;
                }
                _ => {}
            }
        }
    }

    /// 这一帧点了什么就做什么（一帧至多一件，存优先）。
    pub(super) fn apply_edit_acts(&mut self, ctx: &egui::Context, acts: &EditActs) {
        if acts.save {
            self.save_edit(Some(ctx.clone()));
        } else if acts.overwrite {
            self.overwrite_edit(Some(ctx.clone()));
        } else if acts.reopen {
            self.reopen_edit(Some(ctx.clone()));
        } else if acts.retry_open {
            if let Some(r) = self.edit_row.clone() {
                let at = self.row_path(&r);
                self.edit_refused = None;
                self.begin_edit_at(r, at, None, Some(ctx.clone()));
            }
        } else if acts.pull {
            if let Some(r) = self.edit_row.clone() {
                self.begin_pull_row(r);
            }
        }
    }

    /// 编辑页那一份的路径（开着的 / 在读的 / 那一行的）；不是编辑页 ⇒ `None`。
    pub fn edit_path(&self) -> Option<String> {
        self.editing
            .as_ref()
            .map(|e| e.path.clone())
            .or_else(|| self.edits.opening())
            .or_else(|| self.edit_row.as_ref().map(|r| r.path.clone()))
    }

    /// 编辑页标签上（与窗口标题里）那个名字。
    pub fn edit_name(&self) -> String {
        match &self.editing {
            Some(e) => e.name.clone(),
            None => self
                .edit_path()
                .map(|p| crate::source::remote_basename(&p).to_string())
                .unwrap_or_default(),
        }
    }

    /// 编辑页在前台时状态栏那一行（底条）：「行 12 · 列 5 · UTF-8 · LF · 41.6 KB / 8 MB」；
    /// 大文件模式左端多「大文件模式 · 长行不换行 · 无查找替换」。超上限那一句另回（画在右端，`--error`）。
    pub fn editor_status(&self, ctx: &egui::Context) -> Option<(String, Option<String>)> {
        let e = self.editing.as_ref()?;
        let sep = copy_text("rsFilewinProgress.detail.sep", &[]);
        let mut parts = Vec::new();
        if e.big.is_big() {
            parts.push(copy_text("rsFilewinShell.editor.findBig", &[]));
        } else {
            let at = egui::TextEdit::load_state(ctx, crate::bigfile::normal_editor_id(&e.path))
                .and_then(|s| s.cursor.char_range())
                .map_or(0, |r| r.primary.index.0);
            let (line, col) = crate::editor::line_col(&e.text, at);
            parts.push(copy_text(
                "rsFilewinEditPage.foot.at",
                &[("line", &line.to_string()), ("col", &col.to_string())],
            ));
        }
        parts.push("UTF-8".to_string());
        parts.push(
            if e.text.contains("\r\n") {
                "CRLF"
            } else {
                "LF"
            }
            .to_string(),
        );
        parts.push(copy_text(
            "rsFilewinEditPage.foot.size",
            &[
                ("size", &copy_core::size_text(e.text.len() as u64)),
                ("cap", &copy_core::size_text(e.cap)),
            ],
        ));
        let over = e.over_cap().then(|| {
            copy_text(
                "rsFilewinEditPage.foot.overCap",
                &[("n", &(-e.headroom()).to_string())],
            )
        });
        Some((parts.join(&sep), over))
    }
}

/// 编辑面查找栏上按了哪一颗。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FindAct {
    Next,
    Prev,
    Replace,
    ReplaceAll,
    Close,
}

/// 查找框的 egui id（Ctrl+F 把焦点给它）。
pub(super) const FIND_ID: &str = "filewin-editor-find";

/// 编辑面那一截查找替换：查找框 · 上一个 · 下一个 · 替换框 · 替换 · 全部替换 · 上一下那句话。
/// 在查找框里按回车 ＝「下一个」。回这一帧按了哪一颗。
pub(super) fn find_row(
    ui: &mut egui::Ui,
    f: &mut crate::editor::FindBar,
    count: Option<(usize, usize)>,
) -> Option<FindAct> {
    use egui_phosphor::regular as ph;
    let mut act = None;
    let p = crate::theme::palette(ui.ctx());
    // 第一行：查找框（框内左侧放大镜、右侧「3 / 12」）· ↑ ↓ · ×；第二行：替换为 · 替换 · 全部替换。
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut f.needle)
                .id(egui::Id::new(FIND_ID))
                .hint_text(format!(
                    "{} {}",
                    ph::MAGNIFYING_GLASS,
                    copy_text("rsFilewinShell.editor.findLabel", &[])
                ))
                .desired_width(180.0),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            act = Some(FindAct::Next);
        }
        if let Some((k, n)) = count {
            ui.label(
                egui::RichText::new(copy_text(
                    "rsFilewinEditPage.find.count",
                    &[("k", &k.to_string()), ("n", &n.to_string())],
                ))
                .small()
                .color(if n == 0 { p.error } else { p.text2 }),
            );
        }
        let icon = |ui: &mut egui::Ui, glyph: &str, hint: String| {
            ui.add(egui::Button::new(glyph).frame_when_inactive(false))
                .on_hover_text(hint)
                .clicked()
        };
        if icon(
            ui,
            ph::CARET_UP,
            copy_text("rsFilewinShell.editor.findPrev", &[]),
        ) {
            act = Some(FindAct::Prev);
        }
        if icon(
            ui,
            ph::CARET_DOWN,
            copy_text("rsFilewinShell.editor.findNext", &[]),
        ) {
            act = Some(FindAct::Next);
        }
        if icon(ui, ph::X, copy_text("rsFilewinEditPage.find.close", &[])) {
            act = Some(FindAct::Close);
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut f.with)
                .hint_text(copy_text("rsFilewinShell.editor.replaceLabel", &[]))
                .desired_width(180.0),
        );
        if ui
            .button(copy_text("rsFilewinShell.editor.replaceOne", &[]))
            .clicked()
        {
            act = Some(FindAct::Replace);
        }
        if ui
            .button(copy_text("rsFilewinShell.editor.replaceAll", &[]))
            .clicked()
        {
            act = Some(FindAct::ReplaceAll);
        }
    });
    if let Some(n) = &f.notice {
        ui.label(egui::RichText::new(n).small().color(p.text2));
    }
    act
}

/// 编辑页这一帧的事实（画头条与那几条要用）。
pub(super) struct EditFacts {
    pane: Option<crate::editor::Pane>,
    path: String,
    name: String,
    idle: bool,
    dirty: bool,
    over: bool,
    big: bool,
}

/// 编辑页这一帧点下的动作（画完统一做）。
#[derive(Default)]
pub(super) struct EditActs {
    save: bool,
    overwrite: bool,
    reopen: bool,
    retry_open: bool,
    pull: bool,
    find_toggle: bool,
}
