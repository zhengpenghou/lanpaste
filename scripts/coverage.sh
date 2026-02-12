#!/usr/bin/env bash
set -euo pipefail

cargo test --all-targets --all-features
cargo llvm-cov --all-features --workspace --tests --json --output-path target/llvm-cov.json

line=$(jq '.data[0].totals.lines.percent' target/llvm-cov.json)
branch=$(jq '.data[0].totals.branches.percent' target/llvm-cov.json)

if [[ "$line" != "100" && "$line" != "100.0" ]]; then
  echo "line coverage gate failed: $line"
  exit 1
fi
if [[ "$branch" != "100" && "$branch" != "100.0" ]]; then
  echo "branch coverage gate failed: $branch"
  exit 1
fi

echo "coverage gate passed: line=$line branch=$branch"
