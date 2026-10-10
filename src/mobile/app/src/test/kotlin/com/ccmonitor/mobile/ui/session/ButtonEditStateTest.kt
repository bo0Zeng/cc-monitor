package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.CustomButton
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [ButtonEditState.build] 的不变式——重点钉 btnGroup 编辑保原组。
 *
 * 为什么值得一条专门的测试：Room `@Upsert` 是全列覆写，`build()` 漏传 btnGroup 就落实体默认 `main`，
 * 于是「编辑一下 ← 键的标签」= 把它从 `fn` 组搬进 `main` 组 = 功能键排少一个键（键排由
 * btnGroup 决定），而编辑器没有选组 UI，搬不回来。
 */
class ButtonEditStateTest {
    private fun seedFnButton() =
        CustomButton(
            id = "ak-fn-left",
            hostId = null,
            label = "←",
            command = "\\e[D",
            sortOrder = 2,
            type = "escape",
            btnGroup = "fn",
        )

    @Test
    fun editKeepsOriginalGroupAndIdAndSortOrder() {
        val initial = seedFnButton()
        val state = ButtonEditState(initial, hostContext = "host-1", fixedGroup = "fn")
        state.label = "左"
        val built = state.build(initial, hostContext = "host-1")
        assertEquals("编辑必须保原组，否则键排掉键", "fn", built.btnGroup)
        assertEquals("ak-fn-left", built.id)
        assertEquals(2, built.sortOrder)
        assertEquals("左", built.label)
        assertNull("原为全局按钮 → 保持全局", built.hostId)
    }

    @Test
    fun newButtonDefaultsToMainGroup() {
        val state = ButtonEditState(initial = null, hostContext = null, fixedGroup = "main")
        state.label = "部署"
        state.command = "make deploy"
        val built = state.build(initial = null, hostContext = null)
        assertEquals("新建按钮进主栏组", "main", built.btnGroup)
        assertEquals("command", built.type)
        assertEquals(0, built.sortOrder)
    }

    /** 新建按钮的组 = 传入的 fixedGroup（组由"进入的组"上下文固定，不再在单按钮里选组）。 */
    @Test
    fun newButtonUsesFixedGroup() {
        val state = ButtonEditState(initial = null, hostContext = null, fixedGroup = "vim")
        state.label = "x"
        state.command = "y"
        assertEquals("vim", state.build(initial = null, hostContext = null).btnGroup)
    }

    /** switch 类型 build——command=目标组、长按强制 none（switch 长按=开设置，非可配项）。 */
    @Test
    fun switchBuildUsesTargetGroupAndForcesNoLongPress() {
        val state =
            ButtonEditState(initial = null, hostContext = null, fixedGroup = "main").apply {
                label = "⇌"
                type = "switch"
                switchTarget = "alt"
                longPressMode = "repeat" // 即便误设，switch 也强制 none
            }
        val built = state.build(initial = null, hostContext = null)
        assertEquals("switch", built.type)
        assertEquals("alt", built.command)
        assertEquals("none", built.longPressMode)
    }

    /** 长按 build——discrete 存载荷 + 阈值/速率 clamp 下限；非 discrete 清空载荷。 */
    @Test
    fun longPressBuildKeepsDiscretePayloadAndClamps() {
        val ds = ButtonEditState(initial = null, hostContext = null, fixedGroup = "main")
        ds.label = "Esc"
        ds.type = "escape"
        ds.command = "\\e"
        ds.longPressMode = "discrete"
        ds.longPressCommand = "\\e\\e"
        ds.longPressThresholdMs = "50"
        ds.longPressRepeatMs = "5"
        val d = ds.build(initial = null, hostContext = null)
        assertEquals("discrete", d.longPressMode)
        assertEquals("\\e\\e", d.longPressCommand)
        assertEquals("阈值 clamp 下限 100", 100, d.longPressThresholdMs)
        assertEquals("速率 clamp 下限 20", 20, d.longPressRepeatMs)

        val rs = ButtonEditState(initial = null, hostContext = null, fixedGroup = "main")
        rs.label = "←"
        rs.type = "escape"
        rs.command = "\\e[D"
        rs.longPressMode = "repeat"
        rs.longPressCommand = "残留"
        val r = rs.build(initial = null, hostContext = null)
        assertEquals("非 discrete 清空载荷", "", r.longPressCommand)
    }

    /** discrete 长按空载荷 → 无效（防 command 型长按 dispatch(command="") 误发裸回车）。 */
    @Test
    fun discreteLongPressRequiresPayload() {
        val state =
            ButtonEditState(initial = null, hostContext = null, fixedGroup = "main").apply {
                label = "X"
                type = "command"
                command = "ls"
                longPressMode = "discrete"
                longPressCommand = ""
            }
        assertFalse("discrete 空载荷不可保存", state.valid)
        state.longPressCommand = "\\e\\e"
        assertTrue("有载荷即可保存", state.valid)
        state.longPressMode = "none"
        state.longPressCommand = ""
        assertTrue("none 模式无需长按载荷", state.valid)
    }
}
