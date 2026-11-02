//! 新建目录 · 改名 · 删除 · 改权限接到窗口上：一次问完才动手（[`crate::writeops::run_writes`]）· 就地那一格 · 撤销 · 做完的回执。

use super::*;

impl FileWindow {
    // ════════════════════════════════════════════════════════════════════
    // 🔴：**新建目录 · 改名 · 删除 · 改权限**
    // ════════════════════════════════════════════════════════════════════

    /// 摆出「新建目录」那个框。回值 = 真的摆出来了。
    ///
    /// 🔴 四个写入口（新建目录 · 改名 · 删除 · 权限）都没有「窗口看着本机 ⇒ 出声拒」那道闸：
    /// `Source` 是 newtype，那个状态**写不出来**（逐条理由住 `crate::source::Source` 头注）。
    pub fn begin_mkdir(&mut self) -> bool {
        self.begin_inline(WritePrompt::for_new(&self.cwd, false))
    }

    /// 摆出就地那一格（改名 · 新建目录 · 新建空文件）：下一帧焦点给它、选中主名 / 整个名字。
    pub(crate) fn begin_inline(&mut self, p: WritePrompt) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        self.inline_select = Some(p.select_chars());
        self.inline_failed_text = None;
        self.inline_busy = false;
        self.inline_gen += 1;
        self.write_prompt = Some(p);
        true
    }

    /// 摆出「把第 `i` 行改名为」那个框。回值 = 真的摆出来了。
    ///
    /// ⚠ **两道闸，刻意重复**（同 [`Self::begin_copy`] 那一条逐字的理由）：
    /// `crate::writeops::is_writable` —— 有损名一律不接。列表上那一档压根不画
    /// 那颗按钮，这里是第二道，防的是「按钮没了、调用还在」。
    pub fn begin_rename(&mut self, i: usize) -> bool {
        let Some(row) = self.writable_row(i) else {
            return false;
        };
        self.begin_inline(WritePrompt::for_rename(&self.cwd, &row))
    }

    /// 摆出「把这几行的权限改成」那个框（一行与多行同一个框）。回值 = 真的摆出来了。
    /// 有一行不能写 ⇒ **整摞不摆**（同 [`Self::delete_picked`] 那条理由）。
    pub fn begin_chmod_rows(&mut self, idx: &[usize]) -> bool {
        let Some(rows) = idx
            .iter()
            .map(|&i| self.writable_row(i))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let refs: Vec<&crate::source::Listed> = rows.iter().collect();
        *self.prompt_error.lock().unwrap() = None;
        self.write_prompt = Some(WritePrompt::for_chmod_many(&self.cwd, &refs));
        // 框一摆出来就逐项问现值（`files-stat` 的 `mode`）；答回来之后框上说、只预填一次。
        let paths: Vec<String> = rows.iter().map(|r| r.path.clone()).collect();
        self.start_mode_probe(paths);
        !refs.is_empty()
    }

    /// 起「现值」那一趟。没有运行时 / 没有通道 ⇒ 当场落「全读不到」（框上说「读不到现在的权限」，不猜、不静默）。
    pub(super) fn start_mode_probe(&mut self, paths: Vec<String>) {
        let gen = self.mode_probe.start();
        let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) else {
            self.mode_probe.land(gen, vec![None; paths.len()]);
            return;
        };
        let board = self.mode_probe.clone();
        let origin = self.source.origin();
        h.spawn(async move {
            let modes = crate::writeops::probe_modes(&line, &origin, &paths).await;
            board.land(gen, modes);
        });
    }

    /// 第 `i` 行，且它**能被写**。`None` ⇒ 不接（越界 / 有损名）。
    ///
    /// ⚠ 没有「本机源」那一档（同 [`Self::begin_mkdir`] 那条）：那个状态写不出来。
    pub(super) fn writable_row(&mut self, i: usize) -> Option<crate::source::Listed> {
        let rows = self.listing.rows.lock().unwrap();
        match rows.get(i) {
            // 交出去的是**整个 `Listed`**：此前只交那五格（链接与时间写操作不读），
            //   现在写操作要读 `raw_name`（有损名的原始字节，改名 · 删除 · 改权限都走它）。
            Some(r) if is_writable(r) => Some(r.clone()),
            _ => None,
        }
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_write(&mut self) {
        self.write_prompt = None;
        self.inline_failed_text = None;
        self.inline_busy = false;
        self.inline_gen += 1;
        *self.prompt_error.lock().unwrap() = None;
    }

    /// 框里那几个字 → 一摞真操作。回值 = 真的起来了。
    ///
    /// ⚠ 输入不合法 ⇒ **框留着、出声**，不静默收掉（同 [`Self::confirm_copy`]：
    /// 收掉的话用户点了确认什么都没发生，与成功长得一模一样）。
    pub fn confirm_write(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.write_prompt.clone() else {
            return false;
        };
        if p.is_inline() {
            return self.commit_inline(ctx);
        }
        // 批量改权限那个框一次出 N 件 ⇒ `to_ops`（新建目录 / 改名恒一件）。
        let ops = match p.to_ops() {
            Ok(ops) => ops,
            Err(why) => {
                *self.say_slot() = Some(why.into());
                return false;
            }
        };
        if !self.start_writes(ops, ctx) {
            return false;
        }
        self.write_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
        true
    }

    /// 就地那一格答完（回车 / 点别处）⇒ 发那一趟；那一格留着、只读，回来了再收（[`Self::settle_inline`]）。
    /// 填错 ⇒ 那一格下面说一句、留着；改名改成原名 ⇒ 当没改（不报错）。回值 ＝ 真的发出去了。
    pub fn commit_inline(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.write_prompt.clone() else {
            return false;
        };
        if self.inline_busy {
            return false;
        }
        let go = match p.to_inline() {
            Ok(Some(go)) => go,
            Ok(None) => {
                self.cancel_write();
                return false;
            }
            Err(why) => {
                *self.prompt_error.lock().unwrap() = Some(why.into());
                self.inline_select = Some(p.select_chars());
                self.inline_failed_text = Some(p.text.clone());
                return false;
            }
        };
        let Some(h) = self.rt.clone() else {
            *self.prompt_error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.writes.noRuntime", &[]).into());
            self.inline_failed_text = Some(p.text.clone());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.prompt_error.lock().unwrap() = Some(NO_LINE.to_string().into());
            self.inline_failed_text = Some(p.text.clone());
            return false;
        };
        *self.prompt_error.lock().unwrap() = None;
        self.inline_busy = true;
        self.inline_gen += 1;
        let gen = self.inline_gen;
        let origin = self.source.origin();
        let root_raw = self.cwd_raw.clone();
        let cwd = self.cwd_path();
        let slot = self.inline_done.clone();
        let name = crate::writeops::clean_name(&p.text);
        h.spawn(async move {
            let got = match go {
                crate::writeops::InlineGo::Op(op) => {
                    crate::writeops::apply_remote_coded(&line, &origin, &op, root_raw.as_deref())
                        .await
                        .map(|_| ())
                }
                crate::writeops::InlineGo::Create => {
                    crate::create::create_typed_coded(&line, &origin, &cwd, &name).await
                }
            };
            *slot.lock().unwrap() = Some((gen, got.map(|()| name)));
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        true
    }

    /// 就地那一趟回来了：成了 ⇒ 收那一格、重列、选中新名字（改名另给带［撤销］的回执）；
    /// 名字被占了 ⇒ 那一格下面说「x 已存在」、焦点回去；别的没成 ⇒ 那一格下面说那句话。回值 ＝ 收到了。
    pub fn settle_inline(&mut self) -> bool {
        let Some((gen, got)) = self.inline_done.lock().unwrap().take() else {
            return false;
        };
        if gen != self.inline_gen {
            return false;
        }
        self.inline_busy = false;
        match got {
            Ok(name) => {
                if let Some(p) = self.write_prompt.take() {
                    if let (PromptKind::Rename { from, raw: None }, None) = (&p.kind, &self.cwd_raw)
                    {
                        self.receipt_undo = Some((
                            copy_text("rsFilewinWriteops.inline.renamed", &[("name", &name)]),
                            vec![WriteOp::Rename {
                                from: crate::source::child_of(&p.dir, &name),
                                name: crate::source::remote_basename(from).to_string(),
                                raw: None,
                            }],
                        ));
                    }
                }
                *self.prompt_error.lock().unwrap() = None;
                self.inline_failed_text = None;
                self.selection.clear_picked();
                self.set_reveal(&name);
                self.reload();
            }
            Err(f) => {
                let why = if f.code == crate::writeops::EXISTS {
                    let name = self
                        .write_prompt
                        .as_ref()
                        .map(|p| p.text.trim().to_string())
                        .unwrap_or_default();
                    copy_text("addMachine.err.taken", &[("name", &name)]).into()
                } else {
                    f
                };
                *self.prompt_error.lock().unwrap() = Some(why);
                self.inline_select = self.write_prompt.as_ref().map(WritePrompt::select_chars);
                self.inline_failed_text = self.write_prompt.as_ref().map(|p| p.text.clone());
            }
        }
        true
    }

    /// 回执上点了［撤销］⇒ 那几件直接做（不问、不再出回执），做完重列。
    pub fn start_undo(&mut self, ops: Vec<WriteOp>, ctx: Option<egui::Context>) -> bool {
        self.start_writes_as(ops, ctx, true)
    }

    /// 起一摞 `§4.6.4`：**一次问完，才动手。**（不再先过围栏）
    ///
    /// 🔴 三段的顺序不在这里，在 [`crate::writeops::run_writes`] 的结构里 ——
    /// 这里只负责把「怎么问 · 怎么做」两个口接上去（同 [`Self::start_drop`]）。
    /// 而本函数接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    pub fn start_writes(&mut self, ops: Vec<WriteOp>, ctx: Option<egui::Context>) -> bool {
        self.start_writes_as(ops, ctx, false)
    }

    /// 同 [`Self::start_writes`]；`quiet` ⇒ 这一摞是撤销本身（做完不再出回执）。
    pub(super) fn start_writes_as(
        &mut self,
        ops: Vec<WriteOp>,
        ctx: Option<egui::Context>,
        quiet: bool,
    ) -> bool {
        if ops.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.writes.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string().into());
            return false;
        };
        let origin = self.source.origin();
        let board = self.write_board.clone();
        board.attach(ctx);
        // 有文件夹要删 ⇒ 「进度」里一行（删大目录要一会儿；那一行撤得了）：确认之后、发那一组时由 `apply_all` 开。
        let (progress, dir, machine) =
            (self.progress.clone(), self.cwd.clone(), self.source.label());
        self.writes_started += 1;
        // 〔有损名全寻址〕写的对象恒是当前目录的直接子项 ⇒ 根就是当前目录；有损 ⇒ 根发字节。
        // 撤销那一摞带的是整条路径（字符串）⇒ 根照路径切，不借这一栏的字节根；当前目录是有损名 ⇒ 不给撤销（字符串寻址不到）。
        let root_raw = if quiet { None } else { self.cwd_raw.clone() };
        let undoable = root_raw.is_none();
        h.spawn(async move {
            let ask_board = board.clone();
            // 做成了的那几件 ＋ 撤销它们的那几件（改权限：后端回的 `before`）—— 回执据它说、［撤销］据它做。
            let mut done = Vec::new();
            let mut undo = Vec::new();
            let kept = std::sync::Mutex::new((&mut done, &mut undo));
            let out = crate::writeops::run_writes(
                ops,
                move |asking| {
                    let rx = ask_board.ask(asking);
                    async move { rx.await.unwrap_or_default() }
                },
                |go| async {
                    let all = crate::writeops::apply_all(
                        &line,
                        &origin,
                        go,
                        root_raw.as_deref(),
                        &progress,
                        &dir,
                        &machine,
                    )
                    .await;
                    let mut k = kept.lock().unwrap();
                    let mut applied = Vec::new();
                    let out = all
                        .into_iter()
                        .map(|(op, r)| {
                            let r = r.map(|reply| {
                                k.0.push(op.clone());
                                applied.push((op.clone(), reply));
                            });
                            (op, r)
                        })
                        .collect();
                    if undoable {
                        k.1.extend(crate::writeops::chmod_undo(&applied));
                    }
                    out
                },
            )
            .await;
            drop(kept);
            board.finish_with(out, done, undo, quiet);
        });
        true
    }

    /// 一摞写操作跑完就重列当前目录（新目录要出现、删掉的要消失）。
    /// 同 [`Self::settle_trips`]。
    pub fn settle_finished_writes(&mut self) -> bool {
        let now = self.write_board.rounds();
        if now == self.seen_write_rounds {
            return false;
        }
        self.seen_write_rounds = now;
        // 做完的回执（删除 · 改权限 ＋［撤销］· 没做成的那一句）⇒ 窗口那一级摆成右下角回执。
        if let Some((text, undo, copy)) = self.write_board.take_receipt() {
            if undo.is_empty() {
                self.receipt = Some(text);
                self.receipt_copy = copy;
            } else {
                self.receipt_undo = Some((text, undo));
            }
        }
        // 删掉 / 改了名的那几个名字已经不在了 ⇒ 选中清掉（光标留着，理由住 `Selection::clear_picked`）。
        self.selection.clear_picked();
        self.reload();
        true
    }

    /// 画「叫什么名字 / 改成什么权限」那个框。**模态** —— 定下来之前不接别的。
    ///
    /// ⚠ 与 [`crate::writeops::WriteBoard::ui`] 分开两处，因为它们的状态住在两个地方：
    /// 这个框是 UI 线程自己的，那一摞（确认 / 结果）是跨线程的。
    pub(super) fn write_ui(&mut self, ui: &mut egui::Ui) {
        // 就地那一格（改名 · 新建）画在列表那一行里（`frame_body`），不在这里。
        let Some(mut p) = self.write_prompt.clone().filter(|p| !p.is_inline()) else {
            return;
        };
        // 改权限：标题「改权限 · deploy.sh」·「当前 644」· 3 × 3 勾 ·「数字写法」两边联动 · ［取消］［改权限］。
        // 现值答回来了 ⇒ 九格与那一栏预填至多一次（`writeops::apply_prefill`）。
        self.mode_probe.attach(ui.ctx().clone());
        let readout = self.mode_probe.readout();
        if let Some(r) = &readout {
            crate::writeops::apply_prefill(&mut p, r);
        }
        let pal = crate::theme::palette(ui.ctx());
        let err = self.prompt_failure();
        let mut typed = false;
        let mut toggled: Option<usize> = None;
        let mut enter = false;
        let heads = [
            copy_text("rsFilewinWriteops.chmod.read", &[]),
            copy_text("rsFilewinWriteops.chmod.write", &[]),
            copy_text("plan.new.line", &[]),
        ];
        let who = [
            copy_text("rsFilewinProps.label.owner", &[]),
            copy_text("rsFilewinWriteops.chmod.group", &[]),
            copy_text("rsFilewinWriteops.chmod.other", &[]),
        ];
        let hit = crate::kit::dialog(
            ui.ctx(),
            "filewin-write-prompt",
            &p.heading(),
            |ui| {
                let now = match &readout {
                    Some(r) => r.line.clone(),
                    None => copy_text("rsFilewinShell.mode.reading", &[]),
                };
                ui.label(egui::RichText::new(now).color(pal.text2));
                ui.add_space(12.0);
                egui::Grid::new("filewin-chmod-grid")
                    .spacing(egui::vec2(36.0, 6.0))
                    .show(ui, |ui| {
                        ui.label("");
                        for h in &heads {
                            ui.label(egui::RichText::new(h).color(pal.text2));
                        }
                        ui.end_row();
                        for (r, w) in who.iter().enumerate() {
                            ui.label(w);
                            for c in 0..3 {
                                let k = r * 3 + c;
                                let r = match p.grid[k] {
                                    Some(on) => {
                                        let mut on = on;
                                        ui.checkbox(&mut on, "")
                                    }
                                    // 「—」：多选时各项这一位不一样 / 读不到 ⇒ 不改这一位，点一下才定。
                                    None => ui.add(
                                        egui::Checkbox::without_text(&mut false)
                                            .indeterminate(true),
                                    ),
                                };
                                if r.clicked() {
                                    toggled = Some(k);
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(copy_text("rsFilewinWriteops.chmod.octal", &[]));
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut p.text)
                            .desired_width(88.0)
                            .font(egui::TextStyle::Monospace),
                    );
                    typed = r.changed();
                    enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                if let Some(f) = &err {
                    let id = ui.id().with("prompt-fail-detail");
                    crate::kit::failure_line(ui, &f.said, &f.detail, id);
                }
            },
            &[
                (
                    copy_text("kit.dialog.cancel", &[]),
                    crate::kit::Btn::Plain,
                ),
                (
                    copy_text("beFiles.batch.verbChmod", &[]),
                    crate::kit::Btn::Primary,
                ),
            ],
            1,
            0,
        );
        if let Some(k) = toggled {
            p.toggle_bit(k);
        }
        if typed {
            p.typed();
        }
        self.write_prompt = Some(p);
        match hit {
            Some(0) => self.cancel_write(),
            Some(_) => {
                let ctx = ui.ctx().clone();
                self.confirm_write(Some(ctx));
            }
            None if enter => {
                let ctx = ui.ctx().clone();
                self.confirm_write(Some(ctx));
            }
            None => {}
        }
    }
}
