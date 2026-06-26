# tokenslim-client

TokenSlim REST client SDK for Python (supports both synchronous and asynchronous operations).

## Installation

```bash
pip install tokenslim-client
```

## Usage

```python
from tokenslim_client import TokenSlimClient

client = TokenSlimClient(host="127.0.0.1", port=10086)
result = client.compress("Your long log message...")
print(result)
```
