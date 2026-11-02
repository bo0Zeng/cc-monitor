//! 起一趟、收一趟：拖入上传 · 复制为 · 一组（复制到另一栏 · 删除 · 改权限）· 解压 · 往外拖 · 算大小。
//! 每一趟是「进度」表里一行（[`crate::progress`]），结局只在那一行判一次；这个标签页只记号、照那一个结局收场（[`FileWindow::settle_trips`]）。

use super::*;

impl FileWindow {
    /// 拖进来的那几个本机文件 → 待传清单（**目标目录 = 当前目录**）。
    ///
    /// 🔴 **这里没有「是不是远端」那道闸，而那是对的。** 「窗口看着本机时把本机文件拖进本机」
    /// 这个状态**写不出来**：`Source` 是 newtype，只可能是一台远端（理由住 `crate::source::Source` 头注）。
    pub fn pending_for(&self, local_paths: &[String]) -> Vec<Pending> {
        local_paths
            .iter()
            .filter_map(|p| Pending::into_remote_dir(p, &self.cwd))
            .collect()
    }

    /// 起一趟上传：整组一条开单（同名由后端在落点那台探，有同名冲突框问一次），开了单的并行传（[`crate::transfer::run_drop`]）。
    /// 接不上（**没运行时**）要**出声**，判据见 `shell_tests`。
    pub fn start_drop(&mut self, items: Vec<Pending>, ctx: Option<egui::Context>) -> bool {
        if items.is_empty() {
            return false;
        }
        // 有损目录里也传：暂存区那一段与目录无关，探在不在与提交按目录的字节寻址（此前 W5-FILES 在这里出声拒）。
        let items: Vec<Pending> = items
            .into_iter()
            .map(|p| Pending {
                remote_dir_raw: self.cwd_raw.clone(),
                ..p
            })
            .collect();
        self.start_drop_items(items, ctx)
    }

    /// 同 [`Self::start_drop`]，只是这一摞的目标目录照它们自己带着的（「进度」表上「重试失败的」「继续」走这里）。
    pub fn start_drop_items(&mut self, items: Vec<Pending>, ctx: Option<egui::Context>) -> bool {
        if items.is_empty() {
            return false;
        }
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.upload.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string().into());
            return false;
        };
        let origin = self.source.origin();
        // 🔴一摞一块新看板（「进度」表里独立的一行）：上一摞还在飞也不碍事、互不清掉。
        //    新看板的取消台是新的 ⇒ 上一摞按过的取消拦不下这一摞。
        let board = self.board.fresh_like();
        self.board = board.clone();
        // 🔴 **把窗口交给看板**，它自己会在「进度动了 / 跑完了」时敲一下。
        board.attach(ctx.clone());
        let dir = crate::source::parent_dir(&items[0].remote_path);
        // 开单那一问（同名冲突框挂在这个标签页上，与复制 · 下载同一个框）。
        let ask = crate::batch::BatchBoard::new(batch_core::Plan {
            verb: batch_core::Verb::Upload,
            cmd: crate::transfer::CMD_UPLOAD,
            args: serde_json::to_value(crate::transfer::group_ask(&items, None, None))
                .unwrap_or_default(),
            names: items.iter().map(|p| p.name.clone()).collect(),
            into: Some(dir.clone()),
            machine: self.source.label(),
        });
        ask.attach(ctx);
        self.adopt_trip(
            crate::progress::Trip::Upload {
                board: board.clone(),
                items: items.clone(),
                dir: crate::source::remote_basename(&dir).to_string(),
                ask: ask.clone(),
            },
            Some(dir),
            None,
        );
        h.spawn(async move {
            let (up_line, up_origin) = (line.clone(), origin.clone());
            let up_board = board.clone();
            let open_board = board.clone();
            let out = crate::transfer::run_drop(
                items,
                crate::transfer::lanes(),
                move || async move {
                    crate::transfer::open_group(&line, &origin, &ask, &open_board).await
                },
                move |t| {
                    let (line, origin) = (up_line.clone(), up_origin.clone());
                    let b = up_board.clone();
                    // 🔴**取消那道闸在这儿**：按过取消之后，还没起的那几件一件都不起
                    //   （键由那道闸造并登记，理由住 `launch_unless_cancelled`）。
                    async move {
                        let desk = b.cancels();
                        let name = t.item.name.clone();
                        crate::transfer::launch_unless_cancelled(&desk, &name, |_id| async move {
                            crate::transfer::upload_remote(&line, &origin, &t, &b).await
                        })
                        .await
                    }
                },
            )
            .await;
            board.finish(out);
        });
        true
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
        let items = self.pending_for(&dropped);
        if items.is_empty() {
            *self.listing.error.lock().unwrap() = Some(
                copy_text(
                    "rsFilewinShell.drop.noNames",
                    &[("n", &(dropped.len()).to_string())],
                )
                .into(),
            );
            return;
        }
        self.start_drop(items, Some(ctx.clone()));
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
    /// ⚠ **这里只剩一道闸了，如实记**：`pick_core::Pick::copyable` —— 名字寻址不到的
    /// 一律不接。列表上那两档压根不画那颗按钮，这里是第二道，防的是
    /// 「按钮没了、调用还在」（那正是死值验刀 2 那一形）。
    ///
    /// 🔴 **没有「远端才有」那道闸**：`copy-data` 是 SFTP 协议的扩展，而窗口**只可能**看着一台远端
    /// （`Source` 是 newtype）⇒ 本机源这个状态写不出来。
    pub fn begin_copy(&mut self, i: usize) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        let row = {
            let rows = self.listing.rows.lock().unwrap();
            match rows.get(i) {
                Some(r) if r.pick().copyable() => r.clone(),
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
        let board = crate::batch::BatchBoard::new(plan);
        board.attach(ctx);
        self.adopt_trip(
            crate::progress::Trip::Batch {
                board: board.clone(),
                icon,
            },
            Some(self.cwd.clone()),
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
            // 建议名按本机平台改成合法的（Windows 不认的字换成 `_`），落在系统的「下载」文件夹。
            Purpose::DownloadMany => PickKind::Folder,
            Purpose::Download => PickKind::SaveFile {
                suggested_name: self
                    .pull_want
                    .as_ref()
                    .map(|(_, n)| crate::download::local_name(n, crate::platform::WINDOWS_NAMES))
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
                    let items = self.pending_for(&paths);
                    self.start_drop(items, ctx);
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
                match (want, picked.and_then(|v| v.into_iter().next())) {
                    (Some((src, _)), Some(dest)) => {
                        self.start_pull(&src, &dest.to_string_lossy(), true, ctx);
                    }
                    _ => self.pull_raw = None,
                }
            }
        }
        true
    }

    /// 下载多项：先选一个本机目录（系统的选目录框），选好了一组一条（[`crate::download::pull_many`]）。只收文件（菜单那张表管）。
    pub fn begin_pull_many(&mut self, idx: &[usize]) -> bool {
        let rows: Vec<Listed> = {
            let all = self.listing.rows.lock().unwrap();
            idx.iter().filter_map(|&i| all.get(i).cloned()).collect()
        };
        if rows.is_empty() || rows.iter().any(|r| !r.pick().downloadable()) {
            self.key_notice = Some(select::refusal(Action::Download, rows.len()));
            return false;
        }
        self.pull_many = rows;
        self.start_pick(crate::picker::Purpose::DownloadMany, None)
    }

    /// 选好了本机目录 ⇒ 起那一组。
    pub fn start_pull_many(&mut self, dir: &str, ctx: Option<egui::Context>) -> bool {
        let rows = std::mem::take(&mut self.pull_many);
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.pull.noRuntime", &[]).into());
            return false;
        };
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string().into());
            return false;
        };
        let ask = cmd_wire::transfer::TransferDownloadAsk {
            items: rows
                .iter()
                .map(|r| cmd_wire::transfer::DownloadItem {
                    remote_path: r.path.clone(),
                    name: Some(cmd_wire::files::WirePath(name_bytes(r))),
                })
                .collect(),
            local_dir: cmd_wire::files::WirePath::from(dir),
            on_exists: None,
            decide: None,
        };
        let board = crate::batch::BatchBoard::new(batch_core::Plan {
            verb: batch_core::Verb::Download,
            cmd: crate::batch::CMD_DOWNLOAD,
            args: serde_json::to_value(&ask).unwrap_or_default(),
            names: rows.iter().map(|r| r.name.clone()).collect(),
            into: Some(dir.to_string()),
            machine: self.source.label(),
        });
        board.attach(ctx);
        // 开单那一问也是「进度」表里一行（开成了 ⇒ 开了单的每一项各一行，这一行拿掉；开不成 ⇒ 这一行说没成）。
        self.adopt_trip(
            crate::progress::Trip::Batch {
                board: board.clone(),
                icon: egui_phosphor::regular::DOWNLOAD_SIMPLE,
            },
            None,
            None,
        );
        let srcs = rows
            .iter()
            .map(|r| (r.path.clone(), r.name.clone()))
            .collect();
        h.spawn(crate::download::pull_many(
            line,
            self.source.origin(),
            board,
            self.progress.clone(),
            srcs,
        ));
        true
    }

    pub fn begin_pull(&mut self, i: usize) -> bool {
        let Some(row) = self.listing.rows.lock().unwrap().get(i).cloned() else {
            return false;
        };
        self.begin_pull_row(row)
    }

    /// 同 [`Self::begin_pull`]，那一行直接给（编辑页「下载到本机」走这里：它手上没有列表）。
    pub fn begin_pull_row(&mut self, row: Listed) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        // 有损名带着字节 / 有损目录里的行也拉得下来（按字节，`lossy_pull.rs`）。
        if !row.pick().downloadable() {
            return false;
        }
        let full = self.row_path(&row);
        self.pull_raw = full
            .is_lossy()
            .then(|| (full.bytes(), row.name.clone(), name_bytes(&row)));
        self.pull_want = Some((row.path.clone(), row.name.clone()));
        self.start_pick(crate::picker::Purpose::Download, None)
    }

    /// 收掉那个框，什么都不做。
    pub(super) fn pull_local(&self, dest: &str) -> (serde_json::Value, Option<String>) {
        match &self.pull_raw {
            Some((_, shown, raw)) => crate::lossy_pull::local_dest(dest, shown, raw),
            None => (serde_json::Value::String(dest.to_string()), None),
        }
    }

    /// 答完当前这一问 → 下一步。回值 = **这一下真的推进了**。
    ///
    /// 🔴 三支各自的下一跳完全不同（理由住 `download::DestVerdict` 的头注）：
    /// 不合法 ⇒ **框留着 ＋ 出声**（收掉的话「点了确认什么都没发生」与成功同形）；
    /// 要确认 ⇒ 换成第二问；可以做 ⇒ 起那一趟并收掉框。
    /// 起了 ⇒ 回它在「进度」表里那一行的号（重试那一处拿它顶替按下的那一行）；没起 ⇒ `None`。
    pub fn start_pull(
        &mut self,
        src_path: &str,
        dest: &str,
        overwrite: bool,
        ctx: Option<egui::Context>,
    ) -> Option<u64> {
        let Some(h) = self.rt.clone() else {
            *self.say_slot() = Some(copy_text("rsFilewinShell.pull.noRuntime", &[]).into());
            return None;
        };
        // 下载经通道开单、订阅进度 —— 要那条线 ＋ 那台机器的地址。
        let Some(line) = self.line.clone() else {
            *self.say_slot() = Some(NO_LINE.to_string().into());
            return None;
        };
        let origin = self.source.origin();
        // 一趟一块新看板（「进度」表里独立的一行）：上一趟还在下也不碍事。
        let board = crate::download::DownloadBoard::default();
        board.attach(ctx);
        let src = src_path.to_string();
        let to = dest.to_string();
        let id = self.adopt_trip(
            crate::progress::Trip::Download {
                board: board.clone(),
                name: crate::source::remote_basename(&src).to_string(),
                src: src.clone(),
                dest: to.clone(),
            },
            None,
            None,
        );
        // 有损名：远端按字节（就地拷进暂存区再下）、本机按平台落名；否则与此前逐字同一条路。
        let (local, note) = self.pull_local(dest);
        let raw_src = self.pull_raw.take().map(|(b, _, _)| b);
        board.set_note(note);
        // ⚠ 走 `source::remote_basename`（**那一对**里的尾段那一份）——
        //   这里原先是第四份 `rsplit('/')`，而那一对的头注逐字说了
        //   「多一份就多一种『Windows 上 `\` 被当分隔符』的机会」。
        //   盘上每一处按 `/` 切的地方由
        //   `source_tests::every_place_that_splits_a_remote_path_is_declared` 逐条钉着。
        let name = crate::source::remote_basename(&src).to_string();
        board.begin(&name);
        h.spawn(async move {
            let desk = board.cancels();
            let run = crate::transfer::launch_unless_cancelled(&desk, &name, |_id| {
                let b = board.clone();
                let (line, origin) = (line.clone(), origin.clone());
                let src = src.clone();
                let to = to.clone();
                let (local, raw_src) = (local.clone(), raw_src.clone());
                async move {
                    match raw_src {
                        Some(bytes) => crate::lossy_pull::pull_by_bytes(
                            &line, &origin, &bytes, local, overwrite, &b,
                        )
                        .await
                        .map_err(crate::source::Failed::from),
                        None => crate::download::pull_one(&line, &origin, &src, &to, &b).await,
                    }
                }
            })
            .await;
            let (got, total) = board.seen();
            board.finish(match run {
                // ⚠ 字节数报**真读数**（进度那一格最后一个值），不报「应该是多少」。
                Ok(()) => crate::download::Outcome::Done {
                    dest: to.clone(),
                    bytes: if got > 0 { got } else { total },
                },
                Err(why) => crate::download::Outcome::Failed {
                    dest: to.clone(),
                    why,
                },
            });
        });
        Some(id)
    }

    /// 记下这一类最近那一趟在表里的号；上一趟（这个标签页手上已经换了一块新看板）改由窗口那一级在落地时重列目录。
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
