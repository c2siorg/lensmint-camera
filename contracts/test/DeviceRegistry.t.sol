// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/DeviceRegistry.sol";

contract DeviceRegistryTest is Test {
    DeviceRegistry public registry;

    address public deployer;
    address public deviceAddr1 = address(0xD001);
    address public deviceAddr2 = address(0xD002);
    address public stranger = address(0xBAD);

    event DeviceRegistered(
        address indexed deviceAddress,
        string deviceId,
        string publicKey,
        address indexed registeredBy
    );
    event DeviceUpdated(address indexed deviceAddress, string deviceId, bool isActive);
    event DeviceDeactivated(address indexed deviceAddress, string deviceId);

    function setUp() public {
        deployer = address(this);
        registry = new DeviceRegistry();
    }

    // ─── Registration: happy path ────────────────────────────

    function testRegisterDevice() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        DeviceRegistry.DeviceInfo memory info = registry.getDevice(deviceAddr1);
        assertEq(info.deviceAddress, deviceAddr1);
        assertEq(info.publicKey, "0x04pub1");
        assertEq(info.deviceId, "dev-001");
        assertEq(info.cameraId, "cam-001");
        assertEq(info.model, "RPi4");
        assertEq(info.firmwareVersion, "1.0.0");
        assertTrue(info.isActive);
        assertEq(info.registeredBy, deployer);
        assertGt(info.registrationTime, 0);
    }

    function testRegisterDeviceEmitsEvent() public {
        vm.expectEmit(true, true, false, true);
        emit DeviceRegistered(deviceAddr1, "dev-001", "0x04pub1", deployer);
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");
    }

    function testRegisterMultipleDevices() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");
        registry.registerDevice(deviceAddr2, "0x04pub2", "dev-002", "cam-002", "RPi5", "2.0.0");

        assertEq(registry.getTotalDevices(), 2);

        address[] memory all = registry.getAllDevices();
        assertEq(all.length, 2);
        assertEq(all[0], deviceAddr1);
        assertEq(all[1], deviceAddr2);
    }

    function testNewDeviceIsActiveByDefault() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");
        assertTrue(registry.isDeviceActive(deviceAddr1));
    }

    // ─── Registration: revert paths ──────────────────────────

    function testRevertRegisterZeroAddress() public {
        vm.expectRevert("Invalid device address");
        registry.registerDevice(address(0), "0x04pub", "dev-001", "cam-001", "RPi4", "1.0.0");
    }

    function testRevertRegisterEmptyPublicKey() public {
        vm.expectRevert("Public key required");
        registry.registerDevice(deviceAddr1, "", "dev-001", "cam-001", "RPi4", "1.0.0");
    }

    function testRevertRegisterEmptyDeviceId() public {
        vm.expectRevert("Device ID required");
        registry.registerDevice(deviceAddr1, "0x04pub", "", "cam-001", "RPi4", "1.0.0");
    }

    function testRevertRegisterEmptyCameraId() public {
        vm.expectRevert("Camera ID required");
        registry.registerDevice(deviceAddr1, "0x04pub", "dev-001", "", "RPi4", "1.0.0");
    }

    function testRevertRegisterDuplicateAddress() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        vm.expectRevert("Device already registered");
        registry.registerDevice(deviceAddr1, "0x04pub2", "dev-002", "cam-002", "RPi4", "1.0.0");
    }

    function testRevertRegisterDuplicateDeviceId() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        vm.expectRevert("Device ID already in use");
        registry.registerDevice(deviceAddr2, "0x04pub2", "dev-001", "cam-002", "RPi4", "1.0.0");
    }

    // ─── Update: happy path ──────────────────────────────────

    function testUpdateDeviceByRegistrar() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        // deployer is registeredBy, so can update
        registry.updateDevice(deviceAddr1, "2.0.0", false);

        DeviceRegistry.DeviceInfo memory info = registry.getDevice(deviceAddr1);
        assertEq(info.firmwareVersion, "2.0.0");
        assertFalse(info.isActive);
    }

    function testUpdateDeviceBySelf() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        // device itself can also update
        vm.prank(deviceAddr1);
        registry.updateDevice(deviceAddr1, "2.0.0", true);

        DeviceRegistry.DeviceInfo memory info = registry.getDevice(deviceAddr1);
        assertEq(info.firmwareVersion, "2.0.0");
        assertTrue(info.isActive);
    }

    function testUpdateDeviceEmitsEvent() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        vm.expectEmit(true, false, false, true);
        emit DeviceUpdated(deviceAddr1, "dev-001", false);
        registry.updateDevice(deviceAddr1, "2.0.0", false);
    }

    // ─── Update: revert paths ────────────────────────────────

    function testRevertUpdateUnregisteredDevice() public {
        vm.expectRevert("Device not registered");
        registry.updateDevice(deviceAddr1, "2.0.0", true);
    }

    function testRevertUpdateByStranger() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        vm.prank(stranger);
        vm.expectRevert("Not authorized");
        registry.updateDevice(deviceAddr1, "2.0.0", false);
    }

    // ─── Deactivate: happy path ──────────────────────────────

    function testDeactivateDeviceByRegistrar() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");
        assertTrue(registry.isDeviceActive(deviceAddr1));

        registry.deactivateDevice(deviceAddr1);
        assertFalse(registry.isDeviceActive(deviceAddr1));
    }

    function testDeactivateDeviceBySelf() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        vm.prank(deviceAddr1);
        registry.deactivateDevice(deviceAddr1);
        assertFalse(registry.isDeviceActive(deviceAddr1));
    }

    function testDeactivateDeviceEmitsEvent() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        vm.expectEmit(true, false, false, true);
        emit DeviceDeactivated(deviceAddr1, "dev-001");
        registry.deactivateDevice(deviceAddr1);
    }

    // ─── Deactivate: revert paths ────────────────────────────

    function testRevertDeactivateUnregistered() public {
        vm.expectRevert("Device not registered");
        registry.deactivateDevice(deviceAddr1);
    }

    function testRevertDeactivateByStranger() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");

        vm.prank(stranger);
        vm.expectRevert("Not authorized");
        registry.deactivateDevice(deviceAddr1);
    }

    // ─── Query functions ─────────────────────────────────────

    function testGetDeviceByDeviceId() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");
        assertEq(registry.getDeviceByDeviceId("dev-001"), deviceAddr1);
    }

    function testGetDeviceByDeviceIdUnknown() public view {
        assertEq(registry.getDeviceByDeviceId("nonexistent"), address(0));
    }

    function testIsDeviceActiveReturnsFalseForUnregistered() public view {
        assertFalse(registry.isDeviceActive(address(0x9999)));
    }

    function testGetTotalDevicesInitiallyZero() public view {
        assertEq(registry.getTotalDevices(), 0);
    }

    function testGetAllDevicesInitiallyEmpty() public view {
        address[] memory all = registry.getAllDevices();
        assertEq(all.length, 0);
    }

    function testGetDeviceReturnsEmptyForUnregistered() public view {
        DeviceRegistry.DeviceInfo memory info = registry.getDevice(address(0x9999));
        assertEq(info.deviceAddress, address(0));
        assertEq(bytes(info.deviceId).length, 0);
    }

    // ─── Reactivation ────────────────────────────────────────

    function testReactivateAfterDeactivate() public {
        registry.registerDevice(deviceAddr1, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");
        registry.deactivateDevice(deviceAddr1);
        assertFalse(registry.isDeviceActive(deviceAddr1));

        // Reactivate via updateDevice
        registry.updateDevice(deviceAddr1, "1.0.0", true);
        assertTrue(registry.isDeviceActive(deviceAddr1));
    }
}
