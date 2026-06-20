// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {DeviceRegistry} from "../src/DeviceRegistry.sol";
import {LensMintERC1155} from "../src/LensMintERC1155.sol";

contract DeployAndVerifyScript is Script {
    function run() external {
        address deployer = msg.sender;
        
        string memory network = vm.envOr("NETWORK", string("unknown"));
        string memory baseURI = vm.envOr("BASE_URI", string("https://ipfs.io/ipfs/"));
        
        console.log("=== Deployment Configuration ===");
        console.log("Network:", network);
        console.log("Deployer address:", deployer);
        console.log("Deployer balance:", deployer.balance);
        console.log("Base URI:", baseURI);
        console.log("");

        vm.startBroadcast();

        console.log("Deploying DeviceRegistry...");
        DeviceRegistry deviceRegistry = new DeviceRegistry();
        address deviceRegistryAddress = address(deviceRegistry);
        console.log("DeviceRegistry deployed at:", deviceRegistryAddress);

        console.log("Deploying LensMintERC1155...");
        LensMintERC1155 lensMint = new LensMintERC1155(
            deviceRegistryAddress,
            baseURI
        );
        address lensMintAddress = address(lensMint);
        console.log("LensMintERC1155 deployed at:", lensMintAddress);

        vm.stopBroadcast();

        console.log("\n=== Deployment Summary ===");
        console.log("DeviceRegistry:", deviceRegistryAddress);
        console.log("LensMintERC1155:", lensMintAddress);
        console.log("Base URI:", baseURI);
    }
}

