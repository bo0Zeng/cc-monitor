plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.ksp)
}

android {
    namespace = "com.ccmonitor.mobile.core.data"
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置。
    defaultConfig {
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    // 让 MigrationTestHelper 读到导出的 schema（按版本校验迁移结果）。
    sourceSets {
        getByName("androidTest").assets.srcDir("$projectDir/schemas")
    }
}

ksp {
    arg("room.schemaLocation", "$projectDir/schemas")
}

dependencies {
    implementation(libs.room.runtime)
    implementation(libs.room.ktx)
    ksp(libs.room.compiler)

    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.tink.android)
    implementation(libs.bouncycastle.prov) // 软件 Ed25519 生成与 OpenSSH 私钥编码

    implementation(libs.koin.android)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.room.testing)
    androidTestImplementation(libs.kotlinx.coroutines.test)
}

/**
 * 把三个模块的 androidTest 源集、设备测入口 `scripts/android-test.sh` 与导出的 schema 登记成单测的输入。
 *
 * 有几条单测读的是这些文件，而它们不在 `:core-data:testDebugUnitTest` 的默认输入里：
 * 只改 androidTest 或入口脚本时，Gradle 会判 UP-TO-DATE，那几条判据根本不跑却报绿。
 *
 * 注意：只登记这三样，别把整个仓挂上去（否则改任何文件都重跑全部单测）。
 * 新模块长出 androidTest 时这里要加一行，否则那个模块的改动不会触发重跑。
 */
tasks.withType<Test>().configureEach {
    listOf("app", "core-data", "core-ssh").forEach { module ->
        inputs
            .dir(rootProject.file("$module/src/androidTest"))
            .withPropertyName("instrumentedSources-$module")
            .withPathSensitivity(PathSensitivity.RELATIVE)
    }
    inputs
        .file(rootProject.file("scripts/android-test.sh"))
        .withPropertyName("deviceTestEntry")
        .withPathSensitivity(PathSensitivity.RELATIVE)
    inputs
        .dir(file("schemas"))
        .withPropertyName("exportedRoomSchemas")
        .withPathSensitivity(PathSensitivity.RELATIVE)
}
