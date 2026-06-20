// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {LensMintVerifier} from "../src/LensMintVerifier.sol";
import {RiscZeroMockVerifier} from "risc0-risc0-ethereum-3.0.0/test/RiscZeroMockVerifier.sol";

contract DeployVerifierScript is Script {
    function run() external {
        // Automatically detected by Forge via --account or --sender
        address deployer = msg.sender;
        
        string memory network = vm.envOr("NETWORK", string("unknown"));
        address riscZeroVerifierAddress = vm.envOr("RISC_ZERO_VERIFIER_ADDRESS", address(0));
        
        bytes32 imageId = vm.envBytes32("ZK_PROVER_GUEST_ID");
        bytes32 notaryKeyFingerprint = vm.envBytes32("NOTARY_KEY_FINGERPRINT");
        bytes32 queriesHash = vm.envBytes32("QUERIES_HASH");
        string memory expectedUrl = vm.envString("EXPECTED_URL");
        
        console.log("=== LensMintVerifier Deployment Configuration ===");
        console.log("Network:", network);
        console.log("Deployer address:", deployer);
        console.log("Deployer balance:", deployer.balance);
        console.log("Image ID:", vm.toString(imageId));
        console.log("Notary Key Fingerprint:", vm.toString(notaryKeyFingerprint));
        console.log("Queries Hash:", vm.toString(queriesHash));
        console.log("Expected URL:", expectedUrl);
        console.log("");
        
        vm.startBroadcast();
        
        address verifierAddress;
        if (riscZeroVerifierAddress != address(0)) {
            console.log("Using existing RISC Zero verifier at:", riscZeroVerifierAddress);
            verifierAddress = riscZeroVerifierAddress;
        } else {
            console.log("Deploying RiscZeroMockVerifier for testing...");
            RiscZeroMockVerifier mockVerifier = new RiscZeroMockVerifier(0xFFFFFFFF);
            verifierAddress = address(mockVerifier);
            console.log("RiscZeroMockVerifier deployed at:", verifierAddress);
        }
        
        console.log("\nDeploying LensMintVerifier...");
        LensMintVerifier verifier = new LensMintVerifier(
            verifierAddress,
            imageId,
            notaryKeyFingerprint,
            queriesHash,
            expectedUrl
        );
        address verifierContractAddress = address(verifier);
        console.log("LensMintVerifier deployed at:", verifierContractAddress);
        
        vm.stopBroadcast();
        
        console.log("\n=== Deployment Summary ===");
        console.log("RISC Zero Verifier:", verifierAddress);
        console.log("LensMintVerifier:", verifierContractAddress);
    }
}

