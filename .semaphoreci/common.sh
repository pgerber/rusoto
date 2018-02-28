# sourced by other scripts

export RUST_BACKTRACE=1
if [ "${SEMAPHORE-}" = "true" ] && ! type cargo 2>/dev/null; then
    export PATH=${HOME}/.cargo/bin:${PATH}
fi
