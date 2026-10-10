import AVFoundation
import SwiftUI
import WebcamEngine

/// A live look through OBS Virtual Camera, the same thing Zoom or FaceTime would see.
struct CameraPreview: NSViewRepresentable {
    var onClose: () -> Void

    func makeNSView(context: Context) -> PreviewView {
        let view = PreviewView()
        view.start()
        return view
    }

    func updateNSView(_ view: PreviewView, context: Context) {}

    static func dismantleNSView(_ view: PreviewView, coordinator: Coordinator) {
        view.stop()
        coordinator.onClose()
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onClose: onClose)
    }

    final class Coordinator {
        let onClose: () -> Void

        init(onClose: @escaping () -> Void) {
            self.onClose = onClose
        }
    }
}

final class PreviewView: NSView {
    private let session = AVCaptureSession()
    private let sessionQueue = DispatchQueue(label: "camera preview")

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
        let preview = AVCaptureVideoPreviewLayer(session: session)
        preview.videoGravity = .resizeAspectFill
        preview.autoresizingMask = [.layerWidthSizable, .layerHeightSizable]
        layer?.addSublayer(preview)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func layout() {
        super.layout()
        layer?.sublayers?.first?.frame = bounds
    }

    func start() {
        AVCaptureDevice.requestAccess(for: .video) { [weak self] allowed in
            guard allowed else { return }
            self?.sessionQueue.async { self?.connectAndRun() }
        }
    }

    func stop() {
        sessionQueue.async { [session] in session.stopRunning() }
    }

    private func connectAndRun() {
        let devices = AVCaptureDevice.DiscoverySession(deviceTypes: [.external], mediaType: .video, position: .unspecified).devices
        guard let camera = devices.first(where: { $0.localizedName == VirtualCamera.deviceName }),
              let input = try? AVCaptureDeviceInput(device: camera),
              session.canAddInput(input)
        else { return }
        session.addInput(input)
        session.startRunning()
    }
}
