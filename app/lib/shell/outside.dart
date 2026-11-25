import 'package:file_selector/file_selector.dart';
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

/// How a person is shown the system's own window for a folder. Tests take its place: they open none.
@visibleForTesting
Future<String?> Function(String? at) folders = (at) =>
    getDirectoryPath(initialDirectory: at, confirmButtonText: 'Choose');

/// The folder a person chose, or null. Only because they pressed something that says so.
Future<String?> chooseFolder(String at) async {
  try {
    return await folders(at.isEmpty ? null : at);
  } on Object {
    return null;
  }
}
