// Firebase AI Logic (Gemini Developer API). Performs one round trip of generation, streaming, or function calling.
//
// The caller (Rust) owns the conversation history and function execution; this module only sends a single request through the SDK.
// Requests and responses are passed as JSON close to the Gemini REST API.
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
// When App Check is enabled, the SDK attaches the token automatically.
//
// Mirrors FirebaseAiModule.kt. The iOS SDK keeps thought signatures internal, so contents are built by
// decoding their REST JSON (`ModelContent` is Decodable) and signatures are read back by encoding parts.

import FirebaseAILogic
import FirebaseCore
import Foundation
import WhiskerModule

@WhiskerModule
public final class FirebaseAiModule: Module {
    private let lock = NSLock()
    private var jobs: [String: (token: UUID, task: Task<Void, Never>)] = [:]

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("FirebaseAi")
            Events("chunk")

            // generate({ request }) -> { parts, usage, finishReason }
            AsyncFunction("generate") { args, promise in
                let json = Bridge.args(args)
                Task {
                    do {
                        guard let request = json["request"] as? [String: Any] else {
                            throw AiError("JSONObject[\"request\"] not found.")
                        }
                        let model = try Self.model(request)
                        let response = try await model.generateContent(Self.contents(request))
                        Bridge.resolve(promise, Self.chunk(response))
                    } catch {
                        Bridge.reject(promise, Self.message(error))
                    }
                }
            }

            // stream({ requestId, request }) — sends a "chunk" event ({ requestId, parts, usage, finishReason }) per chunk,
            // then { requestId, done: true } at the end, or { requestId, error } on failure.
            Function("stream") { [weak self] args in
                guard let self else { return Bridge.err("FirebaseAi module released") }
                let json = Bridge.args(args)
                guard let requestId = json["requestId"] as? String else {
                    return Bridge.err("JSONObject[\"requestId\"] not found.")
                }
                guard let request = json["request"] as? [String: Any] else {
                    return Bridge.err("JSONObject[\"request\"] not found.")
                }
                self.cancel(requestId)
                let token = UUID()
                let task = Task { [weak self] in
                    do {
                        let stream = try Self.model(request).generateContentStream(Self.contents(request))
                        for try await response in stream {
                            try Task.checkCancellation()
                            var payload = Self.chunk(response)
                            payload["requestId"] = requestId
                            self?.emit(payload)
                        }
                        try Task.checkCancellation()
                        self?.emit(["requestId": requestId, "done": true])
                    } catch {
                        if Task.isCancelled || error is CancellationError {
                            self?.emit(["requestId": requestId, "error": "cancelled"])
                        } else {
                            self?.emit(["requestId": requestId, "error": Self.message(error)])
                        }
                    }
                    self?.finish(requestId, token)
                }
                self.lock.lock()
                self.jobs[requestId] = (token, task)
                self.lock.unlock()
                return Bridge.ok(true)
            }

            // cancel({ requestId })
            Function("cancel") { [weak self] args in
                self?.cancel(Bridge.args(args)["requestId"] as? String ?? "")
                return Bridge.ok(true)
            }
        }
    }

    // MARK: - Streams

    private func cancel(_ requestId: String) {
        lock.lock()
        let job = jobs.removeValue(forKey: requestId)
        lock.unlock()
        job?.task.cancel()
    }

    private func finish(_ requestId: String, _ token: UUID) {
        lock.lock()
        if jobs[requestId]?.token == token { jobs.removeValue(forKey: requestId) }
        lock.unlock()
    }

    /// Events go out on the main thread, in order.
    private func emit(_ payload: [String: Any?]) {
        Bridge.onMain { [weak self] in self?.sendEvent("chunk", Bridge.event(payload)) }
    }

    // MARK: - Requests

    private static func model(_ request: [String: Any]) throws -> GenerativeModel {
        try FirebaseBoot.ensure()
        guard let name = request["model"] as? String else { throw AiError("JSONObject[\"model\"] not found.") }
        var generationConfig: GenerationConfig?
        if let config = request["generationConfig"] as? [String: Any] {
            generationConfig = GenerationConfig(
                temperature: (config["temperature"] as? NSNumber).map { $0.floatValue },
                maxOutputTokens: (config["maxOutputTokens"] as? NSNumber).map { $0.intValue },
                responseMIMEType: config["responseMimeType"] as? String
            )
        }
        var tools: [Tool]?
        var toolConfig: ToolConfig?
        if let declarations = request["tools"] as? [Any], !declarations.isEmpty {
            tools = [.functionDeclarations(try Self.declarations(declarations))]
            let mode: FunctionCallingConfig
            switch request["functionCallingMode"] as? String ?? "auto" {
            case "none": mode = .none()
            case "any": mode = .any()
            default: mode = .auto()
            }
            toolConfig = ToolConfig(functionCallingConfig: mode)
        }
        var systemInstruction: ModelContent?
        if let text = request["systemInstruction"] as? String, !text.isEmpty {
            systemInstruction = ModelContent(role: "system", parts: [TextPart(text)])
        }
        return FirebaseAI.firebaseAI(backend: .googleAI()).generativeModel(
            modelName: name,
            generationConfig: generationConfig,
            tools: tools,
            toolConfig: toolConfig,
            systemInstruction: systemInstruction
        )
    }

    private static func declarations(_ tools: [Any]) throws -> [FunctionDeclaration] {
        try tools.map { item in
            guard let tool = item as? [String: Any], let name = tool["name"] as? String else {
                throw AiError("JSONObject[\"name\"] not found.")
            }
            let parameters = tool["parameters"] as? [String: Any] ?? [:]
            return FunctionDeclaration(
                name: name,
                description: tool["description"] as? String ?? "",
                parameters: parameters.mapValues { schema($0 as? [String: Any] ?? [:]) },
                optionalParameters: tool["optionalParameters"] as? [String] ?? []
            )
        }
    }

    private static func schema(_ json: [String: Any]) -> Schema {
        let description = (json["description"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let nullable = json["nullable"] as? Bool ?? false
        if let values = json["enum"] as? [Any] {
            return .enumeration(values: values.map { "\($0)" }, description: description, nullable: nullable)
        }
        switch json["type"] as? String ?? "string" {
        case "integer":
            return .integer(description: description, nullable: nullable)
        case "number":
            return .double(description: description, nullable: nullable)
        case "boolean":
            return .boolean(description: description, nullable: nullable)
        case "array":
            return .array(items: schema(json["items"] as? [String: Any] ?? [:]), description: description, nullable: nullable)
        case "object":
            let properties = json["properties"] as? [String: Any] ?? [:]
            return .object(
                properties: properties.mapValues { schema($0 as? [String: Any] ?? [:]) },
                optionalProperties: json["optionalProperties"] as? [String] ?? [],
                description: description,
                nullable: nullable
            )
        default:
            return .string(description: description, nullable: nullable)
        }
    }

    /// Builds the SDK contents from their REST JSON so thought signatures are kept.
    private static func contents(_ request: [String: Any]) throws -> [ModelContent] {
        let decoder = JSONDecoder()
        return try (request["contents"] as? [Any] ?? []).map { item in
            let content = item as? [String: Any] ?? [:]
            let parts = (content["parts"] as? [Any] ?? []).map { part($0 as? [String: Any] ?? [:]) }
            let rest: [String: Any] = ["role": content["role"] as? String ?? "user", "parts": parts]
            let data = try JSONSerialization.data(withJSONObject: rest)
            return try decoder.decode(ModelContent.self, from: data)
        }
    }

    /// One part in REST form, with the same defaults as Android.
    private static func part(_ json: [String: Any]) -> [String: Any] {
        var rest: [String: Any] = [:]
        if let signature = json["thoughtSignature"] as? String, !signature.isEmpty {
            rest["thoughtSignature"] = signature
        }
        if let call = json["functionCall"] as? [String: Any] {
            var functionCall: [String: Any] = [
                "name": call["name"] as? String ?? "",
                "args": call["args"] as? [String: Any] ?? [:],
            ]
            if let id = call["id"] as? String, !id.isEmpty { functionCall["id"] = id }
            rest["functionCall"] = functionCall
            return rest
        }
        if let response = json["functionResponse"] as? [String: Any] {
            var functionResponse: [String: Any] = [
                "name": response["name"] as? String ?? "",
                "response": response["response"] as? [String: Any] ?? [:],
            ]
            if let id = response["id"] as? String, !id.isEmpty { functionResponse["id"] = id }
            // Android sends function responses without a thought signature.
            return ["functionResponse": functionResponse]
        }
        rest["text"] = json["text"] as? String ?? ""
        if json["thought"] as? Bool == true { rest["thought"] = true }
        return rest
    }

    // MARK: - Responses

    private static func chunk(_ response: GenerateContentResponse) -> [String: Any?] {
        let candidate = response.candidates.first
        let parts: [[String: Any?]] = candidate?.content.parts.compactMap { part in
            switch part {
            case let part as TextPart:
                return ["text": part.text, "thought": part.isThought, "thoughtSignature": signature(part)]
            case let part as FunctionCallPart:
                return [
                    "functionCall": ["name": part.name, "args": arguments(part), "id": part.functionId] as [String: Any?],
                    "thoughtSignature": signature(part),
                ]
            default:
                return nil
            }
        } ?? []
        let usage = response.usageMetadata
        return [
            "parts": parts,
            "usage": [
                "promptTokenCount": usage?.promptTokenCount ?? 0,
                "candidatesTokenCount": usage?.candidatesTokenCount ?? 0,
            ],
            "finishReason": candidate?.finishReason?.rawValue,
        ]
    }

    /// The part as REST JSON (the SDK's own encoding).
    private static func encoded(_ part: any Part) -> [String: Any]? {
        guard let data = try? JSONEncoder().encode(ModelContent(role: nil, parts: [part])),
              let content = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let parts = content["parts"] as? [[String: Any]]
        else { return nil }
        return parts.first
    }

    private static func signature(_ part: any Part) -> String? {
        encoded(part)?["thoughtSignature"] as? String
    }

    private static func arguments(_ part: FunctionCallPart) -> [String: Any] {
        let call = encoded(part)?["functionCall"] as? [String: Any]
        return call?["args"] as? [String: Any] ?? [:]
    }

    // MARK: - Errors

    /// A readable message, like the exception messages of the Android SDK.
    private static func message(_ error: Error) -> String {
        switch error {
        case let error as GenerateContentError:
            switch error {
            case let .internalError(underlying), let .promptImageContentError(underlying):
                return message(underlying)
            case let .promptBlocked(response):
                let reason = response.promptFeedback?.blockReason?.rawValue ?? "unknown"
                return "Prompt was blocked: \(reason)"
            case let .responseStoppedEarly(reason, _):
                return "Content generation stopped. Reason: \(reason.rawValue)"
            }
        case let error as AiError:
            return error.message
        default:
            let ns = error as NSError
            if let description = (error as? LocalizedError)?.errorDescription, !description.isEmpty {
                return description
            }
            return ns.localizedDescription.isEmpty ? "\(ns.domain) \(ns.code)" : ns.localizedDescription
        }
    }

    private struct AiError: Error {
        let message: String
        init(_ message: String) { self.message = message }
    }
}
