// Experimental Kotlin Engine. Modules are admitted only with an executable
// conformance or research slice; this build is not a product Runtime.
rootProject.name = "engine-kt"

include(":core")
include(":config")
include(":llm")
include(":ledger")
include(":tools")
include(":skill")
include(":memory")
include(":knowledge")
include(":scheduler")
include(":persistence-postgres")
include(":adapter-openai-responses")
include(":adapter-anthropic-messages")
include(":provider-compatible")
include(":provider-profile")
include(":provider-openai")
include(":provider-anthropic")
include(":proto")
include(":server-host")
