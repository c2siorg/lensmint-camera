// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console} from "forge-std/Script.sol";
import {DeviceRegistry} from "../src/DeviceRegistry.sol";

contract RegisterDeviceScript is Script {
    function run() external {
        address deployer = msg.sender;
        
        address deviceRegistryAddress = vm.envAddress("DEVICE_REGISTRY_ADDRESS");
        DeviceRegistry deviceRegistry = DeviceRegistry(deviceRegistryAddress);

        address deviceAddress = vm.envAddress("DEVICE_ADDRESS");
        string memory publicKey = vm.envString("DEVICE_PUBLIC_KEY");
        string memory deviceId = vm.envString("DEVICE_ID");
        string memory cameraId = vm.envString("CAMERA_ID");
        string memory model = vm.envString("DEVICE_MODEL");
        string memory firmwareVersion = vm.envString("FIRMWARE_VERSION");

        console.log("Registering device...");
        console.log("Device Address:", deviceAddress);
        console.log("Device ID:", deviceId);
        console.log("Camera ID:", cameraId);
        console.log("Model:", model);

        vm.startBroadcast();

        deviceRegistry.registerDevice(
            deviceAddress,
            publicKey,
            deviceId,
            cameraId,
            model,
            firmwareVersion
        );

        vm.stopBroadcast();

        console.log("Device registered successfully!");
    }
}

