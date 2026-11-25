import 'dart:async';
import 'dart:io';

import 'package:flutter/widgets.dart';

import 'app.dart';
import 'host/host_store.dart';
import 'shell/prefs.dart';
import 'shell/shell.dart';
import 'ui/surface.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final paths = HostPaths.discover();
  if (paths == null) {
    stderr.writeln('axiom: no data directory: set AXIOM_DATA');
    exit(1);
  }
  final store = HostStore(
    paths: paths,
    binary: Platform.environment['AXIOM_HOST'] ?? HostStore.findBinary(),
  );
  unawaited(store.start());
  await Shades.load();
  runApp(
    AxiomApp(
      prefs: PrefsFile('${paths.data}/window.json').open(),
      home: Shell(store, titleInset: Platform.isMacOS ? 72 : 0),
    ),
  );
}
