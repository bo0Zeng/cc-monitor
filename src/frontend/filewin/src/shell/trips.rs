//! 起一趟、收一趟：上传 · 复制为 · 一组（复制到另一栏 · 删除 · 改权限 · 下载）· 解压 · 算大小。
//! 每一趟是「进度」表里一行（[`crate::progress`]），结局只在那一行判一次；这个标签页只记号、照那一个结局收场（[`FileWindow::settle_trips`]）。

use super::*;

impl FileWindow {
    /// 拖进来（选到）的那几个本机文件 ⇒ 一组上传到当前目录：一条寻址本机的 `transfer-copy`（[`crate::transfer::upload_plan`]），
    /// 同名由本机后端在落点那台探、冲突框问一次。有损目录里也传（落点按目录的字节）。
    /// 一份能放的都没有（拿不到末段名）· 接不上（**没运行时** · 没通道）⇒ 不起、后者出声。
    pub fn start_drop(&mut self, local_paths: &[String], ctx: Option<egui::Context>) -> bool {
        let Some(plan) = crate::transfer::upload_plan(
            &self.source.origin(),
            self.source.label(),
            local_paths,
            &self.cwd,
            self.cwd_raw.as_deref(),
        ) else {
            return false;
        };
        self.start_batch(
            plan,
            crate::source::Origin::local(),
            egui_phosphor::regular::UPLOAD_SIMPLE,
            ctx,
        )
    }

    /// 把这一帧 egui 收到的拖入文件接走。
    ///
    /// ⚠ **上一问还没答完就不接新的** —— 两摞问题叠在一个模态框上，
    /// 「一次问完」就变成「两次问完」。
    pub(super) fn take_drops(&mut self, ctx: &egui::Context) {
        // ⚠复制那一摞也在问的时候同样不接 —— 两个模态框叠起来，
        //   「一次问完」就变成「答错了哪一个都不知道」。
        // ⚠写操作那一摞同理，而它的代价更大：那个框上「都别做」与
        //   上传那个框上「全都不覆盖」叠在一起，答错一个就是删错东西。
        // 不是焦点那一个 ⇒ 不接（两栏都接的话，拖一个文件进来两栏各传一次）。
        if !self.focused || self.modal_up() {
            return;
        }
        let dropped: Vec<String> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_string_lossy().to_string())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        if dropped
            .iter()
            .all(|p| std::path::Path::new(p).file_name().is_none())
        {
            *self.listing.error.lock().unwrap() = Some(
                copy_text(
                    "rsFilewinShell.drop.noNames",
                    &[("n", &(dropped.len()).to_string())],
                )
                .into(),
            );
            return;
        }
        self.start_drop(&dropped, Some(ctx.clone()));
    }

    /// 开一趟：「进度」表里一行，记进这个标签页起的那几趟（收场由 [`Self::settle_trips`] 照它的结局做）。回那一行的号。
    pub fn adopt_trip(
        &mut self,
        trip: crate::progress::Trip,
        dir: Option<String>,
        no_stop: Option<String>,
    ) -> u64 {
        let id = self.progress.add(trip, dir, no_stop);
        self.trips.push(id);
        id
    }

    /// 🔴 **这个标签页起的那几趟，落地一趟收一趟** —— 只照那一行的结局（[`crate::progress::Job::landing`]）做，不另判：
    /// 成了带回执的 ⇒ 右下角一句；落在一个目录上的 ⇒ 重列（新文件要出现 · 删掉的要消失）；一组 ⇒ 选中清掉。
    /// 那一问上答了「取消」/ 已交出去的（[`crate::progress::Job::dropped`]）· 表里已经没有了的 ⇒ 不收场，只不再记。
    /// 回值 ＝ 这一帧有一趟收了场。**放在 UI 线程**（理由见 [`Self::trips`]）。
    pub fn settle_trips(&mut self) -> bool {
        if self.trips.is_empty() {
            return false;
        }
        let jobs = self.progress.jobs();
        let (mut landed, mut reload, mut unpick) = (false, false, false);
        self.trips.retain(|id| {
            let Some(j) = jobs.iter().find(|j| j.id == *id) else {
                return false;
            };
            if j.dropped() {
                return false;
            }
            let Some(l) = j.landing() else {
                return true;
            };
            if let Some(t) = l.receipt {
                self.receipt = Some(t);
            }
            reload |= j.dir.is_some();
            unpick |= matches!(j.trip, crate::progress::Trip::Batch { .. });
            landed = true;
            false
        });
        if unpick {
            self.selection.clear_picked();
        }
        if reload {
            self.reload();
        }
        landed
    }

    /// 这个标签页起的、还没收场的那几趟（表里那几行，判据与画那几问用）。
    pub fn my_trips(&self) -> Vec<crate::progress::Job> {
        let mut jobs = self.progress.jobs();
        jobs.retain(|j| self.trips.contains(&j.id));
        jobs
    }

    // ════════════════════════════════════════════════════════════════════
    // 第二段：**零流量复制**接到这一侧
    // ════════════════════════════════════════════════════════════════════

    /// 摆出「把第 `i` 行复制成什么名字」那个框。回值 = 真的摆出来了。
    ///
    /// 能不能复制只经 [`Self::perform`] 进来时判过了（本机后端 `files-menu` 成品里「复制」那一件）。
    ///
    /// 🔴 **没有「远端才有」那道闸**：`copy-data` 是 SFTP 协议的扩展，而窗口**只可能**看着一台远端
    /// （`Source` 是 newtype）⇒ 本机源这个状态写不出来。
    pub fn begin_copy(&mut self, i: usize) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        let row = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                Some(r) => r.clone(),
                _ => return false,
            }
        };
        let mut p = CopyPrompt::for_row(&self.cwd, &row);
        // 〔有损名全寻址〕源与当前目录的字节跟着框走（合法 UTF-8 ⇒ 两格都是 `None`，与此前逐字节同）。
        p.from_raw = self.row_path(&row).raw;
        p.dir_raw = self.cwd_raw.clone();
        self.copy_prompt = Some(p);
        true
    }

    /// 收掉那个框，什么都不做。
    pub fn cancel_copy(&mut self) {
        self.copy_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
    }

    /// 框里那个名字 → 一趟真复制（名字由那台判，不合的那一句照登在「进度」那一行）。回值 = 真的起来了。
    pub fn confirm_copy(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.copy_prompt.clone() else {
            return false;
        };
        if !self.start_copy_req(p.to_req(), vec![p.new_name.trim().to_string()], ctx) {
            return false;
        }
        self.copy_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
        true
    }

    /// **一组复制到这一栏**（「复制到另一栏」的按钮 · 菜单 · 拖，`Workspace::copy_to_other` 起在目标那一栏上）：
    /// 一条 `files-copy`（带票），同名由后端探、回 `exists` 就画冲突框（[`crate::batch`]）。参数是跨目录那一形（[`crate::workspace::across_req`]）。
    pub fn start_copy_batch(&mut self, jobs: Vec<CopyJob>, ctx: Option<egui::Context>) -> bool {
        let req = match crate::workspace::across_req(&jobs) {
            Ok(r) => r,
            Err(e) => {
                *self.say_slot() = Some(e.into());
                return false;
            }
        };
        self.start_copy_req(req, jobs.iter().map(|j| j.name.clone()).collect(), ctx)
    }

    pub(super) fn start_copy_req(
        &mut self,
        req: cmd_wire::files::FilesCopyReq,
        names: Vec<String>,
        ctx: Option<egui::Context>,
    ) -> bool {
        let plan = batch_core::Plan {
            verb: batch_core::Verb::Copy,
            cmd: crate::copy::CMD_COPY,
            args: serde_json::to_value(&req).unwrap_or_default(),
            names,
            into: Some(self.cwd.clone()),
            machine: self.source.label(),
        };
        let at = self.source.origin();
        self.start_batch(plan, at, egui_phosphor::regular::COPY, ctx)
    }

    /// 起一组（[`crate::batch::spawn`]）：「进度」表里一行（问的时候不画）、冲突框挂在这个标签页上、落地后重列目录。
    /// `home` ＝ 那一组在哪台的后端里做（撤与断开都认它）。接不上（没运行时 · 没通道）⇒ 出声。
    pub fn start_batch(
        &mut self,
        plan: batch_core::Plan,
        home: crate::source::Origin,
        icon: &'static str,
        ctx: Option<egui::Context>,
    ) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.copy.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string().into());
            return false;
        };
        // 落在这一台当前目录上的 ⇒ 落地后重列它；下载落在本机，这一栏不用重列。
        let dir = (plan.verb != batch_core::Verb::Download).then(|| self.cwd.clone());
        let board = crate::batch::BatchBoard::new(plan);
        board.attach(ctx);
        self.adopt_trip(
            crate::progress::Trip::Batch {
                board: board.clone(),
                icon,
            },
            dir,
            None,
        );
        crate::batch::spawn(&h, line, self.source.origin(), home, board);
        true
    }

    /// 第 `i` 行那份文件「解压到这里」：发 `files-extract`，撞名就问（[`crate::extract::run`]）。
    /// 接不上（没运行时 / 没通道）⇒ 出声。回值 ＝ 真的起来了。
    pub fn start_extract(&mut self, i: usize, ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.size.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string().into());
            return false;
        };
        let Some((rel, name)) = self
            .listing
            .rows
            .lock()
            .unwrap()
            .get(i)
            .filter(|r| !r.opens_as_dir())
            .map(|r| (cmd_wire::files::WirePath(name_bytes(r)), r.name.clone()))
        else {
            return false;
        };
        let root = self.cwd_path().path();
        let origin = self.source.origin();
        // 一趟一块新看板（「进度」表里独立的一行）。
        let board = crate::extract::ExtractBoard::default();
        board.attach(ctx);
        board.begin(&name);
        let no_stop = self.no_stop(crate::extract::CMD_EXTRACT);
        self.adopt_trip(
            crate::progress::Trip::Extract {
                board: board.clone(),
                name: name.clone(),
            },
            Some(self.cwd.clone()),
            no_stop,
        );
        h.spawn(async move {
            let asker = board.clone();
            let o = crate::extract::run(&line, &origin, &root, &rel, |said| async move {
                asker.ask(said).await.unwrap_or(false)
            })
            .await;
            board.finish(&name, o);
        });
        true
    }

    /// 画「复制为」那个框。**模态** —— 名字没定下来之前不接别的。
    pub(super) fn copy_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut p) = self.copy_prompt.clone() else {
            return;
        };
        let mut go = false;
        let title = copy_text(
            "rsFilewinShell.copyUi.heading",
            &[("name", &p.src_name.to_string())],
        );
        let hit = crate::kit::dialog(
            ui.ctx(),
            "filewin-copy-as",
            &title,
            |ui| {
                go |= crate::kit::dialog_field(ui, &mut p.new_name);
                self.prompt_error_ui(ui);
                ui.label(copy_text("rsFilewinShell.copyUi.sameDirOnly", &[]));
            },
            &[
                (copy_text("kit.dialog.cancel", &[]), crate::kit::Btn::Plain),
                (
                    crate::copy::COPY_LABEL.to_string(),
                    crate::kit::Btn::Primary,
                ),
            ],
            1,
            0,
        );
        go |= hit == Some(1);
        let cancel = hit == Some(0);
        self.copy_prompt = Some(p);
        if cancel {
            self.cancel_copy();
        } else if go {
            let ctx = ui.ctx().clone();
            self.confirm_copy(Some(ctx));
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // 🔴往外拖：**两问，然后拉**
    // ═══════════════════════════════════════════════════════════════════

    /// 〔「原生选文件框」〕起一趟原生选择框（在 tokio 那条线程上，不堵 UI 线程）。
    /// 上传 ⇒ 选一个或几个文件；下载 ⇒ 选保存位置（建议名 ＝ 那一问里现在的名字）。接不上（没运行时）⇒ 说一句。回值 ＝ 真的起了。
    pub fn start_pick(
        &mut self,
        purpose: crate::picker::Purpose,
        ctx: Option<egui::Context>,
    ) -> bool {
        use crate::picker::{PickKind, Purpose};
        let kind = match purpose {
            Purpose::Upload => PickKind::OpenFiles,
            Purpose::DownloadMany => PickKind::Folder,
            // 建议名是那一项显示的名字（没改它 ⇒ 落名由本机后端照源那一段定，落本机那一段改成这台认的合法名）。
            Purpose::Download => PickKind::SaveFile {
                suggested_name: self
                    .pull_want
                    .as_ref()
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default(),
            },
        };
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinPicker.pick.noRuntime", &[]).into());
            return false;
        };
        let board = self.pick_board.clone();
        board.attach(ctx);
        let picker = self.picker.clone();
        h.spawn(async move {
            let got = picker.pick(kind).await;
            board.deliver(purpose, got);
        });
        true
    }

    /// 选择框的结局：上传 ⇒ 选到的那几份走拖入那一条；下载 ⇒ 起那一趟（覆盖由系统存盘框自己问过了）；没选（取消）⇒ 什么都不做。
    pub fn settle_pick_with(&mut self, ctx: Option<egui::Context>) -> bool {
        use crate::picker::Purpose;
        let Some((purpose, picked)) = self.pick_board.take() else {
            return false;
        };
        match purpose {
            Purpose::Upload => {
                let paths: Vec<String> = picked
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                if !paths.is_empty() {
                    self.start_drop(&paths, ctx);
                }
            }
            Purpose::DownloadMany => match picked.and_then(|v| v.into_iter().next()) {
                Some(dir) => {
                    self.start_pull_many(&dir.to_string_lossy(), ctx);
                }
                None => self.pull_many.clear(),
            },
            Purpose::Download => {
                let want = self.pull_want.take();
                if let (Some((src, shown)), Some(dest)) =
                    (want, picked.and_then(|v| v.into_iter().next()))
                {
                    self.start_pull(src, &shown, &dest.to_string_lossy(), ctx);
                }
            }
        }
        true
    }

    /// 下载多项：先选一个本机目录（系统的选目录框），选好了一组一条（[`crate::download::many_plan`]）。
    /// 只收文件：只经 [`Self::perform`] 进来，那里照 `files-menu` 成品判过了。
    pub fn begin_pull_many(&mut self, idx: &[usize]) -> bool {
        let rows: Vec<Listed> = {
            let all = self.listing.rows.lock().unwrap();
            idx.iter().filter_map(|&i| all.get(i).cloned()).collect()
        };
        if rows.is_empty() {
            return false;
        }
        self.pull_many = rows;
        self.start_pick(crate::picker::Purpose::DownloadMany, None)
    }

    /// 选好了本机目录 ⇒ 起那一组（这一台 → 本机，同名问）。
    pub fn start_pull_many(&mut self, dir: &str, ctx: Option<egui::Context>) -> bool {
        let rows = std::mem::take(&mut self.pull_many);
        let rows: Vec<(cmd_wire::files::WirePath, String)> = rows
            .iter()
            .map(|r| (self.row_path(r).path(), r.name.clone()))
            .collect();
        let plan = crate::download::many_plan(&self.source.origin(), rows, dir);
        self.start_batch(
            plan,
            crate::source::Origin::local(),
            egui_phosphor::regular::DOWNLOAD_SIMPLE,
            ctx,
        )
    }

    pub fn begin_pull(&mut self, i: usize) -> bool {
        let Some(row) = self.listing.rows.lock().unwrap().get(i).cloned() else {
            return false;
        };
        self.begin_pull_row(row)
    }

    /// 同 [`Self::begin_pull`]，那一行直接给（编辑页「下载到本机」走这里：它手上没有列表）。
    /// 有损名带着字节 / 有损目录里的行也下得了（路径一路是原始字节）。
    pub fn begin_pull_row(&mut self, row: Listed) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        self.pull_want = Some((self.row_path(&row).bytes(), row.name.clone()));
        self.start_pick(crate::picker::Purpose::Download, None)
    }

    /// 存盘框给了落点 ⇒ 起那一组（这一台 → 本机，一项；存盘框问过盖不盖 ⇒ 同名照盖）。落点拆不出目录与名字 ⇒ 出声。
    pub fn start_pull(
        &mut self,
        src: Vec<u8>,
        shown: &str,
        dest: &str,
        ctx: Option<egui::Context>,
    ) -> bool {
        let Some(plan) = crate::download::one_plan(
            &self.source.origin(),
            cmd_wire::files::WirePath(src),
            shown,
            dest,
        ) else {
            *self.say_slot() = Some(copy_text("rsFilewinLossyPull.local.badDest", &[]).into());
            return false;
        };
        self.start_batch(
            plan,
            crate::source::Origin::local(),
            egui_phosphor::regular::DOWNLOAD_SIMPLE,
            ctx,
        )
    }

    /// 这个标签页开的那几趟里还有在跑的吗（标签上那个忙点）。
    pub fn has_running_jobs(&self) -> bool {
        self.my_trips()
            .iter()
            .any(|j| j.shown() && j.state() == crate::progress::State::Running)
    }

    /// 这个标签页要关了：它开过的那几趟照跑，落地后由窗口那一级重列目录。
    pub fn orphan_jobs(&mut self) {
        for id in self.trips.drain(..) {
            self.progress.orphan(id);
        }
    }

    /// 那台后端的阻塞档（机器上复制 · 解压 · 算大小 · 删除）开跑之后撤不动 ⇒ 「停」灰着时悬停那一句。
    /// 撤得动撤不动是那台握手时交来的事实（`Offer::withdraw`）；还没问到 ⇒ 照它是阻塞档说。
    pub(super) fn no_stop(&self, op: &str) -> Option<String> {
        let machine = self.source.label();
        let offer = self.offer.lock().unwrap();
        let offered = offer
            .as_ref()
            .is_some_and(|o| o.withdraw(op) == comms_inward::chan::wire::Withdraw::Asked);
        // 撤得动的那几条窗口这一侧也没有撤它的口（一问一答、阻塞到回话）⇒ 照样灰；说法分两种。
        Some(if offered {
            copy_text("rsFilewinProgress.stop.noHandle", &[])
        } else {
            copy_text(
                "rsFilewinProgress.stop.uncancellable",
                &[("machine", &machine)],
            )
        })
    }

    /// 〔「算目录大小」〕选中的这几项逐项问后端 `files-size`（顺序发），跑完一句话摆出来。
    /// 纯读 ⇒ 不重列目录。接不上（没运行时 / 没通道）⇒ 出声。回值 ＝ 真的起来了。
    pub fn start_sizes(&mut self, idx: &[usize], ctx: Option<egui::Context>) -> bool {
        let Some(h) = self.rt.clone() else {
            *self.listing.error.lock().unwrap() =
                Some(copy_text("rsFilewinShell.size.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.listing.error.lock().unwrap() = Some(NO_LINE.to_string().into());
            return false;
        };
        let items: Vec<(cmd_wire::files::WirePath, String)> = {
            let rows = self.listing.rows.lock().unwrap();
            // 〔有损名全寻址〕路径按字节发。
            idx.iter()
                .filter_map(|&i| rows.get(i))
                .map(|r| (self.row_path(r).path(), r.name.clone()))
                .collect()
        };
        let origin = self.source.origin();
        // 一摞一块新看板（「进度」表里独立的一行）。
        let board = crate::size::SizeBoard::default();
        board.attach(ctx);
        let no_stop = self.no_stop(crate::size::CMD_SIZE);
        let first = items.first().map(|(_, n)| n.clone()).unwrap_or_default();
        self.adopt_trip(
            crate::progress::Trip::Size {
                board: board.clone(),
                name: first,
            },
            None,
            no_stop,
        );
        h.spawn(async move {
            let mut out = Vec::with_capacity(items.len());
            for (path, name) in items {
                board.begin(&name);
                out.push(
                    crate::size::size_remote(&line, &origin, &path, &name)
                        .await
                        .map_err(|e| (name, e)),
                );
            }
            board.finish(out);
        });
        true
    }
}
