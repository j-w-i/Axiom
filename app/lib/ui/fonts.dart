import 'package:flutter/painting.dart';

import 'tokens.dart';

/// Whether there is a family of this name: whether it is drawn in another way than one that certainly is
/// not there. Behind each stands a shipped one, of which neither is drawn as the other.
bool hasFamily(String name) {
  (double, double) drawn(String family, String behind) {
    final p = TextPainter(
      text: TextSpan(
        text: 'iiiWWW 0123 {}()',
        style: TextStyle(fontFamily: family, fontFamilyFallback: [behind], fontSize: 64),
      ),
      textDirection: TextDirection.ltr,
    )..layout();
    final size = (p.width, p.height);
    p.dispose();
    return size;
  }

  if (name.isEmpty) return false;
  return [
    AxType.words,
    AxType.literals,
  ].any((behind) => drawn(name, behind) != drawn('no family has this name', behind));
}
