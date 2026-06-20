// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {LensMintVerifier} from "../src/LensMintVerifier.sol";
import {RiscZeroMockVerifier} from "risc0-risc0-ethereum-3.0.0/test/RiscZeroMockVerifier.sol";

contract DeployLensMintVerifierScript is Script {
    function run() external {
        address deployer = msg.sender;
        
        console.log("=== Deploying LensMintVerifier ===");
        console.log("Deployer:", deployer);
        console.log("Balance:", deployer.balance);
        
        vm.startBroadcast();
        
        console.log("\nDeploying RiscZeroMockVerifier...");
        RiscZeroMockVerifier mockVerifier = new RiscZeroMockVerifier(0xFFFFFFFF);
        address verifierAddress = address(mockVerifier);
        console.log("RiscZeroMockVerifier deployed at:", verifierAddress);
        
        bytes32 imageId;
        bytes32 notaryKeyFingerprint;
        bytes32 queriesHash;
        string memory expectedUrl;
        
        try vm.envBytes32("ZK_PROVER_GUEST_ID") returns (bytes32 _imageId) {
            imageId = _imageId;
        } catch {
            imageId = bytes32(uint256(0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef));
        }
        
        try vm.envBytes32("NOTARY_KEY_FINGERPRINT") returns (bytes32 _notary) {
            notaryKeyFingerprint = _notary;
        } catch {
            notaryKeyFingerprint = bytes32(uint256(0xa7e62d7f17aa7a22c26bdb93b7ce9400e826ffb2c6f54e54d2ded015677499af));
        }
        
        try vm.envBytes32("QUERIES_HASH") returns (bytes32 _hash) {
            queriesHash = _hash;
        } catch {
            queriesHash = bytes32(uint256(0xe71522e1a30a829226017e79ea606d946dae20d107ae9e310fd4e47dccf8ccfa));
        }
        
        try vm.envString("EXPECTED_URL") returns (string memory _url) {
            expectedUrl = _url;
        } catch {
            expectedUrl = "https://lensmint.onrender.com/api/metadata/";
        }
        
        console.log("\nConfiguration:");
        console.log("Image ID:", vm.toString(imageId));
        console.log("Notary Key Fingerprint:", vm.toString(notaryKeyFingerprint));
        console.log("Queries Hash:", vm.toString(queriesHash));
        console.log("Expected URL:", expectedUrl);
        
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
        
        console.log("\n=== Deployment Complete ===");
        console.log("RISC Zero Verifier (Mock):", verifierAddress);
        console.log("LensMintVerifier:", verifierContractAddress);
    }
}

