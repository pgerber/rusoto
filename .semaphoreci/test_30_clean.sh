#!/bin/bash
set -Eeu

root=$(git rev-parse --show-toplevel)
. "$root/.semaphoreci/common.sh"

# Free some space to avoid disk space issues on Semaphore
cd "$root/integration_tests"
cargo clean
