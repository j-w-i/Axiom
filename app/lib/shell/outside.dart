import 'package:flutter/foundation.dart';
import 'package:url_launcher/url_launcher.dart';

/// Where a person sees what their plan has used.
const planUsage = 'https://chatgpt.com/settings/usage';

/// Only these, and only because a person pressed something that says so.
const _places = {'auth.openai.com', 'chatgpt.com'};

/// How a page is opened in the person's own browser. Tests take its place: they open nothing.
@visibleForTesting
Future<bool> Function(Uri) opens = (u) => launchUrl(u, mode: LaunchMode.externalApplication);

/// Whether it was opened.
Future<bool> browse(String address) async {
  final u = Uri.tryParse(address);
  if (u == null || u.scheme != 'https' || u.userInfo.isNotEmpty || u.hasPort && u.port != 443) {
    return false;
  }
  if (!_places.contains(u.host)) return false;
  try {
    return await opens(u);
  } on Object {
    return false;
  }
}
