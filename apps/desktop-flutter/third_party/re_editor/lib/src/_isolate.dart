part of re_editor;

typedef IsolateRunnable<Req, Res> = Res Function(Req req);
typedef IsolateCallback<Res> = void Function(Res res);

// Axiom patch (FL-PERF-003): worker isolates are shared by every editor, two per kind
// of task for the whole app, instead of new isolates for each editor (a tab switch
// spawned three). Each tasker keeps only its newest request; a request is handed to a
// worker only when one is free, newest tasker first, and a closed tasker's waiting
// request is dropped. Typing or switching tabs quickly never queues stale work.
class _IsolateLane {
  _IsolateLane(this.manager);

  static const int workers = 2;

  final Object manager;
  final List<_IsolateTasker<dynamic, dynamic>> _waiting = [];
  int _busy = 0;

  void schedule(_IsolateTasker<dynamic, dynamic> tasker) {
    _waiting.remove(tasker);
    _waiting.add(tasker);
    _pump();
  }

  void cancel(_IsolateTasker<dynamic, dynamic> tasker) {
    _waiting.remove(tasker);
  }

  void _pump() {
    while (_busy < workers && _waiting.isNotEmpty) {
      final Future<void>? job = _waiting.removeLast()._take();
      if (job == null) {
        continue;
      }
      _busy++;
      job.whenComplete(() {
        _busy--;
        _pump();
      });
    }
  }
}

class _IsolateTasker<Req, Res> {
  static final Map<String, _IsolateLane> _lanes = {};

  final String name;
  final _IsolateLane _lane;
  bool _closed = false;
  Req? _req;
  IsolateCallback<Res>? _callback;

  _IsolateTasker(this.name, IsolateRunnable<Req, Res> runnable)
      : _lane = _lanes[name] ??= _IsolateLane(IsolateManager<Res, Req>.create(
          runnable,
          concurrent: _IsolateLane.workers,
        ));

  void run(Req req, IsolateCallback<Res> callback) {
    if (_closed) {
      return;
    }
    _req = req;
    _callback = callback;
    _lane.schedule(this);
  }

  // The lane's turn: start this tasker's newest request, or null if there is none.
  Future<void>? _take() {
    final Req? req = _req;
    final IsolateCallback<Res>? callback = _callback;
    _req = null;
    _callback = null;
    if (_closed || req == null || callback == null) {
      return null;
    }
    final IsolateManager<Res, Req> manager = _lane.manager as IsolateManager<Res, Req>;
    return manager.compute(req).then((Res res) {
      if (!_closed) {
        callback(res);
      }
    }, onError: (Object _) {});
  }

  void close() {
    _closed = true;
    _req = null;
    _callback = null;
    _lane.cancel(this);
  }
}
