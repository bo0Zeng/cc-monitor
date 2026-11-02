//! 复制到另一台那一问：填机器 · 「放到」那一块小目录选择器 · 盖不盖；答完起一组（[`crate::cross_copy`]）。

use super::*;

impl FileWindow {
    /// 摆出「复制到另一台」那一问（机器 ＋ 目标目录）：第 `idx` 那几行（文件与目录都行；目录整棵逐个文件）。回值 ＝ 真的摆出来了。
    /// 名字只有原始字节的那几项寻址不到（那一条按路径串寻址）⇒ 不摆、出声。
    pub fn begin_cross_rows(&mut self, idx: &[usize]) -> bool {
        *self.prompt_error.lock().unwrap() = None;
        let rows: Vec<Listed> = {
            let all = self.listing.rows.lock().unwrap();
            idx.iter().filter_map(|&i| all.get(i).cloned()).collect()
        };
        if rows.is_empty() {
            return false;
        }
        let mut srcs = Vec::with_capacity(rows.len());
        for r in &rows {
            let p = self.row_path(r);
            if p.is_lossy() {
                self.key_notice = Some(copy_text(
                    "rsFilewinWorkspace.copyToOther.unaddressable",
                    &[("name", &r.name.to_string())],
                ));
                return false;
            }
            srcs.push(p);
        }
        let name = match rows.as_slice() {
            [r] => r.name.clone(),
            _ => copy_text(
                "rsFilewinBatch.label.nItems",
                &[("n", &rows.len().to_string())],
            ),
        };
        // 缺省机器：除开这一台的第一台远端（本机不在这张表里：落到本机是「下载」）；落点缺省那台上次选到的，没有就那台的主目录。
        let here = self.source.origin().to_string();
        let machine = self
            .machines
            .iter()
            .find(|m| **m != here && m.as_str() != cmd_wire::LOCAL)
            .cloned()
            .unwrap_or_default();
        self.cross_prompt = Some(crate::cross_copy::CrossPrompt {
            name,
            names: rows.iter().map(|r| r.name.clone()).collect(),
            srcs,
            machine,
            dir: String::new(),
        });
        self.cross_pick = Default::default();
        true
    }

    /// 答完那一问 ⇒ 起那一趟（机器名空 ⇒ 框留着、出声）。回值 ＝ 真的起来了。
    pub fn confirm_cross(&mut self, ctx: Option<egui::Context>) -> bool {
        let Some(p) = self.cross_prompt.clone() else {
            return false;
        };
        if p.machine.trim().is_empty() {
            *self.say_slot() = Some(copy_text("rsFilewinCrossCopy.prompt.noMachine", &[]).into());
            return false;
        }
        // 落点 ＝ 选择器上选到的（还没读出来 ⇒ 框留着、出声：落点要一条绝对路径）。
        let pick = self.cross_pick.shown();
        let p = if pick.machine == crate::cross_copy::origin_of(&p.machine) && !pick.path.is_empty()
        {
            crate::cross_copy::remember_dir(&pick.machine, &pick.target());
            crate::cross_copy::CrossPrompt {
                dir: pick.target(),
                ..p
            }
        } else {
            p
        };
        if !p.dir.starts_with('/') {
            *self.say_slot() =
                Some(copy_text("rsFilewinCrossCopy.dir.notAbsolute", &[("dir", &p.dir)]).into());
            return false;
        }
        // 一组一条：`transfer-copy`（寻址 ＝ 这一台，`to` ＝ 落点那台）；在本机后端里做 ⇒ 撤与断开认本机。
        let ask = cmd_wire::transfer::TransferCopyAsk {
            to: p.machine.trim().to_string(),
            items: p
                .srcs
                .iter()
                .map(|s| cmd_wire::transfer::CopyFrom {
                    remote_path: s.shown.clone(),
                })
                .collect(),
            to_dir: p.dir.clone(),
            on_exists: None,
            decide: None,
            ticket: None,
        };
        let plan = batch_core::Plan {
            verb: batch_core::Verb::Copy,
            cmd: crate::cross_copy::CMD_CROSS,
            args: serde_json::to_value(&ask).unwrap_or_default(),
            names: p.names.clone(),
            into: Some(format!("{}:{}", p.machine.trim(), p.dir)),
            machine: self.source.label(),
        };
        let home = crate::source::Origin::local();
        if !self.start_batch(plan, home, egui_phosphor::regular::ARROWS_LEFT_RIGHT, ctx) {
            return false;
        }
        self.cross_prompt = None;
        *self.prompt_error.lock().unwrap() = None;
        true
    }

    /// 画「复制到另一台」那一问（kit 的对话框）：「复制到」下拉（别的机器 · 本机）·「放到」一块小目录选择器
    /// （面包屑 ＋ 那台的文件夹，单击选中、双击进去，右上「新建文件夹」）·「目标 gpu-01:/home/user/inbox」· ［取消］［复制到 gpu-01］。
    pub(super) fn cross_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut p) = self.cross_prompt.clone() else {
            return;
        };
        let pal = crate::theme::palette(ui.ctx());
        let machine = crate::cross_copy::origin_of(&p.machine);
        // 选择器跟着下拉那台走：换了机器（或刚打开）⇒ 去那台上次的落点 / 主目录。
        let pick = self.cross_pick_follow(ui, &machine);
        let here = self.source.origin().to_string();
        let machines: Vec<String> = self
            .machines
            .iter()
            .filter(|m| **m != here && m.as_str() != cmd_wire::LOCAL)
            .map(|m| crate::cross_copy::shown_machine(m))
            .collect();
        let err = self.prompt_failure();
        let mut go_to: Option<String> = None;
        let mut picked: Option<String> = None;
        let mut mkdir = false;
        let shown_machine = p.machine.clone();
        let hit = crate::kit::dialog(
            ui.ctx(),
            "filewin-cross-copy",
            &copy_text("rsFilewinCrossCopy.prompt.heading", &[("name", &p.name)]),
            |ui| {
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinCrossCopy.prompt.machine", &[]))
                        .color(pal.text2),
                );
                let w = ui.available_width();
                let entries: Vec<crate::kit::Entry> = machines
                    .iter()
                    .map(|m| {
                        crate::kit::Entry::Row(
                            crate::kit::MenuRow::new("", m.clone()).checked(*m == p.machine),
                        )
                    })
                    .collect();
                if let Some(k) = crate::kit::select_field(
                    ui,
                    "filewin-cross-machine",
                    w,
                    format!("{}  {}", egui_phosphor::regular::DESKTOP, shown_machine),
                    &entries,
                ) {
                    p.machine = machines[k].clone();
                }
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinCrossCopy.prompt.dir", &[]))
                        .color(pal.text2),
                );
                egui::Frame::new()
                    .fill(pal.bg)
                    .corner_radius(8.0)
                    .stroke(egui::Stroke::new(1.0, pal.border_soft))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        cross_crumbs(ui, &pal, &pick, &mut go_to, &mut mkdir);
                        ui.separator();
                        cross_dirs(ui, &pal, &pick, &mut go_to, &mut picked);
                    });
                ui.add_space(6.0);
                if !pick.path.is_empty() {
                    let target = copy_text(
                        "rsFilewinCrossCopy.prompt.hint",
                        &[("machine", &shown_machine), ("path", &pick.target())],
                    );
                    ui.add(
                        egui::Label::new(egui::RichText::new(&target).size(12.0).color(pal.text2))
                            .truncate(),
                    )
                    .on_hover_text(target);
                }
                if let Some(f) = &err {
                    let id = ui.id().with("prompt-fail-detail");
                    crate::kit::failure_line(ui, &f.said, &f.detail, id);
                }
            },
            &[
                (
                    copy_text("rsFilewinShell.copyUi.cancel", &[]),
                    crate::kit::Btn::Plain,
                ),
                (
                    copy_text(
                        "rsFilewinCrossCopy.prompt.go",
                        &[("machine", &shown_machine)],
                    ),
                    crate::kit::Btn::Primary,
                ),
            ],
            1,
            0,
        );
        self.cross_after(
            ui,
            p,
            &pick,
            hit,
            CrossClicks {
                picked,
                go_to,
                mkdir,
            },
        );
    }

    /// 那一问画完之后：选中 / 进去 / 新建文件夹 · 取消 · 确认。
    pub(super) fn cross_after(
        &mut self,
        ui: &egui::Ui,
        p: crate::cross_copy::CrossPrompt,
        pick: &crate::cross_copy::Pick,
        hit: Option<usize>,
        clicks: CrossClicks,
    ) {
        let CrossClicks {
            picked,
            go_to,
            mkdir,
        } = clicks;
        if let Some(n) = picked {
            self.cross_pick.pick(&n);
        }
        if let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) {
            if let Some(to) = go_to {
                self.cross_pick
                    .go(&h, &line, &pick.machine, &to, Some(ui.ctx().clone()));
            } else if mkdir {
                self.cross_pick.mkdir(&h, &line, Some(ui.ctx().clone()));
            }
        }
        self.cross_prompt = Some(p);
        match hit {
            Some(0) => {
                self.cross_prompt = None;
                *self.prompt_error.lock().unwrap() = None;
            }
            Some(_) => {
                let ctx = ui.ctx().clone();
                self.confirm_cross(Some(ctx));
            }
            None => {}
        }
    }

    /// 「放到」那块选择器跟着下拉那台走：换了机器（或刚打开）⇒ 去那台上次的落点 / 主目录。交回此刻摆的那一份。
    pub(super) fn cross_pick_follow(
        &mut self,
        ui: &egui::Ui,
        machine: &str,
    ) -> crate::cross_copy::Pick {
        let mut pick = self.cross_pick.shown();
        if !machine.is_empty() && pick.machine != *machine {
            if let (Some(h), Some(line)) = (self.rt.clone(), self.line.clone()) {
                let start = crate::cross_copy::last_dir(machine).unwrap_or_default();
                self.cross_pick
                    .go(&h, &line, machine, &start, Some(ui.ctx().clone()));
                pick = self.cross_pick.shown();
            }
        }
        pick
    }
}

/// 「放到」那块的面包屑行：那台 › 最后三级（前面折成「…」）；右端「新建文件夹」。
pub(super) fn cross_crumbs(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    pick: &crate::cross_copy::Pick,
    go_to: &mut Option<String>,
    mkdir: &mut bool,
) {
    // 面包屑：那台 › 一段一段（点哪一段回到那一级）；右端「新建文件夹」。
    ui.horizontal(|ui| {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(crate::cross_copy::shown_machine(&pick.machine))
                        .size(12.0)
                        .color(pal.text2),
                )
                .frame(false),
            )
            .clicked()
        {
            *go_to = Some("/".to_string());
        }
        // 只摆最后三级，前面折成一颗「…」（点了到折起来的最深那一级）：那台的主目录可能很深。
        let crumbs: Vec<(String, String)> = crate::source::breadcrumbs(&pick.path)
            .into_iter()
            .skip(1)
            .collect();
        let keep = crumbs.len().saturating_sub(3);
        if keep > 0 {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CARET_RIGHT)
                    .size(10.0)
                    .color(pal.faint),
            );
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(copy_text("rsFilewinEditPage.crumb.more", &[]))
                            .size(12.0)
                            .color(pal.text2),
                    )
                    .frame(false),
                )
                .clicked()
            {
                *go_to = Some(crumbs[keep - 1].1.clone());
            }
        }
        for (seg, at) in crumbs.into_iter().skip(keep) {
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CARET_RIGHT)
                    .size(10.0)
                    .color(pal.faint),
            );
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(&seg).size(12.0).color(pal.text))
                        .frame(false),
                )
                .clicked()
            {
                *go_to = Some(at);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = ui.add_enabled(
                !pick.path.is_empty(),
                egui::Button::new(
                    egui::RichText::new(format!(
                        "{} {}",
                        egui_phosphor::regular::PLUS,
                        copy_text("rsFilewinWriteops.inline.newDir", &[])
                    ))
                    .size(12.0),
                )
                .frame(false),
            );
            *mkdir = r.clicked();
        });
    });
}

/// 那台的文件夹清单（至多五行高）：单击选中、双击进去。
pub(super) fn cross_dirs(
    ui: &mut egui::Ui,
    pal: &crate::theme::Palette,
    pick: &crate::cross_copy::Pick,
    go_to: &mut Option<String>,
    picked: &mut Option<String>,
) {
    let row_h = crate::theme::metrics(ui.ctx()).row_h;
    egui::ScrollArea::vertical()
        .max_height(row_h * 5.0)
        .auto_shrink([false, true])
        .show(ui, |ui| match &pick.dirs {
            None => crate::kit::skeleton_rows(ui, 3),
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).color(pal.error_text));
            }
            Some(Ok(dirs)) if dirs.is_empty() => {
                ui.label(
                    egui::RichText::new(copy_text("rsFilewinCrossCopy.prompt.noDirs", &[]))
                        .color(pal.text2),
                );
            }
            Some(Ok(dirs)) => {
                for d in dirs {
                    let on = pick.picked.as_deref() == Some(d.as_str());
                    let (rect, r) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), row_h),
                        egui::Sense::click(),
                    );
                    if on {
                        ui.painter().rect_filled(rect, 4.0, pal.picked);
                    } else if r.hovered() {
                        ui.painter().rect_filled(rect, 4.0, pal.hover);
                    }
                    ui.painter().text(
                        rect.left_center() + egui::vec2(8.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{}  {}", egui_phosphor::regular::FOLDER_SIMPLE, d),
                        egui::TextStyle::Body.resolve(ui.style()),
                        pal.text,
                    );
                    if r.double_clicked() {
                        *go_to = Some(crate::source::child_of(&pick.path, d));
                    } else if r.clicked() {
                        *picked = Some(d.clone());
                    }
                }
            }
        });
}

/// 「复制到另一台」那一问里这一帧点下的：选中哪个文件夹 · 进到哪 · 新建文件夹。
pub(super) struct CrossClicks {
    picked: Option<String>,
    go_to: Option<String>,
    mkdir: bool,
}
