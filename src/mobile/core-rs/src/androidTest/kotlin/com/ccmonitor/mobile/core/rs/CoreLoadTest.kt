package com.ccmonitor.mobile.core.rs

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** 冒烟：装进包里的 `libmobcore.so` 加载得起来、口接得上（拨号核心认得 PTY 那一种用法）。 */
@RunWith(AndroidJUnit4::class)
class CoreLoadTest {
    @Test
    fun the_core_loads_and_answers() {
        val v = Core.version()
        assertTrue(v, v.contains("\"shell\""))
        assertTrue(v, v.contains("\"ack_v\":2"))
    }
}
