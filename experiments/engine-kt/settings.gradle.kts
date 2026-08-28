// Experimental Kotlin Engine. Modules are admitted only with an executable
// conformance or research slice; this build is not a product Runtime.
rootProject.name = "engine-kt"

include(":core")
include(":llm")
include(":ledger")
include(":persistence-postgres")
include(":provider-openai")
include(":provider-anthropic")
include(":proto")
include(":server-host")
