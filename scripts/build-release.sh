#!/usr/bin/env bash
# Build release artifacts locally.
#
# Usage:
#   ./scripts/build-release.sh          # Build for current platform
#   ./scripts/build-release.sh all      # Build all macOS targets (arm64 + amd64)

set -euo pipefail

BINARY_NAME="zero"
DIST_DIR="dist"

# Read version from Cargo.toml
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Building ${BINARY_NAME} v${VERSION}"

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *) echo "unknown" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "amd64" ;;
        aarch64|arm64) echo "arm64" ;;
        *) echo "unknown" ;;
    esac
}

rust_target() {
    local os="$1" arch="$2"
    case "${os}-${arch}" in
        macos-arm64)  echo "aarch64-apple-darwin" ;;
        macos-amd64)  echo "x86_64-apple-darwin" ;;
        linux-amd64)  echo "x86_64-unknown-linux-gnu" ;;
        linux-arm64)  echo "aarch64-unknown-linux-gnu" ;;
        *) echo ""; return 1 ;;
    esac
}

package_artifact() {
    local target="$1" os_arch="$2"
    local artifact_name="${BINARY_NAME}-${VERSION}-${os_arch}"

    echo "Building for ${target}..."
    cargo build --release --target "${target}" -p zero

    mkdir -p "${DIST_DIR}/${artifact_name}"
    cp "target/${target}/release/${BINARY_NAME}" "${DIST_DIR}/${artifact_name}/"
    cp LICENSE README.md "${DIST_DIR}/${artifact_name}/" 2>/dev/null || true

    cd "${DIST_DIR}"
    tar -czf "${artifact_name}.tgz" "${artifact_name}"
    shasum -a 512 "${artifact_name}.tgz" | cut -d' ' -f1 > "${artifact_name}.tgz.sha512"
    cd ..

    echo "  → ${DIST_DIR}/${artifact_name}.tgz"
    echo "  → ${DIST_DIR}/${artifact_name}.tgz.sha512"
}

# Clean previous dist
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

OS=$(detect_os)
ARCH=$(detect_arch)

if [ "${1:-}" = "all" ]; then
    if [ "$OS" != "macos" ]; then
        echo "Error: 'all' only supports macOS (cross-compiles arm64 + amd64)"
        exit 1
    fi
    # Ensure both targets are installed
    rustup target add aarch64-apple-darwin x86_64-apple-darwin 2>/dev/null || true
    package_artifact "aarch64-apple-darwin" "macos-arm64"
    package_artifact "x86_64-apple-darwin" "macos-amd64"
else
    TARGET=$(rust_target "$OS" "$ARCH")
    if [ -z "$TARGET" ]; then
        echo "Error: unsupported platform ${OS}-${ARCH}"
        exit 1
    fi
    package_artifact "$TARGET" "${OS}-${ARCH}"
fi

echo ""
echo "Done. Artifacts in ${DIST_DIR}/:"
ls -lh "${DIST_DIR}"/*.tgz "${DIST_DIR}"/*.sha512 2>/dev/null
