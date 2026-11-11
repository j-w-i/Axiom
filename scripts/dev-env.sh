# Source this file: `source scripts/dev-env.sh`.
# Locates the Qt SDK and defines `axiom_cmake`, a cmake wrapper with the right
# prefix paths. See docs/contributing/dev-setup.md.
#
# Qt lookup order:
#   1. $AXIOM_QT                      (a Qt prefix, e.g. ~/Qt/6.12.0/gcc_64)
#   2. .toolchains/qt/<ver>/<arch>    (installed by scripts/bootstrap-qt.sh)
#   3. whatever CMake finds on the system
_axiom_root="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

if [[ -z "${AXIOM_QT:-}" ]]; then
  for _d in "$_axiom_root"/.toolchains/qt/6.12.*/* "$_axiom_root"/.toolchains/qt/6.11.*/*; do
    if [[ -d "$_d/lib/cmake/Qt6" ]]; then export AXIOM_QT="$_d"; break; fi
  done
fi

# Optional: headers extracted without root by scripts/bootstrap-qt.sh --gl-headers
# (only needed where the distro's OpenGL/xkbcommon -devel packages are absent).
if [[ -z "${AXIOM_SYSROOT:-}" && -d "$_axiom_root/.toolchains/sysroot-dev/usr" ]]; then
  export AXIOM_SYSROOT="$_axiom_root/.toolchains/sysroot-dev/usr"
fi

axiom_cmake() {
  local args=(-G Ninja -DCMAKE_BUILD_TYPE="${AXIOM_BUILD_TYPE:-RelWithDebInfo}" -Wno-dev)
  local prefix="${AXIOM_QT:-}"
  if [[ -n "${AXIOM_SYSROOT:-}" ]]; then
    prefix="${prefix:+$prefix;}$AXIOM_SYSROOT"
    args+=(-DCMAKE_INCLUDE_PATH="$AXIOM_SYSROOT/include" -DCMAKE_LIBRARY_PATH="$AXIOM_SYSROOT/lib64"
           -DCMAKE_CXX_FLAGS="-isystem $AXIOM_SYSROOT/include")
  fi
  [[ -n "$prefix" ]] && args+=(-DCMAKE_PREFIX_PATH="$prefix")
  cmake "${args[@]}" "$@"
}
