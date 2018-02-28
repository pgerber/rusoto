#!/bin/bash
set -Eeu

# Free some space to avoid disk space issues on Semaphore
cd "$GIT_ROOT/integration_tests"
cargo clean
