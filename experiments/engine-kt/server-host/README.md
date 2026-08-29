# Experimental Kotlin PostgreSQL recovery host

JVM verification boundary for portable recovery decisions and the real
PostgreSQL Ledger adapter. It is not a product server and does not implement
HTTP routing or the Rust R1 composition.

Run its verified tests with:

```text
java -classpath ../gradle/wrapper/gradle-wrapper.jar \
  org.gradle.wrapper.GradleWrapperMain :server-host:test
```
