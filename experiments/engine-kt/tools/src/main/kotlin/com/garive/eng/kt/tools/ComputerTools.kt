package com.garive.eng.kt.tools

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Exact T2 Computer Use tool revision. */
public const val T2_COMPUTER_TOOL_REVISION: String = "1"
/** Pure T2 Computer Use binding revision. */
public const val T2_COMPUTER_RESOLVER_REVISION: String = "garive.t2.computer.access.v1"
/** Observe one admitted native application window. */
public const val T2_COMPUTER_OBSERVE: String = "garive.computer.observe"
/** Perform one snapshot-bound native action. */
public const val T2_COMPUTER_ACT: String = "garive.computer.act"

private const val COMPUTER_MAX_RESULT_BYTES: Long = 2_097_152

/** Runtime-owned opaque identity for one admitted native application window. */
public class ComputerTargetScope private constructor(
    desktopSessionId: String,
    applicationId: String,
    windowId: String,
) {
    internal val resourceKey: String = "computer:$desktopSessionId:$applicationId:$windowId"

    public companion object {
        /** Constructs one exact target from portable opaque Runtime identifiers. */
        public fun create(
            desktopSessionId: String,
            applicationId: String,
            windowId: String,
        ): ToolContractResult<ComputerTargetScope> =
            if (listOf(desktopSessionId, applicationId, windowId).all(::computerToken)) {
                ToolContractResult.Success(ComputerTargetScope(desktopSessionId, applicationId, windowId))
            } else {
                computerFailure()
            }
    }
}

/** Frozen Computer observe/act catalogue for one capability snapshot. */
public class BuiltinT2ComputerCatalogue private constructor(
    definitions: List<ToolDefinition>,
    private val catalogue: ToolCatalog,
) {
    /** Exact definitions frozen into the Agent snapshot. */
    public val definitions: List<ToolDefinition> = definitions.toList()

    /** Prepares one Computer intent through exact target/action bindings. */
    public fun prepare(intent: ToolIntent): ToolContractResult<PreparedToolCall> =
        catalogue.prepareV3(intent, ComputerResolver(intent.toolName))

    public companion object {
        /** Freezes an explicit policy revision and exact target identities. */
        public fun create(
            policyRevision: String,
            targets: List<ComputerTargetScope>,
        ): ToolContractResult<BuiltinT2ComputerCatalogue> = computerContract {
            val definitions = listOf(
                computerObserveDefinition(policyRevision, targets),
                computerActDefinition(policyRevision, targets),
            ).sortedBy(ToolDefinition::name)
            BuiltinT2ComputerCatalogue(definitions, ToolCatalog.create(definitions).computerRequired())
        }
    }
}

private class ComputerResolver(private val toolName: String) : ToolAccessResolver {
    override val revision: String = T2_COMPUTER_RESOLVER_REVISION

    override fun resolve(arguments: JsonElement): ToolContractResult<InvocationAccessSet> = computerContract {
        val target = ComputerTargetScope.create(
            arguments.computerText("desktop_session_id"),
            arguments.computerText("application_id"),
            arguments.computerText("window_id"),
        ).computerRequired()
        val mode = when (toolName) {
            T2_COMPUTER_OBSERVE -> AccessMode.READ
            T2_COMPUTER_ACT -> AccessMode.WRITE.also { validateComputerAction(arguments) }
            else -> computerAbort()
        }
        InvocationAccessSet.create(
            listOf(ResourceAccess.create(AccessNamespace.RUNTIME, target.resourceKey, mode).computerRequired()),
        ).computerRequired()
    }
}

private fun computerObserveDefinition(policy: String, targets: List<ComputerTargetScope>): ToolDefinition =
    computerDefinition(
        T2_COMPUTER_OBSERVE,
        "Observe one bounded native accessibility tree and optional window capture.",
        computerSchema("""{"type":"object","properties":{"desktop_session_id":{"type":"string","minLength":1,"maxLength":128},"application_id":{"type":"string","minLength":1,"maxLength":128},"window_id":{"type":"string","minLength":1,"maxLength":128},"expected_previous_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"max_nodes":{"type":"integer","minimum":1,"maximum":10000},"max_text_bytes":{"type":"integer","minimum":1,"maximum":1048576},"capture":{"type":"string","enum":["none","window"]},"max_capture_bytes":{"type":"integer","minimum":1,"maximum":8388608},"max_capture_pixels":{"type":"integer","minimum":1,"maximum":16777216}},"required":["desktop_session_id","application_id","window_id","max_nodes","max_text_bytes","capture","max_capture_bytes","max_capture_pixels"],"additionalProperties":false}"""),
        ExecutionCapability.COMPUTER_OBSERVE,
        ReplayClass.READ_ONLY,
        AccessMode.READ,
        policy,
        targets,
    )

private fun computerActDefinition(policy: String, targets: List<ComputerTargetScope>): ToolDefinition =
    computerDefinition(
        T2_COMPUTER_ACT,
        "Perform one exact snapshot-bound native semantic or coordinate action.",
        computerSchema("""{"type":"object","properties":{"desktop_session_id":{"type":"string","minLength":1,"maxLength":128},"application_id":{"type":"string","minLength":1,"maxLength":128},"window_id":{"type":"string","minLength":1,"maxLength":128},"expected_snapshot_id":{"type":"string","minLength":1,"maxLength":128},"target_revision":{"type":"string","minLength":1,"maxLength":128},"action":{"type":"string","enum":["press","set_value","type_text","press_key","scroll","move_pointer","click_point","drag"]},"node_ref":{"type":"string","minLength":1,"maxLength":128},"value":{"type":"string","maxLength":32768},"text":{"type":"string","maxLength":32768},"key":{"type":"string","enum":["enter","tab","escape","backspace","delete","arrow_up","arrow_down","arrow_left","arrow_right","home","end","page_up","page_down","space"]},"delta_x":{"type":"integer","minimum":-100000,"maximum":100000},"delta_y":{"type":"integer","minimum":-100000,"maximum":100000},"display_id":{"type":"string","minLength":1,"maxLength":128},"point_x":{"type":"integer","minimum":0,"maximum":1000000},"point_y":{"type":"integer","minimum":0,"maximum":1000000},"start_x":{"type":"integer","minimum":0,"maximum":1000000},"start_y":{"type":"integer","minimum":0,"maximum":1000000},"end_x":{"type":"integer","minimum":0,"maximum":1000000},"end_y":{"type":"integer","minimum":0,"maximum":1000000},"snapshot_pixel_width":{"type":"integer","minimum":1,"maximum":32768},"snapshot_pixel_height":{"type":"integer","minimum":1,"maximum":32768},"scale_milli":{"type":"integer","minimum":1000,"maximum":8000},"visible_frame_x":{"type":"integer","minimum":0,"maximum":1000000},"visible_frame_y":{"type":"integer","minimum":0,"maximum":1000000},"visible_frame_width":{"type":"integer","minimum":1,"maximum":32768},"visible_frame_height":{"type":"integer","minimum":1,"maximum":32768}},"required":["desktop_session_id","application_id","window_id","expected_snapshot_id","target_revision","action"],"additionalProperties":false}"""),
        ExecutionCapability.COMPUTER_ACT,
        ReplayClass.NEVER_REPLAY,
        AccessMode.WRITE,
        policy,
        targets,
    )

@Suppress("LongParameterList")
private fun computerDefinition(
    name: String,
    description: String,
    inputSchema: JsonElement,
    capability: ExecutionCapability,
    replay: ReplayClass,
    mode: AccessMode,
    policy: String,
    targets: List<ComputerTargetScope>,
): ToolDefinition {
    val requirements = ExecutionRequirements.create(listOf(capability), 30_000, COMPUTER_MAX_RESULT_BYTES)
        .computerRequired()
    val controls = mutableListOf(
        SandboxControl.NATIVE_TARGET_SCOPE,
        SandboxControl.SNAPSHOT_BINDING,
        SandboxControl.SCREEN_CAPTURE_SCOPE,
        SandboxControl.RESOURCE_LIMITS,
    )
    if (capability == ExecutionCapability.COMPUTER_ACT) controls += SandboxControl.FOCUS_REVALIDATION
    return ToolDefinition.createV3(
        name,
        T2_COMPUTER_TOOL_REVISION,
        description,
        inputSchema,
        requirements,
        replay,
        ToolAccessPolicyV1.create(
            policy,
            emptyList(),
            emptyList(),
            emptyList(),
            targets.map { AccessPolicyEntry.create(it.resourceKey, listOf(mode)).computerRequired() },
            1,
            COMPUTER_MAX_RESULT_BYTES,
        ).computerRequired(),
        T2_COMPUTER_RESOLVER_REVISION,
        SandboxRequirementsV1.create(listOf(capability), controls, null, 64).computerRequired(),
    ).computerRequired()
}

private val computerDetails: Set<String> = setOf(
    "node_ref", "value", "text", "key", "delta_x", "delta_y", "display_id", "point_x", "point_y",
    "start_x", "start_y", "end_x", "end_y", "snapshot_pixel_width", "snapshot_pixel_height",
    "scale_milli", "visible_frame_x", "visible_frame_y", "visible_frame_width", "visible_frame_height",
)
private val computerGeometry: Set<String> = setOf(
    "display_id", "snapshot_pixel_width", "snapshot_pixel_height", "scale_milli",
    "visible_frame_x", "visible_frame_y", "visible_frame_width", "visible_frame_height",
)

private fun validateComputerAction(arguments: JsonElement) {
    val value = arguments.jsonObject
    fun exact(required: Set<String>): Boolean = required.all(value::containsKey) &&
        (computerDetails - required).none(value::containsKey)
    val valid = when (arguments.computerText("action")) {
        "press" -> exact(setOf("node_ref"))
        "set_value" -> exact(setOf("node_ref", "value"))
        "type_text" -> exact(setOf("node_ref", "text"))
        "press_key" -> exact(setOf("key"))
        "scroll" -> exact(setOf("node_ref", "delta_x", "delta_y")) &&
            (value.number("delta_x") != 0 || value.number("delta_y") != 0)
        "move_pointer", "click_point" -> pointComputerAction(value)
        "drag" -> dragComputerAction(value)
        else -> false
    }
    if (!valid) computerAbort()
}

private fun pointComputerAction(value: JsonObject): Boolean {
    val required = computerGeometry + setOf("point_x", "point_y")
    return required.all(value::containsKey) && (computerDetails - required).none(value::containsKey) &&
        computerGeometryValid(value) && computerPointInside(value, value.number("point_x"), value.number("point_y"))
}

private fun dragComputerAction(value: JsonObject): Boolean {
    val points = setOf("start_x", "start_y", "end_x", "end_y")
    val required = computerGeometry + points
    if (!required.all(value::containsKey) || (computerDetails - required).any(value::containsKey)) return false
    val startX = value.number("start_x")
    val startY = value.number("start_y")
    val endX = value.number("end_x")
    val endY = value.number("end_y")
    return (startX != endX || startY != endY) && computerGeometryValid(value) &&
        computerPointInside(value, startX, startY) && computerPointInside(value, endX, endY)
}

private fun computerGeometryValid(value: JsonObject): Boolean {
    val right = value.number("visible_frame_x") + value.number("visible_frame_width")
    val bottom = value.number("visible_frame_y") + value.number("visible_frame_height")
    return computerToken(value.getValue("display_id").jsonPrimitive.content) &&
        right <= value.number("snapshot_pixel_width") && bottom <= value.number("snapshot_pixel_height")
}

private fun computerPointInside(value: JsonObject, x: Int, y: Int): Boolean {
    val left = value.number("visible_frame_x")
    val top = value.number("visible_frame_y")
    return x in left until left + value.number("visible_frame_width") &&
        y in top until top + value.number("visible_frame_height")
}

private fun JsonObject.number(name: String): Int = getValue(name).jsonPrimitive.int
private fun computerToken(value: String): Boolean = value.isNotEmpty() && value.length <= 128 &&
    value.all { (it.isLetterOrDigit() && it.code < 128) || it in "-_." }
private fun JsonElement.computerText(name: String): String =
    (this as? JsonObject)?.get(name)?.jsonPrimitive?.content ?: computerAbort()
private fun computerSchema(value: String): JsonElement = Json.parseToJsonElement(value)
private class ComputerContractFailure(val error: PreparationError) : RuntimeException()
private fun computerAbort(): Nothing = throw ComputerContractFailure(
    PreparationError(PreparationErrorCode.EFFECT_ACCESS_INVALID),
)
private fun computerFailure(): ToolContractResult.Failure =
    ToolContractResult.Failure(PreparationError(PreparationErrorCode.EFFECT_ACCESS_INVALID))
private fun <T> ToolContractResult<T>.computerRequired(): T = when (this) {
    is ToolContractResult.Success -> value
    is ToolContractResult.Failure -> throw ComputerContractFailure(error)
}
private inline fun <T> computerContract(block: () -> T): ToolContractResult<T> = try {
    ToolContractResult.Success(block())
} catch (failure: ComputerContractFailure) {
    ToolContractResult.Failure(failure.error)
}
