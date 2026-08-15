#!/usr/bin/env python3
"""生成 Tauri v2 updater 更新清单 latest.json。

用法: python3 make_updater_manifest.py <version> <sig-file> > latest.json

更新清单发布为 GitHub Release 资产 latest.json，应用端
tauri.conf.json 的 updater.endpoints 指向
https://github.com/wenfer/vid2audio/releases/latest/download/latest.json，
所以每次发布都用同名文件覆盖即可。
"""
import datetime
import json
import sys


def main() -> int:
    if len(sys.argv) != 3:
        print("用法: make_updater_manifest.py <version> <sig-file>", file=sys.stderr)
        return 1
    version, sig_path = sys.argv[1], sys.argv[2]
    signature = open(sig_path, encoding="utf-8").read().strip()
    manifest = {
        "version": version,
        "notes": f"Vid2Audio v{version}",
        "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platforms": {
            "windows-x86_64": {
                "signature": signature,
                "url": (
                    "https://github.com/wenfer/vid2audio/releases/download/"
                    f"v{version}/vid2audio-{version}-windows-x64-setup.exe"
                ),
            }
        },
    }
    print(json.dumps(manifest, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
