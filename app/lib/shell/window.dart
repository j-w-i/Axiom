import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

const _channel = MethodChannel('axiom/window');

/// What the system is asked to do with the window. Tests put their own in its place.
Future<void> Function(String what) window = (what) async {
  try {
    await _channel.invokeMethod<void>(what);
  } on PlatformException catch (e) {
    debugPrint('window: ${e.message}');
  } on MissingPluginException {
    // It has a bar of its own.
  }
};

/// Where the window has no bar of its own it is moved by this, and a double click does what the system
/// is set to. What can be pressed in it is pressed as ever.
class MoveArea extends StatefulWidget {
  const MoveArea({required this.child, super.key});
  final Widget child;
  @override
  State<MoveArea> createState() => _MoveAreaState();
}

class _MoveAreaState extends State<MoveArea> {
  Timer? _once;

  @override
  void dispose() {
    _once?.cancel();
    super.dispose();
  }

  void _tapped() {
    if (_once?.isActive ?? false) {
      _once!.cancel();
      unawaited(window('zoom'));
    } else {
      _once = Timer(kDoubleTapTimeout, () {});
    }
  }

  @override
  Widget build(BuildContext context) => defaultTargetPlatform != TargetPlatform.macOS
      ? widget.child
      : GestureDetector(
          behavior: HitTestBehavior.opaque,
          excludeFromSemantics: true,
          supportedDevices: const {PointerDeviceKind.mouse, PointerDeviceKind.trackpad},
          // Only presses that nothing in it has taken: one that has is not held back.
          onTapUp: (_) => _tapped(),
          onPanStart: (_) => unawaited(window('move')),
          child: widget.child,
        );
}
