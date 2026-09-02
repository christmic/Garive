package com.garive.eng.kt.tools

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Exact immutable revision shared by every T1 definition. */
public const val T1_TOOL_REVISION: String = "1"
/** Pure resolver revision for the exact T1 argument contract. */
public const val T1_ACCESS_RESOLVER_REVISION: String = "garive.t1.access.v1"
/** Exact read-text tool name. */
public const val T1_READ_TEXT: String = "garive.workspace.read_text"
/** Exact directory-list tool name. */
public const val T1_LIST: String = "garive.workspace.list"
/** Exact literal-search tool name. */
public const val T1_SEARCH_TEXT: String = "garive.workspace.search_text"
/** Exact create-only UTF-8 write tool name. */
public const val T1_WRITE_TEXT: String = "garive.workspace.write_text"
/** Exact journaled-patch tool name. */
public const val T1_APPLY_PATCH: String = "garive.workspace.apply_patch"
/** Exact bounded-process tool name. */
public const val T1_PROCESS_RUN: String = "garive.process.run"

private const val MAX_RESULT_BYTES: Long = 2_097_152
private const val MAX_EXPECTED_FILES: Int = 128
private const val MAX_PROCESS_DURATION_MS: Long = 300_000

/** Frozen six-tool catalogue for one effective Agent snapshot. */
public class BuiltinT1Catalogue private constructor(
    definitions: List<ToolDefinition>,
    private val catalogue: ToolCatalog,
) {
    /** Exact definitions to freeze in an Agent capability snapshot. */
    public val definitions: List<ToolDefinition> = definitions.toList()

    /** Validates and prepares one admitted T1 intent through its exact resolver. */
    public fun prepare(intent: ToolIntent): ToolContractResult<PreparedToolCall> =
        catalogue.prepareV3(intent, BuiltinT1Resolver(intent.toolName))

    public companion object {
        /** Constructs definitions from an explicit policy revision and process lane set. */
        public fun create(
            policyRevision: String,
            processLanes: List<String>,
        ): ToolContractResult<BuiltinT1Catalogue> = contract {
            val definitions = listOf(
                readDefinition(policyRevision),
                listDefinition(policyRevision),
                searchDefinition(policyRevision),
                writeDefinition(policyRevision),
                patchDefinition(policyRevision),
                processDefinition(policyRevision, processLanes),
            ).sortedBy(ToolDefinition::name)
            BuiltinT1Catalogue(definitions, ToolCatalog.create(definitions).required())
        }
    }
}

private class BuiltinT1Resolver(private val toolName: String) : ToolAccessResolver {
    override val revision: String = T1_ACCESS_RESOLVER_REVISION

    override fun resolve(arguments: JsonElement): ToolContractResult<InvocationAccessSet> = contract {
        when (toolName) {
            T1_READ_TEXT -> oneFile(arguments, AccessMode.READ, false)
            T1_WRITE_TEXT -> oneFile(arguments, AccessMode.WRITE, false)
            T1_LIST, T1_SEARCH_TEXT -> oneFile(arguments, AccessMode.READ, true)
            T1_APPLY_PATCH -> patchAccesses(arguments)
            T1_PROCESS_RUN -> processAccesses(arguments)
            else -> throw ContractFailure(accessError())
        }
    }
}

private fun oneFile(arguments: JsonElement, mode: AccessMode, rootAllowed: Boolean): InvocationAccessSet {
    val path = arguments.text("path")
    if (!rootAllowed && path == ".") throw ContractFailure(accessError())
    return InvocationAccessSet.create(
        listOf(ResourceAccess.create(AccessNamespace.FILESYSTEM, path, mode).required()),
    ).required()
}

private fun processAccesses(arguments: JsonElement): InvocationAccessSet {
    val workspaceMode = when (arguments.text("workspace_mode")) {
        "read" -> AccessMode.READ
        "write" -> AccessMode.WRITE
        else -> throw ContractFailure(accessError())
    }
    return InvocationAccessSet.create(listOf(
        ResourceAccess.create(AccessNamespace.PROCESS, arguments.text("lane"), AccessMode.EXCLUSIVE).required(),
        ResourceAccess.create(
            AccessNamespace.FILESYSTEM,
            arguments.text("working_directory"),
            workspaceMode,
        ).required(),
    )).required()
}

private fun patchAccesses(arguments: JsonElement): InvocationAccessSet {
    val targets = patchTargets(arguments.text("patch"))
    val declared = sortedSetOf<String>()
    arguments.jsonObject["expected_files"]?.jsonArray?.forEach { value ->
        val path = value.text("path")
        val digest = value.text("before_digest")
        if (path == "." || digest.length != 64 || digest.any { it !in '0'..'9' && it !in 'a'..'f' } ||
            !declared.add(path)
        ) {
            throw ContractFailure(accessError())
        }
    } ?: throw ContractFailure(accessError())
    if (targets != declared) throw ContractFailure(accessError())
    return InvocationAccessSet.create(
        targets.map { ResourceAccess.create(AccessNamespace.FILESYSTEM, it, AccessMode.WRITE).required() },
    ).required()
}

private fun patchTargets(patch: String): Set<String> {
    val targets = t1PatchTargets(patch) ?: throw ContractFailure(accessError())
    targets.forEach { ResourceAccess.create(AccessNamespace.FILESYSTEM, it, AccessMode.WRITE).required() }
    return targets
}

private fun readDefinition(policy: String): ToolDefinition = fileDefinition(
    T1_READ_TEXT,
    "Read one bounded UTF-8 workspace file.",
    schema("""{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"max_bytes":{"type":"integer","minimum":1,"maximum":1048576}},"required":["path","max_bytes"],"additionalProperties":false}"""),
    ReplayClass.READ_ONLY,
    listOf(ExecutionCapability.FILESYSTEM_READ),
    listOf(AccessMode.READ),
    policy,
    5_000,
    1,
)

private fun listDefinition(policy: String): ToolDefinition = fileDefinition(
    T1_LIST,
    "List one workspace directory without following links.",
    schema("""{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"max_entries":{"type":"integer","minimum":1,"maximum":4096},"include_hidden":{"type":"boolean"},"max_nodes":{"type":"integer","minimum":1,"maximum":10000}},"required":["path","max_entries","include_hidden","max_nodes"],"additionalProperties":false}"""),
    ReplayClass.READ_ONLY,
    listOf(ExecutionCapability.FILESYSTEM_READ),
    listOf(AccessMode.READ),
    policy,
    5_000,
    1,
)

private fun searchDefinition(policy: String): ToolDefinition = fileDefinition(
    T1_SEARCH_TEXT,
    "Search bounded workspace text for one literal query.",
    schema("""{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"query":{"type":"string","minLength":1,"maxLength":4096},"case_sensitive":{"type":"boolean"},"max_matches":{"type":"integer","minimum":1,"maximum":4096},"max_file_bytes":{"type":"integer","minimum":1,"maximum":1048576},"max_nodes":{"type":"integer","minimum":1,"maximum":10000}},"required":["path","query","case_sensitive","max_matches","max_file_bytes","max_nodes"],"additionalProperties":false}"""),
    ReplayClass.READ_ONLY,
    listOf(ExecutionCapability.FILESYSTEM_READ),
    listOf(AccessMode.READ),
    policy,
    30_000,
    1,
)

private fun writeDefinition(policy: String): ToolDefinition = fileDefinition(
    T1_WRITE_TEXT,
    "Create one bounded UTF-8 workspace file without overwriting.",
    schema("""{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"text":{"type":"string","maxLength":1048576}},"required":["path","text"],"additionalProperties":false}"""),
    ReplayClass.NEVER_REPLAY,
    listOf(ExecutionCapability.FILESYSTEM_WRITE),
    listOf(AccessMode.WRITE),
    policy,
    5_000,
    1,
)

private fun patchDefinition(policy: String): ToolDefinition = fileDefinition(
    T1_APPLY_PATCH,
    "Apply a standard unified diff or Garive patch to existing workspace files. Every target must include its digest from read_text.",
    schema("""{"type":"object","properties":{"patch":{"type":"string","description":"A standard unified diff with --- a/path, +++ b/path and @@ range @@ headers, or a Garive *** Begin Patch block.","minLength":1,"maxLength":1048576},"expected_files":{"type":"array","description":"Every patched path exactly once, bound to the SHA-256 content_digest returned by read_text.","minItems":1,"maxItems":128,"items":{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"before_digest":{"type":"string","minLength":64,"maxLength":64}},"required":["path","before_digest"],"additionalProperties":false}}},"required":["patch","expected_files"],"additionalProperties":false}"""),
    ReplayClass.RECEIPT_RECOVERABLE,
    listOf(ExecutionCapability.FILESYSTEM_READ, ExecutionCapability.FILESYSTEM_WRITE),
    listOf(AccessMode.READ, AccessMode.WRITE),
    policy,
    30_000,
    MAX_EXPECTED_FILES,
)

@Suppress("LongParameterList")
private fun fileDefinition(
    name: String,
    description: String,
    inputSchema: JsonElement,
    replay: ReplayClass,
    capabilities: List<ExecutionCapability>,
    modes: List<AccessMode>,
    policy: String,
    duration: Long,
    maxAccesses: Int,
): ToolDefinition {
    val requirements = ExecutionRequirements.create(capabilities, duration, MAX_RESULT_BYTES).required()
    return ToolDefinition.createV3(
        name,
        T1_TOOL_REVISION,
        description,
        inputSchema,
        requirements,
        replay,
        ToolAccessPolicyV1.create(
            policy,
            listOf(AccessPolicyEntry.create(".", modes).required()),
            emptyList(),
            emptyList(),
            emptyList(),
            maxAccesses,
            MAX_RESULT_BYTES,
        ).required(),
        T1_ACCESS_RESOLVER_REVISION,
        filesystemSandbox(capabilities),
    ).required()
}

private fun processDefinition(policy: String, lanes: List<String>): ToolDefinition {
    val capabilities = listOf(
        ExecutionCapability.FILESYSTEM_READ,
        ExecutionCapability.FILESYSTEM_WRITE,
        ExecutionCapability.PROCESS,
    )
    val requirements = ExecutionRequirements.create(capabilities, MAX_PROCESS_DURATION_MS, MAX_RESULT_BYTES).required()
    return ToolDefinition.createV3(
        T1_PROCESS_RUN,
        T1_TOOL_REVISION,
        "Run one configured executable lane without shell parsing.",
        schema("""{"type":"object","properties":{"lane":{"type":"string","minLength":1,"maxLength":256},"argv":{"type":"array","minItems":1,"maxItems":256,"items":{"type":"string","minLength":1,"maxLength":32768}},"working_directory":{"type":"string","minLength":1,"maxLength":4096},"workspace_mode":{"type":"string","enum":["read","write"]},"max_output_bytes":{"type":"integer","minimum":1,"maximum":1048576},"timeout_ms":{"type":"integer","minimum":1,"maximum":300000}},"required":["lane","argv","working_directory","workspace_mode","max_output_bytes","timeout_ms"],"additionalProperties":false}"""),
        requirements,
        ReplayClass.NEVER_REPLAY,
        ToolAccessPolicyV1.create(
            policy,
            listOf(AccessPolicyEntry.create(".", listOf(AccessMode.READ, AccessMode.WRITE)).required()),
            lanes.map { AccessPolicyEntry.create(it, listOf(AccessMode.EXCLUSIVE)).required() },
            emptyList(),
            emptyList(),
            2,
            MAX_RESULT_BYTES,
        ).required(),
        T1_ACCESS_RESOLVER_REVISION,
        SandboxRequirementsV1.create(
            capabilities,
            listOf(
                SandboxControl.FILESYSTEM_SCOPE,
                SandboxControl.SYMLINK_CONTAINMENT,
                SandboxControl.PROCESS_CONTAINMENT,
                SandboxControl.STRUCTURED_ARGUMENTS,
                SandboxControl.ENVIRONMENT_ALLOWLIST,
                SandboxControl.RESOURCE_LIMITS,
            ),
            16,
            64,
        ).required(),
    ).required()
}

private fun filesystemSandbox(capabilities: List<ExecutionCapability>): SandboxRequirementsV1 =
    SandboxRequirementsV1.create(
        capabilities,
        listOf(
            SandboxControl.FILESYSTEM_SCOPE,
            SandboxControl.SYMLINK_CONTAINMENT,
            SandboxControl.RESOURCE_LIMITS,
        ),
        null,
        64,
    ).required()

private fun JsonElement.text(name: String): String =
    (this as? JsonObject)?.get(name)?.jsonPrimitive?.content ?: throw ContractFailure(accessError())

private fun schema(value: String): JsonElement = Json.parseToJsonElement(value)

private class ContractFailure(val error: PreparationError) : RuntimeException()

private fun accessError(): PreparationError = PreparationError(PreparationErrorCode.EFFECT_ACCESS_INVALID)

private fun <T> ToolContractResult<T>.required(): T = when (this) {
    is ToolContractResult.Success -> value
    is ToolContractResult.Failure -> throw ContractFailure(error)
}

private inline fun <T> contract(block: () -> T): ToolContractResult<T> = try {
    ToolContractResult.Success(block())
} catch (failure: ContractFailure) {
    ToolContractResult.Failure(failure.error)
}
