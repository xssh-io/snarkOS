#!/bin/bash -xe
PACKAGES=( "$@" )
PROJECT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && cd ../.. && pwd )
HOST_TRIPLET=$(rustc --version --verbose | grep host | awk '{ print $2 }')
CROSS_TARGET=x86_64-unknown-linux-gnu
if [ "$HOST_TRIPLET" != "$CROSS_TARGET" ]; then
    BUILD="cargo zigbuild --target=$CROSS_TARGET"
    TARGET_DIR=$PROJECT_DIR/target/$CROSS_TARGET/release
else
    BUILD="cargo build"
    TARGET_DIR=$PROJECT_DIR/target/release
fi

for PKG in "${PACKAGES[@]}"; do
  $BUILD --package=$PKG --all-targets --release
done
echo "TARGET_DIR $TARGET_DIR"