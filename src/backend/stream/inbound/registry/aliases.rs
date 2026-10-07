//! 命令表 · 别名：`aliases-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // 别名六条：`assets/aliases/`（规则 · 方言 · 围栏），读写经 [`LocalFiles`]。
    CommandSpec {
        name: "aliases-render",
        summary: "清单 → 代码",
        codes: &["bad_args", "refused"],
        fields: &[arg("aliases", "清单：每条 `{name, args, restTo}`（`args` 是原样的 ccm argv"), out("collisions", "撞名提示（自带别名块 · **这台** `PATH` 上的同名程序 · PowerShell 内建别名；只出声、不拦）"), out("fileText", "整份文件"), out("lines", "每条合格别名的写法"), out("problems", "不合格的那几条 `{name, message}`（非空时 `aliases-install` 一个字节都不写）"), arg("shell", "`posix` / `powershell`（这台后端不在 Windows ⇒ `powershell` 拒）")],
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
        fields: &[out("accounts", "这台的账号表（具名号，按账号库的顺序；没有账号库 ⇒ `[]`）"), out("aliasPath", "这台上那份别名文件的路径"), out("aliases", "读回的清单（不在 ⇒ 首建会带上的 `cc` · `cct` · `cca`，PowerShell 只有 `cc`）"), out("exists", "在不在"), out("fingerprint", "盘上那份别名文件的指纹（不透明的串：长度 ＋ 一个 64 位散列；不在 ⇒ `null`），存的时候交回 `aliases-install`"), out("groups", "与 `aliases` 逐条对应：参数恰是「某号」或「某号 ＋ tmux」⇒ `{account, tmux}`，其余 ⇒ `null`（只看参数，不看名字；界面照这一格分组）"), out("missing", "账号表里的号缺哪一条：`{account, tmux, alias}`（`alias` 就是点「加上」要加进清单的那一条；没有 tmux 的目标只看 `<号>cc`）"), out("otherRc", "`rcPath` 过了围栏之后的绝对路径"), out("rcCandidates", "启动文件候选（方言答列哪几份）：每份 `{path, sourced, exists, block, unreadable, policy}`，`block` = 别名块现状 `{present, version, outdated, conflictingFunctions, manualCleanupHint}`（`conflictingFunctions` = 块外自己定义的、与清单里某条同名的函数 `{name, line, wins}`；`wins` = 新开的终端里敲这个名字起的是哪一个：`yours`（你写的）· `list`（清单那条）· `unclear`（说不清）"), arg("rcPath", "人另指的那一份（`null` = 不指）：过围栏（只许落在 home 之内 · 符号链接不许跑出去）后并进候选"), arg("shell", "同 `aliases-render`"), out("unparsed", "认不出的行（原文带原因）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_read(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-install",
        summary: "写别名文件",
        codes: &["bad_args", "refused", "stale"],
        fields: &[out("aliasPath", "写到哪"), arg("aliases", "同 `aliases-render`（有一条不合格 ⇒ 整批不写、`refused`）"), arg("fingerprint", "必给（字符串或 `null`）：读回时那份的指纹（`aliases-read` 的 `fingerprint`）"), out("reload", "真写了 ⇒ 给人的那一句「已开的终端要运行「. <aliasPath>」或新开一个」（已开着的终端不会自己重读）；没写 ⇒ `null`"), arg("shell", "同 `aliases-render`（有一条不合格 ⇒ 整批不写、`refused`）"), out("wroteAliasFile", "真写了没有（内容一致就一个字节不写）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_install(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
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
