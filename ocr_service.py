import sys
import asyncio
from config import Config

# Try to import Windows SDK components
try:
    import winsdk.windows.media.ocr as ocr
    import winsdk.windows.graphics.imaging as imaging
    import winsdk.windows.storage.streams as streams
    from winsdk.windows.security.cryptography import CryptographicBuffer
    IS_WINDOWS = True
except ImportError:
    IS_WINDOWS = False
    from hardware_mocks import MockOcrEngine

class OcrService:
    def __init__(self):
        self.engine = None
        self._init_engine()

    def _init_engine(self):
        if IS_WINDOWS:
            try:
                # Attempt to create the OCR engine with the preferred language
                lang = ocr.OcrEngine.try_create_from_user_profile_languages()
                # If specific language needed:
                # lang_code = Config.OCR_LANGUAGE
                # lang = ocr.OcrEngine.try_create_from_language(Language(lang_code))
                
                if not lang:
                    # Fallback to default if user profile lang not supported
                    self.engine = ocr.OcrEngine.try_create_from_language(ocr.OcrEngine.available_recognizer_languages[0])
                else:
                    self.engine = lang
                
                print("Native Windows OCR Engine initialized.")
            except Exception as e:
                print(f"Failed to initialize Windows OCR: {e}")
                self.engine = None
        else:
            print("Non-Windows environment detected. Using Mock OCR.")
            self.engine = MockOcrEngine()

    def recognize_text(self, image_data, width, height):
        """
        image_data: raw bytes (BGRA usually from mss)
        width: int
        height: int
        Returns: string of detected text
        """
        if not self.engine:
            return ""

        if IS_WINDOWS:
            try:
                # Create SoftwareBitmap from raw bytes
                # MSS returns BGRA pixels
                
                # We need to write bytes to an IBuffer
                # Using CryptographicBuffer to create IBuffer from bytearray
                # Note: This part can be tricky in Python/WinRT. 
                # Alternative: Use a stream.
                
                # Efficient approach:
                # software_bitmap = imaging.SoftwareBitmap(imaging.BitmapPixelFormat.bgra8, width, height)
                # But populating it is hard from python bytes without copying.
                
                # Let's try the stream approach which is often more robust in projection
                # Or simply:
                # Create a buffer from bytes
                ibuffer = CryptographicBuffer.create_from_byte_array(bytearray(image_data))
                
                software_bitmap = imaging.SoftwareBitmap.create_copy_from_buffer(
                    ibuffer,
                    imaging.BitmapPixelFormat.bgra8,
                    width,
                    height
                )

                # Run OCR
                # recognize_async returns IAsyncOperation
                # In a standard python script we can wait.
                # Since this is likely running in a worker thread, we can block or await.
                # Windows implementation in Python often allows .get() to block? 
                # If not, we use asyncio.run or loop.run_until_complete
                
                # For now, let's assume we are in a context where we can block or we wrap in async
                # Check if we can just call .get() on the projection
                # Usually: result = await self.engine.recognize_async(software_bitmap)
                
                # We'll define a helper to run async if needed, but lets try to keep it synchronous-looking for the caller
                # by spinning a loop if necessary.
                
                # Using a quick localized event loop
                loop = asyncio.new_event_loop()
                asyncio.set_event_loop(loop)
                result = loop.run_until_complete(self.engine.recognize_async(software_bitmap))
                loop.close()
                
                # Extract text
                # result is OcrResult
                lines = [line.text for line in result.lines]
                full_text = " ".join(lines)
                return full_text.strip()

            except Exception as e:
                print(f"OCR Error: {e}")
                return ""
        else:
            # Mock Implementation
            # Ignore image data, just return mock
            result_op = self.engine.recognize_async(None)
            return result_op.get().Text
