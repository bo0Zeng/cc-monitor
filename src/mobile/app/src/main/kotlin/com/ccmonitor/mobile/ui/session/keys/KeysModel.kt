package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.ResetPreview
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.UnconvertedNote
import com.ccmonitor.mobile.core.data.db.appActionName
import com.ccmonitor.mobile.core.data.db.autoCap
import com.ccmonitor.mobile.core.data.db.isRepeatable
import com.ccmonitor.mobile.core.data.db.modName
import com.ccmonitor.mobile.core.data.repo.SettingsRepository

/*
 * 编辑按键那几屏的判定：不碰 Compose，屏只照这里画。
 */

/** 一颗按键的三个触发。 */
enum class Trigger { TAP, DOUBLE, LONG }

/** 一个触发那三段：没设 · 送输入 · App 动作。 */
enum class BindKind { NONE, INPUT, ACTION }

/**
 * 一个触发的那一格。三段各记各的（切走再切回来，录好的内容还在）；真正存的是 [binding]。
 * 选了「送输入」还没录 ⇒ 当没设（单击就还是必填）。
 */
data class Slot(
    val kind: BindKind = BindKind.NONE,
    val input: List<KeyPress> = emptyList(),
    val action: KeyBinding.App? = null,
) {
    val binding: KeyBinding?
        get() =
            when (kind) {
                BindKind.NONE -> null
                BindKind.INPUT -> input.takeIf { it.isNotEmpty() }?.let { KeyBinding.Input(it) }
                BindKind.ACTION -> action
            }

    companion object {
        fun of(b: KeyBinding?): Slot =
            when (b) {
                null -> Slot()
                is KeyBinding.Input -> Slot(BindKind.INPUT, input = b.presses)
                is KeyBinding.App -> Slot(BindKind.ACTION, action = b)
            }
    }
}

/** 编辑一颗时的草稿。[id] 为 null ＝ 新建。[label] 空 ＝ 照单击自动写。 */
data class KeyDraft(
    val id: String?,
    val groupId: String,
    val label: String = "",
    val modifier: KeyMod? = null,
    val tap: Slot = Slot(),
    val doubleTap: Slot = Slot(),
    val longPress: Slot = Slot(),
    val doubleTapMs: Int? = null,
    val longPressMs: Int? = null,
) {
    fun slot(t: Trigger): Slot =
        when (t) {
            Trigger.TAP -> tap
            Trigger.DOUBLE -> doubleTap
            Trigger.LONG -> longPress
        }

    fun withSlot(
        t: Trigger,
        s: Slot,
    ): KeyDraft =
        when (t) {
            Trigger.TAP -> copy(tap = s)
            Trigger.DOUBLE -> copy(doubleTap = s)
            Trigger.LONG -> copy(longPress = s)
        }

    /** 单击必填（修饰键不绑触发）；另设的时长越界不许存（不悄悄夹回去）。 */
    val canSave: Boolean
        get() =
            (modifier != null || tap.binding != null) &&
                doubleTapMs.inOrNull(SettingsRepository.DOUBLE_TAP_MS_RANGE) &&
                longPressMs.inOrNull(SettingsRepository.LONG_PRESS_MS_RANGE)

    /**
     * 写回库的那一颗：改过就没有「未能转换」那条记号了；默认来源 · 图标照原样留。换了组 ⇒ 排到新组最后（[endOf]）。
     * 修饰键三个触发恒为空。
     */
    fun toKey(
        existing: TermKey?,
        endOf: (String) -> Int,
        newId: () -> String,
    ): TermKey {
        val order = if (existing != null && existing.groupId == groupId) existing.sortOrder else endOf(groupId)
        val base = existing ?: TermKey(id = newId(), groupId = groupId, sortOrder = order)
        return base.copy(
            groupId = groupId,
            sortOrder = order,
            label = label.trim().ifEmpty { null },
            modifier = modifier,
            tap = if (modifier == null) tap.binding else null,
            doubleTap = if (modifier == null) doubleTap.binding else null,
            longPress = if (modifier == null) longPress.binding else null,
            doubleTapMs = if (modifier == null) doubleTapMs else null,
            longPressMs = if (modifier == null) longPressMs else null,
            unconverted = null,
        )
    }

    companion object {
        fun of(k: TermKey): KeyDraft =
            KeyDraft(
                id = k.id,
                groupId = k.groupId,
                label = k.label.orEmpty(),
                modifier = k.modifier,
                tap = Slot.of(k.tap),
                doubleTap = Slot.of(k.doubleTap),
                longPress = Slot.of(k.longPress),
                doubleTapMs = k.doubleTapMs,
                longPressMs = k.longPressMs,
            )
    }
}

private fun Int?.inOrNull(r: IntRange) = this == null || this in r

/** 没设时那一行灰字（自然行为）；单击没设 ＝「必填」。 */
fun naturalHint(
    t: Trigger,
    tap: KeyBinding?,
): String =
    when (t) {
        Trigger.TAP -> copyText("mobKeys.natural.tap")
        Trigger.DOUBLE -> copyText("mobKeys.natural.double")
        Trigger.LONG ->
            when {
                tap == null -> copyText("mobKeys.natural.longEither")
                tap.isRepeatable() -> copyText("mobKeys.natural.longRepeat")
                else -> copyText("mobKeys.natural.longOnce")
            }
    }

/** 双击绑上之后当场那一句：单击晚多久才送。 */
fun doubleCostLine(ms: Int): String = copyText("mobKeys.double.cost", "ms" to ms)

/** 一个触发绑的东西叫什么（App 动作写动作名，切到某一组写组名；送输入照键帽写法）。 */
fun bindingName(
    b: KeyBinding,
    groupName: (String) -> String?,
): String =
    when (b) {
        is KeyBinding.App ->
            when (b.action) {
                AppAction.GOTO_GROUP -> b.groupId?.let(groupName) ?: appActionName(b.action)
                else -> appActionName(b.action)
            }
        is KeyBinding.Input -> autoCap(b)
    }

/** 编辑页每颗那一行的第一行：单击做什么（修饰键写「修饰键」；单击没设 ⇒ 空）。 */
fun firstLine(
    k: TermKey,
    groupName: (String) -> String?,
): String =
    when {
        k.modifier != null -> copyText("mobKeys.mod.title")
        else -> k.tap?.let { bindingName(it, groupName) }.orEmpty()
    }

/** 第二行：设了的双击 / 长按；都没设 ⇒ null（不写）。 */
fun secondLine(
    k: TermKey,
    groupName: (String) -> String?,
): String? =
    listOfNotNull(
        k.doubleTap?.let { copyText("mobKeys.trigger.double") + " " + bindingName(it, groupName) },
        k.longPress?.let { copyText("mobKeys.trigger.long") + " " + bindingName(it, groupName) },
    ).joinToString(" · ").ifEmpty { null }

/** 这颗绑着「切到某一组」、而那组已经不在了的触发。 */
fun goneTargets(
    d: KeyDraft,
    groups: List<KeyGroup>,
): Set<Trigger> =
    Trigger.entries
        .filter { t ->
            val b = d.slot(t).binding as? KeyBinding.App
            b?.action == AppAction.GOTO_GROUP && groups.none { it.id == b.groupId }
        }.toSet()

/** 新建组 · 改名：组名去掉首尾空白；空 ⇒ 不许建；和别的组重名 ⇒ 那句报错。[self] 是改名时自己那一组。 */
sealed interface NameCheck {
    data class Ok(
        val name: String,
    ) : NameCheck

    data object Empty : NameCheck

    data class Dup(
        val message: String,
    ) : NameCheck
}

fun checkGroupName(
    raw: String,
    groups: List<KeyGroup>,
    self: String? = null,
): NameCheck {
    val name = raw.trim()
    if (name.isEmpty()) return NameCheck.Empty
    return if (groups.any { it.id != self && it.name == name }) NameCheck.Dup(copyText("mobKeys.group.nameDup", "name" to name)) else NameCheck.Ok(name)
}

/** 把 [from] 那一项挪到 [to]，先后照新位置重编（0 起）。越界 ⇒ 原样。 */
fun <T> List<T>.moved(
    from: Int,
    to: Int,
): List<T> {
    if (from !in indices || to !in indices || from == to) return this
    val m = toMutableList()
    m.add(to, m.removeAt(from))
    return m
}

fun List<KeyGroup>.renumbered(): List<KeyGroup> = mapIndexed { i, g -> g.copy(sortOrder = i) }

fun List<TermKey>.renumberedKeys(): List<TermKey> = mapIndexed { i, k -> k.copy(sortOrder = i) }

/** 升级那一次没转过来的一处，照实说哪里没转过来。 */
fun issueLine(n: UnconvertedNote): String =
    when (n.issue) {
        UnconvertedNote.Issue.TAP_UNPARSED -> copyText("mobKeys.migrate.tapUnparsed")
        UnconvertedNote.Issue.LONG_UNPARSED -> copyText("mobKeys.migrate.longUnparsed")
        UnconvertedNote.Issue.LONG_NO_REPEAT -> copyText("mobKeys.migrate.noRepeat")
    }

/** 恢复默认确认框的正文两行：（改动，回到默认的那几个默认组）· （删除，自建组与改过的颗数）。没有的那一行 ⇒ null。 */
data class ResetLines(
    val changed: String?,
    val deleted: String?,
)

fun resetLines(p: ResetPreview): ResetLines {
    val changed = p.changedDefaultGroups.takeIf { it.isNotEmpty() }?.let { copyText("mobKeys.reset.back", "names" to it.joinToString(" · ")) }
    val deleted =
        listOfNotNull(
            p.deletedGroups.takeIf { it.isNotEmpty() }?.let { gs -> copyText("mobKeys.reset.groups", "names" to gs.joinToString(" · ") { "「${it.name}」" }) },
            p.changedKeys.takeIf { it > 0 }?.let { copyText("mobKeys.reset.keys", "n" to it) },
        ).joinToString(" · ").ifEmpty { null }
    return ResetLines(changed, deleted)
}

/** 修饰键那页单击一行的意思。 */
fun modTapLine(m: KeyMod): String = copyText("mobKeys.mod.tap", "mod" to modName(m))

/** 时长那一格的写法：「300 ms · 跟系统」「220 ms · 另设」（单颗：「跟总的」）。 */
fun durationLine(
    own: Int?,
    effective: Int,
    single: Boolean,
): String {
    val how =
        when {
            own != null -> copyText("mobKeys.dur.custom")
            single -> copyText("mobKeys.dur.followAll")
            else -> copyText("mobKeys.dur.followSys")
        }
    return "$effective ms · $how"
}

/** 删一颗 · 删一组之后撤销窗开多久。 */
const val UNDO_WINDOW_MS: Long = 8_000L
