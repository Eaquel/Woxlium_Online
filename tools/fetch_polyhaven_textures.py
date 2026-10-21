import json, os, sys, urllib.request
op = urllib.request.build_opener(); op.addheaders = [("User-Agent", "Mozilla/5.0")]; urllib.request.install_opener(op)
for a in sys.argv[1:]:
    f = json.load(urllib.request.urlopen(f"https://api.polyhaven.com/files/{a}"))
    for key in ["Diffuse", "nor_gl", "Rough", "AO"]:
        if key in f and "1k" in f[key]:
            u = f[key]["1k"]["jpg"]["url"]
            urllib.request.urlretrieve(u, f"{a}_{key}.jpg")
    print("ok", a, [k for k in f.keys()][:12])
