package com.garive.eng.kt.tools

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class T1PatchTest {
    @Test
    fun appliesOrderedUniqueHunksAndPreservesNewline() {
        val patch = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n alpha\n-beta\n+BETA\n@@\n gamma\n+delta\n*** End Patch"
        assertEquals(setOf("src/lib.rs"), t1PatchTargets(patch))
        assertEquals(
            "alpha\nBETA\ngamma\ndelta\n",
            assertIs<T1PatchResult.Success>(applyT1Patch(patch, "src/lib.rs", "alpha\nbeta\ngamma\n")).value,
        )
    }

    @Test
    fun rejectsAmbiguousMissingAndUnanchoredHunks() {
        val ambiguous = "*** Begin Patch\n*** Update File: f\n@@\n-same\n+new\n*** End Patch"
        assertEquals(
            T1PatchError.CONTEXT_MISMATCH,
            assertIs<T1PatchResult.Failure>(applyT1Patch(ambiguous, "f", "same\nsame\n")).error,
        )
        assertEquals(
            T1PatchError.TARGET_MISSING,
            assertIs<T1PatchResult.Failure>(applyT1Patch(ambiguous, "missing", "same\n")).error,
        )
        assertEquals(null, t1PatchTargets("*** Begin Patch\n*** Update File: f\n@@\n+new\n*** End Patch"))
    }

    @Test
    fun validatesFinalNoNewlineMarker() {
        val patch = "*** Begin Patch\n*** Update File: f\n@@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n*** End Patch"
        assertEquals("new", assertIs<T1PatchResult.Success>(applyT1Patch(patch, "f", "old")).value)
        assertEquals(
            T1PatchError.CONTEXT_MISMATCH,
            assertIs<T1PatchResult.Failure>(applyT1Patch(patch, "f", "old\n")).error,
        )
    }
}
