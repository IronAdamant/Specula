#!/usr/bin/env bash
# Package a Bevy demo: release binary + assets/ → dist/<name>/
# Usage:
#   ./scripts/package-demo.sh <package_name> <game_dir> [--out DIR] [--no-zip]
# Examples:
#   ./scripts/package-demo.sh demo_2d games/demo-2d
#   ./scripts/package-demo.sh kit_2d /path/to/scaffold --out /tmp/dist-out

set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <package_name> <game_dir> [--out DIR] [--no-zip]" >&2
  exit 2
fi

PKG="$1"
GAME_DIR="$2"
shift 2

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_ROOT="${ROOT}/dist"
DO_ZIP=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      OUT_ROOT="$2"
      shift 2
      ;;
    --no-zip)
      DO_ZIP=0
      shift
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

GAME_DIR="$(cd "$GAME_DIR" && pwd)"
ASSETS="${GAME_DIR}/assets"
if [[ ! -d "$ASSETS" ]]; then
  echo "error: assets/ not found under ${GAME_DIR}" >&2
  exit 1
fi

MANIFEST="${GAME_DIR}/Cargo.toml"
if [[ ! -f "$MANIFEST" ]]; then
  echo "error: Cargo.toml not found under ${GAME_DIR}" >&2
  exit 1
fi

cd "$ROOT"

# Prefer workspace package build when the crate is a member; else manifest-path.
if cargo metadata --no-deps --format-version 1 2>/dev/null | grep -q "\"name\":\"${PKG}\""; then
  echo ">> cargo build --release -p ${PKG}"
  cargo build --release -p "${PKG}"
  BIN_SRC="${ROOT}/target/release/${PKG}"
else
  echo ">> cargo build --release --manifest-path ${MANIFEST}"
  cargo build --release --manifest-path "${MANIFEST}"
  # target may be under game dir or workspace
  if [[ -x "${GAME_DIR}/target/release/${PKG}" ]]; then
    BIN_SRC="${GAME_DIR}/target/release/${PKG}"
  elif [[ -x "${ROOT}/target/release/${PKG}" ]]; then
    BIN_SRC="${ROOT}/target/release/${PKG}"
  else
    echo "error: could not find release binary for ${PKG}" >&2
    exit 1
  fi
fi

if [[ "$(uname -s)" == MINGW* ]] || [[ "$(uname -s)" == MSYS* ]] || [[ "$(uname -s)" == CYGWIN* ]]; then
  if [[ -f "${BIN_SRC}.exe" ]]; then
    BIN_SRC="${BIN_SRC}.exe"
  fi
fi

if [[ ! -f "$BIN_SRC" ]]; then
  echo "error: binary not found at ${BIN_SRC}" >&2
  exit 1
fi

DEST="${OUT_ROOT}/${PKG}"
rm -rf "${DEST}"
mkdir -p "${DEST}"
cp "${BIN_SRC}" "${DEST}/"
cp -R "${ASSETS}" "${DEST}/assets"

# Small readme for players
cat > "${DEST}/HOW_TO_RUN.txt" <<EOF
Run the binary from this folder so the game finds assets/.

  cd "$(basename "${DEST}")"
  ./${PKG}

Built with Specula package-demo (non-Steam).
EOF

echo ">> packaged: ${DEST}"
ls -la "${DEST}"

if [[ "$DO_ZIP" -eq 1 ]]; then
  OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
  ZIP="${OUT_ROOT}/${PKG}-${OS}.zip"
  rm -f "${ZIP}"
  (
    cd "${OUT_ROOT}"
    if command -v zip >/dev/null 2>&1; then
      zip -r "$(basename "${ZIP}")" "$(basename "${DEST}")"
    else
      tar -czf "${PKG}-${OS}.tar.gz" "$(basename "${DEST}")"
      ZIP="${OUT_ROOT}/${PKG}-${OS}.tar.gz"
    fi
  )
  echo ">> archive: ${ZIP}"
fi
