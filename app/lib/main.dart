import 'dart:async';
import 'dart:io';

import 'package:flutter/widgets.dart';

import 'app.dart';
import 'host/host_store.dart';
import 'shell/shell.dart';

void main() {
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
  runApp(AxiomApp(home: Shell(store, titleInset: Platform.isMacOS ? 58 : 0)));
}
