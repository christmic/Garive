# Experimental Kotlin verification host

Executable JVM composition root. It depends inward on the portable Agent and
ledger contracts and outward on the PostgreSQL/provider adapters. The current
pre-network slice emits the generated Host API v1 fake scenario; production
HTTP routing, credentials and deployment configuration remain Runtime work.

Run the verified shell with:

```text
java -classpath ../gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain :server-host:run
```
