cmake_minimum_required(VERSION 3.13)
project(runner LANGUAGES CXX)

# Define the application target. To change its name, change BINARY_NAME in the
# top-level CMakeLists.txt, not the value here, or `flutter run` will no longer
# work.
#
# Any new source files that you add to the application should be added here.
add_executable(${BINARY_NAME}
  "main.cc"
  "my_application.cc"
  "wayland_present.cc"
  "${FLUTTER_MANAGED_DIR}/generated_plugin_registrant.cc"
)

# Apply the standard set of build settings. This can be removed for applications
# that need different build settings.
apply_standard_settings(${BINARY_NAME})

# Add preprocessor definitions for the application ID.
add_definitions(-DAPPLICATION_ID="${APPLICATION_ID}")

# Add dependency libraries. Add any application-specific dependencies here.
target_link_libraries(${BINARY_NAME} PRIVATE flutter)
target_link_libraries(${BINARY_NAME} PRIVATE PkgConfig::GTK)
# FL-PERF-004: wayland_present.cc commits the window's surface itself, and the executable
# exports its gtk_widget_queue_draw so the Flutter embedder's call binds to it.
pkg_check_modules(WAYLAND_CLIENT REQUIRED IMPORTED_TARGET wayland-client)
target_link_libraries(${BINARY_NAME} PRIVATE PkgConfig::WAYLAND_CLIENT ${CMAKE_DL_LIBS})
set_target_properties(${BINARY_NAME} PROPERTIES ENABLE_EXPORTS ON)
# The hook relies on the Linux embedder's internals (FlViewRendererSubsurface, one
# gtk_widget_queue_draw per frame). It was checked against the engine named in
# wayland_present.engine; another engine needs that checked again (audit finding 11).
file(READ "${CMAKE_CURRENT_SOURCE_DIR}/wayland_present.engine" AXIOM_CHECKED_ENGINE)
string(STRIP "${AXIOM_CHECKED_ENGINE}" AXIOM_CHECKED_ENGINE)
if(NOT DEFINED FLUTTER_ROOT AND EXISTS "${CMAKE_SOURCE_DIR}/flutter/ephemeral/generated_config.cmake")
  include("${CMAKE_SOURCE_DIR}/flutter/ephemeral/generated_config.cmake")
endif()
if(EXISTS "${FLUTTER_ROOT}/bin/internal/engine.version")
  file(READ "${FLUTTER_ROOT}/bin/internal/engine.version" AXIOM_ENGINE)
  string(STRIP "${AXIOM_ENGINE}" AXIOM_ENGINE)
  if(NOT AXIOM_ENGINE STREQUAL AXIOM_CHECKED_ENGINE)
    message(WARNING "Flutter's engine is ${AXIOM_ENGINE}; runner/wayland_present.cc was checked "
      "against ${AXIOM_CHECKED_ENGINE}. Check the embedder's FlViewRendererSubsurface still calls "
      "gtk_widget_queue_draw once per frame, run scripts/frames.sh, then update "
      "runner/wayland_present.engine.")
  endif()
endif()

target_include_directories(${BINARY_NAME} PRIVATE "${CMAKE_SOURCE_DIR}")
