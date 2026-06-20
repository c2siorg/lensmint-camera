#!/usr/bin/env bash

# ==============================================================================
# Script: deployMultichain.sh
# Documentation: 
#   Automates deployment across multiple EVM-compatible networks using Foundry.
#   Uses Foundry Keystores (--account) to avoid storing private keys in .env.
# 
# Usage:
#   ./deployMultichain.sh --networks sepolia,polygon --account my-account
#   ./deployMultichain.sh --networks sepolia --account my-account --deploy-verifier
# ==============================================================================

# Exit the script if any command fails
set -e

# 1. Load variables from .env file if it exists safely
if [ -f .env ]; then
    set -a
    source .env
    set +a
fi

# 2. Argument Parsing
TARGET_NETWORKS=""
DEPLOY_VERIFIER=false
FORGE_PASS_THROUGH_FLAGS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --networks)
            TARGET_NETWORKS="$2"
            shift 2
            ;;
        --deploy-verifier)
            DEPLOY_VERIFIER=true
            shift
            ;;
        *)
            # Collect all other flags (like --account, --keystore, --verify)
            FORGE_PASS_THROUGH_FLAGS+=("$1")
            shift
            ;;
    esac
done

# Basic validation
if [ -z "$TARGET_NETWORKS" ]; then
    echo "Error: Missing mandatory argument --networks."
    echo "Usage: ./deployMultichain.sh --networks <sepolia,polygon> --account <your-acc>"
    exit 1
fi

# 3. Deployment Pipeline
# Split the comma-separated network names into an array
IFS=',' read -ra NETWORK_LIST <<< "$TARGET_NETWORKS"

for network_name in "${NETWORK_LIST[@]}"; do
    echo "----------------------------------------------------"
    echo "Processing Network: $network_name"
    echo "----------------------------------------------------"

    # Auto-generate RPC variable name (e.g., SEPOLIA_RPC_URL)
    # Using posix compatible upper-casing logic
    NETWORK_UPPER=$(echo "$network_name" | tr '[:lower:]' '[:upper:]')
    RPC_VAR_NAME="${NETWORK_UPPER}_RPC_URL"
    RPC_URL="${!RPC_VAR_NAME}"

    if [ -z "$RPC_URL" ]; then
        echo "Warning: RPC URL not found for $network_name ($RPC_VAR_NAME). Skipping..."
        continue
    fi

    echo "RPC URL found: $RPC_URL"
    echo "Executing standard Forge Deployment Script..."

    # Use array for pass through flags to prevent splitting issues
    NETWORK_ENV="NETWORK=$network_name"
    
    # Deploy Registry and LensMint
    env $NETWORK_ENV forge script script/Deploy.s.sol \
        --rpc-url "$RPC_URL" \
        --broadcast \
        "${FORGE_PASS_THROUGH_FLAGS[@]}"
    
    # Optionally Deploy the Verifier
    if [ "$DEPLOY_VERIFIER" = true ]; then
        echo "Executing Verifier Deployment Script (--deploy-verifier enabled)..."
        env $NETWORK_ENV forge script script/DeployVerifier.s.sol \
            --rpc-url "$RPC_URL" \
            --broadcast \
            "${FORGE_PASS_THROUGH_FLAGS[@]}"
    fi
    
    echo "Done with $network_name."
    echo ""
done

echo "Global deployment sequence completed successfully."
