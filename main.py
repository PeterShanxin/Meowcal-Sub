import sys
import time
import threading
import mss
import numpy as np
from PyQt6.QtWidgets import QApplication
from PyQt6.QtCore import QRect, QObject, pyqtSignal

from config import Config
from overlay_ui import SelectionOverlay, SubtitleWindow, create_tray_icon
from ocr_service import OcrService
from llm_service import LLMService

class WorkerSignals(QObject):
    translation_result = pyqtSignal(str)
    
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
        
        self.overlay = None
        self.subtitle_window = SubtitleWindow()
        self.tray = create_tray_icon(self.app, self.start_selection)
        
        # Initialize Services
        print("Initializing Services...")
        self.ocr_service = OcrService()
        self.llm_service = LLMService()
        
        self.worker = None
        self.signals = WorkerSignals()
        self.signals.translation_result.connect(self.subtitle_window.update_text)
        
        # Start immediately with selection if desired, or wait for user
        # self.start_selection()

    def start_selection(self):
        if self.worker:
            self.worker.stop()
            self.worker.join()
            self.worker = None
        
        self.subtitle_window.hide()
        self.overlay = SelectionOverlay()
        self.overlay.area_selected.connect(self.on_area_selected)
        self.overlay.show()

    def on_area_selected(self, rect: QRect):
        print(f"Area selected: {rect}")
        if rect.width() > 10 and rect.height() > 10:
            self.subtitle_window.update_position(rect)
            self.subtitle_window.show()
            
            # Start Worker
            self.worker = TranslationWorker(rect, self.ocr_service, self.llm_service, self.signals)
            self.worker.start()

    def run(self):
        sys.exit(self.app.exec())

if __name__ == "__main__":
    app = TranslationApp()
    app.run()
