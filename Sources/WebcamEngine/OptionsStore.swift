import Foundation

/// Mirror and zoom live on the Mac, unlike ISO and friends which the camera remembers itself.
public struct OptionsStore {
    public let file: URL

    public init(file: URL = Paths.options) {
        self.file = file
    }

    public func load() -> FrameOptions {
        guard let data = try? Data(contentsOf: file),
              let options = try? JSONDecoder().decode(FrameOptions.self, from: data)
        else { return FrameOptions() }
        return options
    }

    public func save(_ options: FrameOptions) throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONEncoder().encode(options).write(to: file, options: .atomic)
    }
}
