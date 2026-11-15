package com.ccmonitor.mobile.core.rs

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** 冒烟：装进包里的 `libmobcore.so` 加载得起来、口接得上（一个不碰网络、不要初始化的纯函数口答得出）。 */
@RunWith(AndroidJUnit4::class)
class CoreLoadTest {
    @Test
    fun the_core_loads_and_answers() {
        assertEquals("[]", Core.parseSshConfig(""))
    }
}
