#!/bin/bash

set -ex

BASEPATH=`dirname $(readlink -f ${BASH_SOURCE[0]})` && cd $BASEPATH

# Generate build-info.json with version and commit info
echo ""
echo "==> Generating build-info.json..."

# Get version from Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
if [ -z "$VERSION" ]; then
    VERSION="unknown"
fi

# Get commit info
COMMIT=$(git rev-parse --short=7 HEAD 2>/dev/null || echo "unknown")
COMMIT_DATE=$(TZ='Asia/Shanghai' git log -1 --format='%cd' --date=format:'%Y-%m-%d %H:%M:%S' 2>/dev/null || echo "unknown")
COMMIT_MESSAGE=$(git log -1 --format='%s' 2>/dev/null || echo "unknown")
BUILD_TIME=$(TZ='Asia/Shanghai' date '+%Y-%m-%d %H:%M:%S')

# Create build-info.json in a temporary location
BUILD_INFO=$(cat <<EOF
{
  "version": "$VERSION",
  "commit": "$COMMIT",
  "commitDate": "$COMMIT_DATE",
  "commitMessage": "$COMMIT_MESSAGE",
  "buildTime": "$BUILD_TIME"
}
EOF
)

echo "$BUILD_INFO" > build-info-temp.json

# Parse target architecture from command line
TARGET=${1:-amd64}

if [ "$TARGET" != "amd64" ] && [ "$TARGET" != "arm64" ]; then
    echo "Usage: $0 [amd64|arm64]"
    echo "Invalid target: $TARGET"
    exit 1
fi

echo "Building for target architecture: $TARGET"

# Step 1: Build frontend
echo ""
echo "==> Building frontend..."
rm -rf public

# Copy build-info.json to frontend/public
mkdir -p frontend/public
cp build-info-temp.json frontend/public/build-info.json

cd frontend
rm -rf out
pnpm install --no-frozen-lockfile
pnpm run build
ls out
cp -rf out ../public
cd ..
rm build-info-temp.json

# Step 2: Build Rust binary with cross-compilation
echo ""
echo "==> Building Rust binary for ${TARGET}..."

if [ "$TARGET" = "arm64" ]; then
    cargo zigbuild --release --features tcp_tunnel --target aarch64-unknown-linux-musl
    ls -alht target/aarch64-unknown-linux-musl/release/miao-rust
    echo ""
    echo "==> Build completed successfully!"
    echo "Binary location: target/aarch64-unknown-linux-musl/release/miao-rust"
else
    cargo zigbuild --release --features tcp_tunnel --target x86_64-unknown-linux-musl
    ls -alht target/x86_64-unknown-linux-musl/release/miao-rust
    echo ""
    echo "==> Build completed successfully!"
    echo "Binary location: target/x86_64-unknown-linux-musl/release/miao-rust"
fi
