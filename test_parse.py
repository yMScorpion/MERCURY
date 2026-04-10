import json
raw = '["8501...", "2527..."]'
parsed = json.loads(raw)
print(parsed[0])
