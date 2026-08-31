# LensMint Camera — GSoC 2026 Technical Documentation

LensMint runs as one Rust daemon on a Raspberry Pi. It captures photos, signs a HashRecord with the camera key, and mints on Sepolia or Solana devnet. We built the GSoC 2026 project in two phases:

1. **Camera runtime:** a native `egui` UI, V4L2 capture, dual-track storage, device keys, and in-process minting for EVM and Solana.
2. **Proof and verification:** a signed HashRecord at capture time, RISC Zero proving on another machine, a local mint gate that requires the proof files, and a separate verification step on Sepolia.

This guide explains the shipped system, shows how to run the daemon, follows the prove and mint flow, and points to the related code, DevLogs, and chain evidence.

If you want to run the camera, start at the [contributor starting guide](#contributor-starting-guide). [What the authenticity result means](#what-the-authenticity-result-means) and [scope not shipped in this season](#scope-not-shipped-in-this-season) are further down this page.

### Reference links

- [GSoC final report](https://gist.github.com/Tenerife-Q/00929ef6fb7c386a75bf5ee10a4a32f3)
- [Full recorded walkthrough](https://drive.google.com/file/d/1sC3sLav34UYtZb6b2B4belUimy_x9D1Z/view?usp=sharing)
- [Short recorded overview](https://drive.google.com/file/d/1TnQTV4MagZ2YkhhMRzhxviGnm3gM2znn/view?usp=sharing)
- [Daemon crate quick start](../../rust-camera-daemon/README.md)
- [ZK CLI and host guide](../../lensmint-zk/README.md)

---

## System at a glance

The camera handles capture, local checks, and minting. Since Groth16 proving is too heavy for the live camera path, a separate PC or server produces the receipt. The same receipt can later be exported for public verification on Sepolia.

```text
Raspberry Pi
  shutter
    ├── JPEG
    └── signed HashRecord (uuid, sha256, and phash)
             │
             │ copy the JPEG and HashRecord
             ▼
PC or server
  recompress → pHash1 → RISC Zero Groth16 prove
    ├── {uuid}.receipt.bin
    ├── {uuid}.journal.json
    └── {uuid}.journal.abi
             │
             │ copy the receipt and JSON journal back
             ▼
Raspberry Pi
  local mint gate
    ├── match → MintQueue → EVM or Solana mint
    └── mismatch or missing files → mint blocked

PC or server
  export-onchain(existing receipt) → seal.bin and journal.abi
             │
             ▼
Sepolia (separate transaction)
  seal and ABI journal → AuthenticityVerifier → RISC Zero Router
```

Minting and verification are separate operations. Once the local gate passes, the daemon calls the selected chain adapter. The RISC Zero seal is then verified through a different Sepolia transaction.

---

## Features delivered

### Camera and UI

| Feature | Behavior | Main implementation |
|---|---|---|
| Viewfinder | Shows a 640×480 RGBA preview with PHOTO and VIDEO modes. | `app.rs`, `backend.rs` |
| Camera controls | Provides the shutter, gallery, settings, digital zoom, and manual focus controls. | `app.rs` |
| Kiosk controls | The on-screen `POWER OFF` button exits the kiosk application without a keyboard. It doesn't shut down Linux. | `app.rs` |
| Gallery | Shows a three-column thumbnail grid and provides MINT and DELETE actions in the photo view. | `app.rs` |
| Video | Streams YUYV frames to `ffmpeg`, then closes stdin so that `ffmpeg` can finish the MP4 cleanly. | `backend.rs` |
| Mint status | Shows queue, confirmation, gas bump, success, and failure states in the photo view. | `app.rs`, `backend.rs` |
| Chain settings | Stores the selected chain, EVM chain ID, and Solana cluster in `sled`. | `app.rs`, `cmd.rs` |
| Hardware capture | Uses V4L2 `mmap` and includes the AArch64 `v4l2_format._pad0` ABI fix. | `backend.rs` |

The UI sends `DaemonCmd` values with `try_send`, which keeps storage and RPC work from blocking the render thread. The current daemon command channel has a capacity of **32**. Video frames use a separate channel with a capacity of **30**.

Both channels use backpressure instead of growing without a limit. When a channel is full, `try_send` can drop a click or video frame. The current UI ignores that send error, so a contributor debugging an unresponsive shutter should check the daemon load and logs before changing the camera driver.

Digital zoom changes the preview texture's UV coordinates. The signed JPEG remains the uncropped capture. If a focus `ioctl` is rejected by the hardware, the UI returns to the last shared hardware value instead of displaying a false state.

![LensMint camera viewfinder with photo and video controls](evidence/devlog4-camera-viewfinder-ui.png)

The on-device viewfinder provides PHOTO and VIDEO modes, a circular shutter, Gallery, and Settings. This screenshot first appeared in [DevLog 4](https://medium.com/@tenerifesea189/lensmint-devlog-4-from-shutter-to-blockchain-week-5-6-mid-term-c75a1fab6edc); only the UI is shown here, not that post's earlier relayer architecture.

Video recording and the photo authenticity flow have different scopes. Videos are saved as MP4 files and receive gallery thumbnails, but the current code doesn't create HashRecord or proof sidecars for them. The UI still shows MINT for a video item, but the gate rejects it because the HashRecord is missing. The proof and mint path described below applies to JPEG photos.

### Storage and identity

| Feature | Behavior | Main implementation |
|---|---|---|
| Dual-track storage | Saves full JPEGs to disk and keeps small gallery thumbnails in `sled`. | `backend.rs`, `main.rs` |
| Cache recovery | Rebuilds gallery thumbnails from the JPEG files when the cache is missing. | `backend.rs` |
| Cascade delete | Removes the selected media, its HashRecord and proof sidecars, and the cached thumbnail. | `backend.rs` |
| Camera identity | Stores an Ed25519 key in `keystore.pem`, which is created with mode `0400`. | `keystore.rs` |
| HashRecord | Writes a capture-time sidecar containing the UUID, SHA-256, pHash, public key, and signature. | `hash_record.rs` |

The camera identity and gas-paying wallets have different roles:

- `keystore.pem` identifies and signs for the camera.
- `evm_wallet.key` and `solana_wallet.key` pay transaction fees.
- Smart contracts register the camera public key as the device identity; it is not the gas wallet address.

### Minting

The final runtime mints directly from the Rust daemon. The TypeScript relayer shown in [DevLog 4](https://medium.com/@tenerifesea189/lensmint-devlog-4-from-shutter-to-blockchain-week-5-6-mid-term-c75a1fab6edc) belongs to an earlier stage of the project and is no longer part of the normal runtime.

`MintQueue` serializes mint requests and keeps process-local outcomes. The chain adapters handle RPC retries and replacement transactions:

- A process-wide lock allows one mint at a time.
- The idempotency key `{chain}-{sha256}` identifies a photo on a particular chain.
- During the same daemon process, a repeat after confirmation returns the earlier transaction instead of broadcasting a duplicate. A daemon restart clears this in-memory history. A second request while the first is still running returns an “already in flight” error.
- A failed request can be retried.
- The EVM adapter pins the nonce and limits gas replacement attempts.
- The RPC pool can move to another endpoint when one fails.
- Both the EVM and Solana adapters sit behind the same queue.

Supported paths delivered in this season:

- **EVM on Sepolia:** `LensMint.mintFromHardware`
- **Solana devnet:** Anchor `mint_from_hardware` with Metaplex Core

We split the EVM contracts by role: `evm-contracts/src/LensMint.sol` contains the mint contract used by the daemon, while `contracts/src/AuthenticityVerifier.sol` contains the separate RISC Zero verification contract.

The settings screen accepts other EVM chain IDs and lists Solana testnet and mainnet, but the checked-in `contracts.json` only contains deployment addresses for Sepolia and Solana devnet. Adding another network requires a matching contract or program deployment and a new config entry; changing the UI selection alone isn't enough.

The current UI has no recipient-address field. On EVM, the mint recipient therefore defaults to the gas wallet. On Solana, the gas wallet pays for the transaction and the daemon creates a fresh Metaplex Core asset keypair for each mint.

### ZK authenticity

At capture time, the daemon writes a signed HashRecord next to the JPEG. The RISC Zero guest later checks two specific conditions:

1. It verifies the Ed25519 signature over the canonical HashRecord message, `uuid|sha256|phash`.
2. It requires `Hamming(pHash0, pHash1) <= 5`.

The full JPEG **doesn't** enter the zkVM. In the normal host path, the host recompresses the image, computes a later perceptual hash, and gives both fingerprints to the guest. The current host doesn't first verify that the supplied JPEG's SHA-256 matches the HashRecord, and `pick_recompress` has a last-resort synthetic distance-2 fallback when no tested JPEG quality lands within the threshold. The proof therefore checks the supplied signature and pHash relationship; it doesn't prove that `pHash1` was derived from a particular JPEG inside the circuit.

The guest commits a fixed ABI journal of 224 bytes, or seven words. The host also writes a JSON version that is easier for the Pi gate to read. The `export-onchain` command extracts the Router-compatible seal and ABI journal from an existing receipt without re-running prove.

---

## Architecture

### Phase 1 — one Rust daemon on the Pi

![Phase 1 — original single-daemon architecture blueprint](phase1-daemon.png)

This bonding-period blueprint shows the intended split between the UI, Tokio workers, storage, hardware, and chain RPCs. We kept it as a high-level overview, even though some final code identifiers changed:

- capture, delete, settings, and mint are `DaemonCmd` variants on a bounded channel of 32 rather than separate `CaptureCmd` and `DeleteCmd` channels
- the data paths are under `~/.local/share/lensmint/`, not `~/Pictures/LensMint/` or `~/.lensmint_cache/`
- `keystore.pem` contains only the Ed25519 camera identity; EVM and Solana gas wallets are separate
- the generic Web3 worker became `MintQueue` plus in-process EVM and Solana adapters
- zoom became a digital preview crop, while focus keeps the V4L2 `ioctl` rollback behavior

The UI renders, sends commands, and manages settings and part of the gallery cache. Tokio workers handle camera capture, media writes, signing, and RPC calls so that an SD-card write or slow network request can't freeze the viewfinder. The design began with the blueprints in [DevLog 1](https://medium.com/@tenerifesea189/lensmint-devlog-1-running-a-native-rust-gui-on-a-bare-metal-raspberry-pi-bf24c5d0067b) and later became the single-daemon mint runtime described in [DevLog 5](https://medium.com/@tenerifesea189/lensmint-devlog-5-edge-cases-one-daemon-post-mid-term-aab4c2c45829). The Phase 2 diagram below adds the local mint gate and off-device proof path.

Worth knowing on the Pi:

- `libcamerify` activates the Pi ISP path used by the daemon.
- `v4l2_format._pad0` preserves the C ABI layout on AArch64.
- full JPEG writes and thumbnail generation happen away from the UI thread.
- `sled` keeps gallery access responsive on SD-card hardware.
- camera identity, EVM gas key, and Solana gas key stay separate.

### Phase 2 — Pi capture, host prove, Sepolia verify

![Phase 2 — ZK mint and verification flow](phase2-zk.png)

Three environments are involved:

1. **Pi daemon:** captures, signs the HashRecord, stores proof sidecars, gates mint, and calls a chain adapter.
2. **PC or server:** recompresses the JPEG, runs guest preflight and Groth16 prove, and exports the on-chain material.
3. **Sepolia:** `AuthenticityVerifier` checks public journal constraints and asks the RISC Zero Router to verify the seal against `IMAGE_ID`.

The Pi gate verifies the Ed25519 signature over `record.message`, requires a non-empty receipt, and checks that the JSON journal's `sha256`, `phash0`, and `device_pubkey` match the record fields. This is a local file-consistency check. It doesn't rebuild the canonical message from the record fields, bind the JSON journal to the receipt, parse or verify the receipt, or recheck `distance`, `threshold`, or `alg`. The separate Sepolia transaction verifies the seal and the public journal constraints.

---

## What the authenticity result means

For the current proof, “authenticity” means two checks:

- the Ed25519 public key carried by the proof signed the HashRecord message
- the supplied later perceptual hash remains within Hamming distance 5 of the signed capture pHash

It **doesn't** claim that:

- the system detects AI-generated images or deepfakes
- the JPEG itself is processed inside the circuit
- the proof establishes that `pHash1` came from the supplied JPEG
- a successful mint transaction also verified the Groth16 seal
- the local mint gate is a replacement for public cryptographic verification

`AuthenticityVerifier` doesn't check whether the proof's public key is an authorized camera. Device authorization belongs to the separate EVM and Solana mint contracts.

`IMAGE_ID` is the identifier of the compiled guest **program**. It is shared by all proofs generated by that guest build. The individual photo digest is the HashRecord's `sha256`.

The mint adapters send the capture-time `phash0`. The later `phash1`, Hamming distance, receipt, and seal aren't included in EVM mint calldata. They belong to the proof sidecars and the separate verification path.

Solana uses the same local receipt and journal gate before minting. This season doesn't include a Solana on-chain Groth16 verifier, so public seal verification currently runs on Sepolia.

---

## Visual evidence and engineering DevLogs

The DevLogs provide the engineering evidence behind this page: design diagrams, hardware photos, UI screenshots, terminal output, and block-explorer results.

| DevLog | Visual and technical evidence |
|---|---|
| [DevLog 1 — Running a Native Rust GUI on a Bare-Metal Raspberry Pi](https://medium.com/@tenerifesea189/lensmint-devlog-1-running-a-native-rust-gui-on-a-bare-metal-raspberry-pi-bf24c5d0067b) | SPI display running `egui`; early UI and worker diagrams; dual-track storage, gallery cache, V4L2, and rollback designs |
| [DevLog 2 — Hitting the Hardware Bottom](https://medium.com/@tenerifesea189/lensmint-devlog-2-hitting-the-hardware-bottom-week-1-2-61bb8a5bfccc) | Green and purple tearing; the failed stride investigation; `_pad0`; clear V4L2 output; digital zoom and hardware photos |
| [DevLog 3 — You Can't Software-Update Physics](https://medium.com/@tenerifesea189/lensmint-devlog-3-you-cant-software-update-physics-week-3-4-9212cdd9f0ec) | Camera and gallery UI evolution; the 16px scrollbar fix; background worker logs; cascade deletion and cache rebuild tests |
| [DevLog 4 — From Shutter to Blockchain](https://medium.com/@tenerifesea189/lensmint-devlog-4-from-shutter-to-blockchain-week-5-6-mid-term-c75a1fab6edc) | The polished camera UI and mid-term shutter-to-chain evidence for the earlier relayer path |
| [DevLog 5 — Edge Cases, One Daemon](https://medium.com/@tenerifesea189/lensmint-devlog-5-edge-cases-one-daemon-post-mid-term-aab4c2c45829) | The final in-process `MintQueue` design, Sepolia mint and duplicate handling, and Solana confirmation |
| [DevLog 6 — Making Authenticity Public](https://blog.c2si.org/lensmint-devlog-6-making-authenticity-public-phase-2-zk-4f2673c01d8c) | The Pi, proving host, and Sepolia flow; prove benchmarks; Router seal export; verifier deployment and verification |

The detailed Flow A–E drawings in DevLog 1 are bonding-period designs rather than diagrams of the final code. Their command names, storage paths, and early RPC path changed during implementation, so they are kept below as development history rather than used as the shipped architecture.

<details>
<summary>DevLog 1 early design drawings (Flow A–E)</summary>

**Flow A — capture offloading**

![Early capture and background-task design](evidence/devlog1-flow-a-capture.png)

The shutter was designed to hand storage and signing work to background tasks. The final runtime keeps this asynchronous split but routes minting through `MintQueue` and in-process chain adapters.

**Flow B — gallery cache and recovery**

![Early gallery cache and recovery design](evidence/devlog1-flow-b-gallery.png)

The two-track gallery design survived: thumbnails live in `sled`, while full JPEGs stay on disk and can rebuild the cache. The final code uses the XDG data directory instead of the path shown here.

**Flow C — controls and rollback**

![Early hardware-control rollback design](evidence/devlog1-flow-c-controls.png)

This sketch introduced rollback when a control `ioctl` fails. Focus still uses that idea; preview zoom later became a digital texture crop.

**Flow D — cascade deletion**

![Early cascade-delete design](evidence/devlog1-flow-d-delete.png)

Deletion began as a JPEG, thumbnail, and cache operation. The final path also removes videos and the HashRecord, receipt, journal, and mint-proof sidecars when present.

**Flow E — preview loop**

![Early V4L2 preview-loop design](evidence/devlog1-flow-e-preview.png)

Frames move from V4L2 `mmap` through YUYV-to-RGBA conversion into an `egui` texture. In the final daemon, the backend owns capture and shares the converted frame with the UI.

</details>

### Physical device photos

![Early Raspberry Pi and SPI display prototype](evidence/devlog1-early-hardware-prototype.jpg)

This bonding-period prototype shows the SPI display running the first camera preview placeholder. The [root README](../../README.md) already contains the larger device gallery, so this page keeps one hardware photo.

The older files named `assets/Screenshot 2025-11-23*` belong to the pre-GSoC Python prototype and should not be used as evidence for this Rust daemon.

### V4L2 diagnosis

![Corrupted green and purple V4L2 frame](evidence/devlog2-v4l2-corrupted-frame.png)

This [DevLog 2](https://medium.com/@tenerifesea189/lensmint-devlog-2-hitting-the-hardware-bottom-week-1-2-61bb8a5bfccc) screenshot shows the original green and purple output. Changing the stride didn't solve it because the corruption wasn't a tunable bytes-per-line problem. Adding `_pad0` restored the AArch64 `v4l2_format` layout and produced a clear frame:

![Clear camera frame after the AArch64 V4L2 ABI fix](evidence/devlog2-v4l2-clear-frame.jpg)

The same DevLog also records the digital zoom tests. Those screenshots are omitted here because the final behavior is already described in the Camera and UI section: zoom changes the preview texture coordinates and doesn't crop the stored JPEG.

### Gallery and mint evidence

![Three-column LensMint gallery](evidence/devlog3-three-column-gallery.png)

The three-column gallery displays thumbnails from `sled`. [DevLog 3](https://medium.com/@tenerifesea189/lensmint-devlog-3-you-cant-software-update-physics-week-3-4-9212cdd9f0ec) also records the cache rebuild and cascade-delete tests behind this UI.

![EVM mint reported as on-chain in the photo view](evidence/devlog5-evm-on-chain-status.png)

After an EVM mint succeeds, the photo view reports `ON-CHAIN`. The matching [DevLog 5](https://medium.com/@tenerifesea189/lensmint-devlog-5-edge-cases-one-daemon-post-mid-term-aab4c2c45829) includes the duplicate-mint result and the Solana devnet confirmation from the same daemon; this page doesn't duplicate the Solana screenshot.

### PR 98 verifier evidence — fixture run

The four screenshots below come from [PR 98](https://github.com/c2siorg/lensmint-camera/pull/98). They document the verifier implementation with a host fixture; they aren't frames from either Drive video. This run used fixture UUID `00000000-0000-4000-8000-000000000020` and took `556813 ms` to prove.

![PR 98 fixture proof benchmark](evidence/devlog6-pr98-fixture-prove.png)

The fixture produced a 945-byte receipt at distance 1 with the same guest `IMAGE_ID` used by later runs.

![PR 98 fixture Router seal and ABI journal export](evidence/devlog6-router-seal-export.png)

The export produced a 260-byte Router seal with selector `0x73c457ba` and a 224-byte ABI journal.

![AuthenticityVerifier deployed on Sepolia](evidence/devlog6-sepolia-verifier-deployment.png)

PR 98 deployed `AuthenticityVerifier` at `0xD2f3E1AB4d685956461F342EbeABEaE585D76927` in [transaction `0x57dda…`](https://sepolia.etherscan.io/tx/0x57dda2a6fb53ab68b776d496e2d5e9b8db192898bd1a49c645d3340a477d60ec).

![Successful PR 98 fixture verifyAuthenticity call](evidence/devlog6-sepolia-verify-authenticity.png)

The fixture then called `verifyAuthenticity(bytes,bytes)` successfully in [transaction `0xdaaacc…`](https://sepolia.etherscan.io/tx/0xdaaacc34c359496ceaed43d2954dacb27304a2f39af85d0f976510c0c1a467be). [DevLog 6](https://blog.c2si.org/lensmint-devlog-6-making-authenticity-public-phase-2-zk-4f2673c01d8c) records the same verifier work.

---

## Recorded device demos

- [Full walkthrough](https://drive.google.com/file/d/1sC3sLav34UYtZb6b2B4belUimy_x9D1Z/view?usp=sharing)
- [Short overview](https://drive.google.com/file/d/1TnQTV4MagZ2YkhhMRzhxviGnm3gM2znn/view?usp=sharing)

These videos were recorded separately from the PR 98 fixture above. The full walkthrough follows a later real-camera loop with capture UUID `ecc4527c-b550-4695-ac62-5f2bd8f0f759`. The short overview is a presentation cut and isn't used as the source for the transaction values below.

Full-walkthrough measurements reported in the video and public chain evidence:

| Item | Evidence |
|---|---|
| Prove | `prove_wall_ms=344084` (~5.7 minutes), distance `1`, receipt `945` bytes |
| Guest `IMAGE_ID` | `0xce0407dc8cd448f4c56f285dd5faa6eeb2bb715c95644ebaec7e46ea5b35f163` |
| Router seal | `260` bytes, selector `0x73c457ba` |
| Sepolia mint | [transaction](https://sepolia.etherscan.io/tx/0x83482970433a1f1b1aca6b1b694e6c63da35a242f8d74a4e8269e37a64a8d0ee) |
| Sepolia verification | [transaction](https://sepolia.etherscan.io/tx/0x699bf0c0c784890cb34f8aada498f810f472c11297f88f6e2704be9d63118606) |
| Reused PR 98 verifier | [`0xD2f3E1AB4d685956461F342EbeABEaE585D76927`](https://sepolia.etherscan.io/address/0xD2f3E1AB4d685956461F342EbeABEaE585D76927) |
| Original PR 98 deployment | [transaction](https://sepolia.etherscan.io/tx/0x57dda2a6fb53ab68b776d496e2d5e9b8db192898bd1a49c645d3340a477d60ec) |
| Earlier Phase 1 Solana devnet mint | [transaction](https://explorer.solana.com/tx/hrRKjyNaoaA1gqzJ4rUN1KKDN2pxZSxg6dAfT96YTRm6ETQzR9r1Ko5sUfg7A62HPxgFweor8K3MQzwoKcDDyrC?cluster=devnet) |

The full walkthrough reuses the verifier deployed during PR 98, but submits a new proof in transaction `0x699bf0…`; it doesn't reuse the fixture verification transaction `0xdaaacc…`. Both verification transactions call `verifyAuthenticity(bytes,bytes)` on the same contract, with different proof and journal inputs. Mint and verification are also separate transactions by design. The Solana row is earlier Phase 1 evidence, not another transaction from the recorded ZK run.

---

## Hardware and runtime layout

Typical hardware:

- Raspberry Pi
- CSI camera (IMX708-class)
- HDMI or SPI/Waveshare display
- SD-card-backed Linux storage

The Pi also needs a working camera stack, `libcamerify`, `ffmpeg`, an active Wayland session, and SSH access from the build host. The build host needs the Rust toolchain, `just`, the aarch64 GNU cross compiler, and `scp`. The proving host needs a RISC Zero toolchain that matches the crate versions, while the Sepolia verification step needs Foundry.

By default on Linux, the daemon uses this data directory:

`~/.local/share/lensmint/`

Setting `XDG_DATA_HOME` changes that base path.

| Path | Purpose |
|---|---|
| `keystore.pem` | camera Ed25519 identity (`0400`) |
| `evm_wallet.key` | EVM testnet gas wallet as a hex private key (`0400`) |
| `solana_wallet.key` | Solana testnet gas wallet as a JSON byte array or base58 key (`0400`) |
| `photos/{uuid}.jpg` | captured JPEG |
| `photos/{uuid}.mp4` | recorded video; no HashRecord or proof sidecars |
| `photos/{uuid}.hash.json` | signed capture-time HashRecord |
| `photos/{uuid}.receipt.bin` | RISC Zero receipt copied back after prove |
| `photos/{uuid}.journal.json` | human-readable journal used by the Pi gate |
| `photos/{uuid}.mint-proof.json` | local proof metadata written after the gate passes and before the chain call |
| `cache_db/` | `sled` gallery thumbnail cache |
| `cache/chainlist_rpcs.json` | cached EVM RPC list used when chainlist.org is unavailable |

The source configuration lives under:

- `rust-camera-daemon/lensmint-daemon/config/contracts.json`
- `rust-camera-daemon/lensmint-daemon/config/solana_rpcs.json`

`just deploy` copies those files to `/tmp/config/` beside `/tmp/lensmint-daemon`. At runtime, `LENSMINT_CONFIG_DIR` can point to another config directory. If no file is found, the daemon falls back to the config embedded at build time. A fresh chainlist cache is used before any network request. A stale cache is used only when sending the request fails; HTTP errors such as 429 or 500 currently return an error instead of falling back.

The proving host also writes `{uuid}.journal.abi`. The Pi doesn't need that file for minting. Later, `export-onchain` writes `seal.bin` and a non-prefixed `journal.abi` into its selected output directory for the Sepolia script.

Never commit `.env` files, private keys, wallet files, generated proof material, or Foundry broadcast output containing local transaction data.

---

## Checks before using the hardware

These commands can be run from the repository root. They check the daemon logic, the shared ZK statement, the host preflight path, and the Solidity verifier without requiring the camera:

```bash
cargo test \
  --manifest-path rust-camera-daemon/lensmint-daemon/Cargo.toml

cargo test \
  --manifest-path lensmint-zk/Cargo.toml \
  -p lensmint-zk-core \
  -p host

forge test \
  --root contracts \
  --match-contract AuthenticityVerifierTest
```

The host tests use RISC Zero development mode for a fast wiring check. A development-mode receipt isn't valid on-chain. The real demo command later in this guide unsets `RISC0_DEV_MODE`.

The daemon test suite contains one ignored live-RPC test. The default command above tests local failover behavior but doesn't prove that the Pi can reach the current public RPC endpoints.

---

## Contributor starting guide

### 1. Build and run the daemon

Prerequisites:

- an aarch64 cross toolchain including `aarch64-linux-gnu-gcc`
- `just`, `ssh`, and `scp` on the build host
- SSH access from the build host to the Pi
- a working Wayland session, camera access, `libcamerify`, and `ffmpeg` on the Pi
- an EVM or Solana gas wallet file for the selected chain
- the camera public key registered as an authorized device before the first mint

From the repository root, set the Pi connection values in the local `Justfile`, then run:

```bash
just run
```

This command:

1. builds `lensmint-daemon` for `aarch64-unknown-linux-gnu`
2. copies the release binary and configuration to the Pi
3. starts `/tmp/lensmint-daemon` through `libcamerify` with the required display variables

Do not publish a personal `PI_IP` value.

The first aarch64 build can take several minutes because the Solana dependency tree builds a vendored OpenSSL. If SSH succeeds but the UI doesn't appear, check the Pi's `XDG_RUNTIME_DIR`, `DISPLAY`, and `WAYLAND_DISPLAY` values before changing the Rust code.

### 2. Capture a photo

Use the on-screen shutter. The daemon logs the UUID and writes:

```text
photos/$UUID.jpg
photos/$UUID.hash.json
```

Before prove, no receipt or journal exists, so the local mint gate will block the capture.

Copy the UUID from the daemon's `[Storage] Save complete` log line:

```bash
export UUID=<capture-uuid>
export PI=<ssh-user>@<pi-address>
```

### 3. Move capture material to the proving host

```bash
mkdir -p /tmp/lensmint-work

scp "$PI":~/.local/share/lensmint/photos/$UUID.jpg \
    "$PI":~/.local/share/lensmint/photos/$UUID.hash.json \
    /tmp/lensmint-work/
```

The `PI` value should match `PI_USER` and `PI_IP` in the local `Justfile`.

### 4. Prepare mint access

On startup, the daemon prints the camera's Ed25519 public key. That 32-byte key, written as 64 lowercase hexadecimal characters without `0x`, is the `deviceId` used by both chain paths.

Before the first mint:

1. Put the selected gas wallet under `~/.local/share/lensmint/` on the Pi and set its file mode to `0400`.
2. Fund the EVM wallet with Sepolia ETH or the Solana wallet with devnet SOL.
3. Register the camera `deviceId` with the deployed mint contract or Solana program.

EVM registration is owner-only. The repository doesn't contain the deployed contract owner's key, so a new contributor must ask the maintainer to call `LensMint.registerDevice(deviceId)` or deploy a separate test contract. Don't use `contracts/script/RegisterDevice.s.sol` for this step: that script targets an older `DeviceRegistry`, not the ERC-721 contract called by the daemon.

Solana registration initializes the device PDA through `register_device`. It needs a funded signer, the program IDL, and the camera public key as 32 bytes. `relayer-backend/src/register_solana_device.ts` is a reference helper, not a clean-checkout command: its package manifest doesn't list every imported dependency, it defaults to a local proxy, falls back to a hard-coded test device when no ID is supplied, and catches final errors without returning a non-zero exit code. Adapt those assumptions and always pass the intended `deviceId` before using it. Although the helper remains in the legacy relayer directory, registration is a one-time setup task and isn't part of the running relayer architecture.

### 5. Generate a Groth16 receipt

Before proving, compare `sha256sum /tmp/lensmint-work/$UUID.jpg` with the `sha256` field in `$UUID.hash.json`; the current host doesn't make that comparison automatically. Then run this command from the repository root:

```bash
unset RISC0_DEV_MODE
CARGO_BUILD_JOBS=1 cargo run \
  --manifest-path lensmint-zk/Cargo.toml \
  -p host \
  --release \
  -- prove \
  --record /tmp/lensmint-work/$UUID.hash.json \
  --jpeg /tmp/lensmint-work/$UUID.jpg \
  --out /tmp/lensmint-work
```

Never generate real chain material in RISC Zero development mode. Proving takes minutes on the demonstrated host and needs much more memory than the camera runtime. If `r0vm` exits with `rx len failed`, check for an out-of-memory kill and confirm that the installed RISC Zero tools match the crate version before retrying.

The command writes `$UUID.receipt.bin`, `$UUID.journal.json`, and `$UUID.journal.abi`. The optional `--quality N` flag changes the preferred JPEG recompression quality; its default is 50.

### 6. Copy gate material back to the Pi

```bash
scp /tmp/lensmint-work/$UUID.receipt.bin \
    /tmp/lensmint-work/$UUID.journal.json \
    "$PI":~/.local/share/lensmint/photos/
```

Only the receipt and JSON journal need to return to the Pi. Keep `$UUID.journal.abi` on the proving host for inspection if needed.

In the camera UI:

1. open Settings
2. select Ethereum or Solana
3. open Gallery
4. select the capture
5. press MINT

### 7. Export and verify on Sepolia

Export the official Router seal and ABI journal from the existing receipt:

```bash
mkdir -p lensmint-zk/out/onchain

cargo run \
  --manifest-path lensmint-zk/Cargo.toml \
  -p host \
  --release \
  -- export-onchain \
  --receipt /tmp/lensmint-work/$UUID.receipt.bin \
  --out lensmint-zk/out/onchain
```

The export step verifies that the receipt matches the compiled guest before writing `seal.bin` and `journal.abi`, without re-running prove.

Before submitting, the following command should print the same `IMAGE_ID` configured in the deployed verifier:

```bash
cargo run \
  --manifest-path lensmint-zk/Cargo.toml \
  -p host \
  --release \
  -- image-id
```

`export-onchain` requires a real Groth16 receipt. A receipt created in development mode can't produce the Router seal used by Sepolia.

Then enter the Foundry project and submit the separate verification transaction:

```bash
cd contracts

export PRIVATE_KEY=...
export SEPOLIA_RPC_URL=...
export AUTHENTICITY_VERIFIER_ADDRESS=0xD2f3E1AB4d685956461F342EbeABEaE585D76927
export SEAL_FILE=../lensmint-zk/out/onchain/seal.bin
export JOURNAL_ABI_FILE=../lensmint-zk/out/onchain/journal.abi

forge script script/SubmitAuthenticityProof.s.sol:SubmitAuthenticityProofScript \
  --rpc-url "$SEPOLIA_RPC_URL" \
  --broadcast
```

Secrets belong in the shell environment, never in source control.

---

## Common first-run problems

| Symptom | What to check |
|---|---|
| The deployed process starts but no UI appears | Confirm the Pi's Wayland session and the `XDG_RUNTIME_DIR`, `DISPLAY`, and `WAYLAND_DISPLAY` values used by `just run`. |
| The viewfinder stays blank | Check `/dev/video0`, camera permissions, the CSI connection, and whether the daemon was started through `libcamerify`. |
| Video recording fails or has no thumbnail | Confirm that `ffmpeg` is installed on the Pi and available in `PATH`. |
| `EVM gas wallet not found` or `Solana gas wallet not found` | Put the correct wallet file in `~/.local/share/lensmint/`, use the expected format, set mode `0400`, and fund it with testnet tokens. |
| `Unauthorized device` or a missing Solana device PDA | Register the camera Ed25519 public key for the selected deployment before minting. |
| `mint blocked: missing receipt` or `missing journal` | Run prove for that UUID and copy both files back to the Pi's `photos/` directory. |
| `no EVM contract configured` or `no Solana program configured` | Use Sepolia or Solana devnet, or add the deployment to `contracts.json`. |
| The EVM RPC pool is empty | Check internet access to chainlist.org. If the Pi has worked before, also inspect the cached RPC file under `cache/`. |
| `r0vm` exits with `rx len failed` | Check host memory and confirm that the installed RISC Zero tools match the crate version. |
| `export-onchain` says the receipt isn't Groth16 | Generate a new receipt with `RISC0_DEV_MODE` unset. |
| A shutter or mint click appears to do nothing under load | The bounded `DaemonCmd` queue may be full. Check daemon logs, wait for the current task, and try once more. |

---

## Code map for future contributors

| Area | Path |
|---|---|
| UI and interaction | `rust-camera-daemon/lensmint-daemon/src/app.rs` |
| Daemon commands | `rust-camera-daemon/lensmint-daemon/src/cmd.rs` |
| V4L2, capture, storage, delete | `rust-camera-daemon/lensmint-daemon/src/backend.rs` |
| Device keystore | `rust-camera-daemon/lensmint-daemon/src/keystore.rs` |
| HashRecord | `rust-camera-daemon/lensmint-daemon/src/hash_record.rs` |
| Local mint gate | `rust-camera-daemon/lensmint-daemon/src/proof_gate.rs` |
| Mint queue | `rust-camera-daemon/lensmint-daemon/src/queue.rs` |
| EVM adapter | `rust-camera-daemon/lensmint-daemon/src/chain/evm.rs` |
| Solana adapter | `rust-camera-daemon/lensmint-daemon/src/chain/solana.rs` |
| Contract and RPC config | `rust-camera-daemon/lensmint-daemon/config/` |
| EVM mint contract | `evm-contracts/src/LensMint.sol` |
| Solana mint program | `solana-contracts/programs/solana-contracts/src/lib.rs` |
| Shared ZK statement | `lensmint-zk/core/src/lib.rs` |
| RISC Zero guest | `lensmint-zk/methods/guest/src/main.rs` |
| Host prove/export CLI | `lensmint-zk/host/src` |
| Sepolia verifier | `contracts/src/AuthenticityVerifier.sol` |
| Verification script | `contracts/script/SubmitAuthenticityProof.s.sol` |
| Build/deploy entry point | `Justfile` |

For a first code review, start with `main.rs`, `cmd.rs`, and `app.rs` to understand the process and UI states. Continue with `backend.rs` for capture and storage, then read `hash_record.rs`, `proof_gate.rs`, and `queue.rs` in that order. The chain adapters come next. For Phase 2, follow the statement from `lensmint-zk/core` into the guest, host, and finally `AuthenticityVerifier.sol`.

---

## Scope not shipped in this season

Several items from the proposal or later design discussions were not included in this season's release:

- **Pi-side Groth16 proving:** proving runs on a PC or server and stays outside the live viewfinder.
- **IPFS or Filecoin as a required storage path:** this season has no IPFS or Filecoin end-to-end demo. The shipped camera stores JPEGs locally and mints their provenance data on test networks.
- **Foundry fuzz tests and the original DeviceRegistry plan:** the current contracts have unit tests, but the proposal's fuzzing and registry work wasn't shipped.
- **Further frame-rate and gas tuning:** the daemon already avoids per-frame allocation in the main conversion path and caps EVM gas bumps, but wider FPS profiling and gas optimization remain polish work.
- **Strict host input binding:** the host should verify the JPEG SHA-256 against the HashRecord and remove the synthetic pHash fallback before treating the proof path as production-grade.

---

## Suggested next contributions

### Build a website for proof requests

We didn't ship a proof-request website or hosted service. The current flow copies the JPEG and signed HashRecord from the Pi to a proving host with `scp`. A future website could upload those files, show the proving job's status, and return the receipt and journal. The website would only replace the manual transfer: the Pi would still sign at capture, the host would run the guest, and Sepolia would verify the seal separately.

### Add a Solana verification path

Solana minting already uses the same local gate. A future contributor can reuse the guest statement and add a Solana verifier or bridge for public seal verification.

### Measure and improve proving

Explore faster hosts, batching, and remote proving. A later experiment can also measure whether proving on the Pi is practical, as long as it stays outside the viewfinder path.

### Calibrate perceptual-hash policy

Hamming distance 5 worked for the season demos, but it remains a fixed project constant rather than a calibrated anti-tamper threshold. A future contributor should test a wider image set and define the threat model before changing it. Any change to the guest statement or committed journal also changes `IMAGE_ID` and requires a matching verifier update.

The same work should bind `pHash1` to a defined image-processing pipeline. Today, the JPEG remains outside the circuit and the host prepares `pHash1`.

### Extend chain support through adapters

New chains should use the existing capture, HashRecord, gate, and `MintQueue` interfaces instead of copying the camera pipeline.

---

## Development history

The season's implementation is collected on [`gsoc2026-tenerife`](https://github.com/c2siorg/lensmint-camera/tree/gsoc2026-tenerife). The DevLogs and linked pull requests document how the design evolved from the early relayer path into the final single-daemon and ZK architecture.
