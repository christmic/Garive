package com.garive.eng.kt.tools

import java.security.MessageDigest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Closed canonical set of enforcement controls an executor must prove. */
public enum class SandboxControl(public val wireName: String) {
    /** Restrict filesystem operations to the exact granted workspace scope. */
    FILESYSTEM_SCOPE("filesystem_scope"),
    /** Prevent symbolic-link or equivalent traversal outside that scope. */
    SYMLINK_CONTAINMENT("symlink_containment"),
    /** Contain spawned processes and their descendants. */
    PROCESS_CONTAINMENT("process_containment"),
    /** Pass an argv vector without implicit shell parsing. */
    STRUCTURED_ARGUMENTS("structured_arguments"),
    /** Construct child environments from an explicit allowlist. */
    ENVIRONMENT_ALLOWLIST("environment_allowlist"),
    /** Restrict network operations to exact granted origins. */
    NETWORK_ORIGIN_SCOPE("network_origin_scope"),
    /** Re-authorize every redirect destination. */
    REDIRECT_REVALIDATION("redirect_revalidation"),
    /** Enforce the declared resource ceilings. */
    RESOURCE_LIMITS("resource_limits"),
}

/** Validated immutable F0 requirement profile for one Tool revision. */
public class SandboxRequirementsV1 private constructor(
    controls: List<SandboxControl>,
    public val maxProcesses: Int?,
    public val maxOpenFiles: Int,
) {
    /** Controls in canonical enum order. */
    public val controls: List<SandboxControl> = controls.toList()

    /** Returns whether an executor proves all controls with equal or tighter limits. */
    public fun isCoveredBy(executor: SandboxRequirementsV1): Boolean =
        executor.controls.containsAll(controls) &&
            executor.maxOpenFiles <= maxOpenFiles &&
            when {
                maxProcesses != null && executor.maxProcesses != null -> executor.maxProcesses <= maxProcesses
                maxProcesses == null && executor.maxProcesses == null -> true
                else -> false
            }

    /** Returns lowercase SHA-256 over the RFC 8785 canonical profile. */
    public fun digest(): ToolContractResult<String> = runCatching {
        MessageDigest.getInstance("SHA-256").digest(JsonCanonicalizer(canonicalJson().toString()).encodedUTF8)
            .joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }
    }.fold(
        onSuccess = { ToolContractResult.Success(it) },
        onFailure = { failure(PreparationErrorCode.NON_CANONICAL_VALUE) },
    )

    /** Revalidates this frozen profile against one exact Tool capability set. */
    public fun validateFor(capabilities: List<ExecutionCapability>): ToolContractResult<Unit> =
        when (create(capabilities, controls, maxProcesses, maxOpenFiles)) {
            is ToolContractResult.Success -> ToolContractResult.Success(Unit)
            is ToolContractResult.Failure -> failure(PreparationErrorCode.SANDBOX_REQUIREMENT_INVALID)
        }

    internal fun canonicalJson(): JsonObject = JsonObject(
        buildMap {
            put("contract", JsonPrimitive("garive.sandbox-requirements"))
            put("version", JsonPrimitive(1))
            put("controls", JsonArray(controls.map { JsonPrimitive(it.wireName) }))
            maxProcesses?.let { put("max_processes", JsonPrimitive(it)) }
            put("max_open_files", JsonPrimitive(maxOpenFiles))
        },
    )

    public companion object {
        /** Validates capability-specific controls, uniqueness and non-zero bounds. */
        public fun create(
            capabilities: List<ExecutionCapability>,
            controls: List<SandboxControl>,
            maxProcesses: Int?,
            maxOpenFiles: Int,
        ): ToolContractResult<SandboxRequirementsV1> {
            val capabilitySet = capabilities.toSet()
            val controlSet = controls.toSet()
            val filesystem = ExecutionCapability.FILESYSTEM_READ in capabilitySet ||
                ExecutionCapability.FILESYSTEM_WRITE in capabilitySet
            val process = ExecutionCapability.PROCESS in capabilitySet
            val network = ExecutionCapability.NETWORK in capabilitySet
            val valid = maxOpenFiles > 0 && controls.isNotEmpty() && controlSet.size == controls.size &&
                process == (maxProcesses != null) && (maxProcesses == null || maxProcesses > 0) &&
                (!filesystem || controlSet.containsAll(FILESYSTEM_CONTROLS)) &&
                (!process || controlSet.containsAll(PROCESS_CONTROLS)) &&
                (!network || controlSet.containsAll(NETWORK_CONTROLS))
            if (!valid) return failure(PreparationErrorCode.SANDBOX_REQUIREMENT_INVALID)
            return ToolContractResult.Success(
                SandboxRequirementsV1(controls.sortedBy(SandboxControl::ordinal), maxProcesses, maxOpenFiles),
            )
        }

        private val FILESYSTEM_CONTROLS: Set<SandboxControl> = setOf(
            SandboxControl.FILESYSTEM_SCOPE,
            SandboxControl.SYMLINK_CONTAINMENT,
            SandboxControl.RESOURCE_LIMITS,
        )
        private val PROCESS_CONTROLS: Set<SandboxControl> = setOf(
            SandboxControl.PROCESS_CONTAINMENT,
            SandboxControl.STRUCTURED_ARGUMENTS,
            SandboxControl.ENVIRONMENT_ALLOWLIST,
            SandboxControl.RESOURCE_LIMITS,
        )
        private val NETWORK_CONTROLS: Set<SandboxControl> = setOf(
            SandboxControl.NETWORK_ORIGIN_SCOPE,
            SandboxControl.REDIRECT_REVALIDATION,
            SandboxControl.RESOURCE_LIMITS,
        )
    }
}
