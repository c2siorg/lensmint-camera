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
#     LENS MINT VERIFIER · ON-CHAIN TRUST LAYER                                #
#                                                                              #
##############################################################################*/

import {IRiscZeroVerifier} from "risc0-risc0-ethereum-3.0.0/IRiscZeroVerifier.sol";

contract LensMintVerifier {
    // RISC Zero verifier for ZK proof validation
    IRiscZeroVerifier public immutable VERIFIER;
    bytes32 public immutable IMAGE_ID;
    bytes32 public immutable EXPECTED_NOTARY_KEY_FINGERPRINT;
    bytes32 public immutable EXPECTED_QUERIES_HASH;
    string public expectedUrlPattern;

    mapping(string => VerifiedMetadata) public verifiedMetadata;
    mapping(uint256 => string) public tokenIdToClaimId;

    struct VerifiedMetadata {
        string signature;
        string deviceAddress;
        string deviceId;
        string imageHash;
        uint256 tokenId;
        string filecoinCid;
        string cameraId;
        uint256 timestamp;
        bool verified;
    }

    event MetadataVerified(
        string claimId,
        uint256 tokenId,
        string deviceAddress,
        string deviceId,
        string imageHash,
        uint256 timestamp,
        uint256 blockNumber
    );

    error InvalidNotaryKeyFingerprint();
    error InvalidQueriesHash();
    error InvalidUrl();
    error ZKProofVerificationFailed();
    error InvalidMetadata();
    constructor(
        address _verifier,
        bytes32 _imageId,
        bytes32 _expectedNotaryKeyFingerprint,
        bytes32 _expectedQueriesHash,
        string memory _expectedUrlPattern
    ) {
        VERIFIER = IRiscZeroVerifier(_verifier);
        IMAGE_ID = _imageId;
        EXPECTED_NOTARY_KEY_FINGERPRINT = _expectedNotaryKeyFingerprint;
        EXPECTED_QUERIES_HASH = _expectedQueriesHash;
        expectedUrlPattern = _expectedUrlPattern;
    }

    function submitMetadata(
        string memory claimId,
        bytes calldata journalData,
        bytes calldata seal
    ) external {
        (
            bytes32 notaryKeyFingerprint,
            string memory method,
            string memory url,
            uint256 timestamp,
            bytes32 queriesHash,
            string memory extractedData
        ) = abi.decode(journalData, (bytes32, string, string, uint256, bytes32, string));

        if (notaryKeyFingerprint != EXPECTED_NOTARY_KEY_FINGERPRINT) {
            revert InvalidNotaryKeyFingerprint();
        }

        if (keccak256(bytes(method)) != keccak256(bytes("GET"))) {
            revert InvalidUrl();
        }

        if (queriesHash != EXPECTED_QUERIES_HASH) {
            revert InvalidQueriesHash();
        }

        bytes memory urlBytes = bytes(url);
        bytes memory patternBytes = bytes(expectedUrlPattern);
        
        if (urlBytes.length < patternBytes.length) {
            revert InvalidUrl();
        }
        
        for (uint256 i = 0; i < patternBytes.length; i++) {
            if (urlBytes[i] != patternBytes[i]) {
                revert InvalidUrl();
            }
        }

        if (bytes(extractedData).length == 0) {
            revert InvalidMetadata();
        }

        try VERIFIER.verify(seal, IMAGE_ID, sha256(journalData)) {
        } catch {
            revert ZKProofVerificationFailed();
        }

        VerifiedMetadata storage metadata = verifiedMetadata[claimId];
        metadata.timestamp = timestamp;
        metadata.verified = true;

        emit MetadataVerified(
            claimId,
            0,
            "",
            "",
            "",
            timestamp,
            block.number
        );
    }

    function getVerifiedMetadata(string memory claimId) external view returns (VerifiedMetadata memory) {
        return verifiedMetadata[claimId];
    }

    function getClaimIdByTokenId(uint256 tokenId) external view returns (string memory) {
        return tokenIdToClaimId[tokenId];
    }
}
