// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title DeviceRegistry
 * @dev Manages registration and activation of camera devices
 */
contract DeviceRegistry {
    struct DeviceInfo {
        address deviceAddress;
        string publicKey;
        string deviceId;
        string cameraId;
        string model;
        string firmwareVersion;
        uint256 registrationTime;
        bool isActive;
        address registeredBy;
    }

    mapping(address => DeviceInfo) public devices;
    mapping(string => address) public deviceIdToAddress;
    address[] public registeredDevices;

    event DeviceRegistered(
        address indexed deviceAddress,
        string deviceId,
        string publicKey,
        address indexed registeredBy
    );

    event DeviceUpdated(
        address indexed deviceAddress,
        string deviceId,
        bool isActive
    );

    event DeviceDeactivated(address indexed deviceAddress, string deviceId);
    function registerDevice(
        address _deviceAddress,
        string memory _publicKey,
        string memory _deviceId,
        string memory _cameraId,
        string memory _model,
        string memory _firmwareVersion
    ) external {
        require(_deviceAddress != address(0), "Invalid device address");
        require(bytes(_publicKey).length > 0, "Public key required");
        require(bytes(_deviceId).length > 0, "Device ID required");
        require(bytes(_cameraId).length > 0, "Camera ID required");
        require(
            devices[_deviceAddress].deviceAddress == address(0),
            "Device already registered"
        );
        require(
            deviceIdToAddress[_deviceId] == address(0),
            "Device ID already in use"
        );

        DeviceInfo memory newDevice = DeviceInfo({
            deviceAddress: _deviceAddress,
            publicKey: _publicKey,
            deviceId: _deviceId,
            cameraId: _cameraId,
            model: _model,
            firmwareVersion: _firmwareVersion,
            registrationTime: block.timestamp,
            isActive: true,
            registeredBy: msg.sender
        });

        devices[_deviceAddress] = newDevice;
        deviceIdToAddress[_deviceId] = _deviceAddress;
        registeredDevices.push(_deviceAddress);

        emit DeviceRegistered(
            _deviceAddress,
            _deviceId,
            _publicKey,
            msg.sender
        );
    }

    function updateDevice(
        address _deviceAddress,
        string memory _firmwareVersion,
        bool _isActive
    ) external {
        DeviceInfo storage device = devices[_deviceAddress];
        require(device.deviceAddress != address(0), "Device not registered");
        require(
            msg.sender == device.registeredBy || msg.sender == _deviceAddress,
            "Not authorized"
        );

        device.firmwareVersion = _firmwareVersion;
        device.isActive = _isActive;

        emit DeviceUpdated(_deviceAddress, device.deviceId, _isActive);
    }

    function deactivateDevice(address _deviceAddress) external {
        DeviceInfo storage device = devices[_deviceAddress];
        require(device.deviceAddress != address(0), "Device not registered");
        require(
            msg.sender == device.registeredBy || msg.sender == _deviceAddress,
            "Not authorized"
        );

        device.isActive = false;

        emit DeviceDeactivated(_deviceAddress, device.deviceId);
    }

    function getDevice(
        address _deviceAddress
    ) external view returns (DeviceInfo memory) {
        return devices[_deviceAddress];
    }

    function getDeviceByDeviceId(
        string memory _deviceId
    ) external view returns (address) {
        return deviceIdToAddress[_deviceId];
    }

    function isDeviceActive(
        address _deviceAddress
    ) external view returns (bool) {
        DeviceInfo memory device = devices[_deviceAddress];
        return device.deviceAddress != address(0) && device.isActive;
    }

    function getTotalDevices() external view returns (uint256) {
        return registeredDevices.length;
    }

    function getAllDevices() external view returns (address[] memory) {
        return registeredDevices;
    }

    /**
     * @dev Returns a paginated list of registered devices to avoid gas limit issues.
     * @param cursor The starting index (0-indexed).
     * @param count The number of device addresses to return.
     */
    function getDevicesPaginated(
        uint256 cursor,
        uint256 count
    ) external view returns (address[] memory) {
        uint256 total = registeredDevices.length;
        if (cursor >= total || count == 0) {
            return new address[](0);
        }

        uint256 length = count;
        if (cursor + count > total) {
            length = total - cursor;
        }

        address[] memory devicesList = new address[](length);
        for (uint256 i = 0; i < length; i++) {
            devicesList[i] = registeredDevices[cursor + i];
        }

        return devicesList;
    }
}
