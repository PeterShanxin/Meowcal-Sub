import sys
import time
import threading
import mss
import numpy as np
from PyQt6.QtWidgets import QApplication
from PyQt6.QtCore import QRect, QObject, pyqtSignal

from config import Config
from overlay_ui import SelectionOverlay, SubtitleWindow, ControlWindow, create_tray_icon
from ocr_service import OcrService
from llm_service import LLMService
from user_settings import UserSettings

class WorkerSignals(QObject):
    translation_result = pyqtSignal(str)
    error = pyqtSignal(str)
    
class TranslationWorker(threading.Thread):
    def __init__(self, rect, ocr_service, llm_service, signals):
        super().__init__()
        self.rect = rect
        self.ocr_service = ocr_service
        self.llm_service = llm_service
        self.signals = signals
        self.running = True
        self.last_text = ""
        self.sct = mss.mss()

    def run(self):
        print(f"Worker started for area: {self.rect}")
        while self.running:
            start_time = time.time()
            
            try:
                # Capture Screen
                monitor = {
                    "top": self.rect.top(),
                    "left": self.rect.left(),
                    "width": self.rect.width(),
                    "height": self.rect.height()
                }
                
                # MSS returns a ScreenShot object
                sct_img = self.sct.grab(monitor)
                
                # Convert to raw bytes for OCR (BGRA)
                # OcrService expects raw bytes + dims
                raw_bytes = sct_img.raw
                
                # Run OCR
                text = self.ocr_service.recognize_text(raw_bytes, sct_img.width, sct_img.height)
                
                # If text found and different from last time
                if text and text != self.last_text:
                    print(f"Detected: {text}")
                    # Translate
                    translated_text = self.llm_service.translate(text)
                    print(f"Translated: {translated_text}")
                    
                    self.signals.translation_result.emit(translated_text)
                    self.last_text = text
            
            except Exception as e:
                print(f"Error in worker loop: {e}")
                self.signals.error.emit(str(e))

            # Sleep to maintain update interval
            elapsed = (time.time() - start_time) * 1000
            sleep_ms = max(100, Config.UPDATE_INTERVAL_MS - elapsed)
            time.sleep(sleep_ms / 1000.0)

    def stop(self):
        self.running = False

class TranslationApp:
    def __init__(self):
        self.app = QApplication(sys.argv)
        self.app.setQuitOnLastWindowClosed(False)
        
        # Load user settings
        self.settings = UserSettings()
        
        # Create UI components
        self.control_window = ControlWindow(
            is_first_time=self.settings.is_first_time,
            settings=self.settings
        )
        self.overlay = None
        self.subtitle_window = SubtitleWindow()
        self.tray = create_tray_icon(
            self.app, 
            self.show_control_window,
            self.start_selection
        )
        
        # Connect control window signals
        self.control_window.select_area_requested.connect(self.start_selection)
        self.control_window.start_requested.connect(self.start_translation)
        self.control_window.stop_requested.connect(self.stop_translation)
        self.control_window.language_changed.connect(self.on_language_changed)
        
        # Initialize Services
        print("Initializing Services...")
        self.ocr_service = OcrService()
        self.llm_service = LLMService()
        
        self.worker = None
        self.signals = WorkerSignals()
        self.signals.translation_result.connect(self.subtitle_window.update_text)
        self.signals.error.connect(self.on_worker_error)
        
        # Store selected rect for start/stop
        self.selected_rect = None
        
        # Load last selected area if available
        if self.settings.last_selected_area:
            area = self.settings.last_selected_area
            self.selected_rect = QRect(area["x"], area["y"], area["width"], area["height"])
            self.control_window.set_selected_area(self.selected_rect)
        
        # Show control window on startup
        self.control_window.show()

    def show_control_window(self):
        """Show or bring control window to front."""
        self.control_window.show()
        self.control_window.raise_()
        self.control_window.activateWindow()

    def start_selection(self):
        """Start the area selection process."""
        if self.worker:
            self.stop_translation()
        
        self.subtitle_window.hide()
        self.overlay = SelectionOverlay()
        self.overlay.area_selected.connect(self.on_area_selected)
        self.overlay.show()

    def on_area_selected(self, rect: QRect):
        """Handle when user has selected an area."""
        print(f"Area selected: {rect}")
        if rect.width() > 10 and rect.height() > 10:
            self.selected_rect = rect
            
            # Save to settings
            self.settings.last_selected_area = {
                "x": rect.x(),
                "y": rect.y(), 
                "width": rect.width(),
                "height": rect.height()
            }
            
            # Update control window
            self.control_window.set_selected_area(rect)
            self.control_window.show()

    def start_translation(self):
        """Start the translation worker."""
        if not self.selected_rect:
            return
        
        print("Starting translation...")
        self.subtitle_window.update_position(self.selected_rect)
        self.subtitle_window.show()
        
        # Start Worker
        self.worker = TranslationWorker(
            self.selected_rect, 
            self.ocr_service, 
            self.llm_service, 
            self.signals
        )
        self.worker.start()

    def stop_translation(self):
        """Stop the translation worker."""
        if self.worker:
            print("Stopping translation...")
            self.worker.stop()
            self.worker.join(timeout=2)
            self.worker = None
        self.subtitle_window.hide()

    def on_language_changed(self, language: str):
        """Handle language change from control window."""
        Config.TARGET_LANGUAGE = language
        print(f"Target language changed to: {language}")

    def on_worker_error(self, error: str):
        """Handle errors from the translation worker."""
        self.control_window.set_status(f"Error: {error}", is_error=True)

    def run(self):
        sys.exit(self.app.exec())

if __name__ == "__main__":
    app = TranslationApp()
    app.run()

