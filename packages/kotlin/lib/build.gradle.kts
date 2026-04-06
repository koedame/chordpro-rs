plugins {
    id("org.jetbrains.kotlin.jvm") version "1.9.25"
    id("java-library")
    id("maven-publish")
    id("signing")
}

group = "com.koedame"
version = "0.1.0"

repositories {
    mavenCentral()
}

dependencies {
    implementation("net.java.dev.jna:jna:5.17.0")
    testImplementation("org.jetbrains.kotlin:kotlin-test")
    testImplementation("org.jetbrains.kotlin:kotlin-test-junit5")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

java {
    withSourcesJar()
    withJavadocJar()
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
}

tasks.test {
    useJUnitPlatform()
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            artifactId = "chordsketch"
            from(components["java"])

            pom {
                name.set("ChordSketch")
                description.set("ChordPro file format parser and renderer")
                url.set("https://github.com/koedame/chordsketch")

                licenses {
                    license {
                        name.set("MIT")
                        url.set("https://github.com/koedame/chordsketch/blob/main/LICENSE")
                    }
                }

                developers {
                    developer {
                        id.set("koedame")
                        name.set("koedame")
                    }
                }

                scm {
                    connection.set("scm:git:git://github.com/koedame/chordsketch.git")
                    developerConnection.set("scm:git:ssh://github.com/koedame/chordsketch.git")
                    url.set("https://github.com/koedame/chordsketch")
                }
            }
        }
    }

    repositories {
        maven {
            name = "OSSRH"
            val releasesRepoUrl = uri("https://s01.oss.sonatype.org/service/local/staging/deploy/maven2/")
            val snapshotsRepoUrl = uri("https://s01.oss.sonatype.org/content/repositories/snapshots/")
            url = if (version.toString().endsWith("SNAPSHOT")) snapshotsRepoUrl else releasesRepoUrl
            credentials {
                username = System.getenv("MAVEN_USERNAME") ?: ""
                password = System.getenv("MAVEN_PASSWORD") ?: ""
            }
        }
    }
}

signing {
    val signingKey = System.getenv("GPG_SIGNING_KEY") ?: ""
    val signingPassword = System.getenv("GPG_SIGNING_PASSWORD") ?: ""
    if (signingKey.isNotEmpty()) {
        useInMemoryPgpKeys(signingKey, signingPassword)
        sign(publishing.publications["maven"])
    }
}
