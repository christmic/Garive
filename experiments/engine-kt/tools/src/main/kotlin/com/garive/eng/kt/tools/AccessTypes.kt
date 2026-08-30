package com.garive.eng.kt.tools

import kotlinx.serialization.json.JsonElement

/** Pure trusted resolver from validated arguments to exact resources. */
public interface ToolAccessResolver {
    /** Immutable resolver implementation revision. */
    public val revision: String

    /** Resolves a bounded exact set without authority or I/O. */
    public fun resolve(arguments: JsonElement): ToolContractResult<InvocationAccessSet>
}

/** Closed namespace order used by access sets and conflict graphs. */
public enum class AccessNamespace(public val wireName: String) {
    /** Workspace-relative filesystem identity. */
    FILESYSTEM("filesystem"),
    /** Admitted process executor lane. */
    PROCESS("process"),
    /** Canonical HTTP origin identity. */
    NETWORK("network"),
    /** Runtime-owned logical lane. */
    RUNTIME("runtime"),
}

/** Access strength in canonical read, write, exclusive order. */
public enum class AccessMode(public val wireName: String) {
    /** Non-mutating access to one exact resource. */
    READ("read"),
    /** Mutation of one exact resource. */
    WRITE("write"),
    /** Namespace-wide exclusion for one planner step. */
    EXCLUSIVE("exclusive"),
}

/** One canonical exact invocation resource. */
public class ResourceAccess private constructor(
    public val namespace: AccessNamespace,
    public val resourceKey: String,
    public val mode: AccessMode,
) : Comparable<ResourceAccess> {
    public companion object {
        /** Validates and constructs one namespace-specific canonical key. */
        public fun create(
            namespace: AccessNamespace,
            resourceKey: String,
            mode: AccessMode,
        ): ToolContractResult<ResourceAccess> =
            if (validKey(namespace, resourceKey)) {
                ToolContractResult.Success(ResourceAccess(namespace, resourceKey, mode))
            } else {
                accessFailure()
            }
    }

    public override fun compareTo(other: ResourceAccess): Int =
        compareValuesBy(this, other, ResourceAccess::namespace, ResourceAccess::resourceKey, ResourceAccess::mode)

    public override fun equals(other: Any?): Boolean =
        other is ResourceAccess && namespace == other.namespace && resourceKey == other.resourceKey && mode == other.mode

    public override fun hashCode(): Int = 31 * (31 * namespace.hashCode() + resourceKey.hashCode()) + mode.hashCode()
}

/** Non-empty canonical ordered unique invocation accesses. */
public class InvocationAccessSet private constructor(accesses: List<ResourceAccess>) {
    /** Canonical exact resources. */
    public val values: List<ResourceAccess> = accesses.toList()

    public companion object {
        /** Sorts accesses and rejects empty or duplicate resource identities. */
        public fun create(accesses: List<ResourceAccess>): ToolContractResult<InvocationAccessSet> {
            val sorted = accesses.sorted()
            val duplicate = sorted.zipWithNext().any { (left, right) ->
                left.namespace == right.namespace && left.resourceKey == right.resourceKey
            }
            return if (sorted.isEmpty() || duplicate) accessFailure()
            else ToolContractResult.Success(InvocationAccessSet(sorted))
        }
    }

    public override fun equals(other: Any?): Boolean = other is InvocationAccessSet && values == other.values

    public override fun hashCode(): Int = values.hashCode()
}

/** One policy ceiling entry used by a namespace-specific list. */
public class AccessPolicyEntry private constructor(
    public val resource: String,
    allowedModes: List<AccessMode>,
) {
    /** Canonical non-empty unique mode set. */
    public val allowedModes: List<AccessMode> = allowedModes.toList()

    public companion object {
        /** Sorts a non-empty unique mode set for one non-empty ceiling. */
        public fun create(resource: String, modes: List<AccessMode>): ToolContractResult<AccessPolicyEntry> {
            val sorted = modes.sortedBy(AccessMode::ordinal)
            return if (resource.isEmpty() || sorted.isEmpty() || sorted.distinct().size != sorted.size) accessFailure()
            else ToolContractResult.Success(AccessPolicyEntry(resource, sorted))
        }
    }
}

/** Frozen v1 maximum access surface and result charge. */
public class ToolAccessPolicyV1 private constructor(
    public val policyRevision: String,
    filesystemRoots: List<AccessPolicyEntry>,
    processLanes: List<AccessPolicyEntry>,
    networkOrigins: List<AccessPolicyEntry>,
    runtimeLanes: List<AccessPolicyEntry>,
    public val maxAccesses: Int,
    public val maxResultBytes: Long,
) {
    /** Canonical filesystem policy roots. */
    public val filesystemRoots: List<AccessPolicyEntry> = filesystemRoots.toList()
    /** Canonical process policy lanes. */
    public val processLanes: List<AccessPolicyEntry> = processLanes.toList()
    /** Canonical network policy origins. */
    public val networkOrigins: List<AccessPolicyEntry> = networkOrigins.toList()
    /** Canonical Runtime policy lanes. */
    public val runtimeLanes: List<AccessPolicyEntry> = runtimeLanes.toList()
    private val entries: Map<AccessNamespace, List<AccessPolicyEntry>> = mapOf(
        AccessNamespace.FILESYSTEM to this.filesystemRoots,
        AccessNamespace.PROCESS to this.processLanes,
        AccessNamespace.NETWORK to this.networkOrigins,
        AccessNamespace.RUNTIME to this.runtimeLanes,
    )

    /** Returns whether every exact access is inside this ceiling. */
    public fun covers(accesses: InvocationAccessSet): Boolean =
        accesses.values.size <= maxAccesses && accesses.values.all { access ->
            entries.getValue(access.namespace).any { entry ->
                access.mode in entry.allowedModes && when (access.namespace) {
                    AccessNamespace.FILESYSTEM ->
                        entry.resource == "." || access.resourceKey == entry.resource ||
                            access.resourceKey.startsWith("${entry.resource}/")
                    else -> access.resourceKey == entry.resource
                }
            }
        }

    public companion object {
        /** Validates namespace keys, ordering, duplicates, and non-zero bounds. */
        @Suppress("LongParameterList")
        public fun create(
            policyRevision: String,
            filesystemRoots: List<AccessPolicyEntry>,
            processLanes: List<AccessPolicyEntry>,
            networkOrigins: List<AccessPolicyEntry>,
            runtimeLanes: List<AccessPolicyEntry>,
            maxAccesses: Int,
            maxResultBytes: Long,
        ): ToolContractResult<ToolAccessPolicyV1> {
            val groups = listOf(
                AccessNamespace.FILESYSTEM to filesystemRoots,
                AccessNamespace.PROCESS to processLanes,
                AccessNamespace.NETWORK to networkOrigins,
                AccessNamespace.RUNTIME to runtimeLanes,
            )
            if (policyRevision.isEmpty() || maxAccesses <= 0 || maxResultBytes <= 0) return accessFailure()
            val canonical = groups.map { (namespace, values) ->
                val sorted = values.sortedBy(AccessPolicyEntry::resource)
                if (sorted.any { !validKey(namespace, it.resource) } ||
                    sorted.zipWithNext().any { (left, right) -> left.resource == right.resource }
                ) return accessFailure()
                sorted
            }
            return ToolContractResult.Success(
                ToolAccessPolicyV1(
                    policyRevision,
                    canonical[0],
                    canonical[1],
                    canonical[2],
                    canonical[3],
                    maxAccesses,
                    maxResultBytes,
                ),
            )
        }
    }
}

private fun validKey(namespace: AccessNamespace, key: String): Boolean = when (namespace) {
    AccessNamespace.FILESYSTEM ->
        key == "." ||
            (!key.startsWith('/') && '\u0000' !in key && '\\' !in key &&
                key.split('/').all { it.isNotEmpty() && it != "." && it != ".." })
    AccessNamespace.NETWORK -> canonicalOrigin(key)
    AccessNamespace.PROCESS, AccessNamespace.RUNTIME ->
        key.isNotEmpty() && key.all { it.isLetterOrDigit() && it.code < 128 || it in "-_.:" }
}

private fun canonicalOrigin(origin: String): Boolean {
    val authority = origin.removePrefix("http://").takeIf { it != origin }
        ?: origin.removePrefix("https://").takeIf { it != origin }
        ?: return false
    if (authority.any { it in "/?#@" }) return false
    val host: String
    val port: String
    if (authority.startsWith('[')) {
        val end = authority.indexOf(']')
        if (end <= 1 || authority.getOrNull(end + 1) != ':') return false
        host = authority.substring(1, end)
        port = authority.substring(end + 2)
        if ('.' in host || !canonicalIpv6(host)) return false
    } else {
        val separator = authority.lastIndexOf(':')
        if (separator <= 0) return false
        host = authority.substring(0, separator)
        port = authority.substring(separator + 1)
        val ipv4 = host.split('.').takeIf { it.size == 4 }?.all { part ->
            part.toIntOrNull()?.let { it in 0..255 && it.toString() == part } == true
        } == true
        val dns = host.length <= 253 && host == host.lowercase() && host.split('.').all { label ->
            label.isNotEmpty() && label.length <= 63 && !label.startsWith('-') && !label.endsWith('-') &&
                label.all { it in 'a'..'z' || it.isDigit() || it == '-' }
        }
        if (!ipv4 && !dns) return false
    }
    val number = port.toIntOrNull()
    return number != null && number in 1..65535 && number.toString() == port
}

private fun canonicalIpv6(value: String): Boolean {
    if (value.any { it !in "0123456789abcdefABCDEF:" }) return false
    val halves = value.split("::")
    if (halves.size > 2) return false
    fun groups(part: String): List<Int>? = if (part.isEmpty()) {
        emptyList()
    } else {
        part.split(':').map { group ->
            if (group.isEmpty() || group.length > 4) return null
            group.toIntOrNull(16) ?: return null
        }
    }
    val left = groups(halves[0]) ?: return false
    val right = if (halves.size == 2) groups(halves[1]) ?: return false else emptyList()
    val parsed = if (halves.size == 1) {
        if (left.size != 8) return false
        left
    } else {
        if (left.size + right.size >= 8) return false
        left + List(8 - left.size - right.size) { 0 } + right
    }
    return renderIpv6(parsed) == value
}

private fun renderIpv6(groups: List<Int>): String {
    var bestStart = -1
    var bestLength = 1
    var index = 0
    while (index < groups.size) {
        if (groups[index] != 0) {
            index += 1
            continue
        }
        val start = index
        while (index < groups.size && groups[index] == 0) index += 1
        if (index - start > bestLength) {
            bestStart = start
            bestLength = index - start
        }
    }
    if (bestStart < 0) return groups.joinToString(":") { it.toString(16) }
    val left = groups.take(bestStart).joinToString(":") { it.toString(16) }
    val right = groups.drop(bestStart + bestLength).joinToString(":") { it.toString(16) }
    return "$left::$right"
}

private fun accessFailure(): ToolContractResult.Failure = failure(PreparationErrorCode.EFFECT_ACCESS_INVALID)
