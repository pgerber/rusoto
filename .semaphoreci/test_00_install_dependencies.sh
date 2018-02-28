#!/bin/bash
set -Eeu

if [ "${SEMAPHORE-}" = "true" ] && ! type cargo 2>/dev/null; then
    sudo apt-get update
    sudo apt-get install -y python3 python3-requests
    curl https://sh.rustup.rs -sSf | sh -s -- -y
else
    echo "Not on Semaphore CI or cargo already exists, skipping setup …"
fi
