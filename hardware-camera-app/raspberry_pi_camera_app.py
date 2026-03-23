#!/usr/bin/env python3

import os
import time
from datetime import datetime
from pathlib import Path
import threading
import numpy as np
import cv2
import hashlib
import json
import requests
from io import BytesIO
import logging
from logging import Filter

# --- NEW: Import for Perceptual Hashing (pHash) ---
try:
    from PIL import Image
    import imagehash
    PHASH_AVAILABLE = True
except ImportError:
    PHASH_AVAILABLE = False
    print("Warning: imagehash library not available. Install with: pip3 install imagehash Pillow")

try:
    from hardware_identity import get_hardware_identity
    HARDWARE_IDENTITY_AVAILABLE = True
except ImportError:
    HARDWARE_IDENTITY_AVAILABLE = False
    print("Warning: hardware_identity module not available - running in demo mode")

os.environ['KIVY_NO_ARGS'] = '1'
os.environ['KIVY_LOG_LEVEL'] = os.getenv('KIVY_LOG_LEVEL', 'info')

from kivy.config import Config
Config.set('graphics', 'borderless', '1')
Config.set('graphics', 'window_state', 'maximized')
Config.set('graphics', 'fullscreen', os.getenv('KIVY_FULLSCREEN', 'auto'))
Config.set('kivy', 'log_level', os.getenv('KIVY_LOG_LEVEL', 'info'))

from kivy.app import App
from kivy.uix.boxlayout import BoxLayout
from kivy.uix.floatlayout import FloatLayout
from kivy.uix.gridlayout import GridLayout
from kivy.uix.scrollview import ScrollView
from kivy.uix.button import Button
from kivy.uix.label import Label
from kivy.uix.image import Image
from kivy.clock import Clock
from kivy.core.window import Window
from kivy.graphics.texture import Texture
from kivy.graphics import Color, Rectangle
from kivy.uix.screenmanager import ScreenManager, Screen
import glob

# Camera imports
try:
    from picamera2 import Picamera2
    from picamera2.encoders import H264Encoder
    from picamera2.outputs import FileOutput
    CAMERA_AVAILABLE = True
except ImportError:
    CAMERA_AVAILABLE = False
    print("Warning: Picamera2 not available. Running in demo mode.")

try:
    import smbus2 as smbus
    UPS_AVAILABLE = True
except ImportError:
    UPS_AVAILABLE = False

class ThrottledFilter(Filter):
    def __init__(self, pattern, interval=30):
        super().__init__()
        self.pattern = pattern
        self.interval = interval
        self.last_log_time = {}
    
    def filter(self, record):
        message = record.getMessage()
        if self.pattern in message:
            key = self.pattern
            now = time.time()
            
            if key not in self.last_log_time:
                self.last_log_time[key] = now
                return True
            
            if now - self.last_log_time[key] >= self.interval:
                self.last_log_time[key] = now
                return True
            
            return False
        
        return True

logging.basicConfig(level=logging.INFO)

kivy_logger = logging.getLogger('kivy')
kivy_logger.setLevel(logging.INFO)

throttle_filter = ThrottledFilter('Execute job', interval=30)
kivy_logger.addFilter(throttle_filter)

for handler in logging.root.handlers:
    handler.addFilter(ThrottledFilter('Execute job', interval=30))

picamera2_logger = logging.getLogger('picamera2')
picamera2_logger.setLevel(logging.INFO)

CAPTURE_DIR = Path(os.getenv('CAPTURE_DIR', str(Path.home() / "captures")))
CAPTURE_DIR.mkdir(parents=True, exist_ok=True)

BACKEND_URL = os.getenv('BACKEND_URL', 'http://localhost:5000')
CLAIM_POLL_INTERVAL = int(os.getenv('CLAIM_POLL_INTERVAL', '5'))

try:
    import qrcode
    QRCODE_AVAILABLE = True
except ImportError:
    QRCODE_AVAILABLE = False
    print("Warning: qrcode library not available. Install with: pip3 install qrcode[pil]")

PREVIEW_SIZE = tuple(map(int, os.getenv('PREVIEW_SIZE', '640,480').split(',')))
PHOTO_SIZE = tuple(map(int, os.getenv('PHOTO_SIZE', '1920,1080').split(',')))
VIDEO_SIZE = tuple(map(int, os.getenv('VIDEO_SIZE', '1280,720').split(',')))

MIN_ZOOM = float(os.getenv('MIN_ZOOM', '1.0'))
MAX_ZOOM = float(os.getenv('MAX_ZOOM', '4.0'))
ZOOM_STEP = float(os.getenv('ZOOM_STEP', '0.5'))

CAMERA_ROTATION = int(os.getenv('CAMERA_ROTATION', '90'))

class BatteryMonitor:

    def __init__(self):
        self.simulated = not UPS_AVAILABLE
        self.bus = None
        self.address = int(os.getenv('UPS_I2C_ADDRESS', '0x36'), 16)
        
        if not self.simulated:
            try:
                self.bus = smbus.SMBus(int(os.getenv('I2C_BUS', '1')))
                try:
                    self.bus.read_i2c_block_data(self.address, 0x04, 2)
                    print("Battery monitor: Waveshare UPS HAT detected")
                except:
                    print("Battery monitor: I2C device not responding, using simulation")
                    self.simulated = True
            except Exception as e:
                print(f"Battery monitor: Initialization failed ({e}), using simulation")
                self.simulated = True

    def get_battery_level(self):
        if self.simulated:
            import random
            return random.randint(75, 100)

        try:
            soc_data = self.bus.read_i2c_block_data(self.address, 0x06, 2)
            
            percentage = soc_data[0]
            if percentage > 100:
                percentage = 100
            
            return min(100, max(0, percentage))
            
        except Exception as e:
            print(f"Battery read error: {e}")
            return 85

class CameraController:

    def __init__(self):
        self.camera = None
        self.recording = False
        self.encoder = None
        self.current_zoom = MIN_ZOOM
        self.sensor_size = None
        self.initialized = False
        self.camera_id = None

    def initialize(self):
        if not CAMERA_AVAILABLE:
            raise RuntimeError("Picamera2 not available")

        if self.camera is not None:
            try:
                if self.initialized:
                    self.camera.stop()
                self.camera.close()
            except:
                pass
            self.camera = None
            self.initialized = False

        try:
            time.sleep(0.5)
            
            self.camera = Picamera2()
            self.camera.start()

            try:
                sensor_props = self.camera.camera_properties
                self.sensor_size = sensor_props.get('PixelArraySize', (2592, 1944))
                
                self.camera_id = None
                
                camera_parts = []
                
                if 'Model' in sensor_props and sensor_props.get('Model'):
                    camera_parts.append(f"model:{sensor_props['Model']}")
                
                if 'SensorName' in sensor_props and sensor_props.get('SensorName'):
                    camera_parts.append(f"sensor:{sensor_props['SensorName']}")
                
                if 'LensName' in sensor_props and sensor_props.get('LensName'):
                    camera_parts.append(f"lens:{sensor_props['LensName']}")
                
                try:
                    import subprocess
                    result = subprocess.run(
                        ['cat', '/proc/device-tree/camera0/compatible'],
                        capture_output=True,
                        text=True,
                        timeout=1
                    )
                    if result.returncode == 0 and result.stdout.strip():
                        camera_parts.append(f"compatible:{result.stdout.strip()}")
                except:
                    pass
                
                if camera_parts:
                    camera_id_str = "|".join(camera_parts)
                    import hashlib
                    self.camera_id = hashlib.sha256(camera_id_str.encode()).hexdigest()[:16]
                    print(f"Camera ID generated from properties: {self.camera_id}")
                    print(f"  Camera info: {camera_id_str[:80]}...")
                
                if not self.camera_id:
                    try:
                        import subprocess
                        result = subprocess.run(
                            ['libcamera-hello', '--list-cameras'],
                            capture_output=True,
                            text=True,
                            timeout=2
                        )
                        if result.returncode == 0 and result.stdout:
                            for line in result.stdout.split('\n'):
                                if 'serial' in line.lower():
                                    parts = line.split()
                                    for i, part in enumerate(parts):
                                        if 'serial' in part.lower() and ':' in part:
                                            serial_part = part.split(':')[-1] if ':' in part else part.split('=')[-1]
                                            if serial_part and len(serial_part) > 3:
                                                self.camera_id = serial_part.strip(':,=')
                                                break
                                    if self.camera_id:
                                        break
                    except:
                        pass
                
                if not self.camera_id:
                    import hashlib
                    props_str = str(sorted(sensor_props.items()))
                    props_str += f"|size:{self.sensor_size[0]}x{self.sensor_size[1]}"
                    self.camera_id = hashlib.sha256(props_str.encode()).hexdigest()[:16]
                    print(f"Camera ID generated from properties hash: {self.camera_id}")
                
            except Exception as e:
                self.sensor_size = (2592, 1944)
                print(f"Warning: Could not extract camera ID: {e}")
                import hashlib
                fallback = hashlib.sha256(f"camera_{time.time()}".encode()).hexdigest()[:16]
                self.camera_id = fallback

            self.initialized = True
            print(f"Camera started")
            return True

        except Exception as e:
            print(f"Error initializing camera: {e}")
            return False

    def get_frame(self):
        if not self.initialized or self.camera is None:
            return None

        try:
            frame = self.camera.capture_array()
            return frame
        except Exception as e:
            return None

    def take_photo(self):
        if not self.initialized:
            return None

        try:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filename = CAPTURE_DIR / f"photo_{timestamp}.jpg"

            request = self.camera.capture_request()
            
            if CAMERA_ROTATION != 0:
                array = request.make_array("main")
                if CAMERA_ROTATION == 90:
                    k = 3
                elif CAMERA_ROTATION == 180:
                    k = 2
                elif CAMERA_ROTATION == 270:
                    k = 1
                else:
                    k = 0
                if k > 0:
                    array = np.rot90(array, k=k)
                    cv2.imwrite(str(filename), cv2.cvtColor(array, cv2.COLOR_RGB2BGR))
                else:
                    request.save("main", str(filename))
            else:
                request.save("main", str(filename))
            
            request.release()

            print(f"Photo saved: {filename}")
            return str(filename)

        except Exception as e:
            print(f"Error taking photo: {e}")
            return None

    def start_recording(self):
        if not self.initialized or self.recording:
            return None

        try:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filename = CAPTURE_DIR / f"video_{timestamp}.h264"

            self.encoder = H264Encoder(bitrate=int(os.getenv('VIDEO_BITRATE', '10000000')))
            self.camera.start_recording(self.encoder, str(filename))

            self.recording = True
            print(f"Recording started: {filename}")
            return str(filename)

        except Exception as e:
            print(f"Error starting recording: {e}")
            return None

    def stop_recording(self):
        if not self.recording:
            return

        try:
            self.camera.stop_recording()
            self.recording = False
            print("Recording stopped")
            self.camera.start()

        except Exception as e:
            print(f"Error stopping recording: {e}")
            self.recording = False

    def zoom_in(self):
        self.current_zoom = min(MAX_ZOOM, self.current_zoom + ZOOM_STEP)
        self._apply_zoom()

    def zoom_out(self):
        self.current_zoom = max(MIN_ZOOM, self.current_zoom - ZOOM_STEP)
        self._apply_zoom()

    def _apply_zoom(self):
        if not self.initialized or self.sensor_size is None:
            return

        try:
            width, height = self.sensor_size

            crop_width = int(width / self.current_zoom)
            crop_height = int(height / self.current_zoom)

            x = (width - crop_width) // 2
            y = (height - crop_height) // 2

            self.camera.set_controls({
                "ScalerCrop": (x, y, crop_width, crop_height)
            })

            print(f"Zoom level: {self.current_zoom}x")

        except Exception as e:
            print(f"Error applying zoom: {e}")

    def get_camera_id(self):
        return self.camera_id
    
    def cleanup(self):
        if self.recording:
            self.stop_recording()

        if self.camera is not None:
            try:
                self.camera.stop()
                self.camera.close()
            except:
                pass

class CameraApp(App):

    def build(self):
        Window.show_cursor = os.getenv('SHOW_CURSOR', 'true').lower() == 'true'

        self.root_layout = FloatLayout()

        self.preview_image = Image(
            size_hint=(1, 1),
            pos_hint={'x': 0, 'y': 0},
            allow_stretch=True,
            keep_ratio=False  # Fill entire screen without black bars
        )
        self.root_layout.add_widget(self.preview_image)

        self.top_bar = BoxLayout(
            orientation='horizontal',
            size_hint=(1, 0.06),  # Smaller top bar
            pos_hint={'x': 0, 'top': 1},
            spacing=5,
            padding=[3, 1]
        )

        self.datetime_label = Label(
            text='',
            size_hint=(0.6, 1),
            halign='left',
            valign='middle',
            font_size='10sp',
            color=(1, 1, 1, 1)
        )
        self.datetime_label.bind(size=self.datetime_label.setter('text_size'))

        self.battery_label = Label(
            text='Battery: ---%',
            size_hint=(0.32, 1),
            halign='right',
            valign='middle',
            font_size='11sp',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.battery_label.bind(size=self.battery_label.setter('text_size'))

        self.fund_button = Button(
            text='💰',
            font_size='14sp',
            size_hint=(0.08, 1),
            background_color=(0.2, 0.5, 0.8, 0.7),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.fund_button.bind(on_press=self._show_funding_qr_button)

        self.top_bar.add_widget(self.datetime_label)
        self.top_bar.add_widget(self.battery_label)
        self.top_bar.add_widget(self.fund_button)
        self.root_layout.add_widget(self.top_bar)

        self.control_panel = BoxLayout(
            orientation='horizontal',
            size_hint=(1, 0.25),  # Reduced to 25% for cleaner look
            pos_hint={'x': 0, 'y': 0},
            spacing=6,
            padding=[6, 6]
        )

        self.photo_button = Button(
            text='PHOTO',
            font_size='16sp',
            size_hint=(0.22, 1),
            background_color=(0.2, 0.6, 0.2, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.photo_button.bind(on_press=self.take_photo)

        self.video_button = Button(
            text='VIDEO',
            font_size='16sp',
            size_hint=(0.22, 1),
            background_color=(0.6, 0.2, 0.2, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.video_button.bind(on_press=self.toggle_recording)

        zoom_layout = BoxLayout(orientation='vertical', size_hint=(0.18, 1), spacing=4)

        self.zoom_in_button = Button(
            text='ZOOM +',
            font_size='14sp',
            background_color=(0.3, 0.3, 0.6, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.zoom_in_button.bind(on_press=self.zoom_in)

        self.zoom_out_button = Button(
            text='ZOOM -',
            font_size='14sp',
            background_color=(0.3, 0.3, 0.6, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.zoom_out_button.bind(on_press=self.zoom_out)

        zoom_layout.add_widget(self.zoom_in_button)
        zoom_layout.add_widget(self.zoom_out_button)

        self.gallery_button = Button(
            text='GALLERY',
            font_size='16sp',
            size_hint=(0.18, 1),
            background_color=(0.5, 0.3, 0.7, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.gallery_button.bind(on_press=self.open_gallery)

        self.quit_button = Button(
            text='QUIT',
            font_size='16sp',
            size_hint=(0.20, 1),
            background_color=(0.6, 0.2, 0.2, 0.8),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True
        )
        self.quit_button.bind(on_press=self.quit_app)

        self.control_panel.add_widget(self.photo_button)
        self.control_panel.add_widget(self.video_button)
        self.control_panel.add_widget(zoom_layout)
        self.control_panel.add_widget(self.gallery_button)
        self.control_panel.add_widget(self.quit_button)

        self.root_layout.add_widget(self.control_panel)

        self.status_label = Label(
            text='',
            size_hint=(None, None),
            size=(200, 40),
            pos_hint={'center_x': 0.5, 'center_y': 0.5},
            font_size='16sp',
            bold=True,
            color=(1, 1, 1, 1)
        )
        self.root_layout.add_widget(self.status_label)

        self.qr_overlay = FloatLayout()
        self.qr_overlay.opacity = 0
        
        with self.qr_overlay.canvas.before:
            Color(0, 0, 0, 0.8)
            self.qr_bg = Rectangle(size=Window.size, pos=(0, 0))
        
        qr_container = BoxLayout(
            orientation='vertical',
            size_hint=(0.6, 0.7),
            pos_hint={'center_x': 0.5, 'center_y': 0.5},
            spacing=10,
            padding=20
        )
        
        self.qr_title = Label(
            text='📱 Scan QR Code',
            font_size='20sp',
            bold=True,
            color=(1, 1, 1, 1),
            size_hint=(1, 0.1)
        )
        
        self.qr_image = Image(
            size_hint=(1, 0.7),
            allow_stretch=True,
            keep_ratio=True
        )
        
        self.qr_status = Label(
            text='Waiting for wallet address...',
            font_size='14sp',
            color=(1, 1, 1, 1),
            size_hint=(1, 0.1),
            halign='center'
        )
        self.qr_status.bind(size=self.qr_status.setter('text_size'))
        
        qr_close = Button(
            text='Close',
            font_size='16sp',
            size_hint=(1, 0.1),
            background_color=(0.6, 0.2, 0.2, 1),
            background_normal='',
            color=(1, 1, 1, 1)
        )
        qr_close.bind(on_press=self.close_qr_overlay)
        
        qr_container.add_widget(self.qr_title)
        qr_container.add_widget(self.qr_image)
        qr_container.add_widget(self.qr_status)
        qr_container.add_widget(qr_close)
        
        self.qr_overlay.add_widget(qr_container)
        self.root_layout.add_widget(self.qr_overlay)
        
        self.active_claims = {}
        self.cleared_mint_status = set()

        self.camera = CameraController()
        self.battery_monitor = BatteryMonitor()
        
        self.hardware_identity = None
        camera_id = None
        
        self.camera_ready = False
        self.balance_check_passed = False
        
        if CAMERA_AVAILABLE:
            try:
                if self.camera.initialize():
                    camera_id = self.camera.get_camera_id()
                else:
                    self.status_label.text = '✗ Camera Error'
                    self.show_error("Camera initialization failed")
            except Exception as e:
                self.status_label.text = '✗ Not Found'
                self.show_error(f"Camera error: {e}")
                try:
                    if self.camera.camera is not None:
                        self.camera.cleanup()
                except:
                    pass
        else:
            self.status_label.text = 'Demo Mode'
            self.show_error("Picamera2 not installed")
        
        if HARDWARE_IDENTITY_AVAILABLE:
            try:
                self.hardware_identity = get_hardware_identity(camera_id=camera_id)
                
                # Automatically export key for backend to use
                self._export_device_key()
                
                # Print all hardware information on initialization
                self._print_hardware_info()
            except Exception as e:
                print(f"Warning: Could not initialize hardware identity: {e}")
                self.hardware_identity = None
        
        # Check balance and show funding QR if needed
        if self.hardware_identity:
            Clock.schedule_once(lambda dt: self._check_balance_and_setup(), 1)
        
        # Schedule UI updates
        Clock.schedule_interval(self.update_datetime, 1.0)
        Clock.schedule_interval(self.update_battery, 5.0)

        return self.root_layout
    
    def _check_balance_and_setup(self):
        """Check wallet balance and setup camera stream if sufficient."""
        def check_thread():
            try:
                # Check balance via backend
                response = requests.get(
                    f'{BACKEND_URL}/api/balance',
                    timeout=10
                )
                
                if response.status_code == 200:
                    result = response.json()
                    if result.get('success'):
                        address = result.get('address')
                        eth_data = result.get('eth', {})
                        usdfc_data = result.get('usdfc', {})
                        
                        balance_eth = eth_data.get('balanceEth', 0)
                        has_enough_eth = eth_data.get('hasEnoughBalance', False)
                        needs_eth_funding = eth_data.get('needsFunding', False)
                        
                        balance_usdfc = usdfc_data.get('balanceUsdfc', 0)
                        has_enough_usdfc = usdfc_data.get('hasEnoughBalance', False)
                        needs_usdfc_funding = usdfc_data.get('needsFunding', False)
                        usdfc_available = usdfc_data.get('available', True)
                        
                        has_enough = result.get('hasEnoughBalance', False)
                        
                        # Always start camera stream - show QR overlay if balance is low
                        Clock.schedule_once(
                            lambda dt: self._start_camera_stream(),
                            0
                        )
                        
                        # Device registration depends only on ETH balance (for gas fees)
                        if has_enough_eth:
                            # ETH balance sufficient - register device
                            Clock.schedule_once(
                                lambda dt: self._try_register_device(),
                                1
                            )
                            self.balance_check_passed = True
                            print(f"✅ ETH balance sufficient: {balance_eth} ETH - Device registration will proceed")
                        else:
                            # ETH balance too low - show funding QR
                            Clock.schedule_once(
                                lambda dt: self._show_funding_qr(address, balance_eth, 'ETH'),
                                0.5
                            )
                            print(f"⚠️ ETH balance too low: {balance_eth} ETH (need 0.01 ETH) - Device registration skipped")
                        
                        # Show USDFC funding QR if needed (but don't block registration)
                        if needs_usdfc_funding and usdfc_available:
                            Clock.schedule_once(
                                lambda dt: self._show_funding_qr(address, balance_usdfc, 'USDFC'),
                                0.5
                            )
                            print(f"⚠️ USDFC balance too low: {balance_usdfc} USDFC (need 0.1 USDFC) - Filecoin uploads may fail")
                            
                            # Start balance polling
                            Clock.schedule_once(
                                lambda dt: self._start_balance_polling(address),
                                1
                            )
                    else:
                        # Balance check failed - try to start anyway
                        print(f"⚠️ Balance check failed, starting camera anyway")
                        Clock.schedule_once(
                            lambda dt: self._start_camera_stream(),
                            0
                        )
                else:
                    # Backend not available - start camera anyway
                    print(f"⚠️ Backend not available, starting camera anyway")
                    Clock.schedule_once(
                        lambda dt: self._start_camera_stream(),
                        0
                    )
            except Exception as e:
                print(f"⚠️ Error checking balance: {e}")
                # Start camera anyway if check fails
                Clock.schedule_once(
                    lambda dt: self._start_camera_stream(),
                    0
                )
        
        threading.Thread(target=check_thread, daemon=True).start()
    
    def _start_camera_stream(self):
        """Start camera preview stream."""
        if CAMERA_AVAILABLE and self.camera.initialized:
            self.status_label.text = '✓ Ready'
            # Schedule preview updates
            Clock.schedule_interval(self.update_preview, 1.0 / 30.0)  # 30 FPS
            # Clear status after 2 seconds
            Clock.schedule_once(lambda dt: setattr(self.status_label, 'text', ''), 2)
            self.camera_ready = True
    
    def _show_funding_qr_button(self, instance):
        if self.hardware_identity:
            hw_info = self.hardware_identity.get_hardware_info()
            address = hw_info['address']
            self._show_funding_qr(address, 0, 'ETH')
    
    def _show_funding_qr(self, address, current_balance, token_type='ETH'):
        """Show QR code for funding the wallet."""
        if not QRCODE_AVAILABLE:
            if token_type == 'ETH':
                self.status_label.text = f'⚠️ Low Balance: {current_balance:.4f} ETH\nFund: {address}'
            else:
                self.status_label.text = f'⚠️ Low Balance: {current_balance:.4f} {token_type}\nFund: {address}'
            return
        
        try:
            # Use plain address - MetaMask can scan it directly
            funding_data = address
            
            # Generate QR code
            qr = qrcode.QRCode(
                version=1,
                error_correction=qrcode.constants.ERROR_CORRECT_L,
                box_size=10,
                border=4,
            )
            qr.add_data(funding_data)
            qr.make(fit=True)
            
            # Create image
            img = qr.make_image(fill_color="black", back_color="white")
            
            # Convert to bytes
            img_bytes = BytesIO()
            img.save(img_bytes, format='PNG')
            img_bytes.seek(0)
            
            # Save to temp file
            temp_path = Path(CAPTURE_DIR) / "funding_qr.png"
            with open(temp_path, 'wb') as f:
                f.write(img_bytes.read())
            
            # Update QR overlay for funding
            self.qr_image.source = str(temp_path)
            self.qr_image.reload()
            
            # Update title and status text
            self.qr_title.text = '💰 Fund Wallet'
            min_amount = '0.01 ETH' if token_type == 'ETH' else '0.1 USDFC'
            self.qr_status.text = f'⚠️ Low Balance: {current_balance:.4f} {token_type}\n\nWallet Address:\n{address}\n\nScan with MetaMask\nSend {min_amount} or more'
            self.qr_status.color = (1, 1, 0, 1)  # Yellow
            
            # Show overlay
            self.qr_overlay.opacity = 1
            
            # Update main status
            self.status_label.text = '⚠️ Funding Required'
            self.status_label.color = (1, 1, 0, 1)  # Yellow
            
        except Exception as e:
            print(f"Error generating funding QR: {e}")
            self.status_label.text = f'⚠️ Fund: {address}'
    
    def _start_balance_polling(self, address):
        """Poll balance until sufficient funds are available."""
        def poll_balance(dt):
            if self.balance_check_passed:
                return False  # Stop polling
            
            try:
                response = requests.get(
                    f'{BACKEND_URL}/api/balance',
                    timeout=5
                )
                
                if response.status_code == 200:
                    result = response.json()
                    if result.get('success'):
                        eth_data = result.get('eth', {})
                        balance_eth = eth_data.get('balanceEth', 0)
                        has_enough_eth = eth_data.get('hasEnoughBalance', False)
                        
                        # Update status
                        self.qr_status.text = f'Current Balance: {balance_eth:.4f} ETH\n\nWaiting for 0.01+ ETH...'
                        
                        if has_enough_eth:
                            # ETH balance is now sufficient - register device!
                            print(f"✅ ETH balance sufficient: {balance_eth} ETH")
                            self.balance_check_passed = True
                            
                            # Hide QR overlay
                            Clock.schedule_once(
                                lambda dt: setattr(self.qr_overlay, 'opacity', 0),
                                0
                            )
                            
                            # Start camera stream
                            Clock.schedule_once(
                                lambda dt: self._start_camera_stream(),
                                0
                            )
                            
                            # Register device (only depends on ETH balance for gas fees)
                            if self.hardware_identity and self.camera.initialized:
                                Clock.schedule_once(
                                    lambda dt: self._try_register_device(),
                                    1
                                )
                            
                            # Update status
                            Clock.schedule_once(
                                lambda dt: setattr(self.status_label, 'text', '✓ Ready'),
                                0
                            )
                            Clock.schedule_once(
                                lambda dt: setattr(self.status_label, 'color', (0, 1, 0, 1)),
                                0
                            )
                            
                            return False  # Stop polling
                            
            except Exception as e:
                print(f"Balance polling error: {e}")
            
            return True  # Continue polling
        
        # Poll every 10 seconds
        Clock.schedule_interval(poll_balance, 10)
    
    def _try_register_device(self):
        """Ensure device is registered and active with backend."""
        print("\n📋 [KIVY] Device registration check starting...")
        
        if not self.hardware_identity:
            print("   ❌ Hardware identity not available")
            return
        
        if not self.camera.initialized:
            print("   ❌ Camera not initialized")
            return
        
        print("   ✅ Hardware identity and camera ready")
        
        def register_thread():
            try:
                print("   🔄 Getting hardware info...")
                hw_info = self.hardware_identity.get_hardware_info()
                device_address = hw_info['address']
                public_key = hw_info['public_key_hex']
                camera_id = self.camera.get_camera_id()
                
                # Generate device ID from hardware info
                device_id = f"{device_address[:8]}_{camera_id[:8]}"
                
                print(f"   📊 Device details:")
                print(f"      Address: {device_address}")
                print(f"      Device ID: {device_id}")
                print(f"      Camera ID: {camera_id}")
                
                # Use ensure-registered endpoint which handles:
                # 1. Check if registered - if not, register
                # 2. Check if active - if not, activate
                print(f"   🔄 Calling {BACKEND_URL}/api/device/ensure-registered...")
                response = requests.post(
                    f'{BACKEND_URL}/api/device/ensure-registered',
                    json={
                        'deviceAddress': device_address,
                        'publicKey': public_key,
                        'deviceId': device_id,
                        'cameraId': camera_id,
                        'model': 'Raspberry Pi',
                        'firmwareVersion': '1.0.0'
                    },
                    timeout=30
                )
                
                print(f"   📊 Response status: {response.status_code}")
                
                if response.status_code == 200:
                    result = response.json()
                    print(f"   📊 Response: {result}")
                    
                    if result.get('success'):
                        if result.get('registered') and result.get('activated'):
                            if result.get('registrationTx'):
                                print(f"✅ Device registered: {result.get('registrationTx')}")
                            if result.get('activationTx'):
                                print(f"✅ Device activated: {result.get('activationTx')}")
                            if not result.get('registrationTx') and not result.get('activationTx'):
                                print("✅ Device already registered and active")
                        else:
                            print(f"⚠️ Device status: registered={result.get('registered')}, activated={result.get('activated')}")
                    else:
                        print(f"⚠️ Registration failed: {result.get('error')}")
                else:
                    print(f"⚠️ Registration failed: HTTP {response.status_code}")
                    try:
                        error_data = response.json()
                        print(f"   Error details: {error_data}")
                    except:
                        print(f"   Response text: {response.text}")
                    
            except requests.exceptions.RequestException as e:
                print(f"❌ Network error during registration: {e}")
            except Exception as e:
                print(f"❌ Could not register device: {e}")
                import traceback
                print(f"   Traceback: {traceback.format_exc()}")
        
        threading.Thread(target=register_thread, daemon=True).start()

    def update_preview(self, dt):
        """Update camera preview frame with rotation support."""
        frame = self.camera.get_frame()

        if frame is not None:
            try:
                # Get frame dimensions
                if len(frame.shape) == 3:
                    height, width, channels = frame.shape
                else:
                    height, width = frame.shape
                    channels = 1

                # Apply rotation if configured
                if CAMERA_ROTATION != 0:
                    # Calculate number of 90-degree rotations (k parameter for np.rot90)
                    # rot90 rotates counter-clockwise, so we need to adjust
                    # 90° clockwise = 270° counter-clockwise = k=3
                    # 180° = k=2
                    # 270° clockwise = 90° counter-clockwise = k=1
                    if CAMERA_ROTATION == 90:
                        k = 3  # 90° clockwise = 270° counter-clockwise
                    elif CAMERA_ROTATION == 180:
                        k = 2
                    elif CAMERA_ROTATION == 270:
                        k = 1  # 270° clockwise = 90° counter-clockwise
                    else:
                        k = 0
                    
                    if k > 0:
                        frame = np.rot90(frame, k=k)
                        # Swap width and height after 90/270 degree rotation
                        if CAMERA_ROTATION in [90, 270]:
                            width, height = height, width

                # Flip vertically for Kivy (if needed)
                frame = frame[::-1, :, :]

                # Convert to bytes
                buf = frame.tobytes()

                # Determine color format based on channels
                if channels == 3:
                    colorfmt = 'rgb'
                elif channels == 4:
                    colorfmt = 'rgba'
                else:
                    colorfmt = 'luminance'

                # Create texture with correct dimensions after rotation
                texture = Texture.create(size=(width, height), colorfmt=colorfmt)
                texture.blit_buffer(buf, colorfmt=colorfmt, bufferfmt='ubyte')

                self.preview_image.texture = texture
            except Exception as e:
                print(f"Preview error: {e}")

    def update_datetime(self, dt):
        """Update date/time display."""
        now = datetime.now()
        self.datetime_label.text = now.strftime("%Y-%m-%d %H:%M:%S")

    def _export_device_key(self):
        """Export device key to file for backend to use."""
        try:
            import json
            from pathlib import Path
            
            if not self.hardware_identity:
                return
            
            # Get hardware info
            hw_info = self.hardware_identity.get_hardware_info()
            private_key_hex = self.hardware_identity.private_key.to_string().hex()
            
            # Export data
            export_data = {
                'privateKey': f'0x{private_key_hex}',
                'address': hw_info['address'],
                'cameraId': hw_info['camera_id'],
                'publicKey': hw_info['public_key_hex']
            }
            
            # Write to file
            export_file = Path(__file__).parent / '.device_key_export'
            with open(export_file, 'w') as f:
                json.dump(export_data, f, indent=2)
            
            print(f"✅ Device key exported to: {export_file}")
            print(f"   Address: {export_data['address']}")
            print(f"   Camera ID: {export_data['cameraId']}")
        except Exception as e:
            print(f"⚠️ Could not export device key: {e}")
    
    def _print_hardware_info(self):
        """Print all hardware information on initialization."""
        print("\n" + "=" * 60)
        print("HARDWARE IDENTITY INFORMATION")
        print("=" * 60)
        
        if self.hardware_identity:
            hw_info = self.hardware_identity.get_hardware_info()
            print(f"✓ Public Address: {hw_info['address']}")
            print(f"✓ Camera ID: {hw_info['camera_id'] or 'Not available'}")
            print(f"✓ Public Key: {hw_info['public_key_hex'][:32]}...{hw_info['public_key_hex'][-8:]}")
            print(f"✓ Salt Path: {hw_info['salt_path']}")
            print(f"✓ Initialized: {hw_info['initialized']}")
        else:
            print("✗ Hardware identity not available")
        
        if self.camera and self.camera.initialized:
            print(f"✓ Camera ID: {self.camera.get_camera_id()}")
            print(f"✓ Camera Initialized: {self.camera.initialized}")
        else:
            print("✗ Camera not initialized")
        
        print("=" * 60 + "\n")
    
    def update_battery(self, dt):
        """Update battery level display."""
        level = self.battery_monitor.get_battery_level()
        self.battery_label.text = f'Battery: {level}%'

        # Change color based on battery level
        if level < 20:
            self.battery_label.color = (1, 0, 0, 1)  # Red
        elif level < 50:
            self.battery_label.color = (1, 1, 0, 1)  # Yellow
        else:
            self.battery_label.color = (0, 1, 0, 1)  # Green

    def take_photo(self, instance):
        """Handle photo capture button press."""
        self.status_label.text = '📸 Capturing...'

        # Run in thread to avoid blocking UI
        def capture_thread():
            filename = self.camera.take_photo()

            if filename:
                try:
                    # Generate hardware signature for the image
                    signature_info = None
                    if self.hardware_identity:
                        try:
                            signature_info = self._sign_image(filename)
                            if signature_info:
                                print(f"Image signed: {signature_info['address']}")
                        except Exception as e:
                            print(f"Warning: Could not sign image: {e}")
                    
                    if signature_info:
                        # Upload to backend and create claim
                        self._upload_and_create_claim(filename, signature_info)
                    else:
                        Clock.schedule_once(
                            lambda dt: setattr(self.status_label, 'text', '✗ Sign Failed'),
                            0
                        )
                        Clock.schedule_once(
                            lambda dt: setattr(self.status_label, 'text', ''),
                            2
                        )
                except Exception as e:
                    print(f"Error processing photo: {e}")
                    Clock.schedule_once(
                        lambda dt: setattr(self.status_label, 'text', '✗ Error'),
                        0
                    )
                    Clock.schedule_once(
                        lambda dt: setattr(self.status_label, 'text', ''),
                        2
                    )
            else:
                Clock.schedule_once(
                    lambda dt: setattr(self.status_label, 'text', '✗ Failed'),
                    0
                )
                Clock.schedule_once(
                    lambda dt: setattr(self.status_label, 'text', ''),
                    2
                )

        threading.Thread(target=capture_thread, daemon=True).start()
    
    def _upload_and_create_claim(self, filename, signature_info):
        """Upload image to backend and create claim."""
        # Check if offline - save to queue
        try:
            # Try to ping backend first with retry
            online = False
            for attempt in range(3):
                try:
                    requests.get(f'{BACKEND_URL}/health', timeout=2)
                    online = True
                    break
                except:
                    if attempt < 2:
                        time.sleep(1)
                    else:
                        online = False
        except:
            online = False
            print("⚠️ Backend offline - saving to queue")
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', '⚠️ Offline - Saved Locally'),
                0
            )
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', ''),
                3
            )
            return
        
        try:
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', '📤 Uploading...'),
                0
            )
            
            # Read image file
            with open(filename, 'rb') as f:
                image_data = f.read()
            
            # Compute strict image hash (SHA256)
            image_hash = hashlib.sha256(image_data).hexdigest()
            
            # --- NEW: Compute Perceptual Hash (pHash) for ZK compression tolerance ---
            phash_value = ""
            if PHASH_AVAILABLE:
                try:
                    img = Image.open(BytesIO(image_data))
                    phash_value = str(imagehash.phash(img))
                    print(f"Generated pHash for ZK verification: {phash_value}")
                except Exception as e:
                    print(f"Error generating pHash: {e}")
            
            # Get device info
            device_address = signature_info['address']
            camera_id = self.camera.get_camera_id() if self.camera.initialized else 'unknown'
            
            # Prepare multipart form data
            files = {'image': (os.path.basename(filename), image_data, 'image/jpeg')}
            data = {
                'imageHash': image_hash,
                'pHash': phash_value,  # Inject pHash into the metadata payload
                'signature': signature_info['signature'],
                'cameraId': camera_id,
                'deviceAddress': device_address
            }
            
            # Upload to backend
            response = requests.post(
                f'{BACKEND_URL}/api/images/upload',
                files=files,
                data=data,
                timeout=60
            )
            
            if response.status_code == 200:
                result = response.json()
                
                if result.get('success'):
                    claim_url = result.get('claimUrl') or result.get('qrCodeUrl')
                    claim_id = result.get('claimId')
                    image_id = result.get('imageId')
                    
                    if claim_url and claim_id:
                        # Store claim for polling
                        self.active_claims[claim_id] = image_id
                        
                        # Show success message
                        Clock.schedule_once(
                            lambda dt: setattr(self.status_label, 'text', '✓ Uploaded! Minting...'),
                            0
                        )
                        
                        # Note: NFT will be minted to owner wallet automatically by backend
                        # QR code is for others to mint editions
                        
                        # Display QR code for claiming editions
                        Clock.schedule_once(
                            lambda dt: self._show_qr_code(claim_url, claim_id),
                            2
                        )
                        
                        # Start polling for claim status (to show when minted)
                        Clock.schedule_once(
                            lambda dt: self._start_claim_polling(claim_id),
                            0
                        )
                    else:
                        Clock.schedule_once(
                            lambda dt: setattr(self.status_label, 'text', '✓ Saved (No Claim)'),
                            0
                        )
                else:
                    raise Exception(result.get('error', 'Upload failed'))
            else:
                raise Exception(f"HTTP {response.status_code}: {response.text}")
                
        except requests.exceptions.RequestException as e:
            print(f"Upload error: {e}")
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', '✗ Upload Failed'),
                0
            )
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', ''),
                3
            )
        except Exception as e:
            print(f"Error uploading: {e}")
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', '✗ Error'),
                0
            )
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', ''),
                3
            )
    
    def _show_qr_code(self, claim_url, claim_id):
        """Display QR code overlay."""
        if not QRCODE_AVAILABLE:
            # Fallback: show URL as text
            self.qr_status.text = f"URL: {claim_url}"
            self.qr_overlay.opacity = 1
            return
        
        try:
            # Generate QR code
            qr = qrcode.QRCode(
                version=1,
                error_correction=qrcode.constants.ERROR_CORRECT_L,
                box_size=10,
                border=4,
            )
            qr.add_data(claim_url)
            qr.make(fit=True)
            
            # Create image
            img = qr.make_image(fill_color="black", back_color="white")
            
            # Convert to bytes
            img_bytes = BytesIO()
            img.save(img_bytes, format='PNG')
            img_bytes.seek(0)
            
            # Save to temp file for Kivy
            temp_path = Path(CAPTURE_DIR) / f"qr_{claim_id}.png"
            with open(temp_path, 'wb') as f:
                f.write(img_bytes.read())
            
            # Load in Kivy
            self.qr_image.source = str(temp_path)
            self.qr_image.reload()
            
            # Update title and status
            self.qr_title.text = '📱 Scan to Claim NFT'
            self.qr_status.text = 'Waiting for wallet address...'
            self.qr_status.color = (1, 1, 1, 1)  # White
            
            # Show overlay
            self.qr_overlay.opacity = 1
            
        except Exception as e:
            print(f"Error generating QR code: {e}")
            self.qr_status.text = f"URL: {claim_url}"
            self.qr_overlay.opacity = 1
    
    def close_qr_overlay(self, instance):
        """Close QR code overlay."""
        self.qr_overlay.opacity = 0
    
    def _start_claim_polling(self, claim_id):
        """Start polling for claim status."""
        def poll_claim(dt):
            if claim_id not in self.active_claims:
                return False  # Stop polling
            
            try:
                response = requests.get(
                    f'{BACKEND_URL}/api/claims/check',
                    params={'claim_id': claim_id},
                    timeout=5
                )
                
                if response.status_code == 200:
                    result = response.json()
                    
                    if result.get('success'):
                        status = result.get('status')
                        recipient = result.get('recipient_address')
                        
                        if status == 'claimed' and recipient:
                            self.qr_status.text = f'✓ Address received!\nMinting NFT to:\n{recipient[:10]}...{recipient[-8:]}'
                            self.qr_status.color = (0, 1, 0, 1)  # Green
                        elif status == 'completed':
                            token_id = result.get('token_id')
                            self.qr_status.text = f'🎉 Original Minted!\nToken ID: {token_id}\n\nScan QR to mint editions'
                            self.qr_status.color = (0, 1, 0, 1)  # Green
                            
                            # Update status label
                            Clock.schedule_once(
                                lambda dt: setattr(self.status_label, 'text', f'✓ Minted #{token_id}'),
                                0
                            )
                            
                            # Clear status label after 10 seconds (only once per claim)
                            if claim_id not in self.cleared_mint_status:
                                Clock.schedule_once(
                                    lambda dt: setattr(self.status_label, 'text', ''),
                                    10
                                )
                                self.cleared_mint_status.add(claim_id)
                            
                            # Keep QR code visible for others to mint editions
                            # Don't stop polling - keep showing QR for edition minting
                            # del self.active_claims[claim_id]
                            # return False
                        else:
                            self.qr_status.text = 'Waiting for wallet address...'
                            self.qr_status.color = (1, 1, 1, 1)  # White
                            
            except Exception as e:
                print(f"Polling error: {e}")
            
            return True  # Continue polling
        
        # Schedule polling
        Clock.schedule_interval(poll_claim, CLAIM_POLL_INTERVAL)
    
    def _sign_image(self, image_path):
        """
        Sign an image file with hardware identity.
        Creates a signature file alongside the image.
        
        Args:
            image_path: Path to image file
            
        Returns:
            dict: Signature information
        """
        if not self.hardware_identity:
            return None
        
        try:
            # Read image file and compute hash
            with open(image_path, 'rb') as f:
                image_data = f.read()
            
            # Compute SHA256 hash of image
            image_hash = hashlib.sha256(image_data).digest()
            image_hash_hex = image_hash.hex()
            
            # Sign the hash
            signature_info = self.hardware_identity.sign_hash(image_hash)
            signature_info['image_hash'] = image_hash_hex
            signature_info['image_path'] = str(image_path)
            signature_info['timestamp'] = datetime.now().isoformat()
            
            # Save signature to JSON file
            sig_path = Path(image_path).with_suffix('.sig.json')
            with open(sig_path, 'w') as f:
                json.dump(signature_info, f, indent=2)
            
            print(f"Signature saved: {sig_path}")
            return signature_info
            
        except Exception as e:
            print(f"Error signing image: {e}")
            return None

    def toggle_recording(self, instance):
        """Handle video recording button press."""
        if not self.camera.recording:
            # Start recording
            filename = self.camera.start_recording()

            if filename:
                self.video_button.text = 'STOP'
                self.video_button.background_color = (0.8, 0.4, 0.1, 1)
                self.status_label.text = '🔴 REC'
                self.status_label.color = (1, 0.2, 0.2, 1)  # Red for recording
            else:
                self.status_label.text = '✗ Failed'
                self.status_label.color = (1, 1, 1, 1)
                Clock.schedule_once(lambda dt: setattr(self.status_label, 'text', ''), 2)
        else:
            # Stop recording
            self.camera.stop_recording()
            self.video_button.text = 'VIDEO'
            self.video_button.background_color = (0.6, 0.2, 0.2, 1)
            self.status_label.text = '✓ Saved'
            self.status_label.color = (1, 1, 1, 1)

            # Reset status after 2 seconds
            Clock.schedule_once(
                lambda dt: setattr(self.status_label, 'text', ''),
                2
            )

    def zoom_in(self, instance):
        """Handle zoom in button press."""
        self.camera.zoom_in()
        self.status_label.text = f'🔍 {self.camera.current_zoom:.1f}x'
        Clock.schedule_once(
            lambda dt: setattr(self.status_label, 'text', ''),
            1
        )

    def zoom_out(self, instance):
        """Handle zoom out button press."""
        self.camera.zoom_out()
        self.status_label.text = f'🔍 {self.camera.current_zoom:.1f}x'
        Clock.schedule_once(
            lambda dt: setattr(self.status_label, 'text', ''),
            1
        )

    def show_error(self, message):
        """Display error message."""
        print(f"ERROR: {message}")
        self.status_label.text = 'Error'
        self.status_label.color = (1, 0, 0, 1)

    def open_gallery(self, instance):
        """Open gallery view to browse photos and videos."""
        # Create gallery overlay
        self.gallery_overlay = FloatLayout()

        # Dark background
        with self.gallery_overlay.canvas.before:
            Color(0, 0, 0, 0.95)
            self.gallery_bg = Rectangle(size=Window.size, pos=(0, 0))

        # Gallery container
        gallery_container = BoxLayout(
            orientation='vertical',
            size_hint=(0.98, 0.98),
            pos_hint={'center_x': 0.5, 'center_y': 0.5},
            spacing=15,
            padding=15
        )

        # Top bar with title and buttons
        top_bar = BoxLayout(
            orientation='horizontal', 
            size_hint=(1, 0.12), 
            spacing=15,
            padding=[10, 5]
        )

        title = Label(
            text='📷 Gallery',
            font_size='26sp',
            size_hint=(0.5, 1),
            halign='left',
            valign='middle',
            color=(1, 1, 1, 1),
            bold=True,
            text_size=(None, None)
        )
        title.bind(size=title.setter('text_size'))

        # Quit Gallery button - goes back to camera
        quit_gallery_button = Button(
            text='📸 Camera',
            font_size='18sp',
            size_hint=(0.3, 1),
            background_color=(0.2, 0.7, 0.3, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True,
            text_size=(None, None),
            halign='center',
            valign='middle'
        )
        quit_gallery_button.bind(size=quit_gallery_button.setter('text_size'))
        quit_gallery_button.bind(on_press=self.quit_gallery)

        # Close button
        close_button = Button(
            text='✕ Close',
            font_size='18sp',
            size_hint=(0.2, 1),
            background_color=(0.8, 0.2, 0.2, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True,
            text_size=(None, None),
            halign='center',
            valign='middle'
        )
        close_button.bind(size=close_button.setter('text_size'))
        close_button.bind(on_press=self.close_gallery)

        top_bar.add_widget(title)
        top_bar.add_widget(quit_gallery_button)
        top_bar.add_widget(close_button)

        # Scrollable grid of thumbnails
        self.gallery_scroll_view = ScrollView(
            size_hint=(1, 0.88),
            bar_width=10,
            scroll_type=['bars', 'content']
        )

        self.gallery_grid = GridLayout(
            cols=3,
            spacing=15,
            size_hint_y=None,
            padding=15
        )
        self.gallery_grid.bind(minimum_height=self.gallery_grid.setter('height'))

        # Load media files
        self.load_gallery_items()

        self.gallery_scroll_view.add_widget(self.gallery_grid)
        gallery_container.add_widget(top_bar)
        gallery_container.add_widget(self.gallery_scroll_view)

        self.gallery_overlay.add_widget(gallery_container)
        self.root_layout.add_widget(self.gallery_overlay)

    def load_gallery_items(self):
        """Load and display photos and videos from capture directory."""
        # Clear existing items
        self.gallery_grid.clear_widgets()

        # Get all photos and videos
        photos = sorted(glob.glob(str(CAPTURE_DIR / "photo_*.jpg")), reverse=True)
        videos = sorted(glob.glob(str(CAPTURE_DIR / "video_*.h264")), reverse=True)

        all_media = []
        for photo in photos:
            all_media.append(('photo', photo))
        for video in videos:
            all_media.append(('video', video))

        # Sort by filename (which includes timestamp)
        all_media.sort(key=lambda x: x[1], reverse=True)

        if not all_media:
            # No files found - better styled message
            no_files_label = Label(
                text='📷\n\nNo photos or videos yet\n\nTake some photos to see them here!',
                font_size='24sp',
                halign='center',
                valign='middle',
                color=(0.8, 0.8, 0.8, 1),
                size_hint_y=None,
                height=300
            )
            no_files_label.bind(size=no_files_label.setter('text_size'))
            self.gallery_grid.add_widget(no_files_label)
            return

        # Add media items
        for media_type, filepath in all_media:
            item_layout = BoxLayout(
                orientation='vertical',
                size_hint_y=None,
                height=250,
                spacing=8
            )

            # Thumbnail button with better styling
            if media_type == 'photo':
                thumb_button = Button(
                    background_normal=filepath,
                    background_down=filepath,
                    size_hint=(1, 0.88),
                    border=(0, 0, 0, 0)
                )
                thumb_button.filepath = filepath
                thumb_button.media_type = media_type
                thumb_button.bind(on_press=self.view_media)

                # Label with better formatting
                filename = os.path.basename(filepath)
                # Extract date and time from filename
                try:
                    date_part = filename[6:14]  # YYYYMMDD
                    time_part = filename[15:21]  # HHMMSS
                    label_text = f'📷 {date_part} {time_part}'
                except:
                    label_text = f'📷 {filename[6:21]}'
            else:
                # Video placeholder with better styling
                thumb_button = Button(
                    text='▶️\nVIDEO',
                    font_size='28sp',
                    background_color=(0.15, 0.15, 0.25, 1),
                    background_normal='',
                    size_hint=(1, 0.88),
                    color=(1, 1, 1, 1),
                    bold=True
                )
                thumb_button.filepath = filepath
                thumb_button.media_type = media_type
                thumb_button.bind(on_press=self.view_media)

                filename = os.path.basename(filepath)
                try:
                    date_part = filename[6:14]
                    time_part = filename[15:21]
                    label_text = f'🎥 {date_part} {time_part}'
                except:
                    label_text = f'🎥 {filename[6:21]}'

            label = Label(
                text=label_text,
                font_size='14sp',
                size_hint=(1, 0.12),
                color=(1, 1, 1, 1),
                halign='center',
                valign='middle'
            )
            label.bind(size=label.setter('text_size'))

            item_layout.add_widget(thumb_button)
            item_layout.add_widget(label)

            self.gallery_grid.add_widget(item_layout)

    def view_media(self, instance):
        """View selected photo or video in full screen."""
        filepath = instance.filepath
        media_type = instance.media_type

        # Create full screen viewer - add to gallery overlay, not root
        self.viewer_overlay = FloatLayout()

        with self.viewer_overlay.canvas.before:
            Color(0, 0, 0, 1)
            self.viewer_bg = Rectangle(size=Window.size, pos=(0, 0))

        viewer_container = BoxLayout(
            orientation='vertical',
            size_hint=(1, 1),
            spacing=0
        )

        # Media display
        if media_type == 'photo':
            media_widget = Image(
                source=filepath,
                allow_stretch=True,
                keep_ratio=True,
                size_hint=(1, 0.92)
            )
            # Prevent touch events from causing navigation
            media_widget.bind(on_touch_down=lambda w, t: True)  # Consume touch events
        else:
            # Simple video info display with better styling
            media_widget = BoxLayout(
                orientation='vertical',
                size_hint=(1, 0.92),
                padding=50
            )
            video_info = Label(
                text=f'🎥\n\nVideo File\n\n{os.path.basename(filepath)}\n\n✓ Video recorded successfully!\n✓ File saved to captures folder',
                font_size='22sp',
                halign='center',
                valign='middle',
                color=(1, 1, 1, 1),
                bold=True
            )
            video_info.bind(size=video_info.setter('text_size'))
            media_widget.add_widget(video_info)

        # Bottom controls with better styling
        controls = BoxLayout(
            orientation='horizontal',
            size_hint=(1, 0.1),
            spacing=15,
            padding=[15, 10]
        )

        back_button = Button(
            text='← Gallery',
            font_size='18sp',
            size_hint=(0.3, 1),
            background_color=(0.3, 0.5, 0.7, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True,
            text_size=(None, None),
            halign='center',
            valign='middle'
        )
        back_button.bind(size=back_button.setter('text_size'))
        # Use a lambda to ensure the event is handled and doesn't propagate
        back_button.bind(on_press=lambda btn: self.close_viewer(btn))

        delete_button = Button(
            text='🗑 Delete',
            font_size='18sp',
            size_hint=(0.3, 1),
            background_color=(0.8, 0.2, 0.2, 1),
            background_normal='',
            color=(1, 1, 1, 1),
            bold=True,
            text_size=(None, None),
            halign='center',
            valign='middle'
        )
        delete_button.bind(size=delete_button.setter('text_size'))
        delete_button.filepath = filepath
        delete_button.bind(on_press=self.delete_media)

        spacer = Label(size_hint=(0.4, 1))

        controls.add_widget(back_button)
        controls.add_widget(spacer)
        controls.add_widget(delete_button)

        viewer_container.add_widget(media_widget)
        viewer_container.add_widget(controls)

        self.viewer_overlay.add_widget(viewer_container)
        # Add viewer to gallery overlay so gallery stays underneath
        # This ensures when we close viewer, gallery is still there
        if hasattr(self, 'gallery_overlay'):
            # Make sure gallery overlay is visible first
            self.gallery_overlay.opacity = 1
            # Add viewer on top of gallery
            self.gallery_overlay.add_widget(self.viewer_overlay)
        else:
            # Fallback to root if gallery not available
            self.root_layout.add_widget(self.viewer_overlay)

    def close_viewer(self, instance):
        """Close media viewer and return to gallery grid."""
        print("Closing viewer and returning to gallery grid...")
        
        # Store reference to viewer overlay before removal
        viewer_to_remove = None
        if hasattr(self, 'viewer_overlay'):
            viewer_to_remove = self.viewer_overlay
        
        # Remove viewer overlay from wherever it is
        if viewer_to_remove:
            try:
                # Try to remove from gallery overlay first
                if hasattr(self, 'gallery_overlay') and viewer_to_remove.parent == self.gallery_overlay:
                    self.gallery_overlay.remove_widget(viewer_to_remove)
                    print("Removed viewer from gallery overlay")
                # Try to remove from root layout
                elif viewer_to_remove.parent == self.root_layout:
                    self.root_layout.remove_widget(viewer_to_remove)
                    print("Removed viewer from root layout")
                # Fallback: try to remove from children list
                elif hasattr(self, 'gallery_overlay') and viewer_to_remove in self.gallery_overlay.children:
                    self.gallery_overlay.remove_widget(viewer_to_remove)
                    print("Removed viewer from gallery overlay (fallback)")
                elif viewer_to_remove in self.root_layout.children:
                    self.root_layout.remove_widget(viewer_to_remove)
                    print("Removed viewer from root layout (fallback)")
            except Exception as e:
                print(f"Error removing viewer overlay: {e}")
            finally:
                # Clean up the viewer overlay reference
                if hasattr(self, 'viewer_overlay'):
                    del self.viewer_overlay
        
        # Ensure gallery grid is visible and properly displayed
        if hasattr(self, 'gallery_overlay'):
            # Make sure gallery overlay is visible and on top
            self.gallery_overlay.opacity = 1
            # Ensure gallery grid exists and is visible
            if hasattr(self, 'gallery_grid'):
                self.gallery_grid.opacity = 1
                # Make sure grid is enabled and can receive events
                self.gallery_grid.disabled = False
            # Ensure scroll view is also visible if it exists
            if hasattr(self, 'gallery_scroll_view'):
                self.gallery_scroll_view.opacity = 1
                self.gallery_scroll_view.disabled = False
            # Bring gallery overlay to front to ensure it's visible
            if self.gallery_overlay.parent:
                self.gallery_overlay.parent.remove_widget(self.gallery_overlay)
                self.root_layout.add_widget(self.gallery_overlay)
            print("Gallery grid should now be visible")

    def delete_media(self, instance):
        """Delete selected media file."""
        filepath = instance.filepath
        try:
            os.remove(filepath)
            print(f"Deleted: {filepath}")

            # Close viewer
            self.close_viewer(instance)

            # Reload gallery
            self.load_gallery_items()
        except Exception as e:
            print(f"Error deleting file: {e}")

    def quit_gallery(self, instance):
        """Quit gallery and return to camera view."""
        if hasattr(self, 'gallery_overlay'):
            self.root_layout.remove_widget(self.gallery_overlay)
            del self.gallery_overlay
        # Ensure camera preview is visible
        if hasattr(self, 'preview_image'):
            self.preview_image.opacity = 1
        print("Returned to camera view from gallery")

    def close_gallery(self, instance):
        """Close gallery view."""
        if hasattr(self, 'gallery_overlay'):
            self.root_layout.remove_widget(self.gallery_overlay)
            del self.gallery_overlay

    def quit_app(self, instance):
        """Quit the application cleanly."""
        print("Quitting application...")
        self.camera.cleanup()
        App.get_running_app().stop()

    def on_stop(self):
        """Clean up when app closes."""
        if self.camera:
            try:
                self.camera.cleanup()
            except:
                pass
        self.camera.cleanup()

if __name__ == '__main__':
    # Ensure capture directory exists
    CAPTURE_DIR.mkdir(parents=True, exist_ok=True)

    print("=" * 60)
    print("Raspberry Pi Camera App")
    print("=" * 60)
    print(f"Capture directory: {CAPTURE_DIR}")
    print(f"Camera available: {CAMERA_AVAILABLE}")
    print(f"Battery monitor: {'Simulated' if not UPS_AVAILABLE else 'Real'}")
    print(f"Hardware identity: {'Available' if HARDWARE_IDENTITY_AVAILABLE else 'Not available'}")
    print("=" * 60)
    print("Initializing hardware identity and camera...")
    print("(Hardware details will be printed after initialization)")
    print("=" * 60 + "\n")

    # Run the app
    CameraApp().run()
