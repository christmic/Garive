// Kotlin production server. Modules are admitted by executable slices.
rootProject.name = "garive-server"

include(":agent-core")
include(":llm-contract")
include(":ledger-contract")
include(":persistence-postgres")
include(":proto")
