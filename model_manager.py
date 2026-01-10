import os
from huggingface_hub import snapshot_download
from config import Config

class ModelManager:
    def __init__(self):
        self.model_dir = Config.MODEL_DIR
        self.repo_id = Config.MODEL_REPO
        self.subfolder = Config.MODEL_SUBFOLDER

    def ensure_model_exists(self):
        """
        Checks if the model exists in the models directory.
        If not, downloads it.
        Returns the path to the model directory.
        """
        # The specific path where onnxruntime-genai expects the config files
        # We need the directml version for NPU/GPU acceleration
        target_path = os.path.join(self.model_dir, self.subfolder)
        
        if os.path.exists(target_path) and os.listdir(target_path):
            print(f"Model found at {target_path}")
            return target_path
        
        print(f"Model not found. Downloading {self.repo_id} (subfolder: {self.subfolder})...")
        print("This may take a while depending on internet speed...")
        
        try:
            # We download only the specific subfolder for DirectML
            snapshot_download(
                repo_id=self.repo_id,
                local_dir=self.model_dir, # This creates the structure models/directml-...
                allow_patterns=[f"{self.subfolder}/*"]
            )
            print("Download complete.")
            return target_path
        except Exception as e:
            print(f"Error downloading model: {e}")
            raise

if __name__ == "__main__":
    # Test the manager
    manager = ModelManager()
    # We don't actually download in the sandbox environment test to avoid huge downloads/timeouts
    # unless we want to verify the logic. 
    # For now, we just print where it would go.
    print(f"Target Model Path: {os.path.join(manager.model_dir, manager.subfolder)}")
