import hashlib
import os
import sys
try:
    from PIL import Image, ImageDraw
    import imagehash
except ImportError:
    print("Error: Required packages missing. Run: pip install Pillow imagehash")
    sys.exit(1)

def calculate_sha256(filepath):
    with open(filepath, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()

def generate_mock_capture(filename="mock_original.jpg"):
    img = Image.new('RGB', (800, 600), color=(45, 66, 88))
    d = ImageDraw.Draw(img)
    d.rectangle([(200, 150), (600, 450)], outline="white", width=5)
    d.text((220, 200), "LensMint Hardware Mock", fill="yellow")
    img.save(filename, "JPEG", quality=100)
    return filename

def run_benchmark():
    print("-" * 60)
    print("LensMint Media Hashing Resilience Benchmark".center(60))
    print("-" * 60)

    # Stage 1: Mock raw capture
    orig_path = generate_mock_capture()
    orig_img = Image.open(orig_path)
    
    orig_sha256 = calculate_sha256(orig_path)
    orig_phash = imagehash.phash(orig_img)
    
    print("\n[Stage 1: Raw Hardware Capture]")
    print(f"Original SHA-256 : {orig_sha256}")
    print(f"Original pHash   : {orig_phash}")

    # Stage 2: Simulate benign compression (e.g., IPFS/Gateway upload)
    compressed_path = "mock_compressed.jpg"
    orig_img.save(compressed_path, "JPEG", quality=95) 
    compressed_img = Image.open(compressed_path)

    new_sha256 = calculate_sha256(compressed_path)
    new_phash = imagehash.phash(compressed_img)

    print("\n[Stage 2: Simulated Storage Gateway (95% JPEG Quality)]")
    print(f"New SHA-256      : {new_sha256}")
    print(f"New pHash        : {new_phash}")

    # Stage 3: Verification logic
    print("\n[Stage 3: Verification Analysis]")
    
    sha_match = (orig_sha256 == new_sha256)
    print(f"SHA-256 Match    : {sha_match}")
    if not sha_match:
        print("                   (Strict hashing failed due to avalanche effect)")

    hamming_distance = orig_phash - new_phash
    phash_match = (hamming_distance <= 5)
    print(f"pHash Match      : {phash_match}")
    print(f"Hamming Distance : {hamming_distance}")
    if phash_match:
        print("                   (Passed within configurable threshold <= 5)")

    print("-" * 60)

    # Cleanup
    if os.path.exists(orig_path):
        os.remove(orig_path)
    if os.path.exists(compressed_path):
        os.remove(compressed_path)

if __name__ == "__main__":
    run_benchmark()