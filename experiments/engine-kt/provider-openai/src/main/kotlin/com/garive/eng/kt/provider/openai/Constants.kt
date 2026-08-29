package com.garive.eng.kt.provider.openai

internal object Constants {
    const val DEFAULT_ENDPOINT: String = "https://api.openai.com/v1/responses"
    const val AUTHORIZATION: String = "authorization"
    const val CONTENT_TYPE: String = "content-type"
    const val ACCEPT: String = "accept"
    val RESERVED_HEADERS: Set<String> = setOf(AUTHORIZATION, CONTENT_TYPE, ACCEPT)
}
