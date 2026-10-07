//! 命令表 · 别名：`aliases-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // 别名六条：`assets/aliases/`（规则 · 方言 · 围栏），读写经 [`LocalFiles`]。
    CommandSpec {
        name: "aliases-render",
        summary: "清单 → 别名文件的样子（每条只有名字，规则住配置文件 `profiles.toml`）",
        codes: &["bad_args", "refused"],
        fields: &[arg("aliases", "清单：每条 `{name, args, restTo}` ＝ 配置文件里一段自己写的那几项（`args` 是 ccm argv 的写法；「基于」不在这里，存的时候按名字从盘上那一段接上；`restTo` 只收 `agent`）"), out("collisions", "撞名提示（自带别名块 · **这台** `PATH` 上的同名程序 · PowerShell 内建别名；只出声、不拦）"), out("fileText", "整份别名文件"), out("lines", "写进别名文件的那几行：每条只有名字，只把调用交给 `ccm @名字`；POSIX 上只有撞名的那几条进文件，其余是 `~/.cc-monitor/bin/<名>` → `ccm` 的链接；PowerShell 每条一个函数"), out("problems", "不合格的那几条 `{name, message}`（逐项判；几项之间的组合存的时候连「基于」整份判。非空时 `aliases-install` 一个字节都不写）"), arg("shell", "`posix` / `powershell`（这台后端不在 Windows ⇒ `powershell` 拒）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_render(&r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 别名表单两向：纯函数，问的是 ccm 自己的解析器（`assets/aliases/form.rs`）。
    CommandSpec {
        name: "aliases-to-form",
        summary: "一条别名摊成表单那几格",
        codes: &["bad_args"],
        fields: &[out("account", "空 = 不指定"), out("agent", "哪一家 agent"), arg("alias", "一条别名 `{name, args, restTo}`（同 `aliases-render` 清单里的一条）"), arg("args", "别名的参数串（数组）"), out("at", "`cwdIf` 的一项：在哪个目录"), out("base", "显式不带账号"), out("busNote", "cc-bus 登记的备注"), out("busRegister", "登记进 cc-bus"), out("ccmOther", "表单没有格子的 ccm 参数，一串"), out("cwd", "空 = 当前目录"), out("cwdIf", "`[{at, to}]`，按序"), out("detach", "起完不接进去"), out("form", "表单那几格：`name` · `cwdIf`"), out("launcher", "启动器"), out("model", "模型"), both("name", "别名名"), out("passthru", "交给 agent 的其余参数，一串"), arg("restTo", "最后那段参数交给谁：`agent` · `ccm`"), out("tmux", "`none` / `auto` / `named` / `base` / `attach`"), out("tmuxName", "tmux 会话名"), out("tmuxSize", "tmux 窗口尺寸"), out("to", "`cwdIf` 的一项：换到哪个目录")],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::assets::aliases::answer_to_form(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "aliases-from-form",
        summary: "表单拼回一条别名",
        codes: &["bad_args", "refused"],
        fields: &[out("alias", "拼出来的那一条 `{name, args, restTo}`"), arg("form", "表单那几格（同 `aliases-to-form` 的 `form`）"), arg("orig", "必给：正在改的那一条（`aliases-to-form` 收的那一条），新增 ⇒ `null`")],
        takes_input: true,
        run: Run::Async(|r| {
            Box::pin(async move {
                crate::assets::aliases::answer_from_form(&r.args)
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    CommandSpec {
        name: "aliases-read",
        summary: "读回清单 ＋ 启动文件候选",
        codes: &["bad_args", "refused"],
        fields: &[out("accounts", "这台的账号表（具名号，按账号库的顺序；没有账号库 ⇒ `[]`）"), out("aliasPath", "这台上那份配置文件（`profiles.toml`）的路径"), out("aliases", "读回的清单：每段自己写的那几项（配置文件不在 ⇒ 首建会带上的 `cc` · `cct`，没有 tmux 的目标只有 `cc`；不在而旧形状的别名文件在 ⇒ 先一次性迁移再读）"), out("exists", "配置文件在不在"), out("fingerprint", "盘上那份配置文件的指纹（不透明的串：长度 ＋ 一个 64 位散列；不在 ⇒ `null`），存的时候交回 `aliases-install`"), out("groups", "与 `aliases` 逐条对应：这一段自己只写了号（可再加 tmux）⇒ `{account, tmux}`，号与 tmux 按合并下来的算，其余 ⇒ `null`（不看名字；界面照这一格分组）"), out("missing", "账号表里的号缺哪一条：`{account, tmux, alias}`（`alias` 就是点「加上」要加进清单的那一条；没有 tmux 的目标只看 `<号>cc`）"), out("otherRc", "`rcPath` 过了围栏之后的绝对路径"), out("rcCandidates", "启动文件候选（方言答列哪几份）：每份 `{path, sourced, exists, block, unreadable, policy}`，`block` = 别名块现状 `{present, version, outdated, conflictingFunctions, manualCleanupHint}`（`conflictingFunctions` = 块外自己定义的、与清单里某条同名的函数 `{name, line, wins}`；`wins` = 新开的终端里敲这个名字起的是哪一个：`yours`（你写的）· `list`（清单那条）· `unclear`（说不清）"), arg("rcPath", "人另指的那一份（`null` = 不指）：过围栏（只许落在 home 之内 · 符号链接不许跑出去）后并进候选"), arg("shell", "同 `aliases-render`"), out("unparsed", "配置文件里写错的那几处（段名 ＋ 第几行 ＋ 原因）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_read(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-install",
        summary: "存清单进配置文件（按条目改，手写的注释与排版留着），照它重写别名文件、补链接",
        codes: &["bad_args", "refused", "stale"],
        fields: &[out("aliasPath", "配置文件的路径"), arg("aliases", "同 `aliases-render`（有一条不合格、清单里没了的那一段还被别的段基于、或改完整份合不下来 ⇒ 整批不写、`refused`）"), arg("fingerprint", "必给（字符串或 `null`）：读回时配置文件的指纹（`aliases-read` 的 `fingerprint`）；盘上此刻不是那一份 ⇒ `stale`"), out("reload", "这种 shell 的别名文件真改了 ⇒ 给人的那一句「已开的终端要运行「. <别名文件>」或新开一个」；只动了配置文件或链接 ⇒ `null`（规则每次起会话现读，链接马上能用）"), arg("shell", "同 `aliases-render`（有一条不合格 ⇒ 整批不写、`refused`）"), out("wroteAliasFile", "配置文件 / 别名文件 / 链接动没动（内容一致就一个字节不写）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_install(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 设置窗「别名与配置文件」那一页的五条（`assets/aliases/page.rs`）：合并只问 `profile::resolve`，界面只排版。
    CommandSpec {
        name: "profiles-read",
        summary: "读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填",
        codes: &["refused"],
        fields: &[out("binDir", "链接住的目录（`~/.cc-monitor/bin`）"), out("exists", "配置文件在不在"), out("fileProblem", "TOML 本身写坏 ⇒ `{line, message}`（这时 `profiles` 为空、不能按条目改）；否则 `null`"), out("fingerprint", "盘上那份的指纹（不在 ⇒ `null`），存的时候交回 `profiles-write`"), out("home", "这台的家目录（界面拿它把路径写成 `~/…`）"), out("migrated", "旧别名清单一次性转进来之后那张说明 `{count, path, skipped}`（「知道了」之后 `null`）"), out("modified", "盘上那份的修改时间（Unix 秒；不在 ⇒ `null`）"), out("path", "配置文件的路径"), out("profiles", "每段 `{name, from, own: [{key, slot, vals, line}], agent, usable, problem: {line, message} | null, kind: link/function, functionWhy, functionLine, said, form}`：`said` 是树里那一行（自己写的几项，「标签 值」）；`form` 是表单回填（没写的格 `null` ＝ 继承）；`problem` 的原话与终端里敲这个名字得到的同一句"), out("seed", "配置文件不在时首建那两条的预览（同 `profiles` 一条的形状；在 ⇒ `[]`）")],
        takes_input: true,
        run: Run::Blocking(|r| crate::assets::aliases::page::answer_read(&LocalFiles, &r.args).map(Some).map_err(|(c, m)| (c.to_string(), m))),
    },
    CommandSpec {
        name: "profiles-resolve",
        summary: "一段合下来的合并表与「等于」那一行（可按未存的表单算）",
        codes: &["bad_args", "refused"],
        fields: &[arg("at", "假设在这个目录敲（`~` 打头按这台家目录展开；`null` ＝ 家目录）"), out("chain", "继承链（父 → 子）"), arg("edit", "未存的表单（同 `profiles-read` 一段的 `form`；`null` ＝ 按盘上那份算）"), out("line", "这台后端算的「等于」那一行（同 `ccm @名 -- --ccm-print`）；算不出 ⇒ `null`"), out("lineError", "算不出那一行时 ccm 的原话"), both("name", "哪一段（带 `edit` 时是正在改的那一段原来的名字，新增写表单里的名字）"), out("problem", "合不下来 ⇒ 那一句（同终端里敲这个名字）；否则 `null`"), out("rows", "合并表 `[{key, slot, label, vals, said, from, overriddenBy}]`：父 → 子、层内照写的顺序；被后来那一层盖掉的也在，`overriddenBy` 是盖掉它的那一段")],
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
        fields: &[arg("changes", "依次做的改动：`{op: \"set\", was, form}`（新增 `was: null`；名字变了 ⇒ 改名，基于它的跟着改）· `{op: \"remove\", name, children}`（还被基于 ⇒ `children` 必给：`reparent` 改成基于它的父 · `cascade` 一起删）· `{op: \"init\", seed}`（配置文件不在时建：`seed` ⇒ 带首建那两条）· `{op: \"ackMigrated\"}`（迁移说明知道了）。改完多出坏处 ⇒ 整批不写、`refused`"), both("fingerprint", "入：必给（字符串或 `null`），读回时的指纹；盘上此刻不是那一份 ⇒ `stale`。出：写完那一份的指纹"), out("modified", "写完那一份的修改时间（Unix 秒）"), out("reload", "终端函数那份文件真改了 ⇒ 给人的那一句「已开的终端要重读」；否则 `null`"), out("wrote", "配置文件 / 终端函数文件 / 链接动没动")],
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
