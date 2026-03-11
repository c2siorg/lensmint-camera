// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC1155/ERC1155.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/Strings.sol";
import "./DeviceRegistry.sol";

contract LensMintERC1155 is ERC1155, Ownable {
    using Strings for uint256;

    // Reference to device registry for validation
    DeviceRegistry public deviceRegistry;
    string public baseURI;
    mapping(uint256 => TokenMetadata) public tokenMetadata;
    mapping(uint256 => uint256) public editionCount;
    uint256 public totalTokens;

    struct TokenMetadata {
        address deviceAddress;
        string deviceId;
        string ipfsHash;
        string imageHash;
        string signature;
        uint256 timestamp;
        uint256 maxEditions;
        bool isOriginal;
        uint256 originalTokenId;
    }

    event TokenMinted(
        uint256 indexed tokenId,
        address indexed deviceAddress,
        string deviceId,
        string ipfsHash,
        bool isOriginal
    );

    event EditionMinted(
        uint256 indexed tokenId,
        uint256 indexed originalTokenId,
        address indexed to
    );

    event BaseURIUpdated(string newBaseURI);
    constructor(
        address _deviceRegistry,
        string memory _baseURI
    ) ERC1155(_baseURI) Ownable(msg.sender) {
        require(_deviceRegistry != address(0), "Invalid device registry");
        deviceRegistry = DeviceRegistry(_deviceRegistry);
        baseURI = _baseURI;
    }

    function _hexStringToBytes(string memory s) internal pure returns (bytes memory) {
        bytes memory ss = bytes(s);
        uint offset = 0;
        if (ss.length >= 2 && ss[0] == '0' && (ss[1] == 'x' || ss[1] == 'X')) {
            offset = 2;
        }
        bytes memory r = new bytes((ss.length - offset) / 2);
        for (uint i = 0; i < r.length; i++) {
            uint16 charCode1 = uint8(ss[offset + i*2]);
            uint16 charCode2 = uint8(ss[offset + i*2 + 1]);
            
            uint8 b1 = uint8(charCode1 >= 97 ? charCode1 - 87 : charCode1 >= 65 ? charCode1 - 55 : charCode1 - 48);
            uint8 b2 = uint8(charCode2 >= 97 ? charCode2 - 87 : charCode2 >= 65 ? charCode2 - 55 : charCode2 - 48);
            
            r[i] = bytes1((b1 << 4) | b2);
        }
        return r;
    }

    function _verifySignature(string memory _imageHash, string memory _signature, address expectedSigner) internal pure returns (bool) {
        bytes memory sig = _hexStringToBytes(_signature);
        bytes memory hashBytes = _hexStringToBytes(_imageHash);
        require(hashBytes.length == 32, "Invalid hash length");
        bytes32 messageHash;
        assembly {
            messageHash := mload(add(hashBytes, 32))
        }

        if (sig.length == 65) {
            bytes32 r;
            bytes32 s;
            uint8 v;
            assembly {
                r := mload(add(sig, 32))
                s := mload(add(sig, 64))
                v := byte(0, mload(add(sig, 96)))
            }
            if (v < 27) v += 27;
            return ecrecover(messageHash, v, r, s) == expectedSigner;
        } else if (sig.length == 64) {
            bytes32 r;
            bytes32 s;
            assembly {
                r := mload(add(sig, 32))
                s := mload(add(sig, 64))
            }
            if (ecrecover(messageHash, 27, r, s) == expectedSigner) return true;
            if (ecrecover(messageHash, 28, r, s) == expectedSigner) return true;
            return false;
        }
        return false;
    }

    function mintOriginal(
        address _to,
        string memory _ipfsHash,
        string memory _imageHash,
        string memory _signature,
        uint256 _maxEditions
    ) external returns (uint256) {
        require(
            deviceRegistry.isDeviceActive(msg.sender),
            "Device not registered or inactive"
        );
        require(
            _verifySignature(_imageHash, _signature, msg.sender),
            "Invalid signature"
        );

        DeviceRegistry.DeviceInfo memory device = deviceRegistry.getDevice(msg.sender);
        uint256 tokenId = ++totalTokens;

        TokenMetadata memory metadata = TokenMetadata({
            deviceAddress: msg.sender,
            deviceId: device.deviceId,
            ipfsHash: _ipfsHash,
            imageHash: _imageHash,
            signature: _signature,
            timestamp: block.timestamp,
            maxEditions: _maxEditions,
            isOriginal: true,
            originalTokenId: tokenId
        });

        tokenMetadata[tokenId] = metadata;
        editionCount[tokenId] = 1;

        _mint(_to, tokenId, 1, "");
        emit TokenMinted(tokenId, msg.sender, device.deviceId, _ipfsHash, true);

        return tokenId;
    }

    function mintEdition(
        address _to,
        uint256 _originalTokenId
    ) external returns (uint256) {
        TokenMetadata memory original = tokenMetadata[_originalTokenId];
        require(original.deviceAddress != address(0), "Token does not exist");
        require(original.isOriginal, "Token is not an original");
        require(
            original.maxEditions == 0 || editionCount[_originalTokenId] < original.maxEditions,
            "Max editions reached"
        );

        uint256 tokenId = ++totalTokens;
        editionCount[_originalTokenId]++;

        TokenMetadata memory edition = TokenMetadata({
            deviceAddress: original.deviceAddress,
            deviceId: original.deviceId,
            ipfsHash: original.ipfsHash,
            imageHash: original.imageHash,
            signature: original.signature,
            timestamp: original.timestamp,
            maxEditions: original.maxEditions,
            isOriginal: false,
            originalTokenId: _originalTokenId
        });

        tokenMetadata[tokenId] = edition;

        _mint(_to, tokenId, 1, "");

        emit EditionMinted(tokenId, _originalTokenId, _to);

        return tokenId;
    }

    function batchMintEditions(
        address _to,
        uint256 _originalTokenId,
        uint256 _quantity
    ) external returns (uint256[] memory) {
        TokenMetadata memory original = tokenMetadata[_originalTokenId];
        require(original.deviceAddress != address(0), "Token does not exist");
        require(original.isOriginal, "Token is not an original");
        require(_quantity > 0, "Quantity must be > 0");

        uint256[] memory tokenIds = new uint256[](_quantity);

        for (uint256 i = 0; i < _quantity; i++) {
            require(
                original.maxEditions == 0 || editionCount[_originalTokenId] < original.maxEditions,
                "Max editions reached"
            );

            uint256 tokenId = ++totalTokens;
            editionCount[_originalTokenId]++;
            tokenIds[i] = tokenId;

            TokenMetadata memory edition = TokenMetadata({
                deviceAddress: original.deviceAddress,
                deviceId: original.deviceId,
                ipfsHash: original.ipfsHash,
                imageHash: original.imageHash,
                signature: original.signature,
                timestamp: original.timestamp,
                maxEditions: original.maxEditions,
                isOriginal: false,
                originalTokenId: _originalTokenId
            });

            tokenMetadata[tokenId] = edition;
            _mint(_to, tokenId, 1, "");

            emit EditionMinted(tokenId, _originalTokenId, _to);
        }

        return tokenIds;
    }

    function getTokenMetadata(uint256 _tokenId) external view returns (TokenMetadata memory) {
        return tokenMetadata[_tokenId];
    }

    function getEditionCount(uint256 _originalTokenId) external view returns (uint256) {
        return editionCount[_originalTokenId];
    }

    function setBaseURI(string memory _newBaseURI) external onlyOwner {
        baseURI = _newBaseURI;
        _setURI(_newBaseURI);
        emit BaseURIUpdated(_newBaseURI);
    }

    function uri(uint256 _tokenId) public view override returns (string memory) {
        require(tokenMetadata[_tokenId].deviceAddress != address(0), "Token does not exist");
        return string(abi.encodePacked(baseURI, _tokenId.toString()));
    }

    function canDeviceMint(address _deviceAddress) external view returns (bool) {
        return deviceRegistry.isDeviceActive(_deviceAddress);
    }
}

