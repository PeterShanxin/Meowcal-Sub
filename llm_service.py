import os
from config import Config
from model_manager import ModelManager

# Try import onnxruntime_genai
try:
    import onnxruntime_genai as og
    IS_NATIVE = True
except ImportError:
    IS_NATIVE = False
    from hardware_mocks import MockLLM

class LLMService:
    def __init__(self):
        self.model = None
        self.tokenizer = None
        self.manager = ModelManager()
        self._init_model()

    def _init_model(self):
        # Ensure model is downloaded
        # Note: In sandbox, we don't want to actually download 2GB file during tests if we are mocking
        # So we check if IS_NATIVE is true before calling ensure_model_exists in a real scenario
        # But per requirements we should try.
        
        try:
            if IS_NATIVE:
                model_path = self.manager.ensure_model_exists()
                print(f"Loading DirectML model from {model_path}...")
                self.model = og.Model(model_path)
                self.tokenizer = og.Tokenizer(self.model)
                print("Model loaded successfully.")
            else:
                print("onnxruntime_genai not found. Using Mock LLM.")
                self.model = MockLLM("mock_path")
                # Mock LLM handles its own generation logic internally for simplicity
        except Exception as e:
            print(f"Failed to load LLM: {e}")
            # Fallback to mock if loading fails (e.g. wrong hardware)
            if IS_NATIVE:
                print("Fallback to Mock LLM due to load error.")
                self.model = MockLLM("mock_path")

    def translate(self, text, target_language=Config.TARGET_LANGUAGE):
        if not text:
            return ""

        prompt = f"<|system|>\nYou are a helpful translator. Translate the following text to {target_language}. Only output the translation.<|end|>\n<|user|>\n{text}<|end|>\n<|assistant|>"
        
        if not IS_NATIVE or isinstance(self.model, MockLLM):
            return self.model.generate(text)

        try:
            # Native GenAI generation
            params = og.GeneratorParams(self.model)
            params.set_search_options(max_length=200) # Limit length
            
            input_ids = self.tokenizer.encode(prompt)
            params.set_input_ids(input_ids)

            generator = og.Generator(self.model, params)

            output_tokens = []
            while not generator.is_done():
                generator.compute_logits()
                generator.generate_next_token()
                
                new_token = generator.get_next_tokens()[0]
                output_tokens.append(new_token)

            # Decode result
            decoded_output = self.tokenizer.decode(output_tokens)
            return decoded_output.strip()

        except Exception as e:
            print(f"Generation Error: {e}")
            return f"[Error] {text}"
