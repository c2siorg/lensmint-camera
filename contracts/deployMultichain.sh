#!/usr/bin/env bash

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Defaults (can be overridden by flags or env)
NETWORKS="${NETWORKS:-sepolia}"
TARGETS="${DEPLOY_TARGETS:-core}"
BROADCAST="${BROADCAST:-true}"
DO_VERIFY="${VERIFY:-false}"

# Signer (CLI)
ACCOUNT=""
KEYSTORE=""
PASSWORD=""
PASSWORD_FILE=""
PRIVATE_KEY_CLI=""

# Verifier params (flags override env)
FLAG_EXPECTED_URL=""
FLAG_ZK_PROVER_GUEST_ID=""
FLAG_NOTARY_KEY_FINGERPRINT=""
FLAG_QUERIES_HASH=""
FLAG_RISC_ZERO_VERIFIER=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --networks)
      NETWORKS="$2"
      shift 2
      ;;
    --targets)
      TARGETS="$2"
      shift 2
      ;;
    --no-broadcast)
      BROADCAST="false"
      shift
      ;;
    --broadcast)
      BROADCAST="true"
      shift
      ;;
    --verify)
      DO_VERIFY="true"
      shift
      ;;
    --account)
      ACCOUNT="$2"
      shift 2
      ;;
    --keystore)
      KEYSTORE="$2"
      shift 2
      ;;
    --password)
      PASSWORD="$2"
      shift 2
      ;;
    --password-file)
      PASSWORD_FILE="$2"
      shift 2
      ;;
    --private-key)
      PRIVATE_KEY_CLI="$2"
      shift 2
      ;;
    --expected-url)
      FLAG_EXPECTED_URL="$2"
      shift 2
      ;;
    --zk-prover-guest-id)
      FLAG_ZK_PROVER_GUEST_ID="$2"
      shift 2
      ;;
    --notary-key-fingerprint)
      FLAG_NOTARY_KEY_FINGERPRINT="$2"
      shift 2
      ;;
    --queries-hash)
      FLAG_QUERIES_HASH="$2"
      shift 2
      ;;
    --risc-zero-verifier)
      FLAG_RISC_ZERO_VERIFIER="$2"
      shift 2
      ;;
    --help|-h)
      cat <<'EOF'
deployMultichain.sh — deploy LensMint contracts via forge script

USAGE
  cd contracts && ./deployMultichain.sh [OPTIONS]

OPTIONS
  --networks LIST     Comma-separated names (default: sepolia). Examples: sepolia,mainnet
  --targets LIST      core | verifier | core,verifier (default: core)
  --broadcast         Send txs (default)
  --no-broadcast      Simulate only; no signer required
  --verify            Pass --verify to forge (needs ETHERSCAN_API_KEY)

SIGNER (required when broadcasting; same as forge script)
  --account NAME
  --keystore PATH [--password-file PATH | --password PASS]
  --private-key HEX

VERIFIER CONFIG (if --targets includes verifier; or set env vars)
  --expected-url URL
  --zk-prover-guest-id 0x...
  --notary-key-fingerprint 0x...
  --queries-hash 0x...
  --risc-zero-verifier ADDR   Optional existing RISC Zero verifier address

ENVIRONMENT
  SEPOLIA_RPC_URL, MAINNET_RPC_URL, ...  For network name "foo", uses FOO_RPC_URL
  EXPECTED_URL, ZK_PROVER_GUEST_ID, NOTARY_KEY_FINGERPRINT, QUERIES_HASH
  RISC_ZERO_VERIFIER_ADDRESS (optional)
  ETHERSCAN_API_KEY                        For --verify
  NETWORKS, DEPLOY_TARGETS, BROADCAST, VERIFY  Default overrides

CAST / WALLET SETUP (example)
  cast wallet import deployer --interactive
  ./deployMultichain.sh --networks sepolia --account deployer

EFFECTIVE COMMAND (per script)
  forge script <script> [--account|...] --rpc-url <url> --non-interactive -vvv [--broadcast] [--verify]
EOF
      exit 0
      ;;
    *)
      echo -e "${RED}Unknown option: $1${NC}" >&2
      exit 1
      ;;
  esac
done

IFS=',' read -ra NETWORK_ARR <<< "$NETWORKS"
IFS=',' read -ra TARGETS_ARR <<< "$TARGETS"

has_target() {
  local t="$1"
  for x in "${TARGETS_ARR[@]}"; do
    if [[ "$x" == "$t" ]]; then
      return 0
    fi
  done
  return 1
}

require_env() {
  local v="$1"
  if [[ -z "${!v:-}" ]]; then
    echo -e "${RED}Missing required env var: $v${NC}" >&2
    exit 1
  fi
}

# Apply flag overrides into env for Solidity vm.env*()
export EXPECTED_URL="${FLAG_EXPECTED_URL:-${EXPECTED_URL:-}}"
export ZK_PROVER_GUEST_ID="${FLAG_ZK_PROVER_GUEST_ID:-${ZK_PROVER_GUEST_ID:-}}"
export NOTARY_KEY_FINGERPRINT="${FLAG_NOTARY_KEY_FINGERPRINT:-${NOTARY_KEY_FINGERPRINT:-}}"
export QUERIES_HASH="${FLAG_QUERIES_HASH:-${QUERIES_HASH:-}}"
if [[ -n "${FLAG_RISC_ZERO_VERIFIER:-}" ]]; then
  export RISC_ZERO_VERIFIER_ADDRESS="$FLAG_RISC_ZERO_VERIFIER"
fi

SIGNER_ARGS=()
if [[ -n "$ACCOUNT" ]]; then SIGNER_ARGS+=(--account "$ACCOUNT"); fi
if [[ -n "$KEYSTORE" ]]; then SIGNER_ARGS+=(--keystore "$KEYSTORE"); fi
if [[ -n "$PASSWORD" ]]; then SIGNER_ARGS+=(--password "$PASSWORD"); fi
if [[ -n "$PASSWORD_FILE" ]]; then SIGNER_ARGS+=(--password-file "$PASSWORD_FILE"); fi
if [[ -n "$PRIVATE_KEY_CLI" ]]; then SIGNER_ARGS+=(--private-key "$PRIVATE_KEY_CLI"); fi

rpc_url_for() {
  local net="$1"
  local key="${net^^}_RPC_URL"
  local v="${!key:-}"
  if [[ -n "$v" ]]; then
    echo "$v"
    return 0
  fi
  echo -e "${YELLOW}Warning: $key not set; using --rpc-url \"$net\". Ensure Foundry rpc_endpoints resolve it.${NC}" >&2
  echo "$net"
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

echo -e "${GREEN}Starting multichain deployment${NC}"
echo -e "${GREEN}Networks:${NC} ${NETWORKS}"
echo -e "${GREEN}Targets:${NC} ${TARGETS}"
echo -e "${GREEN}Broadcast mode:${NC} ${BROADCAST}"
echo -e "${GREEN}Etherscan verify:${NC} ${DO_VERIFY}"

if [[ "$BROADCAST" == "true" ]] && [[ ${#SIGNER_ARGS[@]} -eq 0 ]]; then
  echo -e "${RED}Broadcasting requires a signer. Use one of:${NC}" >&2
  echo -e "  ${RED}--account <name> | --keystore <path> [--password-file <path>] | --private-key <hex>${NC}" >&2
  echo -e "${YELLOW}Tip: cast wallet import mydeployer --interactive${NC}" >&2
  exit 1
fi

if has_target "verifier"; then
  require_env "EXPECTED_URL"
  require_env "ZK_PROVER_GUEST_ID"
  require_env "NOTARY_KEY_FINGERPRINT"
  require_env "QUERIES_HASH"
fi

if [[ "$DO_VERIFY" == "true" ]]; then
  if [[ -z "${ETHERSCAN_API_KEY:-}" ]]; then
    echo -e "${YELLOW}Warning: ETHERSCAN_API_KEY is not set, --verify may fail.${NC}"
  fi
fi

run_forge_script() {
  local script_path="$1"
  local rpc_url="$2"

  local cmd=(forge script "$script_path")
  if [[ ${#SIGNER_ARGS[@]} -gt 0 ]]; then
    cmd+=("${SIGNER_ARGS[@]}")
  fi
  cmd+=(--rpc-url "$rpc_url" --non-interactive -vvv)
  if [[ "$BROADCAST" == "true" ]]; then
    cmd+=(--broadcast)
  fi
  if [[ "$DO_VERIFY" == "true" ]]; then
    cmd+=(--verify)
  fi

  echo -e "\n${GREEN}========================================${NC}"
  echo -e "${GREEN}Running:${NC} $script_path"
  echo -e "${GREEN}RPC:${NC} $rpc_url"
  echo -e "${GREEN}Broadcast:${NC} $BROADCAST"
  echo -e "${GREEN}========================================${NC}\n"

  "${cmd[@]}"
}

for net in "${NETWORK_ARR[@]}"; do
  echo ""
  echo -e "${GREEN}=== Deploying to: ${net}${NC} ==="
  export NETWORK="$net"
  rpc_url="$(rpc_url_for "$net")"

  if has_target "core"; then
    echo -e "${YELLOW}[core] Deploy DeviceRegistry + LensMintERC1155${NC}"
    run_forge_script "script/Deploy.s.sol" "$rpc_url"
  fi

  if has_target "verifier"; then
    echo -e "${YELLOW}[verifier] Deploy LensMintVerifier (zk verifier wrapper)${NC}"
    run_forge_script "script/DeployVerifier.s.sol" "$rpc_url"
  fi
done

echo ""
echo -e "${GREEN}Multichain deployment finished.${NC}"
