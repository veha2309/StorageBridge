import Cocoa
import FlutterMacOS
import UniformTypeIdentifiers

class MainFlutterWindow: NSWindow, NSWindowDelegate {
  private var bridge: FlutterMethodChannel?
  private var statusItem: NSStatusItem?
  private let worker = DispatchQueue(label: "dev.storagebridge.engine")

  override func awakeFromNib() {
    let controller = FlutterViewController()
    let windowFrame = frame
    contentViewController = controller
    setFrame(windowFrame, display: true)
    RegisterGeneratedPlugins(registry: controller)
    bridge = FlutterMethodChannel(name: "dev.storagebridge/desktop", binaryMessenger: controller.engine.binaryMessenger)
    bridge?.setMethodCallHandler { [weak self] call, result in
      guard let self = self else { return }
      switch call.method {
      case "request":
        guard let payload = call.arguments as? String else {
          result(FlutterError(code: "bridge", message: "Expected JSON request", details: nil)); return
        }
        self.worker.async {
          do {
            let process = try self.engineProcess()
            process.arguments = ["--request", payload]
            let output = Pipe(), errors = Pipe()
            process.standardOutput = output
            process.standardError = errors
            try process.run()
            // Drain stdout before waiting so large framed status replies cannot deadlock.
            let data = output.fileHandleForReading.readDataToEndOfFile()
            let errorData = errors.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            guard process.terminationStatus == 0, let body = String(data: data, encoding: .utf8) else {
              throw NSError(domain: "StorageBridge", code: 1, userInfo: [NSLocalizedDescriptionKey: String(data: errorData, encoding: .utf8) ?? "Engine request failed"])
            }
            DispatchQueue.main.async { result(body) }
          } catch {
            DispatchQueue.main.async { result(FlutterError(code: "engine", message: error.localizedDescription, details: nil)) }
          }
        }
      case "launch":
        let demo = call.arguments as? Bool == true
        self.worker.async {
          do {
            let process = try self.engineProcess()
            process.arguments = ["--daemon"] + (demo ? ["--demo"] : [])
            let errors = Pipe()
            process.standardOutput = FileHandle.nullDevice
            process.standardError = errors
            try process.run()
            // Drain errors while the daemon runs so its pipe cannot fill and stall it.
            errors.fileHandleForReading.readabilityHandler = { handle in
              let data = handle.availableData
              if data.isEmpty { handle.readabilityHandler = nil }
              else { NSLog("StorageBridge engine: %@", String(decoding: data, as: UTF8.self)) }
            }
            DispatchQueue.main.async { result(nil) }
          } catch {
            DispatchQueue.main.async { result(FlutterError(code: "bridge", message: error.localizedDescription, details: nil)) }
          }
        }
      case "chooseSource":
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.beginSheetModal(for: self) { response in
          result(response == .OK ? panel.url?.path : nil)
        }
      case "chooseClient":
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.beginSheetModal(for: self) { response in
          guard response == .OK, let url = panel.url else { result(nil); return }
          do {
            let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
            guard size <= 65536 else { throw NSError(domain: "StorageBridge", code: 2, userInfo: [NSLocalizedDescriptionKey: "OAuth client JSON must be smaller than 64 KiB"]) }
            result(try String(contentsOf: url, encoding: .utf8))
          } catch { result(FlutterError(code: "bridge", message: error.localizedDescription, details: nil)) }
        }
      case "openWinfsp":
        NSWorkspace.shared.open(URL(string: "https://macfuse.github.io/")!)
        result(nil)
      case "openDrive":
        let directory = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("StorageBridge", isDirectory: true)
        let finder = URL(fileURLWithPath: "/System/Library/CoreServices/Finder.app", isDirectory: true)
        NSWorkspace.shared.open([directory], withApplicationAt: finder, configuration: NSWorkspace.OpenConfiguration()) { _, error in
          DispatchQueue.main.async {
            if let error = error {
              result(FlutterError(code: "finder", message: "Cannot open the mounted drive: \(error.localizedDescription)", details: nil))
            } else { result(nil) }
          }
        }
      default: result(FlutterMethodNotImplemented)
      }
    }
    delegate = self
    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    statusItem?.button?.image = NSImage(systemSymbolName: "externaldrive.connected.to.line.below", accessibilityDescription: "Storage Bridge")
    let menu = NSMenu()
    let show = menu.addItem(withTitle: "Open Storage Bridge", action: #selector(showWindow), keyEquivalent: "")
    show.target = self
    menu.addItem(.separator())
    menu.addItem(withTitle: "Quit Storage Bridge", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    statusItem?.menu = menu
    super.awakeFromNib()
  }
  private func engineProcess() throws -> Process {
    let url = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/storage-bridge")
    guard FileManager.default.isExecutableFile(atPath: url.path) else {
      throw NSError(domain: "StorageBridge", code: 3, userInfo: [NSLocalizedDescriptionKey: "Engine is missing. Build using scripts/build-macos.sh."])
    }
    let process = Process()
    process.executableURL = url
    return process
  }
  @objc private func showWindow() {
    makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
  }
  func windowShouldClose(_ sender: NSWindow) -> Bool { orderOut(nil); return false }
}
