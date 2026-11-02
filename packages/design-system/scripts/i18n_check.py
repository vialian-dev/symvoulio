# i18n completeness gate (brief §0б) of the shared design system.
#
#   python design-system/scripts/i18n_check.py [locales_dir] [--require design-system/i18n/required-keys.json]
#
# For every language in <locales_dir>/languages.json, every namespace that English has must carry exactly the English key
# set, no empty value, and the same {{placeholders}}. Plural keys count once by their base name (`_one`, `_few`, `_other`
# … differ by language). With --require, the listed keys (the frame's texts) must exist in English too.
# One line per language, «OK» or «FAIL»; the exit code is the number of failing languages capped to 1.
import json, os, re, sys

args = [a for a in sys.argv[1:]]
require = None
if "--require" in args:
    i = args.index("--require")
    require = args[i + 1]
    del args[i:i + 2]
ROOT = args[0] if args else os.path.join(os.getcwd(), "public", "locales")
LANGS = [l["code"] for l in json.load(open(os.path.join(ROOT, "languages.json"), encoding="utf-8"))]
NAMESPACES = sorted(f[:-5] for f in os.listdir(os.path.join(ROOT, "en")) if f.endswith(".json"))
PLURAL = re.compile(r"_(zero|one|two|few|many|other)$")
PH = re.compile(r"{{\s*\w+\s*}}")


def flat(d, p=""):
    out = {}
    for k, v in d.items():
        key = f"{p}.{k}" if p else k
        out.update(flat(v, key) if isinstance(v, dict) else {key: v})
    return out


def load(code, ns):
    path = os.path.join(ROOT, code, ns + ".json")
    if not os.path.exists(path):
        return None
    out = {}
    for k, v in flat(json.load(open(path, encoding="utf-8"))).items():
        base = PLURAL.sub("", k)
        if base not in out or k.endswith("_other"):
            out[base] = v
    return out


bad = 0
if require:
    req = json.load(open(require, encoding="utf-8"))
    miss = [f"{ns}:{k}" for ns, keys in req.items() if not ns.startswith("_") for k in keys if k not in (load("en", ns) or {})]
    print(f"{'required':8} keys={sum(len(v) for n, v in req.items() if not n.startswith('_'))} missing={len(miss)} {'OK' if not miss else 'FAIL'}")
    for m in miss[:10]:
        print(f"  en: missing required {m}")
    bad += bool(miss)

ok_langs = 0
for code in LANGS:
    missing = extra = ph = 0
    for ns in NAMESPACES:
        en, cur = load("en", ns), load(code, ns) or {}
        m = [k for k in en if k not in cur or (isinstance(cur[k], str) and not cur[k].strip())]
        x = [k for k in cur if k not in en]
        p = [k for k in en if k in cur and sorted(PH.findall(str(en[k]))) != sorted(PH.findall(str(cur[k])))]
        missing += len(m); extra += len(x); ph += len(p)
        for k in m[:3]:
            print(f"  {code}/{ns}: missing {k}")
    keys = sum(len(load("en", ns) or {}) for ns in NAMESPACES)
    state = "OK" if missing == extra == ph == 0 else "FAIL"
    bad += state == "FAIL"
    ok_langs += state == "OK"
    print(f"{code:8} scope=full keys={keys} missing={missing} extra={extra} placeholder_mismatch={ph} {state}")
print(f"languages OK {ok_langs}/{len(LANGS)}, namespaces {len(NAMESPACES)}")
sys.exit(1 if bad else 0)
