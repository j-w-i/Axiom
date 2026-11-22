#include <stdio.h>
#include <unistd.h>

#include "my_application.h"

int main(int argc, char** argv) {
  int status;
  {
    g_autoptr(MyApplication) app = my_application_new();
    status = g_application_run(G_APPLICATION(app), argc, argv);
  }
  // End the process here, without exit()'s handlers (FL-PERF-003). The Dart UI
  // runs on its own thread, which Flutter's quit leaves running: exit() would
  // run the static destructors (Skia's text support among them) under it while
  // it may still be laying out text, a crash on quit. The app has finished its
  // own shutdown (the daemon is stopped) before it asks to quit.
  fflush(nullptr);
  _exit(status);
}
