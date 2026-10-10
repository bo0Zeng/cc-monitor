//! 命令表 · 别名：`aliases-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "aliases-read",
        summary: "启动文件候选（接入那一格）",
        codes: &["bad_args", "refused"],
        fields: &[out("home", "这台的家目录"), out("otherRc", "`rcPath` 过了围栏之后的绝对路径"), out("rcCandidates", "启动文件候选（方言答列哪几份）：每份 `{path, sourced, exists, block, unreadable, policy, blockLines}`（`blockLines` = 把别名块装进这一份会写几行），`block` = 别名块现状 `{present, conflictingFunctions}`（`conflictingFunctions` = 块外自己定义的、与配置文件里某一段同名的函数 `{name, line, wins}`；`wins` = 新开的终端里敲这个名字起的是哪一个：`yours`（你写的）· `list`（清单那条）· `unclear`（说不清））"), arg("rcPath", "人另指的那一份（`null` = 不指）：过围栏（只许落在 home 之内 · 符号链接不许跑出去）后并进候选"), arg("shell", "`posix` / `powershell`（这台后端不在 Windows ⇒ `powershell` 拒）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_read(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "profiles-read",
        summary: "读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填",
        codes: &["refused"],
        fields: &[out("accounts", "这台的账号表（具名号，按账号库的顺序；表单「账号」那一格的选项）"), out("binDir", "链接住的目录（`~/.cc-monitor/bin`）"), out("editedAt", "上次 cc-monitor 写过之后有人改过 ⇒ 那份的修改时刻（按这台本地钟写好：当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年）；没改过 · cc-monitor 没写过 ⇒ `null`"), out("exists", "配置文件在不在"), out("fileProblem", "TOML 本身写坏 ⇒ `{line, message}`（这时 `profiles` 为空、不能按条目改）；否则 `null`"), out("fingerprint", "盘上那份的指纹（不在 ⇒ `null`），存的时候交回 `profiles-write`"), out("home", "这台的家目录（界面拿它把路径写成 `~/…`）"), out("migrated", "旧别名清单一次性转进来之后那张说明 `{count, path, skipped}`（「知道了」之后 `null`）"), out("path", "配置文件的路径"), out("profiles", "每段 `{name, from, own: [{key, slot, vals, line}], agent, usable, problem: {line, message} | null, kind: link/function, functionWhy, functionLine, said, form, accountShape}`：`accountShape` 是「账号那一形」`{account, tmux}`（自己只写了号、可再加 tmux，按合并下来的算；其余 `null`）：`said` 是树里那一行（自己写的几项，「标签 值」）；`form` 是表单回填（没写的格 `null` ＝ 继承）；`problem` 的原话与终端里敲这个名字得到的同一句"), out("seed", "配置文件不在时首建那两条的预览（同 `profiles` 一条的形状；在 ⇒ `[]`）"), out("tmux", "这台有没有 tmux（「在哪起」那一格选不选得了 tmux 看它）；探不出 ⇒ `null`")],
        takes_input: true,
        run: Run::Blocking(|r| crate::assets::aliases::page::answer_read(&LocalFiles, &r.args).map(Some).map_err(|(c, m)| (c.to_string(), m))),
    },
    CommandSpec {
        name: "profiles-resolve",
        summary: "一段合下来的合并表与「等于」那一行（可按未存的表单算）",
        codes: &["bad_args", "refused"],
        fields: &[arg("at", "假设在这个目录敲（`~` 打头按这台家目录展开；`null` ＝ 家目录）"), out("chain", "继承链（父 → 子）"), arg("edit", "未存的表单（同 `profiles-read` 一段的 `form`；`null` ＝ 按盘上那份算）"), out("line", "这台后端算的「等于」那一行（同 `ccm @名 -- --ccm-print`）；算不出 ⇒ `null`"), out("lineError", "算不出那一行时 ccm 的原话"), arg("name", "哪一段（带 `edit` 时是正在改的那一段原来的名字，新增写表单里的名字）"), out("problem", "合不下来 ⇒ 那一句（同终端里敲这个名字）；否则 `null`"), out("rows", "合并表 `[{key, slot, label, vals, said, from, overriddenBy}]`：父 → 子、层内照写的顺序；被后来那一层盖掉的也在，`overriddenBy` 是盖掉它的那一段")],
        takes_input: true,
        run: Run::Blocking(|r| crate::assets::aliases::page::answer_resolve(&LocalFiles, &r.args).map(Some).map_err(|(c, m)| (c.to_string(), m))),
    },
    CommandSpec {
        name: "profiles-impact",
        summary: "这几处改动会让哪几段合下来变（改前改后）",
        codes: &["bad_args", "refused"],
        fields: &[out("affected", "改动直接点名的那几段之外、合下来会变的 `[{name, changes: [{slot, label, before, after}], problem}]`（变得合不下来 ⇒ `problem` 是那一句）"), arg("changes", "同 `profiles-write` 的 `changes`（一个字节不写）")],
        takes_input: true,
        run: Run::Blocking(|r| crate::assets::aliases::page::answer_impact(&LocalFiles, &r.args).map(Some).map_err(|(c, m)| (c.to_string(), m))),
    },
    CommandSpec {
        name: "profiles-bases",
        summary: "「基于」下拉能选的几段（选了不成圈）",
        codes: &["bad_args", "refused"],
        fields: &[out("bases", "`[{name, from, said, selectable}]`：选了不会绕成圈的那几段 ＋ 自己（`selectable: false`）"), arg("name", "正在改 / 新建的那一段的名字")],
        takes_input: true,
        run: Run::Blocking(|r| crate::assets::aliases::page::answer_bases(&LocalFiles, &r.args).map(Some).map_err(|(c, m)| (c.to_string(), m))),
    },
    CommandSpec {
        name: "profiles-write",
        summary: "按条目改配置文件（手写的注释与排版留着），照它补链接 / 终端函数",
        codes: &["bad_args", "refused", "stale"],
        fields: &[arg("changes", "依次做的改动：`{op: \"set\", was, form}`（新增 `was: null`；名字变了 ⇒ 改名，基于它的跟着改）· `{op: \"remove\", name, children}`（还被基于 ⇒ `children` 必给：`reparent` 改成基于它的父 · `cascade` 一起删）· `{op: \"init\", seed}`（配置文件不在时建：`seed` ⇒ 带首建那两条）· `{op: \"ackMigrated\"}`（迁移说明知道了）。改完多出坏处 ⇒ 整批不写、`refused`"), both("fingerprint", "入：必给（字符串或 `null`），读回时的指纹；盘上此刻不是那一份 ⇒ `stale`。出：写完那一份的指纹"), out("reload", "终端函数那份文件真改了 ⇒ 给人的那一句「已开的终端要重读」；否则 `null`"), out("wrote", "配置文件 / 终端函数文件 / 链接动没动")],
        takes_input: true,
        run: Run::Blocking(|r| crate::assets::aliases::page::answer_write(&LocalFiles, &r.args).map(Some).map_err(|(c, m)| (c.to_string(), m))),
    },
    CommandSpec {
        name: "aliases-block-render",
        summary: "别名块预览",
        codes: &["bad_args", "refused"],
        fields: &[arg("rcPath", "目标文件（方言由它的扩展名定：`.ps1` ⇒ PowerShell）"), out("text", "往一份空文件里装一次会写成什么（与 `aliases-block-install` 调同一个 `plan_install`）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_block_render(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-block-install",
        summary: "别名块装进人选的那份启动文件",
        codes: &["bad_args", "refused"],
        fields: &[arg("rcPath", "人选的那份启动文件（过围栏；方言由扩展名定，再过方言那一道闸）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_block_install(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-block-remove",
        summary: "别名块卸掉",
        codes: &["bad_args", "refused"],
        fields: &[arg("rcPath", "同 `aliases-block-install`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_block_remove(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
