import random

class MockOcrResult:
    def __init__(self, text):
        self.Text = text

class MockOcrEngine:
    def recognize_async(self, bitmap):
        return MockAsyncOperation(MockOcrResult("This is some dummy text detected by the Mock OCR engine."))

class MockAsyncOperation:
    def __init__(self, result):
        self.result = result
    
    # In a real async/await scenario we'd await this, but for sync mocking we just return the result
    def get(self):
        return self.result

class MockModel:
    pass

class MockTokenizer:
    def encode(self, text):
        return [1, 2, 3]
    
    def decode(self, tokens):
        return "Translated text (Spanish): Hola mundo"

class MockGeneratorParams:
    def set_input_ids(self, ids):
        pass
    def set_search_options(self, **kwargs):
        pass

class MockGenerator:
    def __init__(self, model, params):
        self.model = model
    
    def is_done(self):
        # Return True immediately for mock
        return True
    
    def compute_logits(self):
        pass
    
    def generate_next_token(self):
        pass
    
    def get_next_tokens(self):
        return [1]

class MockLLM:
    """
    Simulates the ONNX Runtime GenAI flow
    """
    def __init__(self, model_path):
        print(f"MockLLM loading model from {model_path}")
        self.model = MockModel()
        self.tokenizer = MockTokenizer()
    
    def generate(self, text):
        return f"[Mock Translation] {text} -> Hola mundo"
