// Install commands in the chat setup are copyable chips (AUD-110).

import 'package:axiom_desktop/chat/models.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('the daemon’s install hint splits into prose and the command', () {
    expect(
      InstallHint.split(
        'Not installed. Install it: npm install -g @agentclientprotocol/claude-agent-acp, '
        'then sign in by running claude once.',
      ),
      [
        (false, 'Not installed. Install it:'),
        (true, 'npm install -g @agentclientprotocol/claude-agent-acp'),
        (false, 'Then sign in by running claude once.'),
      ],
    );
  });
  test('prose without a command stays one part; a command at the end loses its full stop', () {
    expect(InstallHint.split('Not available on this computer.'), [
      (false, 'Not available on this computer.'),
    ]);
    expect(InstallHint.split('Install it: npm i -g foo.'), [
      (false, 'Install it:'),
      (true, 'npm i -g foo'),
    ]);
  });
}
