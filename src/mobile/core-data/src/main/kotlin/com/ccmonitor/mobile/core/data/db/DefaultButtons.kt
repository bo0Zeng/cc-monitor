@file:Suppress("MatchingDeclarationName") // 种子数据类、种子列表与写入函数放在一处

package com.ccmonitor.mobile.core.data.db

import androidx.sqlite.db.SupportSQLiteDatabase

/*
 * 默认按键种子：功能键、vim 键、顶部工具条内建都是可编辑的 custom_buttons 行。
 *
 * 升级走迁移、新装走 AppDatabase 的 RoomDatabase.Callback.onCreate，两处都调 seedDefaultButtons；
 * Room 迁移只在升级时跑，只写迁移的话新装一个键都没有。
 * INSERT OR IGNORE + 固定 id，幂等：不覆盖用户改过的同 id 行，不碰其他按钮。
 *
 * fn/vim 用 escape 类型，command 经 escapeSequenceBytes 出字节（\e=0x1b、\t=0x09、vim 原字符透传、↵=\n）。
 * main 组工具条用 builtin 类型（show_tabs/command_palette/tmux_sessions，不发字节）。
 */

/** 一条种子按钮定义（[group] = [CustomButton.btnGroup]；[sortOrder] 按列表序生成）。 */
internal data class SeedButton(
    val id: String,
    val label: String,
    val type: String,
    val command: String,
    val group: String,
)

internal val DEFAULT_SEED_BUTTONS: List<SeedButton> =
    listOf(
        // main 组：顶部工具条内建（builtin 类型；tmux 会话在渲染侧按 supportsTmux 门控）。
        // 放在列表最前，sortOrder 最低，`▤ tab`/`命令` 在按钮条最左。
        SeedButton("ak-tb-tabs", "▤ tab", "builtin", "show_tabs", ButtonGroups.MAIN),
        SeedButton("ak-tb-cmd", "命令", "builtin", "command_palette", ButtonGroups.MAIN),
        SeedButton("ak-tb-tmux", "tmux 会话", "builtin", "tmux_sessions", ButtonGroups.MAIN),
        // fn 组（近键盘默认排）：`⇌` 切到 alt 组；`Ctrl` 粘性（builtin）；Tab/Esc/方向键 escape；粘贴。
        // `⇌` 是 switch 类型，command=目标组，fn→alt→vim→fn 循环。
        SeedButton("ak-fn-switch", "⇌", "switch", ButtonGroups.ALT, ButtonGroups.FN),
        SeedButton("ak-fn-ctrl", "Ctrl", "builtin", "toggle_ctrl", ButtonGroups.FN),
        SeedButton("ak-fn-tab", "Tab", "escape", "\\t", ButtonGroups.FN),
        SeedButton("ak-fn-esc", "Esc", "escape", "\\e", ButtonGroups.FN),
        SeedButton("ak-fn-up", "↑", "escape", "\\e[A", ButtonGroups.FN),
        SeedButton("ak-fn-down", "↓", "escape", "\\e[B", ButtonGroups.FN),
        SeedButton("ak-fn-left", "←", "escape", "\\e[D", ButtonGroups.FN),
        SeedButton("ak-fn-right", "→", "escape", "\\e[C", ButtonGroups.FN),
        SeedButton("ak-fn-paste", "粘贴", "builtin", "paste", ButtonGroups.FN),
        // alt 组（第二排）：`⇌` 切到 vim 组；`Alt` 粘性；Ctrl+End/Home/End/PgUp/PgDn/Esc escape（xterm 序列）。
        SeedButton("ak-alt-switch", "⇌", "switch", ButtonGroups.VIM, ButtonGroups.ALT),
        SeedButton("ak-alt-alt", "Alt", "builtin", "toggle_alt", ButtonGroups.ALT),
        SeedButton("ak-alt-ctrlend", "Ctrl+End", "escape", "\\e[1;5F", ButtonGroups.ALT),
        SeedButton("ak-alt-home", "Home", "escape", "\\e[H", ButtonGroups.ALT),
        SeedButton("ak-alt-end", "End", "escape", "\\e[F", ButtonGroups.ALT),
        SeedButton("ak-alt-pgup", "PgUp", "escape", "\\e[5~", ButtonGroups.ALT),
        SeedButton("ak-alt-pgdn", "PgDn", "escape", "\\e[6~", ButtonGroups.ALT),
        SeedButton("ak-alt-esc", "Esc", "escape", "\\e", ButtonGroups.ALT),
        // vim 组：`⇌` 切回 fn 组（闭环）；其余 19 键 VIM_KEYS 对应（escape 类型，原字符透传；`↵`=`\n` 回车执行）
        SeedButton("ak-vim-switch", "⇌", "switch", ButtonGroups.FN, ButtonGroups.VIM),
        SeedButton("ak-vim-esc", "Esc", "escape", "\\e", ButtonGroups.VIM),
        SeedButton("ak-vim-i", "i", "escape", "i", ButtonGroups.VIM),
        SeedButton("ak-vim-colon", ":", "escape", ":", ButtonGroups.VIM),
        SeedButton("ak-vim-slash", "/", "escape", "/", ButtonGroups.VIM),
        SeedButton("ak-vim-w", ":w↵", "escape", ":w\\n", ButtonGroups.VIM),
        SeedButton("ak-vim-wq", ":wq↵", "escape", ":wq\\n", ButtonGroups.VIM),
        SeedButton("ak-vim-q", ":q!↵", "escape", ":q!\\n", ButtonGroups.VIM),
        SeedButton("ak-vim-dd", "dd", "escape", "dd", ButtonGroups.VIM),
        SeedButton("ak-vim-yy", "yy", "escape", "yy", ButtonGroups.VIM),
        SeedButton("ak-vim-p", "p", "escape", "p", ButtonGroups.VIM),
        SeedButton("ak-vim-u", "u", "escape", "u", ButtonGroups.VIM),
        SeedButton("ak-vim-gg", "gg", "escape", "gg", ButtonGroups.VIM),
        SeedButton("ak-vim-G", "G", "escape", "G", ButtonGroups.VIM),
        SeedButton("ak-vim-h", "h", "escape", "h", ButtonGroups.VIM),
        SeedButton("ak-vim-j", "j", "escape", "j", ButtonGroups.VIM),
        SeedButton("ak-vim-k", "k", "escape", "k", ButtonGroups.VIM),
        SeedButton("ak-vim-l", "l", "escape", "l", ButtonGroups.VIM),
        SeedButton("ak-vim-0", "0", "escape", "0", ButtonGroups.VIM),
        SeedButton("ak-vim-dollar", "$", "escape", "$", ButtonGroups.VIM),
    )

/**
 * 把 [DEFAULT_SEED_BUTTONS] 写入 `custom_buttons`（`INSERT OR IGNORE`，全局 hostId=NULL），ctrl/alt/shift 取列默认 0。
 * 迁移与 onCreate 用（只有 raw [SupportSQLiteDatabase]）；有 DAO 的路径走 [defaultButtonEntities]。
 */
fun seedDefaultButtons(db: SupportSQLiteDatabase) {
    DEFAULT_SEED_BUTTONS.forEachIndexed { i, b ->
        db.execSQL(
            "INSERT OR IGNORE INTO `custom_buttons` (`id`,`hostId`,`label`,`command`,`sortOrder`,`type`,`btnGroup`) " +
                "VALUES (?, NULL, ?, ?, ?, ?, ?)",
            arrayOf<Any?>(b.id, b.label, b.command, i, b.type, b.group),
        )
    }
}

/**
 * [DEFAULT_SEED_BUTTONS] 映射成 Room 实体，给恢复默认按键用（有 DAO、无 raw db）。
 * 字段必须与 [seedDefaultButtons] 的 raw SQL 一致：同一份列表、下标做 sortOrder、hostId=null、ctrl/alt/shift 取默认。
 */
internal fun defaultButtonEntities(): List<CustomButton> =
    DEFAULT_SEED_BUTTONS.mapIndexed { i, b ->
        CustomButton(id = b.id, hostId = null, label = b.label, command = b.command, sortOrder = i, type = b.type, btnGroup = b.group)
    }

/**
 * 恢复默认按键：把被删的默认键补回。`INSERT OR IGNORE` 固定 id，不覆盖用户改过的同 id 行，不碰自建按钮。
 * 覆盖 main/fn/alt/vim 四组 40 条。经注入的 [CustomButtonDao] 调，不碰 raw db。
 */
suspend fun CustomButtonDao.restoreDefaultButtons() = insertIgnore(defaultButtonEntities())
