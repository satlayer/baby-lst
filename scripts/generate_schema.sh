#!/bin/bash

start_dir=$(pwd)

for dir in "$@"; do
    if [ -f "$dir/Cargo.toml" ]; then
        echo "Running command in $dir"
        cd $dir
        cargo schema
        cd $start_dir
    else
        echo "No Cargo.toml found in $dir, skipping"
    fi
done
