package com.ccmonitor.mobile.core.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import com.ccmonitor.mobile.core.copy.copyText

/*
 * 默认三组：常用 · shell · vim。每组最左一颗 ⇌（单击切到下一组、长按编辑按键）。
 * 「常用」照答 Claude 卡住的菜单来排（Esc · Ctrl-C · ↑↓ · 回车 · Shift-Tab）。
 *
 * 新装走 AppDatabase 的 onCreate、「恢复默认」走 [KeyDao.replaceAll]，读的是同一份 [defaultKeyGroups] / [defaultKeys]。
 * 默认那几颗 id 固定、[TermKey.defaultId] 等于自己的 id；「还原一颗」照 defaultId 找回原样。
 */

const val DEFAULT_GROUP_COMMON = "kg-common"
const val DEFAULT_GROUP_SHELL = "kg-shell"
const val DEFAULT_GROUP_VIM = "kg-vim"

private fun special(
    k: SpecialKey,
    vararg mods: KeyMod,
) = KeyBinding.Input(listOf(KeyPress.Special(k, mods.toSet())))

private fun chord(
    c: String,
    vararg mods: KeyMod,
) = KeyBinding.Input(listOf(KeyPress.Chord(c, mods.toSet())))

private fun text(s: String) = KeyBinding.Input(listOf(KeyPress.Text(s)))

private fun line(s: String) = KeyBinding.Input(listOf(KeyPress.Text(s), KeyPress.Special(SpecialKey.ENTER)))

private fun app(a: AppAction) = KeyBinding.App(a)

/** 一颗默认按键的样子（不带组与先后）。 */
private data class Seed(
    val id: String,
    val tap: KeyBinding? = null,
    val longPress: KeyBinding? = null,
    val modifier: KeyMod? = null,
)

private fun switchKey(group: String) = Seed("dk-$group-switch", tap = app(AppAction.NEXT_GROUP), longPress = app(AppAction.EDIT_KEYS))

private val SEEDS: List<Pair<String, List<Seed>>> =
    listOf(
        DEFAULT_GROUP_COMMON to
            listOf(
                switchKey("common"),
                Seed("dk-common-esc", special(SpecialKey.ESC)),
                Seed("dk-common-ctrlc", chord("c", KeyMod.CTRL)),
                Seed("dk-common-up", special(SpecialKey.UP)),
                Seed("dk-common-down", special(SpecialKey.DOWN)),
                Seed("dk-common-enter", special(SpecialKey.ENTER)),
                Seed("dk-common-tab", special(SpecialKey.TAB)),
                Seed("dk-common-backtab", special(SpecialKey.TAB, KeyMod.SHIFT)),
                Seed("dk-common-paste", app(AppAction.PASTE)),
            ),
        DEFAULT_GROUP_SHELL to
            listOf(
                switchKey("shell"),
                Seed("dk-shell-ctrl", modifier = KeyMod.CTRL),
                Seed("dk-shell-alt", modifier = KeyMod.ALT),
                Seed("dk-shell-tab", special(SpecialKey.TAB)),
                Seed("dk-shell-left", special(SpecialKey.LEFT)),
                Seed("dk-shell-right", special(SpecialKey.RIGHT)),
                Seed("dk-shell-home", special(SpecialKey.HOME)),
                Seed("dk-shell-end", special(SpecialKey.END)),
                Seed("dk-shell-pgup", special(SpecialKey.PAGE_UP)),
                Seed("dk-shell-pgdn", special(SpecialKey.PAGE_DOWN)),
                Seed("dk-shell-backspace", special(SpecialKey.BACKSPACE)),
                Seed("dk-shell-delete", special(SpecialKey.DELETE)),
            ),
        DEFAULT_GROUP_VIM to
            listOf(
                switchKey("vim"),
                Seed("dk-vim-esc", special(SpecialKey.ESC)),
                Seed("dk-vim-i", text("i")),
                Seed("dk-vim-colon", text(":")),
                Seed("dk-vim-slash", text("/")),
                Seed("dk-vim-dd", text("dd")),
                Seed("dk-vim-u", text("u")),
                Seed("dk-vim-p", text("p")),
                Seed("dk-vim-w", line(":w")),
                Seed("dk-vim-wq", line(":wq")),
                Seed("dk-vim-q", line(":q!")),
            ),
    )

/** 默认三组，先后就是下标。「常用」的名字取自文案表；shell · vim 是原样的词。 */
fun defaultKeyGroups(): List<KeyGroup> =
    listOf(
        KeyGroup(DEFAULT_GROUP_COMMON, copyText("mobKeys.seed.common"), 0),
        KeyGroup(DEFAULT_GROUP_SHELL, "shell", 1),
        KeyGroup(DEFAULT_GROUP_VIM, "vim", 2),
    )

/** 默认按键，组内先后就是下标。 */
fun defaultKeys(): List<TermKey> =
    SEEDS.flatMap { (group, seeds) ->
        seeds.mapIndexed { i, s ->
            TermKey(id = s.id, groupId = group, sortOrder = i, modifier = s.modifier, tap = s.tap, longPress = s.longPress, defaultId = s.id)
        }
    }

/** 新装那一次把默认三组写进库（onCreate 只有 raw db）。 */
fun seedDefaultKeys(db: SupportSQLiteDatabase) = insertKeyRows(db, defaultKeyGroups(), defaultKeys())

/** 恢复默认：真的回到默认（自建组与改过的按键都没了），不是只补缺的。 */
suspend fun KeyDao.restoreDefaults() = replaceAll(defaultKeyGroups(), defaultKeys())

/** 这颗默认按键原来的样子；不是从默认来的（或那颗默认已不在默认里）⇒ null。 */
fun TermKey.defaultShape(): TermKey? = defaultId?.let { d -> defaultKeys().firstOrNull { it.id == d } }

/** 这颗是默认按键、且改过（键帽 · 图标 · 修饰 · 三个触发 · 两个时长有一处不同）。编辑一颗时据此多一颗「还原默认」。 */
fun TermKey.isModifiedDefault(): Boolean = defaultShape()?.let { !sameContent(it) } ?: false

/** 还原一颗：内容回到默认原样，留在现在的组与位置；没转过来的记号一并清掉。不是从默认来的 ⇒ 原样返回。 */
fun TermKey.revertedToDefault(): TermKey =
    defaultShape()?.let {
        copy(
            label = it.label,
            icon = it.icon,
            modifier = it.modifier,
            tap = it.tap,
            doubleTap = it.doubleTap,
            longPress = it.longPress,
            doubleTapMs = it.doubleTapMs,
            longPressMs = it.longPressMs,
            unconverted = null,
        )
    } ?: this

/** 还原一颗并写回库；回的是写进去的那一颗。 */
suspend fun KeyDao.revertToDefault(key: TermKey): TermKey = key.revertedToDefault().also { upsertKey(it) }

private fun TermKey.sameContent(o: TermKey) =
    label == o.label &&
        icon == o.icon &&
        modifier == o.modifier &&
        tap == o.tap &&
        doubleTap == o.doubleTap &&
        longPress == o.longPress &&
        doubleTapMs == o.doubleTapMs &&
        longPressMs == o.longPressMs

/**
 * 「恢复默认按键」确认框要列的东西。
 * - [changedDefaultGroups]：会回到默认的那几个默认组（组名 · 先后 · 组里的按键有一处不同，或整组被删了）；
 * - [deletedGroups]：会被删掉的自建组；
 * - [changedKeys]：留下的默认组里会没了或回原样的按键颗数（改过的默认按键 ＋ 自己加进去的）。
 * 一处都没改过 ⇒ [pristine]，那一项灰掉。
 */
data class ResetPreview(
    val changedDefaultGroups: List<String>,
    val deletedGroups: List<KeyGroup>,
    val changedKeys: Int,
) {
    val pristine: Boolean get() = changedDefaultGroups.isEmpty() && deletedGroups.isEmpty() && changedKeys == 0
}

fun resetPreview(
    groups: List<KeyGroup>,
    keys: List<TermKey>,
): ResetPreview {
    val defaults = defaultKeyGroups()
    val defaultIds = defaults.map { it.id }.toSet()
    val defaultKeysByGroup = defaultKeys().groupBy { it.groupId }
    val changedKeys =
        keys.count { k ->
            k.groupId in defaultIds && k.defaultShape()?.let { d -> !k.sameContent(d) || k.groupId != d.groupId } != false
        }
    val ordered = groups.sortedWith(compareBy({ it.sortOrder }, { it.id }))
    val changedGroups =
        defaults
            .filter { d ->
                val now = ordered.firstOrNull { it.id == d.id }
                val nowKeys = keys.filter { it.groupId == d.id }.sortedWith(compareBy({ it.sortOrder }, { it.id }))
                now == null ||
                    now.name != d.name ||
                    ordered.indexOf(now) != d.sortOrder ||
                    nowKeys.map { it.copy(sortOrder = 0) } != defaultKeysByGroup[d.id].orEmpty().map { it.copy(sortOrder = 0) }
            }.map { it.name }
    return ResetPreview(changedGroups, groups.filter { it.id !in defaultIds }, changedKeys)
}

/** 往 raw db 里写几组与几颗按键（迁移与新装用；有 DAO 的路径走 [KeyDao]）。各格照 [KeyConverters] 存。 */
internal fun insertKeyRows(
    db: SupportSQLiteDatabase,
    groups: List<KeyGroup>,
    keys: List<TermKey>,
) {
    val conv = KeyConverters()
    groups.forEach { g ->
        db.execSQL("INSERT INTO `key_groups` (`id`,`name`,`sortOrder`) VALUES (?, ?, ?)", arrayOf<Any?>(g.id, g.name, g.sortOrder))
    }
    keys.forEach { k ->
        db.execSQL(
            "INSERT INTO `term_keys` (`id`,`groupId`,`sortOrder`,`label`,`icon`,`modifier`,`tap`,`doubleTap`,`longPress`," +
                "`doubleTapMs`,`longPressMs`,`defaultId`,`unconverted`) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
            arrayOf<Any?>(
                k.id,
                k.groupId,
                k.sortOrder,
                k.label,
                k.icon,
                conv.modToText(k.modifier),
                conv.bindingToText(k.tap),
                conv.bindingToText(k.doubleTap),
                conv.bindingToText(k.longPress),
                k.doubleTapMs,
                k.longPressMs,
                k.defaultId,
                conv.notesToText(k.unconverted),
            ),
        )
    }
}
