import 'package:axiom/host/proto.dart';
import 'package:axiom/shell/settings.dart';
import 'package:axiom/ui/overlay.dart';
import 'package:axiom/ui/prefs.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

/// Who runs the models of accounts (decision 0023), in Settings.
void main() {
  const dsh = HarnessInfo(
    'dsh',
    'DeepSeek Harness',
    problem: 'dsh is not in any of the folders that are looked in',
    runsAccounts: true,
    chosen: true,
    install: 'npm install -g @deepseek-ai/dsh@0.2.2-rc.5',
  );
  const opencode = HarnessInfo(
    'opencode',
    'OpenCode',
    program: '/opt/homebrew/bin/opencode',
    runsAccounts: true,
  );
  const goose = HarnessInfo('goose', 'Goose', program: '/usr/local/bin/goose');

  Future<List<String>> page(WidgetTester t, List<HarnessInfo> all, {AxTheme? theme}) async {
    final prefs = ValueNotifier(const Prefs());
    addTearDown(prefs.dispose);
    final said = <String>[];
    await mount(
      t,
      PrefsScope(
        prefs,
        child: Builder(
          // The page stands on the window's ground, as in the app.
          builder: (context) => ColoredBox(
            color: Ax.of(context).colors.chassis.fill,
            child: SettingsPage(
              harnesses: all,
              onChoose: (id) => said.add('choose $id'),
              onLook: () => said.add('look'),
            ),
          ),
        ),
      ),
      size: const Size(800, 900),
      theme: theme ?? const AxTheme(),
    );
    return said;
  }

  testWidgets('the chosen one, not installed: how to install it', (t) async {
    final said = await page(t, const [dsh, opencode, goose]);
    await snap(t, 'settings-harness-missing');
    expect(find.text('DeepSeek Harness is not installed'), findsOneWidget);
    expect(find.text('npm install -g @deepseek-ai/dsh@0.2.2-rc.5'), findsOneWidget);
    // Only those that can run them are offered.
    await t.tap(find.text('DeepSeek Harness'));
    await t.pumpAndSettle();
    final menu = find.byType(AxMenu<String>);
    expect(find.descendant(of: menu, matching: find.text('Goose')), findsNothing);
    await t.tap(find.descendant(of: menu, matching: find.text('OpenCode')));
    await t.pumpAndSettle();
    await t.tap(find.text('Look again'));
    expect(said, ['choose opencode', 'look']);
  });

  testWidgets('the chosen one, installed', (t) async {
    await page(t, [
      const HarnessInfo(
        'dsh',
        'DeepSeek Harness',
        program: '/opt/homebrew/bin/dsh',
        runsAccounts: true,
        chosen: true,
      ),
      opencode,
    ]);
    await snap(t, 'settings-harness');
    expect(find.textContaining('is not installed'), findsNothing);
  });

  testWidgets('light', (t) async {
    await page(t, const [dsh, opencode], theme: const AxTheme(colors: AxColors.light));
    await snap(t, 'settings-harness-light');
  });
}
