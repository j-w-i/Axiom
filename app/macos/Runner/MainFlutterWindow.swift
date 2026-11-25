import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
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
    super.awakeFromNib()
  }

  private func setBar() {
    toolbar = NSToolbar(identifier: "header")
    toolbarStyle = .unified
  }
}
