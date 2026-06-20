// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {DeviceRegistry} from "../src/DeviceRegistry.sol";
import {LensMintERC1155} from "../src/LensMintERC1155.sol";

contract DeployScript is Script {
    function run() external {
        // Automatically detected by Forge via --account or --sender
        address deployer = msg.sender;
        
        console.log("Deploying contracts...");
        console.log("Deployer address:", deployer);
        console.log("Deployer balance:", deployer.balance);

        vm.startBroadcast();

        console.log("\nDeploying DeviceRegistry...");
        DeviceRegistry deviceRegistry = new DeviceRegistry();
        console.log("DeviceRegistry deployed at:", address(deviceRegistry));

        console.log("\nDeploying LensMintERC1155...");
        string memory baseURI = vm.envOr("BASE_URI", string("https://ipfs.io/ipfs/"));
        LensMintERC1155 lensMint = new LensMintERC1155(
            address(deviceRegistry),
            baseURI
        );
        console.log("LensMintERC1155 deployed at:", address(lensMint));

        vm.stopBroadcast();

        console.log("\n=== Deployment Summary ===");
        console.log("DeviceRegistry:", address(deviceRegistry));
        console.log("LensMintERC1155:", address(lensMint));
        console.log("Base URI:", baseURI);
    }
}

