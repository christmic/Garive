package com.garive.eng.kt.tools

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Exact T2 Browser tool revision. */
public const val T2_BROWSER_TOOL_REVISION: String = "1"
/** Pure T2 Browser binding implementation revision. */
public const val T2_BROWSER_RESOLVER_REVISION: String = "garive.t2.browser.access.v1"
/** Observe one admitted browser page. */
public const val T2_BROWSER_OBSERVE: String = "garive.browser.observe"
/** Navigate one admitted browser page. */
public const val T2_BROWSER_NAVIGATE: String = "garive.browser.navigate"
/** Perform one bounded semantic browser action. */
public const val T2_BROWSER_ACT: String = "garive.browser.act"

private const val BROWSER_MAX_RESULT_BYTES: Long = 2_097_152

/** Validated exact browser session/page identity admitted by a catalogue. */
public class BrowserPageScope private constructor(
    private val sessionId: String,
    private val pageId: String,
) {
    internal val resourceKey: String = "browser:$sessionId:$pageId"

    public companion object {
        /** Constructs one scope from portable non-empty ASCII identifiers. */
        public fun create(sessionId: String, pageId: String): ToolContractResult<BrowserPageScope> =
            if (browserToken(sessionId) && browserToken(pageId)) {
                ToolContractResult.Success(BrowserPageScope(sessionId, pageId))
            } else {
                browserFailure()
            }
    }
}

/** Frozen three-tool Browser catalogue for one exact capability snapshot. */
public class BuiltinT2BrowserCatalogue private constructor(
    definitions: List<ToolDefinition>,
    private val catalogue: ToolCatalog,
) {
    /** Exact definitions frozen into the Agent snapshot. */
    public val definitions: List<ToolDefinition> = definitions.toList()

    /** Prepares one Browser intent through exact page and origin bindings. */
    public fun prepare(intent: ToolIntent): ToolContractResult<PreparedToolCall> =
        catalogue.prepareV3(intent, BrowserResolver(intent.toolName))

    public companion object {
        /** Freezes exact admitted pages, canonical origins and policy revision. */
        public fun create(
            policyRevision: String,
            pages: List<BrowserPageScope>,
            origins: List<String>,
        ): ToolContractResult<BuiltinT2BrowserCatalogue> = browserContract {
            val definitions = listOf(
                browserObserveDefinition(policyRevision, pages),
                browserNavigateDefinition(policyRevision, pages, origins),
                browserActDefinition(policyRevision, pages, origins),
            ).sortedBy(ToolDefinition::name)
            BuiltinT2BrowserCatalogue(definitions, ToolCatalog.create(definitions).browserRequired())
        }
    }
}

private class BrowserResolver(private val toolName: String) : ToolAccessResolver {
    override val revision: String = T2_BROWSER_RESOLVER_REVISION

    override fun resolve(arguments: JsonElement): ToolContractResult<InvocationAccessSet> = browserContract {
        val page = BrowserPageScope.create(arguments.browserText("session_id"), arguments.browserText("page_id"))
            .browserRequired()
        val mode = if (toolName == T2_BROWSER_OBSERVE) AccessMode.READ else AccessMode.WRITE
        val accesses = mutableListOf(
            ResourceAccess.create(AccessNamespace.RUNTIME, page.resourceKey, mode).browserRequired(),
        )
        when (toolName) {
            T2_BROWSER_OBSERVE -> Unit
            T2_BROWSER_NAVIGATE -> {
                val origin = arguments.browserText("destination_origin")
                if (browserUrlOrigin(arguments.browserText("destination_url")) != origin) browserAbort()
                accesses += ResourceAccess.create(AccessNamespace.NETWORK, origin, AccessMode.WRITE).browserRequired()
            }
            T2_BROWSER_ACT -> {
                validateBrowserAction(arguments)
                arguments.browserStrings("allowed_navigation_origins").forEach { origin ->
                    accesses += ResourceAccess.create(AccessNamespace.NETWORK, origin, AccessMode.WRITE)
                        .browserRequired()
                }
            }
            else -> browserAbort()
        }
        InvocationAccessSet.create(accesses).browserRequired()
    }
}

private fun browserObserveDefinition(policy: String, pages: List<BrowserPageScope>): ToolDefinition =
    browserDefinition(
        T2_BROWSER_OBSERVE,
        "Observe one bounded browser semantic tree.",
        browserSchema("""{"type":"object","properties":{"session_id":{"type":"string","minLength":1,"maxLength":128},"page_id":{"type":"string","minLength":1,"maxLength":128},"expected_previous_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"max_nodes":{"type":"integer","minimum":1,"maximum":10000},"max_text_bytes":{"type":"integer","minimum":1,"maximum":1048576}},"required":["session_id","page_id","max_nodes","max_text_bytes"],"additionalProperties":false}"""),
        listOf(ExecutionCapability.BROWSER_OBSERVE),
        ReplayClass.READ_ONLY,
        pages,
        emptyList(),
        AccessMode.READ,
        policy,
    )

private fun browserNavigateDefinition(
    policy: String,
    pages: List<BrowserPageScope>,
    origins: List<String>,
): ToolDefinition = browserDefinition(
    T2_BROWSER_NAVIGATE,
    "Navigate one browser page to an exact admitted origin.",
    browserSchema("""{"type":"object","properties":{"session_id":{"type":"string","minLength":1,"maxLength":128},"page_id":{"type":"string","minLength":1,"maxLength":128},"expected_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"destination_url":{"type":"string","minLength":1,"maxLength":8192},"destination_origin":{"type":"string","minLength":10,"maxLength":512},"wait_until":{"type":"string","enum":["dom_content_loaded","load","network_idle"]},"timeout_ms":{"type":"integer","minimum":1,"maximum":120000},"max_nodes":{"type":"integer","minimum":1,"maximum":10000},"max_text_bytes":{"type":"integer","minimum":1,"maximum":1048576}},"required":["session_id","page_id","expected_snapshot_id","destination_url","destination_origin","wait_until","timeout_ms","max_nodes","max_text_bytes"],"additionalProperties":false}"""),
    listOf(ExecutionCapability.BROWSER_ACT, ExecutionCapability.NETWORK),
    ReplayClass.NEVER_REPLAY,
    pages,
    origins,
    AccessMode.WRITE,
    policy,
)

private fun browserActDefinition(
    policy: String,
    pages: List<BrowserPageScope>,
    origins: List<String>,
): ToolDefinition = browserDefinition(
    T2_BROWSER_ACT,
    "Perform one snapshot-bound semantic browser action.",
    browserSchema("""{"type":"object","properties":{"session_id":{"type":"string","minLength":1,"maxLength":128},"page_id":{"type":"string","minLength":1,"maxLength":128},"expected_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"action":{"type":"string","enum":["click","type_text","clear","select_option","press_key","scroll","go_back","go_forward","reload"]},"node_ref":{"type":"string","minLength":1,"maxLength":128},"text":{"type":"string","maxLength":32768},"option":{"type":"string","minLength":1,"maxLength":4096},"key":{"type":"string","enum":["enter","tab","escape","backspace","delete","arrow_up","arrow_down","arrow_left","arrow_right","home","end","page_up","page_down","space"]},"delta_x":{"type":"integer","minimum":-100000,"maximum":100000},"delta_y":{"type":"integer","minimum":-100000,"maximum":100000},"allowed_navigation_origins":{"type":"array","maxItems":16,"items":{"type":"string","minLength":10,"maxLength":512}}},"required":["session_id","page_id","expected_snapshot_id","action","allowed_navigation_origins"],"additionalProperties":false}"""),
    listOf(ExecutionCapability.BROWSER_ACT, ExecutionCapability.NETWORK),
    ReplayClass.NEVER_REPLAY,
    pages,
    origins,
    AccessMode.WRITE,
    policy,
)

@Suppress("LongParameterList")
private fun browserDefinition(
    name: String,
    description: String,
    inputSchema: JsonElement,
    capabilities: List<ExecutionCapability>,
    replay: ReplayClass,
    pages: List<BrowserPageScope>,
    origins: List<String>,
    pageMode: AccessMode,
    policy: String,
): ToolDefinition {
    val observe = name == T2_BROWSER_OBSERVE
    val requirements = ExecutionRequirements.create(
        capabilities,
        if (observe) 30_000 else 120_000,
        BROWSER_MAX_RESULT_BYTES,
    ).browserRequired()
    val controls = mutableListOf(
        SandboxControl.BROWSER_SESSION_SCOPE,
        SandboxControl.SNAPSHOT_BINDING,
        SandboxControl.RESOURCE_LIMITS,
    )
    if (ExecutionCapability.NETWORK in capabilities) {
        controls += listOf(SandboxControl.NETWORK_ORIGIN_SCOPE, SandboxControl.REDIRECT_REVALIDATION)
    }
    return ToolDefinition.createV3(
        name,
        T2_BROWSER_TOOL_REVISION,
        description,
        inputSchema,
        requirements,
        replay,
        ToolAccessPolicyV1.create(
            policy,
            emptyList(),
            emptyList(),
            origins.map { AccessPolicyEntry.create(it, listOf(AccessMode.WRITE)).browserRequired() },
            pages.map { AccessPolicyEntry.create(it.resourceKey, listOf(pageMode)).browserRequired() },
            if (observe) 1 else 17,
            BROWSER_MAX_RESULT_BYTES,
        ).browserRequired(),
        T2_BROWSER_RESOLVER_REVISION,
        SandboxRequirementsV1.create(capabilities, controls, null, 64).browserRequired(),
    ).browserRequired()
}

private fun validateBrowserAction(arguments: JsonElement) {
    val value = arguments.jsonObject
    fun present(name: String): Boolean = name in value
    val valid = when (arguments.browserText("action")) {
        "click", "clear" -> present("node_ref") && absent(value, "text", "option", "key", "delta_x", "delta_y")
        "type_text" -> present("node_ref") && present("text") && absent(value, "option", "key", "delta_x", "delta_y")
        "select_option" -> present("node_ref") && present("option") && absent(value, "text", "key", "delta_x", "delta_y")
        "press_key" -> present("key") && absent(value, "node_ref", "text", "option", "delta_x", "delta_y")
        "scroll" -> present("delta_x") && present("delta_y") &&
            (value.getValue("delta_x").jsonPrimitive.int != 0 || value.getValue("delta_y").jsonPrimitive.int != 0) &&
            absent(value, "node_ref", "text", "option", "key")
        "go_back", "go_forward", "reload" -> absent(value, "node_ref", "text", "option", "key", "delta_x", "delta_y")
        else -> false
    }
    if (!valid) browserAbort()
}

private fun absent(value: JsonObject, vararg names: String): Boolean = names.none(value::containsKey)
private fun browserUrlOrigin(url: String): String? {
    val split = url.indexOf("://")
    if (split < 0 || url.substring(0, split) !in setOf("http", "https")) return null
    val end = url.indexOfAny(charArrayOf('/', '?', '#'), split + 3).let { if (it < 0) url.length else it }
    return url.substring(0, end)
}
private fun browserToken(value: String): Boolean = value.isNotEmpty() && value.length <= 128 &&
    value.all { (it.isLetterOrDigit() && it.code < 128) || it in "-_." }
private fun JsonElement.browserText(name: String): String =
    (this as? JsonObject)?.get(name)?.jsonPrimitive?.content ?: browserAbort()
private fun JsonElement.browserStrings(name: String): List<String> =
    (this as? JsonObject)?.get(name)?.let { it as? JsonArray }?.map { it.jsonPrimitive.content } ?: browserAbort()
private fun browserSchema(value: String): JsonElement = Json.parseToJsonElement(value)
private class BrowserContractFailure(val error: PreparationError) : RuntimeException()
private fun browserAbort(): Nothing = throw BrowserContractFailure(PreparationError(PreparationErrorCode.EFFECT_ACCESS_INVALID))
private fun browserFailure(): ToolContractResult.Failure =
    ToolContractResult.Failure(PreparationError(PreparationErrorCode.EFFECT_ACCESS_INVALID))
private fun <T> ToolContractResult<T>.browserRequired(): T = when (this) {
    is ToolContractResult.Success -> value
    is ToolContractResult.Failure -> throw BrowserContractFailure(error)
}
private inline fun <T> browserContract(block: () -> T): ToolContractResult<T> = try {
    ToolContractResult.Success(block())
} catch (failure: BrowserContractFailure) {
    ToolContractResult.Failure(failure.error)
}
