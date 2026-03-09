// SPDX-License-Identifier: MIT
pragma solidity ^0.8.31;

/*##############################################################################
#                                                                              #
#   __     _____  _    _  _____  __   __  _____  _    _  _____                 #
#  |  |   | ____|| \  | |/ ____||  \ /  ||__ __|| \  | ||_   _|                #
#  |  |   |  _|  | \ \| |\___ \ | |\ /| |  | |  | \ \| |  | |                  #
#  |  |__ | |___ | |\ | | ___) || | | | | _| |_ | |\ | |  | |                  #
#  |_____||_____||_| \|_||____/ |_|   |_||_____||_| \|_|  |_|                  #
#                                                                              #
#     DEVICE REGISTRY · ON-CHAIN TRUST LAYER                                   #
#                                                                              #
##############################################################################*/

/**
 * @title DeviceRegistry
 * @author LensMint
 * @notice Registry and status tracker for LensMint-enabled camera devices.
 * @dev Stores device identity and metadata, allows controlled updates and deactivation,
 * @dev and exposes view functions to query devices and overall registry state.
 */
contract DeviceRegistry { 

/////////////////////////
///   ERRORS          ///
/////////////////////////

///@dev Error to emit when the device address is invalid
error InvalidDeviceAddress();

///@dev Error to emit when the public key is required
error PublicKeyRequired();

///@dev Error to emit when the device ID is required
error DeviceIdRequired();

///@dev Error to emit when the camera ID is required
error CameraIdRequired();
    
///@dev Error to emit when the device is already registered
error DeviceAlreadyRegistered();
    
///@dev Error to emit when the device ID is already in use
error DeviceIdAlreadyInUse();
    
///@dev Error to emit when the firmware version is required
error FirmwareVersionRequired();
    
///@dev Error to emit when the device is not registered
error DeviceNotRegistered();
    
///@dev Error to emit when the not authorized to update the device
error NotAuthorizedToUpdate();
    
///@dev Error to emit when the not authorized to deactivate the device
error NotAuthorizedToDeactivate();
    
///@dev Error to emit when the not authorized to update the device
error NotAuthorizedToUpdate();
    
///@dev Error to emit when the not authorized to deactivate the device
error NotAuthorizedToDeactivate();
    

    /**
     * @dev Struct to store device information
     * @param deviceAddress The address of the device
     * @param publicKey The public key of the device
     * @param deviceId The ID of the device
     * @param cameraId The ID of the camera
     * @param model The model of the device
     * @param firmwareVersion The firmware version of the device
     * @param registrationTime The time the device was registered
     * @param isActive Whether the device is active
     * @param registeredBy The address that registered the device
     */
    struct DeviceInfo {
        address deviceAddress;
        address registeredBy;
        uint256 registrationTime;
        bool isActive;

        string publicKey;
        string deviceId;
        string cameraId;
        string model;
        string firmwareVersion;
    }

    ///@dev Mapping to store device information by device address
    mapping(address => DeviceInfo) public devices;

    ///@dev Mapping to store device address by device ID
    mapping(string => address) public deviceIdToAddress;

    ///@dev Array of registered device addresses
    address[] public registeredDevices;

    ///@dev Event to emit when a device is registered
    event DeviceRegistered(
        address indexed deviceAddress,
        string deviceId,
        string publicKey,
        address indexed registeredBy
    );

    ///@dev Event to emit when a device is updated
    event DeviceUpdated(
        address indexed deviceAddress,
        string deviceId,
        bool isActive
    );

    ///@dev Event to emit when a device is deactivated
    event DeviceDeactivated(
        address indexed deviceAddress,
        string deviceId
    );

    /**
     * @notice Function to register a device
     * @param _deviceAddress The address of the device
     * @param _publicKey The public key of the device
     * @param _deviceId The ID of the device
     * @param _cameraId The ID of the camera
     * @param _model The model of the device
     * @param _firmwareVersion The firmware version of the device
     * @return True if the device is registered successfully, false otherwise
     **/
    function registerDevice(
        address _deviceAddress,
        string memory _publicKey,
        string memory _deviceId,
        string memory _cameraId,
        string memory _model,
        string memory _firmwareVersion
    ) external returns (bool) {
        if (_deviceAddress == address(0)) {
            revert InvalidDeviceAddress();
        }
        if (bytes(_publicKey).length == 0) {
            revert PublicKeyRequired();
        }
        if (bytes(_deviceId).length == 0) {
            revert DeviceIdRequired();
        }
        if (bytes(_cameraId).length == 0) {
            revert CameraIdRequired();
        }
        if (devices[_deviceAddress].deviceAddress != address(0)) {
            revert DeviceAlreadyRegistered();
        }
        if (deviceIdToAddress[_deviceId] != address(0)) {
            revert DeviceIdAlreadyInUse();
        }

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

        return true;
    }

    /**
     * @notice Function to update a device
     * @param _deviceAddress The address of the device
     * @param _firmwareVersion The firmware version of the device
     * @param _isActive Whether the device is active
     * @return True if the device is updated successfully, false otherwise
     **/
    function updateDevice(
        address _deviceAddress,
        string memory _firmwareVersion,
        bool _isActive
    ) external returns (bool) {
        DeviceInfo storage device = devices[_deviceAddress];
        if (device.deviceAddress == address(0)) {
            revert DeviceNotRegistered();
        }   if (bytes(_firmwareVersion).length == 0) {
            revert FirmwareVersionRequired();
        }
        if (msg.sender != device.registeredBy && msg.sender != _deviceAddress) {
            revert NotAuthorized();
        }


        device.firmwareVersion = _firmwareVersion;
        device.isActive = _isActive;

        emit DeviceUpdated(_deviceAddress, device.deviceId, _isActive);
        return true;
    }

    function deactivateDevice(address _deviceAddress) external returns (bool) {
        DeviceInfo storage device = devices[_deviceAddress];
        if (device.deviceAddress == address(0)) {
            revert DeviceNotRegistered();
        }
        if (msg.sender != device.registeredBy && msg.sender != _deviceAddress) {
            revert NotAuthorizedToDeactivate();
        }

        device.isActive = false;

        emit DeviceDeactivated(_deviceAddress, device.deviceId);
        return true;
    }

    /**
     * @notice Function to get a device
     * @param _deviceAddress The address of the device
     * @return DeviceInfo memory The device information
     **/
    function getDevice(address _deviceAddress) external view returns (DeviceInfo memory) {
        return devices[_deviceAddress];
    }

    /**
     * @notice Function to get a device by device ID
     * @param _deviceId The ID of the device
     * @return address The address of the device
     **/
    function getDeviceByDeviceId(string memory _deviceId) external view returns (address) {
        return deviceIdToAddress[_deviceId];
    }

    /**
     * @notice Function to check if a device is active
     * @param _deviceAddress The address of the device
     * @return bool True if the device is active, false otherwise
     **/
    function isDeviceActive(address _deviceAddress) external view returns (bool) {
        DeviceInfo memory device = devices[_deviceAddress];
        return device.deviceAddress != address(0) && device.isActive;
    }

    /**
     * @notice Function to get the total number of devices
     * @return uint256 The total number of devices
     **/
    function getTotalDevices() external view returns (uint256) {
        return registeredDevices.length;
    }

    /**
     * @notice Function to get all devices
     * @return address[] memory The addresses of all devices
     **/
    function getAllDevices() external view returns (address[] memory) {
        return registeredDevices;
    }
}

