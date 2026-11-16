#!/usr/bin/env python3
"""只读探查 Cursor 的 state.vscdb 结构：只打印表名、键前缀计数、JSON 字段名与类型。
不打印任何值；凡是键名含 auth / token / secret / credential 的一律只计数、不读值。"""
import collections, json, os, sqlite3, sys

ROOT = os.path.expanduser("~/Library/Application Support/Cursor/User")
DENY = ("auth", "token", "secret", "credential", "password", "cookie", "session")

def denied(key):
    k = key.lower()
    return any(word in k for word in DENY)

def shape(value, depth=0):
    if isinstance(value, dict):
        if depth >= 2:
            return "{…}"
        return {k: shape(v, depth + 1) for k, v in list(value.items())[:60]}
    if isinstance(value, list):
        return [shape(value[0], depth + 1)] if value else []
    return type(value).__name__

def probe(path, label):
    print(f"## {label}")
    if not os.path.exists(path):
        print("不存在"); return
    print("大小 MB", round(os.path.getsize(path) / 1e6, 1))
    con = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    tables = [r[0] for r in con.execute("select name from sqlite_master where type='table'")]
    print("表", tables)
    for table in tables:
        cols = [r[1] for r in con.execute(f"pragma table_info({table})")]
        if "key" not in cols:
            continue
        prefixes = collections.Counter()
        for (key,) in con.execute(f"select key from {table}"):
            prefix = key.split(":")[0] if ":" in key else key
            prefixes["(凭据类，已跳过)" if denied(prefix) else prefix] += 1
        interesting = {p: n for p, n in prefixes.items() if any(w in p.lower() for w in ("composer", "bubble", "chat", "aichat", "conversation", "prompt"))}
        print(f"{table} 键前缀（与对话相关）:", dict(sorted(interesting.items())))
        print(f"{table} 键前缀总数 {len(prefixes)}，键总数 {sum(prefixes.values())}")
        for prefix in sorted(interesting):
            if denied(prefix):
                continue
            row = con.execute(f"select value from {table} where key like ? limit 1", (prefix + "%",)).fetchone()
            if not row or row[0] is None:
                continue
            raw = row[0]
            if isinstance(raw, bytes):
                raw = raw.decode("utf-8", "replace")
            try:
                data = json.loads(raw)
            except Exception:
                print(f"  {prefix}: 非 JSON，长度 {len(raw)}")
                continue
            print(f"  {prefix} 结构:", json.dumps(shape(data), ensure_ascii=False)[:2500])

probe(os.path.join(ROOT, "globalStorage", "state.vscdb"), "globalStorage/state.vscdb")
ws = os.path.join(ROOT, "workspaceStorage")
if os.path.isdir(ws):
    dbs = [os.path.join(ws, d, "state.vscdb") for d in os.listdir(ws) if os.path.exists(os.path.join(ws, d, "state.vscdb"))]
    print("## workspaceStorage 下的 state.vscdb 个数", len(dbs))
    if dbs:
        probe(sorted(dbs, key=os.path.getmtime)[-1], "最近的 workspace state.vscdb")
chats = os.path.expanduser("~/.cursor/chats")
print("## ~/.cursor/chats", "存在" if os.path.isdir(chats) else "不存在")
if os.path.isdir(chats):
    n = sum(len(files) for _, _, files in os.walk(chats))
    print("文件数", n)
