import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow, NSDraggingDestination {
  private var drops: FlutterMethodChannel?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    self.contentViewController = flutterViewController
    self.setContentSize(NSSize(width: 1280, height: 820))
    self.contentMinSize = NSSize(width: 760, height: 520)
    self.center()

    // The header is ours; the window's own buttons sit on it.
    self.titleVisibility = .hidden
    self.titlebarAppearsTransparent = true
    self.styleMask.insert(.fullSizeContentView)
    self.isMovableByWindowBackground = false
    // Nothing is in it: with it the system sets its buttons 26 from the top, on the header's line.
    setBar()
    let told = NotificationCenter.default
    told.addObserver(
      forName: NSWindow.willEnterFullScreenNotification, object: self, queue: .main
    ) { [weak self] _ in self?.toolbar = nil }
    told.addObserver(
      forName: NSWindow.didExitFullScreenNotification, object: self, queue: .main
    ) { [weak self] _ in self?.setBar() }
    self.setFrameAutosaveName("Axiom")

    RegisterGeneratedPlugins(registry: flutterViewController)
    FlutterMethodChannel(
      name: "axiom/window", binaryMessenger: flutterViewController.engine.binaryMessenger
    ).setMethodCallHandler { [weak self] call, result in
      switch call.method {
      case "move":
        if let e = NSApp.currentEvent, e.type == .leftMouseDragged || e.type == .leftMouseDown {
          self?.performDrag(with: e)
        }
        result(nil)
      case "zoom":
        // What the person has chosen for all windows.
        switch UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick") {
        case "Minimize": self?.performMiniaturize(nil)
        case "None": break
        default: self?.performZoom(nil)
        }
        result(nil)
      default: result(FlutterMethodNotImplemented)
      }
    }
    drops = FlutterMethodChannel(
      name: "axiom/drop", binaryMessenger: flutterViewController.engine.binaryMessenger)
    // Files that are there. Nothing that would have to be written somewhere first.
    registerForDraggedTypes([.fileURL])
    super.awakeFromNib()
  }

  /// What is dragged, two at most: of more than one, only that there are more is of interest.
  private func dragged(_ sender: NSDraggingInfo) -> [URL] {
    let only: [NSPasteboard.ReadingOptionKey: Any] = [.urlReadingFileURLsOnly: true]
    let all = sender.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: only)
    return (all ?? []).prefix(2).compactMap { ($0 as? NSURL)?.filePathURL }
  }

  /// Never to be moved: whoever drags it would remove it.
  private func taken(_ sender: NSDraggingInfo) -> NSDragOperation {
    let may = sender.draggingSourceOperationMask
    return [.copy, .link, .generic].first { may.contains($0) } ?? []
  }

  func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
    let all = dragged(sender)
    // As it is named, without looking at the disk.
    if all.count == 1, all[0].hasDirectoryPath {
      drops?.invokeMethod("over", arguments: all[0].path)
    }
    return all.isEmpty ? [] : taken(sender)
  }

  func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation { taken(sender) }

  func draggingExited(_ sender: NSDraggingInfo?) { drops?.invokeMethod("out", arguments: nil) }

  func draggingEnded(_ sender: NSDraggingInfo) { drops?.invokeMethod("out", arguments: nil) }

  func prepareForDragOperation(_ sender: NSDraggingInfo) -> Bool { true }

  func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
    let all = dragged(sender)
    drops?.invokeMethod("drop", arguments: all.map(\.path))
    return !all.isEmpty
  }

  private func setBar() {
    toolbar = NSToolbar(identifier: "header")
    toolbarStyle = .unified
  }
}
