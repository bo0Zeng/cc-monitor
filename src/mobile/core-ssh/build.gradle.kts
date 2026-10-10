plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "com.ccmonitor.mobile.core.ssh"
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置。
    defaultConfig {
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    packaging {
        resources {
            // sshj / bouncycastle 带的重复 META-INF 资源
            excludes +=
                setOf(
                    "META-INF/DEPENDENCIES",
                    "META-INF/LICENSE*",
                    "META-INF/NOTICE*",
                    "META-INF/versions/**",
                    "META-INF/INDEX.LIST",
                    "META-INF/*.kotlin_module",
                )
        }
    }
}

dependencies {
    implementation(project(":core-data"))
    // 用 api：RemoteCommandChannel 出现在公开 API（`SshConnectionManager.commandChannel()` 的返回类型）里，
    // 依赖 :core-ssh 的模块不另外声明 :core-remote 也要编得过。
    api(project(":core-remote"))

    implementation(libs.sshj)
    implementation(libs.bouncycastle.prov)
    implementation(libs.bouncycastle.pkix)

    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.koin.android)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test) // runTest 控并发与延时

    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.kotlinx.coroutines.test)
}
