// SPDX-License-Identifier: MIT
pragma solidity ^0.8.31;

import {Test} from "forge-std/Test.sol";
import {LensMintERC1155} from "../src/LensMintERC1155.sol"; // solhint-disable-line
import {DeviceRegistry} from "../src/DeviceRegistry.sol"; // solhint-disable-line

/// @notice Unit tests for LensMintERC1155 (constructor, mintOriginal, mintEdition, batchMintEditions, getters, access control).
contract LensMintERC1155Test is Test {
    DeviceRegistry public deviceRegistry;
    LensMintERC1155 public lensMint;

    address public owner;
    address public device;
    address public recipient;
    address public stranger;

    string constant BASE_URI = "https://ipfs.io/ipfs/";
    string constant IPFS_HASH = "QmTest123";
    string constant IMAGE_HASH = "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
    string constant SIGNATURE = "0xsignature123";
    string constant PUBLIC_KEY = "0x04e3b0b9c3a4f0b01e3fbbef6e3b0b9c3a4f0b01e3fbbef6e3b0b9c3a4f0b01e3";
    string constant DEVICE_ID = "device-001";
    string constant CAMERA_ID = "camera-001";
    string constant MODEL = "LensMint Pi";
    string constant FW = "1.0.0";

    event TokenMinted(
        uint256 indexed tokenId, address indexed deviceAddress, string deviceId, string ipfsHash, bool isOriginal
    );
    event EditionMinted(uint256 indexed tokenId, uint256 indexed originalTokenId, address indexed to);
    event BaseURIUpdated(string newBaseURI);

    function setUp() public {
        deviceRegistry = new DeviceRegistry();
        lensMint = new LensMintERC1155(address(deviceRegistry), BASE_URI);

        owner = address(this);
        device = address(0xD1);
        recipient = address(0xD2);
        stranger = address(0xBAD);

        deviceRegistry.registerDevice(device, PUBLIC_KEY, DEVICE_ID, CAMERA_ID, MODEL, FW);
    }

    // -------------------------------------------------------------------------
    // Constructor
    // -------------------------------------------------------------------------

    function test_Constructor_Success() public view {
        assertEq(address(lensMint.deviceRegistry()), address(deviceRegistry));
        assertEq(lensMint.baseURI(), BASE_URI);
        assertEq(lensMint.owner(), owner);
        assertEq(lensMint.totalTokens(), 0);
    }

    function test_Constructor_Revert_DeviceRegistryAddressIsZero() public {
        vm.expectRevert(LensMintERC1155.DeviceRegistryAddressIsZero.selector);
        new LensMintERC1155(address(0), BASE_URI);
    }

    function test_Constructor_Revert_BaseURIEmptyString() public {
        vm.expectRevert(LensMintERC1155.BaseURIEmptyString.selector);
        new LensMintERC1155(address(deviceRegistry), "");
    }

    // -------------------------------------------------------------------------
    // mintOriginal
    // -------------------------------------------------------------------------

    function test_MintOriginal_Success() public {
        vm.prank(device);
        uint256 tokenId = lensMint.mintOriginal(recipient, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);

        assertEq(tokenId, 1);
        assertEq(lensMint.totalTokens(), 1);
        assertEq(lensMint.balanceOf(recipient, tokenId), 1);
        assertEq(lensMint.getEditionCount(tokenId), 1);

        LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(tokenId);
        assertEq(m.deviceAddress, device);
        assertTrue(m.isOriginal);
        assertEq(m.ipfsHash, IPFS_HASH);
        assertEq(m.imageHash, IMAGE_HASH);
        assertEq(m.signature, SIGNATURE);
        assertEq(m.maxEditions, 0);
        assertEq(m.originalTokenId, tokenId);
        assertEq(m.deviceId, DEVICE_ID);
    }

    function test_MintOriginal_Revert_DeviceNotRegisteredOrInactive() public {
        vm.prank(stranger);
        vm.expectRevert(LensMintERC1155.DeviceNotRegisteredOrInactive.selector);
        lensMint.mintOriginal(recipient, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);
    }

    function test_MintOriginal_Revert_WhenDeviceDeactivated() public {
        deviceRegistry.deactivateDevice(device);
        vm.prank(device);
        vm.expectRevert(LensMintERC1155.DeviceNotRegisteredOrInactive.selector);
        lensMint.mintOriginal(recipient, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);
    }

    // -------------------------------------------------------------------------
    // mintEdition
    // -------------------------------------------------------------------------

    function test_MintEdition_Success() public {
        vm.prank(device);
        uint256 originalId = lensMint.mintOriginal(owner, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);

        vm.prank(device);
        uint256 editionId = lensMint.mintEdition(recipient, originalId);

        assertEq(editionId, 2);
        assertEq(lensMint.totalTokens(), 2);
        assertEq(lensMint.balanceOf(recipient, editionId), 1);
        assertEq(lensMint.getEditionCount(originalId), 2);

        LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(editionId);
        assertEq(m.deviceAddress, device);
        assertFalse(m.isOriginal);
        assertEq(m.originalTokenId, originalId);
        assertEq(m.ipfsHash, IPFS_HASH);
    }

    function test_MintEdition_Revert_TokenDoesNotExist() public {
        vm.prank(device);
        vm.expectRevert(LensMintERC1155.TokenDoesNotExist.selector);
        lensMint.mintEdition(recipient, 999);
    }

    function test_MintEdition_Revert_TokenIsNotAnOriginal() public {
        vm.prank(device);
        uint256 originalId = lensMint.mintOriginal(owner, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);
        vm.prank(device);
        uint256 editionId = lensMint.mintEdition(recipient, originalId);

        vm.prank(device);
        vm.expectRevert(LensMintERC1155.TokenIsNotAnOriginal.selector);
        lensMint.mintEdition(recipient, editionId);
    }

    function test_MintEdition_Revert_MaxEditionsReached() public {
        vm.prank(device);
        uint256 originalId = lensMint.mintOriginal(owner, IPFS_HASH, IMAGE_HASH, SIGNATURE, 1);
        vm.prank(device);
        lensMint.mintEdition(recipient, originalId);

        vm.prank(device);
        vm.expectRevert(LensMintERC1155.MaxEditionsReached.selector);
        lensMint.mintEdition(recipient, originalId);
    }

    // -------------------------------------------------------------------------
    // batchMintEditions
    // -------------------------------------------------------------------------

    function test_BatchMintEditions_Success() public {
        vm.prank(device);
        uint256 originalId = lensMint.mintOriginal(owner, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);

        vm.prank(device);
        uint256[] memory tokenIds = lensMint.batchMintEditions(recipient, originalId, 3);

        assertEq(tokenIds.length, 3);
        assertEq(tokenIds[0], 2);
        assertEq(tokenIds[1], 3);
        assertEq(tokenIds[2], 4);
        assertEq(lensMint.getEditionCount(originalId), 4);
        assertEq(lensMint.balanceOf(recipient, 2), 1);
        assertEq(lensMint.balanceOf(recipient, 3), 1);
        assertEq(lensMint.balanceOf(recipient, 4), 1);
    }

    function test_BatchMintEditions_Revert_QuantityMustBeGreaterThanZero() public {
        vm.prank(device);
        uint256 originalId = lensMint.mintOriginal(owner, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);
        vm.prank(device);
        vm.expectRevert(LensMintERC1155.QuantityMustBeGreaterThanZero.selector);
        lensMint.batchMintEditions(recipient, originalId, 0);
    }

    function test_BatchMintEditions_Revert_MaxEditionsReached() public {
        vm.prank(device);
        uint256 originalId = lensMint.mintOriginal(owner, IPFS_HASH, IMAGE_HASH, SIGNATURE, 2);
        vm.prank(device);
        vm.expectRevert(LensMintERC1155.MaxEditionsReached.selector);
        lensMint.batchMintEditions(recipient, originalId, 3);
    }

    // -------------------------------------------------------------------------
    // uri / getTokenMetadata / getEditionCount
    // -------------------------------------------------------------------------

    function test_Uri_Revert_TokenDoesNotExist() public {
        vm.expectRevert(LensMintERC1155.TokenDoesNotExist.selector);
        lensMint.uri(1);
    }

    function test_Uri_ReturnsCorrectFormat() public {
        vm.prank(device);
        uint256 tokenId = lensMint.mintOriginal(recipient, IPFS_HASH, IMAGE_HASH, SIGNATURE, 0);
        assertEq(lensMint.uri(tokenId), string(abi.encodePacked(BASE_URI, "1")));
    }

    function test_GetTokenMetadata_NonExistentReturnsDefaults() public view {
        LensMintERC1155.TokenMetadata memory m = lensMint.getTokenMetadata(1);
        assertEq(m.deviceAddress, address(0));
        assertEq(m.originalTokenId, 0);
    }

    function test_GetEditionCount_NonExistentReturnsZero() public view {
        assertEq(lensMint.getEditionCount(1), 0);
    }

    // -------------------------------------------------------------------------
    // setBaseURI (onlyOwner)
    // -------------------------------------------------------------------------

    function test_SetBaseURI_Success() public {
        string memory newUri = "https://new.base/";
        vm.prank(owner);
        lensMint.setBaseURI(newUri);
        assertEq(lensMint.baseURI(), newUri);
    }

    function test_SetBaseURI_Revert_NotOwner() public {
        vm.prank(stranger);
        vm.expectRevert();
        lensMint.setBaseURI("https://evil/");
    }

    // -------------------------------------------------------------------------
    // canDeviceMint
    // -------------------------------------------------------------------------

    function test_CanDeviceMint_TrueWhenActive() public view {
        assertTrue(lensMint.canDeviceMint(device));
    }

    function test_CanDeviceMint_FalseWhenInactive() public {
        deviceRegistry.deactivateDevice(device);
        assertFalse(lensMint.canDeviceMint(device));
    }

    function test_CanDeviceMint_FalseWhenUnregistered() public view {
        assertFalse(lensMint.canDeviceMint(stranger));
    }
}
