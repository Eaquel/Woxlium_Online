import json, os, sys, urllib.request
op = urllib.request.build_opener(); op.addheaders = [("User-Agent", "Mozilla/5.0")]; urllib.request.install_opener(op)
ids = sys.argv[1:]
for a in ids:
    f = json.load(urllib.request.urlopen(f"https://api.polyhaven.com/files/{a}"))
    g = f["gltf"]["1k"]["gltf"]
    os.makedirs(a, exist_ok=True)
    urllib.request.urlretrieve(g["url"], os.path.join(a, os.path.basename(g["url"])))
    for rel, inc in g.get("include", {}).items():
        p = os.path.join(a, rel); os.makedirs(os.path.dirname(p), exist_ok=True)
        urllib.request.urlretrieve(inc["url"], p)
    print("ok", a)
