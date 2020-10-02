#!/bin/sh
cd sand
set -e
cargo +nightly build --release
ls -l target/release/bandsocks-sand

