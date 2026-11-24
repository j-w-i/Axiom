// Axiom's icon set: 16 px icons (1.4 px strokes, round caps and joins, filled where a fill tells two
// apart), drawn in-house; this file is their source.

import 'icon_data.dart';

abstract final class AxiomIcons {
  static const chevron = IconData16([IconPath('M6 4l4 4-4 4')]);
  static const folder = IconData16([
    IconPath('M2 4.5a1 1 0 011-1h3l1.5 1.5H13a1 1 0 011 1V12a1 1 0 01-1 1H3a1 1 0 01-1-1z'),
  ]);
  static const folderOpen = IconData16([
    IconPath('M2 12V4.5a1 1 0 011-1h3l1.5 1.5H12a1 1 0 011 1V7'),
    IconPath('M2.5 13l1.6-5.2a1 1 0 011-.8H14l-1.7 5.3a1 1 0 01-1 .7z'),
  ]);
  static const file = IconData16([
    IconPath('M4 2h5l3 3v8.5a.5.5 0 01-.5.5h-7a.5.5 0 01-.5-.5z'),
    IconPath('M9 2v3h3'),
  ]);
  static const link = IconData16([
    IconPath('M7 9a2.5 2.5 0 003.5 0l2-2A2.5 2.5 0 009 3.5l-.5.5'),
    IconPath('M9 7a2.5 2.5 0 00-3.5 0l-2 2A2.5 2.5 0 007 12.5l.5-.5'),
  ]);
  static const close = IconData16([IconPath('M4.5 4.5l7 7M11.5 4.5l-7 7')]);
  static const play = IconData16([IconPath('M5 3.5v9l7-4.5z', fill: true)]);
  static const stop = IconData16([IconRect(4, 4, 8, 8, rx: 1, fill: true)]);
  static const trash = IconData16([IconPath('M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.5 8.5h6l.5-8.5')]);

  /// Soft wrap: lines, one returning under itself.
  static const wrap = IconData16([
    IconPath('M2.5 4h11M2.5 8h9a2 2 0 010 4H8.5M2.5 12h3'),
    IconPath('M10 10.5L8.5 12l1.5 1.5'),
  ]);
  static const copy = IconData16([
    IconRect(5, 5, 8, 8, rx: 1.2),
    IconPath('M3 10.5V3.8A.8.8 0 013.8 3h6.7'),
  ]);
  static const sidebar = IconData16([IconRect(2, 3, 12, 10, rx: 1.5), IconPath('M6 3v10')]);
  static const panel = IconData16([IconRect(2, 3, 12, 10, rx: 1.5), IconPath('M2 9.5h12')]);
  static const gear = IconData16([
    IconCircle(8, 8, 2),
    IconPath(
      'M8 1.8v1.6M8 12.6v1.6M14.2 8h-1.6M3.4 8H1.8M12.4 3.6l-1.1 1.1M4.7 11.3l-1.1 1.1M12.4 12.4l-1.1-1.1M4.7 4.7L3.6 3.6',
    ),
  ]);
  static const search = IconData16([IconCircle(7, 7, 4), IconPath('M10 10l3.5 3.5')]);
  static const refresh = IconData16([IconPath('M13 4v3h-3'), IconPath('M12.6 7A5 5 0 103 9')]);

  /// The rail's destinations: each its own silhouette, so none is read as another.
  static const files = IconData16([
    IconPath(
      'M1.8 4.3A1.3 1.3 0 013.1 3h2.7l1.6 1.6h5.5a1.3 1.3 0 011.3 1.3v5.8a1.3 1.3 0 01-1.3 1.3H3.1a1.3 1.3 0 01-1.3-1.3z',
    ),
    IconPath('M2.5 8h11v3.6a.9.9 0 01-.9.9H3.4a.9.9 0 01-.9-.9z', fill: true),
  ]);

  /// Environments: a stack of installed parts.
  static const layers = IconData16([
    IconPath('M8 2l5.4 3L8 8 2.6 5z', fill: true),
    IconPath('M2.6 8L8 11l5.4-3'),
    IconPath('M2.6 11L8 14l5.4-3'),
  ]);

  /// The Inspector: a pointer over a frame.
  static const inspect = IconData16([
    IconPath('M6 13.5H3.5a1 1 0 01-1-1v-9a1 1 0 011-1h9a1 1 0 011 1V6'),
    IconPath('M8 8l5.6 2.2-2.4 1-1 2.4z', fill: true),
  ]);

  /// The agent: a spark.
  static const agent = IconData16([
    IconPath(
      'M8 2.2C8.4 5.7 10.3 7.6 13.8 8 10.3 8.4 8.4 10.3 8 13.8 7.6 10.3 5.7 8.4 2.2 8 5.7 7.6 7.6 5.7 8 2.2z',
      fill: true,
    ),
  ]);
  static const check = IconData16([IconPath('M3.5 8.5l3 3 6-7')]);
  static const download = IconData16([
    IconPath('M8 2.5v8M4.8 7.5L8 10.7l3.2-3.2'),
    IconPath('M3 13h10'),
  ]);
  static const archive = IconData16([
    IconRect(2, 3, 12, 3, rx: .8),
    IconPath('M3 6v6.5a.5.5 0 00.5.5h9a.5.5 0 00.5-.5V6M6.5 8.5h3'),
  ]);
  static const terminal = IconData16([
    IconRect(2, 3, 12, 10, rx: 1.2),
    IconPath('M4.5 6.5l2 1.5-2 1.5M8 10h3'),
  ]);
  static const shield = IconData16([
    IconPath('M8 2l5 2v3.5c0 3-2.2 5.3-5 6.5-2.8-1.2-5-3.5-5-6.5V4z'),
  ]);
  static const warning = IconData16([IconPath('M8 2.5l6 11H2z'), IconPath('M8 6.5v3M8 11.5v.2')]);
  static const dock = IconData16([IconRect(2, 3, 12, 10, rx: 1.5), IconPath('M10 3v10')]);
  static const chat = IconData16([
    IconPath('M2.5 4a1 1 0 011-1h9a1 1 0 011 1v6a1 1 0 01-1 1H7l-3 2.5V11h-.5a1 1 0 01-1-1z'),
  ]);
  static const branch = IconData16([
    IconCircle(4.5, 3.5, 1.5),
    IconCircle(4.5, 12.5, 1.5),
    IconCircle(11.5, 5.5, 1.5),
    IconPath('M4.5 5v6M11.5 7c0 2.5-2 3.2-5 4'),
  ]);
  static const plus = IconData16([IconPath('M8 3v10M3 8h10')]);
  static const send = IconData16([
    IconPath('M2.5 8L13.5 3l-3 10.5-2.5-4.5z'),
    IconPath('M8 9l5.5-6'),
  ]);
  static const image = IconData16([
    IconRect(2, 3, 12, 10, rx: 1.2),
    IconCircle(6, 6.5, 1.2),
    IconPath('M2.5 12l3.5-3.5 2.5 2.5 2-2 3 3'),
  ]);
  static const edit = IconData16([
    IconPath('M3 13l.6-2.6L10.5 3.5l2 2-6.9 6.9z'),
    IconPath('M9.5 4.5l2 2'),
  ]);
  static const more = IconData16([
    IconCircle(4, 8, .6, fill: true),
    IconCircle(8, 8, .6, fill: true),
    IconCircle(12, 8, .6, fill: true),
  ]);
  static const arrowDown = IconData16([IconPath('M8 3v10M4 9l4 4 4-4')]);
  static const collapse = IconData16([IconPath('M5 7l3-3 3 3M5 12l3-3 3 3')]);
  static const logo = IconData16([
    IconCircle(8, 3, 1.5),
    IconPath('M7.3 4.35L3.25 14M8.7 4.35l4.05 9.65'),
    IconPath('M4.6 10.75Q8 12.4 11.4 10.75'),
  ]);
}
