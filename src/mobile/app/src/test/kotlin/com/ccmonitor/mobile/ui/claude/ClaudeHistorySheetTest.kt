package com.ccmonitor.mobile.ui.claude

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 历史面板的纯函数：相对时间标签与分组折叠组键（渲染留给设备端测试）。 */
class ClaudeHistorySheetTest {
    private val now = 1_800_000_000L

    // ---------- 分组折叠组键 ----------

    @Test fun groupKeyPrefersRealCwd() =
        assertEquals("cwd:/home/u/proj", historyGroupKey("/home/u/proj", "/h/p/-home-u-proj/s1.jsonl"))

    @Test fun groupKeyFallsBackToEncodedDirOfFirstPath() =
        assertEquals("enc:-home-u-proj", historyGroupKey(null, "/h/p/-home-u-proj/s1.jsonl"))

    @Test fun groupKeyPrefixIsolationPreventsCraftedCollision() {
        // cwd 是 JSONL 可写文本——构造 cwd="enc:-x" 不得与编码兜底组 "enc:-x" 撞键（组键兼作 LazyColumn item key，撞键即崩）。
        assertEquals("cwd:enc:-x", historyGroupKey("enc:-x", null))
        assertEquals("enc:-x", historyGroupKey(null, "/h/p/-x/s.jsonl"))
    }

    @Test fun groupKeyDegenerateInputsStillStable() {
        assertEquals("空组（无会话）也有确定键", "enc:", historyGroupKey(null, null))
        assertEquals("无父段路径 → 空编码名", "enc:", historyGroupKey(null, "s.jsonl"))
    }

    // ---------- 相对时间 ----------

    @Test fun nullEpochHidesLabel() = assertNull(relativeTimeLabel(null, now))

    @Test fun justNowWithinAMinute() {
        assertEquals("刚刚", relativeTimeLabel(now - 5, now))
        assertEquals("远端时钟快于本机（未来 mtime）容忍为刚刚", "刚刚", relativeTimeLabel(now + 120, now))
    }

    @Test fun minutesHoursDays() {
        assertEquals("3 分钟前", relativeTimeLabel(now - 3 * 60, now))
        assertEquals("59 分钟前", relativeTimeLabel(now - 59 * 60, now))
        assertEquals("2 小时前", relativeTimeLabel(now - 2 * 3_600, now))
        assertEquals("1 天前", relativeTimeLabel(now - 86_400, now))
        assertEquals("29 天前", relativeTimeLabel(now - 29L * 86_400, now))
    }

    @Test fun oldSessionsShowAbsoluteDate() {
        val label = relativeTimeLabel(now - 40L * 86_400, now)
        assertEquals("≥30 天 → yyyy-MM-dd", true, label != null && Regex("^\\d{4}-\\d{2}-\\d{2}$").matches(label))
    }
}
