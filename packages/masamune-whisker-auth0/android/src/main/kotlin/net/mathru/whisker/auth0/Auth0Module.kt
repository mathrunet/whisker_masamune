// Auth0 Universal Login and credentials storage with Auth0.Android. Mirrors Auth0Module.swift.
//
// Login opens Universal Login in Custom Tabs with the callback
// `{scheme}://{domain}/android/{application id}/callback` (Auth0.Android's RedirectActivity, whose
// intent filter comes from the auth0Scheme / auth0Domain manifest placeholders set by the CNG
// plugin) and stores the credentials with SecureCredentialsManager (Android Keystore).
// Rejections are `cancelled`, `no-credentials` or `<code>: <message>`.
//
// Every call receives the account as JSON: { domain, clientId, scheme, scope, prompt?, minTtl? }.
// Credentials are returned as
// { accessToken, idToken, refreshToken?, expiresAtMs, scope?, user: { sub, email?, name? } }.

package net.mathru.whisker.auth0

import android.content.Context
import com.auth0.android.Auth0
import com.auth0.android.authentication.AuthenticationAPIClient
import com.auth0.android.authentication.AuthenticationException
import com.auth0.android.authentication.storage.CredentialsManagerException
import com.auth0.android.authentication.storage.SecureCredentialsManager
import com.auth0.android.authentication.storage.SharedPreferencesStorage
import com.auth0.android.callback.Callback
import com.auth0.android.provider.WebAuthProvider
import com.auth0.android.result.Credentials
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise

@WhiskerModule
class Auth0Module : Module() {
    override fun definition() = ModuleDefinition {
        Name("Auth0")

        // login({ ..., prompt? }) -> credentials / rejects with "cancelled"
        AsyncFunction("login") { args, promise ->
            val json = Bridge.args(args)
            val account = Account.from(json) ?: return@AsyncFunction invalid(promise)
            val activity = appContext.currentActivity ?: return@AsyncFunction Bridge.reject(promise, "no activity")
            val prompt = json.optString("prompt", "").takeIf { it.isNotEmpty() && !json.isNull("prompt") }
            Bridge.onMain {
                val builder = WebAuthProvider.login(account.auth0)
                    .withScheme(account.scheme)
                    .withScope(account.scope)
                if (prompt != null) builder.withParameters(mapOf("prompt" to prompt))
                builder.start(activity, object : Callback<Credentials, AuthenticationException> {
                    override fun onSuccess(result: Credentials) {
                        try {
                            manager(activity, account).saveCredentials(result)
                        } catch (error: CredentialsManagerException) {
                            return Bridge.reject(promise, "credentials_manager: ${error.message ?: "save failed"}")
                        }
                        Bridge.resolve(promise, encode(result))
                    }

                    override fun onFailure(error: AuthenticationException) {
                        if (error.isCanceled) {
                            Bridge.reject(promise, "cancelled")
                        } else {
                            Bridge.reject(promise, message(error))
                        }
                    }
                })
            }
        }

        // logout({ ... }) -> null. Clears the browser session, then the stored credentials.
        AsyncFunction("logout") { args, promise ->
            val account = Account.from(Bridge.args(args)) ?: return@AsyncFunction invalid(promise)
            val activity = appContext.currentActivity ?: return@AsyncFunction Bridge.reject(promise, "no activity")
            Bridge.onMain {
                WebAuthProvider.logout(account.auth0)
                    .withScheme(account.scheme)
                    .start(activity, object : Callback<Void?, AuthenticationException> {
                        override fun onSuccess(result: Void?) {
                            manager(activity, account).clearCredentials()
                            Bridge.resolve(promise, null)
                        }

                        override fun onFailure(error: AuthenticationException) {
                            manager(activity, account).clearCredentials()
                            if (error.isCanceled) {
                                Bridge.reject(promise, "cancelled")
                            } else {
                                Bridge.reject(promise, message(error))
                            }
                        }
                    })
            }
        }

        // credentials({ ..., minTtl }) -> credentials (renewed and stored when needed)
        // / rejects with "no-credentials" when nothing usable is stored.
        AsyncFunction("credentials") { args, promise ->
            val json = Bridge.args(args)
            val account = Account.from(json) ?: return@AsyncFunction invalid(promise)
            val context = context() ?: return@AsyncFunction Bridge.reject(promise, "no context")
            val minTtl = json.optInt("minTtl", 0)
            val manager = manager(context, account)
            // hasValidCredentials is true for renewable credentials as well.
            if (!manager.hasValidCredentials(minTtl.toLong())) {
                return@AsyncFunction Bridge.reject(promise, "no-credentials")
            }
            manager.getCredentials(
                account.scope,
                minTtl,
                emptyMap(),
                object : Callback<Credentials, CredentialsManagerException> {
                    override fun onSuccess(result: Credentials) {
                        Bridge.resolve(promise, encode(result))
                    }

                    override fun onFailure(error: CredentialsManagerException) {
                        Bridge.reject(promise, message(error))
                    }
                },
            )
        }

        // hasValidCredentials({ ..., minTtl }) -> Boolean (valid for minTtl or renewable)
        AsyncFunction("hasValidCredentials") { args, promise ->
            val json = Bridge.args(args)
            val account = Account.from(json) ?: return@AsyncFunction invalid(promise)
            val context = context() ?: return@AsyncFunction Bridge.reject(promise, "no context")
            Bridge.resolve(promise, manager(context, account).hasValidCredentials(json.optInt("minTtl", 0).toLong()))
        }

        // clear({ ... }) -> null. Removes the stored credentials only.
        AsyncFunction("clear") { args, promise ->
            val account = Account.from(Bridge.args(args)) ?: return@AsyncFunction invalid(promise)
            val context = context() ?: return@AsyncFunction Bridge.reject(promise, "no context")
            manager(context, account).clearCredentials()
            Bridge.resolve(promise, null)
        }
    }

    private data class Account(
        val domain: String,
        val clientId: String,
        val scheme: String,
        val scope: String,
    ) {
        val auth0: Auth0 get() = Auth0.getInstance(clientId, domain)

        companion object {
            fun from(json: JSONObject): Account? {
                val domain = json.optString("domain", "")
                val clientId = json.optString("clientId", "")
                val scheme = json.optString("scheme", "")
                if (domain.isEmpty() || clientId.isEmpty() || scheme.isEmpty()) return null
                val scope = json.optString("scope", "").ifEmpty { "openid profile email offline_access" }
                return Account(domain, clientId, scheme, scope)
            }
        }
    }

    private fun invalid(promise: WhiskerPromise) {
        Bridge.reject(promise, "invalid-config: domain, clientId and scheme are required")
    }

    private fun context(): Context? = WhiskerApplication.appContext ?: appContext.currentActivity?.applicationContext

    private fun message(error: AuthenticationException): String {
        val code = error.getCode().ifEmpty { "authentication" }.replace(Regex("\\s+"), "_")
        return "$code: ${error.getDescription()}"
    }

    private fun message(error: CredentialsManagerException): String =
        "credentials_manager: ${error.message ?: error.javaClass.simpleName}"

    private fun encode(credentials: Credentials): Map<String, Any?> {
        val user = credentials.user
        val sub = user.getExtraInfo()["sub"] as? String ?: ""
        return mapOf(
            "accessToken" to credentials.accessToken,
            "idToken" to credentials.idToken,
            "refreshToken" to credentials.refreshToken,
            "expiresAtMs" to credentials.expiresAt.time,
            "scope" to credentials.scope,
            "user" to mapOf(
                "sub" to sub,
                "email" to user.email,
                "name" to user.name,
            ),
        )
    }

    private companion object {
        private val managers = HashMap<Account, SecureCredentialsManager>()

        /** One manager per account, so concurrent renewals are serialized by the SDK. */
        @Synchronized
        fun manager(context: Context, account: Account): SecureCredentialsManager =
            managers.getOrPut(account) {
                val app = context.applicationContext
                SecureCredentialsManager(
                    AuthenticationAPIClient(account.auth0),
                    app,
                    account.auth0,
                    SharedPreferencesStorage(app),
                )
            }
    }
}
