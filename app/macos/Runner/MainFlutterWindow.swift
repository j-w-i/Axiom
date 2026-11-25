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
    self.setFrameAutosaveName("Axiom")

    RegisterGeneratedPlugins(registry: flutterViewController)
    super.awakeFromNib()
  }
}
