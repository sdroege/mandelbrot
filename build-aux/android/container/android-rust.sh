#!/bin/bash

if [ -z $GOOGLE_RUST_BRANCH -o -z $RUST_VERSION ]; then
    echo "GOOGLE_RUST_BRANCH and RUST_VERSION env var must be set!"
    exit 1
fi

git clone --depth 1 --filter=blob:none --sparse https://android.googlesource.com/platform/prebuilts/rust --branch=${GOOGLE_RUST_BRANCH} /opt/rust/
git -C /opt/rust/ sparse-checkout set linux-x86/${RUST_VERSION}
rm -rf /opt/rust/.git
