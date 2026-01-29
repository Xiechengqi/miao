#!/usr/bin/env bash

set -ex
BASEPATH=`dirname $(readlink -f ${BASH_SOURCE[0]})` && cd $BASEPATH

ls target/release/miao-rust
cd target/release/
ps aux | grep -v grep | grep miao | awk '{print $2}' | xargs -n1 -I{} kill -9 {} || true
kill -9 $(ss -plunt | grep 6161 | awk -F 'pid=' '{print $NF}' | awk -F ',' '{print $1}') || true
./miao-rust
