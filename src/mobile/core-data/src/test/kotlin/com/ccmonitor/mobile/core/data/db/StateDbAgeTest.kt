package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Test

/** 盘上那一份状态库的版本 ⇒ 开不开：低于 [OLDEST_STATE_DB] 的照「太老」拒开、不迁；新装（没有那份文件）与 23 起照常开。 */
class StateDbAgeTest {
    @Test
    fun aVersion22DatabaseIsRefusedAsTooOld() {
        assertEquals(StateDbAge.TooOld(22), stateDbAge(22))
        assertEquals(StateDbAge.TooOld(1), stateDbAge(1))
    }

    @Test
    fun theOldestKeptVersionAndNewerOpen() {
        assertEquals(23, OLDEST_STATE_DB)
        for (v in listOf(OLDEST_STATE_DB, 27, 28)) assertEquals("第 $v 版", StateDbAge.Openable, stateDbAge(v))
    }

    @Test
    fun noFileYetOpensAsAFreshInstall() {
        assertEquals(StateDbAge.Openable, stateDbAge(0))
    }
}
