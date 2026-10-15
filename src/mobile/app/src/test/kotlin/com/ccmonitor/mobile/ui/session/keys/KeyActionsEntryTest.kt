package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.ui.session.KeyActions
import org.junit.Assert.assertEquals
import org.junit.Test

/** 键条上「编辑按键」进编辑页；要 tmux 的在没 tmux 的服务器上不做、交给那一句（出错不许点了没反应）。 */
class KeyActionsEntryTest {
    private val hits = mutableListOf<String>()

    private fun actions(tmux: Boolean) =
        KeyActions(
            supportsTmux = tmux,
            onShowTabs = { hits += "tabs" },
            onCommandPalette = { hits += "palette" },
            onTmuxSessions = { hits += "tmux" },
            onPaste = { hits += "paste" },
            onCapture = { hits += "capture" },
            onPushKey = { hits += "push" },
            onShowKeyboard = { hits += "ime" },
            onEditKeys = { hits += "edit" },
            onNextGroup = { hits += "next" },
            onGotoGroup = { hits += "goto:$it" },
            onNeedsTmux = { hits += "needs:${it.wire}" },
        )

    @Test
    fun editKeysOpensTheEditor() {
        actions(tmux = false).invoke(KeyBinding.App(AppAction.EDIT_KEYS))
        assertEquals(listOf("edit"), hits)
    }

    @Test
    fun tmuxOnlyActionsSayWhyInsteadOfDoingNothing() {
        val a = actions(tmux = false)
        AppAction.NEEDS_TMUX.forEach { a.invoke(KeyBinding.App(it)) }
        assertEquals(AppAction.NEEDS_TMUX.map { "needs:${it.wire}" }, hits)
        hits.clear()
        actions(tmux = true).invoke(KeyBinding.App(AppAction.CAPTURE))
        assertEquals(listOf("capture"), hits)
    }
}
