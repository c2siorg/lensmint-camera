// SPDX-License-Identifier: MIT
pragma solidity ^0.8.31;

import {Test} from "forge-std/Test.sol";
import {console} from "forge-std/console.sol";
import {LensMintERC1155} from "../../src/LensMintERC1155.sol";
import {DeviceRegistry} from "../../src/DeviceRegistry.sol";

contract MintEditionDebugTest is Test {
    DeviceRegistry public deviceRegistry;
    LensMintERC1155 public lensMint;

    address public deviceAddress;
    address public recipient = 0x1B8b939710c5b61EA4ab0bD4524Cbe92c06bdA71;
    uint256 private deviceKey;

    function setUp() public {
        deviceRegistry = new DeviceRegistry();
        lensMint = new LensMintERC1155(address(deviceRegistry), "https://ipfs.io/ipfs/");

        deviceKey = 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef;
        deviceAddress = vm.addr(deviceKey);

        deviceRegistry.registerDevice(
            deviceAddress,
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            "device-123",
            "camera-456",
            "Raspberry Pi 4",
            "1.0.0"
        );

        deviceRegistry.updateDevice(deviceAddress, "1.0.0", true);
    }

    function testMintEdition() public {
        address owner = address(0x1234567890123456789012345678901234567890);

        vm.prank(deviceAddress);
        uint256 originalTokenId = lensMint.mintOriginal(
            owner,
            "QmTest123",
            "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "0xsignature123",
            0
        );

        console.log("Original Token ID:", originalTokenId);

        LensMintERC1155.TokenMetadata memory metadata = lensMint.getTokenMetadata(originalTokenId);
        console.log("Token deviceAddress:", metadata.deviceAddress);
        console.log("Token isOriginal:", metadata.isOriginal);
        console.log("Token maxEditions:", metadata.maxEditions);

        assertTrue(metadata.deviceAddress != address(0), "Token should exist");
        assertTrue(metadata.isOriginal, "Token should be original");

        uint256 editionCount = lensMint.getEditionCount(originalTokenId);
        console.log("Edition count before:", editionCount);

        vm.prank(deviceAddress);
        uint256 editionTokenId = lensMint.mintEdition(recipient, originalTokenId);

        console.log("Edition Token ID:", editionTokenId);

        uint256 balance = lensMint.balanceOf(recipient, editionTokenId);
        assertEq(balance, 1, "Recipient should have 1 edition");

        console.log("Edition minted successfully!");
    }

    function testMintEditionToMetaMaskAddress() public {
        address owner = address(0x1234567890123456789012345678901234567890);
        vm.prank(deviceAddress);
        uint256 originalTokenId = lensMint.mintOriginal(
            owner,
            "QmTest123",
            "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            "0xsignature123",
            0
        );

        console.log("Original Token ID:", originalTokenId);
        console.log("Recipient address:", recipient);
        console.log("Recipient code length:", recipient.code.length);

        vm.prank(deviceAddress);
        uint256 editionTokenId = lensMint.mintEdition(recipient, originalTokenId);

        console.log("Edition Token ID:", editionTokenId);

        uint256 balance = lensMint.balanceOf(recipient, editionTokenId);
        assertEq(balance, 1, "Recipient should have 1 edition");

        console.log("SUCCESS: Edition minted to MetaMask address!");
    }

    function testMintEditionWithToken5() public {
        LensMintERC1155.TokenMetadata memory metadata = lensMint.getTokenMetadata(5);
        console.log("Token 5 deviceAddress:", metadata.deviceAddress);
        console.log("Token 5 isOriginal:", metadata.isOriginal);

        if (metadata.deviceAddress == address(0)) {
            console.log("ERROR: Token 5 does not exist");
            return;
        }

        if (!metadata.isOriginal) {
            console.log("ERROR: Token 5 is not an original");
            return;
        }

        vm.prank(deviceAddress);
        try lensMint.mintEdition(recipient, 5) returns (uint256 editionTokenId) {
            console.log("SUCCESS: Edition minted! Token ID:", editionTokenId);
        } catch Error(string memory reason) {
            console.log("ERROR:", reason);
            revert(reason);
        } catch (bytes memory lowLevelData) {
            console.log("ERROR: Low level error");
            console.logBytes(lowLevelData);
            revert();
        }
    }
}

