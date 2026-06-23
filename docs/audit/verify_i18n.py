import json
ref = json.load(open(r"resources\messages.zh-CN.json", encoding="utf-8-sig"))
print("zh-CN ref keys:", len(ref))
for k in ["server_webui_enabled", "server_webui_disabled", "server_endpoint_plugins"]:
    print(f"  {k}: {ref[k][:60]}")
print()
for l in ["ar","de","en","es","fr","ja","ko","ru","zh-TW"]:
    b = json.load(open(f"resources/messages.{l}.json", encoding="utf-8-sig"))
    print(f"{l}: {len(b)} keys, webui_enabled = {b['server_webui_enabled'][:60]}")
