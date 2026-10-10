package com.ccmonitor.mobile.core.data

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ccmonitor.mobile.core.data.db.AppDatabase
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.HostDao
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class HostDaoTest {
    private lateinit var db: AppDatabase
    private lateinit var dao: HostDao

    @Before
    fun setup() {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        db =
            Room
                .inMemoryDatabaseBuilder(ctx, AppDatabase::class.java)
                .allowMainThreadQueries()
                .build()
        dao = db.hostDao()
    }

    @After
    fun teardown() = db.close()

    @Test
    fun insertObserveDelete() =
        runTest {
            assertEquals(0, dao.observeAll().first().size)

            val host = Host(id = "h1", label = "NanoPi", host = "192.0.2.2", port = 22, username = "pi")
            dao.upsert(host)

            val all = dao.observeAll().first()
            assertEquals(1, all.size)
            assertEquals("NanoPi", all[0].label)
            assertEquals("192.0.2.2", all[0].host)

            // upsert 更新而非重复插入
            dao.upsert(host.copy(label = "NanoPi-LAN"))
            assertEquals(1, dao.observeAll().first().size)
            assertEquals("NanoPi-LAN", dao.get("h1")?.label)

            dao.deleteById("h1")
            assertEquals(0, dao.observeAll().first().size)
            assertNull(dao.get("h1"))
        }
}
