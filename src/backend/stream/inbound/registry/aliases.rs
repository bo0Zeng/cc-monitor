//! 命令表 · 别名：`aliases-*`。

use crate::stream::inbound::spec::CommandSpec;
use crate::stream::inbound::LocalFiles;

pub(super) const SPECS: &[CommandSpec] = &[
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::AliasesRead>(
        "启动文件候选（接入那一格）",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::page::ProfilesRead>(
        "读回配置文件整份：每段自己写的几项 · 能不能用 · 链接还是终端函数 · 表单回填",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::page::ProfilesResolve>(
        "一段合下来的合并表与「等于」那一行（可按未存的表单算）",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::page::ProfilesImpactCmd>(
        "这几处改动会让哪几段合下来变（改前改后）",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::page::ProfilesBasesCmd>(
        "「基于」下拉能选的几段（选了不成圈）",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::page::ProfilesWrite>(
        "按条目改配置文件（手写的注释与排版留着），照它补链接 / 终端函数",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::AliasesBlockRender>(
        "别名块预览",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::AliasesBlockInstall>(
        "别名块装进人选的那份启动文件",
        &LocalFiles,
    ),
    crate::stream::inbound::typed::with_files::<crate::assets::aliases::AliasesBlockRemove>(
        "别名块卸掉",
        &LocalFiles,
    ),
];
