import json
import urllib.request
import urllib.error

class TokenSlimClient:
    """
    TokenSlim REST API Client for Python.
    Connects to a running TokenSlim Sidecar Server.
    """
    def __init__(self, host="127.0.0.1", port=10086):
        self.base_url = f"http://{host}:{port}"

    def is_healthy(self):
        """Check if the server is alive and well."""
        try:
            with urllib.request.urlopen(f"{self.base_url}/health") as response:
                return response.getcode() == 200
        except:
            return False

    def compress(self, text):
        """
        Compress the given text using TokenSlim.
        Returns a dictionary containing 'tokens', 'dictionary', and 'metadata'.
        """
        url = f"{self.base_url}/compress"
        data = json.dumps({"text": text}).encode("utf-8")
        req = urllib.request.Request(url, data=data, headers={'Content-Type': 'application/json'})
        
        try:
            with urllib.request.urlopen(req) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            raise Exception(f"TokenSlim Error: {e.code} - {e.read().decode('utf-8')}")

    def decompress(self, tokens, dictionary):
        """
        Decompress tokens back to original text using the provided dictionary.
        """
        url = f"{self.base_url}/decompress"
        data = json.dumps({"tokens": tokens, "dictionary": dictionary}).encode("utf-8")
        req = urllib.request.Request(url, data=data, headers={'Content-Type': 'application/json'})
        
        try:
            with urllib.request.urlopen(req) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            raise Exception(f"TokenSlim Error: {e.code} - {e.read().decode('utf-8')}")

if __name__ == "__main__":
    # Example Usage
    client = TokenSlimClient()
    if client.is_healthy():
        print("Connected to TokenSlim Server.")
        sample_log = "2024-12-26T07:47:22.609Z [INFO] Processing user data..."
        result = client.compress(sample_log)
        print(f"Compressed! Ratio: {result['metadata']['compression_ratio']:.2%}")
        
        restored = client.decompress(result['tokens'], result['dictionary'])
        print(f"Restored: {restored}")
    else:
        print("Server not found. Please start tokenslim-server first.")
