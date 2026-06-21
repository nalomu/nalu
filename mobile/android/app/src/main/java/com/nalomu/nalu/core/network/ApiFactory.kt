package com.nalomu.nalu.core.network

import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import retrofit2.Retrofit
import retrofit2.converter.kotlinx.serialization.asConverterFactory

object ApiFactory {
    private val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
    }

    fun create(baseUrl: String, token: String? = null): NaluApi {
        val normalizedUrl = baseUrl.trim().trimEnd('/') + "/api/"
        val client = OkHttpClient.Builder()
            .apply {
                if (!token.isNullOrBlank()) {
                    addInterceptor { chain ->
                        val request = chain.request().newBuilder()
                            .addHeader("Authorization", "Bearer $token")
                            .build()
                        chain.proceed(request)
                    }
                }
            }
            .build()

        return Retrofit.Builder()
            .baseUrl(normalizedUrl)
            .client(client)
            .addConverterFactory(json.asConverterFactory("application/json".toMediaType()))
            .build()
            .create(NaluApi::class.java)
    }
}
