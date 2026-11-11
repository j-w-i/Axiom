cmake_minimum_required(VERSION 3.21)
project(axiom_desktop VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_AUTOMOC ON)

# Only LGPL-available Qt modules (ADR-0011). WebEngine arrives with M1-SHELL-003.
find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 QuickDialogs2 WebEngineQuick WebChannel Test)
qt_standard_project_setup(REQUIRES 6.8)

function(axiom_warnings target)
  if(MSVC)
    target_compile_options(${target} PRIVATE /W4)
  else()
    target_compile_options(${target} PRIVATE -Wall -Wextra)
  endif()
endfunction()

# ---- QML modules -----------------------------------------------------------
# Axiom.Ui: design system. Axiom.App: views, composed from Axiom.Ui only.
set_source_files_properties(qml/Axiom/Ui/Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_library(axiom_ui STATIC)
qt_add_qml_module(axiom_ui
  URI Axiom.Ui
  VERSION 1.0
  OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/Axiom/Ui
  QML_FILES
    qml/Axiom/Ui/Theme.qml
    qml/Axiom/Ui/AxLabel.qml
    qml/Axiom/Ui/AxButton.qml
    qml/Axiom/Ui/AxIconButton.qml
    qml/Axiom/Ui/AxSeparator.qml
    qml/Axiom/Ui/AxListRow.qml
    qml/Axiom/Ui/AxTabButton.qml
    qml/Axiom/Ui/AxComboBox.qml
    qml/Axiom/Ui/AxCheckBox.qml
    qml/Axiom/Ui/AxSpinBox.qml
    qml/Axiom/Ui/AxTextField.qml
    qml/Axiom/Ui/AxBanner.qml
    qml/Axiom/Ui/AxPanelHeader.qml
    qml/Axiom/Ui/AxSplitHandle.qml
    qml/Axiom/Ui/AxDialog.qml
    qml/Axiom/Ui/AxMenuItem.qml
    qml/Axiom/Ui/AxMenuBackground.qml
    qml/Axiom/Ui/ToolTipArea.qml
)
target_link_libraries(axiom_ui PRIVATE Qt6::Quick Qt6::QuickControls2)

qt_add_library(axiom_app STATIC)
qt_add_qml_module(axiom_app
  URI Axiom.App
  VERSION 1.0
  OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml/Axiom/App
  QML_FILES
    qml/Axiom/App/Main.qml
    qml/Axiom/App/ExplorerView.qml
    qml/Axiom/App/WelcomeView.qml
    qml/Axiom/App/EditorArea.qml
    qml/Axiom/App/OutputPanel.qml
    qml/Axiom/App/SettingsDialog.qml
)
target_link_libraries(axiom_app PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::WebEngineQuick Qt6::WebChannel)

# ---- shell core (everything except main(), so tests can link it) ------------
add_library(axiom_core STATIC
  src/daemon/LineBuffer.h src/daemon/LineBuffer.cpp
  src/daemon/RpcError.h
  src/daemon/DaemonClient.h src/daemon/DaemonClient.cpp
  src/Backend.h src/Backend.cpp
  src/models/FileTreeModel.h src/models/FileTreeModel.cpp
  src/models/RecentModel.h
  src/models/WorkspaceController.h src/models/WorkspaceController.cpp
  src/models/OpenDocumentsModel.h src/models/OpenDocumentsModel.cpp
  src/models/TaskOutputModel.h src/models/TaskOutputModel.cpp
  src/models/TaskController.h src/models/TaskController.cpp
  src/models/SettingsStore.h src/models/SettingsStore.cpp
  src/editor/EditorBridge.h src/editor/EditorBridge.cpp
  src/editor/AssetSchemeHandler.h src/editor/AssetSchemeHandler.cpp
  src/editor/EditorSetup.h src/editor/EditorSetup.cpp
  src/Shell.h src/Shell.cpp
)
target_include_directories(axiom_core PUBLIC src)
target_compile_definitions(axiom_core PUBLIC AXIOM_VERSION="${PROJECT_VERSION}")
target_link_libraries(axiom_core PUBLIC Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::QuickDialogs2 Qt6::WebEngineQuick Qt6::WebChannel)
axiom_warnings(axiom_core)

qt_add_executable(axiom src/main.cpp)
target_link_libraries(axiom PRIVATE axiom_core axiom_appplugin axiom_uiplugin)
axiom_warnings(axiom)

# ---- editor assets ----------------------------------------------------------
# <build>/editor is a copy of editor/dist (built by `pnpm run build`). The
# target fails with a clear message instead of shipping a stale or missing bundle.
set(AXIOM_EDITOR_DIST "${CMAKE_SOURCE_DIR}/../../editor/dist")
add_custom_target(axiom_editor_assets ALL
  COMMAND ${CMAKE_COMMAND}
    -DSRC=${AXIOM_EDITOR_DIST} -DDST=${CMAKE_BINARY_DIR}/editor
    -P ${CMAKE_SOURCE_DIR}/cmake/CopyEditorAssets.cmake
  COMMENT "Copying the editor bundle"
  VERBATIM)
add_dependencies(axiom axiom_editor_assets)

# ---- tests ------------------------------------------------------------------
enable_testing()

# The daemon under test; check.sh builds it (debug) before running ctest.
set(AXIOM_FIXTURES_DIR "${CMAKE_SOURCE_DIR}/../../tests/fixtures")
set(AXIOMD_UNDER_TEST "${CMAKE_SOURCE_DIR}/../../target/debug/axiomd${CMAKE_EXECUTABLE_SUFFIX}")
set(AXIOM_EMIT "${CMAKE_SOURCE_DIR}/../../target/debug/emit${CMAKE_EXECUTABLE_SUFFIX}")

function(axiom_test name)
  qt_add_executable(${name} tests/${name}.cpp)
  target_link_libraries(${name} PRIVATE axiom_core Qt6::Test ${ARGN})
  target_compile_definitions(${name} PRIVATE AXIOM_FIXTURES_DIR="${AXIOM_FIXTURES_DIR}")
  axiom_warnings(${name})
  # QtTest logs to a file as well as stdout: a test killed by a timeout still leaves
  # a readable record (stdout of a killed process is lost when it is not a terminal).
  add_test(NAME ${name}
    COMMAND ${name} -o ${CMAKE_BINARY_DIR}/${name}.log,txt -o -,txt)
  set_tests_properties(${name} PROPERTIES
    ENVIRONMENT "AXIOM_EMIT=${AXIOM_EMIT};AXIOM_EDITOR_ASSETS=${CMAKE_BINARY_DIR}/editor;QT_QPA_PLATFORM=offscreen;QTWEBENGINE_CHROMIUM_FLAGS=--no-sandbox --disable-gpu;AXIOMD_UNDER_TEST=${AXIOMD_UNDER_TEST};XDG_DATA_HOME=${CMAKE_BINARY_DIR}/test-home;XDG_CONFIG_HOME=${CMAKE_BINARY_DIR}/test-home"
    TIMEOUT 45)  # the slowest test takes ~8 s locally; a hang fails in 45 s, not minutes
endfunction()

axiom_test(tst_linebuffer)
axiom_test(tst_daemonclient)
axiom_test(tst_workspace)
axiom_test(tst_assethandler)
axiom_test(tst_documents)
axiom_test(tst_tasks)
axiom_test(tst_settings)
axiom_test(tst_smoke_launch axiom_appplugin axiom_uiplugin)
axiom_test(tst_smoke_editor axiom_appplugin axiom_uiplugin)
