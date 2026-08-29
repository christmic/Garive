package com.garive.eng.kt.tools

import kotlinx.serialization.json.JsonElement

/** Validates a schema against Garive's portable value-schema subset. */
public fun validatePortableValueSchema(schema: JsonElement): Boolean =
    PortableSchema.validateValueDefinition(schema) == null

/** Validates a value against a previously validated portable value schema. */
public fun validatePortableValue(schema: JsonElement, value: JsonElement): Boolean =
    validatePortableValueSchema(schema) && PortableSchema.validateArguments(schema, value).isEmpty()
