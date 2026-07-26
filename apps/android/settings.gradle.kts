pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "FeatherKey"

include(":app")
include(":ime-service")
include(":keyboard-view")
include(":settings-ui")
include(":onboarding")
include(":accessibility-adapter")
include(":platform-services")
include(":ffi-bridge")
