// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/LensMintERC1155.sol";
import "../src/DeviceRegistry.sol";

/**
 * @title Fuzz & Integration tests for LensMint
 * @notice Property-based testing for edition minting, batch operations,
 *         and cross-contract integration between DeviceRegistry and LensMintERC1155.
 */
contract LensMintFuzzTest is Test {
    DeviceRegistry public registry;
    LensMintERC1155 public lensMint;

    uint256 internal constant DEVICE_KEY = 0xA11CE;
    address internal deviceAddr;

    address internal owner = address(0xCAFE);
    address internal claimer = address(0xBEEF);

    bytes32 internal constant MINT_ORIGINAL_TYPEHASH = keccak256(
        "MintOriginal(address to,string ipfsHash,bytes32 imageHash,uint256 maxEditions,uint256 nonce)"
    );

    function setUp() public {
        deviceAddr = vm.addr(DEVICE_KEY);
        registry = new DeviceRegistry();
        lensMint = new LensMintERC1155(address(registry), "https://ipfs.io/ipfs/");
        registry.registerDevice(deviceAddr, "0x04pub", "dev-001", "cam-001", "RPi4", "1.0.0");
    }

    // ─── Helpers ──────────────────────────────────────────────

    function _sign(
        uint256 pk,
        address to,
        string memory ipfs,
        bytes32 imgHash,
        uint256 maxEd,
        uint256 nonce
    ) internal view returns (uint8 v, bytes32 r, bytes32 s) {
        bytes32 structHash = keccak256(
            abi.encode(MINT_ORIGINAL_TYPEHASH, to, keccak256(bytes(ipfs)), imgHash, maxEd, nonce)
        );
        bytes32 digest = keccak256(
            abi.encodePacked("\x19\x01", lensMint.domainSeparator(), structHash)
        );
        (v, r, s) = vm.sign(pk, digest);
    }

    function _mintOriginal(bytes32 imgHash, uint256 maxEd) internal returns (uint256) {
        uint256 nonce = lensMint.nonces(deviceAddr);
        (uint8 v, bytes32 r, bytes32 s) = _sign(DEVICE_KEY, owner, "QmFuzz", imgHash, maxEd, nonce);
        vm.prank(deviceAddr);
        return lensMint.mintOriginal(owner, "QmFuzz", imgHash, maxEd, v, r, s);
    }

    // ═════════════════════════════════════════════════════════
    //  FUZZ: Edition minting quantity
    // ═════════════════════════════════════════════════════════

    /// @dev Fuzz: mint a random number of editions (1..50) and verify counts
    function testFuzz_mintEditionQuantity(uint8 rawQty) public {
        // Bound to reasonable range to avoid gas limits
        uint256 qty = bound(rawQty, 1, 50);

        uint256 origId = _mintOriginal(keccak256("fuzz-img"), 0); // unlimited editions

        vm.startPrank(deviceAddr);
        for (uint256 i = 0; i < qty; i++) {
            uint256 edId = lensMint.mintEdition(claimer, origId);
            assertEq(lensMint.balanceOf(claimer, edId), 1);
        }
        vm.stopPrank();

        // editionCount = 1 (original mint) + qty
        assertEq(lensMint.getEditionCount(origId), 1 + qty);
        assertEq(lensMint.totalTokens(), 1 + qty);
    }

    /// @dev Fuzz: batchMintEditions with random quantity
    function testFuzz_batchMintEditions(uint8 rawQty) public {
        uint256 qty = bound(rawQty, 1, 50);

        uint256 origId = _mintOriginal(keccak256("fuzz-batch"), 0);

        vm.prank(deviceAddr);
        uint256[] memory ids = lensMint.batchMintEditions(claimer, origId, qty);

        assertEq(ids.length, qty);
        assertEq(lensMint.getEditionCount(origId), 1 + qty);

        for (uint256 i = 0; i < qty; i++) {
            assertEq(lensMint.balanceOf(claimer, ids[i]), 1);
        }
    }

    /// @dev Fuzz: maxEditions is respected — minting exactly maxEditions-1 editions succeeds,
    ///      then one more reverts
    function testFuzz_maxEditionsEnforced(uint8 rawMax) public {
        // maxEditions between 2 and 30 (1 would mean no editions allowed)
        uint256 maxEd = bound(rawMax, 2, 30);

        uint256 origId = _mintOriginal(keccak256("fuzz-max"), maxEd);

        uint256 allowedEditions = maxEd - 1; // editionCount starts at 1

        vm.startPrank(deviceAddr);
        for (uint256 i = 0; i < allowedEditions; i++) {
            lensMint.mintEdition(claimer, origId);
        }

        // Next one should fail
        vm.expectRevert("Max editions reached");
        lensMint.mintEdition(claimer, origId);
        vm.stopPrank();

        assertEq(lensMint.getEditionCount(origId), maxEd);
    }

    /// @dev Fuzz: batchMintEditions reverts if quantity exceeds remaining slots
    function testFuzz_batchMintRevertsWhenExceedingMax(uint8 rawMax, uint8 rawQty) public {
        uint256 maxEd = bound(rawMax, 2, 20);
        uint256 allowedEditions = maxEd - 1;
        // Request more than allowed
        uint256 qty = bound(rawQty, allowedEditions + 1, allowedEditions + 20);

        uint256 origId = _mintOriginal(keccak256("fuzz-exceed"), maxEd);

        vm.prank(deviceAddr);
        vm.expectRevert("Max editions reached");
        lensMint.batchMintEditions(claimer, origId, qty);
    }

    // ═════════════════════════════════════════════════════════
    //  FUZZ: Unique image hashes
    // ═════════════════════════════════════════════════════════

    /// @dev Fuzz: every unique image hash produces a unique token
    function testFuzz_uniqueImageHash(bytes32 seed) public {
        // Skip zero hash (unlikely edge case)
        vm.assume(seed != bytes32(0));

        uint256 id = _mintOriginal(seed, 0);
        assertEq(lensMint.balanceOf(owner, id), 1);
        assertTrue(lensMint.usedImageHashes(seed));
    }

    // ═════════════════════════════════════════════════════════
    //  INTEGRATION: Full lifecycle
    // ═════════════════════════════════════════════════════════

    /// @dev Integration: register device → mint original → mint editions → deactivate → verify state
    function testIntegration_fullLifecycle() public {
        // 1. Device is already registered in setUp

        // 2. Mint an original
        bytes32 imgHash = keccak256("lifecycle-photo.jpg");
        uint256 origId = _mintOriginal(imgHash, 5);
        assertEq(origId, 1);
        assertEq(lensMint.balanceOf(owner, origId), 1);

        // 3. Mint 3 editions
        vm.startPrank(deviceAddr);
        uint256 ed1 = lensMint.mintEdition(claimer, origId);
        uint256 ed2 = lensMint.mintEdition(claimer, origId);
        uint256 ed3 = lensMint.mintEdition(claimer, origId);
        vm.stopPrank();

        assertEq(lensMint.getEditionCount(origId), 4); // 1 + 3

        // 4. Verify claimer owns all editions
        assertEq(lensMint.balanceOf(claimer, ed1), 1);
        assertEq(lensMint.balanceOf(claimer, ed2), 1);
        assertEq(lensMint.balanceOf(claimer, ed3), 1);

        // 5. Verify edition metadata links back
        LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(ed2);
        assertFalse(m.isOriginal);
        assertEq(m.originalTokenId, origId);
        assertEq(m.deviceId, "dev-001");

        // 6. Deactivate device
        registry.deactivateDevice(deviceAddr);
        assertFalse(registry.isDeviceActive(deviceAddr));
        assertFalse(lensMint.canDeviceMint(deviceAddr));

        // 7. Editions already minted still exist
        assertEq(lensMint.balanceOf(claimer, ed1), 1);

        // 8. But new mints from this device fail
        bytes32 newHash = keccak256("after-deactivation.jpg");
        (uint8 v, bytes32 r, bytes32 s) = _sign(DEVICE_KEY, owner, "Qm", newHash, 0, lensMint.nonces(deviceAddr));
        vm.prank(deviceAddr);
        vm.expectRevert("Device not registered or inactive");
        lensMint.mintOriginal(owner, "Qm", newHash, 0, v, r, s);
    }

    /// @dev Integration: multiple devices mint originals, then cross-mint editions
    function testIntegration_multiDeviceMinting() public {
        // Register a second device
        uint256 dev2Key = 0xB0B;
        address dev2 = vm.addr(dev2Key);
        registry.registerDevice(dev2, "0x04pub2", "dev-002", "cam-002", "RPi5", "2.0.0");

        // Device 1 mints an original
        bytes32 h1 = keccak256("dev1-photo");
        uint256 orig1 = _mintOriginal(h1, 0);

        // Device 2 mints a different original
        uint256 nonce2 = lensMint.nonces(dev2);
        bytes32 h2 = keccak256("dev2-photo");
        (uint8 v2, bytes32 r2, bytes32 s2) = _sign(dev2Key, owner, "Qm2", h2, 0, nonce2);
        vm.prank(dev2);
        uint256 orig2 = lensMint.mintOriginal(owner, "Qm2", h2, 0, v2, r2, s2);

        assertEq(orig1, 1);
        assertEq(orig2, 2);

        // Anyone (even device 2) can mint editions of device 1's original
        vm.prank(dev2);
        uint256 edOfOrig1 = lensMint.mintEdition(claimer, orig1);
        assertEq(lensMint.balanceOf(claimer, edOfOrig1), 1);

        LensMintERC1155.TokenMetadata memory edMeta = lensMint.getTokenMetadata(edOfOrig1);
        assertEq(edMeta.deviceAddress, deviceAddr); // original's device, not minter
        assertEq(edMeta.originalTokenId, orig1);
    }

    /// @dev Integration: device registration → firmware update → still functional
    function testIntegration_firmwareUpdate() public {
        // Update firmware
        registry.updateDevice(deviceAddr, "2.0.0", true);

        DeviceRegistry.DeviceInfo memory info = registry.getDevice(deviceAddr);
        assertEq(info.firmwareVersion, "2.0.0");
        assertTrue(info.isActive);

        // Can still mint after firmware update
        uint256 id = _mintOriginal(keccak256("post-update"), 0);
        assertEq(id, 1);
    }

    /// @dev Integration: deactivate then reactivate device, verify minting resumes
    function testIntegration_deactivateReactivate() public {
        registry.deactivateDevice(deviceAddr);
        assertFalse(lensMint.canDeviceMint(deviceAddr));

        registry.updateDevice(deviceAddr, "1.0.0", true);
        assertTrue(lensMint.canDeviceMint(deviceAddr));

        uint256 id = _mintOriginal(keccak256("reactivated-photo"), 0);
        assertEq(id, 1);
    }
}
