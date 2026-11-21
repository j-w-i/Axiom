// Opens an http(s) link in the system browser (the Tauri host's `open_external`): the one
// process the shell starts besides axiomd, to present a page the person asked for.

import 'dart:io';

/// Returns an error sentence, or '' when the browser was asked to open [url].
Future<String> openExternal(String url) async {
  final lower = url.toLowerCase();
  if (!(lower.startsWith('http://') || lower.startsWith('https://'))) {
    return 'Only http and https links are opened.';
  }
  final (program, args) = Platform.isMacOS
      ? ('open', [url])
      : Platform.isWindows
      ? ('rundll32', ['url.dll,FileProtocolHandler', url])
      : ('xdg-open', [url]);
  try {
    await Process.start(program, args, mode: ProcessStartMode.detached);
    return '';
  } on ProcessException catch (e) {
    return 'The browser could not be opened: ${e.message}';
  }
}
