#!/bin/sh
# DO NOT CALL DIRECTLY
# Meant to be called by meson

set -eu

if [ "$#" -ne "3" ]; then
  echo 'DO NOT CALL DIRECTLY!'
  exit 1
fi

prevdir=`pwd`

echo '/* Empty source file to track dependency */' > "$1"

cd "$2"  # Source directory
cargo build
cd "$prevdir"
cp "$3" .

