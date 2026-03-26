// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/DeviceRegistry.sol";

contract DeviceRegistryPaginationTest is Test {
    DeviceRegistry public registry;

    function setUp() public {
        registry = new DeviceRegistry();

        // Register 10 devices for pagination tests
        for (uint256 i = 1; i <= 10; i++) {
            address addr = address(uint160(0xD000 + i));
            registry.registerDevice(
                addr,
                string(abi.encodePacked("0x04pub", vm.toString(i))),
                string(abi.encodePacked("dev-", vm.toString(i))),
                string(abi.encodePacked("cam-", vm.toString(i))),
                "RPi4",
                "1.0.0"
            );
        }
    }

    // ─── Basic pagination ────────────────────────────────────

    function testPaginatedFirstPage() public view {
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(0, 3);
        assertEq(total, 10);
        assertEq(page.length, 3);
        assertEq(page[0], address(uint160(0xD001)));
        assertEq(page[1], address(uint160(0xD002)));
        assertEq(page[2], address(uint160(0xD003)));
    }

    function testPaginatedMiddlePage() public view {
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(3, 3);
        assertEq(total, 10);
        assertEq(page.length, 3);
        assertEq(page[0], address(uint160(0xD004)));
        assertEq(page[1], address(uint160(0xD005)));
        assertEq(page[2], address(uint160(0xD006)));
    }

    function testPaginatedLastPageTruncated() public view {
        // Offset 8, limit 5 → only 2 items left
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(8, 5);
        assertEq(total, 10);
        assertEq(page.length, 2);
        assertEq(page[0], address(uint160(0xD009)));
        assertEq(page[1], address(uint160(0xD00A)));
    }

    function testPaginatedExactFit() public view {
        // Fetch all 10 at once
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(0, 10);
        assertEq(total, 10);
        assertEq(page.length, 10);
    }

    function testPaginatedLimitLargerThanTotal() public view {
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(0, 100);
        assertEq(total, 10);
        assertEq(page.length, 10);
    }

    // ─── Edge cases ──────────────────────────────────────────

    function testPaginatedOffsetBeyondLength() public view {
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(20, 5);
        assertEq(total, 10);
        assertEq(page.length, 0);
    }

    function testPaginatedOffsetExactlyAtLength() public view {
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(10, 5);
        assertEq(total, 10);
        assertEq(page.length, 0);
    }

    function testPaginatedZeroLimit() public view {
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(0, 0);
        assertEq(total, 10);
        assertEq(page.length, 0);
    }

    function testPaginatedEmptyRegistry() public {
        DeviceRegistry emptyRegistry = new DeviceRegistry();
        (address[] memory page, uint256 total) = emptyRegistry.getDevicesPaginated(0, 10);
        assertEq(total, 0);
        assertEq(page.length, 0);
    }

    function testPaginatedSingleItem() public view {
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(4, 1);
        assertEq(total, 10);
        assertEq(page.length, 1);
        assertEq(page[0], address(uint160(0xD005)));
    }

    // ─── Fuzz ────────────────────────────────────────────────

    function testFuzz_paginatedNeverReverts(uint256 offset, uint256 limit) public view {
        // Should never revert regardless of inputs
        (address[] memory page, uint256 total) = registry.getDevicesPaginated(offset, limit);
        assertEq(total, 10);
        // page.length should always be <= limit and <= total
        assertTrue(page.length <= limit || limit == 0);
        assertTrue(page.length <= total);
    }

    function testFuzz_paginatedCoversAll(uint8 rawLimit) public view {
        uint256 limit = bound(rawLimit, 1, 10);

        // Walk through all pages, collect every address
        uint256 collected = 0;
        uint256 offset = 0;

        while (offset < 10) {
            (address[] memory page, ) = registry.getDevicesPaginated(offset, limit);
            collected += page.length;
            offset += limit;
        }

        assertEq(collected, 10);
    }
}
