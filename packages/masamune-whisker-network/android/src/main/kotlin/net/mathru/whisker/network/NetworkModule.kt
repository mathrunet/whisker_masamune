// Interface addresses (java.net.NetworkInterface) and DNS-SD discovery (NsdManager).

package net.mathru.whisker.network

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import android.os.Handler
import android.os.Looper
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.NetworkInterface

@WhiskerModule
class NetworkModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("Network")

        // interfaces() -> [{ name, ipv4, ipv6, pointToPoint }]
        Function("interfaces") { _ ->
            try {
                Bridge.ok(interfaces())
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // discoverServices({ serviceType, timeoutMs }) -> [{ name, serviceType, addresses, port }]
        AsyncFunction("discoverServices") { args, promise ->
            val json = Bridge.args(args)
            val serviceType = json.optString("serviceType")
            val timeoutMs = json.optLong("timeoutMs", 3000L).coerceIn(500L, 30_000L)
            val context = WhiskerApplication.appContext
            when {
                serviceType.isBlank() -> Bridge.reject(promise, "serviceType is required")
                context == null -> Bridge.reject(promise, "no context")
                else -> Bridge.onMain { Discovery(context, serviceType, timeoutMs, promise).start() }
            }
        }
    }

    private fun interfaces(): List<Map<String, Any?>> {
        val list = NetworkInterface.getNetworkInterfaces()?.toList() ?: return emptyList()
        return list.filter { it.isUp && !it.isLoopback }.mapNotNull { nif ->
            val addresses = nif.inetAddresses.toList()
            val ipv4 = addresses.filterIsInstance<Inet4Address>().mapNotNull { it.hostAddress }
            val ipv6 = addresses.filterIsInstance<Inet6Address>()
                .filter { !it.isLinkLocalAddress }
                .mapNotNull { it.hostAddress?.substringBefore('%') }
            if (ipv4.isEmpty() && ipv6.isEmpty()) return@mapNotNull null
            mapOf(
                "name" to nif.name,
                "ipv4" to ipv4,
                "ipv6" to ipv6,
                "pointToPoint" to nif.isPointToPoint,
            )
        }
    }

    /**
     * Browses one service type for `timeoutMs`, resolving the services one at a time
     * (NsdManager rejects concurrent resolves before Android 14). Main thread only.
     */
    private class Discovery(
        context: Context,
        private val serviceType: String,
        private val timeoutMs: Long,
        private val promise: WhiskerPromise,
    ) {
        private val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
        private val main = Handler(Looper.getMainLooper())
        private val queue = ArrayDeque<NsdServiceInfo>()
        private val seen = mutableSetOf<String>()
        private val results = mutableListOf<Map<String, Any?>>()
        private var resolving = false
        private var finished = false
        private var discovering = false

        private val listener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(type: String) {
                main.post { discovering = true }
            }

            override fun onServiceFound(info: NsdServiceInfo) {
                main.post {
                    if (finished || !seen.add(info.serviceName)) return@post
                    queue.addLast(info)
                    resolveNext()
                }
            }

            override fun onServiceLost(info: NsdServiceInfo) {}

            override fun onDiscoveryStopped(type: String) {}

            override fun onStartDiscoveryFailed(type: String, errorCode: Int) {
                main.post {
                    if (finished) return@post
                    finished = true
                    Bridge.reject(promise, "discovery failed to start ($errorCode)")
                }
            }

            override fun onStopDiscoveryFailed(type: String, errorCode: Int) {}
        }

        fun start() {
            try {
                nsd.discoverServices(serviceType, NsdManager.PROTOCOL_DNS_SD, listener)
            } catch (error: Throwable) {
                finished = true
                Bridge.reject(promise, error)
                return
            }
            main.postDelayed({ finish() }, timeoutMs)
        }

        private fun resolveNext() {
            if (finished || resolving) return
            val info = queue.removeFirstOrNull() ?: return
            resolving = true
            @Suppress("DEPRECATION")
            nsd.resolveService(info, object : NsdManager.ResolveListener {
                override fun onResolveFailed(serviceInfo: NsdServiceInfo, errorCode: Int) {
                    main.post {
                        resolving = false
                        resolveNext()
                    }
                }

                override fun onServiceResolved(serviceInfo: NsdServiceInfo) {
                    main.post {
                        resolving = false
                        if (!finished) results.add(describe(serviceInfo))
                        resolveNext()
                    }
                }
            })
        }

        private fun finish() {
            if (finished) return
            finished = true
            if (discovering) {
                try {
                    nsd.stopServiceDiscovery(listener)
                } catch (_: Throwable) {
                    // Already stopped.
                }
            }
            Bridge.resolve(promise, results.toList())
        }

        private fun describe(info: NsdServiceInfo): Map<String, Any?> {
            val addresses = if (Build.VERSION.SDK_INT >= 34) {
                info.hostAddresses.mapNotNull { it.hostAddress?.substringBefore('%') }
            } else {
                @Suppress("DEPRECATION")
                listOfNotNull(info.host?.hostAddress?.substringBefore('%'))
            }
            return mapOf(
                "name" to info.serviceName,
                "serviceType" to info.serviceType,
                "addresses" to addresses,
                "port" to info.port,
            )
        }
    }
}
