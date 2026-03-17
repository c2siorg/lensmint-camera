// SPDX-License-Identifier: MIT
pragma solidity ^0.8.31;

import "forge-std/Test.sol";
import "../../src/DeviceRegistry.sol";

contract DeviceRegistryTest is Test {
    DeviceRegistry private registry;

    // Mirror contract events so we can use vm.expectEmit with (emitter)
    event DeviceRegistered(
        address indexed deviceAddress,
        string deviceId,
        string publicKey,
        address indexed registeredBy
    );
    event DeviceUpdated(address indexed deviceAddress, string deviceId, bool isActive);
    event DeviceDeactivated(address indexed deviceAddress, string deviceId);

    address private registrar;       // address that registers devices
    address private deviceAddress1;  // first device wallet
    address private deviceAddress2;  // second device wallet
    address private attacker;        // unauthorized actor

    string private constant PUBLIC_KEY =
        "0x04e3b0b9c3a4f0b01e3fbbef6e3b0b9c3a4f0b01e3fbbef6e3b0b9c3a4f0b01e3";
    string private constant DEVICE_ID_1 = "device-001";
    string private constant DEVICE_ID_2 = "device-002";
    string private constant CAMERA_ID_1 = "camera-001";
    string private constant CAMERA_ID_2 = "camera-002";
    string private constant MODEL = "LensMint Pi";
    string private constant FW_V1 = "1.0.0";  // firmware version 1.0.0
    string private constant FW_V2 = "1.1.0";  // firmware version 1.1.0

    function setUp() public {
        registry = new DeviceRegistry();

        registrar = address(this);
        deviceAddress1 = address(0xD1);
        deviceAddress2 = address(0xD2);
        attacker = address(0xBAD);
    }

    // -------------------------------------------------------------------------
    // registerDevice
    // -------------------------------------------------------------------------
    
    function test_RegisterDevice_Success() public {
        bool ok = registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        assertTrue(ok, "registerDevice should return true");

        DeviceRegistry.DeviceInfo memory info = registry.getDevice(
            deviceAddress1
        );

        assertEq(info.deviceAddress, deviceAddress1, "deviceAddress stored");
        assertEq(info.publicKey, PUBLIC_KEY, "publicKey stored");
        assertEq(info.deviceId, DEVICE_ID_1, "deviceId stored");
        assertEq(info.cameraId, CAMERA_ID_1, "cameraId stored");
        assertEq(info.model, MODEL, "model stored");
        assertEq(info.firmwareVersion, FW_V1, "firmware stored");
        assertEq(info.registeredBy, registrar, "registeredBy is caller");
        assertEq(info.registrationTime, block.timestamp, "registrationTime is current timestamp");
        assertTrue(info.isActive, "device should start active");

        address deviceAddressById = registry.getDeviceByDeviceId(DEVICE_ID_1);
        assertEq(deviceAddressById, deviceAddress1, "deviceIdToAddress mapping set");
        
        uint256 totalDevices = registry.getTotalDevices();
        assertEq(totalDevices, 1, "one device registered");

        address[] memory all = registry.getAllDevices();
        assertEq(all.length, 1, "one element in array");
        assertEq(all[0], deviceAddress1, "array contains device");
    }

    function test_RegisterDevice_Revert_InvalidDeviceAddress() public {
        vm.expectRevert(DeviceRegistry.InvalidDeviceAddress.selector);
        registry.registerDevice(
            address(0),
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );
    }

    function test_RegisterDevice_Revert_PublicKeyRequired() public {
        vm.expectRevert(DeviceRegistry.PublicKeyRequired.selector);
        registry.registerDevice(
            deviceAddress1,
            "",
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );
    }

    function test_RegisterDevice_Revert_DeviceIdRequired() public {
        vm.expectRevert(DeviceRegistry.DeviceIdRequired.selector);
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            "",
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );
    }

    function test_RegisterDevice_Revert_CameraIdRequired() public {
        vm.expectRevert(DeviceRegistry.CameraIdRequired.selector);
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            "",
            MODEL,
            FW_V1
        );
    }

    function test_RegisterDevice_Revert_DeviceAlreadyRegistered() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.expectRevert(DeviceRegistry.DeviceAlreadyRegistered.selector);
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_2,
            CAMERA_ID_2,
            MODEL,
            FW_V1
        );
    }

    function test_RegisterDevice_Revert_DeviceIdAlreadyInUse() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.expectRevert(DeviceRegistry.DeviceIdAlreadyInUse.selector);
        registry.registerDevice(
            deviceAddress2,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_2,
            MODEL,
            FW_V1
        );
    }

    // -------------------------------------------------------------------------
    // updateDevice
    // -------------------------------------------------------------------------

    function test_UpdateDevice_Success_ByRegistrar() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        bool ok = registry.updateDevice(
            deviceAddress1,
            FW_V2,
            false
        );
        assertTrue(ok, "updateDevice should return true");

        DeviceRegistry.DeviceInfo memory info = registry.getDevice(
            deviceAddress1
        );
        assertEq(info.firmwareVersion, FW_V2, "firmware updated");
        assertFalse(info.isActive, "isActive updated");
    }

    function test_UpdateDevice_Success_ByDeviceAddress() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.prank(deviceAddress1);
        bool ok = registry.updateDevice(
            deviceAddress1,
            FW_V2,
            true
        );
        assertTrue(ok, "updateDevice should return true for device");

        DeviceRegistry.DeviceInfo memory info = registry.getDevice(
            deviceAddress1
        );
        assertEq(info.firmwareVersion, FW_V2, "firmware updated");
        assertTrue(info.isActive, "isActive updated");
    }

    function test_UpdateDevice_Revert_DeviceNotRegistered() public {
        vm.expectRevert(DeviceRegistry.DeviceNotRegistered.selector);
        registry.updateDevice(deviceAddress1, FW_V2, true);
    }

    function test_UpdateDevice_Revert_FirmwareVersionRequired() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.expectRevert(DeviceRegistry.FirmwareVersionRequired.selector);
        registry.updateDevice(deviceAddress1, "", true);
    }

    function test_UpdateDevice_Revert_NotAuthorizedToUpdate() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.prank(attacker);
        vm.expectRevert(DeviceRegistry.NotAuthorizedToUpdate.selector);
        registry.updateDevice(deviceAddress1, FW_V2, true);
    }

    // -------------------------------------------------------------------------
    // deactivateDevice
    // -------------------------------------------------------------------------

    function test_DeactivateDevice_Success_ByRegistrar() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        bool ok = registry.deactivateDevice(deviceAddress1);
        assertTrue(ok, "deactivateDevice should return true");

        bool active = registry.isDeviceActive(deviceAddress1);
        assertFalse(active, "device should be inactive");
    }

    function test_DeactivateDevice_Success_ByDeviceAddress() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.prank(deviceAddress1);
        bool ok = registry.deactivateDevice(deviceAddress1);
        assertTrue(ok, "deactivateDevice should return true for device");

        bool active = registry.isDeviceActive(deviceAddress1);
        assertFalse(active, "device should be inactive");
    }

    function test_DeactivateDevice_Revert_DeviceNotRegistered() public {
        vm.expectRevert(DeviceRegistry.DeviceNotRegistered.selector);
        registry.deactivateDevice(deviceAddress1);
    }

    function test_DeactivateDevice_Revert_NotAuthorizedToDeactivate() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.prank(attacker);
        vm.expectRevert(DeviceRegistry.NotAuthorizedToDeactivate.selector);
        registry.deactivateDevice(deviceAddress1);
    }

    // -------------------------------------------------------------------------
    // isDeviceActive / getters
    // -------------------------------------------------------------------------

    function test_IsDeviceActive_FlagReflectsState() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        bool active = registry.isDeviceActive(deviceAddress1);
        assertTrue(active, "device should start active");

        registry.deactivateDevice(deviceAddress1);
        active = registry.isDeviceActive(deviceAddress1);
        assertFalse(active, "device should be inactive after deactivation");
    }

    function test_IsDeviceActive_UnregisteredReturnsFalse() public view {
        bool active = registry.isDeviceActive(deviceAddress1);
        assertFalse(active, "unregistered device should not be active");
    }

    // -------------------------------------------------------------------------
    // getDevice / getDeviceByDeviceId (edge cases)
    // -------------------------------------------------------------------------

    function test_GetDevice_UnregisteredReturnsEmptyStruct() public view {
        DeviceRegistry.DeviceInfo memory info = registry.getDevice(deviceAddress1);
        assertEq(info.deviceAddress, address(0), "unregistered device has zero address");
        assertEq(info.registeredBy, address(0), "unregistered has no registrar");
        assertEq(info.registrationTime, 0, "unregistered has zero time");
        assertFalse(info.isActive, "unregistered is not active");
        assertEq(bytes(info.deviceId).length, 0, "unregistered has empty deviceId");
    }

    function test_GetDeviceByDeviceId_UnknownReturnsZero() public view {
        address addr = registry.getDeviceByDeviceId("nonexistent-id");
        assertEq(addr, address(0), "unknown deviceId should return zero address");
    }

    // -------------------------------------------------------------------------
    // getTotalDevices / getAllDevices (multiple devices)
    // -------------------------------------------------------------------------

    function test_GetTotalDevices_MultipleDevices() public {
        assertEq(registry.getTotalDevices(), 0, "starts at zero");

        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );
        assertEq(registry.getTotalDevices(), 1, "one after first register");

        registry.registerDevice(
            deviceAddress2,
            PUBLIC_KEY,
            DEVICE_ID_2,
            CAMERA_ID_2,
            MODEL,
            FW_V1
        );
        assertEq(registry.getTotalDevices(), 2, "two after second register");
    }

    function test_GetAllDevices_OrderAndContent() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );
        registry.registerDevice(
            deviceAddress2,
            PUBLIC_KEY,
            DEVICE_ID_2,
            CAMERA_ID_2,
            MODEL,
            FW_V1
        );

        address[] memory all = registry.getAllDevices();
        assertEq(all.length, 2, "two devices");
        assertEq(all[0], deviceAddress1, "first is device1");
        assertEq(all[1], deviceAddress2, "second is device2");
    }

    // -------------------------------------------------------------------------
    // Re-activation (deactivate then updateDevice(..., true))
    // -------------------------------------------------------------------------

    function test_UpdateDevice_ReactivationAfterDeactivate() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );
        registry.deactivateDevice(deviceAddress1);
        assertFalse(registry.isDeviceActive(deviceAddress1), "device inactive");

        registry.updateDevice(deviceAddress1, FW_V2, true);
        assertTrue(registry.isDeviceActive(deviceAddress1), "device active again");
        DeviceRegistry.DeviceInfo memory info = registry.getDevice(deviceAddress1);
        assertEq(info.firmwareVersion, FW_V2, "firmware updated on reactivation");
    }

    // -------------------------------------------------------------------------
    // Events
    // -------------------------------------------------------------------------

    function test_RegisterDevice_EmitsDeviceRegistered() public {
        vm.expectEmit(true, true, true, true, address(registry));
        emit DeviceRegistered(deviceAddress1, DEVICE_ID_1, PUBLIC_KEY, registrar);
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );
    }

    function test_UpdateDevice_EmitsDeviceUpdated() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.expectEmit(true, true, true, true, address(registry));
        emit DeviceUpdated(deviceAddress1, DEVICE_ID_1, false);
        registry.updateDevice(deviceAddress1, FW_V2, false);
    }

    function test_DeactivateDevice_EmitsDeviceDeactivated() public {
        registry.registerDevice(
            deviceAddress1,
            PUBLIC_KEY,
            DEVICE_ID_1,
            CAMERA_ID_1,
            MODEL,
            FW_V1
        );

        vm.expectEmit(true, true, true, true, address(registry));
        emit DeviceDeactivated(deviceAddress1, DEVICE_ID_1);
        registry.deactivateDevice(deviceAddress1);
    }
}

