// Firebase AI Logic（Gemini Developer API）。生成・逐次生成・関数呼び出しの 1 往復を行う。
//
// 会話の履歴と関数の実行は呼び出し側（Rust）が持ち、ここは 1 回のリクエストを SDK で送るだけにする。
// リクエスト・レスポンスは Gemini の REST に近い JSON で受け渡す。
//
// request: {
//   model, systemInstruction?: string,
//   contents: [{ role, parts: [Part] }],
//   tools?: [{ name, description, parameters: { name: Schema }, optionalParameters?: [name] }],
//   functionCallingMode?: "auto" | "none" | "any",
//   generationConfig?: { temperature?, maxOutputTokens?, responseMimeType? },
// }
// Part: { text, thought?, thoughtSignature? } | { functionCall: { name, args, id? }, thoughtSignature? }
//     | { functionResponse: { name, response, id? } }
// Schema: { type: "string" | "integer" | "number" | "boolean" | "array" | "object", description?, nullable?,
//           enum?, items?, properties?, optionalProperties? }
// App Check が有効なら SDK がトークンを自動で付ける。

package net.mathru.whisker.firebase_ai

import com.google.firebase.Firebase
import com.google.firebase.ai.ai
import com.google.firebase.ai.type.Content
import com.google.firebase.ai.type.FunctionCallPart
import com.google.firebase.ai.type.FunctionCallingConfig
import com.google.firebase.ai.type.FunctionDeclaration
import com.google.firebase.ai.type.FunctionResponsePart
import com.google.firebase.ai.type.GenerateContentResponse
import com.google.firebase.ai.type.GenerativeBackend
import com.google.firebase.ai.type.Part
import com.google.firebase.ai.type.PublicPreviewAPI
import com.google.firebase.ai.type.Schema
import com.google.firebase.ai.type.TextPart
import com.google.firebase.ai.type.Tool
import com.google.firebase.ai.type.ToolConfig
import com.google.firebase.ai.type.generationConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import java.util.concurrent.CancellationException

@WhiskerModule
class FirebaseAiModule : Module() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val jobs = mutableMapOf<String, Job>()

    override fun definition() = ModuleDefinition {
        Name("FirebaseAi")
        Events("chunk")

        // generate({ request }) -> { parts, usage, finishReason }
        AsyncFunction("generate") { args, promise ->
            scope.launch {
                try {
                    val request = Bridge.args(args).getJSONObject("request")
                    val response = model(request).generateContent(contents(request))
                    Bridge.resolve(promise, chunk(response))
                } catch (error: Throwable) {
                    Bridge.reject(promise, error)
                }
            }
        }

        // stream({ requestId, request }) — 断片ごとに "chunk"（{ requestId, parts, usage, finishReason }）、
        // 最後に { requestId, done: true }、失敗は { requestId, error } を送る。
        Function("stream") { args ->
            try {
                val json = Bridge.args(args)
                val requestId = json.getString("requestId")
                val request = json.getJSONObject("request")
                jobs.remove(requestId)?.cancel()
                jobs[requestId] = scope.launch {
                    try {
                        model(request).generateContentStream(contents(request)).collect { response ->
                            sendEvent("chunk", Bridge.event(chunk(response) + ("requestId" to requestId)))
                        }
                        sendEvent("chunk", Bridge.event(mapOf("requestId" to requestId, "done" to true)))
                    } catch (_: CancellationException) {
                        sendEvent("chunk", Bridge.event(mapOf("requestId" to requestId, "error" to "cancelled")))
                    } catch (error: Throwable) {
                        sendEvent("chunk", Bridge.event(mapOf("requestId" to requestId, "error" to (error.message ?: error.javaClass.simpleName))))
                    } finally {
                        jobs.remove(requestId)
                    }
                }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // cancel({ requestId })
        Function("cancel") { args ->
            jobs.remove(Bridge.args(args).optString("requestId"))?.cancel()
            Bridge.ok(true)
        }
    }

    private fun model(request: JSONObject) = Firebase.ai(backend = GenerativeBackend.googleAI()).generativeModel(
        modelName = request.getString("model"),
        generationConfig = request.optJSONObject("generationConfig")?.let { config ->
            generationConfig {
                if (config.has("temperature")) temperature = config.getDouble("temperature").toFloat()
                if (config.has("maxOutputTokens")) maxOutputTokens = config.getInt("maxOutputTokens")
                if (config.has("responseMimeType")) responseMimeType = config.getString("responseMimeType")
            }
        },
        tools = request.optJSONArray("tools")?.takeIf { it.length() > 0 }?.let { listOf(Tool.functionDeclarations(declarations(it))) },
        toolConfig = request.optJSONArray("tools")?.takeIf { it.length() > 0 }?.let {
            ToolConfig(
                when (request.optString("functionCallingMode", "auto")) {
                    "none" -> FunctionCallingConfig.none()
                    "any" -> FunctionCallingConfig.any()
                    else -> FunctionCallingConfig.auto()
                },
            )
        },
        systemInstruction = request.optString("systemInstruction").takeIf { it.isNotEmpty() }?.let {
            Content(role = "system", parts = listOf(TextPart(it)))
        },
    )

    private fun declarations(tools: JSONArray): List<FunctionDeclaration> = (0 until tools.length()).map { index ->
        val tool = tools.getJSONObject(index)
        val parameters = tool.optJSONObject("parameters") ?: JSONObject()
        FunctionDeclaration(
            tool.getString("name"),
            tool.optString("description"),
            parameters.keys().asSequence().associateWith { schema(parameters.getJSONObject(it)) },
            tool.optJSONArray("optionalParameters")?.let { list -> (0 until list.length()).map { list.getString(it) } } ?: emptyList(),
        )
    }

    private fun schema(json: JSONObject): Schema {
        val description = json.optString("description").takeIf { it.isNotEmpty() }
        val nullable = json.optBoolean("nullable", false)
        json.optJSONArray("enum")?.let { values ->
            return Schema.enumeration((0 until values.length()).map { values.getString(it) }, description, nullable)
        }
        return when (json.optString("type", "string")) {
            "integer" -> Schema.integer(description = description, nullable = nullable)
            "number" -> Schema.double(description = description, nullable = nullable)
            "boolean" -> Schema.boolean(description = description, nullable = nullable)
            "array" -> Schema.array(schema(json.optJSONObject("items") ?: JSONObject()), description = description, nullable = nullable)
            "object" -> {
                val properties = json.optJSONObject("properties") ?: JSONObject()
                Schema.obj(
                    properties.keys().asSequence().associateWith { schema(properties.getJSONObject(it)) },
                    json.optJSONArray("optionalProperties")?.let { list -> (0 until list.length()).map { list.getString(it) } } ?: emptyList(),
                    description = description,
                    nullable = nullable,
                )
            }
            else -> Schema.string(description = description, nullable = nullable)
        }
    }

    private fun contents(request: JSONObject): List<Content> {
        val contents = request.optJSONArray("contents") ?: return emptyList()
        return (0 until contents.length()).map { index ->
            val content = contents.getJSONObject(index)
            val parts = content.optJSONArray("parts") ?: JSONArray()
            Content(role = content.optString("role", "user"), parts = (0 until parts.length()).map { part(parts.getJSONObject(it)) })
        }
    }

    @OptIn(PublicPreviewAPI::class)
    private fun part(json: JSONObject): Part {
        val signature = json.optString("thoughtSignature").takeIf { it.isNotEmpty() }
        json.optJSONObject("functionCall")?.let { call ->
            val args = Json.parseToJsonElement((call.optJSONObject("args") ?: JSONObject()).toString()).jsonObject
            return FunctionCallPart.createWithThinking(call.getString("name"), args, call.optString("id").takeIf { it.isNotEmpty() }, false, signature)
        }
        json.optJSONObject("functionResponse")?.let { response ->
            val body = Json.parseToJsonElement((response.optJSONObject("response") ?: JSONObject()).toString()).jsonObject
            return FunctionResponsePart(response.getString("name"), body, response.optString("id").takeIf { it.isNotEmpty() })
        }
        return TextPart.createWithThinking(json.optString("text"), json.optBoolean("thought", false), signature)
    }

    private fun chunk(response: GenerateContentResponse): Map<String, Any?> {
        val candidate = response.candidates.firstOrNull()
        val parts = candidate?.content?.parts?.mapNotNull { part ->
            when (part) {
                is TextPart -> mapOf("text" to part.text, "thought" to part.isThought, "thoughtSignature" to part.thoughtSignature)
                is FunctionCallPart -> mapOf(
                    "functionCall" to mapOf("name" to part.name, "args" to JSONObject(JsonObject(part.args).toString()), "id" to part.id),
                    "thoughtSignature" to part.thoughtSignature,
                )
                else -> null
            }
        } ?: emptyList()
        val usage = response.usageMetadata
        return mapOf(
            "parts" to parts,
            "usage" to mapOf(
                "promptTokenCount" to (usage?.promptTokenCount ?: 0),
                "candidatesTokenCount" to (usage?.candidatesTokenCount ?: 0),
            ),
            "finishReason" to candidate?.finishReason?.name,
        )
    }
}
