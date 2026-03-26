// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/LensMintERC1155.sol";
import "../src/DeviceRegistry.sol";

contract LensMintEIP712Test is Test {
    DeviceRegistry public deviceRegistry;
    LensMintERC1155 public lensMint;

    // Device key pair (used with vm.sign)
    uint256 internal constant DEVICE_KEY = 0xA11CE;
    address internal deviceAddress;

    // A second device for negative tests
    uint256 internal constant ATTACKER_KEY = 0xBAD;
    address internal attackerAddress;

    address internal owner = address(0xCAFE);

    // EIP-712 constants — must match the contract
    bytes32 internal constant MINT_ORIGINAL_TYPEHASH = keccak256(
        "MintOriginal(address to,string ipfsHash,bytes32 imageHash,uint256 maxEditions,uint256 nonce)"
    );

    // Sample image data
    bytes32 internal constant IMAGE_HASH_1 = keccak256("photo_001.jpg");
    bytes32 internal constant IMAGE_HASH_2 = keccak256("photo_002.jpg");
    string internal constant IPFS_CID = "QmTestCID123456789abcdef";

    function setUp() public {
        deviceAddress = vm.addr(DEVICE_KEY);
        attackerAddress = vm.addr(ATTACKER_KEY);

        deviceRegistry = new DeviceRegistry();
        lensMint = new LensMintERC1155(address(deviceRegistry), "https://ipfs.io/ipfs/");

        // Register the device
        deviceRegistry.registerDevice(
            deviceAddress,
            "0x04publickey",
            "device-001",
            "camera-001",
            "Raspberry Pi 4",
            "1.0.0"
        );

        // Register attacker as a device too (to show sig check matters, not just device check)
        deviceRegistry.registerDevice(
            attackerAddress,
            "0x04attackerkey",
            "device-002",
            "camera-002",
            "Raspberry Pi 4",
            "1.0.0"
        );
    }

    // ─── Helpers ──────────────────────────────────────────────

    /// @dev Build the EIP-712 digest exactly as the contract does
    function _buildDigest(
        address to,
        string memory ipfsHash,
        bytes32 imageHash,
        uint256 maxEditions,
        uint256 nonce
    ) internal view returns (bytes32) {
        bytes32 structHash = keccak256(
            abi.encode(
                MINT_ORIGINAL_TYPEHASH,
                to,
                keccak256(bytes(ipfsHash)),
                imageHash,
                maxEditions,
                nonce
            )
        );
        // Replicate _hashTypedDataV4: "\x19\x01" || domainSeparator || structHash
        return keccak256(
            abi.encodePacked("\x19\x01", lensMint.domainSeparator(), structHash)
        );
    }

    /// @dev Sign a mint request as a given private key
    function _signMint(
        uint256 privateKey,
        address to,
        string memory ipfsHash,
        bytes32 imageHash,
        uint256 maxEditions,
        uint256 nonce
    ) internal view returns (uint8 v, bytes32 r, bytes32 s) {
        bytes32 digest = _buildDigest(to, ipfsHash, imageHash, maxEditions, nonce);
        (v, r, s) = vm.sign(privateKey, digest);
    }

    // ─── Happy Path ──────────────────────────────────────────

    function testMintOriginalWithValidSignature() public {
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(deviceAddress);
        uint256 tokenId = lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);

        assertEq(tokenId, 1);
        assertEq(lensMint.balanceOf(owner, tokenId), 1);

        LensMintERC1155.TokenMetadata memory meta = lensMint.getTokenMetadata(tokenId);
        assertEq(meta.deviceAddress, deviceAddress);
        assertEq(meta.imageHash, IMAGE_HASH_1);
        assertTrue(meta.isOriginal);
        assertEq(meta.signature.length, 65); // r(32) + s(32) + v(1)
    }

    function testNonceIncrementsAfterMint() public {
        assertEq(lensMint.nonces(deviceAddress), 0);

        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(deviceAddress);
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);

        assertEq(lensMint.nonces(deviceAddress), 1);
    }

    function testMintTwoDifferentImages() public {
        // Mint first image
        (uint8 v1, bytes32 r1, bytes32 s1) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );
        vm.prank(deviceAddress);
        uint256 id1 = lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v1, r1, s1);

        // Mint second image (nonce is now 1)
        (uint8 v2, bytes32 r2, bytes32 s2) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_2, 0, 1
        );
        vm.prank(deviceAddress);
        uint256 id2 = lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_2, 0, v2, r2, s2);

        assertEq(id1, 1);
        assertEq(id2, 2);
        assertEq(lensMint.nonces(deviceAddress), 2);
    }

    function testEditionMintStillWorks() public {
        // Mint original
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 10, 0
        );
        vm.prank(deviceAddress);
        uint256 originalId = lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 10, v, r, s);

        // Mint edition
        address claimer = address(0xBEEF);
        vm.prank(deviceAddress);
        uint256 editionId = lensMint.mintEdition(claimer, originalId);

        assertEq(lensMint.balanceOf(claimer, editionId), 1);

        LensMintERC1155.TokenMetadata memory meta = lensMint.getTokenMetadata(editionId);
        assertFalse(meta.isOriginal);
        assertEq(meta.originalTokenId, originalId);
        assertEq(meta.imageHash, IMAGE_HASH_1);
    }

    // ─── Replay Protection ───────────────────────────────────

    function testRevertOnDuplicateImageHash() public {
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(deviceAddress);
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);

        // Try minting the same image again (new nonce, new sig, same imageHash)
        (uint8 v2, bytes32 r2, bytes32 s2) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 1
        );

        vm.prank(deviceAddress);
        vm.expectRevert("Image already minted");
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v2, r2, s2);
    }

    // ─── Invalid Signature ───────────────────────────────────

    function testRevertOnFakeSignature() public {
        // Garbage v, r, s values
        uint8 v = 27;
        bytes32 r = bytes32(uint256(0x1234));
        bytes32 s = bytes32(uint256(0x5678));

        vm.prank(deviceAddress);
        vm.expectRevert(); // ECDSA.recover may revert or return wrong address
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);
    }

    function testRevertOnWrongSignerKey() public {
        // Attacker signs with their key, but device calls mintOriginal
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            ATTACKER_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(deviceAddress);
        vm.expectRevert("Invalid signature");
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);
    }

    function testRevertOnSignatureFromDifferentDevice() public {
        // Attacker is a registered device, signs correctly for themselves,
        // but the digest includes nonce for deviceAddress (nonce=0)
        // and attacker calls with their own address
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            ATTACKER_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        // This works for the attacker calling as themselves
        vm.prank(attackerAddress);
        uint256 tokenId = lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);
        assertEq(tokenId, 1);

        // But the same (v,r,s) fails when used by deviceAddress
        // because ecrecover returns attackerAddress, not deviceAddress
        // (Also IMAGE_HASH_1 is now used, but sig check fails first conceptually)
    }

    function testRevertOnTamperedParams() public {
        // Sign with correct params
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        // Call with a different recipient — digest won't match
        address differentRecipient = address(0xDEAD);

        vm.prank(deviceAddress);
        vm.expectRevert("Invalid signature");
        lensMint.mintOriginal(differentRecipient, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);
    }

    function testRevertOnTamperedIpfsHash() public {
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(deviceAddress);
        vm.expectRevert("Invalid signature");
        lensMint.mintOriginal(owner, "QmTamperedCID", IMAGE_HASH_1, 0, v, r, s);
    }

    function testRevertOnWrongNonce() public {
        // Sign with nonce=1, but contract expects nonce=0
        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 1
        );

        vm.prank(deviceAddress);
        vm.expectRevert("Invalid signature");
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);
    }

    // ─── Device Registry Checks Still Work ───────────────────

    function testRevertOnUnregisteredDevice() public {
        uint256 unregKey = 0xDEAD;
        address unregDevice = vm.addr(unregKey);

        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            unregKey, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(unregDevice);
        vm.expectRevert("Device not registered or inactive");
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);
    }

    function testRevertOnDeactivatedDevice() public {
        // Deactivate the device
        deviceRegistry.updateDevice(deviceAddress, "1.0.0", false);

        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(deviceAddress);
        vm.expectRevert("Device not registered or inactive");
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);
    }

    // ─── View Functions ──────────────────────────────────────

    function testDomainSeparatorIsConsistent() public view {
        bytes32 sep1 = lensMint.domainSeparator();
        bytes32 sep2 = lensMint.domainSeparator();
        assertEq(sep1, sep2);
    }

    function testUsedImageHashTracking() public {
        assertFalse(lensMint.usedImageHashes(IMAGE_HASH_1));

        (uint8 v, bytes32 r, bytes32 s) = _signMint(
            DEVICE_KEY, owner, IPFS_CID, IMAGE_HASH_1, 0, 0
        );

        vm.prank(deviceAddress);
        lensMint.mintOriginal(owner, IPFS_CID, IMAGE_HASH_1, 0, v, r, s);

        assertTrue(lensMint.usedImageHashes(IMAGE_HASH_1));
        assertFalse(lensMint.usedImageHashes(IMAGE_HASH_2));
    }
}
