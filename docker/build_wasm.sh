#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${REPO_ROOT:-$(pwd)}"
TARGET="${ADO_DOCKER_TARGET:-wasm}"
PORT="${WEB_PORT:-4173}"

CPU_COUNT="${CPU_COUNT:-$(nproc)}"
WASM_BUILD_JOBS="${WASM_BUILD_JOBS:-$((CPU_COUNT - 2))}"
if [ "${WASM_BUILD_JOBS}" -lt 1 ]; then
  WASM_BUILD_JOBS=1
fi

fix_output_ownership() {
  if [ "$(id -u)" != "0" ] || [ -z "${HOST_UID:-}" ] || [ -z "${HOST_GID:-}" ]; then
    return
  fi

  for path in \
    "${REPO_ROOT}/web/pkg" \
    "${REPO_ROOT}/web/public/samples" \
    "${REPO_ROOT}/wasm_bindings/target" \
    "${REPO_ROOT}/target"; do
    if [ -e "${path}" ]; then
      chown -R "${HOST_UID}:${HOST_GID}" "${path}" || true
    fi
  done
}

trap fix_output_ownership EXIT

build_wasm() {
  cd "${REPO_ROOT}/wasm_bindings"
  export CARGO_BUILD_JOBS="${WASM_BUILD_JOBS}"
  rustup run nightly wasm-pack build . \
    --target web \
    --out-dir ../web/pkg \
    --out-name tablegram_wasm \
    --release \
    -- --jobs "${WASM_BUILD_JOBS}"
}

case "${TARGET}" in
  wasm)
    build_wasm
    ;;

  web)
    build_wasm
    cd "${REPO_ROOT}"
    cargo run --manifest-path "${REPO_ROOT}/Cargo.toml" --bin tablegram-web-server -- --root "${REPO_ROOT}/web" --host 0.0.0.0 --port "${PORT}"
    ;;

  *)
    echo "Unsupported ADO_DOCKER_TARGET=${TARGET}. Use wasm or web." >&2
    exit 1
    ;;
esac
