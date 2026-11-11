#include "Shell.h"
#include "editor/EditorSetup.h"

#include <QCommandLineParser>
#include <QGuiApplication>
#include <QTextStream>

int main(int argc, char *argv[])
{
    // `--version` must work without a display, so answer before creating the application.
    for (int i = 1; i < argc; ++i) {
        if (qstrcmp(argv[i], "--version") == 0 || qstrcmp(argv[i], "-v") == 0) {
            QTextStream(stdout) << "axiom " << AXIOM_VERSION << Qt::endl;
            return 0;
        }
    }

    // WebEngine needs the scheme registered and initialised before the application exists.
    initEditorWebEngine();
    QGuiApplication app(argc, argv);
    QGuiApplication::setOrganizationName(QStringLiteral("axiom"));
    QGuiApplication::setApplicationName(QStringLiteral("Axiom"));
    QGuiApplication::setApplicationVersion(QStringLiteral(AXIOM_VERSION));

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("Axiom desktop shell"));
    parser.addHelpOption();
    parser.addOption({ QStringLiteral("daemon"), QStringLiteral("Path to the axiomd binary."),
                       QStringLiteral("path") });
    parser.addOption({ QStringLiteral("data-dir"),
                       QStringLiteral("Data directory forwarded to axiomd."),
                       QStringLiteral("dir") });
    parser.addOption({ QStringLiteral("editor-assets"),
                       QStringLiteral("Directory with the built editor bundle (editor/dist)."),
                       QStringLiteral("dir") });
    parser.addOption({ QStringLiteral("version"), QStringLiteral("Print the version.") });
    parser.process(app);

    ShellOptions options;
    options.daemonPath = parser.value(QStringLiteral("daemon"));
    options.dataDir = parser.value(QStringLiteral("data-dir"));
    options.editorAssets = parser.value(QStringLiteral("editor-assets"));

    Shell shell(options);
    if (!shell.load())
        return 1;
    return app.exec();
}
