#!/bin/sh
# Builds the Linux release: publish/kilno-<version>-linux-x64.tar.gz and its .sha256, the way
# scripts/publish.ps1 builds the Windows zip. Run from the repository root.
set -eu
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
cargo build --release -p kilno-app
rm -rf publish
stage=publish/kilno
mkdir -p "$stage"
cp target/release/kilno "$stage/"
cp crates/kilno-app/res/linux/kilno.desktop "$stage/"
cp docs/assets/logo.png "$stage/kilno.png"
cp LICENSE README.md "$stage/"
name="kilno-$version-linux-x64.tar.gz"
tar -C publish -czf "publish/$name" kilno
(cd publish && sha256sum "$name" > "$name.sha256")
echo "publish/$name"
