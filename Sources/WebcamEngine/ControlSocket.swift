import Foundation

public enum Paths {
    public static var supportDirectory: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/canon-open-webcam")
    }

    public static var controlSocket: URL { supportDirectory.appendingPathComponent("control.sock") }
    public static var options: URL { supportDirectory.appendingPathComponent("options.json") }
    public static var installedBinary: URL { supportDirectory.appendingPathComponent("canon-webcam") }
    public static var commandLink: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".local/bin/canon-webcam")
    }
}

/// Listens on a Unix socket so `canon-webcam set iso 400` can reach the running engine.
public final class ControlServer {
    private let path: String
    private let handle: (ControlRequest) -> ControlResponse
    private var listener: Int32 = -1

    public init(path: URL = Paths.controlSocket, handle: @escaping (ControlRequest) -> ControlResponse) {
        self.path = path.path
        self.handle = handle
    }

    deinit {
        if listener >= 0 { close(listener) }
        unlink(path)
    }

    public func start() throws {
        try FileManager.default.createDirectory(atPath: (path as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
        // A socket file left over from a crash would make bind fail
        unlink(path)
        listener = socket(AF_UNIX, SOCK_STREAM, 0)
        guard listener >= 0 else { throw SocketError.failed("socket") }
        let bound = withSocketAddress(path) { bind(listener, $0, $1) }
        guard bound == 0, listen(listener, 8) == 0 else { throw SocketError.failed("bind \(path)") }
        // Only this user gets to boss the camera around
        chmod(path, 0o600)

        let thread = Thread { [weak self] in self?.acceptForever() }
        thread.name = "control socket"
        thread.start()
    }

    private func acceptForever() {
        while true {
            let client = accept(listener, nil, nil)
            guard client >= 0 else {
                if errno == EINTR { continue }
                return
            }
            serve(client)
            close(client)
        }
    }

    private func serve(_ client: Int32) {
        guard let line = readLine(from: client) else { return }
        let response: ControlResponse
        if let request = try? JSONDecoder().decode(ControlRequest.self, from: line) {
            response = handle(request)
        } else {
            response = .failure("couldn't read that request")
        }
        guard var reply = try? JSONEncoder().encode(response) else { return }
        reply.append(UInt8(ascii: "\n"))
        reply.withUnsafeBytes { _ = write(client, $0.baseAddress, $0.count) }
    }
}

public enum ControlClient {
    public static func send(_ request: ControlRequest, to path: URL = Paths.controlSocket) throws -> ControlResponse {
        let connection = socket(AF_UNIX, SOCK_STREAM, 0)
        guard connection >= 0 else { throw SocketError.failed("socket") }
        defer { close(connection) }
        let connected = withSocketAddress(path.path) { connect(connection, $0, $1) }
        guard connected == 0 else { throw SocketError.engineNotRunning }

        var line = try JSONEncoder().encode(request)
        line.append(UInt8(ascii: "\n"))
        line.withUnsafeBytes { _ = write(connection, $0.baseAddress, $0.count) }
        guard let reply = readLine(from: connection) else { throw SocketError.failed("no reply from the engine") }
        return try JSONDecoder().decode(ControlResponse.self, from: reply)
    }
}

public enum SocketError: Error, CustomStringConvertible {
    case failed(String)
    case engineNotRunning

    public var description: String {
        switch self {
        case .failed(let what): return "control socket trouble (\(what))"
        case .engineNotRunning: return "canon-webcam isn't running. Start it with `canon-webcam start` or `canon-webcam install`."
        }
    }
}

private func readLine(from descriptor: Int32) -> Data? {
    var collected = Data()
    var chunk = [UInt8](repeating: 0, count: 4096)
    while true {
        let count = read(descriptor, &chunk, chunk.count)
        guard count > 0 else { return collected.isEmpty ? nil : collected }
        collected.append(contentsOf: chunk[0..<count])
        if let newline = collected.firstIndex(of: UInt8(ascii: "\n")) {
            return collected.prefix(upTo: newline)
        }
    }
}

private func withSocketAddress<T>(_ path: String, _ body: (UnsafePointer<sockaddr>, socklen_t) -> T) -> T {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let capacity = MemoryLayout.size(ofValue: address.sun_path)
    withUnsafeMutableBytes(of: &address.sun_path) { raw in
        let bytes = Array(path.utf8.prefix(capacity - 1)) + [0]
        raw.copyBytes(from: bytes)
    }
    return withUnsafePointer(to: &address) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { body($0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
    }
}
