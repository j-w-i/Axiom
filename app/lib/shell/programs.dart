import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/host_store.dart';
import '../host/proto.dart';
import '../transcript/safe_text.dart';
import '../ui/controls.dart';
import '../ui/icons.dart';
import '../ui/marks.dart';
import '../ui/overlay.dart';
import '../ui/tokens.dart';
import 'outside.dart';
import 'start.dart';

/// Programs that can be agents: where each is and how it is started, and those its person added
/// (decision 0016).
class ProgramsPart extends StatefulWidget {
  const ProgramsPart(this.store, {super.key});
  final HostStore store;
  @override
  State<ProgramsPart> createState() => _ProgramsPartState();
}

/// What is being set: a program there is, or one to be added.
class _Draft {
  _Draft(this.of, ProgramInfo? p, {this.account}) {
    name.text = p?.name ?? '';
    path.text = p?.path ?? '';
    args.text = (p?.args ?? const []).join('\n');
    vars = [for (final v in p?.vars ?? const <VarInfo>[]) _Var.of(v)];
    said = p != null;
  }

  /// Null: another.
  final HarnessInfo? of;
  final String? account;
  late final bool said;
  bool get own => of?.own ?? true;
  final name = TextEditingController(), path = TextEditingController();
  final args = TextEditingController();
  late List<_Var> vars;

  void dispose() {
    for (final c in [name, path, args]) {
      c.dispose();
    }
    for (final v in vars) {
      v.dispose();
    }
  }
}

class _Var {
  _Var() : kept = false, secret = false;
  _Var.of(VarInfo v) : kept = v.value == null, secret = v.value == null {
    name.text = v.name;
    value.text = v.value ?? '';
  }

  /// There is a secret already, which is not shown.
  final bool kept;
  bool secret;
  final name = TextEditingController(), value = TextEditingController();

  /// A name that says it is a secret. The host judges so too, whatever is said here.
  bool get must => RegExp(
    r'(^|_)(KEY|APIKEY|SECRET|TOKEN|PAT|PASS[A-Z]*)(_|$)',
    caseSensitive: false,
  ).hasMatch(name.text);

  void dispose() {
    name.dispose();
    value.dispose();
  }
}

class _ProgramsPartState extends State<ProgramsPart> {
  _Draft? _draft;
  String? _problem, _leaving;
  bool _busy = false;
  HostStore get s => widget.store;

  @override
  void initState() {
    super.initState();
    unawaited(s.loadPrograms().catchError((Object _) {}));
  }

  @override
  void dispose() {
    _draft?.dispose();
    super.dispose();
  }

  Future<bool> _do(Future<void> Function() f) async {
    setState(() {
      _busy = true;
      _problem = null;
    });
    try {
      await f();
      return true;
    } on Object catch (e) {
      if (mounted) setState(() => _problem = wordsOf(e));
      return false;
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  ProgramInfo? _said(String harness, [String? account]) =>
      s.programs.where((p) => p.harness == harness && p.account == account).firstOrNull;

  void _edit(HarnessInfo? h, {String? account}) => setState(() {
    _draft?.dispose();
    _draft = _Draft(h, h == null ? null : _said(h.id, account), account: account);
    _problem = _leaving = null;
  });

  void _close() => setState(() {
    _draft?.dispose();
    _draft = null;
    _problem = null;
  });

  Future<void> _save(_Draft d) async {
    final done = await _do(
      () => s.setProgram(
        harness: d.of?.id,
        account: d.account,
        name: d.own ? d.name.text.trim() : null,
        path: d.path.text.trim().isEmpty ? null : wholePath(d.path.text.trim()),
        args: [
          for (final a in d.args.text.split('\n'))
            if (a.trim().isNotEmpty) a.trim(),
        ],
        vars: [
          for (final v in d.vars)
            if (v.name.text.trim().isNotEmpty)
              VarSet(
                v.name.text.trim(),
                // Left empty, what is kept stays.
                value: v.kept && v.value.text.isEmpty ? null : v.value.text,
                secret: v.secret || v.must,
              ),
        ],
      ),
    );
    if (done && mounted && _draft == d) _close();
  }

  Future<void> _remove(HarnessInfo h, {String? account}) async {
    final done = await _do(() => s.removeProgram(h.id, account: account));
    if (!mounted || !done) return;
    _leaving = null;
    if (_draft != null) _close();
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: s,
    builder: (context, _) {
      final t = Ax.of(context), c = t.colors;
      bool shown(HarnessInfo h) =>
          h.program != null || h.own || s.programs.any((p) => p.harness == h.id);
      final here = s.harnesses.where(shown).toList();
      final rest = [
        for (final h in s.harnesses)
          if (!shown(h)) h,
      ];
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Semantics(header: true, child: Text('Programs', style: t.type.title)),
          const SizedBox(height: 4),
          Text(
            'Agents that are programs on this computer. Each start is asked about.',
            style: t.type.label.ink(c.ink3),
          ),
          const SizedBox(height: 12),
          if (here.isNotEmpty) ...[
            Card([for (final h in here) _line(context, h)]),
            const SizedBox(height: 12),
          ],
          if (_draft case final d? when !here.any((h) => h.id == d.of?.id))
            _Form(d, this, key: ObjectKey(d))
          else
            Align(
              alignment: Alignment.centerLeft,
              child: AxSelect<String>(
                label: 'Add a program',
                named: false,
                face: 'Add a program',
                items: [
                  const AxMenuItem('', 'One of your own', detail: 'any that speaks ACP'),
                  for (final h in rest)
                    AxMenuItem(
                      h.id,
                      oneLine(h.name, 40),
                      detail: 'say where it is',
                      mark: markOf(null, null, h.id),
                    ),
                ],
                value: '-',
                onChanged: (v) => _edit(s.harnesses.where((h) => h.id == v).firstOrNull),
              ),
            ),
        ],
      );
    },
  );

  Widget _line(BuildContext context, HarnessInfo h) {
    final t = Ax.of(context), c = t.colors;
    final said = _said(h.id);
    final parts = [
      if (h.own) 'your own',
      if (h.program case final p?) oneLine(literal(p), 80),
      if (said != null && said.vars.isNotEmpty)
        said.vars.length == 1 ? '1 variable' : '${said.vars.length} variables',
      if (s.programs.any((p) => p.harness == h.id && p.account != null)) 'more for an account',
    ];
    final d = _draft, open = d != null && d.of?.id == h.id;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            if (markOf(null, null, h.id) case final m?)
              AxServiceMark(m, size: 16)
            else
              AxGlyph(AxIcon.terminal, color: c.ink3),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    oneLine(h.name, 80),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.uiMedium,
                  ),
                  Text(
                    parts.join(' · '),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: t.type.small.ink(c.ink3),
                  ),
                  if (h.problem case final p?)
                    Text(visible(p), style: t.type.small.ink(c.faultText)),
                ],
              ),
            ),
            const SizedBox(width: 10),
            if (_leaving == h.id) ...[
              AxButton(
                label: 'Keep',
                autofocus: true,
                onPressed: () => setState(() => _leaving = null),
              ),
              const SizedBox(width: 8),
              AxButton(
                label: 'Remove ${oneLine(h.name, 30)}',
                onPressed: _busy ? null : () => unawaited(_remove(h)),
              ),
            ] else ...[
              if (h.own) ...[
                AxButton(
                  label: 'Remove',
                  onPressed: _busy ? null : () => setState(() => _leaving = h.id),
                ),
                const SizedBox(width: 8),
              ],
              AxButton(label: 'Edit', onPressed: _busy ? null : () => open ? _close() : _edit(h)),
            ],
          ],
        ),
        if (open) ...[const SizedBox(height: 14), _Form(d, this, key: ObjectKey(d))],
        if (!open && _leaving == h.id && _problem != null) _Problem(_problem!),
      ],
    );
  }
}

class _Problem extends StatelessWidget {
  const _Problem(this.words);
  final String words;
  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    return Semantics(
      liveRegion: true,
      child: Padding(
        padding: const EdgeInsets.only(top: 8, bottom: 4),
        child: Text(visible(words), style: t.type.ui.ink(c.faultText)),
      ),
    );
  }
}

/// How one is started.
class _Form extends StatefulWidget {
  const _Form(this.d, this.page, {super.key});
  final _Draft d;
  final _ProgramsPartState page;
  @override
  State<_Form> createState() => _FormState();
}

class _FormState extends State<_Form> {
  _Draft get d => widget.d;
  _ProgramsPartState get page => widget.page;

  @override
  void initState() {
    super.initState();
    for (final c in [d.name, d.path]) {
      c.addListener(_now);
    }
  }

  void _now() => mounted ? setState(() {}) : null;

  Future<void> _look() async {
    final f = await chooseFile(wholePath(d.path.text.trim()));
    if (f != null && mounted) d.path.text = f;
  }

  @override
  Widget build(BuildContext context) {
    final t = Ax.of(context), c = t.colors;
    final accounts = d.of == null ? const <Connection>[] : page.s.accountsOf(d.of!.id);
    final ready =
        !page._busy && (!d.own || d.name.text.trim().isNotEmpty && d.path.text.trim().isNotEmpty);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (d.own)
          Padding(
            padding: const EdgeInsets.only(bottom: 14),
            child: Text(
              'It speaks the Agent Client Protocol. Nothing is known of its modes, so it can only ask '
              'first, and nothing but itself holds it back.',
              style: t.type.label.ink(c.ink3),
            ),
          ),
        if (accounts.isNotEmpty)
          FormPart(
            null,
            child: Align(
              alignment: Alignment.centerLeft,
              child: AxSelect<String>(
                label: 'For',
                items: [
                  const AxMenuItem('', 'Every account'),
                  for (final a in accounts) AxMenuItem(a.id, oneLine(a.name, 60)),
                ],
                value: d.account ?? '',
                onChanged: (v) => page._edit(d.of, account: v.isEmpty ? null : v),
              ),
            ),
          ),
        if (d.own)
          FormPart(
            'Name',
            child: AxField(label: 'Program name', controller: d.name, sunken: d.of != null),
          ),
        FormPart(
          'Path',
          note: d.own ? null : 'Empty: it is looked for where programs usually are.',
          child: Row(
            children: [
              Expanded(
                child: AxField(
                  label: 'Path',
                  controller: d.path,
                  mono: true,
                  sunken: d.of != null,
                  placeholder: '/path/to/program',
                ),
              ),
              const SizedBox(width: 8),
              AxButton(label: 'Choose…', onPressed: () => unawaited(_look())),
            ],
          ),
        ),
        FormPart(
          'Arguments',
          note: 'One on each line. Other users of this computer can see them.',
          child: AxField(
            label: 'Arguments',
            controller: d.args,
            mono: true,
            lines: 3,
            sunken: d.of != null,
            placeholder: d.own ? 'acp' : '',
          ),
        ),
        FormPart(
          'Variables',
          note: 'A secret goes to the system\'s store and to this program, and is not shown again.',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final v in d.vars)
                Padding(
                  key: ObjectKey(v),
                  padding: const EdgeInsets.only(bottom: 8),
                  child: _VarLine(
                    v,
                    sunken: d.of != null,
                    onRemove: () => setState(() {
                      d.vars = d.vars.where((x) => x != v).toList();
                      WidgetsBinding.instance.addPostFrameCallback((_) => v.dispose());
                    }),
                  ),
                ),
              Align(
                alignment: Alignment.centerLeft,
                child: AxButton(
                  label: 'Add a variable',
                  icon: AxIcon.plus,
                  onPressed: () => setState(() => d.vars = [...d.vars, _Var()]),
                ),
              ),
            ],
          ),
        ),
        if (page._problem case final p?) _Problem(p),
        Wrap(
          alignment: WrapAlignment.end,
          spacing: 8,
          runSpacing: 8,
          children: [
            if (d.said && (!d.own || d.account != null))
              AxButton(
                label: 'Reset',
                onPressed: page._busy
                    ? null
                    : () => unawaited(page._remove(d.of!, account: d.account)),
              ),
            AxButton(label: 'Cancel', onPressed: page._close),
            AxButton(
              label: d.of == null ? 'Add program' : 'Save',
              tone: AxTone.primary,
              onPressed: ready ? () => unawaited(page._save(d)) : null,
            ),
          ],
        ),
      ],
    );
  }
}

class _VarLine extends StatefulWidget {
  const _VarLine(this.v, {required this.onRemove, required this.sunken});
  final _Var v;
  final VoidCallback onRemove;
  final bool sunken;
  @override
  State<_VarLine> createState() => _VarLineState();
}

class _VarLineState extends State<_VarLine> {
  @override
  void initState() {
    super.initState();
    widget.v.name.addListener(_now);
  }

  @override
  void dispose() {
    widget.v.name.removeListener(_now);
    super.dispose();
  }

  void _now() => setState(() {});

  @override
  Widget build(BuildContext context) {
    final v = widget.v, secret = v.secret || v.must;
    return Wrap(
      spacing: 8,
      runSpacing: 6,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        SizedBox(
          width: 220,
          child: AxField(
            label: 'Variable',
            controller: v.name,
            mono: true,
            sunken: widget.sunken,
            placeholder: 'NAME',
          ),
        ),
        SizedBox(
          width: 220,
          child: AxField(
            label: 'Value',
            controller: v.value,
            mono: true,
            sunken: widget.sunken,
            secret: secret,
            placeholder: v.kept ? 'kept, not shown' : '',
          ),
        ),
        AxCheck(
          label: 'Secret',
          value: secret,
          onChanged: v.must || v.kept ? null : (x) => setState(() => v.secret = x),
        ),
        AxButton.icon(AxIcon.close, label: 'Remove variable', onPressed: widget.onRemove),
      ],
    );
  }
}
