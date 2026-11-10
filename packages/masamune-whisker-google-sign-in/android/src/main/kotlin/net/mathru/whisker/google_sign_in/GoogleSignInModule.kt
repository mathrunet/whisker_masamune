// Google サインイン。Flutter 版（masamune_auth_google_firebase）と同じく、Firebase Auth の OAuthProvider（Web 画面）でサインインする。
// 匿名ユーザーへの紐付け（link）も同じ画面で行う。

package net.mathru.whisker.google_sign_in

import android.app.Activity
import com.google.android.gms.tasks.Task
import com.google.firebase.auth.AuthResult
import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.auth.FirebaseAuthUserCollisionException
import com.google.firebase.auth.OAuthProvider
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class GoogleSignInModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("GoogleSignIn")

        // signIn({ scopes? }) -> { uid, isNewUser } / キャンセル時は "cancelled" で reject
        AsyncFunction("signIn") { args, promise ->
            val activity: Activity? = appContext.currentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            val auth = FirebaseAuth.getInstance()
            val task = auth.pendingAuthResult ?: auth.startActivityForSignInWithProvider(activity, provider(Bridge.args(args)))
            complete(task, promise)
        }

        // link({ scopes? }) -> { uid, isNewUser } — 今のユーザーに Google を紐付ける。
        // 別ユーザーとして登録済みなら "already-registered"、キャンセル時は "cancelled" で reject。
        AsyncFunction("link") { args, promise ->
            val activity: Activity? = appContext.currentActivity
            val user = FirebaseAuth.getInstance().currentUser
            if (activity == null || user == null) {
                Bridge.reject(promise, if (activity == null) "no activity" else "no user")
                return@AsyncFunction
            }
            complete(user.startActivityForLinkWithProvider(activity, provider(Bridge.args(args))), promise)
        }
    }

    private fun provider(args: org.json.JSONObject): OAuthProvider {
        val scopesJson = args.optJSONArray("scopes")
        val scopes = if (scopesJson == null) listOf("email", "profile") else (0 until scopesJson.length()).map { scopesJson.getString(it) }
        return OAuthProvider.newBuilder("google.com")
            .setScopes(scopes)
            .addCustomParameter("prompt", "select_account")
            .build()
    }

    private fun complete(task: Task<AuthResult>, promise: rs.whisker.runtime.WhiskerPromise) {
        task.addOnSuccessListener { result ->
            Bridge.resolve(
                promise,
                mapOf(
                    "uid" to result.user?.uid,
                    "isNewUser" to (result.additionalUserInfo?.isNewUser ?: false),
                ),
            )
        }.addOnFailureListener { error ->
            val message = error.message ?: ""
            when {
                error is FirebaseAuthUserCollisionException -> Bridge.reject(promise, "already-registered")
                message.contains("canceled", ignoreCase = true) || message.contains("cancelled", ignoreCase = true) ->
                    Bridge.reject(promise, "cancelled")
                else -> Bridge.reject(promise, error)
            }
        }
    }
}
