package com.garive.eng.kt.tools

/** Stable failure returned while parsing or applying one T1 patch. */
public enum class T1PatchError {
    /** Patch is outside the admitted revision-1 grammar. */
    INVALID_SYNTAX,
    /** Requested target is absent from the patch. */
    TARGET_MISSING,
    /** A hunk anchor is absent, repeated, or out of order. */
    CONTEXT_MISMATCH,
}

/** Portable result of applying one target without I/O. */
public sealed interface T1PatchResult {
    /** Exact transformed UTF-8 content. */
    public data class Success(public val value: String) : T1PatchResult
    /** Stable parse or application failure. */
    public data class Failure(public val error: T1PatchError) : T1PatchResult
}

private enum class LineKind { CONTEXT, ADD, REMOVE }
private data class PatchLine(val kind: LineKind, val text: String, var noNewline: Boolean = false)
private data class Hunk(val lines: MutableList<PatchLine> = mutableListOf())
private data class Target(val path: String, val hunks: MutableList<Hunk> = mutableListOf())

/** Applies exact target hunks to bounded UTF-8 content without I/O. */
public fun applyT1Patch(patch: String, path: String, current: String): T1PatchResult {
    val targets = parseT1Patch(patch) ?: return T1PatchResult.Failure(T1PatchError.INVALID_SYNTAX)
    val target = targets.singleOrNull { it.path == path }
        ?: return T1PatchResult.Failure(T1PatchError.TARGET_MISSING)
    val terminalNewline = current.endsWith('\n')
    val lines = if (current.isEmpty()) mutableListOf() else current.removeSuffix("\n").split('\n').toMutableList()
    var cursor = 0
    var resultTerminalNewline = terminalNewline
    target.hunks.forEachIndexed { hunkIndex, hunk ->
        val before = hunk.lines.filter { it.kind != LineKind.ADD }.map(PatchLine::text)
        if (before.isEmpty()) return T1PatchResult.Failure(T1PatchError.INVALID_SYNTAX)
        val positions = (cursor..(lines.size - before.size).coerceAtLeast(0)).filter { start ->
            start + before.size <= lines.size && lines.subList(start, start + before.size) == before
        }.take(2)
        if (positions.size != 1) return T1PatchResult.Failure(T1PatchError.CONTEXT_MISMATCH)
        val start = positions.single()
        if (hunk.lines.any(PatchLine::noNewline)) {
            val validMarker = hunkIndex + 1 == target.hunks.size &&
                hunk.lines.last().noNewline && start + before.size == lines.size
            if (!validMarker) return T1PatchResult.Failure(T1PatchError.INVALID_SYNTAX)
            if (hunk.lines.any { it.noNewline && it.kind != LineKind.ADD } && terminalNewline) {
                return T1PatchResult.Failure(T1PatchError.CONTEXT_MISMATCH)
            }
            resultTerminalNewline = false
        }
        val after = hunk.lines.filter { it.kind != LineKind.REMOVE }.map(PatchLine::text)
        repeat(before.size) { lines.removeAt(start) }
        lines.addAll(start, after)
        cursor = start + after.size
    }
    return T1PatchResult.Success(lines.joinToString("\n") + if (resultTerminalNewline) "\n" else "")
}

internal fun t1PatchTargets(patch: String): Set<String>? =
    parseT1Patch(patch)?.map(Target::path)?.toSortedSet()

private fun parseT1Patch(patch: String): List<Target>? {
    val prefix = "*** Begin Patch\n"
    val suffix = "\n*** End Patch"
    val normalized = patch.removeSuffix("\n")
    if (!normalized.startsWith(prefix) || !normalized.endsWith(suffix)) return null
    val targets = mutableListOf<Target>()
    var currentHunk: Hunk? = null
    fun finishHunk(): Boolean {
        val hunk = currentHunk ?: return true
        currentHunk = null
        val valid = hunk.lines.isNotEmpty() && hunk.lines.any { it.kind != LineKind.ADD } &&
            hunk.lines.any { it.kind != LineKind.CONTEXT }
        if (!valid) return false
        targets.lastOrNull()?.hunks?.add(hunk) ?: return false
        return true
    }
    for (line in normalized.removePrefix(prefix).removeSuffix(suffix).lines()) {
        when {
            line.startsWith("*** Update File: ") -> {
                if (!finishHunk()) return null
                val path = line.removePrefix("*** Update File: ")
                if (path.isEmpty() || path == "." || targets.any { it.path == path }) return null
                targets += Target(path)
            }
            line == "@@" -> {
                if (!finishHunk() || targets.isEmpty()) return null
                currentHunk = Hunk()
            }
            line == "\\ No newline at end of file" -> {
                val previous = currentHunk?.lines?.lastOrNull() ?: return null
                if (previous.noNewline) return null
                previous.noNewline = true
            }
            else -> {
                val kind = when (line.firstOrNull()) {
                    ' ' -> LineKind.CONTEXT
                    '+' -> LineKind.ADD
                    '-' -> LineKind.REMOVE
                    else -> return null
                }
                currentHunk?.lines?.add(PatchLine(kind, line.drop(1))) ?: return null
            }
        }
    }
    if (!finishHunk() || targets.isEmpty() || targets.any { it.hunks.isEmpty() }) return null
    return targets
}
