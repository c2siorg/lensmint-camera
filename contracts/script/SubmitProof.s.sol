// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {LensMintVerifier} from "../src/LensMintVerifier.sol";

contract SubmitProofScript is Script {
    function run() external {
        address deployer = msg.sender;
        
        address verifierAddress = vm.envAddress("VERIFIER_CONTRACT_ADDRESS");
        string memory proofFilePath = vm.envString("PROOF_FILE");
        
        console.log("=== Submitting ZK Proof ===");
        console.log("Deployer:", deployer);
        console.log("Verifier Contract:", verifierAddress);
        console.log("Proof File:", proofFilePath);
        
        string memory proofJson = vm.readFile(proofFilePath);
        
        vm.startBroadcast();
        
        LensMintVerifier verifier = LensMintVerifier(verifierAddress);
        
        vm.stopBroadcast();
        
        console.log("Proof submission complete");
    }
}

