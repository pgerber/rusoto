#!/bin/bash
set -Eeu

cd "$GIT_ROOT/service_crategen"
cargo +nightly run -- generate -c ./services.json -o ../rusoto/services
diff=$(git diff)
if [ -n "$diff" ]; then
    echo -en "\\e[31m"
    echo "ERROR: Generated files differ after regenerating them. Make sure you check in changes"
    echo "ERROR: in generated code. Details can be found in service_crategen/README.md."
    echo -en "\\e[0m"
    echo
    echo "Differences after regenerating:"
    echo
    echo
    echo "$diff"
    exit 1
fi
