#!/bin/sh

lodestone_state_root() {
  printf '%s\n' "${LODESTONE_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/lodestone}"
}

lodestone_cargo_target_dir() {
  printf '%s\n' "${LODESTONE_CARGO_TARGET_DIR:-$(lodestone_state_root)/cargo-target}"
}

lodestone_export_cargo_env() {
  export CARGO_TARGET_DIR
  CARGO_TARGET_DIR=$(lodestone_cargo_target_dir)
}
