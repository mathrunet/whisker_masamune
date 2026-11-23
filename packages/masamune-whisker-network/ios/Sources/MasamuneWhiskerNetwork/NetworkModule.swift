// Interface addresses (getifaddrs). DNS-SD discovery is not supported on iOS yet.

import Foundation
import WhiskerModule

@WhiskerModule
public final class NetworkModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Network")

            // interfaces() -> [{ name, ipv4, ipv6, pointToPoint }]
            Function("interfaces") { _ in
                Bridge.ok(Self.interfaces())
            }

            // discoverServices({ serviceType, timeoutMs }) — browsing needs NSBonjourServices and the
            // local network permission, which this module does not declare yet.
            AsyncFunction("discoverServices") { _, promise in
                Bridge.reject(promise, "discoverServices is not supported on iOS")
            }
        }
    }

    private static func interfaces() -> [[String: Any]] {
        var head: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&head) == 0, let first = head else { return [] }
        defer { freeifaddrs(head) }

        var order: [String] = []
        var entries: [String: (ipv4: [String], ipv6: [String], p2p: Bool)] = [:]
        var cursor: UnsafeMutablePointer<ifaddrs>? = first
        while let current = cursor {
            defer { cursor = current.pointee.ifa_next }
            let flags = Int32(current.pointee.ifa_flags)
            guard flags & IFF_UP != 0, flags & IFF_LOOPBACK == 0,
                  let sockaddr = current.pointee.ifa_addr else { continue }
            let family = sockaddr.pointee.sa_family
            guard family == UInt8(AF_INET) || family == UInt8(AF_INET6) else { continue }
            var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
            let length = socklen_t(family == UInt8(AF_INET) ? MemoryLayout<sockaddr_in>.size : MemoryLayout<sockaddr_in6>.size)
            guard getnameinfo(sockaddr, length, &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST) == 0 else { continue }
            let address = String(cString: host).components(separatedBy: "%")[0]
            let name = String(cString: current.pointee.ifa_name)
            if entries[name] == nil {
                order.append(name)
                entries[name] = ([], [], flags & IFF_POINTOPOINT != 0)
            }
            if family == UInt8(AF_INET) {
                entries[name]?.ipv4.append(address)
            } else if !address.lowercased().hasPrefix("fe80") {
                entries[name]?.ipv6.append(address)
            }
        }
        return order.compactMap { name in
            guard let entry = entries[name], !(entry.ipv4.isEmpty && entry.ipv6.isEmpty) else { return nil }
            return ["name": name, "ipv4": entry.ipv4, "ipv6": entry.ipv6, "pointToPoint": entry.p2p]
        }
    }
}
