package com.garive.eng.kt.memory

/** Current Runtime-owned Memory projection row supplied to the pure planner. */
public data class MemoryCurrentEntry(
    public val recordId: String,
    public val revisionId: String,
    public val authority: MemoryAuthority,
    public val memoryType: MemoryType,
    public val memoryRole: MemoryKind,
    public val scope: MemoryScopeClass,
    public val scopeOwnerId: String,
    public val lifecycle: HypothesisState,
    public val sensitivity: MemorySensitivity,
    public val contentDigest: String,
)

/** One exact scope binding admitted for new documents. */
public data class MemoryAuthorizedScope(
    public val scope: MemoryScopeClass,
    public val ownerId: String,
) : Comparable<MemoryAuthorizedScope> {
    public override fun compareTo(other: MemoryAuthorizedScope): Int =
        compareValuesBy(this, other, MemoryAuthorizedScope::scope, MemoryAuthorizedScope::ownerId)
}

/** Runtime-generated identities frozen before plan presentation. */
public sealed interface MemoryIdentityAllocation {
    /** Identities for a package-local new document. */
    public data class Add(
        public val draftToken: String,
        public val recordId: String,
        public val revisionId: String,
    ) : MemoryIdentityAllocation

    /** Fresh revision identity for an existing record edit. */
    public data class Supersede(
        public val recordId: String,
        public val revisionId: String,
    ) : MemoryIdentityAllocation
}

/** One canonical M2 import operation. */
public sealed interface MemoryImportOperation {
    /** Exact affected record identity. */
    public val recordId: String

    /** Add one newly allocated user-declared record. */
    public data class Add(
        public val sourceDraftToken: String,
        public override val recordId: String,
        public val revisionId: String,
        public val expectedAbsent: Boolean,
        public val documentDigest: String,
    ) : MemoryImportOperation

    /** Create a new immutable revision under an existing record. */
    public data class Supersede(
        public override val recordId: String,
        public val expectedActiveRevisionId: String,
        public val newRevisionId: String,
        public val authority: MemoryAuthority,
        public val documentDigest: String,
        public val supersedesLearnedRevisionId: String?,
    ) : MemoryImportOperation

    /** Archive an existing current revision. */
    public data class Archive(
        public override val recordId: String,
        public val expectedActiveRevisionId: String,
        public val documentDigest: String,
    ) : MemoryImportOperation

    /** Erase an existing current revision through M1 erasure. */
    public data class Erase(
        public override val recordId: String,
        public val expectedActiveRevisionId: String,
        public val documentDigest: String,
    ) : MemoryImportOperation
}

/** Canonical pure M2 import plan. */
public data class MemoryImportPlan(
    public val exportId: String,
    public val namespaceId: String,
    public val throughRevision: ULong,
    public val inputManifestDigest: String,
    public val expectedRepositoryRevision: ULong,
    public val operations: List<MemoryImportOperation>,
    public val addCount: ULong,
    public val supersedeCount: ULong,
    public val archiveCount: ULong,
    public val eraseCount: ULong,
    public val planDigest: String,
)
