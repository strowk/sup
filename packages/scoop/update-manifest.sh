#!/bin/bash

version=$(cargo metadata --format-version=1 --no-deps | jq '.packages[0].version' -r)
cat << EOF > packages/scoop/sup.json
{
    "bin":  "sup.exe",
    "license":  "MIT",
    "version":  "${version}",
    "homepage":  "https://github.com/strowk/sup",
    "extract_dir":  "target/x86_64-pc-windows-gnu/release",
    "url":  "https://github.com/strowk/sup/releases/download/v${version}/sup-x86_64-pc-windows-gnu.tar.gz"
}
EOF
