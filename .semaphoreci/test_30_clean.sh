#!/bin/bash
set -Eeu

# Free some space to avoid disk space issues on Semaphore
root=$(git rev-parse --show-toplevel)
cd "$root/integration_tests"
cargo clean
