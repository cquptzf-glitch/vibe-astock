package com.vibeastock.securestorage

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private const val KEYSTORE_PROVIDER = "AndroidKeyStore"
private const val MASTER_KEY_ALIAS = "vibe_astock_secure_storage_v1"
private const val PREFS_NAME = "vibe_astock_secure_storage"
private const val MAX_SECRET_BYTES = 16 * 1024
private const val GCM_TAG_BITS = 128

@InvokeArg
class SecretKeyArgs {
    lateinit var key: String
}

@InvokeArg
class SecretSetArgs {
    lateinit var key: String
    lateinit var value: String
}

@TauriPlugin
class SecureStoragePlugin(private val activity: Activity) : Plugin(activity) {
    private val preferences by lazy {
        activity.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
    }

    @Command
    fun setSecret(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SecretSetArgs::class.java)
            validateKey(args.key)
            val plaintext = args.value.toByteArray(StandardCharsets.UTF_8)
            require(plaintext.size <= MAX_SECRET_BYTES) { "secret is too large" }

            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, getOrCreateMasterKey())
            val encrypted = cipher.doFinal(plaintext)

            val editor = preferences.edit()
                .putString(ivKey(args.key), Base64.encodeToString(cipher.iv, Base64.NO_WRAP))
                .putString(dataKey(args.key), Base64.encodeToString(encrypted, Base64.NO_WRAP))
            if (!editor.commit()) {
                throw IllegalStateException("failed to persist encrypted secret")
            }
            invoke.resolve(okResponse())
        } catch (error: Exception) {
            invoke.reject(error.message ?: "secure storage write failed")
        }
    }

    @Command
    fun getSecret(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SecretKeyArgs::class.java)
            validateKey(args.key)
            val ivEncoded = preferences.getString(ivKey(args.key), null)
            val dataEncoded = preferences.getString(dataKey(args.key), null)
            if (ivEncoded == null || dataEncoded == null) {
                invoke.resolve(JSObject().apply { put("value", null) })
                return
            }

            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            val iv = Base64.decode(ivEncoded, Base64.NO_WRAP)
            val encrypted = Base64.decode(dataEncoded, Base64.NO_WRAP)
            cipher.init(Cipher.DECRYPT_MODE, getOrCreateMasterKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
            val plaintext = cipher.doFinal(encrypted)
            val value = String(plaintext, StandardCharsets.UTF_8)
            invoke.resolve(JSObject().apply { put("value", value) })
        } catch (error: Exception) {
            // A ciphertext restored without its device-bound Keystore key cannot be decrypted.
            // Remove the unusable record rather than repeatedly returning corrupted data.
            runCatching {
                val args = invoke.parseArgs(SecretKeyArgs::class.java)
                preferences.edit().remove(ivKey(args.key)).remove(dataKey(args.key)).commit()
            }
            invoke.reject(error.message ?: "secure storage read failed")
        }
    }

    @Command
    fun containsSecret(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SecretKeyArgs::class.java)
            validateKey(args.key)
            val present = preferences.contains(ivKey(args.key)) && preferences.contains(dataKey(args.key))
            invoke.resolve(JSObject().apply { put("present", present) })
        } catch (error: Exception) {
            invoke.reject(error.message ?: "secure storage lookup failed")
        }
    }

    @Command
    fun deleteSecret(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SecretKeyArgs::class.java)
            validateKey(args.key)
            if (!preferences.edit().remove(ivKey(args.key)).remove(dataKey(args.key)).commit()) {
                throw IllegalStateException("failed to remove encrypted secret")
            }
            invoke.resolve(okResponse())
        } catch (error: Exception) {
            invoke.reject(error.message ?: "secure storage delete failed")
        }
    }

    private fun getOrCreateMasterKey(): SecretKey {
        val keyStore = KeyStore.getInstance(KEYSTORE_PROVIDER).apply { load(null) }
        val existing = keyStore.getKey(MASTER_KEY_ALIAS, null)
        if (existing is SecretKey) {
            return existing
        }

        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE_PROVIDER)
        val spec = KeyGenParameterSpec.Builder(
            MASTER_KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setRandomizedEncryptionRequired(true)
            .setUserAuthenticationRequired(false)
            .build()
        generator.init(spec)
        return generator.generateKey()
    }

    private fun validateKey(key: String) {
        require(key.matches(Regex("[A-Za-z0-9_.-]{1,128}"))) { "invalid secure storage key" }
    }

    private fun ivKey(key: String) = "$key.iv"
    private fun dataKey(key: String) = "$key.data"

    private fun okResponse() = JSObject().apply { put("ok", true) }
}
