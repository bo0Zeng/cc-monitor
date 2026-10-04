//! 命令表 · 别名：`aliases-*`。

use crate::stream::inbound::spec::{CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    // 别名六条：`assets/aliases/`（规则 · 方言 · 围栏），读写经 [`LocalFiles`]。
    CommandSpec {
        name: "aliases-render",
        doc_anchor: Some("#### `aliases-render`"),
        codes: &["bad_args", "refused"],
        fields: &[
            "aliases",
            "collisions",
            "fileText",
            "lines",
            "problems",
            "shell",
        ],
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
        doc_anchor: Some("#### `aliases-to-form`"),
        codes: &["bad_args"],
        fields: &[
            "account",
            "agent",
            "alias",
            "args",
            "at",
            "base",
            "busNote",
            "busRegister",
            "ccmOther",
            "cwd",
            "cwdIf",
            "detach",
            "form",
            "launcher",
            "model",
            "name",
            "passthru",
            "restTo",
            "tmux",
            "tmuxName",
            "tmuxSize",
            "to",
        ],
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
        doc_anchor: Some("#### `aliases-from-form`"),
        codes: &["bad_args", "refused"],
        fields: &["alias", "form", "orig"],
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
        doc_anchor: Some("#### `aliases-read`"),
        codes: &["bad_args", "refused"],
        fields: &[
            "accounts",
            "aliasPath",
            "aliases",
            "exists",
            "fingerprint",
            "groups",
            "missing",
            "otherRc",
            "rcCandidates",
            "rcPath",
            "shell",
            "unparsed",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_read(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-install",
        doc_anchor: Some("#### `aliases-install`"),
        codes: &["bad_args", "refused", "stale"],
        fields: &[
            "aliasPath",
            "aliases",
            "fingerprint",
            "shell",
            "wroteAliasFile",
        ],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_install(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-block-render",
        doc_anchor: Some("#### `aliases-block-render`"),
        codes: &["bad_args", "refused"],
        fields: &["rcPath", "text"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_block_render(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-block-install",
        doc_anchor: Some("#### `aliases-block-install`"),
        codes: &["bad_args", "refused"],
        fields: &["rcPath"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_block_install(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "aliases-block-remove",
        doc_anchor: Some("#### `aliases-block-remove`"),
        codes: &["bad_args", "refused"],
        fields: &["rcPath"],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::assets::aliases::answer_block_remove(&LocalFiles, &r.args)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
];
