import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    self.contentViewController = flutterViewController
    // The first size, as on Linux (1180x760 logical points); AXIOM_WINDOW_SIZE=WxH sets it (the
    // frame benchmark and the end-to-end suites). The Xcode template's 800x600 left the editor a
    // few dozen points beside the side bar and the AI dock.
    var size = NSSize(width: 1180, height: 760)
    if let s = ProcessInfo.processInfo.environment["AXIOM_WINDOW_SIZE"] {
      let parts = s.split(separator: "x").compactMap { Double($0) }
      if parts.count == 2, parts[0] > 0, parts[1] > 0 {
        size = NSSize(width: parts[0], height: parts[1])
      }
    }
    self.setContentSize(size)
    self.contentMinSize = NSSize(width: 900, height: 560)
    self.center()

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
}
