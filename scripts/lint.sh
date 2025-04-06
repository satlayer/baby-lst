#!/bin/bash

cargo fmt --all -- --check
cargo clippy -- -D warnings