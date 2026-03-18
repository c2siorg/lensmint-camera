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
#     LENS MINT ERC1155 · ON-CHAIN TRUST LAYER                                 #
#                                                                              #
##############################################################################*/

import {ERC1155} from "@openzeppelin/contracts/token/ERC1155/ERC1155.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Strings} from "@openzeppelin/contracts/utils/Strings.sol";
import {DeviceRegistry} from "./DeviceRegistry.sol";

/**
 * @title LensMintERC1155
 * @author LensMint
 * @notice ERC1155 collection for camera-captured assets backed by registered devices.
 * @dev Only active devices in the DeviceRegistry can mint originals.
 * @dev Editions inherit metadata from an original token and are capped by maxEditions (0 = unlimited).
 */
contract LensMintERC1155 is ERC1155, Ownable {
    using Strings for uint256;

    /////////////////////////
    ///   ERRORS          ///
    /////////////////////////

    ///@dev Error to emit when the device is not registered or inactive
    error DeviceNotRegisteredOrInactive();

    ///@dev Error to emit when the token does not exist
    error TokenDoesNotExist();

    ///@dev Error to emit when the token is not an original
    error TokenIsNotAnOriginal();

    ///@dev Error to emit when the maximum number of editions has been reached
    error MaxEditionsReached();

    ///@dev Error to emit when the base URI is an empty string
    error BaseURIEmptyString();

    ///@dev Error to emit when the device registry address is zero
    error DeviceRegistryAddressIsZero();

    ///@dev Error to emit when the batch mint quantity is zero
    error QuantityMustBeGreaterThanZero();

    ///@dev Error to emit when the sender is not authorized to mint editions
    error NotAuthorizedToMintEditions();

    //////////////////////////
    ///   STATE VARIABLES  ///
    //////////////////////////

    ///@dev Reference to device registry for validation
    DeviceRegistry public deviceRegistry;

    ///@dev Base URI for the token metadata
    string public baseURI;

    ///@dev Mapping of token ID to token metadata
    mapping(uint256 => TokenMetadata) public tokenMetadata;

    ///@dev Mapping of original token ID to edition count
    mapping(uint256 => uint256) public editionCount;

    ///@dev Total number of tokens minted
    uint256 public totalTokens;

    struct TokenMetadata {
        // value types first for tighter packing
        address deviceAddress;
        bool isOriginal;
        uint256 timestamp;
        uint256 maxEditions;
        uint256 originalTokenId;
        // dynamic data (one slot each, points to separate storage)
        string deviceId;
        string ipfsHash;
        string imageHash;
        string signature;
    }

    /////////////////////////
    ///   EVENTS          ///
    /////////////////////////

    ///@dev Event to emit when a token is minted
    event TokenMinted(
        uint256 indexed tokenId, address indexed deviceAddress, string deviceId, string ipfsHash, bool isOriginal
    );

    ///@dev Event to emit when an edition is minted
    event EditionMinted(uint256 indexed tokenId, uint256 indexed originalTokenId, address indexed to);

    ///@dev Event to emit when the base URI is updated
    event BaseURIUpdated(string newBaseURI);

    /////////////////////////
    ///   FUNCTIONS       ///
    /////////////////////////

    ///@dev Constructor to initialize the contract
    ///@param _deviceRegistry The address of the device registry
    ///@param _baseURI The base URI for the token metadata
    constructor(address _deviceRegistry, string memory _baseURI) ERC1155(_baseURI) Ownable(msg.sender) {
        if (_deviceRegistry == address(0)) {
            revert DeviceRegistryAddressIsZero();
        }

        if (bytes(_baseURI).length == 0) {
            revert BaseURIEmptyString();
        }
        deviceRegistry = DeviceRegistry(_deviceRegistry);
        baseURI = _baseURI;
    }

    ///@notice Function to mint an original token
    ///@param _to The address to mint the token to
    ///@param _ipfsHash The IPFS hash of the token
    ///@param _imageHash The image hash of the token
    ///@param _signature The signature of the token
    ///@param _maxEditions The maximum number of editions for the token
    ///@return uint256 The token ID
    function mintOriginal(
        address _to,
        string memory _ipfsHash,
        string memory _imageHash,
        string memory _signature,
        uint256 _maxEditions
    ) external returns (uint256) {
        if (!deviceRegistry.isDeviceActive(msg.sender)) {
            revert DeviceNotRegisteredOrInactive();
        }

        DeviceRegistry.DeviceInfo memory device = deviceRegistry.getDevice(msg.sender);
        uint256 tokenId = ++totalTokens;

        TokenMetadata memory metadata = TokenMetadata({
            deviceAddress: msg.sender,
            isOriginal: true,
            timestamp: block.timestamp,
            maxEditions: _maxEditions,
            originalTokenId: tokenId,
            deviceId: device.deviceId,
            ipfsHash: _ipfsHash,
            imageHash: _imageHash,
            signature: _signature
        });

        tokenMetadata[tokenId] = metadata;
        editionCount[tokenId] = 1;

        _mint(_to, tokenId, 1, "");
        emit TokenMinted(tokenId, msg.sender, device.deviceId, _ipfsHash, true);

        return tokenId;
    }

    ///@notice Function to mint an edition token
    ///@param _to The address to mint the token to
    ///@param _originalTokenId The ID of the original token
    ///@return uint256 The token ID
    function mintEdition(address _to, uint256 _originalTokenId) external returns (uint256) {
        TokenMetadata memory original = tokenMetadata[_originalTokenId];
        if (original.deviceAddress == address(0)) {
            revert TokenDoesNotExist();
        }
        if (!original.isOriginal) {
            revert TokenIsNotAnOriginal();
        }
        if (original.maxEditions != 0 && editionCount[_originalTokenId] > original.maxEditions) {
            revert MaxEditionsReached();
        }

        uint256 tokenId = ++totalTokens;
        editionCount[_originalTokenId]++;

        TokenMetadata memory edition = TokenMetadata({
            deviceAddress: original.deviceAddress,
            isOriginal: false,
            timestamp: original.timestamp,
            maxEditions: original.maxEditions,
            originalTokenId: _originalTokenId,
            deviceId: original.deviceId,
            ipfsHash: original.ipfsHash,
            imageHash: original.imageHash,
            signature: original.signature
        });

        tokenMetadata[tokenId] = edition;

        _mint(_to, tokenId, 1, "");

        emit EditionMinted(tokenId, _originalTokenId, _to);

        return tokenId;
    }

    ///@notice Function to batch mint editions
    ///@param _to The address to mint the tokens to
    ///@param _originalTokenId The ID of the original token
    ///@param _quantity The quantity of tokens to mint
    ///@return uint256[] The token IDs
    function batchMintEditions(address _to, uint256 _originalTokenId, uint256 _quantity)
        external
        returns (uint256[] memory)
    {
        TokenMetadata memory original = tokenMetadata[_originalTokenId];
        if (original.deviceAddress == address(0)) {
            revert TokenDoesNotExist();
        }
        if (!original.isOriginal) {
            revert TokenIsNotAnOriginal();
        }
        if (msg.sender != original.deviceAddress && msg.sender != owner()) {
            revert NotAuthorizedToMintEditions();
        }
        if (_quantity == 0) {
            revert QuantityMustBeGreaterThanZero();
        }

        uint256[] memory tokenIds = new uint256[](_quantity);
    
        for (uint256 i = 0; i < _quantity; i++) {
            if (original.maxEditions != 0 && editionCount[_originalTokenId] > original.maxEditions) {
                revert MaxEditionsReached();
            }

            uint256 tokenId = ++totalTokens;
            editionCount[_originalTokenId]++;
            tokenIds[i] = tokenId;

            TokenMetadata memory edition = TokenMetadata({
                deviceAddress: original.deviceAddress,
                isOriginal: false,
                timestamp: original.timestamp,
                maxEditions: original.maxEditions,
                originalTokenId: _originalTokenId,
                deviceId: original.deviceId,
                ipfsHash: original.ipfsHash,
                imageHash: original.imageHash,
                signature: original.signature
            });

            tokenMetadata[tokenId] = edition;
            _mint(_to, tokenId, 1, "");

            emit EditionMinted(tokenId, _originalTokenId, _to);
        }

        return tokenIds;
    }

    ///@notice Function to get the metadata of a token
    ///@param _tokenId The ID of the token
    ///@return TokenMetadata memory The metadata of the token
    function getTokenMetadata(uint256 _tokenId) external view returns (TokenMetadata memory) {
        return tokenMetadata[_tokenId];
    }

    ///@notice Function to get the edition count of an original token
    ///@param _originalTokenId The ID of the original token
    ///@return uint256 The edition count
    function getEditionCount(uint256 _originalTokenId) external view returns (uint256) {
        return editionCount[_originalTokenId];
    }

    ///@notice Function to set the base URI
    ///@param _newBaseURI The new base URI
    function setBaseURI(string memory _newBaseURI) external onlyOwner {
        baseURI = _newBaseURI;
        _setURI(_newBaseURI);
        emit BaseURIUpdated(_newBaseURI);
    }

    ///@notice Function to get the URI of a token
    ///@param _tokenId The ID of the token
    ///@return string The URI of the token
    function uri(uint256 _tokenId) public view override returns (string memory) {
        if (tokenMetadata[_tokenId].deviceAddress == address(0)) {
            revert TokenDoesNotExist();
        }
        return string(abi.encodePacked(baseURI, _tokenId.toString()));
    }

    ///@notice Function to check if a device can mint
    ///@param _deviceAddress The address of the device
    ///@return bool True if the device can mint, false otherwise
    function canDeviceMint(address _deviceAddress) external view returns (bool) {
        return deviceRegistry.isDeviceActive(_deviceAddress);
    }
}

