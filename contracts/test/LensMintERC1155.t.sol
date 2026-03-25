// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/LensMintERC1155.sol";
import "../src/DeviceRegistry.sol";

/**
 * @title LensMintERC1155 — Unit & Integration Tests
 * @notice Covers mintOriginal, mintEdition, batchMintEditions, URI,
 *         ownership, metadata, and edge cases. EIP-712 signature tests
 *         live in LensMintEIP712.t.sol to keep files focused.
 */
contract LensMintERC1155Test is Test {
    DeviceRegistry public registry;
    LensMintERC1155 public lensMint;

    uint256 internal constant DEVICE_KEY = 0xA11CE;
    address internal deviceAddr;

    uint256 internal constant DEVICE2_KEY = 0xB0B;
    address internal device2Addr;

    address internal owner = address(0xCAFE);
    address internal claimer = address(0xBEEF);

    bytes32 internal constant MINT_ORIGINAL_TYPEHASH = keccak256(
        "MintOriginal(address to,string ipfsHash,bytes32 imageHash,uint256 maxEditions,uint256 nonce)"
    );

    // ─── Events (redeclared for expectEmit) ──────────────────

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

    // ─── Setup ───────────────────────────────────────────────

    function setUp() public {
        deviceAddr = vm.addr(DEVICE_KEY);
        device2Addr = vm.addr(DEVICE2_KEY);

        registry = new DeviceRegistry();
        lensMint = new LensMintERC1155(address(registry), "https://ipfs.io/ipfs/");

        registry.registerDevice(deviceAddr, "0x04pub1", "dev-001", "cam-001", "RPi4", "1.0.0");
        registry.registerDevice(device2Addr, "0x04pub2", "dev-002", "cam-002", "RPi4", "1.0.0");
    }

    // ─── EIP-712 signing helper ──────────────────────────────

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

    /// @dev Mint an original and return its tokenId (convenience)
    function _mintOriginal(
        uint256 pk,
        address device,
        address to,
        string memory ipfs,
        bytes32 imgHash,
        uint256 maxEd
    ) internal returns (uint256) {
        uint256 nonce = lensMint.nonces(device);
        (uint8 v, bytes32 r, bytes32 s) = _sign(pk, to, ipfs, imgHash, maxEd, nonce);
        vm.prank(device);
        return lensMint.mintOriginal(to, ipfs, imgHash, maxEd, v, r, s);
    }

    // ═════════════════════════════════════════════════════════
    //  CONSTRUCTOR
    // ═════════════════════════════════════════════════════════

    function testConstructorSetsState() public view {
        assertEq(address(lensMint.deviceRegistry()), address(registry));
        assertEq(lensMint.baseURI(), "https://ipfs.io/ipfs/");
        assertEq(lensMint.totalTokens(), 0);
    }

    function testConstructorRevertsZeroRegistry() public {
        vm.expectRevert("Invalid device registry");
        new LensMintERC1155(address(0), "https://ipfs.io/ipfs/");
    }

    // ═════════════════════════════════════════════════════════
    //  mintOriginal — success cases
    // ═════════════════════════════════════════════════════════

    function testMintOriginalIncrementsTokenId() public {
        bytes32 h1 = keccak256("img1");
        bytes32 h2 = keccak256("img2");

        uint256 id1 = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm1", h1, 0);
        uint256 id2 = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm2", h2, 0);

        assertEq(id1, 1);
        assertEq(id2, 2);
        assertEq(lensMint.totalTokens(), 2);
    }

    function testMintOriginalStoresMetadata() public {
        bytes32 imgHash = keccak256("photo.jpg");
        uint256 tokenId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "QmCID", imgHash, 5);

        LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(tokenId);
        assertEq(m.deviceAddress, deviceAddr);
        assertEq(m.deviceId, "dev-001");
        assertEq(m.ipfsHash, "QmCID");
        assertEq(m.imageHash, imgHash);
        assertTrue(m.isOriginal);
        assertEq(m.originalTokenId, tokenId);
        assertEq(m.maxEditions, 5);
        assertGt(m.timestamp, 0);
        assertEq(m.signature.length, 65);
    }

    function testMintOriginalMintsToRecipient() public {
        bytes32 h = keccak256("img");
        uint256 id = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", h, 0);
        assertEq(lensMint.balanceOf(owner, id), 1);
    }

    function testMintOriginalSetsEditionCountToOne() public {
        bytes32 h = keccak256("img");
        uint256 id = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", h, 0);
        assertEq(lensMint.getEditionCount(id), 1);
    }

    function testMintOriginalEmitsTokenMinted() public {
        bytes32 h = keccak256("img");
        uint256 nonce = lensMint.nonces(deviceAddr);
        (uint8 v, bytes32 r, bytes32 s) = _sign(DEVICE_KEY, owner, "QmCID", h, 0, nonce);

        vm.expectEmit(true, true, false, true);
        emit TokenMinted(1, deviceAddr, "dev-001", "QmCID", true);

        vm.prank(deviceAddr);
        lensMint.mintOriginal(owner, "QmCID", h, 0, v, r, s);
    }

    function testTwoDevicesCanMintIndependently() public {
        bytes32 h1 = keccak256("dev1photo");
        bytes32 h2 = keccak256("dev2photo");

        uint256 id1 = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm1", h1, 0);
        uint256 id2 = _mintOriginal(DEVICE2_KEY, device2Addr, owner, "Qm2", h2, 0);

        assertEq(id1, 1);
        assertEq(id2, 2);

        LensMintERC1155.TokenMetadata memory m1 = lensMint.getTokenMetadata(id1);
        LensMintERC1155.TokenMetadata memory m2 = lensMint.getTokenMetadata(id2);
        assertEq(m1.deviceAddress, deviceAddr);
        assertEq(m2.deviceAddress, device2Addr);
    }

    // ═════════════════════════════════════════════════════════
    //  mintOriginal — revert cases
    // ═════════════════════════════════════════════════════════

    function testMintOriginalRevertsInactiveDevice() public {
        registry.deactivateDevice(deviceAddr);

        bytes32 h = keccak256("img");
        (uint8 v, bytes32 r, bytes32 s) = _sign(DEVICE_KEY, owner, "Qm", h, 0, 0);

        vm.prank(deviceAddr);
        vm.expectRevert("Device not registered or inactive");
        lensMint.mintOriginal(owner, "Qm", h, 0, v, r, s);
    }

    function testMintOriginalRevertsUnregisteredDevice() public {
        uint256 unknownKey = 0xDEAD;
        address unknown = vm.addr(unknownKey);
        (uint8 v, bytes32 r, bytes32 s) = _sign(unknownKey, owner, "Qm", keccak256("x"), 0, 0);

        vm.prank(unknown);
        vm.expectRevert("Device not registered or inactive");
        lensMint.mintOriginal(owner, "Qm", keccak256("x"), 0, v, r, s);
    }

    // ═════════════════════════════════════════════════════════
    //  mintEdition — success cases
    // ═════════════════════════════════════════════════════════

    function testMintEditionSuccess() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 10);

        vm.prank(deviceAddr);
        uint256 edId = lensMint.mintEdition(claimer, origId);

        assertEq(lensMint.balanceOf(claimer, edId), 1);
        assertEq(lensMint.getEditionCount(origId), 2); // 1 (original) + 1 edition
    }

    function testMintEditionMetadataLinksToOriginal() public {
        bytes32 h = keccak256("img");
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "QmOrig", h, 0);

        vm.prank(deviceAddr);
        uint256 edId = lensMint.mintEdition(claimer, origId);

        LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(edId);
        assertFalse(m.isOriginal);
        assertEq(m.originalTokenId, origId);
        assertEq(m.imageHash, h);
        assertEq(m.ipfsHash, "QmOrig");
        assertEq(m.deviceAddress, deviceAddr);
    }

    function testMintEditionEmitsEvent() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);

        vm.expectEmit(true, true, true, true);
        emit EditionMinted(2, origId, claimer);

        vm.prank(deviceAddr);
        lensMint.mintEdition(claimer, origId);
    }

    function testMintEditionRespectsMaxEditions() public {
        // maxEditions = 3 means original(1) + 2 editions allowed
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 3);

        vm.startPrank(deviceAddr);
        lensMint.mintEdition(claimer, origId); // edition count -> 2
        lensMint.mintEdition(claimer, origId); // edition count -> 3

        vm.expectRevert("Max editions reached");
        lensMint.mintEdition(claimer, origId); // should fail
        vm.stopPrank();
    }

    function testMintEditionUnlimitedWhenMaxZero() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);

        vm.startPrank(deviceAddr);
        for (uint256 i = 0; i < 20; i++) {
            lensMint.mintEdition(claimer, origId);
        }
        vm.stopPrank();

        assertEq(lensMint.getEditionCount(origId), 21); // 1 + 20
    }

    // ─── mintEdition — revert cases ──────────────────────────

    function testMintEditionRevertsNonexistentToken() public {
        vm.prank(deviceAddr);
        vm.expectRevert("Token does not exist");
        lensMint.mintEdition(claimer, 999);
    }

    function testMintEditionRevertsOnEditionToken() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);

        vm.prank(deviceAddr);
        uint256 edId = lensMint.mintEdition(claimer, origId);

        // Try to mint an edition of an edition
        vm.prank(deviceAddr);
        vm.expectRevert("Token is not an original");
        lensMint.mintEdition(claimer, edId);
    }

    // ═════════════════════════════════════════════════════════
    //  batchMintEditions
    // ═════════════════════════════════════════════════════════

    function testBatchMintEditionsSuccess() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);

        vm.prank(deviceAddr);
        uint256[] memory ids = lensMint.batchMintEditions(claimer, origId, 5);

        assertEq(ids.length, 5);
        for (uint256 i = 0; i < 5; i++) {
            assertEq(lensMint.balanceOf(claimer, ids[i]), 1);
            LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(ids[i]);
            assertFalse(m.isOriginal);
            assertEq(m.originalTokenId, origId);
        }
        assertEq(lensMint.getEditionCount(origId), 6); // 1 + 5
    }

    function testBatchMintEditionsRespectsMaxEditions() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 4);
        // editionCount starts at 1, maxEditions = 4, so only 3 more allowed

        vm.prank(deviceAddr);
        vm.expectRevert("Max editions reached");
        lensMint.batchMintEditions(claimer, origId, 4); // needs 4 but only 3 slots
    }

    function testBatchMintEditionsPartialFillRevertsAtomically() public {
        // maxEditions = 3 → 2 more editions allowed
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 3);

        // Mint 1 edition first
        vm.prank(deviceAddr);
        lensMint.mintEdition(claimer, origId); // count = 2, 1 slot left

        // Trying to batch 2 should revert (only 1 slot left), entire tx reverts
        vm.prank(deviceAddr);
        vm.expectRevert("Max editions reached");
        lensMint.batchMintEditions(claimer, origId, 2);

        // Edition count should still be 2 (atomicity — nothing changed)
        assertEq(lensMint.getEditionCount(origId), 2);
    }

    function testBatchMintEditionsRevertsQuantityZero() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);

        vm.prank(deviceAddr);
        vm.expectRevert("Quantity must be > 0");
        lensMint.batchMintEditions(claimer, origId, 0);
    }

    function testBatchMintEditionsRevertsNonexistentToken() public {
        vm.prank(deviceAddr);
        vm.expectRevert("Token does not exist");
        lensMint.batchMintEditions(claimer, 999, 3);
    }

    function testBatchMintEditionsRevertsOnEditionToken() public {
        uint256 origId = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);

        vm.prank(deviceAddr);
        uint256 edId = lensMint.mintEdition(claimer, origId);

        vm.prank(deviceAddr);
        vm.expectRevert("Token is not an original");
        lensMint.batchMintEditions(claimer, edId, 1);
    }

    // ═════════════════════════════════════════════════════════
    //  URI
    // ═════════════════════════════════════════════════════════

    function testUriReturnsCorrectFormat() public {
        uint256 id = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);
        assertEq(lensMint.uri(id), "https://ipfs.io/ipfs/1");
    }

    function testUriRevertsForNonexistentToken() public {
        vm.expectRevert("Token does not exist");
        lensMint.uri(42);
    }

    // ═════════════════════════════════════════════════════════
    //  setBaseURI (owner-only)
    // ═════════════════════════════════════════════════════════

    function testSetBaseURIByOwner() public {
        lensMint.setBaseURI("https://new.uri/");
        assertEq(lensMint.baseURI(), "https://new.uri/");

        uint256 id = _mintOriginal(DEVICE_KEY, deviceAddr, owner, "Qm", keccak256("img"), 0);
        assertEq(lensMint.uri(id), "https://new.uri/1");
    }

    function testSetBaseURIEmitsEvent() public {
        vm.expectEmit(false, false, false, true);
        emit BaseURIUpdated("https://new.uri/");
        lensMint.setBaseURI("https://new.uri/");
    }

    function testSetBaseURIRevertsNonOwner() public {
        vm.prank(address(0x999));
        vm.expectRevert();
        lensMint.setBaseURI("https://evil.com/");
    }

    // ═════════════════════════════════════════════════════════
    //  canDeviceMint
    // ═════════════════════════════════════════════════════════

    function testCanDeviceMintActive() public view {
        assertTrue(lensMint.canDeviceMint(deviceAddr));
    }

    function testCanDeviceMintInactive() public {
        registry.deactivateDevice(deviceAddr);
        assertFalse(lensMint.canDeviceMint(deviceAddr));
    }

    function testCanDeviceMintUnregistered() public view {
        assertFalse(lensMint.canDeviceMint(address(0x9999)));
    }

    // ═════════════════════════════════════════════════════════
    //  getTokenMetadata / getEditionCount for missing tokens
    // ═════════════════════════════════════════════════════════

    function testGetTokenMetadataReturnsEmptyForMissing() public view {
        LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(999);
        assertEq(m.deviceAddress, address(0));
    }

    function testGetEditionCountReturnsZeroForMissing() public view {
        assertEq(lensMint.getEditionCount(999), 0);
    }
}
