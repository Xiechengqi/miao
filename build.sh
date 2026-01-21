#!/bin/bash

set -ex

BASEPATH=`dirname $(readlink -f ${BASH_SOURCE[0]})` && cd $BASEPATH

cd frontend
rm -rf out
pnpm i
pnpm run build
ls -alht out
cp -rf -v out ../public
cd ..
cargo build --release
ls -alht target/release/miao-rust
