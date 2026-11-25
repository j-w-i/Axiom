import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../patterns/model_name.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/prefs.dart';
import '../ui/tokens.dart';
import 'offers.dart';
import 'start.dart';

enum _How { favourite, offered, hidden }

/// Which models are offered, over all accounts and programs: those that come first, and those that are
/// not offered at all.
class ModelsPart extends StatefulWidget {
  const ModelsPart(this.store, {super.key});
  final HostStore store;

  /// More than this are looked for.
  static const most = 25;
  @override
  State<ModelsPart> createState() => _ModelsPartState();
}

class _ModelsPartState extends State<ModelsPart> {
  final _words = TextEditingController();
  HostStore get s => widget.store;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _words.addListener(() => setState(() {}));
    unawaited(s.loadLists().catchError((Object _) {}));
  }

  @override
  void dispose() {
    _words.dispose();
    super.dispose();
  }

  /// Asks every account. What cannot be asked stays as it was.
  Future<void> _refresh() async {
    setState(() => _busy = true);
    for (final c in s.own) {
      try {
        await s.models(c.id);
      } on Object {
        // Its last list is still there.
      }
    }
    if (mounted) setState(() => _busy = false);
  }

  static String _day(int at) {
    final d = DateTime.fromMillisecondsSinceEpoch(at);
    String two(int n) => '$n'.padLeft(2, '0');
    return '${d.year}-${two(d.month)}-${two(d.day)}';
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: s,
    builder: (context, _) {
      final t = Ax.of(context), c = t.colors;
      final p = PrefsScope.of(context), all = PrefsScope.maybeOf(context);
      final words = _words.text.toLowerCase().split(' ').where((w) => w.isNotEmpty).toList();
      final offers = [...arranged(offersOf(s), p.copyWith(hidden: const []))];
      final fit = [
        for (final o in offers)
          if (words.every(
            '${modelName(o.model.id, o.model.name)} ${o.model.id} ${o.of}'.toLowerCase().contains,
          ))
            o,
      ];
      final shown = fit.take(ModelsPart.most).toList();
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Semantics(header: true, child: Text('Models', style: t.type.title)),
              ),
              AxButton(
                label: _busy ? 'Refreshing' : 'Refresh',
                onPressed: _busy || s.own.isEmpty ? null : () => unawaited(_refresh()),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            'What is offered where a model is chosen. A program tells its models when it has been '
            'started. A name can always be written.',
            style: t.type.label.ink(c.ink3),
          ),
          const SizedBox(height: 12),
          if (offers.length > ModelsPart.most) ...[
            AxField(
              label: 'Find one of ${offers.length}',
              placeholder: 'Find one of ${offers.length}',
              controller: _words,
            ),
            const SizedBox(height: 12),
          ],
          if (shown.isNotEmpty)
            Card([
              for (final o in shown)
                Row(
                  key: ValueKey(o.key),
                  children: [
                    if (markOf(o.provider, o.model.id, o.harness) case final m?)
                      AxServiceMark(m, size: 16)
                    else
                      const SizedBox(width: 16),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            oneLine(modelName(o.model.id, o.model.name), 80),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: t.type.uiMedium.ink(p.hidden.contains(o.key) ? c.ink3 : c.ink1),
                          ),
                          Text(
                            [
                              oneLine(o.of, 40),
                              oneLine(o.model.id, 60),
                              if (o.model.context case final n?) contextName(n),
                              switch (o.at) {
                                null => null,
                                0 => 'as documented',
                                final at => 'listed ${_day(at)}',
                              },
                            ].nonNulls.join(' · '),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: t.type.small.ink(c.ink3),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(width: 10),
                    AxSelect<_How>(
                      label: oneLine(modelName(o.model.id, o.model.name), 60),
                      named: false,
                      items: const [
                        AxMenuItem(_How.favourite, 'Favourite', detail: 'comes first'),
                        AxMenuItem(_How.offered, 'Offered'),
                        AxMenuItem(_How.hidden, 'Hidden', detail: 'not offered'),
                      ],
                      value: p.favourites.contains(o.key)
                          ? _How.favourite
                          : p.hidden.contains(o.key)
                          ? _How.hidden
                          : _How.offered,
                      onChanged: (v) => all?.value = p.offering(
                        o.key,
                        favourite: v == _How.favourite,
                        hide: v == _How.hidden,
                      ),
                    ),
                  ],
                ),
            ])
          else
            Text(offers.isEmpty ? 'None is known yet.' : 'None fits', style: t.type.ui.ink(c.ink3)),
          if (fit.length > shown.length)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text('${fit.length - shown.length} more', style: t.type.small.ink(c.ink3)),
            ),
        ],
      );
    },
  );
}
