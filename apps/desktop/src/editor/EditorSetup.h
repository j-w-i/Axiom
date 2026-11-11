#pragma once
#include <QString>

class QObject;

// Must run BEFORE the QGuiApplication is constructed: registers the secure
// `axiom` scheme and initialises Qt WebEngine for Qt Quick.
void initEditorWebEngine();

// Where the editor bundle lives: `explicitDir`, then $AXIOM_EDITOR_ASSETS,
// then `<application dir>/editor`. Returns the first candidate that has an
// index.html, or an empty string.
QString locateEditorAssets(const QString &explicitDir);

// Serves `axiom://editor/...` from `assetDir` on the default web profile.
// `parent` owns the handler.
void installEditorSchemeHandler(const QString &assetDir, QObject *parent);
