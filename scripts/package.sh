#!/bin/bash -xe

PACKAGES=(snarkos snarkos-cli)

RESULT=$(scripts/compile.sh "${PACKAGES[@]}")

# must supress
# shellcheck disable=SC2086
TARGET_DIR=$(echo $RESULT | grep "TARGET_DIR" | awk '{ print $2 }')
IFS=$'\n'
EXECUTABLES=( $(find "$TARGET_DIR" -maxdepth 1 -type f ! -name "*.*" ) )
unset IFS

DIST_DIR=target/dist

mkdir -p $DIST_DIR/bin $DIST_DIR/log $DIST_DIR/etc $DIST_DIR/js/ $DIST_DIR/scripts
rsync -avizh "${EXECUTABLES[@]}" $DIST_DIR/bin/
rsync -avizh --delete etc/. $DIST_DIR/etc_example/
rsync -avizh scripts/link.sh $DIST_DIR/scripts/
scripts/info.sh $DIST_DIR/info.txt
