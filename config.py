import os

class Config:
    # Default model configuration
    MODEL_REPO = "microsoft/Phi-3-mini-4k-instruct-onnx"
    # We will append the specific folder for DirectML in the manager
    MODEL_SUBFOLDER = "directml-int4-awq-block-128" 
    MODEL_DIR = os.path.join(os.getcwd(), "models")
    
    # OCR Configuration
    OCR_LANGUAGE = "en-US" # Windows OCR Language tag
    
    # Translation Defaults
    TARGET_LANGUAGE = "Spanish" # Default target for the prompt
    
    # App Settings
    UPDATE_INTERVAL_MS = 1000 # How often to capture and translate
