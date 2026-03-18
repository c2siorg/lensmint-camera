// SPDX-License-Identifier: MIT
pragma solidity ^0.8.31;

import {Test} from "forge-std/Test.sol";
import {LensMintVerifier} from "../../src/LensMintVerifier.sol";

/// @notice Mock that matches IRiscZeroVerifier.verify(bytes,bytes32,bytes32) used by LensMintVerifier.
contract MockRiscZeroVerifier {
    bool public shouldRevert;

    function setShouldRevert(bool _revert) external {
        shouldRevert = _revert;
    }

    function verify(bytes calldata, bytes32, bytes32) external view {
        if (shouldRevert) revert("MockRiscZeroVerifier: verify failed");
    }
}

/// @notice Unit tests for LensMintVerifier (constructor, submitMetadata validation, getters).
/// @dev Uses MockRiscZeroVerifier; full ZK proof flow is integration-only.
contract LensMintVerifierTest is Test {
    LensMintVerifier public verifier;
    MockRiscZeroVerifier public mockVerifier;

    bytes32 constant IMAGE_ID = bytes32(uint256(1));
    bytes32 constant NOTARY_KEY = keccak256("notary-key");
    bytes32 constant QUERIES_HASH = keccak256("queries-hash");
    string constant URL_PATTERN = "https://lensmint.example/";

    function setUp() public {
        mockVerifier = new MockRiscZeroVerifier();
        verifier = new LensMintVerifier(address(mockVerifier), IMAGE_ID, NOTARY_KEY, QUERIES_HASH, URL_PATTERN);
    }

    function _validJournalData() internal view returns (bytes memory) {
        return abi.encode(
            NOTARY_KEY,
            "GET",
            "https://lensmint.example/claim/abc-123", // This is the URL pattern we expect
            uint256(block.timestamp),
            QUERIES_HASH,
            "extracted-data-json"
        );
    }

    // -------------------------------------------------------------------------
    // Constructor / immutables
    // -------------------------------------------------------------------------

    function test_Constructor_Success() public view {
        assertEq(address(verifier.VERIFIER()), address(mockVerifier));
        assertEq(verifier.IMAGE_ID(), IMAGE_ID);
        assertEq(verifier.EXPECTED_NOTARY_KEY_FINGERPRINT(), NOTARY_KEY);
        assertEq(verifier.EXPECTED_QUERIES_HASH(), QUERIES_HASH);
        assertEq(verifier.expectedUrlPattern(), URL_PATTERN);
    }

    // -------------------------------------------------------------------------
    // getVerifiedMetadata / getClaimIdByTokenId (defaults)
    // -------------------------------------------------------------------------

    function test_GetVerifiedMetadata_UnknownClaimReturnsDefaults() public view {
        LensMintVerifier.VerifiedMetadata memory m = verifier.getVerifiedMetadata("unknown");
        assertEq(m.timestamp, 0);
        assertFalse(m.verified);
        assertEq(bytes(m.signature).length, 0);
    }

    function test_GetClaimIdByTokenId_UnknownReturnsEmpty() public view {
        assertEq(verifier.getClaimIdByTokenId(1), "");
    }

    // -------------------------------------------------------------------------
    // submitMetadata validation (reverts before calling verifier)
    // -------------------------------------------------------------------------

    function test_SubmitMetadata_Revert_InvalidNotaryKeyFingerprint() public {
        bytes memory journalData = abi.encode(
            bytes32(0), "GET", "https://lensmint.example/claim/x", uint256(block.timestamp), QUERIES_HASH, "data"
        );
        bytes32 journalHash = sha256(journalData);
        vm.expectRevert(LensMintVerifier.InvalidNotaryKeyFingerprint.selector);
        verifier.submitMetadata("claim-1", journalData, abi.encode(journalHash));
    }

    function test_SubmitMetadata_Revert_InvalidUrl_WrongMethod() public {
        bytes memory journalData = abi.encode(
            NOTARY_KEY, "POST", "https://lensmint.example/claim/x", uint256(block.timestamp), QUERIES_HASH, "data"
        );
        vm.expectRevert(LensMintVerifier.InvalidUrl.selector);
        verifier.submitMetadata("claim-1", journalData, "");
    }

    function test_SubmitMetadata_Revert_InvalidQueriesHash() public {
        bytes memory journalData = abi.encode(
            NOTARY_KEY, "GET", "https://lensmint.example/claim/x", uint256(block.timestamp), bytes32(0), "data"
        );
        vm.expectRevert(LensMintVerifier.InvalidQueriesHash.selector);
        verifier.submitMetadata("claim-1", journalData, "");
    }

    function test_SubmitMetadata_Revert_InvalidUrl_TooShort() public {
        bytes memory journalData = abi.encode(NOTARY_KEY, "GET", "ht", uint256(block.timestamp), QUERIES_HASH, "data");
        vm.expectRevert(LensMintVerifier.InvalidUrl.selector);
        verifier.submitMetadata("claim-1", journalData, "");
    }

    function test_SubmitMetadata_Revert_InvalidUrl_PatternMismatch() public {
        bytes memory journalData = abi.encode(
            NOTARY_KEY, "GET", "https://evil.example/claim/x", uint256(block.timestamp), QUERIES_HASH, "data"
        );
        vm.expectRevert(LensMintVerifier.InvalidUrl.selector);
        verifier.submitMetadata("claim-1", journalData, "");
    }

    function test_SubmitMetadata_Revert_InvalidMetadata() public {
        bytes memory journalData = abi.encode(
            NOTARY_KEY, "GET", "https://lensmint.example/claim/x", uint256(block.timestamp), QUERIES_HASH, ""
        );
        vm.expectRevert(LensMintVerifier.InvalidMetadata.selector);
        verifier.submitMetadata("claim-1", journalData, "");
    }

    // -------------------------------------------------------------------------
    // submitMetadata ZK verification (mock reverts => ZKProofVerificationFailed)
    // -------------------------------------------------------------------------

    function test_SubmitMetadata_Revert_ZKProofVerificationFailed() public {
        mockVerifier.setShouldRevert(true);
        bytes memory journalData = _validJournalData();
        bytes32 journalHash = sha256(journalData);
        bytes memory seal = abi.encode(journalHash);

        vm.expectRevert(LensMintVerifier.ZKProofVerificationFailed.selector);
        verifier.submitMetadata("claim-1", journalData, seal);
    }

    function test_SubmitMetadata_Success_WhenMockSucceeds() public {
        bytes memory journalData = _validJournalData();
        bytes32 journalHash = sha256(journalData);
        bytes memory seal = abi.encode(journalHash);

        verifier.submitMetadata("claim-1", journalData, seal);

        LensMintVerifier.VerifiedMetadata memory m = verifier.getVerifiedMetadata("claim-1");
        assertTrue(m.verified);
        assertEq(m.timestamp, block.timestamp);
    }
}
