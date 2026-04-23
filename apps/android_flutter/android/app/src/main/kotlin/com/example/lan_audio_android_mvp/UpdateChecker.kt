package com.example.lan_audio_android_mvp

import android.util.Log
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

data class AndroidUpdateInfo(
    val latestVersion: String,
    val releaseUrl: String,
)

object UpdateChecker {
    private const val TAG = "lan_audio_update"
    private const val LATEST_API = "https://api.github.com/repos/lan-audio/lan-audio/releases/latest"
    private const val RELEASE_FALLBACK_URL = "https://github.com/lan-audio/lan-audio/releases"

    fun checkForUpdate(currentVersion: String): AndroidUpdateInfo? {
        return try {
            val conn = URL(LATEST_API).openConnection() as HttpURLConnection
            conn.requestMethod = "GET"
            conn.connectTimeout = 5000
            conn.readTimeout = 5000
            conn.setRequestProperty("Accept", "application/vnd.github+json")
            conn.setRequestProperty("User-Agent", "lan-audio-android-update-checker")
            if (conn.responseCode !in 200..299) {
                conn.disconnect()
                return null
            }
            val body = conn.inputStream.bufferedReader().use { it.readText() }
            conn.disconnect()
            val json = JSONObject(body)
            if (json.optBoolean("draft") || json.optBoolean("prerelease")) {
                return null
            }
            val tag = json.optString("tag_name", "").removePrefix("v").trim()
            if (!isNewer(tag, currentVersion)) {
                return null
            }
            AndroidUpdateInfo(
                latestVersion = tag,
                releaseUrl = json.optString("html_url").ifBlank { RELEASE_FALLBACK_URL },
            )
        } catch (t: Throwable) {
            Log.d(TAG, "silent update check ignored: ${t.message}")
            null
        }
    }

    private fun isNewer(latest: String, current: String): Boolean {
        val latestParts = latest.split(".").mapNotNull { it.toIntOrNull() }
        val currentParts = current.removePrefix("v").split(".").mapNotNull { it.toIntOrNull() }
        if (latestParts.isEmpty() || currentParts.isEmpty()) {
            return false
        }
        val max = maxOf(latestParts.size, currentParts.size)
        for (i in 0 until max) {
            val lv = latestParts.getOrElse(i) { 0 }
            val cv = currentParts.getOrElse(i) { 0 }
            if (lv > cv) return true
            if (lv < cv) return false
        }
        return false
    }
}
