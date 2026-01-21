#!/bin/bash

set -ex

BASEPATH=`dirname $(readlink -f ${BASH_SOURCE[0]})` && cd $BASEPATH

# Detect architecture
ARCH=$(uname -m)
if [ "$ARCH" = "x86_64" ]; then
    TARGET="amd64"
elif [ "$ARCH" = "aarch64" ]; then
    TARGET="arm64"
else
    echo "Unsupported architecture: $ARCH"
    exit 1
fi

echo "Building for architecture: $TARGET"

# Step 1: Download embedded binaries
echo "==> Downloading embedded binaries..."
mkdir -p embedded

# Download gotty
if [ ! -f "embedded/gotty-${TARGET}" ] || [ ! -s "embedded/gotty-${TARGET}" ]; then
    echo "Downloading gotty-${TARGET}..."
    curl -L -o "embedded/gotty-${TARGET}" \
        "https://github.com/Xiechengqi/gotty/releases/download/latest/gotty-linux-${TARGET}"
    chmod +x "embedded/gotty-${TARGET}"
fi

# Download sy
if [ ! -f "embedded/sy-${TARGET}" ] || [ ! -s "embedded/sy-${TARGET}" ]; then
    echo "Downloading sy-${TARGET}..."
    curl -L -o "embedded/sy-${TARGET}" \
        "https://github.com/Xiechengqi/sy/releases/download/latest/sy-linux-${TARGET}"
    chmod +x "embedded/sy-${TARGET}"
fi

# Download sing-box
if [ ! -f "embedded/sing-box-${TARGET}" ] || [ ! -s "embedded/sing-box-${TARGET}" ]; then
    echo "Downloading sing-box-${TARGET}..."
    SINGBOX_VERSION=$(curl -s https://api.github.com/repos/SagerNet/sing-box/releases/latest | grep '"tag_name"' | cut -d'"' -f4)
    curl -L -o "embedded/sing-box-${TARGET}.tar.gz" \
        "https://github.com/SagerNet/sing-box/releases/download/${SINGBOX_VERSION}/sing-box-${SINGBOX_VERSION#v}-linux-${TARGET}.tar.gz"
    tar -xzf "embedded/sing-box-${TARGET}.tar.gz" -C embedded --strip-components=1 "sing-box-${SINGBOX_VERSION#v}-linux-${TARGET}/sing-box"
    mv "embedded/sing-box" "embedded/sing-box-${TARGET}"
    rm -f "embedded/sing-box-${TARGET}.tar.gz"
    chmod +x "embedded/sing-box-${TARGET}"
fi

ls -lh embedded/

# Step 2: Build frontend
echo ""
echo "==> Building frontend..."
rm -rf public
cd frontend
rm -rf out
pnpm install --no-frozen-lockfile
pnpm run build
ls out
cp -rf out ../public
cd ..

# Step 3: Build Rust binary
echo ""
echo "==> Building Rust binary..."
cargo build --release
ls -alht target/release/miao-rust

echo ""
echo "==> Build completed successfully!"
echo "Binary location: target/release/miao-rust"

