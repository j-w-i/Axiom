# Copies the built editor bundle next to the executable.
# Usage: cmake -DSRC=<editor/dist> -DDST=<build>/editor -P CopyEditorAssets.cmake
if(NOT EXISTS "${SRC}/index.html")
  message(FATAL_ERROR
    "The editor bundle is missing: ${SRC}/index.html does not exist.\n"
    "Build it first:  (cd editor && pnpm install --frozen-lockfile && pnpm run build)")
endif()
file(REMOVE_RECURSE "${DST}")
file(COPY "${SRC}/" DESTINATION "${DST}")
