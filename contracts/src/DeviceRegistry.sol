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
    
    event DeviceDeactivated(
        address indexed deviceAddress,
        string deviceId
    );
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
        require(devices[_deviceAddress].deviceAddress == address(0), "Device already registered");
        require(deviceIdToAddress[_deviceId] == address(0), "Device ID already in use");

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

        emit DeviceRegistered(_deviceAddress, _deviceId, _publicKey, msg.sender);
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

    function getDevice(address _deviceAddress) external view returns (DeviceInfo memory) {
        return devices[_deviceAddress];
    }

    function getDeviceByDeviceId(string memory _deviceId) external view returns (address) {
        return deviceIdToAddress[_deviceId];
    }

    function isDeviceActive(address _deviceAddress) external view returns (bool) {
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
     * @notice Returns a paginated slice of the registered devices array.
     * @dev    Prevents out-of-gas on unbounded getAllDevices() calls at scale.
     * @param _offset Starting index in the registeredDevices array
     * @param _limit  Maximum number of addresses to return
     * @return page   The slice of device addresses
     * @return total  Total number of registered devices (for client-side paging)
     */
    function getDevicesPaginated(
        uint256 _offset,
        uint256 _limit
    ) external view returns (address[] memory page, uint256 total) {
        total = registeredDevices.length;

        if (_offset >= total || _limit == 0) {
            return (new address[](0), total);
        }

        uint256 end = _offset + _limit;
        if (end > total) {
            end = total;
        }

        uint256 size = end - _offset;
        page = new address[](size);
        for (uint256 i = 0; i < size; i++) {
            page[i] = registeredDevices[_offset + i];
        }
    }
}

