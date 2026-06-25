"""
TokenSlim REST API Client for Python.
支持 compress / decompress / compress_file / batch_compress 和 async 版本。
连接运行中的 TokenSlim Sidecar Server (默认 http://127.0.0.1:10086)。
"""

import json
import urllib.request
import urllib.error
import time
from typing import Any, Dict, List, Optional
from pathlib import Path


class TokenSlimClient:
    """同步版 TokenSlim 客户端。"""

    def __init__(self, host: str = "127.0.0.1", port: int = 10086,
                 max_retries: int = 3, retry_delay: float = 1.0):
        self.base_url = f"http://{host}:{port}"
        self.max_retries = max_retries
        self.retry_delay = retry_delay

    def is_healthy(self) -> bool:
        """检测 server 是否在线。"""
        try:
            with urllib.request.urlopen(f"{self.base_url}/health", timeout=3) as response:
                return response.getcode() == 200
        except Exception:
            return False

    def compress(self, text: str) -> Dict[str, Any]:
        """
        压缩文本。
        Returns: 包含 tokens, dictionary, metadata 的字典。
        """
        return self._request_with_retry("POST", "/compress", {"text": text})

    def decompress(self, tokens: Any, dictionary: Any) -> Dict[str, Any]:
        """
        解压 TokenSlim payload。
        """
        return self._request_with_retry("POST", "/decompress", {
            "tokens": tokens, "dictionary": dictionary
        })

    def compress_file(self, file_path: str | Path) -> Dict[str, Any]:
        """
        压缩文件内容。
        @param file_path: 文件路径
        """
        text = Path(file_path).read_text(encoding="utf-8")
        return self.compress(text)

    def batch_compress(self, texts: List[str]) -> List[Dict[str, Any]]:
        """
        批量压缩多段文本（串行执行）。
        """
        return [self.compress(t) for t in texts]

    def stats(self) -> Dict[str, Any]:
        """获取 server 累计统计数据。"""
        return self._request_with_retry("GET", "/stats/aggregate")

    # ---------- 内部方法 ----------

    def _request_with_retry(self, method: str, path: str,
                            body: Optional[Dict] = None, attempt: int = 0) -> Any:
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode("utf-8") if body else None
        headers = {'Content-Type': 'application/json'} if body else {}

        req = urllib.request.Request(url, data=data, headers=headers, method=method)

        try:
            with urllib.request.urlopen(req, timeout=30) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.URLError as e:
            # 网络错误时自动重试
            if attempt < self.max_retries:
                time.sleep(self.retry_delay * (attempt + 1))
                return self._request_with_retry(method, path, body, attempt + 1)
            raise ConnectionError(f"TokenSlim server 不可达: {e}") from e
        except urllib.error.HTTPError as e:
            raise Exception(f"TokenSlim Error: {e.code} - {e.read().decode('utf-8')}")


class AsyncTokenSlimClient:
    """
    异步版 TokenSlim 客户端（需要 aiohttp）。
    用法: async with AsyncTokenSlimClient() as client: ...
    """

    def __init__(self, host: str = "127.0.0.1", port: int = 10086):
        self.base_url = f"http://{host}:{port}"
        self._session = None

    async def __aenter__(self):
        try:
            import aiohttp
            self._session = aiohttp.ClientSession()
        except ImportError:
            raise ImportError("AsyncTokenSlimClient 需要安装 aiohttp: pip install aiohttp")
        return self

    async def __aexit__(self, *args):
        if self._session:
            await self._session.close()

    async def is_healthy(self) -> bool:
        try:
            async with self._session.get(f"{self.base_url}/health", timeout=3) as resp:
                return resp.status == 200
        except Exception:
            return False

    async def compress(self, text: str) -> Dict[str, Any]:
        async with self._session.post(
            f"{self.base_url}/compress",
            json={"text": text},
            timeout=30
        ) as resp:
            return await resp.json()

    async def decompress(self, tokens: Any, dictionary: Any) -> Dict[str, Any]:
        async with self._session.post(
            f"{self.base_url}/decompress",
            json={"tokens": tokens, "dictionary": dictionary},
            timeout=30
        ) as resp:
            return await resp.json()

    async def compress_file(self, file_path: str | Path) -> Dict[str, Any]:
        text = Path(file_path).read_text(encoding="utf-8")
        return await self.compress(text)

    async def batch_compress(self, texts: List[str]) -> List[Dict[str, Any]]:
        """并发批量压缩。"""
        import asyncio
        tasks = [self.compress(t) for t in texts]
        return await asyncio.gather(*tasks)


if __name__ == "__main__":
    # 同步示例
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
