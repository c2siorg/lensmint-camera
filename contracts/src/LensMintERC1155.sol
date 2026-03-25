// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC1155/ERC1155.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/Strings.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import "./DeviceRegistry.sol";

contract LensMintERC1155 is ERC1155, Ownable, EIP712 {
    using Strings for uint256;

    // EIP-712 typehash for the MintOriginal struct
    bytes32 public constant MINT_ORIGINAL_TYPEHASH = keccak256(
        "MintOriginal(address to,string ipfsHash,bytes32 imageHash,uint256 maxEditions,uint256 nonce)"
    );

    // Reference to device registry for validation
    DeviceRegistry public deviceRegistry;
    string public baseURI;
    mapping(uint256 => TokenMetadata) public tokenMetadata;
    mapping(uint256 => uint256) public editionCount;
    uint256 public totalTokens;

    // Replay protection: prevents the same image from being minted twice
    mapping(bytes32 => bool) public usedImageHashes;

    // Per-device nonce for additional replay protection
    mapping(address => uint256) public nonces;

    struct TokenMetadata {
        address deviceAddress;
        string deviceId;
        string ipfsHash;
        bytes32 imageHash;
        bytes signature;       // stored as raw bytes (65-byte r,s,v)
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
    ) ERC1155(_baseURI) Ownable(msg.sender) EIP712("LensMintERC1155", "1") {
        require(_deviceRegistry != address(0), "Invalid device registry");
        deviceRegistry = DeviceRegistry(_deviceRegistry);
        baseURI = _baseURI;
    }

    /**
     * @notice Mint an original photo NFT with on-chain EIP-712 signature verification.
     * @dev The caller (msg.sender) must be a registered, active device.
     *      The signature must be a valid EIP-712 signature over the mint params,
     *      signed by msg.sender's private key. Each imageHash can only be used once.
     * @param _to           Recipient of the minted NFT (owner wallet)
     * @param _ipfsHash     Filecoin/IPFS CID of the image
     * @param _imageHash    SHA-256 hash of the raw image bytes
     * @param _maxEditions  Maximum editions allowed (0 = unlimited)
     * @param v             ECDSA recovery id
     * @param r             ECDSA signature component
     * @param s             ECDSA signature component
     */
    function mintOriginal(
        address _to,
        string memory _ipfsHash,
        bytes32 _imageHash,
        uint256 _maxEditions,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external returns (uint256) {
        // 1. Verify device is registered and active
        require(
            deviceRegistry.isDeviceActive(msg.sender),
            "Device not registered or inactive"
        );

        // 2. Prevent replay: same image cannot be minted twice
        require(!usedImageHashes[_imageHash], "Image already minted");

        // 3. Reconstruct the EIP-712 struct hash and verify signature
        uint256 currentNonce = nonces[msg.sender];
        bytes32 structHash = keccak256(
            abi.encode(
                MINT_ORIGINAL_TYPEHASH,
                _to,
                keccak256(bytes(_ipfsHash)),
                _imageHash,
                _maxEditions,
                currentNonce
            )
        );
        bytes32 digest = _hashTypedDataV4(structHash);
        address signer = ECDSA.recover(digest, v, r, s);
        require(signer == msg.sender, "Invalid signature");

        // 4. Increment nonce and mark image hash as used
        nonces[msg.sender] = currentNonce + 1;
        usedImageHashes[_imageHash] = true;

        // 5. Create token
        DeviceRegistry.DeviceInfo memory device = deviceRegistry.getDevice(msg.sender);
        uint256 tokenId = ++totalTokens;

        TokenMetadata memory metadata = TokenMetadata({
            deviceAddress: msg.sender,
            deviceId: device.deviceId,
            ipfsHash: _ipfsHash,
            imageHash: _imageHash,
            signature: abi.encodePacked(r, s, v),
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

    /// @notice Returns the EIP-712 domain separator (useful for off-chain signing)
    function domainSeparator() external view returns (bytes32) {
        return _domainSeparatorV4();
    }
}
